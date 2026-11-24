//! Configuration types frozen in Wave 0.
//!
//! TOML loading, validation, token resolution, and redaction are implemented
//! in Wave 1.

use crate::events::EntityId;
use chrono_tz::Tz;
use secrecy::SecretString;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration loading is not implemented until Wave 1")]
    NotImplemented,
}

/// Home Assistant connection settings.
#[derive(Debug, Clone)]
pub struct HaConfig {
    /// Base URL of the Home Assistant instance. A path prefix is preserved.
    pub url: Url,
    /// Long-lived access token. `Debug` output is redacted.
    pub token: SecretString,
    /// Optional file that contains only the access token.
    pub token_file: Option<PathBuf>,
    /// Disable TLS certificate validation (last resort for self-signed hosts).
    pub allow_invalid_certs: bool,
    /// Additional entity IDs subscribed to on top of widget and tile entities.
    pub extra_entity_ids: Vec<EntityId>,
    /// IANA fallback zone used only when `get_config` omits one.
    pub time_zone: Option<Tz>,
}

/// Display settings. Fields are frozen in Wave 1.
#[derive(Debug, Clone, Default)]
pub struct DisplayConfig;

/// Page and widget ordering. Fields are frozen in Wave 2.
#[derive(Debug, Clone, Default)]
pub struct PagesConfig;

/// Widget settings. Fields are frozen in Wave 1.
#[derive(Debug, Clone, Default)]
pub struct WidgetsConfig;

/// Tile settings. Fields are frozen in Wave 3.
#[derive(Debug, Clone, Default)]
pub struct TilesConfig;

/// Offline cache settings. Fields are frozen in Wave 1.
#[derive(Debug, Clone, Default)]
pub struct CacheConfig;

/// Root configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub ha: HaConfig,
    pub display: DisplayConfig,
    pub pages: PagesConfig,
    pub widgets: WidgetsConfig,
    pub tiles: TilesConfig,
    pub cache: CacheConfig,
}

impl Config {
    /// Load and validate a configuration file.
    ///
    /// Wave 0 only freezes the signature; Wave 1 implements loading.
    pub fn load(_path: &std::path::Path) -> Result<Self, ConfigError> {
        Err(ConfigError::NotImplemented)
    }

    /// Widget and tile entities plus `ha.extra_entity_ids`, deduplicated.
    ///
    /// Widget and tile sources do not exist yet, so the Wave 0 stub returns
    /// the configured extra entity IDs only.
    pub fn subscribed_entity_ids(&self) -> Vec<EntityId> {
        let mut ids = self.ha.extra_entity_ids.clone();
        ids.sort();
        ids.dedup();
        ids
    }
}
