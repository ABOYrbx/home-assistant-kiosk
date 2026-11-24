//! Widget-level test for the departures states, run in its own test binary so
//! the Slint testing platform is initialized once per process.

use ha_kiosk::DeparturesState;
use ha_kiosk::state::{DepartureItem, DeparturesView};
use ha_kiosk::ui::bridge::UiBridge;
use i_slint_backend_testing::ElementHandle;
use slint::Model;

#[test]
fn departures_widget_renders_each_data_state() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        app.set_departures_visible(true);

        let has = |id: &str| {
            ElementHandle::find_by_element_id(&app, &format!("DeparturesWidget::{id}"))
                .next()
                .is_some()
        };

        assert_eq!(app.get_departures_state(), DeparturesState::Loading);
        assert!(has("departures-loading"));

        app.set_departures_state(DeparturesState::Unavailable);
        assert!(has("departures-unavailable"));

        app.set_departures_state(DeparturesState::Empty);
        assert!(has("departures-empty"));

        let view = DeparturesView {
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
        };

        UiBridge::apply_departures(&app, Some(&view), "Departures");
        assert_eq!(app.get_departures_state(), DeparturesState::Populated);
        assert_eq!(app.get_departures_rows().row_count(), 2);
        assert!(has("departures-rows"));

        let unavailable = DeparturesView {
            available: false,
            ..view
        };
        UiBridge::apply_departures(&app, Some(&unavailable), "Departures");
        assert_eq!(app.get_departures_state(), DeparturesState::Unavailable);
        assert_eq!(app.get_departures_rows().row_count(), 0);

        app.set_departures_visible(false);
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
