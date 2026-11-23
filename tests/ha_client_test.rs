use futures_util::{SinkExt, StreamExt};
use ha_kiosk::events::HaEvent;
use ha_kiosk::ha;
use secrecy::SecretString;
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn auth_and_snapshot_and_change() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(Message::Text(
            json!({"type":"auth_required","ha_version":"2024.1.0"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        // read auth
        let _ = ws.next().await.unwrap().unwrap();
        ws.send(Message::Text(
            json!({"type":"auth_ok","ha_version":"2024.1.0"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();

        // handle requests
        loop {
            let msg = ws.next().await;
            let msg = match msg {
                Some(Ok(m)) => m,
                _ => break,
            };
            let text = match msg {
                Message::Text(t) => t.to_string(),
                _ => continue,
            };
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            match v.get("type").and_then(|x| x.as_str()) {
                Some("get_config") => {
                    let id = v.get("id").unwrap().as_u64().unwrap();
                    ws.send(Message::Text(
                        json!({"id":id,"type":"result","success":true,"result":{"time_zone":"UTC"}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                }
                Some("subscribe_entities") => {
                    let id = v.get("id").unwrap().as_u64().unwrap();
                    ws.send(Message::Text(
                        json!({"id":id,"type":"result","success":true,"result":null})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                    ws.send(Message::Text(
                        json!({"type":"event","event":{"a":[{"entity_id":"light.example","state":"on","attributes":{"brightness":100}}]}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                    ws.send(Message::Text(
                        json!({"type":"event","event":{"c":[{"entity_id":"light.example","state":"off","attributes":{"brightness":null}}]}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                    break;
                }
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    });

    let (tx, mut rx) = mpsc::channel(100);
    let (_cmd_tx, cmd_rx) = mpsc::channel(10);
    let config = ha_kiosk::config::HaConfig {
        url: url::Url::parse(&format!("http://{}", addr)).unwrap(),
        token: SecretString::from("test".to_string()),
        token_file: None,
        allow_invalid_certs: false,
        extra_entity_ids: vec![],
        time_zone: None,
    };
    tokio::spawn(async move {
        ha::run(config, vec!["light.example".to_string()], tx, cmd_rx).await;
    });

    let mut events = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(e) = rx.recv().await {
            events.push(e);
            if events.len() >= 4 {
                break;
            }
        }
    })
    .await;

    server.await.unwrap();

    // assertions
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HaEvent::Connected { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HaEvent::EntitiesAdded(v) if !v.is_empty()))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HaEvent::EntitiesChanged(v) if !v.is_empty()))
    );
}

#[tokio::test]
async fn path_prefixed_url() {
    let u = url::Url::parse("https://ha.example.com/ha").unwrap();
    let ws = ha::websocket_url(&u).unwrap();
    assert_eq!(ws.as_str(), "wss://ha.example.com/ha/api/websocket");
}

#[tokio::test]
async fn auth_invalid_stops() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(Message::Text(
            json!({"type":"auth_required","ha_version":"2024.1.0"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let _ = ws.next().await.unwrap().unwrap();
        ws.send(Message::Text(
            json!({"type":"auth_invalid","message":"bad token"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let (tx, mut rx) = mpsc::channel(100);
    let (_cmd_tx, cmd_rx) = mpsc::channel(10);
    let config = ha_kiosk::config::HaConfig {
        url: url::Url::parse(&format!("http://{}", addr)).unwrap(),
        token: SecretString::from("test".to_string()),
        token_file: None,
        allow_invalid_certs: false,
        extra_entity_ids: vec![],
        time_zone: None,
    };
    tokio::spawn(async move {
        ha::run(config, vec![], tx, cmd_rx).await;
    });

    let mut events = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(e) = rx.recv().await {
            events.push(e);
        }
    })
    .await;

    server.await.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HaEvent::AuthFailed { .. }))
    );
}
