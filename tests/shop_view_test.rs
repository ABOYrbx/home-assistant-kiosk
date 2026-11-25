use ha_kiosk::extensions::shop::{
    ConfigField, DEFAULT_SECRET_DIR, ExtensionPermissions, ExtensionStatus, InstalledExtension,
    ShopAction, ShopEntry, ShopError, ShopView,
};
use std::path::Path;

fn entry(id: &str, name: &str, version: &str) -> ShopEntry {
    ShopEntry {
        id: id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        author: "Example Author".to_string(),
        license: "MIT".to_string(),
        description: "Shows upcoming bus departures for a configured stop.".to_string(),
        provides: "departures".to_string(),
        permissions: ExtensionPermissions {
            http_hosts: vec!["api.example.com".to_string()],
            ha_entities: vec!["sensor.example".to_string()],
        },
        config_schema: vec![
            ConfigField {
                key: "stop_id".to_string(),
                label: "Stop ID".to_string(),
                kind: "string".to_string(),
                required: true,
            },
            ConfigField {
                key: "limit".to_string(),
                label: "Maximum departures".to_string(),
                kind: "integer".to_string(),
                required: false,
            },
        ],
        installed_version: None,
        enabled: false,
        configured: false,
        error: None,
        available: true,
    }
}

fn view_with_catalog() -> ShopView {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.apply_catalog(vec![entry(
        "org.example.bus-departures",
        "Bus Departures",
        "1.0.0",
    )]);
    view
}

#[test]
fn catalog_entries_are_listed_as_available() {
    let view = view_with_catalog();
    assert!(view.catalog_online);
    assert_eq!(view.available_entries().len(), 1);
    assert_eq!(view.installed_entries().len(), 0);
    assert_eq!(
        view.available_entries()[0].status(),
        ExtensionStatus::Available
    );
}

#[test]
fn install_requires_first_install_confirmation() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";

    let err = view.request(id, ShopAction::Install, false).unwrap_err();
    assert_eq!(err, ShopError::ConfirmationRequired(id.to_string()));
    assert!(view.pending.is_none());

    let request = view.request(id, ShopAction::Install, true).unwrap();
    assert_eq!(request.action, ShopAction::Install);
    assert_eq!(request.version, "1.0.0");
    assert!(view.pending.is_some());
}

#[test]
fn install_flow_marks_the_extension_installed_after_store_success() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    view.request(id, ShopAction::Install, true).unwrap();
    view.complete(id, ShopAction::Install, Ok(())).unwrap();

    let installed = view.entry(id).unwrap();
    assert_eq!(installed.installed_version.as_deref(), Some("1.0.0"));
    assert!(installed.enabled);
    assert!(installed.error.is_none());
    assert_eq!(installed.status(), ExtensionStatus::Installed);
    assert!(view.pending.is_none());
    assert_eq!(view.installed_entries().len(), 1);
    assert_eq!(view.available_entries().len(), 0);
}

#[test]
fn failed_install_is_not_reported_as_success() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    view.request(id, ShopAction::Install, true).unwrap();
    view.complete(
        id,
        ShopAction::Install,
        Err("checksum mismatch".to_string()),
    )
    .unwrap();

    let installed = view.entry(id).unwrap();
    assert!(installed.installed_version.is_none());
    assert!(!installed.enabled);
    assert_eq!(installed.error.as_deref(), Some("checksum mismatch"));
    assert_eq!(installed.status(), ExtensionStatus::Error);
    assert!(view.pending.is_none());
}

#[test]
fn update_flow_uses_the_catalog_version() {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: "org.example.bus-departures".to_string(),
        version: "1.0.0".to_string(),
        enabled: true,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(vec![entry(
        "org.example.bus-departures",
        "Bus Departures",
        "1.1.0",
    )]);
    let id = "org.example.bus-departures";
    assert_eq!(
        view.entry(id).unwrap().status(),
        ExtensionStatus::UpdateAvailable
    );

    let request = view.request(id, ShopAction::Update, true).unwrap();
    assert_eq!(request.version, "1.1.0");
    view.complete(id, ShopAction::Update, Ok(())).unwrap();
    assert_eq!(
        view.entry(id).unwrap().installed_version.as_deref(),
        Some("1.1.0")
    );
    assert_eq!(view.entry(id).unwrap().status(), ExtensionStatus::Installed);
}

#[test]
fn failed_update_keeps_the_update_available_for_retry() {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: "org.example.bus-departures".to_string(),
        version: "1.0.0".to_string(),
        enabled: true,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(vec![entry(
        "org.example.bus-departures",
        "Bus Departures",
        "1.1.0",
    )]);
    let id = "org.example.bus-departures";

    view.request(id, ShopAction::Update, true).unwrap();
    view.complete(
        id,
        ShopAction::Update,
        Err("package download failed".to_string()),
    )
    .unwrap();

    let installed = view.entry(id).unwrap();
    assert_eq!(installed.status(), ExtensionStatus::Error);
    assert_eq!(installed.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(installed.error.as_deref(), Some("package download failed"));
    assert_eq!(installed.primary_action(), Some(ShopAction::Update));

    let retry = view.request(id, ShopAction::Update, true).unwrap();
    assert_eq!(retry.version, "1.1.0");
    view.complete(id, ShopAction::Update, Ok(())).unwrap();
    assert_eq!(
        view.entry(id).unwrap().installed_version.as_deref(),
        Some("1.1.0")
    );
    assert!(view.entry(id).unwrap().error.is_none());
}

#[test]
fn disabled_installed_extension_can_be_updated() {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: "org.example.bus-departures".to_string(),
        version: "1.0.0".to_string(),
        enabled: false,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(vec![entry(
        "org.example.bus-departures",
        "Bus Departures",
        "1.1.0",
    )]);
    let id = "org.example.bus-departures";

    assert_eq!(view.entry(id).unwrap().status(), ExtensionStatus::Disabled);
    assert_eq!(
        view.entry(id).unwrap().primary_action(),
        Some(ShopAction::Update)
    );
    let request = view.request(id, ShopAction::Update, true).unwrap();
    assert_eq!(request.version, "1.1.0");
    view.complete(id, ShopAction::Update, Ok(())).unwrap();
    let updated = view.entry(id).unwrap();
    assert_eq!(updated.installed_version.as_deref(), Some("1.1.0"));
    assert!(!updated.enabled);
    assert_eq!(updated.status(), ExtensionStatus::Disabled);
}

#[test]
fn update_requires_an_installed_extension_with_a_newer_version() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    assert_eq!(
        view.request(id, ShopAction::Update, true).unwrap_err(),
        ShopError::NotInstalled(id.to_string())
    );

    view.request(id, ShopAction::Install, true).unwrap();
    view.complete(id, ShopAction::Install, Ok(())).unwrap();
    assert_eq!(
        view.request(id, ShopAction::Update, true).unwrap_err(),
        ShopError::NoUpdate(id.to_string())
    );
}

#[test]
fn remove_flow_clears_the_installation() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    view.request(id, ShopAction::Install, true).unwrap();
    view.complete(id, ShopAction::Install, Ok(())).unwrap();
    view.set_configured(id, true);

    view.request(id, ShopAction::Remove, true).unwrap();
    view.complete(id, ShopAction::Remove, Ok(())).unwrap();

    let removed = view.entry(id).unwrap();
    assert!(removed.installed_version.is_none());
    assert!(!removed.enabled);
    assert!(!removed.configured);
    assert_eq!(removed.status(), ExtensionStatus::Available);
}

#[test]
fn disable_and_enable_flow_updates_status() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    view.request(id, ShopAction::Install, true).unwrap();
    view.complete(id, ShopAction::Install, Ok(())).unwrap();

    view.request(id, ShopAction::Disable, true).unwrap();
    view.complete(id, ShopAction::Disable, Ok(())).unwrap();
    assert!(!view.entry(id).unwrap().enabled);
    assert_eq!(view.entry(id).unwrap().status(), ExtensionStatus::Disabled);

    view.request(id, ShopAction::Enable, true).unwrap();
    view.complete(id, ShopAction::Enable, Ok(())).unwrap();
    assert!(view.entry(id).unwrap().enabled);
    assert_eq!(view.entry(id).unwrap().status(), ExtensionStatus::Installed);
}

#[test]
fn a_pending_action_blocks_a_second_request() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    view.request(id, ShopAction::Install, true).unwrap();
    assert_eq!(
        view.request(id, ShopAction::Install, true).unwrap_err(),
        ShopError::Busy
    );
    view.complete(id, ShopAction::Install, Ok(())).unwrap();
    assert!(view.pending.is_none());
}

#[test]
fn offline_catalog_keeps_installed_extensions_usable() {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: "org.example.bus-departures".to_string(),
        version: "1.0.0".to_string(),
        enabled: true,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(vec![
        entry("org.example.bus-departures", "Bus Departures", "1.1.0"),
        entry("org.example.weather", "Weather", "2.0.0"),
    ]);
    view.mark_catalog_offline("network unreachable");

    assert!(!view.catalog_online);
    assert_eq!(view.catalog_error.as_deref(), Some("network unreachable"));
    assert_eq!(view.installed_entries().len(), 1);
    assert_eq!(
        view.installed_entries()[0].status(),
        ExtensionStatus::Installed
    );
    // The catalog-only extension is no longer offered for install.
    assert!(view.available_entries().is_empty());
}

#[test]
fn installed_extension_missing_from_the_catalog_stays_installed() {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: "org.example.retired".to_string(),
        version: "0.9.0".to_string(),
        enabled: true,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(Vec::new());

    let entry = view.entry("org.example.retired").unwrap();
    assert!(entry.is_installed());
    assert!(!entry.available);
    assert_eq!(entry.status(), ExtensionStatus::Installed);
}

#[test]
fn permissions_and_config_keys_are_described_without_inventing_values() {
    let view = view_with_catalog();
    let entry = view.entry("org.example.bus-departures").unwrap();
    assert_eq!(
        entry.permissions_summary(),
        "Network: api.example.com; Home Assistant: sensor.example"
    );
    assert_eq!(entry.config_keys_summary(), "stop_id (required), limit");

    let empty = ShopEntry {
        permissions: ExtensionPermissions::default(),
        config_schema: Vec::new(),
        ..entry.clone()
    };
    assert_eq!(
        empty.permissions_summary(),
        "No network or Home Assistant access"
    );
    assert_eq!(empty.config_keys_summary(), "No configuration required");
}

#[test]
fn settings_table_and_secret_path_are_exact() {
    let view = view_with_catalog();
    let entry = view.entry("org.example.bus-departures").unwrap();
    assert_eq!(
        entry.settings_table(),
        "[extensions.settings.\"org.example.bus-departures\"]"
    );
    assert_eq!(
        entry.secret_file(Path::new(DEFAULT_SECRET_DIR)),
        Path::new("/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml")
    );
}

#[test]
fn extension_errors_are_collected_for_the_status_page() {
    let mut view = view_with_catalog();
    let id = "org.example.bus-departures";
    view.request(id, ShopAction::Install, true).unwrap();
    view.complete(
        id,
        ShopAction::Install,
        Err("network unreachable".to_string()),
    )
    .unwrap();
    assert_eq!(
        view.extension_errors(),
        vec!["Bus Departures: network unreachable".to_string()]
    );
}

#[test]
fn bridge_exposes_permissions_config_and_paths_to_the_ui() {
    let view = view_with_catalog();
    let entry = view.entry("org.example.bus-departures").unwrap();
    let ui = ha_kiosk::ui::bridge::shop_entry_view(entry, &view);

    assert_eq!(ui.id.as_str(), "org.example.bus-departures");
    assert_eq!(ui.name.as_str(), "Bus Departures");
    assert_eq!(ui.version.as_str(), "1.0.0");
    assert_eq!(ui.status.as_str(), "Available");
    assert_eq!(
        ui.permissions.as_str(),
        "Network: api.example.com; Home Assistant: sensor.example"
    );
    assert_eq!(ui.config_keys.as_str(), "stop_id (required), limit");
    assert!(
        ui.config_path
            .as_str()
            .contains("[extensions.settings.\"org.example.bus-departures\"]")
    );
    assert!(
        ui.secret_path
            .as_str()
            .ends_with("org.example.bus-departures.secret.toml")
    );
    assert!(!ui.installed);
    assert!(ui.available);
}

#[test]
fn bridge_shows_the_installed_version_while_an_update_is_available() {
    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.set_installed(vec![InstalledExtension {
        id: "org.example.bus-departures".to_string(),
        version: "1.0.0".to_string(),
        enabled: true,
        configured: true,
        error: None,
    }]);
    view.apply_catalog(vec![entry(
        "org.example.bus-departures",
        "Bus Departures",
        "1.1.0",
    )]);

    let ui = ha_kiosk::ui::bridge::shop_entry_view(
        view.entry("org.example.bus-departures").unwrap(),
        &view,
    );
    assert_eq!(ui.version.as_str(), "1.0.0");
    assert!(ui.update_available);
}

#[test]
fn catalog_entry_json_matches_the_documented_index_shape() {
    let json = r#"{
        "id": "org.example.bus-departures",
        "name": "Bus Departures",
        "version": "1.0.0",
        "api_version": 1,
        "min_app_version": "0.6.0",
        "description": "Shows upcoming bus departures for a configured stop.",
        "author": "Example Author",
        "license": "MIT",
        "homepage": "https://example.com/bus-departures",
        "provides": "departures",
        "permissions": {"http_hosts": ["api.example.com"], "ha_entities": []},
        "config_schema": [
            {"key": "stop_id", "label": "Stop ID", "type": "string", "required": true},
            {"key": "limit", "label": "Maximum departures", "type": "integer", "required": false}
        ],
        "package_url": "https://example.com/org.example.bus-departures-1.0.0.hkext",
        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "size_bytes": 12345
    }"#;

    let entry: ShopEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.id, "org.example.bus-departures");
    assert_eq!(entry.version, "1.0.0");
    assert_eq!(entry.config_schema[0].key, "stop_id");
    assert_eq!(entry.config_schema[0].kind, "string");
    assert!(entry.config_schema[0].required);
    assert_eq!(entry.permissions.http_hosts, vec!["api.example.com"]);
    assert!(!entry.available);

    let mut view = ShopView::new("/etc/ha-kiosk/config.toml", DEFAULT_SECRET_DIR);
    view.apply_catalog(vec![entry]);
    assert_eq!(view.available_entries().len(), 1);
    assert!(view.available_entries()[0].available);
}
