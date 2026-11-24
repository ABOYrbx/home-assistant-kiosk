//! Application state, view models, and the cache interface.
//!
//! Wave 0 freezes the types. State transitions and cache persistence are
//! implemented in Wave 1.

use crate::events::{EntityId, PendingAction};
use crate::ha::models::HaEntity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Offline cache failures.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct CacheError(pub String);

/// One forecast period from a weather entity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeatherForecast {
    pub datetime: String,
    pub condition: String,
    pub temperature: Option<f64>,
    pub templow: Option<f64>,
}

/// View model for the weather widget.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeatherView {
    pub entity_id: EntityId,
    pub condition: String,
    pub temperature: Option<f64>,
    pub temperature_unit: String,
    pub forecast: Vec<WeatherForecast>,
}

/// View model for the media player widget.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaView {
    pub entity_id: EntityId,
    pub state: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub entity_picture: Option<String>,
    pub volume_level: Option<f64>,
    pub supported_features: u64,
}

/// One row of the departures widget.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DepartureItem {
    pub line: String,
    pub destination: String,
    pub departure: String,
    pub delay_minutes: Option<i64>,
    pub platform: Option<String>,
    pub cancelled: bool,
    pub remarks: Option<String>,
}

/// View model for the departures widget.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeparturesView {
    pub entity_id: EntityId,
    pub title: String,
    pub items: Vec<DepartureItem>,
}

/// One entry of the agenda page.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CalendarItem {
    pub calendar_id: EntityId,
    pub summary: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
}

/// Connection state shown by the status page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    AuthFailed,
}

/// All state owned by the UI thread.
#[derive(Debug, Clone)]
pub struct AppState {
    pub connection: ConnectionState,
    pub ha_version: Option<String>,
    pub time_zone: chrono_tz::Tz,
    pub entities: HashMap<EntityId, HaEntity>,
    pub weather: Option<WeatherView>,
    pub media: Option<MediaView>,
    pub departures: Option<DeparturesView>,
    pub calendar: Vec<CalendarItem>,
    pub stale: bool,
    pub last_success: Option<DateTime<Utc>>,
    pub pending: HashMap<u64, PendingAction>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            connection: ConnectionState::Disconnected,
            ha_version: None,
            time_zone: chrono_tz::Tz::UTC,
            entities: HashMap::new(),
            weather: None,
            media: None,
            departures: None,
            calendar: Vec::new(),
            stale: true,
            last_success: None,
            pending: HashMap::new(),
        }
    }
}

/// Persistence boundary for the versioned snapshot cache.
pub trait CacheStore {
    fn load(&self) -> Result<Option<AppState>, CacheError>;
    fn save(&self, state: &AppState) -> Result<(), CacheError>;
}
