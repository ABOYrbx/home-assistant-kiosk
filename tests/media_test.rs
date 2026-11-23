use ha_kiosk::ha::models::HaEntity;
use ha_kiosk::sources::media::*;
use ha_kiosk::state::MediaView;
use serde_json::json;
use std::time::{Duration, Instant};
use url::Url;

fn fixture(name: &str) -> HaEntity {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw = std::fs::read_to_string(path).expect("fixture");
    serde_json::from_str(&raw).expect("fixture parses")
}

fn view(features: u64, state: &str, volume: Option<f64>) -> MediaView {
    MediaView {
        entity_id: "media_player.example".into(),
        state: state.into(),
        supported_features: features,
        volume_level: volume,
        available: is_available_state(state),
        ..Default::default()
    }
}

#[test]
fn parses_playing_entity() {
    let parsed = parse_media_entity(&fixture("media_player_playing.json"));

    assert_eq!(parsed.entity_id, "media_player.example");
    assert_eq!(parsed.state, "playing");
    assert_eq!(parsed.title.as_deref(), Some("Example Song"));
    assert_eq!(parsed.artist.as_deref(), Some("Example Artist"));
    assert_eq!(parsed.album.as_deref(), Some("Example Album"));
    assert_eq!(
        parsed.entity_picture.as_deref(),
        Some("/api/media_player_proxy/media_player.example?token=fixture-token")
    );
    assert_eq!(parsed.volume_level, Some(0.42));
    assert!(!parsed.is_volume_muted);
    assert!(parsed.available);
    assert!(supports(parsed.supported_features, FEATURE_PLAY));
    assert!(supports(parsed.supported_features, FEATURE_PAUSE));
    assert_eq!(display_title(&parsed), "Example Song");
    assert_eq!(display_subtitle(&parsed), "Example Artist · Example Album");
    assert_eq!(display_state(&parsed), "Playing");
}

#[test]
fn parses_paused_entity_and_keeps_missing_fields_empty() {
    let parsed = parse_media_entity(&fixture("media_player_paused.json"));

    assert_eq!(parsed.state, "paused");
    assert!(parsed.album.is_none());
    assert!(parsed.entity_picture.is_none());
    assert_eq!(parsed.volume_level, Some(0.0));
    assert_eq!(display_state(&parsed), "Paused");
    assert_eq!(display_subtitle(&parsed), "Example Artist");
}

#[test]
fn unavailable_entity_disables_every_control() {
    let parsed = parse_media_entity(&fixture("media_player_unavailable.json"));
    let controls = controls(&parsed);

    assert!(!parsed.available);
    assert!(!controls.play_pause);
    assert!(!controls.previous);
    assert!(!controls.next);
    assert!(!controls.volume);
    assert_eq!(display_title(&parsed), "Unavailable");
    assert_eq!(display_state(&parsed), "Unavailable");
}

#[test]
fn minimal_entity_reports_idle_state_honestly() {
    let parsed = parse_media_entity(&fixture("media_player_minimal.json"));
    let controls = controls(&parsed);

    assert_eq!(parsed.title, None);
    assert_eq!(parsed.volume_level, None);
    assert_eq!(parsed.supported_features, 0);
    assert!(!controls.play_pause && !controls.previous && !controls.next && !controls.volume);
    assert_eq!(display_title(&parsed), "Nothing playing");
    assert_eq!(display_subtitle(&parsed), "");
    assert_eq!(display_state(&parsed), "Idle");
}

#[test]
fn falls_back_to_episode_channel_and_album_artist() {
    let entity = HaEntity {
        entity_id: "media_player.example".into(),
        state: "playing".into(),
        attributes: json!({
            "media_title": "   ",
            "media_episode": "Episode 4",
            "media_album_artist": "Album Artist",
            "volume_level": 4.2,
            "is_volume_muted": true,
        }),
        last_changed: None,
        last_updated: None,
    };

    let parsed = parse_media_entity(&entity);

    assert_eq!(parsed.title.as_deref(), Some("Episode 4"));
    assert_eq!(parsed.artist.as_deref(), Some("Album Artist"));
    assert_eq!(parsed.volume_level, Some(1.0));
    assert!(parsed.is_volume_muted);
    assert_eq!(display_state(&parsed), "Playing · Muted");
}

#[test]
fn long_titles_are_preserved_for_the_widget_to_elide() {
    let title = vec!["A very long track title"; 40].join(" ");
    let entity = HaEntity {
        entity_id: "media_player.example".into(),
        state: "playing".into(),
        attributes: json!({ "media_title": title }),
        last_changed: None,
        last_updated: None,
    };

    assert_eq!(
        parse_media_entity(&entity).title.as_deref(),
        Some(title.as_str())
    );
}

#[test]
fn unknown_state_is_reported_honestly() {
    let unknown = view(0, "unknown", None);
    assert!(!unknown.available);
    assert_eq!(display_title(&unknown), "Unknown");
    assert_eq!(display_state(&unknown), "Unknown");
}

#[test]
fn controls_follow_supported_features() {
    let none = view(0, "playing", Some(0.5));
    assert_eq!(controls(&none), MediaControls::default());

    let next_only = view(FEATURE_NEXT_TRACK, "playing", None);
    assert_eq!(
        controls(&next_only),
        MediaControls {
            next: true,
            ..Default::default()
        }
    );

    let volume_without_level = view(FEATURE_VOLUME_SET, "playing", None);
    assert!(!controls(&volume_without_level).volume);

    let full = view(
        FEATURE_PLAY
            | FEATURE_PAUSE
            | FEATURE_PREVIOUS_TRACK
            | FEATURE_NEXT_TRACK
            | FEATURE_VOLUME_SET,
        "paused",
        Some(0.3),
    );
    assert_eq!(
        controls(&full),
        MediaControls {
            play_pause: true,
            previous: true,
            next: true,
            volume: true,
        }
    );

    assert!(is_playing(&view(FEATURE_PLAY, "buffering", None)));
    assert!(!is_playing(&view(FEATURE_PLAY, "paused", None)));
}

#[test]
fn transport_and_volume_calls_target_the_entity() {
    let play = play_pause_call(7, "media_player.example");
    assert_eq!(play.request_id, 7);
    assert_eq!(play.domain, "media_player");
    assert_eq!(play.service, "media_play_pause");
    assert_eq!(
        play.target.as_ref().unwrap()["entity_id"],
        json!("media_player.example")
    );
    assert!(!play.return_response);

    assert_eq!(next_track_call(1, "x").service, "media_next_track");
    assert_eq!(previous_track_call(1, "x").service, "media_previous_track");

    let volume = volume_call(9, "media_player.example", 1.5);
    assert_eq!(volume.service, "volume_set");
    assert_eq!(volume.data["volume_level"], json!(1.0));
    assert_eq!(
        volume.target.as_ref().unwrap()["entity_id"],
        json!("media_player.example")
    );
}

#[test]
fn volume_debouncer_allows_one_call_per_100_ms_and_always_sends_release() {
    let mut debouncer = VolumeDebouncer::new();
    let start = Instant::now();

    assert_eq!(debouncer.on_change(0.1, start), Some(0.1));
    assert_eq!(
        debouncer.on_change(0.2, start + Duration::from_millis(50)),
        None
    );
    assert_eq!(
        debouncer.on_change(0.3, start + Duration::from_millis(99)),
        None
    );
    assert_eq!(
        debouncer.on_change(0.4, start + Duration::from_millis(100)),
        Some(0.4)
    );
    assert_eq!(
        debouncer.on_change(0.5, start + Duration::from_millis(150)),
        None
    );

    assert_eq!(
        debouncer.on_release(0.5, start + Duration::from_millis(160)),
        0.5
    );
    assert_eq!(
        debouncer.on_release(2.0, start + Duration::from_millis(161)),
        1.0
    );
    assert_eq!(
        debouncer.on_release(-1.0, start + Duration::from_millis(162)),
        0.0
    );
}

#[test]
fn resolves_image_urls_and_rejects_unsafe_schemes() {
    let base = Url::parse("http://homeassistant.local:8123").unwrap();

    assert_eq!(
        resolve_image_url(&base, "/api/media_player_proxy/x?token=abc")
            .unwrap()
            .as_str(),
        "http://homeassistant.local:8123/api/media_player_proxy/x?token=abc"
    );
    assert_eq!(
        resolve_image_url(&base, "https://cdn.example.test/art.png")
            .unwrap()
            .as_str(),
        "https://cdn.example.test/art.png"
    );
    assert_eq!(
        resolve_image_url(&base, "//cdn.example.test/art.png")
            .unwrap()
            .as_str(),
        "http://cdn.example.test/art.png"
    );
    assert!(resolve_image_url(&base, "data:image/png;base64,AAAA").is_none());
    assert!(resolve_image_url(&base, "file:///etc/passwd").is_none());
    assert!(resolve_image_url(&base, "").is_none());
    assert!(resolve_image_url(&base, "   ").is_none());
}
