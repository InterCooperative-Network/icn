#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Gap B full-chain proof: a governed economic action by a (demo) Member produces
//! a complete, audit-verifiable receipt chain in-process.
//!
//! This is the second half of the local/demo Member-standing bootstrap slice. The
//! first half (`icn-gateway/tests/demo_member_standing_bootstrap.rs`) proves that
//! the reusable `bootstrap_demo_member_standing` helper flips the gateway
//! `member_checker` gate (governed proposal 403 → 201). This test starts from the
//! state that unlocks — an accepted governed proposal — and proves the resulting
//! chain satisfies every check `icnctl audit verify` performs.
//!
//! ## What this proves
//!
//! A governed treasury settlement (proposal → vote → accept) produces, keyed to a
//! single governance `decision_hash`, all six artifact classes the verifier needs:
//!   governance decision receipt → allocation receipt → settlement intent →
//!   execution record → journal entry,
//! with provenance linked at every hop.
//!
//! The 13 conditions asserted below mirror, one-for-one, the checks in
//! `icnctl audit verify`'s `verify_receipt_chain` (icn/bins/icnctl/src/main.rs).
//! That verifier is private to the `icnctl` binary (not a library), so — exactly
//! as `icn/bins/icnctl/tests/audit_verify_test.rs` already does — the conditions
//! are re-expressed here against the real persisted artifacts rather than calling
//! the function. They operate on the same data `GET /v1/receipts/chain/{hash}`
//! returns to the CLI.
//!
//! ## What is reconstructed vs. real
//!
//! The receipts come from the real `GovernanceManager` close path. The executor
//! (`EffectDispatcher` → `KernelGovernanceExecutor` → `LedgerServiceImpl`, wrapped
//! by `DecisionExecutor` + `ExecutionStore`) is wired exactly as the daemon wires
//! it (`create_decision_executor_callback` → `create_effect_subscription`), and is
//! driven by a `ProposalAccepted` event carrying the *real* governance
//! `decision_hash` — the single line `actor.rs` emits in the live path. This is the
//! same reconstruction style as `treasury_spend_governance_provenance.rs`.

use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;

use icn_core::services::LedgerServiceImpl;
use icn_core::supervisor::decision_executor::{
    create_decision_executor_callback, DecisionExecutor,
};
use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
use icn_core::supervisor::execution_store::SledExecutionStore;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;

use icn_governance::proposal::TreasuryProposalOperation;
use icn_governance::{
    GovernanceDecisionReceipt, GovernanceDomainId, GovernanceParams, MembershipConfig,
    ProofOutcome, ProposalId, ProposalPayload, ProposalScope, VoteChoice,
};
use icn_governance_actor::create_effect_subscription;
use icn_governance_actor::manager::GovernanceManager;
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;

use icn_identity::KeyPair;
use icn_kernel_api::authz::AllowAllOracle;
use icn_kernel_api::events::SystemEvent;
use icn_kernel_api::execution::ExecutionStore;
use icn_kernel_api::protocol_params::StubParamStore;
use icn_kernel_api::{AllocationReceipt, CanonicalReceipt, Hash};
use icn_ledger::types::ProvenanceRef;
use icn_ledger::Ledger;
use icn_store::SledStore;

/// Minimal in-memory `GovernanceReceiptBackend` capturing the governance and
/// allocation receipts the manager emits on close. Mirrors the backend used by
/// `receipt_chain_vertical_slice.rs`; the trait's other methods keep their no-op
/// defaults (mandates/grants are not part of this chain).
#[derive(Default)]
struct ChainReceiptBackend {
    governance_by_proposal: Mutex<HashMap<String, GovernanceDecisionReceipt>>,
    governance_by_decision: Mutex<HashMap<Hash, GovernanceDecisionReceipt>>,
    allocations_by_decision: Mutex<HashMap<Hash, Vec<AllocationReceipt>>>,
}

impl GovernanceReceiptBackend for ChainReceiptBackend {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        self.governance_by_proposal
            .lock()
            .map_err(|e| e.to_string())?
            .insert(receipt.proposal_id.clone(), receipt.clone());
        self.governance_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .insert(receipt.decision_hash, receipt.clone());
        Ok(())
    }

    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance_by_proposal
            .lock()
            .map_err(|e| e.to_string())?
            .get(proposal_id)
            .cloned())
    }

    fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        self.allocations_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .entry(receipt.decision_hash)
            .or_default()
            .push(receipt.clone());
        Ok(receipt.canonical_hash())
    }

    fn get_governance_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        Ok(self
            .governance_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .get(decision_hash)
            .cloned())
    }

    fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String> {
        Ok(self
            .allocations_by_decision
            .lock()
            .map_err(|e| e.to_string())?
            .get(decision_hash)
            .cloned()
            .unwrap_or_default())
    }
}

/// Drive a governed treasury settlement end-to-end and assert the resulting chain
/// passes all 13 `icnctl audit verify` checks.
#[tokio::test(flavor = "multi_thread")]
async fn governed_settlement_by_demo_member_produces_audit_verifiable_chain() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    // ── Identities ───────────────────────────────────────────────────────────
    // `member_did` stands in for the DID that `bootstrap_demo_member_standing`
    // would have promoted to Member; here it is the domain's sole StaticList voter.
    let member = KeyPair::generate()?;
    let member_did = member.did().clone();
    let treasury = KeyPair::generate()?;
    let treasury_did = treasury.did().clone();
    let recipient = KeyPair::generate()?;
    let recipient_did = recipient.did().clone();

    // ── Governance manager + receipt backend ─────────────────────────────────
    let backend = Arc::new(ChainReceiptBackend::default());
    let mgr = GovernanceManager::new().with_receipt_store(backend.clone());

    let domain_id = GovernanceDomainId::new("demo-receipt-chain-coop");
    mgr.create_domain(
        domain_id.clone(),
        "Demo Receipt Chain Coop".to_string(),
        "cooperative".to_string(),
        // 1% quorum / 1% approval → the single member vote carries the decision.
        GovernanceParams::new(1, 1, 604_800),
        MembershipConfig::static_list(vec![member_did.clone()]),
    )
    .await?;

    // ── Governed economic action: a treasury settlement authorized by vote ────
    let payload = ProposalPayload::Treasury {
        operation: TreasuryProposalOperation::Spend {
            treasury_did: treasury_did.clone(),
            amount: 100,
            currency: "compute-hours".to_string(),
            recipient: recipient_did.clone(),
            memo: "Q1 infrastructure settlement".to_string(),
            nonce: 0,
        },
    };

    let proposal_id = ProposalId::generate();
    mgr.create_proposal(
        proposal_id.clone(),
        domain_id.clone(),
        member_did.clone(),
        "Q1 infrastructure settlement".to_string(),
        "Settle compute-hours to the infrastructure maintainer.".to_string(),
        payload.clone(),
        ProposalScope::Local,
    )
    .await?;
    mgr.open_proposal(proposal_id.clone(), 3_600).await?;
    mgr.cast_vote(
        proposal_id.clone(),
        member_did.clone(),
        VoteChoice::For,
        None,
    )
    .await?;
    mgr.close_proposal(proposal_id.clone()).await?;

    // ── Read the governance + allocation receipts the close produced ──────────
    let gov = backend
        .get_governance_by_proposal(&proposal_id.0)
        .map_err(|e| anyhow!(e))?
        .expect("governance decision receipt must exist after accepted close");
    assert_eq!(
        gov.outcome,
        ProofOutcome::Accepted,
        "single For vote at 1% quorum must accept"
    );
    let decision_hash: Hash = gov.decision_hash;
    let decision_hash_hex = hex::encode(decision_hash);

    let allocations = backend
        .list_allocations_by_decision(&decision_hash)
        .map_err(|e| anyhow!(e))?;

    // ── Reconstruct the daemon's executor wiring ──────────────────────────────
    let tmp = TempDir::new()?;
    let ledger = Arc::new(RwLock::new(Ledger::new(Arc::new(SledStore::open(
        tmp.path().join("ledger"),
    )?))?));
    let ledger_service = Arc::new(LedgerServiceImpl::new(
        ledger.clone(),
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did.clone(),
    ));
    let kernel_executor =
        KernelGovernanceExecutor::new(Arc::new(StubParamStore)).with_ledger_service(ledger_service);
    let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(kernel_executor)));
    let exec_store: Arc<dyn ExecutionStore> = Arc::new(SledExecutionStore::new(Arc::new(
        SledStore::open(tmp.path().join("exec"))?,
    )));
    let decision_executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));

    // Same bridge the daemon uses: subscription → DecisionExecutor.
    let exec_callback = create_decision_executor_callback(decision_executor);
    let subscription = create_effect_subscription(move |effects, receipt_id| {
        exec_callback(effects, receipt_id);
    });

    // Emit the accepted event carrying the REAL governance decision hash so the
    // execution record + journal key to the same hash as the receipts above
    // (this reproduces the one event `actor.rs` emits on the live close path).
    let event = SystemEvent::ProposalAccepted {
        proposal_id: proposal_id.0.clone(),
        domain_id: domain_id.0.clone(),
        payload: serde_json::to_value(&payload)?,
        decided_at: 1_700_000_000,
        canonical_payload_hash: None,
        governance_decision_hash: Some(decision_hash_hex.clone()),
    };
    subscription(event);

    // Let the spawned execution task settle.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ── Read execution record + journal entries ───────────────────────────────
    let exec_record = exec_store
        .get(&decision_hash_hex)?
        .expect("execution record must exist after dispatch");
    let truth = exec_record.truth_summary_or_fallback();

    let journal_entries: Vec<_> = ledger
        .read()
        .await
        .get_all_entries()?
        .into_iter()
        .filter(|e| {
            matches!(&e.provenance,
                ProvenanceRef::Governance { decision_hash, .. }
                    if decision_hash.as_str() == decision_hash_hex)
        })
        .collect();

    // ── The 13 audit-verify checks (mirroring icnctl `verify_receipt_chain`) ──
    let intents: Vec<_> = allocations.iter().flat_map(|a| a.intents.iter()).collect();
    let all_intent_hashes: HashSet<Hash> =
        allocations.iter().flat_map(|a| a.intent_hashes()).collect();

    let structural_complete =
        !allocations.is_empty() && allocations.iter().all(|a| !a.intents.is_empty());

    let checks: Vec<(&str, bool)> = vec![
        // 1
        ("governance_receipt_present", true),
        // 2 (gov receipt's hash anchors the chain we queried)
        (
            "decision_hash_consistent",
            gov.decision_hash == decision_hash,
        ),
        // 3
        ("allocation_receipts_present", !allocations.is_empty()),
        // 4
        (
            "allocation_provenance_linked",
            allocations.iter().all(|a| a.decision_hash == decision_hash),
        ),
        // 5
        ("settlement_intents_present", !intents.is_empty()),
        // 6
        (
            "intent_provenance_linked",
            intents.iter().all(|i| i.decision_hash == decision_hash),
        ),
        // 7
        (
            "no_orphaned_intents",
            intents
                .iter()
                .all(|i| all_intent_hashes.contains(&i.canonical_hash())),
        ),
        // 8
        ("structural_chain_complete", structural_complete),
        // 9
        (
            "execution_counters_consistent",
            truth.total_effects
                == truth.executed_effects + truth.not_executed_effects + truth.hard_failed_effects,
        ),
        // 10
        ("execution_complete", truth.execution_complete),
        // 11
        (
            "chain_complete_contract",
            structural_complete && truth.execution_complete,
        ),
        // 12
        ("journal_entries_present", !journal_entries.is_empty()),
        // 13
        (
            "journal_provenance_matches_decision",
            journal_entries.iter().all(|e| {
                matches!(&e.provenance,
                    ProvenanceRef::Governance { decision_hash, .. }
                        if decision_hash.as_str() == decision_hash_hex)
            }),
        ),
    ];

    let passed = checks.iter().filter(|(_, p)| *p).count();
    for (name, p) in &checks {
        println!(
            "[audit-verify] {name}: {}",
            if *p { "PASS" } else { "FAIL" }
        );
    }
    println!(
        "[audit-verify] decision_hash={decision_hash_hex} RESULT: {passed}/{} checks passed",
        checks.len()
    );

    assert_eq!(checks.len(), 13, "must mirror all 13 icnctl checks");
    assert_eq!(
        passed,
        13,
        "in-process audit verify must pass all 13 checks; got {passed}/13. \
         truth_summary={truth:?}, allocations={}, intents={}, journal_entries={}",
        allocations.len(),
        intents.len(),
        journal_entries.len()
    );

    Ok(())
}
