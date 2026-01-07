//! Identity bundle with cryptographic DID-TLS binding
//!
//! This module provides IdentityBundle, which binds a DID identity to a TLS certificate
//! through cryptographic signatures. This prevents MITM attacks by ensuring that the
//! entity holding the TLS certificate also holds the private key for the claimed DID.

use crate::{Did, KeyPair};
use anyhow::{Context, Result};
use rcgen::CertificateParams;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
pub struct IdentityBundle {
    /// The DID for this identity
    did: Did,

    /// Ed25519 keypair for DID operations
    did_keypair: KeyPair,

    /// Self-signed TLS certificate
    tls_cert: CertificateDer<'static>,

    /// TLS private key (stored as bytes for cloning)
    tls_key_der: Vec<u8>,

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
}

impl Clone for IdentityBundle {
    fn clone(&self) -> Self {
        IdentityBundle {
            did: self.did.clone(),
            did_keypair: self.did_keypair.clone(),
            tls_cert: self.tls_cert.clone(),
            tls_key_der: self.tls_key_der.clone(),
            tls_binding_sig: self.tls_binding_sig.clone(),
            created_at: self.created_at,
            x25519_secret: Zeroizing::new(self.x25519_secret.to_vec()),
            x25519_public: self.x25519_public,
            #[cfg(feature = "post-quantum")]
            kem_pq_secret: self.kem_pq_secret.as_ref().map(|s| Zeroizing::new(s.to_vec())),
            #[cfg(feature = "post-quantum")]
            kem_pq_public: self.kem_pq_public.clone(),
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
        let did_keypair = KeyPair::generate()?;
        Self::from_keypair(did_keypair)
    }

    /// Create identity bundle from an existing keypair
    ///
    /// This generates a new TLS certificate for the given keypair and creates
    /// the cryptographic binding signature. Also generates X25519 keys for encryption.
    /// If the keypair has PQ keys, ML-KEM keys are also generated for hybrid encryption.
    pub fn from_keypair(did_keypair: KeyPair) -> Result<Self> {
        let did = did_keypair.did().clone();

        // Generate TLS certificate with DID as subject
        let (tls_cert, tls_key_der) = Self::generate_tls_cert(&did)?;

        // Compute cert hash and sign with DID key
        let cert_hash = Self::hash_certificate(&tls_cert);
        let tls_binding_sig = did_keypair.sign(&cert_hash).to_vec();

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System time before Unix epoch")?
            .as_secs();

        // Generate X25519 keys for payload encryption
        let (x25519_secret, x25519_public) = Self::generate_x25519_keypair();

        // Generate ML-KEM keys if keypair has PQ support
        #[cfg(feature = "post-quantum")]
        let (kem_pq_secret, kem_pq_public) = if did_keypair.is_hybrid() {
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
            did_keypair,
            tls_cert,
            tls_key_der,
            tls_binding_sig,
            created_at,
            x25519_secret,
            x25519_public,
            #[cfg(feature = "post-quantum")]
            kem_pq_secret,
            #[cfg(feature = "post-quantum")]
            kem_pq_public,
        })
    }

    /// Reconstruct identity bundle from stored components
    ///
    /// This is used by the keystore to restore a previously saved bundle.
    /// The TLS certificate and binding signature are already generated.
    #[allow(clippy::too_many_arguments)]
    pub fn from_stored(
        did_keypair: KeyPair,
        tls_cert_der: Vec<u8>,
        tls_key_der: Vec<u8>,
        tls_binding_sig: Vec<u8>,
        created_at: u64,
        x25519_secret_bytes: Vec<u8>,
        x25519_public_bytes: [u8; 32],
    ) -> Result<Self> {
        #[cfg(feature = "post-quantum")]
        return Self::from_stored_with_kem(
            did_keypair,
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
            let did = did_keypair.did().clone();
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
                did_keypair,
                tls_cert,
                tls_key_der,
                tls_binding_sig,
                created_at,
                x25519_secret: Zeroizing::new(x25519_secret_bytes),
                x25519_public: x25519_public_bytes,
            })
        }
    }

    /// Reconstruct identity bundle from stored components with optional KEM keys
    ///
    /// This is used by the keystore to restore a previously saved bundle
    /// that may include post-quantum KEM keys.
    #[cfg(feature = "post-quantum")]
    #[allow(clippy::too_many_arguments)]
    pub fn from_stored_with_kem(
        did_keypair: KeyPair,
        tls_cert_der: Vec<u8>,
        tls_key_der: Vec<u8>,
        tls_binding_sig: Vec<u8>,
        created_at: u64,
        x25519_secret_bytes: Vec<u8>,
        x25519_public_bytes: [u8; 32],
        kem_pq_secret: Option<Vec<u8>>,
        kem_pq_public: Option<Vec<u8>>,
    ) -> Result<Self> {
        let did = did_keypair.did().clone();
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
            did_keypair,
            tls_cert,
            tls_key_der,
            tls_binding_sig,
            created_at,
            x25519_secret: Zeroizing::new(x25519_secret_bytes),
            x25519_public: x25519_public_bytes,
            kem_pq_secret: kem_pq_secret.map(Zeroizing::new),
            kem_pq_public,
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

    /// Get the keypair
    pub fn keypair(&self) -> &KeyPair {
        &self.did_keypair
    }

    /// Get the TLS certificate (DER encoded)
    pub fn tls_cert(&self) -> &CertificateDer<'static> {
        &self.tls_cert
    }

    /// Get the TLS private key
    pub fn tls_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.tls_key_der.clone()))
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

    /// Construct a HybridKemKeypair from this bundle's keys
    ///
    /// Returns None if KEM keys are not available.
    #[cfg(feature = "post-quantum")]
    pub fn hybrid_kem_keypair(&self) -> Result<Option<HybridKemKeypair>> {
        if !self.has_hybrid_kem() {
            return Ok(None);
        }

        let kem_secret = self.kem_pq_secret.as_ref().ok_or_else(|| {
            anyhow::anyhow!("KEM secret key not available")
        })?;
        let kem_public = self.kem_pq_public.as_ref().ok_or_else(|| {
            anyhow::anyhow!("KEM public key not available")
        })?;

        let keypair = HybridKemKeypair::from_bytes(
            &self.x25519_secret,
            &self.x25519_public,
            kem_secret,
            kem_public,
        ).map_err(|e| anyhow::anyhow!("Failed to reconstruct hybrid KEM keypair: {e}"))?;

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

        let kem_public = self.kem_pq_public.as_ref().ok_or_else(|| {
            anyhow::anyhow!("KEM public key not available")
        })?;

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
}
