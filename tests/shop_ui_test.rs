use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;
use slint::language::AccessibleRole;
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
        config_keys: "stop_id (required), limit".into(),
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

async fn click(app: &ha_kiosk::MainWindow, id: &str) {
    let element = ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"));
    element.single_click(PointerEventButton::Left).await;
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

        // A pending store action disables further install input.
        app.set_shop_pending_id("org.example.bus-departures".into());
        let install = ElementHandle::find_by_element_id(&app, "ShopEntryCard::install-button")
            .next()
            .expect("install button");
        assert_eq!(install.accessible_role(), Some(AccessibleRole::Button));
        assert_eq!(install.accessible_enabled(), Some(false));
        click(&app, "ShopEntryCard::install-button").await;
        assert_eq!(installs.borrow().len(), 1);
        app.set_shop_pending_id("".into());

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

        // Pending input is disabled for every installed-entry action.
        app.set_shop_pending_id("org.example.bus-departures".into());
        for id in [
            "ShopEntryCard::update-button",
            "ShopEntryCard::toggle-button",
            "ShopEntryCard::remove-button",
        ] {
            let button = ElementHandle::find_by_element_id(&app, id)
                .next()
                .unwrap_or_else(|| panic!("element {id} not found"));
            assert_eq!(button.accessible_enabled(), Some(false));
        }
        click(&app, "ShopEntryCard::update-button").await;
        click(&app, "ShopEntryCard::toggle-button").await;
        click(&app, "ShopEntryCard::remove-button").await;
        assert_eq!(updates.borrow().len(), 1);
        assert_eq!(toggles.borrow().len(), 1);
        assert_eq!(removes.borrow().len(), 1);
        app.set_shop_pending_id("".into());

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

        // Maximum-size permission lists must remain reviewable at the primary
        // kiosk resolution, with both confirmation actions still reachable.
        app.set_shop_installed(
            Rc::new(slint::VecModel::from(Vec::<ha_kiosk::ShopEntryView>::new())).into(),
        );
        let mut permission_heavy = entry_view(false, false, false, "");
        permission_heavy.permissions = (0..32)
            .map(|index| format!("Network: api{index}.example.com"))
            .chain((0..64).map(|index| format!("Home Assistant: sensor.entity_{index}")))
            .collect::<Vec<_>>()
            .join("; ")
            .into();
        app.set_shop_available(Rc::new(slint::VecModel::from(vec![permission_heavy])).into());
        let install = ElementHandle::find_by_element_id(&app, "ShopEntryCard::install-button")
            .next()
            .expect("install button");
        assert_eq!(
            install.accessible_label().as_deref(),
            Some("Bus Departures: Install")
        );
        click(&app, "ShopEntryCard::install-button").await;
        app.window().set_size(slint::PhysicalSize::new(800, 480));
        for id in ["ShopPage::cancel-button", "ShopPage::confirm-button"] {
            let button = ElementHandle::find_by_element_id(&app, id)
                .next()
                .unwrap_or_else(|| panic!("element {id} not found"));
            let position = button.absolute_position();
            let size = button.size();
            assert!(size.width >= 44.0 && size.height >= 44.0);
            assert!(position.y >= 0.0);
            assert!(
                position.y + size.height <= 480.0,
                "{id} is outside the 800x480 viewport: {position:?} {size:?}"
            );
        }

        // A catalog update invalidates an open confirmation.
        app.set_shop_confirm_id("".into());
        assert!(
            ElementHandle::find_by_element_id(&app, "ShopPage::confirm-button")
                .next()
                .is_none()
        );

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
