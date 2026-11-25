use ha_kiosk::extensions::shop::{
    ConfigField, DEFAULT_SECRET_DIR, ExtensionPermissions, InstalledExtension, ShopAction,
    ShopEntry, ShopView,
};
use ha_kiosk::ui::bridge;
use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, Model, PhysicalSize};
use std::cell::RefCell;
use std::rc::Rc;

const EXTENSION_ID: &str = "org.example.bus-departures";
const WEATHER_ID: &str = "org.example.weather";
const CALENDAR_ID: &str = "org.example.calendar";
const PERMISSIONS: &str = "Network: api.example.com; Home Assistant: sensor.example";
const TRUST_WARNING: &str = "Community extensions are third-party code. A signed catalog proves who published a package, not that it is safe. Only install extensions you trust, and review the permissions below.";

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShopCallback {
    Refresh,
    Install(String),
    Update(String),
    Remove(String),
    SetEnabled(String, bool),
}

struct ShopCallbacks(Rc<RefCell<Vec<ShopCallback>>>);

impl ShopCallbacks {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Vec::new())))
    }

    fn wire(&self, app: &ha_kiosk::MainWindow) {
        let events = self.0.clone();
        app.on_shop_refresh(move || events.borrow_mut().push(ShopCallback::Refresh));

        let events = self.0.clone();
        app.on_shop_install(move |id| {
            events
                .borrow_mut()
                .push(ShopCallback::Install(id.to_string()));
        });

        let events = self.0.clone();
        app.on_shop_update(move |id| {
            events
                .borrow_mut()
                .push(ShopCallback::Update(id.to_string()));
        });

        let events = self.0.clone();
        app.on_shop_remove(move |id| {
            events
                .borrow_mut()
                .push(ShopCallback::Remove(id.to_string()));
        });

        let events = self.0.clone();
        app.on_shop_set_enabled(move |id, enabled| {
            events
                .borrow_mut()
                .push(ShopCallback::SetEnabled(id.to_string(), enabled));
        });
    }

    fn events(&self) -> Vec<ShopCallback> {
        self.0.borrow().clone()
    }
}

fn catalog_entry() -> ShopEntry {
    ShopEntry {
        id: EXTENSION_ID.to_string(),
        name: "Bus Departures".to_string(),
        version: "1.0.0".to_string(),
        author: "Example Author".to_string(),
        license: "MIT".to_string(),
        description: "Shows upcoming bus departures for a configured stop.".to_string(),
        provides: "departures".to_string(),
        permissions: ExtensionPermissions {
            http_hosts: vec!["api.example.com".to_string()],
            ha_entities: vec!["sensor.example".to_string()],
        },
        config_schema: vec![
            ConfigField {
                key: "stop_id".to_string(),
                label: "Stop ID".to_string(),
                kind: "string".to_string(),
                required: true,
            },
            ConfigField {
                key: "limit".to_string(),
                label: "Maximum departures".to_string(),
                kind: "integer".to_string(),
                required: false,
            },
        ],
        installed_version: None,
        enabled: false,
        configured: false,
        error: None,
        available: false,
    }
}

fn named_entry(id: &str, name: &str, version: &str) -> ShopEntry {
    let mut entry = catalog_entry();
    entry.id = id.to_string();
    entry.name = name.to_string();
    entry.version = version.to_string();
    entry
}

fn view_with_catalog() -> ShopView {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.apply_catalog(vec![catalog_entry()]);
    view
}

fn render_shop(app: &ha_kiosk::MainWindow, view: &ShopView) {
    app.set_shop_installed(bridge::shop_installed(view));
    app.set_shop_available(bridge::shop_available(view));
    app.set_shop_catalog_online(view.catalog_online);
    app.set_shop_catalog_error(view.catalog_error.clone().unwrap_or_default().into());
    app.set_shop_config_path(view.config_path.clone().into());
    app.set_shop_secret_dir(view.secret_dir.clone().into());
    app.set_shop_pending_id(
        view.pending
            .as_ref()
            .map(|pending| pending.id.clone())
            .unwrap_or_default()
            .into(),
    );
}

fn element(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"))
}

fn has_element(app: &ha_kiosk::MainWindow, id: &str) -> bool {
    ElementHandle::find_by_element_id(app, id).next().is_some()
}

fn matching_text(root: &ElementHandle, text: &str) -> Vec<ElementHandle> {
    root.query_descendants()
        .match_accessible_label(text)
        .find_all()
}

fn app_text(app: &ha_kiosk::MainWindow, text: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(app, text).collect()
}

fn assert_app_text(app: &ha_kiosk::MainWindow, text: &str) {
    assert!(
        !app_text(app, text).is_empty(),
        "expected rendered text {text:?}"
    );
}

fn assert_no_app_text(app: &ha_kiosk::MainWindow, text: &str) {
    assert!(
        app_text(app, text).is_empty(),
        "unexpected rendered text {text:?}"
    );
}

fn card_by_name(app: &ha_kiosk::MainWindow, name: &str) -> ElementHandle {
    ElementHandle::find_by_element_type_name(app, "ShopEntryCard")
        .find(|card| !matching_text(card, name).is_empty())
        .unwrap_or_else(|| panic!("shop card {name:?} not found"))
}

fn assert_card_text(app: &ha_kiosk::MainWindow, name: &str, text: &str) {
    let card = card_by_name(app, name);
    assert!(
        !matching_text(&card, text).is_empty(),
        "card {name:?} must render {text:?}"
    );
}

fn assert_no_card_text(app: &ha_kiosk::MainWindow, name: &str, text: &str) {
    let card = card_by_name(app, name);
    assert!(
        matching_text(&card, text).is_empty(),
        "card {name:?} must not render {text:?}"
    );
}

fn has_card_element(app: &ha_kiosk::MainWindow, name: &str, id: &str) -> bool {
    card_by_name(app, name)
        .query_descendants()
        .match_id(id)
        .find_all()
        .into_iter()
        .next()
        .is_some()
}

async fn click(app: &ha_kiosk::MainWindow, id: &str) {
    element(app, id)
        .single_click(PointerEventButton::Left)
        .await;
}

async fn click_in_card(app: &ha_kiosk::MainWindow, name: &str, id: &str) {
    card_by_name(app, name)
        .query_descendants()
        .match_id(id)
        .find_first()
        .unwrap_or_else(|| panic!("element {id:?} not found in card {name:?}"))
        .single_click(PointerEventButton::Left)
        .await;
}

async fn click_card_control(app: &ha_kiosk::MainWindow, name: &str, label: &str) {
    matching_text(&card_by_name(app, name), label)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("control {label:?} not found in card {name:?}"))
        .single_click(PointerEventButton::Left)
        .await;
}

fn scroll_shop(app: &ha_kiosk::MainWindow, delta_y: f32) {
    element(app, "ShopPage::fl").scroll(0.0, delta_y);
}

fn assert_pending(app: &ha_kiosk::MainWindow, id: &str) {
    assert_app_text(app, &format!("Working on {id}…"));
}

async fn install_flow_shows_permissions_and_requires_trust_confirmation() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    assert!(!has_element(&app, "MainWindow::tab-shop"));
    app.set_extensions_enabled(true);
    click(&app, "MainWindow::tab-shop").await;
    assert_eq!(app.get_current_page(), 4);

    let view = view_with_catalog();
    render_shop(&app, &view);

    assert_card_text(&app, "Bus Departures", "Bus Departures");
    assert_card_text(
        &app,
        "Bus Departures",
        &format!("Permissions: {PERMISSIONS}"),
    );
    assert_card_text(
        &app,
        "Bus Departures",
        "Configuration: stop_id (required), limit",
    );
    assert_card_text(
        &app,
        "Bus Departures",
        "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]",
    );
    assert_card_text(
        &app,
        "Bus Departures",
        "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml",
    );

    let callbacks = ShopCallbacks::new();
    callbacks.wire(&app);
    let permissions = format!("Permissions: {PERMISSIONS}");
    assert_eq!(app_text(&app, &permissions).len(), 1);

    click_in_card(&app, "Bus Departures", "ShopEntryCard::install-button").await;
    assert!(callbacks.events().is_empty());
    assert_app_text(&app, "Install Bus Departures?");
    assert_app_text(&app, TRUST_WARNING);
    assert_eq!(
        app_text(&app, &permissions).len(),
        2,
        "permissions must remain visible on the card and in the trust confirmation"
    );

    click(&app, "ShopPage::cancel-button").await;
    assert_no_app_text(&app, "Install Bus Departures?");
    assert_eq!(app_text(&app, &permissions).len(), 1);
    assert!(callbacks.events().is_empty());

    click_in_card(&app, "Bus Departures", "ShopEntryCard::install-button").await;
    click(&app, "ShopPage::confirm-button").await;
    assert_eq!(
        callbacks.events(),
        vec![ShopCallback::Install(EXTENSION_ID.to_string())],
        "confirmation must emit only the exact extension ID"
    );
    assert_no_app_text(&app, "Install Bus Departures?");
}

async fn installed_lifecycle_updates_models_after_store_results() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_extensions_enabled(true);
    app.set_current_page(4);

    let mut view = view_with_catalog();
    render_shop(&app, &view);
    assert_card_text(&app, "Bus Departures", "Available");

    let callbacks = ShopCallbacks::new();
    callbacks.wire(&app);

    click_in_card(&app, "Bus Departures", "ShopEntryCard::install-button").await;
    click(&app, "ShopPage::confirm-button").await;
    assert_eq!(
        callbacks.events(),
        vec![ShopCallback::Install(EXTENSION_ID.to_string())]
    );

    view.request(EXTENSION_ID, ShopAction::Install, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Available");
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::install-button"
    ));
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::remove-button"
    ));

    view.complete(EXTENSION_ID, ShopAction::Install, Ok(()))
        .unwrap();
    render_shop(&app, &view);
    assert_no_app_text(&app, &format!("Working on {EXTENSION_ID}…"));
    assert_card_text(&app, "Bus Departures", "Installed");
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::install-button"
    ));
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::toggle-button"
    ));
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::remove-button"
    ));

    click_in_card(&app, "Bus Departures", "ShopEntryCard::toggle-button").await;
    view.request(EXTENSION_ID, ShopAction::Disable, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Installed");
    view.complete(EXTENSION_ID, ShopAction::Disable, Ok(()))
        .unwrap();
    render_shop(&app, &view);
    assert_card_text(&app, "Bus Departures", "Disabled");

    click_in_card(&app, "Bus Departures", "ShopEntryCard::toggle-button").await;
    view.request(EXTENSION_ID, ShopAction::Enable, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Disabled");
    view.complete(EXTENSION_ID, ShopAction::Enable, Ok(()))
        .unwrap();
    render_shop(&app, &view);
    assert_card_text(&app, "Bus Departures", "Installed");

    view.apply_catalog(vec![named_entry(EXTENSION_ID, "Bus Departures", "1.1.0")]);
    render_shop(&app, &view);
    assert_card_text(&app, "Bus Departures", "Update available");
    assert_card_text(&app, "Bus Departures", "v1.0.0");
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::update-button"
    ));
    click_in_card(&app, "Bus Departures", "ShopEntryCard::update-button").await;

    view.request(EXTENSION_ID, ShopAction::Update, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Update available");
    assert_card_text(&app, "Bus Departures", "v1.0.0");
    view.complete(EXTENSION_ID, ShopAction::Update, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_no_card_text(&app, "Bus Departures", "Update available");
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::update-button"
    ));
    assert_card_text(&app, "Bus Departures", "v1.1.0");

    click_in_card(&app, "Bus Departures", "ShopEntryCard::remove-button").await;
    view.request(EXTENSION_ID, ShopAction::Remove, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Installed");
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::remove-button"
    ));
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::install-button"
    ));
    view.complete(EXTENSION_ID, ShopAction::Remove, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_card_text(&app, "Bus Departures", "Available");
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::install-button"
    ));
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::remove-button"
    ));
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::toggle-button"
    ));
    assert_eq!(
        callbacks.events(),
        vec![
            ShopCallback::Install(EXTENSION_ID.to_string()),
            ShopCallback::SetEnabled(EXTENSION_ID.to_string(), false),
            ShopCallback::SetEnabled(EXTENSION_ID.to_string(), true),
            ShopCallback::Update(EXTENSION_ID.to_string()),
            ShopCallback::Remove(EXTENSION_ID.to_string()),
        ]
    );
}

async fn offline_state_hides_offers_and_keeps_installed_actions_usable() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_extensions_enabled(true);
    app.set_current_page(4);

    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![
        InstalledExtension {
            id: EXTENSION_ID.to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            configured: true,
            error: None,
        },
        InstalledExtension {
            id: WEATHER_ID.to_string(),
            version: "2.0.0".to_string(),
            enabled: false,
            configured: true,
            error: None,
        },
    ]);
    view.apply_catalog(vec![
        named_entry(EXTENSION_ID, "Bus Departures", "1.1.0"),
        named_entry(WEATHER_ID, "Weather", "2.0.0"),
        named_entry(CALENDAR_ID, "Calendar", "1.0.0"),
    ]);
    render_shop(&app, &view);

    scroll_shop(&app, -600.0);
    assert_card_text(&app, "Calendar", "Install");
    assert_eq!(app.get_shop_installed().row_count(), 2);
    assert_eq!(app.get_shop_available().row_count(), 1);
    assert!(
        app.get_shop_installed()
            .row_data(0)
            .unwrap()
            .update_available
    );

    view.mark_catalog_offline("network unreachable");
    render_shop(&app, &view);
    scroll_shop(&app, 600.0);

    assert_app_text(&app, "Catalog offline");
    assert_app_text(
        &app,
        "The catalog is offline. Installed extensions keep working; install and update are unavailable.",
    );
    assert_app_text(&app, "network unreachable");
    assert_no_app_text(&app, "Calendar");
    assert_eq!(app.get_shop_installed().row_count(), 2);
    assert_eq!(app.get_shop_available().row_count(), 0);
    assert_no_card_text(&app, "Bus Departures", "Update available");
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::update-button"
    ));
    assert_card_text(&app, "Bus Departures", "Disable");
    assert_card_text(&app, "Weather", "Enable");
    assert_card_text(&app, "Bus Departures", "Remove");
    assert_card_text(&app, "Weather", "Remove");

    let callbacks = ShopCallbacks::new();
    callbacks.wire(&app);
    click(&app, "ShopPage::refresh-button").await;
    click_card_control(&app, "Bus Departures", "Disable").await;
    click_card_control(&app, "Weather", "Enable").await;
    click_card_control(&app, "Bus Departures", "Remove").await;
    click_card_control(&app, "Weather", "Remove").await;

    assert_eq!(
        callbacks.events(),
        vec![
            ShopCallback::Refresh,
            ShopCallback::SetEnabled(EXTENSION_ID.to_string(), false),
            ShopCallback::SetEnabled(WEATHER_ID.to_string(), true),
            ShopCallback::Remove(EXTENSION_ID.to_string()),
            ShopCallback::Remove(WEATHER_ID.to_string()),
        ]
    );
}

async fn failed_installs_render_errors_on_their_own_cards_and_retry() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_extensions_enabled(true);
    app.set_current_page(4);

    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.apply_catalog(vec![
        catalog_entry(),
        named_entry(WEATHER_ID, "Weather", "2.0.0"),
    ]);
    view.request(EXTENSION_ID, ShopAction::Install, true)
        .unwrap();
    view.complete(
        EXTENSION_ID,
        ShopAction::Install,
        Err("checksum mismatch".to_string()),
    )
    .unwrap();
    view.request(WEATHER_ID, ShopAction::Install, true).unwrap();
    view.complete(
        WEATHER_ID,
        ShopAction::Install,
        Err("network unreachable".to_string()),
    )
    .unwrap();
    render_shop(&app, &view);

    assert_card_text(&app, "Bus Departures", "Error");
    assert_card_text(&app, "Bus Departures", "checksum mismatch");
    assert_no_card_text(&app, "Bus Departures", "network unreachable");
    assert_card_text(&app, "Weather", "Error");
    assert_card_text(&app, "Weather", "network unreachable");
    assert_no_card_text(&app, "Weather", "checksum mismatch");
    assert_card_text(&app, "Bus Departures", "Install");
    assert_no_card_text(&app, "Bus Departures", "Remove");
    assert_no_card_text(&app, "Bus Departures", "Disable");

    let callbacks = ShopCallbacks::new();
    callbacks.wire(&app);
    click_in_card(&app, "Bus Departures", "ShopEntryCard::install-button").await;
    assert_app_text(&app, "Install Bus Departures?");
    assert!(callbacks.events().is_empty());
    click(&app, "ShopPage::cancel-button").await;
    assert!(callbacks.events().is_empty());

    click_in_card(&app, "Bus Departures", "ShopEntryCard::install-button").await;
    click(&app, "ShopPage::confirm-button").await;
    assert_eq!(
        callbacks.events(),
        vec![ShopCallback::Install(EXTENSION_ID.to_string())]
    );

    view.request(EXTENSION_ID, ShopAction::Install, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Error");
    view.complete(EXTENSION_ID, ShopAction::Install, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_card_text(&app, "Bus Departures", "Installed");
    assert_no_card_text(&app, "Bus Departures", "Error");
    assert_no_card_text(&app, "Bus Departures", "checksum mismatch");
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::install-button"
    ));
    assert_card_text(&app, "Weather", "network unreachable");
}

async fn failed_update_keeps_the_installed_version_and_retry_available() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_extensions_enabled(true);
    app.set_current_page(4);

    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: EXTENSION_ID.to_string(),
        version: "1.0.0".to_string(),
        enabled: true,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(vec![named_entry(EXTENSION_ID, "Bus Departures", "1.1.0")]);
    render_shop(&app, &view);
    assert_card_text(&app, "Bus Departures", "Update available");
    assert_card_text(&app, "Bus Departures", "v1.0.0");

    view.request(EXTENSION_ID, ShopAction::Update, true)
        .unwrap();
    view.complete(
        EXTENSION_ID,
        ShopAction::Update,
        Err("package download failed".to_string()),
    )
    .unwrap();
    render_shop(&app, &view);

    assert_eq!(
        view.entry(EXTENSION_ID)
            .and_then(|entry| entry.installed_version.as_deref()),
        Some("1.0.0")
    );
    assert_card_text(&app, "Bus Departures", "v1.0.0");
    assert_no_card_text(&app, "Bus Departures", "v1.1.0");
    assert_card_text(&app, "Bus Departures", "Error");
    assert_card_text(&app, "Bus Departures", "package download failed");
    assert_no_card_text(&app, "Bus Departures", "Update available");
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::update-button"
    ));
    assert_card_text(&app, "Bus Departures", "Remove");

    let callbacks = ShopCallbacks::new();
    callbacks.wire(&app);
    click_in_card(&app, "Bus Departures", "ShopEntryCard::update-button").await;
    assert_eq!(
        callbacks.events(),
        vec![ShopCallback::Update(EXTENSION_ID.to_string())]
    );

    view.request(EXTENSION_ID, ShopAction::Update, true)
        .unwrap();
    render_shop(&app, &view);
    assert_pending(&app, EXTENSION_ID);
    assert_card_text(&app, "Bus Departures", "Error");
    assert!(has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::update-button"
    ));
    view.complete(EXTENSION_ID, ShopAction::Update, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_card_text(&app, "Bus Departures", "Installed");
    assert_card_text(&app, "Bus Departures", "v1.1.0");
    assert_no_card_text(&app, "Bus Departures", "Error");
    assert_no_card_text(&app, "Bus Departures", "package download failed");
    assert!(!has_card_element(
        &app,
        "Bus Departures",
        "ShopEntryCard::update-button"
    ));
}

#[test]
fn shop_page_flows_cover_install_update_remove_offline_and_errors() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        install_flow_shows_permissions_and_requires_trust_confirmation().await;
        installed_lifecycle_updates_models_after_store_results().await;
        offline_state_hides_offers_and_keeps_installed_actions_usable().await;
        failed_installs_render_errors_on_their_own_cards_and_retry().await;
        failed_update_keeps_the_installed_version_and_retry_available().await;
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
