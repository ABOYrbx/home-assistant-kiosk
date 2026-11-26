//! Tile board controller behaviour behind the consolidated Overview page.
//!
//! The shell no longer exposes tile models or an `AppController::tap_tile`
//! shortcut. Tile input is handled by `TileBoard`, which is exercised here
//! with a recorded Home Assistant state.

use ha_kiosk::config::{TileConfig, TilesConfig};
use ha_kiosk::events::{HaError, ServiceResponse};
use ha_kiosk::ha::models::HaEntity;
use ha_kiosk::sources::tiles::TileBoard;
use ha_kiosk::state::AppState;

fn config() -> TilesConfig {
    TilesConfig {
        columns: 2,
        items: vec![
            TileConfig {
                entity: "switch.example".into(),
                label: "Example switch".into(),
                icon: String::new(),
            },
            TileConfig {
                entity: "sensor.example".into(),
                label: "Example sensor".into(),
                icon: String::new(),
            },
        ],
    }
}

fn entity(id: &str, state: &str) -> HaEntity {
    HaEntity {
        entity_id: id.to_string(),
        state: state.to_string(),
        attributes: serde_json::json!({}),
        last_changed: None,
        last_updated: None,
    }
}

fn state() -> AppState {
    let mut state = AppState::default();
    state
        .entities
        .insert("switch.example".into(), entity("switch.example", "on"));
    state
        .entities
        .insert("sensor.example".into(), entity("sensor.example", "42"));
    state
}

#[test]
fn a_tap_sends_one_call_and_a_failed_result_allows_a_retry() {
    let mut board = TileBoard::new(&config());
    board.refresh(&state());
    assert_eq!(board.columns(), 2);
    assert_eq!(board.tiles().len(), 2);

    let call = board
        .tap("switch.example", &state(), 1)
        .expect("a service call");
    assert_eq!(call.domain, "homeassistant");
    assert_eq!(call.service, "toggle");
    assert!(board.tile("switch.example").unwrap().pending);
    assert!(
        board.tap("switch.example", &state(), 2).is_none(),
        "a second tap while pending must be ignored"
    );

    let failed: Result<ServiceResponse, HaError> = Err(HaError::Service("boom".into()));
    assert_eq!(
        board.on_result(1, &failed).as_deref(),
        Some("switch.example")
    );
    assert!(!board.tile("switch.example").unwrap().pending);
    assert!(board.tile("switch.example").unwrap().error.is_some());

    let retry = board.tap("switch.example", &state(), 3).expect("a retry");
    assert_eq!(retry.request_id, 3);
    let ok: Result<ServiceResponse, HaError> = Ok(ServiceResponse::default());
    board.on_result(3, &ok);
    assert!(board.tile("switch.example").unwrap().error.is_none());
}

#[test]
fn read_only_tiles_never_emit_a_service_call() {
    let mut board = TileBoard::new(&config());
    board.refresh(&state());
    assert!(board.tile("sensor.example").unwrap().read_only);
    assert!(board.tap("sensor.example", &state(), 1).is_none());
    assert!(board.tap("does.not.exist", &state(), 2).is_none());
}
