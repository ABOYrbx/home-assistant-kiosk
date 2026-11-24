use crate::MainWindow;
use crate::ui::bridge::UiBridge;
use crate::ui::redraw::{RedrawScheduler, RedrawSlot};
use crate::{AppEvent, AppState, Config, HaCommand};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Cadence of the clock timer. The clock is refreshed once per second and is
/// deliberately independent of the state redraw limiter.
pub const CLOCK_INTERVAL: Duration = Duration::from_secs(1);

/// UI-thread state shared with the deferred-redraw timer.
///
/// `AppState` still lives on the Slint thread and is never wrapped in a mutex;
/// the `Rc<RefCell<_>>` only lets the timer callback publish the newest state.
struct UiState {
    state: AppState,
    scheduler: RedrawScheduler,
    /// A redraw is owed because the limiter deferred it.
    dirty: bool,
    /// A trailing redraw has already been scheduled.
    flush_pending: bool,
    redraws: u64,
    clock_ticks: u64,
}

impl UiState {
    fn new() -> Self {
        Self {
            state: AppState::default(),
            scheduler: RedrawScheduler::new(),
            dirty: false,
            flush_pending: false,
            redraws: 0,
            clock_ticks: 0,
        }
    }

    /// Publish the current state to Slint and ask for one frame.
    fn render(&mut self, window: &MainWindow) {
        UiBridge::apply(&self.state);
        self.redraws += 1;
        window.window().request_redraw();
    }
}

#[allow(dead_code)]
pub struct AppController {
    window: MainWindow,
    ui: Rc<RefCell<UiState>>,
    config: Config,
    commands: mpsc::Sender<HaCommand>,
    started: Instant,
    clock_timer: slint::Timer,
}

impl AppController {
    pub fn new(window: MainWindow, config: Config, commands: mpsc::Sender<HaCommand>) -> Self {
        let weak_next = window.as_weak();
        window.on_swipe_next(move || {
            if let Some(w) = weak_next.upgrade() {
                let p = w.get_current_page();
                let next = if p >= 3 { 0 } else { p + 1 };
                w.set_current_page(next);
            }
        });

        let weak_prev = window.as_weak();
        window.on_swipe_prev(move || {
            if let Some(w) = weak_prev.upgrade() {
                let p = w.get_current_page();
                let prev = if p == 0 { 3 } else { p - 1 };
                w.set_current_page(prev);
            }
        });

        let ui = Rc::new(RefCell::new(UiState::new()));
        let clock_timer = slint::Timer::default();

        // Separate 1 Hz clock timer: it does not travel through the network
        // scheduler and does not consume the state redraw budget. The clock
        // widget binds to its own properties; this timer guarantees one fresh
        // frame per second even while state updates are being coalesced.
        let clock_ui = Rc::clone(&ui);
        let clock_window = window.as_weak();
        clock_timer.start(slint::TimerMode::Repeated, CLOCK_INTERVAL, move || {
            let Some(window) = clock_window.upgrade() else {
                return;
            };
            clock_ui.borrow_mut().clock_ticks += 1;
            window.window().request_redraw();
        });

        Self {
            window,
            ui,
            config,
            commands,
            started: Instant::now(),
            clock_timer,
        }
    }

    /// Applies one background event. Call this only from the Slint UI thread;
    /// `AppController` owns the window and is not `Send`.
    pub fn apply(&mut self, event: AppEvent) {
        // Fold the event into the state immediately so nothing is lost while
        // the redraw is deferred; publish it to Slint at the capped cadence.
        {
            let mut ui = self.ui.borrow_mut();
            apply_event(&mut ui.state, event);
        }
        self.schedule_redraw();
    }

    /// Ask the limiter for a redraw slot and publish the state if one is free.
    /// Otherwise schedule a single trailing redraw.
    fn schedule_redraw(&mut self) {
        let now = self.started.elapsed();
        let delay = {
            let mut ui = self.ui.borrow_mut();
            match ui.scheduler.request(now) {
                RedrawSlot::Ready => {
                    ui.render(&self.window);
                    ui.dirty = false;
                    return;
                }
                RedrawSlot::Deferred(delay) => {
                    ui.dirty = true;
                    if ui.flush_pending {
                        return;
                    }
                    ui.flush_pending = true;
                    delay
                }
            }
        };
        schedule_flush(
            Rc::clone(&self.ui),
            self.window.as_weak(),
            self.started,
            delay,
        );
    }

    /// Number of state-driven redraws published so far (diagnostics and tests).
    pub fn redraw_count(&self) -> u64 {
        self.ui.borrow().redraws
    }

    /// Number of one-second clock ticks observed so far (diagnostics and tests).
    pub fn clock_tick_count(&self) -> u64 {
        self.ui.borrow().clock_ticks
    }

    /// Snapshot of the applied state. The controller owns the live state; this
    /// clone is for diagnostics and tests.
    pub fn state_snapshot(&self) -> AppState {
        self.ui.borrow().state.clone()
    }

    pub fn next_page(&mut self) {
        let p = self.window.get_current_page();
        let next = if p >= 3 { 0 } else { p + 1 };
        self.window.set_current_page(next);
    }

    pub fn previous_page(&mut self) {
        let p = self.window.get_current_page();
        let prev = if p == 0 { 3 } else { p - 1 };
        self.window.set_current_page(prev);
    }

    pub fn window(&self) -> &MainWindow {
        &self.window
    }
}

/// Publish the newest state once the limiter allows it.
///
/// The timer is single-shot: either it renders, or it re-arms itself for the
/// remainder of the interval. It holds a weak window so a closed UI cannot keep
/// the application alive.
fn schedule_flush(
    ui: Rc<RefCell<UiState>>,
    window: slint::Weak<MainWindow>,
    started: Instant,
    delay: Duration,
) {
    slint::Timer::single_shot(delay, move || {
        let Some(window) = window.upgrade() else {
            return;
        };
        let next_delay = {
            let mut ui = ui.borrow_mut();
            ui.flush_pending = false;
            if !ui.dirty {
                return;
            }
            match ui.scheduler.request(started.elapsed()) {
                RedrawSlot::Ready => {
                    ui.render(&window);
                    ui.dirty = false;
                    return;
                }
                RedrawSlot::Deferred(delay) => {
                    ui.flush_pending = true;
                    delay
                }
            }
        };
        schedule_flush(ui, window.as_weak(), started, next_delay);
    });
}

fn apply_event(state: &mut AppState, event: AppEvent) {
    match event {
        AppEvent::Ha(ha_event) => state.apply_ha_event(ha_event),
        AppEvent::Weather(r) => {
            if let Ok(v) = r {
                state.weather = Some(v);
            }
        }
        AppEvent::Media(m) => state.media = m,
        AppEvent::Departures(r) => {
            if let Ok(v) = r {
                state.departures = Some(v);
            }
        }
        AppEvent::Calendar(r) => {
            if let Ok(items) = r {
                state.calendar = items;
            }
        }
        AppEvent::AlbumArt { .. } | AppEvent::ImageFailed { .. } => {}
    }
}
