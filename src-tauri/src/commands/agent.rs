use std::path::PathBuf;

use tauri::State;

use crate::core::agent::{Agent, AgentDraft, AgentUpdate, AttachmentKind};
use crate::core::error::{CommandError, CoreError};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::state::AppState;

#[tauri::command]
pub async fn list_agents(state: State<'_, AppState>) -> Result<Vec<Agent>, CommandError> {
    let repo = state.agent_repo();
    repo.list().map_err(Into::into)
}

#[tauri::command]
pub async fn get_agent(id: String, state: State<'_, AppState>) -> Result<Agent, CommandError> {
    let repo = state.agent_repo();
    repo.get(&id).map_err(Into::into)
}

#[tauri::command]
pub async fn create_agent(
    draft: AgentDraft,
    state: State<'_, AppState>,
) -> Result<Agent, CommandError> {
    let repo = state.agent_repo();
    let agent = Agent::from_draft(draft);
    repo.save(&agent)?;
    Ok(agent)
}

#[tauri::command]
pub async fn update_agent(
    id: String,
    update: AgentUpdate,
    state: State<'_, AppState>,
) -> Result<Agent, CommandError> {
    let repo = state.agent_repo();
    let mut agent = repo.get(&id)?;
    agent.apply_update(update);
    repo.save(&agent)?;
    Ok(agent)
}

#[tauri::command]
pub async fn delete_agent(id: String, state: State<'_, AppState>) -> Result<(), CommandError> {
    let repo = state.agent_repo();
    repo.delete(&id).map_err(Into::into)
}

/// Set or clear an agent's attachment for a given kind. `path = None` clears.
/// Paths must lie inside the agent's folder; the function rejects with
/// `PathOutsideAgentFolder` otherwise. Required-skill check is implicit:
/// the UI hides the button when the skill is off, but we still allow setting
/// here — the skill-removal flow in `apply_update` clears it again later.
#[tauri::command]
pub async fn set_agent_attachment(
    agent_id: String,
    kind: AttachmentKind,
    path: Option<PathBuf>,
    state: State<'_, AppState>,
) -> Result<Agent, CommandError> {
    let repo = state.agent_repo();
    let mut agent = repo.get(&agent_id)?;

    let resolved = match path {
        None => None,
        Some(requested) => {
            let folder = agent
                .folder
                .as_ref()
                .ok_or_else(|| CoreError::PathInvalid("agent has no folder".into()))?;
            Some(ensure_in_agent_folder(folder, &requested)?)
        }
    };

    agent.set_attachment(kind, resolved);
    repo.save(&agent)?;
    Ok(agent)
}

/// Remove a single context document from an agent's attachment list.
#[tauri::command]
pub async fn remove_agent_context_path(
    agent_id: String,
    path: PathBuf,
    state: State<'_, AppState>,
) -> Result<Agent, CommandError> {
    let repo = state.agent_repo();
    let mut agent = repo.get(&agent_id)?;
    agent.remove_context_path(&path);
    repo.save(&agent)?;
    Ok(agent)
}
