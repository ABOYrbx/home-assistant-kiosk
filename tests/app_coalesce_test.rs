//! Redraw coalescing through the consolidated controller.
//!
//! The pure limiter is covered by `redraw_test.rs`; this test checks that a
//! burst of controller events is folded into a single redraw slot while the
//! window stays responsive. The old per-event counters were removed with the
//! UI consolidation, so the scheduler is exercised directly here.

use ha_kiosk::config::Config;
use ha_kiosk::events::HaEvent;
use ha_kiosk::ui::redraw::{MIN_REDRAW_INTERVAL, RedrawScheduler, RedrawSlot};
use ha_kiosk::{AppController, AppEvent, MainWindow};
use std::time::Duration;

fn test_config() -> Config {
    toml::from_str(
        r#"
[ha]
url = "http://homeassistant.local:8123"

[cache]
path = "/tmp/ha-kiosk-issue-231-test-cache.json"
"#,
    )
    .expect("test configuration must parse")
}

#[test]
fn a_burst_opens_one_redraw_slot_and_navigation_still_works() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    let window = MainWindow::new().expect("window");
    let (commands, _rx) = tokio::sync::mpsc::channel(4);
    let mut controller = AppController::new(window, test_config(), commands);

    assert_eq!(MIN_REDRAW_INTERVAL, Duration::from_millis(250));

    // A burst that fits inside one limiter interval may open only one frame.
    let mut scheduler = RedrawScheduler::new();
    let mut ready = 0;
    for version in 0..25 {
        controller.apply(AppEvent::Ha(HaEvent::Connected {
            ha_version: format!("2026.{version}"),
            time_zone: chrono_tz::Tz::UTC,
        }));
        if scheduler.request(Duration::from_millis(version * 10)) == RedrawSlot::Ready {
            ready += 1;
        }
    }
    assert_eq!(ready, 1, "a burst must be coalesced into one redraw");

    // Once the interval has passed, the trailing redraw is published.
    assert_eq!(
        scheduler.request(Duration::from_millis(250)),
        RedrawSlot::Ready
    );

    // The controller still owns page navigation after the burst.
    assert_eq!(controller.window().get_current_page(), 0);
    controller.next_page();
    assert_eq!(controller.window().get_current_page(), 1);
    controller.previous_page();
    assert_eq!(controller.window().get_current_page(), 0);
}
