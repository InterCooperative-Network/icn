//! Secure key storage with encryption at rest
//!
//! Supports multiple keystore formats:
//! - v1: Basic Ed25519 keypair only
//! - v2: Added TLS binding
//! - v2.1: Added X25519 encryption keys
//! - v3: Added DID Document and multi-device support
//! - v4: Added SDIS Anchor and KeyBundle with hybrid signatures

use crate::anchor::Anchor;
use crate::keybundle::KeyBundle;
#[cfg(feature = "post-quantum")]
use crate::DidKey;
use crate::{Did, DidDocument, IdentityBundle, KeyPair, RotationEvent};
use anyhow::{Context, Result};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use zeroize::Zeroize;
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
    fn get_keypair(&self) -> Result<KeyPair>;

    /// Rotate to a new keypair
    fn rotate(&mut self, new_keypair: KeyPair) -> Result<KeyRotation>;

    /// Get the storage path
    fn path(&self) -> &Path;
}

/// Key rotation record documenting a DID key change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotation {
    /// DID before rotation
    pub old_did: Did,
    /// DID after rotation
    pub new_did: Did,
    /// Unix timestamp when rotation occurred
    pub timestamp: u64,
    /// Reason for key rotation
    pub reason: RotationReason,
    /// Signature from old key proving authorization
    pub signature_old: Vec<u8>,
    /// Signature from new key proving possession
    pub signature_new: Vec<u8>,
}

/// Reason for key rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationReason {
    /// Regular scheduled rotation
    Scheduled,
    /// Key was potentially compromised
    Compromised,
    /// Upgrading to stronger key algorithm
    Upgrade,
    /// Manual user-initiated rotation
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

    // PQ keys (v5 addition - feature gated)
    #[cfg(feature = "post-quantum")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pq_secret: Option<Vec<u8>>,
    #[cfg(feature = "post-quantum")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pq_public: Option<Vec<u8>>,
}

/// Keystore v3 format: Multi-device support with DID Document
#[derive(Serialize, Deserialize)]
struct StoredKeyV3 {
    /// Format version (always 3 for this struct)
    version: u8,

    /// This device's identity keys (SENSITIVE - auto-zeroized via Zeroizing)
    secret_bytes: Zeroizing<[u8; 32]>,
    public_bytes: [u8; 32],
    did: String,

    /// TLS binding (from IdentityBundle) (SENSITIVE - auto-zeroized via Zeroizing)
    tls_cert_der: Vec<u8>,
    tls_key_der: Zeroizing<Vec<u8>>,
    tls_binding_sig: Vec<u8>,
    created_at: u64,

    /// X25519 encryption keys (SENSITIVE - auto-zeroized via Zeroizing)
    x25519_secret: Zeroizing<Vec<u8>>,
    x25519_public: [u8; 32],

    /// DID Document (public, no need to zeroize)
    did_document: DidDocument,

    /// This device's ID in the DID Document
    device_id: String,

    /// Rotation event history (public, no need to zeroize)
    rotation_chain: Vec<RotationEvent>,
}

// Note: No manual Drop needed - Zeroizing handles secure cleanup automatically

/// Keystore v4 format: SDIS support with Anchor and KeyBundles
#[derive(Serialize, Deserialize)]
struct StoredKeyV4 {
    /// Format version (always 4 for this struct)
    version: u8,

    // === Legacy fields from v3 (for backward compatibility) ===
    /// This device's Ed25519 identity keys (SENSITIVE - auto-zeroized via Zeroizing)
    secret_bytes: Zeroizing<[u8; 32]>,
    public_bytes: [u8; 32],
    did: String,

    /// TLS binding (SENSITIVE - auto-zeroized via Zeroizing)
    tls_cert_der: Vec<u8>,
    tls_key_der: Zeroizing<Vec<u8>>,
    tls_binding_sig: Vec<u8>,
    created_at: u64,

    /// X25519 encryption keys (SENSITIVE - auto-zeroized via Zeroizing)
    x25519_secret: Zeroizing<Vec<u8>>,
    x25519_public: [u8; 32],

    /// DID Document
    did_document: DidDocument,

    /// This device's ID
    device_id: String,

    /// Rotation event history
    rotation_chain: Vec<RotationEvent>,

    // === SDIS fields (v4 additions) ===
    /// SDIS Anchor (None for legacy identities)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anchor: Option<Anchor>,

    /// Stored KeyBundles (hybrid signature keys)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keybundles: Vec<StoredKeyBundleV4>,

    /// Current active KeyBundle version (0 if no SDIS keys)
    #[serde(default)]
    current_keybundle_version: u32,

    // === Post-Quantum fields (v5 additions) ===
    /// PQ signature secret key for core identity (ML-DSA) (SENSITIVE - auto-zeroized)
    #[cfg(feature = "post-quantum")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pq_secret: Option<Zeroizing<Vec<u8>>>,

    /// PQ signature public key for core identity (ML-DSA)
    #[cfg(feature = "post-quantum")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pq_public: Option<Vec<u8>>,

    /// PQ encryption secret key (ML-KEM) (SENSITIVE - auto-zeroized)
    #[cfg(feature = "post-quantum")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kem_pq_secret: Option<Zeroizing<Vec<u8>>>,

    /// PQ encryption public key (ML-KEM)
    #[cfg(feature = "post-quantum")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kem_pq_public: Option<Vec<u8>>,

    /// Whether this is a native hybrid identity
    #[cfg(feature = "post-quantum")]
    #[serde(default)]
    is_hybrid: bool,
}

/// Stored KeyBundle for v4 keystore
#[derive(Serialize, Deserialize)]
struct StoredKeyBundleV4 {
    /// KeyBundle version
    version: u32,

    /// Ed25519 signing key (from hybrid keypair) (SENSITIVE - auto-zeroized)
    classical_secret: Zeroizing<Vec<u8>>,
    classical_public: Vec<u8>,

    /// ML-DSA signing key (from hybrid keypair) (SENSITIVE - auto-zeroized)
    pq_secret: Zeroizing<Vec<u8>>,
    pq_public: Vec<u8>,

    /// X25519 encryption key for this bundle (SENSITIVE - auto-zeroized)
    bundle_x25519_secret: Zeroizing<Vec<u8>>,
    bundle_x25519_public: [u8; 32],

    /// Timestamps
    issued_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revoked_at: Option<u64>,
}

// Note: No manual Drop needed for StoredKeyV4 or StoredKeyBundleV4
// Zeroizing<T> handles secure cleanup automatically when dropped

/// Age-encrypted key storage
pub struct AgeKeyStore {
    path: PathBuf,
    identity_bundle: Option<IdentityBundle>,
    did_document: Option<DidDocument>,
    device_id: Option<String>,
    rotation_chain: Vec<RotationEvent>,

    // SDIS fields (v4)
    anchor: Option<Anchor>,
    keybundles: Vec<KeyBundle>,
    current_keybundle_version: u32,
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
            anchor: None,
            keybundles: Vec::new(),
            current_keybundle_version: 0,
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
        self.did_document.as_ref().ok_or_else(|| {
            anyhow::anyhow!("DID document not available (keystore locked or v2.1 format)")
        })
    }

    /// Get this device's ID in the DID document (fails if locked or v2.1 keystore)
    pub fn get_device_id(&self) -> Result<&str> {
        self.device_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Device ID not available"))
    }

    /// Get the rotation event chain
    pub fn get_rotation_chain(&self) -> &[RotationEvent] {
        &self.rotation_chain
    }

    // === SDIS Methods (v4) ===

    /// Get the SDIS anchor (if this identity has one)
    pub fn get_anchor(&self) -> Option<&Anchor> {
        self.anchor.as_ref()
    }

    /// Check if this keystore has SDIS support enabled
    pub fn has_sdis(&self) -> bool {
        self.anchor.is_some()
    }

    /// Get the current KeyBundle (fails if no SDIS or locked)
    pub fn get_current_keybundle(&self) -> Result<&KeyBundle> {
        if self.identity_bundle.is_none() {
            anyhow::bail!("Keystore is locked");
        }
        if self.keybundles.is_empty() {
            anyhow::bail!("No SDIS KeyBundles available");
        }
        self.keybundles
            .iter()
            .find(|kb| kb.version == self.current_keybundle_version)
            .ok_or_else(|| anyhow::anyhow!("Current KeyBundle version not found"))
    }

    /// Get all KeyBundles
    pub fn get_keybundles(&self) -> &[KeyBundle] {
        &self.keybundles
    }

    /// Get the current KeyBundle version
    pub fn get_current_keybundle_version(&self) -> u32 {
        self.current_keybundle_version
    }

    /// Initialize SDIS support for this identity
    ///
    /// Creates an anchor and initial KeyBundle. This is a one-time operation
    /// that cannot be undone. The anchor is derived from the provided VUI.
    pub fn init_sdis(&mut self, anchor: Anchor, passphrase: &[u8]) -> Result<&KeyBundle> {
        if self.identity_bundle.is_none() {
            anyhow::bail!("Keystore must be unlocked to initialize SDIS");
        }
        if self.anchor.is_some() {
            anyhow::bail!("SDIS already initialized for this identity");
        }

        // Generate initial KeyBundle (v1)
        let keybundle = KeyBundle::generate(anchor.clone(), 1)?;

        self.anchor = Some(anchor);
        self.keybundles = vec![keybundle];
        self.current_keybundle_version = 1;

        // Save to disk
        self.save_v4(passphrase)?;

        info!(
            "Initialized SDIS for identity: {}",
            self.get_keypair()?.did()
        );

        Ok(&self.keybundles[0])
    }

    /// Rotate to a new KeyBundle
    ///
    /// Creates a new KeyBundle with incremented version and sets it as current.
    /// The old KeyBundle is retained for verification of old signatures.
    pub fn rotate_keybundle(&mut self, passphrase: &[u8]) -> Result<&KeyBundle> {
        let anchor = self
            .anchor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SDIS not initialized"))?
            .clone();

        let new_version = self.current_keybundle_version + 1;
        let new_keybundle = KeyBundle::generate(anchor, new_version)?;

        self.keybundles.push(new_keybundle);
        self.current_keybundle_version = new_version;

        // Save to disk
        self.save_v4(passphrase)?;

        info!("Rotated to KeyBundle v{}", new_version);

        // SAFETY: We just pushed to keybundles above, so last() is guaranteed Some
        #[allow(clippy::expect_used)]
        Ok(self
            .keybundles
            .last()
            .expect("keybundles cannot be empty after push"))
    }

    /// Upgrade identity to post-quantum security
    ///
    /// This adds ML-DSA (signing) and ML-KEM (encryption) keys to an existing
    /// Ed25519 identity. The DID remains unchanged.
    ///
    /// # Arguments
    /// * `passphrase` - Passphrase to encrypt the upgraded keystore
    ///
    /// # Returns
    /// The upgraded DID (unchanged from original)
    #[cfg(feature = "post-quantum")]
    pub fn upgrade_to_pq(&mut self, passphrase: &[u8]) -> Result<Did> {
        // Ensure keystore is unlocked
        let identity_bundle = self
            .identity_bundle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Keystore must be unlocked to upgrade"))?;

        // Only software keys can be upgraded to PQ
        if identity_bundle.did_key().is_hardware_backed() {
            anyhow::bail!("Cannot upgrade hardware-backed keys to PQ in software");
        }

        // Reconstruct KeyPair to check PQ status
        let keypair = identity_bundle.keypair()?;

        // Check if already has PQ keys
        if keypair.has_pq_keys() {
            info!("Identity already has post-quantum keys");
            return Ok(identity_bundle.did().clone());
        }

        let did = identity_bundle.did().clone();
        info!("Upgrading identity {} to post-quantum security", did);

        // Generate ML-DSA keypair for signatures
        let pq_keypair = icn_crypto_pq::MlDsaKeypair::generate()
            .map_err(|e| anyhow::anyhow!("Failed to generate ML-DSA keypair: {e}"))?;

        // Create upgraded KeyPair with PQ signing keys
        let (secret_bytes, public_bytes) = keypair.export_for_upgrade();
        let upgraded_keypair = KeyPair::from_bytes_with_pq(
            &secret_bytes,
            &public_bytes,
            pq_keypair.secret_key_bytes(),
            pq_keypair.public_key().as_bytes(),
        )?;

        // Convert upgraded KeyPair to DidKey.
        // PQ signing keys are stored alongside the bundle for persistence.
        let upgraded_did_key = DidKey::Software {
            secret_bytes: Zeroizing::new(upgraded_keypair.to_signing_key_bytes()),
            verifying_key: *upgraded_keypair.verifying_key(),
            did: upgraded_keypair.did().clone(),
        };

        // Generate ML-KEM keypair for hybrid encryption
        let kem_keypair = icn_crypto_pq::MlKemKeypair::generate()
            .map_err(|e| anyhow::anyhow!("Failed to generate ML-KEM keypair: {e}"))?;

        // Create new IdentityBundle with upgraded keypair and KEM keys
        let upgraded_bundle = IdentityBundle::from_stored_with_pq(
            upgraded_did_key,
            identity_bundle.tls_cert().as_ref().to_vec(),
            Zeroizing::new(identity_bundle.tls_key_der_bytes().to_vec()),
            identity_bundle.binding_info().tls_binding_sig.clone(),
            identity_bundle.binding_info().created_at,
            Zeroizing::new(identity_bundle.x25519_secret_bytes().to_vec()),
            *identity_bundle.x25519_public_bytes(),
            Some(Zeroizing::new(pq_keypair.secret_key_bytes().to_vec())),
            Some(pq_keypair.public_key().as_bytes().to_vec()),
            Some(Zeroizing::new(kem_keypair.secret_key_bytes().to_vec())),
            Some(kem_keypair.public_key().as_bytes().to_vec()),
        )?;

        // Update in-memory state
        self.identity_bundle = Some(upgraded_bundle);

        // Save to disk
        self.save_v4(passphrase)?;

        info!(
            "Successfully upgraded identity {} to post-quantum security",
            did
        );

        Ok(did)
    }

    /// Save keystore in v4 format
    fn save_v4(&self, passphrase: &[u8]) -> Result<()> {
        let identity_bundle = self
            .identity_bundle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Keystore is locked"))?;
        let did_document = self
            .did_document
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DID document not available"))?;
        let device_id = self
            .device_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Device ID not available"))?;

        // Convert KeyBundles to stored format
        let stored_keybundles: Vec<StoredKeyBundleV4> = self
            .keybundles
            .iter()
            .map(|kb| StoredKeyBundleV4 {
                version: kb.version,
                classical_secret: Zeroizing::new(kb.classical_secret_bytes().to_vec()),
                classical_public: kb.classical_public_bytes(),
                pq_secret: Zeroizing::new(kb.pq_secret_bytes().to_vec()),
                pq_public: kb.pq_public_bytes(),
                bundle_x25519_secret: Zeroizing::new(kb.x25519_secret_bytes().to_vec()),
                bundle_x25519_public: kb.x25519_public(),
                issued_at: kb.issued_at,
                expires_at: kb.expires_at,
                revoked_at: None,
            })
            .collect();

        // Extract secret and public bytes from DidKey (only works for software keys)
        let did_key = identity_bundle.did_key();
        let (secret_bytes, public_bytes) = match did_key {
            crate::DidKey::Software {
                secret_bytes,
                verifying_key,
                ..
            } => (Zeroizing::new(**secret_bytes), verifying_key.to_bytes()),
            crate::DidKey::Hardware { backend_type, .. } => {
                anyhow::bail!(
                    "Cannot save hardware-backed keystore to v4 format (backend: {backend_type}). \
                     Hardware keys should be managed by their backend."
                )
            }
        };

        let stored = StoredKeyV4 {
            version: 4,
            secret_bytes,
            public_bytes,
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: Zeroizing::new(identity_bundle.tls_key_der_bytes().to_vec()),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: Zeroizing::new(identity_bundle.x25519_secret_bytes().to_vec()),
            x25519_public: *identity_bundle.x25519_public_bytes(),
            did_document: did_document.clone(),
            device_id: device_id.clone(),
            rotation_chain: self.rotation_chain.clone(),
            anchor: self.anchor.clone(),
            keybundles: stored_keybundles,
            current_keybundle_version: self.current_keybundle_version,
            #[cfg(feature = "post-quantum")]
            pq_secret: identity_bundle
                .pq_signing_secret_bytes()
                .map(|s| Zeroizing::new(s.to_vec())),
            #[cfg(feature = "post-quantum")]
            pq_public: identity_bundle
                .pq_signing_public_bytes()
                .map(|p| p.to_vec()),
            #[cfg(feature = "post-quantum")]
            kem_pq_secret: identity_bundle
                .kem_pq_secret_bytes()
                .map(|s| Zeroizing::new(s.to_vec())),
            #[cfg(feature = "post-quantum")]
            kem_pq_public: identity_bundle.kem_pq_public_bytes().map(|p| p.to_vec()),
            #[cfg(feature = "post-quantum")]
            is_hybrid: identity_bundle.has_pq_signing_keys(),
        };

        Self::encrypt_and_save_v4(&self.path, &stored, passphrase)
    }

    /// Encrypt and save v4 key material
    /// Mode every filesystem keystore this crate writes must end up with.
    ///
    /// Matches the sibling TPM-sealed backend in this crate, which already sets
    /// `0o600` at creation.
    #[cfg(unix)]
    const KEYSTORE_FILE_MODE: u32 = 0o600;

    /// Write encrypted keystore bytes to `path` with owner-only permissions.
    ///
    /// The one place this crate turns keystore ciphertext into a file (#2748).
    /// Every `encrypt_and_save*` variant goes through here, so the rule is a
    /// property of the owner rather than something three call sites each have to
    /// remember — and a fourth keystore format would inherit it by construction.
    ///
    /// # Why the mode is set at creation
    ///
    /// `std::fs::write` is `File::create` plus a write, and `File::create`
    /// requests `0o666`, which the kernel masks with the process umask. The mode
    /// was therefore an inherited ambient property, not a decision this code
    /// made: under the common `umask 0002` a new keystore landed at `0664`.
    /// Passing `mode` replaces that requested `0o666`, so the umask has nothing
    /// permissive left to mask off and the file is never *momentarily* readable
    /// the way a write-then-`chmod` sequence would leave it.
    ///
    /// `mode` is still only a *request* — the kernel applies `mode & !umask` —
    /// so an unusual umask can clear owner bits as well, and `umask 0777` would
    /// otherwise leave a mode-000 keystore the node cannot reopen. A file this
    /// call creates is therefore normalised to exactly the target mode while it
    /// is still empty. The two cases are told apart with `create_new`, which is
    /// race-free, rather than by a prior `exists()` check.
    ///
    /// # Existing files, and why nothing is truncated on open
    ///
    /// `OpenOptionsExt::mode` applies only when the call actually creates the
    /// file; on an existing one it is ignored. Since the callers rewrite a
    /// keystore's contents as part of normal operation, a pre-existing file is
    /// tightened here before any new bytes are written, so fresh key material
    /// never exists while group or other bits are set.
    ///
    /// The open deliberately does **not** use `truncate(true)`. Truncating at
    /// open would destroy the existing ciphertext before permissions are known
    /// to be safe: a subsequent `set_permissions` failure would then return an
    /// error having already emptied the operator's only keystore. The order is
    /// therefore open (no truncate) -> harden -> `set_len(0)` -> write, so every
    /// failure before the truncation leaves the previous keystore byte-intact.
    ///
    /// `set_len(0)` is load-bearing precisely because the open no longer
    /// truncates: without it a shorter ciphertext would leave a tail of the
    /// previous one behind.
    ///
    /// For a pre-existing file only group and other bits are cleared
    /// (`existing & 0o7700`); owner and special bits are preserved, so an
    /// operator who hardened a keystore beyond `0600` keeps that. This is
    /// deliberately not a migration — it touches only files this code was
    /// already rewriting. A permissive keystore that is never rewritten is left
    /// exactly as it is.
    fn write_keystore_file(path: &Path, contents: &[u8]) -> Result<()> {
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            // `create_new` distinguishes the two cases without a TOCTOU race,
            // and the distinction matters: a file this call creates must be
            // normalised to exactly the target mode, while one that already
            // existed must keep an owner mode the operator may have hardened.
            //
            // No `truncate(true)` on either path: nothing is destroyed until the
            // permissions are known to be safe.
            let (mut file, created) = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(Self::KEYSTORE_FILE_MODE)
                .open(path)
            {
                Ok(file) => (file, true),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open(path)
                        .context("Failed to open keystore file")?,
                    false,
                ),
                Err(e) => return Err(e).context("Failed to create keystore file"),
            };

            let existing = file
                .metadata()
                .context("Failed to inspect keystore file")?
                .permissions()
                .mode()
                & 0o7777;

            if created {
                // `mode` is a *request*: the kernel applies `mode & !umask`, so a
                // hostile or unusual umask can clear owner bits too and leave a
                // keystore its own node cannot reopen (a `umask 0777` yields
                // mode 000). Normalise unconditionally — the file is empty at
                // this point, so there is no window in which contents exist at
                // the wrong mode.
                if existing != Self::KEYSTORE_FILE_MODE {
                    file.set_permissions(std::fs::Permissions::from_mode(Self::KEYSTORE_FILE_MODE))
                        .context("Failed to set keystore file permissions")?;
                }
            } else if existing & 0o077 != 0 {
                // Pre-existing file: clear only group and other bits. Owner and
                // special bits are preserved, so a keystore hardened beyond the
                // target keeps that.
                file.set_permissions(std::fs::Permissions::from_mode(existing & 0o7700))
                    .context("Failed to restrict keystore file permissions")?;
            }

            // Past this point the previous keystore is gone, so it is the last
            // step before the new bytes land.
            file.set_len(0)
                .context("Failed to truncate keystore file before rewrite")?;
            file.write_all(contents)
                .context("Failed to write keystore file")?;
            file.sync_all()
                .context("Failed to flush keystore file to disk")?;

            Ok(())
        }

        #[cfg(not(unix))]
        {
            // Non-Unix targets have no umask and no portable mode to request, so
            // there is nothing to tighten here; access control is the platform
            // ACL's job. Behaviour is unchanged from before #2748.
            std::fs::write(path, contents).context("Failed to write keystore file")
        }
    }

    fn encrypt_and_save_v4(path: &Path, stored: &StoredKeyV4, passphrase: &[u8]) -> Result<()> {
        let json = Zeroizing::new(serde_json::to_vec(stored)?);

        let encryptor = age::Encryptor::with_user_passphrase(SecretString::new(
            String::from_utf8(passphrase.to_vec())
                .context("Passphrase must be valid UTF-8")?
                .into_boxed_str(),
        ));

        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .context("Failed to create age writer")?;

        std::io::copy(&mut json.as_slice(), &mut writer)
            .context("Failed to encrypt key material")?;

        writer
            .finish()
            .map(|_| ())
            .context("Failed to finalize encryption")?;

        Self::write_keystore_file(path, &encrypted)?;

        Ok(())
    }

    /// Decrypt and load v4 key material
    fn decrypt_and_load_v4(path: &Path, passphrase: &[u8]) -> Result<StoredKeyV4> {
        // Read encrypted file
        let encrypted = std::fs::read(path).context("Failed to read keystore file")?;

        // Create age decryptor
        let decryptor = age::Decryptor::new(encrypted.as_slice())?;
        if !decryptor.is_scrypt() {
            anyhow::bail!("Unsupported age encryption type");
        }

        // Decrypt with scrypt identity
        let passphrase_str =
            String::from_utf8(passphrase.to_vec()).context("Passphrase must be valid UTF-8")?;
        let identity =
            age::scrypt::Identity::new(SecretString::new(passphrase_str.into_boxed_str()));

        let mut decrypted = Vec::new();
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .context("Failed to decrypt (wrong passphrase?)")?;

        std::io::copy(&mut reader, &mut decrypted).context("Failed to read decrypted data")?;

        // Deserialize
        let stored: StoredKeyV4 =
            serde_json::from_slice(&decrypted).context("Failed to parse v4 key material")?;

        Ok(stored)
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

        // Extract secret and public bytes from DidKey (only works for software keys)
        let did_key = identity_bundle.did_key();
        let (secret_bytes, public_bytes) = match did_key {
            crate::DidKey::Software {
                secret_bytes,
                verifying_key,
                ..
            } => (Zeroizing::new(**secret_bytes), verifying_key.to_bytes()),
            crate::DidKey::Hardware { backend_type, .. } => {
                anyhow::bail!(
                    "Cannot save hardware-backed keystore to v3 format (backend: {backend_type}). \
                     Hardware keys should be managed by their backend."
                )
            }
        };

        // Save to disk
        let stored_v3 = StoredKeyV3 {
            version: 3,
            secret_bytes,
            public_bytes,
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: Zeroizing::new(identity_bundle.tls_key_der_bytes().to_vec()),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: Zeroizing::new(identity_bundle.x25519_secret_bytes().to_vec()),
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
            anyhow::bail!("Keystore already exists at {path:?}");
        }

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create keystore directory")?;
        }

        // Generate new identity bundle with DID-TLS binding
        let identity_bundle = IdentityBundle::generate()?;
        info!(
            "Generated new identity with DID-TLS binding: {}",
            identity_bundle.did()
        );

        // Create DID Document v2
        let did_key = identity_bundle.did_key();
        let verifying_key = match did_key {
            crate::DidKey::Software { verifying_key, .. } => *verifying_key,
            crate::DidKey::Hardware { .. } => {
                anyhow::bail!("Cannot initialize keystore with hardware-backed key")
            }
        };

        let did_document = DidDocument::new(
            identity_bundle.did().clone(),
            &verifying_key,
            identity_bundle.x25519_public_bytes(),
        );

        // Extract secret and public bytes from DidKey
        let (secret_bytes, public_bytes) = match did_key {
            crate::DidKey::Software {
                secret_bytes,
                verifying_key,
                ..
            } => (Zeroizing::new(**secret_bytes), verifying_key.to_bytes()),
            crate::DidKey::Hardware { .. } => {
                anyhow::bail!("Cannot initialize keystore with hardware-backed key")
            }
        };

        // Save to disk (v3 format for multi-device)
        let stored_v3 = StoredKeyV3 {
            version: 3,
            secret_bytes,
            public_bytes,
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: Zeroizing::new(identity_bundle.tls_key_der_bytes().to_vec()),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: Zeroizing::new(identity_bundle.x25519_secret_bytes().to_vec()),
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
            anchor: None,
            keybundles: Vec::new(),
            current_keybundle_version: 0,
        })
    }

    /// Open an existing keystore (locked)
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        if !path.exists() {
            anyhow::bail!("Keystore not found at {path:?}");
        }

        Ok(Self {
            path,
            identity_bundle: None,
            did_document: None,
            device_id: None,
            rotation_chain: Vec::new(),
            anchor: None,
            keybundles: Vec::new(),
            current_keybundle_version: 0,
        })
    }

    /// Encrypt and save key material (legacy, used in migration tests)
    #[allow(dead_code)]
    fn encrypt_and_save(path: &Path, stored: &StoredKey, passphrase: &[u8]) -> Result<()> {
        // Serialize key material - use Zeroizing to ensure plaintext is cleared
        let json = Zeroizing::new(serde_json::to_vec(stored)?);

        // Create age encryptor with passphrase
        let encryptor = age::Encryptor::with_user_passphrase(SecretString::new(
            String::from_utf8(passphrase.to_vec())
                .context("Passphrase must be valid UTF-8")?
                .into_boxed_str(),
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
            .map(|_| ())
            .context("Failed to finalize encryption")?;

        // Write to file
        Self::write_keystore_file(path, &encrypted)?;

        // json is automatically zeroized when dropped here
        Ok(())
    }

    /// Decrypt and load key material
    fn decrypt_and_load(path: &Path, passphrase: &[u8]) -> Result<StoredKey> {
        // Read encrypted file
        let encrypted = std::fs::read(path).context("Failed to read keystore file")?;

        // Create age decryptor
        let decryptor = age::Decryptor::new(encrypted.as_slice())?;
        if !decryptor.is_scrypt() {
            anyhow::bail!("Unsupported age encryption type");
        }

        // Decrypt with scrypt identity
        let passphrase_str =
            String::from_utf8(passphrase.to_vec()).context("Passphrase must be valid UTF-8")?;
        let identity =
            age::scrypt::Identity::new(SecretString::new(passphrase_str.into_boxed_str()));

        let mut decrypted = Vec::new();
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
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
        let encryptor = age::Encryptor::with_user_passphrase(SecretString::new(
            String::from_utf8(passphrase.to_vec())
                .context("Passphrase must be valid UTF-8")?
                .into_boxed_str(),
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
            .map(|_| ())
            .context("Failed to finalize encryption")?;

        // Write to file
        Self::write_keystore_file(path, &encrypted)?;

        // json is automatically zeroized when dropped here
        Ok(())
    }

    /// Decrypt and load v3 key material
    fn decrypt_and_load_v3(path: &Path, passphrase: &[u8]) -> Result<StoredKeyV3> {
        // Read encrypted file
        let encrypted = std::fs::read(path).context("Failed to read keystore file")?;

        // Create age decryptor
        let decryptor = age::Decryptor::new(encrypted.as_slice())?;
        if !decryptor.is_scrypt() {
            anyhow::bail!("Unsupported age encryption type");
        }

        // Decrypt with scrypt identity
        let passphrase_str =
            String::from_utf8(passphrase.to_vec()).context("Passphrase must be valid UTF-8")?;
        let identity =
            age::scrypt::Identity::new(SecretString::new(passphrase_str.into_boxed_str()));

        let mut decrypted = Vec::new();
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
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

        // Try loading as v4 first (newest format with SDIS support)
        if let Ok(stored_v4) = Self::decrypt_and_load_v4(&self.path, passphrase) {
            info!(
                "Unlocked v4 keystore (SDIS support) for DID: {}",
                stored_v4.did
            );

            // Construct DidKey::Software from stored bytes
            let did_key = crate::DidKey::from_software_bytes(
                *stored_v4.secret_bytes,
                stored_v4.public_bytes,
            )?;

            // Reconstruct IdentityBundle with optional KEM keys
            #[cfg(feature = "post-quantum")]
            let identity_bundle = IdentityBundle::from_stored_with_pq(
                did_key,
                stored_v4.tls_cert_der.clone(),
                stored_v4.tls_key_der.clone(),
                stored_v4.tls_binding_sig.clone(),
                stored_v4.created_at,
                stored_v4.x25519_secret.clone(),
                stored_v4.x25519_public,
                stored_v4.pq_secret.clone(),
                stored_v4.pq_public.clone(),
                stored_v4.kem_pq_secret.clone(),
                stored_v4.kem_pq_public.clone(),
            )?;

            #[cfg(not(feature = "post-quantum"))]
            let identity_bundle = IdentityBundle::from_stored(
                did_key,
                stored_v4.tls_cert_der.clone(),
                stored_v4.tls_key_der.clone(),
                stored_v4.tls_binding_sig.clone(),
                stored_v4.created_at,
                stored_v4.x25519_secret.clone(),
                stored_v4.x25519_public,
            )?;

            // Reconstruct KeyBundles if present
            let mut keybundles = Vec::new();
            for stored_kb in &stored_v4.keybundles {
                if let Some(ref anchor) = stored_v4.anchor {
                    let classical_secret: [u8; 32] = stored_kb
                        .classical_secret
                        .as_slice()
                        .try_into()
                        .context("Invalid classical secret key length")?;
                    let classical_public: [u8; 32] = stored_kb
                        .classical_public
                        .as_slice()
                        .try_into()
                        .context("Invalid classical public key length")?;
                    let x25519_secret: [u8; 32] = stored_kb
                        .bundle_x25519_secret
                        .as_slice()
                        .try_into()
                        .context("Invalid X25519 secret key length")?;

                    let kb = KeyBundle::from_stored(
                        anchor.clone(),
                        stored_kb.version,
                        &classical_secret,
                        &classical_public,
                        &stored_kb.pq_secret,
                        &stored_kb.pq_public,
                        x25519_secret,
                        stored_kb.bundle_x25519_public,
                        stored_kb.issued_at,
                        stored_kb.expires_at,
                    )?;
                    keybundles.push(kb);
                }
            }

            // Load all data
            self.identity_bundle = Some(identity_bundle);
            self.did_document = Some(stored_v4.did_document.clone());
            self.device_id = Some(stored_v4.device_id.clone());
            self.rotation_chain = stored_v4.rotation_chain.clone();
            self.anchor = stored_v4.anchor.clone();
            self.keybundles = keybundles;
            self.current_keybundle_version = stored_v4.current_keybundle_version;

            return Ok(());
        }

        // Try loading as v3
        if let Ok(stored_v3) = Self::decrypt_and_load_v3(&self.path, passphrase) {
            // V3 keystore: has multi-device support
            info!(
                "Unlocked v3 keystore (multi-device) for DID: {}",
                stored_v3.did
            );

            // Construct DidKey::Software from stored bytes
            let did_key = crate::DidKey::from_software_bytes(
                *stored_v3.secret_bytes,
                stored_v3.public_bytes,
            )?;

            // Reconstruct IdentityBundle
            let identity_bundle = IdentityBundle::from_stored(
                did_key,
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

        // Construct DidKey::Software from stored bytes
        let did_key = crate::DidKey::from_software_bytes(stored.secret_bytes, stored.public_bytes)?;

        // Check if we have TLS binding info (v2+ keystore)
        let identity_bundle = if let (
            Some(tls_cert_der),
            Some(tls_key_der),
            Some(tls_binding_sig),
            Some(created_at),
        ) = (
            stored.tls_cert_der.clone(),
            stored.tls_key_der.clone(),
            stored.tls_binding_sig.clone(),
            stored.created_at,
        ) {
            // Wrap TLS key in Zeroizing for secure handling
            let tls_key_der = Zeroizing::new(tls_key_der);

            // Check if we have X25519 keys (v2.1+)
            let (x25519_secret, x25519_public) = if let (Some(secret), Some(public)) =
                (stored.x25519_secret.clone(), stored.x25519_public)
            {
                // V2.1+ keystore: has X25519 keys - wrap in Zeroizing
                info!("Unlocked v2.1 keystore, migrating to v3 (multi-device)");
                (Zeroizing::new(secret), public)
            } else {
                // V2.0 keystore: has TLS but no X25519, generate new X25519 keys
                info!("Unlocked v2.0 keystore, migrating to v3 with X25519 keys");
                warn!("⚠️  Generating X25519 encryption keys");

                // Generate X25519 keys
                // rand_core 0.6 OsRng: x25519-dalek 2.x pins rand_core 0.6
                use rand_core::OsRng;
                use x25519_dalek::{PublicKey, StaticSecret};

                let secret_key = StaticSecret::random_from_rng(OsRng);
                let public_key = PublicKey::from(&secret_key);
                let secret_bytes = Zeroizing::new(secret_key.to_bytes().to_vec());
                let public_bytes = public_key.to_bytes();

                (secret_bytes, public_bytes)
            };

            // Reconstruct IdentityBundle using the stored data
            IdentityBundle::from_stored(
                did_key.clone(),
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

            // Generate new IdentityBundle from the did_key (includes X25519 keys)
            IdentityBundle::from_did_key(did_key.clone())?
        };

        // Create DID Document v2 for this identity
        let did_document = DidDocument::new(
            identity_bundle.did().clone(),
            identity_bundle.did_key().verifying_key(),
            identity_bundle.x25519_public_bytes(),
        );

        // Save as v3 keystore
        let stored_v3 = StoredKeyV3 {
            version: 3,
            secret_bytes: Zeroizing::new(*identity_bundle.keypair()?.secret_bytes()),
            public_bytes: identity_bundle.keypair()?.verifying_key().to_bytes(),
            did: identity_bundle.did().as_str().to_string(),
            tls_cert_der: identity_bundle.tls_cert().as_ref().to_vec(),
            tls_key_der: Zeroizing::new(identity_bundle.tls_key_der_bytes().to_vec()),
            tls_binding_sig: identity_bundle.binding_info().tls_binding_sig.clone(),
            created_at: identity_bundle.binding_info().created_at,
            x25519_secret: Zeroizing::new(identity_bundle.x25519_secret_bytes().to_vec()),
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
            // Clear SDIS fields
            self.anchor = None;
            self.keybundles.clear();
            self.current_keybundle_version = 0;
        }
    }

    fn is_locked(&self) -> bool {
        self.identity_bundle.is_none()
    }

    fn get_keypair(&self) -> Result<KeyPair> {
        self.get_identity_bundle()?.keypair()
    }

    fn rotate(&mut self, new_keypair: KeyPair) -> Result<KeyRotation> {
        let old_keypair = self.get_keypair()?;

        // Create rotation record
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let message = format!(
            "key-rotation:{}:{}:{}",
            old_keypair.did(),
            new_keypair.did(),
            timestamp
        );

        let rotation = KeyRotation {
            old_did: old_keypair.did().clone(),
            new_did: new_keypair.did().clone(),
            timestamp,
            reason: RotationReason::Manual,
            signature_old: old_keypair.sign(message.as_bytes()).to_vec(),
            signature_new: new_keypair.sign(message.as_bytes()).to_vec(),
        };

        info!("Rotating key: {} -> {}", rotation.old_did, rotation.new_did);

        // Create new identity bundle with fresh TLS binding
        let new_bundle = IdentityBundle::from_keypair(new_keypair.clone())?;

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

    /// Keystore files must never be created at the process umask (#2748).
    ///
    /// `AgeKeyStore` wrote every keystore with a plain `std::fs::write`, which
    /// creates at `0o666 & ~umask` — `0664` under the common `umask 0002`. The
    /// contents are age-encrypted, so this is not plaintext key exposure; it
    /// hands any local user the ciphertext and the scrypt salt, which turns a
    /// weak or reused passphrase into an offline target instead of an online
    /// one.
    ///
    /// These tests assert the mode is *exactly* `0600` rather than merely "not
    /// group-readable", so they fail on the unfixed crate under any umask that
    /// leaves a group or other bit set (`0002` and `0022` both do).
    #[cfg(unix)]
    mod owner_only_keystore_files {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn mode_of(path: &Path) -> u32 {
            std::fs::metadata(path)
                .expect("keystore file should exist")
                .permissions()
                .mode()
                & 0o777
        }

        const PASSPHRASE: &[u8] = b"2748-owner-only-fixture-passphrase";

        /// A v1-shaped record, the same construction the migration test uses.
        fn stored_v1_fixture() -> StoredKey {
            let keypair = KeyPair::generate().unwrap();
            StoredKey {
                secret_bytes: *keypair.secret_bytes(),
                public_bytes: keypair.verifying_key().to_bytes(),
                did: keypair.did().as_str().to_string(),
                tls_cert_der: None,
                tls_key_der: None,
                tls_binding_sig: None,
                created_at: None,
                x25519_secret: None,
                x25519_public: None,
                #[cfg(feature = "post-quantum")]
                pq_secret: None,
                #[cfg(feature = "post-quantum")]
                pq_public: None,
            }
        }

        /// `AgeKeyStore::init` — the path `icnctl id init` and `init-coop` take.
        /// Reaches `encrypt_and_save_v3`.
        #[test]
        fn a_new_keystore_is_created_owner_only() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("identity.age");

            AgeKeyStore::init(&path, PASSPHRASE).unwrap();

            assert_eq!(
                mode_of(&path),
                0o600,
                "a freshly created keystore must be owner-only, not umask-derived"
            );
        }

        /// Reaches `encrypt_and_save` (the v1/v2 writer).
        #[test]
        fn the_legacy_save_path_creates_an_owner_only_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("legacy.age");

            AgeKeyStore::encrypt_and_save(&path, &stored_v1_fixture(), PASSPHRASE).unwrap();

            assert_eq!(mode_of(&path), 0o600);
        }

        /// Reaches `encrypt_and_save_v4` through the real API, so the third
        /// writer is covered by behaviour rather than by inspection.
        #[test]
        fn the_v4_save_path_writes_an_owner_only_file() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("identity.age");

            let mut ks = AgeKeyStore::init(&path, PASSPHRASE).unwrap();
            // `init_sdis` is the first thing that rewrites the keystore in v4
            // format; `rotate_keybundle` then rewrites it again.
            ks.init_sdis(Anchor::genesis("2748-mode-fixture"), PASSPHRASE)
                .unwrap();
            ks.rotate_keybundle(PASSPHRASE).unwrap();

            assert_eq!(
                mode_of(&path),
                0o600,
                "the v4 writer must observe the same rule as the others"
            );
        }

        /// An overwrite of an already-permissive keystore tightens it.
        ///
        /// This is not a fleet migration: it applies only where the code was
        /// already rewriting that file's contents as part of normal operation.
        /// Leaving it alone would mean freshly written key material sits at
        /// `0644` — the very thing #2748 is about.
        #[test]
        fn overwriting_tightens_an_already_permissive_keystore() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("permissive.age");
            std::fs::write(&path, b"stale contents").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(mode_of(&path), 0o644, "fixture precondition");

            AgeKeyStore::encrypt_and_save(&path, &stored_v1_fixture(), PASSPHRASE).unwrap();

            assert_eq!(
                mode_of(&path),
                0o600,
                "rewriting a keystore must not leave the new contents group-readable"
            );
        }

        /// An owner that is *stricter* than `0600` is left alone.
        ///
        /// The rule clears group and other bits; it never widens owner bits, so
        /// an operator who deliberately hardened a keystore keeps their choice.
        #[test]
        fn overwriting_does_not_weaken_a_stricter_owner_mode() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("strict.age");
            std::fs::write(&path, b"stale contents").unwrap();
            // 0o200 is write-only for the owner: stricter on read than 0o600,
            // and still writable, so the save path can actually run.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();

            AgeKeyStore::encrypt_and_save(&path, &stored_v1_fixture(), PASSPHRASE).unwrap();

            assert_eq!(
                mode_of(&path),
                0o200,
                "a stricter owner mode must be preserved, not reset to 0600"
            );
        }

        /// The write is in place: no temporary or sibling file ever holds
        /// keystore bytes at a looser mode.
        #[test]
        fn writing_a_keystore_creates_no_sibling_files() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("identity.age");

            AgeKeyStore::init(&path, PASSPHRASE).unwrap();

            let entries: Vec<String> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                entries,
                vec!["identity.age".to_string()],
                "the keystore directory must hold only the keystore itself"
            );
        }

        /// Removing `truncate(true)` from the open means `set_len(0)` is the only
        /// thing clearing the previous contents.
        ///
        /// If it were ever dropped, a ciphertext shorter than its predecessor
        /// would leave a tail of the old one behind — a silently corrupt
        /// keystore. This fails against exactly that mistake.
        #[test]
        fn rewriting_over_a_longer_file_leaves_no_trailing_bytes() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("stale.age");

            let junk = vec![b'X'; 64 * 1024];
            std::fs::write(&path, &junk).unwrap();

            AgeKeyStore::encrypt_and_save(&path, &stored_v1_fixture(), PASSPHRASE).unwrap();

            let written = std::fs::metadata(&path).unwrap().len();
            assert!(
                written < junk.len() as u64,
                "the previous, longer contents were not truncated: {written} bytes remain"
            );

            // The decisive check: it must still decrypt. A stale tail would make
            // this fail even if the length looked plausible.
            let mut ks = AgeKeyStore::open(&path).unwrap();
            ks.unlock(PASSPHRASE)
                .expect("a rewritten keystore must still decrypt");
        }

        /// A write that cannot even be opened must leave the previous keystore
        /// intact.
        ///
        /// **Scope of this claim, stated precisely: this test does _not_ prove
        /// that permission hardening precedes destructive truncation.** A `0400`
        /// file fails the write-open before any truncation could occur, so this
        /// passes against a `truncate(true)` implementation too. The only failure
        /// that would discriminate the ordering is `set_permissions` itself
        /// failing on an already-open file, and forcing `fchmod` to fail needs
        /// privileges or a filesystem fixture a unit test should not assume —
        /// and this repository has no existing seam for it. That ordering is
        /// established by reading `write_keystore_file`, where `set_len(0)` sits
        /// after the hardening, not by this test.
        ///
        /// What it does carry: a generic non-destructive-failure regression, so
        /// a future refactor cannot make a refused write cost the operator their
        /// keystore.
        #[test]
        fn a_failed_write_does_not_destroy_the_existing_keystore() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("identity.age");

            AgeKeyStore::init(&path, PASSPHRASE).unwrap();
            let before = std::fs::read(&path).unwrap();
            assert!(!before.is_empty(), "fixture precondition");

            // Read-only for the owner: the save path cannot open it for writing.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();

            // Establish the precondition instead of assuming it. Root and some
            // container runners bypass DAC entirely, and there the write-open
            // simply succeeds — there is no refused write to regress against, so
            // the test has nothing to assert rather than something to fail on.
            if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
                return;
            }

            let result = AgeKeyStore::encrypt_and_save(&path, &stored_v1_fixture(), PASSPHRASE);
            assert!(result.is_err(), "the write should have been refused");

            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "a refused write truncated or altered the existing keystore"
            );
            assert_eq!(mode_of(&path), 0o400, "the mode must be untouched too");
        }

        /// Tightening permissions must not have broken encryption or reload.
        #[test]
        fn the_keystore_still_roundtrips_after_the_mode_change() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("identity.age");

            let created = AgeKeyStore::init(&path, PASSPHRASE).unwrap();
            let expected_did = created.get_keypair().unwrap().did().clone();

            let mut reopened = AgeKeyStore::open(&path).unwrap();
            reopened.unlock(PASSPHRASE).unwrap();

            assert_eq!(reopened.get_keypair().unwrap().did(), &expected_did);
            assert_eq!(mode_of(&path), 0o600);
        }
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
            #[cfg(feature = "post-quantum")]
            pq_secret: None,
            #[cfg(feature = "post-quantum")]
            pq_public: None,
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
        assert_eq!(
            cert1_der, cert2_der,
            "TLS certificate should persist across unlocks"
        );
        assert_eq!(
            binding_sig1, binding_sig2,
            "TLS binding signature should persist across unlocks"
        );

        // Open in a new keystore instance to verify disk persistence
        let mut ks3 = AgeKeyStore::open(&path).unwrap();
        ks3.unlock(passphrase).unwrap();
        let bundle3 = ks3.get_identity_bundle().unwrap();
        let cert3_der = bundle3.tls_cert().as_ref().to_vec();

        assert_eq!(
            cert1_der, cert3_der,
            "TLS certificate should persist to disk"
        );
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
            #[cfg(feature = "post-quantum")]
            pq_secret: None,
            #[cfg(feature = "post-quantum")]
            pq_public: None,
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

    // === SDIS / v4 Keystore Tests ===

    #[test]
    fn test_sdis_init() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();

        // Should not have SDIS initially
        assert!(!ks.has_sdis());
        assert!(ks.get_anchor().is_none());
        assert!(ks.get_current_keybundle().is_err());

        // Initialize SDIS
        let anchor = Anchor::genesis("test");
        let anchor_id = anchor.id;
        ks.init_sdis(anchor, passphrase).unwrap();

        // Should now have SDIS
        assert!(ks.has_sdis());
        assert!(ks.get_anchor().is_some());
        assert_eq!(ks.get_anchor().unwrap().id, anchor_id);

        // Should have a KeyBundle
        let kb = ks.get_current_keybundle().unwrap();
        assert_eq!(kb.version, 1);
        assert_eq!(ks.get_current_keybundle_version(), 1);

        // KeyBundle should be able to sign
        let message = b"test message";
        let sig = kb.sign(message);
        let pub_bundle = kb.public_bundle();
        assert!(pub_bundle.verify(message, &sig));
    }

    #[test]
    fn test_sdis_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore and SDIS
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        let anchor = Anchor::genesis("test");
        let anchor_id = anchor.id;
        ks.init_sdis(anchor, passphrase).unwrap();

        // Get KeyBundle info before locking
        let kb_version = ks.get_current_keybundle_version();
        let kb_x25519_public = ks.get_current_keybundle().unwrap().x25519_public();

        // Lock and reopen
        ks.lock();

        let mut ks2 = AgeKeyStore::open(&path).unwrap();
        ks2.unlock(passphrase).unwrap();

        // SDIS should be restored
        assert!(ks2.has_sdis());
        assert_eq!(ks2.get_anchor().unwrap().id, anchor_id);
        assert_eq!(ks2.get_current_keybundle_version(), kb_version);

        // KeyBundle should be restored with same keys
        let kb2 = ks2.get_current_keybundle().unwrap();
        assert_eq!(kb2.version, kb_version);
        assert_eq!(kb2.x25519_public(), kb_x25519_public);
    }

    #[test]
    fn test_sdis_keybundle_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore and SDIS
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        let anchor = Anchor::genesis("test");
        ks.init_sdis(anchor, passphrase).unwrap();

        // Get v1 KeyBundle
        let kb_v1_x25519 = ks.get_current_keybundle().unwrap().x25519_public();
        assert_eq!(ks.get_current_keybundle_version(), 1);

        // Rotate to v2
        ks.rotate_keybundle(passphrase).unwrap();

        // Should now have v2 as current
        assert_eq!(ks.get_current_keybundle_version(), 2);
        let kb_v2_x25519 = ks.get_current_keybundle().unwrap().x25519_public();
        let kb_v2_version = ks.get_current_keybundle().unwrap().version;
        assert_eq!(kb_v2_version, 2);

        // V2 should have different keys
        assert_ne!(kb_v2_x25519, kb_v1_x25519);

        // Should have both bundles in history
        assert_eq!(ks.get_keybundles().len(), 2);

        // Lock and reopen to verify persistence
        ks.lock();
        let mut ks2 = AgeKeyStore::open(&path).unwrap();
        ks2.unlock(passphrase).unwrap();

        // Should have both KeyBundles restored
        assert_eq!(ks2.get_keybundles().len(), 2);
        assert_eq!(ks2.get_current_keybundle_version(), 2);
        assert_eq!(
            ks2.get_current_keybundle().unwrap().x25519_public(),
            kb_v2_x25519
        );
    }

    #[test]
    fn test_sdis_keybundle_sign_verify() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore and SDIS
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        let anchor = Anchor::genesis("test");
        ks.init_sdis(anchor, passphrase).unwrap();

        // Sign a message
        let message = b"important message to sign";
        let kb = ks.get_current_keybundle().unwrap();
        let signature = kb.sign(message);
        let pub_bundle = kb.public_bundle();

        // Verify signature
        assert!(pub_bundle.verify(message, &signature));

        // Lock, reopen, and verify signature still works with restored keys
        ks.lock();
        let mut ks2 = AgeKeyStore::open(&path).unwrap();
        ks2.unlock(passphrase).unwrap();

        let kb2 = ks2.get_current_keybundle().unwrap();
        let pub_bundle2 = kb2.public_bundle();

        // Old signature should still verify
        assert!(pub_bundle2.verify(message, &signature));

        // New signature should verify
        let signature2 = kb2.sign(message);
        assert!(pub_bundle2.verify(message, &signature2));
    }

    #[test]
    fn test_sdis_cannot_reinitialize() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore and SDIS
        let mut ks = AgeKeyStore::init(&path, passphrase).unwrap();
        let anchor = Anchor::genesis("first");
        ks.init_sdis(anchor, passphrase).unwrap();

        // Try to reinitialize - should fail
        let anchor2 = Anchor::genesis("second");
        let result = ks.init_sdis(anchor2, passphrase);
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err());
        assert!(
            err_msg.contains("already initialized"),
            "Expected 'already initialized' in: {err_msg}"
        );
    }

    #[test]
    fn test_sdis_requires_unlocked() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keypair.age");
        let passphrase = b"test-passphrase";

        // Initialize keystore
        let _ks = AgeKeyStore::init(&path, passphrase).unwrap();

        // Open locked keystore
        let mut ks2 = AgeKeyStore::open(&path).unwrap();
        assert!(ks2.is_locked());

        // Try to init SDIS while locked - should fail
        let anchor = Anchor::genesis("test");
        let result = ks2.init_sdis(anchor, passphrase);
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err());
        assert!(
            err_msg.contains("unlocked"),
            "Expected 'unlocked' in: {err_msg}"
        );
    }
}
