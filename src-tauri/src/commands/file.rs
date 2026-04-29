use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::core::error::{CommandError, CoreError};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// List the contents of the given agent's folder (one level, sorted:
/// directories first, then files, both alphabetically).
///
/// If `sub_path` is provided it must resolve inside the agent folder.
#[tauri::command]
pub async fn list_agent_folder(
    agent_id: String,
    sub_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FileEntry>, CommandError> {
    let repo = state.agent_repo();
    let agent = repo.get(&agent_id)?;

    let root = agent.folder.ok_or_else(|| {
        CommandError::new(
            "agent_has_no_folder",
            "Für diesen Agenten ist kein Ordner konfiguriert.",
        )
    })?;

    let target = match sub_path {
        Some(p) => ensure_in_agent_folder(&root, &PathBuf::from(p))?,
        None => root
            .canonicalize()
            .map_err(|e| CoreError::PathInvalid(e.to_string()))?,
    };

    if !target.is_dir() {
        return Err(CommandError::new(
            "not_a_directory",
            "Der angefragte Pfad ist kein Verzeichnis.",
        )
        .with_details(target.display().to_string()));
    }

    let mut entries: Vec<FileEntry> = Vec::new();
    for entry in std::fs::read_dir(&target).map_err(CoreError::from)? {
        let entry = entry.map_err(CoreError::from)?;
        let file_type = entry.file_type().map_err(CoreError::from)?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip noisy OS-level metadata files; never surface or manipulate them.
        if matches!(name.as_str(), ".DS_Store" | "Thumbs.db" | ".Spotlight-V100") {
            continue;
        }

        entries.push(FileEntry {
            name,
            path: entry.path(),
            is_dir: file_type.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

/// Start (or replace) the FS watch on the given agent's folder. Subsequent
/// changes inside that folder emit a debounced `"fs-changed"` Tauri event.
#[tauri::command]
pub async fn watch_agent_folder(
    agent_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let repo = state.agent_repo();
    let agent = repo.get(&agent_id)?;
    let folder = agent.folder.ok_or_else(|| {
        CommandError::new(
            "agent_has_no_folder",
            "Für diesen Agenten ist kein Ordner konfiguriert.",
        )
    })?;
    state
        .folder_watcher(&app)
        .watch(&folder, agent_id, repo)
        .map_err(CommandError::from)?;
    Ok(())
}

/// Stop the FS watcher (e.g. when no agent is active).
#[tauri::command]
pub async fn unwatch_agent_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    state.folder_watcher(&app).unwatch();
    Ok(())
}

/// Copy files (e.g. drag-dropped from Finder) into an agent's folder.
/// Existing files are renamed by appending a counter so the user never
/// loses content. Returns the resolved final filenames so the frontend
/// can confirm what was imported.
#[tauri::command]
pub async fn import_files_to_agent(
    agent_id: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, CommandError> {
    let agent = state.agent_repo().get(&agent_id)?;
    let folder = agent.folder.ok_or_else(|| {
        CommandError::new(
            "agent_has_no_folder",
            "Für diesen Agenten ist kein Ordner konfiguriert.",
        )
    })?;
    let mut imported = Vec::new();
    for raw in paths {
        let src = std::path::PathBuf::from(&raw);
        if !src.is_file() {
            // Skip directories and non-existent paths silently — drag-drop
            // from Finder can deliver folders, but recursive copy is out of
            // scope for v1.
            continue;
        }
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let target = unique_target_in(&folder, name);
        std::fs::copy(&src, &target).map_err(CoreError::from)?;
        imported.push(target.file_name().unwrap().to_string_lossy().to_string());
    }
    Ok(imported)
}

fn unique_target_in(folder: &std::path::Path, name: &str) -> PathBuf {
    let candidate = folder.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    for n in 1..1000 {
        let alt = folder.join(format!("{stem} ({n}){ext}"));
        if !alt.exists() {
            return alt;
        }
    }
    candidate
}

/// Reveal the daily log directory in Finder/Explorer/Files. Wired to the
/// "Logs öffnen" link on chat error banners.
#[tauri::command]
pub async fn open_logs_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    use tauri_plugin_opener::OpenerExt;
    let dir = state.paths.logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| CommandError::new("open_failed", e.to_string()))?;
    Ok(())
}
