use crate::MainWindow;
use crate::events::{AppEvent, HaCommand, HaEvent, PendingAction};
use crate::sources::tiles::TileBoard;
use crate::ui::bridge::UiBridge;
use crate::{AppState, Config};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// UI-thread state shared with Slint callbacks. Everything here is touched only
/// from the UI thread; no lock is involved.
struct ControllerCore {
    state: AppState,
    tiles: TileBoard,
    commands: mpsc::Sender<HaCommand>,
    last_redraw: Instant,
}

impl ControllerCore {
    fn new(config: &Config, commands: mpsc::Sender<HaCommand>) -> Self {
        Self {
            state: AppState::default(),
            tiles: TileBoard::new(&config.tiles),
            commands,
            last_redraw: Instant::now(),
        }
    }

    /// Returns true when the caller may redraw now. Coalesces to at most four
    /// redraws per second.
    fn apply(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::Ha(ha_event) => {
                if let HaEvent::ServiceResult { request_id, result } = &ha_event {
                    self.tiles.on_result(*request_id, result);
                }
                if matches!(
                    &ha_event,
                    HaEvent::Disconnected { .. } | HaEvent::AuthFailed { .. }
                ) {
                    self.tiles.reset_pending();
                    self.state.pending.clear();
                }
                self.state.apply_ha_event(ha_event);
                self.tiles.refresh(&self.state);
            }
            AppEvent::Weather(r) => {
                if let Ok(v) = r {
                    self.state.weather = Some(v);
                }
            }
            AppEvent::Media(m) => {
                self.state.media = m;
            }
            AppEvent::Departures(r) => {
                if let Ok(v) = r {
                    self.state.departures = Some(v);
                }
            }
            AppEvent::Calendar(r) => {
                if let Ok(v) = r {
                    self.state.calendar = v;
                }
            }
            AppEvent::AlbumArt { .. } | AppEvent::ImageFailed { .. } => {}
        }

        let now = Instant::now();
        if now.duration_since(self.last_redraw) >= Duration::from_millis(250) {
            self.last_redraw = now;
            true
        } else {
            false
        }
    }

    /// Send one service call for a tapped tile. Returns false when the tile is
    /// read-only, already pending, or the command channel is closed.
    fn tap_tile(&mut self, entity_id: &str) -> bool {
        let request_id = crate::ha::client::next_request_id();
        let Some(call) = self.tiles.tap(entity_id, &self.state, request_id) else {
            return false;
        };
        self.state.pending.insert(
            request_id,
            PendingAction::ToggleTile {
                entity_id: entity_id.to_string(),
            },
        );
        if self
            .commands
            .try_send(HaCommand::CallService(call))
            .is_err()
        {
            self.state.pending.remove(&request_id);
            self.tiles.set_error(entity_id, "not connected");
            return false;
        }
        true
    }
}

pub struct AppController {
    window: MainWindow,
    core: Rc<RefCell<ControllerCore>>,
    clock_timer: slint::Timer,
    redraw_timer: slint::Timer,
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

        let core = Rc::new(RefCell::new(ControllerCore::new(&config, commands)));

        let tap_core = Rc::downgrade(&core);
        let tap_window = window.as_weak();
        window.on_tile_tapped(move |entity_id| {
            if let Some(core) = tap_core.upgrade() {
                let mut core = core.borrow_mut();
                core.tap_tile(&entity_id);
                if let Some(w) = tap_window.upgrade() {
                    UiBridge::apply_tiles(&w, &core.tiles);
                }
            }
        });

        UiBridge::apply_tiles(&window, &core.borrow().tiles);

        let ctrl = Self {
            window,
            core,
            clock_timer: slint::Timer::default(),
            redraw_timer: slint::Timer::default(),
        };

        // Separate 1 Hz clock timer (does not go through the network scheduler).
        let weak_clock = ctrl.window.as_weak();
        ctrl.clock_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                // Clock widget updates are handled by the UI binding; this timer
                // keeps the one-second cadence available for future widgets.
                let _ = &weak_clock;
            },
        );

        ctrl
    }

    pub fn apply(&mut self, event: AppEvent) {
        let redraw_now = self.core.borrow_mut().apply(event);
        if redraw_now {
            self.sync_ui();
        } else {
            // Trailing redraw so the last event of a burst is not lost.
            let core = Rc::downgrade(&self.core);
            let window = self.window.as_weak();
            self.redraw_timer.start(
                slint::TimerMode::SingleShot,
                Duration::from_millis(250),
                move || {
                    if let (Some(core), Some(w)) = (core.upgrade(), window.upgrade()) {
                        UiBridge::apply_tiles(&w, &core.borrow().tiles);
                    }
                },
            );
        }
    }

    /// Handle a tile tap from outside the Slint callback (tests, future input).
    pub fn tap_tile(&mut self, entity_id: &str) -> bool {
        let tapped = self.core.borrow_mut().tap_tile(entity_id);
        self.sync_ui();
        tapped
    }

    pub fn state(&self) -> std::cell::Ref<'_, AppState> {
        std::cell::Ref::map(self.core.borrow(), |core| &core.state)
    }

    fn sync_ui(&self) {
        let core = self.core.borrow();
        UiBridge::apply_tiles(&self.window, &core.tiles);
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
