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
    state: AppState,
    config: Config,
    shop: SharedShop,
    page_count: usize,
    commands: mpsc::Sender<HaCommand>,
    last_redraw: Instant,
    pending_redraw: bool,
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
        window.set_shop_loading(shop.borrow().catalog_loading);

        let (task_tx, task_rx) = std_mpsc::channel();
        let (event_tx, event_rx) = std_mpsc::channel();
        let shop_tasks = if config.extensions.enabled {
            Some(task_tx)
        } else {
            None
        };

        let mut ctrl = Self {
            window,
            state: AppState::default(),
            config,
            shop,
            page_count,
            commands,
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

        ctrl
    }

    fn wire_shop_callbacks(&self) {
        let weak_refresh = self.window.as_weak();
        let shop_refresh = self.shop.clone();
        let tasks_refresh = self.shop_tasks.clone();
        self.window.on_shop_refresh(move || {
            {
                let mut shop = shop_refresh.borrow_mut();
                shop.begin_refresh();
                if let Some(tasks) = &tasks_refresh
                    && tasks.send(ShopTask::Refresh).is_err()
                {
                    shop.mark_catalog_offline("The extension store worker is unavailable.");
                }
            }
            if let Some(window) = weak_refresh.upgrade() {
                push_shop(&window, &shop_refresh.borrow());
            }
        });

        let weak_install = self.window.as_weak();
        let shop_install = self.shop.clone();
        let tasks_install = self.shop_tasks.clone();
        self.window.on_shop_install(move |id, confirmed| {
            run_shop_action(
                &weak_install,
                &shop_install,
                tasks_install.as_ref(),
                id.as_str(),
                ShopAction::Install,
                confirmed,
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
        // Update state on the UI thread.
        match event {
            AppEvent::Ha(ha_event) => {
                self.state.apply_ha_event(ha_event);
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
                    self.state.calendar = Some(v);
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
                    if tasks.send(task).is_err() {
                        let _ = shop.complete(
                            id,
                            action,
                            Err("The extension store worker is unavailable.".to_string()),
                        );
                    }
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
    window.set_shop_loading(shop.catalog_loading);
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
                let result = manager.install(&id);
                refresh_after_action(manager, &events, catalog_online);
                report_action(&events, &id, ShopAction::Install, result);
            }
            ShopTask::Update(id) => {
                let result = manager.update(&id);
                refresh_after_action(manager, &events, catalog_online);
                report_action(&events, &id, ShopAction::Update, result);
            }
            ShopTask::Remove(id) => {
                let result = manager.remove(&id);
                refresh_after_action(manager, &events, catalog_online);
                report_action(&events, &id, ShopAction::Remove, result);
            }
            ShopTask::SetEnabled(id, enabled) => {
                let action = if enabled {
                    ShopAction::Enable
                } else {
                    ShopAction::Disable
                };
                let result = manager.set_enabled(&id, enabled);
                refresh_after_action(manager, &events, catalog_online);
                report_action(&events, &id, action, result);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::shop::{ExtensionPermissions, ShopEntry};
    use std::collections::BTreeMap;
    use std::sync::Once;
    use std::time::Duration;

    static INIT_BACKEND: Once = Once::new();

    fn manager_options(store_path: PathBuf) -> ManagerOptions {
        ManagerOptions {
            app_version: semver::Version::new(0, 1, 0),
            catalog_url: None,
            public_keys: Vec::new(),
            allow_unsigned: false,
            dev_mode: false,
            max_catalog_bytes: 2 * 1024 * 1024,
            max_package_bytes: 5 * 1024 * 1024,
            store_path,
            secret_dir: PathBuf::from(DEFAULT_SECRET_DIR),
            settings: BTreeMap::new(),
        }
    }

    fn catalog_entry() -> ShopEntry {
        ShopEntry {
            id: "org.example.bus-departures".to_string(),
            name: "Bus Departures".to_string(),
            version: "1.0.0".to_string(),
            author: "Example Author".to_string(),
            license: "MIT".to_string(),
            description: "Shows upcoming bus departures.".to_string(),
            provides: "departures".to_string(),
            permissions: ExtensionPermissions::default(),
            config_schema: Vec::new(),
            installed_version: None,
            enabled: false,
            configured: false,
            error: None,
            available: true,
        }
    }

    #[test]
    fn worker_sends_installed_snapshot_before_action_result() {
        let directory = tempfile::tempdir().unwrap();
        let (tasks_tx, tasks_rx) = std_mpsc::channel();
        let (events_tx, events_rx) = std_mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_shop_worker(
                manager_options(directory.path().to_path_buf()),
                tasks_rx,
                events_tx,
            )
        });

        tasks_tx
            .send(ShopTask::Install("org.example.bus-departures".to_string()))
            .unwrap();
        let first = events_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(first, ShopEvent::InstalledLoaded(_)),
            "the installed snapshot must arrive first so it cannot erase the action error: {first:?}"
        );
        let second = events_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(
                second,
                ShopEvent::ActionResult {
                    action: ShopAction::Install,
                    result: Err(_),
                    ..
                }
            ),
            "the action result must be applied after the snapshot: {second:?}"
        );

        drop(tasks_tx);
        worker.join().unwrap();
    }

    #[test]
    fn shop_bridge_and_dispatch_failures_are_honest() {
        INIT_BACKEND.call_once(i_slint_backend_testing::init_no_event_loop);
        let window = MainWindow::new().unwrap();
        let shop = Rc::new(RefCell::new(ShopView::new(
            "/etc/ha-kiosk/config.toml",
            DEFAULT_SECRET_DIR,
        )));

        push_shop(&window, &shop.borrow());
        assert!(window.get_shop_loading());
        shop.borrow_mut().apply_catalog(vec![catalog_entry()]);
        push_shop(&window, &shop.borrow());
        assert!(!window.get_shop_loading());

        let (tasks_tx, tasks_rx) = std_mpsc::channel();
        drop(tasks_rx);

        run_shop_action(
            &window.as_weak(),
            &shop,
            Some(&tasks_tx),
            "org.example.bus-departures",
            ShopAction::Install,
            true,
        );

        assert!(shop.borrow().pending.is_none());
        assert_eq!(
            shop.borrow()
                .entry("org.example.bus-departures")
                .unwrap()
                .error
                .as_deref(),
            Some("The extension store worker is unavailable.")
        );
    }

    #[test]
    fn unconfirmed_install_is_rejected_before_a_task_is_sent() {
        // No window is needed: the action must be rejected before any UI push.
        let window = slint::Weak::<MainWindow>::default();
        let shop = Rc::new(RefCell::new(ShopView::new(
            "/etc/ha-kiosk/config.toml",
            DEFAULT_SECRET_DIR,
        )));
        shop.borrow_mut().apply_catalog(vec![catalog_entry()]);

        let (tasks_tx, tasks_rx) = std_mpsc::channel();
        run_shop_action(
            &window,
            &shop,
            Some(&tasks_tx),
            "org.example.bus-departures",
            ShopAction::Install,
            false,
        );

        assert!(
            tasks_rx.try_recv().is_err(),
            "no store task may be sent without the trust confirmation"
        );
        assert!(shop.borrow().pending.is_none());
        assert_eq!(
            shop.borrow()
                .entry("org.example.bus-departures")
                .unwrap()
                .error
                .as_deref(),
            Some("confirmation is required before installing org.example.bus-departures")
        );
    }
}
