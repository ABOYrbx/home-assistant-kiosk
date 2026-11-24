use i_slint_backend_testing::ElementHandle;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

fn center(element: &ElementHandle) -> LogicalPosition {
    let position = element.absolute_position();
    let size = element.size();
    LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    )
}

#[test]
fn tab_pressed_and_keyboard_focus_states() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();

        let media = ElementHandle::find_by_element_id(&app, "MainWindow::tab-media")
            .next()
            .unwrap();

        assert_eq!(app.get_focused_tab(), -1);

        // Tab moves focus in document order and reveals the ring on the focused tab.
        app.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Tab.into(),
        });
        assert_eq!(app.get_focused_tab(), 0);
        app.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Tab.into(),
        });
        assert_eq!(app.get_focused_tab(), 1);

        // Space activates the focused tab, so keyboard users can navigate the shell.
        app.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Space.into(),
        });
        assert_eq!(app.get_current_page(), 1);

        // The pressed state is visible while the pointer is down and disappears on
        // release.
        assert!(!app.get_tab_pressed());
        let position = center(&media);
        app.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        app.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        assert!(app.get_tab_pressed());
        app.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        assert!(!app.get_tab_pressed());

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
