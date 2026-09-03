// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Encryption at rest (R22): tenant payloads are sealed as AES-256-GCM
//! ciphertext under a key derived from a user-held passphrase with Argon2id
//! (memory 64 MiB, 3 iterations) — a stolen device yields ciphertext, not
//! production data.
//!
//! Application-level envelope encryption keeps the build hermetic (pure
//! Rust RustCrypto crates, no C cipher plugins): reference tables (public
//! law data) stay plaintext; everything a customer produced is sealed.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::Argon2;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::errors::DomainError;

/// Argon2id memory cost, KiB (64 MiB).
pub const ARGON2_MEMORY_KIB: u32 = 65_536;
/// Argon2id iterations.
pub const ARGON2_ITERATIONS: u32 = 3;
/// Argon2id parallelism.
pub const ARGON2_PARALLELISM: u32 = 1;
/// AES-GCM nonce length (bytes).
pub const NONCE_LEN: usize = 12;
/// Derived key length (bytes).
pub const KEY_LEN: usize = 32;
/// Salt length (bytes).
pub const SALT_LEN: usize = 16;
/// Envelope format version.
pub const ENVELOPE_VERSION: u8 = 1;

/// A random salt, generated per store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Salt(#[serde(with = "hex_ser")] pub [u8; SALT_LEN]);

/// Generate a fresh random salt.
#[must_use]
pub fn new_salt() -> Salt {
    let mut bytes = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut bytes);
    Salt(bytes)
}

/// Derive the 32-byte store key from a passphrase and salt (Argon2id v19,
/// 64 MiB, t=3, p=1).
///
/// # Errors
///
/// [`DomainError::CryptoError`] when Argon2id fails (never on valid params).
pub fn derive_key(passphrase: &str, salt: &Salt) -> Result<[u8; KEY_LEN], DomainError> {
    let params = argon2::Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(KEY_LEN),
    )
    .map_err(|e| DomainError::CryptoError(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon
        .hash_password_into(passphrase.as_bytes(), &salt.0, &mut key)
        .map_err(|e| DomainError::CryptoError(format!("argon2 kdf: {e}")))?;
    Ok(key)
}

/// A sealed payload: version byte, nonce, ciphertext (includes the GCM tag).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedPayload {
    /// Envelope format version.
    pub version: u8,
    /// AES-GCM nonce, base64.
    #[serde(with = "b64_arr_ser")]
    pub nonce: [u8; NONCE_LEN],
    /// Ciphertext + tag, base64.
    #[serde(with = "b64_ser")]
    pub ciphertext: Vec<u8>,
}

impl SealedPayload {
    /// Encrypt plaintext under `key` with a fresh random nonce. `aad`
    /// (additional authenticated data, e.g. the record id) binds the
    /// ciphertext to its row — swapping ciphertexts between rows fails
    /// decryption.
    ///
    /// # Errors
    ///
    /// [`DomainError::CryptoError`] on AEAD failure.
    pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8], aad: &[u8]) -> Result<Self, DomainError> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| DomainError::CryptoError(format!("aead encrypt: {e}")))?;
        Ok(Self {
            version: ENVELOPE_VERSION,
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt under `key` with the same `aad` used at encryption.
    ///
    /// # Errors
    ///
    /// [`DomainError::CryptoError`] on tag failure (wrong key, wrong aad,
    /// tampered ciphertext) or an unknown envelope version.
    pub fn decrypt(&self, key: &[u8; KEY_LEN], aad: &[u8]) -> Result<Vec<u8>, DomainError> {
        if self.version != ENVELOPE_VERSION {
            return Err(DomainError::CryptoError(format!(
                "unknown envelope version {}",
                self.version
            )));
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
        cipher
            .decrypt(
                Nonce::from_slice(&self.nonce),
                Payload {
                    msg: &self.ciphertext,
                    aad,
                },
            )
            .map_err(|_| {
                DomainError::CryptoError(
                    "aead decrypt failed: wrong key or tampered data".to_string(),
                )
            })
    }
}

/// A key checksum to display (first 8 hex of SHA-256 over the raw key) so a
/// user can confirm two devices derived the same key without exposing it.
#[must_use]
pub fn key_checksum(key: &[u8; KEY_LEN]) -> String {
    let digest = Sha256::digest(key);
    hex::encode(&digest[..4])
}

mod hex_ser {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8; 16], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 16], D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("salt must be 16 bytes"))
    }
}

mod b64_ser {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

mod b64_arr_ser {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer, const N: usize>(v: &[u8; N], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        d: D,
    ) -> Result<[u8; N], D::Error> {
        let s = String::deserialize(d)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("fixed-size field has the wrong length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REGULATORY PIN (R22): AES-256-GCM envelope unlocked by a
    /// passphrase-derived Argon2id key — round-trips with the right key,
    /// fails with the wrong one.
    #[test]
    fn sealed_payload_round_trips_and_rejects_wrong_key() {
        let salt = new_salt();
        let key = derive_key("correct horse battery staple", &salt).expect("kdf");
        let payload = SealedPayload::encrypt(
            &key,
            b"consignment payload {\"cn\":\"73181500\"}",
            b"row-42",
        )
        .expect("encrypt");

        let round = payload.decrypt(&key, b"row-42").expect("decrypt");
        assert_eq!(round, b"consignment payload {\"cn\":\"73181500\"}");

        let wrong = derive_key("wrong passphrase", &salt).expect("kdf");
        assert!(
            payload.decrypt(&wrong, b"row-42").is_err(),
            "wrong key yields ciphertext, not data"
        );

        // The AAD binds ciphertext to its row — no swap attacks.
        assert!(payload.decrypt(&key, b"row-43").is_err());
    }

    #[test]
    fn ciphertext_is_not_plaintext() {
        let salt = new_salt();
        let key = derive_key("pass", &salt).expect("kdf");
        let payload =
            SealedPayload::encrypt(&key, b"KAIMETER-PLAINTEXT-MARKER", b"aad").expect("encrypt");
        let bytes = serde_json::to_vec(&payload).expect("ser");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("KAIMETER-PLAINTEXT-MARKER"),
            "plaintext leaked: {text}"
        );
        assert_eq!(payload.version, ENVELOPE_VERSION);
    }

    #[test]
    fn keys_are_deterministic_per_passphrase_and_salt() {
        let salt = new_salt();
        let a = derive_key("same passphrase", &salt).expect("kdf");
        let b = derive_key("same passphrase", &salt).expect("kdf");
        assert_eq!(a, b);
        let other_salt = new_salt();
        let c = derive_key("same passphrase", &other_salt).expect("kdf");
        assert_ne!(a, c, "a different salt derives a different key");
    }

    #[test]
    fn key_checksum_is_short_and_stable() {
        let salt = new_salt();
        let key = derive_key("check", &salt).expect("kdf");
        assert_eq!(key_checksum(&key), key_checksum(&key));
        assert_eq!(key_checksum(&key).len(), 8);
    }
}
