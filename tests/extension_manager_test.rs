use ha_kiosk::extensions::{
    CatalogEntry, CatalogFetcher, CatalogIndex, CatalogPolicy, EntitySnapshot, ExtensionManager,
    ExtensionOutput, ExtensionStore, FetchError, Limits, ManagerOptions, sha256_hex,
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

[[config_schema]]
key = "api_key"
label = "API Key"
type = "secret"
required = false
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
            "config_schema": [{
                "key": "api_key",
                "label": "API Key",
                "type": "secret",
                "required": false
            }],
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
    installed_manager_with_settings(script, BTreeMap::new())
}

fn installed_manager_with_settings(
    script: &str,
    settings: BTreeMap<String, toml::Value>,
) -> (TempDir, ExtensionManager<UnusedFetcher, UnusedFetcher>) {
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
        limits: Limits::default(),
        store_path,
        secret_dir: temp.path().join("secrets"),
        settings,
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

fn assert_errors_do_not_contain(
    manager: &mut ExtensionManager<UnusedFetcher, UnusedFetcher>,
    marker: &str,
) {
    let health_error = manager
        .health(ID)
        .and_then(|health| health.last_error.clone());
    let mut messages = manager.errors();
    messages.extend(health_error);
    messages.extend(
        manager
            .shop_entries()
            .first()
            .and_then(|entry| entry.error.clone()),
    );
    messages.extend(
        manager
            .installed_entries()
            .first()
            .and_then(|entry| entry.error.clone()),
    );
    for message in messages {
        assert!(
            !message.contains(marker),
            "secret leaked in error: {message}"
        );
    }
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
fn malformed_secret_errors_never_expose_secret_values_in_health_or_ui() {
    let (temp, mut manager) = installed_manager(
        r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
    );
    let good = run(&mut manager).unwrap();
    let marker = "secret-marker-7f3c2a";
    let secret_dir = temp.path().join("secrets");
    fs::create_dir_all(&secret_dir).unwrap();
    fs::write(
        secret_dir.join(format!("{ID}.secret.toml")),
        format!("api_key = \"{marker}\" trailing\n"),
    )
    .unwrap();

    let error = run(&mut manager).unwrap_err().to_string();
    let health = manager.health(ID).unwrap();
    assert_eq!(health.last_good.as_ref(), Some(&good));
    assert_eq!(health.consecutive_failures, 1);
    assert!(
        !error.contains(marker),
        "returned error leaked a secret: {error}"
    );
    assert_errors_do_not_contain(&mut manager, marker);
}

#[test]
fn output_validation_errors_are_redacted_before_health_and_ui_exposure() {
    let (temp, mut manager) = installed_manager(
        r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
    );
    let good = run(&mut manager).unwrap();
    let marker = "secret-marker-7f3c2a";
    let secret_dir = temp.path().join("secrets");
    fs::create_dir_all(&secret_dir).unwrap();
    fs::write(
        secret_dir.join(format!("{ID}.secret.toml")),
        format!("api_key = \"{marker}\"\n"),
    )
    .unwrap();
    fs::write(
        script_path(&manager),
        r#"fn refresh(config) { #{ kind: config.api_key, items: [] } }"#,
    )
    .unwrap();

    let error = run(&mut manager).unwrap_err().to_string();
    let health = manager.health(ID).unwrap();
    assert_eq!(health.last_good.as_ref(), Some(&good));
    assert_eq!(health.consecutive_failures, 1);
    assert!(
        !error.contains(marker),
        "returned error leaked a secret: {error}"
    );
    assert_errors_do_not_contain(&mut manager, marker);
}

#[test]
fn malformed_output_errors_never_expose_secret_values_in_health_or_ui() {
    let (temp, mut manager) = installed_manager(
        r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
    );
    let good = run(&mut manager).unwrap();
    let marker = "secret-marker-7f3c2a";
    let secret_dir = temp.path().join("secrets");
    fs::create_dir_all(&secret_dir).unwrap();
    fs::write(
        secret_dir.join(format!("{ID}.secret.toml")),
        format!("api_key = \"{marker}\"\n"),
    )
    .unwrap();
    fs::write(
        script_path(&manager),
        r#"fn refresh(config) { #{ kind: "key_value", items: config.api_key } }"#,
    )
    .unwrap();

    let error = run(&mut manager).unwrap_err().to_string();
    let health = manager.health(ID).unwrap();
    assert_eq!(health.last_good.as_ref(), Some(&good));
    assert_eq!(health.consecutive_failures, 1);
    assert!(
        !error.contains(marker),
        "returned error leaked a secret: {error}"
    );
    assert_errors_do_not_contain(&mut manager, marker);
}

#[test]
fn non_table_settings_are_reported_as_a_run_failure() {
    let mut settings = BTreeMap::new();
    settings.insert(
        ID.to_string(),
        toml::Value::String("not a table".to_string()),
    );
    let (_temp, mut manager) = installed_manager_with_settings(
        r#"fn refresh(config) { #{ kind: "key_value", items: [] } }"#,
        settings,
    );

    assert!(run(&mut manager).is_err());
    let health = manager.health(ID).expect("invalid settings update health");
    assert_eq!(health.consecutive_failures, 1);
    assert!(
        health
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("table"))
    );
}

#[test]
fn inaccessible_secret_paths_keep_last_good_data_and_update_health() {
    let (temp, mut manager) = installed_manager(
        r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
    );
    let good = run(&mut manager).unwrap();
    fs::write(temp.path().join("secrets"), b"not a directory").unwrap();

    assert!(run(&mut manager).is_err());
    let health = manager.health(ID).unwrap();
    assert_eq!(health.last_good.as_ref(), Some(&good));
    assert_eq!(health.consecutive_failures, 1);
    assert!(health.last_error.is_some());
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

#[test]
fn removing_a_degraded_extension_clears_health_from_every_view() {
    let (_temp, mut manager) = installed_manager(
        r#"fn refresh(config) { #{ kind: "key_value", items: [#{ label: "Status", value: "ok" }] } }"#,
    );
    run(&mut manager).unwrap();
    fs::write(
        script_path(&manager),
        r#"fn refresh(config) { throw "upstream unavailable"; }"#,
    )
    .unwrap();
    for _ in 0..3 {
        assert!(run(&mut manager).is_err());
    }

    // The degraded state is visible through every manager view the UI uses.
    assert!(manager.health(ID).unwrap().degraded);
    assert!(manager.installed_entries()[0].degraded);
    assert!(manager.shop_entries()[0].degraded);
    assert!(
        manager
            .errors()
            .iter()
            .any(|error| error.contains("(degraded)"))
    );

    manager.remove(ID).unwrap();

    assert!(manager.health(ID).is_none());
    assert!(manager.errors().is_empty());
    assert!(manager.installed_entries().is_empty());
    assert!(manager.shop_entries().is_empty());
}
