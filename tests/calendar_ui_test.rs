//! Calendar refresh round: entries, partial failures and staleness.
//!
//! The consolidated shell no longer exposes per-page Slint properties, so the
//! rendering itself is covered by the widget; this test covers the view model
//! that feeds the agenda.

use chrono::{TimeZone, Utc};
use ha_kiosk::events::ServiceResponse;
use ha_kiosk::sources::calendar::{CalendarRefresh, refresh_due};
use std::time::Duration;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 8, 0, 0).unwrap()
}

fn berlin() -> chrono_tz::Tz {
    "Europe/Berlin".parse().unwrap()
}

/// Find the request that targets a given calendar entity.
fn request_for<'a>(
    calls: &'a [ha_kiosk::events::ServiceCall],
    id: &str,
) -> &'a ha_kiosk::events::ServiceCall {
    calls
        .iter()
        .find(|call| {
            call.target
                .as_ref()
                .and_then(|target| target.get("entity_id"))
                .and_then(|value| value.as_str())
                == Some(id)
        })
        .unwrap_or_else(|| panic!("no request for {id}"))
}

#[test]
fn a_round_keeps_successful_events_and_reports_a_failed_calendar() {
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(
        &["calendar.family".into(), "calendar.work".into()],
        2,
        berlin(),
        now(),
    );
    assert_eq!(calls.len(), 2);
    assert!(refresh.is_pending(), "the round waits for both calendars");

    let family = request_for(&calls, "calendar.family");
    let work = request_for(&calls, "calendar.work");

    let response = ServiceResponse {
        data: serde_json::json!({
            "calendar.family": {
                "events": [{
                    "summary": "Meeting",
                    "start": { "dateTime": "2026-11-23T09:00:00+01:00" },
                    "end": { "dateTime": "2026-11-23T10:00:00+01:00" },
                    "location": "Room 1"
                }]
            }
        }),
    };
    assert!(refresh.accept(family.request_id, Ok(&response), berlin()));
    assert!(refresh.is_pending(), "one answer does not finish the round");

    assert!(refresh.accept(
        work.request_id,
        Err("calendar.work: unavailable".into()),
        berlin()
    ));
    assert!(!refresh.is_pending());

    let view = refresh.finish(now()).expect("a complete round");
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].summary, "Meeting");
    assert_eq!(view.items[0].location.as_deref(), Some("Room 1"));
    assert_eq!(view.errors.len(), 1);
    assert_eq!(view.errors[0].calendar_id, "calendar.work");
    assert_eq!(view.last_success, Some(now()));
    assert!(!view.stale, "fresh results are never marked stale");
}

#[test]
fn an_empty_calendar_round_has_no_events_and_no_errors() {
    let mut refresh = CalendarRefresh::default();
    let calls = refresh.begin(&["calendar.family".into()], 2, berlin(), now());
    let response = ServiceResponse {
        data: serde_json::json!({ "calendar.family": { "events": [] } }),
    };
    assert!(refresh.accept(calls[0].request_id, Ok(&response), berlin()));

    let view = refresh.finish(now()).expect("a complete round");
    assert!(view.items.is_empty());
    assert!(view.errors.is_empty());
    assert_eq!(view.last_success, Some(now()));
}

#[test]
fn refresh_is_due_only_after_the_interval() {
    let interval = Duration::from_secs(300);
    assert!(refresh_due(None, now(), interval));
    assert!(!refresh_due(Some(now()), now(), interval));
    assert!(refresh_due(
        Some(now()),
        now() + Duration::from_secs(300),
        interval
    ));
}
