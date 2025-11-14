//! Secure key storage with encryption at rest

use crate::{Did, IdentityBundle, KeyPair};
use anyhow::{Context, Result};
use secrecy::{Secret, Zeroize};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use zeroize::Zeroizing;

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

/// Serialized key material (v2 format with optional TLS binding)
#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct StoredKey {
    secret_bytes: [u8; 32],
    public_bytes: [u8; 32],
    did: String,

    // v2 fields for IdentityBundle (optional for backward compatibility)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tls_cert_der: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tls_key_der: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tls_binding_sig: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<u64>,
}

/// Age-encrypted key storage
pub struct AgeKeyStore {
    path: PathBuf,
    identity_bundle: Option<IdentityBundle>,
}

impl AgeKeyStore {
    /// Create a new keystore at the given path
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            identity_bundle: None,
        }
    }

    /// Get the identity bundle (fails if locked)
    pub fn get_identity_bundle(&self) -> Result<&IdentityBundle> {
        self.identity_bundle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Keystore is locked"))
    }

    /// Initialize a new keystore with a generated identity bundle
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

        // Generate new identity bundle with DID-TLS binding
        let identity_bundle = IdentityBundle::generate()?;
        info!("Generated new identity with DID-TLS binding: {}", identity_bundle.did());

        // Extract key material and TLS binding info for storage
        let keypair = identity_bundle.keypair();
        let stored = StoredKey {
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: Some(identity_bundle.tls_cert().as_ref().to_vec()),
            tls_key_der: Some(identity_bundle.tls_key_der_bytes().to_vec()),
            tls_binding_sig: Some(identity_bundle.binding_info().tls_binding_sig.clone()),
            created_at: Some(identity_bundle.binding_info().created_at),
        };

        Self::encrypt_and_save(&path, &stored, passphrase)?;
        info!("Saved encrypted identity bundle to {:?}", path);

        Ok(Self {
            path,
            identity_bundle: Some(identity_bundle),
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
            identity_bundle: None,
        })
    }

    /// Encrypt and save key material
    fn encrypt_and_save(path: &Path, stored: &StoredKey, passphrase: &[u8]) -> Result<()> {
        // Serialize key material - use Zeroizing to ensure plaintext is cleared
        let json = Zeroizing::new(serde_json::to_vec(stored)?);

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

        // json is automatically zeroized when dropped here
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
        if self.identity_bundle.is_some() {
            warn!("Keystore already unlocked");
            return Ok(());
        }

        // Decrypt and load
        let stored = Self::decrypt_and_load(&self.path, passphrase)?;

        // Reconstruct keypair from stored bytes
        let keypair = KeyPair::from_bytes(&stored.secret_bytes, &stored.public_bytes)?;

        // Check if we have TLS binding info (v2 keystore)
        let identity_bundle = if let (Some(tls_cert_der), Some(tls_key_der), Some(tls_binding_sig), Some(created_at)) =
            (stored.tls_cert_der.clone(), stored.tls_key_der.clone(), stored.tls_binding_sig.clone(), stored.created_at) {
            // V2 keystore: reconstruct IdentityBundle from stored TLS data
            info!("Unlocked v2 keystore with DID-TLS binding: {}", keypair.did());

            // Reconstruct IdentityBundle using the stored data
            // We need to access the private fields, so we'll use the from_stored method
            IdentityBundle::from_stored(
                keypair,
                tls_cert_der,
                tls_key_der,
                tls_binding_sig,
                created_at,
            )?
        } else {
            // V1 keystore: generate new TLS certificate and binding
            info!("Unlocked v1 keystore: {} (generating DID-TLS binding)", keypair.did());
            warn!("⚠️  Migrating v1 keystore to v2 format with DID-TLS binding");

            // Generate new IdentityBundle from the keypair
            let bundle = IdentityBundle::from_keypair(keypair.clone())?;

            // Auto-save the upgraded keystore with TLS binding
            // This ensures TLS certificates persist across restarts
            let stored_v2 = StoredKey {
                secret_bytes: *keypair.secret_bytes(),
                public_bytes: keypair.verifying_key().to_bytes(),
                did: bundle.did().as_str().to_string(),
                tls_cert_der: Some(bundle.tls_cert().as_ref().to_vec()),
                tls_key_der: Some(bundle.tls_key_der_bytes().to_vec()),
                tls_binding_sig: Some(bundle.binding_info().tls_binding_sig.clone()),
                created_at: Some(bundle.binding_info().created_at),
            };

            Self::encrypt_and_save(&self.path, &stored_v2, passphrase)
                .context("Failed to save upgraded v2 keystore")?;

            info!("✅ Successfully migrated and saved v2 keystore with persistent TLS binding");

            bundle
        };

        self.identity_bundle = Some(identity_bundle);
        Ok(())
    }

    fn lock(&mut self) {
        if self.identity_bundle.is_some() {
            info!("Locking keystore");
            self.identity_bundle = None;
        }
    }

    fn is_locked(&self) -> bool {
        self.identity_bundle.is_none()
    }

    fn get_keypair(&self) -> Result<&KeyPair> {
        Ok(self.get_identity_bundle()?.keypair())
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

        // Create new identity bundle with fresh TLS binding
        let new_bundle = IdentityBundle::from_keypair(new_keypair)?;

        // Replace identity bundle
        self.identity_bundle = Some(new_bundle);

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

    #[test]
    fn test_v1_to_v2_migration_persists_tls() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Create a v1 keystore manually (no TLS binding fields)
        let keypair = KeyPair::generate().unwrap();
        let stored_v1 = StoredKey {
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: keypair.did().as_str().to_string(),
            tls_cert_der: None,
            tls_key_der: None,
            tls_binding_sig: None,
            created_at: None,
        };
        AgeKeyStore::encrypt_and_save(&path, &stored_v1, passphrase).unwrap();

        // First unlock: should trigger v1->v2 migration
        let mut ks = AgeKeyStore::open(&path).unwrap();
        ks.unlock(passphrase).unwrap();

        // Get TLS certificate from migrated bundle
        let bundle1 = ks.get_identity_bundle().unwrap();
        let cert1_der = bundle1.tls_cert().as_ref().to_vec();
        let binding_sig1 = bundle1.binding_info().tls_binding_sig.clone();

        // Lock and unlock again
        ks.lock();
        ks.unlock(passphrase).unwrap();

        // Get TLS certificate again
        let bundle2 = ks.get_identity_bundle().unwrap();
        let cert2_der = bundle2.tls_cert().as_ref().to_vec();
        let binding_sig2 = bundle2.binding_info().tls_binding_sig.clone();

        // TLS certificates should be IDENTICAL (not regenerated)
        assert_eq!(cert1_der, cert2_der, "TLS certificate should persist across unlocks");
        assert_eq!(binding_sig1, binding_sig2, "TLS binding signature should persist across unlocks");

        // Open in a new keystore instance to verify disk persistence
        let mut ks3 = AgeKeyStore::open(&path).unwrap();
        ks3.unlock(passphrase).unwrap();
        let bundle3 = ks3.get_identity_bundle().unwrap();
        let cert3_der = bundle3.tls_cert().as_ref().to_vec();

        assert_eq!(cert1_der, cert3_der, "TLS certificate should persist to disk");
    }
}
