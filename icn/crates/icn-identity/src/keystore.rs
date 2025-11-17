//! Secure key storage with encryption at rest

use crate::{Did, DidDocument, IdentityBundle, KeyPair, RotationEvent};
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

/// Serialized key material (v2 format with optional TLS binding + X25519 encryption keys)
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

    // X25519 encryption keys (v2.1 addition)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x25519_secret: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x25519_public: Option<[u8; 32]>,
}

/// Keystore v3 format: Multi-device support with DID Document
#[derive(Serialize, Deserialize)]
struct StoredKeyV3 {
    /// Format version (always 3 for this struct)
    version: u8,

    /// This device's identity keys (SENSITIVE - must be zeroized)
    secret_bytes: [u8; 32],
    public_bytes: [u8; 32],
    did: String,

    /// TLS binding (from IdentityBundle) (SENSITIVE - must be zeroized)
    tls_cert_der: Vec<u8>,
    tls_key_der: Vec<u8>,
    tls_binding_sig: Vec<u8>,
    created_at: u64,

    /// X25519 encryption keys (SENSITIVE - must be zeroized)
    x25519_secret: Vec<u8>,
    x25519_public: [u8; 32],

    /// DID Document (public, no need to zeroize)
    did_document: DidDocument,

    /// This device's ID in the DID Document
    device_id: String,

    /// Rotation event history (public, no need to zeroize)
    rotation_chain: Vec<RotationEvent>,
}

impl Drop for StoredKeyV3 {
    fn drop(&mut self) {
        // Zeroize sensitive fields
        self.secret_bytes.zeroize();
        self.x25519_secret.zeroize();
        self.tls_key_der.zeroize();
    }
}

/// Age-encrypted key storage
pub struct AgeKeyStore {
    path: PathBuf,
    identity_bundle: Option<IdentityBundle>,
    did_document: Option<DidDocument>,
    device_id: Option<String>,
    rotation_chain: Vec<RotationEvent>,
}

impl AgeKeyStore {
    /// Create a new keystore at the given path
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            identity_bundle: None,
            did_document: None,
            device_id: None,
            rotation_chain: Vec::new(),
        }
    }

    /// Get the identity bundle (fails if locked)
    pub fn get_identity_bundle(&self) -> Result<&IdentityBundle> {
        self.identity_bundle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Keystore is locked"))
    }

    /// Get the DID document (fails if locked or v2.1 keystore)
    pub fn get_did_document(&self) -> Result<&DidDocument> {
        self.did_document
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DID document not available (keystore locked or v2.1 format)"))
    }

    /// Get this device's ID in the DID document (fails if locked or v2.1 keystore)
    pub fn get_device_id(&self) -> Result<&str> {
        self.device_id
            .as_ref()
            .map(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("Device ID not available"))
    }

    /// Get the rotation event chain
    pub fn get_rotation_chain(&self) -> &[RotationEvent] {
        &self.rotation_chain
    }

    /// Update the DID document and save to disk
    ///
    /// This method:
    /// 1. Checks that keystore is unlocked
    /// 2. Applies the update function to modify the DID document
    /// 3. Optionally adds a rotation event to the chain
    /// 4. Saves the updated keystore to disk
    ///
    /// # Arguments
    /// * `update_fn` - Function that modifies the DID document
    /// * `rotation_event` - Optional rotation event to append to the chain
    /// * `passphrase` - Passphrase to encrypt the keystore
    ///
    /// # Example
    /// ```no_run
    /// # use icn_identity::*;
    /// # use std::path::Path;
    /// # fn example(keystore: &mut AgeKeyStore, passphrase: &[u8]) -> anyhow::Result<()> {
    /// keystore.update_did_document(
    ///     |did_doc| {
    ///         did_doc.add_device(
    ///             "device-2".to_string(),
    ///             "Laptop".to_string(),
    ///             vec![0u8; 32],
    ///             KeyType::Ed25519,
    ///             vec![Capability::Sign],
    ///         )
    ///     },
    ///     None,
    ///     passphrase,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_did_document<F>(
        &mut self,
        update_fn: F,
        rotation_event: Option<RotationEvent>,
        passphrase: &[u8],
    ) -> Result<()>
    where
        F: FnOnce(&mut DidDocument) -> Result<()>,
    {
        // Ensure keystore is unlocked
        let identity_bundle = self
            .identity_bundle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Keystore is locked"))?;

        let mut did_document = self
            .did_document
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DID document not available"))?
            .clone();

        let device_id = self
            .device_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Device ID not available"))?
            .clone();

        // Apply update
        update_fn(&mut did_document)?;

        // Append rotation event if provided
        let mut rotation_chain = self.rotation_chain.clone();
        if let Some(event) = rotation_event {
            rotation_chain.push(event);
        }

        // Save to disk
        let keypair = identity_bundle.keypair();
        let stored_v3 = StoredKeyV3 {
            version: 3,
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: identity_bundle.tls_key_der_bytes().to_vec(),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: identity_bundle.x25519_secret_bytes().to_vec(),
            x25519_public: *identity_bundle.x25519_public_bytes(),
            did_document: did_document.clone(),
            device_id: device_id.clone(),
            rotation_chain: rotation_chain.clone(),
        };

        Self::encrypt_and_save_v3(&self.path, &stored_v3, passphrase)?;

        // Update in-memory state
        self.did_document = Some(did_document);
        self.device_id = Some(device_id);
        self.rotation_chain = rotation_chain;

        Ok(())
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

        // Create DID Document v2
        let did_document = DidDocument::new(
            identity_bundle.did().clone(),
            identity_bundle.keypair().verifying_key(),
            identity_bundle.x25519_public_bytes(),
        );

        // Extract key material for storage (v3 format)
        let keypair = identity_bundle.keypair();
        let stored_v3 = StoredKeyV3 {
            version: 3,
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: identity_bundle.tls_key_der_bytes().to_vec(),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: identity_bundle.x25519_secret_bytes().to_vec(),
            x25519_public: *identity_bundle.x25519_public_bytes(),
            did_document: did_document.clone(),
            device_id: "device-1".to_string(),
            rotation_chain: Vec::new(),
        };

        Self::encrypt_and_save_v3(&path, &stored_v3, passphrase)?;
        info!("Saved encrypted v3 keystore (multi-device) to {:?}", path);

        Ok(Self {
            path,
            identity_bundle: Some(identity_bundle),
            did_document: Some(did_document),
            device_id: Some("device-1".to_string()),
            rotation_chain: Vec::new(),
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
            did_document: None,
            device_id: None,
            rotation_chain: Vec::new(),
        })
    }

    /// Encrypt and save key material (legacy, used in migration tests)
    #[allow(dead_code)]
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

    /// Encrypt and save v3 key material
    fn encrypt_and_save_v3(path: &Path, stored: &StoredKeyV3, passphrase: &[u8]) -> Result<()> {
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

    /// Decrypt and load v3 key material
    fn decrypt_and_load_v3(path: &Path, passphrase: &[u8]) -> Result<StoredKeyV3> {
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
        let stored: StoredKeyV3 =
            serde_json::from_slice(&decrypted).context("Failed to parse v3 key material")?;

        Ok(stored)
    }
}

impl KeyStore for AgeKeyStore {
    fn unlock(&mut self, passphrase: &[u8]) -> Result<()> {
        if self.identity_bundle.is_some() {
            warn!("Keystore already unlocked");
            return Ok(());
        }

        // Try loading as v3 first
        if let Ok(stored_v3) = Self::decrypt_and_load_v3(&self.path, passphrase) {
            // V3 keystore: has multi-device support
            info!("Unlocked v3 keystore (multi-device) for DID: {}", stored_v3.did);

            // Reconstruct keypair
            let keypair = KeyPair::from_bytes(&stored_v3.secret_bytes, &stored_v3.public_bytes)?;

            // Reconstruct IdentityBundle
            let identity_bundle = IdentityBundle::from_stored(
                keypair,
                stored_v3.tls_cert_der.clone(),
                stored_v3.tls_key_der.clone(),
                stored_v3.tls_binding_sig.clone(),
                stored_v3.created_at,
                stored_v3.x25519_secret.clone(),
                stored_v3.x25519_public,
            )?;

            // Load DID Document and device info
            self.identity_bundle = Some(identity_bundle);
            self.did_document = Some(stored_v3.did_document.clone());
            self.device_id = Some(stored_v3.device_id.clone());
            self.rotation_chain = stored_v3.rotation_chain.clone();

            return Ok(());
        }

        // Fall back to v2.1/v2/v1 format and migrate to v3
        info!("Not a v3 keystore, trying legacy format...");
        let stored = Self::decrypt_and_load(&self.path, passphrase)?;

        // Reconstruct keypair from stored bytes
        let keypair = KeyPair::from_bytes(&stored.secret_bytes, &stored.public_bytes)?;

        // Check if we have TLS binding info (v2+ keystore)
        let identity_bundle = if let (Some(tls_cert_der), Some(tls_key_der), Some(tls_binding_sig), Some(created_at)) =
            (stored.tls_cert_der.clone(), stored.tls_key_der.clone(), stored.tls_binding_sig.clone(), stored.created_at) {

            // Check if we have X25519 keys (v2.1+)
            let (x25519_secret, x25519_public) = if let (Some(secret), Some(public)) =
                (stored.x25519_secret.clone(), stored.x25519_public) {
                // V2.1+ keystore: has X25519 keys
                info!("Unlocked v2.1 keystore, migrating to v3 (multi-device)");
                (secret, public)
            } else {
                // V2.0 keystore: has TLS but no X25519, generate new X25519 keys
                info!("Unlocked v2.0 keystore, migrating to v3 with X25519 keys");
                warn!("⚠️  Generating X25519 encryption keys");

                // Generate X25519 keys
                use rand::rngs::OsRng;
                use x25519_dalek::{PublicKey, StaticSecret};

                let secret_key = StaticSecret::random_from_rng(OsRng);
                let public_key = PublicKey::from(&secret_key);
                let secret_bytes = secret_key.to_bytes().to_vec();
                let public_bytes = public_key.to_bytes();

                (secret_bytes, public_bytes)
            };

            // Reconstruct IdentityBundle using the stored data
            IdentityBundle::from_stored(
                keypair.clone(),
                tls_cert_der,
                tls_key_der,
                tls_binding_sig,
                created_at,
                x25519_secret.clone(),
                x25519_public,
            )?
        } else {
            // V1 keystore: generate new TLS certificate and binding + X25519 keys
            info!("Unlocked v1 keystore, migrating to v3 with TLS binding and X25519 keys");
            warn!("⚠️  Generating TLS binding and X25519 encryption keys");

            // Generate new IdentityBundle from the keypair (includes X25519 keys)
            IdentityBundle::from_keypair(keypair.clone())?
        };

        // Create DID Document v2 for this identity
        let did_document = DidDocument::new(
            identity_bundle.did().clone(),
            identity_bundle.keypair().verifying_key(),
            identity_bundle.x25519_public_bytes(),
        );

        // Save as v3 keystore
        let stored_v3 = StoredKeyV3 {
            version: 3,
            secret_bytes: *identity_bundle.keypair().secret_bytes(),
            public_bytes: identity_bundle.keypair().verifying_key().to_bytes(),
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: identity_bundle.tls_key_der_bytes().to_vec(),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: identity_bundle.x25519_secret_bytes().to_vec(),
            x25519_public: *identity_bundle.x25519_public_bytes(),
            did_document: did_document.clone(),
            device_id: "device-1".to_string(),
            rotation_chain: Vec::new(),
        };

        Self::encrypt_and_save_v3(&self.path, &stored_v3, passphrase)
            .context("Failed to save upgraded v3 keystore")?;

        info!("✅ Successfully migrated to v3 keystore (multi-device support enabled)");

        // Load into memory
        self.identity_bundle = Some(identity_bundle);
        self.did_document = Some(did_document);
        self.device_id = Some("device-1".to_string());
        self.rotation_chain = Vec::new();

        Ok(())
    }

    fn lock(&mut self) {
        if self.identity_bundle.is_some() {
            info!("Locking keystore");
            self.identity_bundle = None;
            self.did_document = None;
            self.device_id = None;
            self.rotation_chain.clear();
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

        // Create a v1 keystore manually (no TLS binding fields, no X25519 keys)
        let keypair = KeyPair::generate().unwrap();
        let stored_v1 = StoredKey {
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: keypair.did().as_str().to_string(),
            tls_cert_der: None,
            tls_key_der: None,
            tls_binding_sig: None,
            created_at: None,
            x25519_secret: None,
            x25519_public: None,
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

    #[test]
    fn test_v3_keystore_init_and_unlock() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize v3 keystore
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        assert!(!ks.is_locked());

        let did = ks.get_keypair().unwrap().did().clone();

        // Should have DID document
        let did_doc = ks.get_did_document().unwrap();
        assert_eq!(did_doc.id, did);
        assert_eq!(did_doc.version, 1);
        assert_eq!(did_doc.verification_method.len(), 2); // Ed25519 + X25519

        // Should have device ID
        assert_eq!(ks.get_device_id().unwrap(), "device-1");

        // Lock and reopen
        ks.lock();
        assert!(ks.is_locked());

        let mut ks2 = AgeKeyStore::open(&path).unwrap();
        assert!(ks2.is_locked());

        // Unlock with correct passphrase
        ks2.unlock(passphrase).unwrap();
        assert!(!ks2.is_locked());
        assert_eq!(ks2.get_keypair().unwrap().did(), &did);

        // DID document should persist
        let did_doc2 = ks2.get_did_document().unwrap();
        assert_eq!(did_doc2.id, did);
        assert_eq!(did_doc2.version, 1);
        assert_eq!(ks2.get_device_id().unwrap(), "device-1");
    }

    #[test]
    fn test_v21_to_v3_migration() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Create a v2.1 keystore manually (has TLS + X25519, but no DID Document)
        let keypair = KeyPair::generate().unwrap();
        let bundle = IdentityBundle::from_keypair(keypair.clone()).unwrap();

        let stored_v21 = StoredKey {
            secret_bytes: *keypair.secret_bytes(),
            public_bytes: keypair.verifying_key().to_bytes(),
            did: bundle.did().as_str().to_string(),
            tls_cert_der: Some(bundle.tls_cert().as_ref().to_vec()),
            tls_key_der: Some(bundle.tls_key_der_bytes().to_vec()),
            tls_binding_sig: Some(bundle.binding_info().tls_binding_sig.clone()),
            created_at: Some(bundle.binding_info().created_at),
            x25519_secret: Some(bundle.x25519_secret_bytes().to_vec()),
            x25519_public: Some(*bundle.x25519_public_bytes()),
        };
        AgeKeyStore::encrypt_and_save(&path, &stored_v21, passphrase).unwrap();

        // First unlock: should trigger v2.1->v3 migration
        let mut ks = AgeKeyStore::open(&path).unwrap();
        ks.unlock(passphrase).unwrap();

        // Should now have DID document
        let did_doc = ks.get_did_document().unwrap();
        assert_eq!(did_doc.id, *keypair.did());
        assert_eq!(did_doc.version, 1);
        assert_eq!(did_doc.verification_method.len(), 2); // Ed25519 + X25519

        // Clone values for later comparison
        let did_doc_id = did_doc.id.clone();
        let did_doc_version = did_doc.version;

        // Should have device ID
        assert_eq!(ks.get_device_id().unwrap(), "device-1");

        // TLS and X25519 keys should be preserved
        let bundle_after = ks.get_identity_bundle().unwrap();
        assert_eq!(
            bundle_after.tls_cert().as_ref(),
            bundle.tls_cert().as_ref(),
            "TLS certificate should be preserved during migration"
        );
        assert_eq!(
            bundle_after.x25519_public_bytes(),
            bundle.x25519_public_bytes(),
            "X25519 public key should be preserved during migration"
        );

        // Lock and unlock again - should load as v3
        ks.lock();
        ks.unlock(passphrase).unwrap();

        // DID document should still be there
        let did_doc2 = ks.get_did_document().unwrap();
        assert_eq!(did_doc2.id, did_doc_id);
        assert_eq!(did_doc2.version, did_doc_version);
    }
}
