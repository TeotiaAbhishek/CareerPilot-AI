//! Anthropic's Messages API (https://docs.claude.com), over plain HTTP:
//! there's no official Rust SDK.
//!
//! - JSON answers use structured outputs (`output_config.format`), so the
//!   answer always matches the schema.
//! - Tool calls stream as server-sent events, so Ask shows text as it comes.
//! - The answer's content blocks (thinking, text, tool_use) go back as-is on
//!   the next call, which keeps the model's reasoning and the tool ids.

use std::time::Duration;

use serde_json::{Value, json};

use super::{
    ChatOptions, LlmError, Message, ToolCall, ToolSpec, Turn, flatten_tools, strict_schema,
    usage::Usage,
};

const API_VERSION: &str = "2026-10-21";
const JSON_MAX_TOKENS: u32 = 16_000;
const CHAT_MAX_TOKENS: u32 = 8_000;
/// Busy (429, 529) and server errors are tried this many times in all.
const ATTEMPTS: u32 = 3;

#[derive(Clone)]
pub struct ClaudeClient {
    http: reqwest::Client,
    base_url: String,
    key: String,
}

impl ClaudeClient {
    pub fn new(base_url: &str, key: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            key: key.to_string(),
        })
    }

    /// One question, an answer that matches `schema`.
    pub async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &Value,
        options: ChatOptions,
    ) -> Result<(Value, Usage), LlmError> {
        let mut body = json!({
            "model": model,
            "max_tokens": JSON_MAX_TOKENS,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
            "output_config": {
                "format": { "type": "json_schema", "schema": strict_schema(schema) }
            }
        });
        apply_thinking(&mut body, model, options.think);

        let response: Value = self
            .post(&body)
            .await?
            .json()
            .await
            .map_err(|e| LlmError::BadAnswer(format!("unexpected response: {e}")))?;
        let usage = read_usage(&response["usage"]);
        check_stop(response["stop_reason"].as_str())?;
        let text: String = response["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        let value = serde_json::from_str(&text)
            .map_err(|e| LlmError::BadAnswer(format!("not JSON: {e}")))?;
        Ok((value, usage))
    }

    /// One streamed step of a conversation with tools.
    pub async fn chat_tools(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSpec],
        options: ChatOptions,
        on_text: &mut (dyn FnMut(&str) + Send),
    ) -> Result<(Turn, Usage), LlmError> {
        let (system, messages) = if tools.is_empty() {
            to_claude(&flatten_tools(messages))
        } else {
            to_claude(messages)
        };
        let mut body = json!({
            "model": model,
            "max_tokens": CHAT_MAX_TOKENS,
            "stream": true,
            "system": system,
            "messages": messages,
        });
        if !tools.is_empty() {
            body["tools"] = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters
                    })
                })
                .collect();
        }
        apply_thinking(&mut body, model, options.think);

        let mut response = self.post(&body).await?;
        let mut stream = Stream::default();
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| transport_error(&e))? {
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=end).collect();
                stream.line(&String::from_utf8_lossy(&line), on_text)?;
            }
        }
        if !buffer.is_empty() {
            stream.line(&String::from_utf8_lossy(&buffer), on_text)?;
        }
        stream.finish()
    }

    async fn post(&self, body: &Value) -> Result<reqwest::Response, LlmError> {
        let mut attempt = 1;
        loop {
            let response = self
                .http
                .post(format!("{}/v1/messages", self.base_url))
                .header("x-api-key", &self.key)
                .header("anthropic-version", API_VERSION)
                .json(body)
                .send()
                .await
                .map_err(|e| transport_error(&e))?;
            let status = response.status().as_u16();
            if response.status().is_success() {
                return Ok(response);
            }
            let text = response.text().await.unwrap_or_default();
            let busy = status == 429 || status == 529 || status >= 500;
            if busy && attempt < ATTEMPTS {
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                attempt += 1;
                continue;
            }
            return Err(http_error(
                status,
                &text,
                body["model"].as_str().unwrap_or(""),
            ));
        }
    }
}

/// Thinking on: the model's default (adaptive). Off: each model has its own
/// way to say so.
fn apply_thinking(body: &mut Value, model: &str, think: bool) {
    if think {
        return;
    }
    if model.starts_with("claude-sonnet-5-5") {
        body["thinking"] = json!({ "type": "between_tools" });
    } else if model.starts_with("claude-opus-5-5") {
        // Thinking can't be turned off on Opus 5.5: think as little as possible.
        body["output_config"]["effort"] = json!("low");
    } else {
        body["thinking"] = json!({ "type": "disabled" });
    }
}

fn check_stop(stop_reason: Option<&str>) -> Result<(), LlmError> {
    match stop_reason {
        Some("max_tokens") => Err(LlmError::BadAnswer(
            "the answer was cut off (too long)".into(),
        )),
        Some("refusal") => Err(LlmError::BadAnswer("Claude declined to answer".into())),
        _ => Ok(()),
    }
}

fn read_usage(usage: &Value) -> Usage {
    let n = |key: &str| usage[key].as_u64().unwrap_or(0) as u32;
    Usage {
        input_tokens: n("input_tokens")
            + n("cache_creation_input_tokens")
            + n("cache_read_input_tokens"),
        output_tokens: n("output_tokens"),
    }
}

fn transport_error(error: &reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Unreachable(format!(
            "Claude's API is not reachable ({error}). Check your internet connection."
        ))
    }
}

fn http_error(status: u16, body: &str, model: &str) -> LlmError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status {
        401 | 403 => LlmError::Unreachable(format!(
            "Claude rejected the API key ({message}). Check ANTHROPIC_API_KEY in backend/.env."
        )),
        404 => LlmError::ModelMissing(format!(
            "Claude has no model {model:?} ({message}). Pick another model in Settings."
        )),
        429 | 529 => LlmError::Unreachable(format!(
            "Claude is busy or your rate limit is reached ({message}). It will try again later."
        )),
        400 if message.contains("credit balance") => LlmError::Unreachable(format!(
            "Your Anthropic credit has run out ({message}). Add credit at console.anthropic.com."
        )),
        _ => LlmError::Http {
            status,
            body: message,
        },
    }
}

/// `messages` in Claude's format: the first system message on its own, the
/// rest as user/assistant turns (Claude merges nothing, so neighbours with
/// the same role are joined here).
fn to_claude(messages: &[Message]) -> (String, Vec<Value>) {
    let mut system = String::new();
    let mut turns: Vec<(&'static str, Vec<Value>)> = Vec::new();
    // Tool-use ids of the last assistant turn, waiting for their results.
    let mut pending: std::collections::VecDeque<String> = Default::default();
    let push =
        |turns: &mut Vec<(&'static str, Vec<Value>)>, role, block: Value| match turns.last_mut() {
            Some((last, blocks)) if *last == role => blocks.push(block),
            _ => turns.push((role, vec![block])),
        };

    for (i, message) in messages.iter().enumerate() {
        match message {
            Message::System(text) if system.is_empty() && i == 0 => system = text.clone(),
            // A later system message ("answer now") reads as a note from the app.
            Message::System(text) => push(&mut turns, "user", text_block(text)),
            Message::User(text) => push(&mut turns, "user", text_block(text)),
            Message::Assistant {
                content,
                tool_calls,
                raw,
            } => {
                pending.clear();
                let blocks = match raw.as_ref().and_then(Value::as_array) {
                    Some(raw) => {
                        // Keep thinking and text; keep only the tool calls we ran.
                        let mut kept = 0;
                        raw.iter()
                            .filter(|b| {
                                if b["type"] == "tool_use" {
                                    kept += 1;
                                    kept <= tool_calls.len()
                                } else {
                                    true
                                }
                            })
                            .cloned()
                            .collect::<Vec<_>>()
                    }
                    None => {
                        let mut blocks = Vec::new();
                        if !content.trim().is_empty() {
                            blocks.push(text_block(content));
                        }
                        for (n, call) in tool_calls.iter().enumerate() {
                            blocks.push(json!({
                                "type": "tool_use",
                                "id": format!("toolu_cp_{i}_{n}"),
                                "name": call.name,
                                "input": call.arguments
                            }));
                        }
                        blocks
                    }
                };
                for block in &blocks {
                    if block["type"] == "tool_use" {
                        pending.push_back(block["id"].as_str().unwrap_or_default().to_string());
                    }
                }
                for block in blocks {
                    push(&mut turns, "assistant", block);
                }
            }
            Message::Tool { content, .. } => {
                let id = pending.pop_front().unwrap_or_default();
                push(
                    &mut turns,
                    "user",
                    json!({ "type": "tool_result", "tool_use_id": id, "content": content }),
                );
            }
        }
    }
    let messages = turns
        .into_iter()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect();
    (system, messages)
}

fn text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

/// Builds the answer from the streamed events.
#[derive(Default)]
struct Stream {
    /// Content blocks by index, filled in as deltas arrive.
    blocks: Vec<Value>,
    /// Tool-call arguments arrive as JSON text in pieces.
    partial_json: Vec<String>,
    usage: Usage,
    stop_reason: Option<String>,
    done: bool,
}

impl Stream {
    /// One line of the event stream. Only `data:` lines carry anything.
    fn line(&mut self, line: &str, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<(), LlmError> {
        let Some(data) = line.trim().strip_prefix("data:") else {
            return Ok(());
        };
        let event: Value = serde_json::from_str(data.trim())
            .map_err(|e| LlmError::BadAnswer(format!("unexpected stream line: {e}")))?;
        let index = event["index"].as_u64().unwrap_or(0) as usize;
        match event["type"].as_str().unwrap_or("") {
            "message_start" => {
                let u = read_usage(&event["message"]["usage"]);
                self.usage.input_tokens = u.input_tokens;
                self.usage.output_tokens = u.output_tokens;
            }
            "content_block_start" => {
                while self.blocks.len() <= index {
                    self.blocks.push(Value::Null);
                    self.partial_json.push(String::new());
                }
                self.blocks[index] = event["content_block"].clone();
            }
            "content_block_delta" => {
                let Some(block) = self.blocks.get_mut(index) else {
                    return Ok(());
                };
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        let text = delta["text"].as_str().unwrap_or("");
                        on_text(text);
                        append(block, "text", text);
                    }
                    "thinking_delta" => {
                        append(block, "thinking", delta["thinking"].as_str().unwrap_or(""))
                    }
                    "signature_delta" => {
                        block["signature"] = delta["signature"].clone();
                    }
                    "input_json_delta" => self.partial_json[index]
                        .push_str(delta["partial_json"].as_str().unwrap_or("")),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(block) = self.blocks.get_mut(index)
                    && block["type"] == "tool_use"
                {
                    let text = self.partial_json[index].trim();
                    block["input"] = if text.is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(text).map_err(|e| {
                            LlmError::BadAnswer(format!("tool arguments are not JSON: {e}"))
                        })?
                    };
                }
            }
            "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                }
                if let Some(n) = event["usage"]["output_tokens"].as_u64() {
                    self.usage.output_tokens = n as u32;
                }
            }
            "message_stop" => self.done = true,
            "error" => {
                let message = event["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                return Err(match event["error"]["type"].as_str() {
                    Some("overloaded_error" | "rate_limit_error") => LlmError::Unreachable(
                        format!("Claude is busy ({message}). Try again in a minute."),
                    ),
                    _ => LlmError::BadAnswer(message.to_string()),
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Result<(Turn, Usage), LlmError> {
        if !self.done {
            return Err(LlmError::BadAnswer("the answer stopped half way".into()));
        }
        if self.stop_reason.as_deref() != Some("tool_use") {
            check_stop(self.stop_reason.as_deref())?;
        }
        let blocks: Vec<Value> = self.blocks.into_iter().filter(|b| !b.is_null()).collect();
        let content = blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        let tool_calls = blocks
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .map(|b| ToolCall {
                name: b["name"].as_str().unwrap_or_default().to_string(),
                arguments: b["input"].clone(),
            })
            .collect();
        Ok((
            Turn {
                content,
                tool_calls,
                raw: Some(Value::Array(blocks)),
            },
            self.usage,
        ))
    }
}

fn append(block: &mut Value, key: &str, text: &str) {
    let mut current = block[key].as_str().unwrap_or("").to_string();
    current.push_str(text);
    block[key] = Value::String(current);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> Result<(Turn, Usage, String), LlmError> {
        let mut stream = Stream::default();
        let mut seen = String::new();
        let mut on_text = |t: &str| seen.push_str(t);
        for line in lines {
            stream.line(line, &mut on_text)?;
        }
        let (turn, usage) = stream.finish()?;
        Ok((turn, usage, seen))
    }

    #[test]
    fn reads_thinking_text_and_tool_calls_from_the_stream() {
        let (turn, usage, seen) = feed(&[
            "event: message_start",
            r#"data: {"type":"message_start","message":{"usage":{"input_tokens":900,"output_tokens":1}}}"#,
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Check jobs."}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Let me "}}"#,
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"look."}}"#,
            r#"data: {"type":"content_block_stop","index":1}"#,
            r#"data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"search_jobs","input":{}}}"#,
            r#"data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"min_sc"}}"#,
            r#"data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"ore\": 70}"}}"#,
            r#"data: {"type":"content_block_stop","index":2}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}}"#,
            r#"data: {"type":"message_stop"}"#,
        ])
        .unwrap();
        assert_eq!(seen, "Let me look.");
        assert_eq!(turn.content, "Let me look.");
        assert_eq!(
            turn.tool_calls,
            vec![ToolCall {
                name: "search_jobs".into(),
                arguments: json!({ "min_score": 70 })
            }]
        );
        assert_eq!(
            usage,
            Usage {
                input_tokens: 900,
                output_tokens: 42
            }
        );
        let raw = turn.raw.unwrap();
        assert_eq!(
            raw[0],
            json!({ "type": "thinking", "thinking": "Check jobs.", "signature": "sig" })
        );
        assert_eq!(raw[2]["id"], "toolu_1");
    }

    #[test]
    fn a_stream_that_never_finishes_is_a_bad_answer() {
        let error = feed(&[
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        ])
        .unwrap_err();
        assert!(matches!(error, LlmError::BadAnswer(_)));
    }

    #[test]
    fn tool_results_pair_with_the_calls_that_ran() {
        let raw = json!([
            { "type": "thinking", "thinking": "", "signature": "s" },
            { "type": "tool_use", "id": "a", "name": "pipeline", "input": {} },
            { "type": "tool_use", "id": "b", "name": "my_profile", "input": {} }
        ]);
        let (system, messages) = to_claude(&[
            Message::System("Be brief.".into()),
            Message::User("How am I doing?".into()),
            Message::Assistant {
                content: String::new(),
                // Only the first call ran (the app runs at most N per round).
                tool_calls: vec![ToolCall {
                    name: "pipeline".into(),
                    arguments: json!({}),
                }],
                raw: Some(raw),
            },
            Message::Tool {
                name: "pipeline".into(),
                content: "{}".into(),
            },
            Message::System("Answer now.".into()),
        ]);
        assert_eq!(system, "Be brief.");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["content"].as_array().unwrap().len(), 2);
        assert_eq!(
            messages[2]["content"],
            json!([
                { "type": "tool_result", "tool_use_id": "a", "content": "{}" },
                { "type": "text", "text": "Answer now." }
            ])
        );
    }

    #[test]
    fn thinking_off_differs_per_model() {
        let mut body = json!({ "output_config": {} });
        apply_thinking(&mut body, "claude-haiku-5-5", false);
        assert_eq!(body["thinking"], json!({ "type": "disabled" }));
        let mut body = json!({ "output_config": {} });
        apply_thinking(&mut body, "claude-sonnet-5-5", false);
        assert_eq!(body["thinking"], json!({ "type": "between_tools" }));
        let mut body = json!({ "output_config": {} });
        apply_thinking(&mut body, "claude-sonnet-5-5", true);
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn errors_say_what_to_do() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert!(
            matches!(http_error(401, body, "m"), LlmError::Unreachable(m) if m.contains("ANTHROPIC_API_KEY"))
        );
        assert!(matches!(
            http_error(404, body, "claude-x"),
            LlmError::ModelMissing(_)
        ));
    }
}
