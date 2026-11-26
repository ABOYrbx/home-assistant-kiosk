//! Executable validation of the canonical bus-departures documentation example.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flate2::{Compression, write::GzEncoder};
use ha_kiosk::extensions::manifest::ConfigFieldType;
use ha_kiosk::extensions::{
    ExtensionError, ExtensionHost, ExtensionManifest, ExtensionOutput, ExtensionRunner, HostConfig,
    HttpError, HttpFetcher, Manifest, OutputError, OutputKind, extract_package,
};
use semver::Version;
use serde_json::{Value, json};
use url::Url;

const AUTHOR_GUIDE: &str = include_str!("../docs/extensions.md");

fn documented_block(start: &str, end: &str, language: &str) -> &'static str {
    let start = AUTHOR_GUIDE
        .find(start)
        .unwrap_or_else(|| panic!("missing documentation marker: {start}"));
    let after_start = &AUTHOR_GUIDE[start..];
    let fence = format!("```{language}\n");
    let body_start = after_start
        .find(&fence)
        .unwrap_or_else(|| panic!("missing {language} fence after {start}"))
        + fence.len();
    let body = &after_start[body_start..];
    let marker_end = body
        .find(end)
        .unwrap_or_else(|| panic!("missing documentation marker: {end}"));
    let body_end = body[..marker_end]
        .find("\n```")
        .unwrap_or_else(|| panic!("unterminated {language} fence after {start}"));
    &body[..body_end]
}

fn example_manifest_toml() -> &'static str {
    documented_block(
        "<!-- example:manifest -->",
        "<!-- /example:manifest -->",
        "toml",
    )
}

fn example_manifest() -> Manifest {
    Manifest::parse(example_manifest_toml()).expect("documented manifest must be valid")
}

fn example_script() -> &'static str {
    documented_block(
        "<!-- example:script -->",
        "<!-- /example:script -->",
        "rhai",
    )
}

fn example_readme() -> &'static str {
    documented_block(
        "<!-- example:readme -->",
        "<!-- /example:readme -->",
        "markdown",
    )
}

fn example_license() -> &'static str {
    documented_block(
        "<!-- example:license -->",
        "<!-- /example:license -->",
        "text",
    )
}

#[test]
fn documented_example_forms_a_runnable_package() {
    let files = [
        ("manifest.toml", example_manifest_toml()),
        ("main.rhai", example_script()),
        ("README.md", example_readme()),
        ("LICENSE", example_license()),
    ];
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        for (path, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path, contents.as_bytes())
                .unwrap();
        }
        builder.finish().unwrap();
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    let package = encoder.finish().unwrap();

    let destination = tempfile::tempdir().unwrap();
    extract_package(&package, destination.path()).unwrap();
    for (path, contents) in files {
        let extracted = std::fs::read_to_string(destination.path().join(path)).unwrap();
        assert_eq!(extracted, contents, "extracted {path} must match the guide");
    }

    let manifest = Manifest::load(destination.path().join("manifest.toml")).unwrap();
    assert_eq!(manifest.id, "org.example.bus-departures");
    let script = std::fs::read_to_string(destination.path().join(&manifest.entry))
        .expect("the manifest-declared entry must be packaged");
    let fetcher = Arc::new(RecordingFetcher::ok(json!({ "departures": [] })));
    let mut config = HostConfig::new(manifest, script);
    config
        .settings
        .insert("stop_id".into(), json!("REPLACE_ME"));
    config.fetcher = Some(fetcher.clone());
    let host = ExtensionHost::new(config).expect("extracted package must build a sandbox");

    let output = host.refresh().expect("extracted entry must run");
    assert!(matches!(output, ExtensionOutput::Departures(_)));
    assert_eq!(
        fetcher.calls(),
        ["https://api.example.com/departures?stop=REPLACE_ME&limit=6"]
    );
}

#[test]
fn documented_example_is_valid() {
    let manifest = example_manifest();
    assert_eq!(manifest.id, "org.example.bus-departures");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.provides, OutputKind::Departures);
    assert_eq!(manifest.permissions.http_hosts, ["api.example.com"]);
    assert!(manifest.permissions.ha_entities.is_empty());

    let stop_id = manifest
        .config_schema
        .iter()
        .find(|field| field.key == "stop_id")
        .expect("stop_id must be declared");
    assert!(stop_id.required);
    assert_eq!(stop_id.field_type, ConfigFieldType::String);

    let limit = manifest
        .config_schema
        .iter()
        .find(|field| field.key == "limit")
        .expect("limit must be declared");
    assert!(!limit.required);
    assert_eq!(limit.field_type, ConfigFieldType::Integer);
    assert_eq!(limit.default, Some(json!(6)));
    assert_eq!(limit.min, Some(1));
    assert_eq!(limit.max, Some(20));

    let catalog_manifest = ExtensionManifest::parse_toml(example_manifest_toml())
        .expect("catalog manifest parser must accept the documented manifest");
    catalog_manifest
        .validate(&Version::parse(env!("CARGO_PKG_VERSION")).unwrap())
        .expect("catalog manifest must be compatible with the current app version");

    assert!(
        rhai::Engine::new().compile(example_script()).is_ok(),
        "the documented Rhai example must compile"
    );
}

struct RecordingFetcher {
    response: Mutex<Result<Value, String>>,
    calls: Mutex<Vec<Url>>,
}

impl RecordingFetcher {
    fn ok(response: Value) -> Self {
        Self {
            response: Mutex::new(Ok(response)),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn failing(reason: &str) -> Self {
        Self {
            response: Mutex::new(Err(reason.to_string())),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(Url::as_str)
            .map(str::to_string)
            .collect()
    }

    fn fail(&self, reason: &str) {
        *self.response.lock().unwrap() = Err(reason.to_string());
    }
}

impl HttpFetcher for RecordingFetcher {
    fn get_json(&self, url: &Url, _timeout: Duration) -> Result<Value, HttpError> {
        self.calls.lock().unwrap().push(url.clone());
        match &*self.response.lock().unwrap() {
            Ok(value) => Ok(value.clone()),
            Err(reason) => Err(HttpError::RequestFailed {
                url: "https://api.example.com/departures".into(),
                reason: reason.clone(),
            }),
        }
    }
}

fn example_host(
    fetcher: Arc<RecordingFetcher>,
    stop_id: &str,
    limit: Option<i64>,
) -> ExtensionHost {
    let mut config = HostConfig::new(example_manifest(), example_script());
    config.settings.insert("stop_id".into(), json!(stop_id));
    if let Some(limit) = limit {
        config.settings.insert("limit".into(), json!(limit));
    }
    config.fetcher = Some(fetcher);
    ExtensionHost::new(config).expect("documented example must build a sandbox")
}

fn two_departures() -> Value {
    json!({
        "departures": [
            {
                "line": "1",
                "destination": "Central",
                "time": "2026-11-25T12:05:00Z",
                "delay_minutes": 2,
                "platform": "A",
                "cancelled": false,
                "remarks": "On time"
            },
            {
                "line": "2",
                "destination": "Airport",
                "time": "2026-11-25T12:10:00+01:00"
            }
        ]
    })
}

#[test]
fn example_requests_configured_stop_and_default_limit() {
    let fetcher = Arc::new(RecordingFetcher::ok(json!({ "departures": [] })));
    let host = example_host(fetcher.clone(), " REPLACE_ME ", None);

    host.refresh().expect("empty fixture must be valid");

    assert_eq!(
        fetcher.calls(),
        ["https://api.example.com/departures?stop=REPLACE_ME&limit=6"]
    );
}

#[test]
fn example_maps_provider_fields_and_optional_defaults() {
    let fetcher = Arc::new(RecordingFetcher::ok(two_departures()));
    let host = example_host(fetcher, "REPLACE_ME", None);

    let output = host.refresh().expect("fixture must produce departures");
    let ExtensionOutput::Departures(view) = output else {
        panic!("expected departures, got {output:?}");
    };
    assert_eq!(view.title.as_deref(), Some("Bus"));
    assert_eq!(view.items.len(), 2);

    let first = &view.items[0];
    assert_eq!(first.line, "1");
    assert_eq!(first.destination, "Central");
    assert_eq!(first.departure, "2026-11-25T12:05:00Z");
    assert_eq!(first.delay_minutes, Some(2));
    assert_eq!(first.platform.as_deref(), Some("A"));
    assert!(!first.cancelled);
    assert_eq!(first.remarks.as_deref(), Some("On time"));

    let second = &view.items[1];
    assert_eq!(second.line, "2");
    assert_eq!(second.destination, "Airport");
    assert_eq!(second.departure, "2026-11-25T12:10:00+01:00");
    assert_eq!(second.delay_minutes, Some(0));
    assert_eq!(second.platform, None);
    assert!(!second.cancelled);
    assert_eq!(second.remarks, None);
}

#[test]
fn example_respects_configured_limit() {
    let fetcher = Arc::new(RecordingFetcher::ok(two_departures()));
    let host = example_host(fetcher.clone(), "REPLACE_ME", Some(1));

    let output = host.refresh().expect("fixture must produce departures");
    let ExtensionOutput::Departures(view) = output else {
        panic!("expected departures, got {output:?}");
    };
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].line, "1");
    assert_eq!(
        fetcher.calls(),
        ["https://api.example.com/departures?stop=REPLACE_ME&limit=1"]
    );
}

#[test]
fn example_accepts_safe_stop_id_punctuation() {
    let fetcher = Arc::new(RecordingFetcher::ok(json!({ "departures": [] })));
    let host = example_host(fetcher.clone(), "REPLACE_ME-_.~", None);

    host.refresh().expect("safe stop ID must be accepted");

    assert_eq!(
        fetcher.calls(),
        ["https://api.example.com/departures?stop=REPLACE_ME-_.~&limit=6"]
    );
}

#[test]
fn example_rejects_out_of_range_limits_before_fetch() {
    for limit in [0, 21] {
        let fetcher = Arc::new(RecordingFetcher::ok(json!({ "departures": [] })));
        let mut config = HostConfig::new(example_manifest(), example_script());
        config
            .settings
            .insert("stop_id".into(), json!("REPLACE_ME"));
        config.settings.insert("limit".into(), json!(limit));
        config.fetcher = Some(fetcher.clone());

        let error = ExtensionHost::new(config).expect_err("out-of-range limit must be rejected");
        assert!(error.to_string().contains("limit"), "got {error:?}");
        assert!(fetcher.calls().is_empty());
    }
}

#[test]
fn example_accepts_empty_departures() {
    let fetcher = Arc::new(RecordingFetcher::ok(json!({ "departures": [] })));
    let host = example_host(fetcher, "REPLACE_ME", None);

    let output = host.refresh().expect("empty fixture must be valid");
    let ExtensionOutput::Departures(view) = output else {
        panic!("expected departures, got {output:?}");
    };
    assert!(view.items.is_empty());
}

#[test]
fn example_maps_null_optional_fields_to_defaults() {
    let fetcher = Arc::new(RecordingFetcher::ok(json!({
        "departures": [{
            "line": "1",
            "destination": "Central",
            "time": "2026-11-25T12:05:00Z",
            "delay_minutes": null,
            "platform": null,
            "cancelled": null,
            "remarks": null
        }]
    })));
    let host = example_host(fetcher, "REPLACE_ME", None);

    let output = host.refresh().expect("null optional fields must be valid");
    let ExtensionOutput::Departures(view) = output else {
        panic!("expected departures, got {output:?}");
    };
    let item = &view.items[0];
    assert_eq!(item.delay_minutes, Some(0));
    assert_eq!(item.platform, None);
    assert!(!item.cancelled);
    assert_eq!(item.remarks, None);
}

#[test]
fn example_rejects_missing_required_provider_field() {
    let fetcher = Arc::new(RecordingFetcher::ok(json!({
        "departures": [{
            "line": "1",
            "time": "2026-11-25T12:05:00Z"
        }]
    })));
    let host = example_host(fetcher, "REPLACE_ME", None);

    let error = host
        .refresh()
        .expect_err("missing destination must be rejected");
    assert!(
        matches!(error, ExtensionError::Output(OutputError::Malformed { .. })),
        "missing destination must be rejected, got {error:?}"
    );
}

#[test]
fn example_rejects_invalid_stop_ids_before_fetch() {
    for stop_id in [
        "",
        "   ",
        "bad&host=evil.example",
        "bad?query",
        "bad#fragment",
        "bad%20id",
        "bad+id",
        "bad/id",
        "München",
    ] {
        let fetcher = Arc::new(RecordingFetcher::ok(json!({ "departures": [] })));
        let host = example_host(fetcher.clone(), stop_id, None);

        let error = host
            .refresh()
            .expect_err("invalid stop ID must be rejected");
        assert!(matches!(error, ExtensionError::Runtime(_)));
        assert!(error.to_string().contains("stop_id"));
        assert!(
            fetcher.calls().is_empty(),
            "stop ID {stop_id:?} must not fetch"
        );
    }
}

#[test]
fn example_propagates_provider_errors() {
    let fetcher = Arc::new(RecordingFetcher::failing("provider is down"));
    let host = example_host(fetcher, "REPLACE_ME", None);

    let error = host.refresh().expect_err("provider failure must propagate");
    assert!(matches!(error, ExtensionError::Runtime(_)));
    assert!(error.to_string().contains("provider is down"));
}

#[test]
fn provider_error_keeps_last_good_example_output() {
    let fetcher = Arc::new(RecordingFetcher::ok(two_departures()));
    let host = example_host(fetcher.clone(), "REPLACE_ME", None);
    let mut runner = ExtensionRunner::new(host);

    let first = runner.refresh().expect("first refresh must succeed");
    fetcher.fail("provider is down");
    let error = runner.refresh().expect_err("second refresh must fail");

    assert!(error.to_string().contains("provider is down"));
    assert_eq!(runner.last_good(), Some(&first));
    assert_eq!(runner.status().consecutive_failures, 1);
    assert!(!runner.status().degraded);
    assert!(runner.status().last_error.is_some());
}
