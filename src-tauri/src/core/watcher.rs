use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::{AppHandle, Emitter, Manager};

use crate::core::agent::AgentRepo;
use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::ensure_in_agent_folder;

/// Single, replace-on-watch FS watcher. Whenever the active agent's folder
/// changes (or the folder switches), `watch` drops the previous debouncer
/// and arms a new one. Filesystem activity is debounced (400 ms) and emitted
/// to the frontend as `"fs-changed"` events so the FileTree can reload
/// without the user having to interact.
///
/// In addition, every debounce tick prunes attachments whose underlying file
/// is gone (rename/delete) — when something is dropped, we emit a separate
/// `"agent-attachments-changed"` event with the affected agent id.
///
/// The watcher also keeps the asset-protocol scope synchronized with the
/// active agent's folder. The frontend uses `convertFileSrc()` to render
/// images and PDFs directly from disk; that only works for paths the asset
/// protocol has been told to allow. We allow the new agent's folder here
/// and revoke the previous one, so the scope reflects exactly the agent
/// the user is currently viewing.
#[derive(Clone)]
pub struct FolderWatcher {
    inner: Arc<Mutex<Option<Debouncer<notify::RecommendedWatcher>>>>,
    /// The folder currently allowed in the asset-protocol scope. Tracked
    /// here so we can revoke it before granting the next one.
    scoped: Arc<Mutex<Option<PathBuf>>>,
    app: AppHandle,
}

impl std::fmt::Debug for FolderWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FolderWatcher").finish()
    }
}

impl FolderWatcher {
    pub fn new(app: AppHandle) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            scoped: Arc::new(Mutex::new(None)),
            app,
        }
    }

    pub fn watch(&self, path: &Path, agent_id: String, repo: AgentRepo) -> CoreResult<()> {
        let app_for_callback = self.app.clone();
        let agent_id_for_callback = agent_id.clone();
        let repo_for_callback = repo.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(400),
            move |result: DebounceEventResult| match result {
                Ok(_events) => {
                    if prune_broken_attachments(&repo_for_callback, &agent_id_for_callback) {
                        let _ = app_for_callback
                            .emit("agent-attachments-changed", &agent_id_for_callback);
                    }
                    let _ = app_for_callback.emit("fs-changed", ());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "fs watcher error");
                }
            },
        )
        .map_err(|e| CoreError::Llm(format!("watcher init failed: {e}")))?;

        debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| CoreError::Llm(format!("watch path failed: {e}")))?;

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| CoreError::Llm("watcher mutex poisoned".to_string()))?;
        *guard = Some(debouncer);
        drop(guard);

        self.set_scope(Some(path));
        Ok(())
    }

    pub fn unwatch(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        self.set_scope(None);
    }

    /// Flip the asset-protocol scope: revoke the previously-allowed folder
    /// (if any) and allow the new one. Best-effort — if the scope plumbing
    /// fails for any reason (e.g. canonicalization), we log and move on,
    /// because the rest of the app still works without `convertFileSrc()`.
    fn set_scope(&self, new_path: Option<&Path>) {
        let Ok(mut current) = self.scoped.lock() else {
            return;
        };
        let scope = self.app.asset_protocol_scope();

        if let Some(prev) = current.take() {
            if let Err(e) = scope.forbid_directory(&prev, true) {
                tracing::warn!(error = %e, path = %prev.display(), "asset scope forbid failed");
            }
        }
        if let Some(new_path) = new_path {
            match scope.allow_directory(new_path, true) {
                Ok(()) => {
                    *current = Some(new_path.to_path_buf());
                }
                Err(e) => {
                    tracing::warn!(error = %e, path = %new_path.display(), "asset scope allow failed");
                }
            }
        }
    }
}

/// Reload the agent, drop any attachment whose path is no longer a valid file
/// inside the agent folder, and persist if anything changed. Returns true iff
/// at least one attachment was cleared.
fn prune_broken_attachments(repo: &AgentRepo, agent_id: &str) -> bool {
    let Ok(mut agent) = repo.get(agent_id) else {
        return false;
    };
    let Some(folder) = agent.folder.clone() else {
        return false;
    };

    let mut changed = false;
    if let Some(path) = agent.attachments.template_path.clone() {
        if ensure_in_agent_folder(&folder, &path).is_err() {
            agent.attachments.template_path = None;
            changed = true;
        }
    }

    if changed {
        if let Err(e) = repo.save(&agent) {
            tracing::warn!(error = %e, agent = %agent_id, "could not save pruned agent");
        }
    }
    changed
}
