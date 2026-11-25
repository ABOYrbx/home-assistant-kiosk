use crate::config::HaConfig;
use crate::events::{EntityId, HaError, HaEvent, ServiceResponse};
use crate::ha::reconnect::ReconnectBackoff;
use futures_util::{SinkExt, StreamExt};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_request_id() -> u64 {
    REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
}

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
    let path = if prefix.is_empty() || prefix == "/" {
        "/api/websocket".to_string()
    } else {
        format!("{}/api/websocket", prefix)
    };
    url.set_path(&path);
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
    mut command_rx: mpsc::Receiver<crate::events::HaCommand>,
) {
    let mut backoff = ReconnectBackoff::new();
    loop {
        match connect_once(&config, &entity_ids, &event_tx, &mut command_rx).await {
            Ok(()) => break,
            Err(HaError::AuthFailed(msg)) => {
                let _ = event_tx.send(HaEvent::AuthFailed { message: msg }).await;
                break;
            }
            Err(e) => {
                let reason = e.to_string();
                let _ = event_tx.send(HaEvent::Disconnected { reason }).await;
                let delay = backoff.next_delay();
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn connect_once(
    config: &HaConfig,
    entity_ids: &[EntityId],
    event_tx: &mpsc::Sender<HaEvent>,
    command_rx: &mut mpsc::Receiver<crate::events::HaCommand>,
) -> Result<(), HaError> {
    let ws_url = websocket_url(&config.url)?;

    let connector = tls_connector(config);
    let (mut ws, _resp) = if let Some(conn) = connector {
        tokio_tungstenite::connect_async_tls_with_config(ws_url.as_str(), None, false, Some(conn))
            .await
            .map_err(|e| HaError::Connection(e.to_string()))?
    } else {
        tokio_tungstenite::connect_async(ws_url.as_str())
            .await
            .map_err(|e| HaError::Connection(e.to_string()))?
    };

    // auth_required
    let auth_required = read_json(&mut ws).await?;
    if auth_required.get("type").and_then(|v| v.as_str()) != Some("auth_required") {
        return Err(HaError::Protocol("expected auth_required".into()));
    }
    let ha_version_auth = auth_required
        .get("ha_version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // auth
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
    let ha_version = auth_resp
        .get("ha_version")
        .and_then(|v| v.as_str())
        .unwrap_or(&ha_version_auth)
        .to_string();

    // get_config + subscribe
    let cfg_id = next_request_id();
    let get_config_msg = json!({"id": cfg_id, "type": "get_config"});
    ws.send(Message::Text(get_config_msg.to_string().into()))
        .await
        .map_err(|e| HaError::Connection(e.to_string()))?;

    let mut time_zone: chrono_tz::Tz = config.time_zone.unwrap_or(chrono_tz::Tz::UTC);
    let mut setup_events = Vec::new();
    loop {
        let msg = read_json(&mut ws).await?;
        if msg.get("type").and_then(|v| v.as_str()) == Some("result") {
            let id = msg.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            if id == cfg_id {
                if let Some(result) = msg.get("result")
                    && let Some(tz) = result.get("time_zone").and_then(|v| v.as_str())
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
            continue;
        }
        if msg.get("type").and_then(|v| v.as_str()) == Some("event") {
            setup_events.push(msg.get("event").cloned().unwrap_or(Value::Null));
        }
    }

    // Entity data must not outrun the server config: the UI installs the Home
    // Assistant timezone on Connected before it applies any setup events.
    for event in setup_events {
        process_event(&event, event_tx).await;
    }

    let sub_id = next_request_id();
    let sub_msg = json!({
        "id": sub_id,
        "type": "subscribe_entities",
        "entity_ids": entity_ids,
    });
    ws.send(Message::Text(sub_msg.to_string().into()))
        .await
        .map_err(|e| HaError::Connection(e.to_string()))?;

    // main loop
    let mut ping_timer = tokio::time::interval(Duration::from_secs(30));
    ping_timer.reset();

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let v: Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if v.get("type").and_then(|x| x.as_str()) == Some("pong") {
                            continue;
                        }
                        if v.get("type").and_then(|x| x.as_str()) == Some("result") {
                            let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
                            if id == sub_id {
                                // The subscription acknowledgement is protocol
                                // setup, not the result of a user action.
                                continue;
                            }
                            // get_states response -> snapshot
                            // We don't track type; if result is array of entities, emit snapshot.
                            if let Some(arr) = v.get("result").and_then(|r| r.as_array())
                                && !arr.is_empty()
                                && arr[0].get("entity_id").is_some()
                            {
                                let mut entities = Vec::new();
                                for e in arr {
                                    if let Some(ent) = parse_entity(e) {
                                        entities.push(ent);
                                    }
                                }
                                let _ = event_tx.send(HaEvent::Snapshot(entities)).await;
                                continue;
                            }
                            let success = v.get("success").and_then(|x| x.as_bool()).unwrap_or(false);
                            let result = if success {
                                let data = v.get("result").cloned().unwrap_or(Value::Null);
                                Ok(ServiceResponse { data })
                            } else {
                                let m = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("service failed").to_string();
                                Err(HaError::Service(m))
                            };
                            let _ = event_tx.send(HaEvent::ServiceResult { request_id: id, result }).await;
                            continue;
                        }
                        if v.get("type").and_then(|x| x.as_str()) == Some("event") {
                            let event = v.get("event").cloned().unwrap_or(Value::Null);
                            process_event(&event, event_tx).await;
                            continue;
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
                        let msg = json!({
                            "id": call.request_id,
                            "type": "call_service",
                            "domain": call.domain,
                            "service": call.service,
                            "target": call.target,
                            "service_data": call.data,
                            "return_response": call.return_response,
                        });
                        if let Err(e) = ws.send(Message::Text(msg.to_string().into())).await {
                            return Err(HaError::Connection(e.to_string()));
                        }
                    }
                    Some(crate::events::HaCommand::RefreshSnapshot) => {
                        let id = next_request_id();
                        let msg = json!({"id": id, "type": "get_states"});
                        if let Err(e) = ws.send(Message::Text(msg.to_string().into())).await {
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
                let id = next_request_id();
                let msg = json!({"id": id, "type": "ping"});
                if let Err(e) = ws.send(Message::Text(msg.to_string().into())).await {
                    return Err(HaError::Connection(e.to_string()));
                }
            }
        }
    }
}

async fn process_event(event: &Value, event_tx: &mpsc::Sender<HaEvent>) {
    if let Some(added) = event.get("a").and_then(|x| x.as_array()) {
        let mut entities = Vec::new();
        for e in added {
            if let Some(ent) = parse_entity(e) {
                entities.push(ent);
            }
        }
        if !entities.is_empty() {
            let _ = event_tx.send(HaEvent::EntitiesAdded(entities)).await;
        }
    }
    if let Some(changed) = event.get("c").and_then(|x| x.as_array()) {
        let mut changes = Vec::new();
        for c in changed {
            if let Some(ch) = parse_change(c) {
                changes.push(ch);
            }
        }
        if !changes.is_empty() {
            let _ = event_tx.send(HaEvent::EntitiesChanged(changes)).await;
        }
    }
    if let Some(removed) = event.get("r").and_then(|x| x.as_array()) {
        let mut ids = Vec::new();
        for r in removed {
            if let Some(id) = r.get("entity_id").and_then(|x| x.as_str()) {
                ids.push(id.to_string());
            } else if let Some(id) = r.as_str() {
                ids.push(id.to_string());
            }
        }
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
    let state = v
        .get("state")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let attributes = v.get("attributes").cloned().unwrap_or(Value::Null);
    Some(crate::events::EntityChange {
        entity_id,
        state,
        attributes,
    })
}
