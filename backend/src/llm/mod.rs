//! Talking to language models.
//!
//! Everything else in the app uses `LlmClient`, never a provider directly.
//! The model's name picks the provider: `claude-…` goes to Anthropic,
//! `gpt-…` to OpenAI, anything else (`qwen3:14b`) to the local Ollama. So a
//! task switches provider when you pick another model in Settings, and
//! nothing outside this folder has to know.
//!
//! Paid calls are logged with what they cost (`usage.rs`). When a provider
//! has spent its monthly cap, its calls go to your local apply model instead.

pub mod claude;
pub mod ollama;
pub mod openai;
pub mod usage;

use serde_json::Value;
use sqlx::PgPool;

pub use claude::ClaudeClient;
pub use ollama::OllamaClient;
pub use openai::OpenAiClient;
pub use usage::Usage;

/// The local model paid models fall back to when settings can't be read.
const LOCAL_DEFAULT: &str = "qwen3:14b";

/// Per-call options. A provider ignores the ones it has no use for.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatOptions {
    /// Let the model reason before answering. Slower, better for writing.
    pub think: bool,
    /// Context window for this call. None uses the provider's default.
    pub num_ctx: Option<u32>,
}

/// A tool the model may call: its name, what it's for, and a JSON schema
/// for its arguments.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

/// One message of a conversation with tools.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    System(String),
    User(String),
    /// What the model said, and the tools it asked for (if any).
    Assistant {
        content: String,
        tool_calls: Vec<ToolCall>,
        /// The provider's own copy of this answer (Claude's content blocks,
        /// OpenAI's output items), sent back as-is on the next call so the
        /// model keeps its reasoning and tool-call ids. None for Ollama and
        /// for answers loaded from the database.
        raw: Option<Value>,
    },
    /// The result of one tool call, sent back to the model. Results follow
    /// their `Assistant` message in the same order as its `tool_calls`.
    Tool {
        name: String,
        content: String,
    },
}

/// The model asking to run a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

/// One answer from the model: text, tool calls, or both.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    /// See `Message::Assistant::raw`.
    pub raw: Option<Value>,
}

/// Why a call failed. The workers treat these differently:
/// `is_unavailable()` pauses the whole queue, `BadAnswer` only fails one item.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// The service can't be reached. The text says what to do about it.
    #[error("{0}")]
    Unreachable(String),
    /// The model doesn't exist. The text says how to get it.
    #[error("{0}")]
    ModelMissing(String),
    #[error("The model service returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("The model's answer was not usable: {0}")]
    BadAnswer(String),
    #[error("The model took longer than 10 minutes to answer.")]
    Timeout,
}

impl LlmError {
    /// True when retrying a different item would fail the same way.
    pub fn is_unavailable(&self) -> bool {
        !matches!(self, LlmError::BadAnswer(_) | LlmError::Timeout)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Ollama,
    Claude,
    OpenAi,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Ollama => "ollama",
            Provider::Claude => "claude",
            Provider::OpenAi => "openai",
        }
    }
}

/// Which service runs `model`.
pub fn provider_of(model: &str) -> Provider {
    if model.starts_with("claude-") {
        Provider::Claude
    } else if model.starts_with("gpt-") {
        Provider::OpenAi
    } else {
        Provider::Ollama
    }
}

/// API keys and addresses for the paid providers, from backend/.env.
#[derive(Debug, Clone, Default)]
pub struct CloudKeys {
    pub anthropic_key: Option<String>,
    pub anthropic_url: String,
    pub openai_key: Option<String>,
    pub openai_url: String,
}

impl CloudKeys {
    pub fn from_env() -> Self {
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        Self {
            anthropic_key: var("ANTHROPIC_API_KEY"),
            anthropic_url: var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|| "https://api.anthropic.com".into()),
            openai_key: var("OPENAI_API_KEY"),
            openai_url: var("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com".into()),
        }
    }
}

/// The one model client the app passes around. Cheap to clone.
#[derive(Clone)]
pub struct LlmClient {
    ollama: OllamaClient,
    claude: Option<ClaudeClient>,
    openai: Option<OpenAiClient>,
    /// For the usage log and the caps. None in tests that only use Ollama.
    db: Option<PgPool>,
}

impl LlmClient {
    /// Adds Claude and OpenAI for the keys that are set.
    pub fn with_cloud(mut self, db: PgPool, keys: &CloudKeys) -> anyhow::Result<Self> {
        if let Some(key) = &keys.anthropic_key {
            self.claude = Some(ClaudeClient::new(&keys.anthropic_url, key)?);
        }
        if let Some(key) = &keys.openai_key {
            self.openai = Some(OpenAiClient::new(&keys.openai_url, key)?);
        }
        self.db = Some(db);
        Ok(self)
    }

    /// Where the local models run, for the settings page.
    pub fn base_url(&self) -> &str {
        self.ollama.base_url()
    }

    /// True when the API key for `provider` ("claude", "openai") is set.
    pub fn has_key(&self, provider: &str) -> bool {
        match provider {
            "claude" => self.claude.is_some(),
            "openai" => self.openai.is_some(),
            _ => false,
        }
    }

    /// Ask `model` one question and get back JSON that matches `schema`.
    pub async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
    ) -> Result<Value, LlmError> {
        self.chat_json_with(model, system, user, schema, ChatOptions::default())
            .await
    }

    /// `chat_json` with options (thinking, a bigger context window).
    pub async fn chat_json_with(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
        options: ChatOptions,
    ) -> Result<Value, LlmError> {
        self.chat_json_cost(model, system, user, schema, options)
            .await
            .map(|answer| answer.value)
    }

    /// `chat_json_with`, also saying which model answered and what it cost.
    pub async fn chat_json_cost(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
        options: ChatOptions,
    ) -> Result<JsonAnswer, LlmError> {
        let model = self.route(model).await?;
        let (value, usage) = match provider_of(&model) {
            Provider::Ollama => {
                let value = self
                    .ollama
                    .chat_json_with(&model, system, user, schema, options)
                    .await?;
                (value, None)
            }
            Provider::Claude => {
                let (value, usage) = self
                    .claude()?
                    .chat_json(&model, system, user, schema, options)
                    .await?;
                (value, Some(usage))
            }
            Provider::OpenAi => {
                let (value, usage) = self
                    .openai()?
                    .chat_json(&model, system, user, schema, options)
                    .await?;
                (value, Some(usage))
            }
        };
        let cost_usd = match usage {
            Some(usage) => Some(self.record(&model, usage).await),
            None => None,
        };
        Ok(JsonAnswer {
            value,
            model,
            cost_usd,
        })
    }

    /// One step of a conversation where the model may call `tools`.
    /// The answer's text is passed to `on_text` piece by piece as it arrives,
    /// so the page can show it while the model is still writing.
    pub async fn chat_tools(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSpec],
        options: ChatOptions,
        on_text: &mut (dyn FnMut(&str) + Send),
    ) -> Result<Turn, LlmError> {
        let model = self.route(model).await?;
        match provider_of(&model) {
            Provider::Ollama => {
                self.ollama
                    .chat_tools(&model, messages, tools, options, on_text)
                    .await
            }
            Provider::Claude => {
                let (turn, usage) = self
                    .claude()?
                    .chat_tools(&model, messages, tools, options, on_text)
                    .await?;
                self.record(&model, usage).await;
                Ok(turn)
            }
            Provider::OpenAi => {
                let (turn, usage) = self
                    .openai()?
                    .chat_tools(&model, messages, tools, options, on_text)
                    .await?;
                self.record(&model, usage).await;
                Ok(turn)
            }
        }
    }

    /// Names of the models you can pick in Settings: the installed Ollama
    /// models, then the paid models whose API key is set.
    pub async fn models(&self) -> Result<Vec<String>, LlmError> {
        let cloud: Vec<String> = usage::CLOUD_MODELS
            .iter()
            .filter(|m| self.has_key(m.provider))
            .map(|m| m.id.to_string())
            .collect();
        match self.ollama.models().await {
            Ok(mut local) => {
                local.extend(cloud);
                Ok(local)
            }
            Err(_) if !cloud.is_empty() => Ok(cloud),
            Err(e) => Err(e),
        }
    }

    /// The model that will really answer `model`'s calls right now.
    pub async fn effective_model(&self, model: &str) -> Result<String, LlmError> {
        self.route(model).await
    }

    /// The model that will really answer: `model`, or your local apply model
    /// when `model`'s provider has no API key or has spent its monthly cap.
    async fn route(&self, model: &str) -> Result<String, LlmError> {
        let provider = provider_of(model);
        if provider == Provider::Ollama {
            return Ok(model.to_string());
        }
        // No key (a fresh install with the default cloud models): use the
        // local model, so everything still works without an account.
        if !self.has_key(provider.name()) {
            let fallback = self.local_model().await;
            tracing::warn!(%model, %fallback, "no API key for this model, using the local model");
            return Ok(fallback);
        }
        let Some(db) = &self.db else {
            return Ok(model.to_string());
        };
        match usage::over_cap(db, provider.name()).await {
            Ok(false) => Ok(model.to_string()),
            Ok(true) => {
                let fallback = self.local_model().await;
                tracing::warn!(%model, %fallback, "monthly cap reached, using the local model");
                Ok(fallback)
            }
            Err(error) => Err(LlmError::Unreachable(format!(
                "Couldn't check this month's spending: {error}"
            ))),
        }
    }

    /// Your local apply model, the fallback for paid models.
    async fn local_model(&self) -> String {
        match &self.db {
            Some(db) => sqlx::query_scalar("SELECT apply_model FROM settings")
                .fetch_one(db)
                .await
                .unwrap_or_else(|_| LOCAL_DEFAULT.into()),
            None => LOCAL_DEFAULT.into(),
        }
    }

    /// Log a paid call. A failed log write is only a warning.
    async fn record(&self, model: &str, usage: Usage) -> f64 {
        let Some(db) = &self.db else {
            return usage::cost_usd(model, usage);
        };
        usage::record(db, model, usage)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(?error, "could not log model usage");
                usage::cost_usd(model, usage)
            })
    }

    fn claude(&self) -> Result<&ClaudeClient, LlmError> {
        self.claude
            .as_ref()
            .ok_or_else(|| LlmError::ModelMissing("ANTHROPIC_API_KEY is not set.".into()))
    }

    fn openai(&self) -> Result<&OpenAiClient, LlmError> {
        self.openai
            .as_ref()
            .ok_or_else(|| LlmError::ModelMissing("OPENAI_API_KEY is not set.".into()))
    }
}

impl From<OllamaClient> for LlmClient {
    fn from(client: OllamaClient) -> Self {
        LlmClient {
            ollama: client,
            claude: None,
            openai: None,
            db: None,
        }
    }
}

/// A `chat_json_cost` answer.
#[derive(Debug, Clone)]
pub struct JsonAnswer {
    pub value: Value,
    /// The model that answered (the local one if a cap was reached).
    pub model: String,
    /// None for local models.
    pub cost_usd: Option<f64>,
}

/// `schema` in the strict form Claude and OpenAI want: every object closed
/// (`additionalProperties: false`), and no number or length limits, which
/// they don't support. The workers still check the values.
pub fn strict_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "minimum" | "maximum" | "minLength" | "maxLength" | "minItems" | "maxItems"
                ) {
                    continue;
                }
                out.insert(key.clone(), strict_schema(value));
            }
            if out.get("type") == Some(&Value::String("object".into())) {
                out.insert("additionalProperties".into(), Value::Bool(false));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(strict_schema).collect()),
        other => other.clone(),
    }
}

/// A conversation that used tools, for a call that offers none (the
/// assistant's last round). The providers reject tool calls in the history
/// when no tools are defined, so they're written out as text.
pub fn flatten_tools(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .map(|m| match m {
            Message::Assistant {
                content,
                tool_calls,
                ..
            } if !tool_calls.is_empty() => {
                let mut text = content.trim().to_string();
                for call in tool_calls {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&format!(
                        "(I looked up {} with {})",
                        call.name, call.arguments
                    ));
                }
                Message::Assistant {
                    content: text,
                    tool_calls: Vec::new(),
                    raw: None,
                }
            }
            Message::Tool { name, content } => {
                Message::User(format!("Result of {name}:\n{content}"))
            }
            other => other.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_model_name_picks_the_provider() {
        assert_eq!(provider_of("claude-haiku-5-5"), Provider::Claude);
        assert_eq!(provider_of("gpt-6-luna"), Provider::OpenAi);
        assert_eq!(provider_of("qwen3:14b"), Provider::Ollama);
    }

    #[test]
    fn strict_schemas_are_closed_and_have_no_limits() {
        let schema = json!({
            "type": "object",
            "properties": {
                "score": { "type": "integer", "minimum": 0, "maximum": 100 },
                "items": { "type": "array", "items": { "type": "object", "properties": {} } }
            },
            "required": ["score", "items"]
        });
        assert_eq!(
            strict_schema(&schema),
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "score": { "type": "integer" },
                    "items": { "type": "array", "items": {
                        "type": "object", "properties": {}, "additionalProperties": false
                    } }
                },
                "required": ["score", "items"]
            })
        );
    }

    #[test]
    fn tool_calls_become_text_when_no_tools_are_offered() {
        let messages = vec![
            Message::User("Which jobs?".into()),
            Message::Assistant {
                content: String::new(),
                tool_calls: vec![ToolCall {
                    name: "search_jobs".into(),
                    arguments: json!({ "min_score": 70 }),
                }],
                raw: Some(json!([])),
            },
            Message::Tool {
                name: "search_jobs".into(),
                content: "[]".into(),
            },
        ];
        let flat = flatten_tools(&messages);
        assert_eq!(
            flat[1],
            Message::Assistant {
                content: "(I looked up search_jobs with {\"min_score\":70})".into(),
                tool_calls: vec![],
                raw: None
            }
        );
        assert_eq!(flat[2], Message::User("Result of search_jobs:\n[]".into()));
    }

    #[tokio::test]
    async fn a_paid_model_without_its_key_falls_back_to_the_local_model() {
        let llm = LlmClient::from(OllamaClient::new("http://127.0.0.1:9", 8192).unwrap());
        assert_eq!(
            llm.effective_model("claude-haiku-5-5").await.unwrap(),
            "qwen3:14b"
        );
        assert_eq!(
            llm.effective_model("gpt-6-luna").await.unwrap(),
            "qwen3:14b"
        );
        assert_eq!(llm.effective_model("llama3:8b").await.unwrap(), "llama3:8b");
    }
}
