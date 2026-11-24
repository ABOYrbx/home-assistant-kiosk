use futures_util::{SinkExt, StreamExt};
use ha_kiosk::events::{HaCommand, HaEntity, HaEvent, ServiceCall};
use ha_kiosk::ha::{self, HaClientOptions};
use ha_kiosk::state::AppState;
use secrecy::SecretString;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, accept_async, accept_hdr_async};

const TEST_TOKEN: &str = "test-token";

type Client = WebSocketStream<tokio::net::TcpStream>;

async fn send_json(ws: &mut Client, value: Value) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

async fn recv_json(ws: &mut Client) -> Option<Value> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => return serde_json::from_str(&text).ok(),
            Some(Ok(Message::Close(_))) | None => return None,
            Some(Ok(_)) => continue,
            Some(Err(_)) => return None,
        }
    }
}

/// Complete the `auth_required` -> `auth` -> `auth_ok` handshake and answer
/// `get_config` and `subscribe_entities`.
async fn serve_auth(ws: &mut Client, version: &str, tz: &str, expected_ids: &[&str]) {
    send_json(ws, json!({"type": "auth_required", "ha_version": version})).await;

    let auth = recv_json(ws).await.expect("auth message");
    assert_eq!(auth.get("type").and_then(Value::as_str), Some("auth"));
    assert_eq!(
        auth.get("access_token").and_then(Value::as_str),
        Some(TEST_TOKEN)
    );

    send_json(ws, json!({"type": "auth_ok", "ha_version": version})).await;

    let mut got_config = false;
    let mut got_subscribe = false;
    while !(got_config && got_subscribe) {
        let Some(message) = recv_json(ws).await else {
            return;
        };
        match message.get("type").and_then(Value::as_str) {
            Some("get_config") => {
                let id = message.get("id").and_then(Value::as_u64).unwrap();
                send_json(
                    ws,
                    json!({
                        "id": id,
                        "type": "result",
                        "success": true,
                        "result": {"time_zone": tz, "version": "2024.2.0"},
                    }),
                )
                .await;
                got_config = true;
            }
            Some("subscribe_entities") => {
                let id = message.get("id").and_then(Value::as_u64).unwrap();
                let ids: Vec<&str> = message
                    .get("entity_ids")
                    .and_then(Value::as_array)
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                assert_eq!(ids, expected_ids);
                send_json(
                    ws,
                    json!({"id": id, "type": "result", "success": true, "result": null}),
                )
                .await;
                got_subscribe = true;
            }
            _ => {}
        }
    }
}

fn test_config(addr: SocketAddr, path: &str) -> ha_kiosk::config::HaConfig {
    ha_kiosk::config::HaConfig {
        url: url::Url::parse(&format!("http://{}{}", addr, path)).unwrap(),
        token: SecretString::from(TEST_TOKEN.to_string()),
        token_file: None,
        allow_invalid_certs: false,
        extra_entity_ids: vec![],
        time_zone: None,
    }
}

type ClientHarness = (
    mpsc::Sender<HaCommand>,
    mpsc::Receiver<HaEvent>,
    tokio::task::JoinHandle<()>,
);

fn spawn_client(
    addr: SocketAddr,
    path: &str,
    entity_ids: &[&str],
    options: HaClientOptions,
) -> ClientHarness {
    let (event_tx, event_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = test_config(addr, path);
    let ids = entity_ids.iter().map(|id| id.to_string()).collect();
    let handle = tokio::spawn(async move {
        ha::run_with_options(config, ids, event_tx, command_rx, options).await;
    });
    (command_tx, event_rx, handle)
}

fn entity(entity_id: &str, state: &str, attributes: Value) -> Value {
    json!({
        "entity_id": entity_id,
        "state": state,
        "attributes": attributes,
        "last_changed": "2024-02-01T10:00:00+00:00",
        "last_updated": "2024-02-01T10:00:00+00:00",
    })
}

#[tokio::test]
async fn auth_config_snapshot_change_and_remove_into_state() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let path = Arc::new(Mutex::new(String::new()));
    let server_path = Arc::clone(&path);

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_hdr_async(
            stream,
            move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                  response| {
                *server_path.lock().unwrap() = request.uri().path().to_string();
                Ok(response)
            },
        )
        .await
        .unwrap();

        serve_auth(&mut ws, "2024.2.0", "Europe/Berlin", &["light.example"]).await;

        // Initial snapshot, then a compact diff that removes `brightness` and
        // changes the state, then a removal.
        send_json(
            &mut ws,
            json!({"type": "event", "event": {"a": [
                entity("light.example", "on", json!({"brightness": 100, "friendly_name": "Lamp"}))
            ]}}),
        )
        .await;
        send_json(
            &mut ws,
            json!({"type": "event", "event": {"c": [
                {"entity_id": "light.example", "s": "off", "+": {"friendly_name": "Desk lamp"}, "-": ["brightness"]}
            ]}}),
        )
        .await;
        send_json(
            &mut ws,
            json!({"type": "event", "event": {"r": ["light.example"]}}),
        )
        .await;

        // Keep the socket open while the client asserts.
        while let Some(message) = recv_json(&mut ws).await {
            if message.get("type").and_then(Value::as_str) == Some("ping") {
                let id = message.get("id").and_then(Value::as_u64).unwrap();
                send_json(&mut ws, json!({"id": id, "type": "pong"})).await;
            }
        }
    });

    let (command_tx, mut events, client) =
        spawn_client(addr, "/ha/", &["light.example"], HaClientOptions::default());

    let mut state = AppState::default();
    let mut connected = false;
    let mut snapshot_seen = false;
    let mut change_seen = false;
    let mut remove_seen = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await else {
            break;
        };
        match event {
            HaEvent::Connected {
                ha_version,
                time_zone,
            } => {
                assert_eq!(ha_version, "2024.2.0");
                assert_eq!(time_zone.to_string(), "Europe/Berlin");
                connected = true;
            }
            HaEvent::Snapshot(entities) => {
                assert_eq!(entities.len(), 1);
                let light = &entities[0];
                assert_eq!(light.state, "on");
                assert_eq!(light.attributes["brightness"], json!(100));
                state.apply_ha_event(HaEvent::Snapshot(entities));
                snapshot_seen = true;
            }
            HaEvent::EntitiesChanged(changes) => {
                assert_eq!(changes.len(), 1);
                state.apply_ha_event(HaEvent::EntitiesChanged(changes));
                let light = &state.entities["light.example"];
                assert_eq!(light.state, "off");
                assert_eq!(light.attributes["friendly_name"], json!("Desk lamp"));
                assert!(light.attributes.get("brightness").is_none());
                change_seen = true;
            }
            HaEvent::EntitiesRemoved(ids) => {
                assert_eq!(ids, vec!["light.example".to_string()]);
                state.apply_ha_event(HaEvent::EntitiesRemoved(ids));
                assert!(state.entities.is_empty());
                remove_seen = true;
                break;
            }
            _ => {}
        }
    }

    client.abort();
    server.abort();
    drop(command_tx);

    assert!(connected, "no Connected event");
    assert!(snapshot_seen, "no snapshot");
    assert!(change_seen, "no change");
    assert!(remove_seen, "no removal");
    assert_eq!(*path.lock().unwrap(), "/ha/api/websocket");
}

#[tokio::test]
async fn service_response_correlation() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        serve_auth(&mut ws, "2024.2.0", "UTC", &["light.example"]).await;

        while let Some(message) = recv_json(&mut ws).await {
            match message.get("type").and_then(Value::as_str) {
                Some("call_service") => {
                    assert_eq!(message["id"], json!(4242));
                    assert_eq!(message["domain"], json!("light"));
                    assert_eq!(message["service"], json!("turn_on"));
                    assert_eq!(message["target"], json!({"entity_id": "light.example"}));
                    assert_eq!(message["return_response"], json!(true));
                    send_json(
                        &mut ws,
                        json!({
                            "id": 4242,
                            "type": "result",
                            "success": true,
                            "result": {"changed_states": []},
                        }),
                    )
                    .await;
                }
                Some("ping") => {
                    let id = message.get("id").and_then(Value::as_u64).unwrap();
                    send_json(&mut ws, json!({"id": id, "type": "pong"})).await;
                }
                _ => {}
            }
        }
    });

    let (command_tx, mut events, client) =
        spawn_client(addr, "", &["light.example"], HaClientOptions::default());

    command_tx
        .send(HaCommand::CallService(ServiceCall {
            request_id: 4242,
            domain: "light".to_string(),
            service: "turn_on".to_string(),
            target: Some(json!({"entity_id": "light.example"})),
            data: json!({"brightness": 10}),
            return_response: true,
        }))
        .await
        .unwrap();

    let mut result_seen = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await else {
            break;
        };
        if let HaEvent::ServiceResult { request_id, result } = event {
            assert_eq!(request_id, 4242);
            let response = result.expect("service response should succeed");
            assert_eq!(response.data["changed_states"], json!([]));
            result_seen = true;
            break;
        }
    }

    command_tx.send(HaCommand::Shutdown).await.unwrap();
    client.abort();
    server.abort();
    assert!(result_seen, "no correlated service result");
}

#[tokio::test]
async fn reconnect_reauthenticates_and_replaces_state() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        for (index, state) in ["on", "off"].into_iter().enumerate() {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            serve_auth(&mut ws, "2024.2.0", "UTC", &["light.reconnect"]).await;
            send_json(
                &mut ws,
                json!({"type": "event", "event": {"a": [
                    entity("light.reconnect", state, json!({"index": index}))
                ]}}),
            )
            .await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Dropping the stream disconnects the client.
        }
    });

    let (_command_tx, mut events, client) =
        spawn_client(addr, "", &["light.reconnect"], HaClientOptions::default());

    let mut state = AppState::default();
    let mut connected = 0;
    let mut snapshots = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await else {
            break;
        };
        match event {
            HaEvent::Connected { .. } => connected += 1,
            HaEvent::Snapshot(entities) => {
                snapshots += 1;
                state.apply_ha_event(HaEvent::Snapshot(entities));
            }
            _ => {}
        }
        if connected >= 2 && snapshots >= 2 {
            break;
        }
    }

    client.abort();
    server.abort();

    assert_eq!(connected, 2, "client did not re-authenticate");
    assert_eq!(snapshots, 2, "second subscribe snapshot missing");
    assert_eq!(state.entities["light.reconnect"].state, "off");
    assert_eq!(
        state.entities["light.reconnect"].attributes["index"],
        json!(1)
    );
}

#[tokio::test]
async fn heartbeat_timeout_disconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        serve_auth(&mut ws, "2024.2.0", "UTC", &["light.example"]).await;
        // Deliberately never answer pings.
        while recv_json(&mut ws).await.is_some() {}
    });

    let options = HaClientOptions {
        heartbeat_interval: Duration::from_millis(100),
        heartbeat_timeout: Duration::from_millis(150),
    };
    let (_command_tx, mut events, client) = spawn_client(addr, "", &["light.example"], options);

    let mut reason = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await else {
            break;
        };
        if let HaEvent::Disconnected { reason: text } = event {
            reason = Some(text);
            break;
        }
    }

    client.abort();
    server.abort();
    let reason = reason.expect("no disconnect after missing pong");
    assert!(
        reason.contains("heartbeat"),
        "unexpected disconnect reason: {reason}"
    );
}

#[tokio::test]
async fn auth_invalid_stops_retries() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        send_json(
            &mut ws,
            json!({"type": "auth_required", "ha_version": "2024.2.0"}),
        )
        .await;
        let _ = recv_json(&mut ws).await;
        send_json(
            &mut ws,
            json!({"type": "auth_invalid", "message": "bad token"}),
        )
        .await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let (_command_tx, mut events, client) =
        spawn_client(addr, "", &["light.example"], HaClientOptions::default());

    let mut failed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await else {
            break;
        };
        if let HaEvent::AuthFailed { message } = event {
            assert_eq!(message, "bad token");
            failed = true;
            break;
        }
    }

    // The client task must end rather than retry.
    let stopped = tokio::time::timeout(Duration::from_secs(2), client).await;
    server.abort();
    assert!(failed, "no AuthFailed event");
    assert!(stopped.is_ok(), "client kept retrying after auth_invalid");
}

#[tokio::test]
async fn get_states_snapshot_for_diagnostics() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        serve_auth(&mut ws, "2024.2.0", "UTC", &["light.example"]).await;
        while let Some(message) = recv_json(&mut ws).await {
            match message.get("type").and_then(Value::as_str) {
                Some("get_states") => {
                    let id = message.get("id").and_then(Value::as_u64).unwrap();
                    send_json(
                        &mut ws,
                        json!({"id": id, "type": "result", "success": true, "result": [
                            entity("light.example", "on", json!({}))
                        ]}),
                    )
                    .await;
                }
                Some("ping") => {
                    let id = message.get("id").and_then(Value::as_u64).unwrap();
                    send_json(&mut ws, json!({"id": id, "type": "pong"})).await;
                }
                _ => {}
            }
        }
    });

    let (command_tx, mut events, client) =
        spawn_client(addr, "", &["light.example"], HaClientOptions::default());

    command_tx.send(HaCommand::RefreshSnapshot).await.unwrap();

    let mut snapshot = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await else {
            break;
        };
        if let HaEvent::Snapshot(entities) = event {
            snapshot = Some(entities);
            break;
        }
    }

    command_tx.send(HaCommand::Shutdown).await.unwrap();
    client.abort();
    server.abort();
    let entities = snapshot.expect("no get_states snapshot");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].entity_id, "light.example");
}

#[test]
fn path_prefixed_url_is_preserved() {
    let base = url::Url::parse("https://ha.example.com/ha").unwrap();
    let ws = ha::websocket_url(&base).unwrap();
    assert_eq!(ws.as_str(), "wss://ha.example.com/ha/api/websocket");
}

#[test]
fn trailing_slash_and_query_are_handled() {
    let base = url::Url::parse("http://ha.example.com:8123/?token=abc").unwrap();
    let ws = ha::websocket_url(&base).unwrap();
    assert_eq!(ws.as_str(), "ws://ha.example.com:8123/api/websocket");
}

#[test]
fn snapshot_entity_type_is_serializable() {
    // Guard the event payload shape used by the cache.
    let entity = HaEntity {
        entity_id: "light.example".to_string(),
        state: "on".to_string(),
        attributes: json!({"brightness": 1}),
        last_changed: None,
        last_updated: None,
    };
    let text = serde_json::to_string(&entity).unwrap();
    assert!(text.contains("light.example"));
}
