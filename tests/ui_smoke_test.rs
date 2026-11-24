use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;

/// Drag the pointer from the centre of the page host by the given logical delta.
async fn drag_page(app: &ha_kiosk::MainWindow, dx: f32, dy: f32) {
    let swipe = ElementHandle::find_by_element_id(app, "MainWindow::swipe-area")
        .next()
        .unwrap();
    let pos = swipe.absolute_position();
    let size = swipe.size();
    let start = slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    let target = slint::LogicalPosition::new(start.x + dx, start.y + dy);
    swipe.drag(target, PointerEventButton::Left).await;
}

#[test]
fn shell_navigation_and_gestures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();

        // The testing backend uses an 800 x 600 window, so the swipe
        // threshold is max(60px, 20% of 800px) = 160px.
        assert_eq!(app.get_current_page(), 0);

        // 120px is more than the 60px floor but below 20% of the width.
        drag_page(&app, -120.0, 0.0).await;
        assert_eq!(app.get_current_page(), 0);

        // Vertical movement does not change the page.
        drag_page(&app, 0.0, -220.0).await;
        assert_eq!(app.get_current_page(), 0);

        // A diagonal drag locks to the dominant (vertical) axis even though
        // the horizontal travel is above the page-swipe threshold.
        drag_page(&app, -220.0, -260.0).await;
        assert_eq!(app.get_current_page(), 0);

        // A horizontal swipe past the threshold moves to the next page.
        drag_page(&app, -220.0, 0.0).await;
        assert_eq!(app.get_current_page(), 1);

        // A horizontal swipe the other way moves back.
        drag_page(&app, 220.0, 0.0).await;
        assert_eq!(app.get_current_page(), 0);

        // The tab bar remains the reliable fallback.
        let tab = ElementHandle::find_by_element_id(&app, "MainWindow::tab-agenda")
            .next()
            .unwrap();
        tab.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 2);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
