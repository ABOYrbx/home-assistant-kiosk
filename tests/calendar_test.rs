use chrono::{TimeZone, Utc};
use ha_kiosk::events::ServiceResponse;
use ha_kiosk::sources::calendar::{CalendarRefresh, group_agenda, parse_calendar_events};
use serde_json::{Value, json};

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

fn mixed() -> Value {
    serde_json::from_str(include_str!("fixtures/calendar_mixed.json")).expect("mixed fixture")
}

#[test]
fn date_only_events_stay_all_day() {
    let payload = json!({
        "calendar.family": { "events": [
            { "summary": "Holiday", "start": "2026-11-24", "end": "2026-11-25" }
        ]}
    });
    let items = parse_calendar_events(&payload, "calendar.family", berlin()).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].all_day);
    assert_eq!(
        items[0].start,
        Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
    );
}

#[test]
fn mixed_fixture_parses_each_calendar_independently() {
    let value = mixed();
    let family = parse_calendar_events(&value, "calendar.family", berlin()).unwrap();
    let work = parse_calendar_events(&value, "calendar.work", berlin()).unwrap_err();

    assert_eq!(family.len(), 2);
    assert!(
        family
            .iter()
            .any(|item| item.summary == "School holiday" && item.all_day)
    );
    assert!(work.0.contains("calendar.work"));
    assert!(work.0.contains("unavailable"));
}

#[test]
fn refresh_round_deduplicates_repeated_calendar_ids() {
    let mixed = mixed();
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &[
            "calendar.family".into(),
            "calendar.family".into(),
            "calendar.work".into(),
        ],
        2,
        berlin(),
        now(),
    );
    assert_eq!(calls.len(), 2);

    assert!(refresh.accept(
        calls[1].request_id,
        Err("calendar entity is unavailable".to_string()),
        berlin()
    ));
    let family_data = json!({
        "calendar.family": mixed.get("calendar.family").cloned().unwrap()
    });
    assert!(refresh.accept(
        calls[0].request_id,
        Ok(&ServiceResponse { data: family_data }),
        berlin()
    ));

    let view = refresh.finish(now()).unwrap();
    let summaries: Vec<_> = view
        .items
        .iter()
        .map(|item| item.summary.as_str())
        .collect();
    assert_eq!(summaries, vec!["School holiday", "Breakfast"]);
    assert_eq!(view.errors.len(), 1);
    assert_eq!(view.errors[0].calendar_id, "calendar.work");
}

#[test]
fn refresh_round_errors_are_sorted_by_calendar_id() {
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &[
            "calendar.work".into(),
            "calendar.school".into(),
            "calendar.family".into(),
        ],
        2,
        berlin(),
        now(),
    );
    for call in &calls {
        assert!(refresh.accept(
            call.request_id,
            Err("connection lost".to_string()),
            berlin()
        ));
    }
    let view = refresh.finish(now()).unwrap();
    let error_ids: Vec<_> = view
        .errors
        .iter()
        .map(|error| error.calendar_id.as_str())
        .collect();
    assert_eq!(
        error_ids,
        vec!["calendar.family", "calendar.school", "calendar.work"]
    );
}
