//! Secure key storage with encryption at rest

use crate::{Did, KeyPair};
use anyhow::{Context, Result};
use secrecy::{Secret, Zeroize};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Trait for secure key storage backends
pub trait KeyStore: Send + Sync {
    /// Unlock the keystore with a passphrase
    fn unlock(&mut self, passphrase: &[u8]) -> Result<()>;

    /// Lock the keystore (clear in-memory keys)
    fn lock(&mut self);

    /// Check if the keystore is currently locked
    fn is_locked(&self) -> bool;

    /// Get the keypair (fails if locked)
    fn get_keypair(&self) -> Result<&KeyPair>;

    /// Rotate to a new keypair
    fn rotate(&mut self, new_keypair: KeyPair) -> Result<KeyRotation>;

    /// Get the storage path
    fn path(&self) -> &Path;
}

/// Key rotation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotation {
    pub old_did: Did,
    pub new_did: Did,
    pub timestamp: u64,
    pub reason: RotationReason,
    pub signature_old: Vec<u8>, // Signature from old key
    pub signature_new: Vec<u8>, // Signature from new key
}

/// Reason for key rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationReason {
    Scheduled,
    Compromised,
    Upgrade,
    Manual,
}

/// Serialized key material
#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct StoredKey {
    secret_bytes: [u8; 32],
    public_bytes: [u8; 32],
    did: String,
}

/// Age-encrypted key storage
pub struct AgeKeyStore {
    path: PathBuf,
    keypair: Option<KeyPair>,
}

impl AgeKeyStore {
    /// Create a new keystore at the given path
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            keypair: None,
        }
    }

    /// Initialize a new keystore with a generated keypair
    pub fn init(path: impl Into<PathBuf>, passphrase: &[u8]) -> Result<Self> {
        let path = path.into();

        // Check if keystore already exists
        if path.exists() {
            anyhow::bail!("Keystore already exists at {:?}", path);
        }

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create keystore directory")?;
        }

        // Generate new keypair
        let keypair = KeyPair::generate()?;
        info!("Generated new identity: {}", keypair.did());

        // Encrypt and save
        let stored = StoredKey {
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: keypair.did().as_str().to_string(),
        };

        Self::encrypt_and_save(&path, &stored, passphrase)?;
        info!("Saved encrypted keystore to {:?}", path);

        Ok(Self {
            path,
            keypair: Some(keypair),
        })
    }

    /// Open an existing keystore (locked)
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        if !path.exists() {
            anyhow::bail!("Keystore not found at {:?}", path);
        }

        Ok(Self {
            path,
            keypair: None,
        })
    }

    /// Encrypt and save key material
    fn encrypt_and_save(path: &Path, stored: &StoredKey, passphrase: &[u8]) -> Result<()> {
        // Serialize key material
        let json = serde_json::to_vec(stored)?;

        // Create age encryptor with passphrase
        let encryptor = age::Encryptor::with_user_passphrase(Secret::new(
            String::from_utf8(passphrase.to_vec())
                .context("Passphrase must be valid UTF-8")?,
        ));

        // Encrypt
        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .context("Failed to create age writer")?;

        std::io::copy(&mut json.as_slice(), &mut writer)
            .context("Failed to encrypt key material")?;

        writer
            .finish()
            .and_then(|_| Ok(()))
            .context("Failed to finalize encryption")?;

        // Write to file
        std::fs::write(path, encrypted).context("Failed to write keystore file")?;

        Ok(())
    }

    /// Decrypt and load key material
    fn decrypt_and_load(path: &Path, passphrase: &[u8]) -> Result<StoredKey> {
        // Read encrypted file
        let encrypted = std::fs::read(path).context("Failed to read keystore file")?;

        // Create age decryptor
        let decryptor = match age::Decryptor::new(encrypted.as_slice())? {
            age::Decryptor::Passphrase(d) => d,
            _ => anyhow::bail!("Unsupported age encryption type"),
        };

        // Decrypt
        let passphrase_str =
            String::from_utf8(passphrase.to_vec()).context("Passphrase must be valid UTF-8")?;

        let mut decrypted = Vec::new();
        let mut reader = decryptor
            .decrypt(&Secret::new(passphrase_str), None)
            .context("Failed to decrypt (wrong passphrase?)")?;

        std::io::copy(&mut reader, &mut decrypted).context("Failed to read decrypted data")?;

        // Deserialize
        let stored: StoredKey =
            serde_json::from_slice(&decrypted).context("Failed to parse key material")?;

        Ok(stored)
    }
}

impl KeyStore for AgeKeyStore {
    fn unlock(&mut self, passphrase: &[u8]) -> Result<()> {
        if self.keypair.is_some() {
            warn!("Keystore already unlocked");
            return Ok(());
        }

        // Decrypt and load
        let stored = Self::decrypt_and_load(&self.path, passphrase)?;

        // Reconstruct keypair
        let keypair = KeyPair::from_bytes(&stored.secret_bytes, &stored.public_bytes)?;

        info!("Unlocked keystore: {}", keypair.did());
        self.keypair = Some(keypair);

        Ok(())
    }

    fn lock(&mut self) {
        if self.keypair.is_some() {
            info!("Locking keystore");
            self.keypair = None;
        }
    }

    fn is_locked(&self) -> bool {
        self.keypair.is_none()
    }

    fn get_keypair(&self) -> Result<&KeyPair> {
        self.keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Keystore is locked"))
    }

    fn rotate(&mut self, new_keypair: KeyPair) -> Result<KeyRotation> {
        let old_keypair = self.get_keypair()?;

        // Create rotation record
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let message = format!("key-rotation:{}:{}:{}", old_keypair.did(), new_keypair.did(), timestamp);

        let rotation = KeyRotation {
            old_did: old_keypair.did().clone(),
            new_did: new_keypair.did().clone(),
            timestamp,
            reason: RotationReason::Manual,
            signature_old: old_keypair.sign(message.as_bytes()).to_vec(),
            signature_new: new_keypair.sign(message.as_bytes()).to_vec(),
        };

        info!(
            "Rotating key: {} -> {}",
            rotation.old_did, rotation.new_did
        );

        // Replace keypair
        self.keypair = Some(new_keypair);

        Ok(rotation)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_keystore_init_unlock() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        assert!(!ks.is_locked());

        let did = ks.get_keypair().unwrap().did().clone();

        // Lock and reopen
        ks.lock();
        assert!(ks.is_locked());

        let mut ks2 = AgeKeyStore::open(&path).unwrap();
        assert!(ks2.is_locked());

        // Unlock with correct passphrase
        ks2.unlock(passphrase).unwrap();
        assert!(!ks2.is_locked());
        assert_eq!(ks2.get_keypair().unwrap().did(), &did);

        // Lock again
        ks2.lock();
        assert!(ks2.is_locked());

        // Wrong passphrase should fail
        assert!(ks2.unlock(b"wrong-passphrase").is_err());
    }

    #[test]
    fn test_key_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        let old_did = ks.get_keypair().unwrap().did().clone();

        // Rotate
        let new_keypair = KeyPair::generate().unwrap();
        let new_did = new_keypair.did().clone();

        let rotation = ks.rotate(new_keypair).unwrap();

        assert_eq!(rotation.old_did, old_did);
        assert_eq!(rotation.new_did, new_did);
        assert_eq!(ks.get_keypair().unwrap().did(), &new_did);
    }
}
