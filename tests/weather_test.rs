use ha_kiosk::config::WeatherWidgetConfig;
use ha_kiosk::ha::models::HaEntity;
use ha_kiosk::sources::weather::*;
use ha_kiosk::state::WeatherView;
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn fixture(name: &str) -> HaEntity {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(path).expect("fixture");
    serde_json::from_str(&raw).expect("fixture parses")
}

fn forecast_fixture(name: &str) -> serde_json::Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(path).expect("fixture");
    serde_json::from_str(&raw).expect("fixture parses")
}

#[test]
fn parses_weather_entity() {
    let parsed = parse_weather_entity(&fixture("weather_entity.json"));
    assert_eq!(parsed.entity_id, "weather.home");
    assert_eq!(parsed.condition, "sunny");
    assert_eq!(parsed.temperature, Some(21.5));
    assert_eq!(parsed.temperature_unit, "°C");
}

#[test]
fn unavailable_state_clears_condition() {
    let mut e = fixture("weather_entity.json");
    e.state = "unavailable".into();
    let parsed = parse_weather_entity(&e);
    assert_eq!(parsed.condition, "");
}

#[test]
fn parses_forecast_response() {
    let v = forecast_fixture("weather_forecast.json");
    let f = parse_forecast_response(&v).unwrap();
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].condition, "rainy");
    assert_eq!(f[0].temperature, Some(18.0));
    assert_eq!(f[0].templow, Some(12.0));
}

#[test]
fn parses_nested_forecast_response() {
    let v = json!({"weather.home": {"forecast": [{"datetime":"2026-11-23T12:00:00","condition":"rainy","temperature":"18.0"}]}});
    let f = parse_forecast_response(&v).unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].temperature, Some(18.0));
}

#[test]
fn weather_icon_maps_known_and_unknown() {
    assert_eq!(weather_icon("sunny"), "weather-clear-day.svg");
    assert_eq!(weather_icon("rainy"), "weather-rainy.svg");
    assert_eq!(weather_icon("not-a-condition"), "weather-unknown.svg");
    assert_eq!(condition_icon("fog"), "weather-fog.svg");
    assert_eq!(icon_for_condition("snowy"), "weather-snowy.svg");
    assert_eq!(icon_name("windy"), "weather-windy.svg");
}

#[test]
fn display_weather_states() {
    let loading = display_weather(None);
    assert_eq!(loading.status, WeatherStatus::Loading);

    let unavailable = WeatherView {
        entity_id: "weather.home".into(),
        ..Default::default()
    };
    let u = display_weather(Some(&unavailable));
    assert_eq!(u.status, WeatherStatus::Unavailable);

    let view = parse_weather_entity(&fixture("weather_entity.json"));
    let p = display_weather(Some(&view));
    assert_eq!(p.status, WeatherStatus::Populated);
    assert_eq!(p.condition, "sunny");
    assert_eq!(p.temperature, Some(21.5));
    assert_eq!(p.forecast.len(), 1);
    assert_eq!(p.forecast[0].condition, "rainy");
}

#[test]
fn refresh_cadence() {
    let start = Instant::now();
    let mut refresher = WeatherRefresher::new(900);
    assert!(refresher.should_refresh(start));
    assert!(refresher.begin(start));
    assert!(!refresher.begin(start));
    refresher.finish();
    assert!(!refresher.should_refresh(start + Duration::from_secs(30)));
    refresher.request_immediate();
    assert!(refresher.should_refresh(start + Duration::from_secs(30)));
}

#[test]
fn runtime_reads_entity() {
    let mut runtime = WeatherRuntime::new(WeatherWidgetConfig {
        enabled: true,
        entity: "weather.home".into(),
        forecast_type: "daily".into(),
        forecast_count: 3,
        refresh_seconds: 900,
    });
    let mut entities = HashMap::new();
    entities.insert("weather.home".to_string(), fixture("weather_entity.json"));
    let view = runtime
        .on_entities(&entities, chrono_tz::Tz::UTC, chrono::Utc::now())
        .expect("view");
    assert_eq!(view.condition, "sunny");
    assert!(
        runtime
            .on_entities(&entities, chrono_tz::Tz::UTC, chrono::Utc::now())
            .is_none()
    );
    assert!(matches!(
        runtime.on_tick(Instant::now(), chrono::Utc::now()),
        TickOutcome::Refreshed(_)
    ));
    assert_eq!(
        runtime.on_tick(Instant::now(), chrono::Utc::now()),
        TickOutcome::Idle
    );
    runtime.on_reconnect();
    assert!(matches!(
        runtime.on_tick(Instant::now(), chrono::Utc::now()),
        TickOutcome::Refreshed(_)
    ));
}

#[test]
fn runtime_forecast() {
    let mut runtime = WeatherRuntime::new(WeatherWidgetConfig {
        enabled: true,
        entity: "weather.home".into(),
        forecast_type: "daily".into(),
        forecast_count: 3,
        refresh_seconds: 900,
    });
    let mut entities = HashMap::new();
    entities.insert("weather.home".to_string(), fixture("weather_entity.json"));
    runtime.on_entities(&entities, chrono_tz::Tz::UTC, chrono::Utc::now());
    let changed = runtime.set_forecast(&forecast_fixture("weather_forecast.json"));
    assert!(changed.is_some());
    assert_eq!(runtime.view().unwrap().forecast.len(), 2);
}
