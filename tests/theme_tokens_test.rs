//! Design-token contract after the UI consolidation.
//!
//! The consolidated Slint theme ports the core of `tokens.css` rather than
//! every CSS-specific value (CSS `color-mix`, shadows and container widths
//! have no Slint equivalent). These checks keep the ported values in sync and
//! stop components from reintroducing literal colours or font families.

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

fn css_tokens() -> HashMap<String, String> {
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
    let property = Regex::new(
        r"(?m)^\s*(?:in-out|out) property <([^>]+)>\s+([A-Za-z0-9_-]+)\s*:\s*(.+?);\s*$",
    )
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

fn normalized(value: &str) -> String {
    value
        .trim()
        .replace([' ', '\t', '"'], "")
        .to_ascii_lowercase()
}

#[test]
fn every_ported_token_matches_tokens_css() {
    let css = css_tokens();
    let theme = slint_tokens();
    let mut checked = 0;

    for (name, (_, actual)) in &theme {
        let Some(expected) = css.get(name) else {
            // Slint-only aliases such as elevation fallbacks have no CSS counterpart.
            continue;
        };
        if expected.starts_with("color-mix") || expected.starts_with("cubic-bezier") {
            continue;
        }
        // CSS shadows are flattened into scalar Slint properties
        // (`elev-ring` 1px, `focus-ring` colour); there is no one-to-one value.
        if name.starts_with("elev-") || name == "focus-ring" {
            continue;
        }
        assert_eq!(
            normalized(actual),
            normalized(expected),
            "ui/theme.slint has a different value for {name}"
        );
        checked += 1;
    }

    assert!(
        checked >= 25,
        "only {checked} tokens could be compared with tokens.css"
    );
}

#[test]
fn theme_exposes_the_core_design_tokens() {
    let theme = slint_tokens();
    for name in [
        "bg",
        "surface",
        "surface-warm",
        "fg",
        "fg-2",
        "muted",
        "border",
        "accent",
        "accent-on",
        "success",
        "warn",
        "danger",
        "text-xs",
        "text-sm",
        "text-base",
        "text-lg",
        "text-xl",
        "text-2xl",
        "text-3xl",
        "text-4xl",
        "space-1",
        "space-2",
        "space-3",
        "space-4",
        "space-5",
        "space-6",
        "space-8",
        "space-12",
        "radius-sm",
        "radius-md",
        "radius-lg",
        "radius-pill",
        "touch-target",
        "motion-fast",
        "motion-base",
    ] {
        assert!(theme.contains_key(name), "missing token {name}");
    }
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
fn components_do_not_hardcode_colours_or_font_families() {
    let root = root();
    let mut files = Vec::new();
    slint_files(&root.join("ui"), &mut files);

    let hex = Regex::new(r"#[0-9a-fA-F]{3,8}\b").unwrap();
    let families = Regex::new(r#"font-family\s*:\s*""#).unwrap();

    let mut errors = Vec::new();
    for path in files {
        if path.file_name().and_then(|name| name.to_str()) == Some("theme.slint") {
            continue;
        }
        let relative = path.strip_prefix(&root).unwrap().display().to_string();
        let text = read(&path);
        for (index, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if hex.is_match(code) {
                errors.push(format!("{relative}:{}: hardcoded colour", index + 1));
            }
            if families.is_match(code) {
                errors.push(format!("{relative}:{}: hardcoded font family", index + 1));
            }
        }
    }

    assert!(
        errors.is_empty(),
        "components must use Theme colours and font families:\n{}",
        errors.join("\n")
    );
}
