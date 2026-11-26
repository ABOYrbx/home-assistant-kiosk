use crate::events::{EntityId, ServiceCall, ServiceResponse, SourceError};
use crate::state::{CalendarError, CalendarItem, CalendarView};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

/// Home Assistant refreshes calendar events every five minutes by default.
pub const DEFAULT_REFRESH: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Default)]
pub struct Agenda {
    pub today: Vec<CalendarItem>,
    pub tomorrow: Vec<CalendarItem>,
}

impl Agenda {
    pub fn is_empty(&self) -> bool {
        self.today.is_empty() && self.tomorrow.is_empty()
    }
}

/// Build one `calendar.get_events` call per configured calendar.
///
/// The range starts at the beginning of today in the Home Assistant timezone
/// and covers `days` whole days, so an event that started earlier today and is
/// still running is included.
pub fn build_calendar_calls(
    calendar_ids: &[EntityId],
    days: u32,
    ha_tz: Tz,
    now: DateTime<Utc>,
) -> Vec<ServiceCall> {
    let start = now
        .with_timezone(&ha_tz)
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| ha_tz.from_local_datetime(&naive).earliest());
    let Some(start) = start else {
        return Vec::new();
    };
    let end = start + ChronoDuration::days(i64::from(days.max(1)));

    calendar_ids
        .iter()
        .map(|id| ServiceCall {
            request_id: crate::ha::client::next_request_id(),
            domain: "calendar".to_string(),
            service: "get_events".to_string(),
            target: Some(json!({ "entity_id": id })),
            data: json!({
                "start_date_time": start.to_rfc3339(),
                "end_date_time": end.to_rfc3339(),
            }),
            return_response: true,
        })
        .collect()
}

/// Parse the `return_response` payload of one `calendar.get_events` call.
///
/// Home Assistant answers with `{ "<entity_id>": { "events": [...] } }`. A
/// single calendar may also fail on its own; that error must not hide the
/// events of the other calendars, so it is reported as an `Err` for this
/// calendar only.
pub fn parse_calendar_events(
    value: &Value,
    calendar_id: &str,
    ha_tz: Tz,
) -> Result<Vec<CalendarItem>, SourceError> {
    let node = calendar_node(value, calendar_id)
        .ok_or_else(|| SourceError(format!("calendar {calendar_id}: unexpected response")))?;

    if let Some(error) = node.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
            .unwrap_or("calendar request failed");
        return Err(SourceError(format!("{calendar_id}: {message}")));
    }

    let events = node
        .get("events")
        .and_then(Value::as_array)
        .or_else(|| node.as_array())
        .ok_or_else(|| SourceError(format!("{calendar_id}: missing events")))?;

    let mut items = Vec::new();
    for event in events {
        if let Some(item) = parse_event(event, calendar_id, ha_tz) {
            items.push(item);
        }
    }
    Ok(items)
}

fn calendar_node<'a>(value: &'a Value, calendar_id: &str) -> Option<&'a Value> {
    if let Some(node) = value.get(calendar_id) {
        return Some(node);
    }
    if value.get("events").is_some() || value.is_array() {
        return Some(value);
    }
    // Older Home Assistant versions wrap the response in a single object.
    value
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.values().next())
}

fn parse_event(event: &Value, calendar_id: &str, ha_tz: Tz) -> Option<CalendarItem> {
    let summary = event
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .unwrap_or("(no title)")
        .to_string();

    let (start, all_day) = parse_timestamp(event.get("start")?, ha_tz)?;
    let end = event
        .get("end")
        .and_then(|value| parse_timestamp(value, ha_tz))
        .map(|(timestamp, _)| timestamp);
    let location = event
        .get("location")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|location| !location.is_empty())
        .map(str::to_string);

    Some(CalendarItem {
        calendar_id: calendar_id.to_string(),
        summary,
        start,
        end,
        all_day,
        location,
    })
}

/// Returns the instant and whether the value was a date-only (all-day) value.
///
/// All-day dates are calendar dates, not instants. They are kept at midnight
/// UTC so that a kiosk in another timezone does not shift them to the previous
/// or next day. Date-times without an offset are interpreted in the Home
/// Assistant timezone from `get_config`.
fn parse_timestamp(value: &Value, ha_tz: Tz) -> Option<(DateTime<Utc>, bool)> {
    if let Some(date) = value.get("date").and_then(Value::as_str) {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        return Some((date.and_hms_opt(0, 0, 0)?.and_utc(), true));
    }
    let raw = value
        .get("dateTime")
        .and_then(Value::as_str)
        .or_else(|| value.as_str())?;
    parse_datetime(raw, ha_tz).map(|timestamp| (timestamp, false))
}

fn parse_datetime(raw: &str, ha_tz: Tz) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    let formats = [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];
    for format in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, format) {
            return ha_tz
                .from_local_datetime(&naive)
                .earliest()
                .map(|local| local.with_timezone(&Utc));
        }
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|midnight| midnight.and_utc())
}

/// All-day events first, then by start, then by title. The calendar id breaks
/// ties so overlapping events from different calendars keep a stable order.
pub fn sort_items(items: &mut [CalendarItem]) {
    items.sort_by(|a, b| {
        b.all_day
            .cmp(&a.all_day)
            .then_with(|| a.start.cmp(&b.start))
            .then_with(|| a.summary.cmp(&b.summary))
            .then_with(|| a.calendar_id.cmp(&b.calendar_id))
    });
}

/// Split events into today and tomorrow in the device timezone.
///
/// An event is listed on a day when it overlaps that day, so an event that
/// started yesterday and is still running appears today. Events outside the
/// two days are dropped.
pub fn group_agenda<Tz: TimeZone>(
    items: &[CalendarItem],
    device_tz: &Tz,
    now: DateTime<Utc>,
) -> Agenda {
    let today = now.with_timezone(device_tz).date_naive();
    let tomorrow = today.succ_opt().unwrap_or(today);
    let day_after_tomorrow = tomorrow.succ_opt().unwrap_or(tomorrow);

    let today_start = local_midnight(today, device_tz).unwrap_or(now);
    let tomorrow_start = local_midnight(tomorrow, device_tz).unwrap_or(today_start);
    let day_after_start = local_midnight(day_after_tomorrow, device_tz).unwrap_or(tomorrow_start);

    let mut agenda = Agenda::default();
    for item in items {
        if item.all_day {
            let start_date = item.start.date_naive();
            let end_date = item
                .end
                .map(|end| end.date_naive())
                .unwrap_or_else(|| start_date.succ_opt().unwrap_or(start_date));
            if start_date <= today && today < end_date {
                agenda.today.push(item.clone());
            }
            if start_date <= tomorrow && tomorrow < end_date {
                agenda.tomorrow.push(item.clone());
            }
            continue;
        }
        if overlaps(item, today_start, tomorrow_start) {
            agenda.today.push(item.clone());
        }
        if overlaps(item, tomorrow_start, day_after_start) {
            agenda.tomorrow.push(item.clone());
        }
    }
    sort_items(&mut agenda.today);
    sort_items(&mut agenda.tomorrow);
    agenda
}

fn overlaps(item: &CalendarItem, day_start: DateTime<Utc>, day_end: DateTime<Utc>) -> bool {
    let end = item.end.unwrap_or(item.start);
    item.start < day_end && end > day_start
}

fn local_midnight<Tz: TimeZone>(date: NaiveDate, device_tz: &Tz) -> Option<DateTime<Utc>> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    device_tz
        .from_local_datetime(&naive)
        .earliest()
        .map(|local| local.with_timezone(&Utc))
}

pub fn refresh_due(
    last_success: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    interval: Duration,
) -> bool {
    let interval = ChronoDuration::from_std(interval).unwrap_or(ChronoDuration::zero());
    match last_success {
        None => true,
        Some(last) => now.signed_duration_since(last) >= interval,
    }
}

/// Collects the per-calendar responses of one refresh round.
#[derive(Debug, Default)]
pub struct CalendarRefresh {
    pending: HashMap<u64, EntityId>,
    results: HashMap<EntityId, Result<Vec<CalendarItem>, String>>,
    requested_at: Option<DateTime<Utc>>,
}

impl CalendarRefresh {
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Start a refresh round and return the service calls to send.
    pub fn begin(
        &mut self,
        calendar_ids: &[EntityId],
        days: u32,
        ha_tz: Tz,
        now: DateTime<Utc>,
    ) -> Vec<ServiceCall> {
        self.pending.clear();
        self.results.clear();
        self.requested_at = Some(now);

        let mut calls = Vec::with_capacity(calendar_ids.len());
        for id in calendar_ids {
            let mut call = build_calendar_calls(std::slice::from_ref(id), days, ha_tz, now);
            if let Some(call) = call.pop() {
                self.pending.insert(call.request_id, id.clone());
                calls.push(call);
            }
        }
        calls
    }

    /// Feed a service result. Returns true when the request belonged to this
    /// refresh round.
    pub fn accept(
        &mut self,
        request_id: u64,
        result: Result<&ServiceResponse, String>,
        ha_tz: Tz,
    ) -> bool {
        let Some(calendar_id) = self.pending.remove(&request_id) else {
            return false;
        };
        let parsed = match result {
            Ok(response) => {
                parse_calendar_events(&response.data, &calendar_id, ha_tz).map_err(|e| e.0)
            }
            Err(message) => Err(format!("{calendar_id}: {message}")),
        };
        self.results.insert(calendar_id, parsed);
        true
    }

    /// Build the view once every request of the round has answered.
    pub fn finish(&mut self, now: DateTime<Utc>) -> Option<CalendarView> {
        if self.requested_at.is_none() || !self.pending.is_empty() {
            return None;
        }
        let mut view = CalendarView::default();
        let mut succeeded = 0usize;
        for (calendar_id, result) in self.results.drain() {
            match result {
                Ok(items) => {
                    succeeded += 1;
                    view.items.extend(items);
                }
                Err(message) => view.errors.push(CalendarError {
                    calendar_id,
                    message,
                }),
            }
        }
        sort_items(&mut view.items);
        if succeeded > 0 {
            view.last_success = Some(now);
        }
        self.requested_at = None;
        Some(view)
    }

    /// Record a failed round, for example after a disconnect.
    pub fn abandon(&mut self, message: &str) -> Option<Vec<CalendarError>> {
        self.requested_at.as_ref()?;
        let errors: Vec<_> = self
            .pending
            .drain()
            .map(|(_, calendar_id)| CalendarError {
                calendar_id,
                message: message.to_string(),
            })
            .collect();
        self.results.clear();
        self.requested_at = None;
        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn berlin() -> Tz {
        "Europe/Berlin".parse().unwrap()
    }

    fn utc() -> Tz {
        Tz::UTC
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 23, 8, 0, 0).unwrap()
    }

    #[test]
    fn parses_timed_and_all_day_events() {
        let value = json!({
            "calendar.family": {
                "events": [
                    {
                        "summary": "Meeting",
                        "start": { "dateTime": "2026-11-23T10:00:00+01:00" },
                        "end": { "dateTime": "2026-11-23T11:00:00+01:00" },
                        "location": "Room 1"
                    },
                    {
                        "summary": "Holiday",
                        "start": { "date": "2026-11-24" },
                        "end": { "date": "2026-11-25" }
                    }
                ]
            }
        });

        let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].summary, "Meeting");
        assert_eq!(
            items[0].start,
            Utc.with_ymd_and_hms(2026, 11, 23, 9, 0, 0).unwrap()
        );
        assert_eq!(items[0].location.as_deref(), Some("Room 1"));
        assert!(items[1].all_day);
        assert_eq!(
            items[1].start,
            Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn interprets_naive_timestamps_in_ha_timezone() {
        let value = json!({
            "calendar.family": {
                "events": [{
                    "summary": "Late",
                    "start": { "dateTime": "2026-11-23T23:30:00" },
                    "end": { "dateTime": "2026-11-24T00:30:00" }
                }]
            }
        });

        let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
        assert_eq!(items.len(), 1);
        // 23:30 in Berlin is 22:30 UTC.
        assert_eq!(
            items[0].start,
            Utc.with_ymd_and_hms(2026, 11, 23, 22, 30, 0).unwrap()
        );
    }

    #[test]
    fn converts_to_the_device_timezone_for_display() {
        let value = json!({
            "calendar.family": {
                "events": [{
                    "summary": "Standup",
                    "start": { "dateTime": "2026-11-23T09:00:00" }
                }]
            }
        });
        let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
        let agenda = group_agenda(&items, &utc(), now());
        assert_eq!(agenda.today.len(), 1);
        let local = agenda.today[0].start.with_timezone(&utc());
        assert_eq!(local.format("%H:%M").to_string(), "08:00");
    }

    #[test]
    fn failing_calendar_does_not_hide_the_others() {
        let good = json!({
            "calendar.family": {
                "events": [{
                    "summary": "Dinner",
                    "start": { "dateTime": "2026-11-23T18:00:00+01:00" }
                }]
            }
        });
        let bad = json!({ "calendar.work": { "error": { "message": "calendar unavailable" } } });

        let mut refresh = CalendarRefresh::default();
        let calls = refresh.begin(
            &["calendar.family".into(), "calendar.work".into()],
            2,
            berlin(),
            now(),
        );
        assert_eq!(calls.len(), 2);
        assert!(refresh.accept(
            calls[0].request_id,
            Ok(&ServiceResponse { data: good }),
            berlin()
        ));
        assert!(refresh.accept(
            calls[1].request_id,
            Ok(&ServiceResponse { data: bad }),
            berlin()
        ));

        let view = refresh.finish(now()).unwrap();
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].summary, "Dinner");
        assert_eq!(view.errors.len(), 1);
        assert_eq!(view.errors[0].calendar_id, "calendar.work");
        assert_eq!(view.last_success, Some(now()));
    }

    #[test]
    fn merges_and_sorts_overlapping_events() {
        let first = json!({
            "calendar.family": { "events": [
                { "summary": "Late", "start": { "dateTime": "2026-11-23T12:00:00+01:00" } },
                { "summary": "All day", "start": { "date": "2026-11-23" }, "end": { "date": "2026-11-24" } }
            ]}
        });
        let second = json!({
            "calendar.work": { "events": [
                { "summary": "Early", "start": { "dateTime": "2026-11-23T09:00:00+01:00" } }
            ]}
        });

        let mut refresh = CalendarRefresh::default();
        let calls = refresh.begin(
            &["calendar.family".into(), "calendar.work".into()],
            2,
            berlin(),
            now(),
        );
        for (call, data) in calls.iter().zip([first, second]) {
            refresh.accept(call.request_id, Ok(&ServiceResponse { data }), berlin());
        }
        let view = refresh.finish(now()).unwrap();
        let summaries: Vec<_> = view
            .items
            .iter()
            .map(|item| item.summary.as_str())
            .collect();
        assert_eq!(summaries, vec!["All day", "Early", "Late"]);
    }

    #[test]
    fn groups_today_and_tomorrow_and_keeps_ongoing_events() {
        let value = json!({
            "calendar.family": { "events": [
                { "summary": "All day today", "start": { "date": "2026-11-23" }, "end": { "date": "2026-11-24" } },
                { "summary": "Ongoing", "start": { "dateTime": "2026-11-22T20:00:00+01:00" }, "end": { "dateTime": "2026-11-23T20:00:00+01:00" } },
                { "summary": "Tomorrow", "start": { "dateTime": "2026-11-24T09:00:00+01:00" } },
                { "summary": "Too late", "start": { "dateTime": "2026-11-25T09:00:00+01:00" } }
            ]}
        });
        let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
        let agenda = group_agenda(&items, &berlin(), now());

        let today: Vec<_> = agenda.today.iter().map(|i| i.summary.as_str()).collect();
        let tomorrow: Vec<_> = agenda.tomorrow.iter().map(|i| i.summary.as_str()).collect();
        assert_eq!(today, vec!["All day today", "Ongoing"]);
        assert_eq!(tomorrow, vec!["Tomorrow"]);
    }

    #[test]
    fn multi_day_all_day_event_appears_on_each_overlapping_day() {
        let value = json!({
            "calendar.family": { "events": [
                { "summary": "Holiday", "start": { "date": "2026-11-23" }, "end": { "date": "2026-11-25" } }
            ]}
        });
        let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
        let agenda = group_agenda(&items, &berlin(), now());
        assert_eq!(agenda.today.len(), 1);
        assert_eq!(agenda.tomorrow.len(), 1);
    }

    #[test]
    fn builds_queries_in_the_ha_timezone() {
        let calls = build_calendar_calls(&["calendar.family".into()], 2, berlin(), now());
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call.domain, "calendar");
        assert_eq!(call.service, "get_events");
        assert!(call.return_response);
        assert_eq!(call.data["start_date_time"], "2026-11-23T00:00:00+01:00");
        assert_eq!(call.data["end_date_time"], "2026-11-25T00:00:00+01:00");
        assert_eq!(
            call.target.as_ref().unwrap()["entity_id"],
            "calendar.family"
        );
    }

    #[test]
    fn refresh_is_due_after_the_interval() {
        assert!(refresh_due(None, now(), DEFAULT_REFRESH));
        assert!(!refresh_due(
            Some(now() - ChronoDuration::seconds(299)),
            now(),
            DEFAULT_REFRESH
        ));
        assert!(refresh_due(
            Some(now() - ChronoDuration::seconds(300)),
            now(),
            DEFAULT_REFRESH
        ));
    }
}
