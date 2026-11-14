//! A small client for a local Ollama server (https://ollama.com).
//! Only the two endpoints we need: `/api/chat` and `/api/tags`.

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    base_url: String,
    num_ctx: u32,
    /// One model call at a time. The scoring and drafting workers share this
    /// client, so they take turns on the GPU instead of fighting over it.
    gpu: Arc<tokio::sync::Mutex<()>>,
}

/// Per-call options for `chat_json_with`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatOptions {
    /// Let the model reason before answering (Qwen3 "thinking"). Slower, better for writing.
    pub think: bool,
    /// Context window for this call. None uses OLLAMA_NUM_CTX.
    pub num_ctx: Option<u32>,
}

/// Why a call failed. The scoring worker treats these differently:
/// `Unavailable` pauses the whole queue, `BadAnswer` only fails one job.
#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("Ollama is not reachable at {0}. Is `ollama serve` running?")]
    Unreachable(String),
    #[error("Model {0:?} is not installed. Run `ollama pull {0}`.")]
    ModelMissing(String),
    #[error("Ollama returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("The model's answer was not usable: {0}")]
    BadAnswer(String),
    #[error("Ollama took longer than 10 minutes to answer.")]
    Timeout,
}

impl OllamaError {
    /// True when retrying a different job would fail the same way.
    pub fn is_unavailable(&self) -> bool {
        !matches!(self, OllamaError::BadAnswer(_) | OllamaError::Timeout)
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

impl OllamaClient {
    pub fn new(base_url: &str, num_ctx: u32) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            // A 14B model on a 12 GB GPU answers in 10-60 s; the first call also loads it.
            .timeout(Duration::from_secs(600))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            num_ctx,
            gpu: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Ask `model` one question and get back JSON that matches `schema`.
    /// temperature 0 makes the answer (nearly) repeatable for evals.
    pub async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
    ) -> Result<Value, OllamaError> {
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
    ) -> Result<Value, OllamaError> {
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "stream": false,
            "format": schema,
            // Qwen3 "thinks" by default. The thinking comes back separately
            // from the answer, so the answer is still only the JSON.
            "think": options.think,
            "options": {
                "temperature": 0,
                "num_ctx": options.num_ctx.unwrap_or(self.num_ctx)
            }
        });

        let _turn = self.gpu.lock().await;
        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| self.transport_error(e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status.as_u16() == 404 && body.contains("not found") {
                return Err(OllamaError::ModelMissing(model.to_string()));
            }
            return Err(OllamaError::Http {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }

        let chat: ChatResponse = response
            .json()
            .await
            .map_err(|e| OllamaError::BadAnswer(format!("unexpected response: {e}")))?;
        serde_json::from_str(&chat.message.content)
            .map_err(|e| OllamaError::BadAnswer(format!("not JSON: {e}")))
    }

    /// Names of the installed models, e.g. ["qwen3:14b", "qwen3:8b"].
    pub async fn models(&self) -> Result<Vec<String>, OllamaError> {
        let response = self
            .http
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| self.transport_error(e))?;
        if !response.status().is_success() {
            return Err(OllamaError::Http {
                status: response.status().as_u16(),
                body: String::new(),
            });
        }
        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| OllamaError::BadAnswer(e.to_string()))?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    fn transport_error(&self, error: reqwest::Error) -> OllamaError {
        if error.is_timeout() {
            OllamaError::Timeout
        } else {
            OllamaError::Unreachable(self.base_url.clone())
        }
    }
}
