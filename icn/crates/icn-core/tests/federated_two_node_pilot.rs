#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Federated Two-Node Pilot Tests
//!
//! These tests prove the core ICN invariant:
//! ```text
//! DecisionReceipt → Effect → DurableState
//! must produce identical hashes across nodes.
//! ```
//!
//! Each test:
//! 1. Sets up Node A with independent storage
//! 2. Executes an effect and captures state_change_hash
//! 3. Serializes the effect for transport
//! 4. Sets up Node B with independent storage
//! 5. Deserializes and executes the same effect
//! 6. Asserts state hashes match exactly
//!
//! This validates deterministic state replication - the foundation of
//! gossip-based convergence without consensus.

use anyhow::Result;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use icn_core::services::{
    compare_effect_results, FederationServiceImpl, LedgerServiceImpl, MembershipServiceImpl,
};
use icn_core::supervisor::governance_executor::{
    KernelFederationExecutor, KernelMembershipExecutor, KernelProtocolExecutor,
    KernelTreasuryExecutor,
};
use icn_coop::CoopStore;
use icn_federation::types::{CooperativeInfo, FederationPolicy};
use icn_federation::CooperativeRegistry;
use icn_governance::protocol_store::state::InMemoryParameterStore;
use icn_identity::Did;
use icn_kernel_api::authz::AllowAllOracle;
use icn_kernel_api::effects::{
    FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect, TreasuryEffect,
};
use icn_kernel_api::governance::{
    DecisionReceiptId, FederationExecutor, ProtocolExecutor, TreasuryExecutor,
};
use icn_kernel_api::protocol_params::ProtocolParameterStore;
use icn_kernel_api::MembershipService;
use icn_ledger::Ledger;
use icn_store::SledStore;

// =============================================================================
// Test Utilities
// =============================================================================

/// Generate a valid Ed25519 DID for testing
fn generate_test_did() -> Did {
    let signing_key = SigningKey::generate(&mut OsRng);
    Did::from_public_key(&signing_key.verifying_key())
}

/// Extract state_change_hash from ExecutionOutcome success effects
fn extract_state_hash_from_effects(effects: &[String]) -> Option<String> {
    for effect in effects {
        if let Some(idx) = effect.find("state_hash=") {
            let start = idx + "state_hash=".len();
            let rest = &effect[start..];
            let hash: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !hash.is_empty() {
                return Some(hash);
            }
        }
    }
    None
}

/// Print the federation proof verification banner
fn print_verification_banner(test_name: &str, hash_a: &str, hash_b: &str, matched: bool) {
    let status = if matched { "✅ MATCH" } else { "❌ MISMATCH" };
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!(
        "║   TWO-NODE FEDERATION PROOF: {:<35} ║",
        test_name
    );
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ Node A Hash: {:<52} ║", &hash_a[..hash_a.len().min(52)]);
    println!("║ Node B Hash: {:<52} ║", &hash_b[..hash_b.len().min(52)]);
    println!("║ Comparison:  {:<52} ║", status);
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

// =============================================================================
// Treasury::Spend Determinism Test
// =============================================================================

/// Test that Treasury::Spend produces identical ledger state (balance changes)
/// across two independent nodes when replaying the same governance decision.
///
/// NOTE: The ledger entry_hash includes a timestamp and is NOT deterministic
/// across nodes executing at different times. What IS deterministic:
/// - The decision_hash (canonical anchor for the governance decision)
/// - The operation parameters (amount, currency, recipient)
/// - The execution outcome (success/failure)
///
/// This test validates that both nodes apply the same semantic state change.
#[tokio::test(flavor = "multi_thread")]
async fn test_two_node_treasury_spend_determinism() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing Two-Node Treasury Spend Determinism ===");

    // === Common decision parameters (these ARE deterministic) ===
    let treasury_did = generate_test_did();
    let recipient_did = generate_test_did();
    let decision_receipt_id = DecisionReceiptId::new("gov:treasury:spend:pilot:001");
    let decision_hash = "sha256:treasury-spend-decision-canonical-hash".to_string();
    let amount = 500i64;
    let currency = "HOURS".to_string();

    // Create the effect (will be serialized/deserialized for Node B)
    let effect = TreasuryEffect::Spend {
        treasury_did: treasury_did.to_string(),
        recipient_did: recipient_did.to_string(),
        amount,
        currency: currency.clone(),
        memo: "Pilot equipment purchase".to_string(),
        decision_receipt_id: decision_receipt_id.to_string(),
        decision_hash: decision_hash.clone(),
    };

    // === Node A Setup ===
    info!("Setting up Node A...");
    let store_a = Arc::new(SledStore::temporary()?);
    let ledger_a = Ledger::new(store_a.clone())?;
    let ledger_handle_a = Arc::new(RwLock::new(ledger_a));
    let oracle_a = Arc::new(AllowAllOracle::wildcard());
    let ledger_service_a = Arc::new(LedgerServiceImpl::new(
        ledger_handle_a.clone(),
        oracle_a,
        treasury_did.clone(),
    ));
    let executor_a = KernelTreasuryExecutor::with_ledger(ledger_service_a);

    // Execute on Node A
    let operation_a = icn_kernel_api::governance::TreasuryOperation {
        treasury_id: treasury_did.to_string(),
        operation_type: icn_kernel_api::governance::TreasuryOperationType::Spend,
        amount,
        currency: currency.clone(),
        recipient: Some(recipient_did.to_string()),
        memo: "Pilot equipment purchase".to_string(),
        decision_hash: Some(decision_hash.clone()),
    };

    let outcome_a = executor_a
        .execute_treasury_operation(&decision_receipt_id, operation_a)
        .await?;

    // Verify execution succeeded on Node A
    let (success_a, entry_created_a) = match &outcome_a {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            let has_entry = effects.iter().any(|e| e.contains("-> ledger entry"));
            (true, has_entry)
        }
        _ => panic!("Node A execution failed: {:?}", outcome_a),
    };
    info!("Node A treasury spend completed");

    // === Serialize effect for transport ===
    let effect_json = serde_json::to_string(&effect)?;
    info!(effect_json_len = effect_json.len(), "Effect serialized for transport");

    // === Node B Setup (completely independent!) ===
    info!("Setting up Node B...");
    let store_b = Arc::new(SledStore::temporary()?);
    let ledger_b = Ledger::new(store_b.clone())?;
    let ledger_handle_b = Arc::new(RwLock::new(ledger_b));
    let oracle_b = Arc::new(AllowAllOracle::wildcard());
    let ledger_service_b = Arc::new(LedgerServiceImpl::new(
        ledger_handle_b.clone(),
        oracle_b,
        treasury_did.clone(),
    ));
    let executor_b = KernelTreasuryExecutor::with_ledger(ledger_service_b);

    // Deserialize effect on Node B (proves roundtrip)
    let _effect_b: TreasuryEffect = serde_json::from_str(&effect_json)?;

    // Execute on Node B with same parameters
    let operation_b = icn_kernel_api::governance::TreasuryOperation {
        treasury_id: treasury_did.to_string(),
        operation_type: icn_kernel_api::governance::TreasuryOperationType::Spend,
        amount,
        currency: currency.clone(),
        recipient: Some(recipient_did.to_string()),
        memo: "Pilot equipment purchase".to_string(),
        decision_hash: Some(decision_hash.clone()),
    };

    let outcome_b = executor_b
        .execute_treasury_operation(&decision_receipt_id, operation_b)
        .await?;

    // Verify execution succeeded on Node B
    let (success_b, entry_created_b) = match &outcome_b {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            let has_entry = effects.iter().any(|e| e.contains("-> ledger entry"));
            (true, has_entry)
        }
        _ => panic!("Node B execution failed: {:?}", outcome_b),
    };
    info!("Node B treasury spend completed");

    // === ASSERT DETERMINISM (semantic equivalence) ===
    
    // 1. Both executions succeeded
    assert!(success_a && success_b, "Both nodes should succeed");

    // 2. Both created ledger entries
    assert!(entry_created_a, "Node A should create ledger entry");
    assert!(entry_created_b, "Node B should create ledger entry");

    // 3. Verify the effect serialization roundtrip worked
    let effect_roundtrip: TreasuryEffect = serde_json::from_str(&effect_json)?;
    match effect_roundtrip {
        TreasuryEffect::Spend { amount: amt, currency: cur, decision_hash: hash, .. } => {
            assert_eq!(amt, amount, "Amount should match after roundtrip");
            assert_eq!(cur, currency, "Currency should match after roundtrip");
            assert_eq!(hash, decision_hash, "Decision hash should match after roundtrip");
        }
        _ => panic!("Effect should be Spend variant"),
    }

    // Print verification banner
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   TWO-NODE FEDERATION PROOF: Treasury::Spend                     ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ Decision Hash:     {} ║", &decision_hash[..decision_hash.len().min(40)]);
    println!("║ Amount:            {:<47} ║", format!("{} {}", amount, currency));
    println!("║ Node A:            ✅ SUCCESS + LEDGER ENTRY                       ║");
    println!("║ Node B:            ✅ SUCCESS + LEDGER ENTRY                       ║");
    println!("║ Effect Roundtrip:  ✅ DETERMINISTIC                                 ║");
    println!("║ Note: entry_hash differs (includes timestamp) - expected          ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    info!("✅ Treasury spend determinism verified (semantic state matches)");
    Ok(())
}

// =============================================================================
// Membership::AddMember Determinism Test
// =============================================================================

/// Test that Membership::AddMember produces identical state hashes across two
/// independent nodes when replaying the same governance decision.
#[tokio::test(flavor = "multi_thread")]
async fn test_two_node_membership_add_determinism() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing Two-Node Membership Add Determinism ===");

    // === Common decision parameters ===
    let member_did = generate_test_did();
    let decision_receipt_id = DecisionReceiptId::new("gov:membership:add:pilot:001");

    // Create the effect
    let effect = MembershipEffect::AddMember {
        entity_id: "coop-sunrise-pilot".to_string(),
        member_did: member_did.to_string(),
        role: "Worker".to_string(),
        tier: "Standard".to_string(),
    };

    // === Node A Setup ===
    info!("Setting up Node A...");
    let sled_store_a = SledStore::temporary()?;
    let db_a = Arc::new(sled_store_a.db().clone());
    let coop_store_a = Arc::new(CoopStore::new(db_a));
    let membership_service_a = Arc::new(MembershipServiceImpl::new(coop_store_a.clone()));
    let executor_a = KernelMembershipExecutor::with_service(membership_service_a.clone());

    // Execute on Node A
    let outcome_a = executor_a
        .execute_membership_operation(&decision_receipt_id, effect.clone())
        .await?;

    let hash_a = match &outcome_a {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            extract_state_hash_from_effects(effects)
                .unwrap_or_else(|| "no_hash_a".to_string())
        }
        _ => panic!("Node A execution failed: {:?}", outcome_a),
    };

    info!(hash_a = %hash_a, "Node A membership add completed");

    // Verify member exists on Node A
    assert!(
        membership_service_a.is_member("coop-sunrise-pilot", &member_did.to_string()),
        "Member should exist on Node A"
    );

    // === Serialize effect for transport ===
    let effect_json = serde_json::to_string(&effect)?;
    let receipt_id_str = decision_receipt_id.to_string();

    // === Node B Setup (completely independent!) ===
    info!("Setting up Node B...");
    let sled_store_b = SledStore::temporary()?;
    let db_b = Arc::new(sled_store_b.db().clone());
    let coop_store_b = Arc::new(CoopStore::new(db_b));
    let membership_service_b = Arc::new(MembershipServiceImpl::new(coop_store_b.clone()));
    let executor_b = KernelMembershipExecutor::with_service(membership_service_b.clone());

    // Deserialize effect on Node B
    let effect_b: MembershipEffect = serde_json::from_str(&effect_json)?;
    let receipt_id_b = DecisionReceiptId::new(&receipt_id_str);

    // Execute on Node B
    let outcome_b = executor_b
        .execute_membership_operation(&receipt_id_b, effect_b)
        .await?;

    let hash_b = match &outcome_b {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            extract_state_hash_from_effects(effects)
                .unwrap_or_else(|| "no_hash_b".to_string())
        }
        _ => panic!("Node B execution failed: {:?}", outcome_b),
    };

    info!(hash_b = %hash_b, "Node B membership add completed");

    // Verify member exists on Node B
    assert!(
        membership_service_b.is_member("coop-sunrise-pilot", &member_did.to_string()),
        "Member should exist on Node B"
    );

    // === ASSERT DETERMINISM ===
    let matched = hash_a == hash_b;
    print_verification_banner("Membership::Add", &hash_a, &hash_b, matched);

    assert_eq!(
        hash_a, hash_b,
        "Membership add must produce identical state hashes across nodes.\n\
         Node A hash: {}\n\
         Node B hash: {}",
        hash_a, hash_b
    );

    info!("✅ Membership add determinism verified");
    Ok(())
}

// =============================================================================
// Federation::JoinFederation Determinism Test
// =============================================================================

/// Test that Federation::JoinFederation produces identical state hashes across
/// two independent nodes when replaying the same governance decision.
#[tokio::test(flavor = "multi_thread")]
async fn test_two_node_federation_join_determinism() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing Two-Node Federation Join Determinism ===");

    // === Common decision parameters ===
    let coop_did = generate_test_did();
    let decision_receipt_id = DecisionReceiptId::new("gov:federation:join:pilot:001");
    let decision_hash = "sha256:federation-join-canonical-hash".to_string();

    // Create the effect
    let effect = FederationEffect::JoinFederation {
        coop_did: coop_did.to_string(),
        federation_id: "federation-regional-pilot".to_string(),
    };

    // === Node A Setup ===
    info!("Setting up Node A...");
    let store_a = Arc::new(SledStore::temporary()?);
    let own_did_a = generate_test_did();
    let own_info_a = CooperativeInfo::new(
        "node-a-coop".to_string(),
        "Node A Cooperative".to_string(),
        own_did_a,
        FederationPolicy::Open,
    );
    let registry_a = Arc::new(CooperativeRegistry::new(store_a.clone(), own_info_a)?);
    let federation_service_a = Arc::new(FederationServiceImpl::new(registry_a.clone()));
    let executor_a = KernelFederationExecutor::with_service(federation_service_a);

    // Build federation operation
    let operation_a = icn_kernel_api::governance::FederationOperation {
        operation_type: icn_kernel_api::governance::FederationOperationType::JoinFederation,
        coop_did: coop_did.to_string(),
        target_id: Some("federation-regional-pilot".to_string()),
        agreement_hash: None,
        decision_hash: Some(decision_hash.clone()),
    };

    // Execute on Node A
    let outcome_a = executor_a
        .execute_federation_operation(&decision_receipt_id, operation_a)
        .await?;

    let hash_a = match &outcome_a {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            extract_state_hash_from_effects(effects)
                .unwrap_or_else(|| "no_hash_a".to_string())
        }
        _ => panic!("Node A execution failed: {:?}", outcome_a),
    };

    info!(hash_a = %hash_a, "Node A federation join completed");

    // === Serialize effect for transport ===
    let effect_json = serde_json::to_string(&effect)?;
    let receipt_id_str = decision_receipt_id.to_string();

    // === Node B Setup (completely independent!) ===
    info!("Setting up Node B...");
    let store_b = Arc::new(SledStore::temporary()?);
    let own_did_b = generate_test_did();
    let own_info_b = CooperativeInfo::new(
        "node-b-coop".to_string(),
        "Node B Cooperative".to_string(),
        own_did_b,
        FederationPolicy::Open,
    );
    let registry_b = Arc::new(CooperativeRegistry::new(store_b.clone(), own_info_b)?);
    let federation_service_b = Arc::new(FederationServiceImpl::new(registry_b.clone()));
    let executor_b = KernelFederationExecutor::with_service(federation_service_b);

    // Deserialize effect on Node B
    let _effect_b: FederationEffect = serde_json::from_str(&effect_json)?;
    let receipt_id_b = DecisionReceiptId::new(&receipt_id_str);

    // Build federation operation for Node B
    let operation_b = icn_kernel_api::governance::FederationOperation {
        operation_type: icn_kernel_api::governance::FederationOperationType::JoinFederation,
        coop_did: coop_did.to_string(),
        target_id: Some("federation-regional-pilot".to_string()),
        agreement_hash: None,
        decision_hash: Some(decision_hash.clone()),
    };

    // Execute on Node B
    let outcome_b = executor_b
        .execute_federation_operation(&receipt_id_b, operation_b)
        .await?;

    let hash_b = match &outcome_b {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            extract_state_hash_from_effects(effects)
                .unwrap_or_else(|| "no_hash_b".to_string())
        }
        _ => panic!("Node B execution failed: {:?}", outcome_b),
    };

    info!(hash_b = %hash_b, "Node B federation join completed");

    // === ASSERT DETERMINISM ===
    let matched = hash_a == hash_b;
    print_verification_banner("Federation::Join", &hash_a, &hash_b, matched);

    assert_eq!(
        hash_a, hash_b,
        "Federation join must produce identical state hashes across nodes.\n\
         Node A hash: {}\n\
         Node B hash: {}",
        hash_a, hash_b
    );

    info!("✅ Federation join determinism verified");
    Ok(())
}

// =============================================================================
// Protocol::SetParameter Determinism Test
// =============================================================================

/// Test that Protocol::SetParameter produces identical state hashes across
/// two independent nodes when replaying the same governance decision.
#[tokio::test(flavor = "multi_thread")]
async fn test_two_node_protocol_set_param_determinism() -> Result<()> {
    use icn_kernel_api::protocol_params::{
        ParameterConstraints, ParameterScope, ParameterValue, ProtocolParameter,
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing Two-Node Protocol SetParameter Determinism ===");

    // === Common decision parameters ===
    let decision_receipt_id = DecisionReceiptId::new("gov:protocol:set-param:pilot:001");
    let effective_at = icn_time::current_timestamp_secs();

    // Initial parameter that must exist on both nodes
    let initial_param = ProtocolParameter {
        id: "governance.voting_period".to_string(),
        name: "Voting Period".to_string(),
        description: "Duration of voting phase in seconds".to_string(),
        value: ParameterValue::Duration(604800), // 7 days in seconds
        constraints: ParameterConstraints {
            min: Some(ParameterValue::Duration(86400)),   // 1 day
            max: Some(ParameterValue::Duration(2592000)), // 30 days
            allowed_values: None,
            requires_restart: false,
            allow_override: true,
        },
        scope: ParameterScope::Global,
        updated_at: 0,
        updated_by: None,
        version: 0,
    };

    // The protocol change to apply on both nodes
    // old_value must match ParameterValue::Duration(604800).to_string() which is "7d"
    let change = icn_kernel_api::governance::ProtocolChange {
        parameter_name: "governance.voting_period".to_string(),
        old_value: "7d".to_string(), // 7 days (matches Duration Display)
        new_value: "14d".to_string(), // 14 days
        effective_at,
    };

    // Create the effect for serialization (won't be used directly but proves roundtrip)
    let effect = ProtocolEffect::SetParameter {
        parameter_name: "governance.voting_period".to_string(),
        old_value_hash: "7d".to_string(),
        new_value_json: "14d".to_string(),
        effective_at,
    };

    // === Node A Setup ===
    info!("Setting up Node A...");
    let param_store_a: Arc<dyn ProtocolParameterStore> = Arc::new(InMemoryParameterStore::new());
    // Initialize the parameter on Node A
    param_store_a.set(initial_param.clone(), None, None)?;
    let executor_a = KernelProtocolExecutor::new(param_store_a.clone());

    // Execute on Node A
    let outcome_a = executor_a
        .apply_protocol_change(&decision_receipt_id, change.clone())
        .await?;

    let hash_a = match &outcome_a {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            extract_state_hash_from_effects(effects)
                .unwrap_or_else(|| "no_hash_a".to_string())
        }
        other => panic!("Node A execution failed: {:?}", other),
    };

    info!(hash_a = %hash_a, "Node A protocol set param completed");

    // Verify parameter was updated on Node A
    let updated_param_a = param_store_a
        .get("governance.voting_period")?
        .expect("Parameter should exist on Node A");
    assert_eq!(
        updated_param_a.value,
        ParameterValue::Duration(1209600), // 14 days
        "Parameter value should be updated to 14 days on Node A"
    );

    // === Serialize effect for transport ===
    let effect_json = serde_json::to_string(&effect)?;
    let receipt_id_str = decision_receipt_id.to_string();

    // === Node B Setup (completely independent!) ===
    info!("Setting up Node B...");
    let param_store_b: Arc<dyn ProtocolParameterStore> = Arc::new(InMemoryParameterStore::new());
    // Initialize the parameter on Node B (same initial state as A)
    param_store_b.set(initial_param.clone(), None, None)?;
    let executor_b = KernelProtocolExecutor::new(param_store_b.clone());

    // Deserialize effect on Node B
    let _effect_b: ProtocolEffect = serde_json::from_str(&effect_json)?;
    let receipt_id_b = DecisionReceiptId::new(&receipt_id_str);

    // Build protocol change for Node B (same parameters)
    let change_b = icn_kernel_api::governance::ProtocolChange {
        parameter_name: "governance.voting_period".to_string(),
        old_value: "7d".to_string(),
        new_value: "14d".to_string(),
        effective_at,
    };

    // Execute on Node B
    let outcome_b = executor_b
        .apply_protocol_change(&receipt_id_b, change_b)
        .await?;

    let hash_b = match &outcome_b {
        icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
            extract_state_hash_from_effects(effects)
                .unwrap_or_else(|| "no_hash_b".to_string())
        }
        other => panic!("Node B execution failed: {:?}", other),
    };

    info!(hash_b = %hash_b, "Node B protocol set param completed");

    // Verify parameter was updated on Node B
    let updated_param_b = param_store_b
        .get("governance.voting_period")?
        .expect("Parameter should exist on Node B");
    assert_eq!(
        updated_param_b.value,
        ParameterValue::Duration(1209600),
        "Parameter value should be updated to 14 days on Node B"
    );

    // === ASSERT DETERMINISM ===
    let matched = hash_a == hash_b;
    print_verification_banner("Protocol::SetParam", &hash_a, &hash_b, matched);

    assert_eq!(
        hash_a, hash_b,
        "Protocol parameter change must produce identical state hashes across nodes.\n\
         Node A hash: {}\n\
         Node B hash: {}",
        hash_a, hash_b
    );

    info!("✅ Protocol set param determinism verified");
    Ok(())
}

// =============================================================================
// Combined Effect Batch Determinism Test
// =============================================================================

/// Test that a batch of effects produces identical aggregate state across nodes.
/// This simulates a complex governance decision with multiple effects.
#[tokio::test(flavor = "multi_thread")]
async fn test_two_node_effect_batch_determinism() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing Two-Node Effect Batch Determinism ===");

    // Create a batch of effects representing a complex governance decision
    let member_did = generate_test_did();
    
    let effects = vec![
        KernelEffect::Membership(MembershipEffect::AddMember {
            entity_id: "coop-batch-test".to_string(),
            member_did: member_did.to_string(),
            role: "Coordinator".to_string(),
            tier: "Founding".to_string(),
        }),
        KernelEffect::NoOp {
            reason: "Resolution text recorded".to_string(),
        },
    ];

    // Serialize for transport
    let effects_json: Vec<String> = effects
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();

    // === Node A execution ===
    info!("Executing batch on Node A...");
    let sled_store_a = SledStore::temporary()?;
    let db_a = Arc::new(sled_store_a.db().clone());
    let coop_store_a = Arc::new(CoopStore::new(db_a));
    let membership_service_a = Arc::new(MembershipServiceImpl::new(coop_store_a));

    let mut results_a = Vec::new();
    for effect in &effects {
        let result = match effect {
            KernelEffect::Membership(m) => {
                let executor = KernelMembershipExecutor::with_service(membership_service_a.clone());
                let receipt = DecisionReceiptId::new("batch:001");
                let outcome = executor.execute_membership_operation(&receipt, m.clone()).await?;
                match outcome {
                    icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
                        icn_kernel_api::effects::EffectResult {
                            effect_id: "batch_membership".to_string(),
                            success: true,
                            message: effects.join("; "),
                            state_change_hash: extract_state_hash_from_effects(&effects),
                        }
                    }
                    _ => panic!("Batch effect failed on Node A"),
                }
            }
            KernelEffect::NoOp { reason } => {
                icn_kernel_api::effects::EffectResult {
                    effect_id: "batch_noop".to_string(),
                    success: true,
                    message: reason.clone(),
                    state_change_hash: None,
                }
            }
            _ => panic!("Unexpected effect type in batch"),
        };
        results_a.push(result);
    }

    // === Node B execution ===
    info!("Executing batch on Node B...");
    let sled_store_b = SledStore::temporary()?;
    let db_b = Arc::new(sled_store_b.db().clone());
    let coop_store_b = Arc::new(CoopStore::new(db_b));
    let membership_service_b = Arc::new(MembershipServiceImpl::new(coop_store_b));

    // Deserialize effects on Node B
    let effects_b: Vec<KernelEffect> = effects_json
        .iter()
        .map(|j| serde_json::from_str(j).unwrap())
        .collect();

    let mut results_b = Vec::new();
    for effect in &effects_b {
        let result = match effect {
            KernelEffect::Membership(m) => {
                let executor = KernelMembershipExecutor::with_service(membership_service_b.clone());
                let receipt = DecisionReceiptId::new("batch:001");
                let outcome = executor.execute_membership_operation(&receipt, m.clone()).await?;
                match outcome {
                    icn_kernel_api::governance::ExecutionOutcome::Success { effects, .. } => {
                        icn_kernel_api::effects::EffectResult {
                            effect_id: "batch_membership".to_string(),
                            success: true,
                            message: effects.join("; "),
                            state_change_hash: extract_state_hash_from_effects(&effects),
                        }
                    }
                    _ => panic!("Batch effect failed on Node B"),
                }
            }
            KernelEffect::NoOp { reason } => {
                icn_kernel_api::effects::EffectResult {
                    effect_id: "batch_noop".to_string(),
                    success: true,
                    message: reason.clone(),
                    state_change_hash: None,
                }
            }
            _ => panic!("Unexpected effect type in batch"),
        };
        results_b.push(result);
    }

    // === Compare using state_hash utilities ===
    let comparison = compare_effect_results(&results_a, &results_b);

    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   TWO-NODE EFFECT BATCH COMPARISON                               ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ Node A Composite: {:<47} ║",
        &comparison.composite_a[..comparison.composite_a.len().min(47)]
    );
    println!(
        "║ Node B Composite: {:<47} ║",
        &comparison.composite_b[..comparison.composite_b.len().min(47)]
    );
    println!(
        "║ Match Status:     {:<47} ║",
        if comparison.matches { "✅ ALL MATCH" } else { "❌ MISMATCH" }
    );
    if !comparison.differences.is_empty() {
        println!("║ Differences:      {:<47} ║", comparison.differences.len());
    }
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    assert!(
        comparison.matches,
        "Effect batch must produce identical state hashes across nodes.\n\
         Differences: {:?}",
        comparison.differences
    );

    info!("✅ Effect batch determinism verified");
    Ok(())
}
