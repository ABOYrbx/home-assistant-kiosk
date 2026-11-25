//! End-to-end scenarios against a mock Home Assistant WebSocket server.
//!
//! These tests drive the production code paths: TOML configuration loading, the
//! WebSocket client with its reconnect loop, `AppState` (the state the UI thread
//! owns), the offline cache, and the tile action mapping. The mock server lives
//! in `tests/support/mock_ha.rs` so the UI-side test can use the same peer.

mod support;

use ha_kiosk::cache::FileCache;
use ha_kiosk::config::{Config, HaConfig};
use ha_kiosk::events::{HaCommand, HaError, HaEvent, PendingAction, ServiceCall};
use ha_kiosk::ha;
use ha_kiosk::sources::{media, tiles, weather};
use ha_kiosk::state::{AppState, CacheStore, ConnectionState};
use ha_kiosk::ui::top_bar::connection_view;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use std::time::Duration;
use support::mock_ha::{Behaviour, HA_VERSION, MockHaServer, TIME_ZONE, free_addr};
use tokio::sync::mpsc;
use url::Url;

/// The application side: a Home Assistant client task plus the UI-owned state.
struct TestApp {
    state: AppState,
    events: mpsc::Receiver<HaEvent>,
    commands: mpsc::Sender<HaCommand>,
    task: tokio::task::JoinHandle<()>,
}

impl TestApp {
    fn start(server: &MockHaServer, token: &str) -> Self {
        Self::start_with(
            ha_config(server.url(), token),
            vec![
                "light.example".to_string(),
                "weather.home".to_string(),
                "media_player.example".to_string(),
            ],
        )
    }

    fn start_with(config: HaConfig, entity_ids: Vec<String>) -> Self {
        let (event_tx, events) = mpsc::channel(64);
        let (commands, command_rx) = mpsc::channel(8);
        let task = tokio::spawn(async move {
            ha::run(config, entity_ids, event_tx, command_rx).await;
        });
        Self {
            state: AppState::default(),
            events,
            commands,
            task,
        }
    }

    async fn next_event(&mut self) -> HaEvent {
        let event = tokio::time::timeout(Duration::from_secs(5), self.events.recv())
            .await
            .expect("timed out waiting for a Home Assistant event")
            .expect("Home Assistant client stopped");
        self.state.apply_ha_event(event.clone());
        event
    }

    async fn wait_until(&mut self, mut condition: impl FnMut(&AppState) -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !condition(&self.state) {
                self.next_event().await;
            }
        })
        .await
        .expect("condition not reached before the timeout");
    }
}

fn ha_config(url: Url, token: &str) -> HaConfig {
    HaConfig {
        url,
        token: SecretString::from(token.to_string()),
        token_file: None,
        allow_invalid_certs: false,
        extra_entity_ids: Vec::new(),
        time_zone: None,
    }
}

fn state_with_entity(entity_id: &str, state: &str) -> AppState {
    let mut app_state = AppState::default();
    app_state.apply_ha_event(HaEvent::Snapshot(vec![ha_kiosk::events::HaEntity {
        entity_id: entity_id.to_string(),
        state: state.to_string(),
        attributes: json!({}),
        last_changed: None,
        last_updated: None,
    }]));
    app_state
}

#[tokio::test]
async fn config_connect_snapshot_reaches_ui_state() {
    let server = MockHaServer::start(Behaviour::Normal).await;
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

[[tiles.items]]
entity = "light.example"
"#,
            url = server.url(),
            cache = dir.path().join("cache.json").display(),
        ),
    )
    .expect("write config");

    let config = Config::load(&config_path).expect("config loads");
    assert_eq!(
        config.subscribed_entity_ids(),
        vec![
            "light.example".to_string(),
            "weather.home".to_string(),
            "media_player.example".to_string(),
        ]
    );

    let mut app = TestApp::start_with(
        ha_config(config.ha.url.clone(), config.ha.token.expose_secret()),
        config.subscribed_entity_ids(),
    );
    app.wait_until(|state| {
        state.connection == ConnectionState::Connected
            && state.entities.contains_key("light.example")
            && state.entities.contains_key("weather.home")
    })
    .await;

    assert_eq!(app.state.connection, ConnectionState::Connected);
    assert_eq!(app.state.ha_version.as_deref(), Some(HA_VERSION));
    assert_eq!(app.state.time_zone, TIME_ZONE.parse().unwrap());
    assert!(!app.state.stale, "a live snapshot must not be stale");
    assert!(app.state.last_success.is_some());
    assert_eq!(app.state.entities["light.example"].state, "on");

    let weather_view = weather::parse_weather_entity(&app.state.entities["weather.home"]);
    assert_eq!(weather_view.condition, "sunny");
    assert_eq!(weather_view.temperature, Some(21.5));
    let media_view = media::parse_media_entity(&app.state.entities["media_player.example"]);
    assert_eq!(media_view.state, "playing");
    assert_eq!(media_view.title.as_deref(), Some("Example track"));

    let requests = server.requests();
    assert_eq!(requests[0]["type"], "auth");
    let expected_token = config.ha.token.expose_secret().to_string();
    assert_eq!(
        requests[0]["access_token"], expected_token,
        "the configured token must be the one sent to Home Assistant"
    );
    let subscribe = requests
        .iter()
        .find(|r| r["type"] == "subscribe_entities")
        .expect("subscription sent");
    assert_eq!(subscribe["entity_ids"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn offline_shows_cached_state_as_stale_and_resyncs() {
    // Reserve a port without listening on it: the first connect is refused.
    let addr = free_addr();

    let dir = tempfile::tempdir().expect("temp dir");
    let cache = FileCache::new(
        dir.path().join("cache.json"),
        1,
        "e2e".to_string(),
        1024 * 1024,
    );
    let mut live = state_with_entity("light.example", "off");
    live.connection = ConnectionState::Connected;
    live.stale = false;
    cache.save(&live).expect("cache write");

    // Startup restores the last known snapshot before the first frame.
    let cached = cache.load().expect("cache read").expect("cached state");
    let mut app = TestApp::start_with(
        ha_config(Url::parse(&format!("http://{addr}")).unwrap(), "test-token"),
        vec!["light.example".to_string()],
    );
    app.state = AppState::restore_from_cache(cached);

    assert!(app.state.stale, "cached data must never look live");
    assert_eq!(app.state.connection, ConnectionState::Disconnected);
    assert_eq!(app.state.entities["light.example"].state, "off");

    // With the server down the client reports the disconnect and keeps the cache.
    let event = app.next_event().await;
    assert!(matches!(event, HaEvent::Disconnected { .. }));
    assert_eq!(app.state.connection, ConnectionState::Disconnected);
    assert!(app.state.stale);
    assert_eq!(app.state.entities["light.example"].state, "off");

    // The server returns on the same port; the client reconnects and resyncs.
    let _server = MockHaServer::start_on(addr.port(), Behaviour::Normal).await;
    app.wait_until(|state| {
        state.connection == ConnectionState::Connected
            && state
                .entities
                .get("light.example")
                .is_some_and(|entity| entity.state == "on")
    })
    .await;

    assert!(!app.state.stale, "a fresh snapshot clears the stale badge");
    assert_eq!(app.state.entities["light.example"].state, "on");
}

#[tokio::test]
async fn stopped_server_keeps_cached_data_stale_until_the_returning_snapshot() {
    let addr = free_addr();
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = FileCache::new(
        dir.path().join("cache.json"),
        1,
        "offline-e2e".to_string(),
        1024 * 1024,
    );
    let subscribed = vec!["light.example".to_string(), "sensor.cached".to_string()];
    let url = Url::parse(&format!("http://{addr}")).expect("mock server URL");

    // A live session with two entities, one of which the resync will drop.
    let server = MockHaServer::start_on(addr.port(), Behaviour::InitialWithRemoved).await;
    let mut app = TestApp::start_with(ha_config(url.clone(), "test-token"), subscribed.clone());
    app.wait_until(|state| {
        state.connection == ConnectionState::Connected
            && state.entities.contains_key("sensor.cached")
            && state
                .entities
                .get("light.example")
                .is_some_and(|entity| entity.state == "on")
    })
    .await;

    let live = connection_view(&app.state.connection, app.state.stale);
    assert_eq!(live.label, "Connected");
    assert!(!live.warning);

    // The kiosk caches the live state, as the runtime does after a snapshot.
    cache.save(&app.state).expect("cache write");
    let cached = cache.load().expect("cache read").expect("cached state");
    assert_eq!(cached.entities["light.example"].state, "on");

    // Home Assistant stops and the socket closes mid-session.
    server.stop().await;
    app.wait_until(|state| state.connection == ConnectionState::Disconnected)
        .await;

    assert!(app.state.stale, "data from a closed connection is not live");
    let offline = connection_view(&app.state.connection, app.state.stale);
    assert_eq!(offline.label, "Offline");
    assert!(offline.warning);
    assert_eq!(
        app.state.entities["light.example"].state, "on",
        "the last known state stays visible while the server is down"
    );

    // A kiosk restart while the server is down restores the cached snapshot
    // and keeps it visibly stale.
    let mut restarted = TestApp::start_with(ha_config(url, "test-token"), subscribed);
    restarted.state = AppState::restore_from_cache(cached);
    assert_eq!(restarted.state.connection, ConnectionState::Disconnected);
    assert!(restarted.state.stale);
    assert_eq!(restarted.state.entities.len(), 2);
    assert_eq!(restarted.state.entities["sensor.cached"].state, "42");
    assert_eq!(
        connection_view(&restarted.state.connection, restarted.state.stale).label,
        "Offline"
    );

    // The server returns on the same port with a fresh snapshot that no longer
    // includes `sensor.cached`.
    let _server = MockHaServer::start_on(addr.port(), Behaviour::Resync).await;

    // A successful handshake alone must not make the restored data look live.
    restarted
        .wait_until(|state| state.connection == ConnectionState::Connected)
        .await;
    assert!(
        restarted.state.stale,
        "the handshake must not clear the stale badge before fresh data arrives"
    );
    assert_eq!(
        connection_view(&restarted.state.connection, restarted.state.stale).label,
        "Stale"
    );
    assert!(restarted.state.entities.contains_key("sensor.cached"));

    // The snapshot replaces the restored state and clears the badge.
    restarted
        .wait_until(|state| {
            !state.stale
                && state
                    .entities
                    .get("light.example")
                    .is_some_and(|entity| entity.state == "off")
        })
        .await;
    assert_eq!(
        restarted.state.connection,
        ConnectionState::Connected,
        "a resynced kiosk reports a live connection"
    );
    assert!(
        !restarted.state.entities.contains_key("sensor.cached"),
        "the resync snapshot replaces entities that disappeared"
    );
    assert_eq!(
        connection_view(&restarted.state.connection, restarted.state.stale).label,
        "Connected"
    );
}

#[tokio::test]
async fn invalid_token_reports_an_error_without_retrying() {
    let server = MockHaServer::start(Behaviour::RejectToken).await;
    let mut app = TestApp::start(&server, "wrong-token");

    let event = app.next_event().await;
    match event {
        HaEvent::AuthFailed { message } => assert_eq!(message, "Invalid access token"),
        other => panic!("expected an authentication failure, got {other:?}"),
    }
    assert_eq!(app.state.connection, ConnectionState::AuthFailed);

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        server.connections(),
        1,
        "a bad token must stop retrying instead of looping"
    );
    match app.events.try_recv() {
        Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {}
        Ok(event) => panic!("unexpected event after an authentication failure: {event:?}"),
    }
    tokio::time::timeout(Duration::from_secs(5), app.task)
        .await
        .expect("the client loop must stop after an authentication failure")
        .expect("client task panicked");
}

#[tokio::test]
async fn failed_service_call_reports_an_error_without_optimistic_state() {
    let server = MockHaServer::start(Behaviour::ServiceFailure).await;
    let mut app = TestApp::start(&server, "test-token");
    app.wait_until(|state| {
        state.connection == ConnectionState::Connected
            && state.entities.contains_key("light.example")
    })
    .await;
    assert_eq!(app.state.entities["light.example"].state, "on");

    let request_id = 7;
    app.state.pending.insert(
        request_id,
        PendingAction::ToggleTile {
            entity_id: "light.example".to_string(),
        },
    );
    app.commands
        .send(HaCommand::CallService(ServiceCall {
            request_id,
            domain: "light".to_string(),
            service: "toggle".to_string(),
            target: Some(json!({"entity_id": "light.example"})),
            data: json!({}),
            return_response: false,
        }))
        .await
        .expect("command channel");

    match app.next_event().await {
        HaEvent::ServiceResult {
            request_id: answered,
            result,
        } => {
            assert_eq!(answered, request_id);
            match result {
                Err(HaError::Service(message)) => {
                    assert!(
                        message.contains("not found"),
                        "unexpected message: {message}"
                    );
                }
                other => panic!("expected a service error, got {other:?}"),
            }
        }
        other => panic!("expected a service result, got {other:?}"),
    }

    assert_eq!(
        app.state.entities["light.example"].state, "on",
        "a failed call must not flip the tile"
    );
    assert!(
        app.state.pending.is_empty(),
        "the pending marker must be cleared after the error"
    );
    assert_eq!(
        app.state.connection,
        ConnectionState::Connected,
        "a failed call must not drop the connection"
    );
    let calls = server.service_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["domain"], "light");
    assert_eq!(calls[0]["service"], "toggle");
}

#[tokio::test]
async fn malformed_messages_and_unknown_domains_do_not_crash() {
    let server = MockHaServer::start(Behaviour::Malformed).await;
    let mut app = TestApp::start(&server, "test-token");

    app.wait_until(|state| {
        state.connection == ConnectionState::Connected
            && state
                .entities
                .get("light.example")
                .is_some_and(|entity| entity.state == "off")
            && state.entities.contains_key("foo.bar")
    })
    .await;

    assert_eq!(app.state.connection, ConnectionState::Connected);
    assert!(
        !app.state.entities.contains_key("does.not.exist"),
        "a change for an unknown entity must be ignored"
    );

    // An unknown domain stays read-only and never produces a service call.
    assert!(tiles::is_read_only("foo.bar"));
    assert!(
        tiles::build_service_call(&tiles::tile_action("foo.bar"), &"foo.bar".to_string(), 1)
            .is_none()
    );
    assert!(server.service_calls().is_empty());
}

/// Read the resident set size of this test process in MiB.
fn rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib / 1024.0)
}

/// Soak the client, the state owner, and the cache with a state-change stream.
///
/// This is the short, host-side leak check that precedes the 24-hour Pi run in
/// `docs/performance.md`. It is ignored by default because it is a soak, not a
/// unit test. Select the duration with `HA_KIOSK_SOAK_SECONDS` (default 60) and
/// run it with `--ignored --nocapture`:
///
/// ```text
/// HA_KIOSK_SOAK_SECONDS=3600 cargo test --test e2e_test \
///   soak_with_state_change_stream_stays_bounded -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "long-running soak; set HA_KIOSK_SOAK_SECONDS to choose the duration"]
async fn soak_with_state_change_stream_stays_bounded() {
    let seconds = std::env::var("HA_KIOSK_SOAK_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60u64);
    let server = MockHaServer::start(Behaviour::Soak).await;
    let mut app = TestApp::start(&server, "test-token");

    app.wait_until(|state| {
        state.connection == ConnectionState::Connected
            && state.entities.contains_key("light.example")
            && state.entities.contains_key("weather.home")
            && state.entities.contains_key("media_player.example")
    })
    .await;

    let dir = tempfile::tempdir().expect("temp dir");
    let cache_path = dir.path().join("cache.json");
    let max_bytes = 256 * 1024;
    let cache = FileCache::new(cache_path.clone(), 1, "soak".to_string(), max_bytes);
    let baseline_entities = app.state.entities.len();

    let started = tokio::time::Instant::now();
    let mut events = 0u64;
    let mut cache_writes = 0u64;
    let mut last_sample = started;
    let mut baseline_rss: Option<f64> = None;
    let mut last_rss = rss_mb();

    while started.elapsed() < Duration::from_secs(seconds) {
        app.next_event().await;
        events += 1;

        if events.is_multiple_of(20) {
            cache.save(&app.state).expect("cache write");
            cache_writes += 1;
            let size = std::fs::metadata(&cache_path)
                .expect("cache metadata")
                .len();
            assert!(
                size <= max_bytes,
                "cache grew to {size} bytes, above the {max_bytes} byte cap"
            );
        }

        if last_sample.elapsed() >= Duration::from_secs(1) {
            last_sample = tokio::time::Instant::now();
            last_rss = rss_mb();
            // Ignore the first seconds of allocator warm-up when comparing.
            if baseline_rss.is_none() && started.elapsed() >= Duration::from_secs(5) {
                baseline_rss = last_rss;
            }
        }
    }

    let cache_size = std::fs::metadata(&cache_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "soak: {seconds}s, {events} events, {cache_writes} cache writes, \
         {cache_size} cache bytes, RSS baseline {baseline_rss:?} MiB, final {last_rss:?} MiB"
    );

    assert_eq!(
        app.state.entities.len(),
        baseline_entities,
        "state updates must not grow the entity map"
    );
    assert_eq!(app.state.connection, ConnectionState::Connected);
    assert!(!app.state.stale, "a live stream must not stay stale");
    assert!(app.state.pending.is_empty());
    assert!(cache_size <= max_bytes, "the final cache exceeds its cap");

    if let (Some(baseline), Some(final_rss)) = (baseline_rss, last_rss) {
        assert!(
            final_rss - baseline < 16.0,
            "RSS grew by {:.1} MiB over the soak ({baseline:.1} -> {final_rss:.1}); \
             investigate the allocation trend",
            final_rss - baseline
        );
    }
}
