// UI bridge: typed view models -> Slint properties. Runs on the UI thread only.

use crate::MainWindow;
use crate::sources::media;
use crate::state::MediaView;
use crate::ui::images::AlbumArtState;
use slint::{Image, SharedString};

pub fn apply_media(
    window: &MainWindow,
    view: Option<&MediaView>,
    art: &AlbumArtState,
    pending: bool,
    error: Option<&str>,
    volume_step: f32,
) {
    let Some(view) = view else {
        window.set_media(empty_media(volume_step));
        return;
    };

    let controls = media::controls(view);
    let current_art = art
        .current()
        .filter(|current| current.entity_id == view.entity_id);
    let image = current_art
        .map(|current| Image::from_rgba8(current.image.clone()))
        .unwrap_or_default();

    window.set_media(crate::MediaPlayerData {
        title: media::display_title(view).into(),
        artist: view.artist.clone().unwrap_or_default().into(),
        album: view.album.clone().unwrap_or_default().into(),
        state_label: media::display_state(view).into(),
        volume_label: volume_label(view).into(),
        art: image,
        has_art: current_art.is_some(),
        volume: view.volume_level.unwrap_or(0.0) as f32,
        volume_step,
        muted: view.is_volume_muted,
        playing: media::is_playing(view),
        available: view.available,
        pending,
        can_play_pause: controls.play_pause,
        can_previous: controls.previous,
        can_next: controls.next,
        can_set_volume: controls.volume,
        error: error.unwrap_or_default().into(),
    });
}

fn volume_label(view: &MediaView) -> String {
    if view.is_volume_muted {
        return "Muted".to_string();
    }
    match view.volume_level {
        Some(level) => format!("{}%", (level * 100.0).round() as i32),
        None => String::new(),
    }
}

fn empty_media(volume_step: f32) -> crate::MediaPlayerData {
    crate::MediaPlayerData {
        title: "No media player configured".into(),
        artist: SharedString::new(),
        album: SharedString::new(),
        state_label: "Add [widgets.media].entity to config.toml".into(),
        volume_label: SharedString::new(),
        art: Image::default(),
        has_art: false,
        volume: 0.0,
        volume_step,
        muted: false,
        playing: false,
        available: false,
        pending: false,
        can_play_pause: false,
        can_previous: false,
        can_next: false,
        can_set_volume: false,
        error: SharedString::new(),
    }
}
