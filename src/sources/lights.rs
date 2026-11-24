use crate::ha::models::HaEntity;
use crate::state::LightView;

fn attribute_u64(entity: &HaEntity, key: &str) -> Option<u64> {
    entity.attributes.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_f64().map(|v| v.round() as u64))
    })
}

fn attribute_string<'a>(entity: &'a HaEntity, key: &str) -> Option<&'a str> {
    entity.attributes.get(key).and_then(|value| value.as_str())
}

fn supported_modes(entity: &HaEntity) -> Option<Vec<&str>> {
    entity
        .attributes
        .get("supported_color_modes")
        .and_then(|value| value.as_array())
        .map(|modes| modes.iter().filter_map(|mode| mode.as_str()).collect())
}

pub fn parse_light_entity(
    entity: &HaEntity,
    fallback_name: &str,
    config_min_kelvin: u16,
    config_max_kelvin: u16,
) -> LightView {
    let available = entity.state != "unavailable" && entity.state != "unknown";
    let modes = supported_modes(entity);
    let has_color_temp_attributes = attribute_u64(entity, "color_temp_kelvin").is_some()
        || attribute_u64(entity, "color_temp").is_some()
        || attribute_u64(entity, "min_color_temp_kelvin").is_some()
        || attribute_u64(entity, "max_color_temp_kelvin").is_some();

    let brightness_supported = match &modes {
        Some(modes) => modes.iter().any(|mode| *mode != "onoff"),
        None => attribute_u64(entity, "brightness").is_some(),
    };
    let color_temp_supported = match &modes {
        Some(modes) => modes.iter().any(|mode| *mode == "color_temp"),
        None => has_color_temp_attributes,
    };

    let min_kelvin = attribute_u64(entity, "min_color_temp_kelvin")
        .map(|value| value.clamp(1000, 10000) as u16)
        .unwrap_or(config_min_kelvin);
    let max_kelvin = attribute_u64(entity, "max_color_temp_kelvin")
        .map(|value| value.clamp(1000, 10000) as u16)
        .unwrap_or(config_max_kelvin);
    let (min_kelvin, max_kelvin) = if min_kelvin < max_kelvin {
        (min_kelvin, max_kelvin)
    } else {
        (config_min_kelvin, config_max_kelvin)
    };

    let brightness_pct = attribute_u64(entity, "brightness")
        .map(|brightness| {
            let brightness = brightness.min(255);
            if brightness == 0 {
                0
            } else {
                (((brightness * 100 + 127) / 255) as u8).max(1)
            }
        })
        .unwrap_or(0);

    let color_temp_kelvin = attribute_u64(entity, "color_temp_kelvin")
        .map(|value| value.clamp(1000, 10000) as u16)
        .or_else(|| {
            attribute_u64(entity, "color_temp").and_then(|mireds| {
                1_000_000u64
                    .checked_div(mireds)
                    .map(|kelvin| kelvin.clamp(1000, 10000) as u16)
            })
        })
        .unwrap_or_else(|| ((min_kelvin as u32 + max_kelvin as u32) / 2) as u16);

    let name = if fallback_name.is_empty() {
        attribute_string(entity, "friendly_name")
            .map(str::to_string)
            .unwrap_or_else(|| entity.entity_id.clone())
    } else {
        fallback_name.to_string()
    };

    LightView {
        entity_id: entity.entity_id.clone(),
        name,
        on: entity.state == "on",
        available,
        brightness_pct,
        brightness_supported,
        color_temp_kelvin,
        min_kelvin,
        max_kelvin,
        color_temp_supported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn light(attributes: serde_json::Value) -> HaEntity {
        HaEntity {
            entity_id: "light.example".into(),
            state: "on".into(),
            attributes,
            last_changed: None,
            last_updated: None,
        }
    }

    #[test]
    fn parses_brightness_and_kelvin() {
        let view = parse_light_entity(
            &light(json!({
                "friendly_name": "Example lamp",
                "brightness": 184,
                "color_temp_kelvin": 3200,
                "min_color_temp_kelvin": 2200,
                "max_color_temp_kelvin": 5000,
                "supported_color_modes": ["color_temp"]
            })),
            "",
            2700,
            6500,
        );
        assert_eq!(view.name, "Example lamp");
        assert!(view.on);
        assert!(view.available);
        assert_eq!(view.brightness_pct, 72);
        assert_eq!(view.color_temp_kelvin, 3200);
        assert_eq!(view.min_kelvin, 2200);
        assert_eq!(view.max_kelvin, 5000);
        assert!(view.brightness_supported);
        assert!(view.color_temp_supported);
    }

    #[test]
    fn converts_mireds_to_kelvin() {
        let view = parse_light_entity(
            &light(json!({
                "brightness": 128,
                "color_temp": 312,
                "supported_color_modes": ["color_temp"]
            })),
            "Configured name",
            2700,
            6500,
        );
        assert_eq!(view.name, "Configured name");
        assert_eq!(view.color_temp_kelvin, 3205);
        assert_eq!(view.min_kelvin, 2700);
        assert_eq!(view.max_kelvin, 6500);
    }

    #[test]
    fn a_colour_only_light_does_not_claim_colour_temperature() {
        let view = parse_light_entity(
            &light(json!({
                "brightness": 128,
                "supported_color_modes": ["hs"]
            })),
            "",
            2700,
            6500,
        );
        assert!(view.brightness_supported);
        assert!(!view.color_temp_supported);
    }

    #[test]
    fn on_off_light_disables_brightness_and_colour() {
        let view = parse_light_entity(
            &light(json!({"supported_color_modes": ["onoff"]})),
            "",
            2700,
            6500,
        );
        assert!(!view.brightness_supported);
        assert!(!view.color_temp_supported);
        assert_eq!(view.brightness_pct, 0);
        assert_eq!(view.color_temp_kelvin, 4600);
    }

    #[test]
    fn a_dim_but_visible_light_does_not_read_as_zero_percent() {
        let view = parse_light_entity(
            &light(json!({"brightness": 1, "supported_color_modes": ["brightness"]})),
            "",
            2700,
            6500,
        );
        assert_eq!(view.brightness_pct, 1);
    }

    #[test]
    fn unavailable_is_not_populated() {
        let mut entity = light(json!({"brightness": 100}));
        entity.state = "unavailable".into();
        let view = parse_light_entity(&entity, "", 2700, 6500);
        assert!(!view.available);
        assert!(!view.on);
    }

    #[test]
    fn entity_id_is_the_last_resort_name() {
        let view = parse_light_entity(&light(json!({})), "", 2700, 6500);
        assert_eq!(view.name, "light.example");
    }
}
