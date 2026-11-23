use crate::MainWindow;
use crate::extensions::shop::{DEFAULT_SECRET_DIR, ShopAction, ShopView};
use crate::ui::bridge;
use crate::{AppEvent, AppState, Config, HaCommand, ShopEvent};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

type SharedShop = Rc<RefCell<ShopView>>;

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

        let ctrl = Self {
            window,
            state: AppState::default(),
            config,
            shop,
            page_count,
            commands,
            last_redraw: Instant::now(),
            pending_redraw: false,
            clock_timer: slint::Timer::default(),
        };

        ctrl.wire_shop_callbacks();

        if ctrl.config.extensions.enabled {
            // The signed catalog and store arrive in Wave 6.1. Until they are
            // wired, report the catalog as offline instead of inventing data.
            ctrl.shop.borrow_mut().mark_catalog_offline(
                "The extension catalog is not available until the extension store is configured.",
            );
            push_shop(&ctrl.window, &ctrl.shop.borrow());
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
        self.window.on_shop_refresh(move || {
            {
                let mut shop = shop_refresh.borrow_mut();
                shop.mark_catalog_offline(
                    "Catalog refresh requires the extension store, which is not configured.",
                );
            }
            if let Some(window) = weak_refresh.upgrade() {
                push_shop(&window, &shop_refresh.borrow());
            }
        });

        let weak_install = self.window.as_weak();
        let shop_install = self.shop.clone();
        self.window.on_shop_install(move |id| {
            run_shop_action(
                &weak_install,
                &shop_install,
                id.as_str(),
                ShopAction::Install,
                true,
            );
        });

        let weak_update = self.window.as_weak();
        let shop_update = self.shop.clone();
        self.window.on_shop_update(move |id| {
            run_shop_action(
                &weak_update,
                &shop_update,
                id.as_str(),
                ShopAction::Update,
                true,
            );
        });

        let weak_remove = self.window.as_weak();
        let shop_remove = self.shop.clone();
        self.window.on_shop_remove(move |id| {
            run_shop_action(
                &weak_remove,
                &shop_remove,
                id.as_str(),
                ShopAction::Remove,
                true,
            );
        });

        let weak_toggle = self.window.as_weak();
        let shop_toggle = self.shop.clone();
        self.window.on_shop_set_enabled(move |id, enabled| {
            let action = if enabled {
                ShopAction::Enable
            } else {
                ShopAction::Disable
            };
            run_shop_action(&weak_toggle, &shop_toggle, id.as_str(), action, true);
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
    id: &str,
    action: ShopAction,
    confirmed: bool,
) {
    {
        let mut shop = shop.borrow_mut();
        match shop.request(id, action, confirmed) {
            Ok(_request) => {
                // The Wave 6.1 store consumes the request off the UI thread and
                // reports back with ShopEvent::ActionResult. Until it is wired,
                // report an honest error instead of faking success.
                let _ = shop.complete(
                    id,
                    action,
                    Err("The extension store is not available in this build.".to_string()),
                );
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
