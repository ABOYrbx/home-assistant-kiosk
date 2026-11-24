// UI bridge: AppState -> Slint properties. Wave 2 only exposes shell chrome;
// widget-specific models arrive in later waves.

use crate::{AppState, ConnectionState, MainWindow};

pub struct UiBridge;

impl UiBridge {
    pub fn apply(window: &MainWindow, state: &AppState) {
        let (label, connected) = match state.connection {
            ConnectionState::Connected => ("Connected", true),
            ConnectionState::Connecting => ("Connecting", false),
            ConnectionState::AuthFailed => ("Auth failed", false),
            ConnectionState::Disconnected => ("Disconnected", false),
        };
        window.set_connection_text(label.into());
        window.set_connected(connected);
        window.set_stale(state.stale);
        window.set_ha_version(state.ha_version.clone().unwrap_or_default().into());
    }
}
