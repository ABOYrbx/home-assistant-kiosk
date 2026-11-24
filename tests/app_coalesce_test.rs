//! Redraw coalescing through the real controller and Slint timer wiring.
//!
//! The pure rate-limit math is covered by `redraw_test.rs`; this test checks
//! that a burst of events produces one publish, that the deferred state is not
//! lost, and that the one-second clock timer is independent of the limiter.
//!
//! A single test function is required because the Slint testing backend can
//! only be initialized once per process.

use ha_kiosk::config::Config;
use ha_kiosk::events::HaEvent;
use ha_kiosk::{AppController, AppEvent};
use std::time::Duration;

fn test_config() -> Config {
    toml::from_str(
        r#"
[ha]
url = "http://homeassistant.local:8123"

[cache]
path = "/tmp/ha-kiosk-issue-46-test-cache.json"
"#,
    )
    .expect("test configuration must parse")
}

#[test]
fn coalesces_state_redraws_and_keeps_the_clock_separate() {
    i_slint_backend_testing::init_integration_test_with_mock_time();

    let window = ha_kiosk::MainWindow::new().expect("window");
    let (commands, _rx) = tokio::sync::mpsc::channel(4);
    let mut controller = AppController::new(window, test_config(), commands);

    assert_eq!(controller.redraw_count(), 0);
    assert_eq!(controller.clock_tick_count(), 0);

    // The clock timer runs on its own one-second cadence and does not spend
    // the state redraw budget.
    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    assert_eq!(controller.clock_tick_count(), 1);
    assert_eq!(
        controller.redraw_count(),
        0,
        "a clock tick must not count as a state redraw"
    );
    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    assert_eq!(controller.clock_tick_count(), 2);
    assert_eq!(controller.redraw_count(), 0);

    // A burst of state updates opens exactly one frame and the newest state
    // wins; older events are coalesced rather than dropped from `AppState`.
    for version in 0..25 {
        controller.apply(AppEvent::Ha(HaEvent::Connected {
            ha_version: format!("2026.{version}"),
            time_zone: chrono_tz::Tz::UTC,
        }));
    }
    assert_eq!(
        controller.redraw_count(),
        1,
        "a burst must be coalesced into one redraw"
    );
    assert_eq!(
        controller.state_snapshot().ha_version.as_deref(),
        Some("2026.24")
    );

    // The clock timer ran on its own cadence while the state burst was
    // coalesced; it must not have spent the state redraw budget.
    assert_eq!(controller.clock_tick_count(), 2);

    // Once the limiter interval has passed, the deferred burst is published.
    std::thread::sleep(Duration::from_millis(300));
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(250));
    assert_eq!(
        controller.redraw_count(),
        2,
        "the trailing redraw must publish the deferred state"
    );
    assert_eq!(
        controller.state_snapshot().ha_version.as_deref(),
        Some("2026.24")
    );

    // A later state update is accepted again instead of being starved.
    std::thread::sleep(Duration::from_millis(250));
    controller.apply(AppEvent::Ha(HaEvent::Disconnected {
        reason: "test".to_string(),
    }));
    assert_eq!(controller.redraw_count(), 3);
}
