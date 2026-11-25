//! Hero-light view model states behind the consolidated Overview page.
//!
//! The shell no longer exposes a `KioskModel` global, so the states the hero
//! card renders are asserted at the view-model layer instead.

use ha_kiosk::config::HeroLightConfig;
use ha_kiosk::events::HaEntity;
use ha_kiosk::ui::hero::HeroLight;

fn config() -> HeroLightConfig {
    HeroLightConfig {
        enabled: true,
        entity: "light.example".into(),
        name: "Example lamp".into(),
        min_kelvin: 2700,
        max_kelvin: 6500,
    }
}

fn light(state: &str) -> HaEntity {
    HaEntity {
        entity_id: "light.example".into(),
        state: state.to_string(),
        attributes: serde_json::json!({
            "friendly_name": "Example lamp",
            "brightness": 184,
            "color_temp_kelvin": 3200,
            "supported_color_modes": ["color_temp"]
        }),
        last_changed: None,
        last_updated: None,
    }
}

#[test]
fn the_hero_reports_loading_populated_and_unavailable_states() {
    let mut hero = HeroLight::new(&config());
    let loading = hero.view_model();
    assert!(loading.configured);
    assert!(loading.loading);
    assert!(loading.status_text.contains("Waiting"));

    hero.apply_entity(Some(&light("on")));
    let ready = hero.view_model();
    assert!(!ready.loading);
    assert!(ready.available);
    assert!(ready.on);
    assert_eq!(ready.name, "Example lamp");
    assert_eq!(ready.brightness_text, "72%");
    assert_eq!(ready.color_temp_text, "3200 K");
    assert_eq!(ready.state_text, "On");

    hero.apply_entity(Some(&light("unavailable")));
    let unavailable = hero.view_model();
    assert!(!unavailable.available);
    assert!(unavailable.status_text.contains("Unavailable"));
}

#[test]
fn an_unconfigured_hero_has_an_empty_state() {
    let hero = HeroLight::new(&HeroLightConfig::default());
    let view = hero.view_model();
    assert!(!view.configured);
    assert!(!view.available);
    assert!(view.status_text.contains("No light configured"));
}
