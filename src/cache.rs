use crate::events::EntityId;
use crate::state::{AppState, CacheError, CacheStore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
    /// When set, only these entities are written to the cache.
    allowed_entities: Option<HashSet<EntityId>>,
}

impl FileCache {
    pub fn new(path: PathBuf, schema_version: u32, fingerprint: String, max_bytes: u64) -> Self {
        Self {
            path,
            schema_version,
            fingerprint,
            max_bytes,
            allowed_entities: None,
        }
    }

    /// Restrict the cached entity set to the configured subscription list.
    pub fn with_entities<I>(mut self, entities: I) -> Self
    where
        I: IntoIterator<Item = EntityId>,
    {
        self.allowed_entities = Some(entities.into_iter().collect());
        self
    }

    fn ensure_dir(&self) -> Result<(), CacheError> {
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| CacheError(e.to_string()))?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                    .map_err(|e| CacheError(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// The state that is actually persisted. Views are owned by the UI layer;
    /// this wave only guarantees that no unconfigured entity leaks into the
    /// cache.
    fn filtered(&self, state: &AppState) -> AppState {
        let mut state = state.clone();
        if let Some(allowed) = &self.allowed_entities {
            state.entities.retain(|id, _| allowed.contains(id));
        }
        state
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
            state: self.filtered(state),
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
            let mut options = OpenOptions::new();
            options.create(true).write(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&tmp_path)
                .map_err(|e| CacheError(e.to_string()))?;
            file.write_all(&data)
                .map_err(|e| CacheError(e.to_string()))?;
            file.sync_all().map_err(|e| CacheError(e.to_string()))?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))
                .map_err(|e| CacheError(e.to_string()))?;
        }

        // `rename` is atomic within one filesystem, so a crash leaves either the
        // old cache or the new one, never a torn file.
        fs::rename(&tmp_path, &self.path).map_err(|e| CacheError(e.to_string()))?;

        // Persist the directory entry itself.
        #[cfg(unix)]
        {
            let dir = File::open(parent).map_err(|e| CacheError(e.to_string()))?;
            dir.sync_all().map_err(|e| CacheError(e.to_string()))?;
        }

        Ok(())
    }
}

/// A write-debounce timer for SD-card friendliness.
///
/// Marking the cache dirty starts one window; the write happens when the
/// window expires. Repeated events inside the window do not extend it, so a
/// chatty Home Assistant cannot postpone persistence forever.
#[derive(Debug, Clone)]
pub struct WriteDebounce {
    delay: Duration,
    pending_since: Option<Instant>,
}

impl WriteDebounce {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending_since: None,
        }
    }

    pub fn mark(&mut self, now: Instant) {
        if self.pending_since.is_none() {
            self.pending_since = Some(now);
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending_since.is_some()
    }

    pub fn is_due(&self, now: Instant) -> bool {
        self.pending_since
            .is_some_and(|since| now.duration_since(since) >= self.delay)
    }

    /// Clear the window and report whether a write is due now.
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.is_due(now) {
            self.pending_since = None;
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self) {
        self.pending_since = None;
    }
}

/// Couples [`FileCache`] with a [`WriteDebounce`].
pub struct CacheWriter {
    cache: FileCache,
    debounce: WriteDebounce,
}

impl CacheWriter {
    pub fn new(cache: FileCache, delay: Duration) -> Self {
        Self {
            cache,
            debounce: WriteDebounce::new(delay),
        }
    }

    pub fn cache(&self) -> &FileCache {
        &self.cache
    }

    pub fn mark_dirty(&mut self) {
        self.debounce.mark(Instant::now());
    }

    pub fn mark_dirty_at(&mut self, now: Instant) {
        self.debounce.mark(now);
    }

    pub fn is_dirty(&self) -> bool {
        self.debounce.is_pending()
    }

    /// Write the state if the debounce window has elapsed.
    pub fn flush_if_due(&mut self, state: &AppState, now: Instant) -> Result<bool, CacheError> {
        if !self.debounce.take_due(now) {
            return Ok(false);
        }
        match self.cache.save(state) {
            Ok(()) => Ok(true),
            Err(e) => {
                // Keep the change pending so the next tick retries.
                self.debounce.mark(now);
                Err(e)
            }
        }
    }

    /// Write immediately, for shutdown and tests.
    pub fn flush(&mut self, state: &AppState) -> Result<(), CacheError> {
        self.cache.save(state)?;
        self.debounce.reset();
        Ok(())
    }
}
