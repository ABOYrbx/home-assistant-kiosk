use crate::config::HeroLightConfig;
use crate::events::{HaEntity, PendingAction, ServiceCall};
use crate::ha::client::next_request_id;
use crate::sources::lights::parse_light_entity;
use crate::state::LightView;
use crate::ui::debounce::SliderDebounce;
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const SLIDER_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroPendingKind {
    Toggle,
    Brightness,
    ColorTemp,
}

#[derive(Debug, Clone)]
pub struct HeroCommand {
    pub call: ServiceCall,
    pub action: PendingAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroViewModel {
    pub configured: bool,
    pub name: String,
    pub on: bool,
    pub brightness: i32,
    pub color_temp: i32,
    pub min_kelvin: i32,
    pub max_kelvin: i32,
    pub available: bool,
    pub loading: bool,
    pub pending: bool,
    pub brightness_supported: bool,
    pub color_temp_supported: bool,
    pub brightness_text: String,
    pub color_temp_text: String,
    pub state_text: String,
    pub status_text: String,
    pub error: bool,
}

pub struct HeroLight {
    configured: bool,
    entity_id: String,
    configured_name: String,
    config_min_kelvin: u16,
    config_max_kelvin: u16,
    view: Option<LightView>,
    loading: bool,
    error: Option<String>,
    brightness_debounce: SliderDebounce,
    color_temp_debounce: SliderDebounce,
    brightness_value: u32,
    color_temp_value: u32,
    pending: HashMap<u64, HeroPendingKind>,
    toggle_pending: bool,
}

impl HeroLight {
    pub fn new(config: &HeroLightConfig) -> Self {
        let configured = config.enabled && !config.entity.is_empty();
        let midpoint = (config.min_kelvin as u32 + config.max_kelvin as u32) / 2;
        Self {
            configured,
            entity_id: config.entity.clone(),
            configured_name: config.name.clone(),
            config_min_kelvin: config.min_kelvin,
            config_max_kelvin: config.max_kelvin,
            view: None,
            loading: configured,
            error: None,
            brightness_debounce: SliderDebounce::new(SLIDER_INTERVAL),
            color_temp_debounce: SliderDebounce::new(SLIDER_INTERVAL),
            brightness_value: 0,
            color_temp_value: midpoint,
            pending: HashMap::new(),
            toggle_pending: false,
        }
    }

    pub fn entity_id(&self) -> &str {
        &self.entity_id
    }

    pub fn is_configured(&self) -> bool {
        self.configured
    }

    pub fn apply_entity(&mut self, entity: Option<&HaEntity>) {
        if !self.configured {
            self.view = None;
            self.loading = false;
            return;
        }
        match entity {
            None => {
                self.view = None;
                self.loading = true;
            }
            Some(entity) => {
                let view = parse_light_entity(
                    entity,
                    &self.configured_name,
                    self.config_min_kelvin,
                    self.config_max_kelvin,
                );
                if view.available {
                    if !self.is_pending(HeroPendingKind::Brightness) {
                        self.brightness_value = view.brightness_pct as u32;
                    }
                    if !self.is_pending(HeroPendingKind::ColorTemp) {
                        self.color_temp_value = view.color_temp_kelvin as u32;
                    }
                }
                self.view = Some(view);
                self.loading = false;
            }
        }
    }

    pub fn view_model(&self) -> HeroViewModel {
        let name = self
            .view
            .as_ref()
            .map(|view| view.name.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| {
                if self.configured_name.is_empty() {
                    None
                } else {
                    Some(self.configured_name.clone())
                }
            })
            .unwrap_or_else(|| self.entity_id.clone());

        let available = self
            .view
            .as_ref()
            .map(|view| view.available)
            .unwrap_or(false);
        let on = self.view.as_ref().map(|view| view.on).unwrap_or(false);
        let brightness_supported = self
            .view
            .as_ref()
            .map(|view| view.brightness_supported)
            .unwrap_or(true);
        let color_temp_supported = self
            .view
            .as_ref()
            .map(|view| view.color_temp_supported)
            .unwrap_or(true);
        let (min_kelvin, max_kelvin) = self
            .view
            .as_ref()
            .map(|view| (view.min_kelvin, view.max_kelvin))
            .unwrap_or((self.config_min_kelvin, self.config_max_kelvin));

        let status_text = if !self.configured {
            "No light configured. Add [widgets.hero_light] with an entity.".to_string()
        } else if self.loading || self.view.is_none() {
            "Waiting for Home Assistant…".to_string()
        } else if !available {
            "Unavailable — check the light in Home Assistant.".to_string()
        } else {
            self.error.clone().unwrap_or_default()
        };

        let brightness_text = if brightness_supported {
            format!("{}%", self.brightness_value)
        } else {
            "Not supported".to_string()
        };
        let color_temp_text = if color_temp_supported {
            format!("{} K", self.color_temp_value)
        } else {
            "Not supported".to_string()
        };

        HeroViewModel {
            configured: self.configured,
            name,
            on,
            brightness: self.brightness_value as i32,
            color_temp: self.color_temp_value as i32,
            min_kelvin: min_kelvin as i32,
            max_kelvin: max_kelvin as i32,
            available,
            loading: self.loading,
            pending: !self.pending.is_empty(),
            brightness_supported,
            color_temp_supported,
            brightness_text,
            color_temp_text,
            state_text: if on {
                "On".to_string()
            } else {
                "Off".to_string()
            },
            status_text,
            error: self.error.is_some(),
        }
    }

    pub fn on_toggle(&mut self) -> Option<HeroCommand> {
        if !self.can_control() || self.toggle_pending {
            return None;
        }
        self.error = None;
        let request_id = next_request_id();
        self.pending.insert(request_id, HeroPendingKind::Toggle);
        self.toggle_pending = true;
        Some(HeroCommand {
            call: ServiceCall {
                request_id,
                domain: "light".into(),
                service: "toggle".into(),
                target: Some(json!({"entity_id": self.entity_id})),
                data: json!({}),
                return_response: false,
            },
            action: PendingAction::HeroToggle {
                entity_id: self.entity_id.clone(),
            },
        })
    }

    pub fn on_brightness_change(&mut self, value: i32, now: Instant) -> Option<HeroCommand> {
        if !self.can_control() || !self.brightness_supported() {
            return None;
        }
        self.error = None;
        let value = value.clamp(0, 100) as u32;
        self.brightness_value = value;
        self.brightness_debounce
            .on_change(value, now)
            .map(|value| self.brightness_call(value))
    }

    pub fn on_brightness_release(&mut self, value: i32, now: Instant) -> Option<HeroCommand> {
        if !self.can_control() || !self.brightness_supported() {
            return None;
        }
        self.error = None;
        let value = value.clamp(0, 100) as u32;
        self.brightness_value = value;
        self.brightness_debounce
            .on_release(value, now)
            .map(|value| self.brightness_call(value))
    }

    pub fn on_color_temp_change(&mut self, value: i32, now: Instant) -> Option<HeroCommand> {
        if !self.can_control() || !self.color_temp_supported() {
            return None;
        }
        self.error = None;
        let value = self.clamp_kelvin(value);
        self.color_temp_value = value;
        self.color_temp_debounce
            .on_change(value, now)
            .map(|value| self.color_temp_call(value))
    }

    pub fn on_color_temp_release(&mut self, value: i32, now: Instant) -> Option<HeroCommand> {
        if !self.can_control() || !self.color_temp_supported() {
            return None;
        }
        self.error = None;
        let value = self.clamp_kelvin(value);
        self.color_temp_value = value;
        self.color_temp_debounce
            .on_release(value, now)
            .map(|value| self.color_temp_call(value))
    }

    pub fn tick(&mut self, now: Instant) -> Vec<HeroCommand> {
        let mut commands = Vec::new();
        if let Some(value) = self.brightness_debounce.tick(now) {
            commands.push(self.brightness_call(value));
        }
        if let Some(value) = self.color_temp_debounce.tick(now) {
            commands.push(self.color_temp_call(value));
        }
        commands
    }

    pub fn service_finished(&mut self, request_id: u64) {
        if let Some(kind) = self.pending.remove(&request_id)
            && kind == HeroPendingKind::Toggle
        {
            self.toggle_pending = false;
        }
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
        self.toggle_pending = false;
        self.brightness_debounce.reset();
        self.color_temp_debounce.reset();
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn can_control(&self) -> bool {
        self.configured
            && self
                .view
                .as_ref()
                .map(|view| view.available)
                .unwrap_or(false)
    }

    fn brightness_supported(&self) -> bool {
        self.view
            .as_ref()
            .map(|view| view.brightness_supported)
            .unwrap_or(false)
    }

    fn color_temp_supported(&self) -> bool {
        self.view
            .as_ref()
            .map(|view| view.color_temp_supported)
            .unwrap_or(false)
    }

    fn clamp_kelvin(&self, value: i32) -> u32 {
        let (min, max) = self
            .view
            .as_ref()
            .map(|view| (view.min_kelvin as i32, view.max_kelvin as i32))
            .unwrap_or((self.config_min_kelvin as i32, self.config_max_kelvin as i32));
        value.clamp(min, max) as u32
    }

    fn is_pending(&self, kind: HeroPendingKind) -> bool {
        self.pending.values().any(|pending| *pending == kind)
    }

    fn brightness_call(&mut self, value: u32) -> HeroCommand {
        let request_id = next_request_id();
        self.pending.insert(request_id, HeroPendingKind::Brightness);
        HeroCommand {
            call: ServiceCall {
                request_id,
                domain: "light".into(),
                service: "turn_on".into(),
                target: Some(json!({"entity_id": self.entity_id})),
                data: json!({"brightness_pct": value}),
                return_response: false,
            },
            action: PendingAction::HeroBrightness {
                entity_id: self.entity_id.clone(),
            },
        }
    }

    fn color_temp_call(&mut self, value: u32) -> HeroCommand {
        let request_id = next_request_id();
        self.pending.insert(request_id, HeroPendingKind::ColorTemp);
        HeroCommand {
            call: ServiceCall {
                request_id,
                domain: "light".into(),
                service: "turn_on".into(),
                target: Some(json!({"entity_id": self.entity_id})),
                data: json!({"color_temp_kelvin": value}),
                return_response: false,
            },
            action: PendingAction::HeroColorTemp {
                entity_id: self.entity_id.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
                "friendly_name": "Example lamp",
                "brightness": 184,
                "color_temp_kelvin": 3200,
                "min_color_temp_kelvin": 2700,
                "max_color_temp_kelvin": 6500,
                "supported_color_modes": ["color_temp"]
            }),
            last_changed: None,
            last_updated: None,
        }));
        hero
    }

    #[test]
    fn a_new_command_clears_the_previous_error() {
        let mut hero = populated_hero();
        hero.set_error(Some("service error: boom".into()));
        assert!(hero.view_model().error);
        assert!(hero.on_toggle().is_some());
        assert!(!hero.view_model().error);
    }

    #[test]
    fn toggle_uses_light_toggle_and_ignores_repeats() {
        let mut hero = populated_hero();
        let command = hero.on_toggle().unwrap();
        assert_eq!(command.call.domain, "light");
        assert_eq!(command.call.service, "toggle");
        assert_eq!(
            command.call.target,
            Some(json!({"entity_id": "light.example"}))
        );
        assert!(hero.has_pending());
        assert!(hero.on_toggle().is_none());
    }

    #[test]
    fn brightness_uses_turn_on_with_brightness_pct() {
        let mut hero = populated_hero();
        let now = Instant::now();
        let command = hero.on_brightness_change(72, now).unwrap();
        assert_eq!(command.call.domain, "light");
        assert_eq!(command.call.service, "turn_on");
        assert_eq!(command.call.data, json!({"brightness_pct": 72}));
        assert_eq!(
            command.call.target,
            Some(json!({"entity_id": "light.example"}))
        );
        assert!(matches!(
            command.action,
            PendingAction::HeroBrightness { .. }
        ));
    }

    #[test]
    fn colour_temperature_uses_turn_on_with_kelvin() {
        let mut hero = populated_hero();
        let now = Instant::now();
        let command = hero.on_color_temp_change(4000, now).unwrap();
        assert_eq!(command.call.service, "turn_on");
        assert_eq!(command.call.data, json!({"color_temp_kelvin": 4000}));
        assert!(matches!(
            command.action,
            PendingAction::HeroColorTemp { .. }
        ));
    }

    #[test]
    fn colour_temperature_is_clamped_to_the_light_range() {
        let mut hero = populated_hero();
        let now = Instant::now();
        let low = hero.on_color_temp_change(1000, now).unwrap();
        assert_eq!(low.call.data, json!({"color_temp_kelvin": 2700}));
        let high = hero.on_color_temp_release(9000, now).unwrap();
        assert_eq!(high.call.data, json!({"color_temp_kelvin": 6500}));
    }

    #[test]
    fn rapid_slider_changes_are_throttled() {
        let mut hero = populated_hero();
        let now = Instant::now();
        assert!(hero.on_brightness_change(40, now).is_some());
        assert!(
            hero.on_brightness_change(50, now + Duration::from_millis(40))
                .is_none()
        );
        assert!(
            hero.on_brightness_change(60, now + Duration::from_millis(70))
                .is_none()
        );
        assert_eq!(
            hero.on_brightness_release(70, now + Duration::from_millis(90))
                .unwrap()
                .call
                .data,
            json!({"brightness_pct": 70})
        );
    }

    #[test]
    fn tick_flushes_a_pending_slider_value() {
        let mut hero = populated_hero();
        let now = Instant::now();
        assert!(hero.on_brightness_change(20, now).is_some());
        assert!(
            hero.on_brightness_change(30, now + Duration::from_millis(50))
                .is_none()
        );
        let commands = hero.tick(now + Duration::from_millis(100));
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].call.data, json!({"brightness_pct": 30}));
    }

    #[test]
    fn unavailable_light_does_not_emit_calls() {
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
    fn unconfigured_hero_has_an_empty_state() {
        let hero = HeroLight::new(&HeroLightConfig::default());
        let view = hero.view_model();
        assert!(!view.configured);
        assert!(!view.available);
        assert!(view.status_text.contains("No light configured"));
    }

    #[test]
    fn missing_entity_is_a_loading_state() {
        let hero = HeroLight::new(&config());
        let view = hero.view_model();
        assert!(view.configured);
        assert!(view.loading);
        assert!(view.status_text.contains("Waiting"));
    }

    #[test]
    fn a_pending_brightness_value_survives_a_stale_entity_update() {
        let mut hero = populated_hero();
        let now = Instant::now();
        assert!(hero.on_brightness_change(80, now).is_some());
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
        assert_eq!(hero.view_model().brightness, 80);
    }
}
