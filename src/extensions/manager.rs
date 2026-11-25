//! Connects the signed catalog, the package store, and the sandboxed host.
//!
//! The manager is intentionally blocking: callers run it on a background
//! thread and send the results to the UI thread. It never runs extension code
//! on the UI thread and it never exposes the Home Assistant token.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value as JsonValue;
use thiserror::Error;
use url::Url;

use super::catalog::{CatalogEntry, CatalogError, CatalogIndex, CatalogPolicy, ExtensionManifest};
use super::client::{CatalogCache, CatalogClient, CatalogFetcher, FetchError};
use super::host::{EntitySnapshot, ExtensionError, ExtensionHost, HostConfig, Limits};
use super::manifest::Manifest;
use super::output::ExtensionOutput;
use super::runner::ExtensionRunner;
use super::shop::{ConfigField, ExtensionPermissions, InstalledExtension, ShopEntry};
use super::store::{ExtensionStore, StoreError};

/// Everything the manager needs from `[extensions]`.
#[derive(Debug, Clone)]
pub struct ManagerOptions {
    pub app_version: semver::Version,
    pub catalog_url: Option<Url>,
    pub public_keys: Vec<String>,
    pub allow_unsigned: bool,
    pub dev_mode: bool,
    pub max_catalog_bytes: u64,
    pub max_package_bytes: u64,
    pub limits: Limits,
    pub store_path: PathBuf,
    pub secret_dir: PathBuf,
    pub settings: BTreeMap<String, toml::Value>,
}

/// Errors surfaced by the manager and the shop.
#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("extensions are not enabled")]
    Disabled,
    #[error("no catalog URL is configured")]
    NoCatalog,
    #[error("extension not installed: {0}")]
    NotInstalled(String),
    #[error("extension already installed: {0}")]
    AlreadyInstalled(String),
    #[error("no update is available for {0}")]
    NoUpdate(String),
    #[error("catalog: {0}")]
    Catalog(#[from] CatalogError),
    #[error("store: {0}")]
    Store(#[from] StoreError),
    #[error("fetch: {0}")]
    Fetch(#[from] FetchError),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("host: {0}")]
    Host(#[from] ExtensionError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Health of one extension after its last runs.
#[derive(Debug, Clone, Default)]
pub struct ExtensionHealth {
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
    pub degraded: bool,
    pub last_good: Option<ExtensionOutput>,
}

impl ExtensionHealth {
    pub fn mark_success(&mut self, output: ExtensionOutput) {
        self.last_error = None;
        self.consecutive_failures = 0;
        self.degraded = false;
        self.last_good = Some(output);
    }

    pub fn mark_failure(&mut self, error: impl Into<String>) {
        self.last_error = Some(error.into());
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.degraded = self.consecutive_failures >= 3;
    }
}

/// The extension manager used by the shop page and the Status page.
pub struct ExtensionManager<C: CatalogFetcher, P: CatalogFetcher> {
    options: ManagerOptions,
    store: ExtensionStore,
    client: Option<CatalogClient<C>>,
    package_fetcher: P,
    catalog: Option<CatalogIndex>,
    health: BTreeMap<String, ExtensionHealth>,
}

impl<C: CatalogFetcher, P: CatalogFetcher> ExtensionManager<C, P> {
    /// Open the store and build the catalog client.
    pub fn new(
        options: ManagerOptions,
        catalog_fetcher: C,
        package_fetcher: P,
    ) -> Result<Self, ManagerError> {
        let store = ExtensionStore::open(options.store_path.clone())?;
        let client = options.catalog_url.clone().map(|url| {
            let policy = CatalogPolicy {
                app_version: options.app_version.clone(),
                public_keys: options.public_keys.clone(),
                allow_unsigned: options.allow_unsigned,
                dev_mode: options.dev_mode,
                max_catalog_bytes: options.max_catalog_bytes,
                max_package_bytes: options.max_package_bytes,
            };
            CatalogClient::new(
                catalog_fetcher,
                policy,
                url,
                CatalogCache::new(options.store_path.join("catalog-cache.json")),
            )
        });
        Ok(Self {
            options,
            store,
            client,
            package_fetcher,
            catalog: None,
            health: BTreeMap::new(),
        })
    }

    pub fn store(&self) -> &ExtensionStore {
        &self.store
    }

    /// Fetch and verify the catalog. Falls back to the last verified cache.
    pub fn refresh_catalog(&mut self) -> Result<Vec<ShopEntry>, ManagerError> {
        let client = self.client.as_mut().ok_or(ManagerError::NoCatalog)?;
        let outcome = client.refresh()?;
        self.catalog = Some(outcome.index);
        Ok(self.shop_entries())
    }

    /// Shop entries from the last verified catalog plus installed extensions.
    pub fn shop_entries(&mut self) -> Vec<ShopEntry> {
        let mut entries: Vec<ShopEntry> = Vec::new();
        let installed: Vec<_> = self.store.installed().to_vec();
        if let Some(catalog) = &self.catalog {
            for entry in &catalog.extensions {
                let state = installed.iter().find(|record| record.id == entry.id);
                entries.push(self.to_shop_entry(entry, state));
            }
        }
        for record in &installed {
            if entries.iter().any(|entry| entry.id == record.id) {
                continue;
            }
            // An installed extension stays available when the catalog is
            // offline or the entry was removed from the catalog.
            let mut entry = placeholder_shop_entry(&record.id);
            entry.version = record.version.to_string();
            entry.installed_version = Some(record.version.to_string());
            entry.enabled = record.enabled;
            entry.configured = self.configured(&record.id);
            entry.error = self
                .health
                .get(&record.id)
                .and_then(|health| health.last_error.clone());
            entries.push(entry);
        }
        entries
    }

    /// Installed snapshot for the shop view.
    pub fn installed_entries(&self) -> Vec<InstalledExtension> {
        self.store
            .installed()
            .iter()
            .map(|record| InstalledExtension {
                id: record.id.clone(),
                version: record.version.to_string(),
                enabled: record.enabled,
                configured: self.configured(&record.id),
                error: self
                    .health
                    .get(&record.id)
                    .and_then(|health| health.last_error.clone()),
            })
            .collect()
    }

    /// Download, verify, and install one catalog entry.
    pub fn install(&mut self, id: &str) -> Result<(), ManagerError> {
        if self.store.get(id).is_some() {
            return Err(ManagerError::AlreadyInstalled(id.to_string()));
        }
        self.install_from_catalog(id)
    }

    /// Install the current catalog version of an installed extension.
    pub fn update(&mut self, id: &str) -> Result<(), ManagerError> {
        let record = self
            .store
            .get(id)
            .ok_or_else(|| ManagerError::NotInstalled(id.to_string()))?;
        let entry = self.catalog_entry(id).ok_or(ManagerError::NoCatalog)?;
        if entry.version == record.version {
            return Err(ManagerError::NoUpdate(id.to_string()));
        }
        self.install_from_catalog(id)
    }

    fn install_from_catalog(&mut self, id: &str) -> Result<(), ManagerError> {
        let entry = self.catalog_entry(id).ok_or(ManagerError::NoCatalog)?;
        let bytes = self
            .package_fetcher
            .get(&entry.package_url, self.options.max_package_bytes)?;
        let source = self
            .options
            .catalog_url
            .as_ref()
            .map(Url::to_string)
            .unwrap_or_default();
        self.store
            .install(&entry, &bytes, &source, self.options.max_package_bytes)?;
        self.health.remove(id);
        Ok(())
    }

    /// Remove an extension and its files. Settings are left in the config.
    pub fn remove(&mut self, id: &str) -> Result<(), ManagerError> {
        self.store.remove(id, false)?;
        self.health.remove(id);
        Ok(())
    }

    /// Switch back to the retained previous version.
    pub fn rollback(&mut self, id: &str) -> Result<(), ManagerError> {
        self.store.rollback(id)?;
        self.health.remove(id);
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), ManagerError> {
        self.store.set_enabled(id, enabled)?;
        Ok(())
    }

    /// Run one installed extension against a read-only entity snapshot.
    ///
    /// A failure keeps the last good output in [`ExtensionHealth`] and marks
    /// the extension degraded after three consecutive failures.
    pub fn run(
        &mut self,
        id: &str,
        entities: BTreeMap<String, EntitySnapshot>,
    ) -> Result<ExtensionOutput, ManagerError> {
        let record = self
            .store
            .get(id)
            .ok_or_else(|| ManagerError::NotInstalled(id.to_string()))?;
        if !record.enabled {
            return Err(ManagerError::NotInstalled(id.to_string()));
        }
        let directory = self.store.extension_dir(id, &record.version);
        let catalog_manifest = super::package::load_manifest(&directory)
            .map_err(|error| ManagerError::Manifest(error.to_string()))?;
        let manifest = host_manifest(&catalog_manifest)?;
        let script_path = directory.join(&manifest.entry);
        let script = std::fs::read_to_string(&script_path)?;
        let settings = self.settings_for(id);
        let secrets = self.secrets_for(id)?;
        let allowed_entities = manifest.permissions.ha_entities.clone();
        let entities: BTreeMap<_, _> = entities
            .into_iter()
            .filter(|(entity, _)| allowed_entities.iter().any(|allowed| allowed == entity))
            .collect();

        let mut config = HostConfig::new(manifest, script);
        config.settings = settings;
        config.secrets = secrets;
        config.entities = entities;
        config.dev_mode = self.options.dev_mode;
        config.limits = self.options.limits.clone();
        let mut runner = ExtensionRunner::new(ExtensionHost::new(config)?);
        match runner.refresh() {
            Ok(output) => {
                self.health
                    .entry(id.to_string())
                    .or_default()
                    .mark_success(output.clone());
                Ok(output)
            }
            Err(error) => {
                let health = self.health.entry(id.to_string()).or_default();
                health.mark_failure(error.to_string());
                Err(ManagerError::Host(error))
            }
        }
    }

    pub fn health(&self, id: &str) -> Option<&ExtensionHealth> {
        self.health.get(id)
    }

    pub fn errors(&self) -> Vec<String> {
        self.health
            .iter()
            .filter_map(|(id, health)| {
                health
                    .last_error
                    .as_ref()
                    .map(|error| format!("{id}: {error}"))
            })
            .collect()
    }

    fn catalog_entry(&self, id: &str) -> Option<CatalogEntry> {
        self.catalog
            .as_ref()
            .and_then(|catalog| catalog.find(id))
            .cloned()
    }

    fn to_shop_entry(
        &self,
        entry: &CatalogEntry,
        installed: Option<&super::store::InstalledExtension>,
    ) -> ShopEntry {
        let mut shop = ShopEntry {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: entry.version.to_string(),
            author: entry.author.clone(),
            license: entry.license.clone(),
            description: entry.description.clone(),
            provides: kind_name(entry.provides),
            permissions: ExtensionPermissions {
                http_hosts: entry.permissions.http_hosts.clone(),
                ha_entities: entry.permissions.ha_entities.clone(),
            },
            config_schema: entry
                .config_schema
                .iter()
                .map(|field| ConfigField {
                    key: field.key.clone(),
                    label: field.label.clone(),
                    kind: field_type_name(field.field_type),
                    required: field.required,
                })
                .collect(),
            installed_version: None,
            enabled: false,
            configured: false,
            error: None,
            available: true,
        };
        if let Some(record) = installed {
            shop.installed_version = Some(record.version.to_string());
            shop.enabled = record.enabled;
            shop.configured = self.configured(&entry.id);
            shop.error = self
                .health
                .get(&entry.id)
                .and_then(|health| health.last_error.clone());
        }
        shop
    }

    fn configured(&self, id: &str) -> bool {
        settings_table(self.options.settings.get(id))
            .map(|table| {
                table
                    .values()
                    .any(|value| !value.as_str().unwrap_or_default().is_empty())
            })
            .unwrap_or(false)
    }

    fn settings_for(&self, id: &str) -> BTreeMap<String, JsonValue> {
        settings_table(self.options.settings.get(id))
            .map(|table| {
                table
                    .into_iter()
                    .filter_map(|(key, value)| {
                        serde_json::to_value(value).ok().map(|value| (key, value))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn secrets_for(&self, id: &str) -> Result<BTreeMap<String, String>, ManagerError> {
        let path = self.options.secret_dir.join(format!("{id}.secret.toml"));
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}

/// Convert the signed package manifest into the host manifest schema.
fn host_manifest(manifest: &ExtensionManifest) -> Result<Manifest, ManagerError> {
    let text =
        toml::to_string(manifest).map_err(|error| ManagerError::Manifest(error.to_string()))?;
    Manifest::parse(&text).map_err(|error| ManagerError::Manifest(error.to_string()))
}

fn placeholder_shop_entry(id: &str) -> ShopEntry {
    ShopEntry {
        id: id.to_string(),
        name: id.to_string(),
        version: String::new(),
        author: String::new(),
        license: String::new(),
        description: String::new(),
        provides: String::new(),
        permissions: ExtensionPermissions::default(),
        config_schema: Vec::new(),
        installed_version: None,
        enabled: false,
        configured: false,
        error: None,
        available: false,
    }
}

fn settings_table(value: Option<&toml::Value>) -> Option<BTreeMap<String, toml::Value>> {
    match value? {
        toml::Value::Table(table) => {
            Some(table.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        _ => None,
    }
}

fn kind_name(kind: super::catalog::ExtensionKind) -> String {
    use super::catalog::ExtensionKind;
    match kind {
        ExtensionKind::Departures => "departures",
        ExtensionKind::List => "list",
        ExtensionKind::KeyValue => "key_value",
    }
    .to_string()
}

fn field_type_name(kind: super::catalog::ConfigFieldType) -> String {
    use super::catalog::ConfigFieldType;
    match kind {
        ConfigFieldType::String => "string",
        ConfigFieldType::Integer => "integer",
        ConfigFieldType::Boolean => "boolean",
        ConfigFieldType::Url => "url",
        ConfigFieldType::Secret => "secret",
        ConfigFieldType::EntityId => "entity_id",
        ConfigFieldType::Select => "select",
    }
    .to_string()
}
