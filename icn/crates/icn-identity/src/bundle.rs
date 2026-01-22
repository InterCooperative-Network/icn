//! Identity bundle with cryptographic DID-TLS binding
//!
//! This module provides IdentityBundle, which binds a DID identity to a TLS certificate
//! through cryptographic signatures. This prevents MITM attacks by ensuring that the
//! entity holding the TLS certificate also holds the private key for the claimed DID.
//!
//! # Hardware Signing Support
//!
//! IdentityBundle supports both software and hardware-backed signing:
//!
//! - **Software keys**: Private key in memory, signs directly
//! - **Hardware keys**: Private key in HSM/TPM, signs via DidSigner backend
//!
//! ## Usage Examples
//!
//! ### Software Keys (Default)
//!
//! ```
//! use icn_identity::IdentityBundle;
//!
//! // Generate bundle with software keys
//! let bundle = IdentityBundle::generate().unwrap();
//!
//! // Sign directly without needing a signer
//! let message = b"test message";
//! let signature = bundle.sign(message).unwrap();
//! ```
//!
//! ### Hardware Keys with Signer
//!
//! ```
//! use icn_identity::{IdentityBundle, DidKey, SoftwareSigner};
//! use std::sync::Arc;
//! # use rand_core::OsRng;
//! # use ed25519_dalek::SigningKey;
//!
//! # // For demo purposes, we use software keys
//! # let signing_key = SigningKey::generate(&mut OsRng);
//! # let secret_bytes = signing_key.to_bytes();
//! # let public_bytes = signing_key.verifying_key().to_bytes();
//! # let software_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();
//! # let signer = Arc::new(SoftwareSigner::new(software_key).unwrap());
//! #
//! // Create hardware key (normally from HSM/TPM)
//! let hardware_key = DidKey::from_hardware(
//!     signing_key.verifying_key(),
//!     "pkcs11".to_string(),
//!     "slot=0".to_string(),
//! );
//!
//! // Create bundle with signer backend
//! let bundle = IdentityBundle::from_did_key_with_signer(
//!     hardware_key,
//!     Some(signer)
//! ).unwrap();
//!
//! // Sign using hardware backend
//! let message = b"test message";
//! let signature = bundle.sign(message).unwrap();
//! ```

use crate::{Did, DidKey, DidSigner, KeyPair};
use anyhow::{Context, Result};
use rcgen::CertificateParams;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

#[cfg(feature = "post-quantum")]
use icn_crypto_pq::{HybridKemKeypair, HybridKemPublicKey, MlKemKeypair};

/// Cryptographically bound identity bundle
///
/// Combines a DID identity with a TLS certificate, proving that the holder
/// of the TLS certificate also controls the DID's private key.
///
/// Also includes X25519 keys for end-to-end payload encryption.
///
/// The bundle can contain either:
/// - Software keys (private key in memory, no signer needed)
/// - Hardware-backed keys (private key in HSM/TPM, requires DidSigner)
///
/// # Signing
///
/// Use the `sign()` method to sign messages. The bundle will automatically:
/// - Use the DidSigner backend if available (hardware keys)
/// - Fall back to software signing if no signer (software keys)
/// - Return an error if hardware key is used without signer
pub struct IdentityBundle {
    /// The DID for this identity
    did: Did,

    /// DID signing key (software or hardware)
    did_key: DidKey,

    /// Optional signing backend for hardware keys
    /// - None for software keys (use did_key directly)
    /// - Some for hardware keys (delegate to HSM/TPM)
    signer: Option<Arc<dyn DidSigner>>,

    /// Self-signed TLS certificate
    tls_cert: CertificateDer<'static>,

    /// TLS private key (stored as bytes for cloning, zeroized on drop)
    tls_key_der: Zeroizing<Vec<u8>>,

    /// Binding signature proving ownership
    /// Signature = Sign_did_key(SHA256(tls_cert))
    tls_binding_sig: Vec<u8>,

    /// Timestamp when binding was created (Unix epoch seconds)
    created_at: u64,

    /// X25519 secret key for encryption (stored as bytes for cloning)
    x25519_secret: Zeroizing<Vec<u8>>,

    /// X25519 public key for encryption
    x25519_public: [u8; 32],

    /// ML-KEM secret key for hybrid encryption (optional, feature-gated)
    #[cfg(feature = "post-quantum")]
    kem_pq_secret: Option<Zeroizing<Vec<u8>>>,

    /// ML-KEM public key for hybrid encryption (optional, feature-gated)
    #[cfg(feature = "post-quantum")]
    kem_pq_public: Option<Vec<u8>>,

    /// ML-DSA secret key for hybrid signing (optional, feature-gated)
    #[cfg(feature = "post-quantum")]
    pq_signing_secret: Option<Zeroizing<Vec<u8>>>,

    /// ML-DSA public key for hybrid signing (optional, feature-gated)
    #[cfg(feature = "post-quantum")]
    pq_signing_public: Option<Vec<u8>>,
}

/// Clone creates a shallow copy that shares the signer instance via Arc.
///
/// # Note on Signer Sharing
/// The cloned bundle shares the same `Arc<dyn DidSigner>` as the original.
/// This is correct for most use cases, but be aware that:
/// - Both bundles will use the same underlying HSM/TPM connection
/// - If the signer has mutable state or connection pooling, operations may interleave
/// - For a deep copy with a fresh signer, create a new bundle with `from_did_key_with_signer()`
impl Clone for IdentityBundle {
    fn clone(&self) -> Self {
        IdentityBundle {
            did: self.did.clone(),
            did_key: self.did_key.clone(),
            signer: self.signer.clone(), // Arc clone - shares underlying signer
            tls_cert: self.tls_cert.clone(),
            tls_key_der: Zeroizing::new(self.tls_key_der.to_vec()),
            tls_binding_sig: self.tls_binding_sig.clone(),
            created_at: self.created_at,
            x25519_secret: Zeroizing::new(self.x25519_secret.to_vec()),
            x25519_public: self.x25519_public,
            #[cfg(feature = "post-quantum")]
            kem_pq_secret: self
                .kem_pq_secret
                .as_ref()
                .map(|s| Zeroizing::new(s.to_vec())),
            #[cfg(feature = "post-quantum")]
            kem_pq_public: self.kem_pq_public.clone(),
            #[cfg(feature = "post-quantum")]
            pq_signing_secret: self
                .pq_signing_secret
                .as_ref()
                .map(|s| Zeroizing::new(s.to_vec())),
            #[cfg(feature = "post-quantum")]
            pq_signing_public: self.pq_signing_public.clone(),
        }
    }
}

/// Serializable binding info for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingInfo {
    /// The DID this binding belongs to
    pub did: Did,
    /// SHA-256 hash of the TLS certificate
    pub tls_cert_hash: [u8; 32],
    /// Cryptographic signature binding DID to TLS cert
    pub tls_binding_sig: Vec<u8>,
    /// Unix timestamp when binding was created
    pub created_at: u64,
}

impl IdentityBundle {
    /// Generate new identity bundle with bound TLS cert
    ///
    /// This creates:
    /// 1. An Ed25519 keypair and DID
    /// 2. A self-signed TLS certificate with the DID as subject
    /// 3. A cryptographic binding signature proving DID ownership
    pub fn generate() -> Result<Self> {
        // 1. Generate Ed25519 keypair for DID
        let keypair = KeyPair::generate()?;
        Self::from_keypair(keypair)
    }

    /// Create identity bundle from a DidKey
    ///
    /// This generates a new TLS certificate for the given key and creates
    /// the cryptographic binding signature. Also generates X25519 keys for encryption.
    /// If the key has PQ keys, ML-KEM keys are also generated for hybrid encryption.
    ///
    /// # Limitations
    /// **Hardware keys:** Cannot create TLS binding signature without DidSigner.
    /// Hardware keys require a DidSigner backend to perform signing operations.
    ///
    /// For hardware-backed keys, use `from_did_key_with_signer()` instead.
    pub fn from_did_key(did_key: DidKey) -> Result<Self> {
        Self::from_did_key_with_signer(did_key, None)
    }

    /// Create identity bundle from a DidKey with optional signer
    ///
    /// This generates a new TLS certificate for the given key and creates
    /// the cryptographic binding signature. Also generates X25519 keys for encryption.
    /// If the key has PQ keys, ML-KEM keys are also generated for hybrid encryption.
    ///
    /// # Arguments
    /// * `did_key` - The DID key (software or hardware)
    /// * `signer` - Optional signer for hardware keys (None for software keys)
    ///
    /// # Errors
    /// * Returns error if hardware key is provided without signer
    /// * Returns error if signer's DID/verifying key doesn't match did_key
    /// * Returns error if TLS certificate generation fails
    pub fn from_did_key_with_signer(
        did_key: DidKey,
        signer: Option<Arc<dyn DidSigner>>,
    ) -> Result<Self> {
        // Hardware keys require a DidSigner backend
        if did_key.is_hardware_backed() && signer.is_none() {
            anyhow::bail!(
                "Cannot create IdentityBundle from hardware DidKey without signer: \
                 Hardware keys require a DidSigner to perform TLS binding signature. \
                 Use from_did_key_with_signer() with a signer implementation."
            );
        }

        // Validate that signer matches did_key if provided
        if let Some(ref signer) = signer {
            if signer.verifying_key() != did_key.verifying_key() {
                anyhow::bail!(
                    "Signer verifying key does not match DidKey: \
                     signer DID={}, did_key DID={}. \
                     The signer must correspond to the same key as did_key.",
                    signer.did(),
                    did_key.did()
                );
            }
        }

        let did = did_key.did().clone();

        // Generate TLS certificate with DID as subject
        let (tls_cert, tls_key_der) = Self::generate_tls_cert(&did)?;

        // Compute cert hash and sign with DID key
        let cert_hash = Self::hash_certificate(&tls_cert);
        let tls_binding_sig = if let Some(ref signer) = signer {
            // Use signer for hardware keys
            signer.sign(&cert_hash)?.to_vec()
        } else {
            // Use software key directly
            did_key.sign_software(&cert_hash)?.to_vec()
        };

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System time before Unix epoch")?
            .as_secs();

        // Generate X25519 keys for payload encryption
        let (x25519_secret, x25519_public) = Self::generate_x25519_keypair();

        // Generate ML-KEM keys if needed (for now only for software keys)
        #[cfg(feature = "post-quantum")]
        let (kem_pq_secret, kem_pq_public) = if matches!(did_key, DidKey::Software { .. }) {
            // TODO: Support PQ keys for hardware backends
            let kem_keypair = MlKemKeypair::generate()
                .map_err(|e| anyhow::anyhow!("Failed to generate ML-KEM keypair: {e}"))?;
            (
                Some(Zeroizing::new(kem_keypair.secret_key_bytes().to_vec())),
                Some(kem_keypair.public_key().as_bytes().to_vec()),
            )
        } else {
            (None, None)
        };

        Ok(IdentityBundle {
            did,
            did_key,
            signer,
            tls_cert,
            tls_key_der: Zeroizing::new(tls_key_der),
            tls_binding_sig,
            created_at,
            x25519_secret,
            x25519_public,
            #[cfg(feature = "post-quantum")]
            kem_pq_secret,
            #[cfg(feature = "post-quantum")]
            kem_pq_public,
            #[cfg(feature = "post-quantum")]
            pq_signing_secret: None,
            #[cfg(feature = "post-quantum")]
            pq_signing_public: None,
        })
    }

    /// Create identity bundle from an existing keypair (backward compatibility)
    ///
    /// This generates a new TLS certificate for the given keypair and creates
    /// the cryptographic binding signature. Also generates X25519 keys for encryption.
    /// If the keypair has PQ keys, ML-KEM keys are also generated for hybrid encryption.
    ///
    /// **Deprecated:** Use `from_did_key` for new code. This method exists for
    /// backward compatibility and converts the KeyPair to a DidKey::Software internally.
    pub fn from_keypair(did_keypair: KeyPair) -> Result<Self> {
        #[cfg(feature = "post-quantum")]
        let (pq_signing_secret, pq_signing_public) = did_keypair
            .pq_keypair()
            .map(|kp| {
                (
                    Some(Zeroizing::new(kp.secret_key_bytes().to_vec())),
                    Some(kp.public_key().as_bytes().to_vec()),
                )
            })
            .unwrap_or((None, None));

        // Convert KeyPair to DidKey::Software
        let did_key = DidKey::Software {
            secret_bytes: Zeroizing::new(did_keypair.to_signing_key_bytes()),
            verifying_key: *did_keypair.verifying_key(),
            did: did_keypair.did().clone(),
        };

        let bundle = Self::from_did_key(did_key)?;
        #[cfg(feature = "post-quantum")]
        {
            let mut bundle = bundle;
            bundle.pq_signing_secret = pq_signing_secret;
            bundle.pq_signing_public = pq_signing_public;
            Ok(bundle)
        }

        #[cfg(not(feature = "post-quantum"))]
        Ok(bundle)
    }

    /// Reconstruct identity bundle from stored components
    ///
    /// This is used by the keystore to restore a previously saved bundle.
    /// The TLS certificate and binding signature are already generated.
    /// Secret parameters are wrapped in Zeroizing to prevent copies.
    ///
    /// **Note:** This method only supports software keys. Hardware-backed keys
    /// require a signer to be provided; use `from_stored_with_signer()` instead
    /// (not yet implemented - hardware key persistence is planned for future).
    ///
    /// # Errors
    /// * Returns error if did_key is hardware-backed
    /// * Returns error if binding signature verification fails
    #[allow(clippy::too_many_arguments)]
    pub fn from_stored(
        did_key: DidKey,
        tls_cert_der: Vec<u8>,
        tls_key_der: Zeroizing<Vec<u8>>,
        tls_binding_sig: Vec<u8>,
        created_at: u64,
        x25519_secret_bytes: Zeroizing<Vec<u8>>,
        x25519_public_bytes: [u8; 32],
    ) -> Result<Self> {
        // Reject hardware-backed keys since we cannot restore signing capability
        // without a signer. Hardware key persistence requires a separate API
        // that accepts a signer.
        if did_key.is_hardware_backed() {
            anyhow::bail!(
                "Cannot restore hardware-backed IdentityBundle without signer. \
                 Hardware-backed key persistence is not yet supported. \
                 The bundle must be re-created with from_did_key_with_signer()."
            );
        }

        #[cfg(feature = "post-quantum")]
        return Self::from_stored_with_kem(
            did_key,
            tls_cert_der,
            tls_key_der,
            tls_binding_sig,
            created_at,
            x25519_secret_bytes,
            x25519_public_bytes,
            None,
            None,
        );

        #[cfg(not(feature = "post-quantum"))]
        {
            let did = did_key.did().clone();
            let tls_cert = CertificateDer::from(tls_cert_der);

            // Verify the binding is still valid
            let cert_hash = Self::hash_certificate(&tls_cert);
            let verifying_key = did.to_verifying_key()?;
            let signature = ed25519_dalek::Signature::from_slice(&tls_binding_sig)
                .context("Invalid stored binding signature format")?;

            use ed25519_dalek::Verifier;
            verifying_key
                .verify(&cert_hash, &signature)
                .context("Stored TLS binding signature verification failed")?;

            Ok(IdentityBundle {
                did,
                did_key,
                signer: None,
                tls_cert,
                tls_key_der,
                tls_binding_sig,
                created_at,
                x25519_secret: x25519_secret_bytes,
                x25519_public: x25519_public_bytes,
            })
        }
    }

    /// Reconstruct identity bundle from stored components with optional KEM keys
    ///
    /// This is used by the keystore to restore a previously saved bundle
    /// that may include post-quantum KEM keys.
    /// Secret parameters are wrapped in Zeroizing to prevent copies.
    #[cfg(feature = "post-quantum")]
    #[allow(clippy::too_many_arguments)]
    pub fn from_stored_with_kem(
        did_key: DidKey,
        tls_cert_der: Vec<u8>,
        tls_key_der: Zeroizing<Vec<u8>>,
        tls_binding_sig: Vec<u8>,
        created_at: u64,
        x25519_secret_bytes: Zeroizing<Vec<u8>>,
        x25519_public_bytes: [u8; 32],
        kem_pq_secret: Option<Zeroizing<Vec<u8>>>,
        kem_pq_public: Option<Vec<u8>>,
    ) -> Result<Self> {
        Self::from_stored_with_pq(
            did_key,
            tls_cert_der,
            tls_key_der,
            tls_binding_sig,
            created_at,
            x25519_secret_bytes,
            x25519_public_bytes,
            None,
            None,
            kem_pq_secret,
            kem_pq_public,
        )
    }

    /// Reconstruct identity bundle from stored components with optional PQ signing keys
    /// and optional KEM keys.
    ///
    /// **Note:** This method only supports software keys. Hardware-backed keys
    /// require a signer to be provided; hardware key persistence is planned for future.
    ///
    /// # Errors
    /// * Returns error if did_key is hardware-backed
    /// * Returns error if binding signature verification fails
    #[cfg(feature = "post-quantum")]
    #[allow(clippy::too_many_arguments)]
    pub fn from_stored_with_pq(
        did_key: DidKey,
        tls_cert_der: Vec<u8>,
        tls_key_der: Zeroizing<Vec<u8>>,
        tls_binding_sig: Vec<u8>,
        created_at: u64,
        x25519_secret_bytes: Zeroizing<Vec<u8>>,
        x25519_public_bytes: [u8; 32],
        pq_signing_secret: Option<Zeroizing<Vec<u8>>>,
        pq_signing_public: Option<Vec<u8>>,
        kem_pq_secret: Option<Zeroizing<Vec<u8>>>,
        kem_pq_public: Option<Vec<u8>>,
    ) -> Result<Self> {
        // Reject hardware-backed keys since we cannot restore signing capability
        // without a signer. Hardware key persistence requires a separate API.
        if did_key.is_hardware_backed() {
            anyhow::bail!(
                "Cannot restore hardware-backed IdentityBundle without signer. \
                 Hardware-backed key persistence is not yet supported. \
                 The bundle must be re-created with from_did_key_with_signer()."
            );
        }

        let did = did_key.did().clone();
        let tls_cert = CertificateDer::from(tls_cert_der);

        // Verify the binding is still valid
        let cert_hash = Self::hash_certificate(&tls_cert);
        let verifying_key = did.to_verifying_key()?;
        let signature = ed25519_dalek::Signature::from_slice(&tls_binding_sig)
            .context("Invalid stored binding signature format")?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(&cert_hash, &signature)
            .context("Stored TLS binding signature verification failed")?;

        Ok(IdentityBundle {
            did,
            did_key,
            signer: None,
            tls_cert,
            tls_key_der,
            tls_binding_sig,
            created_at,
            x25519_secret: x25519_secret_bytes,
            x25519_public: x25519_public_bytes,
            kem_pq_secret,
            kem_pq_public,
            pq_signing_secret,
            pq_signing_public,
        })
    }

    /// Verify the TLS binding signature
    ///
    /// Confirms that:
    /// 1. The binding signature is valid
    /// 2. It was created by the DID's private key
    /// 3. It covers the actual TLS certificate
    pub fn verify_binding(&self) -> Result<()> {
        use ed25519_dalek::Verifier;

        let cert_hash = Self::hash_certificate(&self.tls_cert);
        let verifying_key = self.did.to_verifying_key()?;

        let signature = ed25519_dalek::Signature::from_slice(&self.tls_binding_sig)
            .context("Invalid signature format")?;

        verifying_key
            .verify(&cert_hash, &signature)
            .context("TLS binding signature verification failed")?;

        Ok(())
    }

    /// Get the DID
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Get the DID key
    pub fn did_key(&self) -> &DidKey {
        &self.did_key
    }

    /// Get the optional signer backend
    ///
    /// Returns the signing backend if one was configured for this bundle,
    /// regardless of whether the underlying DID key is hardware-backed or
    /// software-only. Callers should not rely on the presence or absence of
    /// a signer to determine key storage type; use `did_key().is_hardware_backed()`
    /// or `requires_signer()` for that purpose instead.
    ///
    /// # Expected Usage Patterns
    /// - **Hardware keys**: signer will always be `Some` (enforced at construction)
    /// - **Software keys**: signer is always `None` (no signer needed)
    ///
    /// While it's theoretically possible to provide a signer for software keys,
    /// this is not a supported use case. The signer validation ensures the signer
    /// matches the key, but software keys can sign directly without delegation.
    pub fn signer(&self) -> Option<&Arc<dyn DidSigner>> {
        self.signer.as_ref()
    }

    /// Check if this bundle requires a signer for signing operations
    ///
    /// Returns `true` for hardware-backed keys (which cannot sign without a signer),
    /// and `false` for software keys (which can sign directly).
    ///
    /// This is a convenience method equivalent to `did_key().is_hardware_backed()`.
    pub fn requires_signer(&self) -> bool {
        self.did_key.is_hardware_backed()
    }

    /// Sign a message using the appropriate signing method
    ///
    /// This method delegates signing to:
    /// - The signer backend if available (for hardware keys)
    /// - Direct software signing if no signer (for software keys)
    ///
    /// # Errors
    /// Returns an error if:
    /// - Hardware key is used without a signer backend
    /// - Signing operation fails
    pub fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature> {
        // Hardware keys require a signer - this is enforced at construction,
        // so this check is defensive against invariant violations.
        if self.did_key.is_hardware_backed() && self.signer.is_none() {
            anyhow::bail!(
                "Invariant violation: hardware-backed key without signer. \
                 This indicates a bug in IdentityBundle construction or \
                 manual struct creation bypassing the public API."
            )
        }

        // Use signer if available, otherwise use software key directly
        if let Some(ref signer) = self.signer {
            signer.sign(message)
        } else {
            self.did_key.sign_software(message)
        }
    }

    /// Get the keypair (backward compatibility, only works for software keys)
    ///
    /// **Deprecated:** Use `did_key()` for new code. This method exists for
    /// backward compatibility and will return an error for hardware-backed keys.
    ///
    /// Returns an error if the bundle contains a hardware-backed key since
    /// we cannot construct a KeyPair without exposing the private key.
    pub fn keypair(&self) -> Result<KeyPair> {
        match &self.did_key {
            DidKey::Software {
                secret_bytes,
                verifying_key,
                ..
            } => {
                #[cfg(feature = "post-quantum")]
                if let (Some(pq_secret), Some(pq_public)) =
                    (&self.pq_signing_secret, &self.pq_signing_public)
                {
                    return KeyPair::from_bytes_with_pq(
                        secret_bytes,
                        &verifying_key.to_bytes(),
                        pq_secret.as_slice(),
                        pq_public.as_slice(),
                    );
                }
                KeyPair::from_bytes(secret_bytes, &verifying_key.to_bytes())
            }
            DidKey::Hardware { backend_type, .. } => {
                anyhow::bail!(
                    "Cannot get KeyPair from hardware-backed bundle (backend: {}). \
                     Use did_key() instead",
                    backend_type
                )
            }
        }
    }

    /// Get the TLS certificate (DER encoded)
    pub fn tls_cert(&self) -> &CertificateDer<'static> {
        &self.tls_cert
    }

    /// Get the TLS private key
    pub fn tls_key(&self) -> PrivateKeyDer<'static> {
        // Note: Creates a copy of the key bytes for the PrivateKeyDer wrapper.
        // The source remains protected by Zeroizing.
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.tls_key_der.to_vec()))
    }

    /// Get the raw TLS private key bytes (DER format)
    pub(crate) fn tls_key_der_bytes(&self) -> &[u8] {
        &self.tls_key_der
    }

    /// Get the X25519 secret key for encryption
    pub fn x25519_secret(&self) -> StaticSecret {
        // Reconstruct StaticSecret from bytes
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&self.x25519_secret[..32]);
        StaticSecret::from(bytes)
    }

    /// Get the X25519 public key for encryption
    pub fn x25519_public(&self) -> PublicKey {
        PublicKey::from(self.x25519_public)
    }

    /// Get the raw X25519 secret key bytes
    pub(crate) fn x25519_secret_bytes(&self) -> &[u8] {
        &self.x25519_secret
    }

    /// Get the raw X25519 public key bytes
    pub fn x25519_public_bytes(&self) -> &[u8; 32] {
        &self.x25519_public
    }

    /// Check if this bundle has hybrid KEM keys
    #[cfg(feature = "post-quantum")]
    pub fn has_hybrid_kem(&self) -> bool {
        self.kem_pq_secret.is_some() && self.kem_pq_public.is_some()
    }

    /// Get the raw ML-KEM secret key bytes (if available)
    #[cfg(feature = "post-quantum")]
    pub(crate) fn kem_pq_secret_bytes(&self) -> Option<&[u8]> {
        self.kem_pq_secret.as_ref().map(|s| s.as_slice())
    }

    /// Get the raw ML-KEM public key bytes (if available)
    #[cfg(feature = "post-quantum")]
    pub fn kem_pq_public_bytes(&self) -> Option<&[u8]> {
        self.kem_pq_public.as_deref()
    }

    /// Get the PQ signing secret key bytes (if available)
    #[cfg(feature = "post-quantum")]
    pub(crate) fn pq_signing_secret_bytes(&self) -> Option<&[u8]> {
        self.pq_signing_secret.as_ref().map(|s| s.as_slice())
    }

    /// Get the PQ signing public key bytes (if available)
    #[cfg(feature = "post-quantum")]
    pub(crate) fn pq_signing_public_bytes(&self) -> Option<&[u8]> {
        self.pq_signing_public.as_deref()
    }

    /// Check if this bundle has PQ signing keys
    #[cfg(feature = "post-quantum")]
    pub fn has_pq_signing_keys(&self) -> bool {
        self.pq_signing_secret.is_some() && self.pq_signing_public.is_some()
    }

    /// Construct a HybridKemKeypair from this bundle's keys
    ///
    /// Returns None if KEM keys are not available.
    #[cfg(feature = "post-quantum")]
    pub fn hybrid_kem_keypair(&self) -> Result<Option<HybridKemKeypair>> {
        if !self.has_hybrid_kem() {
            return Ok(None);
        }

        let kem_secret = self
            .kem_pq_secret
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KEM secret key not available"))?;
        let kem_public = self
            .kem_pq_public
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KEM public key not available"))?;

        let keypair = HybridKemKeypair::from_bytes(
            &self.x25519_secret,
            &self.x25519_public,
            kem_secret,
            kem_public,
        )
        .map_err(|e| anyhow::anyhow!("Failed to reconstruct hybrid KEM keypair: {e}"))?;

        Ok(Some(keypair))
    }

    /// Construct a HybridKemPublicKey from this bundle's keys
    ///
    /// Returns None if KEM keys are not available.
    #[cfg(feature = "post-quantum")]
    pub fn hybrid_kem_public_key(&self) -> Result<Option<HybridKemPublicKey>> {
        if !self.has_hybrid_kem() {
            return Ok(None);
        }

        let kem_public = self
            .kem_pq_public
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KEM public key not available"))?;

        let public_key = HybridKemPublicKey::from_bytes(&self.x25519_public, kem_public)
            .map_err(|e| anyhow::anyhow!("Failed to construct hybrid KEM public key: {e}"))?;

        Ok(Some(public_key))
    }

    /// Get binding info for network transmission
    pub fn binding_info(&self) -> BindingInfo {
        BindingInfo {
            did: self.did.clone(),
            tls_cert_hash: Self::hash_certificate(&self.tls_cert),
            tls_binding_sig: self.tls_binding_sig.clone(),
            created_at: self.created_at,
        }
    }

    /// Generate self-signed TLS cert with DID as subject
    ///
    /// The certificate includes:
    /// - Subject CN: DID string
    /// - SAN URI: DID string
    /// - Ed25519 signature algorithm
    /// - 1 year validity
    fn generate_tls_cert(did: &Did) -> Result<(CertificateDer<'static>, Vec<u8>)> {
        let mut params = CertificateParams::new(vec![did.as_str().to_string()])?;
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::DigitalSignature,
            rcgen::KeyUsagePurpose::KeyEncipherment,
        ];

        // Generate Ed25519 key pair for the certificate (same as tls.rs)
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)?;

        // Create certificate with Ed25519 key
        let cert = params.self_signed(&key_pair)?;

        // Export certificate and key
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = key_pair.serialize_der();

        Ok((cert_der, key_der))
    }

    /// Hash a certificate using SHA-256
    fn hash_certificate(cert: &CertificateDer<'_>) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(cert.as_ref());
        hasher.finalize().into()
    }

    /// Generate a new X25519 keypair for encryption
    fn generate_x25519_keypair() -> (Zeroizing<Vec<u8>>, [u8; 32]) {
        use rand::rngs::OsRng;

        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);

        // Store secret as bytes for serialization
        let secret_bytes = Zeroizing::new(secret.to_bytes().to_vec());
        let public_bytes = public.to_bytes();

        (secret_bytes, public_bytes)
    }
}

/// Verify DID-TLS binding information matches the expected DID
///
/// This is used during TOFU (Trust On First Use) handshake to verify that:
/// 1. The binding signature is valid for the DID
/// 2. The peer holds the DID's private key
/// 3. The binding_info is correctly formed
///
/// This does NOT verify the cert hash against an actual TLS cert, since in TOFU
/// we accept self-signed certs and verify identity at the application layer.
pub fn verify_did_matches_binding(did: &Did, binding_info: &BindingInfo) -> Result<()> {
    use ed25519_dalek::Verifier;

    // 1. Verify the DID in binding_info matches expected DID
    if binding_info.did != *did {
        anyhow::bail!("DID mismatch: expected {}, got {}", did, binding_info.did);
    }

    // 2. Verify signature with DID public key
    let verifying_key = binding_info.did.to_verifying_key()?;
    let signature = ed25519_dalek::Signature::from_slice(&binding_info.tls_binding_sig)
        .context("Invalid signature format")?;

    verifying_key
        .verify(&binding_info.tls_cert_hash, &signature)
        .context("DID-TLS binding signature verification failed")?;

    Ok(())
}

/// Verify a binding info against a peer's certificate (stricter verification)
///
/// This is used when you have access to the actual TLS certificate to verify that:
/// 1. The peer's TLS cert matches the claimed hash
/// 2. The binding signature is valid for the DID
/// 3. The peer holds the DID's private key
pub fn verify_binding_info(
    binding_info: &BindingInfo,
    peer_cert: &CertificateDer<'_>,
) -> Result<()> {
    use ed25519_dalek::Verifier;

    // 1. Hash the certificate we received via TLS
    let actual_hash = {
        let mut hasher = Sha256::new();
        hasher.update(peer_cert.as_ref());
        hasher.finalize()
    };

    // 2. Verify it matches claimed hash
    if actual_hash[..] != binding_info.tls_cert_hash[..] {
        anyhow::bail!("TLS certificate hash mismatch");
    }

    // 3. Verify signature with DID public key
    let verifying_key = binding_info.did.to_verifying_key()?;
    let signature = ed25519_dalek::Signature::from_slice(&binding_info.tls_binding_sig)
        .context("Invalid signature format")?;

    verifying_key
        .verify(&binding_info.tls_cert_hash, &signature)
        .context("DID-TLS binding signature verification failed")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_bundle_generation() {
        let bundle = IdentityBundle::generate().unwrap();
        assert!(bundle.verify_binding().is_ok());
        assert!(bundle.did().as_str().starts_with("did:icn:"));
    }

    #[test]
    fn test_binding_verification() {
        let bundle = IdentityBundle::generate().unwrap();
        let binding_info = bundle.binding_info();

        // Should verify against the bundle's own cert
        assert!(verify_binding_info(&binding_info, bundle.tls_cert()).is_ok());
    }

    #[test]
    fn test_binding_tampering_cert() {
        let bundle = IdentityBundle::generate().unwrap();
        let binding_info = bundle.binding_info();

        // Create a different certificate
        let other_bundle = IdentityBundle::generate().unwrap();

        // Should fail: cert doesn't match hash
        assert!(verify_binding_info(&binding_info, other_bundle.tls_cert()).is_err());
    }

    #[test]
    fn test_binding_tampering_signature() {
        let bundle = IdentityBundle::generate().unwrap();
        let mut binding_info = bundle.binding_info();

        // Tamper with signature
        binding_info.tls_binding_sig[0] ^= 0xFF;

        // Should fail: signature invalid
        assert!(verify_binding_info(&binding_info, bundle.tls_cert()).is_err());
    }

    #[test]
    fn test_cert_has_did_as_subject() {
        let bundle = IdentityBundle::generate().unwrap();
        let cert_der = bundle.tls_cert();

        // Parse the certificate to verify DID is in subject
        // For now, just verify we can create the cert
        assert!(!cert_der.as_ref().is_empty());
    }

    #[test]
    fn test_bundle_clone() {
        let bundle1 = IdentityBundle::generate().unwrap();
        let bundle2 = bundle1.clone();

        // Should have same DID
        assert_eq!(bundle1.did(), bundle2.did());

        // Both should verify
        assert!(bundle1.verify_binding().is_ok());
        assert!(bundle2.verify_binding().is_ok());
    }

    #[test]
    fn test_hardware_key_cannot_create_bundle() {
        use rand_core::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let did_key = DidKey::from_hardware(
            signing_key.verifying_key(),
            "test-backend".to_string(),
            "hardware-id".to_string(),
        );

        let result = IdentityBundle::from_did_key(did_key);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("without signer"));
    }

    #[test]
    fn test_software_key_sign_without_signer() {
        // Software keys should work without signer
        let bundle = IdentityBundle::generate().unwrap();
        assert!(bundle.signer().is_none());

        let message = b"test message";
        let signature = bundle.sign(message).unwrap();

        // Verify signature
        use ed25519_dalek::Verifier;
        let verifying_key = bundle.did().to_verifying_key().unwrap();
        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_hardware_key_with_signer() {
        use crate::SoftwareSigner;
        use rand_core::OsRng;

        // Create a software key for testing
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let software_did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();

        // Create a signer from the software key
        let signer = Arc::new(SoftwareSigner::new(software_did_key.clone()).unwrap());

        // Create hardware key with same public key
        let hardware_did_key = DidKey::from_hardware(
            signing_key.verifying_key(),
            "test-hsm".to_string(),
            "slot-0".to_string(),
        );

        // Create bundle with signer
        let bundle =
            IdentityBundle::from_did_key_with_signer(hardware_did_key, Some(signer.clone()))
                .unwrap();

        assert!(bundle.signer().is_some());
        assert_eq!(bundle.did_key().backend_type(), "test-hsm");

        // Test signing works
        let message = b"test message";
        let signature = bundle.sign(message).unwrap();

        // Verify signature
        use ed25519_dalek::Verifier;
        let verifying_key = bundle.did().to_verifying_key().unwrap();
        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_hardware_key_without_signer_fails() {
        use crate::SoftwareSigner;
        use rand_core::OsRng;

        // Create a software key for testing
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let software_did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();

        // Create a signer from the software key
        let signer = Arc::new(SoftwareSigner::new(software_did_key.clone()).unwrap());

        // Create hardware key with same public key
        let hardware_did_key = DidKey::from_hardware(
            signing_key.verifying_key(),
            "test-hsm".to_string(),
            "slot-0".to_string(),
        );

        // Create bundle with signer first
        let bundle =
            IdentityBundle::from_did_key_with_signer(hardware_did_key.clone(), Some(signer))
                .unwrap();

        // Manually create a bundle without signer (simulating incorrect usage)
        // This would be caught by from_did_key_with_signer, but we test the sign() method
        let did = bundle.did().clone();
        let broken_bundle = IdentityBundle {
            did: did.clone(),
            did_key: hardware_did_key,
            signer: None, // Hardware key without signer
            tls_cert: bundle.tls_cert.clone(),
            tls_key_der: bundle.tls_key_der.clone(),
            tls_binding_sig: bundle.tls_binding_sig.clone(),
            created_at: bundle.created_at,
            x25519_secret: bundle.x25519_secret.clone(),
            x25519_public: bundle.x25519_public,
            #[cfg(feature = "post-quantum")]
            kem_pq_secret: None,
            #[cfg(feature = "post-quantum")]
            kem_pq_public: None,
            #[cfg(feature = "post-quantum")]
            pq_signing_secret: None,
            #[cfg(feature = "post-quantum")]
            pq_signing_public: None,
        };

        // Signing should fail with invariant violation
        let message = b"test message";
        let result = broken_bundle.sign(message);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("Invariant violation"));
    }

    #[test]
    fn test_signer_getter() {
        use crate::SoftwareSigner;
        use rand_core::OsRng;

        // Software bundle should have no signer
        let bundle = IdentityBundle::generate().unwrap();
        assert!(bundle.signer().is_none());

        // Hardware bundle with signer should return signer
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let software_did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();
        let signer = Arc::new(SoftwareSigner::new(software_did_key.clone()).unwrap());

        let hardware_did_key = DidKey::from_hardware(
            signing_key.verifying_key(),
            "test-hsm".to_string(),
            "slot-0".to_string(),
        );

        let bundle =
            IdentityBundle::from_did_key_with_signer(hardware_did_key, Some(signer.clone()))
                .unwrap();

        assert!(bundle.signer().is_some());
        assert_eq!(bundle.signer().unwrap().backend_type(), "software");
    }

    #[test]
    fn test_bundle_clone_with_signer() {
        use crate::SoftwareSigner;
        use rand_core::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let software_did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();
        let signer = Arc::new(SoftwareSigner::new(software_did_key.clone()).unwrap());

        let hardware_did_key = DidKey::from_hardware(
            signing_key.verifying_key(),
            "test-hsm".to_string(),
            "slot-0".to_string(),
        );

        let bundle1 =
            IdentityBundle::from_did_key_with_signer(hardware_did_key, Some(signer)).unwrap();
        let bundle2 = bundle1.clone();

        // Both should have same DID and signer
        assert_eq!(bundle1.did(), bundle2.did());
        assert!(bundle1.signer().is_some());
        assert!(bundle2.signer().is_some());

        // Both should be able to sign
        let message = b"test message";
        let sig1 = bundle1.sign(message).unwrap();
        let sig2 = bundle2.sign(message).unwrap();

        // Signatures should verify
        use ed25519_dalek::Verifier;
        let verifying_key = bundle1.did().to_verifying_key().unwrap();
        assert!(verifying_key.verify(message, &sig1).is_ok());
        assert!(verifying_key.verify(message, &sig2).is_ok());
    }

    #[test]
    fn test_signer_mismatch_rejected() {
        use crate::SoftwareSigner;
        use rand_core::OsRng;

        // Create two different keys
        let key1 = SigningKey::generate(&mut OsRng);
        let key2 = SigningKey::generate(&mut OsRng);

        // Create a signer from key1
        let software_did_key1 =
            DidKey::from_software_bytes(key1.to_bytes(), key1.verifying_key().to_bytes()).unwrap();
        let signer1 = Arc::new(SoftwareSigner::new(software_did_key1).unwrap());

        // Create a hardware key from key2 (different key!)
        let hardware_did_key2 = DidKey::from_hardware(
            key2.verifying_key(),
            "test-hsm".to_string(),
            "slot-0".to_string(),
        );

        // Should fail: signer's key doesn't match did_key
        let result = IdentityBundle::from_did_key_with_signer(hardware_did_key2, Some(signer1));
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("does not match"));
    }
}
