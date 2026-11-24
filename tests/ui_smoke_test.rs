use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn shell_navigation_and_gestures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        let pages = ["OverviewPage", "MediaPage", "AgendaPage", "StatusPage"];
        let page_count = |name: &str| ElementHandle::find_by_element_type_name(&app, name).count();

        // The shell hosts exactly one page, and the page host has a non-zero
        // layout area to draw into.
        assert_eq!(
            pages.iter().map(|name| page_count(name)).sum::<usize>(),
            1,
            "the page host shows exactly one page"
        );
        let page_host = ElementHandle::find_by_element_id(&app, "MainWindow::page-host")
            .next()
            .expect("the shell exposes a page host");
        assert!(page_host.size().width > 0.0 && page_host.size().height > 0.0);

        // Every tab meets the 44 x 44 logical-pixel touch-target minimum.
        for id in ["tab-overview", "tab-media", "tab-agenda", "tab-status"] {
            let tab = ElementHandle::find_by_element_id(&app, &format!("MainWindow::{id}"))
                .next()
                .unwrap_or_else(|| panic!("{id} exists"));
            assert!(
                tab.size().width >= 44.0 && tab.size().height >= 44.0,
                "{id} is smaller than the 44 x 44 touch target: {:?}",
                tab.size()
            );
        }

        // Tapping a tab selects its page and shows only that page.
        for (id, expected_page) in [
            ("tab-overview", 0usize),
            ("tab-media", 1),
            ("tab-agenda", 2),
            ("tab-status", 3),
        ] {
            let tab = ElementHandle::find_by_element_id(&app, &format!("MainWindow::{id}"))
                .next()
                .unwrap();
            tab.single_click(PointerEventButton::Left).await;
            assert_eq!(
                app.get_current_page() as usize,
                expected_page,
                "{id} selects its page"
            );
            for (index, name) in pages.iter().enumerate() {
                assert_eq!(
                    page_count(name),
                    usize::from(index == expected_page),
                    "{name} visibility after tapping {id}"
                );
            }
        }

        // The controller owns page changes for swipes: the shell reports the
        // gesture through the callbacks, so record them here.
        let next = Rc::new(Cell::new(0));
        let prev = Rc::new(Cell::new(0));
        {
            let next = next.clone();
            app.on_swipe_next(move || next.set(next.get() + 1));
        }
        {
            let prev = prev.clone();
            app.on_swipe_prev(move || prev.set(prev.get() + 1));
        }

        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .expect("the page host has a swipe area");
        let pos = swipe.absolute_position();
        let size = swipe.size();
        let start =
            slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);

        // A horizontal drag past the 60 px threshold reports one swipe.
        let target = slint::LogicalPosition::new(start.x - 120.0, start.y);
        swipe.drag(target, PointerEventButton::Left).await;
        assert_eq!(next.get(), 1, "a left drag reports swipe-next");
        assert_eq!(prev.get(), 0);

        // A short horizontal drag stays below the threshold and reports nothing.
        let short = slint::LogicalPosition::new(start.x - 40.0, start.y);
        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        swipe.drag(short, PointerEventButton::Left).await;
        assert_eq!(next.get(), 1, "a short drag is not a page swipe");

        // A mostly vertical drag never reports a page swipe.
        let vertical = slint::LogicalPosition::new(start.x, start.y - 120.0);
        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        swipe.drag(vertical, PointerEventButton::Left).await;
        assert_eq!(next.get(), 1, "a vertical drag is not a page swipe");
        assert_eq!(prev.get(), 0);

        // A right drag reports swipe-prev.
        let right = slint::LogicalPosition::new(start.x + 120.0, start.y);
        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        swipe.drag(right, PointerEventButton::Left).await;
        assert_eq!(prev.get(), 1, "a right drag reports swipe-prev");

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
