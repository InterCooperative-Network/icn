//! Slice A — receipt-index anti-entropy rehearsal (fixture-only).
//!
//! Implements `docs/spec/network-anti-entropy-proof-loops.md` §"First safe
//! proof-loop / dogfood slice" → Slice A. This is the first end-to-end
//! exercise of the schema chain landed by #1843 (`AntiEntropyProbe` +
//! `StateDigest` family) and #1844 (`DivergenceEvidence` + `RepairPlan` +
//! 18-class `DivergenceClass`).
//!
//! # What this is
//!
//! A deterministic in-memory test harness that proves the schema chain can
//! be driven end-to-end without inventing runtime behavior. The decisive
//! property under test:
//!
//! ```text
//! Peer A has r1, r2, r3.
//! Peer B has r1, r2.
//! Peer A probes the receipt-index state class.
//! Fixture compare detects B missing r3.
//! Fixture classify produces DivergenceEvidence { class: MissingReceipt }.
//! Fixture plan produces RepairPlan { action: FetchMissing }.
//! Fixture apply copies only public fixture receipt r3 into B's index.
//! After-state matches.
//! Public RepairReceipt (#1849) with EffectOutcome::Applied is constructed
//! over the after-state digest; verify_binding() passes; the cross-link
//! hashes resolve back to the evidence and plan.
//! Cockpit / member-shell surfaces move from open / "sync delayed" to
//! resolved / "receipt available".
//! ```
//!
//! # What this is NOT
//!
//! * Not a live network. No sockets, no QUIC, no gossip actor, no spawned
//!   tasks. The fixture uses in-memory `BTreeMap`s only.
//! * Not a runtime mutation. Nothing on disk, in K3s, or in any deployed
//!   service is touched. Nothing is gossiped.
//! * Not real federation. The DIDs are fixture strings; the policy clause
//!   is a fixture identifier; the freshness window is fixture timestamps.
//! * Not private data. Every "receipt" is a public fixture-only hash with
//!   a human-readable label and no body bytes. The
//!   `private_content_implication` flag is `false` throughout.
//! * Not a chaos test (`#1010` owns that).
//! * Not a production claim. No clause of this fixture is an assertion
//!   that ICN-native anti-entropy operates today against real peers.
//! * Not a `PeerSyncReport` implementation. That identifier remains
//!   design-level per spec §"Proof artifacts"; the fixture uses a
//!   private `FixtureSyncOutcome` enum scoped to this file for the
//!   compare-phase result, distinct from the resolved repair artifact.
//! * Not a live repair. The `RepairReceipt` constructed at phase 7 is
//!   evidence of what a fixture peer would have produced; no real
//!   `FetchMissing` ran against a real peer.

use std::collections::{BTreeMap, BTreeSet};

use icn_gossip::{to_bloom_projection, BloomFilter};
use icn_kernel_api::{
    AntiEntropyProbe, AuthorityBasis, BoundaryRuleRef, BoundaryRuleSet, Did, DigestMismatch,
    DivergenceClass, DivergenceEvidence, EffectOutcome, ExpectedRepairReceiptClass, Hash, PeerSet,
    PolicyClauseRef, ProbeScope, ReceiptDigest, RepairAction, RepairPlan, RepairReceipt,
    RepairReceiptClass, RequestedResponseClass, StateClass, StateDigest, TriggerSource,
};

// ---------------------------------------------------------------------------
// Fixture types (private to this test crate)
// ---------------------------------------------------------------------------

/// A fixture receipt held by a peer's in-memory index.
///
/// Public fixture-only. `receipt_hash` is the content-addressed key (a
/// deterministic 32-byte value chosen by the test); `label` is a
/// human-readable tag that appears only in assertion messages. There is no
/// body field by construction — the privacy contract of Slice A (no
/// private content) is enforced structurally by this struct's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureReceipt {
    receipt_hash: Hash,
    label: &'static str,
}

fn fixture_receipt(label: &'static str, byte: u8) -> FixtureReceipt {
    FixtureReceipt {
        receipt_hash: [byte; 32],
        label,
    }
}

/// A fixture peer with an in-memory receipt index.
///
/// No sockets, no actor handle, no spawned task. Just a DID string and a
/// `BTreeMap` of public fixture receipts. The peer's `state_digest()`
/// returns a `StateDigest::Bloom(BloomProjection)` constructed via the
/// real `icn-gossip` Bloom-filter primitive — the cross-link helper
/// (`icn_gossip::to_bloom_projection`) is the same one PR #1843 added.
#[derive(Debug, Clone)]
struct FixturePeer {
    did: Did,
    receipt_index: BTreeMap<Hash, FixtureReceipt>,
}

impl FixturePeer {
    fn new(did: &str, receipts: impl IntoIterator<Item = FixtureReceipt>) -> Self {
        let mut receipt_index = BTreeMap::new();
        for r in receipts {
            receipt_index.insert(r.receipt_hash, r);
        }
        Self {
            did: did.to_string(),
            receipt_index,
        }
    }

    fn receipt_hashes(&self) -> Vec<Hash> {
        self.receipt_index.keys().copied().collect()
    }

    fn receipt_hash_set(&self) -> BTreeSet<Hash> {
        self.receipt_index.keys().copied().collect()
    }

    /// Build the peer's Bloom projection over its receipt-index hashes.
    ///
    /// Uses the real `icn-gossip::BloomFilter` primitive that the spec
    /// names in §"State classes covered" → receipt index → "Bloom filter
    /// over receipt content-hashes". The `hint_count` field carries the
    /// exact cardinality (this is a fixture; we know it).
    fn bloom_projection(&self) -> icn_kernel_api::BloomProjection {
        let hashes = self.receipt_hashes();
        let mut filter = BloomFilter::new_adaptive(hashes.len().max(1));
        for h in &hashes {
            filter.insert(h);
        }
        to_bloom_projection(&filter.to_data(), hashes.len() as u32)
    }

    fn state_digest(&self) -> StateDigest {
        StateDigest::Bloom(self.bloom_projection())
    }

    fn receipt_digest(&self) -> ReceiptDigest {
        ReceiptDigest::new(self.state_digest())
    }

    /// Fixture-only fetch. Copies the named receipt from `source` into
    /// `self`'s in-memory index. This is NOT runtime repair, NOT a gossip
    /// fetch, NOT a network call — it is the fixture's stand-in for the
    /// `RepairAction::FetchMissing` action.
    fn fixture_apply_fetch_missing(&mut self, source: &FixturePeer, hashes: &[Hash]) {
        for h in hashes {
            if let Some(receipt) = source.receipt_index.get(h) {
                self.receipt_index.insert(*h, receipt.clone());
            }
        }
    }
}

/// Result of comparing two fixture receipt indexes.
///
/// **Private to this test file.** The public `PeerSyncReport` identifier
/// from the spec is still design-level. When the kernel lands its real
/// wire-stable comparison record, this enum will be deleted or replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FixtureSyncOutcome {
    /// Both peers' receipt-index hash sets are equal.
    Matching,
    /// The remote peer is missing entries the local peer has.
    MissingOnRemote { missing: Vec<Hash> },
    /// The local peer is missing entries the remote peer has.
    MissingOnLocal { missing: Vec<Hash> },
    /// Both peers have entries the other does not (not exercised by
    /// Slice A).
    BothDirectionsDiverge {
        missing_on_remote: Vec<Hash>,
        missing_on_local: Vec<Hash>,
    },
}

/// Pure set-difference between two fixture receipt indexes.
fn fixture_compare_receipt_indexes(
    local: &FixturePeer,
    remote: &FixturePeer,
) -> FixtureSyncOutcome {
    let local_set = local.receipt_hash_set();
    let remote_set = remote.receipt_hash_set();

    let mut missing_on_remote: Vec<Hash> = local_set.difference(&remote_set).copied().collect();
    let mut missing_on_local: Vec<Hash> = remote_set.difference(&local_set).copied().collect();
    missing_on_remote.sort();
    missing_on_local.sort();

    match (missing_on_remote.is_empty(), missing_on_local.is_empty()) {
        (true, true) => FixtureSyncOutcome::Matching,
        (false, true) => FixtureSyncOutcome::MissingOnRemote {
            missing: missing_on_remote,
        },
        (true, false) => FixtureSyncOutcome::MissingOnLocal {
            missing: missing_on_local,
        },
        (false, false) => FixtureSyncOutcome::BothDirectionsDiverge {
            missing_on_remote,
            missing_on_local,
        },
    }
}

/// Fixture renderer for the steward cockpit. **Private to this test
/// file.** Matches the spec §"Steward cockpit surface" field list at the
/// design level; the real cockpit (#1795) owns the live rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureCockpitView {
    open: bool,
    affected_scope: ProbeScope,
    state_class: StateClass,
    peers: Vec<Did>,
    divergence_class: DivergenceClass,
    repair_action: RepairAction,
    evidence_hash: Hash,
    plan_hash: Hash,
    /// Present only on the resolved view. The public `RepairReceipt`
    /// (#1849) binding hash that resolves the open divergence.
    repair_receipt_hash: Option<Hash>,
}

/// Fixture renderer for the member shell. **Private to this test file.**
/// The strings come verbatim from spec §"Member shell surface" — the
/// closed set of vocabulary the member shell may render. Slice A
/// exercises the `SyncDelayed → ReceiptAvailable` transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureMemberShellState {
    SyncDelayed,
    ReceiptAvailable,
}

impl FixtureMemberShellState {
    fn as_str(self) -> &'static str {
        match self {
            // Verbatim from spec §"Member shell surface".
            Self::SyncDelayed => "sync delayed",
            Self::ReceiptAvailable => "receipt available",
        }
    }
}

/// Deterministic fixture policy clause used throughout Slice A.
fn fixture_policy_clause() -> PolicyClauseRef {
    PolicyClauseRef {
        policy_id: "fixture-receipt-index-sync".to_string(),
        policy_version_id: "v1".to_string(),
        clause_id: "slice-a.missing-receipt".to_string(),
    }
}

fn fixture_scope() -> ProbeScope {
    ProbeScope::LocalDomain {
        domain_id: "fixture-local-domain-a".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Slice A end-to-end test
// ---------------------------------------------------------------------------

#[test]
fn slice_a_receipt_index_probe_classify_plan_apply_surface() {
    // ---- Fixtures ----
    let r1 = fixture_receipt("r1", 0x01);
    let r2 = fixture_receipt("r2", 0x02);
    let r3 = fixture_receipt("r3", 0x03);

    let peer_a = FixturePeer::new(
        "did:icn:fixture:a",
        vec![r1.clone(), r2.clone(), r3.clone()],
    );
    let mut peer_b = FixturePeer::new("did:icn:fixture:b", vec![r1.clone(), r2.clone()]);

    assert_eq!(
        peer_a.receipt_index.len(),
        3,
        "peer A starts with 3 receipts"
    );
    assert_eq!(
        peer_b.receipt_index.len(),
        2,
        "peer B starts with 2 receipts"
    );

    let scope = fixture_scope();
    let policy = fixture_policy_clause();

    // ---- 1. Probe ----
    let probe = AntiEntropyProbe::new(
        StateClass::ReceiptIndex,
        scope.clone(),
        peer_a.state_digest(),
        peer_a.did.clone(),
        TriggerSource::Periodic,
        1_715_000_000,
        1_715_000_030,
        RequestedResponseClass::DigestExchange,
        [0xAA; 32],
    );
    assert_eq!(probe.state_class, StateClass::ReceiptIndex);
    assert!(probe.verify_binding(), "probe binding must verify");
    assert!(probe.is_fresh(probe.freshness_emitted_at));
    // The receipt-digest wrapper is the spec's typed specialization for
    // a receipt-index `StateDigest`.
    let receipt_digest = peer_a.receipt_digest();
    assert_eq!(receipt_digest.state_class(), StateClass::ReceiptIndex);
    assert_eq!(*receipt_digest.digest(), peer_a.state_digest());

    // ---- 2. Compare ----
    let outcome = fixture_compare_receipt_indexes(&peer_a, &peer_b);
    let missing = match &outcome {
        FixtureSyncOutcome::MissingOnRemote { missing } => missing.clone(),
        other => panic!("expected MissingOnRemote; got {other:?}"),
    };
    assert_eq!(missing.len(), 1, "exactly one receipt missing on remote");
    assert_eq!(
        missing[0], r3.receipt_hash,
        "the missing receipt should be r3"
    );

    // ---- 3. Classify ----
    let evidence = DivergenceEvidence::new(
        DivergenceClass::MissingReceipt,
        StateClass::ReceiptIndex,
        scope.clone(),
        PeerSet::from_dids(vec![peer_a.did.clone(), peer_b.did.clone()]),
        DigestMismatch::MissingOnRemote {
            local: peer_a.state_digest(),
        },
        policy.clone(),
        1_715_000_001,
        1_715_000_031,
        false, // public fixture receipts; no private content
        [0xBB; 32],
    );
    assert_eq!(evidence.divergence_class, DivergenceClass::MissingReceipt);
    assert_eq!(evidence.affected_state_class, StateClass::ReceiptIndex);
    assert_eq!(evidence.scope, scope);
    assert!(
        !evidence.private_content_implication,
        "Slice A uses public fixture receipts; the private-content flag must be false"
    );
    assert!(evidence.verify_binding(), "evidence binding must verify");
    // PeerSet is canonicalized — both DIDs are present, sorted lexicographically.
    let evidence_peers = evidence.peers.dids();
    assert_eq!(evidence_peers.len(), 2);
    assert!(evidence_peers.contains(&peer_a.did));
    assert!(evidence_peers.contains(&peer_b.did));
    assert!(
        evidence_peers.windows(2).all(|w| w[0] < w[1]),
        "PeerSet must be sorted"
    );

    // ---- 4. Plan ----
    let plan = RepairPlan::new(
        RepairAction::FetchMissing,
        AuthorityBasis::DomainPolicyClause(policy.clone()),
        scope.clone(),
        BoundaryRuleSet::from_rules(vec![
            BoundaryRuleRef::NoRepairBeyondAuthority,
            BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            BoundaryRuleRef::NoRawPrivateContentInGossipOrProbes,
            BoundaryRuleRef::NoProductionOrLiveFederationClaim,
        ]),
        ExpectedRepairReceiptClass::FetchMissingReceipt,
        evidence.evidence_hash,
        1_715_000_002,
        1_715_000_032,
        [0xCC; 32],
    );
    assert_eq!(plan.action, RepairAction::FetchMissing);
    assert!(
        !matches!(plan.authority_basis, AuthorityBasis::NoAutomaticAuthority),
        "plan must NOT use authority-free repair: Slice A names a DomainPolicyClause"
    );
    assert!(matches!(
        plan.authority_basis,
        AuthorityBasis::DomainPolicyClause(_)
    ));
    let rules = plan.boundary_rules.rules();
    assert!(
        rules.contains(&BoundaryRuleRef::NoRepairBeyondAuthority),
        "must include NoRepairBeyondAuthority"
    );
    assert!(
        rules.contains(&BoundaryRuleRef::NoLocalityOrDisclosureWidening),
        "must include NoLocalityOrDisclosureWidening"
    );
    assert!(
        rules.contains(&BoundaryRuleRef::NoRawPrivateContentInGossipOrProbes),
        "must include NoRawPrivateContentInGossipOrProbes"
    );
    assert_eq!(
        plan.expected_repair_receipt_class,
        ExpectedRepairReceiptClass::FetchMissingReceipt
    );
    assert_eq!(
        plan.divergence_evidence_hash, evidence.evidence_hash,
        "plan must link back to the evidence"
    );
    assert!(plan.verify_binding(), "plan binding must verify");

    // ---- 5. Surface (open) ----
    let cockpit_open = FixtureCockpitView {
        open: true,
        affected_scope: scope.clone(),
        state_class: StateClass::ReceiptIndex,
        peers: evidence.peers.dids().to_vec(),
        divergence_class: evidence.divergence_class,
        repair_action: plan.action,
        evidence_hash: evidence.evidence_hash,
        plan_hash: plan.plan_hash,
        repair_receipt_hash: None,
    };
    assert!(cockpit_open.open);
    assert_eq!(
        cockpit_open.divergence_class,
        DivergenceClass::MissingReceipt
    );
    assert_eq!(cockpit_open.repair_action, RepairAction::FetchMissing);
    assert!(cockpit_open.repair_receipt_hash.is_none());

    let member_shell_open = FixtureMemberShellState::SyncDelayed;
    assert_eq!(member_shell_open.as_str(), "sync delayed");

    // ---- 6. Apply (fixture-only) ----
    //
    // Copies r3 from peer A's in-memory index into peer B's in-memory
    // index. This is the fixture stand-in for the FetchMissing action;
    // no network, no gossip, no runtime mutation.
    peer_b.fixture_apply_fetch_missing(&peer_a, &missing);

    // ---- 7. Evidence (after-state + RepairReceipt) ----
    assert_eq!(
        peer_b.receipt_index.len(),
        3,
        "peer B now has all three receipts"
    );
    assert!(
        peer_b.receipt_index.contains_key(&r3.receipt_hash),
        "peer B specifically now has r3"
    );
    let outcome_after = fixture_compare_receipt_indexes(&peer_a, &peer_b);
    assert_eq!(
        outcome_after,
        FixtureSyncOutcome::Matching,
        "after fixture apply, the two peers' indexes are equal"
    );
    assert_eq!(
        peer_a.receipt_hash_set(),
        peer_b.receipt_hash_set(),
        "the receipt-hash sets match"
    );
    // The plan's link back to the evidence is still intact.
    assert_eq!(plan.divergence_evidence_hash, evidence.evidence_hash);
    // Bloom digests over the equal sets are byte-identical because
    // BloomFilter is deterministic for the same insert order, and the
    // hash-keyed BTreeMap iterates in sorted order.
    assert_eq!(peer_a.state_digest(), peer_b.state_digest());

    // Public RepairReceipt (#1849) — the wire-stable evidence artifact
    // for the resolved repair. The fixture builds the receipt over
    // peer B's now-converged state digest, cross-linked back to the
    // evidence and the plan; verify_binding() proves the receipt has
    // not been tampered with. The receipt's `affected_state_class`,
    // `scope`, `authority_basis`, and `boundary_rules` are sourced
    // directly from `evidence` and `plan` so any drift in the plan→
    // receipt chain would diverge the binding hash. No live network,
    // no live repair: the receipt records what a fixture peer would
    // have produced had the bounded FetchMissing action run against
    // real peers.
    let repair_receipt = RepairReceipt::new(
        RepairReceiptClass::from(plan.expected_repair_receipt_class),
        EffectOutcome::Applied,
        evidence.evidence_hash,
        plan.plan_hash,
        evidence.affected_state_class,
        plan.scope.clone(),
        "did:icn:fixture:repair-actor".to_string(),
        plan.authority_basis.clone(),
        plan.boundary_rules.clone(),
        None,
        Some(peer_b.state_digest()),
        1_715_000_003,
        1_715_000_033,
        evidence.private_content_implication,
        None,
        [0xEE; 32],
    )
    .expect("Slice A receipt is structurally consistent");
    assert!(repair_receipt.verify_binding());
    assert_eq!(repair_receipt.effect_outcome, EffectOutcome::Applied);
    assert_eq!(
        repair_receipt.repair_receipt_class,
        RepairReceiptClass::FetchMissingReceipt
    );
    assert_eq!(
        repair_receipt.divergence_evidence_hash,
        evidence.evidence_hash
    );
    assert_eq!(repair_receipt.repair_plan_hash, plan.plan_hash);
    assert!(repair_receipt.failure_reason.is_none());
    assert!(repair_receipt.after_state_digest.is_some());
    // The receipt's class matches the plan's expected receipt class
    // (1:1 from ExpectedRepairReceiptClass per #1849).
    assert_eq!(
        ExpectedRepairReceiptClass::from(repair_receipt.repair_receipt_class),
        plan.expected_repair_receipt_class
    );

    // ---- 8. Surface (resolved) ----
    let cockpit_resolved = FixtureCockpitView {
        open: false,
        repair_receipt_hash: Some(repair_receipt.receipt_hash),
        ..cockpit_open.clone()
    };
    assert!(!cockpit_resolved.open);
    // The cross-link to evidence / plan / receipt persists in the
    // resolved view; only the open flag flips and the receipt link
    // populates.
    assert_eq!(cockpit_resolved.evidence_hash, evidence.evidence_hash);
    assert_eq!(cockpit_resolved.plan_hash, plan.plan_hash);
    assert_eq!(
        cockpit_resolved.repair_receipt_hash,
        Some(repair_receipt.receipt_hash)
    );

    let member_shell_resolved = FixtureMemberShellState::ReceiptAvailable;
    assert_eq!(member_shell_resolved.as_str(), "receipt available");
    assert_ne!(member_shell_open, member_shell_resolved);
}

// ---------------------------------------------------------------------------
// Targeted unit tests around the Slice A primitives
// ---------------------------------------------------------------------------

#[test]
fn slice_a_matching_indexes_yield_no_divergence() {
    let r1 = fixture_receipt("r1", 0x01);
    let r2 = fixture_receipt("r2", 0x02);
    let peer_a = FixturePeer::new("did:icn:fixture:a", vec![r1.clone(), r2.clone()]);
    let peer_b = FixturePeer::new("did:icn:fixture:b", vec![r1, r2]);

    assert_eq!(
        fixture_compare_receipt_indexes(&peer_a, &peer_b),
        FixtureSyncOutcome::Matching
    );
    assert_eq!(peer_a.state_digest(), peer_b.state_digest());
}

#[test]
fn slice_a_compare_distinguishes_missing_on_remote_from_missing_on_local() {
    let r1 = fixture_receipt("r1", 0x01);
    let r2 = fixture_receipt("r2", 0x02);
    let r3 = fixture_receipt("r3", 0x03);

    let peer_a = FixturePeer::new(
        "did:icn:fixture:a",
        vec![r1.clone(), r2.clone(), r3.clone()],
    );
    let peer_b = FixturePeer::new("did:icn:fixture:b", vec![r1.clone(), r2.clone()]);

    // Local has more: missing on remote.
    match fixture_compare_receipt_indexes(&peer_a, &peer_b) {
        FixtureSyncOutcome::MissingOnRemote { missing } => {
            assert_eq!(missing, vec![r3.receipt_hash]);
        }
        other => panic!("expected MissingOnRemote; got {other:?}"),
    }

    // Roles swapped: remote has more, so local is missing.
    match fixture_compare_receipt_indexes(&peer_b, &peer_a) {
        FixtureSyncOutcome::MissingOnLocal { missing } => {
            assert_eq!(missing, vec![r3.receipt_hash]);
        }
        other => panic!("expected MissingOnLocal; got {other:?}"),
    }
}

#[test]
fn slice_a_fixture_apply_is_idempotent() {
    let r1 = fixture_receipt("r1", 0x01);
    let r2 = fixture_receipt("r2", 0x02);
    let r3 = fixture_receipt("r3", 0x03);

    let peer_a = FixturePeer::new(
        "did:icn:fixture:a",
        vec![r1.clone(), r2.clone(), r3.clone()],
    );
    let mut peer_b = FixturePeer::new("did:icn:fixture:b", vec![r1, r2]);

    peer_b.fixture_apply_fetch_missing(&peer_a, &[r3.receipt_hash]);
    assert_eq!(peer_b.receipt_index.len(), 3);
    // Apply again with the same hash list — must not duplicate or
    // corrupt anything.
    peer_b.fixture_apply_fetch_missing(&peer_a, &[r3.receipt_hash]);
    assert_eq!(peer_b.receipt_index.len(), 3);
    assert_eq!(peer_b.receipt_index[&r3.receipt_hash].label, "r3");
}

#[test]
fn slice_a_member_shell_strings_match_spec_vocabulary() {
    // Spec §"Member shell surface" defines a CLOSED set of plain-language
    // status strings. Slice A's transition is "sync delayed" → "receipt
    // available". Lock the wire strings here.
    assert_eq!(
        FixtureMemberShellState::SyncDelayed.as_str(),
        "sync delayed"
    );
    assert_eq!(
        FixtureMemberShellState::ReceiptAvailable.as_str(),
        "receipt available"
    );
}

#[test]
fn slice_a_fixture_peer_has_no_runtime_state_fields() {
    // Structural contract: a fixture peer must be a plain in-memory data
    // shape with no sockets, no actor handles, no spawned tasks. This
    // test exists as a compile-time tripwire — if a future change adds a
    // runtime field to FixturePeer, a reviewer will see the size of the
    // struct grow and ask why.
    //
    // We don't measure exact size (alignment + allocator vary), but we
    // do confirm that constructing a peer never panics, never blocks,
    // and never requires a runtime.
    let _peer = FixturePeer::new(
        "did:icn:fixture:no-runtime",
        std::iter::empty::<FixtureReceipt>(),
    );
    // The size of a FixturePeer should be bounded by Did + BTreeMap
    // header — no Arc<RwLock<...>>, no JoinHandle, no Sender/Receiver.
    // (We assert this loosely by checking that the struct fits in a
    // small bound; the exact value depends on stdlib layout but the
    // order of magnitude is the point.)
    assert!(
        std::mem::size_of::<FixturePeer>() < 256,
        "FixturePeer must remain a plain data struct; if you added a runtime field, that is out of scope for Slice A"
    );
}

#[test]
fn slice_a_evidence_hash_changes_when_class_changes() {
    // Defense-in-depth: even though #1844 already covers per-field tamper
    // detection on DivergenceEvidence, this fixture verifies that the
    // "MissingReceipt" classification is genuinely bound to the evidence
    // hash and not interchangeable with another class.
    let scope = fixture_scope();
    let policy = fixture_policy_clause();
    let peers = PeerSet::from_dids(vec![
        "did:icn:fixture:a".to_string(),
        "did:icn:fixture:b".to_string(),
    ]);
    let digest = StateDigest::Bloom(
        FixturePeer::new("did:icn:fixture:a", vec![fixture_receipt("r1", 0x01)]).bloom_projection(),
    );
    let missing = DivergenceEvidence::new(
        DivergenceClass::MissingReceipt,
        StateClass::ReceiptIndex,
        scope.clone(),
        peers.clone(),
        DigestMismatch::MissingOnRemote {
            local: digest.clone(),
        },
        policy.clone(),
        1_715_000_001,
        1_715_000_031,
        false,
        [0xBB; 32],
    );
    let conflicting = DivergenceEvidence::new(
        DivergenceClass::ConflictingReceipt,
        StateClass::ReceiptIndex,
        scope,
        peers,
        DigestMismatch::MissingOnRemote { local: digest },
        policy,
        1_715_000_001,
        1_715_000_031,
        false,
        [0xBB; 32],
    );
    assert_ne!(missing.evidence_hash, conflicting.evidence_hash);
}

#[test]
fn slice_a_plan_evidence_link_breaks_if_evidence_rebuilt() {
    // If a reviewer ever changes the evidence nonce after the plan is
    // built, the link must break — that is the property the evidence_hash
    // field exists to guarantee.
    let scope = fixture_scope();
    let policy = fixture_policy_clause();
    let peers = PeerSet::from_dids(vec![
        "did:icn:fixture:a".to_string(),
        "did:icn:fixture:b".to_string(),
    ]);
    let digest = StateDigest::Bloom(
        FixturePeer::new("did:icn:fixture:a", vec![fixture_receipt("r1", 0x01)]).bloom_projection(),
    );

    let evidence_v1 = DivergenceEvidence::new(
        DivergenceClass::MissingReceipt,
        StateClass::ReceiptIndex,
        scope.clone(),
        peers.clone(),
        DigestMismatch::MissingOnRemote {
            local: digest.clone(),
        },
        policy.clone(),
        1_715_000_001,
        1_715_000_031,
        false,
        [0xBB; 32],
    );
    let plan = RepairPlan::new(
        RepairAction::FetchMissing,
        AuthorityBasis::DomainPolicyClause(policy.clone()),
        scope.clone(),
        BoundaryRuleSet::from_rules(vec![BoundaryRuleRef::NoRepairBeyondAuthority]),
        ExpectedRepairReceiptClass::FetchMissingReceipt,
        evidence_v1.evidence_hash,
        1_715_000_002,
        1_715_000_032,
        [0xCC; 32],
    );
    let evidence_v2 = DivergenceEvidence::new(
        DivergenceClass::MissingReceipt,
        StateClass::ReceiptIndex,
        scope,
        peers,
        DigestMismatch::MissingOnRemote { local: digest },
        policy,
        1_715_000_001,
        1_715_000_031,
        false,
        [0xDD; 32], // different nonce → different hash
    );
    assert_ne!(evidence_v1.evidence_hash, evidence_v2.evidence_hash);
    assert_eq!(plan.divergence_evidence_hash, evidence_v1.evidence_hash);
    assert_ne!(plan.divergence_evidence_hash, evidence_v2.evidence_hash);
}
