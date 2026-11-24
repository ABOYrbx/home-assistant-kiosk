use crate::state::ConnectionState;
use chrono::NaiveDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionView {
    pub label: String,
    pub connected: bool,
    pub warning: bool,
    pub error: bool,
}

pub fn connection_view(state: &ConnectionState, stale: bool) -> ConnectionView {
    match state {
        ConnectionState::Connected if stale => ConnectionView {
            label: "Stale".to_string(),
            connected: false,
            warning: true,
            error: false,
        },
        ConnectionState::Connected => ConnectionView {
            label: "Connected".to_string(),
            connected: true,
            warning: false,
            error: false,
        },
        ConnectionState::Connecting => ConnectionView {
            label: "Connecting".to_string(),
            connected: false,
            warning: true,
            error: false,
        },
        ConnectionState::AuthFailed => ConnectionView {
            label: "Auth failed".to_string(),
            connected: false,
            warning: false,
            error: true,
        },
        ConnectionState::Disconnected => ConnectionView {
            label: "Offline".to_string(),
            connected: false,
            warning: true,
            error: false,
        },
    }
}

pub fn format_clock(now: &NaiveDateTime, use_24_hour: bool, show_seconds: bool) -> String {
    let format = match (use_24_hour, show_seconds) {
        (true, true) => "%H:%M:%S",
        (true, false) => "%H:%M",
        (false, true) => "%-I:%M:%S %p",
        (false, false) => "%-I:%M %p",
    };
    now.format(format).to_string()
}

pub fn format_date(now: &NaiveDateTime) -> String {
    now.format("%a %-d %b").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sample() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(14, 32, 7)
            .unwrap()
    }

    #[test]
    fn formats_24_hour_clock() {
        assert_eq!(format_clock(&sample(), true, false), "14:32");
        assert_eq!(format_clock(&sample(), true, true), "14:32:07");
    }

    #[test]
    fn formats_12_hour_clock() {
        assert_eq!(format_clock(&sample(), false, false), "2:32 PM");
        assert_eq!(format_clock(&sample(), false, true), "2:32:07 PM");
    }

    #[test]
    fn formats_date_without_a_leading_zero() {
        assert_eq!(format_date(&sample()), "Mon 23 Nov");
    }

    #[test]
    fn maps_connection_states_honestly() {
        let connected = connection_view(&ConnectionState::Connected, false);
        assert!(connected.connected);
        assert_eq!(connected.label, "Connected");

        let stale = connection_view(&ConnectionState::Connected, true);
        assert!(stale.warning);
        assert_eq!(stale.label, "Stale");

        let auth = connection_view(&ConnectionState::AuthFailed, false);
        assert!(auth.error);
        assert_eq!(auth.label, "Auth failed");

        let offline = connection_view(&ConnectionState::Disconnected, true);
        assert!(offline.warning);
        assert_eq!(offline.label, "Offline");
    }
}
