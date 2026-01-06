//! Integration tests for cross-cooperative task execution (Phase 21).
//!
//! These tests verify federation functionality including:
//! - Trust attenuation
//! - Federation policy enforcement
//! - Federated executor discovery
//! - Cross-cooperative task routing

// Allow unwrap in tests - panics are acceptable
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_compute::{
    FederatedExecutorAttestation, FederatedExecutorInfo, FederatedPaymentTerms,
    FederatedPlacementConstraints, FederationPolicy, PaymentTrigger,
};
use icn_federation::{FederatedTrustAttestation, TrustContext};

/// Test trust attenuation formula correctness
#[test]
fn test_trust_attenuation_formula() {
    // Test case 1: High local trust, high coop trust
    // Formula: local_trust × coop_trust × 0.9 (default attenuation)
    let result = icn_compute::FederatedExecutorRegistry::attenuate_trust_static(0.9, 0.8);
    let expected = 0.9 * 0.8 * 0.9; // 0.648
    assert!(
        (result - expected).abs() < 0.001,
        "Expected {expected}, got {result}"
    );

    // Test case 2: Low local trust
    let result = icn_compute::FederatedExecutorRegistry::attenuate_trust_static(0.3, 0.8);
    let expected = 0.3 * 0.8 * 0.9; // 0.216
    assert!(
        (result - expected).abs() < 0.001,
        "Expected {expected}, got {result}"
    );

    // Test case 3: Low coop trust
    let result = icn_compute::FederatedExecutorRegistry::attenuate_trust_static(0.9, 0.2);
    let expected = 0.9 * 0.2 * 0.9; // 0.162
    assert!(
        (result - expected).abs() < 0.001,
        "Expected {expected}, got {result}"
    );

    // Test case 4: Both low
    let result = icn_compute::FederatedExecutorRegistry::attenuate_trust_static(0.3, 0.2);
    let expected = 0.3 * 0.2 * 0.9; // 0.054
    assert!(
        (result - expected).abs() < 0.001,
        "Expected {expected}, got {result}"
    );
}

/// Test trust attenuation clamps out-of-range inputs
#[test]
fn test_trust_attenuation_clamps() {
    // Values > 1.0 should be clamped
    let result = icn_compute::FederatedExecutorRegistry::attenuate_trust_static(1.5, 1.2);
    let expected = 1.0 * 1.0 * 0.9; // 0.9
    assert!(
        (result - expected).abs() < 0.001,
        "Expected {expected}, got {result}"
    );

    // Negative values should be clamped to 0
    let result = icn_compute::FederatedExecutorRegistry::attenuate_trust_static(-0.5, -0.3);
    assert!(result.abs() < 0.001, "Expected 0.0, got {result}");
}

/// Test LocalOnly policy rejects federated executors
#[test]
fn test_federation_policy_local_only() {
    let constraints = FederatedPlacementConstraints {
        federation_policy: FederationPolicy::LocalOnly,
        ..Default::default()
    };

    // Local executor (no coop_id or same coop) should be allowed
    assert!(constraints.allows_cooperative(None, true));
    assert!(constraints.allows_cooperative(Some("coop-a"), true));

    // Federated executor should be rejected
    assert!(!constraints.allows_cooperative(Some("coop-b"), false));
}

/// Test AllowFederated policy accepts all executors
#[test]
fn test_federation_policy_allow_federated() {
    let constraints = FederatedPlacementConstraints {
        federation_policy: FederationPolicy::AllowFederated,
        ..Default::default()
    };

    // All executors should be allowed
    assert!(constraints.allows_cooperative(None, true));
    assert!(constraints.allows_cooperative(Some("coop-a"), true));
    assert!(constraints.allows_cooperative(Some("coop-b"), false));
    assert!(constraints.allows_cooperative(Some("coop-c"), false));
}

/// Test FederatedWhitelist policy
#[test]
fn test_federation_policy_whitelist() {
    let constraints = FederatedPlacementConstraints {
        federation_policy: FederationPolicy::FederatedWhitelist {
            cooperatives: vec!["coop-a".to_string(), "coop-b".to_string()],
        },
        ..Default::default()
    };

    // Local executor should always be allowed
    assert!(constraints.allows_cooperative(None, true));

    // Whitelisted cooperatives should be allowed
    assert!(constraints.allows_cooperative(Some("coop-a"), false));
    assert!(constraints.allows_cooperative(Some("coop-b"), false));

    // Non-whitelisted cooperative should be rejected
    assert!(!constraints.allows_cooperative(Some("coop-c"), false));
}

/// Test PreferLocal policy (default)
#[test]
fn test_federation_policy_prefer_local() {
    let constraints = FederatedPlacementConstraints {
        federation_policy: FederationPolicy::PreferLocal,
        ..Default::default()
    };

    // All executors should be allowed (preference is handled in scoring)
    assert!(constraints.allows_cooperative(None, true));
    assert!(constraints.allows_cooperative(Some("coop-a"), true));
    assert!(constraints.allows_cooperative(Some("coop-b"), false));
}

/// Test effective minimum trust threshold
#[test]
fn test_effective_min_trust() {
    // Default should be 0.3
    let constraints = FederatedPlacementConstraints::default();
    assert!((constraints.effective_min_trust() - 0.3).abs() < 0.001);

    // Custom threshold
    let constraints = FederatedPlacementConstraints {
        min_federated_trust: Some(0.5),
        ..Default::default()
    };
    assert!((constraints.effective_min_trust() - 0.5).abs() < 0.001);
}

/// Test FederatedPaymentTerms default values
#[test]
fn test_federated_payment_terms_default() {
    let terms = FederatedPaymentTerms::default();
    assert_eq!(terms.amount, 0);
    assert_eq!(terms.source_currency, "ICN");
    assert_eq!(terms.dest_currency, "ICN");
    assert_eq!(terms.payment_trigger, PaymentTrigger::Completion);
    assert!((terms.max_exchange_variance - 0.05).abs() < 0.001);
}

/// Test FederatedExecutorInfo structure
#[test]
fn test_federated_executor_info() {
    let info = FederatedExecutorInfo {
        did: "did:icn:coop-a:z6MkTest".to_string(),
        cooperative_id: "coop-a".to_string(),
        capabilities: vec![icn_compute::ExecutorCapability::Ccl],
        local_trust_score: 0.8,
        federated_trust_score: 0.576, // 0.8 * 0.8 * 0.9
        last_seen: 1000000,
        capacity: None,
        gateway_endpoint: Some("https://coop-a.example.com".to_string()),
    };

    assert_eq!(info.cooperative_id, "coop-a");
    assert!(info.federated_trust_score < info.local_trust_score);
    assert!(info.gateway_endpoint.is_some());
}

/// Test FederatedExecutorAttestation expiry checking
#[test]
fn test_federated_executor_attestation_expiry() {
    let now_secs = icn_time::current_timestamp_millis() / 1000;

    // Generate test DIDs
    let coop_keypair = icn_identity::KeyPair::generate().unwrap();
    let exec_keypair = icn_identity::KeyPair::generate().unwrap();
    let coop_did = coop_keypair.did();
    let exec_did = exec_keypair.did();

    // Create expired attestation
    let expired = FederatedExecutorAttestation {
        source_coop_id: "coop-a".to_string(),
        executor_did: "did:icn:coop-a:test".to_string(),
        trust_attestation: FederatedTrustAttestation::new(
            "coop-a".to_string(),
            coop_did.clone(),
            exec_did.clone(),
            0.8,
            TrustContext::General,
            0, // Already expired
        ),
        capabilities: vec![],
        capacity: None,
        gateway_endpoint: None,
        issued_at: now_secs - 3600,
        expires_at: now_secs - 1, // 1 second ago
        signature: vec![],
    };
    assert!(expired.is_expired());
    assert_eq!(expired.remaining_validity(), 0);

    // Create valid attestation
    let valid = FederatedExecutorAttestation {
        source_coop_id: "coop-a".to_string(),
        executor_did: "did:icn:coop-a:test".to_string(),
        trust_attestation: FederatedTrustAttestation::new(
            "coop-a".to_string(),
            coop_did.clone(),
            exec_did.clone(),
            0.8,
            TrustContext::General,
            3600,
        ),
        capabilities: vec![],
        capacity: None,
        gateway_endpoint: Some("https://coop-a.example.com".to_string()),
        issued_at: now_secs,
        expires_at: now_secs + 3600, // 1 hour from now
        signature: vec![],
    };
    assert!(!valid.is_expired());
    assert!(valid.remaining_validity() > 3500);
}

/// Test local preference weight
#[test]
fn test_local_preference_weight() {
    // Default should be 0.15
    let constraints = FederatedPlacementConstraints::default();
    assert!(
        (constraints.effective_local_preference() - 0.15).abs() < 0.001,
        "Expected 0.15, got {}",
        constraints.effective_local_preference()
    );

    // Custom preference
    let constraints = FederatedPlacementConstraints {
        local_preference_weight: Some(1.5),
        ..Default::default()
    };
    assert!((constraints.effective_local_preference() - 1.5).abs() < 0.001);
}
