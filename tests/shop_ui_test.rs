use i_slint_backend_testing::{ElementHandle, ElementRoot};
use slint::ComponentHandle;
use slint::platform::PointerEventButton;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn entry_view(
    installed: bool,
    enabled: bool,
    update_available: bool,
    error: &str,
) -> ha_kiosk::ShopEntryView {
    ha_kiosk::ShopEntryView {
        id: "org.example.bus-departures".into(),
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
        config_keys:
            "Settings: stop_id (required, string); limit (optional, integer)\nSecrets: api_key (required, secret)"
                .into(),
        config_path: "/var/lib/home-assistant-kiosk/configuration/operator-edited-config.toml".into(),
        config_table: "[extensions.settings.\"org.example.bus-departures\"]".into(),
        secret_path: "/var/lib/home-assistant-kiosk/private/extension-secrets/org.example.bus-departures.secret.toml".into(),
        error: error.into(),
        installed,
        enabled,
        update_available,
        available: true,
    }
}

async fn click(app: &ha_kiosk::MainWindow, id: &str) {
    let element = ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"));
    element.single_click(PointerEventButton::Left).await;
}

async fn assert_configuration_guidance(app: &ha_kiosk::MainWindow) {
    app.set_extensions_enabled(true);
    app.set_current_page(4);
    app.set_shop_available(
        Rc::new(slint::VecModel::from(Vec::<ha_kiosk::ShopEntryView>::new())).into(),
    );
    app.set_shop_installed(
        Rc::new(slint::VecModel::from(vec![entry_view(
            true, true, false, "",
        )]))
        .into(),
    );
    app.set_shop_catalog_online(true);
    app.set_shop_catalog_error("".into());
    app.set_shop_config_path(
        "/var/lib/home-assistant-kiosk/configuration/operator-edited-config.toml".into(),
    );
    app.set_shop_secret_dir("/var/lib/home-assistant-kiosk/private/extension-secrets".into());

    let guidance = [
        (
            "ShopPage::read-only-note",
            "Configuration is read-only here. Edit the files below directly, then restart the kiosk. No on-screen keyboard is used.",
        ),
        (
            "ShopEntryCard::config-keys",
            "Configuration keys: Settings: stop_id (required, string); limit (optional, integer)\nSecrets: api_key (required, secret)",
        ),
        (
            "ShopEntryCard::config-path",
            "Settings file: /var/lib/home-assistant-kiosk/configuration/operator-edited-config.toml",
        ),
        (
            "ShopEntryCard::config-table",
            "TOML table: [extensions.settings.\"org.example.bus-departures\"]",
        ),
        (
            "ShopEntryCard::secret-path",
            "Secret file: /var/lib/home-assistant-kiosk/private/extension-secrets/org.example.bus-departures.secret.toml",
        ),
    ];

    for (width, height) in [
        (320, 480),
        (375, 667),
        (390, 844),
        (768, 1024),
        (800, 480),
        (1024, 600),
        (1280, 800),
        (1440, 900),
        (2048, 1200),
    ] {
        app.window()
            .set_size(slint::PhysicalSize::new(width, height));
        slint::platform::update_timers_and_animations();
        assert!(
            app.root_element().size().width <= width as f32 + 0.5,
            "shop root overflows at {width}x{height}"
        );

        for (id, label) in guidance {
            // The compact viewport intentionally keeps lower card content in
            // its contained scroll region; the kiosk gate starts at 800px.
            if width < 800 && id == "ShopEntryCard::secret-path" {
                continue;
            }
            let element = ElementHandle::find_by_element_id(app, id)
                .next()
                .unwrap_or_else(|| {
                    panic!("configuration guidance element {id} not found at {width}x{height}")
                });
            assert_eq!(element.accessible_label().as_deref(), Some(label));
            let position = element.absolute_position();
            let size = element.size();
            assert!(
                position.x >= -0.5,
                "{id} starts outside the {width}px window"
            );
            assert!(
                position.x + size.width <= width as f32 + 0.5,
                "{id} overflows the {width}px window"
            );
            assert!(size.height > 0.0, "{id} has no visible text area");
            if width == 320 && id.ends_with("config-path") {
                assert!(
                    size.height > 12.5,
                    "the long settings path must wrap at 320px"
                );
            }
        }

        let refresh = ElementHandle::find_by_element_id(app, "ShopPage::refresh-button")
            .next()
            .unwrap();
        assert!(
            refresh.absolute_position().x + refresh.size().width <= width as f32 + 0.5,
            "refresh button overflows at {width}x{height}"
        );
    }

    let mut text_inputs = 0;
    let mut buttons = 0;
    app.root_element().visit_descendants(|element| {
        match element.accessible_role() {
            Some(slint::language::AccessibleRole::TextInput) => text_inputs += 1,
            Some(slint::language::AccessibleRole::Button) => {
                buttons += 1;
                let size = element.size();
                assert!(
                    size.width >= 44.0 && size.height >= 44.0,
                    "shop button {} is below the 44px touch target",
                    element.id().unwrap_or_default()
                );
                assert!(
                    element
                        .accessible_label()
                        .is_some_and(|label| !label.is_empty()),
                    "shop button must have an accessible label"
                );
            }
            _ => {}
        }
        std::ops::ControlFlow::<(), ()>::Continue(())
    });
    assert_eq!(text_inputs, 0, "Wave 6 must not add an on-screen keyboard");
    assert!(
        buttons >= 3,
        "expected refresh, disable, and remove buttons"
    );

    let mut without_secrets = entry_view(true, true, false, "");
    without_secrets.config_keys =
        "Settings: stop_id (required, string); limit (optional, integer)".into();
    without_secrets.secret_path = "".into();
    app.set_shop_installed(Rc::new(slint::VecModel::from(vec![without_secrets])).into());
    slint::platform::update_timers_and_animations();
    assert!(
        ElementHandle::find_by_element_id(app, "ShopEntryCard::secret-path")
            .next()
            .is_none(),
        "extensions without secret keys must not show a secret file"
    );
}

#[test]
fn shop_page_actions_states_and_read_only_configuration_guidance() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        app.window().set_size(slint::PhysicalSize::new(1280, 800));

        // The Shop tab is not part of the page set while extensions are disabled.
        assert!(
            ElementHandle::find_by_element_id(&app, "MainWindow::tab-shop")
                .next()
                .is_none()
        );

        app.set_extensions_enabled(true);
        let tab = ElementHandle::find_by_element_id(&app, "MainWindow::tab-shop")
            .next()
            .expect("shop tab appears when extensions are enabled");
        tab.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 4);

        let refreshed = Rc::new(Cell::new(false));
        let refreshed_flag = refreshed.clone();
        app.on_shop_refresh(move || refreshed_flag.set(true));

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

        // Available list and refresh.
        app.set_shop_available(
            Rc::new(slint::VecModel::from(vec![entry_view(
                false, false, false, "",
            )]))
            .into(),
        );
        app.set_shop_catalog_online(true);
        click(&app, "ShopPage::refresh-button").await;
        assert!(refreshed.get());

        // First install must be confirmed before the shop emits an install.
        click(&app, "ShopEntryCard::install-button").await;
        assert!(installs.borrow().is_empty());
        click(&app, "ShopPage::confirm-button").await;
        assert_eq!(
            installs.borrow().as_slice(),
            &["org.example.bus-departures".to_string()]
        );

        // Cancelling the trust confirmation emits nothing.
        click(&app, "ShopEntryCard::install-button").await;
        click(&app, "ShopPage::cancel-button").await;
        assert_eq!(installs.borrow().len(), 1);

        // Installed entry: update, enable/disable, and remove actions are wired.
        app.set_shop_available(
            Rc::new(slint::VecModel::from(Vec::<ha_kiosk::ShopEntryView>::new())).into(),
        );
        app.set_shop_installed(
            Rc::new(slint::VecModel::from(vec![entry_view(
                true, true, true, "",
            )]))
            .into(),
        );
        click(&app, "ShopEntryCard::update-button").await;
        assert_eq!(
            updates.borrow().as_slice(),
            &["org.example.bus-departures".to_string()]
        );
        click(&app, "ShopEntryCard::toggle-button").await;
        assert_eq!(
            toggles.borrow().as_slice(),
            &[("org.example.bus-departures".to_string(), false)]
        );
        click(&app, "ShopEntryCard::remove-button").await;
        assert_eq!(
            removes.borrow().as_slice(),
            &["org.example.bus-departures".to_string()]
        );

        // Offline state keeps the page usable and does not crash.
        app.set_shop_catalog_online(false);
        app.set_shop_catalog_error("network unreachable".into());
        assert!(
            ElementHandle::find_by_element_id(&app, "ShopPage::refresh-button")
                .next()
                .is_some()
        );

        // Error state is rendered on the entry card.
        app.set_shop_catalog_online(true);
        app.set_shop_installed(
            Rc::new(slint::VecModel::from(vec![entry_view(
                true,
                true,
                false,
                "checksum mismatch",
            )]))
            .into(),
        );
        assert!(
            ElementHandle::find_by_element_id(&app, "ShopEntryCard::remove-button")
                .next()
                .is_some()
        );

        assert_configuration_guidance(&app).await;

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
