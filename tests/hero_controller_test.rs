use ha_kiosk::config::{
    CacheConfig, DisplayConfig, ExtensionsConfig, HaConfig, HeroLightConfig, PagesConfig,
    TilesConfig, WidgetsConfig,
};
use ha_kiosk::events::{HaEntity, HaEvent, ServiceResponse};
use ha_kiosk::{AppController, AppEvent, Config, HaCommand, KioskModel, MainWindow};
use i_slint_backend_testing::ElementHandle;
use secrecy::SecretString;
use slint::ComponentHandle;
use slint::platform::PointerEventButton;
use std::path::PathBuf;
use tokio::sync::mpsc;

fn test_config() -> Config {
    Config {
        ha: HaConfig {
            url: url::Url::parse("http://homeassistant.local:8123").unwrap(),
            token: SecretString::from("test-token".to_string()),
            token_file: None,
            allow_invalid_certs: false,
            extra_entity_ids: Vec::new(),
            time_zone: None,
        },
        display: DisplayConfig::default(),
        pages: PagesConfig::default(),
        widgets: WidgetsConfig {
            hero_light: HeroLightConfig {
                enabled: true,
                entity: "light.example".into(),
                name: "Example lamp".into(),
                min_kelvin: 2700,
                max_kelvin: 6500,
            },
            ..Default::default()
        },
        tiles: TilesConfig {
            columns: 3,
            items: Vec::new(),
        },
        cache: CacheConfig {
            path: PathBuf::from("/tmp/ha-kiosk-hero-controller-test.json"),
            write_debounce_seconds: 10,
            max_bytes: 262144,
        },
        extensions: ExtensionsConfig::default(),
    }
}

fn light_entity() -> HaEntity {
    HaEntity {
        entity_id: "light.example".into(),
        state: "on".into(),
        attributes: serde_json::json!({
            "brightness": 184,
            "color_temp_kelvin": 3200,
            "min_color_temp_kelvin": 2700,
            "max_color_temp_kelvin": 6500,
            "supported_color_modes": ["color_temp"]
        }),
        last_changed: None,
        last_updated: None,
    }
}

#[test]
fn controller_binds_hero_state_and_sends_debounced_service_calls() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let window = MainWindow::new().unwrap();
        let (command_tx, mut command_rx) = mpsc::channel(16);
        let mut controller = AppController::new(window, test_config(), command_tx);

        controller.apply(AppEvent::Ha(HaEvent::Snapshot(vec![light_entity()])));

        {
            let model = controller.window().global::<KioskModel>();
            assert!(model.get_hero_configured());
            assert!(model.get_hero_available());
            assert!(!model.get_hero_loading());
            assert_eq!(model.get_hero_name(), "Example lamp");
            assert!(model.get_hero_on());
            assert_eq!(model.get_hero_brightness(), 72);
            assert_eq!(model.get_hero_color_temp(), 3200);
            assert_eq!(model.get_hero_brightness_text(), "72%");
            assert_eq!(model.get_hero_color_temp_text(), "3200 K");
        }

        let slider =
            ElementHandle::find_by_element_id(controller.window(), "HeroLight::hero-brightness")
                .next()
                .expect("brightness slider exists");
        slider.single_click(PointerEventButton::Left).await;

        let command = command_rx.try_recv().expect("brightness service call");
        match command {
            HaCommand::CallService(call) => {
                assert_eq!(call.domain, "light");
                assert_eq!(call.service, "turn_on");
                assert_eq!(
                    call.target,
                    Some(serde_json::json!({"entity_id": "light.example"}))
                );
                let value = call.data["brightness_pct"].as_u64().unwrap();
                assert!((45..=55).contains(&value), "brightness_pct: {value}");
                let model = controller.window().global::<KioskModel>();
                assert_eq!(model.get_hero_brightness(), value as i32);
                assert_eq!(model.get_hero_brightness_text(), format!("{value}%"));
                assert!(model.get_hero_pending());

                controller.apply(AppEvent::Ha(HaEvent::ServiceResult {
                    request_id: call.request_id,
                    result: Ok(ServiceResponse::default()),
                }));
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let toggle =
            ElementHandle::find_by_element_id(controller.window(), "HeroLight::hero-toggle")
                .next()
                .expect("hero toggle exists");
        toggle.single_click(PointerEventButton::Left).await;

        let command = command_rx.try_recv().expect("toggle service call");
        match command {
            HaCommand::CallService(call) => {
                assert_eq!(call.domain, "light");
                assert_eq!(call.service, "toggle");
                controller.apply(AppEvent::Ha(HaEvent::ServiceResult {
                    request_id: call.request_id,
                    result: Err(ha_kiosk::events::HaError::Service("boom".into())),
                }));
            }
            other => panic!("unexpected command: {other:?}"),
        }

        {
            let model = controller.window().global::<KioskModel>();
            assert!(model.get_hero_error());
            assert_eq!(model.get_hero_status_text(), "service error: boom");
        }

        slint::quit_event_loop().unwrap();
    })
    .unwrap();

    slint::run_event_loop().unwrap();
}
