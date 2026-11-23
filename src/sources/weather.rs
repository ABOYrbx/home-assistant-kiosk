use crate::events::SourceError;
use crate::ha::models::HaEntity;
use crate::state::{WeatherForecast, WeatherView};

pub fn parse_weather_entity(entity: &HaEntity) -> WeatherView {
    let mut view = WeatherView {
        entity_id: entity.entity_id.clone(),
        ..Default::default()
    };
    if entity.state != "unavailable" && entity.state != "unknown" {
        view.condition = entity.state.clone();
    }
    let attrs = &entity.attributes;
    if let Some(t) = attrs.get("temperature").and_then(|v| v.as_f64()) {
        view.temperature = Some(t);
    }
    if let Some(u) = attrs.get("temperature_unit").and_then(|v| v.as_str()) {
        view.temperature_unit = u.to_string();
    } else if view.temperature_unit.is_empty() {
        view.temperature_unit = "°C".to_string();
    }
    view
}

pub fn parse_forecast_response(
    value: &serde_json::Value,
) -> Result<Vec<WeatherForecast>, SourceError> {
    // HA returns { "forecast": [ { datetime, condition, temperature, templow } ] }
    // Possibly nested under entity or top-level.
    let mut out = Vec::new();
    let arr = value
        .get("forecast")
        .and_then(|v| v.as_array())
        .or_else(|| value.get("forecasts").and_then(|v| v.as_array()))
        .or_else(|| {
            // response may be { "<entity>": { "forecast": [...] } }
            value.as_object().and_then(|o| {
                o.values()
                    .find_map(|v| v.get("forecast").and_then(|f| f.as_array()))
            })
        });
    let Some(arr) = arr else {
        return Ok(out);
    };
    for item in arr {
        let datetime = item
            .get("datetime")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let condition = item
            .get("condition")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let temperature = item.get("temperature").and_then(|v| v.as_f64());
        let templow = item.get("templow").and_then(|v| v.as_f64());
        out.push(WeatherForecast {
            datetime,
            condition,
            temperature,
            templow,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_entity_basic() {
        let e = HaEntity {
            entity_id: "weather.home".into(),
            state: "sunny".into(),
            attributes: json!({"temperature": 21.5, "temperature_unit": "°C"}),
            last_changed: None,
            last_updated: None,
        };
        let v = parse_weather_entity(&e);
        assert_eq!(v.condition, "sunny");
        assert_eq!(v.temperature, Some(21.5));
        assert_eq!(v.temperature_unit, "°C");
    }

    #[test]
    fn unavailable_state() {
        let e = HaEntity {
            entity_id: "weather.home".into(),
            state: "unavailable".into(),
            attributes: json!({}),
            last_changed: None,
            last_updated: None,
        };
        let v = parse_weather_entity(&e);
        assert_eq!(v.condition, "");
    }

    #[test]
    fn parse_forecast() {
        let v = json!({"forecast": [{"datetime":"2026-11-23T12:00:00","condition":"rainy","temperature":18.0,"templow":12.0}]});
        let f = parse_forecast_response(&v).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].condition, "rainy");
        assert_eq!(f[0].temperature, Some(18.0));
    }
}
