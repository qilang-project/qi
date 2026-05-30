//! Compilation cache for Qi language

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub timestamp: u64,
    pub source_hash: String,
    pub compiled_data: Vec<u8>,
    pub dependencies: Vec<PathBuf>,
    pub target_triple: String,
    pub optimization_level: String,
}

/// Compilation cache
pub struct CompilationCache {
    entries: HashMap<String, CacheEntry>,
    cache_dir: PathBuf,
    max_entries: usize,
}

/// Cache errors
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// I/O error
    #[error("缓存 I/O 错误: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("缓存序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Cache not found
    #[error("缓存未找到: {0}")]
    NotFound(String),
}

impl CompilationCache {
    pub fn new() -> Result<Self, CacheError> {
        let cache_dir = std::env::current_dir()?.join(".qi_cache");

        Self::with_cache_dir(cache_dir)
    }

    pub fn with_cache_dir(cache_dir: PathBuf) -> Result<Self, CacheError> {
        std::fs::create_dir_all(&cache_dir)?;

        let mut cache = Self {
            entries: HashMap::new(),
            cache_dir,
            max_entries: 1000,
        };

        cache.load_cache_index()?;
        Ok(cache)
    }

    pub fn get(&mut self, key: &str, source_hash: &str) -> Option<&CacheEntry> {
        if let Some(entry) = self.entries.get(key) {
            if entry.source_hash == source_hash {
                return Some(entry);
            }
        }
        None
    }

    pub fn put(
        &mut self,
        key: String,
        source_hash: String,
        compiled_data: Vec<u8>,
        dependencies: Vec<PathBuf>,
        target_triple: String,
        optimization_level: String,
    ) -> Result<(), CacheError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry {
            timestamp,
            source_hash,
            compiled_data,
            dependencies,
            target_triple,
            optimization_level,
        };

        self.entries.insert(key.clone(), entry);
        self.save_cache_index()?;
        self.save_cache_data(&key)?;

        // Cleanup old entries if necessary
        self.cleanup()?;

        Ok(())
    }

    pub fn invalidate(&mut self, key: &str) -> Result<(), CacheError> {
        if self.entries.remove(key).is_some() {
            self.save_cache_index()?;
            self.remove_cache_data(key)?;
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), CacheError> {
        self.entries.clear();
        self.save_cache_index()?;

        // Remove all cache data files
        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.file_name() != Some(std::ffi::OsStr::new("index.json")) {
                std::fs::remove_file(path)?;
            }
        }

        Ok(())
    }

    fn load_cache_index(&mut self) -> Result<(), CacheError> {
        let index_path = self.cache_dir.join("index.json");
        if index_path.exists() {
            let content = std::fs::read_to_string(index_path)?;
            let loaded_entries: HashMap<String, CacheEntry> = serde_json::from_str(&content)?;
            self.entries = loaded_entries;
        }
        Ok(())
    }

    fn save_cache_index(&self) -> Result<(), CacheError> {
        let index_path = self.cache_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(index_path, content)?;
        Ok(())
    }

    fn save_cache_data(&self, key: &str) -> Result<(), CacheError> {
        if let Some(entry) = self.entries.get(key) {
            let data_path = self.cache_dir.join(format!("{}.data", key));
            std::fs::write(data_path, &entry.compiled_data)?;
        }
        Ok(())
    }

    fn remove_cache_data(&self, key: &str) -> Result<(), CacheError> {
        let data_path = self.cache_dir.join(format!("{}.data", key));
        if data_path.exists() {
            std::fs::remove_file(data_path)?;
        }
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), CacheError> {
        if self.entries.len() <= self.max_entries {
            return Ok(());
        }

        // Sort entries by timestamp (oldest first)
        let entries: Vec<_> = self
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), v.timestamp))
            .collect();

        let mut sorted_entries = entries;
        sorted_entries.sort_by_key(|(_, timestamp)| *timestamp);

        // Remove oldest entries
        let to_remove = sorted_entries.len() - self.max_entries;
        let keys_to_remove: Vec<String> = sorted_entries
            .iter()
            .take(to_remove)
            .map(|(key, _)| key.clone())
            .collect();

        for key in keys_to_remove {
            self.remove_cache_data(&key)?;
            self.entries.remove(&key);
        }

        self.save_cache_index()?;
        Ok(())
    }

    pub fn is_dependency_modified(&self, dependencies: &[PathBuf], timestamp: u64) -> bool {
        dependencies.iter().any(|dep| {
            if let Ok(metadata) = std::fs::metadata(dep) {
                if let Ok(modified) = metadata.modified() {
                    let dep_timestamp = modified
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    return dep_timestamp > timestamp;
                }
            }
            true // Assume modified if we can't check
        })
    }

    pub fn set_max_entries(&mut self, max: usize) {
        self.max_entries = max;
    }

    pub fn get_cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            total_entries: self.entries.len(),
            max_entries: self.max_entries,
            cache_dir: self.cache_dir.clone(),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub max_entries: usize,
    pub cache_dir: PathBuf,
}

impl Default for CompilationCache {
    fn default() -> Self {
        Self::new().expect("Failed to create default compilation cache")
    }
}
