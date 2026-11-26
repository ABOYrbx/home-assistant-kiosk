//! Shell interaction on the consolidated Slint window: tab selection and
//! swipe arbitration on the background area.

use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, LogicalPosition};
use std::cell::Cell;
use std::rc::Rc;

fn element(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("element id {id} not found"))
}

#[test]
fn tabs_select_pages_and_background_swipes_report_the_callback() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        app.window().set_size(slint::PhysicalSize::new(800, 480));
        assert_eq!(app.get_current_page(), 0);

        for (id, page) in [
            ("MainWindow::tab-media", 1),
            ("MainWindow::tab-agenda", 2),
            ("MainWindow::tab-status", 3),
            ("MainWindow::tab-overview", 0),
        ] {
            element(&app, id)
                .single_click(PointerEventButton::Left)
                .await;
            assert_eq!(app.get_current_page(), page, "{id} must select page {page}");
        }

        // The shell reports gestures through callbacks; the controller owns the
        // page change. Record the callbacks and drive a horizontal swipe on the
        // background area.
        let next = Rc::new(Cell::new(0));
        let prev = Rc::new(Cell::new(0));
        {
            let next = Rc::clone(&next);
            app.on_swipe_next(move || next.set(next.get() + 1));
        }
        {
            let prev = Rc::clone(&prev);
            app.on_swipe_prev(move || prev.set(prev.get() + 1));
        }

        let swipe = element(&app, "MainWindow::swipe-area");
        let position = swipe.absolute_position();
        let size = swipe.size();
        let start = LogicalPosition::new(
            position.x + size.width / 2.0,
            position.y + size.height / 2.0,
        );

        swipe
            .drag(
                LogicalPosition::new(start.x - 200.0, start.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(next.get(), 1, "a left swipe must report swipe-next");
        assert_eq!(prev.get(), 0);

        swipe
            .drag(
                LogicalPosition::new(start.x + 200.0, start.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(prev.get(), 1, "a right swipe must report swipe-prev");

        // A vertical drag must not be treated as a page swipe.
        swipe
            .drag(
                LogicalPosition::new(start.x, start.y - 120.0),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(next.get(), 1, "a vertical drag must not page-flip");
        assert_eq!(prev.get(), 1);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
