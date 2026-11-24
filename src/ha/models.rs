//! Home Assistant entity models shared by the client and the UI state.
//!
//! The WebSocket protocol encoding is added in Wave 1.

use crate::events::EntityId;
use serde::{Deserialize, Serialize};

/// One Home Assistant entity state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaEntity {
    pub entity_id: EntityId,
    pub state: String,
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub last_changed: Option<String>,
    pub last_updated: Option<String>,
}

/// A partial state update; `None` fields were not present in the message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityChange {
    pub entity_id: EntityId,
    pub state: Option<String>,
    #[serde(default)]
    pub attributes: serde_json::Value,
}
