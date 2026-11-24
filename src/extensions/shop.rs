//! Shop view model for the community extension shop (Wave 6).
//!
//! This module owns the UI-facing state machine only. Fetching the signed
//! catalog, downloading packages, and running extensions belong to the store
//! and host modules. Keeping the state machine here lets the shop page be
//! tested without a network, a display, or a real package store.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default directory for per-extension secret files, from `docs/extensions.md`.
pub const DEFAULT_SECRET_DIR: &str = "/etc/ha-kiosk/extensions";
/// Default installation root, from `PLAN.md` section 8 (Wave 6).
pub const DEFAULT_INSTALL_DIR: &str = "/var/lib/ha-kiosk/extensions";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionStatus {
    Available,
    Installed,
    UpdateAvailable,
    Disabled,
    Error,
}

impl ExtensionStatus {
    pub fn label(self) -> &'static str {
        match self {
            ExtensionStatus::Available => "Available",
            ExtensionStatus::Installed => "Installed",
            ExtensionStatus::UpdateAvailable => "Update available",
            ExtensionStatus::Disabled => "Disabled",
            ExtensionStatus::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopAction {
    Install,
    Update,
    Remove,
    Enable,
    Disable,
}

impl ShopAction {
    pub fn label(self) -> &'static str {
        match self {
            ShopAction::Install => "Install",
            ShopAction::Update => "Update",
            ShopAction::Remove => "Remove",
            ShopAction::Enable => "Enable",
            ShopAction::Disable => "Disable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExtensionPermissions {
    #[serde(default)]
    pub http_hosts: Vec<String>,
    #[serde(default)]
    pub ha_entities: Vec<String>,
}

impl ExtensionPermissions {
    pub fn is_empty(&self) -> bool {
        self.http_hosts.is_empty() && self.ha_entities.is_empty()
    }

    /// Human-readable summary for the shop card. Never invents permissions.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.http_hosts.is_empty() {
            parts.push(format!("Network: {}", self.http_hosts.join(", ")));
        }
        if !self.ha_entities.is_empty() {
            parts.push(format!("Home Assistant: {}", self.ha_entities.join(", ")));
        }
        if parts.is_empty() {
            "No network or Home Assistant access".to_string()
        } else {
            parts.join("; ")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
}

/// Catalog entry plus local installation state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub provides: String,
    #[serde(default)]
    pub permissions: ExtensionPermissions,
    #[serde(default)]
    pub config_schema: Vec<ConfigField>,
    #[serde(default)]
    pub installed_version: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// True when the entry came from the verified catalog. Installed entries
    /// that disappeared from the catalog stay installed but are not available.
    #[serde(default)]
    pub available: bool,
}

impl ShopEntry {
    pub fn is_installed(&self) -> bool {
        self.installed_version.is_some()
    }

    pub fn status(&self) -> ExtensionStatus {
        if self.error.is_some() {
            return ExtensionStatus::Error;
        }
        match &self.installed_version {
            Some(installed) => {
                if !self.enabled {
                    ExtensionStatus::Disabled
                } else if self.available && self.version != *installed {
                    ExtensionStatus::UpdateAvailable
                } else {
                    ExtensionStatus::Installed
                }
            }
            None => ExtensionStatus::Available,
        }
    }

    pub fn primary_action(&self) -> Option<ShopAction> {
        match self.status() {
            ExtensionStatus::Available => Some(ShopAction::Install),
            ExtensionStatus::UpdateAvailable => Some(ShopAction::Update),
            ExtensionStatus::Installed | ExtensionStatus::Disabled | ExtensionStatus::Error => None,
        }
    }

    pub fn permissions_summary(&self) -> String {
        self.permissions.summary()
    }

    pub fn config_keys_summary(&self) -> String {
        if self.config_schema.is_empty() {
            return "No configuration required".to_string();
        }
        self.config_schema
            .iter()
            .map(|field| {
                if field.required {
                    format!("{} (required)", field.key)
                } else {
                    field.key.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn settings_table(&self) -> String {
        format!("[extensions.settings.\"{}\"]", self.id)
    }

    pub fn secret_file(&self, secret_dir: &Path) -> PathBuf {
        secret_dir.join(format!("{}.secret.toml", self.id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopPending {
    pub id: String,
    pub action: ShopAction,
}

/// A validated request for the extension store. The store performs the work
/// off the UI thread and reports back with `ShopView::complete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopRequest {
    pub id: String,
    pub action: ShopAction,
    pub version: String,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ShopError {
    #[error("extension {0} was not found")]
    Unknown(String),
    #[error("confirmation is required before installing {0}")]
    ConfirmationRequired(String),
    #[error("{0} is not available in the catalog")]
    NotAvailable(String),
    #[error("{0} is already installed")]
    AlreadyInstalled(String),
    #[error("{0} is not installed")]
    NotInstalled(String),
    #[error("no update is available for {0}")]
    NoUpdate(String),
    #[error("another extension action is already pending")]
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopView {
    pub entries: Vec<ShopEntry>,
    pub catalog_online: bool,
    pub catalog_error: Option<String>,
    pub config_path: String,
    pub secret_dir: String,
    pub pending: Option<ShopPending>,
}

impl Default for ShopView {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            catalog_online: false,
            catalog_error: None,
            config_path: String::new(),
            secret_dir: DEFAULT_SECRET_DIR.to_string(),
            pending: None,
        }
    }
}

impl ShopView {
    pub fn new(config_path: impl Into<String>, secret_dir: impl Into<String>) -> Self {
        Self {
            config_path: config_path.into(),
            secret_dir: secret_dir.into(),
            ..Self::default()
        }
    }

    /// Replace the installed snapshot. Entries that are not in the catalog yet
    /// are created as installed-only so the shop works while the catalog is
    /// offline.
    pub fn set_installed(&mut self, installed: Vec<InstalledExtension>) {
        for record in installed {
            if let Some(entry) = self.entries.iter_mut().find(|e| e.id == record.id) {
                entry.installed_version = Some(record.version);
                entry.enabled = record.enabled;
                entry.configured = record.configured;
                entry.error = record.error;
            } else {
                let mut entry = ShopEntry::default_for(&record.id);
                entry.version = record.version.clone();
                entry.installed_version = Some(record.version);
                entry.enabled = record.enabled;
                entry.configured = record.configured;
                entry.error = record.error;
                self.entries.push(entry);
            }
        }
        self.sort();
    }

    /// Merge a verified catalog into the view. Installed state is preserved.
    pub fn apply_catalog(&mut self, catalog: Vec<ShopEntry>) {
        let mut merged: Vec<ShopEntry> = Vec::with_capacity(catalog.len() + self.entries.len());
        for mut incoming in catalog {
            incoming.available = true;
            if let Some(existing) = self.entries.iter().find(|e| e.id == incoming.id) {
                incoming.installed_version = existing.installed_version.clone();
                incoming.enabled = existing.enabled;
                incoming.configured = existing.configured;
                incoming.error = existing.error.clone();
            }
            merged.push(incoming);
        }
        for existing in &self.entries {
            if existing.is_installed() && !merged.iter().any(|e| e.id == existing.id) {
                let mut kept = existing.clone();
                kept.available = false;
                merged.push(kept);
            }
        }
        self.entries = merged;
        self.catalog_online = true;
        self.catalog_error = None;
        self.sort();
    }

    /// Keep installed entries usable when the catalog cannot be reached.
    pub fn mark_catalog_offline(&mut self, reason: impl Into<String>) {
        self.catalog_online = false;
        self.catalog_error = Some(reason.into());
        for entry in &mut self.entries {
            entry.available = false;
        }
    }

    pub fn set_error(&mut self, id: &str, message: impl Into<String>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.error = Some(message.into());
        }
    }

    pub fn clear_error(&mut self, id: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.error = None;
        }
    }

    /// Reflect whether the extension's required settings are present.
    pub fn set_configured(&mut self, id: &str, configured: bool) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.configured = configured;
        }
    }

    pub fn installed_entries(&self) -> Vec<&ShopEntry> {
        self.entries.iter().filter(|e| e.is_installed()).collect()
    }

    pub fn available_entries(&self) -> Vec<&ShopEntry> {
        self.entries
            .iter()
            .filter(|e| !e.is_installed() && e.available)
            .collect()
    }

    pub fn extension_errors(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| {
                e.error
                    .as_ref()
                    .map(|message| format!("{}: {}", e.name, message))
            })
            .collect()
    }

    pub fn entry(&self, id: &str) -> Option<&ShopEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Validate an action and turn it into a store request. The first install
    /// requires an explicit trust confirmation.
    pub fn request(
        &mut self,
        id: &str,
        action: ShopAction,
        confirmed: bool,
    ) -> Result<ShopRequest, ShopError> {
        if self.pending.is_some() {
            return Err(ShopError::Busy);
        }
        let entry = self
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| ShopError::Unknown(id.to_string()))?;
        match action {
            ShopAction::Install => {
                if entry.is_installed() {
                    return Err(ShopError::AlreadyInstalled(id.to_string()));
                }
                if !entry.available {
                    return Err(ShopError::NotAvailable(id.to_string()));
                }
                if !confirmed {
                    return Err(ShopError::ConfirmationRequired(id.to_string()));
                }
            }
            ShopAction::Update => {
                if !entry.is_installed() {
                    return Err(ShopError::NotInstalled(id.to_string()));
                }
                if entry.status() != ExtensionStatus::UpdateAvailable {
                    return Err(ShopError::NoUpdate(id.to_string()));
                }
            }
            ShopAction::Remove => {
                if !entry.is_installed() {
                    return Err(ShopError::NotInstalled(id.to_string()));
                }
            }
            ShopAction::Enable | ShopAction::Disable => {
                if !entry.is_installed() {
                    return Err(ShopError::NotInstalled(id.to_string()));
                }
            }
        }
        let request = ShopRequest {
            id: id.to_string(),
            action,
            version: entry.version.clone(),
        };
        self.pending = Some(ShopPending {
            id: id.to_string(),
            action,
        });
        Ok(request)
    }

    /// Apply the store result. Errors are shown on the entry and never turn a
    /// failed action into an apparent success.
    pub fn complete(
        &mut self,
        id: &str,
        action: ShopAction,
        result: Result<(), String>,
    ) -> Result<(), ShopError> {
        if let Some(pending) = &self.pending
            && (pending.id != id || pending.action != action)
        {
            return Err(ShopError::Busy);
        }
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| ShopError::Unknown(id.to_string()))?;
        self.pending = None;
        match result {
            Ok(()) => {
                entry.error = None;
                match action {
                    ShopAction::Install | ShopAction::Update => {
                        entry.installed_version = Some(entry.version.clone());
                        entry.enabled = true;
                    }
                    ShopAction::Remove => {
                        entry.installed_version = None;
                        entry.enabled = false;
                        entry.configured = false;
                    }
                    ShopAction::Enable => entry.enabled = true,
                    ShopAction::Disable => entry.enabled = false,
                }
            }
            Err(message) => entry.error = Some(message),
        }
        Ok(())
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
    }
}

impl ShopEntry {
    fn default_for(id: &str) -> Self {
        Self {
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
}
