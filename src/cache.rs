use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Cache for tracking shown headlines with timestamps
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ShownCache {
    /// Map of URL/title -> unix timestamp when marked as shown
    entries: HashMap<String, i64>,
}

impl ShownCache {
    /// Load cache from disk, or return empty cache if not found
    pub fn load() -> Self {
        let path = Self::cache_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cache) = serde_json::from_str(&content) {
                    return cache;
                }
            }
        }
        Self::default()
    }

    /// Save cache to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::cache_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string(&self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Prune entries older than max_age
    pub fn prune(&mut self, max_age: Duration) {
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - max_age.as_secs() as i64;
        self.entries.retain(|_, ts| *ts > cutoff);
    }

    /// Get all shown keys as a HashSet for efficient lookup
    pub fn shown_keys(&self) -> std::collections::HashSet<String> {
        self.entries.keys().cloned().collect()
    }

    /// Merge shown keys back (for updating from ticker's runtime set)
    pub fn merge_shown(&mut self, keys: &std::collections::HashSet<String>) {
        let now = chrono::Utc::now().timestamp();
        for key in keys {
            self.entries.entry(key.clone()).or_insert(now);
        }
    }

    fn cache_path() -> PathBuf {
        dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache")
            .join("chyron")
            .join("shown.json")
    }
}
