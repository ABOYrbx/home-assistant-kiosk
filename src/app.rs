use crate::ui::bridge::UiBridge;
use crate::{AppEvent, AppState, HaCommand, MainWindow};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// The UI is redrawn at most four times per second. Events arriving between
/// ticks are coalesced into one state update and one bridge pass.
const REDRAW_INTERVAL: Duration = Duration::from_millis(250);
const CLOCK_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_CHANNEL_CAPACITY: usize = 64;

/// Caps UI updates at a fixed cadence while still allowing the first event to
/// show up immediately. Kept independent of Slint so it can be unit tested.
pub(crate) struct RedrawLimiter {
    interval: Duration,
    last_flush: Option<Instant>,
    dirty: bool,
}

impl RedrawLimiter {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_flush: None,
            dirty: false,
        }
    }

    /// Record that the UI is stale. Returns true when the caller should redraw
    /// right now; otherwise the pending change is flushed by `flush_if_due`.
    pub(crate) fn mark_dirty(&mut self, now: Instant) -> bool {
        self.dirty = true;
        self.flush_if_due(now)
    }

    /// Returns true at most once per interval while changes are pending.
    pub(crate) fn flush_if_due(&mut self, now: Instant) -> bool {
        if !self.dirty {
            return false;
        }
        let due = self
            .last_flush
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if due {
            self.last_flush = Some(now);
            self.dirty = false;
            true
        } else {
            false
        }
    }
}

struct UiState {
    state: AppState,
    limiter: RedrawLimiter,
}

/// Owns the Slint window and applies `AppEvent` values on the UI thread.
///
/// Events may be queued through `event_sender` from any thread; the controller
/// drains that queue on a 250 ms UI timer. `apply` is for callers that are
/// already on the UI thread. Both paths share the same 4 Hz redraw limiter.
pub struct AppController {
    window: MainWindow,
    commands: mpsc::Sender<HaCommand>,
    events_tx: mpsc::Sender<AppEvent>,
    ui: Rc<RefCell<UiState>>,
    clock_timer: slint::Timer,
    poll_timer: slint::Timer,
}

impl AppController {
    pub fn new(window: MainWindow, commands: mpsc::Sender<HaCommand>) -> Self {
        let (events_tx, events_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        Self::new_with_events(window, commands, events_tx, events_rx)
    }

    fn new_with_events(
        window: MainWindow,
        commands: mpsc::Sender<HaCommand>,
        events_tx: mpsc::Sender<AppEvent>,
        events_rx: mpsc::Receiver<AppEvent>,
    ) -> Self {
        let ui = Rc::new(RefCell::new(UiState {
            state: AppState::default(),
            limiter: RedrawLimiter::new(REDRAW_INTERVAL),
        }));

        update_clock(&window);

        let clock_timer = slint::Timer::default();
        let weak_clock = window.as_weak();
        clock_timer.start(slint::TimerMode::Repeated, CLOCK_INTERVAL, move || {
            if let Some(window) = weak_clock.upgrade() {
                update_clock(&window);
            }
        });

        let poll_timer = slint::Timer::default();
        let weak_poll = window.as_weak();
        let poll_ui = ui.clone();
        let events_rx = Rc::new(RefCell::new(events_rx));
        poll_timer.start(slint::TimerMode::Repeated, REDRAW_INTERVAL, move || {
            let now = Instant::now();
            let mut state_changed = false;
            {
                let mut rx = events_rx.borrow_mut();
                while let Ok(event) = rx.try_recv() {
                    apply_event(&mut poll_ui.borrow_mut().state, event);
                    state_changed = true;
                }
            }

            let mut ui = poll_ui.borrow_mut();
            let should_flush = if state_changed {
                ui.limiter.mark_dirty(now)
            } else {
                ui.limiter.flush_if_due(now)
            };
            if should_flush && let Some(window) = weak_poll.upgrade() {
                UiBridge::apply(&window, &ui.state);
            }
        });

        Self {
            window,
            commands,
            events_tx,
            ui,
            clock_timer,
            poll_timer,
        }
    }

    /// Apply an event that has already been delivered on the UI thread.
    pub fn apply(&mut self, event: AppEvent) {
        let mut ui = self.ui.borrow_mut();
        apply_event(&mut ui.state, event);
        if ui.limiter.mark_dirty(Instant::now()) {
            UiBridge::apply(&self.window, &ui.state);
        }
    }

    /// A handle that may be cloned and used from network or worker threads.
    pub fn event_sender(&self) -> mpsc::Sender<AppEvent> {
        self.events_tx.clone()
    }

    pub fn next_page(&mut self) {
        let p = self.window.get_current_page();
        self.window
            .set_current_page(if p >= self.window.get_page_count() - 1 {
                0
            } else {
                p + 1
            });
    }

    pub fn previous_page(&mut self) {
        let p = self.window.get_current_page();
        self.window.set_current_page(if p == 0 {
            self.window.get_page_count() - 1
        } else {
            p - 1
        });
    }

    pub fn window(&self) -> &MainWindow {
        &self.window
    }

    pub fn state(&self) -> std::cell::Ref<'_, AppState> {
        std::cell::Ref::map(self.ui.borrow(), |ui| &ui.state)
    }

    pub fn command_sender(&self) -> mpsc::Sender<HaCommand> {
        self.commands.clone()
    }
}

fn update_clock(window: &MainWindow) {
    let now = chrono::Local::now();
    window.set_clock_text(now.format("%H:%M").to_string().into());
    window.set_date_text(now.format("%a %d %b").to_string().into());
}

fn apply_event(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::Ha(ha_event) => state.apply_ha_event(ha_event),
        AppEvent::Weather(result) => {
            if let Ok(view) = result {
                state.weather = Some(view);
            }
        }
        AppEvent::Media(media) => state.media = media,
        AppEvent::Departures(result) => {
            if let Ok(view) = result {
                state.departures = Some(view);
            }
        }
        AppEvent::Calendar(result) => {
            if let Ok(items) = result {
                state.calendar = items;
            }
        }
        AppEvent::AlbumArt { .. } | AppEvent::ImageFailed { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::RedrawLimiter;
    use std::time::{Duration, Instant};

    #[test]
    fn first_update_is_immediate() {
        let t0 = Instant::now();
        let mut limiter = RedrawLimiter::new(Duration::from_millis(250));
        assert!(limiter.mark_dirty(t0));
    }

    #[test]
    fn updates_are_coalesced_and_capped_at_four_per_second() {
        let t0 = Instant::now();
        let mut limiter = RedrawLimiter::new(Duration::from_millis(250));
        assert!(limiter.mark_dirty(t0));

        let mut flushes = 0;
        for ms in 1..1000 {
            if limiter.mark_dirty(t0 + Duration::from_millis(ms)) {
                flushes += 1;
            }
        }
        assert_eq!(
            flushes, 3,
            "1000 ms window after the initial redraw holds three more"
        );
        assert!(!limiter.flush_if_due(t0 + Duration::from_millis(999)));
        assert!(limiter.flush_if_due(t0 + Duration::from_millis(1000)));
        assert!(!limiter.mark_dirty(t0 + Duration::from_millis(1049)));
        assert!(!limiter.flush_if_due(t0 + Duration::from_millis(1249)));
        assert!(limiter.flush_if_due(t0 + Duration::from_millis(1250)));
    }

    #[test]
    fn idle_limiter_never_flushes() {
        let t0 = Instant::now();
        let mut limiter = RedrawLimiter::new(Duration::from_millis(250));
        assert!(!limiter.flush_if_due(t0 + Duration::from_secs(10)));
    }
}
