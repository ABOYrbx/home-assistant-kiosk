//! Wave 6.1: signed catalog, package verification, and the extension store.

use ha_kiosk::extensions::{
    CatalogCache, CatalogClient, CatalogEntry, CatalogError, CatalogFetcher, CatalogIndex,
    CatalogPolicy, ExtensionKind, ExtensionManifest, ExtensionStore, FetchError, PackageError,
    SignatureError, SignatureStatus, StoreError, download_package, extract_package,
    extract_package_with_limit, sha256_hex, verify_package, verify_signature, verify_unsigned,
};
use semver::Version;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use url::Url;

const ID: &str = "org.example.bus-departures";
const PACKAGE_URL: &str = "https://catalog.example.com/org.example.bus-departures-1.0.0.hkext";
const CATALOG_URL: &str = "https://catalog.example.com/index.json";
const MAX_PACKAGE_BYTES: u64 = 5 * 1024 * 1024;

fn app_version() -> Version {
    Version::parse("0.6.0").unwrap()
}

fn policy() -> CatalogPolicy {
    CatalogPolicy {
        app_version: app_version(),
        public_keys: Vec::new(),
        allow_unsigned: false,
        dev_mode: false,
        max_catalog_bytes: 2 * 1024 * 1024,
        max_package_bytes: MAX_PACKAGE_BYTES,
    }
}

fn manifest_toml(id: &str, version: &str) -> String {
    format!(
        r#"
id = "{id}"
name = "Bus Departures"
version = "{version}"
api_version = 1
min_app_version = "0.6.0"
entry = "main.rhai"
provides = "departures"
author = "Example Author"
license = "MIT"
description = "Shows upcoming bus departures."

[permissions]
http_hosts = ["api.example.com"]
ha_entities = ["sensor.example"]
"#
    )
}

fn append(builder: &mut tar::Builder<&mut Vec<u8>>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, data).unwrap();
}

fn append_link(
    builder: &mut tar::Builder<&mut Vec<u8>>,
    path: &str,
    kind: tar::EntryType,
    target: &str,
) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_link_name(target).unwrap();
    header.set_cksum();
    builder
        .append_data(&mut header, path, std::io::empty())
        .unwrap();
}

/// Write a raw header so tests can build archives that `tar::Builder` refuses
/// to create, such as absolute or parent-traversal paths.
fn append_raw(builder: &mut tar::Builder<&mut Vec<u8>>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    {
        let bytes = header.as_mut_bytes();
        let name = path.as_bytes();
        assert!(name.len() < 100);
        bytes[..name.len()].copy_from_slice(name);
    }
    header.set_cksum();
    builder.append(&header, data).unwrap();
}

fn append_device(builder: &mut tar::Builder<&mut Vec<u8>>, path: &str, kind: tar::EntryType) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(0);
    header.set_mode(0o666);
    header.set_cksum();
    builder
        .append_data(&mut header, path, std::io::empty())
        .unwrap();
}

fn gzip(data: &[u8]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn build_package(id: &str, version: &str) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(
            &mut builder,
            "manifest.toml",
            manifest_toml(id, version).as_bytes(),
        );
        append(&mut builder, "main.rhai", b"fn refresh(config) { 42 }");
        append(&mut builder, "README.md", b"# Bus Departures");
        append(&mut builder, "LICENSE", b"MIT");
        builder.finish().unwrap();
    }
    gzip(&tar_bytes)
}

fn catalog_entry(id: &str, version: &str, package: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": "Bus Departures",
        "version": version,
        "api_version": 1,
        "min_app_version": "0.6.0",
        "description": "Shows upcoming bus departures.",
        "author": "Example Author",
        "license": "MIT",
        "provides": "departures",
        "permissions": {
            "http_hosts": ["api.example.com"],
            "ha_entities": ["sensor.example"]
        },
        "config_schema": [],
        "package_url": PACKAGE_URL,
        "sha256": sha256_hex(package),
        "size_bytes": package.len(),
    })
}

fn index_json(entries: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "generated_at": "2026-11-22T20:00:00Z",
        "extensions": entries,
    }))
    .unwrap()
}

fn keypair() -> (String, minisign::KeyPair) {
    let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let public_key = keypair.pk.to_base64();
    (public_key, keypair)
}

fn sign(keypair: &minisign::KeyPair, data: &[u8]) -> String {
    minisign::sign(
        Some(&keypair.pk),
        &keypair.sk,
        std::io::Cursor::new(data),
        None,
        None,
    )
    .unwrap()
    .to_string()
}

#[derive(Default)]
struct FakeFetcher {
    responses: HashMap<String, Vec<u8>>,
}

impl FakeFetcher {
    fn with(url: &str, body: impl Into<Vec<u8>>) -> Self {
        let mut fetcher = Self::default();
        fetcher.responses.insert(url.to_string(), body.into());
        fetcher
    }

    fn insert(&mut self, url: &str, body: impl Into<Vec<u8>>) {
        self.responses.insert(url.to_string(), body.into());
    }
}

impl CatalogFetcher for FakeFetcher {
    fn get(&mut self, url: &Url, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
        let body = self
            .responses
            .get(url.as_str())
            .cloned()
            .ok_or_else(|| FetchError::Status {
                status: 404,
                url: url.to_string(),
            })?;
        if body.len() as u64 > max_bytes {
            return Err(FetchError::TooLarge {
                url: url.to_string(),
                limit: max_bytes,
            });
        }
        Ok(body)
    }
}

fn entry_from_index(index: &[u8]) -> CatalogEntry {
    CatalogIndex::parse(index, &policy())
        .unwrap()
        .extensions
        .into_iter()
        .next()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Signature verification
// ---------------------------------------------------------------------------

#[test]
fn valid_signature_verifies() {
    let (public_key, keypair) = keypair();
    let data = b"{\"schema_version\":1}";
    let signature = sign(&keypair, data);
    let status = verify_signature(data, &signature, &[public_key]).unwrap();
    assert_eq!(status, SignatureStatus::Verified { key_index: 0 });
}

#[test]
fn tampered_index_is_rejected() {
    let (public_key, keypair) = keypair();
    let signature = sign(&keypair, b"original");
    let error = verify_signature(b"tampered", &signature, &[public_key]).unwrap_err();
    assert_eq!(error, SignatureError::NoMatchingKey);
}

#[test]
fn signature_from_another_key_is_rejected() {
    let (public_key, _) = keypair();
    let (_, other_keypair) = keypair();
    let signature = sign(&other_keypair, b"index");
    let error = verify_signature(b"index", &signature, &[public_key]).unwrap_err();
    assert_eq!(error, SignatureError::NoMatchingKey);
}

#[test]
fn key_rotation_accepts_the_new_key() {
    let (old_key, _) = keypair();
    let (new_key, new_keypair) = keypair();
    let signature = sign(&new_keypair, b"index");
    let status = verify_signature(b"index", &signature, &[old_key, new_key]).unwrap();
    assert_eq!(status, SignatureStatus::Verified { key_index: 1 });
}

#[test]
fn invalid_public_keys_are_reported() {
    let (_, keypair) = keypair();
    let signature = sign(&keypair, b"index");
    let error = verify_signature(b"index", &signature, &["not-a-key".to_string()]).unwrap_err();
    assert_eq!(error, SignatureError::InvalidPublicKeys);
}

#[test]
fn no_public_keys_is_an_error() {
    let (_, keypair) = keypair();
    let signature = sign(&keypair, b"index");
    assert_eq!(
        verify_signature(b"index", &signature, &[]).unwrap_err(),
        SignatureError::NoPublicKeys
    );
}

#[test]
fn unsigned_catalogs_require_both_flags() {
    let mut policy = policy();
    policy.allow_unsigned = true;
    assert_eq!(
        verify_unsigned(&policy).unwrap_err(),
        SignatureError::UnsignedNotAllowed
    );
    policy.dev_mode = true;
    assert_eq!(verify_unsigned(&policy).unwrap(), SignatureStatus::Unsigned);
}

// ---------------------------------------------------------------------------
// Catalog parsing
// ---------------------------------------------------------------------------

#[test]
fn valid_index_parses() {
    let package = build_package(ID, "1.0.0");
    let index = index_json(vec![catalog_entry(ID, "1.0.0", &package)]);
    let parsed = CatalogIndex::parse(&index, &policy()).unwrap();
    assert_eq!(parsed.extensions.len(), 1);
    assert_eq!(parsed.extensions[0].provides, ExtensionKind::Departures);
    assert_eq!(
        parsed.extensions[0].version,
        Version::parse("1.0.0").unwrap()
    );
}

#[test]
fn unsupported_schema_is_rejected() {
    let mut value: serde_json::Value = serde_json::from_slice(&index_json(vec![])).unwrap();
    value["schema_version"] = serde_json::json!(2);
    let error = CatalogIndex::parse(&serde_json::to_vec(&value).unwrap(), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::UnsupportedSchema(2)));
}

#[test]
fn insecure_package_url_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["package_url"] = serde_json::json!("http://catalog.example.com/pkg.hkext");
    let error = CatalogIndex::parse(&index_json(vec![entry]), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::InsecureUrl { .. }));
}

#[test]
fn http_is_allowed_only_in_development_mode() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["package_url"] = serde_json::json!("http://catalog.example.com/pkg.hkext");
    let mut dev = policy();
    dev.allow_unsigned = true;
    dev.dev_mode = true;
    assert!(CatalogIndex::parse(&index_json(vec![entry]), &dev).is_ok());
}

#[test]
fn incompatible_api_version_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["api_version"] = serde_json::json!(2);
    let error = CatalogIndex::parse(&index_json(vec![entry]), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::IncompatibleApi { .. }));
}

#[test]
fn newer_min_app_version_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["min_app_version"] = serde_json::json!("9.0.0");
    let error = CatalogIndex::parse(&index_json(vec![entry]), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::IncompatibleApp { .. }));
}

#[test]
fn duplicate_ids_are_rejected() {
    let package = build_package(ID, "1.0.0");
    let index = index_json(vec![
        catalog_entry(ID, "1.0.0", &package),
        catalog_entry(ID, "1.1.0", &package),
    ]);
    let error = CatalogIndex::parse(&index, &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::DuplicateId(_)));
}

#[test]
fn invalid_checksum_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["sha256"] = serde_json::json!("not-a-checksum");
    let error = CatalogIndex::parse(&index_json(vec![entry]), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::InvalidChecksum { .. }));
}

#[test]
fn oversized_catalog_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let index = index_json(vec![catalog_entry(ID, "1.0.0", &package)]);
    let mut small = policy();
    small.max_catalog_bytes = 32;
    let error = CatalogIndex::parse(&index, &small).unwrap_err();
    assert!(matches!(error, CatalogError::CatalogTooLarge { .. }));
}

#[test]
fn unknown_fields_are_rejected() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["unexpected"] = serde_json::json!(true);
    let error = CatalogIndex::parse(&index_json(vec![entry]), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::Json(_)));
}

#[test]
fn invalid_permissions_are_rejected() {
    let package = build_package(ID, "1.0.0");
    let mut entry = catalog_entry(ID, "1.0.0", &package);
    entry["permissions"]["http_hosts"] = serde_json::json!(["https://api.example.com/path"]);
    let error = CatalogIndex::parse(&index_json(vec![entry]), &policy()).unwrap_err();
    assert!(matches!(error, CatalogError::InvalidManifest { .. }));
}

#[test]
fn manifest_toml_round_trips() {
    let manifest = ExtensionManifest::parse_toml(&manifest_toml(ID, "1.0.0")).unwrap();
    manifest.validate(&app_version()).unwrap();
    assert_eq!(manifest.id, ID);
    assert_eq!(manifest.permissions.ha_entities, vec!["sensor.example"]);
}

// ---------------------------------------------------------------------------
// Package verification and extraction
// ---------------------------------------------------------------------------

#[test]
fn valid_package_verifies_and_extracts() {
    let package = build_package(ID, "1.0.0");
    verify_package(
        &package,
        &sha256_hex(&package),
        package.len() as u64,
        MAX_PACKAGE_BYTES,
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    extract_package(&package, dir.path()).unwrap();
    assert!(dir.path().join("manifest.toml").is_file());
    assert!(dir.path().join("main.rhai").is_file());
}

#[test]
fn checksum_mismatch_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let error = verify_package(
        &package,
        &"0".repeat(64),
        package.len() as u64,
        MAX_PACKAGE_BYTES,
    )
    .unwrap_err();
    assert!(matches!(error, PackageError::ChecksumMismatch));
}

#[test]
fn size_mismatch_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let error = verify_package(
        &package,
        &sha256_hex(&package),
        package.len() as u64 + 1,
        MAX_PACKAGE_BYTES,
    )
    .unwrap_err();
    assert!(matches!(error, PackageError::SizeMismatch { .. }));
}

#[test]
fn oversized_package_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let error =
        verify_package(&package, &sha256_hex(&package), package.len() as u64, 8).unwrap_err();
    assert!(matches!(error, PackageError::TooLarge { .. }));
}

#[test]
fn parent_traversal_entry_is_rejected() {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(
            &mut builder,
            "manifest.toml",
            manifest_toml(ID, "1.0.0").as_bytes(),
        );
        append(&mut builder, "main.rhai", b"fn refresh(config) {}");
        append_raw(&mut builder, "../evil.txt", b"escaped");
        builder.finish().unwrap();
    }
    let package = gzip(&tar_bytes);
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package(&package, dir.path()).unwrap_err();
    assert!(matches!(error, PackageError::UnsafePath(_)));
}

#[test]
fn absolute_path_entry_is_rejected() {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(
            &mut builder,
            "manifest.toml",
            manifest_toml(ID, "1.0.0").as_bytes(),
        );
        append_raw(&mut builder, "/tmp/evil.txt", b"escaped");
        builder.finish().unwrap();
    }
    let package = gzip(&tar_bytes);
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package(&package, dir.path()).unwrap_err();
    assert!(matches!(error, PackageError::UnsafePath(_)));
}

#[test]
fn symlink_entry_is_rejected() {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(
            &mut builder,
            "manifest.toml",
            manifest_toml(ID, "1.0.0").as_bytes(),
        );
        append_link(&mut builder, "link", tar::EntryType::Symlink, "/etc/passwd");
        builder.finish().unwrap();
    }
    let package = gzip(&tar_bytes);
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package(&package, dir.path()).unwrap_err();
    assert!(matches!(error, PackageError::UnsupportedEntry(_)));
}

#[test]
fn hardlink_entry_is_rejected() {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(
            &mut builder,
            "manifest.toml",
            manifest_toml(ID, "1.0.0").as_bytes(),
        );
        append_link(&mut builder, "hard", tar::EntryType::Link, "manifest.toml");
        builder.finish().unwrap();
    }
    let package = gzip(&tar_bytes);
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package(&package, dir.path()).unwrap_err();
    assert!(matches!(error, PackageError::UnsupportedEntry(_)));
}

#[test]
fn device_entry_is_rejected() {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(
            &mut builder,
            "manifest.toml",
            manifest_toml(ID, "1.0.0").as_bytes(),
        );
        append_device(&mut builder, "dev", tar::EntryType::Char);
        builder.finish().unwrap();
    }
    let package = gzip(&tar_bytes);
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package(&package, dir.path()).unwrap_err();
    assert!(matches!(error, PackageError::UnsupportedEntry(_)));
}

#[test]
fn decompression_bomb_is_rejected() {
    let package = build_package(ID, "1.0.0");
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package_with_limit(&package, dir.path(), 16).unwrap_err();
    assert!(matches!(error, PackageError::DecompressedTooLarge { .. }));
}

#[test]
fn missing_manifest_is_rejected() {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        append(&mut builder, "main.rhai", b"fn refresh(config) {}");
        builder.finish().unwrap();
    }
    let package = gzip(&tar_bytes);
    let dir = tempfile::tempdir().unwrap();
    let error = extract_package(&package, dir.path()).unwrap_err();
    assert!(matches!(error, PackageError::Invalid(_)));
}

// ---------------------------------------------------------------------------
// Catalog client and offline cache
// ---------------------------------------------------------------------------

#[test]
fn refresh_verifies_and_caches_for_offline_use() {
    let package = build_package(ID, "1.0.0");
    let index = index_json(vec![catalog_entry(ID, "1.0.0", &package)]);
    let (public_key, keypair) = keypair();
    let signature = sign(&keypair, &index);
    let catalog_url = Url::parse(CATALOG_URL).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cache_path = dir.path().join("catalog-cache.json");

    let mut fetcher = FakeFetcher::with(CATALOG_URL, index.clone());
    fetcher.insert("https://catalog.example.com/index.json.minisig", signature);
    let mut policy = policy();
    policy.public_keys = vec![public_key.clone()];
    let mut client = CatalogClient::new(
        fetcher,
        policy.clone(),
        catalog_url.clone(),
        CatalogCache::new(&cache_path),
    );
    let outcome = client.refresh().unwrap();
    assert!(!outcome.from_cache);
    assert_eq!(
        outcome.signature,
        SignatureStatus::Verified { key_index: 0 }
    );
    assert_eq!(outcome.index.extensions.len(), 1);
    assert!(cache_path.is_file());

    let mut offline = CatalogClient::new(
        FakeFetcher::default(),
        policy,
        catalog_url,
        CatalogCache::new(&cache_path),
    );
    let outcome = offline.refresh().unwrap();
    assert!(outcome.from_cache);
    assert_eq!(outcome.index.extensions.len(), 1);
}

#[test]
fn refresh_rejects_a_tampered_index() {
    let package = build_package(ID, "1.0.0");
    let index = index_json(vec![catalog_entry(ID, "1.0.0", &package)]);
    let (public_key, keypair) = keypair();
    let signature = sign(&keypair, &index);
    let mut tampered = index.clone();
    tampered.push(b' ');

    let catalog_url = Url::parse(CATALOG_URL).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut fetcher = FakeFetcher::with(CATALOG_URL, tampered);
    fetcher.insert("https://catalog.example.com/index.json.minisig", signature);
    let mut policy = policy();
    policy.public_keys = vec![public_key];
    let mut client = CatalogClient::new(
        fetcher,
        policy,
        catalog_url,
        CatalogCache::new(dir.path().join("catalog-cache.json")),
    );
    let error = client.refresh().unwrap_err();
    assert!(matches!(error, CatalogError::Signature(_)));
}

#[test]
fn refresh_requires_https() {
    let catalog_url = Url::parse("http://catalog.example.com/index.json").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut client = CatalogClient::new(
        FakeFetcher::default(),
        policy(),
        catalog_url,
        CatalogCache::new(dir.path().join("catalog-cache.json")),
    );
    let error = client.refresh().unwrap_err();
    assert!(matches!(error, CatalogError::HttpNotAllowed));
}

#[test]
fn refresh_rejects_url_credentials() {
    let catalog_url = Url::parse("https://user:pass@catalog.example.com/index.json").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut client = CatalogClient::new(
        FakeFetcher::default(),
        policy(),
        catalog_url,
        CatalogCache::new(dir.path().join("catalog-cache.json")),
    );
    let error = client.refresh().unwrap_err();
    assert!(matches!(error, CatalogError::CredentialsInUrl));
}

#[test]
fn unsigned_refresh_works_only_in_development_mode() {
    let package = build_package(ID, "1.0.0");
    let index = index_json(vec![catalog_entry(ID, "1.0.0", &package)]);
    let catalog_url = Url::parse(CATALOG_URL).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let mut policy = policy();
    policy.allow_unsigned = true;
    let mut client = CatalogClient::new(
        FakeFetcher::with(CATALOG_URL, index.clone()),
        policy.clone(),
        catalog_url.clone(),
        CatalogCache::new(dir.path().join("catalog-cache.json")),
    );
    assert!(client.refresh().is_err());

    policy.dev_mode = true;
    let mut client = CatalogClient::new(
        FakeFetcher::with(CATALOG_URL, index),
        policy,
        catalog_url,
        CatalogCache::new(dir.path().join("catalog-cache.json")),
    );
    let outcome = client.refresh().unwrap();
    assert_eq!(outcome.signature, SignatureStatus::Unsigned);
}

// ---------------------------------------------------------------------------
// Store: install, update, rollback, remove
// ---------------------------------------------------------------------------

fn entry_and_package(id: &str, version: &str) -> (CatalogEntry, Vec<u8>) {
    let package = build_package(id, version);
    let index = index_json(vec![catalog_entry(id, version, &package)]);
    (entry_from_index(&index), package)
}

#[test]
fn install_update_rollback_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("extensions");
    let mut store = ExtensionStore::open(&root).unwrap();

    let (entry_v1, package_v1) = entry_and_package(ID, "1.0.0");
    let outcome = store
        .install(&entry_v1, &package_v1, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap();
    assert_eq!(outcome.previous, None);
    assert!(
        store
            .extension_dir(ID, &Version::parse("1.0.0").unwrap())
            .join("manifest.toml")
            .is_file()
    );
    assert_eq!(store.installed().len(), 1);
    assert!(store.get(ID).unwrap().enabled);

    let (entry_v2, package_v2) = entry_and_package(ID, "2.0.0");
    let outcome = store
        .install(&entry_v2, &package_v2, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap();
    assert_eq!(outcome.previous, Some(Version::parse("1.0.0").unwrap()));
    assert!(
        store
            .extension_dir(ID, &Version::parse("1.0.0").unwrap())
            .exists()
    );
    assert!(
        store
            .extension_dir(ID, &Version::parse("2.0.0").unwrap())
            .exists()
    );
    assert_eq!(
        store.get(ID).unwrap().version,
        Version::parse("2.0.0").unwrap()
    );

    store.rollback(ID).unwrap();
    assert_eq!(
        store.get(ID).unwrap().version,
        Version::parse("1.0.0").unwrap()
    );
    assert!(
        store
            .extension_dir(ID, &Version::parse("1.0.0").unwrap())
            .exists()
    );
    assert!(
        !store
            .extension_dir(ID, &Version::parse("2.0.0").unwrap())
            .exists()
    );

    store.remove(ID, false).unwrap();
    assert!(store.get(ID).is_none());
    assert!(!root.join(ID).exists());
    assert!(matches!(
        store.remove(ID, false).unwrap_err(),
        StoreError::NotInstalled(_)
    ));
}

#[test]
fn store_persists_installed_json() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("extensions");
    let (entry, package) = entry_and_package(ID, "1.0.0");
    {
        let mut store = ExtensionStore::open(&root).unwrap();
        store
            .install(&entry, &package, CATALOG_URL, MAX_PACKAGE_BYTES)
            .unwrap();
        store.set_enabled(ID, false).unwrap();
    }
    let store = ExtensionStore::open(&root).unwrap();
    let record = store.get(ID).unwrap();
    assert_eq!(record.version, Version::parse("1.0.0").unwrap());
    assert!(!record.enabled);
    assert_eq!(record.source_catalog, CATALOG_URL);
    assert_eq!(record.manifest_sha256.len(), 64);
}

#[cfg(unix)]
#[test]
fn store_enforces_private_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("extensions");
    let (entry, package) = entry_and_package(ID, "1.0.0");
    let mut store = ExtensionStore::open(&root).unwrap();
    store
        .install(&entry, &package, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap();

    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&root), 0o700);
    assert_eq!(mode(&root.join("installed.json")), 0o600);
    let version_dir = store.extension_dir(ID, &Version::parse("1.0.0").unwrap());
    assert_eq!(mode(&version_dir), 0o700);
    assert_eq!(mode(&version_dir.join("manifest.toml")), 0o600);
}

#[test]
fn store_rejects_manifest_that_differs_from_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let package = build_package("org.example.other", "1.0.0");
    let index = index_json(vec![catalog_entry(ID, "1.0.0", &package)]);
    let entry = entry_from_index(&index);
    let mut store = ExtensionStore::open(dir.path().join("extensions")).unwrap();
    let error = store
        .install(&entry, &package, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap_err();
    assert!(matches!(error, StoreError::ManifestMismatch(_)));
}

#[test]
fn store_rejects_checksum_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let (entry, mut package) = entry_and_package(ID, "1.0.0");
    package[0] ^= 0xff;
    let mut store = ExtensionStore::open(dir.path().join("extensions")).unwrap();
    let error = store
        .install(&entry, &package, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap_err();
    assert!(matches!(
        error,
        StoreError::Package(PackageError::ChecksumMismatch)
    ));
}

#[test]
fn store_rejects_reinstalling_the_same_version() {
    let dir = tempfile::tempdir().unwrap();
    let (entry, package) = entry_and_package(ID, "1.0.0");
    let mut store = ExtensionStore::open(dir.path().join("extensions")).unwrap();
    store
        .install(&entry, &package, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap();
    let error = store
        .install(&entry, &package, CATALOG_URL, MAX_PACKAGE_BYTES)
        .unwrap_err();
    assert!(matches!(error, StoreError::AlreadyInstalled { .. }));
}

#[test]
fn store_rejects_unknown_rollback_and_enable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ExtensionStore::open(dir.path().join("extensions")).unwrap();
    assert!(matches!(
        store.rollback(ID).unwrap_err(),
        StoreError::NotInstalled(_)
    ));
    assert!(matches!(
        store.set_enabled(ID, false).unwrap_err(),
        StoreError::NotInstalled(_)
    ));
}

#[test]
fn store_rejects_unsafe_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ExtensionStore::open(dir.path().join("extensions")).unwrap();
    assert!(matches!(
        store.remove("../../etc", false).unwrap_err(),
        StoreError::InvalidId(_)
    ));
}

// ---------------------------------------------------------------------------
// Package download
// ---------------------------------------------------------------------------

#[test]
fn downloads_package_to_temp_dir() {
    let package = build_package(ID, "1.0.0");
    let dir = tempfile::tempdir().unwrap();
    let mut fetcher = FakeFetcher::with(PACKAGE_URL, package.clone());
    let path = download_package(
        &mut fetcher,
        &Url::parse(PACKAGE_URL).unwrap(),
        MAX_PACKAGE_BYTES,
        dir.path(),
    )
    .unwrap();
    assert!(path.starts_with(dir.path()));
    assert!(path.is_file());
    assert_eq!(std::fs::read(&path).unwrap(), package);
}

#[test]
fn download_rejects_oversized_package() {
    let package = build_package(ID, "1.0.0");
    let dir = tempfile::tempdir().unwrap();
    let mut fetcher = FakeFetcher::with(PACKAGE_URL, package);
    let error = download_package(
        &mut fetcher,
        &Url::parse(PACKAGE_URL).unwrap(),
        8,
        dir.path(),
    )
    .unwrap_err();
    assert!(matches!(error, FetchError::TooLarge { .. }));
}
