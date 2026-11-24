//! Typed events exchanged between background tasks and the UI thread.
//!
//! These names and shapes are frozen in Wave 0. Later waves may add variants
//! and fields but must not silently rename or change the meaning of existing
//! items.

use crate::ha::models::{EntityChange, HaEntity};
use crate::state::{CalendarItem, DeparturesView, MediaView, WeatherView};
use serde::{Deserialize, Serialize};

pub type EntityId = String;

/// A widget data source could not produce a view.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct SourceError(pub String);

/// Result payload of a Home Assistant service call.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceResponse {
    #[serde(default)]
    pub data: serde_json::Value,
}

/// Home Assistant client failures.
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

/// Commands sent from the UI thread to the Home Assistant task.
#[derive(Debug, Clone)]
pub enum HaCommand {
    CallService(ServiceCall),
    RefreshSnapshot,
    Shutdown,
}

/// A UI-generated service call.
#[derive(Debug, Clone)]
pub struct ServiceCall {
    pub request_id: u64,
    pub domain: String,
    pub service: String,
    pub target: Option<serde_json::Value>,
    pub data: serde_json::Value,
    pub return_response: bool,
}

/// An optimistic UI action that awaits server confirmation.
#[derive(Debug, Clone)]
pub enum PendingAction {
    ToggleTile { entity_id: EntityId },
    MediaTransport { entity_id: EntityId },
    MediaVolume { entity_id: EntityId },
}

/// Events emitted by the Home Assistant client.
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

/// Events delivered to the UI thread.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Ha(HaEvent),
    Weather(Result<WeatherView, SourceError>),
    Media(Option<MediaView>),
    Departures(Result<DeparturesView, SourceError>),
    Calendar(Result<Vec<CalendarItem>, SourceError>),
    AlbumArt {
        entity_id: EntityId,
        image: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    },
    ImageFailed {
        entity_id: EntityId,
        reason: String,
    },
}
