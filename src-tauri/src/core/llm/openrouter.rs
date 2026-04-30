use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::openai_compat::OpenAiCompat;
use super::{GenerateRequest, LlmEvent, LlmProvider};
use crate::core::error::CoreResult;

// OpenRouter asks for these headers for attribution on their leaderboard.
const EXTRA: &[(&str, &str)] = &[
    ("HTTP-Referer", "https://processfox.ai"),
    ("X-Title", "ProcessFox"),
];

#[derive(Debug)]
pub struct OpenRouterProvider {
    inner: OpenAiCompat,
}

impl OpenRouterProvider {
    pub fn new() -> CoreResult<Self> {
        Ok(Self {
            inner: OpenAiCompat::new(
                "openrouter",
                "openrouter",
                "https://openrouter.ai/api/v1",
                EXTRA,
                true,
            )?,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    fn id(&self) -> &'static str {
        "openrouter"
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
        self.inner.generate(request, sink, cancel).await
    }

    async fn validate(&self) -> CoreResult<()> {
        self.inner.validate().await
    }
}
