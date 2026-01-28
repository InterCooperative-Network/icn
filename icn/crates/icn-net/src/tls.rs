//! TLS certificate generation using DIDs with TOFU verification
//!
//! ICN uses DID-based certificates for mTLS authentication:
//!
//! ## Certificate Generation
//! - Self-signed certificates with DID as Subject Alternative Name (SAN)
//! - Ed25519 keypairs for TLS (separate from DID signing key)
//! - Certificates store DID in DNS SAN field for peer identification
//!
//! ## TOFU (Trust-On-First-Use) Verification
//! The kernel accepts all cryptographically valid DID certificates at TLS layer.
//! Trust-based access control is enforced at the application layer via PolicyOracle.
//!
//! ## Security Model
//! - TLS layer: Accept all authenticated DIDs (TOFU mode)
//! - Application layer: Gate operations based on PolicyOracle decisions
//! - This avoids chicken-and-egg problems with trust bootstrapping
//!
//! ## Ed25519 Signature Verification
//! - TLS 1.3 handshake signatures verified using certificate's Ed25519 public key
//! - 32-byte public key extracted from X.509 certificate
//! - Full cryptographic verification prevents MITM attacks

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use icn_identity::KeyPair;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use tracing::{debug, info};

/// TOFU (Trust-On-First-Use) Certificate Verifier
///
/// Accepts any valid self-signed certificate during TLS handshake.
/// Identity verification is deferred to the application layer (Hello message).
#[derive(Debug)]
struct TofuCertificateVerifier {}

impl rustls::server::danger::ClientCertVerifier for TofuCertificateVerifier {
    fn offer_client_auth(&self) -> bool {
        // Request client certificates
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        // Don't require client cert at TLS layer - Hello message will verify
        false
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        // Self-signed certs don't have root CAs
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        // Accept any certificate - verification happens at application layer
        debug!(
            "TOFU: Accepting client certificate (identity verification deferred to Hello message)"
        );
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        // TLS 1.2 not used with QUIC
        Err(rustls::Error::General("TLS 1.2 not supported".to_string()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        use rustls::SignatureScheme;

        // Verify we're using Ed25519
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::Error::General(format!(
                "Unsupported signature scheme: {:?} (expected Ed25519)",
                dss.scheme
            )));
        }

        // Parse the certificate to extract the public key
        use x509_parser::prelude::*;
        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {e}")))?;

        // Extract the public key bytes
        let public_key_bytes = parsed_cert.public_key().subject_public_key.data.clone();

        // Ed25519 public keys are 32 bytes
        if public_key_bytes.len() != 32 {
            return Err(rustls::Error::General(format!(
                "Invalid Ed25519 public key length: {} (expected 32)",
                public_key_bytes.len()
            )));
        }

        // Convert to [u8; 32] array
        let key_array: [u8; 32] = public_key_bytes[..32]
            .try_into()
            .map_err(|_| rustls::Error::General("Failed to convert public key".to_string()))?;

        // Create Ed25519 verifying key
        let verifying_key = VerifyingKey::from_bytes(&key_array)
            .map_err(|e| rustls::Error::General(format!("Invalid Ed25519 public key: {e}")))?;

        // Parse signature
        let signature = Signature::from_slice(dss.signature())
            .map_err(|e| rustls::Error::General(format!("Invalid Ed25519 signature: {e}")))?;

        // Verify signature
        verifying_key
            .verify(message, &signature)
            .map_err(|e| rustls::Error::General(format!("Signature verification failed: {e}")))?;

        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

/// TOFU Server Certificate Verifier (for client connections)
///
/// Accepts any valid self-signed certificate during TLS handshake.
/// Identity and trust verification happens at the application layer via PolicyOracle.
#[derive(Debug)]
struct TofuServerCertVerifier {}

impl rustls::client::danger::ServerCertVerifier for TofuServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Accept any certificate - verification happens at application layer
        debug!("TOFU: Accepting server certificate (identity verification via PolicyOracle)");
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        // TLS 1.2 not used with QUIC
        Err(rustls::Error::General("TLS 1.2 not supported".to_string()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        use rustls::SignatureScheme;

        // Verify we're using Ed25519
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::Error::General(format!(
                "Unsupported signature scheme: {:?} (expected Ed25519)",
                dss.scheme
            )));
        }

        // Parse the certificate to extract the public key
        use x509_parser::prelude::*;
        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {e}")))?;

        // Extract the public key bytes
        let public_key_bytes = parsed_cert.public_key().subject_public_key.data.clone();

        // Ed25519 public keys are 32 bytes
        if public_key_bytes.len() != 32 {
            return Err(rustls::Error::General(format!(
                "Invalid Ed25519 public key length: {} (expected 32)",
                public_key_bytes.len()
            )));
        }

        // Convert to [u8; 32] array
        let key_array: [u8; 32] = public_key_bytes[..32]
            .try_into()
            .map_err(|_| rustls::Error::General("Failed to convert public key".to_string()))?;

        // Create Ed25519 verifying key
        let verifying_key = VerifyingKey::from_bytes(&key_array)
            .map_err(|e| rustls::Error::General(format!("Invalid Ed25519 public key: {e}")))?;

        // Parse signature
        let signature = Signature::from_slice(dss.signature())
            .map_err(|e| rustls::Error::General(format!("Invalid Ed25519 signature: {e}")))?;

        // Verify signature
        verifying_key
            .verify(message, &signature)
            .map_err(|e| rustls::Error::General(format!("Signature verification failed: {e}")))?;

        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

/// Generate a self-signed certificate for a DID
///
/// The certificate uses the DID as the Common Name and generates a fresh Ed25519 key
/// for TLS. This separates the TLS layer key from the DID signing key.
pub fn generate_self_signed_cert(
    keypair: &KeyPair,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let did = keypair.did();

    info!("Generating self-signed certificate for DID: {}", did);

    // Create certificate parameters with Ed25519 algorithm
    let mut params = rcgen::CertificateParams::new(vec![did.as_str().to_string()])?;
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyEncipherment,
    ];

    // Generate Ed25519 key pair for the certificate
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)?;

    // Create certificate with Ed25519 key
    let cert = params.self_signed(&key_pair)?;

    // Export certificate and key
    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(key_pair.serialize_der())
        .map_err(|e| anyhow::anyhow!("Failed to serialize private key: {e}"))?;

    Ok((vec![cert_der], key_der))
}

/// Create a rustls server configuration for QUIC with TOFU trust model
///
/// This uses self-signed certificates and defers identity verification to the application
/// layer (Hello message handler). This implements Trust-On-First-Use (TOFU):
/// 1. Accept any TLS connection with valid self-signed cert  
/// 2. Verify DID-TLS binding in Hello message handler
/// 3. Use trust graph for authorization decisions
pub fn create_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<rustls::ServerConfig> {
    // Create a permissive verifier that accepts all self-signed certificates
    // Identity verification happens at the application layer (Hello message)
    let verifier = TofuCertificateVerifier {};
    let mut config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(verifier))
        .with_single_cert(certs, key)
        .context("Failed to create server config")?;

    // Enable ALPN for QUIC
    config.alpn_protocols = vec![b"icn/1".to_vec()];

    Ok(config)
}

/// Create a rustls server configuration for QUIC without client authentication
///
/// **WARNING**: This is for development/testing only. Production deployments
/// should always use `create_server_config` with client certificate verification.
pub fn create_server_config_no_client_auth(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<rustls::ServerConfig> {
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Failed to create server config")?;

    // Enable ALPN for QUIC
    config.alpn_protocols = vec![b"icn/1".to_vec()];

    Ok(config)
}

/// Create TLS client configuration for QUIC with TOFU mode
///
/// This uses a simple TOFU certificate verifier that accepts all valid self-signed
/// certificates. Trust enforcement happens at the application layer via PolicyOracle.
///
/// ## Trust Enforcement Flow
///
/// The ICN network uses a layered trust model:
///
/// 1. **TLS Layer (this function)**: TOFU accepts any cryptographically valid certificate.
///    The peer's DID is embedded in the certificate's Subject Alternative Name (SAN).
///    This layer only ensures the peer owns the corresponding private key.
///
/// 2. **Protocol Layer**: After TLS handshake, the NetworkActor extracts the peer's DID
///    from the certificate. HelloMessage exchange confirms both parties' identities.
///
/// 3. **Application Layer**: When messages arrive, the `RateLimiter::check_rate_limit()`
///    method consults the PolicyOracle to determine if the peer should be allowed:
///    - `PolicyDecision::Allow { constraints }`: Message allowed, rate limit applied from constraints
///    - `PolicyDecision::Deny { .. }`: Message immediately rejected (returns false)
///
/// 4. **Untrusted Peer Handling**: Peers that fail PolicyOracle checks have their
///    messages dropped. If no oracle is configured, the fallback rate limit is applied.
///
/// This architecture implements the "meaning firewall" principle: TLS accepts connections,
/// but the PolicyOracle (provided by apps like TrustPolicyOracle) makes authorization decisions.
pub fn create_tofu_client_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<rustls::ClientConfig> {
    let verifier = TofuServerCertVerifier {};

    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_client_auth_cert(certs, key)
        .context("Failed to configure client certificate")?;

    // Enable ALPN for QUIC
    config.alpn_protocols = vec![b"icn/1".to_vec()];

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        // Install default crypto provider for tests
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[test]
    fn test_generate_self_signed_cert() {
        setup();
        let keypair = KeyPair::generate().unwrap();
        let (certs, _key) = generate_self_signed_cert(&keypair).unwrap();

        assert_eq!(certs.len(), 1);
        // Certificate should be valid DER
        assert!(!certs[0].is_empty());
    }

    #[test]
    fn test_create_server_config() {
        setup();
        let keypair = KeyPair::generate().unwrap();
        let (certs, key) = generate_self_signed_cert(&keypair).unwrap();

        let config = create_server_config(certs, key).unwrap();

        // Should have ALPN configured
        assert_eq!(config.alpn_protocols, vec![b"icn/1".to_vec()]);
    }

    #[test]
    fn test_create_tofu_client_config() {
        setup();
        let identity_bundle = icn_identity::IdentityBundle::generate().unwrap();

        let certs = vec![identity_bundle.tls_cert().clone()];
        let key = identity_bundle.tls_key();
        let config = create_tofu_client_config(certs, key).unwrap();

        // Should have ALPN configured
        assert_eq!(config.alpn_protocols, vec![b"icn/1".to_vec()]);
    }
}
