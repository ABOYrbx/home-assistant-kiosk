//! End-to-end tests for the provider-neutral departures widget.
//!
//! Fixtures follow `docs/departures-entity-contract.md`; no live Home
//! Assistant and no transit provider is involved.

use chrono::{DateTime, TimeZone, Utc};
use ha_kiosk::config::DeparturesWidgetConfig;
use ha_kiosk::ha::models::HaEntity;
use ha_kiosk::sources::departures::{
    DeparturesRefresher, DeparturesRuntime, DeparturesStatus, MAX_ITEMS, TickOutcome,
    display_departures, parse_departures,
};
use ha_kiosk::state::DeparturesView;
use std::time::{Duration, Instant};

const CONTRACT: &str = include_str!("fixtures/departures/contract.json");
const MALFORMED: &str = include_str!("fixtures/departures/malformed.json");
const TIMEZONES: &str = include_str!("fixtures/departures/timezones.json");
const CANCELLED: &str = include_str!("fixtures/departures/cancelled.json");
const PAST: &str = include_str!("fixtures/departures/past.json");

fn entity(attributes: &str) -> HaEntity {
    HaEntity {
        entity_id: "sensor.departures".into(),
        state: "ok".into(),
        attributes: serde_json::from_str(attributes).expect("fixture is valid JSON"),
        last_changed: None,
        last_updated: None,
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 22, 20, 0, 0).unwrap()
}

fn berlin() -> chrono_tz::Tz {
    "Europe/Berlin".parse().unwrap()
}

fn utc() -> chrono_tz::Tz {
    "UTC".parse().unwrap()
}

#[test]
fn contract_fixture_parses_sorts_and_keeps_optional_fields() {
    let view = parse_departures(&entity(CONTRACT), "Departures", berlin(), 6, now());
    assert!(view.available);
    assert_eq!(view.items.len(), 2);

    let first = &view.items[0];
    assert_eq!(first.line, "42");
    assert_eq!(first.destination, "Central Station");
    assert_eq!(first.delay_minutes, Some(2));
    assert_eq!(first.platform.as_deref(), Some("2"));
    assert!(!first.cancelled);

    let second = &view.items[1];
    assert_eq!(second.line, "S1");
    assert!(second.cancelled);
    assert_eq!(second.remarks.as_deref(), Some("Replacement bus"));
}

#[test]
fn malformed_entries_are_skipped_and_valid_entries_still_render() {
    let view = parse_departures(&entity(MALFORMED), "Departures", berlin(), 6, now());
    assert!(view.available);
    let lines: Vec<&str> = view.items.iter().map(|item| item.line.as_str()).collect();
    assert_eq!(lines, ["10", "11"]);
}

#[test]
fn timestamps_with_and_without_offsets_are_ordered_by_instant() {
    let view = parse_departures(&entity(TIMEZONES), "Departures", berlin(), 6, now());
    let lines: Vec<&str> = view.items.iter().map(|item| item.line.as_str()).collect();
    // 20:30Z, 21:00Z (16:00-05:00), 21:10Z (22:10 naive in Europe/Berlin).
    assert_eq!(lines, ["B", "C", "A"]);

    let display = display_departures(Some(&view), &utc());
    let times: Vec<&str> = display.rows.iter().map(|row| row.time.as_str()).collect();
    assert_eq!(times, ["20:30", "21:00", "21:10"]);
}

#[test]
fn cancelled_service_stays_in_the_list_and_is_marked() {
    let view = parse_departures(&entity(CANCELLED), "Departures", berlin(), 6, now());
    assert_eq!(view.items.len(), 2);

    let display = display_departures(Some(&view), &berlin());
    assert_eq!(display.status, DeparturesStatus::Populated);
    assert!(display.rows[0].cancelled);
    assert_eq!(display.rows[0].remarks, "Replacement bus");
    assert_eq!(display.rows[0].platform, "1");
    assert_eq!(display.rows[0].time, "21:05");
    assert!(!display.rows[1].cancelled);
    assert_eq!(display.rows[1].delay, "4 min early");
}

#[test]
fn entries_more_than_sixty_seconds_in_the_past_are_dropped() {
    let view = parse_departures(&entity(PAST), "Departures", berlin(), 6, now());
    let lines: Vec<&str> = view.items.iter().map(|item| item.line.as_str()).collect();
    assert_eq!(lines, ["recent", "next"]);
}

#[test]
fn max_items_limits_the_rendered_rows() {
    let view = parse_departures(&entity(TIMEZONES), "Departures", berlin(), 1, now());
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.items[0].line, "B");

    let view = parse_departures(&entity(TIMEZONES), "Departures", berlin(), 500, now());
    assert!(view.items.len() <= MAX_ITEMS as usize);
}

#[test]
fn empty_and_unavailable_are_distinct_display_states() {
    let empty = DeparturesView {
        entity_id: "sensor.departures".into(),
        title: "Departures".into(),
        items: Vec::new(),
        available: true,
    };
    let unavailable = DeparturesView {
        available: false,
        ..empty.clone()
    };

    assert_eq!(
        display_departures(None, &berlin()).status,
        DeparturesStatus::Loading
    );
    assert_eq!(
        display_departures(Some(&empty), &berlin()).status,
        DeparturesStatus::Empty
    );
    assert_eq!(
        display_departures(Some(&unavailable), &berlin()).status,
        DeparturesStatus::Unavailable
    );

    let missing = HaEntity {
        state: "ok".into(),
        attributes: serde_json::json!({}),
        ..entity(CONTRACT)
    };
    let view = parse_departures(&missing, "Departures", berlin(), 6, now());
    assert!(!view.available);
}

#[test]
fn refresh_cadence_waits_ignores_stacked_refreshes_and_runs_on_reconnect() {
    let start = Instant::now();
    let mut refresher = DeparturesRefresher::new(60);

    assert!(refresher.should_refresh(start));
    assert!(refresher.begin(start));
    assert!(
        !refresher.begin(start),
        "a stacked refresh is ignored while one is in flight"
    );
    refresher.finish();
    assert!(!refresher.should_refresh(start + Duration::from_secs(30)));

    refresher.request_immediate();
    assert!(
        refresher.should_refresh(start + Duration::from_secs(30)),
        "a reconnect refreshes immediately"
    );
}

#[test]
fn runtime_reads_the_sensor_and_refreshes_on_tick() {
    let mut runtime = DeparturesRuntime::new(DeparturesWidgetConfig {
        enabled: true,
        entity: "sensor.departures".into(),
        title: "Departures".into(),
        max_items: 6,
        refresh_seconds: 60,
    });

    let mut entities = std::collections::HashMap::new();
    entities.insert("sensor.departures".to_string(), entity(CONTRACT));

    let view = runtime
        .on_entities(&entities, berlin(), now())
        .expect("a new entity yields a view");
    assert_eq!(view.items.len(), 2);

    assert!(
        runtime.on_entities(&entities, berlin(), now()).is_none(),
        "an unchanged entity does not trigger a redraw"
    );
    assert!(matches!(
        runtime.on_tick(Instant::now(), now()),
        TickOutcome::Refreshed(_)
    ));
    assert_eq!(
        runtime.on_tick(Instant::now(), now()),
        TickOutcome::Idle,
        "the second tick inside the interval is skipped"
    );

    runtime.on_reconnect();
    assert!(matches!(
        runtime.on_tick(Instant::now(), now()),
        TickOutcome::Refreshed(_)
    ));
}
