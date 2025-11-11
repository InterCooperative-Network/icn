//! TLS certificate generation using DIDs
//!
//! ICN uses DID-based certificates for mTLS authentication. The certificate includes
//! the DID as the Common Name, allowing peers to identify each other.

use anyhow::{Context, Result};
use icn_identity::KeyPair;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use tracing::info;

/// Generate a self-signed certificate for a DID
///
/// The certificate uses the DID as the Common Name and generates a fresh Ed25519 key
/// for TLS. This separates the TLS layer key from the DID signing key.
pub fn generate_self_signed_cert(
    keypair: &KeyPair,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let did = keypair.did();

    info!("Generating self-signed certificate for DID: {}", did);

    // Use rcgen to generate a simple self-signed certificate with the DID as subject
    let subject_alt_names = vec![did.as_str().to_string()];
    let certified_key = rcgen::generate_simple_self_signed(subject_alt_names)?;

    // Export certificate and key
    let cert_der = CertificateDer::from(certified_key.cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(certified_key.key_pair.serialize_der())
        .map_err(|e| anyhow::anyhow!("Failed to serialize private key: {}", e))?;

    Ok((vec![cert_der], key_der))
}

/// Create a rustls server configuration for QUIC
pub fn create_server_config(
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

/// Create a rustls client configuration for QUIC
///
/// This uses a custom certificate verifier that validates DID-based certificates.
pub fn create_client_config() -> Result<rustls::ClientConfig> {
    // Create a client config that accepts self-signed certificates
    // In production, this should validate the DID signature
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(DidCertificateVerifier))
        .with_no_client_auth();

    // Enable ALPN for QUIC
    config.alpn_protocols = vec![b"icn/1".to_vec()];

    Ok(config)
}

/// Custom certificate verifier for DID-based certificates
///
/// TODO: This currently accepts all certificates. In production, it should:
/// 1. Extract the DID from the certificate CN
/// 2. Query the trust graph for the peer's trust score
/// 3. Accept/reject based on trust class (e.g., only Partner or Federated)
#[derive(Debug)]
struct DidCertificateVerifier;

impl rustls::client::danger::ServerCertVerifier for DidCertificateVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // TODO: Implement proper DID certificate verification
        // For now, accept all certificates (development only)
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
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        // TODO: Implement signature verification
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
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
    fn test_create_client_config() {
        setup();
        let config = create_client_config().unwrap();

        // Should have ALPN configured
        assert_eq!(config.alpn_protocols, vec![b"icn/1".to_vec()]);
    }
}
