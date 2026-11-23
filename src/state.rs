use crate::events::{EntityId, HaEntity, HaEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct CacheError(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeatherForecast {
    pub datetime: String,
    pub condition: String,
    pub temperature: Option<f64>,
    pub templow: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeatherView {
    pub entity_id: EntityId,
    pub condition: String,
    pub temperature: Option<f64>,
    pub temperature_unit: String,
    pub forecast: Vec<WeatherForecast>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DepartureItem {
    pub line: String,
    pub destination: String,
    /// The raw `departure` value from the sensor attribute.
    pub departure: String,
    /// Absolute departure time, derived from `departure` for display and caching.
    #[serde(default)]
    pub departure_utc: Option<String>,
    pub delay_minutes: Option<i64>,
    pub platform: Option<String>,
    pub cancelled: bool,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeparturesView {
    pub entity_id: EntityId,
    pub title: String,
    pub items: Vec<DepartureItem>,
    /// `false` when the entity is missing or its `departures` attribute is invalid.
    #[serde(default = "default_true_bool")]
    pub available: bool,
}

fn default_true_bool() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CalendarItem {
    pub calendar_id: EntityId,
    pub summary: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    AuthFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub pending: HashMap<u64, crate::events::PendingAction>,
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

impl AppState {
    pub fn apply_ha_event(&mut self, event: HaEvent) {
        match event {
            HaEvent::Connected {
                ha_version,
                time_zone,
            } => {
                self.connection = ConnectionState::Connected;
                self.ha_version = Some(ha_version);
                self.time_zone = time_zone;
                self.stale = false;
                self.last_success = Some(Utc::now());
            }
            HaEvent::AuthFailed { .. } => {
                self.connection = ConnectionState::AuthFailed;
            }
            HaEvent::Disconnected { .. } => {
                self.connection = ConnectionState::Disconnected;
                self.stale = true;
            }
            HaEvent::Snapshot(entities) => {
                self.entities.clear();
                for e in entities {
                    self.entities.insert(e.entity_id.clone(), e);
                }
                self.last_success = Some(Utc::now());
            }
            HaEvent::EntitiesAdded(entities) => {
                for e in entities {
                    self.entities.insert(e.entity_id.clone(), e);
                }
                self.last_success = Some(Utc::now());
            }
            HaEvent::EntitiesChanged(changes) => {
                for change in changes {
                    if let Some(entity) = self.entities.get_mut(&change.entity_id) {
                        if let Some(state) = change.state {
                            entity.state = state;
                        }
                        if change.attributes.is_object()
                            && let (Some(dst), Some(src)) = (
                                entity.attributes.as_object_mut(),
                                change.attributes.as_object(),
                            )
                        {
                            for (k, v) in src {
                                if v.is_null() {
                                    dst.remove(k);
                                } else {
                                    dst.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                self.last_success = Some(Utc::now());
            }
            HaEvent::EntitiesRemoved(ids) => {
                for id in ids {
                    self.entities.remove(&id);
                }
            }
            HaEvent::ServiceResult { request_id, .. } => {
                self.pending.remove(&request_id);
            }
        }
    }
}

pub trait CacheStore {
    fn load(&self) -> Result<Option<AppState>, CacheError>;
    fn save(&self, state: &AppState) -> Result<(), CacheError>;
}
