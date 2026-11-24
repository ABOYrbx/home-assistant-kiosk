use crate::events::ServiceCall;
use crate::state::CalendarItem;
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};

pub fn parse_calendar_events(
    value: &serde_json::Value,
    calendar_id: &str,
    ha_tz: chrono_tz::Tz,
    device_tz: chrono_tz::Tz,
) -> Vec<CalendarItem> {
    let events_arr: Option<Vec<serde_json::Value>> = value
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| {
            value.as_object().and_then(|o| {
                o.values()
                    .find_map(|v| v.get("events").and_then(|e| e.as_array()).cloned())
            })
        })
        .or_else(|| {
            value
                .as_object()
                .and_then(|o| o.values().find_map(|v| v.as_array().cloned()))
        })
        .or_else(|| value.as_array().cloned());

    let Some(arr) = events_arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ev in &arr {
        if let Some(item) = parse_event(ev, calendar_id, ha_tz, device_tz) {
            out.push(item);
        }
    }
    out
}

#[allow(clippy::question_mark, clippy::collapsible_if)]
fn parse_event(
    ev: &serde_json::Value,
    calendar_id: &str,
    ha_tz: chrono_tz::Tz,
    device_tz: chrono_tz::Tz,
) -> Option<CalendarItem> {
    let summary = ev
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("(no title)")
        .to_string();

    let start_obj = ev.get("start")?;
    let (start_str, all_day) = if let Some(d) = start_obj.get("date").and_then(|v| v.as_str()) {
        (d.to_string(), true)
    } else if let Some(dt) = start_obj.get("dateTime").and_then(|v| v.as_str()) {
        (convert_dt(dt, ha_tz, device_tz), false)
    } else if let Some(s) = start_obj.as_str() {
        (convert_dt(s, ha_tz, device_tz), false)
    } else {
        return None;
    };

    let end_str = if let Some(end_obj) = ev.get("end") {
        if let Some(d) = end_obj.get("date").and_then(|v| v.as_str()) {
            d.to_string()
        } else if let Some(dt) = end_obj.get("dateTime").and_then(|v| v.as_str()) {
            convert_dt(dt, ha_tz, device_tz)
        } else if let Some(s) = end_obj.as_str() {
            convert_dt(s, ha_tz, device_tz)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    Some(CalendarItem {
        calendar_id: calendar_id.to_string(),
        summary,
        start: start_str,
        end: end_str,
        all_day,
    })
}

fn convert_dt(s: &str, ha_tz: chrono_tz::Tz, device_tz: chrono_tz::Tz) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&device_tz).to_rfc3339();
    }
    let fmts = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"];
    for fmt in &fmts {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt)
            && let Some(local) = ndt.and_local_timezone(ha_tz).single()
        {
            return local.with_timezone(&device_tz).to_rfc3339();
        }
    }
    s.to_string()
}

/// Build one `calendar.get_events` call per configured calendar.
///
/// The range starts at the beginning of today in the Home Assistant timezone
/// and covers `days` whole days, so an event that started earlier today and is
/// still running is included.
pub fn build_calendar_calls(
    calendar_ids: &[crate::events::EntityId],
    days: u32,
    ha_tz: chrono_tz::Tz,
    now: DateTime<Utc>,
) -> Vec<ServiceCall> {
    let start = now
        .with_timezone(&ha_tz)
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| ha_tz.from_local_datetime(&naive).earliest());
    let Some(start) = start else {
        return Vec::new();
    };
    let end = start + ChronoDuration::days(i64::from(days.max(1)));

    calendar_ids
        .iter()
        .map(|id| ServiceCall {
            request_id: crate::ha::client::next_request_id(),
            domain: "calendar".to_string(),
            service: "get_events".to_string(),
            target: Some(serde_json::json!({ "entity_id": id })),
            data: serde_json::json!({
                "start_date_time": start.to_rfc3339(),
                "end_date_time": end.to_rfc3339(),
            }),
            return_response: true,
        })
        .collect()
}

pub fn merge_and_sort(items: &mut [CalendarItem]) {
    items.sort_by(|a, b| a.start.cmp(&b.start));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_timed_and_allday() {
        let v = json!({"events": [
            {"summary":"Meeting","start":{"dateTime":"2026-11-23T10:00:00+01:00"},"end":{"dateTime":"2026-11-23T11:00:00+01:00"}},
            {"summary":"Holiday","start":{"date":"2026-11-24"},"end":{"date":"2026-11-25"}}
        ]});
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        let items = parse_calendar_events(&v, "calendar.family", tz, tz);
        assert_eq!(items.len(), 2);
        assert!(items[1].all_day);
    }

    #[test]
    fn merge_sort() {
        let mut items = vec![
            CalendarItem {
                calendar_id: "a".into(),
                summary: "b".into(),
                start: "2026-11-23T12:00:00+01:00".into(),
                end: String::new(),
                all_day: false,
            },
            CalendarItem {
                calendar_id: "a".into(),
                summary: "a".into(),
                start: "2026-11-23T10:00:00+01:00".into(),
                end: String::new(),
                all_day: false,
            },
        ];
        merge_and_sort(&mut items);
        assert_eq!(items[0].summary, "a");
    }

    #[test]
    fn one_failing_calendar_does_not_hide() {
        // parse returns empty for malformed; merge keeps others
        let bad = json!({"error": "boom"});
        let tz: chrono_tz::Tz = "UTC".parse().unwrap();
        let items = parse_calendar_events(&bad, "c", tz, tz);
        assert!(items.is_empty());
    }
}
