//! End-to-end: TOML configuration -> WebSocket -> snapshot -> UI view models.
//!
//! The production client connects to the shared mock Home Assistant server in
//! `tests/support/mock_ha.rs`. The test feeds the events into `AppState` (the
//! state the Slint thread owns) and builds the same view models the Overview
//! renders: the top-bar connection badge, the hero light card, and the tile
//! row. No Home Assistant instance and no display are required.

mod support;

use ha_kiosk::config::Config;
use ha_kiosk::events::HaEvent;
use ha_kiosk::ha;
use ha_kiosk::sources::tiles;
use ha_kiosk::state::{AppState, ConnectionState};
use ha_kiosk::ui::hero::HeroLight;
use ha_kiosk::ui::top_bar;
use secrecy::ExposeSecret;
use serde_json::json;
use std::time::Duration;
use support::mock_ha::{Behaviour, MockHaServer};
use tokio::sync::mpsc;

const TIMEOUT: Duration = Duration::from_secs(5);

async fn next_event(events: &mut mpsc::Receiver<HaEvent>) -> HaEvent {
    tokio::time::timeout(TIMEOUT, events.recv())
        .await
        .expect("timed out waiting for a Home Assistant event")
        .expect("the Home Assistant client stopped")
}

/// Write a configuration that points at the mock server and load it through
/// the production parser, so the test covers the file on disk as well.
fn load_config(server: &MockHaServer) -> (Config, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[ha]
url = "{url}"
token = "test-token"
extra_entity_ids = ["weather.home", "media_player.example"]

[cache]
path = "{cache}"

[widgets.hero_light]
entity = "light.example"

[[tiles.items]]
entity = "light.example"
"#,
            url = server.url(),
            cache = dir.path().join("cache.json").display(),
        ),
    )
    .expect("write config");
    (Config::load(&config_path).expect("config loads"), dir)
}

#[tokio::test]
async fn config_connect_snapshot_reaches_the_overview_view_models() {
    let server = MockHaServer::start(Behaviour::Normal).await;
    let (config, _dir) = load_config(&server);
    assert_eq!(
        config.subscribed_entity_ids(),
        vec![
            "light.example".to_string(),
            "weather.home".to_string(),
            "media_player.example".to_string(),
        ],
        "the hero, the tile, and the extra entities form one subscription without duplicates"
    );

    let expected_zone: chrono_tz::Tz = support::mock_ha::TIME_ZONE.parse().expect("test timezone");

    let (events_tx, mut events) = mpsc::channel(64);
    let (commands, command_rx) = mpsc::channel(8);
    let client = tokio::spawn(ha::run(
        config.ha.clone(),
        config.subscribed_entity_ids(),
        events_tx,
        command_rx,
    ));

    let mut state = AppState::default();

    // The server configuration must reach the UI before any entity data.
    let connected = next_event(&mut events).await;
    assert!(
        matches!(
            connected,
            HaEvent::Connected { ref ha_version, time_zone }
                if ha_version == support::mock_ha::HA_VERSION && time_zone == expected_zone
        ),
        "the first application event must carry the authenticated server config; got {connected:?}"
    );
    state.apply_ha_event(connected);

    let snapshot = next_event(&mut events).await;
    let entity_count = match &snapshot {
        HaEvent::EntitiesAdded(entities) => entities.len(),
        other => panic!("expected the initial entity snapshot, got {other:?}"),
    };
    assert_eq!(entity_count, 3);
    state.apply_ha_event(snapshot);

    // Connection badge (the top bar's UI state).
    assert_eq!(state.connection, ConnectionState::Connected);
    assert!(!state.stale, "a live snapshot must not render as stale");
    let badge = top_bar::connection_view(&state.connection, state.stale);
    assert_eq!(badge.label, "Connected");
    assert!(badge.connected && !badge.warning && !badge.error);

    // Hero light card.
    let mut hero = HeroLight::new(&config.widgets.hero_light);
    hero.apply_entity(state.entities.get("light.example"));
    let card = hero.view_model();
    assert!(card.configured && card.available && !card.loading);
    assert!(card.on);
    assert_eq!(card.state_text, "On");
    assert_eq!(card.brightness, 50);
    assert_eq!(card.brightness_text, "50%");
    assert_eq!(card.status_text, "");

    // Tile row.
    let light = &state.entities["light.example"];
    assert_eq!(tiles::tile_state_text(light), "on");
    assert_eq!(tiles::tile_detail(light), "50%");

    // Protocol order: authenticate, read the server config, then subscribe.
    server.wait_for_request("subscribe_entities").await;
    let requests = server.requests();
    let request_types: Vec<&str> = requests
        .iter()
        .filter_map(|request| request["type"].as_str())
        .collect();
    assert_eq!(
        request_types,
        ["auth", "get_config", "subscribe_entities"],
        "the client must not subscribe before it knows the Home Assistant timezone"
    );
    let expected_token = config.ha.token.expose_secret().to_string();
    assert_eq!(
        requests[0]["access_token"].as_str(),
        Some(expected_token.as_str()),
        "the configured token must be the one sent to Home Assistant"
    );
    assert_ne!(
        requests[1]["id"], requests[2]["id"],
        "config and subscription requests need distinct correlation IDs"
    );
    assert_eq!(
        requests[2]["entity_ids"],
        json!(["light.example", "weather.home", "media_player.example"])
    );

    let _ = commands.send(ha_kiosk::HaCommand::Shutdown).await;
    tokio::time::timeout(TIMEOUT, client)
        .await
        .expect("the client must stop on shutdown")
        .expect("client task panicked");
}

#[tokio::test]
async fn entity_events_never_overtake_the_server_timezone() {
    // This server pushes an entity event before answering `get_config`, which a
    // slow or unusual peer can do. The UI must still learn the timezone first.
    let server = MockHaServer::start(Behaviour::EarlySnapshot).await;
    let (config, _dir) = load_config(&server);
    let (events_tx, mut events) = mpsc::channel(64);
    let (commands, command_rx) = mpsc::channel(8);
    let client = tokio::spawn(ha::run(
        config.ha.clone(),
        config.subscribed_entity_ids(),
        events_tx,
        command_rx,
    ));

    let first = next_event(&mut events).await;
    assert!(
        matches!(first, HaEvent::Connected { .. }),
        "entities must not reach the UI before the server timezone; got {first:?}"
    );

    let expected_zone: chrono_tz::Tz = support::mock_ha::TIME_ZONE.parse().expect("test timezone");
    let mut state = AppState::default();
    state.apply_ha_event(first);
    assert_eq!(state.time_zone, expected_zone);

    let _ = commands.send(ha_kiosk::HaCommand::Shutdown).await;
    tokio::time::timeout(TIMEOUT, client)
        .await
        .expect("the client must stop on shutdown")
        .expect("client task panicked");
}
