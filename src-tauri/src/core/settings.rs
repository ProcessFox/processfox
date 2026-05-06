use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::CoreResult;
use super::storage::AppPaths;

/// Global, non-sensitive application settings. API keys are kept separately in
/// the OS keychain via `core::secrets`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Fallback provider used when an agent has no `model` set.
    #[serde(default)]
    pub default_provider: Option<String>,
    /// Per-provider default model id (e.g. `"anthropic" -> "claude-sonnet-4-6"`).
    #[serde(default)]
    pub default_models: BTreeMap<String, String>,
    /// Flipped to `true` after the first-run onboarding completes.
    #[serde(default)]
    pub first_run_done: bool,
    /// Base URL for the generic "OpenAI-compatible" custom provider
    /// (e.g. `https://api.cortecs.ai/v1`, `http://localhost:11434/v1`).
    #[serde(default)]
    pub custom_openai_base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    file: PathBuf,
}

impl SettingsStore {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            file: paths.settings_file(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    pub fn exists(&self) -> bool {
        self.file.exists()
    }

    pub fn load(&self) -> CoreResult<Settings> {
        if !self.file.exists() {
            return Ok(Settings::default());
        }
        let body = std::fs::read(&self.file)?;
        let parsed: Settings = serde_json::from_slice(&body)?;
        Ok(parsed)
    }

    pub fn save(&self, settings: &Settings) -> CoreResult<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(settings)?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &self.file)?;
        Ok(())
    }

    pub fn update<F>(&self, mutate: F) -> CoreResult<Settings>
    where
        F: FnOnce(&mut Settings),
    {
        let mut current = self.load()?;
        mutate(&mut current);
        self.save(&current)?;
        Ok(current)
    }
}
