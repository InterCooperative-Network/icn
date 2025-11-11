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
/// This verifier extracts the DID from certificate SAN (Subject Alternative Name) and performs
/// basic validation. Full trust graph integration is TODO.
///
/// Security notes:
/// - Currently accepts self-signed certificates (required for P2P architecture)
/// - Does NOT yet integrate with trust graph for trust score verification
/// - Logs all certificate verifications for security auditing
/// - Validates DID format and certificate expiration
#[derive(Debug)]
struct DidCertificateVerifier;

impl DidCertificateVerifier {
    /// Extract DID from certificate Subject Alternative Names
    fn extract_did_from_cert(cert: &CertificateDer<'_>) -> Result<String, rustls::Error> {
        use x509_parser::prelude::*;

        // Parse X.509 certificate
        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {}", e)))?;

        // Look for DID in Subject Alternative Names
        if let Ok(Some(san_ext)) = parsed_cert.subject_alternative_name() {
            for name in &san_ext.value.general_names {
                if let GeneralName::DNSName(dns) = name {
                    // DIDs are stored in DNS SAN fields
                    if dns.starts_with("did:icn:") {
                        return Ok(dns.to_string());
                    }
                }
            }
        }

        Err(rustls::Error::General(
            "No DID found in certificate SAN".to_string(),
        ))
    }

    /// Validate certificate hasn't expired
    fn check_expiration(cert: &CertificateDer<'_>, now: rustls::pki_types::UnixTime) -> Result<(), rustls::Error> {
        use x509_parser::prelude::*;

        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {}", e)))?;

        let current_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(now.as_secs());
        let not_before = parsed_cert.validity().not_before.to_datetime();
        let not_after = parsed_cert.validity().not_after.to_datetime();

        if current_time < not_before {
            return Err(rustls::Error::General(
                "Certificate not yet valid".to_string(),
            ));
        }

        if current_time > not_after {
            return Err(rustls::Error::General(
                "Certificate expired".to_string(),
            ));
        }

        Ok(())
    }
}

impl rustls::client::danger::ServerCertVerifier for DidCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Extract and validate DID from certificate
        let did = Self::extract_did_from_cert(end_entity)?;

        // Validate DID format
        if !did.starts_with("did:icn:") {
            return Err(rustls::Error::General(format!(
                "Invalid DID format: {}",
                did
            )));
        }

        // Check certificate expiration
        Self::check_expiration(end_entity, now)?;

        // Log certificate verification for security audit
        tracing::info!(
            "Certificate verification: Accepted self-signed cert for DID: {}",
            did
        );
        tracing::warn!(
            "⚠️  SECURITY: Trust graph verification not yet implemented for DID: {}",
            did
        );

        // TODO: Query trust graph and reject if trust score < Partner
        // For now, accept all valid DID certificates (development mode)
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
