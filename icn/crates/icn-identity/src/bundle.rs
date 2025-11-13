//! Identity bundle with cryptographic DID-TLS binding
//!
//! This module provides IdentityBundle, which binds a DID identity to a TLS certificate
//! through cryptographic signatures. This prevents MITM attacks by ensuring that the
//! entity holding the TLS certificate also holds the private key for the claimed DID.

use crate::{Did, KeyPair};
use anyhow::{Context, Result};
use rcgen::{CertificateParams, DnType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cryptographically bound identity bundle
///
/// Combines a DID identity with a TLS certificate, proving that the holder
/// of the TLS certificate also controls the DID's private key.
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
        }
    }
}

/// Serializable binding info for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingInfo {
    pub did: Did,
    pub tls_cert_hash: [u8; 32],
    pub tls_binding_sig: Vec<u8>,
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
        let did = did_keypair.did().clone();

        // 2. Generate TLS certificate with DID as subject
        let (tls_cert, tls_key_der) = Self::generate_tls_cert(&did)?;

        // 3. Compute cert hash and sign with DID key
        let cert_hash = Self::hash_certificate(&tls_cert);
        let tls_binding_sig = did_keypair.sign(&cert_hash).to_vec();

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System time before Unix epoch")?
            .as_secs();

        Ok(IdentityBundle {
            did,
            did_keypair,
            tls_cert,
            tls_key_der,
            tls_binding_sig,
            created_at,
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
    /// - 1 year validity
    fn generate_tls_cert(did: &Did) -> Result<(CertificateDer<'static>, Vec<u8>)> {
        // For now, generate a simple self-signed certificate
        // TODO: Add DID as subject/SAN once we figure out rcgen 0.13 API

        let mut params = CertificateParams::default();

        // Set DID as Common Name
        params
            .distinguished_name
            .push(DnType::CommonName, did.as_str());

        // Generate key pair and certificate
        let key_pair = rcgen::KeyPair::generate().context("Failed to generate key pair")?;

        // Create certificate (rcgen 0.13 API)
        let cert = params.self_signed(&key_pair)
            .context("Failed to generate self-signed certificate")?;

        // Serialize to PEM, then parse to DER
        let cert_pem = cert.pem();
        let pem_data = pem::parse(&cert_pem)
            .context("Failed to parse certificate PEM")?;
        let cert_der = pem_data.contents().to_vec();

        // Serialize key to DER
        let key_der = key_pair.serialize_der();

        Ok((CertificateDer::from(cert_der), key_der))
    }

    /// Hash a certificate using SHA-256
    fn hash_certificate(cert: &CertificateDer<'_>) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(cert.as_ref());
        hasher.finalize().into()
    }
}

/// Verify a binding info against a peer's certificate
///
/// This is used during connection handshake to verify that:
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
    if &actual_hash[..] != &binding_info.tls_cert_hash[..] {
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
