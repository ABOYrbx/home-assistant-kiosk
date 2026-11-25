//! End-to-end scenarios against a mock Home Assistant WebSocket server.
//!
//! These tests drive the production code paths: TOML configuration loading, the
//! WebSocket client with its reconnect loop, `AppState` (the state the UI thread
//! owns), the offline cache, and the tile action mapping. The mock server is a
//! real TCP WebSocket peer; no Home Assistant instance and no network are used.

use futures_util::{SinkExt, StreamExt};
use ha_kiosk::cache::FileCache;
use ha_kiosk::config::{Config, HaConfig};
use ha_kiosk::events::{HaCommand, HaError, HaEvent, PendingAction, ServiceCall};
use ha_kiosk::ha;
use ha_kiosk::sources::{media, tiles, weather};
use ha_kiosk::state::{AppState, CacheStore, ConnectionState};
use ha_kiosk::ui::top_bar::connection_view;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, accept_async};
use url::Url;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const HA_VERSION: &str = "2024.1.0";
const TIME_ZONE: &str = "Europe/Berlin";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Behaviour {
    /// Accept the token, answer requests, and send an initial snapshot.
    Normal,
    /// Reject the token the way Home Assistant does.
    RejectToken,
    /// Answer `call_service` with an error result.
    ServiceFailure,
    /// Send malformed and unexpected messages before valid data.
    Malformed,
    /// Stream state changes for an unbounded soak run.
    Soak,
}

struct MockHaServer {
    addr: SocketAddr,
    connections: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockHaServer {
    async fn start(behaviour: Behaviour) -> Self {
        Self::start_on(0, behaviour).await
    }

    /// Bind a specific port so a client can reconnect to the same address.
    async fn start_on(port: u16, behaviour: Behaviour) -> Self {
        let listener = bind_listener(port);
        let addr = listener.local_addr().expect("listener address");
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));

        let task = {
            let connections = Arc::clone(&connections);
            let requests = Arc::clone(&requests);
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    connections.fetch_add(1, Ordering::SeqCst);
                    let requests = Arc::clone(&requests);
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, behaviour, requests).await;
                    });
                }
            })
        };

        Self {
            addr,
            connections,
            requests,
            task,
        }
    }

    fn url(&self) -> Url {
        Url::parse(&format!("http://{}", self.addr)).expect("mock server URL")
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("request log").clone()
    }

    fn service_calls(&self) -> Vec<Value> {
        self.requests()
            .into_iter()
            .filter(|v| v.get("type").and_then(Value::as_str) == Some("call_service"))
            .collect()
    }
}

impl Drop for MockHaServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn bind_listener(port: u16) -> TcpListener {
    let socket = TcpSocket::new_v4().expect("tcp socket");
    socket.set_reuseaddr(true).expect("SO_REUSEADDR");
    socket
        .bind(SocketAddr::from(([127, 0, 0, 1], port)))
        .expect("bind mock server");
    socket.listen(64).expect("listen")
}

fn free_addr() -> SocketAddr {
    let listener = bind_listener(0);
    listener.local_addr().expect("listener address")
}

async fn serve_connection(
    stream: TcpStream,
    behaviour: Behaviour,
    requests: Arc<Mutex<Vec<Value>>>,
) -> Result<(), BoxError> {
    let mut ws = accept_async(stream).await?;
    send(
        &mut ws,
        json!({"type": "auth_required", "ha_version": HA_VERSION}),
    )
    .await?;

    let auth = read_json(&mut ws).await?;
    record(&requests, auth.clone());
    if behaviour == Behaviour::RejectToken {
        send(
            &mut ws,
            json!({"type": "auth_invalid", "message": "Invalid access token"}),
        )
        .await?;
        let _ = ws.close(None).await;
        return Ok(());
    }
    send(
        &mut ws,
        json!({"type": "auth_ok", "ha_version": HA_VERSION}),
    )
    .await?;

    loop {
        let message = match ws.next().await {
            Some(Ok(message)) => message,
            _ => return Ok(()),
        };
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(request) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        record(&requests, request.clone());

        match request.get("type").and_then(Value::as_str) {
            Some("get_config") => {
                send(
                    &mut ws,
                    json!({
                        "id": request["id"],
                        "type": "result",
                        "success": true,
                        "result": {"time_zone": TIME_ZONE, "version": HA_VERSION},
                    }),
                )
                .await?;
            }
            Some("subscribe_entities") => {
                send(
                    &mut ws,
                    json!({"id": request["id"], "type": "result", "success": true, "result": null}),
                )
                .await?;
                if behaviour == Behaviour::Soak {
                    stream_soak_messages(&mut ws).await?;
                    return Ok(());
                }
                send_scenario_messages(&mut ws, behaviour).await?;
            }
            Some("call_service") => {
                let response = if behaviour == Behaviour::ServiceFailure {
                    json!({
                        "id": request["id"],
                        "type": "result",
                        "success": false,
                        "error": {
                            "code": "service_not_found",
                            "message": "Service light.toggle not found",
                        },
                    })
                } else {
                    json!({"id": request["id"], "type": "result", "success": true, "result": null})
                };
                send(&mut ws, response).await?;
            }
            Some("ping") => {
                send(&mut ws, json!({"id": request["id"], "type": "pong"})).await?;
            }
            _ => {}
        }
    }
}

async fn send_scenario_messages(
    ws: &mut WebSocketStream<TcpStream>,
    behaviour: Behaviour,
) -> Result<(), BoxError> {
    if behaviour == Behaviour::Malformed {
        ws.send(Message::Text("this is not json".into())).await?;
        ws.send(Message::Text("{\"type\":\"event\",\"event\":".into()))
            .await?;
        ws.send(Message::Binary(vec![0x00, 0xff, 0x7f].into()))
            .await?;
        send(ws, json!({"type": "event", "event": "not-an-object"})).await?;
        send(
            ws,
            json!({"type": "event", "event": {"a": "not-an-array", "c": 42, "r": null}}),
        )
        .await?;
        send(ws, json!({"type": "totally_unknown"})).await?;
        send(
            ws,
            json!({"id": 9999, "type": "result", "success": true, "result": {"unexpected": true}}),
        )
        .await?;
        // An unknown domain and a change for an entity that was never seen.
        send(
            ws,
            json!({"type": "event", "event": {
                "a": [
                    {"entity_id": "foo.bar", "state": "weird"},
                    {"entity_id": "light.example", "state": "on"},
                ],
                "c": [{"entity_id": "does.not.exist", "state": "x"}],
            }}),
        )
        .await?;
        // Valid traffic after the malformed messages proves the loop still works.
        send(
            ws,
            json!({"type": "event", "event": {
                "c": [{"entity_id": "light.example", "state": "off", "attributes": {}}],
            }}),
        )
        .await?;
        return Ok(());
    }

    send(
        ws,
        json!({"type": "event", "event": {
            "a": [
                {
                    "entity_id": "light.example",
                    "state": "on",
                    "attributes": {"brightness": 128},
                },
                {
                    "entity_id": "weather.home",
                    "state": "sunny",
                    "attributes": {"temperature": 21.5, "temperature_unit": "°C"},
                },
                {
                    "entity_id": "media_player.example",
                    "state": "playing",
                    "attributes": {
                        "media_title": "Example track",
                        "media_artist": "Example artist",
                        "volume_level": 0.4,
                    },
                },
            ],
        }}),
    )
    .await
}

/// Keep a connection busy with a simulated state-change stream.
///
/// The first event is a snapshot so the client starts from a known state; later
/// events only change entities that already exist, which exercises the update
/// path, the cache writer, and the view-model builders for hours without
/// growing the entity set.
async fn stream_soak_messages(ws: &mut WebSocketStream<TcpStream>) -> Result<(), BoxError> {
    send(
        ws,
        json!({"type": "event", "event": {
            "a": [
                {"entity_id": "light.example", "state": "on", "attributes": {"brightness": 1}},
                {
                    "entity_id": "weather.home",
                    "state": "sunny",
                    "attributes": {"temperature": 20.0, "temperature_unit": "°C"},
                },
                {
                    "entity_id": "media_player.example",
                    "state": "playing",
                    "attributes": {"media_title": "Example track", "volume_level": 0.4},
                },
            ],
        }}),
    )
    .await?;

    let mut tick: u64 = 0;
    loop {
        match tokio::time::timeout(Duration::from_millis(50), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let Ok(request) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                if request.get("type").and_then(Value::as_str) == Some("ping") {
                    send(ws, json!({"id": request["id"], "type": "pong"})).await?;
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(_))) | Ok(None) => return Ok(()),
            Err(_) => {
                tick += 1;
                let brightness = (tick % 255) as u8;
                send(
                    ws,
                    json!({"type": "event", "event": {
                        "c": [
                            {
                                "entity_id": "light.example",
                                "state": if tick.is_multiple_of(2) { "on" } else { "off" },
                                "attributes": {"brightness": brightness},
                            },
                            {
                                "entity_id": "weather.home",
                                "state": "sunny",
                                "attributes": {"temperature": 20.0 + (tick % 10) as f64},
                            },
                        ],
                    }}),
                )
                .await?;
            }
        }
    }
}

async fn send(ws: &mut WebSocketStream<TcpStream>, value: Value) -> Result<(), BoxError> {
    ws.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}

async fn read_json(ws: &mut WebSocketStream<TcpStream>) -> Result<Value, BoxError> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => return Ok(serde_json::from_str(&text)?),
            Some(Ok(_)) => continue,
            Some(Err(e)) => return Err(e.into()),
            None => return Err("socket closed".into()),
        }
    }
}

fn record(requests: &Arc<Mutex<Vec<Value>>>, value: Value) {
    requests.lock().expect("request log").push(value);
}

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
async fn invalid_token_reports_an_error_without_retrying() {
    let server = MockHaServer::start(Behaviour::RejectToken).await;
    let mut app = TestApp::start(&server, "wrong-token");

    let event = app.next_event().await;
    match event {
        HaEvent::AuthFailed { message } => assert_eq!(message, "Invalid access token"),
        other => panic!("expected an authentication failure, got {other:?}"),
    }
    assert_eq!(app.state.connection, ConnectionState::AuthFailed);

    // The badge view model is what the UI consumes. A rejected token must be
    // an error, not a transient warning that looks like a network hiccup.
    let badge = connection_view(&app.state.connection, app.state.stale);
    assert!(badge.error, "an auth failure must be shown as an error");
    assert!(!badge.warning && !badge.connected);
    assert_eq!(badge.label, "Auth failed");

    // Wait through several reconnect backoff windows. A client that retried a
    // rejected token would open another socket during this time.
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    assert_eq!(
        server.connections(),
        1,
        "a bad token must stop retrying instead of looping"
    );
    let auth_attempts = server
        .requests()
        .into_iter()
        .filter(|request| request.get("type").and_then(Value::as_str) == Some("auth"))
        .count();
    assert_eq!(
        auth_attempts, 1,
        "the rejected token must be offered to Home Assistant exactly once"
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
