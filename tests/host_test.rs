//! Integration tests for the sandboxed extension host.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use ha_kiosk::extensions::{
    Clock, EntitySnapshot, ExtensionError, ExtensionHost, ExtensionOutput, ExtensionRunner,
    HostConfig, HttpError, HttpFetcher, HttpPolicy, Limits, Manifest, ManifestError, OutputError,
};
use serde_json::{Value, json};
use url::Url;

const MANIFEST: &str = r#"
id = "org.example.bus-departures"
name = "Bus Departures"
version = "1.0.0"
api_version = 1
min_app_version = "0.6.0"
entry = "main.rhai"
provides = "departures"
author = "Example Author"
license = "MIT"
description = "Shows upcoming departures."

[permissions]
http_hosts = ["api.example.com"]
ha_entities = ["sensor.departures"]
"#;

const SECRET_MANIFEST: &str = r#"
id = "org.example.secret"
name = "Secret Extension"
version = "1.0.0"
api_version = 1
min_app_version = "0.6.0"
entry = "main.rhai"
provides = "key_value"
author = "Example Author"
license = "MIT"
description = "Reads one declared secret."

[permissions]
http_hosts = []
ha_entities = []

[[config_schema]]
key = "api_key"
label = "API key"
type = "secret"
required = true

[[config_schema]]
key = "stop_id"
label = "Stop"
type = "string"
required = true
"#;

const SUCCESS_SCRIPT: &str = r#"
fn refresh(config) {
    #{
        kind: "departures",
        title: "Bus",
        items: [
            #{
                line: "1",
                destination: "Central",
                departure: "2026-11-23T12:05:00Z"
            }
        ]
    }
}
"#;

struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn fixed_clock() -> Arc<FixedClock> {
    Arc::new(FixedClock(
        Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap(),
    ))
}

struct PanicFetcher;

impl HttpFetcher for PanicFetcher {
    fn get_json(&self, _url: &Url) -> Result<Value, HttpError> {
        panic!("the policy must reject before the fetcher is called");
    }
}

struct RecordingFetcher {
    calls: AtomicUsize,
    response: Mutex<Result<Value, String>>,
}

impl RecordingFetcher {
    fn ok(value: Value) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            response: Mutex::new(Ok(value)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl HttpFetcher for RecordingFetcher {
    fn get_json(&self, _url: &Url) -> Result<Value, HttpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match &*self.response.lock().unwrap() {
            Ok(value) => Ok(value.clone()),
            Err(reason) => Err(HttpError::RequestFailed {
                url: "https://api.example.com/fixture".into(),
                reason: reason.clone(),
            }),
        }
    }
}

struct FlakyFetcher {
    failing: AtomicBool,
}

impl FlakyFetcher {
    fn new() -> Self {
        Self {
            failing: AtomicBool::new(false),
        }
    }

    fn set_failing(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }
}

impl HttpFetcher for FlakyFetcher {
    fn get_json(&self, _url: &Url) -> Result<Value, HttpError> {
        if self.failing.load(Ordering::SeqCst) {
            Err(HttpError::RequestFailed {
                url: "https://api.example.com/fixture".into(),
                reason: "provider is down".into(),
            })
        } else {
            Ok(json!({"value": "ok"}))
        }
    }
}

fn manifest() -> Manifest {
    Manifest::parse(MANIFEST).unwrap()
}

fn config(script: &str) -> HostConfig {
    let mut config = HostConfig::new(manifest(), script);
    config.fetcher = Some(Arc::new(PanicFetcher));
    config.clock = Some(fixed_clock());
    config
}

fn host(script: &str) -> ExtensionHost {
    ExtensionHost::new(config(script)).unwrap()
}

fn refresh(script: &str) -> Result<ExtensionOutput, ExtensionError> {
    host(script).refresh()
}

fn key_value_manifest() -> Manifest {
    Manifest::parse(&MANIFEST.replace("provides = \"departures\"", "provides = \"key_value\""))
        .unwrap()
}

fn key_value_config(script: &str) -> HostConfig {
    let mut config = HostConfig::new(key_value_manifest(), script);
    config.fetcher = Some(Arc::new(PanicFetcher));
    config.clock = Some(fixed_clock());
    config
}

fn key_value_host(script: &str) -> ExtensionHost {
    ExtensionHost::new(key_value_config(script)).unwrap()
}

fn fetch_script(url: &str) -> String {
    r#"fn refresh(config) {
        let data = http_get_json("__URL__");
        #{ kind: "key_value", items: [ #{ label: "value", value: data.value } ] }
    }"#
    .replace("__URL__", url)
}

fn loopback_manifest(port: u16) -> Manifest {
    let text = format!(
        r#"
id = "org.example.local"
name = "Local Extension"
version = "1.0.0"
api_version = 1
min_app_version = "0.6.0"
entry = "main.rhai"
provides = "key_value"
author = "Example Author"
license = "MIT"
description = "Uses a loopback test server."

[permissions]
http_hosts = ["127.0.0.1:{port}"]
ha_entities = []
"#
    );
    Manifest::parse(&text).unwrap()
}

fn loopback_host(port: u16, script: &str, limits: Limits) -> ExtensionHost {
    let mut config = HostConfig::new(loopback_manifest(port), script);
    config.dev_mode = true;
    config.clock = Some(fixed_clock());
    config.limits = limits;
    ExtensionHost::new(config).unwrap()
}

struct TestServer {
    port: u16,
}

impl TestServer {
    fn start(response: Vec<u8>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer);
                thread::sleep(delay);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
            }
        });
        Self { port }
    }
}

fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(body);
    response.into_bytes()
}

#[test]
fn manifest_parses_and_rejects_unknown_fields() {
    let parsed = manifest();
    assert_eq!(parsed.id, "org.example.bus-departures");
    assert_eq!(parsed.permissions.http_hosts, vec!["api.example.com"]);
    assert_eq!(parsed.permissions.ha_entities, vec!["sensor.departures"]);

    let unknown = format!("{MANIFEST}\nbogus = true\n");
    assert!(matches!(
        Manifest::parse(&unknown),
        Err(ManifestError::Toml(_))
    ));
}

#[test]
fn manifest_rejects_invalid_identity_version_and_api() {
    let bad_id = MANIFEST.replace("org.example.bus-departures", "Org Example");
    assert!(matches!(
        Manifest::parse(&bad_id),
        Err(ManifestError::InvalidId(_))
    ));

    let bad_version = MANIFEST.replace("version = \"1.0.0\"", "version = \"one\"");
    assert!(matches!(
        Manifest::parse(&bad_version),
        Err(ManifestError::InvalidVersion(_))
    ));

    let bad_api = MANIFEST.replace("api_version = 1", "api_version = 2");
    assert!(matches!(
        Manifest::parse(&bad_api),
        Err(ManifestError::UnsupportedApiVersion { .. })
    ));
}

#[test]
fn manifest_rejects_bad_entry_paths() {
    for entry in [
        "/etc/passwd.rhai",
        "../main.rhai",
        "main.txt",
        "dir//main.rhai",
    ] {
        let text = MANIFEST.replace("entry = \"main.rhai\"", &format!("entry = \"{entry}\""));
        assert!(
            matches!(Manifest::parse(&text), Err(ManifestError::InvalidEntry(_))),
            "entry {entry} should be rejected"
        );
    }
}

#[test]
fn manifest_rejects_wildcard_scheme_and_entity_syntax() {
    for host in [
        "*.example.com",
        "https://example.com",
        "example.com/path",
        "user@example.com",
    ] {
        let text = MANIFEST.replace(
            "http_hosts = [\"api.example.com\"]",
            &format!("http_hosts = [\"{host}\"]"),
        );
        assert!(
            matches!(
                Manifest::parse(&text),
                Err(ManifestError::InvalidHttpHost(_))
            ),
            "host {host} should be rejected"
        );
    }

    let text = MANIFEST.replace(
        "ha_entities = [\"sensor.departures\"]",
        "ha_entities = [\"Sensor.Departures\"]",
    );
    assert!(matches!(
        Manifest::parse(&text),
        Err(ManifestError::InvalidEntityId(_))
    ));
}

#[test]
fn manifest_rejects_invalid_config_schema() {
    let duplicate = SECRET_MANIFEST.replace(
        "key = \"stop_id\"\nlabel = \"Stop\"\ntype = \"string\"\nrequired = true",
        "key = \"api_key\"\nlabel = \"Again\"\ntype = \"string\"\nrequired = true",
    );
    assert!(matches!(
        Manifest::parse(&duplicate),
        Err(ManifestError::InvalidConfig(_))
    ));

    let empty_select = SECRET_MANIFEST.replace(
        "key = \"stop_id\"\nlabel = \"Stop\"\ntype = \"string\"\nrequired = true",
        "key = \"stop_id\"\nlabel = \"Stop\"\ntype = \"select\"\nrequired = true",
    );
    assert!(matches!(
        Manifest::parse(&empty_select),
        Err(ManifestError::InvalidConfig(_))
    ));

    let secret_default = SECRET_MANIFEST.replace(
        "type = \"secret\"\nrequired = true",
        "type = \"secret\"\nrequired = true\ndefault = \"nope\"",
    );
    assert!(matches!(
        Manifest::parse(&secret_default),
        Err(ManifestError::InvalidConfig(_))
    ));
}

#[test]
fn build_config_injects_declared_secrets_and_rejects_unknown_keys() {
    let parsed = Manifest::parse(SECRET_MANIFEST).unwrap();
    let settings = BTreeMap::from([("stop_id".to_string(), json!("REPLACE_ME"))]);
    let secrets = BTreeMap::from([("api_key".to_string(), "s3cret-value".to_string())]);
    let config = parsed.build_config(&settings, &secrets).unwrap();
    assert_eq!(config.get("stop_id"), Some(&json!("REPLACE_ME")));
    assert_eq!(config.get("api_key"), Some(&json!("s3cret-value")));

    let mut unknown_settings = settings.clone();
    unknown_settings.insert("bogus".into(), json!("x"));
    assert!(matches!(
        parsed.build_config(&unknown_settings, &secrets),
        Err(ManifestError::UnknownSetting(key)) if key == "bogus"
    ));

    let empty_settings = BTreeMap::new();
    assert!(matches!(
        parsed.build_config(&empty_settings, &secrets),
        Err(ManifestError::MissingSetting(key)) if key == "stop_id"
    ));

    let mut wrong_type = settings.clone();
    wrong_type.insert("stop_id".into(), json!(42));
    assert!(matches!(
        parsed.build_config(&wrong_type, &secrets),
        Err(ManifestError::InvalidSetting { key, .. }) if key == "stop_id"
    ));
}

#[test]
fn sandbox_registers_only_the_four_host_functions() {
    for call in [
        r#"read_file("/etc/passwd")"#,
        r#"ha_call_service("light.turn_on", #{})"#,
        r#"call_service("light", "turn_on", #{})"#,
        r#"ha_token()"#,
        r#"access_token()"#,
        r#"http_get("https://api.example.com/x")"#,
        r#"system("id")"#,
    ] {
        let script =
            format!("fn refresh(config) {{ {call}; #{{ kind: \"departures\", items: [] }} }}");
        let error = refresh(&script).unwrap_err();
        assert!(
            matches!(error, ExtensionError::Runtime(_)),
            "{call} should not exist, got {error:?}"
        );
    }

    let import = r#"import "std" as std;
fn refresh(config) { #{ kind: "departures", items: [] } }"#;
    assert!(matches!(
        refresh(import),
        Err(ExtensionError::Compile(_)) | Err(ExtensionError::ModulesNotAllowed)
    ));

    let eval = r#"fn refresh(config) { eval("1 + 1"); #{ kind: "departures", items: [] } }"#;
    assert!(matches!(refresh(eval), Err(ExtensionError::Compile(_))));
}

#[test]
fn http_policy_enforces_scheme_host_port_and_credentials() {
    let policy = HttpPolicy::new(
        &[
            "api.example.com".into(),
            "api.example.com:8443".into(),
            "localhost:1234".into(),
        ],
        false,
    )
    .unwrap();

    assert!(policy.check("https://api.example.com/departures").is_ok());
    assert!(policy.check("https://API.EXAMPLE.COM/departures").is_ok());
    assert!(
        policy
            .check("https://api.example.com:8443/departures")
            .is_ok()
    );
    assert!(matches!(
        policy.check("http://api.example.com/departures"),
        Err(HttpError::InsecureScheme { .. })
    ));
    assert!(matches!(
        policy.check("https://user:pass@api.example.com/departures"),
        Err(HttpError::CredentialsNotAllowed { .. })
    ));
    assert!(matches!(
        policy.check("https://evil.example/departures"),
        Err(HttpError::HostNotAllowed { host }) if host == "evil.example"
    ));
    assert!(matches!(
        policy.check("https://api.example.com:9000/departures"),
        Err(HttpError::HostNotAllowed { .. })
    ));

    let dev = HttpPolicy::new(&["127.0.0.1:8000".into()], true).unwrap();
    assert!(dev.check("http://127.0.0.1:8000/x").is_ok());
    assert!(dev.check("http://127.0.0.2:8000/x").is_err());

    let strict = HttpPolicy::new(&["127.0.0.1:8000".into()], false).unwrap();
    assert!(strict.check("http://127.0.0.1:8000/x").is_err());
}

#[test]
fn forbidden_host_is_rejected_before_the_fetcher_runs() {
    let error = refresh(&fetch_script("https://evil.example/departures")).unwrap_err();
    assert!(matches!(error, ExtensionError::Runtime(_)));
    assert!(error.to_string().contains("not allowed"));
}

#[test]
fn allowed_host_uses_the_fetcher() {
    let fetcher = Arc::new(RecordingFetcher::ok(json!({"value": "ok"})));
    let mut config = key_value_config(&fetch_script("https://api.example.com/departures?stop=1"));
    config.fetcher = Some(fetcher.clone());
    let host = ExtensionHost::new(config).unwrap();

    let output = host.refresh().unwrap();
    assert_eq!(fetcher.calls(), 1);
    match output {
        ExtensionOutput::KeyValue(view) => assert_eq!(view.items[0].value, "ok"),
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn ha_state_is_read_only_and_allowlisted() {
    let script = r#"fn refresh(config) {
        let snapshot = ha_state("sensor.departures");
        #{ kind: "key_value", items: [
            #{ label: "state", value: snapshot.state },
            #{ label: "next", value: snapshot.attributes.next }
        ] }
    }"#;
    let mut config = key_value_config(script);
    config.entities.insert(
        "sensor.departures".into(),
        EntitySnapshot {
            state: "3".into(),
            attributes: json!({"next": "12:05"}),
        },
    );
    let host = ExtensionHost::new(config).unwrap();
    match host.refresh().unwrap() {
        ExtensionOutput::KeyValue(view) => {
            assert_eq!(view.items.len(), 2);
            assert_eq!(view.items[0].label, "state");
            assert_eq!(view.items[0].value, "3");
            assert_eq!(view.items[1].label, "next");
            assert_eq!(view.items[1].value, "12:05");
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let forbidden = r#"fn refresh(config) {
        ha_state("sensor.private");
        #{ kind: "key_value", items: [] }
    }"#;
    let mut forbidden_config = key_value_config(forbidden);
    forbidden_config.entities.insert(
        "sensor.private".into(),
        EntitySnapshot {
            state: "private-state".into(),
            attributes: json!({"marker": "private-attribute"}),
        },
    );
    let error = ExtensionHost::new(forbidden_config)
        .unwrap()
        .refresh()
        .unwrap_err()
        .to_string();
    assert!(error.contains("not declared"));
    assert!(!error.contains("private-state"));
    assert!(!error.contains("private-attribute"));
}

#[test]
fn ha_state_returns_a_fresh_snapshot_on_each_call() {
    let script = r#"fn refresh(config) {
        let first = ha_state("sensor.departures");
        first.state = "tampered";
        first.attributes.next = "tampered";
        let second = ha_state("sensor.departures");
        #{ kind: "key_value", items: [
            #{ label: "state", value: second.state },
            #{ label: "next", value: second.attributes.next }
        ] }
    }"#;
    let mut config = key_value_config(script);
    config.entities.insert(
        "sensor.departures".into(),
        EntitySnapshot {
            state: "3".into(),
            attributes: json!({"next": "12:05"}),
        },
    );

    match ExtensionHost::new(config).unwrap().refresh().unwrap() {
        ExtensionOutput::KeyValue(view) => {
            assert_eq!(view.items.len(), 2);
            assert_eq!(view.items[0].value, "3");
            assert_eq!(view.items[1].value, "12:05");
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn ha_state_exposes_only_state_and_attributes() {
    let script = r#"fn refresh(config) {
        let keys = ha_state("sensor.departures").keys();
        #{ kind: "key_value", items: [
            #{ label: "count", value: keys.len().to_string() },
            #{ label: "state", value: keys.contains("state").to_string() },
            #{ label: "attributes", value: keys.contains("attributes").to_string() },
            #{ label: "entity_id", value: keys.contains("entity_id").to_string() },
            #{ label: "context", value: keys.contains("context").to_string() },
            #{ label: "token", value: keys.contains("token").to_string() }
        ] }
    }"#;
    let mut config = key_value_config(script);
    config.entities.insert(
        "sensor.departures".into(),
        EntitySnapshot {
            state: "3".into(),
            attributes: json!({"next": "12:05"}),
        },
    );

    match ExtensionHost::new(config).unwrap().refresh().unwrap() {
        ExtensionOutput::KeyValue(view) => {
            let values: BTreeMap<_, _> = view
                .items
                .iter()
                .map(|item| (item.label.as_str(), item.value.as_str()))
                .collect();
            assert_eq!(values["count"], "2");
            assert_eq!(values["state"], "true");
            assert_eq!(values["attributes"], "true");
            assert_eq!(values["entity_id"], "false");
            assert_eq!(values["context"], "false");
            assert_eq!(values["token"], "false");
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn ha_state_allowlist_rejects_near_miss_entity_ids() {
    for id in [
        "sensor.departures.extra",
        "sensor.departure",
        "SENSOR.DEPARTURES",
        "sensor.departures ",
    ] {
        let script = format!(
            r#"fn refresh(config) {{
                ha_state("{id}");
                #{{ kind: "key_value", items: [] }}
            }}"#
        );
        let mut config = key_value_config(&script);
        config.entities.insert(
            id.into(),
            EntitySnapshot {
                state: "leaked".into(),
                attributes: json!({"secret": "leaked-attribute"}),
            },
        );
        let error = ExtensionHost::new(config)
            .unwrap()
            .refresh()
            .unwrap_err()
            .to_string();
        assert!(error.contains("not declared"), "{id}: {error}");
        assert!(!error.contains("leaked"), "{id}: {error}");
    }
}

#[test]
fn ha_state_error_never_echoes_secret_values() {
    let script = r#"fn refresh(config) {
        ha_state(config.api_key);
        #{ kind: "key_value", items: [] }
    }"#;
    let mut config = HostConfig::new(Manifest::parse(SECRET_MANIFEST).unwrap(), script);
    config.fetcher = Some(Arc::new(PanicFetcher));
    config.clock = Some(fixed_clock());
    config
        .settings
        .insert("stop_id".into(), json!("REPLACE_ME"));
    config
        .secrets
        .insert("api_key".into(), "s3cret-value".into());

    let error = ExtensionHost::new(config)
        .unwrap()
        .refresh()
        .unwrap_err()
        .to_string();
    assert!(error.contains("not declared"), "{error}");
    assert!(!error.contains("s3cret-value"), "{error}");
    assert!(error.contains("[redacted]"), "{error}");
}

#[test]
fn ha_state_returns_unit_for_a_declared_but_missing_entity() {
    let script = r#"fn refresh(config) {
        let value = if ha_state("sensor.departures") == () { "missing" } else { "present" };
        #{ kind: "key_value", items: [ #{ label: "state", value: value } ] }
    }"#;
    match key_value_host(script).refresh().unwrap() {
        ExtensionOutput::KeyValue(view) => assert_eq!(view.items[0].value, "missing"),
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn now_uses_the_configured_timezone() {
    let script = r#"fn refresh(config) {
        #{ kind: "key_value", items: [ #{ label: "now", value: now() } ] }
    }"#;
    let mut config = key_value_config(script);
    config.time_zone = Some(chrono_tz::Europe::Berlin);
    let host = ExtensionHost::new(config).unwrap();
    match host.refresh().unwrap() {
        ExtensionOutput::KeyValue(view) => {
            assert_eq!(view.items[0].value, "2026-11-23T13:00:00+01:00");
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn log_is_bounded_and_redacts_secret_values() {
    let script = r#"fn refresh(config) {
        for i in 0..150 { log("line " + i); }
        #{ kind: "key_value", items: [] }
    }"#;
    let host = key_value_host(script);
    host.refresh().unwrap();
    let logs = host.logs();
    assert_eq!(logs.len(), 100);
    assert_eq!(logs.last().unwrap(), "line 149");

    let secret_script = r#"fn refresh(config) {
        log("key is " + config.api_key);
        #{ kind: "key_value", items: [] }
    }"#;
    let mut config = HostConfig::new(Manifest::parse(SECRET_MANIFEST).unwrap(), secret_script);
    config.fetcher = Some(Arc::new(PanicFetcher));
    config.clock = Some(fixed_clock());
    config
        .settings
        .insert("stop_id".into(), json!("REPLACE_ME"));
    config
        .secrets
        .insert("api_key".into(), "s3cret-value".into());
    let host = ExtensionHost::new(config).unwrap();
    host.refresh().unwrap();
    let logs = host.logs();
    assert_eq!(logs[0], "key is [redacted]");
    assert!(!logs[0].contains("s3cret-value"));
}

#[test]
fn secret_values_are_redacted_from_errors_and_debug_output() {
    let script = r#"fn refresh(config) {
        throw config.api_key;
    }"#;
    let mut config = HostConfig::new(Manifest::parse(SECRET_MANIFEST).unwrap(), script);
    config.fetcher = Some(Arc::new(PanicFetcher));
    config
        .settings
        .insert("stop_id".into(), json!("REPLACE_ME"));
    config
        .secrets
        .insert("api_key".into(), "s3cret-value".into());

    let debug = format!("{config:?}");
    assert!(!debug.contains("s3cret-value"));
    assert!(debug.contains("<1 redacted>"));

    let host = ExtensionHost::new(config).unwrap();
    let error = host.refresh().unwrap_err().to_string();
    assert!(!error.contains("s3cret-value"));
    assert!(error.contains("[redacted]"));
}

#[test]
fn output_kind_must_match_the_manifest() {
    let script = r#"fn refresh(config) {
        #{ kind: "list", columns: ["a"], rows: [["b"]] }
    }"#;
    let error = refresh(script).unwrap_err();
    assert!(matches!(
        error,
        ExtensionError::Output(OutputError::KindMismatch { .. })
    ));
}

#[test]
fn malformed_output_is_rejected() {
    let cases = [
        r#"fn refresh(config) { #{ kind: "departures", items: [ #{ line: "1", departure: "x" } ] } }"#,
        r#"fn refresh(config) { #{ kind: "departures", items: [ #{ line: 1, destination: "x", departure: "y" } ] } }"#,
        r#"fn refresh(config) { #{ kind: "departures", items: [ #{ line: "1", destination: "x", departure: "y", extra: true } ] } }"#,
        r#"fn refresh(config) { #{ kind: "departures", items: [ #{ line: "", destination: "x", departure: "y" } ] } }"#,
        r#"fn refresh(config) { 42 }"#,
    ];
    for script in cases {
        let error = refresh(script).unwrap_err();
        assert!(
            matches!(
                error,
                ExtensionError::Output(OutputError::Malformed { .. })
                    | ExtensionError::Output(OutputError::NotAMap)
            ),
            "{script} should be rejected, got {error:?}"
        );
    }

    let missing_kind = r#"fn refresh(config) { #{ items: [] } }"#;
    assert!(matches!(
        refresh(missing_kind).unwrap_err(),
        ExtensionError::Output(OutputError::MissingKind)
    ));

    let unknown_kind = r#"fn refresh(config) { #{ kind: "table", items: [] } }"#;
    assert!(matches!(
        refresh(unknown_kind).unwrap_err(),
        ExtensionError::Output(OutputError::UnknownKind(_))
    ));
}

#[test]
fn list_output_is_validated() {
    let text = MANIFEST.replace("provides = \"departures\"", "provides = \"list\"");
    let manifest = Manifest::parse(&text).unwrap();

    let mut config = HostConfig::new(
        manifest.clone(),
        r#"fn refresh(config) { #{ kind: "list", columns: ["a", "b"], rows: [["1", "2"]] } }"#,
    );
    config.fetcher = Some(Arc::new(PanicFetcher));
    let host = ExtensionHost::new(config).unwrap();
    match host.refresh().unwrap() {
        ExtensionOutput::List(view) => assert_eq!(view.rows.len(), 1),
        other => panic!("unexpected output: {other:?}"),
    }

    for script in [
        r#"fn refresh(config) { #{ kind: "list", columns: [], rows: [] } }"#,
        r#"fn refresh(config) { #{ kind: "list", columns: ["a"], rows: [["1", "2"]] } }"#,
    ] {
        let mut config = HostConfig::new(manifest.clone(), script);
        config.fetcher = Some(Arc::new(PanicFetcher));
        let host = ExtensionHost::new(config).unwrap();
        assert!(matches!(
            host.refresh().unwrap_err(),
            ExtensionError::Output(OutputError::Malformed { .. })
        ));
    }
}

#[test]
fn output_is_capped_at_one_hundred_items() {
    let script = r#"fn refresh(config) {
        let items = [];
        for i in 0..150 {
            items.push(#{ line: "1", destination: "Central", departure: "2026-11-23T12:05:00Z" });
        }
        #{ kind: "departures", items: items }
    }"#;
    match refresh(script).unwrap() {
        ExtensionOutput::Departures(view) => assert_eq!(view.items.len(), 100),
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn operation_limit_stops_an_infinite_loop() {
    let script = r#"fn refresh(config) {
        let i = 0;
        while true { i += 1; }
    }"#;
    let mut config = config(script);
    config.limits.max_operations = 10_000;
    let host = ExtensionHost::new(config).unwrap();
    assert!(matches!(
        host.refresh().unwrap_err(),
        ExtensionError::OperationLimit
    ));
}

#[test]
fn wall_clock_limit_stops_a_slow_script() {
    let script = r#"fn refresh(config) {
        let i = 0;
        while true { i += 1; }
    }"#;
    let mut config = config(script);
    config.limits.max_operations = 100_000_000;
    config.limits.wall_clock = Duration::from_millis(100);
    let host = ExtensionHost::new(config).unwrap();
    assert!(matches!(
        host.refresh().unwrap_err(),
        ExtensionError::Timeout
    ));
}

#[test]
fn allocation_limits_are_enforced() {
    let mut limits = Limits {
        max_string_bytes: 1_024,
        ..Limits::default()
    };
    let string_script = r#"fn refresh(config) {
        let text = "";
        for i in 0..200 { text += "0123456789"; }
        #{ kind: "departures", items: [] }
    }"#;
    let mut string_config = config(string_script);
    string_config.limits = limits.clone();
    let host = ExtensionHost::new(string_config).unwrap();
    assert!(matches!(
        host.refresh().unwrap_err(),
        ExtensionError::ResourceLimit { what } if what == "Length of string"
    ));

    limits.max_array_entries = 8;
    let array_script = r#"fn refresh(config) {
        let items = [];
        for i in 0..50 { items.push(i); }
        #{ kind: "departures", items: [] }
    }"#;
    let mut array_config = config(array_script);
    array_config.limits = limits.clone();
    let host = ExtensionHost::new(array_config).unwrap();
    assert!(matches!(
        host.refresh().unwrap_err(),
        ExtensionError::ResourceLimit { .. }
    ));

    limits.max_map_entries = 8;
    let map_script = r#"fn refresh(config) {
        let map = #{};
        for i in 0..50 { map["k" + i] = i; }
        #{ kind: "departures", items: [], data: map }
    }"#;
    let mut map_config = config(map_script);
    map_config.limits = limits;
    let host = ExtensionHost::new(map_config).unwrap();
    assert!(matches!(
        host.refresh().unwrap_err(),
        ExtensionError::ResourceLimit { .. }
    ));
}

#[test]
fn oversized_script_is_rejected() {
    let script = format!(
        "fn refresh(config) {{ #{}{} }}",
        "#".repeat(64),
        "\n// padding"
    );
    let mut config = config(&script);
    config.limits.max_script_bytes = 32;
    assert!(matches!(
        ExtensionHost::new(config).unwrap_err(),
        ExtensionError::ScriptTooLarge { .. }
    ));
}

#[test]
fn missing_refresh_function_is_reported() {
    let script = r#"fn other(config) { #{ kind: "departures", items: [] } }"#;
    assert!(matches!(
        refresh(script).unwrap_err(),
        ExtensionError::MissingRefresh
    ));
}

#[test]
fn runner_keeps_last_good_output_and_marks_repeated_failures_degraded() {
    let fetcher = Arc::new(FlakyFetcher::new());
    let script = fetch_script("https://api.example.com/departures");
    let mut config = key_value_config(&script);
    config.fetcher = Some(fetcher.clone());
    let host = ExtensionHost::new(config).unwrap();
    let mut runner = ExtensionRunner::new(host);

    assert!(runner.refresh().is_ok());
    assert!(runner.last_good().is_some());
    assert_eq!(runner.status().consecutive_failures, 0);
    assert!(!runner.status().degraded);

    fetcher.set_failing(true);
    for _ in 0..2 {
        assert!(runner.refresh().is_err());
    }
    assert!(runner.last_good().is_some());
    assert_eq!(runner.status().consecutive_failures, 2);
    assert!(!runner.status().degraded);

    assert!(runner.refresh().is_err());
    assert!(runner.status().degraded);
    assert!(runner.status().last_error.is_some());

    fetcher.set_failing(false);
    assert!(runner.refresh().is_ok());
    assert!(!runner.status().degraded);
    assert_eq!(runner.status().consecutive_failures, 0);
    assert!(runner.status().last_success.is_some());
}

#[test]
fn query_strings_never_reach_error_messages() {
    let server = TestServer::start(
        http_response("500 Internal Server Error", &[], "no"),
        Duration::ZERO,
    );
    let script = fetch_script(&format!(
        "http://127.0.0.1:{}/departures?token=supersecret",
        server.port
    ));
    let host = loopback_host(server.port, &script, Limits::default());
    let error = host.refresh().unwrap_err().to_string();
    assert!(!error.contains("supersecret"), "{error}");
    assert!(error.contains("/departures"), "{error}");
    assert!(error.contains("500"), "{error}");
}

#[test]
fn loopback_http_fetch_returns_json() {
    let server = TestServer::start(
        http_response(
            "200 OK",
            &[("Content-Type", "application/json")],
            r#"{"value":"ok"}"#,
        ),
        Duration::ZERO,
    );
    let script = fetch_script(&format!("http://127.0.0.1:{}/data", server.port));
    let host = loopback_host(server.port, &script, Limits::default());
    match host.refresh().unwrap() {
        ExtensionOutput::KeyValue(view) => assert_eq!(view.items[0].value, "ok"),
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn redirects_are_rejected() {
    let server = TestServer::start(
        http_response(
            "302 Found",
            &[("Location", "https://evil.example/steal")],
            "",
        ),
        Duration::ZERO,
    );
    let script = fetch_script(&format!("http://127.0.0.1:{}/data", server.port));
    let host = loopback_host(server.port, &script, Limits::default());
    let error = host.refresh().unwrap_err().to_string();
    assert!(error.contains("redirects are not allowed"), "{error}");
}

#[test]
fn oversized_http_response_is_rejected() {
    let body = format!(r#"{{"value":"{}"}}"#, "x".repeat(200));
    let server = TestServer::start(
        http_response("200 OK", &[("Content-Type", "application/json")], &body),
        Duration::ZERO,
    );
    let limits = Limits {
        max_http_bytes: 64,
        ..Limits::default()
    };
    let script = fetch_script(&format!("http://127.0.0.1:{}/data", server.port));
    let host = loopback_host(server.port, &script, limits);
    let error = host.refresh().unwrap_err().to_string();
    assert!(error.contains("64 byte limit"), "{error}");
}

#[test]
fn http_timeout_is_rejected() {
    let server = TestServer::start(
        http_response("200 OK", &[], r#"{"value":"late"}"#),
        Duration::from_secs(2),
    );
    let limits = Limits {
        http_timeout: Duration::from_millis(200),
        ..Limits::default()
    };
    let script = fetch_script(&format!("http://127.0.0.1:{}/data", server.port));
    let host = loopback_host(server.port, &script, limits);
    let error = host.refresh().unwrap_err().to_string();
    assert!(error.contains("timed out"), "{error}");
}

#[test]
fn output_serializes_back_to_json() {
    let output = refresh(SUCCESS_SCRIPT).unwrap();
    let value = serde_json::to_value(&output).unwrap();
    assert_eq!(value["kind"], "departures");
    assert_eq!(value["items"][0]["line"], "1");
}
