pub mod commands;
pub mod core;
pub mod state;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use crate::core::llm::{
    anthropic::AnthropicProvider, cortecs::CortecsProvider,
    custom_openai::CustomOpenAiProvider, local_gguf::LocalGgufProvider,
    openai::OpenAiProvider, openrouter::OpenRouterProvider, ProviderRegistry,
};
use crate::core::models::ModelCatalog;
use crate::core::skill::SkillRegistry;
use crate::core::storage::AppPaths;
use crate::core::tool::{
    tools::{
        AppendToDocxTool, AppendToMdTool, AskUserTool, DelegateIntoXlsxColumnTool, GrepInFilesTool,
        ListFolderTool, ReadDocxTool, ReadFileTool, ReadPdfTool, ReadSkillTool, ReadXlsxRangeTool,
        RewriteFileTool, UpdateXlsxCellTool, WriteDocxFromTemplateTool, WriteDocxTool,
        WriteXlsxTool,
    },
    ToolRegistry,
};
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Discover paths first so we can install a daily-rotating file appender
    // alongside the stderr layer. If discovery fails (e.g. CI without a
    // home dir), fall back to stderr-only.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr_layer = fmt::layer().with_writer(std::io::stderr);

    if let Ok(paths) = AppPaths::discover() {
        let log_dir = paths.logs_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let file_appender = tracing_appender::rolling::daily(&log_dir, "processfox.log");
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        // Leak the guard so the worker thread keeps flushing for the
        // lifetime of the process — non_blocking drops writes silently if
        // the guard goes out of scope.
        Box::leak(Box::new(guard));
        let file_layer = fmt::layer().with_writer(file_writer).with_ansi(false);
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(stderr_layer)
            .with(file_layer)
            .try_init();
    } else {
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(stderr_layer)
            .try_init();
    }

    tauri::Builder::default()
        .on_window_event(|window, event| {
            use tauri::Emitter;
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                let strs: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                let _ = window.emit("files-dropped", strs);
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let paths = AppPaths::discover()?;
            paths.ensure_dirs()?;
            tracing::info!(root = %paths.root().display(), "ProcessFox app data initialized");

            let settings_store = crate::core::settings::SettingsStore::new(&paths);

            let mut registry = ProviderRegistry::new();
            registry.register(Arc::new(AnthropicProvider::new()?));
            registry.register(Arc::new(OpenAiProvider::new()?));
            registry.register(Arc::new(OpenRouterProvider::new()?));
            registry.register(Arc::new(CortecsProvider::new()?));
            registry.register(Arc::new(CustomOpenAiProvider::new(settings_store)?));
            registry.register(Arc::new(LocalGgufProvider::new(
                paths.models_downloads_dir(),
            )));

            let catalog = ModelCatalog::embedded()?;
            tracing::info!(models = catalog.models.len(), "model catalog loaded");

            let skills = SkillRegistry::load_builtin()?;
            tracing::info!(skills = skills.all().len(), "skill registry loaded");

            let mut tools = ToolRegistry::new();
            tools.register(Arc::new(ListFolderTool));
            tools.register(Arc::new(ReadFileTool));
            tools.register(Arc::new(GrepInFilesTool));
            tools.register(Arc::new(ReadPdfTool));
            tools.register(Arc::new(ReadDocxTool));
            tools.register(Arc::new(ReadXlsxRangeTool));
            tools.register(Arc::new(AppendToMdTool));
            tools.register(Arc::new(WriteDocxTool));
            tools.register(Arc::new(AppendToDocxTool));
            tools.register(Arc::new(RewriteFileTool));
            tools.register(Arc::new(UpdateXlsxCellTool));
            tools.register(Arc::new(WriteXlsxTool));
            tools.register(Arc::new(AskUserTool));
            tools.register(Arc::new(WriteDocxFromTemplateTool));
            tools.register(Arc::new(DelegateIntoXlsxColumnTool));
            tools.register(Arc::new(ReadSkillTool::new(skills.clone())));
            tracing::info!(tools = tools.names().len(), "tool registry ready");

            app.manage(AppState::new(paths, registry, catalog, tools, skills));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent::list_agents,
            commands::agent::get_agent,
            commands::agent::create_agent,
            commands::agent::update_agent,
            commands::agent::delete_agent,
            commands::agent::set_agent_attachment,
            commands::file::list_agent_folder,
            commands::file::watch_agent_folder,
            commands::file::unwatch_agent_folder,
            commands::file::open_logs_folder,
            commands::file::import_files_to_agent,
            commands::file::read_text_file,
            commands::file::write_text_file,
            commands::preview::preview_docx,
            commands::preview::preview_xlsx,
            commands::preview::preview_pptx,
            commands::settings::get_settings,
            commands::settings::set_default_provider,
            commands::settings::set_default_model,
            commands::settings::set_first_run_done,
            commands::settings::set_custom_openai_base_url,
            commands::settings::available_providers,
            commands::secrets::set_api_key,
            commands::secrets::has_api_key,
            commands::secrets::clear_api_key,
            commands::secrets::validate_api_key,
            commands::chat::list_messages,
            commands::chat::send_message,
            commands::chat::cancel_run,
            commands::chat::approve_hitl,
            commands::chat::reject_hitl,
            commands::chat::respond_to_question,
            commands::models::list_catalog,
            commands::models::list_installed_models,
            commands::models::get_hardware_info,
            commands::models::download_from_catalog,
            commands::models::download_from_url,
            commands::models::cancel_download,
            commands::models::delete_model,
            commands::skill::list_skills,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
