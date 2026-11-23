use ha_kiosk::config::Config;
use secrecy::ExposeSecret;
use std::fs;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;

fn base_config(token_line: &str) -> String {
    format!(
        r#"
[ha]
url = "http://homeassistant.local:8123"
{token_line}
allow_invalid_certs = false
extra_entity_ids = []

[display]
width = 800
height = 480
rotation = 0
fullscreen = true
theme = "dark"

[pages.overview]
widgets = ["clock", "weather", "tiles"]

[pages.media]
widgets = ["media"]

[pages.agenda]
widgets = ["calendar"]

[pages.status]
widgets = ["status"]

[widgets.clock]
enabled = true

[widgets.weather]
enabled = true
entity = "weather.home"
forecast_type = "daily"
forecast_count = 3
refresh_seconds = 900

[widgets.media]
enabled = true
entity = "media_player.living_room"

[widgets.departures]
enabled = false
entity = ""

[widgets.calendar]
enabled = true
entities = ["calendar.family"]
days = 2
refresh_seconds = 300

[tiles]
columns = 2

[[tiles.items]]
entity = "light.example"
label = "Example light"

[cache]
path = "/tmp/ha-kiosk-test-cache.json"
write_debounce_seconds = 10
max_bytes = 262144

[extensions]
enabled = false
"#
    )
}

#[test]
fn valid_config_with_literal_token() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let content = base_config("token = \"secret-token\"");
    fs::write(&path, content).unwrap();
    let cfg = Config::load(&path).unwrap();
    assert_eq!(cfg.ha.url.as_str(), "http://homeassistant.local:8123/");
    assert!(cfg.subscribed_entity_ids().len() >= 3);
}

#[test]
fn missing_required_field_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[ha]\nurl = \"http://x\"\n").unwrap();
    let res = Config::load(&path);
    assert!(res.is_err());
}

#[test]
fn unknown_key_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut content = base_config("token = \"t\"");
    content.push_str("bogus = 1\n");
    fs::write(&path, content).unwrap();
    let res = Config::load(&path);
    assert!(res.is_err());
}

#[test]
fn bad_url_scheme_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let content = base_config("token = \"t\"").replace(
        "url = \"http://homeassistant.local:8123\"",
        "url = \"ftp://homeassistant.local:8123\"",
    );
    fs::write(&path, content).unwrap();
    let res = Config::load(&path);
    assert!(res.is_err());
}

#[test]
fn token_file_mode_0600_accepted() {
    let _lock = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("HA_TOKEN");
    }
    let dir = tempdir().unwrap();
    let token_path = dir.path().join("token");
    fs::write(&token_path, "file-token\n").unwrap();
    fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600)).unwrap();
    let cache_path = dir.path().join("cache.json");
    let mut content = base_config(&format!("token_file = \"{}\"", token_path.display()));
    content = content.replace(
        "/tmp/ha-kiosk-test-cache.json",
        &cache_path.display().to_string(),
    );
    let path = dir.path().join("config.toml");
    fs::write(&path, content).unwrap();
    let cfg = Config::load(&path).unwrap();
    assert!(!cfg.ha.token.expose_secret().is_empty());
}

#[test]
fn token_file_mode_0644_rejected() {
    let _lock = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("HA_TOKEN");
    }
    let dir = tempdir().unwrap();
    let token_path = dir.path().join("token");
    fs::write(&token_path, "file-token\n").unwrap();
    fs::set_permissions(&token_path, fs::Permissions::from_mode(0o644)).unwrap();
    let content = base_config(&format!("token_file = \"{}\"", token_path.display()));
    let path = dir.path().join("config.toml");
    fs::write(&path, content).unwrap();
    let res = Config::load(&path);
    assert!(res.is_err());
}

#[test]
fn token_redacted_in_debug() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let content = base_config("token = \"super-secret\"");
    fs::write(&path, content).unwrap();
    let cfg = Config::load(&path).unwrap();
    let dbg = format!("{:?}", cfg);
    assert!(!dbg.contains("super-secret"));
}

#[test]
fn empty_subscription_list_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut content = base_config("token = \"t\"");
    // disable all widgets/tiles
    content = content.replace("enabled = true", "enabled = false");
    content = content.replace(
        "[[tiles.items]]\nentity = \"light.example\"\nlabel = \"Example light\"\n",
        "",
    );
    fs::write(&path, content).unwrap();
    let res = Config::load(&path);
    assert!(res.is_err());
}

#[test]
fn env_token_overrides() {
    let _lock = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("HA_TOKEN");
    }
    // set env
    unsafe {
        std::env::set_var("HA_TOKEN", "env-token");
    }
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let content = base_config("token = \"literal\"");
    fs::write(&path, content).unwrap();
    let cfg = Config::load(&path).unwrap();
    assert_eq!(cfg.ha.token.expose_secret(), "env-token");
    unsafe {
        std::env::remove_var("HA_TOKEN");
    }
}

#[test]
fn departures_max_items_outside_the_contract_range_is_rejected() {
    for (value, accepted) in [("0", false), ("1", true), ("20", true), ("21", false)] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let content = base_config("token = \"t\"").replace(
            "[widgets.departures]\nenabled = false\nentity = \"\"",
            &format!(
                "[widgets.departures]\nenabled = true\nentity = \"sensor.departures\"\nmax_items = {value}"
            ),
        );
        fs::write(&path, content).unwrap();
        assert_eq!(
            Config::load(&path).is_ok(),
            accepted,
            "max_items = {value} should be {}",
            if accepted { "accepted" } else { "rejected" }
        );
    }
}
