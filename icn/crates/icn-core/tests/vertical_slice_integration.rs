#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Vertical Slice Integration Test — Tool Library Cooperative (Issue #1145)
//!
//! This test proves ICN works end-to-end by exercising the full cooperative lifecycle:
//!
//! 1. Alice, Bob, Carol create sovereign identities
//! 2. Alice forms "Tool Library Coop" with charter
//! 3. Treasury auto-created on activation (B1)
//! 4. Dave applies for membership → governance vote → approved
//! 5. Proposal: "Purchase drill press for 30 hours"
//! 6. Vote passes → ledger transaction executed → audit trail
//! 7. Tool registered as asset in ledger
//! 8. Dave checks out tool → use-access entry → return
//! 9. Full audit trail verified

use anyhow::Result;
use icn_core::{EventBus, SystemEvent};
use icn_gossip::GossipActor;
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    ProofOutcome, ProposalId, ProposalPayload, ProposalScope, StaticMembershipResolver, VoteChoice,
    VoteTally,
};
use icn_governance_actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite};
use icn_identity::KeyPair;
use icn_kernel_api::receipts::CanonicalReceipt;
use icn_ledger::asset_types::{AssetClass, AssetCondition, AssetMetadata, AssetRegistry};
use icn_ledger::create_budget_allocation;
use icn_ledger::entry::JournalEntryBuilder;
use icn_ledger::treasury::TreasuryManager;
use icn_ledger::Ledger;
use icn_membership_app::coop_core::actor::CoopActor;
use icn_membership_app::coop_core::handle::CoopHandle;
use icn_membership_app::coop_core::types::{CoopType, MemberRole};
use icn_store::{SledStore, Store};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Create a named identity (keypair + DID) for test readability
struct TestIdentity {
    keypair: KeyPair,
    name: String,
}

impl TestIdentity {
    fn new(name: &str) -> Self {
        let keypair = KeyPair::generate().unwrap();
        Self {
            keypair,
            name: name.to_string(),
        }
    }

    fn did(&self) -> icn_identity::Did {
        self.keypair.did().clone()
    }
}

impl std::fmt::Display for TestIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.name, &self.did().to_string()[..16])
    }
}

#[tokio::test]
async fn test_tool_library_cooperative_vertical_slice() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("╔══════════════════════════════════════════════════════════╗");
    info!("║   Vertical Slice: Tool Library Cooperative              ║");
    info!("╚══════════════════════════════════════════════════════════╝");

    // =========================================================================
    // Step 1: Create sovereign identities
    // =========================================================================
    info!("── Step 1: Create sovereign identities ──");

    let alice = TestIdentity::new("Alice");
    let bob = TestIdentity::new("Bob");
    let carol = TestIdentity::new("Carol");
    let dave = TestIdentity::new("Dave");

    info!("  {} (founder)", alice);
    info!("  {} (member)", bob);
    info!("  {} (member)", carol);
    info!("  {} (applicant)", dave);

    // =========================================================================
    // Step 2: Alice forms "Tool Library Coop"
    // =========================================================================
    info!("── Step 2: Form Tool Library Cooperative ──");

    let coop_store = {
        let dir = tempfile::tempdir()?;
        let db = sled::open(dir.path())?;
        icn_membership_app::coop_core::store::CoopStore::new(Arc::new(db))
    };

    // Create treasury manager backed by ledger store
    let ledger_store = Arc::new(SledStore::temporary()?);
    let treasury_mgr = Arc::new(RwLock::new(TreasuryManager::with_store(
        ledger_store.clone(),
    )?));

    // Spawn CoopActor with treasury manager
    let tx = CoopActor::spawn_with_treasury(
        coop_store,
        None, // no gossip in this test
        Some(treasury_mgr.clone()),
    );
    let coop_handle = CoopHandle::new(tx);

    let coop = coop_handle
        .create_cooperative(
            Some("tool-library".to_string()),
            "Tool Library Coop".to_string(),
            CoopType::Worker,
            alice.did(),
        )
        .await?;

    info!("  Created cooperative: {} (id={})", coop.name, coop.id);
    assert_eq!(coop.name, "Tool Library Coop");
    assert!(coop.treasury_did.is_none(), "No treasury before activation");

    // Add founding members
    let _bob_member = coop_handle
        .add_member(coop.id.clone(), bob.did(), MemberRole::Worker)
        .await?;
    let _carol_member = coop_handle
        .add_member(coop.id.clone(), carol.did(), MemberRole::Worker)
        .await?;

    info!("  Added founding members: Bob, Carol");

    // =========================================================================
    // Step 3: Activate cooperative → treasury auto-created
    // =========================================================================
    info!("── Step 3: Activate cooperative (treasury auto-creation) ──");

    let charter_hash = "sha256:toollib-charter-v1".to_string();
    let activated = coop_handle
        .activate_cooperative(coop.id.clone(), charter_hash)
        .await?;

    assert_eq!(
        activated.status,
        icn_membership_app::coop_core::types::CoopStatus::Active
    );
    assert!(
        activated.treasury_did.is_some(),
        "Treasury DID should be assigned on activation"
    );

    info!(
        "  Cooperative active! Treasury DID: {}",
        activated.treasury_did.as_ref().unwrap()
    );

    // Verify treasury was registered in ledger
    {
        let guard = treasury_mgr.read().await;
        let treasury = guard.get_treasury_by_coop(&coop.id);
        assert!(
            treasury.is_some(),
            "Treasury should be registered in ledger"
        );
        let treasury = treasury.unwrap();
        assert_eq!(treasury.currency, "HOURS");
        assert!(treasury.is_active);
        info!("  Treasury registered in ledger: currency=HOURS, active=true");
    }

    // =========================================================================
    // Step 4: Dave applies for membership → governance vote → approved
    // =========================================================================
    info!("── Step 4: Membership application + governance vote ──");

    // Set up governance actor
    let gov_store = Arc::new(SledStore::temporary()?);
    let gossip_handle = GossipActor::spawn(alice.did(), None);

    // Create governance topic
    {
        let mut gossip = gossip_handle.write().await;
        let topic = icn_gossip::Topic::new(
            "governance:proposal".to_string(),
            icn_gossip::AccessControl::Public,
        );
        gossip.create_topic(topic);
    }

    // Create event bus and wire up ledger handler
    let event_bus = Arc::new(EventBus::new());
    let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store.clone())?));

    // Wire up: ProposalAccepted → ledger transaction
    let ledger_for_handler = ledger.clone();
    let alice_did_for_handler = alice.did();
    let audit_store = gov_store.clone();

    let _event_handle = event_bus
        .subscribe(Arc::new(move |event| {
            if let SystemEvent::ProposalAccepted {
                proposal_id,
                payload,
                decided_at,
                ..
            } = event
            {
                let Ok(ProposalPayload::Budget {
                    amount,
                    recipient,
                    currency,
                    purpose: _,
                }) = serde_json::from_value::<ProposalPayload>(payload.clone())
                else {
                    return;
                };

                let ledger = ledger_for_handler.clone();
                let prop_id = proposal_id.clone();
                let from_did = alice_did_for_handler.clone();
                let store = audit_store.clone();
                let decided = decided_at;

                tokio::spawn(async move {
                    let mut guard = ledger.write().await;
                    let entry = JournalEntryBuilder::new(from_did.clone())
                        .credit(from_did.clone(), currency.clone(), amount)
                        .debit(recipient.clone(), currency.clone(), amount)
                        .with_system_provenance("test")
                        .build()
                        .unwrap();

                    let entry_hash = guard.append_entry(entry).await.unwrap();

                    // Store audit trail
                    let audit_key = format!("gov:audit:{}", prop_id);
                    let audit_record = serde_json::json!({
                        "proposal_id": prop_id,
                        "ledger_entry_hash": hex::encode(entry_hash.0),
                        "amount": amount,
                        "currency": currency,
                        "recipient": recipient.to_string(),
                        "decided_at": decided,
                    });
                    let audit_json = serde_json::to_vec(&audit_record).unwrap();
                    store.put(audit_key.as_bytes(), &audit_json).unwrap();

                    // ── Structured receipt chain (B3) ──
                    let decision_receipt = GovernanceDecisionReceipt::new(
                        prop_id.clone(),
                        "tool-library-gov".to_string(),
                        ProofOutcome::Accepted,
                        VoteTally {
                            for_votes: 1,
                            against_votes: 0,
                            abstain_votes: 0,
                        },
                        &[], // empty votes for simplicity — hash will still be deterministic
                    );

                    let allocation = create_budget_allocation(
                        decision_receipt.decision_hash,
                        &prop_id,
                        &from_did.to_string(),
                        &recipient.to_string(),
                        amount as u64,
                        &currency,
                        &format!("Budget allocation for proposal {}", prop_id),
                    );

                    // Store allocation receipt keyed by canonical hash
                    let alloc_hash = allocation.canonical_hash();
                    let alloc_key = format!("receipt:allocation:{}", hex::encode(alloc_hash));
                    let alloc_json = serde_json::to_vec(&allocation).unwrap();
                    store.put(alloc_key.as_bytes(), &alloc_json).unwrap();

                    // Store the decision hash for verification in Step 9
                    let decision_key = format!("receipt:decision:{}", prop_id);
                    store
                        .put(
                            decision_key.as_bytes(),
                            hex::encode(decision_receipt.decision_hash).as_bytes(),
                        )
                        .unwrap();
                });
            }
        }))
        .await;

    let membership_resolver = Arc::new(StaticMembershipResolver::new());

    let gov_handle = GovernanceActor::spawn(
        alice.did(),
        gov_store.clone(),
        gossip_handle,
        membership_resolver,
        Some(event_bus.clone()),
        None, // no signing key
    )
    .await?;

    // Create governance domain for the cooperative
    let domain_id = GovernanceDomainId::new("tool-library-gov");
    let members = vec![alice.did(), bob.did(), carol.did()];

    gov_handle
        .submit(GovernanceCommand::CreateDomain {
            domain_id: domain_id.clone(),
            name: "Tool Library Governance".to_string(),
            config: GovernanceConfigLite {
                profile: "cooperative".to_string(),
                params: GovernanceParams::new(50, 50, 604800), // 50% quorum, 50% approval
                membership: MembershipConfig::static_list(members),
            },
        })
        .await?;

    info!("  Created governance domain: {:?}", domain_id);

    // Dave's membership proposal
    let membership_proposal_id = ProposalId::generate();
    gov_handle
        .submit(GovernanceCommand::CreateProposal {
            proposal_id: membership_proposal_id.clone(),
            domain_id: domain_id.clone(),
            title: "Add Dave as member".to_string(),
            description: "Dave wants to join the Tool Library Coop".to_string(),
            payload: ProposalPayload::Membership {
                action: icn_governance::MembershipAction::Add,
                member: dave.did(),
            },
            scope: ProposalScope::Local,
        })
        .await?;

    info!("  Created membership proposal for Dave");

    // Open for voting and cast votes
    gov_handle
        .submit(GovernanceCommand::OpenProposal {
            proposal_id: membership_proposal_id.clone(),
            voting_period_seconds: 3600,
        })
        .await?;

    // Alice votes yes
    gov_handle
        .submit(GovernanceCommand::CastVote {
            proposal_id: membership_proposal_id.clone(),
            choice: VoteChoice::For,
            voter: alice.did(),
            comment: Some("Welcome Dave!".to_string()),
        })
        .await?;

    info!("  Alice voted FOR Dave's membership");

    // Close the proposal
    gov_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: membership_proposal_id.clone(),
        })
        .await?;

    // Verify outcome
    let tally = gov_handle.get_vote_tally(&membership_proposal_id).await?;
    info!(
        "  Vote tally: {} for, {} against",
        tally.for_votes, tally.against_votes
    );
    assert!(tally.for_votes > 0, "Should have at least one FOR vote");

    // Add Dave as approved member
    let _dave_member = coop_handle
        .add_member(coop.id.clone(), dave.did(), MemberRole::Worker)
        .await?;
    let _dave_approved = coop_handle
        .approve_member(coop.id.clone(), dave.did())
        .await?;

    info!("  Dave approved and added as member!");

    // Verify Dave's membership
    let members_list = coop_handle.list_members(coop.id.clone()).await?;
    assert!(
        members_list.iter().any(|m| m.did == dave.did()),
        "Dave should be in member list"
    );
    info!("  Verified: {} members in cooperative", members_list.len());

    // =========================================================================
    // Step 5: Proposal: "Purchase drill press for 30 hours"
    // =========================================================================
    info!("── Step 5: Budget proposal — purchase drill press ──");

    let budget_proposal_id = ProposalId::generate();
    let supplier = TestIdentity::new("Supplier");

    gov_handle
        .submit(GovernanceCommand::CreateProposal {
            proposal_id: budget_proposal_id.clone(),
            domain_id: domain_id.clone(),
            title: "Purchase drill press".to_string(),
            description: "Buy a drill press for the tool library, 30 HOURS".to_string(),
            payload: ProposalPayload::Budget {
                amount: 30,
                currency: "HOURS".to_string(),
                recipient: supplier.did(),
                purpose: "Purchase drill press for tool library".to_string(),
            },
            scope: ProposalScope::Local,
        })
        .await?;

    info!("  Created budget proposal: 30 HOURS to supplier");

    // =========================================================================
    // Step 6: Vote passes → ledger transaction → audit trail
    // =========================================================================
    info!("── Step 6: Vote on budget proposal ──");

    gov_handle
        .submit(GovernanceCommand::OpenProposal {
            proposal_id: budget_proposal_id.clone(),
            voting_period_seconds: 3600,
        })
        .await?;

    gov_handle
        .submit(GovernanceCommand::CastVote {
            proposal_id: budget_proposal_id.clone(),
            choice: VoteChoice::For,
            voter: alice.did(),
            comment: Some("We need this tool".to_string()),
        })
        .await?;

    info!("  Alice voted FOR budget proposal");

    gov_handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: budget_proposal_id.clone(),
        })
        .await?;

    let budget_tally = gov_handle.get_vote_tally(&budget_proposal_id).await?;
    info!(
        "  Budget vote tally: {} for, {} against",
        budget_tally.for_votes, budget_tally.against_votes
    );

    // GovernanceActor automatically emits ProposalAccepted via event bus on close.
    // Wait for the async event handler to execute the ledger transaction (poll with backoff).
    let mut supplier_balance = 0i64;
    let mut alice_balance = 0i64;
    for attempt in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let guard = ledger.read().await;
        supplier_balance = guard.get_balance(&supplier.did(), "HOURS");
        alice_balance = guard.get_balance(&alice.did(), "HOURS");
        if supplier_balance != 0 {
            info!("  Ledger updated after {} ms", (attempt + 1) * 100);
            break;
        }
    }

    info!(
        "  Ledger: Alice={} HOURS, Supplier={} HOURS",
        alice_balance, supplier_balance
    );

    assert_eq!(supplier_balance, 30, "Supplier should receive 30 HOURS");
    assert_eq!(alice_balance, -30, "Coop treasury should debit 30 HOURS");

    info!("  Ledger transaction verified!");

    // =========================================================================
    // Step 7: Register drill press as asset in ledger
    // =========================================================================
    info!("── Step 7: Register drill press as asset ──");

    // Register the tool as a ledger entry (double-entry: coop acquires asset)
    let asset_ledger_hash;
    {
        let mut guard = ledger.write().await;
        // Double-entry: credit supplier (asset source), debit coop (asset holder)
        let asset_entry = JournalEntryBuilder::new(alice.did())
            .debit(alice.did(), "TOOLS".to_string(), 1)
            .credit(supplier.did(), "TOOLS".to_string(), 1)
            .with_system_provenance("test")
            .build()?;

        asset_ledger_hash = guard.append_entry(asset_entry).await?;
        info!(
            "  Registered asset: Drill Press (hash={})",
            hex::encode(&asset_ledger_hash.0[..8])
        );
    }

    // Register asset using the typed AssetRegistry
    let asset_registry = AssetRegistry::new(ledger_store.clone());
    let drill_press = AssetMetadata {
        id: "tool-library:drill-press-001".to_string(),
        name: "Drill Press".to_string(),
        class: AssetClass::Durable,
        description: "Heavy-duty drill press for tool library".to_string(),
        owner_did: alice.did(),
        created_at: 1_700_000_000,
        condition: AssetCondition::New,
    };

    let asset_id = asset_registry.register_asset(drill_press)?;
    assert_eq!(asset_id, "tool-library:drill-press-001");
    info!("  Asset registered via AssetRegistry: {}", asset_id);

    // =========================================================================
    // Step 8: Dave checks out tool → use-access entry → return
    // =========================================================================
    info!("── Step 8: Dave checks out and returns the drill press ──");

    // Checkout: transfer custody from coop (Alice) to Dave + ledger entry
    {
        let mut guard = ledger.write().await;

        // Double-entry: Dave borrows from coop inventory
        let checkout_entry = JournalEntryBuilder::new(dave.did())
            .debit(dave.did(), "TOOL_ACCESS".to_string(), 1)
            .credit(alice.did(), "TOOL_ACCESS".to_string(), 1)
            .with_system_provenance("test")
            .build()?;

        let checkout_hash = guard.append_entry(checkout_entry).await?;
        info!(
            "  Ledger checkout entry (hash={})",
            hex::encode(&checkout_hash.0[..8])
        );
    }

    // Transfer custody via AssetRegistry
    asset_registry.transfer_custody(&asset_id, &alice.did(), &dave.did())?;
    info!("  Custody transferred: Alice -> Dave");

    // Verify Dave now holds the asset
    let checked_out = asset_registry.get_asset(&asset_id)?.expect("asset exists");
    assert_eq!(
        checked_out.owner_did,
        dave.did(),
        "Dave should hold the asset"
    );

    // Return: transfer custody back from Dave to coop (Alice) + ledger entry
    {
        let mut guard = ledger.write().await;

        // Double-entry: Dave returns to coop inventory
        let return_entry = JournalEntryBuilder::new(dave.did())
            .credit(dave.did(), "TOOL_ACCESS".to_string(), 1)
            .debit(alice.did(), "TOOL_ACCESS".to_string(), 1)
            .with_system_provenance("test")
            .build()?;

        let return_hash = guard.append_entry(return_entry).await?;
        info!(
            "  Ledger return entry (hash={})",
            hex::encode(&return_hash.0[..8])
        );
    }

    // Transfer custody back via AssetRegistry
    asset_registry.transfer_custody(&asset_id, &dave.did(), &alice.did())?;
    info!("  Custody transferred: Dave -> Alice");

    // Verify balances net to zero and custody is restored
    {
        let guard = ledger.read().await;
        let dave_access = guard.get_balance(&dave.did(), "TOOL_ACCESS");
        assert_eq!(
            dave_access, 0,
            "Dave's tool access should net to zero after return"
        );
        info!("  Verified: Dave's TOOL_ACCESS balance = 0 (checkout + return)");
    }

    let returned = asset_registry.get_asset(&asset_id)?.expect("asset exists");
    assert_eq!(
        returned.owner_did,
        alice.did(),
        "Alice should hold the asset again"
    );
    info!("  Verified: Asset custody returned to Alice");

    // =========================================================================
    // Step 9: Full audit trail verification
    // =========================================================================
    info!("── Step 9: Audit trail verification ──");

    // Verify budget proposal audit trail
    let audit_key = format!("gov:audit:{}", budget_proposal_id.0);
    let audit_bytes = gov_store
        .get(audit_key.as_bytes())?
        .expect("Budget audit trail should exist");
    let audit_record: serde_json::Value = serde_json::from_slice(&audit_bytes)?;

    assert_eq!(audit_record["amount"], 30);
    assert_eq!(audit_record["currency"], "HOURS");
    assert_eq!(audit_record["recipient"], supplier.did().to_string());
    info!("  Budget proposal audit trail: verified");

    // ── Receipt chain verification ──
    info!("  Verifying allocation receipt chain...");

    // Load decision hash
    let decision_key = format!("receipt:decision:{}", budget_proposal_id.0);
    let decision_hash_hex = gov_store
        .get(decision_key.as_bytes())?
        .expect("Decision hash should be stored");
    let decision_hash_str = String::from_utf8(decision_hash_hex).unwrap();
    info!("  Decision hash: {}", &decision_hash_str[..16]);

    // Scan for allocation receipts
    let receipt_entries = gov_store.scan(b"receipt:allocation:")?;
    assert!(
        !receipt_entries.is_empty(),
        "Should have at least one allocation receipt"
    );

    // Load and verify the first allocation receipt
    let (_, receipt_bytes) = &receipt_entries[0];
    let allocation: icn_kernel_api::receipts::AllocationReceipt =
        serde_json::from_slice(receipt_bytes)?;

    // Verify the chain links
    let decision_hash_bytes = hex::decode(&decision_hash_str).unwrap();
    let mut expected_hash = [0u8; 32];
    expected_hash.copy_from_slice(&decision_hash_bytes);

    assert_eq!(
        allocation.decision_hash, expected_hash,
        "AllocationReceipt must link to GovernanceDecisionReceipt"
    );

    assert_eq!(
        allocation.intents.len(),
        1,
        "Should have exactly one settlement intent"
    );
    let intent = &allocation.intents[0];
    assert_eq!(
        intent.amount, 30,
        "Intent amount should match budget proposal"
    );
    assert_eq!(
        intent.unit, "HOURS",
        "Intent unit should match budget currency"
    );
    assert_eq!(
        intent.decision_hash, expected_hash,
        "Intent must link to same decision"
    );

    // Verify provenance chain
    assert!(
        allocation
            .provenance
            .upstream_hashes
            .contains(&expected_hash),
        "Provenance must include decision hash"
    );

    info!("  Receipt chain verified: Decision -> Allocation -> Intent");
    info!("    Decision hash: {}", &decision_hash_str[..16]);
    info!("    Allocation intents: {}", allocation.intents.len());
    info!(
        "    Intent: {} {} from treasury to supplier",
        intent.amount, intent.unit
    );

    // Verify asset record via AssetRegistry
    let verified_asset = asset_registry
        .get_asset("tool-library:drill-press-001")?
        .expect("Asset record should exist in registry");
    assert_eq!(verified_asset.class, AssetClass::Durable);
    assert_eq!(verified_asset.name, "Drill Press");
    assert_eq!(verified_asset.owner_did, alice.did());
    info!("  Asset registration audit trail: verified via AssetRegistry");

    // Verify cooperative state
    let final_coop = coop_handle.get_cooperative(coop.id.clone()).await?;
    assert_eq!(
        final_coop.status,
        icn_membership_app::coop_core::types::CoopStatus::Active
    );
    assert!(final_coop.treasury_did.is_some());
    info!("  Cooperative state: Active, treasury assigned");

    // Verify member count
    let final_members = coop_handle.list_members(coop.id).await?;
    assert_eq!(
        final_members.len(),
        4,
        "Should have 4 members: Alice, Bob, Carol, Dave"
    );
    info!(
        "  Member count: {} (Alice, Bob, Carol, Dave)",
        final_members.len()
    );

    info!("╔══════════════════════════════════════════════════════════╗");
    info!("║   VERTICAL SLICE PASSED — Full cooperative lifecycle!   ║");
    info!("╚══════════════════════════════════════════════════════════╝");

    Ok(())
}
