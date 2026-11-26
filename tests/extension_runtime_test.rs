//! The built-in extension card receives only validated display rows.

use ha_kiosk::MainWindow;
use ha_kiosk::extensions::ExtensionDisplay;
use ha_kiosk::ui::bridge;
use slint::Model;

#[test]
fn bridge_maps_validated_extension_displays_to_the_ui_model() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let app = MainWindow::new().unwrap();
    let displays = vec![
        ExtensionDisplay {
            id: "org.example.bus".into(),
            title: "Buses".into(),
            lines: vec!["42 · Central Station · 18:05".into()],
            error: None,
        },
        ExtensionDisplay {
            id: "org.example.status".into(),
            title: "Stop".into(),
            lines: vec!["Updated: 18:00".into()],
            error: Some("provider timeout".into()),
        },
    ];

    bridge::apply_extension_outputs(&app, &displays);

    let model = app.get_extension_outputs();
    assert_eq!(model.row_count(), 2);
    let first = model.row_data(0).unwrap();
    assert_eq!(first.id, "org.example.bus");
    assert_eq!(first.title, "Buses");
    assert_eq!(first.lines.row_count(), 1);
    assert_eq!(
        first.lines.row_data(0).unwrap(),
        "42 · Central Station · 18:05"
    );
    let second = model.row_data(1).unwrap();
    assert_eq!(second.error, "provider timeout");
}
