use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::core::error::{CommandError, CoreError};
use crate::core::preview::docx::docx_to_html;
use crate::core::preview::pptx::{pptx_preview, SlidePreview};
use crate::core::preview::xlsx::{xlsx_preview, XlsxPreview};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxPreview {
    /// Sanitised, body-only HTML. Render in a sandboxed iframe — we never
    /// emit `<script>`, but defense in depth is cheap.
    pub html: String,
}

/// Convert a .docx inside the agent's folder to a body-HTML preview.
/// Heavy work (zip + XML walk) runs on a blocking thread so the Tauri
/// runtime stays responsive even on big documents.
#[tauri::command]
pub async fn preview_docx(
    agent_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<DocxPreview, CommandError> {
    let agent = state.agent_repo().get(&agent_id)?;
    let folder = agent.folder.ok_or_else(|| {
        CommandError::new(
            "agent_has_no_folder",
            "Für diesen Agenten ist kein Ordner konfiguriert.",
        )
    })?;
    let target = ensure_in_agent_folder(&folder, &PathBuf::from(&path))?;
    if !target.is_file() {
        return Err(CommandError::new(
            "not_a_file",
            "Der angefragte Pfad ist keine Datei.",
        ));
    }

    let target_for_blocking = target.clone();
    let html = tokio::task::spawn_blocking(move || docx_to_html(&target_for_blocking))
        .await
        .map_err(|e| CommandError::from(CoreError::Llm(format!("DOCX-Vorschau abgebrochen: {e}"))))?
        .map_err(CommandError::from)?;

    Ok(DocxPreview { html })
}

/// Read a worksheet of an .xlsx into a 2D string grid for the table viewer.
/// `sheet` is optional — when omitted, the first sheet is returned. The
/// response includes all sheet names so the UI can render tabs without a
/// separate metadata round-trip.
#[tauri::command]
pub async fn preview_xlsx(
    agent_id: String,
    path: String,
    sheet: Option<String>,
    state: State<'_, AppState>,
) -> Result<XlsxPreview, CommandError> {
    let agent = state.agent_repo().get(&agent_id)?;
    let folder = agent.folder.ok_or_else(|| {
        CommandError::new(
            "agent_has_no_folder",
            "Für diesen Agenten ist kein Ordner konfiguriert.",
        )
    })?;
    let target = ensure_in_agent_folder(&folder, &PathBuf::from(&path))?;
    if !target.is_file() {
        return Err(CommandError::new(
            "not_a_file",
            "Der angefragte Pfad ist keine Datei.",
        ));
    }

    let target_for_blocking = target.clone();
    let preview =
        tokio::task::spawn_blocking(move || xlsx_preview(&target_for_blocking, sheet.as_deref()))
            .await
            .map_err(|e| {
                CommandError::from(CoreError::Llm(format!("XLSX-Vorschau abgebrochen: {e}")))
            })?
            .map_err(CommandError::from)?;
    Ok(preview)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxPreview {
    pub slides: Vec<SlidePreview>,
}

/// Extract per-slide outline (title, body bullets, speaker notes) from a
/// .pptx in the agent's folder. Visual fidelity is intentionally skipped —
/// see `core::preview::pptx` for the rationale.
#[tauri::command]
pub async fn preview_pptx(
    agent_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<PptxPreview, CommandError> {
    let agent = state.agent_repo().get(&agent_id)?;
    let folder = agent.folder.ok_or_else(|| {
        CommandError::new(
            "agent_has_no_folder",
            "Für diesen Agenten ist kein Ordner konfiguriert.",
        )
    })?;
    let target = ensure_in_agent_folder(&folder, &PathBuf::from(&path))?;
    if !target.is_file() {
        return Err(CommandError::new(
            "not_a_file",
            "Der angefragte Pfad ist keine Datei.",
        ));
    }

    let target_for_blocking = target.clone();
    let slides = tokio::task::spawn_blocking(move || pptx_preview(&target_for_blocking))
        .await
        .map_err(|e| CommandError::from(CoreError::Llm(format!("PPTX-Vorschau abgebrochen: {e}"))))?
        .map_err(CommandError::from)?;

    Ok(PptxPreview { slides })
}
