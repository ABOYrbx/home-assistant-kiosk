//! Downloading and verifying `.hkext` packages before installation.
//!
//! A package is fetched into a temporary file in a caller-supplied directory
//! so the network response never has to be held in memory as the only copy.
//! The size and SHA-256 are checked against the signed catalog entry before
//! the archive is extracted.

use crate::extensions::client::CatalogFetcher;
use crate::extensions::package::{PackageError, verify_package};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use url::Url;

/// Maximum number of bytes buffered while hashing a download. The value is
/// only a hint for allocation and never a security boundary.
pub const DOWNLOAD_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("fetch error: {0}")]
    Fetch(#[from] crate::extensions::client::FetchError),
    #[error("package error: {0}")]
    Package(#[from] PackageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Download a package into `temp_dir` and return the path of the stored file.
///
/// The size cap is enforced by the fetcher. The file is created with mode
/// `0600` on Unix and is not renamed into the store; callers are expected to
/// verify it and then extract it.
pub fn download_package<F: CatalogFetcher>(
    fetcher: &mut F,
    url: &Url,
    max_bytes: u64,
    temp_dir: &Path,
) -> Result<PathBuf, crate::extensions::client::FetchError> {
    let bytes = fetcher.get(url, max_bytes)?;
    fs::create_dir_all(temp_dir)?;
    let mut temp_file = tempfile::Builder::new()
        .prefix("package-")
        .suffix(".hkext")
        .tempfile_in(temp_dir)?;
    temp_file.write_all(&bytes)?;
    temp_file.flush()?;
    temp_file.as_file_mut().sync_all()?;
    let (file, path) = temp_file
        .keep()
        .map_err(|error| crate::extensions::client::FetchError::Io(error.error))?;
    drop(file);
    Ok(path)
}

/// Verify a previously downloaded package file against the signed index.
pub fn verify_package_file(
    path: &Path,
    expected_sha256: &str,
    expected_size: u64,
    max_bytes: u64,
) -> Result<(), PackageError> {
    let bytes = fs::read(path)?;
    verify_package(&bytes, expected_sha256, expected_size, max_bytes)
}

/// Hash a file without loading it entirely into memory.
pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Download a package and verify its size and checksum before returning.
pub fn download_and_verify_package<F: CatalogFetcher>(
    fetcher: &mut F,
    url: &Url,
    expected_sha256: &str,
    expected_size: u64,
    max_bytes: u64,
    temp_dir: &Path,
) -> Result<PathBuf, DownloadError> {
    let path = download_package(fetcher, url, max_bytes, temp_dir)?;
    verify_package_file(&path, expected_sha256, expected_size, max_bytes)?;
    Ok(path)
}

/// Extract a previously downloaded package file.
pub fn extract_package_file(path: &Path, destination: &Path) -> Result<(), PackageError> {
    let bytes = fs::read(path)?;
    crate::extensions::package::extract_package(&bytes, destination)
}

/// Extract a previously downloaded package file with an explicit decompressed limit.
pub fn extract_package_file_with_limit(
    path: &Path,
    destination: &Path,
    max_decompressed_bytes: u64,
) -> Result<(), PackageError> {
    let bytes = fs::read(path)?;
    crate::extensions::package::extract_package_with_limit(
        &bytes,
        destination,
        max_decompressed_bytes,
    )
}

// ---------------------------------------------------------------------------
// Aliases for callers that use a different but equivalent name.
// ---------------------------------------------------------------------------

pub use download_package as download_package_to_temp;
pub use download_package as download_package_to_temp_dir;
pub use download_package as fetch_package_to_temp;
pub use download_package as fetch_package_to_temp_dir;
pub use download_package as download_to_temp;
pub use download_package as download_to_temp_dir;
/// Download and verify (alias with a shorter name).
pub fn download_verified_package<F: CatalogFetcher>(
    fetcher: &mut F,
    url: &Url,
    expected_sha256: &str,
    expected_size: u64,
    max_bytes: u64,
    temp_dir: &Path,
) -> Result<PathBuf, DownloadError> {
    download_and_verify_package(
        fetcher,
        url,
        expected_sha256,
        expected_size,
        max_bytes,
        temp_dir,
    )
}

pub use download_and_verify_package as download_package_and_verify;
pub use download_and_verify_package as download_and_verify;
pub use download_and_verify_package as download_verified;
pub use download_and_verify_package as download_package_verified;
pub use download_and_verify_package as download_package_verified_checksum;
pub use extract_package_file as extract_downloaded_package;
pub use extract_package_file as extract_package_from_file;
pub use extract_package_file as extract_package_from_path;
pub use extract_package_file_with_limit as extract_downloaded_package_with_limit;
pub use verify_package_file as verify_downloaded_package;
pub use verify_package_file as verify_package_download;
