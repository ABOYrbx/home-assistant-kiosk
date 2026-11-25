//! An in-process Home Assistant WebSocket peer for end-to-end tests.
//!
//! `tests/support` is shared by the end-to-end test binaries and is not a test
//! target of its own. Every connection is a real loopback TCP WebSocket, so the
//! production client, its reconnect loop, and the JSON handling run unchanged.
//! No real Home Assistant instance and no external network are used.
#![allow(dead_code)] // each test binary exercises a different subset of the scenarios

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, accept_async};
use url::Url;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub const HA_VERSION: &str = "2024.1.0";
pub const TIME_ZONE: &str = "Europe/Berlin";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Accept the token, answer requests, and send an initial snapshot.
    Normal,
    /// Reject the token the way Home Assistant does.
    RejectToken,
    /// Answer `call_service` with an error result.
    ServiceFailure,
    /// Send an entity event before the `get_config` result, as a server that
    /// does not wait for the client's setup order would.
    EarlySnapshot,
    /// Send malformed and unexpected messages before valid data.
    Malformed,
    /// Stream state changes for an unbounded soak run.
    Soak,
    /// First connection includes an entity that disappears on the next snapshot.
    InitialWithRemoved,
    /// Reconnect snapshot replaces the initial state and omits the removed entity.
    Resync,
    /// Send malformed frames during the handshake before valid protocol messages.
    MalformedHandshake,
    /// Send nothing but malformed frames, so every connection is unusable.
    GarbageHandshake,
}

pub struct MockHaServer {
    addr: SocketAddr,
    connections: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    task: Option<tokio::task::JoinHandle<()>>,
    connection_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl MockHaServer {
    pub async fn start(behaviour: Behaviour) -> Self {
        Self::start_on(0, behaviour).await
    }

    /// Bind a specific port so a client can reconnect to the same address.
    pub async fn start_on(port: u16, behaviour: Behaviour) -> Self {
        let listener = bind_listener(port);
        let addr = listener.local_addr().expect("listener address");
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let connection_tasks = Arc::new(Mutex::new(Vec::new()));

        let task = {
            let connections = Arc::clone(&connections);
            let requests = Arc::clone(&requests);
            let connection_tasks = Arc::clone(&connection_tasks);
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    connections.fetch_add(1, Ordering::SeqCst);
                    let requests = Arc::clone(&requests);
                    let connection_tasks = Arc::clone(&connection_tasks);
                    let task = tokio::spawn(async move {
                        let _ = serve_connection(stream, behaviour, requests).await;
                    });
                    connection_tasks
                        .lock()
                        .expect("connection task log")
                        .push(task);
                }
            })
        };

        Self {
            addr,
            connections,
            requests,
            task: Some(task),
            connection_tasks,
        }
    }

    /// Stop accepting connections and close every active WebSocket peer.
    pub async fn stop(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
        let tasks =
            std::mem::take(&mut *self.connection_tasks.lock().expect("connection task log"));
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
    }

    pub fn url(&self) -> Url {
        Url::parse(&format!("http://{}", self.addr)).expect("mock server URL")
    }

    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("request log").clone()
    }

    /// Wait until the client sent a request of `request_type`, then return it.
    pub async fn wait_for_request(&self, request_type: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(request) = self
                    .requests()
                    .into_iter()
                    .find(|request| request["type"] == request_type)
                {
                    return request;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for a {request_type} request"))
    }

    pub fn service_calls(&self) -> Vec<Value> {
        self.requests()
            .into_iter()
            .filter(|v| v.get("type").and_then(Value::as_str) == Some("call_service"))
            .collect()
    }
}

impl Drop for MockHaServer {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
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

pub fn free_addr() -> SocketAddr {
    let listener = bind_listener(0);
    listener.local_addr().expect("listener address")
}

async fn serve_connection(
    stream: TcpStream,
    behaviour: Behaviour,
    requests: Arc<Mutex<Vec<Value>>>,
) -> Result<(), BoxError> {
    let mut ws = accept_async(stream).await?;
    if behaviour == Behaviour::MalformedHandshake {
        ws.send(Message::Text("this is not json".into())).await?;
        ws.send(Message::Text("{\"type\":".into())).await?;
        ws.send(Message::Binary(vec![0x00, 0xff, 0x7f].into()))
            .await?;
    }
    if behaviour == Behaviour::GarbageHandshake {
        for _ in 0..32 {
            ws.send(Message::Text("not json at all".into())).await?;
        }
    }
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
                if behaviour == Behaviour::MalformedHandshake {
                    ws.send(Message::Text("{\"id\":".into())).await?;
                }
                if behaviour == Behaviour::EarlySnapshot {
                    send_scenario_messages(&mut ws, behaviour).await?;
                }
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

    if behaviour == Behaviour::InitialWithRemoved {
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
                        "entity_id": "sensor.cached",
                        "state": "42",
                        "attributes": {"unit_of_measurement": "example"},
                    },
                ],
            }}),
        )
        .await?;
        return Ok(());
    }

    if behaviour == Behaviour::Resync {
        send(
            ws,
            json!({"type": "event", "event": {
                "a": [
                    {
                        "entity_id": "light.example",
                        "state": "off",
                        "attributes": {},
                    },
                ],
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
