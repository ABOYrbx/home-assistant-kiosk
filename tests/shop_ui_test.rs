use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;
use slint::language::{AccessibleLiveness, AccessibleRole};
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
        config_status: if installed {
            "Configured".into()
        } else {
            "Configuration required".into()
        },
        config_keys: "stop_id (required), limit".into(),
        config_path:
            "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]".into(),
        secret_path: "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml".into(),
        error: error.into(),
        installed,
        installed_version: if installed {
            (if update_available { "1.0.0" } else { "1.1.0" }).into()
        } else {
            "".into()
        },
        metadata_available: true,
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

fn element(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"))
}

fn assert_inside_horizontally(element: &ElementHandle, width: f32, label: &str) {
    let position = element.absolute_position();
    let size = element.size();
    assert!(
        position.x >= -0.5 && position.x + size.width <= width + 0.5,
        "{label} overflows {width}px: x={} width={}",
        position.x,
        size.width
    );
}

#[test]
fn shop_page_navigation_actions_offline_and_error_states() {
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

        // Error state is rendered on the entry card and remains actionable offline.
        app.set_shop_catalog_online(false);
        app.set_shop_catalog_error("The catalog could not be reached.".into());
        let mut installed = entry_view(true, true, true, "checksum mismatch");
        installed.name = "Community transit departures with an exceptionally long display name".into();
        installed.author = "Example community extension author".into();
        installed.description = "Shows upcoming departures with detailed status remarks, platform changes, cancellations, and a long explanation that must wrap inside the card.".into();
        installed.permissions = "Network: api.example.com; Home Assistant: sensor.example, sensor.example_secondary".into();
        installed.config_keys = "stop_id (required), api_key (required), limit, platform_filter".into();
        installed.config_path = "/etc/ha-kiosk/config.toml [extensions.settings.\"org.example.bus-departures\"]".into();
        installed.secret_path = "/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml".into();
        installed.error = "The package checksum did not match the signed catalog entry.".into();
        app.set_shop_installed(Rc::new(slint::VecModel::from(vec![installed])).into());

        for (width, height) in [
            (375, 812),
            (390, 844),
            (800, 480),
            (1024, 600),
            (1280, 800),
            (1440, 900),
            (2048, 1200),
        ] {
            app.window()
                .set_size(slint::PhysicalSize::new(width, height));
            slint::platform::update_timers_and_animations();

            let page = element(&app, "MainWindow::shop-page");
            let scroll = element(&app, "ShopPage::shop-scroll");
            let card = element(&app, "ShopPage::entry-card");
            assert_inside_horizontally(&page, width as f32, "Shop page");
            assert_inside_horizontally(&scroll, width as f32, "Shop scroll area");
            assert_inside_horizontally(&card, width as f32, "extension card");
            assert_inside_horizontally(
                &element(&app, "ShopPage::refresh-button"),
                width as f32,
                "refresh button",
            );
            assert!(element(&app, "ShopEntryCard::entry-version").size().width > 0.0);
            assert!(element(&app, "ShopEntryCard::entry-config-status").size().width > 0.0);
            let error = element(&app, "ShopEntryCard::entry-error");
            assert!(error.size().width > 0.0);
            assert_eq!(
                error.accessible_live_region(),
                Some(AccessibleLiveness::Assertive)
            );
            for (id, label) in [
                ("ShopPage::refresh-button", "Refresh"),
                ("ShopEntryCard::update-button", "Update"),
                ("ShopEntryCard::toggle-button", "Disable"),
                ("ShopEntryCard::remove-button", "Remove"),
            ] {
                let button = element(&app, id);
                assert_eq!(button.accessible_role(), Some(AccessibleRole::Button));
                assert_eq!(button.accessible_label().as_deref(), Some(label));
                assert!(button.size().width >= 44.0 && button.size().height >= 44.0);
            }
        }

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
