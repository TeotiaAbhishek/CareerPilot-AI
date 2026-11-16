//! A small client for a local Ollama server (https://ollama.com).
//! Only the two endpoints we need: `/api/chat` and `/api/tags`.

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ChatOptions, LlmError, Message, ToolCall, ToolSpec, Turn};

#[derive(Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    base_url: String,
    num_ctx: u32,
    /// One model call at a time. The scoring and drafting workers share this
    /// client, so they take turns on the GPU instead of fighting over it.
    gpu: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

/// One line of a streamed answer. The last one has `done: true`.
#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    message: Option<StreamMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct StreamMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
}

#[derive(Deserialize)]
struct StreamToolCall {
    function: StreamFunction,
}

#[derive(Deserialize)]
struct StreamFunction {
    name: String,
    #[serde(default)]
    arguments: Value,
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
    pub async fn chat_json_with(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
        options: ChatOptions,
    ) -> Result<Value, LlmError> {
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
        let response = self.post_chat(model, &body).await?;
        let chat: ChatResponse = response
            .json()
            .await
            .map_err(|e| LlmError::BadAnswer(format!("unexpected response: {e}")))?;
        serde_json::from_str(&chat.message.content)
            .map_err(|e| LlmError::BadAnswer(format!("not JSON: {e}")))
    }

    /// One streamed step of a conversation with tools (Ollama's native tool
    /// calling). Text goes to `on_text` as it arrives; tool calls come back
    /// in the `Turn`.
    pub async fn chat_tools(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSpec],
        options: ChatOptions,
        on_text: &mut (dyn FnMut(&str) + Send),
    ) -> Result<Turn, LlmError> {
        let mut body = json!({
            "model": model,
            "messages": messages.iter().map(message_json).collect::<Vec<_>>(),
            "stream": true,
            "think": options.think,
            "options": {
                // A little variety reads more naturally in a chat.
                "temperature": 0.2,
                "num_ctx": options.num_ctx.unwrap_or(self.num_ctx)
            }
        });
        if !tools.is_empty() {
            body["tools"] = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect();
        }

        let _turn = self.gpu.lock().await;
        let mut response = self.post_chat(model, &body).await?;

        // The answer arrives as one JSON object per line.
        let mut turn = Turn::default();
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            let chunk = response
                .chunk()
                .await
                .map_err(|e| self.transport_error(e))?;
            let Some(chunk) = chunk else { break };
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                if read_line(&line, &mut turn, on_text)? {
                    return Ok(turn);
                }
            }
        }
        if !buffer.is_empty() {
            read_line(&buffer, &mut turn, on_text)?;
        }
        Ok(turn)
    }

    async fn post_chat(&self, model: &str, body: &Value) -> Result<reqwest::Response, LlmError> {
        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(body)
            .send()
            .await
            .map_err(|e| self.transport_error(e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status.as_u16() == 404 && body.contains("not found") {
                return Err(LlmError::ModelMissing(format!(
                    "Model {model:?} is not installed. Run `ollama pull {model}`."
                )));
            }
            if status.as_u16() == 400 && body.contains("does not support tools") {
                return Err(LlmError::ModelMissing(format!(
                    "Model {model:?} can't call tools. Pick a qwen3 model in Settings."
                )));
            }
            return Err(LlmError::Http {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }
        Ok(response)
    }

    /// Names of the installed models, e.g. ["qwen3:14b", "qwen3:8b"].
    pub async fn models(&self) -> Result<Vec<String>, LlmError> {
        let response = self
            .http
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| self.transport_error(e))?;
        if !response.status().is_success() {
            return Err(LlmError::Http {
                status: response.status().as_u16(),
                body: String::new(),
            });
        }
        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| LlmError::BadAnswer(e.to_string()))?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    fn transport_error(&self, error: reqwest::Error) -> LlmError {
        if error.is_timeout() {
            LlmError::Timeout
        } else {
            LlmError::Unreachable(format!(
                "Ollama is not reachable at {}. Is `ollama serve` running?",
                self.base_url
            ))
        }
    }
}

/// A conversation message in Ollama's format.
fn message_json(message: &Message) -> Value {
    match message {
        Message::System(text) => json!({ "role": "system", "content": text }),
        Message::User(text) => json!({ "role": "user", "content": text }),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let mut m = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                m["tool_calls"] = tool_calls
                    .iter()
                    .map(|c| json!({ "function": { "name": c.name, "arguments": c.arguments } }))
                    .collect();
            }
            m
        }
        Message::Tool { name, content } => {
            json!({ "role": "tool", "tool_name": name, "content": content })
        }
    }
}

/// Adds one streamed line to `turn`. True when it was the last line.
fn read_line(
    line: &[u8],
    turn: &mut Turn,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<bool, LlmError> {
    let line = String::from_utf8_lossy(line);
    let line = line.trim();
    if line.is_empty() {
        return Ok(false);
    }
    let chunk: StreamChunk = serde_json::from_str(line)
        .map_err(|e| LlmError::BadAnswer(format!("unexpected stream line: {e}")))?;
    if let Some(error) = chunk.error {
        return Err(LlmError::BadAnswer(error));
    }
    if let Some(message) = chunk.message {
        if !message.content.is_empty() {
            on_text(&message.content);
            turn.content.push_str(&message.content);
        }
        turn.tool_calls
            .extend(message.tool_calls.into_iter().map(|c| ToolCall {
                name: c.function.name,
                arguments: c.function.arguments,
            }));
    }
    Ok(chunk.done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_text_and_tool_calls_from_stream_lines() {
        let mut turn = Turn::default();
        let mut seen = String::new();
        let mut on_text = |t: &str| seen.push_str(t);
        let lines = [
            r#"{"message":{"role":"assistant","content":"Let me "},"done":false}"#,
            r#"{"message":{"role":"assistant","content":"look."},"done":false}"#,
            r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"search_jobs","arguments":{"min_score":70}}}]},"done":false}"#,
            r#"{"message":{"role":"assistant","content":""},"done":true}"#,
        ];
        let mut done = false;
        for line in lines {
            done = read_line(line.as_bytes(), &mut turn, &mut on_text).unwrap();
        }
        assert!(done);
        assert_eq!(seen, "Let me look.");
        assert_eq!(turn.content, "Let me look.");
        assert_eq!(
            turn.tool_calls,
            vec![ToolCall {
                name: "search_jobs".into(),
                arguments: json!({ "min_score": 70 })
            }]
        );
    }

    #[test]
    fn a_stream_error_is_a_bad_answer() {
        let mut turn = Turn::default();
        let error = read_line(br#"{"error":"out of memory"}"#, &mut turn, &mut |_| {}).unwrap_err();
        assert!(matches!(error, LlmError::BadAnswer(m) if m == "out of memory"));
    }

    #[test]
    fn tool_results_go_back_with_the_tool_name() {
        let m = message_json(&Message::Tool {
            name: "pipeline".into(),
            content: "{}".into(),
        });
        assert_eq!(
            m,
            json!({ "role": "tool", "tool_name": "pipeline", "content": "{}" })
        );
    }
}
