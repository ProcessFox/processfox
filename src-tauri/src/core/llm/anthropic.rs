use std::collections::HashMap;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    ChatRole, ChatTurn, FinishReason, GenerateRequest, LlmEvent, LlmProvider, TokenUsage, ToolCall,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::tool::ToolSchema;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

#[derive(Debug)]
pub struct AnthropicProvider {
    http: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new() -> CoreResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent("ProcessFox/0.1")
            .build()
            .map_err(|e| CoreError::Http(e.to_string()))?;
        Ok(Self { http })
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new().expect("Anthropic HTTP client must build")
    }
}

#[derive(Serialize)]
struct Request {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<JsonValue>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<JsonValue>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    MessageStart {
        message: MessageStartPayload,
    },
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockStartPayload,
    },
    Ping {},
    ContentBlockDelta {
        index: u32,
        delta: ContentDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: MessageDelta,
        #[serde(default)]
        usage: Option<UsagePayload>,
    },
    MessageStop {},
    Error {
        error: ApiErrorBody,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug)]
struct MessageStartPayload {
    #[serde(default)]
    usage: Option<UsagePayload>,
}

/// Anthropic's usage block. `input_tokens` is what we'll be billed full price
/// for; `cache_creation_input_tokens` was billed at 1.25× to *write* the
/// cache; `cache_read_input_tokens` was billed at 0.1× because it hit cache.
/// `output_tokens` arrives cumulatively in `message_delta`; the final value
/// is in the last delta.
#[derive(Deserialize, Debug, Default, Clone, Copy)]
struct UsagePayload {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlockStartPayload {
    Text {},
    ToolUse {
        id: String,
        name: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentDelta {
    TextDelta {
        text: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug)]
struct MessageDelta {
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ApiErrorBody {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Partial state of a tool_use content block being streamed in.
struct PendingToolUse {
    id: String,
    name: String,
    partial_json: String,
}

fn map_stop_reason(raw: Option<&str>) -> FinishReason {
    match raw {
        Some("tool_use") => FinishReason::ToolUse,
        Some("end_turn") | Some("stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::MaxTokens,
        _ => FinishReason::Stop,
    }
}

fn role_str(role: ChatRole) -> &'static str {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::System => "user",
    }
}

/// Convert our internal turns into Anthropic's message array. Assistant
/// turns that called tools become `[{text}, {tool_use}]` content arrays;
/// user turns carrying tool results become `[{tool_result}]` arrays.
fn to_wire_messages(turns: &[ChatTurn]) -> Vec<JsonValue> {
    let mut out = Vec::new();
    for turn in turns {
        if turn.role == ChatRole::System {
            continue;
        }
        let mut blocks: Vec<JsonValue> = Vec::new();
        if !turn.content.is_empty() {
            blocks.push(json!({ "type": "text", "text": turn.content }));
        }
        for tc in &turn.tool_calls {
            blocks.push(json!({
                "type": "tool_use",
                "id": tc.id,
                "name": tc.name,
                "input": tc.arguments,
            }));
        }
        for tr in &turn.tool_results {
            blocks.push(json!({
                "type": "tool_result",
                "tool_use_id": tr.tool_use_id,
                "content": tr.content,
                "is_error": tr.is_error,
            }));
        }
        if blocks.is_empty() {
            continue;
        }
        out.push(json!({
            "role": role_str(turn.role),
            "content": blocks,
        }));
    }
    out
}

fn tools_to_wire(tools: &[ToolSchema]) -> Vec<JsonValue> {
    tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        })
        .collect()
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate(&self) -> CoreResult<()> {
        let api_key = crate::core::secrets::get_api_key("anthropic")?
            .ok_or_else(|| CoreError::MissingApiKey("anthropic".to_string()))?;

        let resp = self
            .http
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &api_key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await
            .map_err(|e| CoreError::Http(e.to_string()))?;

        if resp.status().is_success() {
            return Ok(());
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(CoreError::Llm(format!(
            "Anthropic-Key abgelehnt ({status}): {}",
            body.chars().take(200).collect::<String>()
        )))
    }

    async fn generate(
        &self,
        request: GenerateRequest,
        sink: mpsc::Sender<LlmEvent>,
        cancel: CancellationToken,
    ) -> CoreResult<()> {
        let api_key = crate::core::secrets::get_api_key("anthropic")?
            .ok_or_else(|| CoreError::MissingApiKey("anthropic".to_string()))?;

        let body = Request {
            model: request.model_id.clone(),
            max_tokens: request.max_tokens,
            system: request.system_prompt.clone(),
            messages: to_wire_messages(&request.turns),
            stream: true,
            temperature: request.temperature,
            tools: tools_to_wire(&request.tools),
        };

        let response = tokio::select! {
            r = self
                .http
                .post(API_URL)
                .header("x-api-key", &api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .json(&body)
                .send() => r.map_err(|e| CoreError::Http(e.to_string()))?,
            _ = cancel.cancelled() => {
                let _ = sink.send(LlmEvent::Finish { reason: FinishReason::Cancelled }).await;
                return Err(CoreError::Cancelled);
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let msg = format!("Anthropic {status}: {text}");
            let _ = sink
                .send(LlmEvent::Error {
                    code: "http_error".to_string(),
                    message: msg.clone(),
                })
                .await;
            return Err(CoreError::Llm(msg));
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut pending_tools: HashMap<u32, PendingToolUse> = HashMap::new();
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

            let parsed: StreamEvent = match serde_json::from_str(&event.data) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(data = %event.data, error = %e, "failed to parse Anthropic SSE event");
                    continue;
                }
            };

            match parsed {
                StreamEvent::MessageStart { message } => {
                    if let Some(u) = message.usage {
                        merge_usage(&mut usage, &u, &mut usage_seen);
                    }
                }
                StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlockStartPayload::ToolUse { id, name },
                } => {
                    pending_tools.insert(
                        index,
                        PendingToolUse {
                            id,
                            name,
                            partial_json: String::new(),
                        },
                    );
                }
                StreamEvent::ContentBlockDelta {
                    index,
                    delta: ContentDelta::TextDelta { text },
                } if !text.is_empty() => {
                    // Text always flows into the assistant-message buffer; no
                    // need to distinguish by index since chunks within a
                    // text block are already in order.
                    let _ = index;
                    if sink.send(LlmEvent::TextDelta { text }).await.is_err() {
                        return Ok(());
                    }
                }
                StreamEvent::ContentBlockDelta {
                    index,
                    delta: ContentDelta::InputJsonDelta { partial_json },
                } => {
                    if let Some(pt) = pending_tools.get_mut(&index) {
                        pt.partial_json.push_str(&partial_json);
                    }
                }
                StreamEvent::ContentBlockStop { index } => {
                    if let Some(pt) = pending_tools.remove(&index) {
                        // An empty arguments payload is valid JSON-wise as
                        // "{}". Accept it.
                        let source = if pt.partial_json.is_empty() {
                            "{}".to_string()
                        } else {
                            pt.partial_json
                        };
                        let arguments: JsonValue = serde_json::from_str(&source)
                            .unwrap_or_else(|_| JsonValue::String(source.clone()));
                        let call = ToolCall {
                            id: pt.id,
                            name: pt.name,
                            arguments,
                        };
                        if sink.send(LlmEvent::ToolCall(call)).await.is_err() {
                            return Ok(());
                        }
                    }
                }
                StreamEvent::MessageDelta { delta, usage: u } => {
                    if let Some(u) = u {
                        merge_usage(&mut usage, &u, &mut usage_seen);
                    }
                    if let Some(reason) = delta.stop_reason.as_deref() {
                        emit_usage_once(&sink, &usage, usage_seen, &mut usage_emitted).await;
                        let _ = sink
                            .send(LlmEvent::Finish {
                                reason: map_stop_reason(Some(reason)),
                            })
                            .await;
                    }
                }
                StreamEvent::MessageStop {} => {
                    // MessageDelta typically emitted the final reason already; if not, send Stop.
                    emit_usage_once(&sink, &usage, usage_seen, &mut usage_emitted).await;
                    let _ = sink
                        .send(LlmEvent::Finish {
                            reason: FinishReason::Stop,
                        })
                        .await;
                    return Ok(());
                }
                StreamEvent::Error { error } => {
                    let msg = error.message.unwrap_or_else(|| "unknown error".into());
                    let code = error.kind.unwrap_or_else(|| "provider_error".into());
                    let _ = sink
                        .send(LlmEvent::Error {
                            code: code.clone(),
                            message: msg.clone(),
                        })
                        .await;
                    return Err(CoreError::Llm(format!("{code}: {msg}")));
                }
                _ => {}
            }
        }

        // Stream ended without explicit MessageStop (unusual).
        emit_usage_once(&sink, &usage, usage_seen, &mut usage_emitted).await;
        let _ = sink
            .send(LlmEvent::Finish {
                reason: FinishReason::Stop,
            })
            .await;
        Ok(())
    }
}

fn merge_usage(acc: &mut TokenUsage, src: &UsagePayload, seen: &mut bool) {
    if let Some(v) = src.input_tokens {
        acc.input_tokens = v;
        *seen = true;
    }
    if let Some(v) = src.output_tokens {
        acc.output_tokens = v;
        *seen = true;
    }
    if let Some(v) = src.cache_creation_input_tokens {
        acc.cache_creation_input_tokens = Some(v);
        *seen = true;
    }
    if let Some(v) = src.cache_read_input_tokens {
        acc.cached_input_tokens = Some(v);
        *seen = true;
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
