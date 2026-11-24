//! `AppController`: owns `AppState`, the Slint window, and the Home Assistant
//! command sender.
//!
//! Event handling and page navigation are implemented in Wave 2.

use crate::config::Config;
use crate::events::{AppEvent, HaCommand};
use crate::state::AppState;
use crate::ui::AppWindow;
use tokio::sync::mpsc;

pub struct AppController {
    pub state: AppState,
    pub window: AppWindow,
    pub config: Config,
    pub commands: mpsc::Sender<HaCommand>,
}

impl AppController {
    pub fn new(window: AppWindow, config: Config, commands: mpsc::Sender<HaCommand>) -> Self {
        Self {
            state: AppState::default(),
            window,
            config,
            commands,
        }
    }

    /// Apply an event to the state and refresh the UI.
    pub fn apply(&mut self, _event: AppEvent) {
        // Wave 2 updates state and Slint properties here.
    }

    pub fn next_page(&mut self) {
        // Wave 2 navigates the page stack here.
    }

    pub fn previous_page(&mut self) {
        // Wave 2 navigates the page stack here.
    }
}
