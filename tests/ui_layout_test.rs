//! Layout checks for the consolidated shell at the kiosk breakpoints.

use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, PhysicalSize};

fn set_window_size(app: &ha_kiosk::MainWindow, width: u32, height: u32) {
    app.window().set_size(PhysicalSize::new(width, height));
    slint::platform::update_timers_and_animations();
}

fn element(app: &ha_kiosk::MainWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("missing element {id}"))
}

fn assert_inside(element: &ElementHandle, label: &str, width: f32, height: f32) {
    let position = element.absolute_position();
    let size = element.size();
    assert!(
        position.x >= -0.5 && position.y >= -0.5,
        "{label} starts outside the window"
    );
    assert!(
        position.x + size.width <= width + 0.5 && position.y + size.height <= height + 0.5,
        "{label} overflows {width}x{height}: {position:?} + {size:?}"
    );
}

#[test]
fn shell_fits_every_kiosk_breakpoint() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();

        for (width, height) in [
            (800u32, 480u32),
            (1024, 600),
            (1280, 800),
            (1440, 900),
            (2048, 1200),
        ] {
            set_window_size(&app, width, height);

            let host = element(&app, "MainWindow::swipe-area");
            assert_inside(&host, "page host", width as f32, height as f32);
            assert!(
                (host.size().width - width as f32).abs() < 1.0,
                "the page host must fill the window width: {} != {width}",
                host.size().width
            );

            for tab in [
                "MainWindow::tab-overview",
                "MainWindow::tab-media",
                "MainWindow::tab-agenda",
                "MainWindow::tab-status",
            ] {
                let button = element(&app, tab);
                let size = button.size();
                assert!(
                    size.width >= 44.0 && size.height >= 44.0,
                    "{tab} is below the 44x44 touch target: {size:?}"
                );
                assert_inside(&button, tab, width as f32, height as f32);
            }
        }

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
