use crate::extensions::shop::{DEFAULT_SECRET_DIR, ShopAction, ShopView};
use crate::extensions::{
    EntitySnapshot, ExtensionDisplay, ExtensionManager, HttpsFetcher, ManagerOptions,
};
use crate::sources::calendar::{CalendarRefresh, refresh_due};
use crate::sources::departures::{DeparturesRuntime, TickOutcome};
use crate::sources::media::{self, VolumeDebouncer};
use crate::sources::tiles::TileBoard;
use crate::sources::weather::{WeatherRuntime, parse_forecast_response};
use crate::state::{AppState, CalendarView, MediaView};
use crate::ui::bridge;
use crate::ui::hero::{HeroCommand, HeroLight};
use crate::ui::images::{AlbumArtRequest, AlbumArtState};
use crate::ui::redraw::{RedrawScheduler, RedrawSlot};
use crate::ui::top_bar::{format_clock, format_date};
use crate::{
    AppEvent, Config, HaCommand, HaEvent, MainWindow, PendingAction, ServiceCall, ShopEvent,
};
use chrono::{DateTime, Utc};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

type SharedShop = Rc<RefCell<ShopView>>;
type SharedCore = Rc<RefCell<ControllerCore>>;

enum ShopTask {
    Refresh,
    Install(String),
    Update(String),
    Remove(String),
    SetEnabled(String, bool),
    RunExtensions(BTreeMap<String, EntitySnapshot>),
}

#[derive(Default)]
struct MediaUi {
    view: Option<MediaView>,
    art: AlbumArtState,
    pending_request: Option<u64>,
    requested_url: Option<String>,
    error: Option<String>,
    volume: VolumeDebouncer,
    volume_step: f32,
}

struct ControllerCore {
    state: AppState,
    config: Config,
    commands: mpsc::Sender<HaCommand>,
    tiles: TileBoard,
    hero: HeroLight,
    media: MediaUi,
    departures: DeparturesRuntime,
    calendar: CalendarRefresh,
    weather: WeatherRuntime,
    album_art_requests: Option<mpsc::Sender<AlbumArtRequest>>,
    last_calendar_attempt: Option<DateTime<Utc>>,
    scheduler: RedrawScheduler,
    started: Instant,
    redraw_count: u64,
    clock_tick_count: u64,
    pending_redraw: bool,
}

impl ControllerCore {
    fn new(config: Config, commands: mpsc::Sender<HaCommand>, state: AppState) -> Self {
        let tiles = TileBoard::new(&config.tiles);
        let hero = HeroLight::new(&config.widgets.hero_light);
        let departures = DeparturesRuntime::new(config.widgets.departures.clone());
        let weather = WeatherRuntime::new(config.widgets.weather.clone());
        let media = MediaUi {
            volume_step: config.widgets.media.volume_step.clamp(0.01, 1.0) as f32,
            ..Default::default()
        };
        Self {
            state,
            config,
            commands,
            tiles,
            hero,
            media,
            departures,
            calendar: CalendarRefresh::default(),
            weather,
            album_art_requests: None,
            last_calendar_attempt: None,
            scheduler: RedrawScheduler::new(),
            started: Instant::now(),
            redraw_count: 0,
            clock_tick_count: 0,
            pending_redraw: false,
        }
    }

    fn apply(&mut self, event: AppEvent) {
        match event {
            AppEvent::Ha(event) => self.apply_ha(event),
            AppEvent::Shop(_) => {}
            AppEvent::Weather(result) => match result {
                Ok(view) => self.state.weather = Some(view),
                Err(error) => {
                    tracing::warn!("weather source failed: {}", error.0);
                }
            },
            AppEvent::Media(view) => {
                self.media.view = view;
                self.media.pending_request = None;
                if self.media.view.is_none() {
                    self.media.art.clear();
                    self.media.requested_url = None;
                }
                self.state.media = self.media.view.clone();
                self.request_album_art();
            }
            AppEvent::Departures(result) => {
                if let Ok(view) = result {
                    self.state.departures = Some(view);
                }
            }
            AppEvent::Calendar(result) => match result {
                Ok(view) => self.state.calendar = Some(view),
                Err(error) => {
                    let mut view = self.state.calendar.clone().unwrap_or_default();
                    view.errors.push(crate::state::CalendarError {
                        calendar_id: String::new(),
                        message: error.0,
                    });
                    view.stale = true;
                    self.state.calendar = Some(view);
                }
            },
            AppEvent::AlbumArt {
                entity_id,
                url,
                image,
            } => {
                let expected = self
                    .media
                    .requested_url
                    .as_deref()
                    .or_else(|| self.media.art.current_url());
                if expected == Some(url.as_str())
                    && self
                        .media
                        .view
                        .as_ref()
                        .is_some_and(|view| view.entity_id == entity_id)
                {
                    self.media.art.apply_loaded(entity_id, url, image);
                    self.media.requested_url = None;
                }
            }
            AppEvent::ImageFailed {
                entity_id,
                url,
                reason,
            } => {
                if self.media.requested_url.as_deref() == Some(url.as_str())
                    && self
                        .media
                        .view
                        .as_ref()
                        .is_some_and(|view| view.entity_id == entity_id)
                {
                    self.media.art.apply_failed(reason);
                }
            }
        }
    }

    fn apply_ha(&mut self, event: HaEvent) {
        match event {
            HaEvent::ServiceResult { request_id, result } => {
                self.tiles.on_result(request_id, &result);
                if self.hero.service_finished(request_id)
                    && let Err(error) = &result
                {
                    self.hero.set_error(Some(error.to_string()));
                }
                if self.media.pending_request == Some(request_id) {
                    self.media.pending_request = None;
                    self.media.error = result.as_ref().err().map(ToString::to_string);
                }
                match &result {
                    Ok(response) => {
                        if let Ok(forecast) = parse_forecast_response(&response.data) {
                            self.weather
                                .finish_success(request_id, forecast, Instant::now());
                        } else {
                            self.weather.finish_failure(
                                request_id,
                                "invalid forecast response".into(),
                                Instant::now(),
                            );
                        }
                    }
                    Err(error) => {
                        self.weather
                            .finish_failure(request_id, error.to_string(), Instant::now());
                    }
                }
                let calendar_result = result.as_ref().map_err(ToString::to_string);
                self.state.weather = self.weather.view().cloned();
                if self
                    .calendar
                    .accept(request_id, calendar_result, self.state.time_zone)
                    && let Some(view) = self.calendar.finish(Utc::now())
                {
                    self.merge_calendar(view);
                }
                self.state
                    .apply_ha_event(HaEvent::ServiceResult { request_id, result });
            }
            HaEvent::Connected {
                ha_version,
                time_zone,
            } => {
                self.state.apply_ha_event(HaEvent::Connected {
                    ha_version,
                    time_zone,
                });
                self.refresh_widgets();
                self.departures.on_reconnect();
                self.departures.on_tick(Instant::now(), Utc::now());
                self.weather.request_immediate();
                self.start_weather_refresh();
                self.refresh_calendar(Utc::now());
                let _ = self.commands.try_send(HaCommand::RefreshSnapshot);
            }
            HaEvent::Disconnected { reason } => {
                self.tiles.reset_pending();
                self.hero.clear_pending();
                self.media.pending_request = None;
                self.state.pending.clear();
                if let Some(errors) = self.calendar.abandon(&format!("disconnected: {reason}")) {
                    let mut view = self.state.calendar.clone().unwrap_or_default();
                    view.errors = errors;
                    view.stale = true;
                    self.state.calendar = Some(view);
                }
                self.state.apply_ha_event(HaEvent::Disconnected { reason });
            }
            HaEvent::AuthFailed { message } => {
                self.tiles.reset_pending();
                self.hero.clear_pending();
                self.media.pending_request = None;
                self.state.pending.clear();
                self.state.apply_ha_event(HaEvent::AuthFailed { message });
            }
            event => {
                self.state.apply_ha_event(event);
                self.refresh_widgets();
            }
        }
    }

    fn refresh_widgets(&mut self) {
        let hero_entity = self.hero.entity_id().to_string();
        self.hero
            .apply_entity(self.state.entities.get(&hero_entity));
        self.tiles.refresh(&self.state);
        self.weather
            .apply_entity(self.state.entities.get(&self.config.widgets.weather.entity));
        self.state.weather = self.weather.view().cloned();
        if self.config.widgets.media.enabled
            && let Some(entity) = self.state.entities.get(&self.config.widgets.media.entity)
        {
            self.media.view = Some(media::parse_media_entity(entity));
        } else if self.config.widgets.media.enabled {
            self.media.view = None;
        }
        self.state.media = self.media.view.clone();
        if self.config.widgets.departures.enabled
            && let Some(view) =
                self.departures
                    .on_entities(&self.state.entities, self.state.time_zone, Utc::now())
        {
            self.state.departures = Some(view);
        }
        self.request_album_art();
    }

    fn request_album_art(&mut self) {
        let Some(sender) = &self.album_art_requests else {
            return;
        };
        let Some(view) = self.media.view.clone() else {
            return;
        };
        if !self.config.widgets.media.show_album_art {
            self.media.art.clear();
            self.media.requested_url = None;
            return;
        }
        let Some(picture) = view.entity_picture.as_deref() else {
            self.media.art.clear();
            self.media.requested_url = None;
            return;
        };
        let Some(url) = media::resolve_image_url(&self.config.ha.url, picture) else {
            return;
        };
        if self.media.art.current_url() == Some(url.as_str())
            || self.media.requested_url.as_deref() == Some(url.as_str())
        {
            return;
        }
        if sender
            .try_send(AlbumArtRequest {
                entity_id: view.entity_id,
                url: url.clone(),
            })
            .is_ok()
        {
            self.media.requested_url = Some(url.to_string());
        }
    }

    fn send_service(&mut self, call: ServiceCall, action: PendingAction) -> bool {
        let request_id = call.request_id;
        self.state.pending.insert(request_id, action);
        if self
            .commands
            .try_send(HaCommand::CallService(call))
            .is_err()
        {
            self.state.pending.remove(&request_id);
            return false;
        }
        true
    }

    fn tap_tile(&mut self, entity_id: &str) -> bool {
        let request_id = crate::ha::client::next_request_id();
        let Some(call) = self.tiles.tap(entity_id, &self.state, request_id) else {
            return false;
        };
        let sent = self.send_service(
            call,
            PendingAction::ToggleTile {
                entity_id: entity_id.to_string(),
            },
        );
        if !sent {
            self.tiles
                .set_error(entity_id, "Home Assistant is not connected");
        }
        sent
    }

    fn media_transport(
        &mut self,
        build: fn(u64, &str) -> ServiceCall,
        allowed: fn(&MediaView) -> bool,
    ) {
        if self.media.pending_request.is_some() {
            return;
        }
        let Some(view) = self.media.view.clone() else {
            return;
        };
        if !allowed(&view) {
            return;
        }
        let request_id = crate::ha::client::next_request_id();
        let call = build(request_id, &view.entity_id);
        let sent = self.send_service(
            call,
            PendingAction::MediaTransport {
                entity_id: view.entity_id.clone(),
            },
        );
        if sent {
            self.media.pending_request = Some(request_id);
            self.media.error = None;
        } else {
            self.media.error = Some("Home Assistant is not connected".into());
        }
    }

    fn media_volume(&mut self, value: f32, release: bool) {
        let Some(view) = self.media.view.clone() else {
            return;
        };
        if !media::can_set_volume(&view) {
            return;
        }
        let now = Instant::now();
        let value = if release {
            self.media.volume.on_release(value, now)
        } else {
            let Some(value) = self.media.volume.on_change(value, now) else {
                return;
            };
            value
        };
        let request_id = crate::ha::client::next_request_id();
        let call = media::volume_call(request_id, &view.entity_id, value);
        if !self.send_service(
            call,
            PendingAction::MediaVolume {
                entity_id: view.entity_id,
            },
        ) {
            self.media.error = Some("Home Assistant is not connected".into());
        }
    }

    fn hero_command(&mut self, command: HeroCommand) -> bool {
        self.send_service(command.call, command.action)
    }

    fn hero_toggle(&mut self) -> bool {
        match self.hero.on_toggle() {
            Some(command) => self.hero_command(command),
            None => false,
        }
    }

    fn hero_brightness_change(&mut self, value: i32) -> bool {
        match self.hero.on_brightness_change(value, Instant::now()) {
            Some(command) => self.hero_command(command),
            None => false,
        }
    }

    fn hero_brightness_release(&mut self, value: i32) -> bool {
        match self.hero.on_brightness_release(value, Instant::now()) {
            Some(command) => self.hero_command(command),
            None => false,
        }
    }

    fn hero_color_temp_change(&mut self, value: i32) -> bool {
        match self.hero.on_color_temp_change(value, Instant::now()) {
            Some(command) => self.hero_command(command),
            None => false,
        }
    }

    fn hero_color_temp_release(&mut self, value: i32) -> bool {
        match self.hero.on_color_temp_release(value, Instant::now()) {
            Some(command) => self.hero_command(command),
            None => false,
        }
    }

    fn start_weather_refresh(&mut self) {
        let request_id = crate::ha::client::next_request_id();
        if let Some(call) = self.weather.begin(Instant::now(), request_id) {
            let _ = self.commands.try_send(HaCommand::CallService(call));
        }
    }

    fn refresh_calendar(&mut self, now: DateTime<Utc>) -> bool {
        let calendar = &self.config.widgets.calendar;
        if !calendar.enabled || calendar.entities.is_empty() || self.calendar.is_pending() {
            return false;
        }
        let calls =
            self.calendar
                .begin(&calendar.entities, calendar.days, self.state.time_zone, now);
        if calls.is_empty() {
            return false;
        }
        self.last_calendar_attempt = Some(now);
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

    fn calendar_refresh_due(&self, now: DateTime<Utc>) -> bool {
        let calendar = &self.config.widgets.calendar;
        if !calendar.enabled || calendar.entities.is_empty() || self.calendar.is_pending() {
            return false;
        }
        refresh_due(
            self.last_calendar_attempt,
            now,
            Duration::from_secs(calendar.refresh_seconds),
        )
    }

    fn merge_calendar(&mut self, mut fresh: CalendarView) {
        if fresh.last_success.is_none()
            && let Some(previous) = self.state.calendar.as_ref()
        {
            fresh.items = previous.items.clone();
            fresh.stale = true;
        }
        self.state.calendar = Some(fresh);
    }

    fn tick(&mut self, now_instant: Instant, now: DateTime<Utc>) {
        for command in self.hero.tick(now_instant) {
            let _ = self.hero_command(command);
        }
        if self.config.widgets.departures.enabled
            && let TickOutcome::Refreshed(Some(view)) = self.departures.on_tick(now_instant, now)
        {
            self.state.departures = Some(view);
        }
        if self.weather.should_refresh(now_instant) {
            self.start_weather_refresh();
        }
        if self.calendar_refresh_due(now) {
            self.refresh_calendar(now);
        } else if self.calendar.is_pending()
            && self
                .last_calendar_attempt
                .is_some_and(|last| now.signed_duration_since(last).num_seconds() >= 30)
            && let Some(errors) = self.calendar.abandon("calendar refresh timed out")
        {
            let mut view = self.state.calendar.clone().unwrap_or_default();
            view.errors = errors;
            view.stale = true;
            self.state.calendar = Some(view);
        }
    }

    fn render(&self, window: &MainWindow) {
        bridge::apply_status(window, &self.state);
        bridge::apply_hero(window, &self.hero.view_model());
        bridge::apply_tiles(window, &self.tiles);
        window.set_weather_visible(
            self.config.widgets.weather.enabled && !self.config.widgets.weather.entity.is_empty(),
        );
        bridge::apply_weather(
            window,
            self.state.weather.as_ref(),
            self.weather.error(),
            self.state.stale,
        );
        window.set_departures_visible(
            self.config.widgets.departures.enabled
                && !self.config.widgets.departures.entity.is_empty(),
        );
        bridge::apply_departures(
            window,
            self.state.departures.as_ref(),
            &self.config.widgets.departures.title,
        );
        bridge::apply_media(
            window,
            self.media.view.as_ref(),
            &self.media.art,
            self.media.pending_request.is_some(),
            self.media.error.as_deref(),
            self.media.volume_step,
        );
        bridge::apply_calendar(
            window,
            self.state.calendar.as_ref(),
            Utc::now(),
            &chrono::Local,
        );
        let global = window.global::<crate::KioskModel>();
        global.set_brand_title(if self.config.pages.overview.title.is_empty() {
            "Home".into()
        } else {
            self.config.pages.overview.title.clone().into()
        });
    }
}

pub struct AppController {
    window: MainWindow,
    core: SharedCore,
    config: Config,
    shop: SharedShop,
    page_count: usize,
    clock_timer: slint::Timer,
    redraw_timer: slint::Timer,
    event_timer: slint::Timer,
    cache_timer: slint::Timer,
    extension_timer: slint::Timer,
    shop_tasks: Option<std_mpsc::Sender<ShopTask>>,
    shop_timer: Option<slint::Timer>,
}

impl AppController {
    pub fn new(window: MainWindow, config: Config, commands: mpsc::Sender<HaCommand>) -> Self {
        Self::with_state(window, config, commands, AppState::default())
    }

    pub fn with_state(
        window: MainWindow,
        config: Config,
        commands: mpsc::Sender<HaCommand>,
        state: AppState,
    ) -> Self {
        let page_count = if config.extensions.enabled { 5 } else { 4 };
        let core = Rc::new(RefCell::new(ControllerCore::new(
            config.clone(),
            commands,
            state,
        )));
        let config_path = config
            .source_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let shop = Rc::new(RefCell::new(ShopView::new(config_path, DEFAULT_SECRET_DIR)));

        window.set_extensions_enabled(config.extensions.enabled);
        window.set_page_count(page_count as i32);
        window.set_shop_config_path(shop.borrow().config_path.clone().into());
        window.set_shop_secret_dir(shop.borrow().secret_dir.clone().into());

        let (task_tx, task_rx) = std_mpsc::channel();
        let (event_tx, event_rx) = std_mpsc::channel();
        let shop_tasks = config.extensions.enabled.then_some(task_tx);

        let mut controller = Self {
            window,
            core,
            config,
            shop,
            page_count,
            clock_timer: slint::Timer::default(),
            redraw_timer: slint::Timer::default(),
            event_timer: slint::Timer::default(),
            cache_timer: slint::Timer::default(),
            extension_timer: slint::Timer::default(),
            shop_tasks,
            shop_timer: None,
        };
        controller.wire_navigation();
        controller.wire_widgets();
        controller.wire_shop_callbacks();
        controller.start_shop(event_rx, task_rx, event_tx);
        controller.start_extensions();
        controller.start_clock();
        controller.render();
        controller
    }

    fn wire_navigation(&self) {
        let weak = self.window.as_weak();
        let pages = self.page_count as i32;
        self.window.on_swipe_next(move || {
            if let Some(window) = weak.upgrade() {
                let next = (window.get_current_page() + 1) % pages;
                window.set_current_page(next);
            }
        });
        let weak = self.window.as_weak();
        self.window.on_swipe_prev(move || {
            if let Some(window) = weak.upgrade() {
                let previous = (window.get_current_page() + pages - 1) % pages;
                window.set_current_page(previous);
            }
        });
    }

    fn wire_widgets(&self) {
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_tile_tapped(move |entity_id| {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut().tap_tile(entity_id.as_str());
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_scene_activated(move |entity_id| {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut().tap_tile(entity_id.as_str());
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });

        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_media_play_pause(move || {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut()
                    .media_transport(media::play_pause_call, media::can_play_pause);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_media_previous_track(move || {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut()
                    .media_transport(media::previous_track_call, media::can_previous);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_media_next_track(move || {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut()
                    .media_transport(media::next_track_call, media::can_next);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_media_volume_changed(move |value| {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut().media_volume(value, false);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        self.window.on_media_volume_released(move |value| {
            if let Some(core) = weak_core.upgrade() {
                core.borrow_mut().media_volume(value, true);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });

        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        let global = self.window.global::<crate::KioskModel>();
        global.on_hero_toggled(move || {
            if let Some(core) = weak_core.upgrade() {
                let sent = core.borrow_mut().hero_toggle();
                if sent && let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });

        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        global.on_hero_brightness_changed(move |value| {
            if let Some(core) = weak_core.upgrade() {
                let _ = core.borrow_mut().hero_brightness_change(value);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        global.on_hero_brightness_released(move |value| {
            if let Some(core) = weak_core.upgrade() {
                let _ = core.borrow_mut().hero_brightness_release(value);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        global.on_hero_color_temp_changed(move |value| {
            if let Some(core) = weak_core.upgrade() {
                let _ = core.borrow_mut().hero_color_temp_change(value);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        global.on_hero_color_temp_released(move |value| {
            if let Some(core) = weak_core.upgrade() {
                let _ = core.borrow_mut().hero_color_temp_release(value);
                if let Some(window) = weak_window.upgrade() {
                    core.borrow().render(&window);
                }
            }
        });
    }

    fn start_clock(&mut self) {
        let weak_core = Rc::downgrade(&self.core);
        let weak_window = self.window.as_weak();
        let use_24_hour = self.config.widgets.clock.use_24_hour;
        let show_seconds = self.config.widgets.clock.show_seconds;
        self.clock_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                let (Some(core), Some(window)) = (weak_core.upgrade(), weak_window.upgrade())
                else {
                    return;
                };
                let mut core = core.borrow_mut();
                core.clock_tick_count += 1;
                let now = chrono::Local::now().naive_local();
                bridge::apply_clock(
                    &window,
                    format_clock(&now, use_24_hour, show_seconds),
                    format_date(&now),
                );
                core.tick(Instant::now(), Utc::now());
            },
        );
    }

    fn start_extensions(&mut self) {
        if !self.config.extensions.enabled {
            return;
        }
        let Some(tasks) = self.shop_tasks.clone() else {
            return;
        };
        let weak_core = Rc::downgrade(&self.core);
        let send = move || {
            let Some(core) = weak_core.upgrade() else {
                return;
            };
            let entities: BTreeMap<String, EntitySnapshot> = core
                .borrow()
                .state
                .entities
                .iter()
                .map(|(id, entity)| {
                    (
                        id.clone(),
                        EntitySnapshot {
                            state: entity.state.clone(),
                            attributes: entity.attributes.clone(),
                        },
                    )
                })
                .collect();
            let _ = tasks.send(ShopTask::RunExtensions(entities));
        };
        send();
        self.extension_timer
            .start(slint::TimerMode::Repeated, Duration::from_secs(60), send);
    }

    fn start_shop(
        &mut self,
        events: std_mpsc::Receiver<ShopEvent>,
        tasks: std_mpsc::Receiver<ShopTask>,
        tx: std_mpsc::Sender<ShopEvent>,
    ) {
        if !self.config.extensions.enabled {
            return;
        }
        let options = manager_options(&self.config);
        std::thread::Builder::new()
            .name("extension-shop".into())
            .spawn(move || run_shop_worker(options, tasks, tx))
            .expect("spawn extension worker");

        let weak_events = self.window.as_weak();
        let shop_events = self.shop.clone();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(200),
            move || {
                let mut changed = false;
                while let Ok(event) = events.try_recv() {
                    match event {
                        ShopEvent::ExtensionOutputs(outputs) => {
                            if let Some(window) = weak_events.upgrade() {
                                bridge::apply_extension_outputs(&window, &outputs);
                            }
                        }
                        event => {
                            apply_shop_event(&mut shop_events.borrow_mut(), event);
                            changed = true;
                        }
                    }
                }
                if changed && let Some(window) = weak_events.upgrade() {
                    push_shop(&window, &shop_events.borrow());
                }
            },
        );
        self.shop_timer = Some(timer);
        if let Some(tasks) = &self.shop_tasks {
            let _ = tasks.send(ShopTask::Refresh);
        }
    }

    fn wire_shop_callbacks(&self) {
        let weak_refresh = self.window.as_weak();
        let shop_refresh = self.shop.clone();
        let tasks_refresh = self.shop_tasks.clone();
        self.window.on_shop_refresh(move || {
            if let Some(tasks) = &tasks_refresh {
                let _ = tasks.send(ShopTask::Refresh);
            }
            if let Some(window) = weak_refresh.upgrade() {
                push_shop(&window, &shop_refresh.borrow());
            }
        });
        let weak_install = self.window.as_weak();
        let shop_install = self.shop.clone();
        let tasks_install = self.shop_tasks.clone();
        self.window.on_shop_install(move |id| {
            run_shop_action(
                &weak_install,
                &shop_install,
                tasks_install.as_ref(),
                id.as_str(),
                ShopAction::Install,
                true,
            );
        });
        let weak_update = self.window.as_weak();
        let shop_update = self.shop.clone();
        let tasks_update = self.shop_tasks.clone();
        self.window.on_shop_update(move |id| {
            run_shop_action(
                &weak_update,
                &shop_update,
                tasks_update.as_ref(),
                id.as_str(),
                ShopAction::Update,
                true,
            );
        });
        let weak_remove = self.window.as_weak();
        let shop_remove = self.shop.clone();
        let tasks_remove = self.shop_tasks.clone();
        self.window.on_shop_remove(move |id| {
            run_shop_action(
                &weak_remove,
                &shop_remove,
                tasks_remove.as_ref(),
                id.as_str(),
                ShopAction::Remove,
                true,
            );
        });
        let weak_toggle = self.window.as_weak();
        let shop_toggle = self.shop.clone();
        let tasks_toggle = self.shop_tasks.clone();
        self.window.on_shop_set_enabled(move |id, enabled| {
            let action = if enabled {
                ShopAction::Enable
            } else {
                ShopAction::Disable
            };
            run_shop_action(
                &weak_toggle,
                &shop_toggle,
                tasks_toggle.as_ref(),
                id.as_str(),
                action,
                true,
            );
        });
    }

    fn render(&self) {
        self.core.borrow().render(&self.window);
    }

    /// Ask the scheduler for a frame. Returns true when this frame was published.
    fn schedule_redraw(&mut self) -> bool {
        let elapsed = self.core.borrow().started.elapsed();
        let slot = self.core.borrow_mut().scheduler.request(elapsed);
        match slot {
            RedrawSlot::Ready => {
                self.core.borrow_mut().pending_redraw = false;
                self.core.borrow_mut().redraw_count += 1;
                self.render();
                true
            }
            RedrawSlot::Deferred(delay) => {
                let core = Rc::downgrade(&self.core);
                let window = self.window.as_weak();
                self.core.borrow_mut().pending_redraw = true;
                self.redraw_timer
                    .start(slint::TimerMode::SingleShot, delay, move || {
                        let (Some(core), Some(window)) = (core.upgrade(), window.upgrade()) else {
                            return;
                        };
                        let elapsed = core.borrow().started.elapsed();
                        let slot = core.borrow_mut().scheduler.request(elapsed);
                        if slot == RedrawSlot::Ready {
                            core.borrow_mut().pending_redraw = false;
                            core.borrow_mut().redraw_count += 1;
                            core.borrow().render(&window);
                        }
                    });
                false
            }
        }
    }

    pub fn apply(&mut self, event: AppEvent) {
        let immediate = matches!(
            &event,
            AppEvent::Ha(
                HaEvent::ServiceResult { .. }
                    | HaEvent::Connected { .. }
                    | HaEvent::AuthFailed { .. }
                    | HaEvent::Disconnected { .. },
            ) | AppEvent::AlbumArt { .. }
                | AppEvent::ImageFailed { .. }
        );
        self.core.borrow_mut().apply(event);
        // Every state update goes through the scheduler so a burst is counted
        // and coalesced into a single frame. Connection state and action
        // outcomes still publish on this frame when the scheduler deferred the
        // next slot, because callers read them immediately after `apply`.
        let published = self.schedule_redraw();
        if immediate && !published {
            self.render();
        }
    }

    pub fn tap_tile(&mut self, entity_id: &str) -> bool {
        let tapped = self.core.borrow_mut().tap_tile(entity_id);
        self.render();
        tapped
    }

    pub fn set_album_art_sender(&mut self, sender: mpsc::Sender<AlbumArtRequest>) {
        self.core.borrow_mut().album_art_requests = Some(sender);
    }

    pub fn refresh_calendar(&mut self, now: DateTime<Utc>) -> bool {
        let refreshed = self.core.borrow_mut().refresh_calendar(now);
        self.render();
        refreshed
    }

    pub fn calendar_refresh_due(&self, now: DateTime<Utc>) -> bool {
        self.core.borrow().calendar_refresh_due(now)
    }

    pub fn tick_widgets(&mut self, now_instant: Instant, now: DateTime<Utc>) {
        self.core.borrow_mut().tick(now_instant, now);
        self.render();
    }

    pub fn start_event_pump(&mut self, mut events: mpsc::Receiver<AppEvent>) {
        let core = Rc::downgrade(&self.core);
        let window = self.window.as_weak();
        self.event_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(50),
            move || {
                let mut received = false;
                while let Ok(event) = events.try_recv() {
                    if let Some(core) = core.upgrade() {
                        core.borrow_mut().apply(event);
                    }
                    received = true;
                }
                if received
                    && core
                        .upgrade()
                        .is_some_and(|core| !core.borrow().pending_redraw)
                {
                    publish_core(core.clone(), window.clone());
                }
            },
        );
    }

    pub fn start_cache_pump(&mut self, cache: std_mpsc::Sender<AppState>, interval_seconds: u64) {
        let core = Rc::downgrade(&self.core);
        self.cache_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(interval_seconds.max(2)),
            move || {
                if let Some(core) = core.upgrade() {
                    let _ = cache.send(core.borrow().state.clone());
                }
            },
        );
    }

    pub fn redraw_count(&self) -> u64 {
        self.core.borrow().redraw_count
    }

    pub fn clock_tick_count(&self) -> u64 {
        self.core.borrow().clock_tick_count
    }

    pub fn state_snapshot(&self) -> AppState {
        self.core.borrow().state.clone()
    }

    pub fn next_page(&mut self) {
        let next = (self.window.get_current_page() + 1) % self.page_count as i32;
        self.window.set_current_page(next);
    }

    pub fn previous_page(&mut self) {
        let previous =
            (self.window.get_current_page() + self.page_count as i32 - 1) % self.page_count as i32;
        self.window.set_current_page(previous);
    }

    pub fn window(&self) -> &MainWindow {
        &self.window
    }

    pub fn shop(&self) -> std::cell::Ref<'_, ShopView> {
        self.shop.borrow()
    }
}

fn publish_core(core: Weak<RefCell<ControllerCore>>, window: slint::Weak<MainWindow>) {
    let (Some(core), Some(window)) = (core.upgrade(), window.upgrade()) else {
        return;
    };
    let elapsed = core.borrow().started.elapsed();
    match core.borrow_mut().scheduler.request(elapsed) {
        RedrawSlot::Ready => {
            core.borrow_mut().pending_redraw = false;
            core.borrow_mut().redraw_count += 1;
            core.borrow().render(&window);
        }
        RedrawSlot::Deferred(delay) => {
            core.borrow_mut().pending_redraw = true;
            let core = Rc::downgrade(&core);
            let window = window.as_weak();
            slint::Timer::single_shot(delay, move || publish_core(core, window));
        }
    }
}

fn run_shop_action(
    window: &slint::Weak<MainWindow>,
    shop: &SharedShop,
    tasks: Option<&std_mpsc::Sender<ShopTask>>,
    id: &str,
    action: ShopAction,
    confirmed: bool,
) {
    {
        let mut shop = shop.borrow_mut();
        match shop.request(id, action, confirmed) {
            Ok(_request) => {
                let task = match action {
                    ShopAction::Install => Some(ShopTask::Install(id.to_string())),
                    ShopAction::Update => Some(ShopTask::Update(id.to_string())),
                    ShopAction::Remove => Some(ShopTask::Remove(id.to_string())),
                    ShopAction::Enable => Some(ShopTask::SetEnabled(id.to_string(), true)),
                    ShopAction::Disable => Some(ShopTask::SetEnabled(id.to_string(), false)),
                };
                if let (Some(tasks), Some(task)) = (tasks, task) {
                    let _ = tasks.send(task);
                } else {
                    let _ = shop.complete(
                        id,
                        action,
                        Err("The extension shop is not enabled.".to_string()),
                    );
                }
            }
            Err(error) => shop.set_error(id, error.to_string()),
        }
    }
    if let Some(window) = window.upgrade() {
        push_shop(&window, &shop.borrow());
    }
}

fn push_shop(window: &MainWindow, shop: &ShopView) {
    window.set_shop_installed(bridge::shop_installed(shop));
    window.set_shop_available(bridge::shop_available(shop));
    window.set_shop_catalog_online(shop.catalog_online);
    window.set_shop_catalog_error(shop.catalog_error.clone().unwrap_or_default().into());
    window.set_shop_config_path(shop.config_path.clone().into());
    window.set_shop_secret_dir(shop.secret_dir.clone().into());
    window.set_shop_pending_id(
        shop.pending
            .as_ref()
            .map(|pending| pending.id.clone())
            .unwrap_or_default()
            .into(),
    );
    window.set_extension_errors(bridge::extension_errors(shop));
}

fn apply_shop_event(shop: &mut ShopView, event: ShopEvent) {
    match event {
        ShopEvent::InstalledLoaded(installed) => shop.set_installed(installed),
        ShopEvent::CatalogLoaded(catalog) => shop.apply_catalog(catalog),
        ShopEvent::CatalogOffline { reason } => shop.mark_catalog_offline(reason),
        ShopEvent::ActionResult { id, action, result } => {
            let _ = shop.complete(&id, action, result);
        }
        ShopEvent::ExtensionOutputs(_) => {}
    }
}

fn manager_options(config: &Config) -> ManagerOptions {
    let app_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| semver::Version::new(0, 1, 0));
    ManagerOptions {
        app_version,
        catalog_url: config.extensions.catalog_url.clone(),
        public_keys: config.extensions.public_keys.clone(),
        allow_unsigned: config.extensions.allow_unsigned,
        dev_mode: config.extensions.dev_mode,
        max_catalog_bytes: config.extensions.max_catalog_bytes,
        max_package_bytes: config.extensions.max_package_bytes,
        limits: config.extensions.script_limits(),
        store_path: config.extensions.store_path.clone(),
        secret_dir: PathBuf::from(DEFAULT_SECRET_DIR),
        settings: config.extensions.settings.clone(),
    }
}

fn run_shop_worker(
    options: ManagerOptions,
    tasks: std_mpsc::Receiver<ShopTask>,
    events: std_mpsc::Sender<ShopEvent>,
) {
    let trust = options.catalog_policy();
    let manager = HttpsFetcher::for_policy(&trust)
        .and_then(|catalog| HttpsFetcher::for_policy(&trust).map(|package| (catalog, package)));
    let mut manager = match manager {
        Ok((catalog, package)) => match ExtensionManager::new(options, catalog, package) {
            Ok(manager) => Some(manager),
            Err(error) => {
                let _ = events.send(ShopEvent::CatalogOffline {
                    reason: error.to_string(),
                });
                None
            }
        },
        Err(error) => {
            let _ = events.send(ShopEvent::CatalogOffline {
                reason: format!("HTTP client: {error}"),
            });
            None
        }
    };
    let mut catalog_online = false;

    while let Ok(task) = tasks.recv() {
        let Some(manager) = manager.as_mut() else {
            let _ = events.send(ShopEvent::CatalogOffline {
                reason: "The extension store could not be opened.".to_string(),
            });
            continue;
        };
        match task {
            ShopTask::RunExtensions(entities) => {
                let mut displays = Vec::new();
                for entry in manager.installed_entries() {
                    if !entry.enabled {
                        continue;
                    }
                    let id = entry.id;
                    match manager.run(&id, entities.clone()) {
                        Ok(output) => displays.push(output.display(&id, None)),
                        Err(error) => {
                            let message = error.to_string();
                            if let Some(output) = manager
                                .health(&id)
                                .and_then(|health| health.last_good.clone())
                            {
                                displays.push(output.display(&id, Some(message)));
                            } else {
                                displays.push(ExtensionDisplay {
                                    id: id.clone(),
                                    title: id,
                                    lines: Vec::new(),
                                    error: Some(message),
                                });
                            }
                        }
                    }
                }
                let _ = events.send(ShopEvent::ExtensionOutputs(displays));
            }
            ShopTask::Refresh => match manager.refresh_catalog() {
                Ok(entries) => {
                    catalog_online = true;
                    let _ = events.send(ShopEvent::CatalogLoaded(entries));
                    let _ = events.send(ShopEvent::InstalledLoaded(manager.installed_entries()));
                }
                Err(error) => {
                    catalog_online = false;
                    let _ = events.send(ShopEvent::CatalogOffline {
                        reason: error.to_string(),
                    });
                    let _ = events.send(ShopEvent::InstalledLoaded(manager.installed_entries()));
                }
            },
            ShopTask::Install(id) => {
                report_action(&events, &id, ShopAction::Install, manager.install(&id));
                refresh_after_action(manager, &events, catalog_online);
            }
            ShopTask::Update(id) => {
                report_action(&events, &id, ShopAction::Update, manager.update(&id));
                refresh_after_action(manager, &events, catalog_online);
            }
            ShopTask::Remove(id) => {
                report_action(&events, &id, ShopAction::Remove, manager.remove(&id));
                refresh_after_action(manager, &events, catalog_online);
            }
            ShopTask::SetEnabled(id, enabled) => {
                let action = if enabled {
                    ShopAction::Enable
                } else {
                    ShopAction::Disable
                };
                report_action(&events, &id, action, manager.set_enabled(&id, enabled));
                refresh_after_action(manager, &events, catalog_online);
            }
        }
    }
}

fn report_action(
    events: &std_mpsc::Sender<ShopEvent>,
    id: &str,
    action: ShopAction,
    result: Result<(), crate::extensions::ManagerError>,
) {
    let _ = events.send(ShopEvent::ActionResult {
        id: id.to_string(),
        action,
        result: result.map_err(|error| error.to_string()),
    });
}

fn refresh_after_action<
    C: crate::extensions::CatalogFetcher,
    P: crate::extensions::CatalogFetcher,
>(
    manager: &mut ExtensionManager<C, P>,
    events: &std_mpsc::Sender<ShopEvent>,
    catalog_online: bool,
) {
    let _ = events.send(ShopEvent::InstalledLoaded(manager.installed_entries()));
    if catalog_online {
        let _ = events.send(ShopEvent::CatalogLoaded(manager.shop_entries()));
    }
}
