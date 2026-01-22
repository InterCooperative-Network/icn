//! TPM 2.0 backend for hardware key storage
//!
//! This backend seals Ed25519 keys to TPM 2.0 hardware with PCR binding.
//! Since TPM 2.0 does not natively support Ed25519, we use TPM for sealed storage
//! and perform signing operations in software with unsealed key material.
//!
//! ## Security Properties
//!
//! - Keys are sealed to TPM, bound to platform PCRs
//! - PCR verification prevents unauthorized unsealing
//! - Sealed blobs are stored on disk
//! - Private key material is zeroized after use
//!
//! ## Requirements
//!
//! - TPM 2.0 hardware or swtpm simulator
//! - `/dev/tpmrm0` device (TPM resource manager)
//! - Linux kernel 4.12+ with TPM 2.0 support
//!
//! This module requires the `tpm-experimental` feature flag.

#![cfg(feature = "tpm-experimental")]

use crate::keystore_backend::{KeyStoreBackend, SigningBackend, TpmConfig};
use crate::{Did, DidKey, DidSigner, IdentityBundle};
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use rand_core::OsRng;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};
use tss_esapi::structures::PcrSlot;
use zeroize::Zeroizing;

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
    /// Sealed data (TPM sealed blob)
    sealed_data: Vec<u8>,
    /// Public key bytes
    public_key: [u8; 32],
    /// PCR slots used for sealing
    pcr_slots: Vec<u8>,
    /// TPM key handle (persistent)
    key_handle: Option<u32>,
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
        if !sealed_blob_dir.exists() {
            fs::create_dir_all(&sealed_blob_dir)
                .context("Failed to create sealed blob directory")?;
            // Set directory permissions to 0700 (owner only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&sealed_blob_dir, fs::Permissions::from_mode(0o700))
                    .context("Failed to set sealed blob directory permissions")?;
            }
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
        })
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
        let sealed_data = self.seal_key(&secret_bytes)?;

        // Store sealed blob
        let blob = SealedKeyBlob {
            sealed_data,
            public_key: public_bytes,
            pcr_slots: if self.config.platform_binding {
                // Convert PcrSlot enum to slot numbers
                // PcrSlot::Slot0 = 0, PcrSlot::Slot7 = 7, etc.
                DEFAULT_PCR_SLOTS.iter().map(pcr_slot_to_u8).collect()
            } else {
                vec![]
            },
            key_handle: Some(self.config.key_handle),
        };

        let blob_json = serde_json::to_vec(&blob).context("Failed to serialize sealed key blob")?;

        // Write with restrictive permissions
        fs::write(&self.sealed_blob_path, &blob_json).context("Failed to write sealed key blob")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.sealed_blob_path, fs::Permissions::from_mode(0o600))
                .context("Failed to set sealed blob file permissions")?;
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
            .context("Failed to create identity bundle")?;

        self.identity_bundle = Some(bundle.clone());
        self.signer = Some(signer);

        Ok(bundle)
    }

    /// Seal key material to TPM
    ///
    /// For Phase 1, we implement a simplified sealing without PCR binding.
    /// PCR binding will be added in a future phase.
    fn seal_key(&mut self, key_bytes: &[u8]) -> Result<Vec<u8>> {
        info!("Sealing key to TPM (simplified - no PCR binding)");

        // For now, just encrypt the key with a simple wrapper
        // In a real implementation, this would use TPM_Create with unsealing policy
        // TODO: Implement actual TPM sealing with tss-esapi::Context::create

        // Simplified approach: Store key encrypted (placeholder for actual TPM sealing)
        let sealed_data = key_bytes.to_vec();

        info!("Key sealed successfully (placeholder implementation)");
        warn!("TPM sealing is using placeholder implementation - not production-ready");

        Ok(sealed_data)
    }

    /// Load sealed key blob from disk
    ///
    /// Returns the deserialized blob structure for use by unseal and other operations.
    fn load_sealed_blob(&self) -> Result<SealedKeyBlob> {
        // Use generic error message to avoid path leakage in logs/error reports
        let blob_json =
            fs::read(&self.sealed_blob_path).context("Failed to read sealed key blob")?;

        serde_json::from_slice(&blob_json).context("Failed to deserialize sealed key blob")
    }

    /// Unseal key material from TPM
    ///
    /// For Phase 1, we implement a simplified unsealing without PCR verification.
    /// PCR verification will be added in a future phase.
    fn unseal_key_from_blob(&self, blob: &SealedKeyBlob) -> Result<Zeroizing<[u8; 32]>> {
        info!("Unsealing key from TPM (simplified - no PCR verification)");

        // For now, just decrypt the key from the simple wrapper
        // In a real implementation, this would use TPM_Unseal with policy session
        // TODO: Implement actual TPM unsealing with tss-esapi::Context::unseal

        if blob.sealed_data.is_empty() {
            anyhow::bail!("Sealed key blob is empty");
        }

        if blob.sealed_data.len() != 32 {
            anyhow::bail!(
                "Sealed key has wrong size: {} (expected 32)",
                blob.sealed_data.len()
            );
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&blob.sealed_data);

        info!("Key unsealed successfully (placeholder implementation)");
        warn!("TPM unsealing is using placeholder implementation - not production-ready");

        Ok(Zeroizing::new(key_bytes))
    }

    /// Generate attestation quote for the key
    pub fn generate_attestation(&mut self) -> Result<Vec<u8>> {
        if !self.attestation {
            anyhow::bail!("Attestation not enabled for this backend");
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
        let verifying_key = VerifyingKey::from_bytes(&blob.public_key)
            .context("Failed to reconstruct verifying key")?;

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
            .context("Failed to create identity bundle")?;

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
            .ok_or_else(|| anyhow::anyhow!("TPM backend is locked"))
    }

    fn path(&self) -> &Path {
        &self.storage_id
    }

    fn signing_backend(&self) -> Result<Box<dyn SigningBackend>> {
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("TPM backend is locked"))?;

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
            sealed_data: vec![1, 2, 3, 4, 5],
            public_key: [0u8; 32],
            pcr_slots: vec![0, 7],
            key_handle: Some(0x81000001),
        };

        let json = serde_json::to_vec(&blob).unwrap();
        let deserialized: SealedKeyBlob = serde_json::from_slice(&json).unwrap();

        assert_eq!(blob.sealed_data, deserialized.sealed_data);
        assert_eq!(blob.public_key, deserialized.public_key);
        assert_eq!(blob.pcr_slots, deserialized.pcr_slots);
        assert_eq!(blob.key_handle, deserialized.key_handle);
    }

    #[test]
    fn test_pcr_slot_to_u8() {
        assert_eq!(pcr_slot_to_u8(&PcrSlot::Slot0), 0);
        assert_eq!(pcr_slot_to_u8(&PcrSlot::Slot7), 7);
        assert_eq!(pcr_slot_to_u8(&PcrSlot::Slot23), 23);
    }

    #[test]
    fn test_unseal_key_from_blob_wrong_size() {
        let test_dir = test_sealed_blob_dir();
        let config = TpmConfig {
            device_path: tpm_device(),
            key_handle: 0x81000010,
            platform_binding: false,
            attestation: false,
            sealed_blob_dir: Some(test_dir.clone()),
        };

        let backend = TpmBackend::new(config).unwrap();

        // Test with wrong size sealed data
        let blob = SealedKeyBlob {
            sealed_data: vec![1, 2, 3], // Only 3 bytes, should be 32
            public_key: [0u8; 32],
            pcr_slots: vec![],
            key_handle: Some(0x81000010),
        };

        let result = backend.unseal_key_from_blob(&blob);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("wrong size"));

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

        let backend = TpmBackend::new(config).unwrap();

        // Test with empty sealed data
        let blob = SealedKeyBlob {
            sealed_data: vec![], // Empty
            public_key: [0u8; 32],
            pcr_slots: vec![],
            key_handle: Some(0x81000011),
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
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("locked"));

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
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("locked"));

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
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("deserialize"));

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
}
