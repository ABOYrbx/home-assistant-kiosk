use chrono::{TimeZone, Utc};
use ha_kiosk::state::{CalendarError, CalendarItem, CalendarView};
use ha_kiosk::ui::bridge::UiBridge;
use slint::Model;

#[test]
fn agenda_page_renders_entries_states_and_partial_failures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 11, 23, 8, 0, 0).unwrap();

        // Before the first response the page shows a loading state.
        UiBridge::apply_calendar(&app, None, now, &tz);
        assert!(app.get_agenda_loading());
        assert_eq!(app.get_agenda_today().row_count(), 0);

        let view = CalendarView {
            items: vec![
                CalendarItem {
                    calendar_id: "calendar.family".into(),
                    summary: "Meeting".into(),
                    start: Utc.with_ymd_and_hms(2026, 11, 23, 9, 0, 0).unwrap(),
                    end: Some(Utc.with_ymd_and_hms(2026, 11, 23, 10, 0, 0).unwrap()),
                    all_day: false,
                    location: Some("Room 1".into()),
                },
                CalendarItem {
                    calendar_id: "calendar.family".into(),
                    summary: "Holiday".into(),
                    start: Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap(),
                    end: None,
                    all_day: true,
                    location: None,
                },
            ],
            errors: vec![CalendarError {
                calendar_id: "calendar.work".into(),
                message: "calendar entity is unavailable".into(),
            }],
            last_success: Some(now),
            stale: false,
        };

        UiBridge::apply_calendar(&app, Some(&view), now, &tz);
        assert!(!app.get_agenda_loading());
        assert_eq!(app.get_agenda_today().row_count(), 1);
        assert_eq!(app.get_agenda_tomorrow().row_count(), 1);

        let meeting = app.get_agenda_today().row_data(0).unwrap();
        assert_eq!(meeting.title, "Meeting");
        assert_eq!(meeting.when, "10:00–11:00");
        assert_eq!(meeting.detail, "Room 1");

        let holiday = app.get_agenda_tomorrow().row_data(0).unwrap();
        assert_eq!(holiday.when, "All day");

        assert_eq!(app.get_agenda_error(), "calendar.work unavailable");
        assert!(!app.get_agenda_empty());

        // A successful load without events shows the empty state.
        let empty = CalendarView {
            items: Vec::new(),
            errors: Vec::new(),
            last_success: Some(now),
            stale: false,
        };
        UiBridge::apply_calendar(&app, Some(&empty), now, &tz);
        assert!(app.get_agenda_empty());
        assert_eq!(app.get_agenda_today().row_count(), 0);

        // Cached data is labelled as offline instead of being shown as live.
        let stale = CalendarView {
            stale: true,
            ..view
        };
        UiBridge::apply_calendar(&app, Some(&stale), now, &tz);
        assert_eq!(app.get_agenda_status(), "Offline — showing cached events");

        slint::quit_event_loop().unwrap();
    })
    .unwrap();

    slint::run_event_loop().unwrap();
}
