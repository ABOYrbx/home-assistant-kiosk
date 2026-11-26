//! Visual evidence for the Wave 3 acceptance gate.
//!
//! Run with:
//! `cargo test --test widget_screenshots -- --ignored --nocapture`

use ha_kiosk::state::{
    CalendarItem, CalendarView, DepartureItem, DeparturesView, MediaView, WeatherForecast,
    WeatherView,
};
use ha_kiosk::ui::bridge;
use ha_kiosk::{AppController, AppEvent, Config, HaEvent, MainWindow};
use i_slint_backend_testing::{TestingBackend, TestingBackendOptions};
use slint::{ComponentHandle, PhysicalSize, VecModel};
use std::rc::Rc;
use std::time::Instant;

#[test]
#[ignore = "writes visual evidence under .openchamber/screenshots/wave3"]
fn render_wave3_widget_fixtures_at_required_sizes() {
    slint::platform::set_platform(Box::new(TestingBackend::new(TestingBackendOptions {
        mock_time: false,
        threading: false,
        renderer_name: Some("software".into()),
    })))
    .expect("testing backend");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[ha]
url = "http://homeassistant.local:8123"
token = "test-token"

[widgets.hero_light]
enabled = true
entity = "light.example"
name = "Example lamp"

[widgets.weather]
enabled = true
entity = "weather.example"
forecast_type = "daily"
forecast_count = 3

[widgets.media]
enabled = true
entity = "media_player.example"

[widgets.departures]
enabled = true
entity = "sensor.departures"

[widgets.calendar]
enabled = true
entities = ["calendar.family", "calendar.work"]

[tiles]
columns = 3

[[tiles.items]]
entity = "light.example"
label = "Example lamp"

[[tiles.items]]
entity = "switch.example"
label = "Example switch"

[[tiles.items]]
entity = "sensor.example_temperature"
label = "Temperature"

[[tiles.items]]
entity = "scene.example"
label = "Evening"

[cache]
path = "{}"
"#,
            dir.path().join("cache.json").display()
        ),
    )
    .unwrap();
    let config = Config::load(&config_path).unwrap();
    let app = MainWindow::new().unwrap();
    let (commands, _commands_rx) = tokio::sync::mpsc::channel(32);
    let mut controller = AppController::new(app, config, commands);

    controller.apply(AppEvent::Ha(HaEvent::Connected {
        ha_version: "2026.11.0".into(),
        time_zone: "Europe/Berlin".parse().unwrap(),
    }));
    controller.apply(AppEvent::Ha(HaEvent::Snapshot(vec![
        ha_kiosk::events::HaEntity {
            entity_id: "light.example".into(),
            state: "on".into(),
            attributes: serde_json::json!({
                "friendly_name": "Example lamp",
                "brightness": 184,
                "color_temp_kelvin": 3200,
                "supported_color_modes": ["color_temp"]
            }),
            last_changed: None,
            last_updated: None,
        },
        ha_kiosk::events::HaEntity {
            entity_id: "switch.example".into(),
            state: "on".into(),
            attributes: serde_json::json!({}),
            last_changed: None,
            last_updated: None,
        },
        ha_kiosk::events::HaEntity {
            entity_id: "sensor.example_temperature".into(),
            state: "21.5".into(),
            attributes: serde_json::json!({"unit_of_measurement": "°C"}),
            last_changed: None,
            last_updated: None,
        },
        ha_kiosk::events::HaEntity {
            entity_id: "scene.example".into(),
            state: "2026-11-24T18:00:00+00:00".into(),
            attributes: serde_json::json!({}),
            last_changed: None,
            last_updated: None,
        },
    ])));
    controller.apply(AppEvent::Weather(Ok(WeatherView {
        entity_id: "weather.example".into(),
        condition: "partlycloudy".into(),
        temperature: Some(18.0),
        temperature_unit: "°C".into(),
        forecast: vec![
            WeatherForecast {
                datetime: "2026-11-25".into(),
                condition: "rainy".into(),
                temperature: Some(16.0),
                templow: Some(10.0),
            },
            WeatherForecast {
                datetime: "2026-11-26".into(),
                condition: "sunny".into(),
                temperature: Some(20.0),
                templow: Some(12.0),
            },
        ],
    })));
    controller.apply(AppEvent::Media(Some(MediaView {
        entity_id: "media_player.example".into(),
        state: "playing".into(),
        title: Some("Example track".into()),
        artist: Some("Example artist".into()),
        album: Some("Example album".into()),
        entity_picture: None,
        volume_level: Some(0.42),
        is_volume_muted: false,
        supported_features: 16437,
        available: true,
    })));
    controller.apply(AppEvent::Departures(Ok(DeparturesView {
        entity_id: "sensor.departures".into(),
        title: "Departures".into(),
        available: true,
        items: vec![
            DepartureItem {
                line: "42".into(),
                destination: "Central Station".into(),
                departure: "2026-11-24T18:05:00+01:00".into(),
                departure_utc: Some("2026-11-24T17:05:00+00:00".into()),
                delay_minutes: Some(2),
                platform: Some("2".into()),
                cancelled: false,
                remarks: None,
            },
            DepartureItem {
                line: "S1".into(),
                destination: "Airport".into(),
                departure: "2026-11-24T18:12:00+01:00".into(),
                departure_utc: Some("2026-11-24T17:12:00+00:00".into()),
                delay_minutes: None,
                platform: None,
                cancelled: true,
                remarks: Some("Replacement bus".into()),
            },
        ],
    })));
    let now = chrono::Utc::now();
    controller.apply(AppEvent::Calendar(Ok(CalendarView {
        items: vec![
            CalendarItem {
                calendar_id: "calendar.family".into(),
                summary: "Family dinner".into(),
                start: now + chrono::Duration::hours(2),
                end: Some(now + chrono::Duration::hours(3)),
                all_day: false,
                location: Some("Dining room".into()),
            },
            CalendarItem {
                calendar_id: "calendar.work".into(),
                summary: "Project review".into(),
                start: now + chrono::Duration::days(1),
                end: Some(now + chrono::Duration::days(1) + chrono::Duration::hours(1)),
                all_day: false,
                location: Some("Meeting room".into()),
            },
        ],
        errors: Vec::new(),
        last_success: Some(now),
        stale: false,
    })));
    bridge::apply_clock(controller.window(), "18:24".into(), "Tue 24 Nov".into());
    controller.tick_widgets(Instant::now(), now);

    let output =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".openchamber/screenshots/wave3");
    std::fs::create_dir_all(&output).unwrap();

    for (width, height) in [
        (800, 480),
        (1024, 600),
        (1280, 800),
        (1440, 900),
        (2048, 1200),
    ] {
        controller.window().set_current_page(0);
        save_snapshot(
            controller.window(),
            width,
            height,
            &output.join(format!("overview-{width}.png")),
        );
    }
    controller.window().set_current_page(1);
    save_snapshot(
        controller.window(),
        1024,
        600,
        &output.join("media-1024.png"),
    );
    controller.window().set_current_page(2);
    save_snapshot(
        controller.window(),
        1024,
        600,
        &output.join("agenda-1024.png"),
    );

    controller.window().set_current_page(3);
    save_snapshot(
        controller.window(),
        1024,
        600,
        &output.join("status-1024.png"),
    );
    save_snapshot(
        controller.window(),
        800,
        480,
        &output.join("status-800.png"),
    );

    let installed = ha_kiosk::ShopEntryView {
        id: "org.example.bus-departures".into(),
        name: "Bus Departures".into(),
        version: "1.0.0".into(),
        author: "Example Author".into(),
        license: "MIT".into(),
        description: "Shows upcoming bus departures for a configured stop.".into(),
        status: "Installed".into(),
        permissions: "api.example.com; no Home Assistant entities".into(),
        config_keys: "stop_id (required), limit (optional)".into(),
        config_path:
            "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]".into(),
        secret_path: "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml".into(),
        error: "".into(),
        installed: true,
        enabled: true,
        update_available: false,
        available: true,
    };
    let available = ha_kiosk::ShopEntryView {
        id: "org.example.stop-status".into(),
        name: "Stop Status".into(),
        version: "0.2.0".into(),
        author: "Example Author".into(),
        license: "MIT".into(),
        description: "Shows a compact key-value status for a configured stop.".into(),
        status: "Available".into(),
        permissions: "api.example.com; sensor.departures".into(),
        config_keys: "stop_id (required)".into(),
        config_path: "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.stop-status\"]"
            .into(),
        secret_path: "/etc/ha-kiosk/extensions/org.example.stop-status.secret.toml".into(),
        error: "".into(),
        installed: false,
        enabled: false,
        update_available: false,
        available: true,
    };
    controller.window().set_extensions_enabled(true);
    controller
        .window()
        .set_shop_installed(Rc::new(VecModel::from(vec![installed])).into());
    controller
        .window()
        .set_shop_available(Rc::new(VecModel::from(vec![available])).into());
    controller
        .window()
        .set_shop_config_path("/etc/ha-kiosk/config.toml".into());
    controller
        .window()
        .set_shop_secret_dir("/etc/ha-kiosk/extensions".into());
    controller.window().set_current_page(4);
    save_snapshot(
        controller.window(),
        1024,
        600,
        &output.join("shop-1024.png"),
    );
    save_snapshot(controller.window(), 800, 480, &output.join("shop-800.png"));
}

fn save_snapshot(app: &MainWindow, width: u32, height: u32, path: &std::path::Path) {
    app.window().set_size(PhysicalSize::new(width, height));
    slint::platform::update_timers_and_animations();
    let snapshot = app
        .window()
        .take_snapshot()
        .expect("software-renderer snapshot");
    image::save_buffer(
        path,
        snapshot.as_bytes(),
        snapshot.width(),
        snapshot.height(),
        image::ExtendedColorType::Rgba8,
    )
    .unwrap_or_else(|error| panic!("could not save {}: {error}", path.display()));
    println!(
        "saved {} ({}x{})",
        path.display(),
        snapshot.width(),
        snapshot.height()
    );
}
