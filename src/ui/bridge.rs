// UI bridge: UI-thread view models -> Slint properties and models.

use crate::extensions::ExtensionDisplay;
use crate::extensions::shop::{ExtensionStatus, ShopEntry, ShopView};
use crate::sources::calendar::{group_agenda, sort_items};
use crate::sources::departures::{DeparturesStatus, display_departures};
use crate::sources::media;
use crate::sources::tiles::{TileBoard, TileView};
use crate::state::{AppState, CalendarView, DeparturesView, MediaView, WeatherView};
use crate::ui::hero::HeroViewModel;
use crate::ui::images::AlbumArtState;
use crate::ui::top_bar::connection_view;
use crate::{
    AgendaEntry, DepartureRow, DeparturesState, ExtensionOutputView, ForecastItem, KioskModel,
    MainWindow, MediaPlayerData, ShopEntryView, TileItem, WeatherState,
};
use chrono::{DateTime, TimeZone, Utc};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::path::Path;
use std::rc::Rc;

pub struct UiBridge;

impl UiBridge {
    pub fn apply(window: &MainWindow, state: &AppState) {
        apply_status(window, state);
    }

    pub fn apply_departures(
        window: &MainWindow,
        view: Option<&DeparturesView>,
        fallback_title: &str,
    ) {
        apply_departures(window, view, fallback_title);
    }

    pub fn apply_calendar<Tz>(
        window: &MainWindow,
        view: Option<&CalendarView>,
        now: DateTime<Utc>,
        device_tz: &Tz,
    ) where
        Tz: TimeZone,
        Tz::Offset: std::fmt::Display,
    {
        apply_calendar(window, view, now, device_tz);
    }
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    Rc::new(VecModel::from(items)).into()
}

pub fn apply_clock(window: &MainWindow, time: String, date: String) {
    let global = window.global::<KioskModel>();
    global.set_clock_text(time.into());
    global.set_date_text(date.into());
}

pub fn apply_status(window: &MainWindow, state: &AppState) {
    let connection = connection_view(&state.connection, state.stale);
    let global = window.global::<KioskModel>();
    global.set_connection_label(connection.label.clone().into());
    global.set_connection_connected(connection.connected);
    global.set_connection_warning(connection.warning);
    global.set_connection_error(connection.error);

    window.set_connection_status(connection.label.into());
    window.set_stale(state.stale);
    window.set_ha_version(state.ha_version.clone().unwrap_or_default().into());
    window.set_last_success(
        state
            .last_success
            .map(|time| time.format("%H:%M:%S").to_string())
            .unwrap_or_default()
            .into(),
    );
    window.set_entity_count(state.entities.len() as i32);
    let mut ids: Vec<SharedString> = state
        .entities
        .keys()
        .cloned()
        .map(SharedString::from)
        .collect();
    ids.sort();
    window.set_entity_ids(model(ids));
}

pub fn apply_hero(window: &MainWindow, view: &HeroViewModel) {
    let global = window.global::<KioskModel>();
    global.set_hero_configured(view.configured);
    global.set_hero_name(view.name.clone().into());
    global.set_hero_on(view.on);
    global.set_hero_brightness(view.brightness);
    global.set_hero_color_temp(view.color_temp);
    global.set_hero_min_kelvin(view.min_kelvin);
    global.set_hero_max_kelvin(view.max_kelvin);
    global.set_hero_available(view.available);
    global.set_hero_loading(view.loading);
    global.set_hero_pending(view.pending);
    global.set_hero_brightness_supported(view.brightness_supported);
    global.set_hero_color_temp_supported(view.color_temp_supported);
    global.set_hero_brightness_text(view.brightness_text.clone().into());
    global.set_hero_color_temp_text(view.color_temp_text.clone().into());
    global.set_hero_state_text(view.state_text.clone().into());
    global.set_hero_status_text(view.status_text.clone().into());
    global.set_hero_error(view.error);
}

pub fn apply_tiles(window: &MainWindow, board: &TileBoard) {
    let columns = board.columns() as i32;
    let mut tiles = Vec::new();
    let mut scenes = Vec::new();
    for (index, tile) in board.tiles().iter().enumerate() {
        if matches!(tile.entity_id.split('.').next(), Some("scene" | "script")) {
            scenes.push(SharedString::from(tile.label.clone()));
            continue;
        }
        tiles.push(tile_item(tile, index, columns));
    }
    window.set_tile_columns(columns);
    window.set_tiles(model(tiles));
    window.set_scenes(model(scenes));
}

fn tile_item(tile: &TileView, index: usize, columns: i32) -> TileItem {
    let columns = columns.max(1) as usize;
    TileItem {
        entity_id: tile.entity_id.clone().into(),
        label: tile.label.clone().into(),
        icon: tile.icon.clone().into(),
        state: tile.state.clone().into(),
        detail: tile.detail.clone().into(),
        read_only: tile.read_only,
        pending: tile.pending,
        error: tile.error.clone().unwrap_or_default().into(),
        row: (index / columns) as i32,
        col: (index % columns) as i32,
    }
}

pub fn apply_weather(
    window: &MainWindow,
    view: Option<&WeatherView>,
    error: Option<&str>,
    stale: bool,
) {
    let Some(view) = view else {
        window.set_weather_state(if error.is_some() {
            WeatherState::Error
        } else {
            WeatherState::Loading
        });
        window.set_weather_error(error.unwrap_or_default().into());
        window.set_weather_forecast(model(Vec::<ForecastItem>::new()));
        return;
    };
    if view.condition.is_empty() {
        window.set_weather_state(WeatherState::Unavailable);
    } else if error.is_some() {
        window.set_weather_state(WeatherState::Error);
    } else {
        window.set_weather_state(WeatherState::Populated);
    }
    window.set_weather_condition(view.condition.clone().into());
    window.set_weather_temperature(
        view.temperature
            .map(|value| format!("{value:.1}°"))
            .unwrap_or_default()
            .into(),
    );
    window.set_weather_unit(view.temperature_unit.clone().into());
    window.set_weather_error(error.unwrap_or_default().into());
    let forecast = view
        .forecast
        .iter()
        .map(|item| ForecastItem {
            label: forecast_label(&item.datetime),
            condition: item.condition.clone().into(),
            temperature: item
                .temperature
                .map(|value| format!("{value:.0}°"))
                .unwrap_or_default()
                .into(),
        })
        .collect();
    window.set_weather_forecast(model(forecast));
    if stale {
        window.set_weather_error("Offline — showing last known weather".into());
    }
}

fn forecast_label(raw: &str) -> SharedString {
    if let Ok(value) = DateTime::parse_from_rfc3339(raw) {
        return SharedString::from(value.format("%H:%M").to_string());
    }
    if let Ok(value) = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S") {
        return SharedString::from(value.format("%H:%M").to_string());
    }
    if let Ok(value) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return SharedString::from(value.format("%a %-d").to_string());
    }
    SharedString::from(raw)
}

pub fn empty_media(volume_step: f32) -> MediaPlayerData {
    MediaPlayerData {
        title: "No media player configured".into(),
        artist: SharedString::new(),
        album: SharedString::new(),
        state_label: "Unavailable".into(),
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
        window.set_media_slider_value(0.0);
        return;
    };
    let controls = media::controls(view);
    let current_art = art
        .current()
        .filter(|current| current.entity_id == view.entity_id);
    let has_art = current_art.is_some();
    let image = current_art
        .map(|current| Image::from_rgba8(current.image.clone()))
        .unwrap_or_default();
    let volume = view.volume_level.unwrap_or(0.0) as f32;
    let data = MediaPlayerData {
        title: media::display_title(view).into(),
        artist: view.artist.clone().unwrap_or_default().into(),
        album: view.album.clone().unwrap_or_default().into(),
        state_label: media::display_state(view).into(),
        volume_label: view
            .volume_level
            .map(|value| format!("{}%", (value * 100.0).round() as i64))
            .unwrap_or_default()
            .into(),
        art: image,
        has_art,
        volume,
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
    };
    window.set_media_slider_value(volume);
    window.set_media(data);
}

pub fn apply_extension_outputs(window: &MainWindow, outputs: &[ExtensionDisplay]) {
    let views: Vec<ExtensionOutputView> = outputs
        .iter()
        .map(|output| ExtensionOutputView {
            id: output.id.clone().into(),
            title: output.title.clone().into(),
            lines: model(
                output
                    .lines
                    .iter()
                    .cloned()
                    .map(SharedString::from)
                    .collect(),
            ),
            error: output.error.clone().unwrap_or_default().into(),
        })
        .collect();
    window.set_extension_outputs(model(views));
}

pub fn apply_departures(window: &MainWindow, view: Option<&DeparturesView>, fallback_title: &str) {
    let display = display_departures(view, &chrono::Local);
    let state = match display.status {
        DeparturesStatus::Loading => DeparturesState::Loading,
        DeparturesStatus::Unavailable => DeparturesState::Unavailable,
        DeparturesStatus::Empty => DeparturesState::Empty,
        DeparturesStatus::Populated => DeparturesState::Populated,
    };
    window.set_departures_state(state);
    window.set_departures_title(
        view.map(|view| view.title.as_str())
            .filter(|title| !title.is_empty())
            .unwrap_or(fallback_title)
            .into(),
    );
    let rows: Vec<DepartureRow> = display
        .rows
        .into_iter()
        .map(|row| DepartureRow {
            line: row.line.into(),
            destination: row.destination.into(),
            time: row.time.into(),
            platform: row.platform.into(),
            delay: row.delay.into(),
            cancelled: row.cancelled,
            remarks: row.remarks.into(),
        })
        .collect();
    window.set_departures_rows(model(rows));
}

pub fn apply_calendar<Tz>(
    window: &MainWindow,
    view: Option<&CalendarView>,
    now: DateTime<Utc>,
    device_tz: &Tz,
) where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let Some(view) = view else {
        window.set_agenda_loading(true);
        window.set_agenda_empty(false);
        window.set_agenda_error(SharedString::new());
        window.set_agenda_status("Connecting…".into());
        window.set_agenda_today(model(Vec::<AgendaEntry>::new()));
        window.set_agenda_tomorrow(model(Vec::<AgendaEntry>::new()));
        return;
    };
    let agenda = group_agenda(&view.items, device_tz, now);
    let empty = agenda.is_empty() && view.errors.is_empty();
    let mut today = agenda.today;
    let mut tomorrow = agenda.tomorrow;
    sort_items(&mut today);
    sort_items(&mut tomorrow);
    window.set_agenda_today(model(agenda_entries(&today, device_tz)));
    window.set_agenda_tomorrow(model(agenda_entries(&tomorrow, device_tz)));
    window.set_agenda_loading(false);
    window.set_agenda_empty(empty);
    window.set_agenda_error(calendar_error_summary(view));
    window.set_agenda_status(if view.stale {
        "Offline — showing cached events".into()
    } else {
        "Up to date".into()
    });
}

fn agenda_entries<Tz>(items: &[crate::state::CalendarItem], device_tz: &Tz) -> Vec<AgendaEntry>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    items
        .iter()
        .map(|item| AgendaEntry {
            title: item.summary.clone().into(),
            when: if item.all_day {
                "All day".into()
            } else {
                let start = item.start.with_timezone(device_tz).format("%H:%M");
                match item.end {
                    Some(end) => {
                        format!("{start}–{}", end.with_timezone(device_tz).format("%H:%M")).into()
                    }
                    None => start.to_string().into(),
                }
            },
            detail: item.location.clone().unwrap_or_default().into(),
            all_day: item.all_day,
        })
        .collect()
}

fn calendar_error_summary(view: &CalendarView) -> SharedString {
    view.errors
        .iter()
        .map(|error| {
            if error.message.to_ascii_lowercase().contains("unavailable") {
                format!("{} unavailable", error.calendar_id)
            } else {
                format!("{}: {}", error.calendar_id, error.message)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
        .into()
}

pub fn shop_entry_view(entry: &ShopEntry, view: &ShopView) -> ShopEntryView {
    let secret_path = entry
        .secret_file(Path::new(&view.secret_dir))
        .display()
        .to_string();
    ShopEntryView {
        id: entry.id.clone().into(),
        name: entry.name.clone().into(),
        version: entry.version.clone().into(),
        author: entry.author.clone().into(),
        license: entry.license.clone().into(),
        description: entry.description.clone().into(),
        status: entry.status().label().into(),
        permissions: entry.permissions_summary().into(),
        config_keys: entry.config_keys_summary().into(),
        config_path: format!("{} {}", view.config_path, entry.settings_table()).into(),
        secret_path: secret_path.into(),
        error: entry.error.clone().unwrap_or_default().into(),
        installed: entry.is_installed(),
        enabled: entry.enabled,
        update_available: entry.status() == ExtensionStatus::UpdateAvailable,
        available: entry.available,
    }
}

pub fn shop_installed(view: &ShopView) -> ModelRc<ShopEntryView> {
    let entries: Vec<ShopEntryView> = view
        .installed_entries()
        .into_iter()
        .map(|entry| shop_entry_view(entry, view))
        .collect();
    model(entries)
}

pub fn shop_available(view: &ShopView) -> ModelRc<ShopEntryView> {
    let entries: Vec<ShopEntryView> = view
        .available_entries()
        .into_iter()
        .map(|entry| shop_entry_view(entry, view))
        .collect();
    model(entries)
}

pub fn extension_errors(view: &ShopView) -> ModelRc<SharedString> {
    let errors: Vec<SharedString> = view
        .extension_errors()
        .into_iter()
        .map(SharedString::from)
        .collect();
    model(errors)
}
