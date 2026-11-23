use crate::ha::models::HaEntity;
use crate::state::{DepartureItem, DeparturesView};
use chrono::{DateTime, Utc};

pub fn parse_departures(
    entity: &HaEntity,
    title: &str,
    tz: chrono_tz::Tz,
    max_items: usize,
    now: DateTime<Utc>,
) -> DeparturesView {
    let mut view = DeparturesView {
        entity_id: entity.entity_id.clone(),
        title: title.to_string(),
        items: Vec::new(),
        available: true,
    };

    let attr = entity.attributes.get("departures");
    let Some(arr) = attr.and_then(|v| v.as_array()) else {
        // missing or non-array attribute => unavailable
        if attr.is_none() || attr.map(|v| v.is_null()).unwrap_or(false) {
            view.available = false;
        } else {
            // present but not an array => unavailable
            view.available = false;
        }
        return view;
    };

    let cutoff = now - chrono::Duration::seconds(60);
    let mut parsed: Vec<(DepartureItem, DateTime<Utc>)> = Vec::new();
    for item in arr {
        let obj = match item.as_object() {
            Some(o) => o,
            None => continue,
        };
        let line = obj.get("line").and_then(|v| v.as_str());
        let destination = obj.get("destination").and_then(|v| v.as_str());
        let departure = obj.get("departure").and_then(|v| v.as_str());
        let (Some(line), Some(destination), Some(departure)) = (line, destination, departure)
        else {
            continue;
        };
        let sort_key = match parse_departure_time(departure, tz) {
            Some(dt) => dt,
            None => continue,
        };
        if sort_key < cutoff {
            continue;
        }
        let delay_minutes = obj.get("delay_minutes").and_then(|v| v.as_i64());
        let platform = obj
            .get("platform")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let cancelled = obj
            .get("cancelled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let remarks = obj
            .get("remarks")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        parsed.push((
            DepartureItem {
                line: line.to_string(),
                destination: destination.to_string(),
                departure: departure.to_string(),
                delay_minutes,
                platform,
                cancelled,
                remarks,
            },
            sort_key,
        ));
    }

    parsed.sort_by_key(|(_, k)| *k);
    for (item, _) in parsed.into_iter().take(max_items) {
        view.items.push(item);
    }
    view
}

fn parse_departure_time(s: &str, tz: chrono_tz::Tz) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    let fmts = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"];
    for fmt in &fmts {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            let local = ndt.and_local_timezone(tz).single()?;
            return Some(local.with_timezone(&Utc));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ha::models::HaEntity;
    use chrono::TimeZone;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 22, 20, 0, 0).unwrap()
    }

    #[test]
    fn parse_basic() {
        let e = HaEntity {
            entity_id: "sensor.departures".into(),
            state: "ok".into(),
            attributes: json!({"departures": [
                {"line":"42","destination":"Central","departure":"2026-11-22T21:55:00+01:00","delay_minutes":2,"platform":"2"},
                {"line":"S1","destination":"Airport","departure":"2026-11-22 22:03:00","cancelled":true}
            ]}),
            last_changed: None,
            last_updated: None,
        };
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        let v = parse_departures(&e, "Departures", tz, 6, now());
        assert!(v.available);
        assert_eq!(v.items.len(), 2);
        assert_eq!(v.items[0].line, "42");
        assert!(v.items[1].cancelled);
    }

    #[test]
    fn missing_attribute_unavailable() {
        let e = HaEntity {
            entity_id: "sensor.departures".into(),
            state: "ok".into(),
            attributes: json!({}),
            last_changed: None,
            last_updated: None,
        };
        let tz: chrono_tz::Tz = "UTC".parse().unwrap();
        let v = parse_departures(&e, "D", tz, 6, now());
        assert!(!v.available);
        assert!(v.items.is_empty());
    }

    #[test]
    fn empty_list_no_departures() {
        let e = HaEntity {
            entity_id: "sensor.departures".into(),
            state: "ok".into(),
            attributes: json!({"departures": []}),
            last_changed: None,
            last_updated: None,
        };
        let tz: chrono_tz::Tz = "UTC".parse().unwrap();
        let v = parse_departures(&e, "D", tz, 6, now());
        assert!(v.available);
        assert!(v.items.is_empty());
    }

    #[test]
    fn drops_past() {
        let e = HaEntity {
            entity_id: "sensor.departures".into(),
            state: "ok".into(),
            attributes: json!({"departures": [
                {"line":"1","destination":"X","departure":"2026-11-22T20:00:00+01:00"}
            ]}),
            last_changed: None,
            last_updated: None,
        };
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        let v = parse_departures(&e, "D", tz, 6, now());
        assert!(v.items.is_empty());
    }
}
