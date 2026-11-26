//! Wave 6 community extension shop.
//!
//! The module has three layers:
//!
//! - [`catalog`], [`signature`], [`client`], [`package`], and [`store`] fetch,
//!   verify, extract, and install signed `.hkext` packages;
//! - [`host`], [`http`], [`manifest`], [`output`], and [`runner`] execute the
//!   sandboxed Rhai data scripts and validate their declarative output;
//! - [`shop`] is the UI-facing state machine for the fifth page.
//!
//! Trust model: the catalog index is signed with Minisign and the configured
//! public keys are the only trust anchor. A valid signature proves provenance,
//! not safety, so every package is still extracted and executed with hard
//! limits.

pub mod catalog;
pub mod client;
pub mod host;
pub mod http;
pub mod install;
pub mod manager;
pub mod manifest;
pub mod output;
pub mod package;
pub mod runner;
pub mod shop;
pub mod signature;
pub mod store;

pub use catalog::{
    API_VERSION, CATALOG_SCHEMA_VERSION, CatalogEntry, CatalogError, CatalogIndex, CatalogPolicy,
    ConfigField as CatalogConfigField, ConfigFieldType, ExtensionKind, ExtensionManifest,
    Permissions as CatalogPermissions,
};
pub use client::{
    CatalogCache, CatalogClient, CatalogFetcher, FetchError, HttpsFetcher, RefreshOutcome,
};
pub use host::{
    Clock, EntitySnapshot, ExtensionError, ExtensionHost, HostConfig, Limits, SystemClock,
};
pub use http::{HttpError, HttpFetcher, HttpPolicy, ReqwestFetcher};
pub use install::{
    DownloadError, download_and_verify, download_and_verify_package, download_package,
    download_package_and_verify, download_package_to_temp, download_package_to_temp_dir,
    download_package_verified, download_package_verified_checksum, download_to_temp,
    download_to_temp_dir, download_verified, download_verified_package, extract_downloaded_package,
    extract_downloaded_package_with_limit, extract_package_file, extract_package_file_with_limit,
    extract_package_from_file, extract_package_from_path, fetch_package_to_temp,
    fetch_package_to_temp_dir, sha256_file, verify_downloaded_package, verify_package_download,
    verify_package_file,
};
pub use manager::{ExtensionHealth, ExtensionManager, ManagerError, ManagerOptions};
pub use manifest::{Manifest, ManifestError, OutputKind};
pub use output::{
    DepartureItem, DeparturesView, ExtensionOutput, KeyValueItem, KeyValueView, ListView,
    MAX_OUTPUT_ITEMS, OutputError,
};
pub use package::{
    MAX_DECOMPRESSED_BYTES, PackageError, extract_package, extract_package_with_limit,
    load_manifest, sha256_hex, verify_package,
};
pub use runner::{ExtensionRunner, ExtensionStatus as HostExtensionStatus};
pub use shop::{
    ConfigField, ExtensionPermissions, ExtensionStatus, InstalledExtension, ShopAction, ShopEntry,
    ShopError, ShopPending, ShopRequest, ShopView,
};
pub use signature::{SignatureError, SignatureStatus, verify_signature, verify_unsigned};
pub use store::{
    DEFAULT_STORE_ROOT, ExtensionStore, InstallOutcome, InstalledExtension as StoredExtension,
    InstalledIndex, STORE_SCHEMA_VERSION, StoreError,
};
