//! Smoke test for the consolidated shell: the four v1 tabs, their touch
//! targets, and the optional Shop tab.

use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;
use slint::platform::PointerEventButton;

fn find(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element id {id} not found"))
}

#[test]
fn shell_exposes_four_touch_friendly_tabs_and_an_optional_shop_tab() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        app.window().set_size(slint::PhysicalSize::new(800, 480));
        slint::platform::update_timers_and_animations();

        for id in [
            "MainWindow::tab-overview",
            "MainWindow::tab-media",
            "MainWindow::tab-agenda",
            "MainWindow::tab-status",
        ] {
            let tab = find(&app, id);
            let size = tab.size();
            assert!(
                size.width >= 44.0 && size.height >= 44.0,
                "{id} is below the 44x44 logical-pixel minimum: {size:?}"
            );
        }

        // The Shop is a fifth page and only exists when extensions are enabled.
        assert!(
            ElementHandle::find_by_element_id(&app, "MainWindow::tab-shop")
                .next()
                .is_none()
        );
        app.set_extensions_enabled(true);
        slint::platform::update_timers_and_animations();
        let shop = find(&app, "MainWindow::tab-shop");
        let size = shop.size();
        assert!(size.width >= 44.0 && size.height >= 44.0);
        shop.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 4);

        app.set_extensions_enabled(false);
        slint::platform::update_timers_and_animations();

        // Every tab reaches the same page property the controller reads.
        for (id, page) in [
            ("MainWindow::tab-overview", 0),
            ("MainWindow::tab-media", 1),
            ("MainWindow::tab-agenda", 2),
            ("MainWindow::tab-status", 3),
        ] {
            find(&app, id).single_click(PointerEventButton::Left).await;
            assert_eq!(app.get_current_page(), page, "{id} must select {page}");
        }

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
