#![deny(unsafe_code)]
#![allow(unused_crate_dependencies, dead_code)]

slint::include_modules!();

pub mod app;
pub mod cache;
pub mod config;
pub mod events;
pub mod extensions;
pub mod ha;
pub mod kiosk;
pub mod sources;
pub mod state;
pub mod ui;

// Re-exports for tests and external consumers.
pub use app::AppController;
pub use config::{Config, ConfigError, HaConfig};
pub use events::{AppEvent, EntityId, HaCommand, HaEvent, PendingAction, ServiceCall, ShopEvent};
pub use state::{AppState, CacheStore, ConnectionState};
