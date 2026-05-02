use std::sync::{Arc, OnceLock};

use crate::core::agent::AgentRepo;
use crate::core::chat::{ChatRepo, ChatRunner};
use crate::core::delegation::DelegationRunner;
use crate::core::llm::ProviderRegistry;
use crate::core::models::{DownloadRunner, InstalledScanner, ModelCatalog};
use crate::core::settings::SettingsStore;
use crate::core::skill::SkillRegistry;
use crate::core::storage::AppPaths;
use crate::core::tool::ToolRegistry;
use crate::core::watcher::FolderWatcher;

#[derive(Debug, Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub providers: ProviderRegistry,
    pub catalog: ModelCatalog,
    pub tools: ToolRegistry,
    pub skills: SkillRegistry,
    chat_runner: Arc<OnceLock<ChatRunner>>,
    delegation_runner: Arc<OnceLock<DelegationRunner>>,
    download_runner: Arc<OnceLock<DownloadRunner>>,
    folder_watcher: Arc<OnceLock<FolderWatcher>>,
}

impl AppState {
    pub fn new(
        paths: AppPaths,
        providers: ProviderRegistry,
        catalog: ModelCatalog,
        tools: ToolRegistry,
        skills: SkillRegistry,
    ) -> Self {
        Self {
            paths,
            providers,
            catalog,
            tools,
            skills,
            chat_runner: Arc::new(OnceLock::new()),
            delegation_runner: Arc::new(OnceLock::new()),
            download_runner: Arc::new(OnceLock::new()),
            folder_watcher: Arc::new(OnceLock::new()),
        }
    }

    pub fn folder_watcher(&self, app: &tauri::AppHandle) -> FolderWatcher {
        self.folder_watcher
            .get_or_init(|| FolderWatcher::new(app.clone()))
            .clone()
    }

    pub fn agent_repo(&self) -> AgentRepo {
        AgentRepo::new(&self.paths)
    }

    pub fn chat_repo(&self) -> ChatRepo {
        ChatRepo::new(&self.paths)
    }

    pub fn settings(&self) -> SettingsStore {
        SettingsStore::new(&self.paths)
    }

    pub fn installed_scanner(&self) -> InstalledScanner {
        InstalledScanner::new(&self.paths)
    }

    pub fn chat_runner(&self, app: &tauri::AppHandle) -> ChatRunner {
        self.chat_runner
            .get_or_init(|| {
                ChatRunner::new(
                    app.clone(),
                    self.chat_repo(),
                    self.providers.clone(),
                    self.tools.clone(),
                    self.skills.clone(),
                )
            })
            .clone()
    }

    pub fn delegation_runner(&self) -> DelegationRunner {
        self.delegation_runner
            .get_or_init(|| DelegationRunner::new(self.providers.clone(), self.settings()))
            .clone()
    }

    pub fn download_runner(
        &self,
        app: &tauri::AppHandle,
    ) -> crate::core::error::CoreResult<DownloadRunner> {
        if let Some(runner) = self.download_runner.get() {
            return Ok(runner.clone());
        }
        let runner = DownloadRunner::new(app.clone(), self.paths.clone())?;
        let _ = self.download_runner.set(runner.clone());
        Ok(runner)
    }
}
