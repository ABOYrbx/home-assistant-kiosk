use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn css_tokens() -> Vec<(String, String)> {
    let text = read(&root().join("tokens.css"));
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("--")?;
            let (name, value) = rest.split_once(':')?;
            Some((
                name.trim().to_string(),
                value.trim().trim_end_matches(';').trim().to_string(),
            ))
        })
        .collect()
}

fn slint_tokens() -> HashMap<String, (String, String)> {
    let text = read(&root().join("ui/theme.slint"));
    let property =
        Regex::new(r"(?m)^\s*in-out property <([^>]+)>\s+([A-Za-z0-9_-]+)\s*:\s*(.+?);\s*$")
            .unwrap();
    property
        .captures_iter(&text)
        .map(|captures| {
            (
                captures[2].to_string(),
                (captures[1].to_string(), captures[3].trim().to_string()),
            )
        })
        .collect()
}

fn expected_theme_names(css_name: &str) -> Vec<String> {
    match css_name {
        "elev-raised" => vec![
            "elev-raised-y".into(),
            "elev-raised-blur".into(),
            "elev-raised-color".into(),
        ],
        "focus-ring" => vec!["focus-ring-width".into(), "focus-ring-color".into()],
        _ => vec![css_name.to_string()],
    }
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .replace([' ', '\t', '"'], "")
        .to_ascii_lowercase()
}

fn compared_directly(css_name: &str, css_value: &str) -> bool {
    !matches!(
        css_name,
        "elev-flat" | "elev-ring" | "elev-raised" | "focus-ring"
    ) && !css_value.starts_with("color-mix")
}

#[test]
fn every_tokens_css_token_is_ported_to_theme_slint() {
    let theme = slint_tokens();
    for (css_name, _) in css_tokens() {
        for theme_name in expected_theme_names(&css_name) {
            assert!(
                theme.contains_key(&theme_name),
                "tokens.css has --{css_name}, but ui/theme.slint has no {theme_name} token"
            );
        }
    }
}

#[test]
fn theme_token_values_match_tokens_css() {
    let theme = slint_tokens();
    for (css_name, css_value) in css_tokens() {
        if !compared_directly(&css_name, &css_value) {
            continue;
        }
        let (_, actual) = theme
            .get(&css_name)
            .unwrap_or_else(|| panic!("ui/theme.slint has no {css_name} token"));
        let expected = if css_name == "tracking-display" {
            css_value.trim_end_matches("em").to_string()
        } else {
            css_value.clone()
        };
        assert_eq!(
            normalized(actual),
            normalized(&expected),
            "ui/theme.slint has a different value for {css_name}"
        );
    }

    assert_eq!(theme["accent-hover"].0, "color");
    assert_eq!(theme["accent-active"].0, "color");
    assert_eq!(theme["ease-standard"].0, "easing");
    assert_eq!(theme["elev-raised-color"].0, "color");
    assert_eq!(theme["focus-ring-color"].0, "color");
}

fn slint_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()))
    {
        let path = entry.unwrap().path();
        if path.is_dir() {
            slint_files(&path, files);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("slint") {
            files.push(path);
        }
    }
}

#[test]
fn components_do_not_hardcode_design_values() {
    let root = root();
    let mut files = Vec::new();
    slint_files(&root.join("ui"), &mut files);

    let hex = Regex::new(r"#[0-9a-fA-F]{3,8}\b").unwrap();
    let pixels = Regex::new(r"(-?\d+(?:\.\d+)?)px").unwrap();
    let durations = Regex::new(r"\d+(?:\.\d+)?(?:ms|s)\b").unwrap();
    let weights = Regex::new(r"font-weight\s*:\s*\d+").unwrap();
    let families = Regex::new(r#"font-family\s*:\s*""#).unwrap();
    let line_heights = Regex::new(r"line-height-factor\s*:\s*[\d.]").unwrap();
    let letter_spacing = Regex::new(r"letter-spacing\s*:\s*-?[\d.]").unwrap();

    let mut errors = Vec::new();
    for path in files {
        if path.file_name().and_then(|name| name.to_str()) == Some("theme.slint") {
            continue;
        }
        let relative = path.strip_prefix(&root).unwrap().display().to_string();
        let text = read(&path);
        for (index, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let mut report = |kind: &str| {
                errors.push(format!(
                    "{relative}:{}: hardcoded {kind}: {}",
                    index + 1,
                    line.trim()
                ));
            };

            if hex.is_match(code) {
                report("colour");
            }
            if pixels.captures_iter(code).any(|capture| {
                capture[1]
                    .parse::<f64>()
                    .map(|value| value != 0.0)
                    .unwrap_or(true)
            }) {
                report("length");
            }
            if durations.is_match(code) {
                report("duration");
            }
            if weights.is_match(code) {
                report("font weight");
            }
            if families.is_match(code) {
                report("font family");
            }
            if line_heights.is_match(code) {
                report("line height");
            }
            if letter_spacing.is_match(code) {
                report("letter spacing");
            }
        }
    }

    assert!(
        errors.is_empty(),
        "components must use Theme tokens instead of literals:\n{}",
        errors.join("\n")
    );
}
