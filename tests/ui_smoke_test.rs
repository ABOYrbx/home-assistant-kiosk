use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{LogicalPosition, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn element_center(element: &ElementHandle) -> LogicalPosition {
    let position = element.absolute_position();
    let size = element.size();
    LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    )
}

#[test]
fn shell_navigation_and_gestures() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();

        // Tap on a tab changes the page.
        let tab_media = ElementHandle::find_by_element_id(&app, "MainWindow::tab-media")
            .next()
            .unwrap();
        tab_media.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 1);

        // A horizontal swipe from the page background changes the page.
        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        let swipe_center = element_center(&swipe);
        swipe
            .drag(
                LogicalPosition::new(swipe_center.x - 120.0, swipe_center.y),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 2);

        // Back to the media page, then a vertical drag on the background must
        // leave the page alone.
        let tab_media = ElementHandle::find_by_element_id(&app, "MainWindow::tab-media")
            .next()
            .unwrap();
        tab_media.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 1);

        let swipe = ElementHandle::find_by_element_id(&app, "MainWindow::swipe-area")
            .next()
            .unwrap();
        swipe
            .drag(
                LogicalPosition::new(swipe_center.x, swipe_center.y - 120.0),
                PointerEventButton::Left,
            )
            .await;
        assert_eq!(app.get_current_page(), 1);

        // Fill the agenda list with more rows than fit in the viewport.
        let rows: Vec<SharedString> = (0..40)
            .map(|index| SharedString::from(format!("Event {index}")))
            .collect();
        app.set_agenda_items(ModelRc::from(Rc::new(VecModel::from(rows))));

        let tab_agenda = ElementHandle::find_by_element_id(&app, "MainWindow::tab-agenda")
            .next()
            .unwrap();
        tab_agenda.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 2);

        // A vertical drag inside the list scrolls the list, not the page.
        let list = ElementHandle::find_by_element_id(&app, "AgendaPage::list")
            .next()
            .unwrap();
        let list_center = element_center(&list);
        list.drag(
            LogicalPosition::new(list_center.x, list_center.y - 120.0),
            PointerEventButton::Left,
        )
        .await;
        assert_eq!(app.get_current_page(), 2);
        assert!(
            app.get_agenda_scroll_y() < 0.0,
            "the list should scroll when dragged vertically"
        );

        // A horizontal drag inside the list must not change the page: the
        // scrollable list owns its gestures and page swipe starts only from the
        // background.
        let list = ElementHandle::find_by_element_id(&app, "AgendaPage::list")
            .next()
            .unwrap();
        list.drag(
            LogicalPosition::new(list_center.x - 120.0, list_center.y),
            PointerEventButton::Left,
        )
        .await;
        assert_eq!(app.get_current_page(), 2);

        // The tab bar remains the reliable fallback from any page.
        let tab_status = ElementHandle::find_by_element_id(&app, "MainWindow::tab-status")
            .next()
            .unwrap();
        tab_status.single_click(PointerEventButton::Left).await;
        assert_eq!(app.get_current_page(), 3);

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
