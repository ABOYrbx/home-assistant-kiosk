use crate::config::WeatherWidgetConfig;
use crate::events::{ServiceCall, SourceError};
use crate::ha::models::HaEntity;
use crate::state::{WeatherForecast, WeatherView};
use serde_json::json;
use std::time::{Duration, Instant};

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
        return Err(SourceError(
            "weather forecast response did not contain a forecast array".to_string(),
        ));
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

pub fn forecast_call(request_id: u64, config: &WeatherWidgetConfig) -> ServiceCall {
    ServiceCall {
        request_id,
        domain: "weather".to_string(),
        service: "get_forecasts".to_string(),
        target: Some(json!({ "entity_id": config.entity })),
        data: json!({ "type": config.forecast_type }),
        return_response: true,
    }
}

/// Forecast refresh cadence with exponential retry after failures.
#[derive(Debug, Clone)]
pub struct ForecastRefresher {
    interval: Duration,
    retry_delay: Duration,
    last_attempt: Option<Instant>,
    pending: bool,
    immediate: bool,
}

impl ForecastRefresher {
    pub fn new(refresh_seconds: u64) -> Self {
        Self {
            interval: Duration::from_secs(refresh_seconds.max(60)),
            retry_delay: Duration::from_secs(30),
            last_attempt: None,
            pending: false,
            immediate: true,
        }
    }

    pub fn should_refresh(&self, now: Instant) -> bool {
        !self.pending
            && (self.immediate
                || self
                    .last_attempt
                    .is_none_or(|last| now.saturating_duration_since(last) >= self.retry_delay))
    }

    pub fn begin(&mut self, now: Instant, _request_id: u64) -> bool {
        if !self.should_refresh(now) {
            return false;
        }
        self.pending = true;
        let retry = self.last_attempt.is_some() && !self.immediate;
        self.immediate = false;
        if retry {
            self.retry_delay = (self.retry_delay.saturating_mul(2)).min(self.interval);
        }
        self.last_attempt = Some(now);
        true
    }

    pub fn finish_success(&mut self, now: Instant) {
        self.pending = false;
        self.retry_delay = Duration::from_secs(30).min(self.interval);
        self.last_attempt = Some(now);
    }

    pub fn finish_failure(&mut self, now: Instant) {
        self.pending = false;
        self.last_attempt = Some(now);
    }

    pub fn request_immediate(&mut self) {
        self.immediate = true;
    }
}

/// UI-thread weather state. Entity updates rebuild current conditions while
/// preserving the last successfully fetched forecast.
#[derive(Debug, Clone)]
pub struct WeatherRuntime {
    config: WeatherWidgetConfig,
    refresher: ForecastRefresher,
    view: Option<WeatherView>,
    pending_request: Option<u64>,
    error: Option<String>,
}

impl WeatherRuntime {
    pub fn new(config: WeatherWidgetConfig) -> Self {
        let refresher = ForecastRefresher::new(config.refresh_seconds);
        Self {
            config,
            refresher,
            view: None,
            pending_request: None,
            error: None,
        }
    }

    pub fn apply_entity(&mut self, entity: Option<&HaEntity>) {
        if !self.config.enabled || self.config.entity.is_empty() {
            self.view = None;
            return;
        }
        let forecast = self
            .view
            .as_ref()
            .map(|view| view.forecast.clone())
            .unwrap_or_default();
        self.view = Some(match entity {
            Some(entity) => WeatherView {
                forecast,
                ..parse_weather_entity(entity)
            },
            None => WeatherView {
                entity_id: self.config.entity.clone(),
                forecast,
                ..Default::default()
            },
        });
    }

    pub fn begin(&mut self, now: Instant, request_id: u64) -> Option<ServiceCall> {
        if !self.config.enabled
            || self.config.entity.is_empty()
            || !self.refresher.begin(now, request_id)
        {
            return None;
        }
        self.pending_request = Some(request_id);
        self.error = None;
        Some(forecast_call(request_id, &self.config))
    }

    pub fn finish_success(
        &mut self,
        request_id: u64,
        mut forecast: Vec<WeatherForecast>,
        now: Instant,
    ) -> bool {
        if self.pending_request != Some(request_id) {
            return false;
        }
        forecast.truncate(self.config.forecast_count.max(1) as usize);
        let view = self.view.get_or_insert_with(|| WeatherView {
            entity_id: self.config.entity.clone(),
            ..Default::default()
        });
        view.forecast = forecast;
        self.pending_request = None;
        self.error = None;
        self.refresher.finish_success(now);
        true
    }

    pub fn finish_failure(&mut self, request_id: u64, error: String, now: Instant) -> bool {
        if self.pending_request != Some(request_id) {
            return false;
        }
        self.pending_request = None;
        self.error = Some(error);
        self.refresher.finish_failure(now);
        true
    }

    pub fn request_immediate(&mut self) {
        self.refresher.request_immediate();
    }

    pub fn should_refresh(&self, now: Instant) -> bool {
        self.refresher.should_refresh(now)
    }

    pub fn view(&self) -> Option<&WeatherView> {
        self.view.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, Instant};

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

    #[test]
    fn forecast_call_uses_configured_type_and_count() {
        let config = crate::config::WeatherWidgetConfig {
            enabled: true,
            entity: "weather.example".into(),
            forecast_type: "hourly".into(),
            forecast_count: 4,
            refresh_seconds: 900,
        };
        let call = forecast_call(17, &config);
        assert_eq!(call.domain, "weather");
        assert_eq!(call.service, "get_forecasts");
        assert_eq!(call.target, Some(json!({"entity_id": "weather.example"})));
        assert_eq!(call.data["type"], "hourly");
        assert!(call.return_response);
    }

    #[test]
    fn refresher_ignores_stacked_requests_and_backs_off_after_failure() {
        let start = Instant::now();
        let mut refresher = ForecastRefresher::new(900);
        assert!(refresher.begin(start, 1));
        assert!(!refresher.begin(start, 2));
        refresher.finish_failure(start);
        assert!(!refresher.should_refresh(start + Duration::from_secs(29)));
        assert!(refresher.should_refresh(start + Duration::from_secs(30)));
        assert!(refresher.begin(start + Duration::from_secs(30), 3));
        refresher.finish_failure(start + Duration::from_secs(30));
        assert!(!refresher.should_refresh(start + Duration::from_secs(89)));
        assert!(refresher.should_refresh(start + Duration::from_secs(90)));
        refresher.request_immediate();
        assert!(refresher.should_refresh(start + Duration::from_secs(91)));
    }

    #[test]
    fn runtime_keeps_last_forecast_when_refresh_fails_and_truncates_on_success() {
        let config = crate::config::WeatherWidgetConfig {
            enabled: true,
            entity: "weather.example".into(),
            forecast_type: "daily".into(),
            forecast_count: 2,
            refresh_seconds: 900,
        };
        let entity = HaEntity {
            entity_id: "weather.example".into(),
            state: "sunny".into(),
            attributes: json!({"temperature": 20.0, "temperature_unit": "°C"}),
            last_changed: None,
            last_updated: None,
        };
        let start = Instant::now();
        let mut runtime = WeatherRuntime::new(config);
        runtime.apply_entity(Some(&entity));
        let call = runtime.begin(start, 9).expect("first forecast request");
        assert_eq!(call.request_id, 9);
        let response = json!({"forecast": [
            {"datetime":"2026-11-24", "condition":"cloudy", "temperature":19.0},
            {"datetime":"2026-11-25", "condition":"rainy", "temperature":18.0},
            {"datetime":"2026-11-26", "condition":"sunny", "temperature":21.0}
        ]});
        runtime.finish_success(9, parse_forecast_response(&response).unwrap(), start);
        assert_eq!(runtime.view().unwrap().forecast.len(), 2);

        let call = runtime.begin(start + Duration::from_secs(900), 10).unwrap();
        runtime.finish_failure(
            call.request_id,
            "forecast unavailable".into(),
            start + Duration::from_secs(900),
        );
        assert_eq!(runtime.view().unwrap().forecast.len(), 2);
        assert_eq!(runtime.view().unwrap().condition, "sunny");
    }

    #[test]
    fn malformed_forecast_response_is_an_error_not_an_empty_success() {
        assert!(parse_forecast_response(&json!({"unexpected": []})).is_err());
        assert!(
            parse_forecast_response(&json!({"forecast": []}))
                .unwrap()
                .is_empty()
        );
    }
}
