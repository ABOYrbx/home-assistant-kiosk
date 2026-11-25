//! HTTPS catalog client and the last-verified-index cache.
//!
//! The client is synchronous so the Rhai host and the shop can run it on a
//! background task. It never touches the UI thread and never logs a URL with
//! credentials.

use crate::extensions::catalog::{CatalogError, CatalogIndex, CatalogPolicy};
use crate::extensions::signature::{
    SignatureError, SignatureStatus, verify_signature, verify_unsigned,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// Signature files are tiny; cap them separately from the index.
pub const MAX_SIGNATURE_BYTES: u64 = 16 * 1024;
/// Default network timeout for one catalog request.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("response for {url} exceeds {limit} bytes")]
    TooLarge { url: String, limit: u64 },
    #[error("unexpected HTTP status {status} for {url}")]
    Status { status: u16, url: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Fetch one HTTPS resource with a hard size cap.
pub trait CatalogFetcher {
    fn get(&mut self, url: &Url, max_bytes: u64) -> Result<Vec<u8>, FetchError>;
}

/// Production fetcher backed by `reqwest`'s blocking client.
pub struct HttpsFetcher {
    client: reqwest::blocking::Client,
}

impl HttpsFetcher {
    /// Production fetcher: plain HTTP is rejected, including after a redirect.
    pub fn new() -> Result<Self, FetchError> {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Result<Self, FetchError> {
        Self::with_options(timeout, false)
    }

    /// Build a fetcher for one catalog policy.
    ///
    /// Plain HTTP is only possible when the policy explicitly allows insecure
    /// catalogs, which requires both `allow_unsigned` and `dev_mode`.
    pub fn for_policy(policy: &CatalogPolicy) -> Result<Self, FetchError> {
        Self::with_options(DEFAULT_TIMEOUT, policy.allow_insecure())
    }

    fn with_options(timeout: Duration, allow_insecure: bool) -> Result<Self, FetchError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .https_only(!allow_insecure)
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        Ok(Self { client })
    }
}

impl CatalogFetcher for HttpsFetcher {
    fn get(&mut self, url: &Url, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        let response = self
            .client
            .get(url.as_str())
            .send()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::Status {
                status: status.as_u16(),
                url: url.to_string(),
            });
        }
        if let Some(length) = response.content_length()
            && length > max_bytes
        {
            return Err(FetchError::TooLarge {
                url: url.to_string(),
                limit: max_bytes,
            });
        }
        let mut body = Vec::new();
        let mut reader = response.take(max_bytes.saturating_add(1));
        reader.read_to_end(&mut body)?;
        if body.len() as u64 > max_bytes {
            return Err(FetchError::TooLarge {
                url: url.to_string(),
                limit: max_bytes,
            });
        }
        Ok(body)
    }
}

/// Last verified index, kept for offline shop use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CachedCatalog {
    pub schema_version: u32,
    pub source_url: String,
    pub fetched_at: DateTime<Utc>,
    pub index: String,
    pub signature: Option<String>,
}

/// Cache file for the last verified index.
pub struct CatalogCache {
    path: PathBuf,
}

impl CatalogCache {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<CachedCatalog>, CatalogError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.path).map_err(|e| CatalogError::Cache(e.to_string()))?;
        let cached: CachedCatalog = serde_json::from_slice(&bytes)
            .map_err(|e| CatalogError::Cache(format!("cached catalog is corrupt: {e}")))?;
        if cached.schema_version != CACHE_SCHEMA_VERSION {
            return Err(CatalogError::Cache(format!(
                "cached catalog schema {} is not supported",
                cached.schema_version
            )));
        }
        Ok(Some(cached))
    }

    pub fn store(
        &self,
        source_url: &Url,
        index: &[u8],
        signature: Option<&str>,
    ) -> Result<(), CatalogError> {
        let index = std::str::from_utf8(index)
            .map_err(|e| CatalogError::Cache(format!("catalog is not UTF-8: {e}")))?;
        let cached = CachedCatalog {
            schema_version: CACHE_SCHEMA_VERSION,
            source_url: source_url.to_string(),
            fetched_at: Utc::now(),
            index: index.to_string(),
            signature: signature.map(str::to_string),
        };
        let data =
            serde_json::to_vec_pretty(&cached).map_err(|e| CatalogError::Cache(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| CatalogError::Cache(e.to_string()))?;
            set_dir_mode(parent);
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut options = OpenOptions::new();
            options.create(true).write(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&tmp)
                .map_err(|e| CatalogError::Cache(e.to_string()))?;
            file.write_all(&data)
                .map_err(|e| CatalogError::Cache(e.to_string()))?;
            file.sync_all()
                .map_err(|e| CatalogError::Cache(e.to_string()))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| CatalogError::Cache(e.to_string()))?;
        Ok(())
    }
}

/// Outcome of a refresh, either fresh or served from the verified cache.
#[derive(Debug, Clone)]
pub struct RefreshOutcome {
    pub index: CatalogIndex,
    pub from_cache: bool,
    pub signature: SignatureStatus,
}

/// Catalog refresh client.
pub struct CatalogClient<F: CatalogFetcher> {
    fetcher: F,
    policy: CatalogPolicy,
    catalog_url: Url,
    cache: CatalogCache,
}

impl<F: CatalogFetcher> CatalogClient<F> {
    pub fn new(fetcher: F, policy: CatalogPolicy, catalog_url: Url, cache: CatalogCache) -> Self {
        Self {
            fetcher,
            policy,
            catalog_url,
            cache,
        }
    }

    /// Fetch and verify the catalog, falling back to the last verified cache
    /// when the network or the catalog is unavailable.
    pub fn refresh(&mut self) -> Result<RefreshOutcome, CatalogError> {
        match self.refresh_online() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                tracing::warn!("catalog refresh failed: {error}");
                match self.load_cached() {
                    Ok(Some(outcome)) => Ok(outcome),
                    Ok(None) => Err(error),
                    Err(cache_error) => {
                        tracing::warn!("cached catalog is unusable: {cache_error}");
                        Err(error)
                    }
                }
            }
        }
    }

    fn refresh_online(&mut self) -> Result<RefreshOutcome, CatalogError> {
        self.check_url()?;
        let index_bytes = self
            .fetcher
            .get(&self.catalog_url, self.policy.max_catalog_bytes)?;
        let signature = self.fetch_signature()?;
        let status = match signature.as_deref() {
            Some(text) => verify_signature(&index_bytes, text, &self.policy.public_keys)?,
            None => verify_unsigned(&self.policy)?,
        };
        let index = CatalogIndex::parse(&index_bytes, &self.policy)?;
        self.cache
            .store(&self.catalog_url, &index_bytes, signature.as_deref())?;
        Ok(RefreshOutcome {
            index,
            from_cache: false,
            signature: status,
        })
    }

    fn fetch_signature(&mut self) -> Result<Option<String>, CatalogError> {
        let url = signature_url(&self.catalog_url);
        match self.fetcher.get(&url, MAX_SIGNATURE_BYTES) {
            Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|e| {
                CatalogError::Signature(SignatureError::InvalidSignature(e.to_string()))
            }),
            Err(error) => {
                // Only a genuinely missing signature file may be treated as an
                // unsigned catalog. A transport error, an oversized response,
                // or any other HTTP status must not silently downgrade
                // verification, even in development mode.
                if self.policy.allow_unsigned
                    && matches!(error, FetchError::Status { status: 404, .. })
                {
                    tracing::debug!("catalog signature is unavailable: {error}");
                    Ok(None)
                } else {
                    Err(CatalogError::Fetch(error))
                }
            }
        }
    }

    fn load_cached(&self) -> Result<Option<RefreshOutcome>, CatalogError> {
        let Some(cached) = self.cache.load()? else {
            return Ok(None);
        };
        if cached.source_url != self.catalog_url.as_str() {
            return Ok(None);
        }
        let status = match cached.signature.as_deref() {
            Some(text) => {
                verify_signature(cached.index.as_bytes(), text, &self.policy.public_keys)?
            }
            None => verify_unsigned(&self.policy)?,
        };
        let index = CatalogIndex::parse(cached.index.as_bytes(), &self.policy)?;
        Ok(Some(RefreshOutcome {
            index,
            from_cache: true,
            signature: status,
        }))
    }

    fn check_url(&self) -> Result<(), CatalogError> {
        let scheme = self.catalog_url.scheme();
        if scheme != "https" && !self.policy.allow_insecure() {
            return Err(CatalogError::HttpNotAllowed);
        }
        if !matches!(scheme, "https" | "http") {
            return Err(CatalogError::HttpNotAllowed);
        }
        if !self.catalog_url.username().is_empty() || self.catalog_url.password().is_some() {
            return Err(CatalogError::CredentialsInUrl);
        }
        if self.catalog_url.host().is_none() {
            return Err(CatalogError::HttpNotAllowed);
        }
        Ok(())
    }
}

fn signature_url(url: &Url) -> Url {
    let mut signature = url.clone();
    signature.set_path(&format!("{}.minisig", url.path()));
    signature
}

fn set_dir_mode(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}
