use ha_kiosk::ha::models::{EntityChange, HaEntity};
use ha_kiosk::{AppController, AppEvent, Config, HaEvent, MainWindow};
use slint::Model;
use std::sync::Once;

static INIT_BACKEND: Once = Once::new();

fn init_backend() {
    INIT_BACKEND.call_once(i_slint_backend_testing::init_integration_test_with_system_time);
}

fn entity(id: &str) -> HaEntity {
    HaEntity {
        entity_id: id.to_string(),
        state: "on".to_string(),
        attributes: serde_json::json!({}),
        last_changed: None,
        last_updated: None,
    }
}

fn test_config() -> (Config, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_path = dir.path().join("cache.json");
    std::fs::write(
        &config_path,
        format!(
            r#"
[ha]
url = "https://ha.example.com"
token = "test-token"

[cache]
path = "{}"

[widgets.weather]
entity = "weather.home"
"#,
            cache_path.display()
        ),
    )
    .unwrap();
    (Config::load(&config_path).unwrap(), dir)
}

#[test]
fn controller_applies_events_and_updates_slint_on_the_ui_thread() {
    init_backend();

    let (config, _temp_dir) = test_config();
    let window = MainWindow::new().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let mut controller = AppController::new(window, config, tx);

    // The controller publishes the initial state so the shell has no blank
    // defaults before the first event arrives.
    {
        let window = controller.window();
        assert_eq!(window.get_connection_status().as_str(), "Offline");
        assert!(window.get_stale());
        assert_eq!(window.get_entity_count(), 0);
        assert_eq!(window.get_entity_ids().row_count(), 0);
    }

    controller.apply(AppEvent::Ha(HaEvent::Connected {
        ha_version: "2026.11.0".into(),
        time_zone: chrono_tz::Tz::UTC,
    }));
    {
        let window = controller.window();
        assert_eq!(window.get_connection_status().as_str(), "Connected");
        assert_eq!(window.get_ha_version().as_str(), "2026.11.0");
        assert!(!window.get_stale());
        assert!(!window.get_last_success().is_empty());
    }

    controller.apply(AppEvent::Ha(HaEvent::Snapshot(vec![
        entity("weather.home"),
        entity("light.example"),
    ])));
    controller.tick_widgets(std::time::Instant::now(), chrono::Utc::now());
    {
        let window = controller.window();
        assert_eq!(window.get_entity_count(), 2);
        let ids = window.get_entity_ids();
        assert_eq!(ids.row_count(), 2);
        assert_eq!(ids.row_data(0).unwrap().as_str(), "light.example");
        assert_eq!(ids.row_data(1).unwrap().as_str(), "weather.home");
    }

    controller.apply(AppEvent::Ha(HaEvent::EntitiesChanged(vec![EntityChange {
        entity_id: "light.example".into(),
        state: Some("off".into()),
        attributes: serde_json::json!({}),
    }])));
    controller.tick_widgets(std::time::Instant::now(), chrono::Utc::now());
    assert_eq!(controller.window().get_entity_count(), 2);

    controller.apply(AppEvent::Ha(HaEvent::EntitiesRemoved(vec![
        "light.example".into(),
    ])));
    controller.tick_widgets(std::time::Instant::now(), chrono::Utc::now());
    {
        let window = controller.window();
        assert_eq!(window.get_entity_count(), 1);
        assert_eq!(
            window.get_entity_ids().row_data(0).unwrap().as_str(),
            "weather.home"
        );
    }

    controller.apply(AppEvent::Ha(HaEvent::AuthFailed {
        message: "bad token".into(),
    }));
    assert_eq!(
        controller.window().get_connection_status().as_str(),
        "Authentication failed"
    );

    controller.apply(AppEvent::Ha(HaEvent::Disconnected {
        reason: "socket closed".into(),
    }));
    assert_eq!(
        controller.window().get_connection_status().as_str(),
        "Offline"
    );
    assert!(controller.window().get_stale());
}
