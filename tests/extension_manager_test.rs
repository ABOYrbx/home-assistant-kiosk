//! Integration tests for running installed extensions through the manager.

use std::collections::BTreeMap;
use std::io::Write;

use ha_kiosk::extensions::{
    CatalogEntry, CatalogFetcher, CatalogPermissions, ExtensionError, ExtensionKind,
    ExtensionManager, ExtensionStore, FetchError, Limits, ManagerError, ManagerOptions, sha256_hex,
};
use semver::Version;
use url::Url;

const ID: &str = "org.example.limit-test";
const PACKAGE_URL: &str = "https://catalog.example.com/limit-test-1.0.0.hkext";
const MAX_PACKAGE_BYTES: u64 = 5 * 1024 * 1024;

struct NoFetch;

impl CatalogFetcher for NoFetch {
    fn get(&mut self, _url: &Url, _max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        panic!("manager.run must not fetch the catalog")
    }
}

fn append(builder: &mut tar::Builder<&mut Vec<u8>>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, data).unwrap();
}

fn package() -> Vec<u8> {
    let manifest = format!(
        r#"
id = "{ID}"
name = "Limit Test"
version = "1.0.0"
api_version = 1
min_app_version = "0.1.0"
entry = "main.rhai"
provides = "key_value"
author = "Example Author"
license = "MIT"
description = "Exercises the configured operation limit."

[permissions]
http_hosts = []
ha_entities = []
"#
    );
    let script = r#"fn refresh(config) {
        let i = 0;
        while i < 1000 { i += 1; }
        #{ kind: "key_value", items: [] }
    }"#;
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(&mut builder, "manifest.toml", manifest.as_bytes());
        append(&mut builder, "main.rhai", script.as_bytes());
        builder.finish().unwrap();
    }
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    encoder.finish().unwrap()
}

fn entry(package: &[u8]) -> CatalogEntry {
    CatalogEntry {
        id: ID.into(),
        name: "Limit Test".into(),
        version: Version::parse("1.0.0").unwrap(),
        api_version: 1,
        min_app_version: Version::parse("0.1.0").unwrap(),
        description: "Exercises the configured operation limit.".into(),
        author: "Example Author".into(),
        license: "MIT".into(),
        homepage: None,
        provides: ExtensionKind::KeyValue,
        permissions: CatalogPermissions::default(),
        config_schema: Vec::new(),
        package_url: Url::parse(PACKAGE_URL).unwrap(),
        sha256: sha256_hex(package),
        size_bytes: package.len() as u64,
    }
}

#[test]
fn configured_operation_limit_reaches_installed_script() {
    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("extensions");
    let package = package();
    let entry = entry(&package);
    let mut store = ExtensionStore::open(&store_path).unwrap();
    store
        .install(&entry, &package, PACKAGE_URL, MAX_PACKAGE_BYTES)
        .unwrap();
    drop(store);

    let options = ManagerOptions {
        app_version: Version::parse("0.1.0").unwrap(),
        catalog_url: None,
        public_keys: Vec::new(),
        allow_unsigned: false,
        dev_mode: false,
        max_catalog_bytes: 2 * 1024 * 1024,
        max_package_bytes: MAX_PACKAGE_BYTES,
        store_path,
        secret_dir: dir.path().join("secrets"),
        settings: BTreeMap::new(),
        limits: Limits {
            max_operations: 100,
            ..Limits::default()
        },
    };
    let mut manager = ExtensionManager::new(options, NoFetch, NoFetch).unwrap();
    let error = manager.run(ID, BTreeMap::new()).unwrap_err();
    assert!(
        matches!(error, ManagerError::Host(ExtensionError::OperationLimit)),
        "expected the configured operation limit, got {error:?}"
    );
}
