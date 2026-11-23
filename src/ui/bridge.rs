//! UI bridge: `AppState` -> Slint properties and models.
//!
//! View models stay Slint-free; only this module knows about Slint types.

use crate::sources::departures::{DeparturesStatus, display_departures};
use crate::state::DeparturesView;
use crate::{AppState, DepartureRow, DeparturesState, MainWindow};
use chrono::Local;

pub struct UiBridge;

impl UiBridge {
    pub fn apply(
        state: &AppState,
        window: &MainWindow,
        departures_title: &str,
        departures_enabled: bool,
    ) {
        window.set_departures_visible(departures_enabled);
        Self::apply_departures(window, state.departures.as_ref(), departures_title);
    }

    /// Render one departures view, keeping the empty, unavailable, and
    /// populated states visibly distinct.
    pub fn apply_departures(
        window: &MainWindow,
        view: Option<&DeparturesView>,
        fallback_title: &str,
    ) {
        let display = display_departures(view, &Local);
        window.set_departures_title(
            view.map(|view| view.title.as_str())
                .unwrap_or(fallback_title)
                .into(),
        );
        window.set_departures_state(match display.status {
            DeparturesStatus::Loading => DeparturesState::Loading,
            DeparturesStatus::Unavailable => DeparturesState::Unavailable,
            DeparturesStatus::Empty => DeparturesState::Empty,
            DeparturesStatus::Populated => DeparturesState::Populated,
        });
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
        window.set_departures_rows(slint::ModelRc::new(slint::VecModel::from(rows)));
    }
}
