use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::openai_compat::OpenAiCompat;
use super::{GenerateRequest, LlmEvent, LlmProvider};
use crate::core::error::CoreResult;

#[derive(Debug)]
pub struct CortecsProvider {
    inner: OpenAiCompat,
}

impl CortecsProvider {
    pub fn new() -> CoreResult<Self> {
        Ok(Self {
            inner: OpenAiCompat::new("cortecs", "cortecs", "https://api.cortecs.ai/v1", &[], true)?,
        })
    }
}

#[async_trait]
impl LlmProvider for CortecsProvider {
    fn id(&self) -> &'static str {
        "cortecs"
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
