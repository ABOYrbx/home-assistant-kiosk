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
