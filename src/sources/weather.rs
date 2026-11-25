use crate::config::WeatherWidgetConfig;
use crate::events::SourceError;
use crate::ha::models::HaEntity;
use crate::state::{WeatherForecast, WeatherView};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
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
    if let Some(t) = attrs.get("temperature").and_then(as_f64) {
        view.temperature = Some(t);
    }
    if let Some(u) = attrs.get("temperature_unit").and_then(|v| v.as_str()) {
        view.temperature_unit = u.to_string();
    } else if view.temperature_unit.is_empty() {
        view.temperature_unit = "°C".to_string();
    }
    // Forecast embedded in attributes (some integrations expose it there).
    if let Some(forecast) = attrs.get("forecast")
        && let Ok(parsed) = parse_forecast_response(&serde_json::json!({"forecast": forecast}))
    {
        view.forecast = parsed;
    }
    view
}

fn as_f64(value: &serde_json::Value) -> Option<f64> {
    if let Some(n) = value.as_f64() {
        return Some(n);
    }
    value.as_str().and_then(|s| s.trim().parse::<f64>().ok())
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
        let temperature = item.get("temperature").and_then(as_f64);
        let templow = item.get("templow").and_then(as_f64);
        out.push(WeatherForecast {
            datetime,
            condition,
            temperature,
            templow,
        });
    }
    Ok(out)
}

/// Map a Home Assistant condition string to the bundled icon file name.
pub fn weather_icon(condition: &str) -> &'static str {
    match condition {
        "sunny" | "clear" => "weather-clear-day.svg",
        "clear-night" => "weather-clear-night.svg",
        "partlycloudy" => "weather-partly-cloudy-day.svg",
        "partlycloudy-night" => "weather-partly-cloudy-night.svg",
        "cloudy" => "weather-cloudy.svg",
        "rainy" | "pouring" | "hail" => "weather-rainy.svg",
        "snowy" | "snowy-rainy" => "weather-snowy.svg",
        "fog" => "weather-fog.svg",
        "lightning" | "lightning-rainy" => "weather-thunderstorm.svg",
        "windy" | "windy-variant" => "weather-windy.svg",
        _ => "weather-unknown.svg",
    }
}

// Aliases for alternate naming used by callers/tests.
pub fn condition_icon(condition: &str) -> &'static str {
    weather_icon(condition)
}
pub fn icon_for_condition(condition: &str) -> &'static str {
    weather_icon(condition)
}
pub fn icon_name(condition: &str) -> &'static str {
    weather_icon(condition)
}

/// Refresh cadence for the forecast. Stacked refreshes are ignored while one
/// runs; a reconnect forces an immediate refresh.
#[derive(Debug, Clone)]
pub struct WeatherRefresher {
    interval: Duration,
    in_flight: bool,
    last_started: Option<Instant>,
    immediate: bool,
    failures: u32,
}

impl WeatherRefresher {
    pub fn new(refresh_seconds: u64) -> Self {
        Self {
            interval: Duration::from_secs(refresh_seconds.max(MIN_REFRESH_SECONDS)),
            in_flight: false,
            last_started: None,
            immediate: true,
            failures: 0,
        }
    }

    pub fn should_refresh(&self, now: Instant) -> bool {
        !self.in_flight
            && (self.immediate
                || self
                    .last_started
                    .is_none_or(|started| now.saturating_duration_since(started) >= self.interval))
    }

    pub fn begin(&mut self, now: Instant) -> bool {
        if !self.should_refresh(now) {
            return false;
        }
        self.in_flight = true;
        self.immediate = false;
        self.last_started = Some(now);
        true
    }

    pub fn finish(&mut self) {
        self.in_flight = false;
        self.failures = 0;
    }

    pub fn fail(&mut self) {
        self.in_flight = false;
        self.failures = self.failures.saturating_add(1);
        // backoff: push next attempt out
        self.last_started = Some(Instant::now());
        self.immediate = false;
    }

    pub fn backoff(&self) -> Duration {
        let secs = 1u64 << self.failures.min(6);
        Duration::from_secs(secs.min(300))
    }

    pub fn request_immediate(&mut self) {
        self.immediate = true;
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }
}

pub const MIN_REFRESH_SECONDS: u64 = 60;
pub const DEFAULT_REFRESH_SECONDS: u64 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherStatus {
    Loading,
    Unavailable,
    Empty,
    Populated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherForecastDisplay {
    pub datetime: String,
    pub condition: String,
    pub icon: String,
    pub temperature: Option<f64>,
    pub templow: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherDisplay {
    pub status: WeatherStatus,
    pub condition: String,
    pub icon: String,
    pub temperature: Option<f64>,
    pub temperature_unit: String,
    pub temperature_text: String,
    pub forecast: Vec<WeatherForecastDisplay>,
}

impl Default for WeatherDisplay {
    fn default() -> Self {
        Self {
            status: WeatherStatus::Loading,
            condition: String::new(),
            icon: "weather-unknown.svg".to_string(),
            temperature: None,
            temperature_unit: "°C".to_string(),
            temperature_text: String::new(),
            forecast: Vec::new(),
        }
    }
}

pub fn display_weather(view: Option<&WeatherView>) -> WeatherDisplay {
    let Some(view) = view else {
        return WeatherDisplay::default();
    };
    let available = !view.condition.is_empty() || view.temperature.is_some();
    if !available {
        return WeatherDisplay {
            status: WeatherStatus::Unavailable,
            condition: view.condition.clone(),
            icon: weather_icon(&view.condition).to_string(),
            temperature: view.temperature,
            temperature_unit: view.temperature_unit.clone(),
            temperature_text: format_temperature(view.temperature, &view.temperature_unit),
            forecast: Vec::new(),
        };
    }
    let forecast: Vec<WeatherForecastDisplay> = view
        .forecast
        .iter()
        .map(|f| WeatherForecastDisplay {
            datetime: f.datetime.clone(),
            condition: f.condition.clone(),
            icon: weather_icon(&f.condition).to_string(),
            temperature: f.temperature,
            templow: f.templow,
        })
        .collect();
    let status = if forecast.is_empty() && view.temperature.is_none() {
        WeatherStatus::Empty
    } else {
        WeatherStatus::Populated
    };
    WeatherDisplay {
        status,
        condition: view.condition.clone(),
        icon: weather_icon(&view.condition).to_string(),
        temperature: view.temperature,
        temperature_unit: view.temperature_unit.clone(),
        temperature_text: format_temperature(view.temperature, &view.temperature_unit),
        forecast,
    }
}

fn format_temperature(temp: Option<f64>, unit: &str) -> String {
    match temp {
        Some(t) => format!("{:.0}{}", t, unit),
        None => String::new(),
    }
}

/// Owns the weather widget data on the UI thread.
#[derive(Debug)]
pub struct WeatherRuntime {
    config: WeatherWidgetConfig,
    refresher: WeatherRefresher,
    entity: Option<HaEntity>,
    time_zone: chrono_tz::Tz,
    view: Option<WeatherView>,
}

impl WeatherRuntime {
    pub fn new(config: WeatherWidgetConfig) -> Self {
        let refresher = WeatherRefresher::new(config.refresh_seconds);
        Self {
            config,
            refresher,
            entity: None,
            time_zone: chrono_tz::Tz::UTC,
            view: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn entity_id(&self) -> &str {
        &self.config.entity
    }

    pub fn view(&self) -> Option<&WeatherView> {
        self.view.as_ref()
    }

    pub fn on_entities(
        &mut self,
        entities: &HashMap<String, HaEntity>,
        ha_tz: chrono_tz::Tz,
        _now: DateTime<Utc>,
    ) -> Option<WeatherView> {
        if !self.config.enabled {
            return None;
        }
        self.time_zone = ha_tz;
        self.entity = entities.get(&self.config.entity).cloned();
        let view = self.parse();
        if self.view.as_ref() == Some(&view) {
            return None;
        }
        self.view = Some(view.clone());
        Some(view)
    }

    pub fn on_reconnect(&mut self) {
        self.refresher.request_immediate();
    }

    pub fn on_tick(&mut self, now_instant: Instant, _now: DateTime<Utc>) -> TickOutcome {
        if !self.config.enabled || !self.refresher.begin(now_instant) {
            return TickOutcome::Idle;
        }
        let view = self.parse();
        let changed = self.view.as_ref() != Some(&view);
        self.view = Some(view.clone());
        self.refresher.finish();
        TickOutcome::Refreshed(changed.then_some(view))
    }

    pub fn set_forecast(&mut self, value: &serde_json::Value) -> Option<WeatherView> {
        if !self.config.enabled {
            return None;
        }
        if let Ok(forecast) = parse_forecast_response(value)
            && let Some(view) = &mut self.view
        {
            view.forecast = forecast;
            let v = view.clone();
            return Some(v);
        }
        None
    }

    pub fn on_forecast(&mut self, value: &serde_json::Value) -> Option<WeatherView> {
        self.set_forecast(value)
    }
    pub fn apply_forecast(&mut self, value: &serde_json::Value) -> Option<WeatherView> {
        self.set_forecast(value)
    }
    pub fn on_forecast_response(&mut self, value: &serde_json::Value) -> Option<WeatherView> {
        self.set_forecast(value)
    }

    fn parse(&self) -> WeatherView {
        match &self.entity {
            Some(entity) => parse_weather_entity(entity),
            None => WeatherView {
                entity_id: self.config.entity.clone(),
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TickOutcome {
    Idle,
    Refreshed(Option<WeatherView>),
}

// Re-export commonly used alias.
pub type WeatherTickOutcome = TickOutcome;
