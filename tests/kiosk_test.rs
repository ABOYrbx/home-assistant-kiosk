use ha_kiosk::config::DisplayConfig;
use ha_kiosk::kiosk;
use slint::ComponentHandle;

#[test]
fn fullscreen_follows_display_config() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        assert!(!app.window().is_fullscreen());

        kiosk::apply_display(
            app.window(),
            &DisplayConfig {
                fullscreen: true,
                ..Default::default()
            },
        );
        assert!(app.window().is_fullscreen());

        kiosk::apply_display(
            app.window(),
            &DisplayConfig {
                fullscreen: false,
                ..Default::default()
            },
        );
        assert!(!app.window().is_fullscreen());

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
