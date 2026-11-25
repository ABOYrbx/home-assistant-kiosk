use ha_kiosk::extensions::shop::{
    ConfigField, DEFAULT_SECRET_DIR, ExtensionPermissions, InstalledExtension, ShopAction,
    ShopEntry, ShopView,
};
use ha_kiosk::ui::bridge;
use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, PhysicalSize};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const EXTENSION_ID: &str = "org.example.bus-departures";
const PERMISSIONS: &str = "Network: api.example.com; Home Assistant: sensor.example";
const TRUST_WARNING: &str = "Community extensions are third-party code. A signed catalog proves who published a package, not that it is safe. Only install extensions you trust, and review the permissions below.";

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

fn matching_text(app: &ha_kiosk::MainWindow, text: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(app, text).collect()
}

fn assert_text(app: &ha_kiosk::MainWindow, text: &str) {
    assert!(
        !matching_text(app, text).is_empty(),
        "expected rendered text {text:?}"
    );
}

fn assert_no_text(app: &ha_kiosk::MainWindow, text: &str) {
    assert!(
        matching_text(app, text).is_empty(),
        "unexpected rendered text {text:?}"
    );
}

async fn click(app: &ha_kiosk::MainWindow, id: &str) {
    element(app, id)
        .single_click(PointerEventButton::Left)
        .await;
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

    assert_text(&app, "Bus Departures");
    assert_text(&app, &format!("Permissions: {PERMISSIONS}"));
    assert_text(&app, "Configuration: stop_id (required), limit");
    assert_text(
        &app,
        "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]",
    );
    assert_text(
        &app,
        "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml",
    );

    let installs = Rc::new(RefCell::new(Vec::new()));
    let installs_sink = installs.clone();
    app.on_shop_install(move |id| installs_sink.borrow_mut().push(id.to_string()));

    click(&app, "ShopEntryCard::install-button").await;
    assert!(installs.borrow().is_empty());
    assert_text(&app, "Install Bus Departures?");
    assert_text(&app, TRUST_WARNING);
    assert!(
        matching_text(&app, &format!("Permissions: {PERMISSIONS}")).len() >= 2,
        "permissions must remain visible on the card and in the trust confirmation"
    );

    click(&app, "ShopPage::cancel-button").await;
    assert_no_text(&app, "Install Bus Departures?");
    assert!(installs.borrow().is_empty());

    click(&app, "ShopEntryCard::install-button").await;
    click(&app, "ShopPage::confirm-button").await;
    assert_eq!(
        installs.borrow().as_slice(),
        &[EXTENSION_ID.to_string()],
        "confirmation must emit the exact extension ID"
    );
    assert_no_text(&app, "Install Bus Departures?");
}

async fn installed_lifecycle_updates_models_after_store_results() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_extensions_enabled(true);
    app.set_current_page(4);

    let mut view = view_with_catalog();
    render_shop(&app, &view);

    let installs = Rc::new(RefCell::new(Vec::new()));
    let installs_sink = installs.clone();
    app.on_shop_install(move |id| installs_sink.borrow_mut().push(id.to_string()));

    let updates = Rc::new(RefCell::new(Vec::new()));
    let updates_sink = updates.clone();
    app.on_shop_update(move |id| updates_sink.borrow_mut().push(id.to_string()));

    let removes = Rc::new(RefCell::new(Vec::new()));
    let removes_sink = removes.clone();
    app.on_shop_remove(move |id| removes_sink.borrow_mut().push(id.to_string()));

    let toggles = Rc::new(RefCell::new(Vec::new()));
    let toggles_sink = toggles.clone();
    app.on_shop_set_enabled(move |id, enabled| {
        toggles_sink.borrow_mut().push((id.to_string(), enabled));
    });

    click(&app, "ShopEntryCard::install-button").await;
    click(&app, "ShopPage::confirm-button").await;
    assert_eq!(installs.borrow().as_slice(), &[EXTENSION_ID.to_string()]);

    view.request(EXTENSION_ID, ShopAction::Install, true)
        .unwrap();
    view.complete(EXTENSION_ID, ShopAction::Install, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_text(&app, "Installed");
    assert!(!has_element(&app, "ShopEntryCard::install-button"));
    assert!(has_element(&app, "ShopEntryCard::toggle-button"));
    assert!(has_element(&app, "ShopEntryCard::remove-button"));

    click(&app, "ShopEntryCard::toggle-button").await;
    view.request(EXTENSION_ID, ShopAction::Disable, true)
        .unwrap();
    view.complete(EXTENSION_ID, ShopAction::Disable, Ok(()))
        .unwrap();
    render_shop(&app, &view);
    assert_text(&app, "Disabled");

    click(&app, "ShopEntryCard::toggle-button").await;
    view.request(EXTENSION_ID, ShopAction::Enable, true)
        .unwrap();
    view.complete(EXTENSION_ID, ShopAction::Enable, Ok(()))
        .unwrap();
    render_shop(&app, &view);
    assert_eq!(
        toggles.borrow().as_slice(),
        &[
            (EXTENSION_ID.to_string(), false),
            (EXTENSION_ID.to_string(), true),
        ]
    );

    let mut update = catalog_entry();
    update.version = "1.1.0".to_string();
    view.apply_catalog(vec![update]);
    render_shop(&app, &view);

    assert_text(&app, "Update available");
    assert_text(&app, "v1.1.0");
    assert!(has_element(&app, "ShopEntryCard::update-button"));
    click(&app, "ShopEntryCard::update-button").await;
    assert_eq!(updates.borrow().as_slice(), &[EXTENSION_ID.to_string()]);

    view.request(EXTENSION_ID, ShopAction::Update, true)
        .unwrap();
    view.complete(EXTENSION_ID, ShopAction::Update, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_no_text(&app, "Update available");
    assert!(!has_element(&app, "ShopEntryCard::update-button"));
    assert_text(&app, "v1.1.0");

    click(&app, "ShopEntryCard::remove-button").await;
    assert_eq!(removes.borrow().as_slice(), &[EXTENSION_ID.to_string()]);
    view.request(EXTENSION_ID, ShopAction::Remove, true)
        .unwrap();
    view.complete(EXTENSION_ID, ShopAction::Remove, Ok(()))
        .unwrap();
    render_shop(&app, &view);

    assert_text(&app, "Available");
    assert!(has_element(&app, "ShopEntryCard::install-button"));
    assert!(!has_element(&app, "ShopEntryCard::remove-button"));
    assert!(!has_element(&app, "ShopEntryCard::toggle-button"));
}

async fn offline_state_keeps_installed_actions_and_refresh_usable() {
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
    let mut update = catalog_entry();
    update.version = "1.1.0".to_string();
    view.apply_catalog(vec![update]);
    view.mark_catalog_offline("network unreachable");
    render_shop(&app, &view);

    assert_text(&app, "Catalog offline");
    assert_text(
        &app,
        "The catalog is offline. Installed extensions keep working; install and update are unavailable.",
    );
    assert_text(&app, "network unreachable");
    assert_no_text(&app, "Update available");
    assert!(!has_element(&app, "ShopEntryCard::update-button"));
    assert!(!has_element(&app, "ShopEntryCard::install-button"));
    assert!(has_element(&app, "ShopEntryCard::toggle-button"));
    assert!(has_element(&app, "ShopEntryCard::remove-button"));

    let refreshes = Rc::new(Cell::new(0));
    let refreshes_counter = refreshes.clone();
    app.on_shop_refresh(move || refreshes_counter.set(refreshes_counter.get() + 1));

    let removes = Rc::new(RefCell::new(Vec::new()));
    let removes_sink = removes.clone();
    app.on_shop_remove(move |id| removes_sink.borrow_mut().push(id.to_string()));

    let toggles = Rc::new(RefCell::new(Vec::new()));
    let toggles_sink = toggles.clone();
    app.on_shop_set_enabled(move |id, enabled| {
        toggles_sink.borrow_mut().push((id.to_string(), enabled));
    });

    click(&app, "ShopPage::refresh-button").await;
    click(&app, "ShopEntryCard::toggle-button").await;
    click(&app, "ShopEntryCard::remove-button").await;

    assert_eq!(refreshes.get(), 1);
    assert_eq!(
        toggles.borrow().as_slice(),
        &[(EXTENSION_ID.to_string(), false)]
    );
    assert_eq!(removes.borrow().as_slice(), &[EXTENSION_ID.to_string()]);
}

async fn failed_install_renders_error_and_keeps_retry_available() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_extensions_enabled(true);
    app.set_current_page(4);

    let mut view = view_with_catalog();
    view.request(EXTENSION_ID, ShopAction::Install, true)
        .unwrap();
    view.complete(
        EXTENSION_ID,
        ShopAction::Install,
        Err("checksum mismatch".to_string()),
    )
    .unwrap();
    render_shop(&app, &view);

    assert_text(&app, "Error");
    assert_text(&app, "checksum mismatch");
    assert!(has_element(&app, "ShopEntryCard::install-button"));
    assert!(!has_element(&app, "ShopEntryCard::remove-button"));
    assert!(!has_element(&app, "ShopEntryCard::toggle-button"));

    let installs = Rc::new(RefCell::new(Vec::new()));
    let installs_sink = installs.clone();
    app.on_shop_install(move |id| installs_sink.borrow_mut().push(id.to_string()));

    click(&app, "ShopEntryCard::install-button").await;
    assert_text(&app, "Install Bus Departures?");
    assert!(installs.borrow().is_empty());
    click(&app, "ShopPage::cancel-button").await;
    assert!(installs.borrow().is_empty());
}

#[test]
fn shop_page_flows_cover_install_update_remove_offline_and_errors() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        install_flow_shows_permissions_and_requires_trust_confirmation().await;
        installed_lifecycle_updates_models_after_store_results().await;
        offline_state_keeps_installed_actions_and_refresh_usable().await;
        failed_install_renders_error_and_keeps_retry_available().await;
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
