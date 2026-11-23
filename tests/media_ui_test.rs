use ha_kiosk::events::AppEvent;
use ha_kiosk::state::MediaView;
use ha_kiosk::ui::bridge;
use ha_kiosk::ui::images::{AlbumArtState, decode_image};
use ha_kiosk::{Config, MediaPlayerData};
use i_slint_backend_testing::ElementHandle;
use slint::platform::PointerEventButton;
use slint::{Image, SharedString};
use std::cell::Cell;
use std::rc::Rc;

fn data(can_play_pause: bool, pending: bool) -> MediaPlayerData {
    MediaPlayerData {
        title: "Example Song".into(),
        artist: "Example Artist".into(),
        album: "Example Album".into(),
        state_label: "Playing".into(),
        volume_label: "42%".into(),
        art: Image::default(),
        has_art: false,
        volume: 0.42,
        volume_step: 0.05,
        muted: false,
        playing: true,
        available: true,
        pending,
        can_play_pause,
        can_previous: true,
        can_next: true,
        can_set_volume: true,
        error: SharedString::new(),
    }
}

#[test]
fn media_widget_controls_states_and_bridge() {
    i_slint_backend_testing::init_integration_test_with_system_time();

    slint::spawn_local(async move {
        let app = ha_kiosk::MainWindow::new().unwrap();
        app.set_current_page(1);

        // Unsupported, supported, and pending transport controls.
        let fired = Rc::new(Cell::new(0));
        let counter = Rc::clone(&fired);
        app.on_media_play_pause(move || counter.set(counter.get() + 1));

        app.set_media(data(false, false));
        let button = ElementHandle::find_by_element_id(&app, "MediaPlayerWidget::play-button")
            .next()
            .expect("play button");
        button.single_click(PointerEventButton::Left).await;
        assert_eq!(fired.get(), 0, "an unsupported control must ignore taps");

        app.set_media(data(true, false));
        let button = ElementHandle::find_by_element_id(&app, "MediaPlayerWidget::play-button")
            .next()
            .expect("play button");
        button.single_click(PointerEventButton::Left).await;
        assert_eq!(
            fired.get(),
            1,
            "a supported control must invoke its callback"
        );

        app.set_media(data(true, true));
        let button = ElementHandle::find_by_element_id(&app, "MediaPlayerWidget::play-button")
            .next()
            .expect("play button");
        button.single_click(PointerEventButton::Left).await;
        assert_eq!(fired.get(), 1, "a pending action must ignore repeated taps");

        // The volume slider is disabled unless the entity supports setting volume.
        let mut media = data(true, false);
        media.can_set_volume = false;
        app.set_media(media);
        let slider = ElementHandle::find_by_element_id(&app, "MediaPlayerWidget::volume-slider")
            .next()
            .expect("volume slider");
        assert_eq!(slider.accessible_enabled(), Some(false));

        let mut media = data(true, false);
        media.can_set_volume = true;
        app.set_media(media);
        let slider = ElementHandle::find_by_element_id(&app, "MediaPlayerWidget::volume-slider")
            .next()
            .expect("volume slider");
        assert_eq!(slider.accessible_enabled(), Some(true));

        // Bridge: view model, art, pending and error states.
        let view = MediaView {
            entity_id: "media_player.example".into(),
            state: "playing".into(),
            title: Some("Example Song".into()),
            artist: Some("Example Artist".into()),
            album: Some("Example Album".into()),
            entity_picture: Some("/api/media_player_proxy/x".into()),
            volume_level: Some(0.42),
            is_volume_muted: false,
            supported_features: 16437,
            available: true,
        };
        let mut art = AlbumArtState::default();
        art.apply_loaded(
            "media_player.example".into(),
            "http://homeassistant.local/api/media_player_proxy/x".into(),
            decode_image(&solid_png()).unwrap(),
        );

        bridge::apply_media(
            &app,
            Some(&view),
            &art,
            true,
            Some("service call failed"),
            0.05,
        );
        let rendered = app.get_media();
        assert_eq!(rendered.title, SharedString::from("Example Song"));
        assert_eq!(rendered.artist, SharedString::from("Example Artist"));
        assert_eq!(rendered.album, SharedString::from("Example Album"));
        assert_eq!(rendered.volume_label, SharedString::from("42%"));
        assert_eq!(rendered.volume_step, 0.05);
        assert!(rendered.has_art);
        assert!(rendered.pending);
        assert_eq!(rendered.error, SharedString::from("service call failed"));
        assert!(rendered.can_play_pause && rendered.can_previous && rendered.can_next);
        assert!(rendered.can_set_volume);

        bridge::apply_media(&app, None, &art, false, None, 0.05);
        let empty = app.get_media();
        assert_eq!(
            empty.title,
            SharedString::from("No media player configured")
        );
        assert!(!empty.has_art && !empty.can_play_pause);

        // The controller requests album art only when entity_picture changes.
        let config: Config = toml::from_str(
            r#"
[ha]
url = "http://homeassistant.local:8123"

[cache]
path = "/tmp/ha-kiosk-media-test-cache.json"
"#,
        )
        .unwrap();
        let window = ha_kiosk::MainWindow::new().unwrap();
        let (commands_tx, _commands_rx) = tokio::sync::mpsc::channel(4);
        let mut controller = ha_kiosk::AppController::new(window, config, commands_tx);
        let (art_tx, mut art_rx) = tokio::sync::mpsc::channel(4);
        controller.set_album_art_sender(art_tx);

        let mut view = MediaView {
            entity_id: "media_player.example".into(),
            state: "playing".into(),
            entity_picture: Some("/api/media_player_proxy/x?token=one".into()),
            available: true,
            ..Default::default()
        };
        controller.apply(AppEvent::Media(Some(view.clone())));
        let request = art_rx.try_recv().expect("first art request");
        assert_eq!(request.entity_id, "media_player.example");
        assert_eq!(request.url.path(), "/api/media_player_proxy/x");

        controller.apply(AppEvent::Media(Some(view.clone())));
        assert!(
            art_rx.try_recv().is_err(),
            "an unchanged entity_picture must not be fetched again"
        );

        view.entity_picture = Some("/api/media_player_proxy/x?token=two".into());
        controller.apply(AppEvent::Media(Some(view)));
        let request = art_rx.try_recv().expect("changed picture requests art");
        assert_eq!(request.url.query(), Some("token=two"));

        controller.apply(AppEvent::Media(None));
        assert!(art_rx.try_recv().is_err());

        // show_album_art = false must never request an image.
        let config: Config = toml::from_str(
            r#"
[ha]
url = "http://homeassistant.local:8123"

[cache]
path = "/tmp/ha-kiosk-media-test-cache.json"

[widgets.media]
show_album_art = false
"#,
        )
        .unwrap();
        let window = ha_kiosk::MainWindow::new().unwrap();
        let (commands_tx, _commands_rx) = tokio::sync::mpsc::channel(4);
        let mut controller = ha_kiosk::AppController::new(window, config, commands_tx);
        let (art_tx, mut art_rx) = tokio::sync::mpsc::channel(4);
        controller.set_album_art_sender(art_tx);
        controller.apply(AppEvent::Media(Some(MediaView {
            entity_id: "media_player.example".into(),
            state: "playing".into(),
            entity_picture: Some("/api/media_player_proxy/x".into()),
            available: true,
            ..Default::default()
        })));
        assert!(art_rx.try_recv().is_err());

        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop().unwrap();
}

fn solid_png() -> Vec<u8> {
    use std::io::Cursor;
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}
