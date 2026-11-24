//! Coalescing and rate limiting for state-driven redraws.
//!
//! Home Assistant can deliver bursts of `state_changed` events. Publishing each
//! one to Slint separately wakes the software renderer far more often than a
//! Pi Zero 2 W can usefully draw, so `AppController` folds a burst into
//! `AppState` and asks this scheduler when the next frame may be published.
//!
//! At most [`MAX_REDRAWS_PER_SECOND`] state-driven frames are allowed inside a
//! sliding one-second window. Deferred requests are not dropped: the caller
//! re-schedules a single trailing redraw, which renders the newest state once
//! the interval has passed. The one-second clock timer is started separately
//! and never goes through this scheduler, so a clock tick neither consumes the
//! state budget nor waits for a deferred state update.

use std::time::Duration;

/// Maximum number of state-driven redraws in a sliding one-second window.
pub const MAX_REDRAWS_PER_SECOND: u32 = 4;

/// Minimum spacing between two state-driven redraws.
///
/// Derived from [`MAX_REDRAWS_PER_SECOND`]: four frames per second means one
/// frame every 250 ms.
pub const MIN_REDRAW_INTERVAL: Duration =
    Duration::from_millis(1000 / MAX_REDRAWS_PER_SECOND as u64);

/// Outcome of asking the scheduler for a redraw slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawSlot {
    /// A redraw may run now; the slot has been recorded.
    Ready,
    /// The cap is reached. Retry after the given delay; the request has not been
    /// recorded, so retrying cannot consume budget twice.
    Deferred(Duration),
}

/// Sliding-window limiter for state-driven redraws.
///
/// The scheduler is time-source agnostic: callers pass a monotonic timestamp.
/// Keeping it free of wall-clock and Slint types makes the rate limit testable
/// without a display.
#[derive(Debug, Clone)]
pub struct RedrawScheduler {
    min_interval: Duration,
    last_redraw: Option<Duration>,
}

impl Default for RedrawScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl RedrawScheduler {
    /// Create a scheduler capped at [`MAX_REDRAWS_PER_SECOND`].
    pub fn new() -> Self {
        Self::with_limit(MAX_REDRAWS_PER_SECOND)
    }

    /// Create a scheduler capped at `max_per_second` redraws.
    ///
    /// # Panics
    ///
    /// Panics if `max_per_second` is zero.
    pub fn with_limit(max_per_second: u32) -> Self {
        assert!(
            max_per_second > 0,
            "at least one redraw per second is required"
        );
        Self {
            min_interval: Duration::from_millis(1000 / u64::from(max_per_second)),
            last_redraw: None,
        }
    }

    /// Minimum spacing between two redraws enforced by this scheduler.
    pub fn min_interval(&self) -> Duration {
        self.min_interval
    }

    /// Ask for permission to redraw at `now`.
    ///
    /// `now` is a monotonic timestamp, for example the time elapsed since the
    /// UI started. The first request always succeeds.
    pub fn request(&mut self, now: Duration) -> RedrawSlot {
        match self.last_redraw {
            Some(last) => {
                let elapsed = now.saturating_sub(last);
                if elapsed >= self.min_interval {
                    self.last_redraw = Some(now);
                    RedrawSlot::Ready
                } else {
                    RedrawSlot::Deferred(self.min_interval - elapsed)
                }
            }
            None => {
                self.last_redraw = Some(now);
                RedrawSlot::Ready
            }
        }
    }

    /// Timestamp of the last granted redraw, if any.
    pub fn last_redraw(&self) -> Option<Duration> {
        self.last_redraw
    }

    /// Forget the recorded redraw, for example after the view was rebuilt.
    pub fn reset(&mut self) {
        self.last_redraw = None;
    }
}
