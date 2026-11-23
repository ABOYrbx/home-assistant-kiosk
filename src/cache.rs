use crate::state::{AppState, CacheError};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEnvelope {
    schema_version: u32,
    fingerprint: String,
    state: AppState,
}

pub struct FileCache {
    pub path: PathBuf,
    pub schema_version: u32,
    pub fingerprint: String,
    pub max_bytes: u64,
}

impl FileCache {
    pub fn new(path: PathBuf, schema_version: u32, fingerprint: String, max_bytes: u64) -> Self {
        Self {
            path,
            schema_version,
            fingerprint,
            max_bytes,
        }
    }

    fn ensure_dir(&self) -> Result<(), CacheError> {
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| CacheError(e.to_string()))?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }
        Ok(())
    }
}

impl crate::state::CacheStore for FileCache {
    fn load(&self) -> Result<Option<AppState>, CacheError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.path).map_err(|e| CacheError(e.to_string()))?;
        let envelope: CacheEnvelope = match serde_json::from_slice(&bytes) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("cache corrupt or unreadable: {}", e);
                return Ok(None);
            }
        };
        if envelope.schema_version != self.schema_version {
            tracing::warn!(
                "cache schema mismatch: {} != {}",
                envelope.schema_version,
                self.schema_version
            );
            return Ok(None);
        }
        if envelope.fingerprint != self.fingerprint {
            tracing::warn!("cache fingerprint mismatch; discarding");
            return Ok(None);
        }
        Ok(Some(envelope.state))
    }

    fn save(&self, state: &AppState) -> Result<(), CacheError> {
        self.ensure_dir()?;
        let envelope = CacheEnvelope {
            schema_version: self.schema_version,
            fingerprint: self.fingerprint.clone(),
            state: state.clone(),
        };
        let data = serde_json::to_vec_pretty(&envelope).map_err(|e| CacheError(e.to_string()))?;
        if data.len() as u64 > self.max_bytes {
            return Err(CacheError(format!(
                "cache size {} exceeds cap {}",
                data.len(),
                self.max_bytes
            )));
        }

        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let tmp_path = parent.join(format!(
            ".{}.tmp",
            self.path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "cache".to_string())
        ));

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp_path)
                .map_err(|e| CacheError(e.to_string()))?;
            file.write_all(&data)
                .map_err(|e| CacheError(e.to_string()))?;
            file.sync_all().map_err(|e| CacheError(e.to_string()))?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        }

        fs::rename(&tmp_path, &self.path).map_err(|e| CacheError(e.to_string()))?;

        // fsync directory
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }

        Ok(())
    }
}
