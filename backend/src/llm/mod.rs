//! Talking to language models.
//!
//! Everything else in the app uses `LlmClient`, never a provider directly.
//! Today the only provider is a local Ollama. To add another one (OpenAI,
//! Claude), write `llm/<provider>.rs` with the same three methods as
//! `OllamaClient` (`chat_json_with`, `chat_tools` and `models`), add a variant to
//! `LlmClient`, and fill in the `match` arms below. Nothing outside this
//! folder has to change, apart from creating the client in `main.rs`.

pub mod ollama;

use serde_json::Value;

pub use ollama::OllamaClient;

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
    },
    /// The result of one tool call, sent back to the model.
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

/// The one model client the app passes around. Cheap to clone.
#[derive(Clone)]
pub enum LlmClient {
    Ollama(OllamaClient),
    // OpenAi(OpenAiClient),
    // Claude(ClaudeClient),
}

impl LlmClient {
    /// Where the model runs, for the settings page.
    pub fn base_url(&self) -> &str {
        match self {
            LlmClient::Ollama(c) => c.base_url(),
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
        match self {
            LlmClient::Ollama(c) => c.chat_json_with(model, system, user, schema, options).await,
        }
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
        match self {
            LlmClient::Ollama(c) => c.chat_tools(model, messages, tools, options, on_text).await,
        }
    }

    /// Names of the models you can pick in Settings.
    pub async fn models(&self) -> Result<Vec<String>, LlmError> {
        match self {
            LlmClient::Ollama(c) => c.models().await,
        }
    }
}

impl From<OllamaClient> for LlmClient {
    fn from(client: OllamaClient) -> Self {
        LlmClient::Ollama(client)
    }
}
