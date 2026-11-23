use crate::Config;
use crate::MainWindow;
use crate::events::{AppEvent, HaCommand, HaEvent};
use crate::sources::calendar::{CalendarRefresh, refresh_due};
use crate::state::{AppState, CalendarError};
use crate::ui::bridge::UiBridge;
use chrono::{DateTime, Utc};
use slint::ComponentHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct AppController {
    window: MainWindow,
    state: AppState,
    config: Config,
    commands: mpsc::Sender<HaCommand>,
    last_redraw: Instant,
    pending_redraw: bool,
    clock_timer: slint::Timer,
    calendar: CalendarRefresh,
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

        let state = AppState::default();
        UiBridge::apply(&state, &window);

        let ctrl = Self {
            window,
            state,
            config,
            commands,
            last_redraw: Instant::now(),
            pending_redraw: false,
            clock_timer: slint::Timer::default(),
            calendar: CalendarRefresh::default(),
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
        match event {
            AppEvent::Ha(ha_event) => {
                if let HaEvent::ServiceResult { request_id, result } = &ha_event {
                    let result = result.as_ref().map_err(ToString::to_string);
                    if self
                        .calendar
                        .accept(*request_id, result, self.state.time_zone)
                        && let Some(view) = self.calendar.finish(Utc::now())
                    {
                        self.state.calendar = Some(view);
                    }
                }
                if let HaEvent::Disconnected { reason } = &ha_event
                    && let Some(errors) = self.calendar.abandon(&format!("disconnected: {reason}"))
                {
                    let mut view = self.state.calendar.clone().unwrap_or_default();
                    view.errors = errors;
                    view.stale = true;
                    self.state.calendar = Some(view);
                }
                self.state.apply_ha_event(ha_event);
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
            AppEvent::Calendar(r) => match r {
                Ok(view) => self.state.calendar = Some(view),
                Err(error) => {
                    let mut view = self.state.calendar.clone().unwrap_or_default();
                    view.errors.push(CalendarError {
                        calendar_id: String::new(),
                        message: error.0,
                    });
                    view.stale = true;
                    self.state.calendar = Some(view);
                }
            },
            AppEvent::AlbumArt { .. } | AppEvent::ImageFailed { .. } => {}
        }

        UiBridge::apply(&self.state, &self.window);

        // Coalesce redraws: at most 4 per second.
        let now = Instant::now();
        if now.duration_since(self.last_redraw) >= Duration::from_millis(250) {
            self.last_redraw = now;
            self.pending_redraw = false;
        } else {
            self.pending_redraw = true;
        }
    }

    /// Send one `calendar.get_events` request per configured calendar.
    ///
    /// Returns false when calendars are disabled, none are configured, or a
    /// refresh is already in flight.
    pub fn refresh_calendar(&mut self, now: DateTime<Utc>) -> bool {
        let calendar = &self.config.widgets.calendar;
        if !calendar.enabled || calendar.entities.is_empty() || self.calendar.is_pending() {
            return false;
        }
        let calls =
            self.calendar
                .begin(&calendar.entities, calendar.days, self.state.time_zone, now);
        if calls.is_empty() {
            self.calendar = CalendarRefresh::default();
            return false;
        }
        for call in calls {
            if self
                .commands
                .try_send(HaCommand::CallService(call))
                .is_err()
            {
                self.calendar.abandon("command channel full");
                return false;
            }
        }
        true
    }

    pub fn calendar_refresh_due(&self, now: DateTime<Utc>) -> bool {
        let calendar = &self.config.widgets.calendar;
        if !calendar.enabled || calendar.entities.is_empty() || self.calendar.is_pending() {
            return false;
        }
        let last_success = self
            .state
            .calendar
            .as_ref()
            .and_then(|view| view.last_success);
        refresh_due(
            last_success,
            now,
            Duration::from_secs(calendar.refresh_seconds),
        )
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
