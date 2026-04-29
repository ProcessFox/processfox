use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::{AppHandle, Emitter};

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
#[derive(Clone)]
pub struct FolderWatcher {
    inner: Arc<Mutex<Option<Debouncer<notify::RecommendedWatcher>>>>,
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
        Ok(())
    }

    pub fn unwatch(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
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
