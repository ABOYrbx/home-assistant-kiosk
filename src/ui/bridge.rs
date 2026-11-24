//! UI bridge: `AppState` to Slint properties and models.
//!
//! Everything here runs on the Slint UI thread. `AppController::apply` folds a
//! background `AppEvent` into `AppState` and then calls [`UiBridge::apply`], so
//! background tasks never touch a Slint handle directly. Widget-specific
//! bindings arrive in later waves.

use crate::{AppState, ConnectionState, MainWindow};
use slint::{ModelRc, VecModel};

pub struct UiBridge;

impl UiBridge {
    pub fn apply(window: &MainWindow, state: &AppState) {
        window.set_connection_status(connection_label(state.connection.clone()).into());
        window.set_stale(state.stale);
        window.set_ha_version(state.ha_version.clone().unwrap_or_default().into());
        window.set_entity_count(i32::try_from(state.entities.len()).unwrap_or(i32::MAX));
        window.set_last_success(
            state
                .last_success
                .map(|instant| instant.to_rfc3339())
                .unwrap_or_default()
                .into(),
        );

        // Keep the model deterministic: `HashMap` iteration order is not stable.
        let mut entity_ids: Vec<slint::SharedString> = state
            .entities
            .keys()
            .cloned()
            .map(slint::SharedString::from)
            .collect();
        entity_ids.sort();
        window.set_entity_ids(ModelRc::new(VecModel::from(entity_ids)));
    }
}

fn connection_label(connection: ConnectionState) -> &'static str {
    match connection {
        ConnectionState::Disconnected => "Disconnected",
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Connected => "Connected",
        ConnectionState::AuthFailed => "Authentication failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_labels_are_stable() {
        assert_eq!(
            connection_label(ConnectionState::Disconnected),
            "Disconnected"
        );
        assert_eq!(connection_label(ConnectionState::Connecting), "Connecting");
        assert_eq!(connection_label(ConnectionState::Connected), "Connected");
        assert_eq!(
            connection_label(ConnectionState::AuthFailed),
            "Authentication failed"
        );
    }
}
