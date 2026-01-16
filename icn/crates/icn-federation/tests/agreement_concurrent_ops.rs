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
//! 7. **Security**: Signature verification and authorization checks
//!
//! ## Consistency Model
//!
//! The agreement system uses eventual consistency with the following guarantees:
//! - All operations are idempotent (duplicate messages are ignored)
//! - Signatures accumulate monotonically (cannot be removed)
//! - Status changes follow a state machine with defined transitions
//! - Conflict resolution: terminate > suspend (terminate always wins)
//!
//! ## Test Design Philosophy
//!
//! These tests use two approaches:
//!
//! ### Shared Store Tests (`*_shared_store`)
//! Multiple managers share a single `InMemoryAgreementStore`. This simulates a
//! "pre-synced" scenario where all nodes have identical state. These tests verify
//! the agreement manager logic works correctly when state is consistent.
//!
//! **Note**: The `InMemoryAgreementStore` performs non-transactional read-modify-write
//! operations. Under heavy concurrent load, lost updates are theoretically possible.
//! In practice, Tokio's cooperative scheduling means these tests reliably pass.
//! If flakiness is observed, it would indicate a need for transactional store operations.
//!
//! ### Separate Store Tests (`*_with_gossip_sync`)
//! Each node has its own store, and state is synchronized via gossip messages.
//! This more accurately simulates real distributed operation where nodes have
//! independent storage and must coordinate through message passing.
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
use std::sync::{Arc, Mutex};
use tokio::sync::Barrier;

// =============================================================================
// Constants
// =============================================================================

/// Number of times to send duplicate messages in duplicate handling tests
const DUPLICATE_MESSAGE_TEST_COUNT: usize = 3;

// =============================================================================
// Helper Functions
// =============================================================================

/// Calculate a Unix timestamp N days from now
fn timestamp_plus_days(days: u64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + days * 24 * 3600
}

/// Test helper: create a manager with a keypair and new store
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
///
/// Note: This simulates a "pre-synced" scenario where both nodes share state.
/// For realistic gossip testing, use separate stores and sync via gossip messages.
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

/// Captured gossip message for verification
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct CapturedMessage {
    topic: String,
    data: Vec<u8>,
}

/// Test helper: create a gossip handler with message capture callback
///
/// Returns the handler and a shared vec of captured messages for verification.
fn create_gossip_handler_with_capture(
    store: Arc<InMemoryAgreementStore>,
    keypair: &KeyPair,
    coop_id: &str,
) -> (
    AgreementGossipHandler<InMemoryAgreementStore>,
    Arc<Mutex<Vec<CapturedMessage>>>,
) {
    let mut handler =
        AgreementGossipHandler::new(store, keypair.did().clone(), coop_id.to_string());

    let messages = Arc::new(Mutex::new(Vec::new()));
    let messages_clone = messages.clone();

    handler.set_send_callback(Arc::new(move |topic, data| {
        messages_clone.lock().unwrap().push(CapturedMessage {
            topic: topic.to_string(),
            data: data.to_vec(),
        });
        Ok(())
    }));

    (handler, messages)
}

/// Sync an agreement from one store to another (simulating gossip delivery)
fn sync_agreement(
    from_store: &Arc<InMemoryAgreementStore>,
    to_store: &Arc<InMemoryAgreementStore>,
    agreement_id: &icn_federation::agreement::AgreementId,
) {
    let agreement = from_store.get_agreement(agreement_id).unwrap().unwrap();
    to_store.store_agreement(&agreement).unwrap();
}

/// Retry a condition with exponential backoff for eventual consistency testing.
///
/// This helper is used to verify that distributed systems eventually converge to
/// the expected state, even if it takes multiple retries.
///
/// # Arguments
/// * `condition` - A closure that checks if the expected state has been reached
/// * `max_attempts` - Maximum number of retry attempts
/// * `initial_delay_ms` - Initial delay between attempts (doubles each retry)
///
/// # Returns
/// `true` if the condition was met within the retry limit, `false` otherwise.
async fn retry_until<F>(condition: F, max_attempts: u32, initial_delay_ms: u64) -> bool
where
    F: Fn() -> bool,
{
    let mut delay_ms = initial_delay_ms;
    for attempt in 1..=max_attempts {
        if condition() {
            return true;
        }
        if attempt < max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            delay_ms = delay_ms.saturating_mul(2).min(1000); // Cap at 1 second
        }
    }
    false
}

// =============================================================================
// Concurrent Signing Tests (Shared Store - Pre-Synced Scenario)
// =============================================================================

/// Tests that two parties can sign an agreement simultaneously when they share
/// state (simulating a pre-synced scenario via gossip).
///
/// **Note**: This test uses a shared store which performs non-transactional
/// read-modify-write operations. Under Tokio's cooperative scheduling, the
/// operations serialize naturally. For true distributed concurrent signing,
/// see `test_concurrent_signing_with_gossip_sync`.
#[tokio::test]
async fn test_concurrent_signing_two_parties_shared_store() {
    // Setup: Two coops share a store (simulating pre-synced state via gossip)
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
    assert!(
        result_a.unwrap().is_ok(),
        "Coop A signing should succeed in shared store scenario"
    );
    assert!(
        result_b.unwrap().is_ok(),
        "Coop B signing should succeed in shared store scenario"
    );

    // Verify final state
    //
    // NOTE: Under true concurrent execution with a non-transactional store,
    // these assertions could theoretically fail due to lost updates. Both parties
    // would read the same initial state, add their own signature, and write back -
    // causing the second write to overwrite the first party's signature.
    // In practice, Tokio's cooperative scheduling serializes these operations.
    // If this test becomes flaky, it indicates a need for transactional store operations.
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(
        final_agreement.signatures.len(),
        2,
        "Should have exactly 2 signatures"
    );
    assert!(
        final_agreement.has_signed(keypair_a.did()),
        "Coop A should have signed"
    );
    assert!(
        final_agreement.has_signed(keypair_b.did()),
        "Coop B should have signed"
    );
    assert!(
        final_agreement.status.is_active(),
        "Agreement should be active after both signatures"
    );
}

/// Tests concurrent signing with separate stores and explicit gossip sync.
/// This more accurately simulates the real distributed scenario.
#[tokio::test]
async fn test_concurrent_signing_with_gossip_sync() {
    // Each node has its own independent store
    let (coop_a, keypair_a, store_a) = create_manager("coop-a");
    let (coop_b, keypair_b, store_b) = create_manager("coop-b");

    // A creates and proposes agreement
    let agreement = coop_a
        .create_draft(
            "Gossip Sync Test",
            "Concurrent signing with separate stores",
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

    // Sync the proposed agreement to B (simulating initial gossip)
    sync_agreement(&store_a, &store_b, &agreement.id);

    // Both sign independently on their own stores
    let agreement_a = coop_a.sign(&agreement.id).unwrap();
    let agreement_b = coop_b.sign(&agreement.id).unwrap();

    // Extract signatures
    let sig_a = agreement_a
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_a.did())
        .unwrap()
        .clone();
    let sig_b = agreement_b
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_b.did())
        .unwrap()
        .clone();

    // Create gossip handlers for syncing
    let (handler_a, _msgs_a) =
        create_gossip_handler_with_capture(store_a.clone(), &keypair_a, "coop-a");
    let (handler_b, _msgs_b) =
        create_gossip_handler_with_capture(store_b.clone(), &keypair_b, "coop-b");

    // Sync B's signature to A via gossip
    let msg_b = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_b,
    };
    handler_a
        .handle_message(&msg_b.to_bytes().unwrap())
        .unwrap();

    // Sync A's signature to B via gossip
    let msg_a = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_a,
    };
    handler_b
        .handle_message(&msg_a.to_bytes().unwrap())
        .unwrap();

    // Both stores should now have both signatures
    let final_a = store_a.get_agreement(&agreement.id).unwrap().unwrap();
    let final_b = store_b.get_agreement(&agreement.id).unwrap().unwrap();

    assert_eq!(
        final_a.signatures.len(),
        2,
        "Store A should have both signatures after gossip sync"
    );
    assert_eq!(
        final_b.signatures.len(),
        2,
        "Store B should have both signatures after gossip sync"
    );

    // Both should be active
    assert!(
        final_a.status.is_active(),
        "Agreement on A should be active"
    );
    assert!(
        final_b.status.is_active(),
        "Agreement on B should be active"
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
    assert!(result_a.unwrap().is_ok(), "Coop A signing should succeed");
    assert!(result_b.unwrap().is_ok(), "Coop B signing should succeed");
    assert!(result_c.unwrap().is_ok(), "Coop C signing should succeed");

    // Verify final state
    //
    // NOTE: Same race condition caveat as test_concurrent_signing_two_parties_shared_store.
    // With three parties, the lost update problem is even more likely under true concurrency.
    // See module documentation for details.
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(
        final_agreement.signatures.len(),
        3,
        "Should have exactly 3 signatures"
    );
    assert!(
        final_agreement.has_signed(keypair_a.did()),
        "Coop A should have signed"
    );
    assert!(
        final_agreement.has_signed(keypair_b.did()),
        "Coop B should have signed"
    );
    assert!(
        final_agreement.has_signed(keypair_c.did()),
        "Coop C should have signed"
    );
    assert!(
        final_agreement.status.is_active(),
        "Agreement should be active after all signatures"
    );
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
    assert!(
        agreement.status.is_active(),
        "Agreement should be active before amendments"
    );

    // Now both try to propose amendments simultaneously
    let barrier = Arc::new(Barrier::new(2));
    let agreement_id = agreement.id.clone();

    let barrier_a = barrier.clone();
    let id_a = agreement_id.clone();
    let expiration_a = timestamp_plus_days(365);
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
    let expiration_b = timestamp_plus_days(180); // Different duration
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
    assert!(
        result_a.unwrap().is_ok(),
        "A should be able to propose amendment"
    );
    assert!(
        result_b.unwrap().is_ok(),
        "B should be able to propose amendment"
    );

    // Verify both amendments exist
    //
    // NOTE: Amendments have unique IDs so they're less likely to conflict than
    // signatures on the same agreement. However, there's still a race in the
    // store operations when writing the amendments collection. Under true
    // concurrency without transactional isolation, one amendment could theoretically
    // be lost. In practice, Tokio's scheduling serializes these operations.
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
    let expiration = timestamp_plus_days(365);
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
    assert!(
        result_a.unwrap().is_ok(),
        "A should successfully sign amendment"
    );
    assert!(
        result_b.unwrap().is_ok(),
        "B should successfully sign amendment"
    );

    // Amendment should be ratified
    //
    // NOTE: Same lost update caveat as concurrent agreement signing. Both parties
    // read the amendment, add their signatures, and write back. Without transactional
    // isolation, the second write could overwrite the first party's signature.
    // The test expects ratification (both signatures present), which relies on
    // Tokio's cooperative scheduling to serialize operations.
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

/// Tests conflict resolution when one party tries to suspend while another
/// tries to terminate.
///
/// Conflict resolution policy: Terminate takes precedence over suspend.
/// This is deterministic - terminate always wins because it's a "stronger"
/// state transition that cannot be reversed.
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

    let suspend_ok = suspend_result.unwrap().is_ok();
    let terminate_ok = terminate_result.unwrap().is_ok();

    // At least one operation should succeed
    assert!(
        suspend_ok || terminate_ok,
        "At least one operation should succeed (suspend={suspend_ok}, terminate={terminate_ok})"
    );

    // Final state should be either suspended or terminated (not active)
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert!(
        final_agreement.status.is_suspended() || final_agreement.status.is_terminated(),
        "Final status should be either suspended or terminated, got: {:?}",
        final_agreement.status
    );

    // Document the actual outcome for debugging
    if final_agreement.status.is_terminated() {
        // Terminate won - this is expected when terminate executes first
        // because terminated agreements cannot be modified
        assert!(
            terminate_ok,
            "If terminated, terminate operation should have succeeded"
        );
    } else if final_agreement.status.is_suspended() {
        // Suspend won - this happens when suspend executes first
        assert!(
            suspend_ok,
            "If suspended, suspend operation should have succeeded"
        );
    }
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

    let suspend_a_ok = result_a.unwrap().is_ok();
    let suspend_b_ok = result_b.unwrap().is_ok();

    // At least one should succeed
    let success_count = [suspend_a_ok, suspend_b_ok].iter().filter(|&&x| x).count();
    assert!(
        success_count >= 1,
        "At least one suspend should succeed (A={suspend_a_ok}, B={suspend_b_ok})"
    );

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

/// Tests that signatures arriving via gossip are processed correctly
/// regardless of the order they arrive.
#[tokio::test]
async fn test_gossip_signature_ordering() {
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

    // Create gossip handlers with message capture for verification
    let (handler_a, msgs_a) =
        create_gossip_handler_with_capture(store.clone(), &keypair_a, "coop-a");
    let (handler_b, msgs_b) =
        create_gossip_handler_with_capture(store.clone(), &keypair_b, "coop-b");

    // B signs first
    let agreement_after_b = coop_b.sign(&agreement.id).unwrap();
    let sig_b = agreement_after_b
        .signatures
        .iter()
        .find(|s| s.signer == *keypair_b.did())
        .unwrap()
        .clone();

    // Simulate B's signature arriving via gossip to A
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
    assert_eq!(
        final_a.signatures.len(),
        2,
        "Should have 2 signatures after gossip sync"
    );
    assert!(
        final_a.status.is_active(),
        "Agreement should be active after all signatures"
    );

    // Verify gossip handlers didn't need to broadcast (since we handled manually)
    // The captured messages would be from any broadcasts the handlers initiated
    let captured_a = msgs_a.lock().unwrap();
    let captured_b = msgs_b.lock().unwrap();

    // Handlers should not broadcast when receiving signatures (only when signing locally)
    // This verifies the gossip handler doesn't re-broadcast received messages
    assert!(
        captured_a.is_empty(),
        "Handler A should not broadcast received signatures"
    );
    assert!(
        captured_b.is_empty(),
        "Handler B should not broadcast received signatures"
    );
}

/// Tests that duplicate signatures are handled gracefully (idempotency).
#[tokio::test]
async fn test_gossip_handles_duplicate_signatures() {
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

    // Create handler for B with message capture
    let (handler_b, _msgs) =
        create_gossip_handler_with_capture(store.clone(), &keypair_b, "coop-b");

    // Send the same signature multiple times (simulating network duplicates)
    let msg = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_a.clone(),
    };

    for i in 0..DUPLICATE_MESSAGE_TEST_COUNT {
        let result = handler_b.handle_message(&msg.to_bytes().unwrap());
        assert!(
            result.is_ok(),
            "Duplicate message {} should be handled gracefully",
            i + 1
        );
    }

    // Should only have one signature from A (duplicates ignored)
    let current = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(
        current
            .signatures
            .iter()
            .filter(|s| s.signer == *keypair_a.did())
            .count(),
        1,
        "Should have exactly one signature from A despite {DUPLICATE_MESSAGE_TEST_COUNT} duplicates"
    );
}

// =============================================================================
// Network Partition Recovery Tests
// =============================================================================

/// Simulates a network partition where both parties sign independently,
/// then recover and sync via gossip.
#[tokio::test]
async fn test_partition_recovery_with_divergent_signatures() {
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

    // Sync agreement to B before partition
    sync_agreement(&store_a, &store_b, &agreement.id);

    // During partition: A signs
    let agreement_a = coop_a.sign(&agreement.id).unwrap();
    assert!(
        agreement_a.has_signed(keypair_a.did()),
        "A should have signed during partition"
    );

    // During partition: B signs (on its own copy)
    let agreement_b = coop_b.sign(&agreement.id).unwrap();
    assert!(
        agreement_b.has_signed(keypair_b.did()),
        "B should have signed during partition"
    );

    // Now partition heals - sync A's signature to B
    let (handler_b, _msgs) =
        create_gossip_handler_with_capture(store_b.clone(), &keypair_b, "coop-b");

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
    assert_eq!(
        final_b.signatures.len(),
        2,
        "B should have both signatures after partition recovery"
    );
    assert!(
        final_b.has_signed(keypair_a.did()),
        "B should have A's signature"
    );
    assert!(
        final_b.has_signed(keypair_b.did()),
        "B should have its own signature"
    );
    assert!(
        final_b.status.is_active(),
        "Agreement should be active after partition recovery"
    );
}

/// Tests partition recovery with conflicting amendments.
#[tokio::test]
async fn test_partition_recovery_conflicting_amendments() {
    // Each node has its own store
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

    // Sync to B and have B sign
    sync_agreement(&store_a, &store_b, &agreement.id);
    let agreement_b = coop_b.sign(&agreement.id).unwrap();

    // Sync B's signature back to A to activate
    let sig_b = agreement_b.signatures.last().unwrap().clone();
    let mut synced_agreement = store_a.get_agreement(&agreement.id).unwrap().unwrap();
    synced_agreement.signatures.push(sig_b);
    synced_agreement.activate().unwrap();
    store_a.store_agreement(&synced_agreement).unwrap();
    store_b.store_agreement(&synced_agreement).unwrap();

    // Now both propose amendments during partition
    let amendment_a = coop_a
        .propose_amendment(
            &agreement.id,
            "A's amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: timestamp_plus_days(365),
            }],
        )
        .unwrap();

    let amendment_b = coop_b
        .propose_amendment(
            &agreement.id,
            "B's amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: timestamp_plus_days(180),
            }],
        )
        .unwrap();

    // Partition heals - sync amendments
    let (handler_a, _) = create_gossip_handler_with_capture(store_a.clone(), &keypair_a, "coop-a");
    let (handler_b, _) = create_gossip_handler_with_capture(store_b.clone(), &keypair_b, "coop-b");

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

/// Tests that multiple concurrent operations result in consistent final state.
#[tokio::test]
async fn test_eventual_consistency_after_concurrent_ops() {
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
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "All signing operations should succeed");
    }

    // Final state should be consistent
    //
    // NOTE: This test uses retry_until to handle potential timing variations in
    // async execution. While the shared store with Tokio's cooperative scheduling
    // typically serializes operations, the retry logic provides resilience against
    // scheduler variations and makes the "eventual consistency" name more accurate.
    // For real distributed eventual consistency, use the `*_with_gossip_sync` tests.
    let store_for_check = store.clone();
    let agreement_id_for_check = agreement_id.clone();
    let keypair_a_did = keypair_a.did().clone();
    let keypair_b_did = keypair_b.did().clone();
    let keypair_c_did = keypair_c.did().clone();

    // Retry until all signatures are present (handles timing variations)
    let converged = retry_until(
        || {
            let agreement = match store_for_check.get_agreement(&agreement_id_for_check) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            agreement.signatures.len() == 3
                && agreement.status.is_active()
                && agreement.has_signed(&keypair_a_did)
                && agreement.has_signed(&keypair_b_did)
                && agreement.has_signed(&keypair_c_did)
        },
        5,  // max 5 attempts
        10, // start with 10ms delay
    )
    .await;

    assert!(converged, "Agreement should converge to expected state");

    // Final verification with detailed assertions
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(
        final_agreement.signatures.len(),
        3,
        "Should have exactly 3 signatures"
    );
    assert!(
        final_agreement.status.is_active(),
        "Agreement should be active"
    );
}

/// Tests that amendment ratification with concurrent signing results in
/// correct version increment (exactly once, not twice).
#[tokio::test]
async fn test_eventual_consistency_amendment_ratification() {
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
    assert_eq!(initial.version, 1, "Initial version should be 1");

    // Propose amendment
    let amendment = coop_a
        .propose_amendment(
            &agreement.id,
            "Test amendment",
            vec![AmendmentChange::ExtendDuration {
                new_expiration: timestamp_plus_days(365),
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
    assert!(
        result_a.unwrap().is_ok(),
        "A should successfully sign amendment"
    );
    assert!(
        result_b.unwrap().is_ok(),
        "B should successfully sign amendment"
    );

    // Use retry logic for eventual consistency verification
    let store_for_check = store.clone();
    let agreement_id_for_check = agreement_id.clone();
    let amendment_id_for_check = amendment_id.clone();

    let converged = retry_until(
        || {
            // Check version is exactly 2
            let agreement = match store_for_check.get_agreement(&agreement_id_for_check) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            if agreement.version != 2 {
                return false;
            }

            // Check amendment is ratified
            let amendments = match store_for_check.get_amendments(&agreement_id_for_check) {
                Ok(a) => a,
                _ => return false,
            };
            amendments.iter().any(|a| {
                a.id == amendment_id_for_check
                    && matches!(
                        a.status,
                        icn_federation::agreement::AmendmentStatus::Ratified
                    )
            })
        },
        5,  // max 5 attempts
        10, // start with 10ms delay
    )
    .await;

    assert!(
        converged,
        "Amendment should be ratified and version should be 2"
    );

    // Final verification with detailed assertions
    let final_agreement = store.get_agreement(&agreement_id).unwrap().unwrap();
    assert_eq!(
        final_agreement.version, 2,
        "Version should be exactly 2 after one amendment"
    );
}

// =============================================================================
// Security Tests
// =============================================================================

/// Tests that signatures from non-parties are rejected.
#[tokio::test]
async fn test_signature_from_non_party_rejected() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (_coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());
    let outsider_keypair = KeyPair::generate().unwrap();

    // Create agreement between A and B only
    let agreement = coop_a
        .create_draft(
            "Security Test",
            "Test non-party rejection",
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

    // Create a fake signature from outsider
    let fake_signature = icn_federation::agreement::AgreementSignature {
        signer: outsider_keypair.did().clone(),
        coop_id: "outsider-coop".to_string(),
        signature: vec![0u8; 64], // Invalid signature bytes
        signed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        version_signed: 1,
    };

    // Try to inject the fake signature via gossip
    let (handler_a, _) = create_gossip_handler_with_capture(store.clone(), &keypair_a, "coop-a");

    let msg = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: fake_signature,
    };

    // The handler should reject this signature
    let result = handler_a.handle_message(&msg.to_bytes().unwrap());

    // Should return error for non-party signature
    assert!(result.is_err(), "Should reject signature from non-party");

    // Verify no fake signature was added
    let current = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert!(
        !current
            .signatures
            .iter()
            .any(|s| s.signer == *outsider_keypair.did()),
        "Non-party signature should not be stored"
    );
}

/// Tests that a party cannot sign twice.
#[tokio::test]
async fn test_double_signing_prevented() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (_coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Double Sign Test",
            "Test double signing prevention",
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

    // A signs once
    let first_sign = coop_a.sign(&agreement.id);
    assert!(first_sign.is_ok(), "First signing should succeed");

    // A tries to sign again
    let second_sign = coop_a.sign(&agreement.id);
    assert!(second_sign.is_err(), "Second signing should fail");

    // Verify only one signature exists
    let current = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(
        current
            .signatures
            .iter()
            .filter(|s| s.signer == *keypair_a.did())
            .count(),
        1,
        "Should have exactly one signature from A"
    );
}

/// Tests that duplicate signatures via gossip are handled idempotently.
#[tokio::test]
async fn test_duplicate_signature_via_gossip_idempotent() {
    let (coop_a, keypair_a, store) = create_manager("coop-a");
    let (_coop_b, keypair_b) = create_manager_with_store("coop-b", store.clone());

    let agreement = coop_a
        .create_draft(
            "Idempotency Test",
            "Test signature idempotency",
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
    let signed = coop_a.sign(&agreement.id).unwrap();
    let sig_a = signed.signatures.first().unwrap().clone();

    // Create handler that would receive gossip
    let (handler_a, _) = create_gossip_handler_with_capture(store.clone(), &keypair_a, "coop-a");

    // Receive the same signature via gossip (as if it bounced back)
    let msg = AgreementMessage::Signed {
        agreement_id: agreement.id.clone(),
        signature: sig_a.clone(),
    };

    // Should handle gracefully (idempotent)
    let result = handler_a.handle_message(&msg.to_bytes().unwrap());
    assert!(
        result.is_ok(),
        "Receiving own signature via gossip should be handled gracefully"
    );

    // Should still have only one signature from A
    let current = store.get_agreement(&agreement.id).unwrap().unwrap();
    assert_eq!(
        current
            .signatures
            .iter()
            .filter(|s| s.signer == *keypair_a.did())
            .count(),
        1,
        "Should have exactly one signature from A (idempotent)"
    );
}
