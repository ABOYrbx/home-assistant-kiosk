//! Declarative extension output schema.
//!
//! An extension returns one of three view models. The host validates the shape,
//! rejects unknown fields, and caps the number of items. The built-in widgets
//! render the result; extensions never draw their own UI.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::manifest::OutputKind;

/// Hard ceiling for rows or items in any extension output.
pub const MAX_OUTPUT_ITEMS: usize = 100;

/// A validated extension view model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtensionOutput {
    Departures(DeparturesView),
    List(ListView),
    KeyValue(KeyValueView),
}

/// A display-ready extension card. The kiosk UI never receives Rhai values or
/// arbitrary markup, only these validated text lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDisplay {
    pub id: String,
    pub title: String,
    pub lines: Vec<String>,
    pub error: Option<String>,
}

impl ExtensionOutput {
    /// Flatten a validated output into text lines for the built-in card.
    ///
    /// The three output kinds keep their meaning: departures stay one row per
    /// trip, list output keeps column order, and key-value output keeps each
    /// label next to its value.
    pub fn display(&self, id: &str, error: Option<String>) -> ExtensionDisplay {
        let (title, lines) = match self {
            Self::Departures(view) => {
                let title = view.title.clone().unwrap_or_else(|| id.to_string());
                let lines = view
                    .items
                    .iter()
                    .map(|item| {
                        let mut parts = vec![
                            item.line.clone(),
                            item.destination.clone(),
                            item.departure.clone(),
                        ];
                        if let Some(delay) = item.delay_minutes
                            && delay != 0
                        {
                            parts.push(format!("{delay:+} min"));
                        }
                        if let Some(platform) = &item.platform
                            && !platform.is_empty()
                        {
                            parts.push(format!("Platform {platform}"));
                        }
                        if item.cancelled {
                            parts.push("Cancelled".to_string());
                        }
                        if let Some(remarks) = &item.remarks
                            && !remarks.is_empty()
                        {
                            parts.push(remarks.clone());
                        }
                        parts.join(" · ")
                    })
                    .collect();
                (title, lines)
            }
            Self::List(view) => {
                let title = view.title.clone().unwrap_or_else(|| id.to_string());
                let mut lines = vec![view.columns.join(" · ")];
                lines.extend(view.rows.iter().map(|row| row.join(" · ")));
                (title, lines)
            }
            Self::KeyValue(view) => {
                let title = view.title.clone().unwrap_or_else(|| id.to_string());
                let lines = view
                    .items
                    .iter()
                    .map(|item| format!("{}: {}", item.label, item.value))
                    .collect();
                (title, lines)
            }
        };
        ExtensionDisplay {
            id: id.to_string(),
            title,
            lines,
            error,
        }
    }
}

impl ExtensionOutput {
    pub fn kind(&self) -> OutputKind {
        match self {
            Self::Departures(_) => OutputKind::Departures,
            Self::List(_) => OutputKind::List,
            Self::KeyValue(_) => OutputKind::KeyValue,
        }
    }

    pub fn item_count(&self) -> usize {
        match self {
            Self::Departures(view) => view.items.len(),
            Self::List(view) => view.rows.len(),
            Self::KeyValue(view) => view.items.len(),
        }
    }
}

/// A departures view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeparturesView {
    pub kind: OutputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub items: Vec<DepartureItem>,
}

/// One departure row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureItem {
    pub line: String,
    pub destination: String,
    pub departure: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remarks: Option<String>,
}

/// A tabular list view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListView {
    pub kind: OutputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub columns: Vec<String>,
    #[serde(deserialize_with = "deserialize_rows")]
    pub rows: Vec<Vec<String>>,
}

/// A label/value view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyValueView {
    pub kind: OutputKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub items: Vec<KeyValueItem>,
}

/// One label/value pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyValueItem {
    pub label: String,
    #[serde(deserialize_with = "deserialize_text")]
    pub value: String,
}

fn deserialize_rows<'de, D>(deserializer: D) -> Result<Vec<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let rows = Vec::<Vec<Value>>::deserialize(deserializer)?;
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(output_text)
                .collect::<Result<Vec<_>, _>>()
                .map_err(serde::de::Error::custom)
        })
        .collect()
}

fn deserialize_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    output_text(value).map_err(serde::de::Error::custom)
}

fn output_text(value: Value) -> Result<String, &'static str> {
    match value {
        Value::String(text) => Ok(text),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => Err("expected a string, number, or boolean"),
    }
}

/// Output validation errors.
///
/// These intentionally omit raw output values so a script cannot leak a
/// configured secret through an error shown in the Shop or Status page.
#[derive(Debug, Error, PartialEq)]
pub enum OutputError {
    #[error("extension output must be a map")]
    NotAMap,
    #[error("extension output is missing the kind field")]
    MissingKind,
    #[error("unknown extension output kind")]
    UnknownKind,
    #[error("extension output kind {found} does not match the declared {expected}")]
    KindMismatch {
        found: OutputKind,
        expected: OutputKind,
    },
    #[error("malformed {kind} output: {reason}")]
    Malformed { kind: OutputKind, reason: String },
}

/// Validate a raw script result against the declared output kind.
///
/// Items beyond the effective cap are dropped after every item is validated,
/// so malformed output cannot hide behind truncation.
pub fn validate_output(
    value: Value,
    expected: OutputKind,
    max_items: usize,
) -> Result<ExtensionOutput, OutputError> {
    let max_items = max_items.min(MAX_OUTPUT_ITEMS);
    let object = value.as_object().ok_or(OutputError::NotAMap)?;
    let kind_value = object.get("kind").ok_or(OutputError::MissingKind)?;
    let kind: OutputKind =
        serde_json::from_value(kind_value.clone()).map_err(|_| OutputError::UnknownKind)?;
    if kind != expected {
        return Err(OutputError::KindMismatch {
            found: kind,
            expected,
        });
    }

    match kind {
        OutputKind::Departures => {
            let mut view: DeparturesView = parse(value, kind)?;
            validate_departures(&view)?;
            view.items.truncate(max_items);
            Ok(ExtensionOutput::Departures(view))
        }
        OutputKind::List => {
            let mut view: ListView = parse(value, kind)?;
            validate_list(&view)?;
            view.rows.truncate(max_items);
            Ok(ExtensionOutput::List(view))
        }
        OutputKind::KeyValue => {
            let mut view: KeyValueView = parse(value, kind)?;
            validate_key_value(&view)?;
            view.items.truncate(max_items);
            Ok(ExtensionOutput::KeyValue(view))
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value, kind: OutputKind) -> Result<T, OutputError> {
    serde_json::from_value(value).map_err(|_| OutputError::Malformed {
        kind,
        reason: "invalid field type or unknown field".to_string(),
    })
}

fn validate_departures(view: &DeparturesView) -> Result<(), OutputError> {
    for item in &view.items {
        if item.line.trim().is_empty() {
            return Err(malformed(OutputKind::Departures, "line must not be empty"));
        }
        if item.destination.trim().is_empty() {
            return Err(malformed(
                OutputKind::Departures,
                "destination must not be empty",
            ));
        }
        if item.departure.trim().is_empty() {
            return Err(malformed(
                OutputKind::Departures,
                "departure must not be empty",
            ));
        }
    }
    Ok(())
}

fn validate_list(view: &ListView) -> Result<(), OutputError> {
    if view.columns.is_empty() {
        return Err(malformed(OutputKind::List, "columns must not be empty"));
    }
    for row in &view.rows {
        if row.len() != view.columns.len() {
            return Err(malformed(
                OutputKind::List,
                "every row must have the same length as columns",
            ));
        }
    }
    Ok(())
}

fn validate_key_value(view: &KeyValueView) -> Result<(), OutputError> {
    for item in &view.items {
        if item.label.trim().is_empty() {
            return Err(malformed(OutputKind::KeyValue, "label must not be empty"));
        }
    }
    Ok(())
}

fn malformed(kind: OutputKind, reason: &str) -> OutputError {
    OutputError::Malformed {
        kind,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn departures_display_one_row_per_trip() {
        let output = ExtensionOutput::Departures(DeparturesView {
            kind: OutputKind::Departures,
            title: Some("Buses".into()),
            items: vec![DepartureItem {
                line: "42".into(),
                destination: "Central Station".into(),
                departure: "18:05".into(),
                delay_minutes: Some(2),
                platform: Some("2".into()),
                cancelled: false,
                remarks: None,
            }],
        });
        let display = output.display("org.example.bus", None);
        assert_eq!(display.title, "Buses");
        assert_eq!(
            display.lines,
            vec!["42 · Central Station · 18:05 · +2 min · Platform 2"]
        );
    }

    #[test]
    fn list_display_keeps_columns_and_rows() {
        let output = ExtensionOutput::List(ListView {
            kind: OutputKind::List,
            title: None,
            columns: vec!["Line".into(), "Time".into()],
            rows: vec![vec!["42".into(), "18:05".into()]],
        });
        let display = output.display("org.example.list", Some("stale".into()));
        assert_eq!(display.title, "org.example.list");
        assert_eq!(display.lines, vec!["Line · Time", "42 · 18:05"]);
        assert_eq!(display.error.as_deref(), Some("stale"));
    }

    #[test]
    fn key_value_display_keeps_labels_and_values() {
        let output = ExtensionOutput::KeyValue(KeyValueView {
            kind: OutputKind::KeyValue,
            title: Some("Stop".into()),
            items: vec![KeyValueItem {
                label: "Updated".into(),
                value: "18:00".into(),
            }],
        });
        let display = output.display("org.example.status", None);
        assert_eq!(display.lines, vec!["Updated: 18:00"]);
    }
}
