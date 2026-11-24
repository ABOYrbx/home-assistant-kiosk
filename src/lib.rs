#![deny(unsafe_code)]
//! Public library surface for `ha-kiosk`.
//!
//! Wave 0 freezes the module layout and the interfaces between waves. The
//! types here compile, but loading, networking, caching, and UI behaviour are
//! implemented in later waves.

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

pub use config::{Config, ConfigError, HaConfig};
pub use events::{AppEvent, EntityId, HaCommand, HaEvent, PendingAction, ServiceCall};
pub use state::{AppState, CacheStore, ConnectionState};
