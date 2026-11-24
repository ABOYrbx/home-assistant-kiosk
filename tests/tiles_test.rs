use ha_kiosk::config::{TileConfig, TilesConfig};
use ha_kiosk::events::{HaEntity, HaError, ServiceCall, ServiceResponse};
use ha_kiosk::sources::tiles::{
    TileAction, TileBoard, build_service_call, cover_service_for_state, is_read_only, tile_action,
};
use ha_kiosk::state::AppState;
use serde_json::json;

fn tiles_config(entities: &[&str]) -> TilesConfig {
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

fn state_with(entities: Vec<HaEntity>) -> AppState {
    let mut state = AppState::default();
    for e in entities {
        state.entities.insert(e.entity_id.clone(), e);
    }
    state
}

fn service_call(entity_id: &str) -> Option<ServiceCall> {
    let action = tile_action(entity_id);
    build_service_call(&action, &entity_id.to_string(), 1)
}

#[test]
fn action_table_maps_each_supported_domain() {
    let switch = service_call("switch.example").unwrap();
    assert_eq!(switch.domain, "homeassistant");
    assert_eq!(switch.service, "toggle");

    let boolean = service_call("input_boolean.example").unwrap();
    assert_eq!(boolean.domain, "homeassistant");
    assert_eq!(boolean.service, "toggle");

    let light = service_call("light.example").unwrap();
    assert_eq!(light.domain, "light");
    assert_eq!(light.service, "toggle");

    let scene = service_call("scene.example").unwrap();
    assert_eq!(scene.domain, "scene");
    assert_eq!(scene.service, "turn_on");

    let script = service_call("script.example").unwrap();
    assert_eq!(script.domain, "script");
    assert_eq!(script.service, "turn_on");

    // Cover actions depend on the current state and are built by the board.
    assert!(matches!(
        tile_action("cover.example"),
        TileAction::CoverToggle
    ));
    assert_eq!(cover_service_for_state("open"), ("cover", "close_cover"));
    assert_eq!(cover_service_for_state("opening"), ("cover", "close_cover"));
    assert_eq!(cover_service_for_state("closed"), ("cover", "open_cover"));
    assert_eq!(cover_service_for_state("closing"), ("cover", "open_cover"));

    for read_only in [
        "climate.example",
        "sensor.example",
        "binary_sensor.example",
        "weather.home",
        "camera.example",
        "vendor.custom",
    ] {
        assert!(is_read_only(read_only), "{read_only} must stay read-only");
        assert!(service_call(read_only).is_none());
    }
}

#[test]
fn unknown_domain_stays_read_only() {
    let mut board = TileBoard::new(&tiles_config(&["vendor.custom"]));
    let state = state_with(vec![entity("vendor.custom", "on", json!({}))]);
    board.refresh(&state);

    let tile = board.tile("vendor.custom").unwrap();
    assert!(tile.read_only);
    assert_eq!(tile.state, "on");
    assert!(board.tap("vendor.custom", &state, 1).is_none());
}

#[test]
fn missing_entity_and_unknown_state_are_shown_honestly() {
    let mut board = TileBoard::new(&tiles_config(&["switch.missing", "switch.unknown"]));
    let state = state_with(vec![entity("switch.unknown", "unknown", json!({}))]);
    board.refresh(&state);

    assert_eq!(board.tile("switch.missing").unwrap().state, "unavailable");
    assert_eq!(board.tile("switch.unknown").unwrap().state, "unknown");
}

#[test]
fn failed_service_call_surfaces_error_and_recovers() {
    let mut board = TileBoard::new(&tiles_config(&["light.example"]));
    let state = state_with(vec![entity("light.example", "off", json!({}))]);
    board.refresh(&state);

    let call = board.tap("light.example", &state, 42).unwrap();
    assert_eq!(call.request_id, 42);
    assert!(board.tile("light.example").unwrap().pending);

    let failed: Result<ServiceResponse, HaError> =
        Err(HaError::Service("permission denied".into()));
    assert_eq!(
        board.on_result(42, &failed).as_deref(),
        Some("light.example")
    );
    let tile = board.tile("light.example").unwrap();
    assert!(!tile.pending);
    assert!(
        tile.error
            .as_deref()
            .is_some_and(|e| e.contains("permission denied"))
    );

    // A later successful call clears the error.
    let retry = board.tap("light.example", &state, 43).unwrap();
    assert_eq!(retry.service, "toggle");
    let ok: Result<ServiceResponse, HaError> = Ok(ServiceResponse::default());
    board.on_result(43, &ok);
    assert!(board.tile("light.example").unwrap().error.is_none());
}

#[test]
fn rapid_taps_produce_at_most_one_in_flight_call() {
    let mut board = TileBoard::new(&tiles_config(&["switch.example"]));
    let state = state_with(vec![entity("switch.example", "off", json!({}))]);
    board.refresh(&state);

    let first = board.tap("switch.example", &state, 100).unwrap();
    assert_eq!(first.request_id, 100);
    for request_id in 101..110 {
        assert!(
            board.tap("switch.example", &state, request_id).is_none(),
            "tap {request_id} must be ignored while a call is in flight"
        );
    }

    let ok: Result<ServiceResponse, HaError> = Ok(ServiceResponse::default());
    board.on_result(100, &ok);
    assert!(board.tap("switch.example", &state, 110).is_some());
}

#[test]
fn pending_survives_a_state_refresh() {
    let mut board = TileBoard::new(&tiles_config(&["switch.example"]));
    let state = state_with(vec![entity("switch.example", "off", json!({}))]);
    board.refresh(&state);
    board.tap("switch.example", &state, 5);

    let updated = state_with(vec![entity("switch.example", "on", json!({}))]);
    board.refresh(&updated);
    let tile = board.tile("switch.example").unwrap();
    assert_eq!(tile.state, "on");
    assert!(tile.pending);
}

#[test]
fn state_and_supported_attributes_are_bound() {
    let mut board = TileBoard::new(&tiles_config(&[
        "light.example",
        "sensor.example_temperature",
        "cover.example",
        "climate.example",
    ]));
    let state = state_with(vec![
        entity(
            "light.example",
            "on",
            json!({ "brightness": 255, "color_temp_kelvin": 2700 }),
        ),
        entity(
            "sensor.example_temperature",
            "19.5",
            json!({ "unit_of_measurement": "°C" }),
        ),
        entity("cover.example", "open", json!({ "current_position": 40 })),
        entity(
            "climate.example",
            "heat",
            json!({ "current_temperature": 19.0, "temperature": 21.5 }),
        ),
    ]);
    board.refresh(&state);

    let light = board.tile("light.example").unwrap();
    assert_eq!(light.state, "on");
    assert_eq!(light.detail, "100% · 2700 K");

    let sensor = board.tile("sensor.example_temperature").unwrap();
    assert_eq!(sensor.state, "19.5 °C");

    let cover = board.tile("cover.example").unwrap();
    assert_eq!(cover.state, "open");
    assert_eq!(cover.detail, "40%");

    let climate = board.tile("climate.example").unwrap();
    assert_eq!(climate.detail, "now 19 · set 21.5");
}

#[test]
fn grid_columns_order_and_labels_come_from_config() {
    let config = TilesConfig {
        columns: 4,
        items: vec![
            TileConfig {
                entity: "switch.example".into(),
                label: "Porch".into(),
                icon: "switch".into(),
            },
            TileConfig {
                entity: "light.example".into(),
                label: String::new(),
                icon: String::new(),
            },
        ],
    };
    let board = TileBoard::new(&config);

    assert_eq!(board.columns(), 4);
    let tiles = board.tiles();
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[0].entity_id, "switch.example");
    assert_eq!(tiles[0].label, "Porch");
    assert_eq!(tiles[0].icon, "switch");
    assert_eq!(tiles[1].label, "light.example");
}

#[test]
fn zero_columns_falls_back_to_a_single_column() {
    let config = TilesConfig {
        columns: 0,
        items: vec![TileConfig {
            entity: "switch.example".into(),
            label: String::new(),
            icon: String::new(),
        }],
    };
    assert_eq!(TileBoard::new(&config).columns(), 1);
}

#[test]
fn results_for_unknown_requests_are_ignored() {
    let mut board = TileBoard::new(&tiles_config(&["switch.example"]));
    let ok: Result<ServiceResponse, HaError> = Ok(ServiceResponse::default());
    assert!(board.on_result(999, &ok).is_none());
}
