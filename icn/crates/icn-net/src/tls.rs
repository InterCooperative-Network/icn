//! TLS certificate generation using DIDs with trust-gated verification
//!
//! ICN uses DID-based certificates for mTLS authentication with trust graph integration:
//!
//! ## Certificate Generation
//! - Self-signed certificates with DID as Subject Alternative Name (SAN)
//! - Ed25519 keypairs for TLS (separate from DID signing key)
//! - Certificates store DID in DNS SAN field for peer identification
//!
//! ## Trust-Gated Verification
//! The `DidCertificateVerifier` implements trust-based access control by:
//! 1. Extracting DID from certificate SAN during TLS handshake
//! 2. Querying trust graph for peer's trust score
//! 3. Rejecting connections if trust score < min_trust_threshold
//! 4. Recording rejection metrics for security monitoring
//!
//! ## Security Model
//! - **Default threshold: 0.0** - Accept all authenticated DIDs (development mode)
//! - **Recommended production: 0.1** - Reject isolated/unknown peers
//! - **High security: 0.4** - Only accept partner-level or higher trust
//! - Trust scores computed from own DID's perspective using trust graph
//! - Prevents Sybil attacks and unauthorized network access
//!
//! ## Ed25519 Signature Verification
//! - TLS 1.3 handshake signatures verified using certificate's Ed25519 public key
//! - 32-byte public key extracted from X.509 certificate
//! - Full cryptographic verification prevents MITM attacks

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use icn_identity::{Did, KeyPair};
use icn_trust::TrustGraph;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
        debug!(
            "TOFU: Accepting server certificate (identity verification via PolicyOracle)"
        );
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

/// Create a rustls client configuration for QUIC with trust-gated verification (deprecated)
///
/// This uses a custom certificate verifier that validates DID-based certificates
/// and enforces trust-based access control.
///
/// **Note**: This function is deprecated in favor of application-layer trust
/// enforcement via PolicyOracle. Use `create_tofu_client_config` instead.
///
/// # Arguments
/// * `trust_graph` - Shared trust graph for peer trust lookups
/// * `own_did` - This node's DID (for trust computation)
/// * `min_trust_threshold` - Minimum trust score required (default: 0.0 = allow all authenticated DIDs)
#[deprecated(note = "Use create_tofu_client_config and enforce trust at application layer")]
pub fn create_client_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    trust_graph: Arc<RwLock<TrustGraph>>,
    own_did: Did,
    min_trust_threshold: Option<f64>,
) -> Result<rustls::ClientConfig> {
    let verifier = DidCertificateVerifier {
        trust_graph,
        own_did,
        min_trust_threshold: min_trust_threshold.unwrap_or(0.0),
    };

    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_client_auth_cert(certs, key)
        .context("Failed to configure client certificate")?;

    // Enable ALPN for QUIC
    config.alpn_protocols = vec![b"icn/1".to_vec()];

    Ok(config)
}

/// Custom certificate verifier for DID-based certificates
///
/// This verifier extracts the DID from certificate SAN (Subject Alternative Name) and performs
/// comprehensive validation including trust graph integration.
///
/// ## Trust-on-First-Use (TOFU) Architecture
///
/// The verifier implements TOFU semantics:
/// 1. **TLS layer**: Accept all valid DID certificates (verify cryptographic validity only)
/// 2. **Application layer**: Gate operations based on trust scores
/// 3. **Trust building**: Allow trust to develop through gossip/attestations over time
///
/// This approach avoids the chicken-and-egg problem where you need trust to establish
/// connections, but need connections to build trust.
///
/// Security features:
/// - Accepts self-signed certificates (required for P2P architecture)
/// - Validates certificate cryptographic signatures
/// - Validates DID format and certificate expiration
/// - Optional trust gating for high-security deployments
/// - Logs all certificate verifications for security auditing
///
/// Trust modes:
/// - **TOFU mode (min_trust_threshold = 0.0)**: Accept all authenticated DIDs, gate operations by trust
/// - **Trust-gated mode (min_trust_threshold > 0.0)**: Reject connections from low-trust peers at TLS layer
#[derive(Clone)]
struct DidCertificateVerifier {
    trust_graph: Arc<RwLock<icn_trust::TrustGraph>>,
    own_did: icn_identity::Did,
    /// Minimum trust threshold for connection acceptance
    /// - 0.0: TOFU mode - accept all valid DIDs (recommended)
    /// - > 0.0: Trust-gated mode - reject low-trust peers at TLS handshake
    min_trust_threshold: f64,
}

impl std::fmt::Debug for DidCertificateVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DidCertificateVerifier")
            .field("own_did", &self.own_did)
            .field("min_trust_threshold", &self.min_trust_threshold)
            .field("trust_graph", &"<Arc<RwLock<TrustGraph>>>")
            .finish()
    }
}

impl DidCertificateVerifier {
    /// Extract DID from certificate Subject Alternative Names
    fn extract_did_from_cert(cert: &CertificateDer<'_>) -> Result<String, rustls::Error> {
        use x509_parser::prelude::*;

        // Parse X.509 certificate
        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {e}")))?;

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
    fn check_expiration(
        cert: &CertificateDer<'_>,
        now: rustls::pki_types::UnixTime,
    ) -> Result<(), rustls::Error> {
        use x509_parser::prelude::*;

        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {e}")))?;

        let current_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(now.as_secs());
        let not_before = parsed_cert.validity().not_before.to_datetime();
        let not_after = parsed_cert.validity().not_after.to_datetime();

        if current_time < not_before {
            return Err(rustls::Error::General(
                "Certificate not yet valid".to_string(),
            ));
        }

        if current_time > not_after {
            return Err(rustls::Error::General("Certificate expired".to_string()));
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
        let did_str = Self::extract_did_from_cert(end_entity)?;

        // Validate DID format
        if !did_str.starts_with("did:icn:") {
            return Err(rustls::Error::General(format!(
                "Invalid DID format: {did_str}"
            )));
        }

        // Parse DID
        let peer_did = Did::from_str(&did_str)
            .map_err(|e| rustls::Error::General(format!("Failed to parse DID: {e}")))?;

        // Check certificate expiration
        Self::check_expiration(end_entity, now)?;

        // Query trust graph for peer's trust score
        // Use try_read since we're in a sync context (rustls callback) but may be
        // called from within a tokio runtime where blocking is not allowed
        let trust_score = {
            match self.trust_graph.try_read() {
                Ok(graph) => graph.compute_trust_score(&peer_did).unwrap_or(0.0),
                Err(_) => {
                    // Lock is held - use default trust score of 0.0
                    // This allows min_trust_threshold to determine outcome
                    warn!("Trust graph lock unavailable during TLS verification for {}, using default trust score 0.0", did_str);
                    0.0
                }
            }
        };

        // Enforce trust threshold
        if trust_score < self.min_trust_threshold {
            warn!(
                "🔒 Connection rejected: DID {} has insufficient trust (score: {:.3}, required: {:.3})",
                did_str, trust_score, self.min_trust_threshold
            );

            // Increment rejection metric
            icn_obs::metrics::network::connections_rejected_untrusted_inc(&did_str, trust_score);

            return Err(rustls::Error::General(format!(
                "Peer DID {} has insufficient trust score {:.3} (required: {:.3})",
                did_str, trust_score, self.min_trust_threshold
            )));
        }

        // Log successful verification
        info!(
            "✅ Certificate verified: DID {} (trust score: {:.3})",
            did_str, trust_score
        );

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

impl rustls::server::danger::ClientCertVerifier for DidCertificateVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        // Extract and validate DID from certificate
        let did_str = Self::extract_did_from_cert(end_entity)?;

        // Validate DID format
        if !did_str.starts_with("did:icn:") {
            return Err(rustls::Error::General(format!(
                "Invalid DID format: {did_str}"
            )));
        }

        // Parse DID
        let peer_did = Did::from_str(&did_str)
            .map_err(|e| rustls::Error::General(format!("Failed to parse DID: {e}")))?;

        // Check certificate expiration
        Self::check_expiration(end_entity, now)?;

        // Query trust graph for peer's trust score
        let trust_score = {
            match self.trust_graph.try_read() {
                Ok(graph) => graph.compute_trust_score(&peer_did).unwrap_or(0.0),
                Err(_) => {
                    warn!("Trust graph lock unavailable during TLS verification for {}, using default trust score 0.0", did_str);
                    0.0
                }
            }
        };

        // Enforce trust threshold
        if trust_score < self.min_trust_threshold {
            warn!(
                "🔒 Client connection rejected: DID {} has insufficient trust (score: {:.3}, required: {:.3})",
                did_str, trust_score, self.min_trust_threshold
            );

            icn_obs::metrics::network::connections_rejected_untrusted_inc(&did_str, trust_score);

            return Err(rustls::Error::General(format!(
                "Client DID {} has insufficient trust score {:.3} (required: {:.3})",
                did_str, trust_score, self.min_trust_threshold
            )));
        }

        info!(
            "✅ Client certificate verified: DID {} (trust score: {:.3})",
            did_str, trust_score
        );

        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 not supported".to_string()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        use rustls::SignatureScheme;

        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::Error::General(format!(
                "Unsupported signature scheme: {:?} (expected Ed25519)",
                dss.scheme
            )));
        }

        use x509_parser::prelude::*;
        let (_, parsed_cert) = X509Certificate::from_der(cert)
            .map_err(|e| rustls::Error::General(format!("Failed to parse certificate: {e}")))?;

        let public_key_bytes = parsed_cert.public_key().subject_public_key.data.clone();

        if public_key_bytes.len() != 32 {
            return Err(rustls::Error::General(format!(
                "Invalid Ed25519 public key length: {} (expected 32)",
                public_key_bytes.len()
            )));
        }

        let key_array: [u8; 32] = public_key_bytes[..32]
            .try_into()
            .map_err(|_| rustls::Error::General("Failed to convert public key".to_string()))?;

        let verifying_key = VerifyingKey::from_bytes(&key_array)
            .map_err(|e| rustls::Error::General(format!("Invalid Ed25519 public key: {e}")))?;

        let signature = Signature::from_slice(dss.signature())
            .map_err(|e| rustls::Error::General(format!("Invalid Ed25519 signature: {e}")))?;

        verifying_key
            .verify(message, &signature)
            .map_err(|e| rustls::Error::General(format!("Signature verification failed: {e}")))?;

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
        let identity_bundle = icn_identity::IdentityBundle::generate().unwrap();
        let own_did = identity_bundle.did().clone();

        // Create a temporary trust graph for testing
        let store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::temporary().unwrap());
        let trust_graph = Arc::new(RwLock::new(icn_trust::TrustGraph::new(
            store,
            own_did.clone(),
        )));

        let certs = vec![identity_bundle.tls_cert().clone()];
        let key = identity_bundle.tls_key();
        let config = create_client_config(certs, key, trust_graph, own_did, None).unwrap();

        // Should have ALPN configured
        assert_eq!(config.alpn_protocols, vec![b"icn/1".to_vec()]);
    }
}
