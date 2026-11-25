//! Executable examples for the Wave 6 extension author guide.

use std::collections::BTreeMap;

use ha_kiosk::extensions::output::validate_output;
use ha_kiosk::extensions::{
    CatalogEntry, CatalogIndex, CatalogPolicy, Clock, EntitySnapshot, ExtensionHealth,
    ExtensionHost, ExtensionManifest, ExtensionOutput, ExtensionRunner, HostConfig, HttpError,
    HttpFetcher, Manifest, OutputKind,
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

trait TestManifest {
    fn with_output_kind(self, kind: OutputKind) -> Self;
    fn with_optional_settings(self) -> Self;
    fn with_http_hosts(self, hosts: &[&str]) -> Self;
}

impl TestManifest for Manifest {
    fn with_output_kind(mut self, kind: OutputKind) -> Self {
        self.provides = kind;
        self
    }

    fn with_optional_settings(mut self) -> Self {
        for field in &mut self.config_schema {
            field.required = false;
        }
        self
    }

    fn with_http_hosts(mut self, hosts: &[&str]) -> Self {
        self.permissions.http_hosts = hosts.iter().map(|host| (*host).to_string()).collect();
        self
    }
}

fn manifest_without_required_settings() -> Manifest {
    Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1"))
        .unwrap()
        .with_output_kind(OutputKind::KeyValue)
        .with_optional_settings()
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

#[derive(Debug)]
struct FixedClock(chrono::DateTime<chrono::Utc>);

impl Clock for FixedClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

#[test]
fn documented_optional_departure_fields_accept_omission_and_null() {
    let manifest = Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1"))
        .unwrap()
        .with_optional_settings();
    let script = r#"
fn refresh(config) {
    if config.limit == 20 {
        let item = #{
            line: "1",
            destination: "Central Station",
            departure: "2026-11-23T12:05:00Z",
            delay_minutes: (),
            platform: (),
            remarks: ()
        };
        let result = #{
            kind: "departures",
            title: (),
            items: [item]
        };
        return result;
    }
    if config.limit == 19 {
        let item = #{
            line: "2",
            destination: "North Station",
            departure: "2026-11-23T12:10:00Z"
        };
        let result = #{ kind: "departures", items: [item] };
        return result;
    }
    #{ kind: "departures", items: [] }
}
"#;
    let mut config = HostConfig::new(manifest, script);
    config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(20)),
    ]);
    let host = ExtensionHost::new(config).unwrap();

    let ExtensionOutput::Departures(view) = host.refresh().unwrap() else {
        panic!("unexpected output");
    };
    assert_eq!(view.title, None);
    assert_eq!(view.items[0].delay_minutes, None);
    assert_eq!(view.items[0].platform, None);
    assert_eq!(view.items[0].remarks, None);

    let mut omitted_config = HostConfig::new(
        Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1"))
            .unwrap()
            .with_optional_settings(),
        script,
    );
    omitted_config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(19)),
    ]);
    let ExtensionOutput::Departures(omitted) = ExtensionHost::new(omitted_config)
        .unwrap()
        .refresh()
        .unwrap()
    else {
        panic!("unexpected output");
    };
    assert_eq!(omitted.title, None);
    assert_eq!(omitted.items[0].delay_minutes, None);
    assert_eq!(omitted.items[0].platform, None);
    assert_eq!(omitted.items[0].remarks, None);
}

#[test]
fn documented_optional_list_fields_accept_omission_and_null() {
    let manifest = Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1"))
        .unwrap()
        .with_output_kind(OutputKind::List)
        .with_optional_settings();
    let script = r#"
fn refresh(config) {
    if config.limit == 20 {
        let result = #{
            kind: "list",
            title: (),
            columns: ["Line"],
            rows: [["1"]]
        };
        return result;
    }
    if config.limit == 19 {
        let result = #{ kind: "list", columns: ["Line"], rows: [["2"]] };
        return result;
    }
    #{ kind: "list", columns: ["Line"], rows: [] }
}
"#;
    let mut config = HostConfig::new(manifest, script);
    config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(20)),
    ]);
    let host = ExtensionHost::new(config).unwrap();

    let ExtensionOutput::List(view) = host.refresh().unwrap() else {
        panic!("unexpected output");
    };
    assert_eq!(view.title, None);
    assert_eq!(view.rows, vec![vec!["1".to_string()]]);

    let mut omitted_config = HostConfig::new(
        Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1"))
            .unwrap()
            .with_output_kind(OutputKind::List)
            .with_optional_settings(),
        script,
    );
    omitted_config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(19)),
    ]);
    let ExtensionOutput::List(omitted) = ExtensionHost::new(omitted_config)
        .unwrap()
        .refresh()
        .unwrap()
    else {
        panic!("unexpected output");
    };
    assert_eq!(omitted.title, None);
    assert_eq!(omitted.rows, vec![vec!["2".to_string()]]);
}

#[test]
fn documented_now_uses_the_host_timezone_and_falls_back_to_utc() {
    let manifest = manifest_without_required_settings();
    let clock = std::sync::Arc::new(FixedClock("2026-11-23T12:00:00Z".parse().unwrap()));
    let script = r#"
fn refresh(config) {
    let item = #{ label: "Now", value: now() };
    let result = #{
        kind: "key_value",
        title: (),
        items: [item]
    };
    return result;
}
"#;

    let settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(20)),
    ]);
    let mut zoned = HostConfig::new(manifest.clone(), script);
    zoned.settings = settings.clone();
    zoned.clock = Some(clock.clone());
    zoned.time_zone = Some("Europe/Berlin".parse().unwrap());
    let ExtensionOutput::KeyValue(zoned) = ExtensionHost::new(zoned).unwrap().refresh().unwrap()
    else {
        panic!("unexpected output");
    };
    assert_eq!(zoned.items[0].value, "2026-11-23T13:00:00+01:00");

    let mut fallback = HostConfig::new(manifest, script);
    fallback.settings = settings;
    fallback.clock = Some(clock);
    let ExtensionOutput::KeyValue(fallback) =
        ExtensionHost::new(fallback).unwrap().refresh().unwrap()
    else {
        panic!("unexpected output");
    };
    assert_eq!(fallback.items[0].value, "2026-11-23T12:00:00+00:00");
}

#[test]
fn documented_ha_state_returns_only_declared_entities() {
    let mut manifest = manifest_without_required_settings();
    manifest.permissions.ha_entities = vec!["sensor.departures".to_string()];
    let script = r#"
fn refresh(config) {
    let state = ha_state("sensor.departures");
    let item = #{ label: "State", value: state.state };
    let result = #{
        kind: "key_value",
        title: (),
        items: [item]
    };
    return result;
}
"#;
    let mut config = HostConfig::new(manifest.clone(), script);
    config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(20)),
    ]);
    config.entities = BTreeMap::from([(
        "sensor.departures".to_string(),
        EntitySnapshot {
            state: "3".to_string(),
            attributes: json!({}),
        },
    )]);

    let ExtensionOutput::KeyValue(output) = ExtensionHost::new(config).unwrap().refresh().unwrap()
    else {
        panic!("unexpected output");
    };
    assert_eq!(output.items[0].value, "3");

    let script = r#"
fn refresh(config) {
    let item = #{ label: "Forbidden", value: ha_state("sensor.example") };
    let result = #{ kind: "key_value", items: [item] };
    return result;
}
"#;
    let mut forbidden_config = HostConfig::new(manifest, script);
    forbidden_config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(20)),
    ]);
    let error = ExtensionHost::new(forbidden_config)
        .unwrap()
        .refresh()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("entity is not declared in the manifest")
    );
}

#[derive(Debug)]
struct FlakyFetcher {
    response: std::sync::Mutex<Result<Value, ()>>,
}

impl FlakyFetcher {
    fn new(response: Value) -> Self {
        Self {
            response: std::sync::Mutex::new(Ok(response)),
        }
    }

    fn set_failing(&self, failing: bool) {
        *self.response.lock().unwrap() = if failing {
            Err(())
        } else {
            Ok(json!({ "departures": [] }))
        };
    }
}

impl HttpFetcher for FlakyFetcher {
    fn get_json(&self, _url: &Url) -> Result<Value, HttpError> {
        match &*self.response.lock().unwrap() {
            Ok(value) => Ok(value.clone()),
            Err(_reason) => Err(HttpError::InvalidJson {
                url: "https://api.example.com/departures".to_string(),
            }),
        }
    }
}

#[test]
fn documented_failure_tracking_keeps_last_good_output_and_recovers() {
    let manifest = Manifest::parse(fenced_block(AUTHOR_GUIDE, "toml", "api_version = 1"))
        .unwrap()
        .with_http_hosts(&["api.example.com"]);
    let script = r#"
fn refresh(config) {
    let data = http_get_json("https://api.example.com/departures");
    let items = [];
    for departure in data.departures {
        let item = #{
            line: departure.line,
            destination: departure.destination,
            departure: departure.time
        };
        items.push(item);
    }
    let result = #{ kind: "departures", items: items };
    return result;
}
"#;
    let fetcher = std::sync::Arc::new(FlakyFetcher::new(json!({
        "departures": [{
            "line": "1",
            "destination": "Central Station",
            "time": "2026-11-23T12:05:00Z"
        }]
    })));
    let mut startup_config = HostConfig::new(manifest.clone(), script);
    startup_config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(1)),
    ]);
    startup_config.fetcher = Some(fetcher.clone());
    let mut runner = ExtensionRunner::new(ExtensionHost::new(startup_config).unwrap());
    runner.refresh().unwrap();
    let good = runner.last_good().unwrap().clone();
    assert_eq!(good.item_count(), 1);

    let mut config = HostConfig::new(manifest, script);
    config.settings = BTreeMap::from([
        ("stop_id".to_string(), json!("REPLACE_ME")),
        ("limit".to_string(), json!(1)),
    ]);
    config.fetcher = Some(fetcher.clone());
    let mut runner = ExtensionRunner::new(ExtensionHost::new(config).unwrap());

    fetcher.set_failing(true);
    for _ in 0..2 {
        runner.refresh().unwrap_err();
        assert!(!runner.status().degraded);
    }
    runner.refresh().unwrap_err();
    assert!(runner.status().degraded);
    assert_eq!(runner.status().consecutive_failures, 3);
    assert_eq!(runner.last_good(), None);

    fetcher.set_failing(false);
    let recovered = runner.refresh().unwrap();
    assert!(!runner.status().degraded);
    assert_eq!(runner.status().consecutive_failures, 0);
    assert_eq!(runner.last_good(), Some(&recovered));

    let mut health = ExtensionHealth::default();
    health.mark_success(good.clone());
    assert_eq!(health.last_good.as_ref(), Some(&good));
    for _ in 0..2 {
        health.mark_failure("invalid output".to_string());
        assert!(!health.degraded);
    }
    health.mark_failure("invalid output".to_string());
    assert!(health.degraded);
    assert_eq!(health.consecutive_failures, 3);
    assert_eq!(health.last_good.as_ref(), Some(&good));
    health.mark_success(recovered);
    assert!(!health.degraded);
    assert_eq!(health.consecutive_failures, 0);
    assert!(health.last_error.is_none());
}
