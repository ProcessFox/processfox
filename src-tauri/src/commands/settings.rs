use serde::Serialize;
use tauri::State;

use crate::core::error::CommandError;
use crate::core::license::Edition;
use crate::core::settings::Settings;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub edition: Edition,
    pub edition_name: &'static str,
    pub license_spdx: &'static str,
}

#[tauri::command]
pub async fn get_app_info() -> Result<AppInfo, CommandError> {
    let edition = Edition::current();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        edition,
        edition_name: edition.name(),
        license_spdx: edition.license_spdx(),
    })
}

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
