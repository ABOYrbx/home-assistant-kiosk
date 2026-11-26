//! The Rhai sandbox and its host functions.
//!
//! The engine has no filesystem, process, environment, or module access. The
//! host registers exactly four functions:
//!
//! - `http_get_json(url)` -- allowlisted, HTTPS, GET-only, no credentials, no
//!   redirects, 10 s timeout, 1 MB response cap.
//! - `ha_state(entity_id)` -- read-only snapshot of a declared entity; no
//!   service calls and no access to the Home Assistant token.
//! - `now()` -- RFC 3339 timestamp in the configured Home Assistant timezone.
//! - `log(message)` -- bounded debug log with secret values redacted.
//!
//! Operation, string, array, map, call-depth, and wall-clock limits bound a
//! hostile script. The host must be driven from a background thread because
//! both the script and the HTTP calls are blocking.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rhai::{Dynamic, Engine, EvalAltResult, Position, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::http::{HttpError, HttpFetcher, HttpPolicy, ReqwestFetcher};
use super::manifest::{Manifest, ManifestError};
use super::output::{ExtensionOutput, MAX_OUTPUT_ITEMS, OutputError, validate_output};

const WALL_CLOCK_MARKER: &str = "extension wall-clock limit exceeded";

/// Resource limits for one extension run.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_operations: u64,
    pub max_string_bytes: usize,
    pub max_array_entries: usize,
    pub max_map_entries: usize,
    pub max_call_levels: usize,
    pub max_expr_depth: usize,
    pub max_modules: usize,
    pub max_functions: usize,
    pub max_variables: usize,
    pub max_script_bytes: usize,
    pub wall_clock: Duration,
    pub http_timeout: Duration,
    pub max_http_bytes: usize,
    pub max_log_entries: usize,
    pub max_log_bytes: usize,
    pub max_items: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_operations: 1_000_000,
            max_string_bytes: 64 * 1024,
            max_array_entries: 1_000,
            max_map_entries: 1_000,
            max_call_levels: 32,
            max_expr_depth: 64,
            max_modules: 0,
            max_functions: 64,
            max_variables: 64,
            max_script_bytes: 256 * 1024,
            wall_clock: Duration::from_secs(10),
            http_timeout: Duration::from_secs(10),
            max_http_bytes: 1024 * 1024,
            max_log_entries: 100,
            max_log_bytes: 2 * 1024,
            max_items: MAX_OUTPUT_ITEMS,
        }
    }
}

/// A read-only Home Assistant entity snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntitySnapshot {
    pub state: String,
    #[serde(default = "empty_object")]
    pub attributes: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// A source of the current time.
///
/// The host takes a clock so tests can produce deterministic timestamps.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Everything the host needs to run one extension.
pub struct HostConfig {
    pub manifest: Manifest,
    pub script: String,
    pub settings: BTreeMap<String, Value>,
    pub secrets: BTreeMap<String, String>,
    pub entities: BTreeMap<String, EntitySnapshot>,
    pub fetcher: Option<Arc<dyn HttpFetcher>>,
    pub clock: Option<Arc<dyn Clock>>,
    pub time_zone: Option<Tz>,
    pub dev_mode: bool,
    pub limits: Limits,
}

impl HostConfig {
    pub fn new(manifest: Manifest, script: impl Into<String>) -> Self {
        Self {
            manifest,
            script: script.into(),
            settings: BTreeMap::new(),
            secrets: BTreeMap::new(),
            entities: BTreeMap::new(),
            fetcher: None,
            clock: None,
            time_zone: None,
            dev_mode: false,
            limits: Limits::default(),
        }
    }
}

impl fmt::Debug for HostConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostConfig")
            .field("manifest", &self.manifest.id)
            .field("script_bytes", &self.script.len())
            .field("settings", &self.settings.keys().collect::<Vec<_>>())
            .field(
                "secrets",
                &format_args!("<{} redacted>", self.secrets.len()),
            )
            .field("entities", &self.entities.keys().collect::<Vec<_>>())
            .field("dev_mode", &self.dev_mode)
            .field("limits", &self.limits)
            .finish()
    }
}

/// A prepared extension sandbox.
pub struct ExtensionHost {
    manifest: Manifest,
    script: String,
    config: serde_json::Map<String, Value>,
    policy: HttpPolicy,
    fetcher: Arc<dyn HttpFetcher>,
    entities: Arc<BTreeMap<String, EntitySnapshot>>,
    clock: Arc<dyn Clock>,
    time_zone: Tz,
    limits: Limits,
    secret_values: Arc<Vec<String>>,
    logs: Arc<Mutex<Vec<String>>>,
}

impl fmt::Debug for ExtensionHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionHost")
            .field("manifest", &self.manifest.id)
            .field("script_bytes", &self.script.len())
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl ExtensionHost {
    /// Validate the manifest and build a sandbox.
    pub fn new(config: HostConfig) -> Result<Self, ExtensionError> {
        config.manifest.validate()?;
        if config.script.len() > config.limits.max_script_bytes {
            return Err(ExtensionError::ScriptTooLarge {
                size: config.script.len(),
                max: config.limits.max_script_bytes,
            });
        }

        let config_values = config
            .manifest
            .build_config(&config.settings, &config.secrets)?;
        let policy = HttpPolicy::new(&config.manifest.permissions.http_hosts, config.dev_mode)?;
        let fetcher = match config.fetcher {
            Some(fetcher) => fetcher,
            None => Arc::new(ReqwestFetcher::new(
                config.limits.http_timeout,
                config.limits.max_http_bytes,
            )?),
        };
        let secret_values = config
            .secrets
            .values()
            .filter(|value| !value.is_empty())
            .cloned()
            .collect();

        Ok(Self {
            manifest: config.manifest,
            script: config.script,
            config: config_values,
            policy,
            fetcher,
            entities: Arc::new(config.entities),
            clock: config.clock.unwrap_or_else(|| Arc::new(SystemClock)),
            time_zone: config.time_zone.unwrap_or(Tz::UTC),
            limits: config.limits,
            secret_values: Arc::new(secret_values),
            logs: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// The captured `log` messages, oldest first.
    pub fn logs(&self) -> Vec<String> {
        self.logs
            .lock()
            .map(|logs| logs.clone())
            .unwrap_or_default()
    }

    /// The current time in the configured timezone as RFC 3339.
    pub fn now_rfc3339(&self) -> String {
        self.clock.now().with_timezone(&self.time_zone).to_rfc3339()
    }

    /// Compile and run `fn refresh(config)` and validate its output.
    ///
    /// This method blocks on script execution and HTTP calls. Run it on a
    /// background thread, never on the UI thread.
    pub fn refresh(&self) -> Result<ExtensionOutput, ExtensionError> {
        let start = Instant::now();
        let engine = self.build_engine(start);
        let ast = engine
            .compile(&self.script)
            .map_err(|error| ExtensionError::Compile(self.redact(&error.to_string())))?;

        let config = rhai::serde::to_dynamic(self.config.clone())
            .map_err(|error| ExtensionError::Runtime(self.redact(&error.to_string())))?;
        let mut scope = Scope::new();
        let result = engine
            .call_fn::<Dynamic>(&mut scope, &ast, "refresh", (config,))
            .map_err(|error| self.classify(*error))?;

        if result.is_unit() {
            return Err(ExtensionError::NotAMap);
        }
        let value = rhai::serde::from_dynamic::<Value>(&result)
            .map_err(|error| ExtensionError::Runtime(self.redact(&error.to_string())))?;
        validate_output(value, self.manifest.provides, self.limits.max_items)
            .map_err(ExtensionError::Output)
    }

    fn build_engine(&self, start: Instant) -> Engine {
        let limits = self.limits.clone();
        let wall_clock = limits.wall_clock;
        let http_timeout = limits.http_timeout;
        let mut engine = Engine::new();
        // No filesystem, process, environment, or module access: drop the
        // default file-based module resolver and use the dummy resolver so
        // `import`/module loading can never touch the filesystem even if a
        // symbol slips past the disabled-keyword list.
        engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
        engine.set_max_operations(limits.max_operations);
        engine.set_max_string_size(limits.max_string_bytes);
        engine.set_max_array_size(limits.max_array_entries);
        engine.set_max_map_size(limits.max_map_entries);
        engine.set_max_call_levels(limits.max_call_levels);
        engine.set_max_expr_depths(limits.max_expr_depth, limits.max_expr_depth / 2);
        engine.set_max_modules(limits.max_modules);
        engine.set_max_functions(limits.max_functions);
        engine.set_max_variables(limits.max_variables);
        engine.set_allow_anonymous_fn(false);
        for symbol in ["eval", "print", "debug", "import", "export"] {
            engine.disable_symbol(symbol);
        }
        // Neutralise `sleep`, which would otherwise block the host thread and
        // bypass the wall-clock limit (it calls std::thread::sleep without
        // yielding to Rhai's progress callback).
        engine.register_fn("sleep", |_seconds: i64| {});
        engine.register_fn("sleep", |_seconds: f64| {});

        let policy = self.policy.clone();
        let fetcher = Arc::clone(&self.fetcher);
        let secrets = Arc::clone(&self.secret_values);
        engine.register_fn(
            "http_get_json",
            move |url: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                check_deadline(start, wall_clock)?;
                let url = policy
                    .check(url)
                    .map_err(|error| runtime_error(redact(&error.to_string(), &secrets)))?;
                let result = fetcher.get_json(
                    &url,
                    http_timeout.min(wall_clock.saturating_sub(start.elapsed())),
                );
                check_deadline(start, wall_clock)?;
                let value =
                    result.map_err(|error| runtime_error(redact(&error.to_string(), &secrets)))?;
                rhai::serde::to_dynamic(value)
                    .map_err(|error| runtime_error(redact(&error.to_string(), &secrets)))
            },
        );

        let entities = Arc::clone(&self.entities);
        let allowed_entities = self.manifest.permissions.ha_entities.clone();
        let secrets = Arc::clone(&self.secret_values);
        engine.register_fn(
            "ha_state",
            move |entity_id: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                check_deadline(start, wall_clock)?;
                if !allowed_entities.iter().any(|entity| entity == entity_id) {
                    return Err(runtime_error(format!(
                        "entity is not declared in the manifest: {entity_id}"
                    )));
                }
                match entities.get(entity_id) {
                    Some(snapshot) => {
                        let value = serde_json::json!({
                            "state": snapshot.state,
                            "attributes": snapshot.attributes,
                        });
                        rhai::serde::to_dynamic(value)
                            .map_err(|error| runtime_error(redact(&error.to_string(), &secrets)))
                    }
                    None => Ok(Dynamic::UNIT),
                }
            },
        );

        let clock = Arc::clone(&self.clock);
        let time_zone = self.time_zone;
        engine.register_fn("now", move || -> Result<String, Box<EvalAltResult>> {
            check_deadline(start, wall_clock)?;
            let value = clock.now().with_timezone(&time_zone).to_rfc3339();
            check_deadline(start, wall_clock)?;
            Ok(value)
        });

        let logs = Arc::clone(&self.logs);
        let secrets = Arc::clone(&self.secret_values);
        let max_log_entries = limits.max_log_entries;
        let max_log_bytes = limits.max_log_bytes;
        engine.register_fn(
            "log",
            move |message: &str| -> Result<(), Box<EvalAltResult>> {
                check_deadline(start, wall_clock)?;
                let message = truncate_bytes(&redact(message, &secrets), max_log_bytes);
                if let Ok(mut logs) = logs.lock() {
                    if logs.len() >= max_log_entries {
                        logs.remove(0);
                    }
                    logs.push(message.clone());
                }
                tracing::debug!(target: "ha_kiosk::extensions", "{message}");
                Ok(())
            },
        );

        engine.on_progress(move |_| {
            if start.elapsed() > wall_clock {
                Some(Dynamic::from(WALL_CLOCK_MARKER))
            } else {
                None
            }
        });

        engine
    }

    fn classify(&self, error: EvalAltResult) -> ExtensionError {
        match error {
            EvalAltResult::ErrorTooManyOperations(_) => ExtensionError::OperationLimit,
            EvalAltResult::ErrorDataTooLarge(what, _) => ExtensionError::ResourceLimit { what },
            EvalAltResult::ErrorTooManyModules(_) => ExtensionError::ModulesNotAllowed,
            EvalAltResult::ErrorStackOverflow(_) => ExtensionError::ResourceLimit {
                what: "call depth".into(),
            },
            EvalAltResult::ErrorTooManyVariables(_) => ExtensionError::ResourceLimit {
                what: "variables".into(),
            },
            EvalAltResult::ErrorTerminated(value, _)
                if value
                    .clone()
                    .into_string()
                    .is_ok_and(|message| message == WALL_CLOCK_MARKER) =>
            {
                ExtensionError::Timeout
            }
            EvalAltResult::ErrorFunctionNotFound(name, _) if name.starts_with("refresh") => {
                ExtensionError::MissingRefresh
            }
            other => ExtensionError::Runtime(self.redact(&other.to_string())),
        }
    }

    fn redact(&self, text: &str) -> String {
        redact(text, &self.secret_values)
    }
}

/// Extension host failures.
#[derive(Debug, Error)]
pub enum ExtensionError {
    #[error("manifest rejected: {0}")]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error("extension script is {size} bytes; the limit is {max}")]
    ScriptTooLarge { size: usize, max: usize },
    #[error("extension script failed to compile: {0}")]
    Compile(String),
    #[error("extension script exceeded the operation limit")]
    OperationLimit,
    #[error("extension script exceeded the {what} limit")]
    ResourceLimit { what: String },
    #[error("extension script exceeded the wall-clock limit")]
    Timeout,
    #[error("extension modules are not allowed")]
    ModulesNotAllowed,
    #[error("extension script must define fn refresh(config)")]
    MissingRefresh,
    #[error("extension script did not return a map")]
    NotAMap,
    #[error("extension script failed: {0}")]
    Runtime(String),
}

fn check_deadline(start: Instant, wall_clock: Duration) -> Result<(), Box<EvalAltResult>> {
    if start.elapsed() > wall_clock {
        Err(terminated())
    } else {
        Ok(())
    }
}

fn terminated() -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorTerminated(
        Dynamic::from(WALL_CLOCK_MARKER),
        Position::NONE,
    ))
}

fn runtime_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(message.into()),
        Position::NONE,
    ))
}

fn redact(text: &str, secrets: &[String]) -> String {
    let mut sorted: Vec<&str> = secrets
        .iter()
        .map(String::as_str)
        .filter(|secret| !secret.is_empty())
        .collect();
    sorted.sort_by_key(|secret| std::cmp::Reverse(secret.len()));

    let mut redacted = text.to_string();
    for secret in sorted {
        if redacted.contains(secret) {
            redacted = redacted.replace(secret, "[redacted]");
        }
    }
    redacted
}

fn truncate_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}
