//! Service discovery gossip messages (Epic 3, Issue #935)
//!
//! Message types for service endpoint discovery across the network:
//! - Service announcements with signed endpoints
//! - Withdrawal notifications
//! - Query/response for service discovery
//!
//! Signing helpers live here (not in kernel-api) because this crate
//! already depends on ed25519-dalek. The kernel-api `ServiceEndpoint`
//! provides the pure-data `signing_payload()` method.

use ed25519_dalek::{Signer, Verifier};
use icn_kernel_api::naming::{NamingError, ServiceEndpoint, ServiceEndpointId, ServiceType};
use icn_kernel_api::scope::ScopeLevel;
use icn_kernel_api::types::Signature;
use serde::{Deserialize, Serialize};

// Re-use icn_identity::Did for gossip message fields.
// Note: kernel-api uses `Did = String`, but icn-identity provides the validated type.
use icn_identity::Did;

/// Service discovery gossip message types.
///
/// These messages propagate on `services:announce` and `services:query` topics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceDiscoveryMessage {
    /// Announce a service endpoint.
    ///
    /// Published when a node registers a new service. The endpoint must
    /// carry a valid Ed25519 signature from the provider.
    Announce {
        /// The signed service endpoint
        endpoint: ServiceEndpoint,
    },

    /// Withdraw a previously announced service.
    ///
    /// Published when a service is being deregistered.
    Withdraw {
        /// ID of the service being withdrawn
        service_id: ServiceEndpointId,
        /// DID of the provider withdrawing the service
        provider: Did,
        /// Unix timestamp of withdrawal
        timestamp: u64,
    },

    /// Query for services matching criteria.
    ///
    /// Peers respond with matching endpoints from their local registry.
    Query {
        /// DID of the requester
        requester: Did,
        /// Service type to search for
        service_type: ServiceType,
        /// Maximum scope level to search within
        max_scope: ScopeLevel,
        /// Required capabilities (all must be present)
        required_capabilities: Vec<String>,
        /// Unique query identifier for correlating responses
        query_id: String,
    },

    /// Response to a service query.
    Response {
        /// Query ID this responds to
        query_id: String,
        /// Matching service endpoints
        endpoints: Vec<ServiceEndpoint>,
        /// DID of the responding peer
        responder: Did,
    },
}

/// Sign a `ServiceEndpoint` with an Ed25519 signing key.
///
/// Computes the canonical signing payload and sets the signature field.
pub fn sign_service_endpoint(
    endpoint: &mut ServiceEndpoint,
    signing_key: &ed25519_dalek::SigningKey,
) {
    let payload = endpoint.signing_payload();
    let sig = signing_key.sign(&payload);
    endpoint.signature = Signature::new(sig.to_bytes().to_vec());
}

/// Verify a `ServiceEndpoint` signature against the provider's DID.
///
/// Extracts the public key from the provider DID string and verifies the
/// Ed25519 signature over the canonical signing payload.
///
/// **Key rotation note:** This function verifies against the current public key
/// embedded in the provider DID. If the provider has rotated keys since signing,
/// verification will fail. Callers that need key-rotation tolerance should check
/// the `KEY_ROTATION_GRACE_PERIOD_SECS` window in `icn-gossip::key_rotation`
/// and attempt verification with the previous key if the current key fails.
pub fn verify_service_endpoint(endpoint: &ServiceEndpoint) -> Result<(), NamingError> {
    let provider_did = icn_identity::Did::from_str(&endpoint.provider)
        .map_err(|e| NamingError::InvalidSignature(format!("Cannot parse provider DID: {e}")))?;

    let verifying_key = provider_did.to_verifying_key().map_err(|e| {
        NamingError::InvalidSignature(format!("Cannot extract public key from DID: {e}"))
    })?;

    let sig_bytes: [u8; 64] = endpoint
        .signature
        .as_bytes()
        .try_into()
        .map_err(|_| NamingError::InvalidSignature("Invalid signature length".to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    let payload = endpoint.signing_payload();
    verifying_key
        .verify(&payload, &signature)
        .map_err(|e| NamingError::InvalidSignature(format!("Verification failed: {e}")))?;

    Ok(())
}

/// Topic constants for service discovery gossip.
pub mod topics {
    /// Service announcements and withdrawals (`MinTrustScore(0.1)` - Known+)
    pub const SERVICES_ANNOUNCE: &str = "services:announce";

    /// Service queries and responses (`MinTrustScore(0.1)` - Known+)
    pub const SERVICES_QUERY: &str = "services:query";
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::types::Endpoint;

    fn make_keypair() -> (ed25519_dalek::SigningKey, icn_identity::Did) {
        let kp = icn_identity::KeyPair::generate().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&kp.to_signing_key_bytes());
        let did = kp.did().clone();
        (signing_key, did)
    }

    fn make_endpoint(did: &icn_identity::Did) -> ServiceEndpoint {
        ServiceEndpoint {
            service_id: "svc-test".to_string(),
            provider: did.to_string(),
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![Endpoint::new("https", "example.com", 8080)],
            capabilities: vec!["read".to_string()],
            trust_threshold: 0.1,
            scope_visibility: ScopeLevel::Org,
            ttl_secs: 3600,
            signature: Signature::new(vec![0; 64]),
            created_at: 1700000000,
        }
    }

    #[test]
    fn test_message_serde_roundtrip() {
        let (_, did) = make_keypair();
        let ep = make_endpoint(&did);

        let announce = ServiceDiscoveryMessage::Announce {
            endpoint: ep.clone(),
        };
        let serialized = icn_encoding::encode(&announce).expect("serialize");
        let deserialized: ServiceDiscoveryMessage =
            icn_encoding::decode(&serialized).expect("deserialize");
        assert_eq!(announce, deserialized);

        let withdraw = ServiceDiscoveryMessage::Withdraw {
            service_id: "svc-test".to_string(),
            provider: did.clone(),
            timestamp: 1700000000,
        };
        let serialized = icn_encoding::encode(&withdraw).expect("serialize");
        let deserialized: ServiceDiscoveryMessage =
            icn_encoding::decode(&serialized).expect("deserialize");
        assert_eq!(withdraw, deserialized);

        let query = ServiceDiscoveryMessage::Query {
            requester: did.clone(),
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            max_scope: ScopeLevel::Federation,
            required_capabilities: vec!["read".to_string()],
            query_id: "q-123".to_string(),
        };
        let serialized = icn_encoding::encode(&query).expect("serialize");
        let deserialized: ServiceDiscoveryMessage =
            icn_encoding::decode(&serialized).expect("deserialize");
        assert_eq!(query, deserialized);

        let response = ServiceDiscoveryMessage::Response {
            query_id: "q-123".to_string(),
            endpoints: vec![ep],
            responder: did.clone(),
        };
        let serialized = icn_encoding::encode(&response).expect("serialize");
        let deserialized: ServiceDiscoveryMessage =
            icn_encoding::decode(&serialized).expect("deserialize");
        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_sign_and_verify() {
        let (signing_key, did) = make_keypair();
        let mut ep = make_endpoint(&did);

        sign_service_endpoint(&mut ep, &signing_key);
        assert_eq!(ep.signature.as_bytes().len(), 64);

        // Verify should succeed
        verify_service_endpoint(&ep).expect("signature should be valid");
    }

    #[test]
    fn test_tamper_detection() {
        let (signing_key, did) = make_keypair();
        let mut ep = make_endpoint(&did);

        sign_service_endpoint(&mut ep, &signing_key);

        // Tamper with a field
        ep.trust_threshold = 0.9;

        // Verify should fail
        let result = verify_service_endpoint(&ep);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_key_rejection() {
        let (signing_key, did) = make_keypair();
        let (_, other_did) = make_keypair();
        let mut ep = make_endpoint(&did);

        sign_service_endpoint(&mut ep, &signing_key);

        // Set provider to a different DID (verify against wrong public key)
        ep.provider = other_did.as_str().to_string();

        // Verify should fail (wrong key)
        let result = verify_service_endpoint(&ep);
        assert!(result.is_err());
    }

    #[test]
    fn test_topic_names() {
        assert_eq!(topics::SERVICES_ANNOUNCE, "services:announce");
        assert_eq!(topics::SERVICES_QUERY, "services:query");
    }
}
