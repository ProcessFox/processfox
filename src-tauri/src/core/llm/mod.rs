pub mod anthropic;
pub mod json_cleanup;
pub mod local_gguf;
pub mod openai;
pub mod openai_compat;
pub mod openrouter;
pub mod registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::error::CoreResult;
use super::tool::ToolSchema;

pub use registry::{ProviderId, ProviderRegistry};

/// A single turn in the conversation, as sent to the LLM. Most turns carry
/// only text content; assistant turns that invoked tools also carry
/// `tool_calls`, and the following user turn carries matching `tool_results`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    pub role: ChatRole,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResult>,
}

impl ChatTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

/// A model's request to invoke a specific tool with structured arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: JsonValue,
}

/// The outcome of executing a tool, paired to a prior `ToolCall` by `tool_use_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub model_id: String,
    pub system_prompt: Option<String>,
    pub turns: Vec<ChatTurn>,
    pub tools: Vec<ToolSchema>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
}

/// Token-usage numbers reported by a provider for a single generation.
/// `cached_input_tokens` and `cache_creation_input_tokens` are only populated
/// by providers that surface prompt-cache statistics (Anthropic always; OpenAI
/// reports `cached_input_tokens` only). Local inference reports exact prompt
/// and output counts.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
}

/// Events emitted by a provider during a single generation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum LlmEvent {
    TextDelta {
        text: String,
    },
    /// Chain-of-thought / reasoning content emitted in a separate channel
    /// from the visible answer (e.g. Gemma 4's `<|channel>thought`,
    /// DeepSeek's `<think>` blocks). Surfaced in the UI as a separate
    /// collapsible chip rather than inline in the assistant bubble.
    ReasoningDelta {
        text: String,
    },
    ToolCall(ToolCall),
    /// Token-usage report. Providers emit this at most once per generation,
    /// just before the terminal `Finish`. Some providers don't emit it at all
    /// (older OpenAI-compat backends without `stream_options` support).
    Usage(TokenUsage),
    Finish {
        reason: FinishReason,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    MaxTokens,
    ToolUse,
    Cancelled,
    Error,
}

/// Implementation-agnostic interface for producing text from an LLM.
///
/// `generate` streams `LlmEvent`s into `sink` and must respect `cancel`
/// promptly. It returns when the stream is exhausted, an unrecoverable error
/// occurs, or cancellation is observed.
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &'static str;

    async fn generate(
        &self,
        request: GenerateRequest,
        sink: mpsc::Sender<LlmEvent>,
        cancel: CancellationToken,
    ) -> CoreResult<()>;

    /// Verify that the currently stored API key is valid by issuing a cheap
    /// request to the provider (e.g. listing models). Returns Ok(()) on
    /// success, a descriptive `CoreError` otherwise.
    async fn validate(&self) -> CoreResult<()>;

    /// Whether this provider can use `GenerateRequest::tools`. Providers
    /// without tool-calling support should return false, and the ReAct loop
    /// will not hand them any tools. Defaults to false so adding tools to
    /// the trait is backward compatible.
    fn supports_tools(&self) -> bool {
        false
    }
}
