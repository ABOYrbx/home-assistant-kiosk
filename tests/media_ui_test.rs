//! Media view model and album-art state for the consolidated Media page.
//!
//! The shell no longer exposes per-widget Slint properties, so widget-level
//! rendering lives with the widget; these tests cover the data it renders and
//! the art state that feeds it.

use ha_kiosk::sources::media::{controls, display_state, display_subtitle, display_title};
use ha_kiosk::state::MediaView;
use ha_kiosk::ui::images::{AlbumArtState, decode_image};
use std::io::Cursor;

fn solid_png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

fn sample_view() -> MediaView {
    MediaView {
        entity_id: "media_player.example".into(),
        state: "playing".into(),
        title: Some("Example Song".into()),
        artist: Some("Example Artist".into()),
        album: Some("Example Album".into()),
        entity_picture: Some("/api/media_player_proxy/x".into()),
        volume_level: Some(0.42),
        supported_features: 16437,
        available: true,
        ..Default::default()
    }
}

#[test]
fn the_view_model_produces_the_labels_the_widget_renders() {
    let view = sample_view();
    assert_eq!(display_title(&view), "Example Song");
    assert_eq!(display_subtitle(&view), "Example Artist · Example Album");
    assert_eq!(display_state(&view), "Playing");

    let enabled = controls(&view);
    assert!(enabled.play_pause && enabled.previous && enabled.next && enabled.volume);

    let unavailable = MediaView {
        available: false,
        ..sample_view()
    };
    let disabled = controls(&unavailable);
    assert!(!disabled.play_pause && !disabled.previous && !disabled.next && !disabled.volume);
}

#[test]
fn album_art_state_keeps_the_last_image_when_a_fetch_fails() {
    let mut art = AlbumArtState::default();
    assert!(art.current().is_none());

    art.apply_loaded(
        "media_player.example".into(),
        "http://homeassistant.local/api/media_player_proxy/x".into(),
        decode_image(&solid_png()).unwrap(),
    );
    assert_eq!(
        art.current_url(),
        Some("http://homeassistant.local/api/media_player_proxy/x")
    );
    assert!(art.current().is_some());
    assert!(art.error().is_none());

    art.apply_failed("network unreachable".into());
    assert_eq!(art.error(), Some("network unreachable"));
    assert!(
        art.current().is_some(),
        "a failed refresh must keep the previously loaded art"
    );

    art.clear();
    assert!(art.current().is_none());
    assert!(art.error().is_none());
}

#[test]
fn album_art_decoding_rejects_garbage() {
    assert!(decode_image(&solid_png()).is_ok());
    assert!(decode_image(b"not a png").is_err());
}
