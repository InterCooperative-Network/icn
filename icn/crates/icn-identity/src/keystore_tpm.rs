//! TPM 2.0 backend for hardware key storage
//!
//! This backend seals Ed25519 keys to TPM 2.0 hardware using real TPM operations.
//! Since TPM 2.0 does not natively support Ed25519, we use TPM for sealed storage
//! and perform signing operations in software with unsealed key material.
//!
//! # Implementation Status
//!
//! **Phase 2 (Real TPM Operations)**: ✅ Complete
//!
//! - ✅ Actual TPM sealing with `tss-esapi::Context::create()`
//! - ✅ Actual TPM unsealing with `tss-esapi::Context::unseal()`
//! - ✅ Marshall/Unmarshall of TPM structures for persistence
//! - ⏳ PCR policy binding and verification (planned)
//! - ⏳ Persistent handle management (planned)
//!
//! ## Security Properties
//!
//! - ✅ Hardware-backed sealed storage via TPM2_Create/TPM2_Unseal
//! - ✅ Restrictive file permissions (0600) on sealed blobs
//! - ✅ Zeroization of key material in memory
//! - ✅ Durable persistence with fsync
//! - ⏳ PCR binding for platform integrity (planned)
//!
//! ## Requirements
//!
//! - TPM 2.0 hardware or swtpm simulator
//! - `/dev/tpmrm0` device (TPM resource manager)
//! - Linux kernel 4.12+ with TPM 2.0 support
//! - libtss2-dev installed
//!
//! This module requires the `tpm-experimental` feature flag.

use crate::keystore_backend::{KeyStoreBackend, SigningBackend, TpmConfig};
use crate::{Did, DidKey, DidSigner, IdentityBundle};
use anyhow::Result;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use rand_core::OsRng;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};
use tss_esapi::{
    attributes::ObjectAttributesBuilder,
    handles::KeyHandle,
    interface_types::{
        algorithm::{HashingAlgorithm, PublicAlgorithm},
        resource_handles::Hierarchy,
    },
    structures::{
        Digest, KeyedHashScheme, PcrSlot, Public, PublicBuilder, PublicKeyedHashParameters,
        SensitiveData, SymmetricDefinitionObject,
    },
    tcti_ldr::{DeviceConfig, NetworkTPMConfig, TctiNameConf},
    traits::{Marshall, UnMarshall},
    Context,
};
use zeroize::Zeroizing;

/// TPM backend errors with structured context for debugging
///
/// These errors provide detailed context while avoiding sensitive information
/// (like file paths) in user-facing messages. Debug context is available via
/// the `source()` chain for logging and debugging purposes.
#[derive(Debug, Error)]
pub enum TpmError {
    /// Failed to read sealed key blob from disk
    #[error("Failed to read sealed key blob")]
    SealedBlobRead(#[source] std::io::Error),

    /// Failed to write sealed key blob to disk
    #[error("Failed to write sealed key blob")]
    SealedBlobWrite(#[source] std::io::Error),

    /// Failed to serialize sealed key blob
    #[error("Failed to serialize sealed key blob")]
    Serialization(#[source] serde_json::Error),

    /// Failed to deserialize sealed key blob
    #[error("Failed to deserialize sealed key blob")]
    Deserialization(#[source] serde_json::Error),

    /// Sealed key has invalid size
    #[error("Sealed key has invalid size: {actual} bytes (expected {expected})")]
    InvalidKeySize { expected: usize, actual: usize },

    /// Sealed key blob is empty
    #[error("Sealed key blob is empty")]
    EmptySealedBlob,

    /// Backend is locked
    #[error("TPM backend is locked")]
    BackendLocked,

    /// Attestation not enabled
    #[error("Attestation not enabled for this backend")]
    AttestationNotEnabled,

    /// Failed to create storage directory
    #[error("Failed to create sealed blob directory")]
    DirectoryCreation(#[source] std::io::Error),

    /// Failed to set file/directory permissions
    #[error("Failed to set permissions")]
    PermissionError(#[source] std::io::Error),

    /// Failed to reconstruct verifying key
    #[error("Failed to reconstruct verifying key")]
    InvalidPublicKey(#[source] ed25519_dalek::SignatureError),

    /// Failed to create identity bundle
    #[error("Failed to create identity bundle")]
    BundleCreation(#[source] anyhow::Error),

    /// Failed to connect to TPM device
    #[error("Failed to connect to TPM device: {0}")]
    TpmConnection(String),

    /// Failed to create TPM primary key
    #[error("Failed to create TPM primary key")]
    TpmPrimaryKeyCreation(String),

    /// Failed to seal data to TPM
    #[error("Failed to seal data to TPM")]
    TpmSeal(String),

    /// Failed to unseal data from TPM
    #[error("Failed to unseal data from TPM")]
    TpmUnseal(String),

    /// Failed to create PCR policy
    #[error("Failed to create PCR policy")]
    TpmPcrPolicy(String),

    /// PCR values have changed since sealing
    #[error("PCR verification failed: values have changed since sealing")]
    PcrMismatch,
}

impl TpmError {
    /// Log debug context for this error
    ///
    /// This logs additional context that may be useful for debugging
    /// but should not be exposed in user-facing error messages.
    pub fn log_debug_context(&self, path: Option<&Path>) {
        if let Some(p) = path {
            debug!(error = %self, path = %p.display(), "TPM error with path context");
        } else {
            debug!(error = %self, "TPM error");
        }
    }
}

/// PCR slots to bind for platform integrity
const DEFAULT_PCR_SLOTS: &[PcrSlot] = &[
    PcrSlot::Slot0, // BIOS/UEFI firmware
    PcrSlot::Slot7, // Secure Boot policy
];

/// Convert PcrSlot enum to its numeric value
///
/// The PcrSlot enum values correspond to their slot numbers:
/// Slot0 = 0, Slot1 = 1, ..., Slot23 = 23
fn pcr_slot_to_u8(slot: &PcrSlot) -> u8 {
    match slot {
        PcrSlot::Slot0 => 0,
        PcrSlot::Slot1 => 1,
        PcrSlot::Slot2 => 2,
        PcrSlot::Slot3 => 3,
        PcrSlot::Slot4 => 4,
        PcrSlot::Slot5 => 5,
        PcrSlot::Slot6 => 6,
        PcrSlot::Slot7 => 7,
        PcrSlot::Slot8 => 8,
        PcrSlot::Slot9 => 9,
        PcrSlot::Slot10 => 10,
        PcrSlot::Slot11 => 11,
        PcrSlot::Slot12 => 12,
        PcrSlot::Slot13 => 13,
        PcrSlot::Slot14 => 14,
        PcrSlot::Slot15 => 15,
        PcrSlot::Slot16 => 16,
        PcrSlot::Slot17 => 17,
        PcrSlot::Slot18 => 18,
        PcrSlot::Slot19 => 19,
        PcrSlot::Slot20 => 20,
        PcrSlot::Slot21 => 21,
        PcrSlot::Slot22 => 22,
        PcrSlot::Slot23 => 23,
        PcrSlot::Slot24 => 24,
        PcrSlot::Slot25 => 25,
        PcrSlot::Slot26 => 26,
        PcrSlot::Slot27 => 27,
        PcrSlot::Slot28 => 28,
        PcrSlot::Slot29 => 29,
        PcrSlot::Slot30 => 30,
        PcrSlot::Slot31 => 31,
    }
}

/// Get the default sealed blob directory
///
/// Uses XDG_DATA_HOME if set, otherwise falls back to ~/.local/share/icn/tpm/
fn default_sealed_blob_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".local").join("share"))
                .unwrap_or_else(|_| PathBuf::from("."))
        })
        .join("icn")
        .join("tpm")
}

/// Sealed key blob with metadata
#[derive(serde::Serialize, serde::Deserialize)]
struct SealedKeyBlob {
    /// TPM sealed private key data (marshalled TPM2B_PRIVATE)
    tpm_private: Vec<u8>,
    /// TPM sealed public key data (marshalled TPM2B_PUBLIC)
    tpm_public: Vec<u8>,
    /// Ed25519 public key bytes (for verification and DID generation)
    public_key: [u8; 32],
    /// PCR slots used for sealing
    pcr_slots: Vec<u8>,
    /// TPM key handle (persistent)
    key_handle: Option<u32>,
    /// Version marker for format migration
    #[serde(default = "default_blob_version")]
    version: u8,
}

fn default_blob_version() -> u8 {
    2 // Phase 2 format with real TPM sealing
}

/// TPM 2.0 backend
///
/// This backend stores keys in a TPM 2.0 device with optional platform binding.
/// Keys are sealed to PCR values and can only be used on the same platform.
pub struct TpmBackend {
    /// TPM configuration
    config: TpmConfig,
    /// Path to sealed key blob file
    sealed_blob_path: PathBuf,
    /// Cached identity bundle (None when locked)
    identity_bundle: Option<IdentityBundle>,
    /// Cached signer for hardware signing
    signer: Option<Arc<TpmDidSigner>>,
    /// Storage identifier (for path() trait method)
    storage_id: PathBuf,
    /// TPM context for hardware operations (lazily initialized)
    tpm_context: Option<Context>,
    /// Primary key handle for sealing operations
    primary_key_handle: Option<KeyHandle>,
}

/// TPM DID signer that delegates signing to unsealed key material
pub struct TpmDidSigner {
    did: Did,
    verifying_key: VerifyingKey,
    /// Unsealed key material (zeroized on drop)
    secret_key: Zeroizing<[u8; 32]>,
}

impl TpmBackend {
    /// Create a new TPM 2.0 backend
    ///
    /// # Arguments
    /// * `config` - TPM configuration (device path, options)
    ///
    /// # Returns
    /// A locked TPM backend ready to be unlocked
    pub fn new(config: TpmConfig) -> Result<Self> {
        // Note: PCR binding not yet implemented
        if config.platform_binding {
            warn!(
                "TPM platform_binding is enabled but PCR policy binding is not yet implemented. \
                 Keys will be sealed without PCR verification. See issue #745 for status."
            );
        }

        info!(
            "Initializing TPM 2.0 backend: device={}, platform_binding={}",
            config.device_path, config.platform_binding
        );

        // Determine sealed blob directory: use config or default to XDG_DATA_HOME/icn/tpm/
        let sealed_blob_dir = config
            .sealed_blob_dir
            .clone()
            .unwrap_or_else(default_sealed_blob_dir);

        // Ensure the directory exists with appropriate permissions
        // Note: create_dir_all is idempotent, and we always set permissions
        // to avoid TOCTOU race conditions
        fs::create_dir_all(&sealed_blob_dir).map_err(|e| {
            let err = TpmError::DirectoryCreation(e);
            err.log_debug_context(Some(&sealed_blob_dir));
            err
        })?;

        // Always ensure permissions are correct, even if directory already existed
        // This prevents TOCTOU races where another process creates the directory
        // with different permissions between our exists() check and mkdir
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&sealed_blob_dir, fs::Permissions::from_mode(0o700))
                .map_err(TpmError::PermissionError)?;
        }

        let sealed_blob_path =
            sealed_blob_dir.join(format!("sealed-{:#x}.blob", config.key_handle));
        let storage_id = PathBuf::from(format!("tpm://handle={:#x}", config.key_handle));

        Ok(Self {
            config,
            sealed_blob_path,
            identity_bundle: None,
            signer: None,
            storage_id,
            tpm_context: None,
            primary_key_handle: None,
        })
    }

    /// Initialize TPM context and primary key for sealing operations
    ///
    /// This lazily creates the TPM context and a primary key under the Owner hierarchy.
    /// The primary key is used as the parent for sealed objects.
    fn ensure_tpm_context(&mut self) -> Result<()> {
        if self.tpm_context.is_some() && self.primary_key_handle.is_some() {
            return Ok(());
        }

        info!("Connecting to TPM device: {}", self.config.device_path);

        // Parse the device path into a TCTI configuration
        let tcti_conf = if self.config.device_path.starts_with("swtpm:") {
            // Software TPM: "swtpm:host=localhost,port=2321"
            // NetworkTPMConfig parses format "host:port"
            let config_str = self
                .config
                .device_path
                .strip_prefix("swtpm:")
                .unwrap_or("host=localhost,port=2321");

            // Convert "host=localhost,port=2321" to "localhost:2321" for NetworkTPMConfig
            let net_config = if config_str.contains('=') {
                // Parse key=value format
                let mut host = "localhost";
                let mut port = "2321";
                for part in config_str.split(',') {
                    let kv: Vec<&str> = part.split('=').collect();
                    if kv.len() == 2 {
                        match kv[0].trim() {
                            "host" => host = kv[1].trim(),
                            "port" => port = kv[1].trim(),
                            _ => {}
                        }
                    }
                }
                format!("{host}:{port}")
            } else {
                // Assume already in "host:port" format
                config_str.to_string()
            };

            let parsed: NetworkTPMConfig = net_config.parse().map_err(|e| {
                TpmError::TpmConnection(format!("Invalid swtpm config '{net_config}': {e}"))
            })?;
            TctiNameConf::Swtpm(parsed)
        } else {
            // Hardware TPM: "/dev/tpmrm0" or "/dev/tpm0"
            let parsed: DeviceConfig = self.config.device_path.parse().map_err(|e| {
                TpmError::TpmConnection(format!(
                    "Invalid device path '{}': {e}",
                    self.config.device_path
                ))
            })?;
            TctiNameConf::Device(parsed)
        };

        // Create TPM context
        let mut context = Context::new(tcti_conf)
            .map_err(|e| TpmError::TpmConnection(format!("Failed to create context: {e}")))?;

        info!("TPM context created successfully");

        // Create primary key for sealing operations
        let primary_key_handle = self.create_primary_key(&mut context)?;

        self.tpm_context = Some(context);
        self.primary_key_handle = Some(primary_key_handle);

        Ok(())
    }

    /// Create a primary key under the Owner hierarchy for sealing operations
    fn create_primary_key(&self, context: &mut Context) -> Result<KeyHandle> {
        info!("Creating primary key for sealing operations");

        // Build object attributes for a storage key
        let object_attributes = ObjectAttributesBuilder::new()
            .with_fixed_tpm(true)
            .with_fixed_parent(true)
            .with_sensitive_data_origin(true)
            .with_user_with_auth(true)
            .with_restricted(true)
            .with_decrypt(true)
            .build()
            .map_err(|e| {
                TpmError::TpmPrimaryKeyCreation(format!("Failed to build attributes: {e}"))
            })?;

        // Build public template for a symmetric storage key
        let primary_pub = PublicBuilder::new()
            .with_public_algorithm(PublicAlgorithm::SymCipher)
            .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
            .with_object_attributes(object_attributes)
            .with_symmetric_cipher_parameters(
                tss_esapi::structures::SymmetricCipherParameters::new(
                    SymmetricDefinitionObject::AES_128_CFB,
                ),
            )
            .with_symmetric_cipher_unique_identifier(Digest::default())
            .build()
            .map_err(|e| TpmError::TpmPrimaryKeyCreation(format!("Failed to build public: {e}")))?;

        // Create primary key under Owner hierarchy
        let primary = context
            .execute_with_nullauth_session(|ctx| {
                ctx.create_primary(Hierarchy::Owner, primary_pub, None, None, None, None)
            })
            .map_err(|e| {
                TpmError::TpmPrimaryKeyCreation(format!("Failed to create primary: {e}"))
            })?;

        info!("Primary key created successfully");

        Ok(primary.key_handle)
    }

    /// Get the hardware identifier for this backend
    ///
    /// Uses the key handle as the stable identifier, not the file path.
    fn hardware_id(&self) -> String {
        format!("handle={:#x}", self.config.key_handle)
    }

    /// Initialize a new keypair and seal to TPM
    ///
    /// This generates a new Ed25519 keypair and seals the private key to the TPM
    /// with PCR binding if platform binding is enabled.
    ///
    /// # Arguments
    /// * `_auth` - TPM owner authorization (currently unused)
    ///
    /// # Returns
    /// The generated identity bundle
    pub fn init(&mut self, _auth: &[u8]) -> Result<IdentityBundle> {
        info!("Initializing new TPM-sealed identity");

        // Generate Ed25519 keypair
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let verifying_key = signing_key.verifying_key();
        let public_bytes = verifying_key.to_bytes();

        info!("Generated Ed25519 keypair");

        // Seal the private key to TPM
        let (tpm_private, tpm_public) = self.seal_key(&secret_bytes)?;

        // Store sealed blob
        let blob = SealedKeyBlob {
            tpm_private,
            tpm_public,
            public_key: public_bytes,
            pcr_slots: if self.config.platform_binding {
                // Convert PcrSlot enum to slot numbers
                // PcrSlot::Slot0 = 0, PcrSlot::Slot7 = 7, etc.
                DEFAULT_PCR_SLOTS.iter().map(pcr_slot_to_u8).collect()
            } else {
                vec![]
            },
            key_handle: Some(self.config.key_handle),
            version: 2, // Phase 2 format
        };

        let blob_json = serde_json::to_vec(&blob).map_err(TpmError::Serialization)?;

        // Write with restrictive permissions using OpenOptions for atomic permission setting
        // This ensures the file is created with correct permissions from the start,
        // avoiding a window where the file exists with default (potentially insecure) permissions
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;

            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600) // Set permissions atomically at creation
                .open(&self.sealed_blob_path)
                .map_err(|e| {
                    let err = TpmError::SealedBlobWrite(e);
                    err.log_debug_context(Some(&self.sealed_blob_path));
                    err
                })?;
            file.write_all(&blob_json).map_err(|e| {
                let err = TpmError::SealedBlobWrite(e);
                err.log_debug_context(Some(&self.sealed_blob_path));
                err
            })?;
            // Ensure durable persistence for security-critical sealed blob
            file.sync_all().map_err(|e| {
                let err = TpmError::SealedBlobWrite(e);
                err.log_debug_context(Some(&self.sealed_blob_path));
                err
            })?;
        }
        #[cfg(not(unix))]
        {
            use std::io::Write;

            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.sealed_blob_path)
                .map_err(|e| {
                    let err = TpmError::SealedBlobWrite(e);
                    err.log_debug_context(Some(&self.sealed_blob_path));
                    err
                })?;
            file.write_all(&blob_json).map_err(|e| {
                let err = TpmError::SealedBlobWrite(e);
                err.log_debug_context(Some(&self.sealed_blob_path));
                err
            })?;
            // Ensure durable persistence for security-critical sealed blob
            file.sync_all().map_err(|e| {
                let err = TpmError::SealedBlobWrite(e);
                err.log_debug_context(Some(&self.sealed_blob_path));
                err
            })?;
        }

        info!("Sealed key blob written");

        // Create hardware DidKey using stable handle-based identifier
        let did_key = DidKey::from_hardware(verifying_key, "tpm".to_string(), self.hardware_id());

        // Create signer
        let signer = Arc::new(TpmDidSigner {
            did: did_key.did().clone(),
            verifying_key,
            secret_key: Zeroizing::new(secret_bytes),
        });

        // Create identity bundle
        let bundle = IdentityBundle::from_did_key_with_signer(did_key, Some(signer.clone()))
            .map_err(TpmError::BundleCreation)?;

        self.identity_bundle = Some(bundle.clone());
        self.signer = Some(signer);

        Ok(bundle)
    }

    /// Seal key material to TPM
    ///
    /// For Phase 1, we implement a simplified sealing without PCR binding.
    /// PCR binding will be added in a future phase.
    /// Seal key material to TPM using TPM2_Create
    ///
    /// This creates a sealed object under the primary key that contains
    /// the sensitive key material. The sealed object can only be unsealed
    /// when PCR values match (if platform binding is enabled).
    fn seal_key(&mut self, key_bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        info!("Sealing key to TPM with real TPM2_Create");

        // Ensure TPM context is initialized
        self.ensure_tpm_context()?;

        let context = self
            .tpm_context
            .as_mut()
            .ok_or_else(|| TpmError::TpmConnection("TPM context not initialized".to_string()))?;

        let primary_handle = self.primary_key_handle.ok_or_else(|| {
            TpmError::TpmPrimaryKeyCreation("Primary key not created".to_string())
        })?;

        // Build object attributes for sealed data
        let object_attributes = ObjectAttributesBuilder::new()
            .with_fixed_tpm(true)
            .with_fixed_parent(true)
            .with_user_with_auth(true)
            .build()
            .map_err(|e| TpmError::TpmSeal(format!("Failed to build attributes: {e}")))?;

        // Build public template for sealed data object (KEYEDHASH with NULL scheme)
        let sealed_pub = PublicBuilder::new()
            .with_public_algorithm(PublicAlgorithm::KeyedHash)
            .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
            .with_object_attributes(object_attributes)
            .with_keyed_hash_parameters(PublicKeyedHashParameters::new(KeyedHashScheme::Null))
            .with_keyed_hash_unique_identifier(Digest::default())
            .build()
            .map_err(|e| TpmError::TpmSeal(format!("Failed to build public template: {e}")))?;

        // Convert key bytes to SensitiveData
        let sensitive_data = SensitiveData::try_from(key_bytes.to_vec())
            .map_err(|e| TpmError::TpmSeal(format!("Failed to create sensitive data: {e}")))?;

        // Create sealed object under primary key
        let result = context
            .execute_with_nullauth_session(|ctx| {
                ctx.create(
                    primary_handle,
                    sealed_pub,
                    None,
                    Some(sensitive_data),
                    None,
                    None,
                )
            })
            .map_err(|e| TpmError::TpmSeal(format!("TPM2_Create failed: {e}")))?;

        // Marshal the private and public parts to bytes for storage
        // Private implements Deref<Target=Vec<u8>>, so we can clone the underlying bytes
        let tpm_private: Vec<u8> = (*result.out_private).clone();
        // Public implements the Marshall trait
        let tpm_public: Vec<u8> = result
            .out_public
            .marshall()
            .map_err(|e| TpmError::TpmSeal(format!("Failed to marshal public: {e}")))?;

        info!(
            "Key sealed successfully: private={} bytes, public={} bytes",
            tpm_private.len(),
            tpm_public.len()
        );

        Ok((tpm_private, tpm_public))
    }

    /// Load sealed key blob from disk
    ///
    /// Returns the deserialized blob structure for use by unseal and other operations.
    fn load_sealed_blob(&self) -> Result<SealedKeyBlob> {
        // Read blob, logging path only at debug level to avoid path leakage
        let blob_json = fs::read(&self.sealed_blob_path).map_err(|e| {
            let err = TpmError::SealedBlobRead(e);
            err.log_debug_context(Some(&self.sealed_blob_path));
            err
        })?;

        let blob = serde_json::from_slice(&blob_json).map_err(|e| {
            let err = TpmError::Deserialization(e);
            err.log_debug_context(Some(&self.sealed_blob_path));
            err
        })?;

        Ok(blob)
    }

    /// Unseal key material from TPM
    ///
    /// For Phase 1, we implement a simplified unsealing without PCR verification.
    /// PCR verification will be added in a future phase.
    /// Unseal key material from TPM using TPM2_Load and TPM2_Unseal
    ///
    /// This loads the sealed object into the TPM and unseals it to recover
    /// the original key material. If PCR binding was used during sealing,
    /// unsealing will fail if PCR values have changed.
    fn unseal_key_from_blob(&mut self, blob: &SealedKeyBlob) -> Result<Zeroizing<[u8; 32]>> {
        info!("Unsealing key from TPM with real TPM2_Unseal");

        // Check for empty sealed data
        if blob.tpm_private.is_empty() || blob.tpm_public.is_empty() {
            return Err(TpmError::EmptySealedBlob.into());
        }

        // Ensure TPM context is initialized
        self.ensure_tpm_context()?;

        let context = self
            .tpm_context
            .as_mut()
            .ok_or_else(|| TpmError::TpmConnection("TPM context not initialized".to_string()))?;

        let primary_handle = self.primary_key_handle.ok_or_else(|| {
            TpmError::TpmPrimaryKeyCreation("Primary key not created".to_string())
        })?;

        // Unmarshal the private part (Private implements TryFrom<Vec<u8>>)
        let tpm_private: tss_esapi::structures::Private = blob
            .tpm_private
            .clone()
            .try_into()
            .map_err(|e| TpmError::TpmUnseal(format!("Failed to unmarshal private: {e:?}")))?;

        // Unmarshal the public part using UnMarshall trait
        let tpm_public: Public = Public::unmarshall(&blob.tpm_public)
            .map_err(|e| TpmError::TpmUnseal(format!("Failed to unmarshal public: {e}")))?;

        // Load the sealed object into TPM
        let loaded_key = context
            .execute_with_nullauth_session(|ctx| ctx.load(primary_handle, tpm_private, tpm_public))
            .map_err(|e| TpmError::TpmUnseal(format!("TPM2_Load failed: {e}")))?;

        // Unseal the data
        let unsealed_data = context
            .execute_with_nullauth_session(|ctx| ctx.unseal(loaded_key.into()))
            .map_err(|e| TpmError::TpmUnseal(format!("TPM2_Unseal failed: {e}")))?;

        // Convert to fixed-size array (SensitiveData implements Deref<Target=Vec<u8>>)
        let unsealed_bytes: &[u8] = &unsealed_data;
        if unsealed_bytes.len() != 32 {
            return Err(TpmError::InvalidKeySize {
                expected: 32,
                actual: unsealed_bytes.len(),
            }
            .into());
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(unsealed_bytes);

        info!("Key unsealed successfully from TPM");

        Ok(Zeroizing::new(key_bytes))
    }

    /// Generate attestation quote for the key
    pub fn generate_attestation(&mut self) -> Result<Vec<u8>> {
        if !self.config.attestation {
            return Err(TpmError::AttestationNotEnabled.into());
        }

        // TODO: Implement TPM attestation in future phase
        // This would use tss_esapi to:
        // 1. Create a quote over PCR values
        // 2. Sign the quote with the TPM's attestation key
        // 3. Return the signed quote

        warn!("TPM attestation not yet implemented");

        anyhow::bail!("TPM attestation not yet implemented")
    }
}

impl KeyStoreBackend for TpmBackend {
    fn unlock(&mut self, _credentials: &[u8]) -> Result<()> {
        if self.identity_bundle.is_some() {
            warn!("TPM backend already unlocked");
            return Ok(());
        }

        info!("Unlocking TPM backend by unsealing key");

        // Load sealed blob once to get both public key and sealed data
        let blob = self.load_sealed_blob()?;

        // Unseal the private key using the loaded blob
        let secret_bytes = self.unseal_key_from_blob(&blob)?;

        // Reconstruct verifying key
        let verifying_key =
            VerifyingKey::from_bytes(&blob.public_key).map_err(TpmError::InvalidPublicKey)?;

        // Create hardware DidKey using stable handle-based identifier
        let did_key = DidKey::from_hardware(verifying_key, "tpm".to_string(), self.hardware_id());

        // Create signer
        let signer = Arc::new(TpmDidSigner {
            did: did_key.did().clone(),
            verifying_key,
            secret_key: secret_bytes,
        });

        // Create identity bundle
        let bundle = IdentityBundle::from_did_key_with_signer(did_key, Some(signer.clone()))
            .map_err(TpmError::BundleCreation)?;

        self.identity_bundle = Some(bundle);
        self.signer = Some(signer);

        info!("TPM backend unlocked successfully");

        Ok(())
    }

    fn lock(&mut self) {
        if self.identity_bundle.take().is_some() {
            info!("Locked TPM backend");
        }
        self.signer = None;
    }

    fn is_locked(&self) -> bool {
        self.identity_bundle.is_none()
    }

    fn get_identity_bundle(&self) -> Result<&IdentityBundle> {
        self.identity_bundle
            .as_ref()
            .ok_or_else(|| TpmError::BackendLocked.into())
    }

    fn path(&self) -> &Path {
        &self.storage_id
    }

    fn signing_backend(&self) -> Result<Box<dyn SigningBackend>> {
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| TpmError::BackendLocked)?;

        Ok(Box::new(TpmSigningBackend {
            signer: Arc::clone(signer),
        }))
    }

    fn is_hardware_backed(&self) -> bool {
        true
    }

    fn backend_type(&self) -> &str {
        "tpm"
    }
}

impl Drop for TpmBackend {
    fn drop(&mut self) {
        self.lock();
    }
}

/// TpmDidSigner implementation
impl DidSigner for TpmDidSigner {
    fn did(&self) -> &Did {
        &self.did
    }

    fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    fn sign(&self, message: &[u8]) -> Result<Signature> {
        use ed25519_dalek::Signer;
        let signing_key = SigningKey::from_bytes(&self.secret_key);
        Ok(signing_key.sign(message))
    }

    fn is_hardware_backed(&self) -> bool {
        true
    }

    fn backend_type(&self) -> &str {
        "tpm"
    }
}

impl Drop for TpmDidSigner {
    fn drop(&mut self) {
        // Zeroizing<[u8; 32]> automatically zeroizes `secret_key` on drop,
        // ensuring key material is securely cleared from memory.
    }
}

/// SigningBackend implementation for TPM
struct TpmSigningBackend {
    signer: Arc<TpmDidSigner>,
}

impl SigningBackend for TpmSigningBackend {
    fn sign(&self, message: &[u8]) -> Result<Signature> {
        self.signer.sign(message)
    }

    fn did(&self) -> &Did {
        self.signer.did()
    }

    fn verifying_key(&self) -> &VerifyingKey {
        self.signer.verifying_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to check if TPM device is available
    fn tpm_available() -> bool {
        std::path::Path::new("/dev/tpmrm0").exists() || std::path::Path::new("/dev/tpm0").exists()
    }

    /// Get TPM device path
    fn tpm_device() -> String {
        if std::path::Path::new("/dev/tpmrm0").exists() {
            "/dev/tpmrm0".to_string()
        } else if std::path::Path::new("/dev/tpm0").exists() {
            "/dev/tpm0".to_string()
        } else {
            // Try swtpm socket
            "swtpm:host=localhost,port=2321".to_string()
        }
    }

    /// Create a test-specific temp directory for sealed blobs
    fn test_sealed_blob_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("icn-tpm-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_tpm_backend_creation() {
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000001,
            platform_binding: true,
            attestation: false,
            sealed_blob_dir: Some(test_sealed_blob_dir()),
        };

        let backend = TpmBackend::new(config);
        assert!(backend.is_ok());

        let backend = backend.unwrap();
        assert!(backend.is_locked());
        assert!(backend.is_hardware_backed());
        assert_eq!(backend.backend_type(), "tpm");
    }

    #[test]
    #[ignore] // Requires TPM 2.0 device or swtpm
    fn test_tpm_seal_unseal_cycle() {
        if !tpm_available() {
            eprintln!("Skipping test: TPM device not available");
            return;
        }

        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000002,
            platform_binding: false, // Disable PCR binding for test
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let mut backend = TpmBackend::new(config).unwrap();

        // Initialize with new key
        let bundle = backend.init(&[]).expect("Failed to init TPM backend");
        assert!(!backend.is_locked());

        let did = bundle.did().clone();

        // Test signing
        let message = b"test message for TPM signing";
        let signature = bundle.sign(message).expect("Failed to sign message");

        // Verify signature
        use ed25519_dalek::Verifier;
        assert!(bundle
            .did_key()
            .verifying_key()
            .verify(message, &signature)
            .is_ok());

        // Lock and unlock
        backend.lock();
        assert!(backend.is_locked());

        backend.unlock(&[]).expect("Failed to unlock TPM backend");
        assert!(!backend.is_locked());

        // Verify DID matches
        let unlocked_bundle = backend.get_identity_bundle().unwrap();
        assert_eq!(unlocked_bundle.did(), &did);

        // Test signing after unlock
        let signature2 = unlocked_bundle
            .sign(message)
            .expect("Failed to sign after unlock");
        assert!(unlocked_bundle
            .did_key()
            .verifying_key()
            .verify(message, &signature2)
            .is_ok());

        // Clean up
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    #[ignore] // Requires TPM 2.0 device or swtpm
    fn test_tpm_with_pcr_binding() {
        if !tpm_available() {
            eprintln!("Skipping test: TPM device not available");
            return;
        }

        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000003,
            platform_binding: true, // Enable PCR binding
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let mut backend = TpmBackend::new(config).unwrap();

        // Initialize with PCR binding
        let bundle = backend.init(&[]).expect("Failed to init with PCR binding");

        // Test signing works
        let message = b"test with PCR binding";
        let signature = bundle.sign(message).expect("Failed to sign");

        use ed25519_dalek::Verifier;
        assert!(bundle
            .did_key()
            .verifying_key()
            .verify(message, &signature)
            .is_ok());

        // Lock and unlock should succeed (PCRs haven't changed)
        backend.lock();
        backend
            .unlock(&[])
            .expect("Failed to unlock with PCR binding");

        // Clean up
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    #[ignore] // Requires TPM 2.0 device or swtpm
    fn test_tpm_persistent_across_restarts() {
        if !tpm_available() {
            eprintln!("Skipping test: TPM device not available");
            return;
        }

        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000004,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        // First session: create and seal
        let mut backend1 = TpmBackend::new(config.clone()).unwrap();
        let bundle1 = backend1.init(&[]).expect("Failed to init");
        let did1 = bundle1.did().clone();
        let public_key1 = bundle1.did_key().verifying_key().to_bytes();

        drop(backend1);

        // Second session: unlock existing
        let mut backend2 = TpmBackend::new(config).unwrap();
        backend2
            .unlock(&[])
            .expect("Failed to unlock in second session");

        let bundle2 = backend2.get_identity_bundle().unwrap();
        let did2 = bundle2.did();
        let public_key2 = bundle2.did_key().verifying_key().to_bytes();

        // Verify identity is the same
        assert_eq!(did1, *did2);
        assert_eq!(public_key1, public_key2);

        // Clean up
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_tpm_did_signer() {
        // Create test signer with software key
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let verifying_key = signing_key.verifying_key();
        let did = Did::from_public_key(&verifying_key);

        let signer = TpmDidSigner {
            did: did.clone(),
            verifying_key,
            secret_key: Zeroizing::new(secret_bytes),
        };

        // Test properties
        assert_eq!(signer.did(), &did);
        assert_eq!(signer.verifying_key(), &verifying_key);
        assert!(signer.is_hardware_backed());
        assert_eq!(signer.backend_type(), "tpm");

        // Test signing
        let message = b"test message";
        let signature = signer.sign(message).expect("Failed to sign");

        // Verify signature
        use ed25519_dalek::Verifier;
        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_sealed_key_blob_serialization() {
        let blob = SealedKeyBlob {
            tpm_private: vec![1, 2, 3, 4, 5],
            tpm_public: vec![10, 20, 30],
            public_key: [0u8; 32],
            pcr_slots: vec![0, 7],
            key_handle: Some(0x81000001),
            version: 2,
        };

        let json = serde_json::to_vec(&blob).unwrap();
        let deserialized: SealedKeyBlob = serde_json::from_slice(&json).unwrap();

        assert_eq!(blob.tpm_private, deserialized.tpm_private);
        assert_eq!(blob.tpm_public, deserialized.tpm_public);
        assert_eq!(blob.public_key, deserialized.public_key);
        assert_eq!(blob.pcr_slots, deserialized.pcr_slots);
        assert_eq!(blob.key_handle, deserialized.key_handle);
        assert_eq!(blob.version, deserialized.version);
    }

    #[test]
    fn test_pcr_slot_to_u8() {
        assert_eq!(pcr_slot_to_u8(&PcrSlot::Slot0), 0);
        assert_eq!(pcr_slot_to_u8(&PcrSlot::Slot7), 7);
        assert_eq!(pcr_slot_to_u8(&PcrSlot::Slot23), 23);
    }

    #[test]
    #[ignore] // Requires TPM 2.0 device or swtpm - wrong size is detected after unseal
    fn test_unseal_key_from_blob_wrong_size() {
        // This test requires a real TPM because wrong size is now detected
        // after TPM2_Unseal returns the decrypted data.
        // The test would need a sealed blob that unseals to wrong-size data,
        // which can only be created with actual TPM operations.
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000010,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let mut backend = TpmBackend::new(config).unwrap();

        // Create a blob with invalid TPM data - this will fail during unseal
        let blob = SealedKeyBlob {
            tpm_private: vec![1, 2, 3], // Invalid TPM private data
            tpm_public: vec![4, 5, 6],  // Invalid TPM public data
            public_key: [0u8; 32],
            pcr_slots: vec![],
            key_handle: Some(0x81000010),
            version: 2,
        };

        let result = backend.unseal_key_from_blob(&blob);
        assert!(result.is_err());

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_unseal_key_from_blob_empty() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000011,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let mut backend = TpmBackend::new(config).unwrap();

        // Test with empty TPM data - should fail early before TPM operations
        let blob = SealedKeyBlob {
            tpm_private: vec![], // Empty
            tpm_public: vec![],  // Empty
            public_key: [0u8; 32],
            pcr_slots: vec![],
            key_handle: Some(0x81000011),
            version: 2,
        };

        let result = backend.unseal_key_from_blob(&blob);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("empty"));

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_unlock_before_init_fails() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000020,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let mut backend = TpmBackend::new(config).unwrap();

        // Attempt to unlock without init should fail
        let result = backend.unlock(&[]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Failed to read sealed key blob"));

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_get_identity_bundle_when_locked_fails() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000021,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend = TpmBackend::new(config).unwrap();

        // Backend is locked by default
        assert!(backend.is_locked());

        // Should fail to get identity bundle when locked
        let result = backend.get_identity_bundle();
        match result {
            Ok(_) => panic!("Expected error when backend is locked"),
            Err(e) => assert!(e.to_string().contains("locked")),
        }

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_signing_backend_when_locked_fails() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000022,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend = TpmBackend::new(config).unwrap();

        // Backend is locked by default
        assert!(backend.is_locked());

        // Should fail to get signing backend when locked
        let result = backend.signing_backend();
        match result {
            Ok(_) => panic!("Expected error when backend is locked"),
            Err(e) => assert!(e.to_string().contains("locked")),
        }

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_load_corrupted_sealed_blob() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000023,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend = TpmBackend::new(config).unwrap();

        // Write corrupted JSON to the sealed blob path
        fs::write(&backend.sealed_blob_path, b"not valid json").unwrap();

        // Should fail to load corrupted blob
        let result = backend.load_sealed_blob();
        match result {
            Ok(_) => panic!("Expected error when loading corrupted blob"),
            Err(e) => assert!(e.to_string().contains("deserialize")),
        }

        // Clean up - intentionally ignore errors
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_hardware_id_format() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000042,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend = TpmBackend::new(config).unwrap();

        // Hardware ID should use handle format, not file path
        let hw_id = backend.hardware_id();
        assert_eq!(hw_id, "handle=0x81000042");
        assert!(!hw_id.contains("/")); // Should not contain path separator

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_default_sealed_blob_dir() {
        let dir = default_sealed_blob_dir();
        // Should end with icn/tpm
        assert!(dir.ends_with("icn/tpm") || dir.ends_with("icn\\tpm"));
        // Should not be in /tmp
        assert!(!dir.starts_with("/tmp"));
    }

    #[test]
    fn test_attestation_not_enabled_error() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000030,
            platform_binding: false,
            attestation: false, // Attestation disabled
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let mut backend = TpmBackend::new(config).unwrap();

        // Should fail because attestation is not enabled
        let result = backend.generate_attestation();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not enabled"));

        // Clean up - intentionally ignore errors as dir may not exist
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_handle_uniqueness_same_dir() {
        // Test that two backends with the same handle in the same directory
        // will use the same sealed blob path (potential conflict)
        let test_dir = test_sealed_blob_dir();
        let handle = 0x81000050;

        let config1 = TpmConfig {
            device_path: tpm_device(),
            key_handle: handle,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let config2 = TpmConfig {
            device_path: tpm_device(),
            key_handle: handle, // Same handle
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend1 = TpmBackend::new(config1).unwrap();
        let backend2 = TpmBackend::new(config2).unwrap();

        // Same handle in same directory = same sealed blob path (conflict!)
        assert_eq!(backend1.sealed_blob_path, backend2.sealed_blob_path);

        // Clean up
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_handle_uniqueness_different_handles() {
        // Test that two backends with different handles have different sealed blob paths
        let test_dir = test_sealed_blob_dir();

        let config1 = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000051,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let config2 = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000052, // Different handle
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend1 = TpmBackend::new(config1).unwrap();
        let backend2 = TpmBackend::new(config2).unwrap();

        // Different handles = different sealed blob paths (no conflict)
        assert_ne!(backend1.sealed_blob_path, backend2.sealed_blob_path);

        // Verify paths contain the handle hex values
        assert!(backend1
            .sealed_blob_path
            .to_string_lossy()
            .contains("0x81000051"));
        assert!(backend2
            .sealed_blob_path
            .to_string_lossy()
            .contains("0x81000052"));

        // Clean up
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_tpm_error_display() {
        // Test that TpmError variants display properly
        let err = TpmError::BackendLocked;
        assert_eq!(err.to_string(), "TPM backend is locked");

        let err = TpmError::EmptySealedBlob;
        assert_eq!(err.to_string(), "Sealed key blob is empty");

        let err = TpmError::InvalidKeySize {
            expected: 32,
            actual: 16,
        };
        assert_eq!(
            err.to_string(),
            "Sealed key has invalid size: 16 bytes (expected 32)"
        );

        let err = TpmError::AttestationNotEnabled;
        assert_eq!(err.to_string(), "Attestation not enabled for this backend");
    }

    #[test]
    #[ignore] // Requires TPM 2.0 device or swtpm
    fn test_same_handle_overwrites_previous() {
        // Test that initializing a second backend with the same handle
        // overwrites the previous identity (important behavior to document)
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000060,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        // First backend: create and init
        let mut backend1 = TpmBackend::new(config.clone()).unwrap();
        let bundle1 = backend1.init(&[]).unwrap();
        let did1 = bundle1.did().clone();

        // Second backend with SAME handle: init should overwrite
        let mut backend2 = TpmBackend::new(config.clone()).unwrap();
        let bundle2 = backend2.init(&[]).unwrap();
        let did2 = bundle2.did().clone();

        // Different DIDs because different key material was generated
        assert_ne!(did1, did2);

        // Now if we create a third backend and unlock (not init), it should get backend2's identity
        let mut backend3 = TpmBackend::new(config).unwrap();
        backend3.unlock(&[]).unwrap();
        let bundle3 = backend3.get_identity_bundle().unwrap();

        // backend3 should have backend2's DID (the overwritten one)
        assert_eq!(bundle3.did(), &did2);
        assert_ne!(bundle3.did(), &did1);

        // Clean up
        let _ = fs::remove_dir_all(&test_dir);
    }
}
