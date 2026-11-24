use crate::Config;
use crate::MainWindow;
use crate::extensions::shop::{DEFAULT_SECRET_DIR, ShopAction, ShopView};
use crate::extensions::{ExtensionManager, HttpsFetcher, ManagerOptions};
use crate::ui::bridge;
use crate::{AppEvent, AppState, Config, HaCommand, ShopEvent};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

type SharedShop = Rc<RefCell<ShopView>>;

/// Work sent to the background extension worker.
enum ShopTask {
    Refresh,
    Install(String),
    Update(String),
    Remove(String),
    SetEnabled(String, bool),
}

#[allow(dead_code)]
pub struct AppController {
    window: MainWindow,
    ui: Rc<RefCell<UiState>>,
    config: Config,
    shop: SharedShop,
    page_count: usize,
    commands: mpsc::Sender<HaCommand>,
    events_tx: mpsc::Sender<AppEvent>,
    ui: Rc<RefCell<UiState>>,
    clock_timer: slint::Timer,
    shop_tasks: Option<std_mpsc::Sender<ShopTask>>,
    shop_timer: Option<slint::Timer>,
}

impl AppController {
    pub fn new(window: MainWindow, config: Config, commands: mpsc::Sender<HaCommand>) -> Self {
        let page_count = if config.extensions.enabled { 5 } else { 4 };

        let weak_next = window.as_weak();
        window.on_swipe_next(move || {
            if let Some(w) = weak_next.upgrade() {
                let p = w.get_current_page();
                let next = if p as usize + 1 >= page_count {
                    0
                } else {
                    p + 1
                };
                w.set_current_page(next);
            }
        });

        let weak_prev = window.as_weak();
        window.on_swipe_prev(move || {
            if let Some(w) = weak_prev.upgrade() {
                let p = w.get_current_page();
                let prev = if p == 0 { page_count as i32 - 1 } else { p - 1 };
                w.set_current_page(prev);
            }
        });

        let config_path = config
            .source_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let shop = Rc::new(RefCell::new(ShopView::new(config_path, DEFAULT_SECRET_DIR)));

        window.set_extensions_enabled(config.extensions.enabled);
        window.set_shop_config_path(shop.borrow().config_path.clone().into());
        window.set_shop_secret_dir(shop.borrow().secret_dir.clone().into());

        let (task_tx, task_rx) = std_mpsc::channel();
        let (event_tx, event_rx) = std_mpsc::channel();
        let shop_tasks = if config.extensions.enabled {
            Some(task_tx)
        } else {
            None
        };

        let mut ctrl = Self {
            window,
            state,
            config,
            shop,
            page_count,
            commands,
            departures,
            last_redraw: Instant::now(),
            pending_redraw: false,
            clock_timer: slint::Timer::default(),
            shop_tasks,
            shop_timer: None,
        };

        ctrl.wire_shop_callbacks();

        if ctrl.config.extensions.enabled {
            let options = manager_options(&ctrl.config);
            std::thread::Builder::new()
                .name("extension-shop".into())
                .spawn(move || run_shop_worker(options, task_rx, event_tx))
                .expect("spawn extension worker");

            let weak_events = ctrl.window.as_weak();
            let shop_events = ctrl.shop.clone();
            let timer = slint::Timer::default();
            timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(200),
                move || {
                    let mut changed = false;
                    while let Ok(event) = event_rx.try_recv() {
                        apply_shop_event(&mut shop_events.borrow_mut(), event);
                        changed = true;
                    }
                    if changed && let Some(window) = weak_events.upgrade() {
                        push_shop(&window, &shop_events.borrow());
                    }
                },
            );
            ctrl.shop_timer = Some(timer);
            if let Some(tasks) = &ctrl.shop_tasks {
                let _ = tasks.send(ShopTask::Refresh);
            }
        }

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

        // Publish the initial state so the shell never shows blank defaults.
        UiBridge::apply(&ctrl.window, &ctrl.state);

        ctrl
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
                departures_dirty = true;
            }
            AppEvent::Shop(shop_event) => {
                {
                    let mut shop = self.shop.borrow_mut();
                    match shop_event {
                        ShopEvent::InstalledLoaded(installed) => shop.set_installed(installed),
                        ShopEvent::CatalogLoaded(catalog) => shop.apply_catalog(catalog),
                        ShopEvent::CatalogOffline { reason } => shop.mark_catalog_offline(reason),
                        ShopEvent::ActionResult { id, action, result } => {
                            let _ = shop.complete(&id, action, result);
                        }
                    }
                }
                push_shop(&self.window, &self.shop.borrow());
            }
            AppEvent::Weather(r) => {
                if let Ok(v) = r {
                    self.state.weather = Some(v);
                }
            }
            AppEvent::Departures(r) => {
                if let Ok(v) = r {
                    self.state.departures = Some(v);
                    departures_dirty = true;
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
        self.schedule_redraw();
    }

        UiBridge::apply(&self.state, &self.window);

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
        let next = if p as usize + 1 >= self.page_count {
            0
        } else {
            p + 1
        };
        self.window.set_current_page(next);
    }

    pub fn previous_page(&mut self) {
        let p = self.window.get_current_page();
        let prev = if p == 0 {
            self.page_count as i32 - 1
        } else {
            p - 1
        };
        self.window.set_current_page(prev);
    }

    pub fn window(&self) -> &MainWindow {
        &self.window
    }

    pub fn shop(&self) -> std::cell::Ref<'_, ShopView> {
        self.shop.borrow()
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
    let manager = HttpsFetcher::new()
        .and_then(|catalog| HttpsFetcher::new().map(|package| (catalog, package)));
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

fn wire_media_callbacks(
    window: &MainWindow,
    media_ui: &Rc<RefCell<MediaUi>>,
    commands: &mpsc::Sender<HaCommand>,
) {
    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_play_pause(move || {
        dispatch_transport(
            &ui,
            &tx,
            &weak,
            media::can_play_pause,
            media::play_pause_call,
        );
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_previous_track(move || {
        dispatch_transport(
            &ui,
            &tx,
            &weak,
            media::can_previous,
            media::previous_track_call,
        );
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_next_track(move || {
        dispatch_transport(&ui, &tx, &weak, media::can_next, media::next_track_call);
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_volume_changed(move |value| {
        let mut state = ui.borrow_mut();
        let Some(view) = state.view.clone() else {
            return;
        };
        if !media::can_set_volume(&view) {
            return;
        }
        let Some(volume) = state.volume.on_change(value, Instant::now()) else {
            return;
        };
        state.next_request_id += 1;
        let call = media::volume_call(state.next_request_id, &view.entity_id, volume);
        if tx.try_send(HaCommand::CallService(call)).is_err() {
            state.error = Some("Home Assistant is not connected".to_string());
        }
        drop(state);
        refresh(&weak, &ui);
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_volume_released(move |value| {
        let mut state = ui.borrow_mut();
        let Some(view) = state.view.clone() else {
            return;
        };
        if !media::can_set_volume(&view) {
            return;
        }
        let volume = state.volume.on_release(value, Instant::now());
        state.next_request_id += 1;
        let call = media::volume_call(state.next_request_id, &view.entity_id, volume);
        if tx.try_send(HaCommand::CallService(call)).is_err() {
            state.error = Some("Home Assistant is not connected".to_string());
        }
        drop(state);
        refresh(&weak, &ui);
    });
}

fn dispatch_transport(
    ui: &Rc<RefCell<MediaUi>>,
    commands: &mpsc::Sender<HaCommand>,
    weak: &slint::Weak<MainWindow>,
    allowed: fn(&MediaView) -> bool,
    build: fn(u64, &str) -> ServiceCall,
) {
    let mut state = ui.borrow_mut();
    if state.pending_request.is_some() {
        return;
    }
    let Some(view) = state.view.clone() else {
        return;
    };
    if !allowed(&view) {
        return;
    }
    state.next_request_id += 1;
    let request_id = state.next_request_id;
    let call = build(request_id, &view.entity_id);
    match commands.try_send(HaCommand::CallService(call)) {
        Ok(()) => {
            state.pending_request = Some(request_id);
            state.error = None;
        }
        Err(_) => state.error = Some("Home Assistant is not connected".to_string()),
    }
    drop(state);
    refresh(weak, ui);
}

fn refresh(window: &slint::Weak<MainWindow>, media_ui: &Rc<RefCell<MediaUi>>) {
    let Some(window) = window.upgrade() else {
        return;
    };
    let ui = media_ui.borrow();
    bridge::apply_media(
        &window,
        ui.view.as_ref(),
        &ui.art,
        ui.pending_request.is_some(),
        ui.error.as_deref(),
        ui.volume_step,
    );
}
