use tauri::State;

use crate::core::error::CommandError;
use crate::core::settings::Settings;
use crate::state::AppState;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, CommandError> {
    state.settings().load().map_err(Into::into)
}

#[tauri::command]
pub async fn set_default_provider(
    provider: Option<String>,
    state: State<'_, AppState>,
) -> Result<Settings, CommandError> {
    state
        .settings()
        .update(|s| s.default_provider = provider)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_default_model(
    provider: String,
    model: Option<String>,
    state: State<'_, AppState>,
) -> Result<Settings, CommandError> {
    state
        .settings()
        .update(|s| match model {
            Some(m) => {
                s.default_models.insert(provider, m);
            }
            None => {
                s.default_models.remove(&provider);
            }
        })
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_first_run_done(state: State<'_, AppState>) -> Result<Settings, CommandError> {
    state
        .settings()
        .update(|s| s.first_run_done = true)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_custom_openai_base_url(
    url: Option<String>,
    state: State<'_, AppState>,
) -> Result<Settings, CommandError> {
    state
        .settings()
        .update(|s| s.custom_openai_base_url = url)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_locale(
    locale: Option<String>,
    state: State<'_, AppState>,
) -> Result<Settings, CommandError> {
    state
        .settings()
        .update(|s| s.locale = locale)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn available_providers(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    Ok(state
        .providers
        .available()
        .into_iter()
        .map(|s| s.to_string())
        .collect())
}
