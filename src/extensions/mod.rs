//! Sandboxed extension host (Wave 6, post-v1).
//!
//! Extensions are untrusted third-party Rhai data scripts. The host builds a
//! Rhai engine without filesystem, process, environment, or module access and
//! registers only four functions: `http_get_json`, `ha_state`, `now`, and
//! `log`.
//!
//! The host enforces the manifest allowlists, resource limits, and the
//! declarative output schema. Callers must run [`ExtensionRunner::refresh`] off
//! the UI thread; it performs blocking HTTP and script work.

pub mod host;
pub mod http;
pub mod manifest;
pub mod output;
pub mod runner;

pub use host::{
    Clock, EntitySnapshot, ExtensionError, ExtensionHost, HostConfig, Limits, SystemClock,
};
pub use http::{HttpError, HttpFetcher, HttpPolicy, ReqwestFetcher};
pub use manifest::{
    ConfigField, ConfigFieldType, Manifest, ManifestError, OutputKind, Permissions,
};
pub use output::{
    DepartureItem, DeparturesView, ExtensionOutput, KeyValueItem, KeyValueView, ListView,
    OutputError,
};
pub use runner::{ExtensionRunner, ExtensionStatus};
