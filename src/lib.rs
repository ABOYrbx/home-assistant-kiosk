#![deny(unsafe_code)]

pub mod app;
pub mod cache;
pub mod config;
pub mod events;
pub mod ha;
pub mod kiosk;
pub mod sources;
pub mod state;
pub mod ui;

// Re-exports for tests and external consumers.
pub use config::{Config, ConfigError, HaConfig};
pub use events::{AppEvent, EntityId, HaCommand, HaEvent, PendingAction, ServiceCall};
pub use state::{AppState, CacheStore, ConnectionState};
