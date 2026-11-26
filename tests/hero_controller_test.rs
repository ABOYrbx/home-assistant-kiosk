//! Hero-light command flow: debounced slider calls and pending state.
//!
//! The consolidated controller no longer binds a `KioskModel` global. The
//! command builder it used is tested here directly instead.

use ha_kiosk::config::HeroLightConfig;
use ha_kiosk::events::{HaEntity, PendingAction};
use ha_kiosk::ui::hero::HeroLight;
use serde_json::json;
use std::time::{Duration, Instant};

fn config() -> HeroLightConfig {
    HeroLightConfig {
        enabled: true,
        entity: "light.example".into(),
        name: "Example lamp".into(),
        min_kelvin: 2700,
        max_kelvin: 6500,
    }
}

fn populated_hero() -> HeroLight {
    let mut hero = HeroLight::new(&config());
    hero.apply_entity(Some(&HaEntity {
        entity_id: "light.example".into(),
        state: "on".into(),
        attributes: json!({
            "brightness": 184,
            "color_temp_kelvin": 3200,
            "supported_color_modes": ["color_temp"]
        }),
        last_changed: None,
        last_updated: None,
    }));
    hero
}

#[test]
fn toggle_uses_light_toggle_and_ignores_repeats_until_finished() {
    let mut hero = populated_hero();
    let command = hero.on_toggle().expect("a toggle command");
    assert_eq!(command.call.domain, "light");
    assert_eq!(command.call.service, "toggle");
    assert_eq!(
        command.call.target,
        Some(json!({ "entity_id": "light.example" }))
    );
    assert!(matches!(command.action, PendingAction::HeroToggle { .. }));
    assert!(hero.view_model().pending);
    assert!(
        hero.on_toggle().is_none(),
        "a pending toggle ignores repeats"
    );

    hero.service_finished(command.call.request_id);
    assert!(!hero.has_pending());
    assert!(
        hero.on_toggle().is_some(),
        "a finished toggle can be repeated"
    );
}

#[test]
fn slider_changes_are_debounced_and_clamped_to_the_light_range() {
    let mut hero = populated_hero();
    let now = Instant::now();

    let first = hero.on_brightness_change(40, now).expect("first change");
    assert_eq!(first.call.data, json!({ "brightness_pct": 40 }));
    assert!(
        hero.on_brightness_change(50, now + Duration::from_millis(40))
            .is_none(),
        "rapid changes are throttled"
    );
    let released = hero
        .on_brightness_release(70, now + Duration::from_millis(90))
        .expect("release");
    assert_eq!(released.call.data, json!({ "brightness_pct": 70 }));

    let low = hero.on_color_temp_change(1000, now).expect("low kelvin");
    assert_eq!(low.call.data, json!({ "color_temp_kelvin": 2700 }));
    let high = hero
        .on_color_temp_release(9000, now + Duration::from_millis(100))
        .expect("high kelvin");
    assert_eq!(high.call.data, json!({ "color_temp_kelvin": 6500 }));
}

#[test]
fn an_unavailable_light_emits_no_commands() {
    let mut hero = HeroLight::new(&config());
    hero.apply_entity(Some(&HaEntity {
        entity_id: "light.example".into(),
        state: "unavailable".into(),
        attributes: json!({}),
        last_changed: None,
        last_updated: None,
    }));
    let now = Instant::now();
    assert!(hero.on_toggle().is_none());
    assert!(hero.on_brightness_change(50, now).is_none());
    assert!(hero.on_color_temp_change(3000, now).is_none());
}

#[test]
fn a_pending_slider_value_survives_a_stale_entity_update() {
    let mut hero = populated_hero();
    let now = Instant::now();
    hero.on_brightness_change(80, now)
        .expect("a brightness call");
    hero.apply_entity(Some(&HaEntity {
        entity_id: "light.example".into(),
        state: "on".into(),
        attributes: json!({ "supported_color_modes": ["color_temp"] }),
        last_changed: None,
        last_updated: None,
    }));
    assert_eq!(hero.view_model().brightness, 80);
    assert!(hero.has_pending());
    assert_eq!(hero.view_model().status_text, "");
}
