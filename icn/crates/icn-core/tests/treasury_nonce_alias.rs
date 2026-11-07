#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Treasury-nonce identity on the real post-acceptance governance -> ledger execution path.
//!
//! Two separate invariants are examined; they must not be conflated.
//!
//! * **Issue 1 (aliasing).** The ledger keys `ledger:treasury_nonce:<treasury_id>` by the raw string a
//!   `Spend` payload names, while `Did` equality (N2 I7) treats several multibase spellings as one
//!   principal. Does an alternate spelling of the same treasury open an independent nonce slot?
//! * **Issue 2 (binding).** The nonce key comes from the payload's treasury, but the account actually
//!   debited is the ledger's *configured* treasury. Does a payload naming some other treasury get its
//!   own nonce slot while still debiting the configured one?
//!
//! Scope of this harness: it fires `SystemEvent::ProposalAccepted`, so it models execution of a
//! proposal that has ALREADY been accepted. Voting, quorum, proposer standing and the gateway's
//! proposal-creation checks are NOT exercised here. Every spend in these tests is a distinct
//! accepted proposal with a distinct receipt id, so a rejection cannot be a duplicate-receipt
//! short-circuit. Tests that need one open with a legitimate spend (a positive control) that
//! proves the fixture applies a valid spend before the case under test is attempted.
//!
//! The ledger is wired as production wires it (`new_with_receipt_index`, persistent nonce index).
//!
//! Test classes: `control_*` and `characterization_*` pass today. `invariant_*` state the safe
//! behaviour, FAIL today, and are `#[ignore]`d with the reason (run with `--ignored`).

use icn_core::services::LedgerServiceImpl;
use icn_core::supervisor::effect_dispatcher::EffectDispatcher;
use icn_core::supervisor::governance_executor::KernelGovernanceExecutor;
use icn_governance::proposal::{ProposalPayload, TreasuryProposalOperation};
use icn_governance_actor::create_effect_subscription;
use icn_identity::{Did, KeyPair};
use icn_kernel_api::authz::AllowAllOracle;
use icn_kernel_api::effects::EffectResult;
use icn_kernel_api::events::SystemEvent;
use icn_kernel_api::protocol_params::StubParamStore;
use icn_kernel_api::services::LedgerService;
use icn_ledger::types::ProvenanceRef;
use icn_ledger::Ledger;
use icn_store::SledStore;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{mpsc, RwLock};

const NONCE_PREFIX: &[u8] = b"ledger:treasury_nonce:";
const AMOUNT: i64 = 10;
// The ledger's `net_change` is `debit - credit` (types.rs), so a DEBITED account's balance rises and
// a credited one's falls. Balances below are in that convention, not a cash-in-hand view.

struct Spend {
    receipt_id: String,
    result: EffectResult,
}

struct Harness {
    _tmp: TempDir,
    ledger: Arc<RwLock<Ledger>>,
    service: Arc<LedgerServiceImpl>,
    index: Arc<SledStore>,
    dispatcher: Arc<EffectDispatcher>,
    configured: Did,
    seq: u32,
}

impl Harness {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let configured = KeyPair::generate().unwrap().did().clone();
        let ledger_store = Arc::new(SledStore::open(tmp.path().join("ledger")).unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let index = Arc::new(SledStore::open(tmp.path().join("ledger-receipt-index")).unwrap());
        // Same constructor production uses (supervisor/lifecycle.rs): persistent nonce index.
        let service = Arc::new(LedgerServiceImpl::new_with_receipt_index(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            configured.clone(),
            index.clone(),
        ));
        let executor = KernelGovernanceExecutor::new(Arc::new(StubParamStore))
            .with_ledger_service(service.clone());
        let dispatcher = Arc::new(EffectDispatcher::new(Arc::new(executor)));
        Self {
            _tmp: tmp,
            ledger,
            service,
            index,
            dispatcher,
            configured,
            seq: 0,
        }
    }

    /// Execute one accepted `Spend` proposal (unique proposal id, hence unique receipt id).
    async fn accept_spend(&mut self, treasury: &Did, recipient: &Did, nonce: u64) -> Spend {
        // Journal entry ids hash a millisecond timestamp but not the governance provenance, and
        // treasury entries carry no nonce, so two identically-shaped spends in one millisecond
        // collide on entry id (observed while writing these tests; unrelated to the nonce). Space
        // the spends so this harness measures the nonce and not that.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        self.seq += 1;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let subscription = create_effect_subscription(move |effects, receipt_id| {
            tx.send((effects, receipt_id.clone())).unwrap();
        });
        let payload = ProposalPayload::Treasury {
            operation: TreasuryProposalOperation::Spend {
                treasury_did: treasury.clone(),
                amount: AMOUNT,
                currency: "HOURS".to_string(),
                recipient: recipient.clone(),
                memo: format!("nonce probe {}", self.seq),
                nonce,
            },
        };
        subscription(SystemEvent::ProposalAccepted {
            proposal_id: format!("spend-prop-{:03}", self.seq),
            domain_id: "test-coop".to_string(),
            payload: serde_json::to_value(&payload).unwrap(),
            decided_at: 1_700_000_000,
            canonical_payload_hash: Some(format!("sha256:probe-{}", self.seq)),
            governance_decision_hash: None,
        });
        let (effects, receipt_id) = rx.recv().await.expect("effects emitted");
        let mut results = self
            .dispatcher
            .execute_effects(effects, &receipt_id)
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "one effect expected: {results:?}");
        Spend {
            receipt_id,
            result: results.remove(0),
        }
    }

    /// Persisted `ledger:treasury_nonce:<id>` rows, as (id, nonce), sorted.
    fn nonce_rows(&self) -> Vec<(String, u64)> {
        let mut rows: Vec<(String, u64)> = self
            .index
            .db()
            .scan_prefix(NONCE_PREFIX)
            .map(|kv| {
                let (k, v) = kv.unwrap();
                let id = String::from_utf8(k[NONCE_PREFIX.len()..].to_vec()).unwrap();
                (id, u64::from_le_bytes(v.as_ref().try_into().unwrap()))
            })
            .collect();
        rows.sort();
        rows
    }

    async fn balance(&self, did: &Did) -> i64 {
        self.ledger.read().await.get_balance(did, "HOURS")
    }

    /// (debited account ids, governance receipt ids) over every journal entry.
    async fn journal(&self) -> (Vec<String>, Vec<String>) {
        let guard = self.ledger.read().await;
        let entries = guard.get_all_entries().unwrap();
        let debited = entries
            .iter()
            .flat_map(|e| e.accounts.iter())
            .filter(|a| a.debit.is_some())
            .map(|a| a.account_id.as_str().to_string())
            .collect();
        let mut receipts: Vec<String> = entries
            .iter()
            .filter_map(|e| match &e.provenance {
                ProvenanceRef::Governance { receipt_id, .. } => Some(receipt_id.to_string()),
                _ => None,
            })
            .collect();
        receipts.sort();
        (debited, receipts)
    }
}

/// `did:icn:f<hex>`: the base16 multibase spelling of the same 32-byte principal.
fn hex_alias(did: &Did) -> Did {
    let hex: String = did
        .identifier_bytes()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Did::from_str(&format!("did:icn:f{hex}")).unwrap()
}

fn rejected_by_nonce(s: &Spend) -> bool {
    !s.result.success && s.result.message.contains("Treasury nonce mismatch")
}

// ---------------------------------------------------------------------------------------------
// Control

// Fails if same-spelling stale-nonce protection regresses, or if the fixture stops being valid.
#[tokio::test(flavor = "multi_thread")]
async fn control_same_spelling_stale_nonce_is_rejected() {
    let mut h = Harness::new();
    let treasury = h.configured.clone();
    let recipient = KeyPair::generate().unwrap().did().clone();

    let first = h.accept_spend(&treasury, &recipient, 0).await;
    assert!(
        first.result.success,
        "fixture must apply a legitimate spend: {:?}",
        first.result
    );
    assert_eq!(h.nonce_rows(), vec![(treasury.to_string(), 1)]);
    assert_eq!(h.balance(&treasury).await, AMOUNT);
    assert_eq!(h.balance(&recipient).await, -AMOUNT);

    let stale = h.accept_spend(&treasury, &recipient, 0).await;
    assert_ne!(
        stale.receipt_id, first.receipt_id,
        "distinct receipts: not a duplicate"
    );
    assert!(
        rejected_by_nonce(&stale),
        "must be the nonce check: {:?}",
        stale.result
    );

    // Rejection left no trace.
    assert_eq!(h.nonce_rows(), vec![(treasury.to_string(), 1)]);
    assert_eq!(h.balance(&treasury).await, AMOUNT);
    assert_eq!(h.balance(&recipient).await, -AMOUNT);
    let (debited, receipts) = h.journal().await;
    assert_eq!(debited, vec![treasury.to_string()]);
    assert_eq!(receipts, vec![first.receipt_id]);
}

// ---------------------------------------------------------------------------------------------
// Issue 1: aliasing. Safe behaviour (fail today).

// Invariant: one treasury principal has one nonce, whatever multibase spelling names it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "fails today: nonce is keyed by the raw spelling (issue 1, open owner decision O-3)"]
async fn invariant_alias_spelling_must_not_reset_the_treasury_nonce() {
    let mut h = Harness::new();
    let treasury = h.configured.clone();
    let alias = hex_alias(&treasury);
    assert_eq!(
        alias, treasury,
        "premise: Did equality identifies the two spellings"
    );
    assert_ne!(
        alias.to_string(),
        treasury.to_string(),
        "premise: the spellings differ"
    );
    let recipient = KeyPair::generate().unwrap().did().clone();

    let first = h.accept_spend(&treasury, &recipient, 0).await;
    assert!(
        first.result.success,
        "fixture must apply a legitimate spend: {:?}",
        first.result
    );

    // A second accepted proposal built from the same stale view (nonce 0), alias-spelled.
    let stale = h.accept_spend(&alias, &recipient, 0).await;
    assert_ne!(
        stale.receipt_id, first.receipt_id,
        "distinct receipts: not a duplicate"
    );
    // Outcome, not mechanism: a safe fix may refuse this at a binding check with another message.
    assert!(
        !stale.result.success,
        "alias-spelled spend at an already-used expected nonce must not apply: {:?}",
        stale.result
    );
    assert_eq!(h.balance(&treasury).await, AMOUNT, "no second debit");
    assert_eq!(
        h.journal().await.1,
        vec![first.receipt_id],
        "only the first receipt journaled"
    );
}

// Invariant: the nonce a client reads is the same for every spelling of the treasury.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "fails today: nonce is keyed by the raw spelling (issue 1, open owner decision O-3)"]
async fn invariant_nonce_read_is_spelling_independent() {
    let mut h = Harness::new();
    let treasury = h.configured.clone();
    let alias = hex_alias(&treasury);
    let recipient = KeyPair::generate().unwrap().did().clone();
    let first = h.accept_spend(&treasury, &recipient, 0).await;
    assert!(first.result.success, "{:?}", first.result);
    assert_eq!(
        h.service.get_treasury_nonce(&alias.to_string()).unwrap(),
        h.service.get_treasury_nonce(&treasury.to_string()).unwrap(),
    );
}

// ---------------------------------------------------------------------------------------------
// Issue 2: binding. Safe behaviour (fails today).

// Invariant: a Spend naming a treasury other than the configured one must not debit the configured
// one (and so must not obtain a private nonce lane for it).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "fails today: payload treasury is not bound to the configured treasury (issue 2, owner decision O-5)"]
async fn invariant_payload_treasury_must_match_the_configured_treasury() {
    let mut h = Harness::new();
    let other = KeyPair::generate().unwrap().did().clone();
    assert_ne!(other, h.configured);
    let recipient = KeyPair::generate().unwrap().did().clone();

    // Positive control in a SEPARATE harness (own ledger and nonce index): a spend naming the
    // configured treasury applies, so the fixture is valid. It must not share a nonce lane with
    // the case under test, or a fix that merely keys the nonce by the debited account could turn
    // this test green without any binding check.
    {
        let mut control_h = Harness::new();
        let control_treasury = control_h.configured.clone();
        let control = control_h
            .accept_spend(&control_treasury, &recipient, 0)
            .await;
        assert!(
            control.result.success,
            "fixture must apply a legitimate spend: {:?}",
            control.result
        );
    }

    // The case under test, on a fresh ledger whose configured nonce lane is untouched (0): the
    // payload names a different treasury at that same expected nonce.
    // Outcome, not mechanism: any refusal satisfies the invariant.
    let spend = h.accept_spend(&other, &recipient, 0).await;
    assert!(
        !spend.result.success,
        "spend naming a non-configured treasury must be refused: {:?}",
        spend.result
    );
    assert_eq!(
        h.balance(&h.configured.clone()).await,
        0,
        "configured treasury untouched"
    );
    assert!(
        h.nonce_rows()
            .iter()
            .all(|(id, _)| id != &other.to_string()),
        "no nonce lane may be created for a treasury that is not the configured one"
    );
}

// ---------------------------------------------------------------------------------------------
// Characterization of what happens TODAY (not an endorsement).

// Three accepted spends, all with expected nonce 0, naming (a) the configured treasury, (b) an
// unrelated principal, (c) an alias spelling of the configured treasury. All three apply, each
// opens its own nonce row, and every one debits the single configured treasury.
#[tokio::test(flavor = "multi_thread")]
async fn characterization_distinct_treasury_identifiers_each_get_nonce_zero_and_debit_one_account()
{
    let mut h = Harness::new();
    let configured = h.configured.clone();
    let unrelated = KeyPair::generate().unwrap().did().clone();
    let alias = hex_alias(&configured);
    let recipient = KeyPair::generate().unwrap().did().clone();

    let a = h.accept_spend(&configured, &recipient, 0).await;
    let b = h.accept_spend(&unrelated, &recipient, 0).await;
    let c = h.accept_spend(&alias, &recipient, 0).await;
    for (name, s) in [("configured", &a), ("unrelated", &b), ("alias", &c)] {
        assert!(s.result.success, "{name} spend applied: {:?}", s.result);
    }
    assert!(
        a.receipt_id != b.receipt_id
            && b.receipt_id != c.receipt_id
            && a.receipt_id != c.receipt_id
    );

    // Persisted nonce rows: one per spelling the payloads used; the account debited has the
    // alias-spelled and the foreign rows as separate, unrelated lanes.
    let mut expected = vec![
        (configured.to_string(), 1),
        (unrelated.to_string(), 1),
        (alias.to_string(), 1),
    ];
    expected.sort();
    assert_eq!(h.nonce_rows(), expected);

    // Resulting balances: the configured treasury paid three times; the named foreign treasury was
    // never touched; the recipient received all three.
    assert_eq!(h.balance(&configured).await, 3 * AMOUNT);
    assert_eq!(h.balance(&unrelated).await, 0);
    assert_eq!(h.balance(&recipient).await, -3 * AMOUNT);

    // Actual debit account and receipt identity, from the journal.
    let (debited, receipts) = h.journal().await;
    assert_eq!(debited, vec![configured.to_string(); 3]);
    let mut want = vec![a.receipt_id, b.receipt_id, c.receipt_id];
    want.sort();
    assert_eq!(receipts, want);
}
