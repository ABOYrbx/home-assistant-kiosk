use ha_kiosk::ShopEntryView;
use i_slint_backend_testing::{ElementHandle, TestingBackend, TestingBackendOptions};
use slint::language::AccessibleRole;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, ModelRc, PhysicalSize, VecModel};
use std::cell::{Cell, RefCell};
use std::env;
use std::rc::Rc;

const EXTENSION_ID: &str = "org.example.bus-departures";

fn entry_view(
    installed: bool,
    enabled: bool,
    update_available: bool,
    error: &str,
) -> ShopEntryView {
    ShopEntryView {
        id: EXTENSION_ID.into(),
        name: "Bus Departures".into(),
        version: "1.1.0".into(),
        author: "Example Author".into(),
        license: "MIT".into(),
        description: "Shows upcoming bus departures for a configured stop.".into(),
        status: if update_available {
            "Update available".into()
        } else if installed {
            "Installed".into()
        } else {
            "Available".into()
        },
        permissions: "Network: api.example.com; Home Assistant: sensor.example".into(),
        config_keys: "stop_id (required), limit".into(),
        config_status: "Configuration required".into(),
        config_path:
            "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]".into(),
        secret_path: "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml".into(),
        error: error.into(),
        installed,
        enabled,
        update_available,
        available: true,
    }
}

fn model(entries: Vec<ShopEntryView>) -> ModelRc<ShopEntryView> {
    Rc::new(VecModel::from(entries)).into()
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

fn labels_within(app: &ha_kiosk::MainWindow, id: &str) -> Vec<String> {
    let mut labels = Vec::new();
    element(app, id).visit_descendants(|element| {
        if let Some(label) = element.accessible_label() {
            labels.push(label.to_string());
        }
        std::ops::ControlFlow::<(), ()>::Continue(())
    });
    labels
}

async fn click(app: &ha_kiosk::MainWindow, id: &str) {
    element(app, id)
        .single_click(PointerEventButton::Left)
        .await;
}

fn resize(app: &ha_kiosk::MainWindow, width: u32, height: u32) {
    let scale = app.window().scale_factor();
    app.window().set_size(PhysicalSize::new(
        (width as f32 * scale) as u32,
        (height as f32 * scale) as u32,
    ));
    slint::platform::update_timers_and_animations();
}

fn capture_snapshot(app: &ha_kiosk::MainWindow, state: &str, width: u32, height: u32) {
    let Ok(directory) = env::var("SHOP_UI_SCREENSHOT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&directory).expect("create screenshot directory");
    slint::platform::update_timers_and_animations();
    let snapshot = app.window().take_snapshot().expect("render Shop snapshot");
    let path = std::path::Path::new(&directory).join(format!("shop-{state}-{width}x{height}.png"));
    image::save_buffer(
        &path,
        snapshot.as_bytes(),
        snapshot.width(),
        snapshot.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("save Shop snapshot");
}

fn assert_inside(element: &ElementHandle, label: &str, width: f32) {
    let position = element.absolute_position();
    let size = element.size();
    assert!(
        position.x >= -0.5 && position.x + size.width <= width + 0.5,
        "{label} is outside the {width}px viewport: x={} width={}",
        position.x,
        size.width
    );
}

fn assert_touch_target(element: &ElementHandle, label: &str) {
    let size = element.size();
    assert!(
        size.width >= 44.0 && size.height >= 44.0,
        "{label} is {}x{}, below the 44x44 minimum",
        size.width,
        size.height
    );
}

async fn run_shop_page_scenarios() {
    let app = ha_kiosk::MainWindow::new().unwrap();
    resize(&app, 390, 844);

    assert!(!has_element(&app, "MainWindow::tab-shop"));
    app.set_extensions_enabled(true);
    click(&app, "MainWindow::tab-shop").await;
    assert_eq!(app.get_current_page(), 4);
    assert!(has_element(&app, "MainWindow::shop-page"));
    assert!(has_element(&app, "ShopPage::installed-list"));
    assert!(has_element(&app, "ShopPage::available-list"));

    // The first frame must not claim that an empty catalog is a loaded catalog.
    assert_text(&app, "Loading extensions…");
    assert_no_text(
        &app,
        "No extensions installed. Choose one from the available list.",
    );
    capture_snapshot(&app, "loading", 390, 844);
    app.set_shop_loading(false);

    let installs = Rc::new(RefCell::new(Vec::new()));
    let installs_sink = installs.clone();
    app.on_shop_install(move |id, confirmed| {
        assert!(
            confirmed,
            "an install may reach the controller only after the trust dialog"
        );
        installs_sink.borrow_mut().push(id.to_string());
    });

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

    let refreshes = Rc::new(Cell::new(0));
    let refreshes_counter = refreshes.clone();
    app.on_shop_refresh(move || refreshes_counter.set(refreshes_counter.get() + 1));

    app.set_shop_available(model(vec![entry_view(false, false, false, "")]));
    app.set_shop_catalog_online(true);
    assert_text(&app, "Bus Departures");
    assert_text(&app, "Configuration: stop_id (required), limit");
    assert_text(&app, "Configuration required");
    assert_text(
        &app,
        "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]",
    );
    assert_text(
        &app,
        "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml",
    );

    // The primary action is a real accessible button with a specific label.
    let install = element(&app, "ShopEntryCard::install-button");
    assert_eq!(install.accessible_role(), Some(AccessibleRole::Button));
    assert_eq!(
        install.accessible_label().as_deref(),
        Some("Install Bus Departures")
    );
    assert_touch_target(&install, "Install button");
    install.invoke_accessible_default_action();
    assert_text(&app, "Install Bus Departures?");
    capture_snapshot(&app, "confirm", 390, 844);
    assert_text(
        &app,
        "Permissions: Network: api.example.com; Home Assistant: sensor.example",
    );
    assert!(installs.borrow().is_empty());
    click(&app, "ShopPage::cancel-button").await;
    assert_no_text(&app, "Install Bus Departures?");
    assert!(installs.borrow().is_empty());

    // Confirming the trust dialog is the only path that submits an install.
    install.invoke_accessible_default_action();
    assert_text(&app, "Install Bus Departures?");
    click(&app, "ShopPage::confirm-button").await;
    assert_no_text(&app, "Install Bus Departures?");
    assert_eq!(installs.borrow().as_slice(), &[EXTENSION_ID.to_string()]);

    // A pending operation disables every competing action and cannot double-submit.
    app.set_shop_pending_id(EXTENSION_ID.into());
    assert_eq!(install.accessible_enabled(), Some(false));
    assert_eq!(
        element(&app, "ShopPage::refresh-button").accessible_enabled(),
        Some(false)
    );
    let before_pending_click = installs.borrow().len();
    install.single_click(PointerEventButton::Left).await;
    assert_eq!(installs.borrow().len(), before_pending_click);
    app.set_shop_pending_id("".into());
    assert_eq!(install.accessible_enabled(), Some(true));

    // Installed and available entries remain in separate lists.
    let mut installed = entry_view(true, true, true, "");
    installed.id = "org.example.weather".into();
    installed.name = "Installed Weather".into();
    let mut available = entry_view(false, false, false, "");
    available.id = "org.example.transit".into();
    available.name = "Available Transit".into();
    app.set_shop_installed(model(vec![installed]));
    app.set_shop_available(model(vec![available]));
    let installed_labels = labels_within(&app, "ShopPage::installed-list");
    let available_labels = labels_within(&app, "ShopPage::available-list");
    assert!(
        installed_labels
            .iter()
            .any(|label| label == "Installed Weather")
    );
    assert!(
        !installed_labels
            .iter()
            .any(|label| label == "Available Transit")
    );
    assert!(
        available_labels
            .iter()
            .any(|label| label == "Available Transit")
    );
    assert!(
        !available_labels
            .iter()
            .any(|label| label == "Installed Weather")
    );

    let update = element(&app, "ShopEntryCard::update-button");
    assert_eq!(
        update.accessible_label().as_deref(),
        Some("Update Installed Weather")
    );
    update.invoke_accessible_default_action();
    assert_eq!(
        updates.borrow().as_slice(),
        &["org.example.weather".to_string()]
    );
    click(&app, "ShopEntryCard::toggle-button").await;
    click(&app, "ShopEntryCard::remove-button").await;
    assert_eq!(
        toggles.borrow().as_slice(),
        &[("org.example.weather".to_string(), false)]
    );
    assert_eq!(
        removes.borrow().as_slice(),
        &["org.example.weather".to_string()]
    );

    // Offline state keeps local actions and retry available.
    app.set_shop_catalog_online(false);
    app.set_shop_catalog_error("network unreachable".into());
    assert_text(&app, "Catalog offline");
    assert_text(&app, "network unreachable");
    capture_snapshot(&app, "offline", 800, 480);
    assert!(has_element(&app, "ShopPage::refresh-button"));
    assert!(has_element(&app, "ShopEntryCard::toggle-button"));
    assert!(has_element(&app, "ShopEntryCard::remove-button"));
    click(&app, "ShopPage::refresh-button").await;
    assert_eq!(refreshes.get(), 1);

    // Entry failures are visible and also reach the Status page.
    let mut failed = entry_view(true, true, false, "checksum mismatch");
    failed.name = "Failed Extension".into();
    app.set_shop_installed(model(vec![failed.clone()]));
    app.set_shop_catalog_online(true);
    assert_text(&app, "checksum mismatch");
    capture_snapshot(&app, "error", 800, 480);
    app.set_extension_errors(
        Rc::new(VecModel::from(vec![slint::SharedString::from(
            "Failed Extension: checksum mismatch",
        )]))
        .into(),
    );
    click(&app, "MainWindow::tab-status").await;
    let status_error = element(&app, "StatusPage::extension-error");
    assert_eq!(
        status_error.accessible_label().as_deref(),
        Some("Failed Extension: checksum mismatch")
    );
    click(&app, "MainWindow::tab-shop").await;

    // Long strings and many cards reflow inside one vertical scroll region.
    let mut many = Vec::new();
    for index in 0..12 {
        let mut entry = entry_view(false, false, false, "");
        entry.id = format!("org.example.extension-{index}").into();
        entry.name = format!("Community extension {index} with a deliberately long name").into();
        entry.description = format!(
            "A long description for extension {index} that must wrap instead of widening the page or hiding the install action."
        )
        .into();
        many.push(entry);
    }
    app.set_shop_installed(model(Vec::new()));
    app.set_shop_available(model(many));

    for (width, height) in [
        (390u32, 844u32),
        (800u32, 480u32),
        (1024u32, 600u32),
        (1280u32, 800u32),
        (2048u32, 1200u32),
    ] {
        resize(&app, width, height);
        let page = element(&app, "MainWindow::shop-page");
        page.visit_descendants(|element| {
            assert_inside(&element, "Shop content", width as f32);
            if let Some(role) = element.accessible_role()
                && role == AccessibleRole::Button
            {
                assert_touch_target(&element, "Shop button");
            }
            std::ops::ControlFlow::<(), ()>::Continue(())
        });
        assert_inside(
            &element(&app, "MainWindow::tab-shop"),
            "Shop tab",
            width as f32,
        );
        let scroll = element(&app, "ShopPage::shop-scroll");
        let content = element(&app, "ShopPage::shop-content");
        assert_inside(&scroll, "Shop scroll region", width as f32);
        assert!(
            content.size().width <= scroll.size().width + 0.5,
            "Shop content must not scroll horizontally"
        );
        assert!(
            scroll.size().width <= 1440.0 + 0.5,
            "wide Shop content must stay capped at 1440px"
        );
        capture_snapshot(&app, "populated", width, height);
    }

    slint::quit_event_loop().unwrap();
}

#[test]
fn shop_page_states_actions_accessibility_and_layout() {
    slint::platform::set_platform(Box::new(TestingBackend::new(TestingBackendOptions {
        mock_time: false,
        threading: true,
        renderer_name: Some("software".into()),
    })))
    .expect("initialize the Slint testing backend");

    slint::spawn_local(async move {
        run_shop_page_scenarios().await;
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
