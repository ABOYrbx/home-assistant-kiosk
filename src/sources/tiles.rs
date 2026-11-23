use crate::events::EntityId;
use crate::events::ServiceCall;
use crate::ha::models::HaEntity;
use serde_json::json;

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
        "climate" | "sensor" | "binary_sensor" | "weather" => TileAction::ReadOnly,
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
        TileAction::Toggle { domain, service } => Some(ServiceCall {
            request_id,
            domain: domain.clone(),
            service: service.clone(),
            target: Some(json!({"entity_id": entity_id})),
            data: json!({}),
            return_response: false,
        }),
        TileAction::Momentary { domain, service } => Some(ServiceCall {
            request_id,
            domain: domain.clone(),
            service: service.clone(),
            target: Some(json!({"entity_id": entity_id})),
            data: json!({}),
            return_response: false,
        }),
        TileAction::CoverToggle => None, // resolved by caller using state
        TileAction::ReadOnly => None,
    }
}

pub fn cover_service_for_state(state: &str) -> (&'static str, &'static str) {
    if state == "open" || state == "opening" {
        ("cover", "close_cover")
    } else {
        ("cover", "open_cover")
    }
}

pub fn tile_state_text(entity: &HaEntity) -> String {
    entity.state.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
