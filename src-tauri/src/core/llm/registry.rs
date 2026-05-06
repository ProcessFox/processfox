use std::collections::HashMap;
use std::sync::Arc;

use super::LlmProvider;
use crate::core::error::{CoreError, CoreResult};

pub type ProviderId = &'static str;

pub const ANTHROPIC: ProviderId = "anthropic";
pub const OPENAI: ProviderId = "openai";
pub const OPENROUTER: ProviderId = "openrouter";
pub const CORTECS: ProviderId = "cortecs";
pub const CUSTOM: ProviderId = "custom";
pub const LOCAL: ProviderId = "local";

/// Holds the set of concrete `LlmProvider` implementations available in the app.
/// Cheap to clone — all providers live behind `Arc`.
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    providers: HashMap<&'static str, Arc<dyn LlmProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn LlmProvider>) {
        self.providers.insert(provider.id(), provider);
    }

    pub fn get(&self, id: &str) -> CoreResult<Arc<dyn LlmProvider>> {
        self.providers
            .get(id)
            .cloned()
            .ok_or_else(|| CoreError::UnknownProvider(id.to_string()))
    }

    pub fn available(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self.providers.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}
