//! Departures view model for the consolidated Overview page.
//!
//! The shell no longer exposes departures-specific Slint properties. These
//! tests cover the data the widget renders, including the unavailable state
//! and the cache round-trip used while Home Assistant is offline.

use ha_kiosk::state::{DepartureItem, DeparturesView};

fn view() -> DeparturesView {
    DeparturesView {
        entity_id: "sensor.departures".into(),
        title: "Departures".into(),
        available: true,
        items: vec![
            DepartureItem {
                line: "42".into(),
                destination: "Central Station".into(),
                departure: "2026-11-22T21:55:00+01:00".into(),
                departure_utc: Some("2026-11-22T20:55:00+00:00".into()),
                delay_minutes: Some(2),
                platform: Some("2".into()),
                cancelled: false,
                remarks: None,
            },
            DepartureItem {
                line: "S1".into(),
                destination: "Airport".into(),
                departure: "2026-11-22 22:03:00".into(),
                departure_utc: Some("2026-11-22T21:03:00+00:00".into()),
                delay_minutes: None,
                platform: None,
                cancelled: true,
                remarks: Some("Replacement bus".into()),
            },
        ],
    }
}

#[test]
fn the_view_holds_rows_and_an_unavailable_state() {
    let populated = view();
    assert_eq!(populated.items.len(), 2);
    assert_eq!(populated.items[0].line, "42");
    assert!(populated.items[1].cancelled);
    assert!(populated.available);

    let unavailable = DeparturesView {
        available: false,
        ..view()
    };
    assert!(!unavailable.available);
}

#[test]
fn departures_survive_a_cache_round_trip_with_their_absence_flag() {
    let bytes = serde_json::to_vec(&view()).unwrap();
    let restored: DeparturesView = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, view());

    // Older cache entries without `available` are treated as available, which
    // is the historical default.
    let legacy = r#"{
        "entity_id": "sensor.departures",
        "title": "Departures",
        "items": []
    }"#;
    let legacy: DeparturesView = serde_json::from_str(legacy).unwrap();
    assert!(legacy.available);
}
