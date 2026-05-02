pub mod registry;
pub mod tools;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use super::error::CoreResult;

pub use registry::ToolRegistry;

/// Build the `, modified <ISO>` suffix that gets appended to every
/// content-read tool's header line — e.g. `--- foo.md (1006 bytes, modified
/// 2026-05-02T10:31:00Z) ---`. The mtime lets the freshness tracker
/// bootstrap a precise baseline when scanning chat history after a process
/// restart. Returns an empty string if metadata can't be read; that just
/// degrades the bootstrap to a "current time" fallback for that file.
///
/// Format is RFC 3339 UTC with seconds resolution. Keep stable —
/// `core/chat/freshness.rs::parse_mtime_from_header` depends on it.
pub fn mtime_suffix(path: &Path) -> String {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            format!(", modified {}", dt.format("%Y-%m-%dT%H:%M:%SZ"))
        }
        Err(_) => String::new(),
    }
}

/// JSON-Schema description of a tool, used to tell the LLM what tools are
/// available and what shape their arguments take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    /// A JSON-Schema (draft-2020-12-ish) describing the tool's input.
    pub input_schema: JsonValue,
}

/// Runtime context handed to each tool on execution. Tools must use
/// `agent_folder` as the sandbox root; writes and reads outside it are
/// rejected in `ensure_in_agent_folder`.
///
/// `channel` is the Tauri event channel for the current chat run, formatted
/// as `chat:run:{run_id}`. Long-running tools (fan-outs) emit progress
/// events on it. `tool_call_id` is the id of the *current* tool call —
/// progress events tag it so the UI can scope them to the right card.
/// `cancel` is the chat run's cancellation token — tools that loop must
/// poll it between iterations to stop promptly.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub agent_id: String,
    pub agent_folder: PathBuf,
    pub app: AppHandle,
    pub channel: String,
    pub tool_call_id: String,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    /// Text payload handed back to the LLM. Keep it compact — it's counted
    /// toward the context window.
    pub content: String,
    /// Optional structured payload for the UI to render (e.g. a list of
    /// files, a diff). Not sent to the LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_hint: Option<JsonValue>,
}

impl ToolOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ui_hint: None,
        }
    }
}

/// Human-in-the-loop preview shape — what the chat runner shows to the user
/// before a write tool actually executes. The runner emits this in a
/// `RunEvent::HitlRequest` and waits for the user's `HitlDecision`.
#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum HitlPreview {
    /// Append text to an existing file (or create it if missing).
    AppendToFile {
        path: String,
        content: String,
        /// True when the target file does not yet exist; UI can surface
        /// "neu erstellen" vs "anhängen".
        creates_file: bool,
        /// Last few lines of the existing file (max ~600 chars). Helps the
        /// reviewer spot a format mismatch before approving. `None` for new
        /// files or when the tail can't be read.
        #[serde(skip_serializing_if = "Option::is_none")]
        existing_tail: Option<String>,
    },
    /// Create a new .docx (or overwrite an existing one). The preview shows
    /// a plaintext rendering of the first few blocks so the user can sanity
    /// check structure before approving.
    WriteDocx {
        path: String,
        block_count: usize,
        preview_text: String,
        creates_file: bool,
    },
    /// Append blocks to an existing .docx (or create one if missing). The
    /// existing document is round-tripped through docx-rs so its formatting
    /// is preserved as far as the parser supports it.
    AppendToDocx {
        path: String,
        block_count: usize,
        preview_text: String,
        creates_file: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        existing_tail: Option<String>,
    },
    /// Replace the entire contents of a text or markdown file. The frontend
    /// renders `before` and `after` as a unified diff so the user can spot
    /// every line that changes.
    RewriteFile {
        path: String,
        before: String,
        after: String,
        creates_file: bool,
    },
    /// Update one or more cells in an Excel workbook. The frontend shows a
    /// small table cell-by-cell so the user can verify each change before
    /// approving.
    UpdateCells {
        path: String,
        sheet: String,
        changes: Vec<CellChange>,
    },
    /// Create a new Excel workbook (or overwrite an existing one) from a
    /// rectangular `rows` payload. The frontend shows the rows as a small
    /// table; on overwrite it warns that existing data will be lost.
    WriteXlsx {
        path: String,
        sheet: String,
        rows: Vec<Vec<String>>,
        creates_file: bool,
    },
    /// Generate a new .docx by filling `{{placeholder}}` slots in an existing
    /// template. The preview shows the substitution table plus any
    /// placeholders the template defines that the LLM did not provide.
    WriteDocxFromTemplate {
        template_path: String,
        output_path: String,
        replacements: Vec<TemplateReplacement>,
        /// Placeholders the tool found in the template's document.xml. UI
        /// can highlight any that don't appear in `replacements`.
        template_placeholders: Vec<String>,
        creates_file: bool,
    },
    /// Fan-out a per-row LLM generation into a target column of an Excel
    /// workbook. The preview shows the affected row count, what worker
    /// configuration will run, and a few sample rendered prompts so the
    /// reviewer can spot template mistakes before the batch starts.
    DelegateIntoXlsxColumn {
        path: String,
        sheet: String,
        target_column: String,
        target_creates_column: bool,
        row_count: u32,
        worker_label: String,
        sample_prompts: Vec<DelegationSamplePrompt>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationSamplePrompt {
    pub row_label: String,
    pub rendered_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateReplacement {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellChange {
    pub cell: String,
    pub before: String,
    pub after: String,
}

/// Decision returned for a pending HITL request.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum HitlDecision {
    Approve,
    Reject {
        #[serde(default)]
        reason: Option<String>,
    },
}

/// Individual tool. Stateless — each call gets a fresh `ToolContext`. Tools
/// must be `Send + Sync` because the registry is cloned into every chat run.
#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    fn schema(&self) -> ToolSchema;

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput>;

    /// Probe before execution: if the tool would mutate the agent's folder,
    /// return a preview. The chat runner emits this to the UI as a HITL
    /// request and only calls `execute` after the user approves. Read-only
    /// tools return `None` (the default). `ctx` lets the tool read existing
    /// file state so the preview can show before/after context.
    fn requires_approval(&self, _input: &JsonValue, _ctx: &ToolContext) -> Option<HitlPreview> {
        None
    }
}
