//! Executable examples for the Wave 6 extension author guide.

use std::collections::BTreeMap;

use ha_kiosk::extensions::output::validate_output;
use ha_kiosk::extensions::{
    CatalogEntry, CatalogIndex, CatalogPolicy, ExtensionHost, ExtensionManifest, ExtensionOutput,
    HostConfig, HttpError, HttpFetcher, Manifest, OutputKind,
};
use semver::Version;
use serde_json::{Value, json};
use url::Url;

const AUTHOR_GUIDE: &str = include_str!("../docs/extensions.md");
const CATALOG_GUIDE: &str = include_str!("../docs/extension-catalog.md");

fn fenced_block<'a>(document: &'a str, language: &str, marker: &str) -> &'a str {
    for block in document.split("```").skip(1).step_by(2) {
        let Some((info, body)) = block.split_once('\n') else {
            continue;
        };
        if info.trim() == language && body.contains(marker) {
            return body.trim_end_matches('\n');
        }
    }
    panic!("missing {language} code block containing {marker:?}");
}

struct DeparturesFetcher;

impl HttpFetcher for DeparturesFetcher {
    fn get_json(&self, _url: &Url) -> Result<Value, HttpError> {
        Ok(json!({
            "departures": [
                {
                    "line": "1",
                    "destination": "Central Station",
                    "time": "2026-11-23T12:05:00Z",
                    "delay_minutes": 0,
                    "platform": "1",
                    "cancelled": false,
                    "remarks": ""
                },
                {
                    "line": "2",
                    "destination": "North Station",
                    "time": "2026-11-23T12:10:00Z",
                    "delay_minutes": 0,
                    "platform": "2",
                    "cancelled": false,
                    "remarks": ""
                }
            ]
        }))
    }
}

fn assert_manifest_matches_entry(manifest: &ExtensionManifest, entry: &CatalogEntry) {
    assert_eq!(manifest.id, entry.id);
    assert_eq!(manifest.name, entry.name);
    assert_eq!(manifest.version, entry.version);
    assert_eq!(manifest.api_version, entry.api_version);
    assert_eq!(manifest.min_app_version, entry.min_app_version);
    assert_eq!(manifest.provides, entry.provides);
    assert_eq!(manifest.permissions, entry.permissions);
    assert_eq!(manifest.config_schema, entry.config_schema);
    assert_eq!(manifest.author, entry.author);
    assert_eq!(manifest.license, entry.license);
    assert_eq!(manifest.homepage, entry.homepage);
    assert_eq!(manifest.description, entry.description);
}

#[test]
fn documented_manifest_parses() {
    let manifest = Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1")).unwrap();

    assert_eq!(manifest.id, "org.example.bus-departures");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.provides, OutputKind::Departures);
}

#[test]
fn documented_catalog_example_matches_the_package_manifest() {
    let app_version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    let manifest_text = fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1");
    let runtime_manifest = Manifest::parse(manifest_text).unwrap();
    let package_manifest = ExtensionManifest::parse_toml(manifest_text).unwrap();
    package_manifest.validate(&app_version).unwrap();

    let index_json = fenced_block(CATALOG_GUIDE, "json", "\"schema_version\": 1");
    let policy = CatalogPolicy {
        app_version,
        public_keys: Vec::new(),
        allow_unsigned: false,
        dev_mode: false,
        max_catalog_bytes: 2 * 1024 * 1024,
        max_package_bytes: 5 * 1024 * 1024,
    };
    let index = CatalogIndex::parse(index_json.as_bytes(), &policy).unwrap();
    let entry = index.find(&runtime_manifest.id).unwrap();
    assert_manifest_matches_entry(&package_manifest, entry);

    let settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(1)),
    ]);
    let config = runtime_manifest
        .build_config(&settings, &BTreeMap::new())
        .unwrap();
    assert_eq!(config.get("stop_id"), Some(&json!("REPLACE_ME")));
    assert_eq!(config.get("limit"), Some(&json!(1)));
}

#[test]
fn documented_bus_example_honors_the_declared_limit() {
    let manifest = Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1")).unwrap();
    let script = fenced_block(AUTHOR_GUIDE, "rhai", "http_get_json");
    let mut config = HostConfig::new(manifest, script);
    config.fetcher = Some(std::sync::Arc::new(DeparturesFetcher));
    config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(1)),
    ]);

    let output = ExtensionHost::new(config).unwrap().refresh().unwrap();

    match output {
        ExtensionOutput::Departures(view) => {
            assert_eq!(view.items.len(), 1);
            assert_eq!(view.items[0].line, "1");
            assert_eq!(view.items[0].destination, "Central Station");
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn documented_list_example_validates() {
    let value: Value =
        serde_json::from_str(fenced_block(AUTHOR_GUIDE, "json", "\"kind\": \"list\"")).unwrap();
    let output = validate_output(value, OutputKind::List, 100).unwrap();

    assert_eq!(output.item_count(), 1);
    assert_eq!(output.kind(), OutputKind::List);
}

#[test]
fn documented_key_value_example_validates() {
    let value: Value = serde_json::from_str(fenced_block(
        AUTHOR_GUIDE,
        "json",
        "\"kind\": \"key_value\"",
    ))
    .unwrap();
    let output = validate_output(value, OutputKind::KeyValue, 100).unwrap();

    assert_eq!(output.item_count(), 2);
    assert_eq!(output.kind(), OutputKind::KeyValue);
}
