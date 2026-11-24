//! End-to-end tests for the provider-neutral calendar widget.
//!
//! Fixtures live under `tests/fixtures/`; no live Home Assistant is involved.

use chrono::{TimeZone, Utc};
use ha_kiosk::sources::calendar::{
    CalendarRefresh, build_calendar_calls, group_agenda, parse_calendar_events, sort_items,
};
use serde_json::Value;

fn berlin() -> chrono_tz::Tz {
    "Europe/Berlin".parse().unwrap()
}

fn utc() -> chrono_tz::Tz {
    "UTC".parse().unwrap()
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 7, 0, 0).unwrap()
}

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    let raw = std::fs::read_to_string(path).expect("fixture file exists");
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

const MULTI: &str = include_str!("fixtures/calendar_multi.json");
const ERROR: &str = include_str!("fixtures/calendar_error.json");
const MIXED: &str = include_str!("fixtures/calendar_mixed.json");

#[test]
fn merges_two_calendars_and_splits_today_tomorrow() {
    let value: Value = serde_json::from_str(MULTI).unwrap();
    let mut items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
    items.extend(parse_calendar_events(&value, "calendar.work", berlin()).unwrap());

    let agenda = group_agenda(&items, &berlin(), now());

    let today: Vec<_> = agenda
        .today
        .iter()
        .map(|item| item.summary.as_str())
        .collect();
    let tomorrow: Vec<_> = agenda
        .tomorrow
        .iter()
        .map(|item| item.summary.as_str())
        .collect();

    // Overlapping events are both kept and sorted by start.
    assert_eq!(today, vec!["Breakfast", "Planning", "Review"]);
    // All-day events sort before timed events.
    assert_eq!(tomorrow, vec!["School holiday", "Offsite"]);
}

#[test]
fn converts_ha_local_times_to_the_device_timezone() {
    let value: Value = serde_json::from_str(MULTI).unwrap();
    let items = parse_calendar_events(&value, "calendar.work", berlin()).unwrap();
    let agenda = group_agenda(&items, &utc(), now());

    let planning = agenda
        .today
        .iter()
        .find(|item| item.summary == "Planning")
        .unwrap();
    // 10:00 in Berlin is 09:00 UTC.
    assert_eq!(planning.start.format("%H:%M").to_string(), "09:00");
}

#[test]
fn a_failing_calendar_reports_an_error_without_events() {
    let value: Value = serde_json::from_str(ERROR).unwrap();
    let error = parse_calendar_events(&value, "calendar.family", berlin()).unwrap_err();
    assert!(error.0.contains("calendar.family"));
    assert!(error.0.contains("unavailable"));
}

#[test]
fn a_calendar_without_events_is_valid_and_empty() {
    let value: Value = serde_json::from_str(r#"{"calendar.family": {"events": []}}"#).unwrap();
    let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
    assert!(items.is_empty());
}

#[test]
fn all_day_events_are_kept_at_midnight_utc() {
    let value: Value = serde_json::from_str(MULTI).unwrap();
    let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
    let holiday = items.iter().find(|i| i.summary == "School holiday").unwrap();
    assert!(holiday.all_day);
    assert_eq!(
        holiday.start,
        Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
    );
}

#[test]
fn multiple_calendars_with_one_failing_calendar_in_a_single_round() {
    let value: Value = serde_json::from_str(MIXED).unwrap();
    // Each calendar is parsed independently from the same combined response.
    let family = parse_calendar_events(&value, "calendar.family", berlin());
    let work = parse_calendar_events(&value, "calendar.work", berlin());

    assert!(family.is_ok());
    let items = family.unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|i| i.summary == "School holiday" && i.all_day));

    let err = work.unwrap_err();
    assert!(err.0.contains("calendar.work"));
    assert!(err.0.contains("unavailable"));
}

#[test]
fn refresh_round_with_multiple_calendars_and_one_failing_calendar() {
    let multi: Value = serde_json::from_str(MULTI).unwrap();
    let mixed: Value = serde_json::from_str(MIXED).unwrap();

    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &["calendar.family".into(), "calendar.work".into()],
        2,
        berlin(),
        now(),
    );
    assert_eq!(calls.len(), 2);

    // First calendar succeeds with the multi fixture's family node.
    let family_data = serde_json::json!({
        "calendar.family": multi.get("calendar.family").cloned().unwrap()
    });
    assert!(refresh.accept(
        calls[0].request_id,
        Ok(&ha_kiosk::events::ServiceResponse { data: family_data }),
        berlin()
    ));

    // Second calendar fails.
    let bad_data = serde_json::json!({
        "calendar.work": mixed.get("calendar.work").cloned().unwrap()
    });
    assert!(refresh.accept(
        calls[1].request_id,
        Ok(&ha_kiosk::events::ServiceResponse { data: bad_data }),
        berlin()
    ));

    let view = refresh.finish(now()).unwrap();
    assert_eq!(view.items.len(), 2);
    assert!(view.items.iter().any(|i| i.summary == "Breakfast"));
    assert!(view.items.iter().any(|i| i.summary == "School holiday"));
    assert_eq!(view.errors.len(), 1);
    assert_eq!(view.errors[0].calendar_id, "calendar.work");
    assert_eq!(view.last_success, Some(now()));
}

#[test]
fn failing_calendar_does_not_hide_the_others_via_refresh() {
    let good = serde_json::json!({
        "calendar.family": {
            "events": [{ "summary": "Dinner", "start": { "dateTime": "2026-11-23T18:00:00+01:00" } }]
        }
    });
    let bad = serde_json::json!({ "calendar.work": { "error": { "message": "calendar unavailable" } } });

    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &["calendar.family".into(), "calendar.work".into()],
        2,
        berlin(),
        now(),
    );
    assert!(refresh.accept(
        calls[0].request_id,
        Ok(&ha_kiosk::events::ServiceResponse { data: good }),
        berlin()
    ));
    assert!(refresh.accept(
        calls[1].request_id,
        Ok(&ha_kiosk::events::ServiceResponse { data: bad }),
        berlin()
    ));

    let view = refresh.finish(now()).unwrap();
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].summary, "Dinner");
    assert_eq!(view.errors.len(), 1);
    assert_eq!(view.errors[0].calendar_id, "calendar.work");
}

#[test]
fn refresh_round_reports_error_when_a_request_fails() {
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &["calendar.family".into(), "calendar.work".into()],
        2,
        berlin(),
        now(),
    );
    assert!(refresh.accept(
        calls[0].request_id,
        Err("connection lost".to_string()),
        berlin()
    ));
    assert!(refresh.accept(
        calls[1].request_id,
        Err("connection lost".to_string()),
        berlin()
    ));
    let view = refresh.finish(now()).unwrap();
    assert!(view.items.is_empty());
    assert_eq!(view.errors.len(), 2);
    assert_eq!(view.last_success, None);
}

#[test]
fn refresh_abandon_reports_pending_errors() {
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &["calendar.family".into(), "calendar.work".into()],
        2,
        berlin(),
        now(),
    );
    // Answer only the first; abandon the round.
    let good = serde_json::json!({
        "calendar.family": { "events": [{ "summary": "Dinner", "start": { "dateTime": "2026-11-23T18:00:00+01:00" } }] }
    });
    assert!(refresh.accept(
        calls[0].request_id,
        Ok(&ha_kiosk::events::ServiceResponse { data: good }),
        berlin()
    ));
    let errors = refresh.abandon("disconnected").unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].calendar_id, "calendar.work");
    assert!(refresh.finish(now()).is_none());
}

#[test]
fn finish_waits_for_all_requests() {
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(&["calendar.family".into()], 2, berlin(), now());
    assert!(refresh.finish(now()).is_none());
    let good = serde_json::json!({
        "calendar.family": { "events": [] }
    });
    assert!(refresh.accept(
        calls[0].request_id,
        Ok(&ha_kiosk::events::ServiceResponse { data: good }),
        berlin()
    ));
    assert!(refresh.finish(now()).is_some());
}

#[test]
fn builds_queries_for_multiple_calendars() {
    let calls = build_calendar_calls(
        &["calendar.family".into(), "calendar.work".into()],
        2,
        berlin(),
        now(),
    );
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].target.as_ref().unwrap()["entity_id"], "calendar.family");
    assert_eq!(calls[1].target.as_ref().unwrap()["entity_id"], "calendar.work");
    for call in &calls {
        assert_eq!(call.domain, "calendar");
        assert_eq!(call.service, "get_events");
        assert!(call.return_response);
        assert_eq!(call.data["start_date_time"], "2026-11-23T00:00:00+01:00");
        assert_eq!(call.data["end_date_time"], "2026-11-25T00:00:00+01:00");
    }
}

#[test]
fn sort_items_puts_all_day_first_then_by_start() {
    use ha_kiosk::state::CalendarItem;
    let mut items = vec![
        CalendarItem {
            calendar_id: "c".into(),
            summary: "Late".into(),
            start: Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap(),
            end: None,
            all_day: false,
            location: None,
        },
        CalendarItem {
            calendar_id: "c".into(),
            summary: "All day".into(),
            start: Utc.with_ymd_and_hms(2026, 11, 23, 0, 0, 0).unwrap(),
            end: None,
            all_day: true,
            location: None,
        },
        CalendarItem {
            calendar_id: "c".into(),
            summary: "Early".into(),
            start: Utc.with_ymd_and_hms(2026, 11, 23, 9, 0, 0).unwrap(),
            end: None,
            all_day: false,
            location: None,
        },
    ];
    sort_items(&mut items);
    let summaries: Vec<_> = items.iter().map(|i| i.summary.as_str()).collect();
    assert_eq!(summaries, vec!["All day", "Early", "Late"]);
}

#[test]
fn group_agenda_keeps_ongoing_events_today() {
    let value: Value = serde_json::from_str(MULTI).unwrap();
    // Use a combined single-calendar payload with an ongoing event.
    let payload = serde_json::json!({
        "calendar.family": { "events": [
            { "summary": "Ongoing", "start": { "dateTime": "2026-11-22T20:00:00+01:00" }, "end": { "dateTime": "2026-11-23T20:00:00+01:00" } },
            { "summary": "Tomorrow", "start": { "dateTime": "2026-11-24T09:00:00+01:00" } },
            { "summary": "Too late", "start": { "dateTime": "2026-11-25T09:00:00+01:00" } }
        ]}
    });
    let items = parse_calendar_events(&payload, "calendar.family", berlin()).unwrap();
    let agenda = group_agenda(&items, &berlin(), now());
    let today: Vec<_> = agenda.today.iter().map(|i| i.summary.as_str()).collect();
    let tomorrow: Vec<_> = agenda.tomorrow.iter().map(|i| i.summary.as_str()).collect();
    assert_eq!(today, vec!["Ongoing"]);
    assert_eq!(tomorrow, vec!["Tomorrow"]);
    let _ = value;
}

#[test]
fn converts_naive_timestamps_in_ha_timezone() {
    let payload = serde_json::json!({
        "calendar.family": { "events": [{
            "summary": "Late",
            "start": { "dateTime": "2026-11-23T23:30:00" },
            "end": { "dateTime": "2026-11-24T00:30:00" }
        }]}
    });
    let items = parse_calendar_events(&payload, "calendar.family", berlin()).unwrap();
    assert_eq!(items.len(), 1);
    // 23:30 in Berlin is 22:30 UTC.
    assert_eq!(
        items[0].start,
        Utc.with_ymd_and_hms(2026, 11, 23, 22, 30, 0).unwrap()
    );
}

#[test]
fn interprets_error_as_string() {
    let payload = serde_json::json!({
        "calendar.family": { "error": "boom" }
    });
    let err = parse_calendar_events(&payload, "calendar.family", berlin()).unwrap_err();
    assert!(err.0.contains("boom"));
}

#[test]
fn parses_response_without_calendar_wrapper() {
    let payload = serde_json::json!({
        "events": [{ "summary": "Solo", "start": { "dateTime": "2026-11-23T10:00:00+01:00" } }]
    });
    let items = parse_calendar_events(&payload, "calendar.solo", berlin()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].summary, "Solo");
}

#[test]
fn parses_array_response() {
    let payload = serde_json::json!([
        { "summary": "Solo", "start": { "dateTime": "2026-11-23T10:00:00+01:00" } }
    ]);
    let items = parse_calendar_events(&payload, "calendar.solo", berlin()).unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn uses_fixture_helper() {
    let value = fixture("calendar_multi.json");
    let items = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
    assert!(!items.is_empty());
}