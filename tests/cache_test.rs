use ha_kiosk::cache::FileCache;
use ha_kiosk::state::{AppState, CacheStore};
use tempfile::tempdir;

#[test]
fn round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let cache = FileCache::new(path.clone(), 1, "fp".to_string(), 1024 * 1024);
    let state = AppState {
        stale: false,
        ..AppState::default()
    };
    cache.save(&state).unwrap();
    let loaded = cache.load().unwrap().unwrap();
    assert!(!loaded.stale);
}

#[test]
fn missing_file_returns_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nope.json");
    let cache = FileCache::new(path, 1, "fp".to_string(), 1024 * 1024);
    assert!(cache.load().unwrap().is_none());
}

#[test]
fn corrupt_json_returns_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    std::fs::write(&path, "{not json").unwrap();
    let cache = FileCache::new(path, 1, "fp".to_string(), 1024 * 1024);
    assert!(cache.load().unwrap().is_none());
}

#[test]
fn schema_mismatch_discards() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let cache1 = FileCache::new(path.clone(), 1, "fp".to_string(), 1024 * 1024);
    cache1.save(&AppState::default()).unwrap();
    let cache2 = FileCache::new(path, 2, "fp".to_string(), 1024 * 1024);
    assert!(cache2.load().unwrap().is_none());
}

#[test]
fn fingerprint_mismatch_discards() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let cache1 = FileCache::new(path.clone(), 1, "fp1".to_string(), 1024 * 1024);
    cache1.save(&AppState::default()).unwrap();
    let cache2 = FileCache::new(path, 1, "fp2".to_string(), 1024 * 1024);
    assert!(cache2.load().unwrap().is_none());
}

#[test]
fn file_mode_0600() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let cache = FileCache::new(path.clone(), 1, "fp".to_string(), 1024 * 1024);
    cache.save(&AppState::default()).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn dir_mode_0700() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("sub");
    std::fs::create_dir_all(&subdir).unwrap();
    let path = subdir.join("cache.json");
    let cache = FileCache::new(path.clone(), 1, "fp".to_string(), 1024 * 1024);
    cache.save(&AppState::default()).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&subdir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn size_cap_enforced() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let cache = FileCache::new(path, 1, "fp".to_string(), 10);
    let res = cache.save(&AppState::default());
    assert!(res.is_err());
}

#[test]
fn failed_rename_is_reported() {
    let dir = tempdir().unwrap();
    // A directory at the target path makes rename fail.
    let path = dir.path().join("cache.json");
    std::fs::create_dir(&path).unwrap();
    let cache = FileCache::new(path, 1, "fp".to_string(), 1024 * 1024);
    assert!(cache.save(&AppState::default()).is_err());
}

#[test]
fn existing_directory_is_narrowed_to_0700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("state");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::set_permissions(&subdir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cache = FileCache::new(subdir.join("cache.json"), 1, "fp".to_string(), 1024 * 1024);
    cache.save(&AppState::default()).unwrap();
    let mode = std::fs::metadata(&subdir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn cache_only_writes_configured_entities() {
    use ha_kiosk::events::HaEntity;
    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let mut state = AppState::default();
    for id in ["light.configured", "light.other"] {
        state.entities.insert(
            id.to_string(),
            HaEntity {
                entity_id: id.to_string(),
                state: "on".to_string(),
                attributes: serde_json::json!({}),
                last_changed: None,
                last_updated: None,
            },
        );
    }
    let cache = FileCache::new(path, 1, "fp".to_string(), 1024 * 1024)
        .with_entities(["light.configured".to_string()]);
    cache.save(&state).unwrap();
    let loaded = cache.load().unwrap().unwrap();
    assert!(loaded.entities.contains_key("light.configured"));
    assert!(!loaded.entities.contains_key("light.other"));
}

#[test]
fn write_debounce_waits_for_the_window() {
    use ha_kiosk::cache::WriteDebounce;
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let mut debounce = WriteDebounce::new(Duration::from_secs(10));
    debounce.mark(start);
    assert!(debounce.is_pending());
    assert!(!debounce.is_due(start + Duration::from_secs(9)));
    assert!(debounce.is_due(start + Duration::from_secs(10)));
    assert!(debounce.take_due(start + Duration::from_secs(11)));
    assert!(!debounce.is_pending());

    // Re-marking inside the window must not extend it.
    debounce.mark(start);
    debounce.mark(start + Duration::from_secs(9));
    assert!(debounce.is_due(start + Duration::from_secs(10)));
}

#[test]
fn cache_writer_debounces_and_flushes() {
    use ha_kiosk::cache::CacheWriter;
    use std::time::{Duration, Instant};

    let dir = tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let cache = FileCache::new(path.clone(), 1, "fp".to_string(), 1024 * 1024);
    let mut writer = CacheWriter::new(cache, Duration::from_secs(10));
    let start = Instant::now();

    writer.mark_dirty_at(start);
    assert!(!writer.flush_if_due(&AppState::default(), start).unwrap());
    assert!(!path.exists());

    assert!(
        writer
            .flush_if_due(&AppState::default(), start + Duration::from_secs(10))
            .unwrap()
    );
    assert!(path.exists());
    assert!(!writer.is_dirty());

    writer.flush(&AppState::default()).unwrap();
    assert!(path.exists());
}
