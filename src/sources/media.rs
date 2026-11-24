use crate::events::ServiceCall;
use crate::ha::models::HaEntity;
use crate::state::MediaView;
use std::time::{Duration, Instant};
use url::Url;

/// Minimum spacing between volume service calls while a slider is dragged.
pub const VOLUME_DEBOUNCE: Duration = Duration::from_millis(100);

// Home Assistant `MediaPlayerEntityFeature` bits.
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
pub const FEATURE_SHUFFLE_SET: u64 = 32768;
pub const FEATURE_SELECT_SOUND_MODE: u64 = 65536;
pub const FEATURE_BROWSE_MEDIA: u64 = 131072;
pub const FEATURE_REPEAT_SET: u64 = 262144;
pub const FEATURE_GROUPING: u64 = 524288;

pub const SERVICE_PLAY_PAUSE: &str = "media_play_pause";
pub const SERVICE_NEXT_TRACK: &str = "media_next_track";
pub const SERVICE_PREVIOUS_TRACK: &str = "media_previous_track";
pub const SERVICE_VOLUME_SET: &str = "volume_set";

pub fn supports(features: u64, bit: u64) -> bool {
    features & bit != 0
}

pub fn is_available_state(state: &str) -> bool {
    !matches!(state, "unavailable" | "unknown")
}

pub fn is_playing(view: &MediaView) -> bool {
    matches!(view.state.as_str(), "playing" | "buffering")
}

pub fn parse_media_entity(entity: &HaEntity) -> MediaView {
    let attrs = &entity.attributes;
    let string_attr = |key: &str| {
        attrs
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };

    MediaView {
        entity_id: entity.entity_id.clone(),
        state: entity.state.clone(),
        title: string_attr("media_title")
            .or_else(|| string_attr("media_episode"))
            .or_else(|| string_attr("media_channel")),
        artist: string_attr("media_artist").or_else(|| string_attr("media_album_artist")),
        album: string_attr("media_album_name"),
        entity_picture: string_attr("entity_picture"),
        volume_level: attrs
            .get("volume_level")
            .and_then(|v| v.as_f64())
            .map(|v| v.clamp(0.0, 1.0)),
        is_volume_muted: attrs
            .get("is_volume_muted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supported_features: attrs
            .get("supported_features")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        available: is_available_state(&entity.state),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MediaControls {
    pub play_pause: bool,
    pub previous: bool,
    pub next: bool,
    pub volume: bool,
}

pub fn can_play_pause(view: &MediaView) -> bool {
    view.available
        && (supports(view.supported_features, FEATURE_PLAY)
            || supports(view.supported_features, FEATURE_PAUSE))
}

pub fn can_previous(view: &MediaView) -> bool {
    view.available && supports(view.supported_features, FEATURE_PREVIOUS_TRACK)
}

pub fn can_next(view: &MediaView) -> bool {
    view.available && supports(view.supported_features, FEATURE_NEXT_TRACK)
}

pub fn can_set_volume(view: &MediaView) -> bool {
    view.available
        && view.volume_level.is_some()
        && supports(view.supported_features, FEATURE_VOLUME_SET)
}

pub fn controls(view: &MediaView) -> MediaControls {
    MediaControls {
        play_pause: can_play_pause(view),
        previous: can_previous(view),
        next: can_next(view),
        volume: can_set_volume(view),
    }
}

/// A relative `entity_picture` is resolved against the Home Assistant base URL.
/// Only HTTP(S) images are accepted; `data:` and `file:` URLs are rejected.
pub fn resolve_image_url(base: &Url, picture: &str) -> Option<Url> {
    let picture = picture.trim();
    if picture.is_empty() {
        return None;
    }
    let url = match Url::parse(picture) {
        Ok(url) => url,
        Err(_) => base.join(picture).ok()?,
    };
    matches!(url.scheme(), "http" | "https").then_some(url)
}

pub fn display_title(view: &MediaView) -> String {
    if let Some(title) = view.title.as_deref().filter(|t| !t.is_empty()) {
        return title.to_string();
    }
    if !view.available {
        return match view.state.as_str() {
            "unknown" => "Unknown".to_string(),
            _ => "Unavailable".to_string(),
        };
    }
    match view.state.as_str() {
        "idle" | "off" => "Nothing playing".to_string(),
        _ => "Unknown title".to_string(),
    }
}

pub fn display_subtitle(view: &MediaView) -> String {
    match (view.artist.as_deref(), view.album.as_deref()) {
        (Some(artist), Some(album)) => format!("{artist} · {album}"),
        (Some(artist), None) => artist.to_string(),
        (None, Some(album)) => album.to_string(),
        (None, None) => String::new(),
    }
}

pub fn display_state(view: &MediaView) -> String {
    let label = match view.state.as_str() {
        "playing" => "Playing",
        "paused" => "Paused",
        "buffering" => "Buffering",
        "idle" => "Idle",
        "off" => "Off",
        "unavailable" => "Unavailable",
        "unknown" => "Unknown",
        other => other,
    };
    if view.is_volume_muted && view.available {
        format!("{label} · Muted")
    } else {
        label.to_string()
    }
}

pub fn play_pause_call(request_id: u64, entity_id: &str) -> ServiceCall {
    transport_call(request_id, entity_id, SERVICE_PLAY_PAUSE)
}

pub fn next_track_call(request_id: u64, entity_id: &str) -> ServiceCall {
    transport_call(request_id, entity_id, SERVICE_NEXT_TRACK)
}

pub fn previous_track_call(request_id: u64, entity_id: &str) -> ServiceCall {
    transport_call(request_id, entity_id, SERVICE_PREVIOUS_TRACK)
}

fn transport_call(request_id: u64, entity_id: &str, service: &str) -> ServiceCall {
    ServiceCall {
        request_id,
        domain: "media_player".to_string(),
        service: service.to_string(),
        target: Some(serde_json::json!({ "entity_id": entity_id })),
        data: serde_json::json!({}),
        return_response: false,
    }
}

pub fn volume_call(request_id: u64, entity_id: &str, volume: f32) -> ServiceCall {
    ServiceCall {
        request_id,
        domain: "media_player".to_string(),
        service: SERVICE_VOLUME_SET.to_string(),
        target: Some(serde_json::json!({ "entity_id": entity_id })),
        data: serde_json::json!({ "volume_level": f64::from(volume.clamp(0.0, 1.0)) }),
        return_response: false,
    }
}

/// Coalesces slider movement into at most one call per [`VOLUME_DEBOUNCE`].
/// The release handler always returns the final value.
#[derive(Debug, Default)]
pub struct VolumeDebouncer {
    last_sent: Option<Instant>,
}

impl VolumeDebouncer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_change(&mut self, volume: f32, now: Instant) -> Option<f32> {
        if self
            .last_sent
            .is_some_and(|last| now.saturating_duration_since(last) < VOLUME_DEBOUNCE)
        {
            return None;
        }
        self.last_sent = Some(now);
        Some(volume.clamp(0.0, 1.0))
    }

    pub fn on_release(&mut self, volume: f32, now: Instant) -> f32 {
        self.last_sent = Some(now);
        volume.clamp(0.0, 1.0)
    }
}
