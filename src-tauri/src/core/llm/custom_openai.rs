use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::openai_compat::OpenAiCompat;
use super::{GenerateRequest, LlmEvent, LlmProvider};
use crate::core::error::{CoreError, CoreResult};
use crate::core::settings::SettingsStore;

/// Generic OpenAI-compatible provider. The base URL is read from settings on
/// every call so users can change it without restarting the app.
///
/// Uses `include_usage: false` because self-hosted backends (Ollama, vLLM, …)
/// often reject the `stream_options` key.
#[derive(Debug)]
pub struct CustomOpenAiProvider {
    settings: SettingsStore,
    http: reqwest::Client,
}

impl CustomOpenAiProvider {
    pub fn new(settings: SettingsStore) -> CoreResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent("ProcessFox/0.1")
            .build()
            .map_err(|e| CoreError::Http(e.to_string()))?;
        Ok(Self { settings, http })
    }

    fn make_compat(&self, base_url: String) -> OpenAiCompat {
        OpenAiCompat::from_parts("custom", "custom", base_url, &[], false, self.http.clone())
    }

    fn get_base_url(&self) -> CoreResult<String> {
        let settings = self.settings.load()?;
        settings
            .custom_openai_base_url
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| {
                CoreError::Llm(
                    "Kein OpenAI-kompatibler Endpunkt konfiguriert. \
                     Bitte zuerst die Basis-URL im Einstellungen-Dialog eintragen."
                        .into(),
                )
            })
    }
}

#[async_trait]
impl LlmProvider for CustomOpenAiProvider {
    fn id(&self) -> &'static str {
        "custom"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn generate(
        &self,
        request: GenerateRequest,
        sink: mpsc::Sender<LlmEvent>,
        cancel: CancellationToken,
    ) -> CoreResult<()> {
        let base_url = self.get_base_url()?;
        self.make_compat(base_url).generate(request, sink, cancel).await
    }

    async fn validate(&self) -> CoreResult<()> {
        let base_url = self.get_base_url()?;
        self.make_compat(base_url).validate().await
    }
}
