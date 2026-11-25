use ha_kiosk::events::{EntityChange, HaEntity, HaEvent, PendingAction};
use ha_kiosk::state::{AppState, ConnectionState};
use serde_json::json;

fn entity(id: &str, state: &str, attributes: serde_json::Value) -> HaEntity {
    HaEntity {
        entity_id: id.to_string(),
        state: state.to_string(),
        attributes,
        last_changed: None,
        last_updated: None,
    }
}

#[test]
fn connection_events_track_version_timezone_and_stale() {
    let mut state = AppState::default();
    assert!(state.stale);
    assert_eq!(state.connection, ConnectionState::Disconnected);

    state.apply_ha_event(HaEvent::Connected {
        ha_version: "2024.2.0".to_string(),
        time_zone: chrono_tz::Europe::Berlin,
    });
    assert_eq!(state.connection, ConnectionState::Connected);
    assert_eq!(state.ha_version.as_deref(), Some("2024.2.0"));
    assert_eq!(state.time_zone, chrono_tz::Europe::Berlin);
    assert!(!state.stale);

    state.apply_ha_event(HaEvent::Disconnected {
        reason: "socket closed".to_string(),
    });
    assert_eq!(state.connection, ConnectionState::Disconnected);
    assert!(state.stale);

    state.apply_ha_event(HaEvent::AuthFailed {
        message: "bad token".to_string(),
    });
    assert_eq!(state.connection, ConnectionState::AuthFailed);
}

#[test]
fn authentication_failure_marks_cached_data_as_stale() {
    let mut state = AppState {
        stale: false,
        ..AppState::default()
    };

    state.apply_ha_event(HaEvent::AuthFailed {
        message: "bad token".to_string(),
    });

    assert_eq!(state.connection, ConnectionState::AuthFailed);
    assert!(
        state.stale,
        "an authentication failure must not let cached data look live"
    );
}

#[test]
fn snapshot_replaces_all_entities_and_marks_fresh() {
    let mut state = AppState {
        stale: true,
        ..AppState::default()
    };
    state.entities.insert(
        "light.stale".to_string(),
        entity("light.stale", "on", json!({})),
    );

    state.apply_ha_event(HaEvent::Snapshot(vec![entity(
        "light.fresh",
        "on",
        json!({"brightness": 5}),
    )]));

    assert!(!state.entities.contains_key("light.stale"));
    assert_eq!(state.entities.len(), 1);
    assert_eq!(state.entities["light.fresh"].state, "on");
    assert!(!state.stale);
    assert!(state.last_success.is_some());
}

#[test]
fn changes_merge_attributes_and_remove_null_attributes() {
    let mut state = AppState::default();
    state.apply_ha_event(HaEvent::Snapshot(vec![entity(
        "light.example",
        "on",
        json!({"brightness": 100, "color_mode": "hs"}),
    )]));

    state.apply_ha_event(HaEvent::EntitiesChanged(vec![EntityChange {
        entity_id: "light.example".to_string(),
        state: Some("off".to_string()),
        attributes: json!({"brightness": null, "friendly_name": "Lamp"}),
    }]));

    let light = &state.entities["light.example"];
    assert_eq!(light.state, "off");
    assert!(light.attributes.get("brightness").is_none());
    assert_eq!(light.attributes["color_mode"], json!("hs"));
    assert_eq!(light.attributes["friendly_name"], json!("Lamp"));
}

#[test]
fn added_entities_are_upserted_and_removed_entities_are_dropped() {
    let mut state = AppState::default();
    state.apply_ha_event(HaEvent::EntitiesAdded(vec![entity(
        "sensor.new",
        "1",
        json!({}),
    )]));
    assert_eq!(state.entities["sensor.new"].state, "1");

    state.apply_ha_event(HaEvent::EntitiesRemoved(vec!["sensor.new".to_string()]));
    assert!(state.entities.is_empty());
}

#[test]
fn service_result_clears_pending_action() {
    let mut state = AppState::default();
    state.pending.insert(
        7,
        PendingAction::ToggleTile {
            entity_id: "light.example".to_string(),
        },
    );

    state.apply_ha_event(HaEvent::ServiceResult {
        request_id: 7,
        result: Ok(Default::default()),
    });

    assert!(state.pending.is_empty());
}

#[test]
fn reconnect_keeps_cached_data_stale_until_the_first_fresh_batch() {
    // A snapshot from the previous session, then a restore before the network
    // is back: the data is cached and must not look live.
    let mut previous = AppState::default();
    previous.apply_ha_event(HaEvent::Snapshot(vec![
        entity("light.cached", "on", json!({})),
        entity("sensor.gone", "42", json!({})),
    ]));
    assert!(!previous.stale);

    let mut state = AppState::restore_from_cache(previous);
    state.apply_ha_event(HaEvent::Connected {
        ha_version: "2024.2.0".to_string(),
        time_zone: chrono_tz::Europe::Berlin,
    });
    assert_eq!(state.connection, ConnectionState::Connected);
    assert!(
        state.stale,
        "a handshake must not present cached data as live"
    );

    // The first batch from the new connection replaces the restored snapshot,
    // including entities that no longer exist.
    state.apply_ha_event(HaEvent::EntitiesAdded(vec![entity(
        "light.cached",
        "off",
        json!({}),
    )]));
    assert!(!state.stale, "fresh data clears the stale badge");
    assert_eq!(state.entities.len(), 1);
    assert_eq!(state.entities["light.cached"].state, "off");
    assert!(!state.entities.contains_key("sensor.gone"));
}
