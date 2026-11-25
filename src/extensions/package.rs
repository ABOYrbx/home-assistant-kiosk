//! `.hkext` package verification and safe extraction.
//!
//! A package is a gzipped tar archive. The signed index is the authority for
//! the checksum and size, and extraction rejects every archive feature that
//! could escape the target directory or exhaust memory.

use crate::extensions::catalog::ExtensionManifest;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tar::EntryType;
use thiserror::Error;

/// Maximum decompressed package size.
pub const MAX_DECOMPRESSED_BYTES: u64 = 20 * 1024 * 1024;
/// Maximum number of archive entries.
pub const MAX_ARCHIVE_ENTRIES: usize = 1000;

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("package size {actual} exceeds the limit {limit}")]
    TooLarge { actual: u64, limit: u64 },
    #[error("package checksum mismatch")]
    ChecksumMismatch,
    #[error("package size mismatch: index says {expected}, download is {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("invalid package: {0}")]
    Invalid(String),
    #[error("unsafe archive path: {0}")]
    UnsafePath(String),
    #[error("unsupported archive entry: {0}")]
    UnsupportedEntry(String),
    #[error("decompressed package exceeds {limit} bytes")]
    DecompressedTooLarge { limit: u64 },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Verify the download against the checksum and size from the signed index.
pub fn verify_package(
    bytes: &[u8],
    expected_sha256: &str,
    expected_size: u64,
    max_bytes: u64,
) -> Result<(), PackageError> {
    let actual = bytes.len() as u64;
    if actual > max_bytes {
        return Err(PackageError::TooLarge {
            actual,
            limit: max_bytes,
        });
    }
    if actual != expected_size {
        return Err(PackageError::SizeMismatch {
            expected: expected_size,
            actual,
        });
    }
    if sha256_hex(bytes) != expected_sha256.to_ascii_lowercase() {
        return Err(PackageError::ChecksumMismatch);
    }
    Ok(())
}

/// Extract a package with the default decompressed-size limit.
pub fn extract_package(bytes: &[u8], destination: &Path) -> Result<(), PackageError> {
    extract_package_with_limit(bytes, destination, MAX_DECOMPRESSED_BYTES)
}

/// Extract a package into an empty staging directory.
///
/// Only regular files and directories are accepted. Absolute paths, parent
/// traversal, symlinks, hardlinks, device files, duplicate entries, and
/// decompressed-size bombs are rejected before anything is written.
pub fn extract_package_with_limit(
    bytes: &[u8],
    destination: &Path,
    max_decompressed_bytes: u64,
) -> Result<(), PackageError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut total: u64 = 0;
    let mut count: usize = 0;

    for entry in archive.entries()? {
        let entry = entry?;
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(PackageError::Invalid("too many archive entries".into()));
        }
        let path = entry.path()?.into_owned();
        if !is_safe_archive_path(&path) {
            return Err(PackageError::UnsafePath(path.display().to_string()));
        }
        if !seen.insert(path.clone()) {
            return Err(PackageError::Invalid(format!(
                "duplicate archive entry: {}",
                path.display()
            )));
        }
        let entry_type = entry.header().entry_type();
        match entry_type {
            EntryType::Regular => {
                let target = destination.join(&path);
                if let Some(parent) = target.parent() {
                    create_private_dir(parent)?;
                }
                let mut file = private_file(&target)?;
                let remaining = max_decompressed_bytes.saturating_sub(total);
                let copied =
                    std::io::copy(&mut entry.take(remaining.saturating_add(1)), &mut file)?;
                total += copied;
                if total > max_decompressed_bytes {
                    return Err(PackageError::DecompressedTooLarge {
                        limit: max_decompressed_bytes,
                    });
                }
                file.sync_all()?;
            }
            EntryType::Directory => {
                create_private_dir(&destination.join(&path))?;
            }
            other => {
                return Err(PackageError::UnsupportedEntry(format!(
                    "{} ({:?})",
                    path.display(),
                    other
                )));
            }
        }
    }

    if !destination.join("manifest.toml").is_file() {
        return Err(PackageError::Invalid(
            "package is missing manifest.toml".into(),
        ));
    }
    Ok(())
}

/// Read and parse `manifest.toml` from an extracted package directory.
pub fn load_manifest(directory: &Path) -> Result<ExtensionManifest, PackageError> {
    let path = directory.join("manifest.toml");
    let text = fs::read_to_string(&path)
        .map_err(|_| PackageError::Invalid("package is missing manifest.toml".into()))?;
    ExtensionManifest::parse_toml(&text)
        .map_err(|e| PackageError::Invalid(format!("invalid manifest.toml: {e}")))
}

/// Read the raw `manifest.toml` bytes so the store can record a stable hash.
pub fn manifest_bytes(directory: &Path) -> Result<Vec<u8>, PackageError> {
    fs::read(directory.join("manifest.toml"))
        .map_err(|_| PackageError::Invalid("package is missing manifest.toml".into()))
}

fn is_safe_archive_path(path: &Path) -> bool {
    if path.is_absolute() || path.as_os_str().is_empty() {
        return false;
    }
    // Reject backslashes so Windows-style traversal cannot slip through on
    // Unix targets, and reject anything that is not a plain relative path.
    if path.to_string_lossy().contains('\\') {
        return false;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
}

fn create_private_dir(path: &Path) -> Result<(), PackageError> {
    fs::create_dir_all(path)?;
    set_mode(path, 0o700);
    Ok(())
}

fn private_file(path: &Path) -> Result<fs::File, PackageError> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    set_mode(path, 0o600);
    Ok(file)
}

fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(!is_safe_archive_path(Path::new("../evil")));
        assert!(!is_safe_archive_path(Path::new("/etc/passwd")));
        assert!(!is_safe_archive_path(Path::new("a/../../evil")));
        assert!(!is_safe_archive_path(Path::new("")));
        assert!(is_safe_archive_path(Path::new("main.rhai")));
        assert!(is_safe_archive_path(Path::new("sub/main.rhai")));
    }
}
