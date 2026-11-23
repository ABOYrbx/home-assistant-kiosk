//! Minisign verification for the signed catalog index.
//!
//! The configured public keys are the only trust anchor. Key rotation works by
//! listing both the old and the new key; any matching key is accepted.

use crate::extensions::catalog::CatalogPolicy;
use minisign_verify::{PublicKey, Signature};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignatureError {
    #[error("no Minisign public key is configured")]
    NoPublicKeys,
    #[error("the catalog signature text is invalid: {0}")]
    InvalidSignature(String),
    #[error("no configured Minisign public key is valid")]
    InvalidPublicKeys,
    #[error("no configured Minisign public key verified the catalog")]
    NoMatchingKey,
    #[error("unsigned catalogs require allow_unsigned and dev_mode")]
    UnsignedNotAllowed,
}

/// Result of verifying the catalog signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    /// Signature verified against the key at this index in the config.
    Verified { key_index: usize },
    /// Unsigned catalog accepted in development mode only.
    Unsigned,
}

/// Verify the signature over the exact catalog bytes.
pub fn verify_signature(
    index: &[u8],
    signature_text: &str,
    public_keys: &[String],
) -> Result<SignatureStatus, SignatureError> {
    if public_keys.is_empty() {
        return Err(SignatureError::NoPublicKeys);
    }
    let signature = Signature::decode(signature_text)
        .map_err(|e| SignatureError::InvalidSignature(e.to_string()))?;
    let mut saw_valid_key = false;
    for (key_index, key) in public_keys.iter().enumerate() {
        let public_key = match PublicKey::from_base64(key.trim()) {
            Ok(key) => key,
            Err(error) => {
                tracing::warn!(
                    "ignoring invalid Minisign public key at index {key_index}: {error}"
                );
                continue;
            }
        };
        saw_valid_key = true;
        match public_key.verify(index, &signature, false) {
            Ok(()) => return Ok(SignatureStatus::Verified { key_index }),
            Err(error) => {
                tracing::debug!("Minisign key at index {key_index} did not verify: {error}");
            }
        }
    }
    if saw_valid_key {
        Err(SignatureError::NoMatchingKey)
    } else {
        Err(SignatureError::InvalidPublicKeys)
    }
}

/// Accept an unsigned catalog only when both development flags are on.
pub fn verify_unsigned(policy: &CatalogPolicy) -> Result<SignatureStatus, SignatureError> {
    if policy.allow_unsigned && policy.dev_mode {
        Ok(SignatureStatus::Unsigned)
    } else {
        Err(SignatureError::UnsignedNotAllowed)
    }
}
