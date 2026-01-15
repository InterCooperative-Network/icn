#![allow(clippy::unwrap_used)]
//! Concurrent operations tests for agreement gossip synchronization
//!
//! These tests verify correct behavior when multiple cooperatives perform
//! concurrent operations on shared agreements, now that gossip synchronization
//! is wired (#655).
//!
//! ## Test Categories
//!
//! 1. **Concurrent Signing**: Multiple parties sign simultaneously
//! 2. **Concurrent Amendments**: Multiple parties propose amendments at once
//! 3. **Status Conflicts**: Competing status changes (suspend vs terminate)
//! 4. **Message Ordering**: Gossip messages arrive in different orders
//! 5. **Partition Recovery**: Network splits and rejoins with conflicting state
//! 6. **Eventual Consistency**: All nodes converge to same state
//!
//! ## Running Tests
//! ```sh
//! cargo test -p icn-federation --test agreement_concurrent_ops
//! cargo test -p icn-federation test_concurrent_signing
//! ```
//!
//! ## Related Issues
//! - Closes #664

use icn_federation::agreement::{
    AgreementGossipHandler, AgreementManager, AgreementMessage, AgreementParty, AgreementType,
    AmendmentChange, InMemoryAgreementStore, PartyRole, TerminationReason, TradeItem,
};
use icn_federation::AgreementStoreOps;
use icn_identity::KeyPair;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Barrier;

/// Test helper: create a manager with a keypair
fn create_manager(
    coop_id: &str,
) -> (
    AgreementManager<InMemoryAgreementStore>,
    Arc<KeyPair>,
    Arc<InMemoryAgreementStore>,
) {
    let store = Arc::new(InMemoryAgreementStore::new());
    let keypair = Arc::new(KeyPair::generate().unwrap());
    let did = keypair.did().clone();
    let manager = AgreementManager::new(store.clone(), coop_id.to_string(), did)
        .with_keypair(keypair.clone());
    (manager, keypair, store)
}

/// Test helper: create a manager sharing an existing store
fn create_manager_with_store(
    coop_id: &str,
    store: Arc<InMemoryAgreementStore>,
) -> (AgreementManager<InMemoryAgreementStore>, Arc<KeyPair>) {
    let keypair = Arc::new(KeyPair::generate().unwrap());
    let did = keypair.did().clone();
    let manager =
        AgreementManager::new(store, coop_id.to_string(), did).with_keypair(keypair.clone());
    (manager, keypair)
}

/// Test helper: create a gossip handler with mock send callback
fn create_gossip_handler(
    store: Arc<InMemoryAgreementStore>,
    keypair: &KeyPair,
    coop_id: &str,
    message_count: Arc<AtomicUsize>,
) -> AgreementGossipHandler<InMemoryAgreementStore> {
    let mut handler =
        AgreementGossipHandler::new(store, keypair.did().clone(), coop_id.to_string());

    // Set up mock send callback that counts messages
    let count_clone = message_count.clone();
    handler.set_send_callback(Arc::new(move |_topic, _data| {
        count_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));

    handler
}

// =============================================================================
// Concurrent Signing Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_signing_two_parties() {
    // Setup: Two coops share a store (simulating synced state via gossip)
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    // Create and propose agreement
    let agreement = coop_a
        .create_draft(
            "Concurrent Test",
            "Both parties sign at once",
            AgreementType::Trade {
                items: vec![TradeItem::new("widgets", 100, "units", 50, "USD")],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();

    // Use barrier to synchronize concurrent signing
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();

    // Spawn concurrent signing tasks
    let barrier_a = barrier.clone();
    let id_a = agreement_id.clone();
    let sign_a = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.sign(&id_a)
    });

    let barrier_b = barrier.clone();
    let id_b = agreement_id.clone();
    let sign_b = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.sign(&id_b)
    });

    // Wait for both to complete
    let (result_a, result_b) = tokio::join!(sign_a, sign_b);

    // Both should succeed
    assert!(result_a.unwrap().is_ok(), "Coop A signing should succeed");
    assert!(result_b.unwrap().is_ok(), "Coop B signing should succeed");

    // Verify final state
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(final_agreement.signatures.len(), 2);
    assert!(final_agreement.has_signed(keypair_a.did()));
    assert!(final_agreement.has_signed(keypair_b.did()));
    assert!(
        final_agreement.status.is_active(),
        "Agreement should be active after both signatures"
    );
}

#[tokio::test]
async fn test_concurrent_signing_three_parties() {
    // Setup: Three coops share a store
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let (coop_c, keypair_c) = create_manager_with_store("coop-c", store.clone());

    // Create and propose agreement
    let agreement = coop_a
        .create_draft(
            "Three Party Concurrent",
            "All three sign simultaneously",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_c.did().clone(), "coop-c", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();

    // Use barrier to synchronize all three
    let barrier = Arc::new(Barrier::new(3));
    let agreement_id = agreement.id.clone();

    let barrier_a = barrier.clone();
    let id_a = agreement_id.clone();
    let sign_a = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.sign(&id_a)
    });

    let barrier_b = barrier.clone();
    let id_b = agreement_id.clone();
    let sign_b = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.sign(&id_b)
    });

    let barrier_c = barrier.clone();
    let id_c = agreement_id.clone();
    let sign_c = tokio::spawn(async move {
        barrier_c.wait().await;
        coop_c.sign(&id_c)
    });

    // Wait for all to complete
    let (result_a, result_b, result_c) = tokio::join!(sign_a, sign_b, sign_c);

    // All should succeed
    assert!(result_a.unwrap().is_ok());
    assert!(result_b.unwrap().is_ok());
    assert!(result_c.unwrap().is_ok());

    // Verify final state
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(final_agreement.signatures.len(), 3);
    assert!(final_agreement.has_signed(keypair_a.did()));
    assert!(final_agreement.has_signed(keypair_b.did()));
    assert!(final_agreement.has_signed(keypair_c.did()));
    assert!(final_agreement.status.is_active());
}

// =============================================================================
// Concurrent Amendment Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_amendment_proposals() {
    // Setup: Two coops with an active agreement
    let (coop_a, _keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    // Create and activate agreement
    let agreement = coop_a
        .create_draft(
            "Amendment Test",
            "Will have concurrent amendments",
            AgreementType::Credit {
                credit_limit: 10000,
                interest_rate_bps: 500,
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    let agreement = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert!(agreement.status.is_active());

    // Now both try to propose amendments simultaneously
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();

    let barrier_a = barrier.clone();
    let id_a = agreement_id.clone();
    let expiration_a = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 365 * 24 * 3600;
    let amend_a = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.propose_amendment(
            &id_a,
            "Extend duration by A",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: expiration_a,
            }],
        )
    });

    let barrier_b = barrier.clone();
    let id_b = agreement_id.clone();
    let expiration_b = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 180 * 24 * 3600; // Different duration
    let amend_b = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.propose_amendment(
            &id_b,
            "Extend duration by B",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: expiration_b,
            }],
        )
    });

    let (result_a, result_b) = tokio::join!(amend_a, amend_b);

    // Both should be able to propose amendments (they get different IDs)
    assert!(result_a.unwrap().is_ok(), "A should be able to propose");
    assert!(result_b.unwrap().is_ok(), "B should be able to propose");

    // Verify both amendments exist
    let amendments = store.get_amendments(&agreement_id).unwrap();
    assert_eq!(amendments.len(), 2, "Both amendments should be stored");
}

#[tokio::test]
async fn test_concurrent_amendment_signing() {
    // Setup: Active agreement with a proposed amendment
    let (coop_a, _keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Amendment Sign Test",
            "Concurrent amendment signing",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Propose amendment
    let expiration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 365 * 24 * 3600;
    let amendment = coop_a
        .propose_amendment(
            &agreement.id,
            "Test amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: expiration,
            }],
        )
        .unwrap();

    // Both sign the amendment concurrently
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();
    let amendment_id = amendment.id.clone();

    let barrier_a = barrier.clone();
    let aid_a = agreement_id.clone();
    let mid_a = amendment_id.clone();
    let sign_a = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.sign_amendment(&aid_a, &mid_a)
    });

    let barrier_b = barrier.clone();
    let aid_b = agreement_id.clone();
    let mid_b = amendment_id.clone();
    let sign_b = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.sign_amendment(&aid_b, &mid_b)
    });

    let (result_a, result_b) = tokio::join!(sign_a, sign_b);

    // Both should succeed
    assert!(result_a.unwrap().is_ok());
    assert!(result_b.unwrap().is_ok());

    // Amendment should be ratified
    let amendments = store.get_amendments(&agreement_id).unwrap();
    let final_amendment = amendments.iter().find(|a| a.id == amendment_id).unwrap();
    assert!(
        matches!(
            final_amendment.status,
            icn_federation::agreement::AmendmentStatus::Ratified
        ),
        "Amendment should be ratified after both sign"
    );
}

// =============================================================================
// Status Conflict Tests
// =============================================================================

#[tokio::test]
async fn test_status_conflict_suspend_vs_terminate() {
    // Setup: Active agreement
    let (coop_a, _keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Conflict Test",
            "Status conflict resolution",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // A tries to suspend while B tries to terminate
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();

    let barrier_a = barrier.clone();
    let id_a = agreement_id.clone();
    let suspend_task = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.suspend(&id_a, "Temporary pause")
    });

    let barrier_b = barrier.clone();
    let id_b = agreement_id.clone();
    let terminate_task = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.terminate(
            &id_b,
            TerminationReason::MutualConsent { explanation: None },
        )
    });

    let (suspend_result, terminate_result) = tokio::join!(suspend_task, terminate_task);

    // One should succeed, one may fail depending on timing
    // The important thing is the final state is consistent
    let _suspend_ok = suspend_result.unwrap().is_ok();
    let _terminate_ok = terminate_result.unwrap().is_ok();

    // Final state should be either suspended or terminated (not active)
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert!(
        final_agreement.status.is_suspended() || final_agreement.status.is_terminated(),
        "Final status should be either suspended or terminated, got: {:?}",
        final_agreement.status
    );
}

#[tokio::test]
async fn test_status_conflict_both_suspend() {
    // Setup: Active agreement
    let (coop_a, _keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Double Suspend Test",
            "Both try to suspend",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Both try to suspend simultaneously
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();

    let barrier_a = barrier.clone();
    let id_a = agreement_id.clone();
    let suspend_a = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.suspend(&id_a, "Pause from A")
    });

    let barrier_b = barrier.clone();
    let id_b = agreement_id.clone();
    let suspend_b = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.suspend(&id_b, "Pause from B")
    });

    let (result_a, result_b) = tokio::join!(suspend_a, suspend_b);

    // At least one should succeed
    let success_count = [result_a.unwrap().is_ok(), result_b.unwrap().is_ok()]
        .iter()
        .filter(|&&x| x)
        .count();
    assert!(success_count >= 1, "At least one suspend should succeed");

    // Final state should be suspended
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert!(
        final_agreement.status.is_suspended(),
        "Agreement should be suspended"
    );
}

// =============================================================================
// Gossip Message Ordering Tests
// =============================================================================

#[tokio::test]
async fn test_gossip_signature_ordering() {
    // Test that signatures arrive and are processed correctly regardless of order
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Ordering Test",
            "Test message ordering",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();

    // Create gossip handlers for both
    let msg_count_a = Arc::new(AtomicUsize::new(0));
    let msg_count_b = Arc::new(AtomicUsize::new(0));
    let handler_a = create_gossip_handler(store.clone(), &keypair_a, "coop-a", msg_count_a.clone());
    let handler_b = create_gossip_handler(store.clone(), &keypair_b, "coop-b", msg_count_b.clone());

    // B signs first
    let agreement_after_b = coop_b.sign(&agreement.id).unwrap();
    let sig_b = agreement_after_b
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_b.did())
        .unwrap()
        .clone();

    // Simulate B's signature arriving via gossip
    let msg_b = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_b,
    };
    handler_a
        .handle_message(&msg_b.to_bytes().unwrap())
        .unwrap();

    // Now A signs
    let agreement_after_a = coop_a.sign(&agreement.id).unwrap();
    let sig_a = agreement_after_a
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_a.did())
        .unwrap()
        .clone();

    // Simulate A's signature arriving via gossip to B
    let msg_a = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_a,
    };
    handler_b
        .handle_message(&msg_a.to_bytes().unwrap())
        .unwrap();

    // Both should have both signatures and be active
    let final_a = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(final_a.signatures.len(), 2);
    assert!(final_a.status.is_active());
}

#[tokio::test]
async fn test_gossip_handles_duplicate_signatures() {
    // Test that duplicate signatures are handled gracefully
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (_coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Duplicate Test",
            "Test duplicate handling",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();

    // A signs
    let agreement_signed = coop_a.sign(&agreement.id).unwrap();
    let sig_a = agreement_signed
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_a.did())
        .unwrap()
        .clone();

    // Create handler for B
    let msg_count = Arc::new(AtomicUsize::new(0));
    let handler_b = create_gossip_handler(store.clone(), &keypair_b, "coop-b", msg_count);

    // Send the same signature multiple times (simulating network duplicates)
    let msg = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_a.clone(),
    };

    for _ in 0..3 {
        handler_b.handle_message(&msg.to_bytes().unwrap()).unwrap();
    }

    // Should only have one signature from A
    let current = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(
        current
            .signatures
            .iter()
            .filter(|s| s.signer == *keypair_a.did())
            .count(),
        1,
        "Should have exactly one signature from A despite duplicates"
    );
}

// =============================================================================
// Network Partition Recovery Tests
// =============================================================================

#[tokio::test]
async fn test_partition_recovery_with_divergent_signatures() {
    // Simulate network partition where both parties sign independently,
    // then recover and sync via gossip

    // Create isolated stores (simulating partition)
    let (coop_a, keypair_a, store_a) = create_manager("coop-a");
    let (coop_b, keypair_b, store_b) = create_manager("coop-b");

    // Create agreement on A's side
    let agreement = coop_a
        .create_draft(
            "Partition Test",
            "Test partition recovery",
            AgreementType::Trade {
                items: vec![TradeItem::new("goods", 10, "units", 100, "USD")],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();

    // Manually copy agreement to B (simulating initial sync before partition)
    let initial_agreement = store_a.get_agreement(&agreement.id).unwrap().unwrap();
    store_b.store_agreement(&initial_agreement).unwrap();

    // During partition: A signs
    let agreement_a = coop_a.sign(&agreement.id).unwrap();
    assert!(agreement_a.has_signed(keypair_a.did()));

    // During partition: B signs (on its own copy)
    let agreement_b = coop_b.sign(&agreement.id).unwrap();
    assert!(agreement_b.has_signed(keypair_b.did()));

    // Now partition heals - sync A's signature to B
    let msg_count = Arc::new(AtomicUsize::new(0));
    let handler_b = create_gossip_handler(store_b.clone(), &keypair_b, "coop-b", msg_count.clone());

    let sig_a = agreement_a
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_a.did())
        .unwrap()
        .clone();
    let msg_a = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_a,
    };
    handler_b
        .handle_message(&msg_a.to_bytes().unwrap())
        .unwrap();

    // B should now have both signatures and be active
    let final_b = store_b.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(final_b.signatures.len(), 2);
    assert!(final_b.has_signed(keypair_a.did()));
    assert!(final_b.has_signed(keypair_b.did()));
    assert!(
        final_b.status.is_active(),
        "Agreement should be active after partition recovery"
    );
}

#[tokio::test]
async fn test_partition_recovery_conflicting_amendments() {
    // Both parties propose amendments during partition, then recover
    let (coop_a, keypair_a, store_a) = create_manager("coop-a");
    let (coop_b, keypair_b, store_b) = create_manager("coop-b");

    // Create and activate agreement
    let agreement = coop_a
        .create_draft(
            "Amendment Partition",
            "Test amendment conflict during partition",
            AgreementType::Credit {
                credit_limit: 10000,
                interest_rate_bps: 500,
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();

    // Copy to B and have B sign
    let mut synced_agreement = store_a.get_agreement(&agreement.id).unwrap().unwrap();
    store_b.store_agreement(&synced_agreement).unwrap();
    let agreement_b = coop_b.sign(&agreement.id).unwrap();

    // Copy B's signature back to A
    let sig_b = agreement_b.signatures.last().unwrap().clone();
    synced_agreement.signatures.push(sig_b);
    synced_agreement.activate().unwrap();
    store_a.store_agreement(&synced_agreement).unwrap();
    store_b.store_agreement(&synced_agreement).unwrap();

    // Now both propose amendments during partition
    let expiration_a = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 365 * 24 * 3600;
    let amendment_a = coop_a
        .propose_amendment(
            &agreement.id,
            "A's amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: expiration_a,
            }],
        )
        .unwrap();

    let expiration_b = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 180 * 24 * 3600;
    let amendment_b = coop_b
        .propose_amendment(
            &agreement.id,
            "B's amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: expiration_b,
            }],
        )
        .unwrap();

    // Partition heals - sync amendments
    let msg_count = Arc::new(AtomicUsize::new(0));
    let handler_a = create_gossip_handler(store_a.clone(), &keypair_a, "coop-a", msg_count.clone());
    let handler_b = create_gossip_handler(store_b.clone(), &keypair_b, "coop-b", msg_count);

    // Sync B's amendment to A
    let msg_b = AgreementMessage::AmendmentProposed {
        amendment: amendment_b.clone(),
    };
    handler_a
        .handle_message(&msg_b.to_bytes().unwrap())
        .unwrap();

    // Sync A's amendment to B
    let msg_a = AgreementMessage::AmendmentProposed {
        amendment: amendment_a.clone(),
    };
    handler_b
        .handle_message(&msg_a.to_bytes().unwrap())
        .unwrap();

    // Both stores should have both amendments
    let amendments_a = store_a.get_amendments(&agreement.id).unwrap();
    let amendments_b = store_b.get_amendments(&agreement.id).unwrap();

    assert_eq!(amendments_a.len(), 2, "Store A should have both amendments");
    assert_eq!(amendments_b.len(), 2, "Store B should have both amendments");
}

// =============================================================================
// Eventual Consistency Tests
// =============================================================================

#[tokio::test]
async fn test_eventual_consistency_after_concurrent_ops() {
    // Multiple concurrent operations should result in consistent final state
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let (coop_c, keypair_c) = create_manager_with_store("coop-c", store.clone());

    // Create agreement with all three parties
    let agreement = coop_a
        .create_draft(
            "Consistency Test",
            "Test eventual consistency",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_c.did().clone(), "coop-c", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();

    // All three sign concurrently
    let barrier = Arc::new(Barrier::new(3));
    let agreement_id = agreement.id.clone();

    let handles: Vec<_> = [
        (coop_a, barrier.clone(), agreement_id.clone()),
        (coop_b, barrier.clone(), agreement_id.clone()),
        (coop_c, barrier.clone(), agreement_id.clone()),
    ]
    .into_iter()
    .map(|(mgr, bar, id)| {
        tokio::spawn(async move {
            bar.wait().await;
            mgr.sign(&id)
        })
    })
    .collect();

    // Wait for all
    for handle in handles {
        let _ = handle.await.unwrap();
    }

    // All views should be consistent
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();

    // Should have exactly 3 signatures (one from each party)
    assert_eq!(
        final_agreement.signatures.len(),
        3,
        "Should have exactly 3 signatures"
    );

    // Should be active
    assert!(
        final_agreement.status.is_active(),
        "Agreement should be active"
    );

    // Each party should have signed exactly once
    assert!(final_agreement.has_signed(keypair_a.did()));
    assert!(final_agreement.has_signed(keypair_b.did()));
    assert!(final_agreement.has_signed(keypair_c.did()));
}

#[tokio::test]
async fn test_eventual_consistency_amendment_ratification() {
    // Amendment ratification with concurrent signing should result in correct version
    let (coop_a, _keypair_a, store) = create_manager("coop-a");
    let (coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    // Create and activate agreement
    let agreement = coop_a
        .create_draft(
            "Version Consistency",
            "Test version consistency",
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string(),
            },
        )
        .unwrap();

    coop_a
        .add_party(
            &agreement.id,
            AgreementParty::new(keypair_b.did().clone(), "coop-b", PartyRole::Counterparty),
        )
        .unwrap();
    coop_a.propose(&agreement.id).unwrap();
    coop_a.sign(&agreement.id).unwrap();
    coop_b.sign(&agreement.id).unwrap();

    // Initial version is 1
    let initial = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(initial.version, 1);

    // Propose amendment
    let expiration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 365 * 24 * 3600;
    let amendment = coop_a
        .propose_amendment(
            &agreement.id,
            "Test amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: expiration,
            }],
        )
        .unwrap();

    // Both sign the amendment concurrently
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();
    let amendment_id = amendment.id.clone();

    let barrier_a = barrier.clone();
    let aid_a = agreement_id.clone();
    let mid_a = amendment_id.clone();
    let sign_a = tokio::spawn(async move {
        barrier_a.wait().await;
        coop_a.sign_amendment(&aid_a, &mid_a)
    });

    let barrier_b = barrier.clone();
    let aid_b = agreement_id.clone();
    let mid_b = amendment_id.clone();
    let sign_b = tokio::spawn(async move {
        barrier_b.wait().await;
        coop_b.sign_amendment(&aid_b, &mid_b)
    });

    let _ = tokio::join!(sign_a, sign_b);

    // Version should be exactly 2 (incremented once, not twice)
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(
        final_agreement.version, 2,
        "Version should be exactly 2 after one amendment"
    );

    // Amendment should be ratified
    let amendments = store.get_amendments(&agreement_id).unwrap();
    let final_amendment = amendments.iter().find(|a| a.id == amendment_id).unwrap();
    assert!(
        matches!(
            final_amendment.status,
            icn_federation::agreement::AmendmentStatus::Ratified
        ),
        "Amendment should be ratified"
    );
}
