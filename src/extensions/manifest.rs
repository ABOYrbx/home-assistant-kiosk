//! Extension manifest schema and validation.
//!
//! The manifest is the trust contract: it declares the entry script, the
//! output kind, the HTTP hosts, the Home Assistant entities, and the
//! configuration fields. Unknown keys are rejected so a typo cannot silently
//! widen a permission.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use url::Url;

use super::http::{HttpError, parse_allowed_host};

/// The extension API version implemented by this kiosk.
pub const SUPPORTED_API_VERSION: u32 = 1;

/// The declarative view model an extension returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    Departures,
    List,
    KeyValue,
}

impl OutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Departures => "departures",
            Self::List => "list",
            Self::KeyValue => "key_value",
        }
    }
}

impl fmt::Display for OutputKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Permission allowlists declared by an extension.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub http_hosts: Vec<String>,
    #[serde(default)]
    pub ha_entities: Vec<String>,
}

/// A configuration field shown by the shop and validated by the host.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ConfigFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// Supported configuration field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

/// A parsed and validated extension manifest.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub min_app_version: String,
    pub entry: String,
    pub provides: OutputKind,
    pub author: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub description: String,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub config_schema: Vec<ConfigField>,
}

impl Manifest {
    /// Parse and validate a manifest from TOML text.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: Self = toml::from_str(text)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Load and validate a manifest from a file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Validate every rule that does not need a running engine.
    pub fn validate(&self) -> Result<(), ManifestError> {
        validate_extension_id(&self.id)?;
        semver::Version::parse(&self.version)
            .map_err(|_| ManifestError::InvalidVersion(self.version.clone()))?;
        semver::Version::parse(&self.min_app_version)
            .map_err(|_| ManifestError::InvalidVersion(self.min_app_version.clone()))?;
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(ManifestError::UnsupportedApiVersion {
                found: self.api_version,
                supported: SUPPORTED_API_VERSION,
            });
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::InvalidConfig(
                "name must not be empty".into(),
            ));
        }
        if self.author.trim().is_empty() {
            return Err(ManifestError::InvalidConfig(
                "author must not be empty".into(),
            ));
        }
        if self.license.trim().is_empty() {
            return Err(ManifestError::InvalidConfig(
                "license must not be empty".into(),
            ));
        }
        if self.description.trim().is_empty() {
            return Err(ManifestError::InvalidConfig(
                "description must not be empty".into(),
            ));
        }
        validate_entry(&self.entry)?;
        if let Some(homepage) = &self.homepage {
            let url = Url::parse(homepage)
                .map_err(|_| ManifestError::InvalidConfig("homepage must be a valid URL".into()))?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(ManifestError::InvalidConfig(
                    "homepage must use http or https".into(),
                ));
            }
        }
        for host in &self.permissions.http_hosts {
            parse_allowed_host(host).map_err(|_| ManifestError::InvalidHttpHost(host.clone()))?;
        }
        for entity in &self.permissions.ha_entities {
            validate_entity_id(entity)?;
        }
        self.validate_config_schema()
    }

    fn validate_config_schema(&self) -> Result<(), ManifestError> {
        let mut seen = std::collections::BTreeSet::new();
        for field in &self.config_schema {
            if !is_valid_config_key(&field.key) {
                return Err(ManifestError::InvalidConfig(format!(
                    "invalid config key: {}",
                    field.key
                )));
            }
            if !seen.insert(field.key.as_str()) {
                return Err(ManifestError::InvalidConfig(format!(
                    "duplicate config key: {}",
                    field.key
                )));
            }
            if field.label.trim().is_empty() {
                return Err(ManifestError::InvalidConfig(format!(
                    "config field {} has an empty label",
                    field.key
                )));
            }
            if field.field_type == ConfigFieldType::Select && field.options.is_empty() {
                return Err(ManifestError::InvalidConfig(format!(
                    "select field {} needs at least one option",
                    field.key
                )));
            }
            if let (Some(min), Some(max)) = (field.min, field.max)
                && min > max
            {
                return Err(ManifestError::InvalidConfig(format!(
                    "config field {} has min greater than max",
                    field.key
                )));
            }
            if field.field_type == ConfigFieldType::Secret && field.default.is_some() {
                return Err(ManifestError::InvalidConfig(format!(
                    "secret field {} must not declare a default",
                    field.key
                )));
            }
            if let Some(default) = &field.default {
                validate_config_value(field, default).map_err(|reason| {
                    ManifestError::InvalidConfig(format!(
                        "default for {} is invalid: {reason}",
                        field.key
                    ))
                })?;
            }
        }
        Ok(())
    }

    /// Build the `config` map passed to `refresh(config)`.
    ///
    /// Only declared secret fields are read from `secrets`; all other fields
    /// come from `settings`. Unknown keys are rejected, and a missing required
    /// value is an error.
    pub fn build_config(
        &self,
        settings: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, String>,
    ) -> Result<serde_json::Map<String, Value>, ManifestError> {
        let mut config = serde_json::Map::new();
        for field in &self.config_schema {
            let raw = if field.field_type == ConfigFieldType::Secret {
                if settings.contains_key(&field.key) {
                    return Err(ManifestError::InvalidSetting {
                        key: field.key.clone(),
                        reason: "secret values belong in the secret file".into(),
                    });
                }
                secrets
                    .get(&field.key)
                    .map(|value| Value::String(value.clone()))
            } else {
                if secrets.contains_key(&field.key) {
                    return Err(ManifestError::InvalidSetting {
                        key: field.key.clone(),
                        reason: "non-secret values belong in the settings table".into(),
                    });
                }
                settings.get(&field.key).cloned()
            };

            match raw {
                Some(value) => {
                    validate_config_value(field, &value).map_err(|reason| {
                        ManifestError::InvalidSetting {
                            key: field.key.clone(),
                            reason,
                        }
                    })?;
                    config.insert(field.key.clone(), value);
                }
                None if field.required => {
                    return Err(ManifestError::MissingSetting(field.key.clone()));
                }
                None => {
                    if let Some(default) = &field.default {
                        config.insert(field.key.clone(), default.clone());
                    }
                }
            }
        }

        for key in settings.keys() {
            let declared = self
                .config_schema
                .iter()
                .any(|field| &field.key == key && field.field_type != ConfigFieldType::Secret);
            if !declared {
                return Err(ManifestError::UnknownSetting(key.clone()));
            }
        }
        for key in secrets.keys() {
            let declared = self
                .config_schema
                .iter()
                .any(|field| &field.key == key && field.field_type == ConfigFieldType::Secret);
            if !declared {
                return Err(ManifestError::UnknownSetting(key.clone()));
            }
        }
        Ok(config)
    }
}

/// Manifest parsing and validation errors.
#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("cannot read manifest: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid extension id: {0}")]
    InvalidId(String),
    #[error("invalid version: {0}")]
    InvalidVersion(String),
    #[error("unsupported api_version {found}; this kiosk supports {supported}")]
    UnsupportedApiVersion { found: u32, supported: u32 },
    #[error("invalid entry path: {0}")]
    InvalidEntry(String),
    #[error("invalid http host: {0}")]
    InvalidHttpHost(String),
    #[error("invalid entity id: {0}")]
    InvalidEntityId(String),
    #[error("invalid manifest: {0}")]
    InvalidConfig(String),
    #[error("missing required setting: {0}")]
    MissingSetting(String),
    #[error("unknown setting: {0}")]
    UnknownSetting(String),
    #[error("invalid setting {key}: {reason}")]
    InvalidSetting { key: String, reason: String },
    #[error(transparent)]
    Http(#[from] HttpError),
}

fn validate_extension_id(id: &str) -> Result<(), ManifestError> {
    if id.is_empty() || id.len() > 128 {
        return Err(ManifestError::InvalidId(id.to_string()));
    }
    let labels: Vec<&str> = id.split('.').collect();
    let valid = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
                && label
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    if valid {
        Ok(())
    } else {
        Err(ManifestError::InvalidId(id.to_string()))
    }
}

fn validate_entry(entry: &str) -> Result<(), ManifestError> {
    let invalid = entry.is_empty()
        || entry.len() > 128
        || entry.starts_with('/')
        || entry.contains('\\')
        || !entry.ends_with(".rhai")
        || entry
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if invalid {
        Err(ManifestError::InvalidEntry(entry.to_string()))
    } else {
        Ok(())
    }
}

fn validate_entity_id(entity_id: &str) -> Result<(), ManifestError> {
    let valid = entity_id.split_once('.').is_some_and(|(domain, object)| {
        !domain.is_empty()
            && !object.is_empty()
            && !object.contains('.')
            && domain
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && object
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    });
    if valid {
        Ok(())
    } else {
        Err(ManifestError::InvalidEntityId(entity_id.to_string()))
    }
}

fn is_valid_config_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn validate_config_value(field: &ConfigField, value: &Value) -> Result<(), String> {
    match field.field_type {
        ConfigFieldType::String | ConfigFieldType::Secret => value
            .is_string()
            .then_some(())
            .ok_or_else(|| "expected a string".to_string()),
        ConfigFieldType::Integer => {
            let number = value
                .as_i64()
                .ok_or_else(|| "expected an integer".to_string())?;
            if let Some(min) = field.min
                && number < min
            {
                return Err(format!("must be at least {min}"));
            }
            if let Some(max) = field.max
                && number > max
            {
                return Err(format!("must be at most {max}"));
            }
            Ok(())
        }
        ConfigFieldType::Boolean => value
            .is_boolean()
            .then_some(())
            .ok_or_else(|| "expected a boolean".to_string()),
        ConfigFieldType::Url => {
            let text = value
                .as_str()
                .ok_or_else(|| "expected a URL string".to_string())?;
            let url = Url::parse(text).map_err(|_| "invalid URL".to_string())?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err("URL must use http or https".into());
            }
            Ok(())
        }
        ConfigFieldType::EntityId => {
            let text = value
                .as_str()
                .ok_or_else(|| "expected an entity ID string".to_string())?;
            validate_entity_id(text).map_err(|_| format!("invalid entity ID: {text}"))
        }
        ConfigFieldType::Select => {
            let text = value
                .as_str()
                .ok_or_else(|| "expected a string option".to_string())?;
            if field.options.iter().any(|option| option == text) {
                Ok(())
            } else {
                Err(format!("must be one of: {}", field.options.join(", ")))
            }
        }
    }
}
