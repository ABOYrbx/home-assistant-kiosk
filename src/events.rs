pub use crate::ha::models::{EntityChange, HaEntity};
use serde::{Deserialize, Serialize};

pub type EntityId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    AuthFailed,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct SourceError(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceResponse {
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum HaError {
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("connection error: {0}")]
    Connection(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("service error: {0}")]
    Service(String),
}

// re-exported above

#[derive(Debug, Clone)]
pub enum HaCommand {
    CallService(ServiceCall),
    RefreshSnapshot,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct ServiceCall {
    pub request_id: u64,
    pub domain: String,
    pub service: String,
    pub target: Option<serde_json::Value>,
    pub data: serde_json::Value,
    pub return_response: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingAction {
    ToggleTile { entity_id: EntityId },
    MediaTransport { entity_id: EntityId },
    MediaVolume { entity_id: EntityId },
    HeroToggle { entity_id: EntityId },
    HeroBrightness { entity_id: EntityId },
    HeroColorTemp { entity_id: EntityId },
}

#[derive(Debug, Clone)]
pub enum HaEvent {
    Connected {
        ha_version: String,
        time_zone: chrono_tz::Tz,
    },
    AuthFailed {
        message: String,
    },
    Disconnected {
        reason: String,
    },
    Snapshot(Vec<HaEntity>),
    EntitiesAdded(Vec<HaEntity>),
    EntitiesChanged(Vec<EntityChange>),
    EntitiesRemoved(Vec<EntityId>),
    ServiceResult {
        request_id: u64,
        result: Result<ServiceResponse, HaError>,
    },
}

/// Results from the Wave 6 extension store and host.
#[derive(Debug, Clone)]
pub enum ShopEvent {
    InstalledLoaded(Vec<crate::extensions::InstalledExtension>),
    CatalogLoaded(Vec<crate::extensions::ShopEntry>),
    CatalogOffline {
        reason: String,
    },
    ActionResult {
        request_id: u64,
        id: String,
        action: crate::extensions::ShopAction,
        result: Result<(), String>,
    },
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Ha(HaEvent),
    Shop(ShopEvent),
    Weather(Result<crate::state::WeatherView, SourceError>),
    Media(Option<crate::state::MediaView>),
    Departures(Result<crate::state::DeparturesView, SourceError>),
    Calendar(Result<crate::state::CalendarView, SourceError>),
    AlbumArt {
        entity_id: EntityId,
        image: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    },
    ImageFailed {
        entity_id: EntityId,
        reason: String,
    },
}

// Keep a stable import path for the HashMap type used by state.
pub use std::collections::HashMap as EntityMap;
