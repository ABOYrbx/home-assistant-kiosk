use chrono::{TimeZone, Utc};
use ha_kiosk::sources::calendar::{group_agenda, parse_calendar_events};
use serde_json::Value;

fn berlin() -> chrono_tz::Tz {
    "Europe/Berlin".parse().unwrap()
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/calendar_multi.json")).unwrap()
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 7, 0, 0).unwrap()
}

#[test]
fn merges_two_calendars_and_splits_today_tomorrow() {
    let value = fixture();
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
    let value = fixture();
    let items = parse_calendar_events(&value, "calendar.work", berlin()).unwrap();
    let agenda = group_agenda(&items, &chrono_tz::Tz::UTC, now());

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
    let value: Value = serde_json::from_str(include_str!("fixtures/calendar_error.json")).unwrap();
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
