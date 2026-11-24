use crate::config::HaConfig;
use crate::events::{EntityId, HaError, HaEvent, ServiceResponse};
use crate::ha::reconnect::ReconnectBackoff;
use futures_util::{SinkExt, StreamExt};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;
use url::Url;

/// Default cadence of the JSON `ping`/`pong` heartbeat.
pub const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// How long to wait for the `pong` reply before the connection is considered dead.
pub const DEFAULT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_request_id() -> u64 {
    REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Tunables for the Home Assistant client.
///
/// Production uses the defaults (30 s heartbeat, 10 s timeout). Tests override
/// them to exercise the heartbeat without waiting half a minute.
#[derive(Debug, Clone)]
pub struct HaClientOptions {
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
}

impl Default for HaClientOptions {
    fn default() -> Self {
        Self {
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
            heartbeat_timeout: DEFAULT_HEARTBEAT_TIMEOUT,
        }
    }
}

/// Derive the WebSocket endpoint from the configured HTTP(S) base URL.
///
/// A path prefix on the base URL is preserved: `https://host/ha` becomes
/// `wss://host/ha/api/websocket`. Query strings and fragments are dropped.
pub fn websocket_url(base: &Url) -> Result<Url, HaError> {
    let mut url = base.clone();
    let scheme = match base.scheme() {
        "https" => "wss",
        "http" => "ws",
        other => {
            return Err(HaError::Connection(format!(
                "unsupported scheme for websocket derivation: {}",
                other
            )));
        }
    };
    if url.set_scheme(scheme).is_err() {
        return Err(HaError::Connection("failed to set websocket scheme".into()));
    }
    let prefix = base.path().trim_end_matches('/');
    let path = if prefix.is_empty() {
        "/api/websocket".to_string()
    } else {
        format!("{}/api/websocket", prefix)
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn tls_connector(config: &HaConfig) -> Option<tokio_tungstenite::Connector> {
    if !config.allow_invalid_certs {
        return None;
    }
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
    use std::sync::Arc;

    #[derive(Debug)]
    struct NoVerifier;
    impl ServerCertVerifier for NoVerifier {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::ECDSA_NISTP521_SHA512,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
                SignatureScheme::ED25519,
            ]
        }
    }

    let cfg = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    Some(tokio_tungstenite::Connector::Rustls(Arc::new(cfg)))
}

pub async fn run(
    config: HaConfig,
    entity_ids: Vec<EntityId>,
    event_tx: mpsc::Sender<HaEvent>,
    command_rx: mpsc::Receiver<crate::events::HaCommand>,
) {
    run_with_options(
        config,
        entity_ids,
        event_tx,
        command_rx,
        HaClientOptions::default(),
    )
    .await;
}

pub async fn run_with_options(
    config: HaConfig,
    entity_ids: Vec<EntityId>,
    event_tx: mpsc::Sender<HaEvent>,
    mut command_rx: mpsc::Receiver<crate::events::HaCommand>,
    options: HaClientOptions,
) {
    let token = config.token.expose_secret().to_string();
    let mut backoff = ReconnectBackoff::new();
    loop {
        match connect_once(
            &config,
            &entity_ids,
            &event_tx,
            &mut command_rx,
            &options,
            &mut backoff,
        )
        .await
        {
            // A clean shutdown never reconnects.
            Ok(()) => break,
            Err(HaError::AuthFailed(message)) => {
                let message = redact(&message, &token);
                let _ = event_tx.send(HaEvent::AuthFailed { message }).await;
                break;
            }
            Err(e) => {
                let reason = redact(&e.to_string(), &token);
                let _ = event_tx.send(HaEvent::Disconnected { reason }).await;
                tokio::time::sleep(backoff.next_delay()).await;
            }
        }
    }
}

/// Replace the configured token with a placeholder if a remote error echoes it.
fn redact(text: &str, token: &str) -> String {
    if token.is_empty() {
        text.to_string()
    } else {
        text.replace(token, "[REDACTED]")
    }
}

async fn connect_once(
    config: &HaConfig,
    entity_ids: &[EntityId],
    event_tx: &mpsc::Sender<HaEvent>,
    command_rx: &mut mpsc::Receiver<crate::events::HaCommand>,
    options: &HaClientOptions,
    backoff: &mut ReconnectBackoff,
) -> Result<(), HaError> {
    let ws_url = websocket_url(&config.url)?;

    let connector = if ws_url.scheme() == "wss" {
        tls_connector(config)
    } else {
        None
    };
    let (mut ws, _resp) = if let Some(conn) = connector {
        tokio_tungstenite::connect_async_tls_with_config(ws_url.as_str(), None, false, Some(conn))
            .await
            .map_err(|e| HaError::Connection(e.to_string()))?
    } else {
        tokio_tungstenite::connect_async(ws_url.as_str())
            .await
            .map_err(|e| HaError::Connection(e.to_string()))?
    };

    // auth_required -> auth -> auth_ok | auth_invalid
    let auth_required = read_json(&mut ws).await?;
    if auth_required.get("type").and_then(|v| v.as_str()) != Some("auth_required") {
        return Err(HaError::Protocol("expected auth_required".into()));
    }
    let ha_version_auth = auth_required
        .get("ha_version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let auth_msg = json!({
        "type": "auth",
        "access_token": config.token.expose_secret(),
    });
    ws.send(Message::Text(auth_msg.to_string().into()))
        .await
        .map_err(|e| HaError::Connection(e.to_string()))?;

    let auth_resp = read_json(&mut ws).await?;
    match auth_resp.get("type").and_then(|v| v.as_str()) {
        Some("auth_ok") => {}
        Some("auth_invalid") => {
            let msg = auth_resp
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("invalid token")
                .to_string();
            return Err(HaError::AuthFailed(msg));
        }
        other => {
            return Err(HaError::Protocol(format!(
                "unexpected auth response: {:?}",
                other
            )));
        }
    }
    backoff.reset();

    let ha_version = auth_resp
        .get("ha_version")
        .and_then(|v| v.as_str())
        .unwrap_or(&ha_version_auth)
        .to_string();

    // get_config supplies the IANA timezone; subscribe_entities supplies the
    // configured entity set. Both are sent immediately after authentication.
    let cfg_id = next_request_id();
    ws.send(Message::Text(
        json!({"id": cfg_id, "type": "get_config"})
            .to_string()
            .into(),
    ))
    .await
    .map_err(|e| HaError::Connection(e.to_string()))?;

    let sub_id = next_request_id();
    ws.send(Message::Text(
        json!({
            "id": sub_id,
            "type": "subscribe_entities",
            "entity_ids": entity_ids,
        })
        .to_string()
        .into(),
    ))
    .await
    .map_err(|e| HaError::Connection(e.to_string()))?;

    let mut time_zone: chrono_tz::Tz = config.time_zone.unwrap_or(chrono_tz::Tz::UTC);
    let mut initial_snapshot_seen = false;

    // Wait for the get_config result before announcing the connection. Events
    // that arrive first (the subscribe snapshot) are still applied.
    loop {
        let msg = read_json(&mut ws).await?;
        match msg.get("type").and_then(|v| v.as_str()) {
            Some("result") => {
                let id = msg.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                if id != cfg_id {
                    continue;
                }
                let success = msg
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if success
                    && let Some(tz) = msg
                        .get("result")
                        .and_then(|r| r.get("time_zone"))
                        .and_then(|v| v.as_str())
                    && let Ok(parsed) = tz.parse::<chrono_tz::Tz>()
                {
                    time_zone = parsed;
                }
                let _ = event_tx
                    .send(HaEvent::Connected {
                        ha_version: ha_version.clone(),
                        time_zone,
                    })
                    .await;
                break;
            }
            Some("event") => {
                let event = msg.get("event").cloned().unwrap_or(Value::Null);
                process_event(&event, event_tx, &mut initial_snapshot_seen).await;
            }
            _ => {}
        }
    }

    // Keep one heartbeat in flight at a time. A missing pong past the timeout
    // tears the connection down so the reconnect loop can restore it.
    let mut ping_timer = tokio::time::interval_at(
        Instant::now() + options.heartbeat_interval,
        options.heartbeat_interval,
    );
    ping_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut awaiting_pong: Option<Instant> = None;
    let mut pending_get_states: HashSet<u64> = HashSet::new();
    let mut pending_services: HashSet<u64> = HashSet::new();
    let token = config.token.expose_secret().to_string();

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let v: Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        match v.get("type").and_then(|x| x.as_str()) {
                            Some("pong") => {
                                awaiting_pong = None;
                            }
                            Some("result") => {
                                let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
                                if pending_get_states.remove(&id) {
                                    let success =
                                        v.get("success").and_then(|x| x.as_bool()).unwrap_or(false);
                                    if success {
                                        let entities = v
                                            .get("result")
                                            .and_then(|r| r.as_array())
                                            .map(|arr| {
                                                arr.iter().filter_map(parse_entity).collect()
                                            })
                                            .unwrap_or_default();
                                        initial_snapshot_seen = true;
                                        let _ = event_tx.send(HaEvent::Snapshot(entities)).await;
                                    }
                                } else if pending_services.remove(&id) {
                                    let success = v
                                        .get("success")
                                        .and_then(|x| x.as_bool())
                                        .unwrap_or(false);
                                    let result = if success {
                                        Ok(ServiceResponse {
                                            data: v.get("result").cloned().unwrap_or(Value::Null),
                                        })
                                    } else {
                                        let message = v
                                            .get("error")
                                            .and_then(|e| e.get("message"))
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("service failed");
                                        Err(HaError::Service(redact(message, &token)))
                                    };
                                    let _ = event_tx
                                        .send(HaEvent::ServiceResult { request_id: id, result })
                                        .await;
                                }
                                // Other results are acknowledgements (subscribe_entities).
                            }
                            Some("event") => {
                                let event = v.get("event").cloned().unwrap_or(Value::Null);
                                process_event(&event, event_tx, &mut initial_snapshot_seen).await;
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err(HaError::Connection("socket closed".into()));
                    }
                    Some(Err(e)) => {
                        return Err(HaError::Connection(e.to_string()));
                    }
                    _ => {}
                }
            }
            cmd = command_rx.recv() => {
                match cmd {
                    Some(crate::events::HaCommand::CallService(call)) => {
                        let mut message = json!({
                            "id": call.request_id,
                            "type": "call_service",
                            "domain": call.domain,
                            "service": call.service,
                            "service_data": call.data,
                        });
                        if let Some(target) = call.target {
                            message["target"] = target;
                        }
                        if let Some(obj) = message.as_object_mut() {
                            obj.insert(
                                "return_response".to_string(),
                                Value::Bool(call.return_response),
                            );
                        }
                        pending_services.insert(call.request_id);
                        if let Err(e) = ws.send(Message::Text(message.to_string().into())).await {
                            pending_services.remove(&call.request_id);
                            return Err(HaError::Connection(e.to_string()));
                        }
                    }
                    Some(crate::events::HaCommand::RefreshSnapshot) => {
                        let id = next_request_id();
                        pending_get_states.insert(id);
                        let msg = json!({"id": id, "type": "get_states"});
                        if let Err(e) = ws.send(Message::Text(msg.to_string().into())).await {
                            pending_get_states.remove(&id);
                            return Err(HaError::Connection(e.to_string()));
                        }
                    }
                    Some(crate::events::HaCommand::Shutdown) => {
                        return Ok(());
                    }
                    None => {
                        return Ok(());
                    }
                }
            }
            _ = ping_timer.tick() => {
                let now = Instant::now();
                if let Some(sent_at) = awaiting_pong {
                    if now.duration_since(sent_at) >= options.heartbeat_timeout {
                        return Err(HaError::Connection("heartbeat timeout".into()));
                    }
                    continue;
                }
                let msg = json!({"id": next_request_id(), "type": "ping"});
                if let Err(e) = ws.send(Message::Text(msg.to_string().into())).await {
                    return Err(HaError::Connection(e.to_string()));
                }
                awaiting_pong = Some(now);
            }
        }
    }
}

/// Decode an `event` payload from `subscribe_entities`.
///
/// The first `a` batch after a subscription is the full snapshot; later `a`
/// batches are incremental additions. `c` carries changed states and attribute
/// diffs (including removed attributes), `r` carries removed entity IDs.
async fn process_event(
    event: &Value,
    event_tx: &mpsc::Sender<HaEvent>,
    initial_snapshot_seen: &mut bool,
) {
    if let Some(added) = event.get("a").and_then(|x| x.as_array()) {
        let entities: Vec<crate::events::HaEntity> =
            added.iter().filter_map(parse_entity).collect();
        if *initial_snapshot_seen {
            if !entities.is_empty() {
                let _ = event_tx.send(HaEvent::EntitiesAdded(entities)).await;
            }
        } else {
            *initial_snapshot_seen = true;
            let _ = event_tx.send(HaEvent::Snapshot(entities)).await;
        }
    }
    if let Some(changed) = event.get("c").and_then(|x| x.as_array()) {
        let changes: Vec<crate::events::EntityChange> =
            changed.iter().filter_map(parse_change).collect();
        if !changes.is_empty() {
            let _ = event_tx.send(HaEvent::EntitiesChanged(changes)).await;
        }
    }
    if let Some(removed) = event.get("r").and_then(|x| x.as_array()) {
        let ids: Vec<EntityId> = removed
            .iter()
            .filter_map(|r| {
                r.get("entity_id")
                    .and_then(|x| x.as_str())
                    .map(str::to_string)
                    .or_else(|| r.as_str().map(str::to_string))
            })
            .collect();
        if !ids.is_empty() {
            let _ = event_tx.send(HaEvent::EntitiesRemoved(ids)).await;
        }
    }
}

async fn read_json<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> Result<Value, HaError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(|e| HaError::Protocol(e.to_string()));
            }
            Some(Ok(Message::Close(_))) | None => {
                return Err(HaError::Connection("socket closed".into()));
            }
            Some(Err(e)) => {
                return Err(HaError::Connection(e.to_string()));
            }
            _ => continue,
        }
    }
}

fn parse_entity(v: &Value) -> Option<crate::events::HaEntity> {
    let entity_id = v.get("entity_id")?.as_str()?.to_string();
    let state = v
        .get("state")
        .and_then(|x| x.as_str())
        .unwrap_or("unknown")
        .to_string();
    let attributes = v
        .get("attributes")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    let last_changed = v
        .get("last_changed")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let last_updated = v
        .get("last_updated")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    Some(crate::events::HaEntity {
        entity_id,
        state,
        attributes,
        last_changed,
        last_updated,
    })
}

fn parse_change(v: &Value) -> Option<crate::events::EntityChange> {
    let entity_id = v.get("entity_id")?.as_str()?.to_string();
    // Home Assistant's `subscribe_entities` uses compact diffs (`s`, `+`, `-`),
    // but full `state`/`attributes` objects are accepted as well.
    let state = v
        .get("s")
        .or_else(|| v.get("state"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let mut attributes = serde_json::Map::new();
    if let Some(changed) = v
        .get("+")
        .or_else(|| v.get("attributes"))
        .and_then(|x| x.as_object())
    {
        for (key, value) in changed {
            attributes.insert(key.clone(), value.clone());
        }
    }
    if let Some(removed) = v.get("-") {
        match removed {
            Value::Array(keys) => {
                for key in keys.iter().filter_map(|k| k.as_str()) {
                    attributes.insert(key.to_string(), Value::Null);
                }
            }
            Value::Object(keys) => {
                for key in keys.keys() {
                    attributes.insert(key.clone(), Value::Null);
                }
            }
            _ => {}
        }
    }

    Some(crate::events::EntityChange {
        entity_id,
        state,
        attributes: Value::Object(attributes),
    })
}
