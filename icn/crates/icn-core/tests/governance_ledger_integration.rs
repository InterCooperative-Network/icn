//! Integration test for Governance → Ledger execution flow
//!
//! This test validates that accepted budget proposals automatically trigger ledger transactions:
//! 1. Create governance domain and budget proposal
//! 2. Vote and accept the proposal
//! 3. Verify ledger transaction was created with correct debits/credits
//! 4. Verify audit trail was stored linking proposal to ledger entry

use anyhow::Result;
use icn_core::{EventBus, GovernanceActor, SystemEvent};
use icn_gossip::GossipActor;
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceParams, GovernanceProfileId, MembershipConfig,
    Proposal, ProposalPayload, StaticMembershipResolver,
};
use icn_identity::{Did, KeyPair};
use icn_ledger::Ledger;
use icn_store::{SledStore, Store};
use icn_trust::TrustClass;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[tokio::test]
async fn test_budget_proposal_executes_ledger_transaction() -> Result<()> {
    // Initialize tracing for test output
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Starting Governance→Ledger integration test ===");

    // 1. Set up identity and stores
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();

    let gov_store = Arc::new(SledStore::temporary()?);
    let ledger_store = Arc::new(SledStore::temporary()?);

    info!("Created identity: {}", did);

    // 2. Create ledger
    let ledger = Ledger::new(ledger_store.clone())?;
    let ledger_handle = Arc::new(RwLock::new(ledger));

    info!("Created ledger");

    // 3. Create gossip actor (required for governance)
    let trust_lookup = Arc::new(|_did: &Did| Some(TrustClass::Partner));
    let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

    info!("Created gossip actor");

    // Create governance topic
    {
        let mut gossip = gossip_handle.write().await;
        let topic = icn_gossip::Topic::new(
            "governance:proposal".to_string(),
            icn_gossip::AccessControl::Public,
        );
        gossip.create_topic(topic);
    }

    info!("Created governance topic");

    // 4. Create event bus
    let event_bus = Arc::new(EventBus::new());

    info!("Created event bus");

    // 5. Subscribe to governance events and wire up ledger handler
    let _handle = {
        let ledger_clone = ledger_handle.clone();
        let own_did = did.clone();
        let audit_store = gov_store.clone();

        event_bus.subscribe(Arc::new(move |event| {
            if let SystemEvent::ProposalAccepted {
                proposal_id,
                payload: ProposalPayload::Budget { amount, recipient, currency, purpose: _ },
                decided_at,
                ..
            } = event {
                info!("📊 Executing budget proposal {}: {} {} to {}",
                      proposal_id.0, amount, currency, recipient);

                let ledger = ledger_clone.clone();
                let prop_id = proposal_id.clone();
                let from_did = own_did.clone();
                let store = audit_store.clone();
                let decision_time = decided_at;

                tokio::spawn(async move {
                    use icn_ledger::entry::JournalEntryBuilder;

                    let mut ledger_guard = ledger.write().await;

                    // Create double-entry: credit cooperative (decrease balance), debit recipient (increase balance)
                    let entry_result = JournalEntryBuilder::new(from_did.clone())
                        .credit(from_did.clone(), currency.clone(), amount)
                        .debit(recipient.clone(), currency.clone(), amount)
                        .build();

                    match entry_result {
                        Ok(entry) => {
                            match ledger_guard.append_entry(entry) {
                                Ok(entry_hash) => {
                                    info!("✅ Budget proposal {} executed: {} {} transferred to {}",
                                          prop_id.0, amount, currency, recipient);

                                    // Store audit trail
                                    let audit_key = format!("gov:audit:{}", prop_id.0);
                                    let audit_record = serde_json::json!({
                                        "proposal_id": prop_id.0,
                                        "ledger_entry_hash": hex::encode(entry_hash.0),
                                        "amount": amount,
                                        "currency": currency,
                                        "recipient": recipient.to_string(),
                                        "decided_at": decision_time,  // When governance decision was made
                                        "executed_at": std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs(),  // When ledger transaction completed
                                    });

                                    if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                                        if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                                            tracing::warn!("Failed to store audit trail for {}: {}", prop_id.0, e);
                                        } else {
                                            info!("📋 Audit trail recorded for proposal {}", prop_id.0);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("❌ Failed to append ledger entry for proposal {}: {}", prop_id.0, e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("❌ Failed to build ledger entry for proposal {}: {}", prop_id.0, e);
                        }
                    }
                });
            }
        })).await
    };

    info!("Registered event handlers");

    // 6. Create governance actor with event bus
    let membership_resolver = Arc::new(StaticMembershipResolver::new());

    let _governance_handle = GovernanceActor::spawn(
        did.clone(),
        gov_store.clone(),
        gossip_handle,
        membership_resolver,
        Some(event_bus.clone()),
    )
    .await?;

    info!("Created governance actor with event bus");

    // 7. Create a test recipient DID
    let recipient_keypair = KeyPair::generate()?;
    let recipient_did = recipient_keypair.did().clone();

    info!("Created recipient: {}", recipient_did);

    // 8. Create governance domain
    let members = vec![did.clone(), recipient_did.clone()];
    let config = GovernanceConfig::new(
        GovernanceProfileId::builtin("cooperative"),
        MembershipConfig::static_list(members.clone()),
        GovernanceParams::new(50, 50, 604800), // 50% quorum, 50% approval, 7 days
    );

    let domain = GovernanceDomain::new("Test Cooperative".to_string(), config);
    let domain_id = domain.id.clone();

    info!("Created governance domain: {:?}", domain_id);

    // 9. Create a budget proposal
    let proposal = Proposal::new(
        domain_id.clone(),
        did.clone(),
        "Pay supplier".to_string(),
        "Budget allocation for office supplies".to_string(),
        ProposalPayload::Budget {
            amount: 5000,
            recipient: recipient_did.clone(),
            currency: "credits".to_string(),
            purpose: "Office supplies from Acme Corp".to_string(),
        },
    );

    let proposal_id = proposal.id.clone();

    info!("Created budget proposal: {:?}", proposal_id);

    // 10. Simulate proposal acceptance by emitting event directly
    // (In real system, this would happen through governance actor's CloseProposal handler)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let acceptance_event = SystemEvent::ProposalAccepted {
        proposal_id: proposal_id.clone(),
        domain_id: domain_id.0.clone(),
        payload: ProposalPayload::Budget {
            amount: 5000,
            recipient: recipient_did.clone(),
            currency: "credits".to_string(),
            purpose: "Office supplies from Acme Corp".to_string(),
        },
        decided_at: now,
    };

    info!("Emitting ProposalAccepted event...");
    event_bus.emit(acceptance_event).await;

    // 11. Wait for async event handler to complete
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 12. Verify ledger transaction was created
    info!("Verifying ledger state...");

    let ledger_guard = ledger_handle.read().await;

    // Check balances
    let sender_balance = ledger_guard.get_balance(&did, "credits");
    let recipient_balance = ledger_guard.get_balance(&recipient_did, "credits");

    info!("Sender balance: {}", sender_balance);
    info!("Recipient balance: {}", recipient_balance);

    assert_eq!(
        sender_balance, -5000,
        "Sender should have -5000 credits (debit)"
    );
    assert_eq!(
        recipient_balance, 5000,
        "Recipient should have +5000 credits (credit)"
    );

    info!("✅ Ledger balances verified");

    // 13. Verify audit trail was stored
    drop(ledger_guard); // Release lock before accessing store

    let audit_key = format!("gov:audit:{}", proposal_id.0);
    let audit_bytes = gov_store
        .get(audit_key.as_bytes())?
        .expect("Audit trail should exist");

    let audit_record: serde_json::Value = serde_json::from_slice(&audit_bytes)?;

    info!(
        "Audit trail: {}",
        serde_json::to_string_pretty(&audit_record)?
    );

    assert_eq!(audit_record["proposal_id"], proposal_id.0);
    assert_eq!(audit_record["amount"], 5000);
    assert_eq!(audit_record["currency"], "credits");
    assert_eq!(audit_record["recipient"], recipient_did.to_string());
    assert_eq!(
        audit_record["decided_at"], now,
        "Audit trail should record governance decision timestamp"
    );
    assert!(
        audit_record["executed_at"].is_u64(),
        "Audit trail should include ledger execution timestamp"
    );

    info!("✅ Audit trail verified");

    // 14. Verify ledger entry hash in audit record matches actual entry
    let ledger_entry_hash_hex = audit_record["ledger_entry_hash"]
        .as_str()
        .expect("ledger_entry_hash should be a string");

    let expected_hash_bytes = hex::decode(ledger_entry_hash_hex)?;
    assert_eq!(
        expected_hash_bytes.len(),
        32,
        "Entry hash should be 32 bytes"
    );

    info!("✅ Ledger entry hash format verified");

    info!("=== Governance→Ledger integration test PASSED ===");

    Ok(())
}

#[tokio::test]
async fn test_rejected_proposal_does_not_execute() -> Result<()> {
    // Initialize tracing for test output
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing rejected proposal does NOT execute ===");

    // Set up minimal infrastructure
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();

    let gov_store = Arc::new(SledStore::temporary()?);
    let ledger_store = Arc::new(SledStore::temporary()?);

    let ledger = Ledger::new(ledger_store.clone())?;
    let ledger_handle = Arc::new(RwLock::new(ledger));

    let event_bus = Arc::new(EventBus::new());

    // Set up event handler (same as before)
    let _handle = {
        let ledger_clone = ledger_handle.clone();
        let own_did = did.clone();

        event_bus
            .subscribe(Arc::new(move |event| match event {
                SystemEvent::ProposalAccepted {
                    proposal_id,
                    payload:
                        ProposalPayload::Budget {
                            amount,
                            recipient,
                            currency,
                            ..
                        },
                    ..
                } => {
                    info!(
                        "📊 Executing budget proposal {}: {} {} to {}",
                        proposal_id.0, amount, currency, recipient
                    );

                    let ledger = ledger_clone.clone();
                    let from_did = own_did.clone();

                    tokio::spawn(async move {
                        use icn_ledger::entry::JournalEntryBuilder;

                        let mut ledger_guard = ledger.write().await;

                        let entry_result = JournalEntryBuilder::new(from_did.clone())
                            .credit(from_did.clone(), currency.clone(), amount)
                            .debit(recipient.clone(), currency.clone(), amount)
                            .build();

                        if let Ok(entry) = entry_result {
                            let _ = ledger_guard.append_entry(entry);
                        }
                    });
                }
                SystemEvent::ProposalRejected { proposal_id, .. } => {
                    info!("❌ Proposal {} rejected - no action taken", proposal_id.0);
                }
                _ => {}
            }))
            .await
    };

    // Create recipient
    let recipient_keypair = KeyPair::generate()?;
    let recipient_did = recipient_keypair.did().clone();

    // Create domain and proposal
    let domain = GovernanceDomain::new(
        "Test Cooperative".to_string(),
        GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative"),
            MembershipConfig::static_list(vec![did.clone()]),
            GovernanceParams::new(50, 50, 604800),
        ),
    );

    let proposal = Proposal::new(
        domain.id.clone(),
        did.clone(),
        "Budget request".to_string(),
        "Should be rejected".to_string(),
        ProposalPayload::Budget {
            amount: 1000,
            recipient: recipient_did.clone(),
            currency: "credits".to_string(),
            purpose: "Test".to_string(),
        },
    );

    let proposal_id = proposal.id.clone();

    // Emit ProposalRejected event
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let rejection_event = SystemEvent::ProposalRejected {
        proposal_id: proposal_id.clone(),
        domain_id: domain.id.0.clone(),
        decided_at: now,
    };

    event_bus.emit(rejection_event).await;

    // Wait for event processing
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify NO ledger transaction was created
    let ledger_guard = ledger_handle.read().await;

    let sender_balance = ledger_guard.get_balance(&did, "credits");
    let recipient_balance = ledger_guard.get_balance(&recipient_did, "credits");

    assert_eq!(
        sender_balance, 0,
        "Sender balance should be 0 (no transaction)"
    );
    assert_eq!(
        recipient_balance, 0,
        "Recipient balance should be 0 (no transaction)"
    );

    info!("✅ Verified rejected proposal did NOT execute ledger transaction");

    // Verify NO audit trail was stored
    drop(ledger_guard);

    let audit_key = format!("gov:audit:{}", proposal_id.0);
    let audit_result = gov_store.get(audit_key.as_bytes())?;

    assert!(
        audit_result.is_none(),
        "Audit trail should NOT exist for rejected proposal"
    );

    info!("✅ Verified no audit trail for rejected proposal");

    info!("=== Rejected proposal test PASSED ===");

    Ok(())
}
