//! Guards the release profile contract for the performance-constrained kiosk
//! build. A profile change invalidates the Pi measurements, so the settings and
//! the behaviour they imply are checked here as well as in `check-docs.py`.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn release_profile() -> toml::Value {
    let manifest =
        std::fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml must be readable");
    let manifest: toml::Value = toml::from_str(&manifest).expect("Cargo.toml must be valid TOML");
    manifest
        .get("profile")
        .and_then(|profiles| profiles.get("release"))
        .expect("Cargo.toml must define [profile.release]")
        .clone()
}

#[test]
fn release_profile_matches_the_performance_contract() {
    let profile = release_profile();
    assert_eq!(
        profile.get("opt-level").and_then(toml::Value::as_str),
        Some("s")
    );
    assert_eq!(
        profile.get("lto").and_then(toml::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        profile.get("strip").and_then(toml::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        profile.get("panic").and_then(toml::Value::as_str),
        Some("abort")
    );
}

#[test]
fn reproducible_build_overrides_agree_with_the_release_profile() {
    let script = std::fs::read_to_string(root().join("scripts/build-all.sh"))
        .expect("build-all.sh must be readable");
    let setting = |name: &str| -> Option<String> {
        let prefix = format!("export CARGO_PROFILE_RELEASE_{name}=");
        script
            .lines()
            .find_map(|line| line.trim().strip_prefix(&prefix).map(str::to_owned))
    };

    assert_eq!(setting("OPT_LEVEL").as_deref(), Some("s"));
    assert_eq!(setting("PANIC").as_deref(), Some("abort"));
    // `fat` LTO and `symbols` stripping are stricter forms of the manifest
    // values; they must not be relaxed by the reproducible build script.
    assert!(matches!(setting("LTO").as_deref(), Some("true" | "fat")));
    assert!(matches!(
        setting("STRIP").as_deref(),
        Some("true" | "symbols")
    ));
    assert_eq!(setting("CODEGEN_UNITS").as_deref(), Some("1"));
}

#[test]
fn production_code_does_not_catch_panics_that_now_abort() {
    fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("source directory must be readable") {
            let path = entry.expect("directory entry must be readable").path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(&root().join("src"), &mut files);
    assert!(!files.is_empty(), "source files must be found under src/");
    for file in files {
        let source = std::fs::read_to_string(&file).expect("source file must be readable");
        assert!(
            !source.contains("catch_unwind") && !source.contains("resume_unwind"),
            "{} catches panics, but panic=abort makes unwinding impossible",
            file.display()
        );
    }
}

#[test]
fn performance_doc_names_the_systemd_restart_policy_that_handles_an_abort() {
    let doc = std::fs::read_to_string(root().join("docs/performance.md"))
        .expect("docs/performance.md must be readable");
    let row = doc
        .lines()
        .find(|line| line.starts_with("| `panic` |"))
        .expect("the release profile table must have a panic row");
    assert!(
        row.contains("Restart=on-failure"),
        "the release profile table must name the restart policy in the unit: {row}"
    );
    assert!(
        !row.contains("Restart=always"),
        "the release profile table must not claim a policy the unit does not use: {row}"
    );

    for unit in ["deploy/ha-kiosk.service", "deploy/ha-kiosk-user.service"] {
        let unit = std::fs::read_to_string(root().join(unit))
            .unwrap_or_else(|error| panic!("{unit} must be readable: {error}"));
        assert!(
            unit.contains("Restart=on-failure"),
            "{unit} must restart the process after a panic abort"
        );
    }
}
