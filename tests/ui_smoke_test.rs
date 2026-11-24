use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, LogicalPosition};

fn element_center(element: &ElementHandle) -> LogicalPosition {
    let position = element.absolute_position();
    let size = element.size();
    LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    )
}

fn find(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element {id} not found"))
}

fn connect_swipe_handlers(app: &ha_kiosk::MainWindow) {
    let weak_next = app.as_weak();
    app.on_swipe_next(move || {
        if let Some(window) = weak_next.upgrade() {
            let page = window.get_current_page();
            window.set_current_page(if page >= 3 { 0 } else { page + 1 });
        }
    });

    let weak_prev = app.as_weak();
    app.on_swipe_prev(move || {
        if let Some(window) = weak_prev.upgrade() {
            let page = window.get_current_page();
            window.set_current_page(if page == 0 { 3 } else { page - 1 });
        }
    });
}

#[test]
fn shell_navigation_and_gestures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        // Pin the 800 x 480 gate layout so the relative swipe threshold is
        // deterministic.
        app.window().set_size(slint::PhysicalSize::new(800, 480));
        connect_swipe_handlers(&app);

        // Tap on a tab changes the page.
        let tab_media = find(&app, "MainWindow::tab-media");
        tab_media.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 1);

        // A horizontal swipe from the non-interactive page background changes
        // the page.
        let background = find(&app, "MainWindow::swipe-area");
        let background_center = element_center(&background);
        background
            .drag(
                LogicalPosition::new(background_center.x - 220.0, background_center.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 2);

        // A vertical drag on the background is not a page swipe.
        let background = find(&app, "MainWindow::swipe-area");
        background
            .drag(
                LogicalPosition::new(background_center.x, background_center.y - 220.0),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 2);

        // A gesture that starts on a button belongs to that button. The tab
        // bar is interactive, so the page must not change even though the
        // movement is horizontal and beyond the swipe threshold.
        let tab_media = find(&app, "MainWindow::tab-media");
        let tab_center = element_center(&tab_media);
        tab_media
            .drag(
                LogicalPosition::new(tab_center.x + 220.0, tab_center.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(
            app.get_current_page(),
            2,
            "a drag starting on a button must not change the page"
        );

        // The reserved edge zones are swipe targets even though they sit above
        // the page: page content is inset, so no control is ever underneath.
        let left_edge = find(&app, "MainWindow::edge-swipe-left");
        let left_edge_center = element_center(&left_edge);
        left_edge
            .drag(
                LogicalPosition::new(left_edge_center.x + 220.0, left_edge_center.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 1);

        let right_edge = find(&app, "MainWindow::edge-swipe-right");
        let right_edge_center = element_center(&right_edge);
        right_edge
            .drag(
                LogicalPosition::new(right_edge_center.x - 220.0, right_edge_center.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 2);

        // A short movement inside an edge zone stays below the threshold.
        let left_edge = find(&app, "MainWindow::edge-swipe-left");
        let left_edge_center = element_center(&left_edge);
        left_edge
            .drag(
                LogicalPosition::new(left_edge_center.x + 20.0, left_edge_center.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 2);

        // The tab bar remains the reliable fallback from any page.
        let tab_status = find(&app, "MainWindow::tab-status");
        tab_status.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 3);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
