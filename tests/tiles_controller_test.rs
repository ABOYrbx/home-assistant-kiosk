use ha_kiosk::events::HaError;
use ha_kiosk::{AppController, AppEvent, Config, HaCommand, HaEvent, MainWindow};
use i_slint_backend_testing::ElementHandle;
use slint::Model;
use slint::platform::PointerEventButton;
use std::fs;
use tempfile::tempdir;

fn test_config(dir: &std::path::Path) -> Config {
    let cache = dir.join("cache.json");
    let content = format!(
        r#"
[ha]
url = "http://homeassistant.local:8123"
token = "test-token"

[tiles]
columns = 2

[[tiles.items]]
entity = "switch.example"
label = "Example switch"

[[tiles.items]]
entity = "sensor.example"
label = "Example sensor"

[cache]
path = "{}"
"#,
        cache.display()
    );
    let path = dir.join("config.toml");
    fs::write(&path, content).unwrap();
    Config::load(&path).unwrap()
}

#[test]
fn controller_wires_tiles_to_slint_and_ha_commands() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let window = MainWindow::new().unwrap();
        let mut controller = AppController::new(window, config, tx);

        // The configured grid is pushed into the Slint model.
        assert_eq!(controller.window().get_tile_columns(), 2);
        assert_eq!(controller.window().get_tiles().row_count(), 2);

        // A tap sends exactly one service call.
        assert!(controller.tap_tile("switch.example"));
        assert!(!controller.tap_tile("switch.example"));
        let request_id = match rx.try_recv().unwrap() {
            HaCommand::CallService(call) => {
                assert_eq!(call.domain, "homeassistant");
                assert_eq!(call.service, "toggle");
                call.request_id
            }
            other => panic!("expected a service call, got {other:?}"),
        };
        assert!(rx.try_recv().is_err());

        // A failed call clears the pending flag, so a retry is accepted.
        controller.apply(AppEvent::Ha(HaEvent::ServiceResult {
            request_id,
            result: Err(HaError::Service("boom".into())),
        }));
        assert!(controller.tap_tile("switch.example"));
        let retry_id = match rx.try_recv().unwrap() {
            HaCommand::CallService(call) => call.request_id,
            other => panic!("expected a service call, got {other:?}"),
        };
        controller.apply(AppEvent::Ha(HaEvent::ServiceResult {
            request_id: retry_id,
            result: Ok(Default::default()),
        }));

        // Read-only tiles never send a command.
        assert!(!controller.tap_tile("sensor.example"));

        // The touch area on a rendered tile reaches the same tap handler.
        let touch = ElementHandle::find_by_element_id(controller.window(), "HaTile::tile-touch")
            .next()
            .expect("a rendered tile touch area");
        touch.single_click(PointerEventButton::Left).await;
        assert!(matches!(rx.try_recv().unwrap(), HaCommand::CallService(_)));

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
