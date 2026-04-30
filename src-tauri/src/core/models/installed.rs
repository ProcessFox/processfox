use std::path::PathBuf;

use serde::Serialize;

use super::catalog::ModelCatalog;
use crate::core::error::CoreResult;
use crate::core::storage::AppPaths;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub filename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    /// Catalog-id this file corresponds to, if it matches a known entry.
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledScanner {
    dir: PathBuf,
}

impl InstalledScanner {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            dir: paths.models_downloads_dir(),
        }
    }

    pub fn scan(&self, catalog: &ModelCatalog) -> CoreResult<Vec<InstalledModel>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut results = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("gguf") {
                continue;
            }
            let metadata = entry.metadata()?;
            let filename = entry.file_name().to_string_lossy().to_string();
            let catalog_id = catalog
                .models
                .iter()
                .find(|m| m.filename == filename)
                .map(|m| m.id.clone());
            results.push(InstalledModel {
                filename,
                path,
                size_bytes: metadata.len(),
                catalog_id,
            });
        }
        results.sort_by_key(|m| m.filename.to_lowercase());
        Ok(results)
    }

    pub fn delete(&self, filename: &str) -> CoreResult<()> {
        // Defensive: only allow relative, single-segment filenames to prevent
        // path traversal via `..` or absolute paths.
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err(crate::core::error::CoreError::PathInvalid(
                filename.to_string(),
            ));
        }
        let path = self.dir.join(filename);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}
