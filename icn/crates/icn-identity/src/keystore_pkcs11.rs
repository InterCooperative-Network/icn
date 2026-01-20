//! PKCS#11 HSM backend for hardware key storage
//!
//! This backend implements keystore operations using PKCS#11 HSMs like:
//! - YubiHSM2
//! - AWS CloudHSM
//! - GCP Cloud HSM
//! - SoftHSM2 (for testing)
//!
//! Keys are generated in the HSM and never leave the secure boundary.
//! All signing operations are delegated to the HSM.

#![cfg(feature = "hsm")]

use crate::keystore_backend::{BackendConfig, KeyStoreBackend, Pkcs11Config, SigningBackend};
use crate::{Did, IdentityBundle, KeyPair};
use anyhow::{Context, Result};
use cryptoki::context::{CInitializeArgs, Pkcs11};
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::types::AuthPin;
use ed25519_dalek::VerifyingKey;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// PKCS#11 HSM backend
///
/// This backend stores keys in a hardware security module using the PKCS#11 interface.
/// Keys are generated within the HSM and never exposed to the application.
pub struct Pkcs11Backend {
    /// PKCS#11 context
    pkcs11: Arc<Pkcs11>,
    /// HSM slot ID
    slot_id: cryptoki::slot::Slot,
    /// Key label for finding the keypair
    key_label: String,
    /// Token label (optional)
    token_label: Option<String>,
    /// Session (None when locked)
    session: Option<Session>,
    /// Cached identity bundle (None when locked)
    identity_bundle: Option<IdentityBundle>,
    /// Cached DID
    did: Option<Did>,
    /// Cached verifying key
    verifying_key: Option<VerifyingKey>,
    /// Storage identifier (for path() trait method)
    storage_id: PathBuf,
}

impl Pkcs11Backend {
    /// Create a new PKCS#11 backend
    ///
    /// # Arguments
    /// * `config` - PKCS#11 configuration (library path, slot ID, key label)
    ///
    /// # Returns
    /// A locked PKCS#11 backend ready to be unlocked
    pub fn new(config: Pkcs11Config) -> Result<Self> {
        // Initialize PKCS#11 context
        let pkcs11 = Pkcs11::new(&config.library_path)
            .context("Failed to load PKCS#11 library")?;

        pkcs11
            .initialize(CInitializeArgs::OsThreads)
            .context("Failed to initialize PKCS#11 library")?;

        info!(
            "Initialized PKCS#11 backend: library={}, slot={}",
            config.library_path, config.slot_id
        );

        let slot_id = cryptoki::slot::Slot::new(config.slot_id);
        let storage_id = PathBuf::from(format!("pkcs11://slot={}", config.slot_id));

        Ok(Self {
            pkcs11: Arc::new(pkcs11),
            slot_id,
            key_label: config.key_label,
            token_label: config.token_label,
            session: None,
            identity_bundle: None,
            did: None,
            verifying_key: None,
            storage_id,
        })
    }

    /// Initialize a new keypair in the HSM
    ///
    /// This generates a new Ed25519 keypair within the HSM and creates
    /// an identity bundle with TLS binding and X25519 encryption keys.
    ///
    /// # Arguments
    /// * `pin` - HSM user PIN
    ///
    /// # Returns
    /// The generated identity bundle
    pub fn init(&mut self, pin: &[u8]) -> Result<IdentityBundle> {
        // Open session and login
        let session = self
            .pkcs11
            .open_rw_session(self.slot_id)
            .context("Failed to open PKCS#11 session")?;

        session
            .login(UserType::User, Some(&AuthPin::new(pin.to_vec())))
            .context("Failed to login to HSM")?;

        info!("Generating Ed25519 keypair in HSM...");

        // Generate Ed25519 keypair in HSM
        let mechanism = Mechanism::EccEdwardsKeyPairGen;

        // Public key template
        let pub_template = vec![
            Attribute::Token(true),
            Attribute::Verify(true),
            Attribute::Label(self.key_label.as_bytes().to_vec()),
            Attribute::EcParams(vec![
                0x06, 0x03, 0x2b, 0x65, 0x70, // OID for Ed25519: 1.3.101.112
            ]),
        ];

        // Private key template
        let priv_template = vec![
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sensitive(true),
            Attribute::Sign(true),
            Attribute::Extractable(false), // Key cannot be exported
            Attribute::Label(self.key_label.as_bytes().to_vec()),
        ];

        let (public_key_handle, private_key_handle) = session
            .generate_key_pair(&mechanism, &pub_template, &priv_template)
            .context("Failed to generate keypair in HSM")?;

        info!(
            "Generated Ed25519 keypair in HSM: pub={:?}, priv={:?}",
            public_key_handle, private_key_handle
        );

        // Retrieve public key
        let public_key_bytes = self.get_public_key(&session, public_key_handle)?;
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
            .context("Invalid Ed25519 public key generated by HSM")?;

        let did = Did::from_public_key(&verifying_key);

        // For HSM backend, we create a keypair that delegates signing to the HSM
        // The keypair doesn't contain the actual secret key
        let keypair = self.create_hsm_keypair(&verifying_key)?;

        // Generate TLS certificate and X25519 keys (not in HSM)
        let identity_bundle = IdentityBundle::from_keypair(keypair)?;

        // Cache the bundle
        self.session = Some(session);
        self.identity_bundle = Some(identity_bundle.clone());
        self.did = Some(did);
        self.verifying_key = Some(verifying_key);

        Ok(identity_bundle)
    }

    /// Get the public key from HSM
    fn get_public_key(&self, session: &Session, handle: ObjectHandle) -> Result<[u8; 32]> {
        let attributes = session
            .get_attributes(
                handle,
                &[AttributeType::EcPoint], // Ed25519 public key
            )
            .context("Failed to get public key from HSM")?;

        for attr in attributes {
            if let Attribute::EcPoint(data) = attr {
                // EC point is DER-encoded, extract the 32-byte key
                if data.len() >= 32 {
                    let key_start = data.len() - 32;
                    let key_bytes: [u8; 32] = data[key_start..]
                        .try_into()
                        .context("Invalid public key length")?;
                    return Ok(key_bytes);
                }
            }
        }

        anyhow::bail!("Public key not found in HSM object");
    }

    /// Create a keypair that delegates signing to the HSM
    ///
    /// This creates a KeyPair with a dummy secret key (all zeros).
    /// The actual signing is done by the HSM via the signing backend.
    fn create_hsm_keypair(&self, verifying_key: &VerifyingKey) -> Result<KeyPair> {
        // For HSM backend, we create a keypair with a dummy secret
        // The actual signing is delegated to the HSM
        let dummy_secret = [0u8; 32];
        KeyPair::from_bytes(&dummy_secret, &verifying_key.to_bytes())
    }

    /// Find key handles by label
    fn find_key_handles(&self, session: &Session) -> Result<(ObjectHandle, ObjectHandle)> {
        // Find public key
        let pub_template = vec![
            Attribute::Class(ObjectClass::PUBLIC_KEY),
            Attribute::Label(self.key_label.as_bytes().to_vec()),
        ];

        let public_keys = session
            .find_objects(&pub_template)
            .context("Failed to find public key")?;

        if public_keys.is_empty() {
            anyhow::bail!("Public key not found in HSM");
        }

        let public_key_handle = public_keys[0];

        // Find private key
        let priv_template = vec![
            Attribute::Class(ObjectClass::PRIVATE_KEY),
            Attribute::Label(self.key_label.as_bytes().to_vec()),
        ];

        let private_keys = session
            .find_objects(&priv_template)
            .context("Failed to find private key")?;

        if private_keys.is_empty() {
            anyhow::bail!("Private key not found in HSM");
        }

        let private_key_handle = private_keys[0];

        Ok((public_key_handle, private_key_handle))
    }
}

impl KeyStoreBackend for Pkcs11Backend {
    fn unlock(&mut self, credentials: &[u8]) -> Result<()> {
        if self.session.is_some() {
            warn!("PKCS#11 backend already unlocked");
            return Ok(());
        }

        // Open session and login
        let session = self
            .pkcs11
            .open_rw_session(self.slot_id)
            .context("Failed to open PKCS#11 session")?;

        session
            .login(UserType::User, Some(&AuthPin::new(credentials.to_vec())))
            .context("Failed to login to HSM")?;

        info!("Unlocked PKCS#11 backend");

        // Find existing keypair
        let (public_key_handle, _private_key_handle) = self.find_key_handles(&session)?;

        // Get public key
        let public_key_bytes = self.get_public_key(&session, public_key_handle)?;
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
            .context("Invalid Ed25519 public key in HSM")?;

        let did = Did::from_public_key(&verifying_key);

        // Create HSM-backed keypair
        let keypair = self.create_hsm_keypair(&verifying_key)?;

        // Load or generate identity bundle
        // For now, generate fresh TLS cert (in production, this should be persisted)
        let identity_bundle = IdentityBundle::from_keypair(keypair)?;

        // Cache state
        self.session = Some(session);
        self.identity_bundle = Some(identity_bundle);
        self.did = Some(did);
        self.verifying_key = Some(verifying_key);

        Ok(())
    }

    fn lock(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.logout();
            let _ = session.close();
            info!("Locked PKCS#11 backend");
        }

        self.identity_bundle = None;
        self.did = None;
        self.verifying_key = None;
    }

    fn is_locked(&self) -> bool {
        self.session.is_none()
    }

    fn get_identity_bundle(&self) -> Result<&IdentityBundle> {
        self.identity_bundle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("PKCS#11 backend is locked"))
    }

    fn path(&self) -> &Path {
        &self.storage_id
    }

    fn signing_backend(&self) -> Result<Box<dyn SigningBackend>> {
        if self.session.is_none() {
            anyhow::bail!("PKCS#11 backend is locked");
        }

        Ok(Box::new(Pkcs11SigningBackend {
            pkcs11: Arc::clone(&self.pkcs11),
            session: self.session.as_ref().unwrap().handle(),
            key_label: self.key_label.clone(),
            did: self.did.clone().unwrap(),
            verifying_key: self.verifying_key.unwrap(),
        }))
    }

    fn is_hardware_backed(&self) -> bool {
        true
    }

    fn backend_type(&self) -> &str {
        "pkcs11"
    }
}

/// PKCS#11 signing backend
///
/// Performs signing operations using the HSM without exposing the private key.
struct Pkcs11SigningBackend {
    pkcs11: Arc<Pkcs11>,
    session: cryptoki::session::SessionHandle,
    key_label: String,
    did: Did,
    verifying_key: VerifyingKey,
}

impl SigningBackend for Pkcs11SigningBackend {
    fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature> {
        // Reconstruct session from handle
        let session = Session::new(self.session, Arc::clone(&self.pkcs11));

        // Find private key
        let priv_template = vec![
            Attribute::Class(ObjectClass::PRIVATE_KEY),
            Attribute::Label(self.key_label.as_bytes().to_vec()),
        ];

        let private_keys = session
            .find_objects(&priv_template)
            .context("Failed to find private key")?;

        if private_keys.is_empty() {
            anyhow::bail!("Private key not found in HSM");
        }

        let private_key_handle = private_keys[0];

        // Sign using HSM
        let mechanism = Mechanism::Eddsa;
        let signature = session
            .sign(&mechanism, private_key_handle, message)
            .context("HSM signing operation failed")?;

        // Convert to ed25519_dalek::Signature
        if signature.len() != 64 {
            anyhow::bail!("Invalid signature length from HSM: {}", signature.len());
        }

        let sig_bytes: [u8; 64] = signature.try_into().unwrap();
        Ok(ed25519_dalek::Signature::from_bytes(&sig_bytes))
    }

    fn did(&self) -> &Did {
        &self.did
    }

    fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

impl Drop for Pkcs11Backend {
    fn drop(&mut self) {
        self.lock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require SoftHSM2 to be installed and configured
    // Run: softhsm2-util --init-token --slot 0 --label "test-token" --pin 1234 --so-pin 5678

    #[test]
    #[ignore] // Requires SoftHSM2
    fn test_pkcs11_init_and_sign() {
        let config = Pkcs11Config {
            library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
            slot_id: 0,
            key_label: "test-key".to_string(),
            token_label: Some("test-token".to_string()),
        };

        let mut backend = Pkcs11Backend::new(config).unwrap();
        let pin = b"1234";

        // Initialize
        let bundle = backend.init(pin).unwrap();
        assert!(!backend.is_locked());

        // Sign a message
        let message = b"test message";
        let signing_backend = backend.signing_backend().unwrap();
        let signature = signing_backend.sign(message).unwrap();

        // Verify signature
        use ed25519_dalek::Verifier;
        assert!(signing_backend
            .verifying_key()
            .verify(message, &signature)
            .is_ok());

        // Lock
        backend.lock();
        assert!(backend.is_locked());
    }
}
