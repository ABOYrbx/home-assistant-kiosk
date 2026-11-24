//! Catalog index and extension manifest schemas.
//!
//! The kiosk parses the signed `index.json` from the companion catalog
//! repository. Every struct uses `deny_unknown_fields` so schema drift is a
//! hard error instead of a silent default.

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path};
use thiserror::Error;
use url::Url;

/// Extension API version understood by this kiosk.
pub const API_VERSION: u32 = 1;
/// Schema version of `index.json`.
pub const CATALOG_SCHEMA_VERSION: u32 = 1;
/// Upper bound on entries in one catalog.
pub const MAX_CATALOG_ENTRIES: usize = 500;
/// Upper bound on declared config fields in one manifest.
pub const MAX_CONFIG_FIELDS: usize = 32;
/// Upper bound on declared HTTP hosts in one manifest.
pub const MAX_HTTP_HOSTS: usize = 32;
/// Upper bound on declared Home Assistant entities in one manifest.
pub const MAX_HA_ENTITIES: usize = 64;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog exceeds {limit} bytes")]
    CatalogTooLarge { limit: u64 },
    #[error("catalog schema {0} is not supported")]
    UnsupportedSchema(u32),
    #[error("catalog JSON is invalid: {0}")]
    Json(String),
    #[error("manifest TOML is invalid: {0}")]
    Manifest(String),
    #[error("catalog has too many entries ({0})")]
    TooManyEntries(usize),
    #[error("duplicate extension id: {0}")]
    DuplicateId(String),
    #[error("invalid extension id: {0}")]
    InvalidId(String),
    #[error("invalid manifest for {id}: {reason}")]
    InvalidManifest { id: String, reason: String },
    #[error("extension {id} requires api_version {required}, kiosk supports {supported}")]
    IncompatibleApi {
        id: String,
        required: u32,
        supported: u32,
    },
    #[error("extension {id} requires app version {required}, kiosk is {current}")]
    IncompatibleApp {
        id: String,
        required: Version,
        current: Version,
    },
    #[error("extension {id} has an insecure package URL: {url}")]
    InsecureUrl { id: String, url: String },
    #[error("extension {id} has an invalid checksum")]
    InvalidChecksum { id: String },
    #[error("extension {id} package size {size} exceeds the limit {limit}")]
    PackageTooLarge { id: String, size: u64, limit: u64 },
    #[error("extension {id} has an invalid package size")]
    InvalidPackageSize { id: String },
    #[error("catalog URL must use HTTPS unless allow_unsigned and dev_mode are both enabled")]
    HttpNotAllowed,
    #[error("catalog URL must not contain credentials")]
    CredentialsInUrl,
    #[error("catalog fetch failed: {0}")]
    Fetch(#[from] crate::extensions::client::FetchError),
    #[error("catalog signature is invalid: {0}")]
    Signature(#[from] crate::extensions::signature::SignatureError),
    #[error("catalog cache error: {0}")]
    Cache(String),
}

/// Extension output kind. The host renders one of three built-in view models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    Departures,
    List,
    KeyValue,
}

/// Declared permissions. The script host enforces both allowlists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub http_hosts: Vec<String>,
    #[serde(default)]
    pub ha_entities: Vec<String>,
}

/// Configuration field type shown by the shop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    String,
    Integer,
    Boolean,
    Url,
    Secret,
    EntityId,
    Select,
}

/// One configuration field declared by a manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ConfigFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub options: Vec<String>,
}

/// Extension manifest shipped inside a `.hkext` package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub api_version: u32,
    pub min_app_version: Version,
    pub entry: String,
    pub provides: ExtensionKind,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub config_schema: Vec<ConfigField>,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<Url>,
    pub description: String,
}

impl ExtensionManifest {
    pub fn parse_toml(text: &str) -> Result<Self, CatalogError> {
        toml::from_str(text).map_err(|e| CatalogError::Manifest(e.to_string()))
    }

    /// Validate the manifest against the API version and kiosk version.
    pub fn validate(&self, app_version: &Version) -> Result<(), CatalogError> {
        validate_fields(
            &self.id,
            &self.name,
            &self.author,
            &self.license,
            &self.description,
            self.api_version,
            &self.min_app_version,
            app_version,
            &self.permissions,
            &self.config_schema,
        )?;
        if !is_safe_relative_path(&self.entry) || !self.entry.ends_with(".rhai") {
            return Err(invalid(
                &self.id,
                "entry must be a relative path ending in .rhai",
            ));
        }
        Ok(())
    }
}

/// One entry in the signed catalog index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub api_version: u32,
    pub min_app_version: Version,
    pub description: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<Url>,
    pub provides: ExtensionKind,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub config_schema: Vec<ConfigField>,
    pub package_url: Url,
    pub sha256: String,
    pub size_bytes: u64,
}

impl CatalogEntry {
    /// Validate the catalog entry. The entry script is only known after
    /// download, so it is validated in the extracted manifest instead.
    pub fn validate(&self, policy: &CatalogPolicy) -> Result<(), CatalogError> {
        validate_fields(
            &self.id,
            &self.name,
            &self.author,
            &self.license,
            &self.description,
            self.api_version,
            &self.min_app_version,
            &policy.app_version,
            &self.permissions,
            &self.config_schema,
        )?;
        let scheme = self.package_url.scheme();
        if scheme != "https" && !policy.allow_insecure() {
            return Err(CatalogError::InsecureUrl {
                id: self.id.clone(),
                url: self.package_url.to_string(),
            });
        }
        if !matches!(scheme, "https" | "http") {
            return Err(CatalogError::InsecureUrl {
                id: self.id.clone(),
                url: self.package_url.to_string(),
            });
        }
        if !self.package_url.username().is_empty() || self.package_url.password().is_some() {
            return Err(CatalogError::InsecureUrl {
                id: self.id.clone(),
                url: self.package_url.to_string(),
            });
        }
        if self.package_url.host().is_none() {
            return Err(CatalogError::InsecureUrl {
                id: self.id.clone(),
                url: self.package_url.to_string(),
            });
        }
        if !is_sha256_hex(&self.sha256) {
            return Err(CatalogError::InvalidChecksum {
                id: self.id.clone(),
            });
        }
        if self.size_bytes == 0 {
            return Err(CatalogError::InvalidPackageSize {
                id: self.id.clone(),
            });
        }
        if self.size_bytes > policy.max_package_bytes {
            return Err(CatalogError::PackageTooLarge {
                id: self.id.clone(),
                size: self.size_bytes,
                limit: policy.max_package_bytes,
            });
        }
        Ok(())
    }
}

/// The parsed `index.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogIndex {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub extensions: Vec<CatalogEntry>,
}

impl CatalogIndex {
    /// Parse and validate a catalog index under the configured policy.
    pub fn parse(bytes: &[u8], policy: &CatalogPolicy) -> Result<Self, CatalogError> {
        if bytes.len() as u64 > policy.max_catalog_bytes {
            return Err(CatalogError::CatalogTooLarge {
                limit: policy.max_catalog_bytes,
            });
        }
        let index: CatalogIndex =
            serde_json::from_slice(bytes).map_err(|e| CatalogError::Json(e.to_string()))?;
        index.validate(policy)?;
        Ok(index)
    }

    pub fn validate(&self, policy: &CatalogPolicy) -> Result<(), CatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(self.schema_version));
        }
        if self.extensions.len() > MAX_CATALOG_ENTRIES {
            return Err(CatalogError::TooManyEntries(self.extensions.len()));
        }
        let mut ids: HashSet<&str> = HashSet::new();
        for entry in &self.extensions {
            entry.validate(policy)?;
            if !ids.insert(entry.id.as_str()) {
                return Err(CatalogError::DuplicateId(entry.id.clone()));
            }
        }
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&CatalogEntry> {
        self.extensions.iter().find(|entry| entry.id == id)
    }
}

/// Trust and compatibility policy for one catalog refresh.
#[derive(Debug, Clone)]
pub struct CatalogPolicy {
    pub app_version: Version,
    pub public_keys: Vec<String>,
    pub allow_unsigned: bool,
    pub dev_mode: bool,
    pub max_catalog_bytes: u64,
    pub max_package_bytes: u64,
}

impl CatalogPolicy {
    /// HTTP catalog and package URLs are accepted only in development mode
    /// with unsigned catalogs explicitly enabled.
    pub fn allow_insecure(&self) -> bool {
        self.allow_unsigned && self.dev_mode
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_fields(
    id: &str,
    name: &str,
    author: &str,
    license: &str,
    description: &str,
    api_version: u32,
    min_app_version: &Version,
    app_version: &Version,
    permissions: &Permissions,
    config_schema: &[ConfigField],
) -> Result<(), CatalogError> {
    if !is_reverse_dns_id(id) {
        return Err(CatalogError::InvalidId(id.to_string()));
    }
    if name.trim().is_empty() {
        return Err(invalid(id, "name must not be empty"));
    }
    if author.trim().is_empty() {
        return Err(invalid(id, "author must not be empty"));
    }
    if license.trim().is_empty() {
        return Err(invalid(id, "license must not be empty"));
    }
    if description.trim().is_empty() {
        return Err(invalid(id, "description must not be empty"));
    }
    if description.len() > 2000 {
        return Err(invalid(id, "description exceeds 2000 bytes"));
    }
    if api_version != API_VERSION {
        return Err(CatalogError::IncompatibleApi {
            id: id.to_string(),
            required: api_version,
            supported: API_VERSION,
        });
    }
    if min_app_version > app_version {
        return Err(CatalogError::IncompatibleApp {
            id: id.to_string(),
            required: min_app_version.clone(),
            current: app_version.clone(),
        });
    }
    if permissions.http_hosts.len() > MAX_HTTP_HOSTS {
        return Err(invalid(id, "too many declared HTTP hosts"));
    }
    for host in &permissions.http_hosts {
        if !is_valid_http_host(host) {
            return Err(invalid(
                id,
                &format!("invalid HTTP host allowlist entry: {host}"),
            ));
        }
    }
    if permissions.ha_entities.len() > MAX_HA_ENTITIES {
        return Err(invalid(id, "too many declared Home Assistant entities"));
    }
    for entity in &permissions.ha_entities {
        if !is_valid_entity_id(entity) {
            return Err(invalid(
                id,
                &format!("invalid entity id allowlist entry: {entity}"),
            ));
        }
    }
    if config_schema.len() > MAX_CONFIG_FIELDS {
        return Err(invalid(id, "too many config fields"));
    }
    let mut keys: HashSet<&str> = HashSet::new();
    for field in config_schema {
        if !is_valid_config_key(&field.key) {
            return Err(invalid(id, &format!("invalid config key: {}", field.key)));
        }
        if !keys.insert(field.key.as_str()) {
            return Err(invalid(id, &format!("duplicate config key: {}", field.key)));
        }
        if field.label.trim().is_empty() {
            return Err(invalid(
                id,
                &format!("config field {} has an empty label", field.key),
            ));
        }
        if field.field_type == ConfigFieldType::Select && field.options.is_empty() {
            return Err(invalid(
                id,
                &format!("select field {} has no options", field.key),
            ));
        }
        if let (Some(min), Some(max)) = (field.min, field.max)
            && min > max
        {
            return Err(invalid(
                id,
                &format!("config field {} has min > max", field.key),
            ));
        }
    }
    Ok(())
}

fn invalid(id: &str, reason: &str) -> CatalogError {
    CatalogError::InvalidManifest {
        id: id.to_string(),
        reason: reason.to_string(),
    }
}

/// Reverse-DNS identifiers: two or more lowercase labels.
pub fn is_reverse_dns_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 {
        return false;
    }
    let labels: Vec<&str> = id.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !label.ends_with('-')
    })
}

/// Reject absolute paths, parent traversal, and Windows prefixes.
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return false;
    }
    Path::new(path)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
}

/// `host` or `host:port` without scheme, path, or wildcard.
pub fn is_valid_http_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let name = match host.rsplit_once(':') {
        Some((name, port)) => {
            if name.contains(':') {
                return false;
            }
            match port.parse::<u16>() {
                Ok(value) if value > 0 => name,
                _ => return false,
            }
        }
        None => host,
    };
    if name.is_empty() || name.contains('*') {
        return false;
    }
    name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Home Assistant entity IDs: `domain.object_id`, lowercase.
pub fn is_valid_entity_id(id: &str) -> bool {
    let Some((domain, object)) = id.split_once('.') else {
        return false;
    };
    if domain.is_empty() || object.is_empty() || object.contains('.') {
        return false;
    }
    domain.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        && object
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_valid_config_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}
