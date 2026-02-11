#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Economic Chain Two-Node Provenance Test (INT-5)
//!
//! This test proves the complete economic execution chain:
//! ```text
//! DecisionReceipt → AllocationReceipt → SettlementIntent → TreasuryEntryRequest
//! ```
//!
//! Invariant: Two nodes independently constructing the same economic chain
//! from the same decision parameters must produce identical canonical hashes
//! at every step.
//!
//! This is the economic equivalent of the governance determinism proofs
//! in `federated_two_node_pilot.rs`.

use icn_kernel_api::economics::{AssetType, DepreciationSchedule, SettlementIntent};
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use icn_kernel_api::services::TreasuryEntryRequest;
use icn_kernel_api::ScopeLevel;

// =============================================================================
// Test Utilities
// =============================================================================

/// Print the economic chain verification banner
fn print_chain_verification(test_name: &str, hash_a: &Hash, hash_b: &Hash, label: &str) {
    let hash_a_hex = hex::encode(hash_a);
    let hash_b_hex = hex::encode(hash_b);
    let matched = hash_a == hash_b;
    let status = if matched { "✅ MATCH" } else { "❌ MISMATCH" };

    println!("║ {:<20} ║", label);
    println!("║   Node A: {}... ║", &hash_a_hex[..16]);
    println!("║   Node B: {}... ║", &hash_b_hex[..16]);
    println!("║   Status: {:<52} ║", status);

    assert_eq!(hash_a, hash_b, "{} {} hash mismatch", test_name, label);
}

// =============================================================================
// Test: Full Economic Chain Determinism
// =============================================================================

/// Test that the complete economic chain produces identical hashes on two nodes.
///
/// This simulates:
/// - Node A: Creates the economic chain independently
/// - Node B: Creates the same chain with same inputs
/// - Verification: All canonical hashes must match exactly
#[test]
fn test_economic_chain_canonical_hash_determinism() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   ECONOMIC CHAIN TWO-NODE DETERMINISM TEST (INT-5)              ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    // === Common decision parameters (shared "gossiped" state) ===
    let decision_hash: Hash = [42u8; 32];
    let decision_receipt_id = "gov:treasury:allocation:pilot:001";
    let timestamp = 1704067200u64; // 2024-01-01 00:00:00 UTC

    // === Node A: Construct economic chain ===
    println!("║ Node A: Constructing economic chain...                           ║");

    // Step 1: Create AllocationReceipt
    let allocation_a = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(timestamp)
        .with_receipt_id("node-a-alloc-001");

    // Step 2: Create SettlementIntent(s)
    let intent_a1 = SettlementIntent::new(
        decision_receipt_id,
        decision_hash,
        "did:icn:treasury-main",
        "did:icn:member-alice",
        1000,
        "HOURS",
    )
    .with_timestamp(timestamp)
    .with_intent_id("node-a-intent-001");

    let intent_a2 = SettlementIntent::new(
        decision_receipt_id,
        decision_hash,
        "did:icn:treasury-main",
        "did:icn:member-bob",
        500,
        "HOURS",
    )
    .with_timestamp(timestamp)
    .with_asset(AssetType::Service {
        duration_secs: Some(3600),
    })
    .with_intent_id("node-a-intent-002");

    // Step 3: Add intents to allocation
    let allocation_a = allocation_a
        .add_intent(intent_a1.clone())
        .add_intent(intent_a2.clone());

    // Step 4: Convert to TreasuryEntryRequest
    let request_a1 =
        TreasuryEntryRequest::from_settlement_intent(&intent_a1, "treasury-main").unwrap();
    let request_a2 =
        TreasuryEntryRequest::from_settlement_intent(&intent_a2, "treasury-main").unwrap();

    // === Node B: Construct same economic chain independently ===
    println!("║ Node B: Constructing economic chain (same inputs)...             ║");

    // Step 1: Create AllocationReceipt (same params, different receipt_id)
    let allocation_b = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(timestamp)
        .with_receipt_id("node-b-alloc-999"); // Different node-local ID

    // Step 2: Create SettlementIntent(s) (same params, different intent_ids)
    let intent_b1 = SettlementIntent::new(
        decision_receipt_id,
        decision_hash,
        "did:icn:treasury-main",
        "did:icn:member-alice",
        1000,
        "HOURS",
    )
    .with_timestamp(timestamp)
    .with_intent_id("node-b-intent-777"); // Different node-local ID

    let intent_b2 = SettlementIntent::new(
        decision_receipt_id,
        decision_hash,
        "did:icn:treasury-main",
        "did:icn:member-bob",
        500,
        "HOURS",
    )
    .with_timestamp(timestamp)
    .with_asset(AssetType::Service {
        duration_secs: Some(3600),
    })
    .with_intent_id("node-b-intent-888"); // Different node-local ID

    // Step 3: Add intents to allocation
    let allocation_b = allocation_b
        .add_intent(intent_b1.clone())
        .add_intent(intent_b2.clone());

    // Step 4: Convert to TreasuryEntryRequest
    let request_b1 =
        TreasuryEntryRequest::from_settlement_intent(&intent_b1, "treasury-main").unwrap();
    let request_b2 =
        TreasuryEntryRequest::from_settlement_intent(&intent_b2, "treasury-main").unwrap();

    // === Verify Chain Determinism ===
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║ VERIFICATION                                                     ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    // Settlement Intent 1
    print_chain_verification(
        "economic_chain",
        &intent_a1.canonical_hash(),
        &intent_b1.canonical_hash(),
        "SettlementIntent #1",
    );

    // Settlement Intent 2
    print_chain_verification(
        "economic_chain",
        &intent_a2.canonical_hash(),
        &intent_b2.canonical_hash(),
        "SettlementIntent #2",
    );

    // Allocation Receipt (includes intent hashes)
    print_chain_verification(
        "economic_chain",
        &allocation_a.canonical_hash(),
        &allocation_b.canonical_hash(),
        "AllocationReceipt",
    );

    // TreasuryEntryRequest fields
    assert_eq!(
        request_a1.decision_hash, request_b1.decision_hash,
        "TreasuryEntryRequest #1 decision_hash mismatch"
    );
    assert_eq!(
        request_a2.decision_hash, request_b2.decision_hash,
        "TreasuryEntryRequest #2 decision_hash mismatch"
    );
    println!("║ TreasuryEntryRequest #1: decision_hash ✅ MATCH                  ║");
    println!("║ TreasuryEntryRequest #2: decision_hash ✅ MATCH                  ║");

    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║   RESULT: ALL CANONICAL HASHES MATCH ✅                          ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

// =============================================================================
// Test: AssetType Hash Stability Across Nodes
// =============================================================================

#[test]
fn test_asset_type_hash_determinism() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   ASSET TYPE HASH DETERMINISM TEST                               ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    let decision_hash: Hash = [99u8; 32];
    let timestamp = 1704067200u64;

    // Test all asset types for cross-node determinism
    let asset_types = vec![
        ("Fungible", AssetType::Fungible),
        (
            "Consumable",
            AssetType::Consumable {
                expires_at: Some(timestamp + 86400),
            },
        ),
        (
            "Depreciating",
            AssetType::Depreciating {
                schedule: DepreciationSchedule::Linear {
                    lifespan_secs: 365 * 86400,
                },
            },
        ),
        (
            "Transformable",
            AssetType::Transformable {
                outputs: vec![("product".to_string(), 100)],
            },
        ),
        (
            "Service",
            AssetType::Service {
                duration_secs: Some(3600),
            },
        ),
        (
            "Claim",
            AssetType::Claim {
                due_by: Some(timestamp + 30 * 86400),
                conditions: Some("must complete training".to_string()),
            },
        ),
    ];

    for (name, asset_type) in asset_types {
        // Node A
        let intent_a = SettlementIntent::new("dec-1", decision_hash, "from", "to", 100, "UNIT")
            .with_asset(asset_type.clone())
            .with_timestamp(timestamp)
            .with_intent_id("node-a-id");

        // Node B (different intent_id)
        let intent_b = SettlementIntent::new("dec-1", decision_hash, "from", "to", 100, "UNIT")
            .with_asset(asset_type)
            .with_timestamp(timestamp)
            .with_intent_id("node-b-id");

        assert_eq!(
            intent_a.canonical_hash(),
            intent_b.canonical_hash(),
            "{} asset type hash mismatch across nodes",
            name
        );
        println!("║ {} ✅                                       ║", name);
    }

    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║   RESULT: ALL ASSET TYPES DETERMINISTIC ✅                       ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

// =============================================================================
// Test: Provenance Chain Integrity
// =============================================================================

#[test]
fn test_provenance_chain_integrity() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   PROVENANCE CHAIN INTEGRITY TEST                                ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    let decision_hash: Hash = [1u8; 32];
    let timestamp = 1704067200u64;

    // Create complete chain
    let intent = SettlementIntent::new(
        "decision-receipt-001",
        decision_hash,
        "treasury",
        "member",
        1000,
        "HOURS",
    )
    .with_timestamp(timestamp);

    let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(timestamp)
        .add_intent(intent.clone());

    let allocation_hash = allocation.canonical_hash();

    let request = TreasuryEntryRequest::from_settlement_with_allocation(
        &intent,
        "treasury-main",
        &allocation_hash,
    )
    .unwrap();

    // Verify chain integrity

    // 1. Allocation provenance includes decision_hash
    assert!(
        allocation
            .provenance()
            .upstream_hashes
            .contains(&decision_hash),
        "AllocationReceipt must include decision_hash in provenance"
    );
    println!("║ AllocationReceipt → DecisionHash: ✅                              ║");

    // 2. TreasuryEntryRequest includes decision provenance
    assert_eq!(
        request.decision_receipt_id, "decision-receipt-001",
        "TreasuryEntryRequest must preserve decision_receipt_id"
    );
    assert_eq!(
        request.decision_hash,
        hex::encode(decision_hash),
        "TreasuryEntryRequest must preserve decision_hash"
    );
    println!("║ TreasuryEntryRequest → DecisionReceipt: ✅                        ║");

    // 3. TreasuryEntryRequest includes allocation provenance
    assert!(
        request.memo.contains(&hex::encode(allocation_hash)),
        "TreasuryEntryRequest memo must include allocation_hash"
    );
    println!("║ TreasuryEntryRequest → AllocationReceipt: ✅                      ║");

    // 4. Verify full chain is reconstructable
    let chain = format!(
        "DecisionHash({}) → AllocationHash({}) → IntentHash({}) → TreasuryRequest",
        hex::encode(&decision_hash[..4]),
        hex::encode(&allocation_hash[..4]),
        hex::encode(&intent.canonical_hash()[..4]),
    );
    println!("║                                                                  ║");
    println!("║ Chain: {}...            ║", &chain[..60.min(chain.len())]);

    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║   RESULT: PROVENANCE CHAIN COMPLETE ✅                           ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

// =============================================================================
// Test: Settlement Intent Serde Roundtrip (Network Transport Simulation)
// =============================================================================

#[test]
fn test_settlement_intent_network_roundtrip() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   SETTLEMENT INTENT NETWORK ROUNDTRIP TEST                       ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    let decision_hash: Hash = [77u8; 32];
    let timestamp = 1704067200u64;

    // Node A creates intent
    let original = SettlementIntent::new(
        "dec-network-001",
        decision_hash,
        "treasury",
        "member",
        2500,
        "CREDITS",
    )
    .with_timestamp(timestamp)
    .with_asset(AssetType::Claim {
        due_by: Some(timestamp + 90 * 86400),
        conditions: Some("quarterly review passed".to_string()),
    })
    .with_memo("Q1 bonus allocation".to_string())
    .with_intent_id("node-a-original-id");

    let original_hash = original.canonical_hash();

    // Simulate network transport (JSON serialization)
    let json = serde_json::to_string(&original).expect("serialize should succeed");
    println!(
        "║ Serialized size: {} bytes                                      ║",
        json.len()
    );

    // Node B receives and deserializes
    let mut restored: SettlementIntent =
        serde_json::from_str(&json).expect("deserialize should succeed");

    // Node B assigns its own intent_id (skipped in serde)
    restored.intent_id = icn_kernel_api::ReceiptId::new("node-b-restored-id");

    let restored_hash = restored.canonical_hash();

    // Verify hashes match
    assert_eq!(
        original_hash, restored_hash,
        "SettlementIntent canonical_hash must survive network roundtrip"
    );

    println!(
        "║ Original hash:  {}...                        ║",
        &hex::encode(&original_hash[..8])
    );
    println!(
        "║ Restored hash:  {}...                        ║",
        &hex::encode(&restored_hash[..8])
    );
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║   RESULT: NETWORK ROUNDTRIP DETERMINISTIC ✅                     ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}

// =============================================================================
// Test: Allocation Receipt with Multiple Intents
// =============================================================================

#[test]
fn test_allocation_receipt_intent_ordering() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   ALLOCATION RECEIPT INTENT ORDERING TEST                        ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    let decision_hash: Hash = [88u8; 32];
    let timestamp = 1704067200u64;

    // Create multiple intents
    let intent1 = SettlementIntent::new("dec-1", decision_hash, "t", "alice", 100, "H")
        .with_timestamp(timestamp);
    let intent2 = SettlementIntent::new("dec-1", decision_hash, "t", "bob", 200, "H")
        .with_timestamp(timestamp);
    let intent3 = SettlementIntent::new("dec-1", decision_hash, "t", "carol", 300, "H")
        .with_timestamp(timestamp);

    // Node A: Add in order 1, 2, 3
    let alloc_a = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(timestamp)
        .add_intent(intent1.clone())
        .add_intent(intent2.clone())
        .add_intent(intent3.clone());

    // Node B: Add in same order 1, 2, 3
    let alloc_b = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(timestamp)
        .add_intent(intent1.clone())
        .add_intent(intent2.clone())
        .add_intent(intent3.clone());

    // Node C: Add in DIFFERENT order 3, 1, 2
    let alloc_c = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
        .with_timestamp(timestamp)
        .add_intent(intent3)
        .add_intent(intent1)
        .add_intent(intent2);

    // Same order → same hash
    assert_eq!(
        alloc_a.canonical_hash(),
        alloc_b.canonical_hash(),
        "Same intent order must produce same hash"
    );
    println!("║ Same order (A vs B): ✅ MATCH                                    ║");

    // D0 hardening: Different order → SAME hash (order is now normalized)
    // This ensures nodes that add intents in different order still agree on canonical hash
    assert_eq!(
        alloc_a.canonical_hash(),
        alloc_c.canonical_hash(),
        "Different intent order must produce SAME hash (D0 determinism hardening)"
    );
    println!("║ Different order (A vs C): ✅ MATCH (order-independent hashing)   ║");

    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║   RESULT: INTENT ORDER NORMALIZED BEFORE HASHING ✅              ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
}
