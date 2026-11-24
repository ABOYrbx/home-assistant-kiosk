//! Versioned, atomic snapshot cache.
//!
//! Wave 0 only provides a compiling stub. Atomic writes, schema versions, and
//! configuration fingerprints are implemented in Wave 1.

use crate::state::{AppState, CacheError, CacheStore};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileCache {
    pub path: PathBuf,
}

impl FileCache {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl CacheStore for FileCache {
    fn load(&self) -> Result<Option<AppState>, CacheError> {
        Ok(None)
    }

    fn save(&self, _state: &AppState) -> Result<(), CacheError> {
        Ok(())
    }
}
