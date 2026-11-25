//! On-disk extension store and `installed.json`.
//!
//! Layout: `<root>/<id>/<version>/`. Installs are atomic: a package is
//! extracted into a staging directory and renamed into place. One previous
//! version is kept for rollback. `installed.json` is mode `0600`; the store
//! root and extension directories are mode `0700`.

use crate::extensions::catalog::{CatalogEntry, ExtensionManifest, is_reverse_dns_id};
use crate::extensions::package::{self, PackageError};
use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Schema version of `installed.json`.
pub const STORE_SCHEMA_VERSION: u32 = 1;
/// Default store root for the kiosk service.
pub const DEFAULT_STORE_ROOT: &str = "/var/lib/ha-kiosk/extensions";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("package error: {0}")]
    Package(#[from] PackageError),
    #[error("invalid extension id: {0}")]
    InvalidId(String),
    #[error("extension {0} is not installed")]
    NotInstalled(String),
    #[error("extension {id} {version} is already installed")]
    AlreadyInstalled { id: String, version: Version },
    #[error("package manifest does not match the signed catalog entry for {0}")]
    ManifestMismatch(String),
    #[error("no previous version is available to roll back {0}")]
    NoPreviousVersion(String),
    #[error("installed.json is corrupt: {0}")]
    CorruptIndex(String),
    #[error("installed.json schema {0} is not supported")]
    UnsupportedSchema(u32),
}

/// One installed extension, mirrored in `installed.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledExtension {
    pub id: String,
    pub version: Version,
    pub enabled: bool,
    pub manifest_sha256: String,
    pub source_catalog: String,
    pub installed_at: DateTime<Utc>,
}

/// The `installed.json` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledIndex {
    pub schema_version: u32,
    pub extensions: Vec<InstalledExtension>,
}

impl Default for InstalledIndex {
    fn default() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            extensions: Vec::new(),
        }
    }
}

/// Result of an install, update, or rollback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub id: String,
    pub version: Version,
    pub previous: Option<Version>,
}

/// Extension store rooted at a writable directory.
pub struct ExtensionStore {
    root: PathBuf,
    index_path: PathBuf,
    index: InstalledIndex,
}

impl ExtensionStore {
    /// Open or create a store. Existing files get their permissions tightened.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        create_dir(&root)?;
        let index_path = root.join("installed.json");
        let index = if index_path.exists() {
            set_file_mode(&index_path, 0o600)?;
            load_index(&index_path)?
        } else {
            InstalledIndex::default()
        };
        Ok(Self {
            root,
            index_path,
            index,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn installed(&self) -> &[InstalledExtension] {
        &self.index.extensions
    }

    pub fn get(&self, id: &str) -> Option<&InstalledExtension> {
        self.index.extensions.iter().find(|entry| entry.id == id)
    }

    pub fn extension_dir(&self, id: &str, version: &Version) -> PathBuf {
        self.root.join(id).join(version.to_string())
    }

    /// Install or update from a verified catalog entry.
    pub fn install(
        &mut self,
        entry: &CatalogEntry,
        package: &[u8],
        source_catalog: &str,
        max_package_bytes: u64,
    ) -> Result<InstallOutcome, StoreError> {
        check_id(&entry.id)?;
        if let Some(record) = self.get(&entry.id)
            && record.version == entry.version
        {
            return Err(StoreError::AlreadyInstalled {
                id: entry.id.clone(),
                version: entry.version.clone(),
            });
        }

        package::verify_package(package, &entry.sha256, entry.size_bytes, max_package_bytes)?;

        let id_dir = self.root.join(&entry.id);
        create_dir(&id_dir)?;

        let staging = tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&self.root)?;
        package::extract_package(package, staging.path())?;
        let manifest = package::load_manifest(staging.path())?;
        if !manifest_matches_entry(&manifest, entry) {
            return Err(StoreError::ManifestMismatch(entry.id.clone()));
        }
        if !crate::extensions::catalog::is_safe_relative_path(&manifest.entry)
            || !manifest.entry.ends_with(".rhai")
            || !staging.path().join(&manifest.entry).is_file()
        {
            return Err(StoreError::Package(PackageError::Invalid(
                "package entry script is missing or unsafe".into(),
            )));
        }
        let manifest_sha256 = package::sha256_hex(&package::manifest_bytes(staging.path())?);

        let target = id_dir.join(entry.version.to_string());
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        let staged_path = staging.keep();
        fs::rename(&staged_path, &target)?;

        let previous = self.get(&entry.id).map(|record| record.version.clone());
        self.prune_versions(&entry.id, &entry.version, previous.as_ref())?;

        let now = Utc::now();
        match self.index.extensions.iter_mut().find(|e| e.id == entry.id) {
            Some(record) => {
                record.version = entry.version.clone();
                record.manifest_sha256 = manifest_sha256;
                record.source_catalog = source_catalog.to_string();
                record.installed_at = now;
            }
            None => self.index.extensions.push(InstalledExtension {
                id: entry.id.clone(),
                version: entry.version.clone(),
                enabled: true,
                manifest_sha256,
                source_catalog: source_catalog.to_string(),
                installed_at: now,
            }),
        }
        self.index.extensions.sort_by(|a, b| a.id.cmp(&b.id));
        self.save()?;

        Ok(InstallOutcome {
            id: entry.id.clone(),
            version: entry.version.clone(),
            previous,
        })
    }

    /// Switch back to the retained previous version and drop the newer files.
    pub fn rollback(&mut self, id: &str) -> Result<InstallOutcome, StoreError> {
        check_id(id)?;
        let current = self
            .get(id)
            .ok_or_else(|| StoreError::NotInstalled(id.to_string()))?
            .version
            .clone();
        let previous = self
            .previous_version(id)
            .ok_or_else(|| StoreError::NoPreviousVersion(id.to_string()))?;

        // Recompute the manifest hash from the retained version so the record
        // stays consistent with the files on disk after a rollback.
        let manifest_hash = package::manifest_bytes(&self.extension_dir(id, &previous))
            .map(|bytes| package::sha256_hex(&bytes))
            .ok();

        let record = self
            .index
            .extensions
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| StoreError::NotInstalled(id.to_string()))?;
        record.version = previous.clone();
        record.installed_at = Utc::now();
        if let Some(hash) = manifest_hash {
            record.manifest_sha256 = hash;
        }
        self.save()?;

        let current_dir = self.extension_dir(id, &current);
        if current_dir.exists() {
            fs::remove_dir_all(&current_dir)?;
        }

        Ok(InstallOutcome {
            id: id.to_string(),
            version: previous.clone(),
            previous: Some(current),
        })
    }

    /// Remove an extension and all of its installed versions.
    ///
    /// Settings live in the kiosk configuration; the caller removes
    /// `[extensions.settings."<id>"]` when `purge_settings` is true.
    pub fn remove(&mut self, id: &str, purge_settings: bool) -> Result<(), StoreError> {
        check_id(id)?;
        if self.get(id).is_none() {
            return Err(StoreError::NotInstalled(id.to_string()));
        }
        let id_dir = self.root.join(id);
        if id_dir.exists() {
            fs::remove_dir_all(&id_dir)?;
        }
        self.index.extensions.retain(|e| e.id != id);
        self.save()?;
        if purge_settings {
            tracing::info!("removed settings for extension {id}");
        }
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), StoreError> {
        check_id(id)?;
        let record = self
            .index
            .extensions
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| StoreError::NotInstalled(id.to_string()))?;
        record.enabled = enabled;
        self.save()
    }

    /// Highest installed version other than the active one.
    pub fn previous_version(&self, id: &str) -> Option<Version> {
        let current = self.get(id).map(|e| e.version.clone());
        let id_dir = self.root.join(id);
        let entries = fs::read_dir(&id_dir).ok()?;
        let mut versions: Vec<Version> = entries
            .flatten()
            .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|entry| Version::parse(&entry.file_name().to_string_lossy()).ok())
            .filter(|version| current.as_ref() != Some(version))
            .collect();
        versions.sort();
        versions.pop()
    }

    fn prune_versions(
        &self,
        id: &str,
        current: &Version,
        previous: Option<&Version>,
    ) -> Result<(), StoreError> {
        let id_dir = self.root.join(id);
        let Ok(entries) = fs::read_dir(&id_dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let Ok(version) = Version::parse(&entry.file_name().to_string_lossy()) else {
                continue;
            };
            if &version == current || previous == Some(&version) {
                continue;
            }
            fs::remove_dir_all(entry.path())?;
        }
        Ok(())
    }

    fn save(&self) -> Result<(), StoreError> {
        let data = serde_json::to_vec_pretty(&self.index)
            .map_err(|e| StoreError::CorruptIndex(e.to_string()))?;
        let tmp = self.root.join(".installed.json.tmp");
        {
            let mut options = OpenOptions::new();
            options.create(true).write(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&tmp)?;
            file.write_all(&data)?;
            file.sync_all()?;
        }
        set_file_mode(&tmp, 0o600)?;
        fs::rename(&tmp, &self.index_path)?;
        sync_dir(&self.root);
        Ok(())
    }
}

fn load_index(path: &Path) -> Result<InstalledIndex, StoreError> {
    let bytes = fs::read(path)?;
    let index: InstalledIndex =
        serde_json::from_slice(&bytes).map_err(|e| StoreError::CorruptIndex(e.to_string()))?;
    if index.schema_version != STORE_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(index.schema_version));
    }
    for entry in &index.extensions {
        check_id(&entry.id)?;
    }
    Ok(index)
}

fn check_id(id: &str) -> Result<(), StoreError> {
    if !is_reverse_dns_id(id) {
        return Err(StoreError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn manifest_matches_entry(manifest: &ExtensionManifest, entry: &CatalogEntry) -> bool {
    manifest.id == entry.id
        && manifest.name == entry.name
        && manifest.version == entry.version
        && manifest.api_version == entry.api_version
        && manifest.min_app_version == entry.min_app_version
        && manifest.provides == entry.provides
        && manifest.permissions == entry.permissions
        && manifest.config_schema == entry.config_schema
        && manifest.author == entry.author
        && manifest.license == entry.license
        && manifest.homepage == entry.homepage
        && manifest.description == entry.description
}

fn create_dir(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path)?;
    set_dir_mode(path, 0o700)?;
    Ok(())
}

fn set_file_mode(path: &Path, mode: u32) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

fn set_dir_mode(path: &Path, mode: u32) -> Result<(), StoreError> {
    set_file_mode(path, mode)
}

fn sync_dir(path: &Path) {
    #[cfg(unix)]
    {
        if let Ok(dir) = fs::File::open(path) {
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}
