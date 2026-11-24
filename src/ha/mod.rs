//! Home Assistant WebSocket client, models, and reconnect policy.
//!
//! The client and reconnect loop are implemented in Wave 1.

pub mod client;
pub mod models;
pub mod reconnect;

pub use models::{EntityChange, HaEntity};
