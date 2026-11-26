use ha_kiosk::{AppController, AppEvent, Config, HaCommand, HaEvent, MainWindow};
use slint::Model;

#[test]
fn connected_widgets_issue_one_refresh_round_and_route_results() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    slint::spawn_local(async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[ha]
url = "http://homeassistant.local:8123"
token = "test-token"

[widgets.weather]
enabled = true
entity = "weather.example"

[widgets.calendar]
enabled = true
entities = ["calendar.family", "calendar.work"]

[cache]
path = "{}"
"#,
                dir.path().join("cache.json").display()
            ),
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        let window = MainWindow::new().unwrap();
        let (commands, mut receiver) = tokio::sync::mpsc::channel(16);
        let mut controller = AppController::new(window, config, commands);

        controller.apply(AppEvent::Ha(HaEvent::Connected {
            ha_version: "2026.11.0".into(),
            time_zone: "Europe/Berlin".parse().unwrap(),
        }));
        let mut weather_id = None;
        let mut calendar_ids = Vec::new();
        let mut snapshot_refresh = false;
        while let Ok(command) = receiver.try_recv() {
            match command {
                HaCommand::CallService(call) if call.domain == "weather" => {
                    assert_eq!(call.service, "get_forecasts");
                    assert!(call.return_response);
                    weather_id = Some(call.request_id);
                }
                HaCommand::CallService(call) if call.domain == "calendar" => {
                    assert_eq!(call.service, "get_events");
                    assert!(call.return_response);
                    calendar_ids.push(call.request_id);
                }
                HaCommand::RefreshSnapshot => snapshot_refresh = true,
                other => panic!("unexpected command: {other:?}"),
            }
        }
        assert!(snapshot_refresh);
        assert_eq!(calendar_ids.len(), 2);
        let weather_id = weather_id.expect("forecast request");

        controller.apply(AppEvent::Ha(HaEvent::Connected {
            ha_version: "2026.11.0".into(),
            time_zone: "Europe/Berlin".parse().unwrap(),
        }));
        while let Ok(command) = receiver.try_recv() {
            assert!(
                matches!(command, HaCommand::RefreshSnapshot),
                "a second connect must not stack weather or calendar refreshes: {command:?}"
            );
        }

        controller.apply(AppEvent::Ha(HaEvent::ServiceResult {
            request_id: weather_id,
            result: Ok(ha_kiosk::events::ServiceResponse {
                data: serde_json::json!({
                    "weather.example": {"forecast": [{
                        "datetime": "2026-11-25",
                        "condition": "rainy",
                        "temperature": 16.0
                    }]}
                }),
            }),
        }));
        assert_eq!(controller.window().get_weather_forecast().row_count(), 1);

        let local_noon = chrono::Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let event_start = local_noon
            .and_local_timezone(chrono::Local)
            .earliest()
            .unwrap()
            .with_timezone(&chrono::Utc);
        for (index, request_id) in calendar_ids.into_iter().enumerate() {
            let entity = if index == 0 {
                "calendar.family"
            } else {
                "calendar.work"
            };
            controller.apply(AppEvent::Ha(HaEvent::ServiceResult {
                request_id,
                result: Ok(ha_kiosk::events::ServiceResponse {
                    data: serde_json::json!({
                        entity: {"events": [{
                            "summary": "Example event",
                            "start": {"dateTime": (event_start + chrono::Duration::hours(1)).to_rfc3339()},
                            "end": {"dateTime": (event_start + chrono::Duration::hours(2)).to_rfc3339()}
                        }]}
                    }),
                }),
            }));
        }
        assert_eq!(controller.window().get_agenda_today().row_count(), 2);
        assert_eq!(
            controller.window().get_agenda_today().row_count()
                + controller.window().get_agenda_tomorrow().row_count(),
            2,
            "an event crossing midnight belongs to both overlapping day sections"
        );
        assert!(controller.window().get_agenda_error().is_empty());

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}
