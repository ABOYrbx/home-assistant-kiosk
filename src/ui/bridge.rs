// UI bridge: AppState -> Slint properties/models.

use crate::sources::calendar::group_agenda;
use crate::state::{AppState, CalendarItem, CalendarView};
use crate::{AgendaEntry, MainWindow};
use chrono::{DateTime, TimeZone, Utc};
use slint::{ModelRc, SharedString, VecModel};
use std::fmt::Display;

pub struct UiBridge;

impl UiBridge {
    pub fn apply(state: &AppState, window: &MainWindow) {
        Self::apply_calendar(window, state.calendar.as_ref(), Utc::now(), &chrono::Local);
    }

    pub fn apply_calendar<Tz>(
        window: &MainWindow,
        view: Option<&CalendarView>,
        now: DateTime<Utc>,
        device_tz: &Tz,
    ) where
        Tz: TimeZone,
        Tz::Offset: Display,
    {
        let Some(view) = view else {
            window.set_agenda_loading(true);
            window.set_agenda_empty(false);
            window.set_agenda_error(SharedString::default());
            window.set_agenda_status(SharedString::default());
            window.set_agenda_today(empty_model());
            window.set_agenda_tomorrow(empty_model());
            return;
        };

        let agenda = group_agenda(&view.items, device_tz, now);
        window.set_agenda_loading(false);
        window.set_agenda_empty(agenda.is_empty() && view.last_success.is_some());
        window.set_agenda_error(error_text(view));
        window.set_agenda_status(status_text(view, device_tz));
        window.set_agenda_today(entry_model(&agenda.today, device_tz));
        window.set_agenda_tomorrow(entry_model(&agenda.tomorrow, device_tz));
    }
}

fn empty_model() -> ModelRc<AgendaEntry> {
    ModelRc::new(VecModel::from(Vec::<AgendaEntry>::new()))
}

fn entry_model<Tz>(items: &[CalendarItem], device_tz: &Tz) -> ModelRc<AgendaEntry>
where
    Tz: TimeZone,
    Tz::Offset: Display,
{
    let rows: Vec<AgendaEntry> = items
        .iter()
        .map(|item| AgendaEntry {
            title: item.summary.as_str().into(),
            when: format_when(item, device_tz).into(),
            detail: item
                .location
                .clone()
                .unwrap_or_else(|| item.calendar_id.clone())
                .into(),
            all_day: item.all_day,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn format_when<Tz>(item: &CalendarItem, device_tz: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: Display,
{
    if item.all_day {
        return "All day".to_string();
    }
    let start = item.start.with_timezone(device_tz).format("%H:%M");
    match item.end {
        Some(end) => {
            let end = end.with_timezone(device_tz).format("%H:%M");
            format!("{start}–{end}")
        }
        None => start.to_string(),
    }
}

fn error_text(view: &CalendarView) -> SharedString {
    if view.errors.is_empty() {
        return SharedString::default();
    }
    let names: Vec<&str> = view
        .errors
        .iter()
        .map(|error| error.calendar_id.as_str())
        .filter(|id| !id.is_empty())
        .collect();
    match names.as_slice() {
        [] => "Calendar data unavailable".into(),
        [id] => format!("{id} unavailable").into(),
        _ => format!("{} calendars unavailable", names.len()).into(),
    }
}

fn status_text<Tz>(view: &CalendarView, device_tz: &Tz) -> SharedString
where
    Tz: TimeZone,
    Tz::Offset: Display,
{
    if view.stale {
        return "Offline — showing cached events".into();
    }
    match view.last_success {
        Some(last_success) => {
            let time = last_success.with_timezone(device_tz).format("%H:%M");
            format!("Updated {time}").into()
        }
        None => SharedString::default(),
    }
}
