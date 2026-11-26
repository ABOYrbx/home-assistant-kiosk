//! Application state behind the consolidated shell.
//!
//! The shell no longer mirrors every state field into Slint properties, so
//! these assertions live at the state layer instead of against the window.

use ha_kiosk::events::HaEvent;
use ha_kiosk::ha::models::HaEntity;
use ha_kiosk::state::{AppState, ConnectionState};

fn entity(id: &str) -> HaEntity {
    HaEntity {
        entity_id: id.to_string(),
        state: "on".to_string(),
        attributes: serde_json::json!({}),
        last_changed: None,
        last_updated: None,
    }
}

#[test]
fn connection_events_track_version_timezone_and_stale() {
    let mut state = AppState::default();
    assert_eq!(state.connection, ConnectionState::Disconnected);
    assert!(state.stale);

    state.apply_ha_event(HaEvent::Connected {
        ha_version: "2026.11.0".into(),
        time_zone: chrono_tz::Tz::UTC,
    });
    assert_eq!(state.connection, ConnectionState::Connected);
    assert_eq!(state.ha_version.as_deref(), Some("2026.11.0"));
    assert_eq!(state.time_zone, chrono_tz::Tz::UTC);
    assert!(!state.stale);

    state.apply_ha_event(HaEvent::Snapshot(vec![
        entity("light.example"),
        entity("weather.home"),
    ]));
    assert_eq!(state.entities.len(), 2);
    assert!(state.last_success.is_some());

    state.apply_ha_event(HaEvent::EntitiesRemoved(vec!["light.example".into()]));
    assert_eq!(state.entities.len(), 1);
    assert!(state.entities.contains_key("weather.home"));

    state.apply_ha_event(HaEvent::Disconnected {
        reason: "socket closed".into(),
    });
    assert_eq!(state.connection, ConnectionState::Disconnected);
    assert!(state.stale, "disconnecting makes cached values stale again");
}

#[test]
fn a_restored_cache_stays_offline_until_a_fresh_batch() {
    let mut cached = AppState::default();
    cached
        .entities
        .insert("light.example".into(), entity("light.example"));

    let mut restored = AppState::restore_from_cache(cached);
    assert_eq!(restored.connection, ConnectionState::Disconnected);
    assert!(restored.stale);
    assert_eq!(restored.entities.len(), 1);

    // A reconnect alone keeps the badge; the first snapshot clears it.
    restored.apply_ha_event(HaEvent::Connected {
        ha_version: "2026.11.0".into(),
        time_zone: chrono_tz::Tz::UTC,
    });
    assert!(
        restored.stale,
        "a reconnect must not make cached values live"
    );
    restored.apply_ha_event(HaEvent::Snapshot(vec![entity("light.example")]));
    assert!(!restored.stale);
}
