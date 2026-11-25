use crate::events::EntityId;
use chrono_tz::Tz;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("token resolution failed: {0}")]
    Token(String),
    #[error("cache error: {0}")]
    Cache(String),
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Toml(e.to_string())
    }
}

fn default_secret() -> SecretString {
    SecretString::from(String::new())
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HaConfig {
    pub url: Url,
    #[serde(default = "default_secret")]
    pub token: SecretString,
    #[serde(default)]
    pub token_file: Option<PathBuf>,
    #[serde(default)]
    pub allow_invalid_certs: bool,
    #[serde(default)]
    pub extra_entity_ids: Vec<EntityId>,
    #[serde(default)]
    pub time_zone: Option<Tz>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayConfig {
    #[serde(default = "default_u32_800")]
    pub width: u32,
    #[serde(default = "default_u32_480")]
    pub height: u32,
    #[serde(default)]
    pub rotation: u16,
    #[serde(default = "default_true")]
    pub fullscreen: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_u32_800() -> u32 {
    800
}
fn default_u32_480() -> u32 {
    480
}
fn default_true() -> bool {
    true
}
fn default_theme() -> String {
    "light".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PageConfig {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub widgets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PagesConfig {
    #[serde(default)]
    pub overview: PageConfig,
    #[serde(default)]
    pub media: PageConfig,
    #[serde(default)]
    pub agenda: PageConfig,
    #[serde(default)]
    pub status: PageConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockWidgetConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub show_seconds: bool,
    #[serde(default = "default_true")]
    pub use_24_hour: bool,
}

impl Default for ClockWidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_seconds: false,
            use_24_hour: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherWidgetConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub entity: String,
    #[serde(default = "default_forecast_type")]
    pub forecast_type: String,
    #[serde(default = "default_forecast_count")]
    pub forecast_count: u32,
    #[serde(default = "default_refresh_900")]
    pub refresh_seconds: u64,
}

fn default_forecast_type() -> String {
    "daily".to_string()
}
fn default_forecast_count() -> u32 {
    3
}
fn default_refresh_900() -> u64 {
    900
}

impl Default for WeatherWidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entity: String::new(),
            forecast_type: "daily".to_string(),
            forecast_count: 3,
            refresh_seconds: 900,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroLightConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_min_kelvin")]
    pub min_kelvin: u16,
    #[serde(default = "default_max_kelvin")]
    pub max_kelvin: u16,
}

fn default_min_kelvin() -> u16 {
    2700
}
fn default_max_kelvin() -> u16 {
    6500
}

impl Default for HeroLightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entity: String::new(),
            name: String::new(),
            min_kelvin: default_min_kelvin(),
            max_kelvin: default_max_kelvin(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaWidgetConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub entity: String,
    #[serde(default = "default_true")]
    pub show_album_art: bool,
    #[serde(default = "default_volume_step")]
    pub volume_step: f64,
}

fn default_volume_step() -> f64 {
    0.05
}

impl Default for MediaWidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entity: String::new(),
            show_album_art: true,
            volume_step: 0.05,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeparturesWidgetConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub entity: String,
    #[serde(default = "default_title_departures")]
    pub title: String,
    #[serde(default = "default_max_items")]
    pub max_items: u32,
    #[serde(default = "default_refresh_60")]
    pub refresh_seconds: u64,
}

fn default_title_departures() -> String {
    "Departures".to_string()
}
fn default_max_items() -> u32 {
    6
}
fn default_refresh_60() -> u64 {
    60
}

impl Default for DeparturesWidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entity: String::new(),
            title: "Departures".to_string(),
            max_items: 6,
            refresh_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarWidgetConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub entities: Vec<EntityId>,
    #[serde(default = "default_days")]
    pub days: u32,
    #[serde(default = "default_refresh_300")]
    pub refresh_seconds: u64,
}

fn default_days() -> u32 {
    2
}
fn default_refresh_300() -> u64 {
    300
}

impl Default for CalendarWidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entities: Vec::new(),
            days: 2,
            refresh_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WidgetsConfig {
    #[serde(default)]
    pub clock: ClockWidgetConfig,
    #[serde(default)]
    pub hero_light: HeroLightConfig,
    #[serde(default)]
    pub weather: WeatherWidgetConfig,
    #[serde(default)]
    pub media: MediaWidgetConfig,
    #[serde(default)]
    pub departures: DeparturesWidgetConfig,
    #[serde(default)]
    pub calendar: CalendarWidgetConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileConfig {
    pub entity: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TilesConfig {
    #[serde(default = "default_columns")]
    pub columns: u32,
    #[serde(default)]
    pub items: Vec<TileConfig>,
}

fn default_columns() -> u32 {
    3
}

impl Default for TilesConfig {
    fn default() -> Self {
        Self {
            columns: 3,
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    pub path: PathBuf,
    #[serde(default = "default_debounce")]
    pub write_debounce_seconds: u64,
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
}

fn default_debounce() -> u64 {
    10
}
fn default_max_bytes() -> u64 {
    262144
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub catalog_url: Option<Url>,
    #[serde(default)]
    pub public_keys: Vec<String>,
    #[serde(default)]
    pub allow_unsigned: bool,
    #[serde(default)]
    pub dev_mode: bool,
    #[serde(default = "default_refresh_hours")]
    pub refresh_hours: u64,
    #[serde(default = "default_max_catalog_bytes")]
    pub max_catalog_bytes: u64,
    #[serde(default = "default_max_package_bytes")]
    pub max_package_bytes: u64,
    #[serde(default = "default_max_script_ops")]
    pub max_script_operations: u64,
    #[serde(default = "default_store_path")]
    pub store_path: PathBuf,
    #[serde(default)]
    pub settings: BTreeMap<String, toml::Value>,
}

impl Default for ExtensionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            catalog_url: None,
            public_keys: Vec::new(),
            allow_unsigned: false,
            dev_mode: false,
            refresh_hours: default_refresh_hours(),
            max_catalog_bytes: default_max_catalog_bytes(),
            max_package_bytes: default_max_package_bytes(),
            max_script_operations: default_max_script_ops(),
            store_path: default_store_path(),
            settings: BTreeMap::new(),
        }
    }
}

impl ExtensionsConfig {
    /// Build the catalog trust policy for the running kiosk version.
    pub fn catalog_policy(&self, app_version: semver::Version) -> crate::extensions::CatalogPolicy {
        crate::extensions::CatalogPolicy {
            app_version,
            public_keys: self.public_keys.clone(),
            allow_unsigned: self.allow_unsigned,
            dev_mode: self.dev_mode,
            max_catalog_bytes: self.max_catalog_bytes,
            max_package_bytes: self.max_package_bytes,
        }
    }

    /// Default cache location for the last verified catalog index.
    pub fn catalog_cache_path(&self) -> PathBuf {
        self.store_path.join("catalog-cache.json")
    }
}

fn default_refresh_hours() -> u64 {
    24
}
fn default_max_catalog_bytes() -> u64 {
    2097152
}
fn default_max_package_bytes() -> u64 {
    5242880
}
fn default_max_script_ops() -> u64 {
    1_000_000
}
fn default_store_path() -> PathBuf {
    PathBuf::from("/var/lib/ha-kiosk/extensions")
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub ha: HaConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub pages: PagesConfig,
    #[serde(default)]
    pub widgets: WidgetsConfig,
    #[serde(default)]
    pub tiles: TilesConfig,
    pub cache: CacheConfig,
    #[serde(default)]
    pub extensions: ExtensionsConfig,
    /// Path the configuration was loaded from. Not part of the file schema; the
    /// shop uses it to show where `[extensions.settings]` belongs.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

// DisplayConfig needs Default
impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 480,
            rotation: 0,
            fullscreen: true,
            theme: "light".to_string(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&raw)?;
        config.source_path = Some(path.to_path_buf());
        // token resolution
        let token = resolve_token(&config.ha)?;
        config.ha.token = token;
        validate(&config)?;
        Ok(config)
    }

    pub fn subscribed_entity_ids(&self) -> Vec<EntityId> {
        let mut ids: Vec<EntityId> = Vec::new();
        let mut push = |id: &str| {
            if !id.is_empty() && !ids.contains(&id.to_string()) {
                ids.push(id.to_string());
            }
        };
        if self.widgets.weather.enabled {
            push(&self.widgets.weather.entity);
        }
        if self.widgets.hero_light.enabled {
            push(&self.widgets.hero_light.entity);
        }
        if self.widgets.media.enabled {
            push(&self.widgets.media.entity);
        }
        if self.widgets.departures.enabled {
            push(&self.widgets.departures.entity);
        }
        if self.widgets.calendar.enabled {
            for e in &self.widgets.calendar.entities {
                push(e);
            }
        }
        for t in &self.tiles.items {
            push(&t.entity);
        }
        for e in &self.ha.extra_entity_ids {
            push(e);
        }
        ids
    }

    pub fn cache_fingerprint(&self) -> String {
        let mut parts: Vec<String> = self.subscribed_entity_ids();
        parts.push(format!("debounce:{}", self.cache.write_debounce_seconds));
        parts.push(format!("max:{}", self.cache.max_bytes));
        parts.push(format!(
            "clock:{}:{}",
            self.widgets.clock.use_24_hour, self.widgets.clock.show_seconds
        ));
        parts.push(format!(
            "weather:{}:{}:{}:{}",
            self.widgets.weather.enabled,
            self.widgets.weather.forecast_type,
            self.widgets.weather.forecast_count,
            self.widgets.weather.refresh_seconds
        ));
        parts.push(format!(
            "media:{}:{}:{}",
            self.widgets.media.enabled,
            self.widgets.media.show_album_art,
            self.widgets.media.volume_step
        ));
        parts.push(format!(
            "departures:{}:{}:{}",
            self.widgets.departures.enabled,
            self.widgets.departures.max_items,
            self.widgets.departures.refresh_seconds
        ));
        parts.push(format!(
            "calendar:{}:{}:{}",
            self.widgets.calendar.enabled,
            self.widgets.calendar.days,
            self.widgets.calendar.refresh_seconds
        ));
        parts.push(format!("tile-columns:{}", self.tiles.columns));
        for tile in &self.tiles.items {
            parts.push(format!("tile:{}:{}:{}", tile.entity, tile.label, tile.icon));
        }
        for (page, config) in [
            ("overview", &self.pages.overview),
            ("media", &self.pages.media),
            ("agenda", &self.pages.agenda),
            ("status", &self.pages.status),
        ] {
            parts.push(format!(
                "page:{page}:{}:{}",
                config.title,
                config.widgets.join(",")
            ));
        }
        parts.sort();
        parts.join("|")
    }
}

fn resolve_token(ha: &HaConfig) -> Result<SecretString, ConfigError> {
    // env overrides
    if let Ok(env_token) = std::env::var("HA_TOKEN") {
        let trimmed = env_token.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(SecretString::from(trimmed));
        }
    }
    if let Some(path) = &ha.token_file {
        if !path.is_absolute() {
            return Err(ConfigError::Token(
                "token_file must be an absolute path".into(),
            ));
        }
        check_file_mode(path)?;
        let contents = std::fs::read_to_string(path)?;
        let trimmed = contents.trim().to_string();
        if trimmed.is_empty() {
            return Err(ConfigError::Token("token file is empty".into()));
        }
        return Ok(SecretString::from(trimmed));
    }
    let exposed = ha.token.expose_secret();
    if !exposed.trim().is_empty() {
        return Ok(ha.token.clone());
    }
    Err(ConfigError::Token(
        "no token provided (set HA_TOKEN, token_file, or ha.token)".into(),
    ))
}

fn check_file_mode(path: &Path) -> Result<(), ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(ConfigError::Token(format!(
                "token file {} is world/group readable (mode {:04o}); require 0600",
                path.display(),
                mode
            )));
        }
        if mode != 0o600 {
            // allow stricter? require exactly 0600 per spec
        }
    }
    Ok(())
}

fn validate_entity_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() != 2 {
        return false;
    }
    let domain = parts[0];
    let object = parts[1];
    if domain.is_empty() || object.is_empty() {
        return false;
    }
    domain.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        && object
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn validate(config: &Config) -> Result<(), ConfigError> {
    // URL scheme
    match config.ha.url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ConfigError::Validation(format!(
                "ha.url must use http or https, got {}",
                other
            )));
        }
    }
    // display
    if !matches!(config.display.rotation, 0 | 90 | 180 | 270) {
        return Err(ConfigError::Validation(
            "display.rotation must be 0, 90, 180, or 270".into(),
        ));
    }
    if config.display.width == 0 || config.display.height == 0 {
        return Err(ConfigError::Validation(
            "display dimensions must be > 0".into(),
        ));
    }
    if !matches!(config.display.theme.as_str(), "dark" | "light") {
        return Err(ConfigError::Validation(
            "display.theme must be dark or light".into(),
        ));
    }
    // refresh intervals
    if config.cache.write_debounce_seconds < 2 {
        return Err(ConfigError::Validation(
            "cache.write_debounce_seconds must be >= 2".into(),
        ));
    }
    if config.widgets.weather.refresh_seconds < 60 {
        return Err(ConfigError::Validation(
            "widgets.weather.refresh_seconds must be >= 60".into(),
        ));
    }
    if !(1..=24).contains(&config.widgets.weather.forecast_count) {
        return Err(ConfigError::Validation(
            "widgets.weather.forecast_count must be between 1 and 24".into(),
        ));
    }
    if !config.widgets.media.volume_step.is_finite()
        || !(0.01..=1.0).contains(&config.widgets.media.volume_step)
    {
        return Err(ConfigError::Validation(
            "widgets.media.volume_step must be between 0.01 and 1.0".into(),
        ));
    }
    if !(1..=14).contains(&config.widgets.calendar.days) {
        return Err(ConfigError::Validation(
            "widgets.calendar.days must be between 1 and 14".into(),
        ));
    }
    if config.widgets.departures.refresh_seconds < 10 {
        return Err(ConfigError::Validation(
            "widgets.departures.refresh_seconds must be >= 10".into(),
        ));
    }
    if !(1..=20).contains(&config.widgets.departures.max_items) {
        return Err(ConfigError::Validation(
            "widgets.departures.max_items must be between 1 and 20".into(),
        ));
    }
    if config.widgets.calendar.refresh_seconds < 30 {
        return Err(ConfigError::Validation(
            "widgets.calendar.refresh_seconds must be >= 30".into(),
        ));
    }
    // entity ids
    if config.widgets.weather.enabled
        && !config.widgets.weather.entity.is_empty()
        && (!validate_entity_id(&config.widgets.weather.entity)
            || !config.widgets.weather.entity.starts_with("weather."))
    {
        return Err(ConfigError::Validation(format!(
            "widgets.weather.entity must be a valid weather.* entity id: {}",
            config.widgets.weather.entity
        )));
    }
    if config.widgets.hero_light.enabled
        && !config.widgets.hero_light.entity.is_empty()
        && !validate_entity_id(&config.widgets.hero_light.entity)
    {
        return Err(ConfigError::Validation(format!(
            "invalid entity id: {}",
            config.widgets.hero_light.entity
        )));
    }
    if config.widgets.hero_light.enabled {
        if config.widgets.hero_light.min_kelvin >= config.widgets.hero_light.max_kelvin {
            return Err(ConfigError::Validation(
                "widgets.hero_light.min_kelvin must be lower than max_kelvin".into(),
            ));
        }
        if config.widgets.hero_light.min_kelvin < 1000
            || config.widgets.hero_light.max_kelvin > 10000
        {
            return Err(ConfigError::Validation(
                "widgets.hero_light colour temperature must stay between 1000 K and 10000 K".into(),
            ));
        }
    }
    if config.widgets.media.enabled
        && !config.widgets.media.entity.is_empty()
        && (!validate_entity_id(&config.widgets.media.entity)
            || !config.widgets.media.entity.starts_with("media_player."))
    {
        return Err(ConfigError::Validation(format!(
            "widgets.media.entity must be a valid media_player.* entity id: {}",
            config.widgets.media.entity
        )));
    }
    if config.widgets.departures.enabled
        && !config.widgets.departures.entity.is_empty()
        && !validate_entity_id(&config.widgets.departures.entity)
    {
        return Err(ConfigError::Validation(format!(
            "invalid entity id: {}",
            config.widgets.departures.entity
        )));
    }
    for e in &config.widgets.calendar.entities {
        if !validate_entity_id(e) || !e.starts_with("calendar.") {
            return Err(ConfigError::Validation(format!(
                "widgets.calendar.entities must contain valid calendar.* entity ids: {}",
                e
            )));
        }
    }
    for t in &config.tiles.items {
        if !validate_entity_id(&t.entity) {
            return Err(ConfigError::Validation(format!(
                "invalid entity id: {}",
                t.entity
            )));
        }
    }
    if !config.tiles.items.is_empty() && config.tiles.columns == 0 {
        return Err(ConfigError::Validation(
            "tiles.columns must be >= 1 when tiles are configured".into(),
        ));
    }
    for e in &config.ha.extra_entity_ids {
        if !validate_entity_id(e) {
            return Err(ConfigError::Validation(format!("invalid entity id: {}", e)));
        }
    }
    // paths absolute
    if !config.cache.path.is_absolute() {
        return Err(ConfigError::Validation(
            "cache.path must be an absolute path".into(),
        ));
    }
    if let Some(tz) = config.ha.time_zone {
        let _ = tz;
    }
    // extensions
    if !config.extensions.store_path.is_absolute() {
        return Err(ConfigError::Validation(
            "extensions.store_path must be an absolute path".into(),
        ));
    }
    if config.extensions.max_catalog_bytes == 0 || config.extensions.max_package_bytes == 0 {
        return Err(ConfigError::Validation(
            "extensions size limits must be greater than zero".into(),
        ));
    }
    if config.extensions.enabled {
        if config.extensions.catalog_url.is_none() {
            return Err(ConfigError::Validation(
                "extensions.catalog_url is required when extensions are enabled".into(),
            ));
        }
        if config.extensions.public_keys.is_empty()
            && !(config.extensions.allow_unsigned && config.extensions.dev_mode)
        {
            return Err(ConfigError::Validation(
                "extensions.public_keys is empty; unsigned catalogs require allow_unsigned and dev_mode"
                    .into(),
            ));
        }
    }
    // subscribed entities non-empty
    if config.subscribed_entity_ids().is_empty() {
        return Err(ConfigError::Validation(
            "at least one subscribed entity is required".into(),
        ));
    }
    // forecast type
    if !matches!(
        config.widgets.weather.forecast_type.as_str(),
        "daily" | "hourly"
    ) {
        return Err(ConfigError::Validation(
            "widgets.weather.forecast_type must be daily or hourly".into(),
        ));
    }
    validate_extensions(&config.extensions)?;
    Ok(())
}

fn validate_extensions(ext: &ExtensionsConfig) -> Result<(), ConfigError> {
    if !ext.enabled {
        return Ok(());
    }
    let Some(url) = &ext.catalog_url else {
        return Err(ConfigError::Validation(
            "extensions.catalog_url is required when extensions are enabled".into(),
        ));
    };
    if url.scheme() != "https" && !ext.dev_mode {
        return Err(ConfigError::Validation(
            "extensions.catalog_url must use https unless extensions.dev_mode is true".into(),
        ));
    }
    if ext.allow_unsigned && !ext.dev_mode {
        return Err(ConfigError::Validation(
            "extensions.allow_unsigned requires extensions.dev_mode = true".into(),
        ));
    }
    if ext.public_keys.is_empty() && !ext.allow_unsigned {
        return Err(ConfigError::Validation(
            "extensions.public_keys must not be empty unless extensions.allow_unsigned is true"
                .into(),
        ));
    }
    Ok(())
}

// Implement Debug redaction safety: Config derives Debug via fields; SecretString Debug is redacted.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("ha", &self.ha)
            .field("display", &self.display)
            .field("cache", &self.cache)
            .finish()
    }
}

impl std::fmt::Debug for HaConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HaConfig")
            .field("url", &self.url)
            .field("token", &"[REDACTED]")
            .field("token_file", &self.token_file)
            .field("allow_invalid_certs", &self.allow_invalid_certs)
            .field("extra_entity_ids", &self.extra_entity_ids)
            .field("time_zone", &self.time_zone)
            .finish()
    }
}
