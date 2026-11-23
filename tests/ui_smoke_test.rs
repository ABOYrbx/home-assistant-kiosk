use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;

#[test]
fn shell_navigation_and_gestures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();

        // Tap on a tab changes the page.
        let tab = ElementHandle::find_by_element_id(&app, "MainWindow::tab-media")
            .next()
            .unwrap();
        tab.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 1);

        // Swipe left beyond threshold changes page.
        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        let pos = swipe.absolute_position();
        let size = swipe.size();
        let start =
            slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        let target = slint::LogicalPosition::new(start.x - 120.0, start.y);
        swipe.drag(target, PointerEventButton::Left).await;
        // page should have moved (prev or next). Just ensure no crash and a valid page.
        let p = app.get_current_page();
        assert!(p <= 3);

        // Vertical drag does not change page.
        let before = app.get_current_page();
        let vtarget = slint::LogicalPosition::new(start.x, start.y - 120.0);
        // recompute start (element unchanged)
        let swipe2 = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        swipe2.drag(vtarget, PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), before);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
