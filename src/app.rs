use crate::MainWindow;
use crate::sources::departures::{DeparturesRuntime, MIN_REFRESH_SECONDS, TickOutcome};
use crate::ui::bridge::UiBridge;
use crate::{AppEvent, AppState, Config, HaCommand, HaEvent};
use chrono::Utc;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct AppController {
    window: MainWindow,
    state: AppState,
    config: Config,
    commands: mpsc::Sender<HaCommand>,
    departures: Rc<RefCell<DeparturesRuntime>>,
    last_redraw: Instant,
    pending_redraw: bool,
    clock_timer: slint::Timer,
    departures_timer: slint::Timer,
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

        let departures = Rc::new(RefCell::new(DeparturesRuntime::new(
            config.widgets.departures.clone(),
        )));

        let ctrl = Self {
            window,
            state: AppState::default(),
            config,
            commands,
            departures,
            last_redraw: Instant::now(),
            pending_redraw: false,
            clock_timer: slint::Timer::default(),
            departures_timer: slint::Timer::default(),
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

        ctrl.start_departures_timer();
        ctrl.window
            .set_departures_visible(ctrl.config.widgets.departures.enabled);
        ctrl.window
            .set_departures_title(ctrl.config.widgets.departures.title.as_str().into());
        ctrl
    }

    fn start_departures_timer(&self) {
        let interval = Duration::from_secs(
            self.config
                .widgets
                .departures
                .refresh_seconds
                .max(MIN_REFRESH_SECONDS),
        );
        let weak_window = self.window.as_weak();
        let runtime = self.departures.clone();
        let commands = self.commands.clone();
        let title = self.config.widgets.departures.title.clone();
        self.departures_timer
            .start(slint::TimerMode::Repeated, interval, move || {
                let outcome = runtime.borrow_mut().on_tick(Instant::now(), Utc::now());
                let TickOutcome::Refreshed(changed) = outcome else {
                    return;
                };
                // Safety refresh: ask for a fresh snapshot on top of the
                // subscription, so a missed update cannot freeze the list.
                let _ = commands.try_send(HaCommand::RefreshSnapshot);
                if let Some(view) = changed
                    && let Some(window) = weak_window.upgrade()
                {
                    UiBridge::apply_departures(&window, Some(&view), &title);
                }
            });
    }

    pub fn apply(&mut self, event: AppEvent) {
        // Update state on the UI thread.
        let mut departures_dirty = false;
        match event {
            AppEvent::Ha(ha_event) => {
                if matches!(ha_event, HaEvent::Connected { .. }) {
                    self.departures.borrow_mut().on_reconnect();
                }
                self.state.apply_ha_event(ha_event);
                departures_dirty = true;
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
                    departures_dirty = true;
                }
            }
            AppEvent::Calendar(r) => {
                if let Ok(v) = r {
                    self.state.calendar = v;
                }
            }
            AppEvent::AlbumArt { .. } | AppEvent::ImageFailed { .. } => {}
        }

        // Re-read the departures sensor whenever entity state or attributes
        // change; the runtime suppresses unchanged views.
        if self.config.widgets.departures.enabled {
            let view = self.departures.borrow_mut().on_entities(
                &self.state.entities,
                self.state.time_zone,
                Utc::now(),
            );
            if let Some(view) = view {
                self.state.departures = Some(view);
                departures_dirty = true;
            }
        }

        if departures_dirty {
            UiBridge::apply(
                &self.state,
                &self.window,
                &self.config.widgets.departures.title,
                self.config.widgets.departures.enabled,
            );
        }

        // Coalesce redraws: at most 4 per second.
        let now = Instant::now();
        if now.duration_since(self.last_redraw) >= Duration::from_millis(250) {
            self.last_redraw = now;
            self.pending_redraw = false;
        } else {
            self.pending_redraw = true;
        }
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
