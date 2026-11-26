//! Provider-neutral departures source.
//!
//! The widget reads a single Home Assistant sensor that exposes a `departures`
//! attribute as documented in `docs/departures-entity-contract.md`. No transit
//! provider URL, stop ID, or integration-specific field is known here.

use crate::config::DeparturesWidgetConfig;
use crate::events::EntityId;
use crate::ha::models::HaEntity;
use crate::state::{DepartureItem, DeparturesView};
use chrono::{DateTime, TimeZone, Utc};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::debug;

/// Entries whose departure is further in the past than this are dropped.
pub const PAST_GRACE: Duration = Duration::from_secs(60);
/// Upper bound for `widgets.departures.max_items`.
pub const MAX_ITEMS: u32 = 20;
/// Lower bound for `widgets.departures.refresh_seconds`.
pub const MIN_REFRESH_SECONDS: u64 = 10;

/// Parse the configured sensor into a view, sorted by departure time.
///
/// A missing entity or a missing/non-array `departures` attribute makes the
/// view unavailable. Invalid entries are skipped and logged; they never hide
/// valid entries. The sensor state itself is not part of the contract.
pub fn parse_departures(
    entity: &HaEntity,
    title: &str,
    ha_tz: chrono_tz::Tz,
    max_items: u32,
    now: DateTime<Utc>,
) -> DeparturesView {
    let mut view = DeparturesView {
        entity_id: entity.entity_id.clone(),
        title: title.to_string(),
        items: Vec::new(),
        available: true,
    };

    let Some(entries) = entity
        .attributes
        .get("departures")
        .and_then(|value| value.as_array())
    else {
        view.available = false;
        return view;
    };

    let cutoff = now - chrono::Duration::from_std(PAST_GRACE).unwrap_or_default();
    let limit = max_items.clamp(1, MAX_ITEMS) as usize;
    let mut parsed: Vec<(DepartureItem, DateTime<Utc>)> = Vec::with_capacity(entries.len());

    for entry in entries {
        match parse_entry(entry, ha_tz) {
            Ok((item, departure)) => {
                if departure < cutoff {
                    continue;
                }
                parsed.push((item, departure));
            }
            Err(reason) => debug!(
                entity_id = %entity.entity_id,
                reason,
                "skipping invalid departure entry"
            ),
        }
    }

    parsed.sort_by_key(|(_, departure)| *departure);
    view.items = parsed
        .into_iter()
        .take(limit)
        .map(|(item, _)| item)
        .collect();
    view
}

fn parse_entry(
    entry: &serde_json::Value,
    ha_tz: chrono_tz::Tz,
) -> Result<(DepartureItem, DateTime<Utc>), &'static str> {
    let object = entry.as_object().ok_or("entry is not an object")?;
    let line = object
        .get("line")
        .and_then(|value| value.as_str())
        .ok_or("missing or non-string line")?;
    let destination = object
        .get("destination")
        .and_then(|value| value.as_str())
        .ok_or("missing or non-string destination")?;
    let departure = object
        .get("departure")
        .and_then(|value| value.as_str())
        .ok_or("missing or non-string departure")?;
    let departure_utc =
        parse_departure_time(departure, ha_tz).ok_or("unparsable departure time")?;

    let delay_minutes = match object.get("delay_minutes") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(value.as_i64().ok_or("delay_minutes is not an integer")?),
    };

    Ok((
        DepartureItem {
            line: line.to_string(),
            destination: destination.to_string(),
            departure: departure.to_string(),
            departure_utc: Some(departure_utc.to_rfc3339()),
            delay_minutes,
            platform: object
                .get("platform")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            cancelled: object
                .get("cancelled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            remarks: object
                .get("remarks")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        },
        departure_utc,
    ))
}

/// RFC 3339 with an offset wins; a naive timestamp is read in the Home
/// Assistant timezone from `get_config`.
pub fn parse_departure_time(value: &str, ha_tz: chrono_tz::Tz) -> Option<DateTime<Utc>> {
    if let Ok(departure) = DateTime::parse_from_rfc3339(value) {
        return Some(departure.with_timezone(&Utc));
    }
    const NAIVE_FORMATS: [&str; 3] = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"];
    for format in NAIVE_FORMATS {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, format)
            && let Some(local) = ha_tz.from_local_datetime(&naive).earliest()
        {
            return Some(local.with_timezone(&Utc));
        }
    }
    None
}

/// The widget data states: loading, unavailable, empty, populated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeparturesStatus {
    /// No data received yet.
    Loading,
    /// The entity or its attribute is missing or invalid.
    Unavailable,
    /// A valid attribute with no future entries.
    Empty,
    /// At least one row is rendered.
    Populated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepartureRowView {
    pub line: String,
    pub destination: String,
    pub time: String,
    pub platform: String,
    pub delay: String,
    pub cancelled: bool,
    pub remarks: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeparturesDisplay {
    pub status: DeparturesStatus,
    pub rows: Vec<DepartureRowView>,
}

impl Default for DeparturesDisplay {
    fn default() -> Self {
        Self {
            status: DeparturesStatus::Loading,
            rows: Vec::new(),
        }
    }
}

/// Turn the cached view into rows for the widget, formatted in the device
/// timezone. Keeps "No departures" and "Data unavailable" distinct.
pub fn display_departures<Tz>(view: Option<&DeparturesView>, device_tz: &Tz) -> DeparturesDisplay
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let Some(view) = view else {
        return DeparturesDisplay::default();
    };
    if !view.available {
        return DeparturesDisplay {
            status: DeparturesStatus::Unavailable,
            rows: Vec::new(),
        };
    }
    if view.items.is_empty() {
        return DeparturesDisplay {
            status: DeparturesStatus::Empty,
            rows: Vec::new(),
        };
    }
    let rows = view
        .items
        .iter()
        .map(|item| DepartureRowView {
            line: item.line.clone(),
            destination: item.destination.clone(),
            time: format_time(item, device_tz),
            platform: item.platform.clone().unwrap_or_default(),
            delay: delay_label(item.delay_minutes),
            cancelled: item.cancelled,
            remarks: item.remarks.clone().unwrap_or_default(),
        })
        .collect();
    DeparturesDisplay {
        status: DeparturesStatus::Populated,
        rows,
    }
}

fn format_time<Tz>(item: &DepartureItem, device_tz: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    if let Some(raw) = &item.departure_utc
        && let Ok(departure) = DateTime::parse_from_rfc3339(raw)
    {
        return departure
            .with_timezone(device_tz)
            .format("%H:%M")
            .to_string();
    }
    if let Ok(departure) = DateTime::parse_from_rfc3339(&item.departure) {
        return departure
            .with_timezone(device_tz)
            .format("%H:%M")
            .to_string();
    }
    item.departure.clone()
}

fn delay_label(delay_minutes: Option<i64>) -> String {
    match delay_minutes {
        None | Some(0) => String::new(),
        Some(delay) if delay > 0 => format!("+{delay} min"),
        Some(delay) => format!("{} min early", delay.unsigned_abs()),
    }
}

/// Decides when the widget refreshes. The refresh is a safety net on top of
/// entity subscription updates; stacked refreshes are ignored while one runs.
#[derive(Debug, Clone)]
pub struct DeparturesRefresher {
    interval: Duration,
    in_flight: bool,
    last_started: Option<Instant>,
    immediate: bool,
}

impl DeparturesRefresher {
    pub fn new(refresh_seconds: u64) -> Self {
        Self {
            interval: Duration::from_secs(refresh_seconds.max(MIN_REFRESH_SECONDS)),
            in_flight: false,
            last_started: None,
            immediate: true,
        }
    }

    pub fn should_refresh(&self, now: Instant) -> bool {
        !self.in_flight
            && (self.immediate
                || self
                    .last_started
                    .is_none_or(|started| now.saturating_duration_since(started) >= self.interval))
    }

    /// Start a refresh. Returns `false` when one is already in flight or the
    /// interval has not elapsed.
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
    }

    /// Ask for a refresh on the next tick, for example after a reconnect.
    pub fn request_immediate(&mut self) {
        self.immediate = true;
    }
}

/// What a timer tick did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickOutcome {
    /// Not due yet, or a refresh is already in flight.
    Idle,
    /// A refresh ran. `Some` when the rendered data changed.
    Refreshed(Option<DeparturesView>),
}

/// Owns the departures widget data on the UI thread: the configured entity,
/// the last parsed view, and the refresh cadence.
#[derive(Debug)]
pub struct DeparturesRuntime {
    config: DeparturesWidgetConfig,
    refresher: DeparturesRefresher,
    entity: Option<HaEntity>,
    time_zone: chrono_tz::Tz,
    view: Option<DeparturesView>,
}

impl DeparturesRuntime {
    pub fn new(config: DeparturesWidgetConfig) -> Self {
        let refresher = DeparturesRefresher::new(config.refresh_seconds);
        Self {
            config,
            refresher,
            entity: None,
            time_zone: chrono_tz::Tz::UTC,
            view: None,
        }
    }

    pub fn title(&self) -> &str {
        &self.config.title
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn view(&self) -> Option<&DeparturesView> {
        self.view.as_ref()
    }

    /// Re-read the sensor whenever entity state or attributes change.
    pub fn on_entities(
        &mut self,
        entities: &HashMap<EntityId, HaEntity>,
        ha_tz: chrono_tz::Tz,
        now: DateTime<Utc>,
    ) -> Option<DeparturesView> {
        if !self.config.enabled {
            return None;
        }
        self.time_zone = ha_tz;
        self.entity = entities.get(&self.config.entity).cloned();
        let view = self.parse(now);
        if self.view.as_ref() == Some(&view) {
            return None;
        }
        self.view = Some(view.clone());
        Some(view)
    }

    pub fn on_reconnect(&mut self) {
        self.refresher.request_immediate();
    }

    /// Safety refresh: re-read the cached entity so expired departures leave
    /// the list even when no state change arrives.
    pub fn on_tick(&mut self, now_instant: Instant, now: DateTime<Utc>) -> TickOutcome {
        if !self.config.enabled || !self.refresher.begin(now_instant) {
            return TickOutcome::Idle;
        }
        let view = self.parse(now);
        let changed = self.view.as_ref() != Some(&view);
        self.view = Some(view.clone());
        self.refresher.finish();
        TickOutcome::Refreshed(changed.then_some(view))
    }

    fn parse(&self, now: DateTime<Utc>) -> DeparturesView {
        match &self.entity {
            Some(entity) => parse_departures(
                entity,
                &self.config.title,
                self.time_zone,
                self.config.max_items,
                now,
            ),
            None => DeparturesView {
                entity_id: self.config.entity.clone(),
                title: self.config.title.clone(),
                items: Vec::new(),
                available: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 22, 20, 0, 0).unwrap()
    }

    fn entity(attributes: serde_json::Value) -> HaEntity {
        HaEntity {
            entity_id: "sensor.departures".into(),
            state: "ok".into(),
            attributes,
            last_changed: None,
            last_updated: None,
        }
    }

    fn berlin() -> chrono_tz::Tz {
        "Europe/Berlin".parse().unwrap()
    }

    #[test]
    fn sorts_by_departure_and_keeps_contract_fields() {
        let view = parse_departures(
            &entity(json!({"departures": [
                {"line":"S1","destination":"Airport","departure":"2026-11-22 22:03:00","cancelled":true},
                {"line":"42","destination":"Central","departure":"2026-11-22T21:55:00+01:00","delay_minutes":2,"platform":"2"}
            ]})),
            "Departures",
            berlin(),
            6,
            now(),
        );
        assert!(view.available);
        assert_eq!(view.items.len(), 2);
        assert_eq!(view.items[0].line, "42");
        assert_eq!(view.items[0].platform.as_deref(), Some("2"));
        assert_eq!(
            view.items[0].departure_utc.as_deref(),
            Some("2026-11-22T20:55:00+00:00")
        );
        assert_eq!(view.items[1].line, "S1");
        assert!(view.items[1].cancelled);
    }

    #[test]
    fn naive_timestamp_uses_home_assistant_timezone() {
        let view = parse_departures(
            &entity(json!({"departures": [
                {"line":"1","destination":"A","departure":"2026-11-22 21:30:00"}
            ]})),
            "Departures",
            berlin(),
            6,
            now(),
        );
        // 21:30 in Europe/Berlin (UTC+1) is 20:30 UTC.
        assert_eq!(
            view.items[0].departure_utc.as_deref(),
            Some("2026-11-22T20:30:00+00:00")
        );
    }

    #[test]
    fn malformed_entries_are_skipped_without_hiding_valid_ones() {
        let view = parse_departures(
            &entity(json!({"departures": [
                "not an object",
                {"destination":"No line","departure":"2026-11-22T21:00:00+01:00"},
                {"line":"7","destination":"No time"},
                {"line":"8","destination":"Bad time","departure":"tomorrow"},
                {"line":"9","destination":"Bad delay","departure":"2026-11-22T21:10:00+01:00","delay_minutes":"2"},
                {"line":"10","destination":"Valid","departure":"2026-11-22T21:20:00+01:00"}
            ]})),
            "Departures",
            berlin(),
            6,
            now(),
        );
        assert!(view.available);
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].line, "10");
    }

    #[test]
    fn cancelled_entry_stays_in_time_order() {
        let view = parse_departures(
            &entity(json!({"departures": [
                {"line":"1","destination":"A","departure":"2026-11-22T21:10:00+01:00"},
                {"line":"2","destination":"B","departure":"2026-11-22T21:20:00+01:00","cancelled":true,"remarks":"Replacement bus"}
            ]})),
            "Departures",
            berlin(),
            6,
            now(),
        );
        assert_eq!(view.items.len(), 2);
        assert!(view.items[1].cancelled);
        assert_eq!(view.items[1].remarks.as_deref(), Some("Replacement bus"));
    }

    #[test]
    fn drops_entries_past_the_grace_window() {
        let view = parse_departures(
            &entity(json!({"departures": [
                {"line":"old","destination":"A","departure":"2026-11-22T20:58:59+01:00"},
                {"line":"recent","destination":"B","departure":"2026-11-22T20:59:30+01:00"}
            ]})),
            "Departures",
            berlin(),
            6,
            now(),
        );
        // now is 21:00 in Berlin; 20:58:59 is 61 s ago, 20:59:30 is 30 s ago.
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].line, "recent");
    }

    #[test]
    fn max_items_is_clamped_to_the_contract_range() {
        let entries: Vec<serde_json::Value> = (0..25)
            .map(|i| {
                json!({
                    "line": i.to_string(),
                    "destination": "X",
                    "departure": "2026-11-22T22:00:00+01:00"
                })
            })
            .collect();
        let view = parse_departures(
            &entity(json!({ "departures": entries })),
            "Departures",
            berlin(),
            1000,
            now(),
        );
        assert_eq!(view.items.len(), MAX_ITEMS as usize);

        let view = parse_departures(
            &entity(json!({"departures": [
                {"line":"1","destination":"X","departure":"2026-11-22T22:00:00+01:00"}
            ]})),
            "Departures",
            berlin(),
            0,
            now(),
        );
        assert_eq!(view.items.len(), 1);
    }

    #[test]
    fn missing_or_invalid_attribute_is_unavailable() {
        for attributes in [
            json!({}),
            json!({"departures": null}),
            json!({"departures": "nope"}),
        ] {
            let view = parse_departures(&entity(attributes), "D", berlin(), 6, now());
            assert!(!view.available);
            assert!(view.items.is_empty());
        }
    }

    #[test]
    fn valid_attributes_are_used_even_when_the_sensor_state_is_unknown() {
        let mut unavailable = entity(json!({"departures": [
            {"line":"1","destination":"A","departure":"2026-11-22T21:10:00+01:00"}
        ]}));
        unavailable.state = "unknown".into();
        let view = parse_departures(&unavailable, "D", berlin(), 6, now());
        assert!(view.available);
        assert_eq!(view.items.len(), 1);
    }

    #[test]
    fn empty_attribute_is_empty_not_unavailable() {
        let view = parse_departures(&entity(json!({"departures": []})), "D", berlin(), 6, now());
        assert!(view.available);
        assert!(view.items.is_empty());
    }

    #[test]
    fn display_keeps_empty_and_unavailable_distinct() {
        let empty = DeparturesView {
            entity_id: "sensor.departures".into(),
            title: "D".into(),
            items: Vec::new(),
            available: true,
        };
        let unavailable = DeparturesView {
            available: false,
            ..empty.clone()
        };
        let tz: chrono_tz::Tz = "UTC".parse().unwrap();
        assert_eq!(
            display_departures(None, &tz).status,
            DeparturesStatus::Loading
        );
        assert_eq!(
            display_departures(Some(&empty), &tz).status,
            DeparturesStatus::Empty
        );
        assert_eq!(
            display_departures(Some(&unavailable), &tz).status,
            DeparturesStatus::Unavailable
        );
    }

    #[test]
    fn display_formats_rows_in_the_device_timezone() {
        let view = parse_departures(
            &entity(json!({"departures": [
                {"line":"42","destination":"Central","departure":"2026-11-22T21:55:00+01:00","delay_minutes":2,"platform":"2"},
                {"line":"S1","destination":"Airport","departure":"2026-11-22T22:03:00+01:00","delay_minutes":-3,"cancelled":true,"remarks":"Replacement bus"}
            ]})),
            "Departures",
            berlin(),
            6,
            now(),
        );
        let utc: chrono_tz::Tz = "UTC".parse().unwrap();
        let display = display_departures(Some(&view), &utc);
        assert_eq!(display.status, DeparturesStatus::Populated);
        assert_eq!(display.rows[0].time, "20:55");
        assert_eq!(display.rows[0].delay, "+2 min");
        assert_eq!(display.rows[0].platform, "2");
        assert_eq!(display.rows[1].delay, "3 min early");
        assert!(display.rows[1].cancelled);
        assert_eq!(display.rows[1].remarks, "Replacement bus");
    }

    #[test]
    fn refresher_waits_for_the_interval_and_ignores_stacked_refreshes() {
        let start = Instant::now();
        let mut refresher = DeparturesRefresher::new(60);
        assert!(refresher.should_refresh(start));
        assert!(refresher.begin(start));
        assert!(!refresher.begin(start));
        assert!(!refresher.should_refresh(start + Duration::from_secs(120)));
        refresher.finish();
        assert!(!refresher.should_refresh(start + Duration::from_secs(59)));
        assert!(refresher.should_refresh(start + Duration::from_secs(60)));
    }

    #[test]
    fn refresher_runs_immediately_after_a_reconnect() {
        let start = Instant::now();
        let mut refresher = DeparturesRefresher::new(60);
        assert!(refresher.begin(start));
        refresher.finish();
        assert!(!refresher.should_refresh(start + Duration::from_secs(5)));
        refresher.request_immediate();
        assert!(refresher.should_refresh(start + Duration::from_secs(5)));
    }

    #[test]
    fn refresher_floor_is_ten_seconds() {
        let start = Instant::now();
        let mut refresher = DeparturesRefresher::new(0);
        assert!(refresher.begin(start));
        refresher.finish();
        assert!(!refresher.should_refresh(start + Duration::from_secs(9)));
        assert!(refresher.should_refresh(start + Duration::from_secs(10)));
    }

    fn runtime_config() -> DeparturesWidgetConfig {
        DeparturesWidgetConfig {
            enabled: true,
            entity: "sensor.departures".into(),
            title: "Departures".into(),
            max_items: 6,
            refresh_seconds: 60,
        }
    }

    #[test]
    fn runtime_parses_on_entity_changes_and_ticks() {
        let mut runtime = DeparturesRuntime::new(runtime_config());
        let mut entities = HashMap::new();
        entities.insert(
            "sensor.departures".to_string(),
            entity(json!({"departures": [
                {"line":"1","destination":"A","departure":"2026-11-22T21:10:00+01:00"}
            ]})),
        );

        let view = runtime
            .on_entities(&entities, berlin(), now())
            .expect("first read yields a view");
        assert_eq!(view.items.len(), 1);
        assert_eq!(runtime.view(), Some(&view));
        assert!(
            runtime.on_entities(&entities, berlin(), now()).is_none(),
            "unchanged entity does not trigger a redraw"
        );

        assert_eq!(
            runtime.on_tick(Instant::now(), now()),
            TickOutcome::Refreshed(None)
        );
        assert_eq!(
            runtime.on_tick(Instant::now(), now()),
            TickOutcome::Idle,
            "a second tick inside the interval does nothing"
        );
    }

    #[test]
    fn runtime_reports_unavailable_when_the_entity_is_missing() {
        let mut runtime = DeparturesRuntime::new(runtime_config());
        let view = runtime
            .on_entities(&HashMap::new(), berlin(), now())
            .expect("missing entity yields an unavailable view");
        assert!(!view.available);
        assert!(view.items.is_empty());
    }
}
