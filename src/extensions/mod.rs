//! Wave 6 extension catalog, signature verification, and package store.
//!
//! This module covers the catalog and installation path only. The Rhai script
//! sandbox and the shop UI are separate Wave 6 tasks.
//!
//! Trust model: the catalog index is signed with Minisign and the configured
//! public keys are the only trust anchor. A valid signature proves provenance,
//! not safety, so every package is still extracted with hard limits.

pub mod catalog;
pub mod client;
pub mod package;
pub mod signature;
pub mod store;

pub use catalog::{
    API_VERSION, CATALOG_SCHEMA_VERSION, CatalogEntry, CatalogError, CatalogIndex, CatalogPolicy,
    ConfigField, ConfigFieldType, ExtensionKind, ExtensionManifest, Permissions,
};
pub use client::{
    CatalogCache, CatalogClient, CatalogFetcher, FetchError, HttpsFetcher, RefreshOutcome,
};
pub use package::{
    MAX_DECOMPRESSED_BYTES, PackageError, extract_package, extract_package_with_limit,
    load_manifest, sha256_hex, verify_package,
};
pub use signature::{SignatureError, SignatureStatus, verify_signature, verify_unsigned};
pub use store::{
    DEFAULT_STORE_ROOT, ExtensionStore, InstallOutcome, InstalledExtension, InstalledIndex,
    STORE_SCHEMA_VERSION, StoreError,
};
