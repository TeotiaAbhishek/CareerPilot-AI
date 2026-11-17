//! OpenAI's Responses API (https://platform.openai.com/docs), over plain HTTP.
//!
//! - JSON answers use structured outputs (`text.format`, strict), so the
//!   answer always matches the schema.
//! - Tool calls stream as server-sent events; the final event carries the
//!   whole answer, which goes back as-is on the next call (`store: false`
//!   with encrypted reasoning, so OpenAI keeps nothing between calls).

use std::time::Duration;

use serde_json::{Value, json};

use super::{
    ChatOptions, LlmError, Message, ToolCall, ToolSpec, Turn, flatten_tools, strict_schema,
    usage::Usage,
};

const MAX_OUTPUT_TOKENS: u32 = 16_000;
const ATTEMPTS: u32 = 3;

#[derive(Clone)]
pub struct OpenAiClient {
    http: reqwest::Client,
    base_url: String,
    key: String,
}

impl OpenAiClient {
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
        let body = json!({
            "model": model,
            "instructions": system,
            "input": [{ "role": "user", "content": user }],
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "answer",
                    "schema": strict_schema(schema),
                    "strict": true
                }
            },
            "reasoning": { "effort": effort(model, options.think) },
            "max_output_tokens": MAX_OUTPUT_TOKENS,
            "store": false
        });
        let response: Value = self
            .post(&body)
            .await?
            .json()
            .await
            .map_err(|e| LlmError::BadAnswer(format!("unexpected response: {e}")))?;
        let usage = read_usage(&response["usage"]);
        check_status(&response)?;
        let text = output_text(&response["output"])?;
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
        let (instructions, input) = if tools.is_empty() {
            to_openai(&flatten_tools(messages))
        } else {
            to_openai(messages)
        };
        let mut body = json!({
            "model": model,
            "instructions": instructions,
            "input": input,
            "reasoning": { "effort": effort(model, options.think) },
            "max_output_tokens": MAX_OUTPUT_TOKENS,
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "stream": true
        });
        if !tools.is_empty() {
            body["tools"] = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                        "strict": false
                    })
                })
                .collect();
        }

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
                .post(format!("{}/v1/responses", self.base_url))
                .bearer_auth(&self.key)
                .json(body)
                .send()
                .await
                .map_err(|e| transport_error(&e))?;
            let status = response.status().as_u16();
            if response.status().is_success() {
                return Ok(response);
            }
            let text = response.text().await.unwrap_or_default();
            let quota = text.contains("insufficient_quota");
            if (status == 429 && !quota || status >= 500) && attempt < ATTEMPTS {
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

/// How hard the model reasons. The small models can skip it; the larger
/// ones only go down to "low".
fn effort(model: &str, think: bool) -> &'static str {
    if think {
        "medium"
    } else if model.starts_with("gpt-6-luna") || model.starts_with("gpt-6.1-terra") {
        "none"
    } else {
        "low"
    }
}

fn check_status(response: &Value) -> Result<(), LlmError> {
    match response["status"].as_str() {
        Some("incomplete") => Err(LlmError::BadAnswer(format!(
            "the answer was cut off ({})",
            response["incomplete_details"]["reason"]
                .as_str()
                .unwrap_or("unknown reason")
        ))),
        Some("failed") => Err(LlmError::BadAnswer(
            response["error"]["message"]
                .as_str()
                .unwrap_or("the response failed")
                .to_string(),
        )),
        _ => Ok(()),
    }
}

/// The answer's text, from the message items of `output`.
fn output_text(output: &Value) -> Result<String, LlmError> {
    let mut text = String::new();
    for item in output.as_array().into_iter().flatten() {
        if item["type"] != "message" {
            continue;
        }
        for part in item["content"].as_array().into_iter().flatten() {
            match part["type"].as_str() {
                Some("output_text") => text.push_str(part["text"].as_str().unwrap_or("")),
                Some("refusal") => {
                    return Err(LlmError::BadAnswer(format!(
                        "OpenAI declined to answer: {}",
                        part["refusal"].as_str().unwrap_or("")
                    )));
                }
                _ => {}
            }
        }
    }
    Ok(text)
}

fn read_usage(usage: &Value) -> Usage {
    Usage {
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0) as u32,
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0) as u32,
    }
}

fn transport_error(error: &reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Unreachable(format!(
            "OpenAI's API is not reachable ({error}). Check your internet connection."
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
            "OpenAI rejected the API key ({message}). Check OPENAI_API_KEY in backend/.env."
        )),
        404 => LlmError::ModelMissing(format!(
            "OpenAI has no model {model:?} ({message}). Pick another model in Settings."
        )),
        429 if body.contains("insufficient_quota") => LlmError::Unreachable(format!(
            "Your OpenAI credit has run out ({message}). Add credit at platform.openai.com."
        )),
        429 => LlmError::Unreachable(format!(
            "OpenAI's rate limit is reached ({message}). It will try again later."
        )),
        _ => LlmError::Http {
            status,
            body: message,
        },
    }
}

/// `messages` as Responses API input: the first system message becomes the
/// instructions, later ones developer messages.
fn to_openai(messages: &[Message]) -> (String, Vec<Value>) {
    let mut instructions = String::new();
    let mut input: Vec<Value> = Vec::new();
    let mut pending: std::collections::VecDeque<String> = Default::default();

    for (i, message) in messages.iter().enumerate() {
        match message {
            Message::System(text) if i == 0 => instructions = text.clone(),
            Message::System(text) => input.push(json!({ "role": "developer", "content": text })),
            Message::User(text) => input.push(json!({ "role": "user", "content": text })),
            Message::Assistant {
                content,
                tool_calls,
                raw,
            } => {
                pending.clear();
                let items: Vec<Value> = match raw.as_ref().and_then(Value::as_array) {
                    Some(raw) => {
                        let mut kept = 0;
                        raw.iter()
                            .filter(|item| {
                                if item["type"] == "function_call" {
                                    kept += 1;
                                    kept <= tool_calls.len()
                                } else {
                                    true
                                }
                            })
                            .cloned()
                            .collect()
                    }
                    None => {
                        let mut items = Vec::new();
                        if !content.trim().is_empty() {
                            items.push(json!({ "role": "assistant", "content": content }));
                        }
                        for (n, call) in tool_calls.iter().enumerate() {
                            items.push(json!({
                                "type": "function_call",
                                "call_id": format!("call_cp_{i}_{n}"),
                                "name": call.name,
                                "arguments": call.arguments.to_string()
                            }));
                        }
                        items
                    }
                };
                for item in &items {
                    if item["type"] == "function_call" {
                        pending.push_back(item["call_id"].as_str().unwrap_or_default().to_string());
                    }
                }
                input.extend(items);
            }
            Message::Tool { content, .. } => {
                let call_id = pending.pop_front().unwrap_or_default();
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": content
                }));
            }
        }
    }
    (instructions, input)
}

/// Builds the answer from the streamed events. Text deltas go to the page;
/// the `response.completed` event has the whole answer.
#[derive(Default)]
struct Stream {
    completed: Option<Value>,
}

impl Stream {
    fn line(&mut self, line: &str, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<(), LlmError> {
        let Some(data) = line.trim().strip_prefix("data:") else {
            return Ok(());
        };
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(());
        }
        let event: Value = serde_json::from_str(data)
            .map_err(|e| LlmError::BadAnswer(format!("unexpected stream line: {e}")))?;
        match event["type"].as_str().unwrap_or("") {
            "response.output_text.delta" => on_text(event["delta"].as_str().unwrap_or("")),
            "response.completed" | "response.incomplete" => {
                self.completed = Some(event["response"].clone())
            }
            "response.failed" => {
                return Err(LlmError::BadAnswer(
                    event["response"]["error"]["message"]
                        .as_str()
                        .unwrap_or("the response failed")
                        .to_string(),
                ));
            }
            "error" => {
                return Err(LlmError::BadAnswer(
                    event["message"]
                        .as_str()
                        .or(event["error"]["message"].as_str())
                        .unwrap_or("unknown error")
                        .to_string(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Result<(Turn, Usage), LlmError> {
        let Some(response) = self.completed else {
            return Err(LlmError::BadAnswer("the answer stopped half way".into()));
        };
        let usage = read_usage(&response["usage"]);
        let output = response["output"].as_array().cloned().unwrap_or_default();
        let tool_calls: Vec<ToolCall> = output
            .iter()
            .filter(|item| item["type"] == "function_call")
            .map(|item| {
                let arguments = item["arguments"].as_str().unwrap_or("{}");
                Ok(ToolCall {
                    name: item["name"].as_str().unwrap_or_default().to_string(),
                    arguments: serde_json::from_str(arguments).map_err(|e| {
                        LlmError::BadAnswer(format!("tool arguments are not JSON: {e}"))
                    })?,
                })
            })
            .collect::<Result<_, LlmError>>()?;
        if tool_calls.is_empty() {
            check_status(&response)?;
        }
        let content = output_text(&response["output"])?;
        Ok((
            Turn {
                content,
                tool_calls,
                raw: Some(Value::Array(output)),
            },
            usage,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_text_and_tool_calls_from_the_stream() {
        let mut stream = Stream::default();
        let mut seen = String::new();
        let mut on_text = |t: &str| seen.push_str(t);
        let lines = [
            "event: response.output_text.delta",
            r#"data: {"type":"response.output_text.delta","delta":"Let me "}"#,
            r#"data: {"type":"response.output_text.delta","delta":"look."}"#,
            "",
            r#"data: {"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":800,"output_tokens":30},"output":[
                {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"xyz"},
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Let me look."}]},
                {"type":"function_call","call_id":"call_1","name":"search_jobs","arguments":"{\"min_score\":70}"}
            ]}}"#,
        ];
        for line in lines {
            // The test's completed event spans lines; join them as the stream would.
            stream.line(&line.replace('\n', ""), &mut on_text).unwrap();
        }
        let (turn, usage) = stream.finish().unwrap();
        assert_eq!(seen, "Let me look.");
        assert_eq!(turn.content, "Let me look.");
        assert_eq!(
            turn.tool_calls,
            vec![ToolCall {
                name: "search_jobs".into(),
                arguments: json!({ "min_score": 70 })
            }]
        );
        assert_eq!(usage.input_tokens, 800);
        assert_eq!(turn.raw.unwrap().as_array().unwrap().len(), 3);
    }

    #[test]
    fn tool_outputs_pair_with_the_calls_that_ran() {
        let raw = json!([
            { "type": "reasoning", "id": "rs_1", "encrypted_content": "x" },
            { "type": "function_call", "call_id": "a", "name": "pipeline", "arguments": "{}" },
            { "type": "function_call", "call_id": "b", "name": "my_profile", "arguments": "{}" }
        ]);
        let (instructions, input) = to_openai(&[
            Message::System("Be brief.".into()),
            Message::User("How am I doing?".into()),
            Message::Assistant {
                content: String::new(),
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
        ]);
        assert_eq!(instructions, "Be brief.");
        assert_eq!(input.len(), 4);
        assert_eq!(input[2]["call_id"], "a");
        assert_eq!(
            input[3],
            json!({ "type": "function_call_output", "call_id": "a", "output": "{}" })
        );
    }

    #[test]
    fn refusals_and_cut_off_answers_are_bad_answers() {
        let refused =
            json!([{ "type": "message", "content": [{ "type": "refusal", "refusal": "no" }] }]);
        assert!(output_text(&refused).is_err());
        let cut = json!({ "status": "incomplete", "incomplete_details": { "reason": "max_output_tokens" } });
        assert!(check_status(&cut).is_err());
    }

    #[test]
    fn small_models_can_skip_reasoning() {
        assert_eq!(effort("gpt-6-luna", false), "none");
        assert_eq!(effort("gpt-6.1-sol", false), "low");
        assert_eq!(effort("gpt-6.1-sol", true), "medium");
    }
}
