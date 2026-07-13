use std::collections::HashMap;
use std::sync::Arc;

use super::Tool;
use crate::core::error::{CoreError, CoreResult};

/// Holds every tool the app knows how to execute. Cheap to clone.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<&'static str, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name(), tool);
    }

    pub fn get(&self, name: &str) -> CoreResult<Arc<dyn Tool>> {
        self.tools.get(name).cloned().ok_or_else(|| {
            CoreError::Llm(format!(
                "Unbekanntes Tool: '{name}'. Verfügbare Tools: {}",
                self.names().join(", ")
            ))
        })
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self.tools.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub fn schemas_for(&self, names: &[String]) -> Vec<super::ToolSchema> {
        names
            .iter()
            .filter_map(|n| self.tools.get(n.as_str()).map(|t| t.schema()))
            .collect()
    }
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}
