use crate::config::TilesConfig;
use crate::events::{EntityId, HaError, ServiceCall, ServiceResponse};
use crate::ha::models::HaEntity;
use crate::state::AppState;
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileAction {
    Toggle { domain: String, service: String },
    Momentary { domain: String, service: String },
    CoverToggle,
    ReadOnly,
}

pub fn domain_of(entity_id: &str) -> String {
    entity_id.split('.').next().unwrap_or("").to_string()
}

/// Action table from PLAN.md section 4.3. Domains without an entry stay read-only.
pub fn tile_action(entity_id: &str) -> TileAction {
    match domain_of(entity_id).as_str() {
        "switch" | "input_boolean" => TileAction::Toggle {
            domain: "homeassistant".into(),
            service: "toggle".into(),
        },
        "light" => TileAction::Toggle {
            domain: "light".into(),
            service: "toggle".into(),
        },
        "scene" => TileAction::Momentary {
            domain: "scene".into(),
            service: "turn_on".into(),
        },
        "script" => TileAction::Momentary {
            domain: "script".into(),
            service: "turn_on".into(),
        },
        "cover" => TileAction::CoverToggle,
        _ => TileAction::ReadOnly,
    }
}

pub fn is_read_only(entity_id: &str) -> bool {
    matches!(tile_action(entity_id), TileAction::ReadOnly)
}

pub fn build_service_call(
    action: &TileAction,
    entity_id: &EntityId,
    request_id: u64,
) -> Option<ServiceCall> {
    match action {
        TileAction::Toggle { domain, service } | TileAction::Momentary { domain, service } => {
            Some(ServiceCall {
                request_id,
                domain: domain.clone(),
                service: service.clone(),
                target: Some(json!({ "entity_id": entity_id })),
                data: json!({}),
                return_response: false,
            })
        }
        TileAction::CoverToggle | TileAction::ReadOnly => None,
    }
}

pub fn cover_service_for_state(state: &str) -> (&'static str, &'static str) {
    if state == "open" || state == "opening" {
        ("cover", "close_cover")
    } else {
        ("cover", "open_cover")
    }
}

fn attribute_f64(entity: &HaEntity, key: &str) -> Option<f64> {
    entity.attributes.get(key).and_then(|v| v.as_f64())
}

fn attribute_str<'a>(entity: &'a HaEntity, key: &str) -> Option<&'a str> {
    entity.attributes.get(key).and_then(|v| v.as_str())
}

/// State text for a tile. Sensors include their unit so the value is readable
/// without opening a detail view. Unavailable and unknown states pass through
/// unchanged instead of being masked by a stale value.
pub fn tile_state_text(entity: &HaEntity) -> String {
    if domain_of(&entity.entity_id) == "sensor"
        && let Some(unit) = attribute_str(entity, "unit_of_measurement")
        && !unit.is_empty()
    {
        return format!("{} {}", entity.state, unit);
    }
    entity.state.clone()
}

/// Attributes that fit on a tile, as a short secondary line.
pub fn tile_detail(entity: &HaEntity) -> String {
    let mut parts: Vec<String> = Vec::new();
    match domain_of(&entity.entity_id).as_str() {
        "light" if entity.state == "on" => {
            if let Some(brightness) = attribute_f64(entity, "brightness") {
                parts.push(format!("{}%", (brightness / 255.0 * 100.0).round() as i64));
            }
            if let Some(kelvin) = attribute_f64(entity, "color_temp_kelvin") {
                parts.push(format!("{} K", kelvin.round() as i64));
            }
        }
        "cover" => {
            if let Some(position) = attribute_f64(entity, "current_position") {
                parts.push(format!("{}%", position.round() as i64));
            }
        }
        "climate" => {
            if let Some(current) = attribute_f64(entity, "current_temperature") {
                parts.push(format!("now {current}"));
            }
            if let Some(target) = attribute_f64(entity, "temperature") {
                parts.push(format!("set {target}"));
            }
        }
        _ => {}
    }
    parts.join(" · ")
}

/// One tile as the UI renders it. Free of Slint types so it stays testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileView {
    pub entity_id: EntityId,
    pub label: String,
    pub icon: String,
    pub state: String,
    pub detail: String,
    pub read_only: bool,
    pub pending: bool,
    pub error: Option<String>,
}

/// Owns the configured tiles, their rendered state, and the single in-flight
/// service call per entity. Lives on the UI thread.
pub struct TileBoard {
    columns: u32,
    tiles: Vec<TileView>,
    inflight: HashMap<EntityId, u64>,
}

impl TileBoard {
    pub fn new(config: &TilesConfig) -> Self {
        let tiles = config
            .items
            .iter()
            .map(|item| TileView {
                entity_id: item.entity.clone(),
                label: if item.label.trim().is_empty() {
                    item.entity.clone()
                } else {
                    item.label.clone()
                },
                icon: item.icon.clone(),
                state: "unavailable".to_string(),
                detail: String::new(),
                read_only: is_read_only(&item.entity),
                pending: false,
                error: None,
            })
            .collect();
        Self {
            columns: config.columns.max(1),
            tiles,
            inflight: HashMap::new(),
        }
    }

    pub fn columns(&self) -> u32 {
        self.columns
    }

    pub fn tiles(&self) -> &[TileView] {
        &self.tiles
    }

    pub fn tile(&self, entity_id: &str) -> Option<&TileView> {
        self.tiles.iter().find(|t| t.entity_id == entity_id)
    }

    /// Rebuild state and attribute text from the latest Home Assistant state.
    /// Pending flags and errors survive a refresh so an update does not hide them.
    pub fn refresh(&mut self, state: &AppState) {
        for tile in &mut self.tiles {
            match state.entities.get(&tile.entity_id) {
                Some(entity) => {
                    tile.state = tile_state_text(entity);
                    tile.detail = tile_detail(entity);
                }
                None => {
                    tile.state = "unavailable".to_string();
                    tile.detail.clear();
                }
            }
            tile.read_only = is_read_only(&tile.entity_id);
            tile.pending = self.inflight.contains_key(&tile.entity_id);
        }
    }

    /// Start a tap action. Returns `None` for read-only tiles, unknown entities,
    /// and while a call for the same entity is still in flight.
    pub fn tap(
        &mut self,
        entity_id: &str,
        state: &AppState,
        request_id: u64,
    ) -> Option<ServiceCall> {
        let tile = self.tiles.iter_mut().find(|t| t.entity_id == entity_id)?;
        if tile.read_only || tile.pending || self.inflight.contains_key(entity_id) {
            return None;
        }
        let entity = state.entities.get(entity_id)?;
        if matches!(entity.state.as_str(), "unavailable" | "unknown") {
            return None;
        }
        let action = tile_action(entity_id);
        let call = match action {
            TileAction::CoverToggle => {
                let (domain, service) = cover_service_for_state(&entity.state);
                Some(ServiceCall {
                    request_id,
                    domain: domain.to_string(),
                    service: service.to_string(),
                    target: Some(json!({ "entity_id": entity_id })),
                    data: json!({}),
                    return_response: false,
                })
            }
            other => build_service_call(&other, &entity_id.to_string(), request_id),
        }?;
        self.inflight.insert(entity_id.to_string(), request_id);
        tile.pending = true;
        tile.error = None;
        Some(call)
    }

    /// Apply a service result. Returns the entity the request belonged to.
    pub fn on_result(
        &mut self,
        request_id: u64,
        result: &Result<ServiceResponse, HaError>,
    ) -> Option<EntityId> {
        let entity_id = self
            .inflight
            .iter()
            .find(|(_, id)| **id == request_id)
            .map(|(entity, _)| entity.clone())?;
        self.inflight.remove(&entity_id);
        let tile = self.tiles.iter_mut().find(|t| t.entity_id == entity_id)?;
        tile.pending = false;
        match result {
            Ok(_) => tile.error = None,
            Err(e) => tile.error = Some(error_text(&e.to_string())),
        }
        Some(entity_id)
    }

    /// Drop every in-flight call, for example after a disconnect where no
    /// service result will arrive.
    pub fn reset_pending(&mut self) {
        self.inflight.clear();
        for tile in &mut self.tiles {
            tile.pending = false;
        }
    }

    pub fn set_error(&mut self, entity_id: &str, message: &str) {
        if let Some(tile) = self.tiles.iter_mut().find(|t| t.entity_id == entity_id) {
            tile.pending = false;
            tile.error = Some(error_text(message));
        }
    }
}

fn error_text(message: &str) -> String {
    const MAX_CHARS: usize = 120;
    let trimmed = message.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    let short: String = trimmed.chars().take(MAX_CHARS).collect();
    format!("{short}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TileConfig;

    fn config(entities: &[&str]) -> TilesConfig {
        TilesConfig {
            columns: 2,
            items: entities
                .iter()
                .map(|entity| TileConfig {
                    entity: entity.to_string(),
                    label: String::new(),
                    icon: String::new(),
                })
                .collect(),
        }
    }

    fn entity(entity_id: &str, state: &str, attributes: serde_json::Value) -> HaEntity {
        HaEntity {
            entity_id: entity_id.to_string(),
            state: state.to_string(),
            attributes,
            last_changed: None,
            last_updated: None,
        }
    }

    #[test]
    fn action_mapping() {
        assert!(matches!(tile_action("switch.a"), TileAction::Toggle { .. }));
        assert!(matches!(tile_action("light.a"), TileAction::Toggle { .. }));
        assert!(matches!(
            tile_action("scene.a"),
            TileAction::Momentary { .. }
        ));
        assert!(matches!(
            tile_action("script.a"),
            TileAction::Momentary { .. }
        ));
        assert!(matches!(tile_action("cover.a"), TileAction::CoverToggle));
        assert!(matches!(tile_action("climate.a"), TileAction::ReadOnly));
        assert!(matches!(tile_action("sensor.a"), TileAction::ReadOnly));
        assert!(matches!(tile_action("foo.bar"), TileAction::ReadOnly));
    }

    #[test]
    fn cover_state() {
        assert_eq!(cover_service_for_state("open").1, "close_cover");
        assert_eq!(cover_service_for_state("closed").1, "open_cover");
    }

    #[test]
    fn missing_entity_is_unavailable() {
        let mut board = TileBoard::new(&config(&["switch.example"]));
        board.refresh(&AppState::default());
        let tile = board.tile("switch.example").unwrap();
        assert_eq!(tile.state, "unavailable");
        assert!(!tile.pending);
    }

    #[test]
    fn rapid_taps_keep_one_call_in_flight() {
        let mut board = TileBoard::new(&config(&["switch.example"]));
        let mut state = AppState::default();
        state.entities.insert(
            "switch.example".into(),
            entity("switch.example", "off", json!({})),
        );
        board.refresh(&state);

        assert!(board.tap("switch.example", &state, 1).is_some());
        assert!(board.tap("switch.example", &state, 2).is_none());
        assert!(board.tile("switch.example").unwrap().pending);

        let done: Result<ServiceResponse, HaError> = Ok(ServiceResponse::default());
        assert_eq!(board.on_result(1, &done).as_deref(), Some("switch.example"));
        assert!(!board.tile("switch.example").unwrap().pending);
        assert!(board.tap("switch.example", &state, 3).is_some());
    }

    #[test]
    fn failed_call_surfaces_error() {
        let mut board = TileBoard::new(&config(&["light.example"]));
        let mut state = AppState::default();
        state.entities.insert(
            "light.example".into(),
            entity("light.example", "off", json!({})),
        );
        board.refresh(&state);

        assert!(board.tap("light.example", &state, 7).is_some());
        let failed: Result<ServiceResponse, HaError> = Err(HaError::Service("boom".into()));
        board.on_result(7, &failed);
        let tile = board.tile("light.example").unwrap();
        assert!(!tile.pending);
        assert!(tile.error.as_deref().is_some_and(|e| e.contains("boom")));
    }

    #[test]
    fn read_only_tiles_ignore_taps() {
        let mut board = TileBoard::new(&config(&["sensor.example", "vendor.custom"]));
        let state = AppState::default();
        board.refresh(&state);
        assert!(board.tile("sensor.example").unwrap().read_only);
        assert!(board.tile("vendor.custom").unwrap().read_only);
        assert!(board.tap("sensor.example", &state, 1).is_none());
        assert!(board.tap("vendor.custom", &state, 2).is_none());
    }

    #[test]
    fn missing_or_unavailable_entities_ignore_taps() {
        let mut board = TileBoard::new(&config(&["switch.example", "cover.example"]));
        let mut state = AppState::default();
        assert!(board.tap("switch.example", &state, 1).is_none());
        state.entities.insert(
            "cover.example".into(),
            entity("cover.example", "unavailable", json!({})),
        );
        board.refresh(&state);
        assert!(board.tap("cover.example", &state, 2).is_none());
    }

    #[test]
    fn disconnect_clears_pending() {
        let mut board = TileBoard::new(&config(&["switch.example"]));
        let mut state = AppState::default();
        state.entities.insert(
            "switch.example".into(),
            entity("switch.example", "off", json!({})),
        );
        board.refresh(&state);
        assert!(board.tap("switch.example", &state, 9).is_some());
        board.reset_pending();
        assert!(!board.tile("switch.example").unwrap().pending);
        assert!(board.tap("switch.example", &state, 10).is_some());
    }

    #[test]
    fn cover_action_follows_current_state() {
        let mut board = TileBoard::new(&config(&["cover.example"]));
        let mut state = AppState::default();
        state.entities.insert(
            "cover.example".into(),
            entity("cover.example", "open", json!({})),
        );
        board.refresh(&state);
        let call = board.tap("cover.example", &state, 11).unwrap();
        assert_eq!(call.domain, "cover");
        assert_eq!(call.service, "close_cover");
    }

    #[test]
    fn attributes_are_bound() {
        let mut board = TileBoard::new(&config(&[
            "light.example",
            "sensor.example_temperature",
            "climate.example",
        ]));
        let mut state = AppState::default();
        state.entities.insert(
            "light.example".into(),
            entity(
                "light.example",
                "on",
                json!({ "brightness": 153, "color_temp_kelvin": 3000 }),
            ),
        );
        state.entities.insert(
            "sensor.example_temperature".into(),
            entity(
                "sensor.example_temperature",
                "21.5",
                json!({ "unit_of_measurement": "°C" }),
            ),
        );
        state.entities.insert(
            "climate.example".into(),
            entity(
                "climate.example",
                "heat",
                json!({ "current_temperature": 20.5, "temperature": 22.0 }),
            ),
        );
        board.refresh(&state);

        let light = board.tile("light.example").unwrap();
        assert_eq!(light.state, "on");
        assert_eq!(light.detail, "60% · 3000 K");

        let sensor = board.tile("sensor.example_temperature").unwrap();
        assert_eq!(sensor.state, "21.5 °C");

        let climate = board.tile("climate.example").unwrap();
        assert_eq!(climate.detail, "now 20.5 · set 22");
    }
}
