// src/credentials.rs
//!
//! Minimal local protection for Provider credentials.
//!
//! Provider API keys are secrets that must never be stored as plaintext, logged,
//! or returned through ordinary API responses. This module provides a small,
//! explicit abstraction for protecting them at rest.
//!
//! The protection key is intentionally separate from any Provider API key:
//!
//! * `XCONTEXT_CREDENTIAL_KEY`, when set, is a *passphrase* used only to derive
//!   the local credential-store protection key. It is never sent to a Provider
//!   and never treated as an API key.
//! * When the environment variable is absent, a per-database key file is created
//!   (base64, restricted permissions on Unix) next to the SQLite database.
//!
//! Secrets are sealed with AES-256-GCM (authenticated encryption). Each sealed
//! value carries its scheme marker and a random nonce.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Environment variable holding the local credential-store passphrase.
pub const CREDENTIAL_KEY_ENV: &str = "XCONTEXT_CREDENTIAL_KEY";

/// Sealed-value scheme marker, persisted alongside each credential.
pub const CREDENTIAL_SCHEME: &str = "aes-256-gcm";

const KEY_FILE_NAME: &str = "credentials.key";

/// Symmetric cipher used to protect Provider secrets at rest.
#[derive(Clone)]
pub struct CredentialCipher {
    key: [u8; 32],
}

impl std::fmt::Debug for CredentialCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialCipher(***)")
    }
}

impl CredentialCipher {
    pub fn from_key_bytes(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Derive a key from a passphrase (e.g. `XCONTEXT_CREDENTIAL_KEY`).
    pub fn from_passphrase(passphrase: &str) -> Self {
        let digest = Sha256::digest(passphrase.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);
        Self { key }
    }

    /// Generate a fresh random key.
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        SystemRandom::new()
            .fill(&mut key)
            .expect("system RNG unavailable");
        Self { key }
    }

    /// Load the protection key for a database.
    ///
    /// Priority: `XCONTEXT_CREDENTIAL_KEY` passphrase, then an existing key
    /// file, then a newly generated key file.
    pub fn load_for(db_path: &Path) -> Result<Self> {
        if let Ok(passphrase) = std::env::var(CREDENTIAL_KEY_ENV) {
            let passphrase = passphrase.trim();
            if !passphrase.is_empty() {
                return Ok(Self::from_passphrase(passphrase));
            }
        }

        let key_path = key_file_path(db_path);
        if key_path.is_file() {
            let raw = std::fs::read(&key_path)
                .with_context(|| format!("failed to read credential key {}", key_path.display()))?;
            let text = String::from_utf8_lossy(&raw);
            let bytes = STANDARD
                .decode(text.trim())
                .context("credential key file is not valid base64")?;
            let key: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow!("credential key must be 32 bytes"))?;
            return Ok(Self::from_key_bytes(key));
        }

        let cipher = Self::generate();
        cipher.persist_key(&key_path)?;
        Ok(cipher)
    }

    pub fn scheme(&self) -> &'static str {
        CREDENTIAL_SCHEME
    }

    /// Seal a secret into a scheme-tagged, base64-encoded payload.
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let key = self.less_safe_key()?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("failed to generate credential nonce"))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plaintext.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| anyhow!("failed to encrypt credential"))?;

        let mut payload = nonce_bytes.to_vec();
        payload.extend_from_slice(&in_out);
        Ok(format!("{CREDENTIAL_SCHEME}:{}", STANDARD.encode(payload)))
    }

    /// Open a value produced by [`CredentialCipher::encrypt`].
    pub fn decrypt(&self, sealed: &str) -> Result<String> {
        let payload = sealed
            .strip_prefix(&format!("{CREDENTIAL_SCHEME}:"))
            .unwrap_or(sealed);
        let raw = STANDARD
            .decode(payload)
            .context("credential is not valid base64")?;
        if raw.len() <= NONCE_LEN {
            return Err(anyhow!("credential payload is too short"));
        }
        let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
        let key = self.less_safe_key()?;
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| anyhow!("credential nonce is invalid"))?;
        let mut in_out = ciphertext.to_vec();
        let plaintext = key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| anyhow!("failed to decrypt credential"))?;
        String::from_utf8(plaintext.to_vec()).context("credential is not valid UTF-8")
    }

    fn less_safe_key(&self) -> Result<LessSafeKey> {
        let unbound = UnboundKey::new(&AES_256_GCM, &self.key)
            .map_err(|_| anyhow!("failed to initialize credential cipher"))?;
        Ok(LessSafeKey::new(unbound))
    }

    fn persist_key(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, STANDARD.encode(self.key))
            .with_context(|| format!("failed to write credential key {}", path.display()))?;
        restrict_permissions(path)?;
        Ok(())
    }
}

fn key_file_path(db_path: &Path) -> PathBuf {
    match db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(dir) => dir.join(KEY_FILE_NAME),
        None => PathBuf::from(KEY_FILE_NAME),
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_secret() {
        let cipher = CredentialCipher::from_passphrase("test-passphrase");
        let sealed = cipher.encrypt("sk-super-secret").unwrap();
        assert!(!sealed.contains("sk-super-secret"));
        assert!(sealed.starts_with(CREDENTIAL_SCHEME));
        assert_eq!(cipher.decrypt(&sealed).unwrap(), "sk-super-secret");
    }

    #[test]
    fn wrong_key_cannot_decrypt() {
        let a = CredentialCipher::from_passphrase("a");
        let b = CredentialCipher::from_passphrase("b");
        let sealed = a.encrypt("secret").unwrap();
        assert!(b.decrypt(&sealed).is_err());
    }

    #[test]
    fn distinct_nonces_produce_distinct_payloads() {
        let cipher = CredentialCipher::from_passphrase("k");
        assert_ne!(
            cipher.encrypt("same").unwrap(),
            cipher.encrypt("same").unwrap()
        );
    }
}
