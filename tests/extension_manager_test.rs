use ha_kiosk::extensions::{
    CatalogFetcher, ExtensionManager, FetchError, ManagerError, ManagerOptions, sha256_hex,
};
use semver::Version;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use url::Url;

const ID: &str = "org.example.bus-departures";
const CATALOG_URL: &str = "https://catalog.example.com/index.json";
const MAX_BYTES: u64 = 2 * 1024 * 1024;

fn package_url(version: &str) -> String {
    format!("https://catalog.example.com/org.example.bus-departures-{version}.hkext")
}

fn build_package(version: &str) -> Vec<u8> {
    let manifest = format!(
        r#"
id = "{ID}"
name = "Bus Departures"
version = "{version}"
api_version = 1
min_app_version = "0.6.0"
entry = "main.rhai"
provides = "departures"
author = "Example Author"
license = "MIT"
description = "Shows upcoming bus departures."

[permissions]
http_hosts = []
ha_entities = []
"#
    );
    let mut tar = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar);
        for (path, bytes) in [
            ("manifest.toml", manifest.as_bytes()),
            (
                "main.rhai",
                b"fn refresh(config) { #{ kind: \"departures\", items: [] } }",
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, bytes).unwrap();
        }
        builder.finish().unwrap();
    }
    use flate2::Compression;
    use flate2::write::GzEncoder;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar).unwrap();
    encoder.finish().unwrap()
}

fn index_json(version: &str, package: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "generated_at": "2026-11-25T00:00:00Z",
        "extensions": [{
            "id": ID,
            "name": "Bus Departures",
            "version": version,
            "api_version": 1,
            "min_app_version": "0.6.0",
            "description": "Shows upcoming bus departures.",
            "author": "Example Author",
            "license": "MIT",
            "provides": "departures",
            "permissions": {"http_hosts": [], "ha_entities": []},
            "config_schema": [],
            "package_url": package_url(version),
            "sha256": sha256_hex(package),
            "size_bytes": package.len(),
        }],
    }))
    .unwrap()
}

#[derive(Clone, Default)]
struct FakeFetcher {
    responses: Rc<RefCell<HashMap<String, Vec<u8>>>>,
}

impl FakeFetcher {
    fn insert(&self, url: &str, body: impl Into<Vec<u8>>) {
        self.responses
            .borrow_mut()
            .insert(url.to_string(), body.into());
    }

    fn set_catalog(&self, version: &str, package: &[u8]) {
        self.insert(CATALOG_URL, index_json(version, package));
        self.insert(&package_url(version), package.to_vec());
    }
}

impl CatalogFetcher for FakeFetcher {
    fn get(&mut self, url: &Url, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        let body = self
            .responses
            .borrow()
            .get(url.as_str())
            .cloned()
            .ok_or_else(|| FetchError::Status {
                status: 404,
                url: url.to_string(),
            })?;
        if body.len() as u64 > max_bytes {
            return Err(FetchError::TooLarge {
                url: url.to_string(),
                limit: max_bytes,
            });
        }
        Ok(body)
    }
}

fn options(root: PathBuf) -> ManagerOptions {
    let mut setting = toml::map::Map::new();
    setting.insert(
        "stop_id".to_string(),
        toml::Value::String("REPLACE_ME".to_string()),
    );
    let mut settings = BTreeMap::new();
    settings.insert(ID.to_string(), toml::Value::Table(setting));

    ManagerOptions {
        app_version: Version::parse("0.6.0").unwrap(),
        catalog_url: Some(Url::parse(CATALOG_URL).unwrap()),
        public_keys: Vec::new(),
        allow_unsigned: true,
        dev_mode: true,
        max_catalog_bytes: MAX_BYTES,
        max_package_bytes: MAX_BYTES,
        store_path: root.join("extensions"),
        secret_dir: root.join("secrets"),
        settings,
    }
}

#[test]
fn manager_installs_updates_toggles_and_removes_extensions() {
    let dir = tempfile::tempdir().unwrap();
    let package_v1 = build_package("1.0.0");
    let package_v2 = build_package("2.0.0");
    let catalog = FakeFetcher::default();
    let packages = FakeFetcher::default();
    catalog.set_catalog("1.0.0", &package_v1);
    packages.set_catalog("1.0.0", &package_v1);
    packages.set_catalog("2.0.0", &package_v2);

    let mut manager = ExtensionManager::new(
        options(dir.path().to_path_buf()),
        catalog.clone(),
        packages.clone(),
    )
    .unwrap();
    manager.refresh_catalog().unwrap();
    manager.install(ID).unwrap();
    assert_eq!(
        manager.store().get(ID).unwrap().version,
        Version::parse("1.0.0").unwrap()
    );
    assert!(manager.store().get(ID).unwrap().enabled);

    catalog.set_catalog("2.0.0", &package_v2);
    manager.refresh_catalog().unwrap();
    manager.update(ID).unwrap();
    assert_eq!(
        manager.store().get(ID).unwrap().version,
        Version::parse("2.0.0").unwrap()
    );

    manager.set_enabled(ID, false).unwrap();
    assert!(!manager.store().get(ID).unwrap().enabled);
    manager.set_enabled(ID, true).unwrap();
    assert!(manager.store().get(ID).unwrap().enabled);

    manager.remove(ID).unwrap();
    assert!(manager.store().get(ID).is_none());
    assert!(
        manager.shop_entries()[0].configured,
        "removal must retain the existing settings status"
    );
}

#[test]
fn manager_update_rejects_an_older_catalog_version() {
    let dir = tempfile::tempdir().unwrap();
    let package_v1 = build_package("1.0.0");
    let package_v2 = build_package("2.0.0");
    let catalog = FakeFetcher::default();
    let packages = FakeFetcher::default();
    catalog.set_catalog("2.0.0", &package_v2);
    packages.set_catalog("1.0.0", &package_v1);
    packages.set_catalog("2.0.0", &package_v2);

    let mut manager = ExtensionManager::new(
        options(dir.path().to_path_buf()),
        catalog.clone(),
        packages.clone(),
    )
    .unwrap();
    manager.refresh_catalog().unwrap();
    manager.install(ID).unwrap();

    catalog.set_catalog("1.0.0", &package_v1);
    manager.refresh_catalog().unwrap();
    assert!(matches!(manager.update(ID), Err(ManagerError::NoUpdate(_))));
    assert_eq!(
        manager.store().get(ID).unwrap().version,
        Version::parse("2.0.0").unwrap()
    );
    assert!(
        !manager
            .store()
            .extension_dir(ID, &Version::parse("1.0.0").unwrap())
            .join("manifest.toml")
            .is_file()
    );
}
