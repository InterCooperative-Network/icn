//! Slice B — replica-count `RedundancyProof` simulation (fixture-only).
//!
//! Implements `docs/spec/network-anti-entropy-proof-loops.md` §"Slice B
//! (after Slice A): replica-count `RedundancyProof` simulation". This is
//! the first end-to-end exercise of the `RedundancyProof` schema landed
//! by #1862 / PR #1863, wired into the existing proof rail
//! (`DivergenceEvidence` from #1844, `RepairPlan` from #1844,
//! `RepairReceipt` from #1850).
//!
//! # What this is
//!
//! A deterministic in-memory test harness that proves the schema chain can
//! be driven end-to-end for the replica-count case without inventing
//! runtime behavior. The decisive property under test:
//!
//! ```text
//! Three fixture peers (A, B, C) and one fixture artifact with
//! ReplicationPolicy.target_replicas = 3.
//! Peers A and B hold the replica; peer C does not.
//! Peer A constructs a public RedundancyProof with outcome BelowTarget
//! (observed = 2 < target = 3); verify_binding() passes.
//! Fixture classify produces DivergenceEvidence { class: ReplicaMissing,
//! affected_state_class: StorageReplicaVerification }.
//! Fixture plan produces RepairPlan { action: ReReplicate,
//! expected_repair_receipt_class: ReReplicationReceipt }.
//! Fixture apply copies the public fixture artifact into peer C's local
//! replica set (no real storage, no real gossip).
//! After-state: observed_replicas = 3, outcome TargetMet.
//! Public RepairReceipt with EffectOutcome::Applied is constructed over
//! the after-state; verify_binding() passes; the cross-link hashes
//! resolve back to the evidence and plan.
//! ```
//!
//! # What this is NOT
//!
//! * Not a live network. No sockets, no QUIC, no gossip actor, no spawned
//!   tasks. The fixture uses in-memory `BTreeSet`s only.
//! * Not a runtime replica probe. The `RedundancyProof` is constructed
//!   in-process from the same in-memory peer state; no live storage
//!   layer is queried.
//! * Not a runtime repair. The fixture's `fixture_apply_re_replicate`
//!   helper inserts a hash into an in-memory set; nothing on disk, in
//!   K3s, or in any deployed service is touched. Nothing is gossiped.
//! * Not real federation. The DIDs are fixture strings; the policy
//!   clause is a fixture identifier; the freshness window is fixture
//!   timestamps.
//! * Not private data. The fixture artifact is a public content-addressed
//!   reference with a human-readable label and no body bytes. The
//!   `private_content_implication` flag is `false` throughout.
//! * Not a chaos test (`#1010` owns that).
//! * Not a production-readiness claim.
//! * Not a live-federation claim.
//! * Not a NYCN / partner-activation claim.

use std::collections::BTreeSet;

use icn_kernel_api::{
    AuthorityBasis, BoundaryRuleRef, BoundaryRuleSet, Did, DigestMismatch, DivergenceClass,
    DivergenceEvidence, EffectOutcome, ExpectedRepairReceiptClass, Hash, PeerSet, PolicyClauseRef,
    ProbeScope, RedundancyOutcome, RedundancyProof, RepairAction, RepairPlan, RepairReceipt,
    RepairReceiptClass, StateClass,
};

// ---------------------------------------------------------------------------
// Fixture types (private to this test crate)
// ---------------------------------------------------------------------------

/// A fixture artifact identified by a deterministic content hash.
///
/// Public fixture-only. `artifact_hash` is the content-addressed key (a
/// deterministic 32-byte value chosen by the test); `label` is a
/// human-readable tag that appears only in assertion messages. There is no
/// body field by construction — the privacy contract of Slice B (no
/// private content) is enforced structurally by this struct's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureArtifact {
    artifact_hash: Hash,
    label: &'static str,
}

fn fixture_artifact(label: &'static str, byte: u8) -> FixtureArtifact {
    FixtureArtifact {
        artifact_hash: [byte; 32],
        label,
    }
}

/// A fixture peer that holds zero or more replica hashes in memory.
///
/// No sockets, no actor handle, no spawned task. Just a DID string and a
/// `BTreeSet` of public fixture artifact hashes the peer claims to hold.
#[derive(Debug, Clone)]
struct FixturePeer {
    did: Did,
    replicas: BTreeSet<Hash>,
}

impl FixturePeer {
    fn new(did: &str, holdings: impl IntoIterator<Item = Hash>) -> Self {
        Self {
            did: did.to_string(),
            replicas: holdings.into_iter().collect(),
        }
    }

    fn holds(&self, artifact: &FixtureArtifact) -> bool {
        self.replicas.contains(&artifact.artifact_hash)
    }

    /// Fixture-only re-replicate. Inserts the artifact hash into the
    /// peer's in-memory set. This is NOT runtime repair, NOT a storage
    /// write, NOT a gossip fetch — it is the fixture's stand-in for the
    /// `RepairAction::ReReplicate` action.
    fn fixture_apply_re_replicate(&mut self, artifact: &FixtureArtifact) {
        self.replicas.insert(artifact.artifact_hash);
    }
}

/// Compute the canonical `PeerSet` of peers that hold a given artifact.
///
/// Peer DIDs are passed to `PeerSet::from_dids`, which sorts and
/// deduplicates them so two reporters observing the same replica set
/// compute the same `proof_hash`.
fn peers_holding(peers: &[&FixturePeer], artifact: &FixtureArtifact) -> PeerSet {
    let dids = peers
        .iter()
        .filter(|p| p.holds(artifact))
        .map(|p| p.did.clone());
    PeerSet::from_dids(dids)
}

/// Deterministic fixture policy clause used throughout Slice B.
fn fixture_policy_clause() -> PolicyClauseRef {
    PolicyClauseRef {
        policy_id: "fixture-replica-redundancy".to_string(),
        policy_version_id: "v1".to_string(),
        clause_id: "slice-b.replica-missing".to_string(),
    }
}

fn fixture_scope() -> ProbeScope {
    ProbeScope::LocalDomain {
        domain_id: "fixture-local-domain-b".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Slice B end-to-end test
// ---------------------------------------------------------------------------

#[test]
fn slice_b_redundancy_proof_below_target_to_applied_repair() {
    // ---- Fixtures ----
    let artifact = fixture_artifact("art-1", 0x10);
    let peer_a = FixturePeer::new("did:icn:fixture:b-a", vec![artifact.artifact_hash]);
    let peer_b = FixturePeer::new("did:icn:fixture:b-b", vec![artifact.artifact_hash]);
    let mut peer_c = FixturePeer::new("did:icn:fixture:b-c", Vec::<Hash>::new());

    assert!(peer_a.holds(&artifact));
    assert!(peer_b.holds(&artifact));
    assert!(
        !peer_c.holds(&artifact),
        "peer C starts without the replica"
    );

    let scope = fixture_scope();
    let policy = fixture_policy_clause();
    const TARGET_REPLICAS: u32 = 3;

    // ---- 1. Probe (RedundancyProof — BelowTarget) ----
    //
    // Peer A acts as reporter. The replica set observed is exactly the
    // peers (A, B) that hold the artifact. Peer C is implicitly excluded
    // — the proof carries the canonical set of peers that DO hold a
    // replica, not the set that should hold one but does not. The
    // shortfall is encoded by `observed_replicas < target_replicas`
    // together with `outcome = BelowTarget`.
    let replica_peers_before = peers_holding(&[&peer_a, &peer_b, &peer_c], &artifact);
    assert_eq!(replica_peers_before.dids().len(), 2);

    let observed_at = 1_715_001_000;
    let freshness_valid_until = 1_715_001_030;

    let redundancy_below = RedundancyProof::new(
        artifact.artifact_hash,
        scope.clone(),
        RedundancyOutcome::BelowTarget,
        2,
        TARGET_REPLICAS,
        replica_peers_before.clone(),
        peer_a.did.clone(),
        observed_at,
        freshness_valid_until,
        false, // public fixture artifact; no private content
        [0xA1; 32],
    )
    .expect("Slice B below-target proof is structurally consistent");

    assert!(
        redundancy_below.verify_binding(),
        "proof binding must verify"
    );
    assert_eq!(redundancy_below.outcome, RedundancyOutcome::BelowTarget);
    assert_eq!(redundancy_below.observed_replicas, 2);
    assert_eq!(redundancy_below.target_replicas, TARGET_REPLICAS);
    assert_eq!(
        redundancy_below.replica_peers.dids().len() as u32,
        redundancy_below.observed_replicas,
        "observed_replicas must equal replica_peers length per schema invariant"
    );
    assert_eq!(
        redundancy_below.replica_peers.dids(),
        replica_peers_before.dids(),
        "replica_peers must match the canonical pre-repair set"
    );
    assert!(
        !redundancy_below.private_content_implication,
        "Slice B uses a public fixture artifact; private-content flag must be false"
    );
    assert!(
        redundancy_below.observed_at <= redundancy_below.freshness_valid_until,
        "freshness ordering invariant must hold"
    );
    assert_eq!(redundancy_below.artifact_hash, artifact.artifact_hash);
    assert_eq!(redundancy_below.scope, scope);
    assert_eq!(redundancy_below.reporter_did, peer_a.did);
    // PeerSet is canonicalized — sorted lexicographically.
    let dids = redundancy_below.replica_peers.dids();
    assert!(
        dids.windows(2).all(|w| w[0] < w[1]),
        "replica_peers must be sorted"
    );

    // ---- 2. Classify (DivergenceEvidence — ReplicaMissing) ----
    //
    // Replica-count divergence is not a two-peer digest comparison; per
    // spec §"Digest mismatch", non-digest divergence classes use
    // `DigestMismatch::NotApplicable`. The `DivergenceClass` still names
    // what diverged.
    let evidence = DivergenceEvidence::new(
        DivergenceClass::ReplicaMissing,
        StateClass::StorageReplicaVerification,
        scope.clone(),
        PeerSet::from_dids(vec![
            peer_a.did.clone(),
            peer_b.did.clone(),
            peer_c.did.clone(),
        ]),
        DigestMismatch::NotApplicable,
        policy.clone(),
        1_715_001_001,
        1_715_001_031,
        false, // no private content
        [0xB2; 32],
    );
    assert_eq!(evidence.divergence_class, DivergenceClass::ReplicaMissing);
    assert_eq!(
        evidence.affected_state_class,
        StateClass::StorageReplicaVerification
    );
    assert_eq!(evidence.scope, scope);
    assert!(matches!(
        evidence.digest_mismatch,
        DigestMismatch::NotApplicable
    ));
    assert!(!evidence.private_content_implication);
    assert!(evidence.verify_binding(), "evidence binding must verify");
    let evidence_peers = evidence.peers.dids();
    assert_eq!(evidence_peers.len(), 3);
    assert!(evidence_peers.contains(&peer_a.did));
    assert!(evidence_peers.contains(&peer_b.did));
    assert!(evidence_peers.contains(&peer_c.did));

    // ---- 3. Plan (RepairPlan — ReReplicate) ----
    let plan = RepairPlan::new(
        RepairAction::ReReplicate,
        AuthorityBasis::DomainPolicyClause(policy.clone()),
        scope.clone(),
        BoundaryRuleSet::from_rules(vec![
            BoundaryRuleRef::NoRepairBeyondAuthority,
            BoundaryRuleRef::NoLocalityOrDisclosureWidening,
            BoundaryRuleRef::NoRawPrivateContentInGossipOrProbes,
            BoundaryRuleRef::NoProductionOrLiveFederationClaim,
        ]),
        ExpectedRepairReceiptClass::ReReplicationReceipt,
        evidence.evidence_hash,
        1_715_001_002,
        1_715_001_032,
        [0xC3; 32],
    );
    assert_eq!(plan.action, RepairAction::ReReplicate);
    assert!(matches!(
        plan.authority_basis,
        AuthorityBasis::DomainPolicyClause(_)
    ));
    let rules = plan.boundary_rules.rules();
    assert!(rules.contains(&BoundaryRuleRef::NoRepairBeyondAuthority));
    assert!(rules.contains(&BoundaryRuleRef::NoLocalityOrDisclosureWidening));
    assert!(rules.contains(&BoundaryRuleRef::NoRawPrivateContentInGossipOrProbes));
    assert!(rules.contains(&BoundaryRuleRef::NoProductionOrLiveFederationClaim));
    assert_eq!(
        plan.expected_repair_receipt_class,
        ExpectedRepairReceiptClass::ReReplicationReceipt
    );
    assert_eq!(
        plan.divergence_evidence_hash, evidence.evidence_hash,
        "plan must link back to the evidence"
    );
    assert!(plan.verify_binding(), "plan binding must verify");

    // ---- 4. Apply (fixture-only re-replication) ----
    //
    // Inserts the artifact hash into peer C's in-memory replica set.
    // This is the fixture stand-in for the ReReplicate action; no
    // network, no gossip, no real storage write.
    peer_c.fixture_apply_re_replicate(&artifact);
    assert!(
        peer_c.holds(&artifact),
        "peer C now holds the replica after fixture apply"
    );

    // ---- 5. After-state RedundancyProof (TargetMet) ----
    //
    // A second proof over the converged replica set demonstrates the
    // outcome transition. The same artifact, the same scope, the same
    // target; only the observed set has grown.
    let replica_peers_after = peers_holding(&[&peer_a, &peer_b, &peer_c], &artifact);
    assert_eq!(replica_peers_after.dids().len(), 3);

    let redundancy_after = RedundancyProof::new(
        artifact.artifact_hash,
        scope.clone(),
        RedundancyOutcome::TargetMet,
        3,
        TARGET_REPLICAS,
        replica_peers_after.clone(),
        peer_a.did.clone(),
        1_715_001_003,
        1_715_001_033,
        false,
        [0xD4; 32],
    )
    .expect("Slice B target-met proof is structurally consistent");
    assert!(redundancy_after.verify_binding());
    assert_eq!(redundancy_after.outcome, RedundancyOutcome::TargetMet);
    assert_eq!(redundancy_after.observed_replicas, 3);
    assert_eq!(
        redundancy_after.replica_peers.dids().len() as u32,
        redundancy_after.observed_replicas
    );

    // ---- 6. RepairReceipt (EffectOutcome::Applied) ----
    //
    // The wire-stable evidence artifact for the resolved repair. Cross-
    // linked back to the evidence and plan; verify_binding() proves
    // the receipt has not been tampered with. `before_state_digest` and
    // `after_state_digest` are left `None` — the fixture does not model
    // a `StateDigest` projection over the storage-replica-verification
    // state class; the replica-count transition is fully captured by
    // the before/after `RedundancyProof` pair above, and the `Applied`
    // outcome accepts `None` for the digests per schema. The receipt is
    // structurally valid evidence of what a fixture peer would have
    // produced had a bounded ReReplicate action run against real peers.
    let repair_receipt = RepairReceipt::new(
        RepairReceiptClass::from(plan.expected_repair_receipt_class),
        EffectOutcome::Applied,
        evidence.evidence_hash,
        plan.plan_hash,
        evidence.affected_state_class,
        plan.scope.clone(),
        "did:icn:fixture:b-repair-actor".to_string(),
        plan.authority_basis.clone(),
        plan.boundary_rules.clone(),
        None, // before-state digest: not modeled in this fixture
        None, // after-state digest: not modeled in this fixture
        1_715_001_004,
        1_715_001_034,
        evidence.private_content_implication,
        None, // Applied → no failure reason per schema
        [0xE5; 32],
    )
    .expect("Slice B receipt is structurally consistent");
    assert!(repair_receipt.verify_binding());
    assert_eq!(repair_receipt.effect_outcome, EffectOutcome::Applied);
    assert_eq!(
        repair_receipt.repair_receipt_class,
        RepairReceiptClass::ReReplicationReceipt
    );
    assert_eq!(
        repair_receipt.divergence_evidence_hash,
        evidence.evidence_hash
    );
    assert_eq!(repair_receipt.repair_plan_hash, plan.plan_hash);
    assert!(repair_receipt.failure_reason.is_none());
    // The receipt's class matches the plan's expected receipt class
    // (1:1 from ExpectedRepairReceiptClass per #1850).
    assert_eq!(
        ExpectedRepairReceiptClass::from(repair_receipt.repair_receipt_class),
        plan.expected_repair_receipt_class
    );

    // ---- 7. Freshness ordering across the chain ----
    //
    // Each phase's emitted timestamp is at or after the previous phase's
    // emitted timestamp. The chain is not a wall-clock claim; it is a
    // structural ordering the fixture controls by construction.
    assert!(redundancy_below.observed_at <= evidence.freshness_emitted_at);
    assert!(evidence.freshness_emitted_at <= plan.freshness_emitted_at);
    assert!(plan.freshness_emitted_at <= redundancy_after.observed_at);
    assert!(redundancy_after.observed_at <= repair_receipt.applied_at);
}
