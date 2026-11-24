//! Rate limiting tests for the state-driven redraw scheduler.
//!
//! These exercise the pure limiter; the Slint timer wiring lives in
//! `app_coalesce_test.rs`.

use ha_kiosk::ui::redraw::{
    MAX_REDRAWS_PER_SECOND, MIN_REDRAW_INTERVAL, RedrawScheduler, RedrawSlot,
};
use std::time::Duration;

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn first_request_is_ready() {
    let mut scheduler = RedrawScheduler::new();
    assert_eq!(scheduler.request(ms(0)), RedrawSlot::Ready);
    assert_eq!(scheduler.last_redraw(), Some(ms(0)));
}

#[test]
fn cap_is_four_per_second() {
    assert_eq!(MAX_REDRAWS_PER_SECOND, 4);
    assert_eq!(MIN_REDRAW_INTERVAL, ms(250));
    assert_eq!(RedrawScheduler::new().min_interval(), ms(250));
    assert_eq!(RedrawScheduler::with_limit(10).min_interval(), ms(100));
}

#[test]
fn burst_is_coalesced_into_one_redraw() {
    let mut scheduler = RedrawScheduler::new();
    let now = ms(1_000);
    let mut ready = 0;
    for _ in 0..100 {
        if scheduler.request(now) == RedrawSlot::Ready {
            ready += 1;
        }
    }
    assert_eq!(ready, 1, "a burst may only open one redraw slot");
}

#[test]
fn deferred_request_reports_the_remaining_delay() {
    let mut scheduler = RedrawScheduler::new();
    assert_eq!(scheduler.request(ms(0)), RedrawSlot::Ready);

    assert_eq!(scheduler.request(ms(100)), RedrawSlot::Deferred(ms(150)));
    assert_eq!(scheduler.request(ms(249)), RedrawSlot::Deferred(ms(1)));
    assert_eq!(scheduler.request(ms(250)), RedrawSlot::Ready);
}

#[test]
fn deferred_requests_do_not_consume_budget() {
    let mut scheduler = RedrawScheduler::new();
    assert_eq!(scheduler.request(ms(0)), RedrawSlot::Ready);

    // A sustained burst keeps asking; the stored slot must stay at 0 ms.
    for tick in 1..=249 {
        assert!(matches!(
            scheduler.request(ms(tick)),
            RedrawSlot::Deferred(_)
        ));
    }
    assert_eq!(scheduler.last_redraw(), Some(ms(0)));
    assert_eq!(scheduler.request(ms(250)), RedrawSlot::Ready);
}

#[test]
fn no_more_than_four_redraws_in_any_second() {
    let mut scheduler = RedrawScheduler::new();
    let mut ready_at: Vec<Duration> = Vec::new();

    // Ten requests per 100 ms for ten seconds: a much faster stream than the
    // display can consume.
    for tick in 0..1_000u64 {
        let now = ms(tick * 10);
        if scheduler.request(now) == RedrawSlot::Ready {
            ready_at.push(now);
        }
    }

    assert!(!ready_at.is_empty());
    for (index, start) in ready_at.iter().enumerate() {
        let in_window = ready_at[index..]
            .iter()
            .take_while(|later| **later < *start + Duration::from_secs(1))
            .count();
        assert!(
            in_window <= MAX_REDRAWS_PER_SECOND as usize,
            "{} redraws inside one second starting at {:?}",
            in_window,
            start
        );
    }
    assert!(
        ready_at.len() <= 40,
        "the limiter must throttle, got {}",
        ready_at.len()
    );
}

#[test]
fn reset_allows_an_immediate_redraw() {
    let mut scheduler = RedrawScheduler::new();
    assert_eq!(scheduler.request(ms(0)), RedrawSlot::Ready);
    assert!(matches!(scheduler.request(ms(0)), RedrawSlot::Deferred(_)));
    scheduler.reset();
    assert_eq!(scheduler.last_redraw(), None);
    assert_eq!(scheduler.request(ms(1)), RedrawSlot::Ready);
}
