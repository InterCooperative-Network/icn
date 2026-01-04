#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Test verifying idempotency in governance→ledger integration
//!
//! **CRITICAL FIX**: If a ProposalAccepted event is processed twice, the ledger
//! transaction should only execute once, preventing double-counting of balances.

use anyhow::Result;
use icn_core::{EventBus, SystemEvent};
use icn_governance::ProposalPayload;
use icn_identity::KeyPair;
use icn_ledger::Ledger;
use icn_store::{SledStore, Store};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[tokio::test]
async fn test_duplicate_proposal_event_is_idempotent() -> Result<()> {
    // This test verifies the idempotency fix
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing duplicate event processing (VERIFYING IDEMPOTENCY FIX) ===");

    // Setup
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();
    let recipient_keypair = KeyPair::generate()?;
    let recipient_did = recipient_keypair.did().clone();

    let ledger_store = Arc::new(SledStore::temporary()?);
    let gov_store = Arc::new(SledStore::temporary()?);
    let ledger = Ledger::new(ledger_store.clone())?;
    let ledger_handle = Arc::new(RwLock::new(ledger));
    let event_bus = Arc::new(EventBus::new());

    // Set up event handler (same as in supervisor)
    let _handle = {
        let ledger_clone = ledger_handle.clone();
        let own_did = did.clone();
        let audit_store = gov_store.clone();

        event_bus
            .subscribe(Arc::new(move |event| {
                if let SystemEvent::ProposalAccepted {
                    proposal_id,
                    payload:
                        ProposalPayload::Budget {
                            amount,
                            recipient,
                            currency,
                            ..
                        },
                    ..
                } = event
                {
                    info!(
                        "📊 Executing budget proposal {}: {} {} to {}",
                        proposal_id.0, amount, currency, recipient
                    );

                    let ledger = ledger_clone.clone();
                    let prop_id = proposal_id.clone();
                    let from_did = own_did.clone();
                    let store = audit_store.clone();

                    tokio::spawn(async move {
                        use icn_ledger::entry::JournalEntryBuilder;

                        // IDEMPOTENCY CHECK: Skip if proposal already executed
                        // Uses fail-safe pattern: refuse execution if cannot verify
                        let audit_key = format!("gov:audit:{}", prop_id.0);
                        match store.get(audit_key.as_bytes()) {
                            Ok(Some(_)) => {
                                info!(
                                    "Proposal {} already executed, skipping duplicate event",
                                    prop_id.0
                                );
                                return;
                            }
                            Ok(None) => {
                                // Not executed yet, proceed
                            }
                            Err(e) => {
                                // Store read error: REFUSE to execute (fail-safe)
                                eprintln!(
                                    "ERROR: Failed to check audit trail for proposal {}: {}",
                                    prop_id.0, e
                                );
                                eprintln!(
                                    "       Refusing to execute to prevent potential duplicate"
                                );
                                return;
                            }
                        }

                        let mut ledger_guard = ledger.write().await;

                        let entry_result = JournalEntryBuilder::new(from_did.clone())
                            .credit(from_did.clone(), currency.clone(), amount)
                            .debit(recipient.clone(), currency.clone(), amount)
                            .build();

                        if let Ok(entry) = entry_result {
                            if let Ok(entry_hash) = ledger_guard.append_entry(entry).await {
                                info!("✅ Executed: {} {}", amount, currency);

                                // Store audit trail
                                let audit_key = format!("gov:audit:{}", prop_id.0);
                                let audit_record = serde_json::json!({
                                    "proposal_id": prop_id.0,
                                    "ledger_entry_hash": hex::encode(entry_hash.0),
                                    "amount": amount,
                                    "currency": currency,
                                    "recipient": recipient.to_string(),
                                });

                                if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                                    let _ = store.put(audit_key.as_bytes(), &audit_json);
                                }
                            }
                        }
                    });
                }
            }))
            .await
    };

    // Create proposal event
    let proposal_id = icn_governance::ProposalId("test-proposal".to_string());
    let event = SystemEvent::ProposalAccepted {
        proposal_id: proposal_id.clone(),
        domain_id: "test-domain".to_string(),
        payload: ProposalPayload::Budget {
            amount: 5000,
            recipient: recipient_did.clone(),
            currency: "credits".to_string(),
            purpose: "Test payment".to_string(),
        },
        decided_at: 1234567890,
    };

    // Emit event ONCE
    info!("Emitting event first time...");
    event_bus.emit(event.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Check balances after first emission
    let sender_balance_1 = ledger_handle.read().await.get_balance(&did, "credits");
    let recipient_balance_1 = ledger_handle
        .read()
        .await
        .get_balance(&recipient_did, "credits");

    info!(
        "After 1st emission - Sender: {}, Recipient: {}",
        sender_balance_1, recipient_balance_1
    );
    assert_eq!(
        sender_balance_1, -5000,
        "First execution should deduct 5000"
    );
    assert_eq!(
        recipient_balance_1, 5000,
        "First execution should credit 5000"
    );

    // Emit SAME event AGAIN (simulating duplicate gossip message or retry)
    info!("Emitting DUPLICATE event...");
    event_bus.emit(event).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Check balances after second emission
    let sender_balance_2 = ledger_handle.read().await.get_balance(&did, "credits");
    let recipient_balance_2 = ledger_handle
        .read()
        .await
        .get_balance(&recipient_did, "credits");

    info!(
        "After 2nd emission - Sender: {}, Recipient: {}",
        sender_balance_2, recipient_balance_2
    );

    // **IDEMPOTENCY VERIFICATION**: Balances should NOT change after duplicate event
    if sender_balance_2 == -5000 && recipient_balance_2 == 5000 {
        info!("✅ IDEMPOTENCY FIX WORKING: Duplicate event was ignored!");
        info!("   Sender balance: -5000 (correct, unchanged)");
        info!("   Recipient balance: +5000 (correct, unchanged)");
    } else {
        info!("❌ IDEMPOTENCY FAILED: Balances changed on duplicate event");
        info!("   Sender balance: {} (expected -5000)", sender_balance_2);
        info!(
            "   Recipient balance: {} (expected +5000)",
            recipient_balance_2
        );
    }

    // Assert that duplicate events are properly ignored
    assert_eq!(
        sender_balance_2, -5000,
        "Duplicate event should not change sender balance"
    );
    assert_eq!(
        recipient_balance_2, 5000,
        "Duplicate event should not change recipient balance"
    );

    Ok(())
}
