//! Shared implementation for OpenAI-compatible chat-completions APIs.
//!
//! Used by `openai` (OpenAI directly) and `openrouter` (OpenRouter proxy).
//! Both endpoints speak the same request/response shape, differing only in
//! base URL and a couple of recommended headers.

use std::collections::HashMap;

use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{ChatRole, ChatTurn, FinishReason, GenerateRequest, LlmEvent, TokenUsage, ToolCall};
use crate::core::error::{CoreError, CoreResult};
use crate::core::tool::ToolSchema;

pub struct OpenAiCompat {
    pub provider_id: &'static str,
    pub secret_key: &'static str,
    pub api_base: String,
    pub extra_headers: &'static [(&'static str, &'static str)],
    /// Whether to send `stream_options: { include_usage: true }` so the
    /// final stream chunk carries a `usage` block. Standard OpenAI and
    /// OpenRouter support this; older self-hosted OpenAI-compat servers may
    /// reject the unknown field, so it's opt-in per backend.
    pub include_usage: bool,
    pub http: reqwest::Client,
}

#[derive(Serialize)]
struct Request {
    model: String,
    messages: Vec<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<JsonValue>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Deserialize, Debug)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<Choice>,
    /// Present in the final chunk when `stream_options.include_usage` was
    /// requested. OpenAI emits one chunk with empty `choices` carrying the
    /// usage totals just before `[DONE]`.
    #[serde(default)]
    usage: Option<UsagePayload>,
}

#[derive(Deserialize, Debug)]
struct UsagePayload {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Deserialize, Debug)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}

#[derive(Deserialize, Debug, Default)]
struct DeltaToolCall {
    /// Index identifying which slot of the assistant's tool_calls array this
    /// fragment belongs to. Critical when the model calls multiple tools.
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<DeltaFunction>,
}

#[derive(Deserialize, Debug, Default)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// In-progress tool call assembled from streamed deltas.
#[derive(Default, Debug)]
struct PendingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments_raw: String,
}

fn role_str(role: ChatRole) -> &'static str {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::System => "system",
    }
}

fn map_finish(raw: Option<&str>) -> FinishReason {
    match raw {
        Some("stop") => FinishReason::Stop,
        Some("length") => FinishReason::MaxTokens,
        Some("tool_calls") => FinishReason::ToolUse,
        _ => FinishReason::Stop,
    }
}

fn tools_to_wire(tools: &[ToolSchema]) -> Vec<JsonValue> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        })
        .collect()
}

/// Convert our internal turns into OpenAI's messages array. Assistant turns
/// that called tools become assistant messages with `tool_calls`; tool
/// results become standalone `role: "tool"` messages with `tool_call_id`.
fn compose_wire(request: &GenerateRequest) -> Vec<JsonValue> {
    let mut out: Vec<JsonValue> = Vec::new();
    if let Some(sys) = request.system_prompt.as_deref() {
        if !sys.trim().is_empty() {
            out.push(json!({ "role": "system", "content": sys }));
        }
    }
    for turn in &request.turns {
        if turn.role == ChatRole::System {
            continue;
        }

        // Tool results become their own `role: "tool"` messages, one per
        // result, paired by `tool_call_id`.
        if !turn.tool_results.is_empty() {
            for tr in &turn.tool_results {
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": tr.tool_use_id,
                    "content": tr.content,
                }));
            }
            // If a tool-results turn also has text content, fall through to
            // emit it below as a regular user/assistant message.
            if turn.content.is_empty() && turn.tool_calls.is_empty() {
                continue;
            }
        }

        let mut msg = json!({ "role": role_str(turn.role) });
        let obj = msg.as_object_mut().unwrap();
        obj.insert("content".into(), JsonValue::String(turn.content.clone()));
        if !turn.tool_calls.is_empty() && turn.role == ChatRole::Assistant {
            // OpenAI stipulates `content` may be null when tool_calls is
            // present; ours can be empty string which is fine.
            let calls: Vec<JsonValue> = turn
                .tool_calls
                .iter()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": serde_json::to_string(&tc.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        }
                    })
                })
                .collect();
            obj.insert("tool_calls".into(), JsonValue::Array(calls));
        }
        out.push(msg);
    }
    out
}

impl OpenAiCompat {
    pub fn new(
        provider_id: &'static str,
        secret_key: &'static str,
        api_base: &str,
        extra_headers: &'static [(&'static str, &'static str)],
        include_usage: bool,
    ) -> CoreResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent("ProcessFox/0.1")
            .build()
            .map_err(|e| CoreError::Http(e.to_string()))?;
        Ok(Self {
            provider_id,
            secret_key,
            api_base: api_base.to_string(),
            extra_headers,
            include_usage,
            http,
        })
    }

    /// Build from an existing HTTP client — avoids creating a new connection
    /// pool when the caller already holds one (e.g. `CustomOpenAiProvider`).
    pub fn from_parts(
        provider_id: &'static str,
        secret_key: &'static str,
        api_base: String,
        extra_headers: &'static [(&'static str, &'static str)],
        include_usage: bool,
        http: reqwest::Client,
    ) -> Self {
        Self {
            provider_id,
            secret_key,
            api_base,
            extra_headers,
            include_usage,
            http,
        }
    }

    fn apply_extras(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in self.extra_headers {
            req = req.header(*k, *v);
        }
        req
    }

    pub async fn validate(&self) -> CoreResult<()> {
        let api_key = crate::core::secrets::get_api_key(self.secret_key)?
            .ok_or_else(|| CoreError::MissingApiKey(self.secret_key.to_string()))?;

        let url = format!("{}/models", self.api_base);
        let req = self
            .http
            .get(&url)
            .header("authorization", format!("Bearer {api_key}"));
        let req = self.apply_extras(req);

        let resp = req
            .send()
            .await
            .map_err(|e| CoreError::Http(e.to_string()))?;

        if resp.status().is_success() {
            return Ok(());
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(CoreError::Llm(format!(
            "{}-Key abgelehnt ({status}): {}",
            self.provider_id,
            body.chars().take(200).collect::<String>()
        )))
    }

    pub async fn generate(
        &self,
        request: GenerateRequest,
        sink: mpsc::Sender<LlmEvent>,
        cancel: CancellationToken,
    ) -> CoreResult<()> {
        let api_key = crate::core::secrets::get_api_key(self.secret_key)?
            .ok_or_else(|| CoreError::MissingApiKey(self.secret_key.to_string()))?;

        let body = Request {
            model: request.model_id.clone(),
            messages: compose_wire(&request),
            max_tokens: Some(request.max_tokens),
            stream: true,
            stream_options: if self.include_usage {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
            temperature: request.temperature,
            tools: tools_to_wire(&request.tools),
        };

        let url = format!("{}/chat/completions", self.api_base);
        let req = self
            .http
            .post(&url)
            .header("authorization", format!("Bearer {api_key}"))
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body);
        let req = self.apply_extras(req);

        let response = tokio::select! {
            r = req.send() => r.map_err(|e| CoreError::Http(e.to_string()))?,
            _ = cancel.cancelled() => {
                let _ = sink
                    .send(LlmEvent::Finish { reason: FinishReason::Cancelled })
                    .await;
                return Err(CoreError::Cancelled);
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let msg = format!("{} {status}: {text}", self.provider_id);
            let _ = sink
                .send(LlmEvent::Error {
                    code: "http_error".to_string(),
                    message: msg.clone(),
                })
                .await;
            return Err(CoreError::Llm(msg));
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut pending: HashMap<u32, PendingToolCall> = HashMap::new();
        let mut usage = TokenUsage::default();
        let mut usage_seen = false;
        let mut usage_emitted = false;

        while let Some(event) = tokio::select! {
            e = stream.next() => e,
            _ = cancel.cancelled() => None,
        } {
            if cancel.is_cancelled() {
                let _ = sink
                    .send(LlmEvent::Finish {
                        reason: FinishReason::Cancelled,
                    })
                    .await;
                return Err(CoreError::Cancelled);
            }

            let event = match event {
                Ok(e) => e,
                Err(e) => {
                    let msg = format!("SSE decode error: {e}");
                    let _ = sink
                        .send(LlmEvent::Error {
                            code: "sse_error".into(),
                            message: msg.clone(),
                        })
                        .await;
                    return Err(CoreError::Llm(msg));
                }
            };

            if event.data.is_empty() {
                continue;
            }
            if event.data.trim() == "[DONE]" {
                flush_pending_tool_calls(&mut pending, &sink).await;
                emit_usage_once(&sink, &usage, usage_seen, &mut usage_emitted).await;
                let _ = sink
                    .send(LlmEvent::Finish {
                        reason: FinishReason::Stop,
                    })
                    .await;
                return Ok(());
            }

            let chunk: StreamChunk = match serde_json::from_str(&event.data) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        provider = self.provider_id,
                        data = %event.data,
                        error = %e,
                        "failed to parse SSE chunk"
                    );
                    continue;
                }
            };

            if let Some(u) = chunk.usage.as_ref() {
                merge_openai_usage(&mut usage, u, &mut usage_seen);
            }

            for choice in chunk.choices {
                if let Some(text) = choice.delta.content {
                    if !text.is_empty() && sink.send(LlmEvent::TextDelta { text }).await.is_err() {
                        return Ok(());
                    }
                }
                for delta_call in choice.delta.tool_calls {
                    let entry = pending.entry(delta_call.index).or_default();
                    if let Some(id) = delta_call.id {
                        entry.id = Some(id);
                    }
                    if let Some(func) = delta_call.function {
                        if let Some(name) = func.name {
                            if !name.is_empty() {
                                entry.name = Some(name);
                            }
                        }
                        if let Some(arg) = func.arguments {
                            entry.arguments_raw.push_str(&arg);
                        }
                    }
                }
                if let Some(reason) = choice.finish_reason.as_deref() {
                    if reason == "tool_calls" {
                        flush_pending_tool_calls(&mut pending, &sink).await;
                    }
                    let _ = sink
                        .send(LlmEvent::Finish {
                            reason: map_finish(Some(reason)),
                        })
                        .await;
                }
            }
        }

        flush_pending_tool_calls(&mut pending, &sink).await;
        emit_usage_once(&sink, &usage, usage_seen, &mut usage_emitted).await;
        let _ = sink
            .send(LlmEvent::Finish {
                reason: FinishReason::Stop,
            })
            .await;
        Ok(())
    }
}

fn merge_openai_usage(acc: &mut TokenUsage, src: &UsagePayload, seen: &mut bool) {
    if let Some(v) = src.prompt_tokens {
        acc.input_tokens = v;
        *seen = true;
    }
    if let Some(v) = src.completion_tokens {
        acc.output_tokens = v;
        *seen = true;
    }
    if let Some(details) = src.prompt_tokens_details.as_ref() {
        if let Some(v) = details.cached_tokens {
            acc.cached_input_tokens = Some(v);
            *seen = true;
        }
    }
}

async fn emit_usage_once(
    sink: &mpsc::Sender<LlmEvent>,
    usage: &TokenUsage,
    seen: bool,
    emitted: &mut bool,
) {
    if *emitted || !seen {
        return;
    }
    *emitted = true;
    let _ = sink.send(LlmEvent::Usage(*usage)).await;
}

async fn flush_pending_tool_calls(
    pending: &mut HashMap<u32, PendingToolCall>,
    sink: &mpsc::Sender<LlmEvent>,
) {
    if pending.is_empty() {
        return;
    }
    let mut entries: Vec<(u32, PendingToolCall)> = pending.drain().collect();
    entries.sort_by_key(|(i, _)| *i);
    for (_, pt) in entries {
        let Some(id) = pt.id else { continue };
        let Some(name) = pt.name else { continue };
        let arguments: JsonValue = if pt.arguments_raw.trim().is_empty() {
            json!({})
        } else {
            super::json_cleanup::extract_json_value(&pt.arguments_raw)
        };
        let _ = sink
            .send(LlmEvent::ToolCall(ToolCall {
                id,
                name,
                arguments,
            }))
            .await;
    }
}

impl std::fmt::Debug for OpenAiCompat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompat")
            .field("provider_id", &self.provider_id)
            .field("api_base", &self.api_base)
            .finish()
    }
}

// `ChatTurn` import is only used by the wire composer above; keeping the
// reference here so a future refactor that drops compose_wire still leaves
// the import accounted for.
const _: fn(&ChatTurn) = |_| {};
