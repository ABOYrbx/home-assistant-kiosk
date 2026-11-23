use crate::events::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaEntity {
    pub entity_id: EntityId,
    pub state: String,
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub last_changed: Option<String>,
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityChange {
    pub entity_id: EntityId,
    pub state: Option<String>,
    #[serde(default)]
    pub attributes: serde_json::Value,
}
