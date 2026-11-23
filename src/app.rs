use crate::MainWindow;
use crate::events::{AppEvent, HaCommand, HaEvent, ServiceCall};
use crate::sources::media;
use crate::state::MediaView;
use crate::ui::bridge;
use crate::ui::images::{AlbumArtRequest, AlbumArtState};
use crate::{AppState, Config};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Default)]
struct MediaUi {
    view: Option<MediaView>,
    art: AlbumArtState,
    pending_request: Option<u64>,
    requested_url: Option<String>,
    error: Option<String>,
    next_request_id: u64,
    volume: media::VolumeDebouncer,
    volume_step: f32,
}

pub struct AppController {
    window: MainWindow,
    state: AppState,
    config: Config,
    commands: mpsc::Sender<HaCommand>,
    last_redraw: Instant,
    pending_redraw: bool,
    clock_timer: slint::Timer,
    media: Rc<RefCell<MediaUi>>,
    album_art_requests: Option<mpsc::Sender<AlbumArtRequest>>,
}

impl AppController {
    pub fn new(window: MainWindow, config: Config, commands: mpsc::Sender<HaCommand>) -> Self {
        let media = Rc::new(RefCell::new(MediaUi {
            volume_step: config.widgets.media.volume_step.clamp(0.01, 1.0) as f32,
            ..Default::default()
        }));
        wire_media_callbacks(&window, &media, &commands);

        let weak_next = window.as_weak();
        window.on_swipe_next(move || {
            if let Some(w) = weak_next.upgrade() {
                let p = w.get_current_page();
                let next = if p >= 3 { 0 } else { p + 1 };
                w.set_current_page(next);
            }
        });

        let weak_prev = window.as_weak();
        window.on_swipe_prev(move || {
            if let Some(w) = weak_prev.upgrade() {
                let p = w.get_current_page();
                let prev = if p == 0 { 3 } else { p - 1 };
                w.set_current_page(prev);
            }
        });

        let ctrl = Self {
            window,
            state: AppState::default(),
            config,
            commands,
            last_redraw: Instant::now(),
            pending_redraw: false,
            clock_timer: slint::Timer::default(),
            media,
            album_art_requests: None,
        };

        // Separate 1 Hz clock timer (does not go through the network scheduler).
        let weak_clock = ctrl.window.as_weak();
        ctrl.clock_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                // Clock widget updates are handled by the UI binding; this timer
                // keeps the one-second cadence available for future widgets.
                let _ = &weak_clock;
            },
        );

        ctrl
    }

    /// Hands the controller the channel to the background album-art task.
    pub fn set_album_art_sender(&mut self, sender: mpsc::Sender<AlbumArtRequest>) {
        self.album_art_requests = Some(sender);
    }

    pub fn apply(&mut self, event: AppEvent) {
        match event {
            AppEvent::Ha(ha_event) => {
                if let HaEvent::ServiceResult { request_id, result } = &ha_event {
                    let mut ui = self.media.borrow_mut();
                    if ui.pending_request == Some(*request_id) {
                        ui.pending_request = None;
                        ui.error = match result {
                            Ok(_) => None,
                            Err(error) => Some(error.to_string()),
                        };
                    }
                }
                self.state.apply_ha_event(ha_event);
                refresh(&self.window.as_weak(), &self.media);
            }
            AppEvent::Media(view) => {
                {
                    let mut ui = self.media.borrow_mut();
                    ui.view = view;
                    ui.pending_request = None;
                    if ui.view.is_none() {
                        ui.art.clear();
                        ui.requested_url = None;
                    }
                }
                self.request_album_art();
                refresh(&self.window.as_weak(), &self.media);
            }
            AppEvent::AlbumArt { entity_id, image } => {
                let url = self
                    .media
                    .borrow()
                    .view
                    .as_ref()
                    .and_then(|view| view.entity_picture.as_deref())
                    .and_then(|picture| media::resolve_image_url(&self.config.ha.url, picture))
                    .map(|url| url.to_string())
                    .unwrap_or_default();
                self.media
                    .borrow_mut()
                    .art
                    .apply_loaded(entity_id, url, image);
                refresh(&self.window.as_weak(), &self.media);
            }
            AppEvent::ImageFailed { reason, .. } => {
                self.media.borrow_mut().art.apply_failed(reason);
                refresh(&self.window.as_weak(), &self.media);
            }
            AppEvent::Weather(r) => {
                if let Ok(v) = r {
                    self.state.weather = Some(v);
                }
            }
            AppEvent::Departures(r) => {
                if let Ok(v) = r {
                    self.state.departures = Some(v);
                }
            }
            AppEvent::Calendar(r) => {
                if let Ok(v) = r {
                    self.state.calendar = v;
                }
            }
        }

        // Coalesce redraws: at most 4 per second.
        let now = Instant::now();
        if now.duration_since(self.last_redraw) >= Duration::from_millis(250) {
            self.last_redraw = now;
            self.pending_redraw = false;
        } else {
            self.pending_redraw = true;
        }
    }

    fn request_album_art(&mut self) {
        let Some(sender) = &self.album_art_requests else {
            return;
        };
        let Some(view) = self.media.borrow().view.clone() else {
            return;
        };
        if !self.config.widgets.media.show_album_art {
            let mut ui = self.media.borrow_mut();
            ui.art.clear();
            ui.requested_url = None;
            return;
        }
        let Some(picture) = view.entity_picture.as_deref() else {
            let mut ui = self.media.borrow_mut();
            ui.art.clear();
            ui.requested_url = None;
            return;
        };
        let Some(url) = media::resolve_image_url(&self.config.ha.url, picture) else {
            return;
        };
        {
            let ui = self.media.borrow();
            let already_known = ui
                .art
                .current_url()
                .is_some_and(|current| current == url.as_str())
                || ui.requested_url.as_deref() == Some(url.as_str());
            if already_known {
                return;
            }
        }
        if sender
            .try_send(AlbumArtRequest {
                entity_id: view.entity_id,
                url: url.clone(),
            })
            .is_ok()
        {
            self.media.borrow_mut().requested_url = Some(url.to_string());
        }
    }

    pub fn next_page(&mut self) {
        let p = self.window.get_current_page();
        let next = if p >= 3 { 0 } else { p + 1 };
        self.window.set_current_page(next);
    }

    pub fn previous_page(&mut self) {
        let p = self.window.get_current_page();
        let prev = if p == 0 { 3 } else { p - 1 };
        self.window.set_current_page(prev);
    }

    pub fn window(&self) -> &MainWindow {
        &self.window
    }
}

fn wire_media_callbacks(
    window: &MainWindow,
    media_ui: &Rc<RefCell<MediaUi>>,
    commands: &mpsc::Sender<HaCommand>,
) {
    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_play_pause(move || {
        dispatch_transport(
            &ui,
            &tx,
            &weak,
            media::can_play_pause,
            media::play_pause_call,
        );
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_previous_track(move || {
        dispatch_transport(
            &ui,
            &tx,
            &weak,
            media::can_previous,
            media::previous_track_call,
        );
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_next_track(move || {
        dispatch_transport(&ui, &tx, &weak, media::can_next, media::next_track_call);
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_volume_changed(move |value| {
        let mut state = ui.borrow_mut();
        let Some(view) = state.view.clone() else {
            return;
        };
        if !media::can_set_volume(&view) {
            return;
        }
        let Some(volume) = state.volume.on_change(value, Instant::now()) else {
            return;
        };
        state.next_request_id += 1;
        let call = media::volume_call(state.next_request_id, &view.entity_id, volume);
        if tx.try_send(HaCommand::CallService(call)).is_err() {
            state.error = Some("Home Assistant is not connected".to_string());
        }
        drop(state);
        refresh(&weak, &ui);
    });

    let weak = window.as_weak();
    let ui = Rc::clone(media_ui);
    let tx = commands.clone();
    window.on_media_volume_released(move |value| {
        let mut state = ui.borrow_mut();
        let Some(view) = state.view.clone() else {
            return;
        };
        if !media::can_set_volume(&view) {
            return;
        }
        let volume = state.volume.on_release(value, Instant::now());
        state.next_request_id += 1;
        let call = media::volume_call(state.next_request_id, &view.entity_id, volume);
        if tx.try_send(HaCommand::CallService(call)).is_err() {
            state.error = Some("Home Assistant is not connected".to_string());
        }
        drop(state);
        refresh(&weak, &ui);
    });
}

fn dispatch_transport(
    ui: &Rc<RefCell<MediaUi>>,
    commands: &mpsc::Sender<HaCommand>,
    weak: &slint::Weak<MainWindow>,
    allowed: fn(&MediaView) -> bool,
    build: fn(u64, &str) -> ServiceCall,
) {
    let mut state = ui.borrow_mut();
    if state.pending_request.is_some() {
        return;
    }
    let Some(view) = state.view.clone() else {
        return;
    };
    if !allowed(&view) {
        return;
    }
    state.next_request_id += 1;
    let request_id = state.next_request_id;
    let call = build(request_id, &view.entity_id);
    match commands.try_send(HaCommand::CallService(call)) {
        Ok(()) => {
            state.pending_request = Some(request_id);
            state.error = None;
        }
        Err(_) => state.error = Some("Home Assistant is not connected".to_string()),
    }
    drop(state);
    refresh(weak, ui);
}

fn refresh(window: &slint::Weak<MainWindow>, media_ui: &Rc<RefCell<MediaUi>>) {
    let Some(window) = window.upgrade() else {
        return;
    };
    let ui = media_ui.borrow();
    bridge::apply_media(
        &window,
        ui.view.as_ref(),
        &ui.art,
        ui.pending_request.is_some(),
        ui.error.as_deref(),
        ui.volume_step,
    );
}
