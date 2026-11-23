use crate::ui::hero::{HeroCommand, HeroLight};
use crate::ui::top_bar::{connection_view, format_clock, format_date};
use crate::{AppEvent, AppState, Config, HaCommand, HaEvent, KioskModel, MainWindow};
use chrono::Local;
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
    last_redraw: Instant,
    pending_redraw: bool,
    clock_timer: slint::Timer,
    hero_timer: slint::Timer,
    hero: Rc<RefCell<HeroLight>>,
}

impl AppController {
    pub fn new(window: MainWindow, config: Config, commands: mpsc::Sender<HaCommand>) -> Self {
        let hero = Rc::new(RefCell::new(HeroLight::new(&config.widgets.hero_light)));
        let model = window.global::<KioskModel>();
        model.set_brand_title(
            if config.pages.overview.title.is_empty() {
                "Home".to_string()
            } else {
                config.pages.overview.title.clone()
            }
            .into(),
        );

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

        install_hero_callbacks(&window, &hero, &commands);

        let mut ctrl = Self {
            window,
            state: AppState::default(),
            config,
            commands,
            last_redraw: Instant::now(),
            pending_redraw: false,
            clock_timer: slint::Timer::default(),
            hero_timer: slint::Timer::default(),
            hero,
        };

        ctrl.sync_hero();
        ctrl.sync_top_bar();
        ctrl.start_clock();
        ctrl.start_hero_tick();
        ctrl
    }

    pub fn apply(&mut self, event: AppEvent) {
        match event {
            AppEvent::Ha(ha_event) => {
                match &ha_event {
                    HaEvent::ServiceResult { request_id, result } => {
                        let mut hero = self.hero.borrow_mut();
                        hero.service_finished(*request_id);
                        match result {
                            Ok(_) => hero.set_error(None),
                            Err(error) => hero.set_error(Some(error.to_string())),
                        }
                    }
                    HaEvent::Disconnected { .. } | HaEvent::AuthFailed { .. } => {
                        self.hero.borrow_mut().clear_pending();
                    }
                    _ => {}
                }
                self.state.apply_ha_event(ha_event);
                self.refresh_hero();
                self.sync_top_bar();
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

    fn refresh_hero(&self) {
        let entity_id = self.hero.borrow().entity_id().to_string();
        let entity = self.state.entities.get(&entity_id).cloned();
        self.hero.borrow_mut().apply_entity(entity.as_ref());
        self.sync_hero();
    }

    fn sync_hero(&self) {
        sync_hero_model(&self.window, &self.hero.borrow());
    }

    fn sync_top_bar(&self) {
        let model = self.window.global::<KioskModel>();
        let view = connection_view(&self.state.connection, self.state.stale);
        model.set_connection_label(view.label.into());
        model.set_connection_connected(view.connected);
        model.set_connection_warning(view.warning);
        model.set_connection_error(view.error);
    }

    fn start_clock(&mut self) {
        let weak = self.window.as_weak();
        let use_24_hour = self.config.widgets.clock.use_24_hour;
        let show_seconds = self.config.widgets.clock.show_seconds;
        update_clock(&self.window, use_24_hour, show_seconds);
        self.clock_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                if let Some(window) = weak.upgrade() {
                    update_clock(&window, use_24_hour, show_seconds);
                }
            },
        );
    }

    fn start_hero_tick(&mut self) {
        let weak = self.window.as_weak();
        let hero = self.hero.clone();
        let commands = self.commands.clone();
        self.hero_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(50),
            move || {
                if let Some(window) = weak.upgrade() {
                    let mut hero = hero.borrow_mut();
                    for command in hero.tick(Instant::now()) {
                        if !send_command(&commands, &command) {
                            hero.service_finished(command.call.request_id);
                            hero.set_error(Some("Not connected to Home Assistant".into()));
                        }
                    }
                    sync_hero_model(&window, &hero);
                }
            },
        );
    }
}

fn install_hero_callbacks(
    window: &MainWindow,
    hero: &Rc<RefCell<HeroLight>>,
    commands: &mpsc::Sender<HaCommand>,
) {
    let model = window.global::<KioskModel>();

    let weak = window.as_weak();
    let hero_toggle = hero.clone();
    let commands_toggle = commands.clone();
    model.on_hero_toggled(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut hero = hero_toggle.borrow_mut();
        if let Some(command) = hero.on_toggle()
            && !send_command(&commands_toggle, &command)
        {
            hero.service_finished(command.call.request_id);
            hero.set_error(Some("Not connected to Home Assistant".into()));
        }
        sync_hero_model(&window, &hero);
    });

    let weak = window.as_weak();
    let hero_brightness = hero.clone();
    let commands_brightness = commands.clone();
    model.on_hero_brightness_changed(move |value| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut hero = hero_brightness.borrow_mut();
        if let Some(command) = hero.on_brightness_change(value, Instant::now())
            && !send_command(&commands_brightness, &command)
        {
            hero.service_finished(command.call.request_id);
            hero.set_error(Some("Not connected to Home Assistant".into()));
        }
        sync_hero_model(&window, &hero);
    });

    let weak = window.as_weak();
    let hero_brightness_release = hero.clone();
    let commands_brightness_release = commands.clone();
    model.on_hero_brightness_released(move |value| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut hero = hero_brightness_release.borrow_mut();
        if let Some(command) = hero.on_brightness_release(value, Instant::now())
            && !send_command(&commands_brightness_release, &command)
        {
            hero.service_finished(command.call.request_id);
            hero.set_error(Some("Not connected to Home Assistant".into()));
        }
        sync_hero_model(&window, &hero);
    });

    let weak = window.as_weak();
    let hero_color_temp = hero.clone();
    let commands_color_temp = commands.clone();
    model.on_hero_color_temp_changed(move |value| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut hero = hero_color_temp.borrow_mut();
        if let Some(command) = hero.on_color_temp_change(value, Instant::now())
            && !send_command(&commands_color_temp, &command)
        {
            hero.service_finished(command.call.request_id);
            hero.set_error(Some("Not connected to Home Assistant".into()));
        }
        sync_hero_model(&window, &hero);
    });

    let weak = window.as_weak();
    let hero_color_temp_release = hero.clone();
    let commands_color_temp_release = commands.clone();
    model.on_hero_color_temp_released(move |value| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut hero = hero_color_temp_release.borrow_mut();
        if let Some(command) = hero.on_color_temp_release(value, Instant::now())
            && !send_command(&commands_color_temp_release, &command)
        {
            hero.service_finished(command.call.request_id);
            hero.set_error(Some("Not connected to Home Assistant".into()));
        }
        sync_hero_model(&window, &hero);
    });
}

fn send_command(commands: &mpsc::Sender<HaCommand>, command: &HeroCommand) -> bool {
    match commands.try_send(HaCommand::CallService(command.call.clone())) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!("failed to queue light service call: {error}");
            false
        }
    }
}

fn sync_hero_model(window: &MainWindow, hero: &HeroLight) {
    let model = window.global::<KioskModel>();
    let view = hero.view_model();
    model.set_hero_configured(view.configured);
    model.set_hero_name(view.name.into());
    model.set_hero_on(view.on);
    model.set_hero_brightness(view.brightness);
    model.set_hero_color_temp(view.color_temp);
    model.set_hero_min_kelvin(view.min_kelvin);
    model.set_hero_max_kelvin(view.max_kelvin);
    model.set_hero_available(view.available);
    model.set_hero_loading(view.loading);
    model.set_hero_pending(view.pending);
    model.set_hero_brightness_supported(view.brightness_supported);
    model.set_hero_color_temp_supported(view.color_temp_supported);
    model.set_hero_brightness_text(view.brightness_text.into());
    model.set_hero_color_temp_text(view.color_temp_text.into());
    model.set_hero_state_text(view.state_text.into());
    model.set_hero_status_text(view.status_text.into());
    model.set_hero_error(view.error);
}

fn update_clock(window: &MainWindow, use_24_hour: bool, show_seconds: bool) {
    let now = Local::now().naive_local();
    let model = window.global::<KioskModel>();
    model.set_clock_text(format_clock(&now, use_24_hour, show_seconds).into());
    model.set_date_text(format_date(&now).into());
}
