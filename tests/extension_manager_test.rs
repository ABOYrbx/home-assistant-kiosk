use ha_kiosk::extensions::{
    CatalogEntry, CatalogFetcher, CatalogIndex, CatalogPolicy, EntitySnapshot, ExtensionManager,
    ExtensionOutput, ExtensionStore, FetchError, ManagerOptions, sha256_hex,
};
use semver::Version;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;
use url::Url;

const ID: &str = "org.example.failure-test";
const VERSION: &str = "1.0.0";

struct UnusedFetcher;

impl CatalogFetcher for UnusedFetcher {
    fn get(&mut self, _url: &Url, _max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::Transport("unexpected fetch".to_string()))
    }
}

fn package(script: &str) -> (CatalogEntry, Vec<u8>) {
    let manifest = format!(
        r#"
id = "{ID}"
name = "Failure Test"
version = "{VERSION}"
api_version = 1
min_app_version = "0.1.0"
entry = "main.rhai"
provides = "key_value"
author = "Example Author"
license = "MIT"
description = "Tests extension failure recovery."

[permissions]
http_hosts = []
ha_entities = []
"#
    );
    let mut archive = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut archive);
        for (path, contents) in [
            ("manifest.toml", manifest.as_bytes()),
            ("main.rhai", script.as_bytes()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, contents).unwrap();
        }
        builder.finish().unwrap();
    }
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&archive).unwrap();
    let package = encoder.finish().unwrap();
    let index = serde_json::json!({
        "schema_version": 1,
        "generated_at": "2026-11-25T00:00:00Z",
        "extensions": [{
            "id": ID,
            "name": "Failure Test",
            "version": VERSION,
            "api_version": 1,
            "min_app_version": "0.1.0",
            "description": "Tests extension failure recovery.",
            "author": "Example Author",
            "license": "MIT",
            "provides": "key_value",
            "permissions": {"http_hosts": [], "ha_entities": []},
            "config_schema": [],
            "package_url": "https://catalog.example.com/failure-test.hkext",
            "sha256": sha256_hex(&package),
            "size_bytes": package.len(),
        }],
    });
    let policy = CatalogPolicy {
        app_version: Version::parse("0.1.0").unwrap(),
        public_keys: Vec::new(),
        allow_unsigned: false,
        dev_mode: false,
        max_catalog_bytes: 1024 * 1024,
        max_package_bytes: 1024 * 1024,
    };
    let entry = CatalogIndex::parse(&serde_json::to_vec(&index).unwrap(), &policy)
        .unwrap()
        .extensions
        .remove(0);
    (entry, package)
}

fn installed_manager(script: &str) -> (TempDir, ExtensionManager<UnusedFetcher, UnusedFetcher>) {
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("extensions");
    let (entry, package) = package(script);
    let mut store = ExtensionStore::open(&store_path).unwrap();
    store
        .install(
            &entry,
            &package,
            "https://catalog.example.com/index.json",
            1024 * 1024,
        )
        .unwrap();
    drop(store);

    let options = ManagerOptions {
        app_version: Version::parse("0.1.0").unwrap(),
        catalog_url: None,
        public_keys: Vec::new(),
        allow_unsigned: false,
        dev_mode: false,
        max_catalog_bytes: 1024 * 1024,
        max_package_bytes: 1024 * 1024,
        store_path,
        secret_dir: temp.path().join("secrets"),
        settings: BTreeMap::new(),
    };
    let manager = ExtensionManager::new(options, UnusedFetcher, UnusedFetcher).unwrap();
    (temp, manager)
}

fn extension_dir(manager: &ExtensionManager<UnusedFetcher, UnusedFetcher>) -> PathBuf {
    manager
        .store()
        .extension_dir(ID, &Version::parse(VERSION).unwrap())
}

fn script_path(manager: &ExtensionManager<UnusedFetcher, UnusedFetcher>) -> PathBuf {
    extension_dir(manager).join("main.rhai")
}

fn run(
    manager: &mut ExtensionManager<UnusedFetcher, UnusedFetcher>,
) -> Result<ExtensionOutput, ha_kiosk::extensions::ManagerError> {
    manager.run(ID, BTreeMap::<String, EntitySnapshot>::new())
}

#[test]
fn preparation_failures_keep_last_good_data_and_degrade_after_three_runs() {
    let (_temp, mut manager) = installed_manager(
        r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
    );
    let good = run(&mut manager).unwrap();
    fs::remove_file(script_path(&manager)).unwrap();

    for expected_failures in 1..=3 {
        assert!(run(&mut manager).is_err());
        let health = manager.health(ID).expect("run failure updates health");
        assert_eq!(health.last_good.as_ref(), Some(&good));
        assert_eq!(health.consecutive_failures, expected_failures);
        assert_eq!(health.degraded, expected_failures == 3);
        assert!(health.last_error.is_some());
    }
}

#[test]
fn manifest_and_secret_failures_update_health_without_replacing_last_good_data() {
    for corrupt_manifest in [true, false] {
        let (temp, mut manager) = installed_manager(
            r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
        );
        let good = run(&mut manager).unwrap();
        if corrupt_manifest {
            fs::write(
                extension_dir(&manager).join("manifest.toml"),
                "not valid = [",
            )
            .unwrap();
        } else {
            let secret_dir = temp.path().join("secrets");
            fs::create_dir_all(&secret_dir).unwrap();
            fs::write(
                secret_dir.join(format!("{ID}.secret.toml")),
                "not valid = [",
            )
            .unwrap();
        }

        assert!(run(&mut manager).is_err());
        let health = manager.health(ID).unwrap();
        assert_eq!(health.last_good.as_ref(), Some(&good));
        assert_eq!(health.consecutive_failures, 1);
        assert!(!health.degraded);
        assert!(health.last_error.is_some());
    }
}

#[test]
fn runtime_failures_keep_exact_output_and_expose_degraded_until_recovery() {
    let good_script = r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#;
    let (_temp, mut manager) = installed_manager(good_script);
    let good = run(&mut manager).unwrap();
    fs::write(
        script_path(&manager),
        r#"fn refresh(config) { throw "upstream unavailable"; }"#,
    )
    .unwrap();

    for expected_failures in 1..=2 {
        assert!(run(&mut manager).is_err());
        let health = manager.health(ID).unwrap();
        assert_eq!(health.last_good.as_ref(), Some(&good));
        assert_eq!(health.consecutive_failures, expected_failures);
        assert!(!health.degraded);
        assert_eq!(manager.shop_entries()[0].status().label(), "Error");
    }

    assert!(run(&mut manager).is_err());
    let health = manager.health(ID).unwrap();
    assert_eq!(health.last_good.as_ref(), Some(&good));
    assert_eq!(health.consecutive_failures, 3);
    assert!(health.degraded);
    assert_eq!(manager.shop_entries()[0].status().label(), "Degraded");
    let errors = manager.errors();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains(ID));
    assert!(errors[0].contains("(degraded)"));
    assert!(errors[0].contains("upstream unavailable"));

    fs::write(script_path(&manager), good_script).unwrap();
    let recovered = run(&mut manager).unwrap();
    assert_eq!(recovered, good);
    let health = manager.health(ID).unwrap();
    assert!(health.last_error.is_none());
    assert_eq!(health.consecutive_failures, 0);
    assert!(!health.degraded);
    assert_eq!(manager.shop_entries()[0].status().label(), "Installed");
    assert!(manager.errors().is_empty());
}
