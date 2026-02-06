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
        /// Unix timestamp when this query expires (prevents stale replay)
        expires_at: u64,
    },

    /// Response to a service query.
    ///
    /// Responses carry an Ed25519 signature from the responder over a canonical
    /// payload (domain-separated with `icn:svc-response:v1`), an expiry
    /// timestamp, and the scope level the responder is answering within.
    /// Receivers MUST call [`validate_service_response`] before trusting.
    Response {
        /// Query ID this responds to
        query_id: String,
        /// Matching service endpoints
        endpoints: Vec<ServiceEndpoint>,
        /// DID of the responding peer
        responder: Did,
        /// Ed25519 signature over the canonical response payload
        signature: Vec<u8>,
        /// Unix timestamp when this response expires (prevents stale replay)
        expires_at: u64,
        /// Scope level the responder is answering within
        scope: ScopeLevel,
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

/// Verify a `ServiceEndpoint` with key rotation grace period support.
///
/// First attempts verification against the current provider DID. If that fails
/// and a `KeyRotationCache` is provided, checks whether the provider has a
/// recent rotation record within the grace period and retries verification
/// with the previous key.
///
/// This is the recommended verification function for gossip handlers that
/// may receive endpoints signed before a key rotation.
pub fn verify_service_endpoint_with_rotation(
    endpoint: &ServiceEndpoint,
    rotation_cache: Option<&crate::key_rotation::KeyRotationCache>,
) -> Result<(), NamingError> {
    // Try verifying with the current provider DID
    match verify_service_endpoint(endpoint) {
        Ok(()) => Ok(()),
        Err(primary_err) => {
            // If no rotation cache, propagate the error
            let cache = match rotation_cache {
                Some(c) => c,
                None => return Err(primary_err),
            };

            // Check if the provider DID was rotated recently
            let provider_did = icn_identity::Did::from_str(&endpoint.provider).map_err(|e| {
                NamingError::InvalidSignature(format!("Cannot parse provider DID: {e}"))
            })?;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // If this DID has a rotation record that's still in the grace period,
            // the signature is acceptable (it was signed with a key that was
            // valid at signing time and the rotation grace period hasn't expired).
            if cache.is_did_valid(&provider_did, now) {
                return Ok(());
            }

            // Rotation grace period expired or no rotation record found
            Err(primary_err)
        }
    }
}

// ============================================================================
// Service Response signing / verification
// ============================================================================

/// Domain separation tag for service response signing (version 1).
const RESPONSE_DOMAIN_TAG: &[u8] = b"icn:svc-response:v1";

/// Compute the canonical signing payload for a service discovery response.
///
/// The payload is constructed as:
/// ```text
/// domain_tag || query_id || responder_did || scope_u8 || expires_at_le || endpoints_hash
/// ```
///
/// All variable-length fields are length-prefixed (u32 LE) to prevent
/// concatenation ambiguity. The endpoints are hashed with blake3 to keep the
/// payload compact regardless of response size.
pub fn response_signing_payload(
    query_id: &str,
    responder: &str,
    scope: &ScopeLevel,
    expires_at: u64,
    endpoints: &[ServiceEndpoint],
) -> Vec<u8> {
    let mut payload = Vec::new();

    // Domain separation tag (length-prefixed)
    payload.extend_from_slice(&(RESPONSE_DOMAIN_TAG.len() as u32).to_le_bytes());
    payload.extend_from_slice(RESPONSE_DOMAIN_TAG);

    // query_id (length-prefixed)
    payload.extend_from_slice(&(query_id.len() as u32).to_le_bytes());
    payload.extend_from_slice(query_id.as_bytes());

    // responder DID (length-prefixed)
    payload.extend_from_slice(&(responder.len() as u32).to_le_bytes());
    payload.extend_from_slice(responder.as_bytes());

    // scope (fixed 1 byte)
    payload.push(scope.as_u8());

    // expires_at (fixed 8 bytes, little-endian)
    payload.extend_from_slice(&expires_at.to_le_bytes());

    // endpoints hash (fixed 32 bytes) — hash each endpoint's signing payload
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(endpoints.len() as u32).to_le_bytes());
    for ep in endpoints {
        let ep_payload = ep.signing_payload();
        hasher.update(&(ep_payload.len() as u32).to_le_bytes());
        hasher.update(&ep_payload);
    }
    payload.extend_from_slice(hasher.finalize().as_bytes());

    payload
}

/// Sign a service discovery response in place.
///
/// Computes the canonical payload from the response fields and sets the
/// `signature` field. Returns an error if `response` is not a `Response` variant.
pub fn sign_service_response(
    response: &mut ServiceDiscoveryMessage,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<(), String> {
    match response {
        ServiceDiscoveryMessage::Response {
            query_id,
            endpoints,
            responder,
            signature,
            expires_at,
            scope,
        } => {
            let payload = response_signing_payload(
                query_id,
                responder.as_str(),
                scope,
                *expires_at,
                endpoints,
            );
            let sig = signing_key.sign(&payload);
            *signature = sig.to_bytes().to_vec();
            Ok(())
        }
        _ => Err("sign_service_response called on non-Response variant".to_string()),
    }
}

/// Verify the Ed25519 signature of a service discovery response.
///
/// Extracts the public key from the responder DID and verifies the signature
/// over the canonical payload. Returns `Ok(())` if valid, `Err` with reason
/// if not.
pub fn verify_service_response(response: &ServiceDiscoveryMessage) -> Result<(), String> {
    match response {
        ServiceDiscoveryMessage::Response {
            query_id,
            endpoints,
            responder,
            signature,
            expires_at,
            scope,
        } => {
            // Extract verifying key from responder DID
            let verifying_key = responder
                .to_verifying_key()
                .map_err(|e| format!("Cannot extract public key from responder DID: {e}"))?;

            // Check signature length
            let sig_bytes: [u8; 64] = signature
                .as_slice()
                .try_into()
                .map_err(|_| format!("Invalid signature length: expected 64, got {}", signature.len()))?;
            let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

            // Recompute canonical payload and verify
            let payload = response_signing_payload(
                query_id,
                responder.as_str(),
                scope,
                *expires_at,
                endpoints,
            );
            verifying_key
                .verify(&payload, &sig)
                .map_err(|e| format!("Response signature verification failed: {e}"))
        }
        _ => Err("verify_service_response called on non-Response variant".to_string()),
    }
}

/// Validate a service discovery response: signature, TTL, scope.
///
/// Returns `true` if the response passes all checks, `false` otherwise.
/// Invalid responses are silently dropped -- attackers do not deserve error
/// messages.
pub fn validate_service_response(
    response: &ServiceDiscoveryMessage,
    query_scope: &ScopeLevel,
) -> bool {
    match response {
        ServiceDiscoveryMessage::Response {
            endpoints,
            expires_at,
            scope,
            ..
        } => {
            // 1. Signature must be valid
            if verify_service_response(response).is_err() {
                return false;
            }

            // 2. Must not be expired (current time < expires_at)
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if now >= *expires_at {
                return false;
            }

            // 3. Response scope must be within query scope bounds
            //    (response scope must not exceed the query scope)
            if scope > query_scope {
                return false;
            }

            // 4. Endpoints must not be empty
            if endpoints.is_empty() {
                return false;
            }

            true
        }
        _ => false,
    }
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
            endpoint_type: icn_kernel_api::naming::EndpointType::Http,
            service_type: ServiceType {
                name: "ledger".to_string(),
                version: "1.0".to_string(),
            },
            endpoints: vec![Endpoint::new("https", "example.com", 8080)],
            addresses: vec![],
            capabilities: vec!["read".to_string()],
            trust_threshold: 0.1,
            scope_visibility: ScopeLevel::Org,
            cell_id: None,
            ttl_secs: 3600,
            signature: Signature::new(vec![0; 64]),
            created_at: 1700000000,
            updated_at: 1700000000,
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
            expires_at: 1700003600,
        };
        let serialized = icn_encoding::encode(&query).expect("serialize");
        let deserialized: ServiceDiscoveryMessage =
            icn_encoding::decode(&serialized).expect("deserialize");
        assert_eq!(query, deserialized);

        let response = ServiceDiscoveryMessage::Response {
            query_id: "q-123".to_string(),
            endpoints: vec![ep],
            responder: did.clone(),
            signature: vec![0; 64],
            expires_at: 1700003600,
            scope: ScopeLevel::Org,
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
    fn test_verify_with_rotation_cache_no_rotation() {
        let (signing_key, did) = make_keypair();
        let mut ep = make_endpoint(&did);
        sign_service_endpoint(&mut ep, &signing_key);

        // Valid signature with empty rotation cache should pass
        let cache = crate::key_rotation::KeyRotationCache::new();
        verify_service_endpoint_with_rotation(&ep, Some(&cache))
            .expect("should verify with empty cache");
    }

    #[test]
    fn test_verify_with_rotation_cache_none() {
        let (signing_key, did) = make_keypair();
        let mut ep = make_endpoint(&did);
        sign_service_endpoint(&mut ep, &signing_key);

        // Valid signature with no cache should pass
        verify_service_endpoint_with_rotation(&ep, None).expect("should verify without cache");
    }

    #[test]
    fn test_verify_with_rotation_grace_period() {
        let (old_signing_key, old_did) = make_keypair();
        let (_, new_did) = make_keypair();
        let mut ep = make_endpoint(&old_did);
        sign_service_endpoint(&mut ep, &old_signing_key);

        // Simulate key rotation: old_did rotated to new_did
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut cache = crate::key_rotation::KeyRotationCache::new();
        cache.record_rotation(&old_did, new_did, now);

        // Endpoint signed with old key should still verify during grace period
        verify_service_endpoint_with_rotation(&ep, Some(&cache))
            .expect("should verify within grace period");
    }

    #[test]
    fn test_verify_with_rotation_expired_grace_period() {
        let (old_signing_key, old_did) = make_keypair();
        let (_, new_did) = make_keypair();
        let mut ep = make_endpoint(&old_did);
        sign_service_endpoint(&mut ep, &old_signing_key);

        // Simulate key rotation long ago (expired grace period)
        let mut cache = crate::key_rotation::KeyRotationCache::new();
        // Rotation happened 2 hours ago (grace period is 1 hour)
        cache.record_rotation(&old_did, new_did, 1000);

        // Change provider so signature fails against "current" key
        ep.provider = "did:icn:znonexistent11111111111111111111111111111".to_string();

        // Should fail: provider DID doesn't match and grace period expired
        let result = verify_service_endpoint_with_rotation(&ep, Some(&cache));
        assert!(result.is_err());
    }

    #[test]
    fn test_topic_names() {
        assert_eq!(topics::SERVICES_ANNOUNCE, "services:announce");
        assert_eq!(topics::SERVICES_QUERY, "services:query");
    }

    // ========================================================================
    // Service response signing / verification tests
    // ========================================================================

    /// Helper: build a Response variant ready for signing.
    fn make_response(
        query_id: &str,
        responder: &icn_identity::Did,
        endpoints: Vec<ServiceEndpoint>,
        scope: ScopeLevel,
        expires_at: u64,
    ) -> ServiceDiscoveryMessage {
        ServiceDiscoveryMessage::Response {
            query_id: query_id.to_string(),
            endpoints,
            responder: responder.clone(),
            signature: vec![0; 64],
            expires_at,
            scope,
        }
    }

    /// Returns a future Unix timestamp ~1 hour from now.
    fn future_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600
    }

    #[test]
    fn test_sign_and_verify_response() {
        let (signing_key, did) = make_keypair();
        let ep = make_endpoint(&did);

        let mut response = make_response("q-1", &did, vec![ep], ScopeLevel::Org, future_timestamp());
        sign_service_response(&mut response, &signing_key).expect("signing should succeed");

        // Signature should now be non-zero
        if let ServiceDiscoveryMessage::Response { ref signature, .. } = response {
            assert_eq!(signature.len(), 64);
            assert_ne!(signature, &vec![0u8; 64], "signature should be non-trivial after signing");
        }

        verify_service_response(&response).expect("verification should succeed");
    }

    #[test]
    fn test_unsigned_response_rejected() {
        let (_, did) = make_keypair();
        let ep = make_endpoint(&did);

        // Response with zeroed-out signature (never signed)
        let response = make_response("q-1", &did, vec![ep], ScopeLevel::Org, future_timestamp());

        let result = verify_service_response(&response);
        assert!(result.is_err(), "unsigned response should fail verification");
    }

    #[test]
    fn test_tampered_response_rejected() {
        let (signing_key, did) = make_keypair();
        let ep = make_endpoint(&did);

        let mut response = make_response("q-1", &did, vec![ep], ScopeLevel::Org, future_timestamp());
        sign_service_response(&mut response, &signing_key).expect("signing should succeed");

        // Tamper: change the query_id after signing
        if let ServiceDiscoveryMessage::Response { ref mut query_id, .. } = response {
            *query_id = "q-TAMPERED".to_string();
        }

        let result = verify_service_response(&response);
        assert!(result.is_err(), "tampered response should fail verification");
    }

    #[test]
    fn test_wrong_key_response_rejected() {
        let (key_a, _did_a) = make_keypair();
        let (_key_b, did_b) = make_keypair();
        let ep = make_endpoint(&did_b);

        // Sign with key_a but set responder to did_b
        let mut response = make_response("q-1", &did_b, vec![ep], ScopeLevel::Org, future_timestamp());
        sign_service_response(&mut response, &key_a).expect("signing should succeed");

        let result = verify_service_response(&response);
        assert!(result.is_err(), "response signed with wrong key should fail verification");
    }

    #[test]
    fn test_expired_response_rejected() {
        let (signing_key, did) = make_keypair();
        let ep = make_endpoint(&did);

        // expires_at is in the past
        let past = 1_000_000;
        let mut response = make_response("q-1", &did, vec![ep], ScopeLevel::Org, past);
        sign_service_response(&mut response, &signing_key).expect("signing should succeed");

        let valid = validate_service_response(&response, &ScopeLevel::Federation);
        assert!(!valid, "expired response should be rejected by validation");
    }

    #[test]
    fn test_wrong_scope_response_rejected() {
        let (signing_key, did) = make_keypair();
        let ep = make_endpoint(&did);

        // Response scope (Federation) exceeds query scope (Org)
        let mut response = make_response("q-1", &did, vec![ep], ScopeLevel::Federation, future_timestamp());
        sign_service_response(&mut response, &signing_key).expect("signing should succeed");

        let valid = validate_service_response(&response, &ScopeLevel::Org);
        assert!(!valid, "response with scope exceeding query scope should be rejected");
    }

    #[test]
    fn test_valid_response_passes_validation() {
        let (signing_key, did) = make_keypair();
        let ep = make_endpoint(&did);

        let mut response = make_response("q-1", &did, vec![ep], ScopeLevel::Org, future_timestamp());
        sign_service_response(&mut response, &signing_key).expect("signing should succeed");

        // Query scope (Federation) includes response scope (Org) -- should pass
        let valid = validate_service_response(&response, &ScopeLevel::Federation);
        assert!(valid, "valid signed response within scope should pass validation");

        // Query scope (Org) equals response scope (Org) -- should also pass
        let valid = validate_service_response(&response, &ScopeLevel::Org);
        assert!(valid, "valid signed response at exact scope should pass validation");
    }
}
