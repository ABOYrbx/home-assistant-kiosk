use crate::ha::models::HaEntity;
use crate::state::MediaView;

pub fn parse_media_entity(entity: &HaEntity) -> MediaView {
    let mut view = MediaView {
        entity_id: entity.entity_id.clone(),
        state: entity.state.clone(),
        ..Default::default()
    };
    let attrs = &entity.attributes;
    view.title = attrs
        .get("media_title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    view.artist = attrs
        .get("media_artist")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    view.album = attrs
        .get("media_album_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    view.entity_picture = attrs
        .get("entity_picture")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    view.volume_level = attrs.get("volume_level").and_then(|v| v.as_f64());
    view.supported_features = attrs
        .get("supported_features")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    view
}

// Feature bits (Home Assistant media_player supported_features).
pub const FEATURE_PAUSE: u64 = 1;
pub const FEATURE_SEEK: u64 = 2;
pub const FEATURE_VOLUME_SET: u64 = 4;
pub const FEATURE_VOLUME_MUTE: u64 = 8;
pub const FEATURE_PREVIOUS_TRACK: u64 = 16;
pub const FEATURE_NEXT_TRACK: u64 = 32;
pub const FEATURE_TURN_ON: u64 = 128;
pub const FEATURE_TURN_OFF: u64 = 256;
pub const FEATURE_PLAY_MEDIA: u64 = 512;
pub const FEATURE_VOLUME_STEP: u64 = 1024;
pub const FEATURE_SELECT_SOURCE: u64 = 2048;
pub const FEATURE_STOP: u64 = 4096;
pub const FEATURE_CLEAR_PLAYLIST: u64 = 8192;
pub const FEATURE_PLAY: u64 = 16384;
pub const FEATURE_TOGGLE: u64 = 32768;

pub fn supports(features: u64, bit: u64) -> bool {
    features & bit != 0
}

pub fn resolve_image_url(base: &url::Url, picture: &str) -> Option<url::Url> {
    if picture.is_empty() {
        return None;
    }
    if let Ok(u) = url::Url::parse(picture) {
        return Some(u);
    }
    base.join(picture).ok()
}

pub fn same_origin(a: &url::Url, b: &url::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ha::models::HaEntity;
    use serde_json::json;

    #[test]
    fn parse_media() {
        let e = HaEntity {
            entity_id: "media_player.living_room".into(),
            state: "playing".into(),
            attributes: json!({
                "media_title": "Song",
                "media_artist": "Artist",
                "media_album_name": "Album",
                "entity_picture": "/api/media_player_proxy/media_player.living_room?token=abc",
                "volume_level": 0.5,
                "supported_features": 16384 | 32768 | 4 | 16 | 32
            }),
            last_changed: None,
            last_updated: None,
        };
        let v = parse_media_entity(&e);
        assert_eq!(v.state, "playing");
        assert_eq!(v.title.as_deref(), Some("Song"));
        assert_eq!(v.volume_level, Some(0.5));
        assert!(supports(v.supported_features, FEATURE_PLAY));
    }

    #[test]
    fn resolve_relative() {
        let base = url::Url::parse("http://ha.local:8123").unwrap();
        let u = resolve_image_url(&base, "/api/img.png").unwrap();
        assert_eq!(u.as_str(), "http://ha.local:8123/api/img.png");
    }

    #[test]
    fn same_origin_check() {
        let a = url::Url::parse("http://ha.local:8123").unwrap();
        let b = url::Url::parse("http://ha.local:8123/x").unwrap();
        let c = url::Url::parse("https://other/x").unwrap();
        assert!(same_origin(&a, &b));
        assert!(!same_origin(&a, &c));
    }
}
