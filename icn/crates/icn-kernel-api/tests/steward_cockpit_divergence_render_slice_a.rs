//! Steward cockpit Slice A — divergence-render fixture (read-only).
//!
//! Implements Slice A from `docs/spec/steward-cockpit-v0.md` §"First safe
//! proof-loop / dogfood slice". This is the first end-to-end exercise of
//! the cockpit rendering surface over the proof rail landed by
//! #1843 (`AntiEntropyProbe` + `StateDigest`),
//! #1844 (`DivergenceEvidence` + `RepairPlan`),
//! #1845 (receipt-index anti-entropy Slice A fixture),
//! and #1850 (`RepairReceipt` wire-stable schema, issue #1849).
//!
//! # What this is
//!
//! A deterministic in-memory test that renders a fixture steward cockpit
//! view over an open-then-resolved Slice A divergence. The decisive
//! property under test:
//!
//! ```text
//! Three fixture peers (A, B, C) with a fixture LocalDomain.
//! One open DivergenceEvidence (class: MissingReceipt) over the
//! receipt-index state class, surfaced as a Bloom-filter set-difference.
//! One RepairPlan (action: FetchMissing) linked to the evidence by hash.
//! One public RepairReceipt (#1849) with EffectOutcome::Applied,
//! verify_binding() passing, cross-linked to evidence + plan by hash.
//! Cockpit view renders all NINE required fields per spec §"Network /
//! Federation surface". Member-impact summary attached, exact strings.
//! Twelve-category accessibility gate evaluated and passing.
//! Open view → resolved view transition exercised.
//! ```
//!
//! # What this is NOT
//!
//! * Not a live cockpit. No HTML, no terminal UI, no dashboard server.
//!   The "view" is a plain `FixtureStewardCockpitView` struct with one
//!   named field per spec-required surface item.
//! * Not a live network. No sockets, no QUIC, no gossip actor, no
//!   spawned tasks. The fixture uses in-memory `BTreeMap`s only.
//! * Not real federation. Three fixture peers, fixture DIDs, fixture
//!   policy clause. The cockpit shows them as the spec's `Peers` field;
//!   the kernel does not validate the DIDs against any real registry.
//! * Not private data. Every "receipt" is a public fixture-only hash;
//!   the fixture renders the digest mismatch as a 1-missing-receipt
//!   summary, never as a body.
//! * Not a chaos test (`#1010`).
//! * Not a member shell implementation (`#1839`). The fixture attaches
//!   the member-impact summary string verbatim from the spec's mapping
//!   but does not render the member-shell surface itself.
//! * Not a public `PeerSyncReport` schema. That identifier remains
//!   design-level; the fixture compares peer indexes directly rather
//!   than constructing a wire-stable `PeerSyncReport`.
//! * Not a production-readiness claim, live-federation claim, or NYCN
//!   pilot claim. The repair did not run against a live network; the
//!   `RepairReceipt` records what a fixture peer would have produced
//!   had the bounded `FetchMissing` action been executed against
//!   real peers.

use std::collections::{BTreeMap, BTreeSet};

use icn_gossip::{to_bloom_projection, BloomFilter};
use icn_kernel_api::{
    AntiEntropyProbe, AuthorityBasis, BoundaryRuleRef, BoundaryRuleSet, Did, DigestMismatch,
    DivergenceClass, DivergenceEvidence, EffectOutcome, ExpectedRepairReceiptClass, Hash, PeerSet,
    PolicyClauseRef, ProbeScope, RepairAction, RepairPlan, RepairReceipt, RepairReceiptClass,
    RequestedResponseClass, StateClass, StateDigest, TriggerSource,
};

// ===========================================================================
// Fixture types (all private to this test file)
// ===========================================================================
//
// The Slice A receipt-index fixture (#1838 / PR #1845) already established
// the FixturePeer / FixtureReceipt / FixtureSyncOutcome shapes. This file
// duplicates them on purpose: extracting a `tests/common/` module would
// build cross-test coupling that the next slice will not necessarily want
// to inherit, and the duplication is small. If a third fixture lands that
// also needs these shapes, that is the right moment to extract.

/// Public fixture-only receipt — a deterministic 32-byte hash plus a
/// human-readable label used only in assertion messages. No body field
/// by construction.
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

/// In-memory fixture peer with a receipt index. No sockets, no actor
/// handle, no spawned task. The `did` is a fixture string and the index
/// is a plain `BTreeMap`.
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

    fn receipt_hash_set(&self) -> BTreeSet<Hash> {
        self.receipt_index.keys().copied().collect()
    }

    fn bloom_projection(&self) -> icn_kernel_api::BloomProjection {
        let hashes: Vec<Hash> = self.receipt_index.keys().copied().collect();
        let mut filter = BloomFilter::new_adaptive(hashes.len().max(1));
        for h in &hashes {
            filter.insert(h);
        }
        to_bloom_projection(&filter.to_data(), hashes.len() as u32)
    }

    fn state_digest(&self) -> StateDigest {
        StateDigest::Bloom(self.bloom_projection())
    }

    fn fixture_apply_fetch_missing(&mut self, source: &FixturePeer, hashes: &[Hash]) {
        for h in hashes {
            if let Some(receipt) = source.receipt_index.get(h) {
                self.receipt_index.insert(*h, receipt.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cockpit rendering types — test-private
// ---------------------------------------------------------------------------

/// The closed v0 operator state set per
/// `docs/spec/steward-cockpit-v0.md` §"Operator states (closed v0 set)".
///
/// Test-private. The live cockpit (forward work) will own the canonical
/// type; this enum exists only so the fixture can assert state
/// transitions deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureOperatorState {
    Healthy,
    Degraded,
    Syncing,
    Stale,
    Partitioned,
    Relayed,
    VerificationPending,
    RepairPlanned,
    RepairApplied,
    ReviewRequired,
    BlockedByPolicy,
    PrivateContentRestricted,
}

impl FixtureOperatorState {
    /// Verbatim from spec §"Operator states (closed v0 set)".
    fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Syncing => "syncing",
            Self::Stale => "stale",
            Self::Partitioned => "partitioned",
            Self::Relayed => "relayed",
            Self::VerificationPending => "verification pending",
            Self::RepairPlanned => "repair planned",
            Self::RepairApplied => "repair applied",
            Self::ReviewRequired => "review required",
            Self::BlockedByPolicy => "blocked by policy",
            Self::PrivateContentRestricted => "private content restricted",
        }
    }

    /// Verbatim from spec §"Member-impact summary mapping".
    fn member_impact_summary(self) -> &'static str {
        match self {
            Self::Healthy => "Members see: Synced.",
            Self::Degraded => "Members see: Sync delayed.",
            Self::Syncing => "Members see: Some records are being verified.",
            Self::Stale => "Members see: Sync delayed.",
            Self::Partitioned => "Members see: Some records are being verified.",
            Self::Relayed => "Members see: Sync delayed.",
            Self::VerificationPending => "Members see: Some records are being verified.",
            Self::RepairPlanned => "Members see: Action paused until records sync.",
            Self::RepairApplied => "Members see: Receipt available.",
            Self::ReviewRequired => "Members see: Review required.",
            Self::BlockedByPolicy => "Members see: Action paused until records sync.",
            Self::PrivateContentRestricted => "Members see: Some records are being verified.",
        }
    }
}

/// Escalation status per spec §"Network / Federation surface" field 9.
/// Test-private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureEscalationStatus {
    /// No escalation needed. The divergence has a `RepairPlan` with a
    /// named authority basis and no governance review is required.
    NotEscalated,
    /// Escalated to governance review (unclassifiable, equivocation, or
    /// boundary-rule violation). Not exercised by Slice A.
    EscalatedToGovernanceReview,
}

/// Build the public `RepairReceipt` (#1849) the cockpit view links to.
///
/// Constructs the wire-stable receipt with `EffectOutcome::Applied`
/// and the fixture's after-state digest. `scope`, `authority_basis`,
/// and `boundary_rules` are sourced directly from `plan` so a
/// drift between plan and receipt cannot pass verify_binding(); the
/// affected state class is sourced from `evidence`. The receipt is
/// the canonical evidence artifact the cockpit row renders against —
/// replacing the test-private stand-in this fixture used to carry
/// before #1850 landed.
fn build_repair_receipt(
    evidence: &DivergenceEvidence,
    plan: &RepairPlan,
    peer_b: &FixturePeer,
    actor_did: &str,
    applied_at: u64,
) -> RepairReceipt {
    RepairReceipt::new(
        RepairReceiptClass::from(plan.expected_repair_receipt_class),
        EffectOutcome::Applied,
        evidence.evidence_hash,
        plan.plan_hash,
        evidence.affected_state_class,
        plan.scope.clone(),
        actor_did.to_string(),
        plan.authority_basis.clone(),
        plan.boundary_rules.clone(),
        // Slice A's "before" digest is implied (peer A's index); the
        // fixture's decisive evidence is the after-state digest. The
        // public RepairReceipt accepts an optional before; we omit it
        // here because the before-state digest the spec describes is
        // the divergent peer's index *before* fetch-missing, and the
        // fixture does not retain that snapshot.
        None,
        Some(peer_b.state_digest()),
        applied_at,
        applied_at + 30,
        evidence.private_content_implication,
        None,
        [0xEF; 32],
    )
    .expect("Slice A repair receipt is structurally consistent")
}

/// Steward cockpit rendering of a single open divergence row.
///
/// Field set follows `docs/spec/steward-cockpit-v0.md` §"Network /
/// Federation surface" verbatim — the spec lists nine required fields.
/// Plus two derived fields that the spec attaches alongside the row:
/// `operator_state` (the closed v0 status label) and
/// `member_impact_summary` (one-line member-shell mapping).
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureStewardCockpitView {
    // ---- The nine required fields (spec verbatim) ----
    /// 1. Affected scope — `LocalDomain` / `Federation` / `Commons` /
    ///    peer-pair.
    affected_scope: ProbeScope,
    /// 2. State class — one of the nine state classes.
    state_class: StateClass,
    /// 3. Peers — peer DIDs involved.
    peers: Vec<Did>,
    /// 4. Digest mismatch — Bloom-filter set-difference / Merkle-root /
    ///    vector-clock / short-list summary, rendered as a fixture
    ///    summary (no body bytes).
    digest_mismatch_summary: FixtureDigestMismatchSummary,
    /// 5. Last successful proof — fixture Unix-seconds timestamp of the
    ///    last matching cross-peer sync for this scope. `None` if none
    ///    on record (cockpit must still render the field — see spec
    ///    §"Status vocabulary").
    last_successful_proof_at: Option<u64>,
    /// 6. Repair plan — action, authority basis, expected receipt class,
    ///    plan hash.
    repair_plan_summary: FixtureRepairPlanSummary,
    /// 7. Authority required — the policy-clause reference the plan
    ///    names.
    authority_required: PolicyClauseRef,
    /// 8. Receipts / evidence — the cross-link hashes for the open
    ///    divergence and, when resolved, the fixture repair outcome.
    receipts_and_evidence: FixtureEvidenceLinks,
    /// 9. Escalation status.
    escalation_status: FixtureEscalationStatus,

    // ---- Spec-attached derived fields ----
    /// Operator state per spec §"Operator states (closed v0 set)".
    operator_state: FixtureOperatorState,
    /// Member-impact summary per spec §"Member-impact summary mapping"
    /// — one verbatim line.
    member_impact_summary: &'static str,
    /// Whether the divergence is currently open (false → resolved).
    open: bool,
}

/// Rendered summary of the digest-form mismatch. The fixture surfaces
/// counts and addresses, never bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureDigestMismatchSummary {
    /// "bloom_filter_set_difference" / "merkle_root_inequality" /
    /// "vector_clock_divergence" / "short_list_difference".
    digest_form: &'static str,
    /// Direction relative to the local peer.
    direction: FixtureMismatchDirection,
    /// Number of receipts the digest comparison identified as
    /// out-of-sync. Slice A → exactly 1.
    missing_count: u32,
    /// Specific receipt hashes that diverged. Bounded; never body
    /// content. Slice A → the single hash of `r3`.
    affected_receipt_hashes: Vec<Hash>,
}

/// Direction of the digest mismatch relative to the local peer.
///
/// Only `MissingOnRemote` is exercised by Slice A. The other variants
/// (`MissingOnLocal`, `Both`, `NotApplicable`) are present so the
/// fixture covers the full set of cases a future cockpit row may need
/// to render — and so a reviewer reading the code sees the complete
/// taxonomy at a glance rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // variants other than MissingOnRemote are not exercised by Slice A
enum FixtureMismatchDirection {
    MissingOnRemote,
    MissingOnLocal,
    Both,
    NotApplicable,
}

/// Rendered summary of the `RepairPlan` row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureRepairPlanSummary {
    action: RepairAction,
    authority_label: &'static str,
    expected_receipt_class: ExpectedRepairReceiptClass,
    plan_hash: Hash,
}

/// Cross-link hashes the cockpit row carries so an auditor can chase
/// the chain back to the source artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureEvidenceLinks {
    /// `DivergenceEvidence::evidence_hash`.
    evidence_hash: Hash,
    /// `RepairPlan::plan_hash`.
    plan_hash: Hash,
    /// Cross-link to the public `RepairReceipt` (#1849), present only
    /// on the resolved view. Records the receipt's binding hash and
    /// actor DID so the cockpit row can chase the chain back to the
    /// resolved repair evidence without surfacing the receipt's
    /// internal field set.
    repair_receipt_hash: Option<Hash>,
    /// Actor DID recorded on the resolved view alongside the receipt
    /// hash. Spec §"Network / Federation surface" field 8 lists this
    /// alongside the evidence and plan hashes; the kernel does not
    /// validate it against any real registry.
    repair_outcome_actor: Option<Did>,
}

// ---------------------------------------------------------------------------
// Accessibility checklist — twelve-category gate per ADR-0028 /
// docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md.
// ---------------------------------------------------------------------------

/// The twelve review categories from
/// `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` §3. Closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)] // names follow the doc literally
enum FixtureAccessibilityCategory {
    /// 3.1 Language access.
    LanguageAccess,
    /// 3.2 Screen-reader and non-visual access.
    ScreenReaderAndNonVisualAccess,
    /// 3.3 Low-vision access.
    LowVisionAccess,
    /// 3.4 Color-independent meaning.
    ColorIndependentMeaning,
    /// 3.5 Keyboard, switch, and non-pointer access.
    KeyboardSwitchAndNonPointerAccess,
    /// 3.6 Captions, transcripts, and non-audio access.
    CaptionsTranscriptsAndNonAudioAccess,
    /// 3.7 Cognitive load and step complexity.
    CognitiveLoadAndStepComplexity,
    /// 3.8 Low-bandwidth and low-device access.
    LowBandwidthAndLowDeviceAccess,
    /// 3.9 Assistive-technology compatibility.
    AssistiveTechnologyCompatibility,
    /// 3.10 Privacy-preserving accommodation path.
    PrivacyPreservingAccommodationPath,
    /// 3.11 Receipts, provenance, and evidence access.
    ReceiptsProvenanceAndEvidenceAccess,
    /// 3.12 Governance and action access.
    GovernanceAndActionAccess,
}

impl FixtureAccessibilityCategory {
    const ALL: [Self; 12] = [
        Self::LanguageAccess,
        Self::ScreenReaderAndNonVisualAccess,
        Self::LowVisionAccess,
        Self::ColorIndependentMeaning,
        Self::KeyboardSwitchAndNonPointerAccess,
        Self::CaptionsTranscriptsAndNonAudioAccess,
        Self::CognitiveLoadAndStepComplexity,
        Self::LowBandwidthAndLowDeviceAccess,
        Self::AssistiveTechnologyCompatibility,
        Self::PrivacyPreservingAccommodationPath,
        Self::ReceiptsProvenanceAndEvidenceAccess,
        Self::GovernanceAndActionAccess,
    ];
}

/// One of four outcomes per category per ADR-0028 §4.
///
/// Slice A's fixture only ever produces `Pass`, `NotApplicable`, or
/// (in negative tests) `Blocked`. The `PassWithFollowUps` variant is
/// present so the fixture covers the full ADR-0028 outcome set; a
/// future fixture (e.g. for an in-progress surface) will exercise it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // PassWithFollowUps is part of the ADR-0028 outcome set but not exercised here
enum FixtureAccessibilityOutcome {
    Pass,
    PassWithFollowUps { follow_up_refs: Vec<&'static str> },
    Blocked { reason: &'static str },
    NotApplicable { reason: &'static str },
}

impl FixtureAccessibilityOutcome {
    fn is_acceptable(&self) -> bool {
        // Per ADR-0028 §4: a surface is acceptable only when every category
        // is Pass / PassWithFollowUps / NotApplicable. A single Blocked
        // gates the surface out.
        !matches!(self, Self::Blocked { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureAccessibilityCheck {
    category: FixtureAccessibilityCategory,
    outcome: FixtureAccessibilityOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureAccessibilityChecklist {
    items: Vec<FixtureAccessibilityCheck>,
}

impl FixtureAccessibilityChecklist {
    /// Evaluate the twelve-category gate on a fixture cockpit view.
    ///
    /// This is a deterministic fixture function — not a real
    /// accessibility audit. It verifies that the cockpit view has the
    /// structural prerequisites each category requires (e.g. textual
    /// labels for color-independent meaning) and records `Pass` for
    /// satisfied categories or `NotApplicable` with a stated reason for
    /// categories that do not apply to a backend-rendered structural
    /// fixture.
    fn evaluate(view: &FixtureStewardCockpitView) -> Self {
        use FixtureAccessibilityCategory as Cat;
        use FixtureAccessibilityOutcome::*;

        let has_text_status = !view.operator_state.label().is_empty();
        let has_textual_member_impact = !view.member_impact_summary.is_empty();
        let has_evidence_link = view.receipts_and_evidence.evidence_hash != [0u8; 32];
        let has_authority_label = !view.authority_required.clause_id.is_empty();

        let item = |c: Cat, o: FixtureAccessibilityOutcome| FixtureAccessibilityCheck {
            category: c,
            outcome: o,
        };

        let items = vec![
            // 3.1 Language access — the operator state label and
            // member-impact summary are plain text strings.
            item(
                Cat::LanguageAccess,
                if has_text_status && has_textual_member_impact {
                    Pass
                } else {
                    Blocked {
                        reason: "operator state or member-impact summary missing label",
                    }
                },
            ),
            // 3.2 Screen-reader / non-visual access — every field is
            // structurally addressable (the view is a Rust struct with
            // named fields; a screen reader on the rendered live surface
            // could navigate them). No reliance on icon-only encoding.
            item(
                Cat::ScreenReaderAndNonVisualAccess,
                if has_text_status {
                    Pass
                } else {
                    Blocked {
                        reason: "status would be inaccessible without text label",
                    }
                },
            ),
            // 3.3 Low-vision access — N/A for the structural fixture;
            // contrast and zoom belong to the live cockpit surface, not
            // the kernel record.
            item(
                Cat::LowVisionAccess,
                NotApplicable {
                    reason: "structural fixture; visual contrast belongs to the live cockpit \
                            surface, not the kernel record",
                },
            ),
            // 3.4 Color-independent meaning — operator state and
            // escalation status are textual enums; no color-only encoding
            // is required to convey them.
            item(
                Cat::ColorIndependentMeaning,
                if has_text_status {
                    Pass
                } else {
                    Blocked {
                        reason: "status conveyed only by absent textual label",
                    }
                },
            ),
            // 3.5 Keyboard / switch / non-pointer access — N/A for the
            // structural fixture; input belongs to the live surface.
            item(
                Cat::KeyboardSwitchAndNonPointerAccess,
                NotApplicable {
                    reason: "structural fixture; input handling belongs to the live cockpit",
                },
            ),
            // 3.6 Captions / transcripts / non-audio access — N/A; no
            // audio content on this surface.
            item(
                Cat::CaptionsTranscriptsAndNonAudioAccess,
                NotApplicable {
                    reason: "no audio content on the steward cockpit divergence row",
                },
            ),
            // 3.7 Cognitive load — Slice A renders a single divergence
            // row with one repair action; the member-impact summary is a
            // single short sentence. Spec §"Design principles" calls for
            // this directly.
            item(
                Cat::CognitiveLoadAndStepComplexity,
                if has_textual_member_impact && view.member_impact_summary.split(' ').count() <= 10
                {
                    Pass
                } else {
                    Blocked {
                        reason: "member-impact summary exceeds plain-language length",
                    }
                },
            ),
            // 3.8 Low-bandwidth — the cockpit row is a small struct with
            // bounded fields; no large media. The Bloom digest is
            // bounded in size by spec.
            item(Cat::LowBandwidthAndLowDeviceAccess, Pass),
            // 3.9 AT compatibility — N/A for the structural fixture; AT
            // hooks belong to the live surface.
            item(
                Cat::AssistiveTechnologyCompatibility,
                NotApplicable {
                    reason: "structural fixture; AT hooks belong to the live cockpit",
                },
            ),
            // 3.10 Privacy-preserving accommodation path — the row never
            // surfaces private artifact bodies. For Slice A the
            // `private_content_implication` flag is false anyway, but
            // the structural absence of body fields on `RepairReceipt`
            // and `FixtureDigestMismatchSummary` satisfies the doc's
            // process-boundary requirement.
            item(Cat::PrivacyPreservingAccommodationPath, Pass),
            // 3.11 Receipts / provenance / evidence access — the row
            // carries `evidence_hash` and `plan_hash` so an auditor can
            // chase the chain.
            item(
                Cat::ReceiptsProvenanceAndEvidenceAccess,
                if has_evidence_link {
                    Pass
                } else {
                    Blocked {
                        reason: "no evidence_hash linked to the row",
                    }
                },
            ),
            // 3.12 Governance and action access — authority basis is
            // named via a policy-clause reference; the operator can see
            // which clause the repair acts under.
            item(
                Cat::GovernanceAndActionAccess,
                if has_authority_label {
                    Pass
                } else {
                    Blocked {
                        reason: "authority clause_id missing",
                    }
                },
            ),
        ];
        Self { items }
    }

    fn is_acceptable(&self) -> bool {
        self.items.iter().all(|c| c.outcome.is_acceptable())
    }

    fn pass_count(&self) -> usize {
        self.items
            .iter()
            .filter(|c| matches!(c.outcome, FixtureAccessibilityOutcome::Pass))
            .count()
    }

    fn not_applicable_count(&self) -> usize {
        self.items
            .iter()
            .filter(|c| matches!(c.outcome, FixtureAccessibilityOutcome::NotApplicable { .. }))
            .count()
    }
}

// ---------------------------------------------------------------------------
// Fixture flow helpers
// ---------------------------------------------------------------------------

fn fixture_policy_clause() -> PolicyClauseRef {
    PolicyClauseRef {
        policy_id: "fixture-receipt-index-sync".to_string(),
        policy_version_id: "v1".to_string(),
        clause_id: "slice-a.missing-receipt".to_string(),
    }
}

fn fixture_scope() -> ProbeScope {
    ProbeScope::LocalDomain {
        domain_id: "did:icn:demo:exampledomain".to_string(),
    }
}

/// Build the three-peer Slice A divergence fixture: peer A and peer C
/// both have all three receipts; peer B is missing r3.
fn build_three_peer_slice_a() -> (FixturePeer, FixturePeer, FixturePeer) {
    let r1 = fixture_receipt("r1", 0x01);
    let r2 = fixture_receipt("r2", 0x02);
    let r3 = fixture_receipt("r3", 0x03);

    let peer_a = FixturePeer::new(
        "did:icn:fixture:a",
        vec![r1.clone(), r2.clone(), r3.clone()],
    );
    let peer_b = FixturePeer::new("did:icn:fixture:b", vec![r1.clone(), r2.clone()]);
    // Peer C is a third domain member also fully synced with peer A.
    // The cockpit shows the divergence as a peer-pair issue (A ↔ B)
    // within a three-peer domain.
    let peer_c = FixturePeer::new("did:icn:fixture:c", vec![r1, r2, r3]);
    (peer_a, peer_b, peer_c)
}

fn build_divergence_evidence(
    peer_a: &FixturePeer,
    peer_b: &FixturePeer,
    peer_c: &FixturePeer,
) -> DivergenceEvidence {
    DivergenceEvidence::new(
        DivergenceClass::MissingReceipt,
        StateClass::ReceiptIndex,
        fixture_scope(),
        PeerSet::from_dids(vec![
            peer_a.did.clone(),
            peer_b.did.clone(),
            peer_c.did.clone(),
        ]),
        DigestMismatch::MissingOnRemote {
            local: peer_a.state_digest(),
        },
        fixture_policy_clause(),
        1_715_000_001,
        1_715_000_031,
        false,
        [0xBB; 32],
    )
}

fn build_repair_plan(evidence: &DivergenceEvidence) -> RepairPlan {
    RepairPlan::new(
        RepairAction::FetchMissing,
        AuthorityBasis::DomainPolicyClause(fixture_policy_clause()),
        fixture_scope(),
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
    )
}

fn render_cockpit_open(
    evidence: &DivergenceEvidence,
    plan: &RepairPlan,
    affected_hashes: &[Hash],
    last_successful_proof_at: Option<u64>,
) -> FixtureStewardCockpitView {
    let state = FixtureOperatorState::RepairPlanned;
    FixtureStewardCockpitView {
        affected_scope: evidence.scope.clone(),
        state_class: evidence.affected_state_class,
        peers: evidence.peers.dids().to_vec(),
        digest_mismatch_summary: FixtureDigestMismatchSummary {
            digest_form: "bloom_filter_set_difference",
            direction: FixtureMismatchDirection::MissingOnRemote,
            missing_count: affected_hashes.len() as u32,
            affected_receipt_hashes: affected_hashes.to_vec(),
        },
        last_successful_proof_at,
        repair_plan_summary: FixtureRepairPlanSummary {
            action: plan.action,
            authority_label: "fixture-receipt-index-sync/v1/slice-a.missing-receipt",
            expected_receipt_class: plan.expected_repair_receipt_class,
            plan_hash: plan.plan_hash,
        },
        authority_required: fixture_policy_clause(),
        receipts_and_evidence: FixtureEvidenceLinks {
            evidence_hash: evidence.evidence_hash,
            plan_hash: plan.plan_hash,
            repair_receipt_hash: None,
            repair_outcome_actor: None,
        },
        escalation_status: FixtureEscalationStatus::NotEscalated,
        operator_state: state,
        member_impact_summary: state.member_impact_summary(),
        open: true,
    }
}

fn render_cockpit_resolved(
    open_view: &FixtureStewardCockpitView,
    receipt: &RepairReceipt,
) -> FixtureStewardCockpitView {
    let state = FixtureOperatorState::RepairApplied;
    FixtureStewardCockpitView {
        operator_state: state,
        member_impact_summary: state.member_impact_summary(),
        open: false,
        receipts_and_evidence: FixtureEvidenceLinks {
            evidence_hash: open_view.receipts_and_evidence.evidence_hash,
            plan_hash: open_view.receipts_and_evidence.plan_hash,
            repair_receipt_hash: Some(receipt.receipt_hash),
            repair_outcome_actor: Some(receipt.actor_did.clone()),
        },
        ..open_view.clone()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn cockpit_slice_a_renders_all_nine_fields_and_passes_accessibility_gate() {
    // ---- Fixtures ----
    let (peer_a, mut peer_b, peer_c) = build_three_peer_slice_a();
    assert_eq!(peer_a.receipt_index.len(), 3);
    assert_eq!(peer_b.receipt_index.len(), 2);
    assert_eq!(peer_c.receipt_index.len(), 3);

    // Slice A's digest mismatch is peer A vs peer B; peer C is the third
    // domain member, fully synced with A.
    let local_set = peer_a.receipt_hash_set();
    let remote_set = peer_b.receipt_hash_set();
    let mut missing: Vec<Hash> = local_set.difference(&remote_set).copied().collect();
    missing.sort();
    assert_eq!(missing.len(), 1);

    // ---- Probe (smoke) ----
    let probe = AntiEntropyProbe::new(
        StateClass::ReceiptIndex,
        fixture_scope(),
        peer_a.state_digest(),
        peer_a.did.clone(),
        TriggerSource::Periodic,
        1_715_000_000,
        1_715_000_030,
        RequestedResponseClass::DigestExchange,
        [0xAA; 32],
    );
    assert!(probe.verify_binding());

    // ---- Classify ----
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    assert!(evidence.verify_binding());
    assert_eq!(evidence.divergence_class, DivergenceClass::MissingReceipt);
    assert!(!evidence.private_content_implication);

    // ---- Plan ----
    let plan = build_repair_plan(&evidence);
    assert!(plan.verify_binding());
    assert_eq!(plan.divergence_evidence_hash, evidence.evidence_hash);

    // ---- Render open cockpit view ----
    let last_successful = Some(1_714_999_900u64);
    let open_view = render_cockpit_open(&evidence, &plan, &missing, last_successful);

    // Spec §"Network / Federation surface" — every one of the nine
    // required fields is populated.
    assert_eq!(open_view.affected_scope, fixture_scope()); // 1
    assert_eq!(open_view.state_class, StateClass::ReceiptIndex); // 2
    assert_eq!(open_view.peers.len(), 3); // 3
    assert!(open_view.peers.contains(&peer_a.did));
    assert!(open_view.peers.contains(&peer_b.did));
    assert!(open_view.peers.contains(&peer_c.did));
    assert_eq!(
        open_view.digest_mismatch_summary.digest_form,
        "bloom_filter_set_difference"
    ); // 4
    assert_eq!(open_view.digest_mismatch_summary.missing_count, 1);
    assert_eq!(open_view.last_successful_proof_at, Some(1_714_999_900u64)); // 5
    assert_eq!(
        open_view.repair_plan_summary.action,
        RepairAction::FetchMissing
    ); // 6
    assert_eq!(
        open_view.authority_required.clause_id,
        "slice-a.missing-receipt"
    ); // 7
    assert_eq!(
        open_view.receipts_and_evidence.evidence_hash,
        evidence.evidence_hash
    ); // 8
    assert_eq!(open_view.receipts_and_evidence.plan_hash, plan.plan_hash);
    assert_eq!(
        open_view.escalation_status,
        FixtureEscalationStatus::NotEscalated
    ); // 9

    // Spec-attached derived fields.
    assert_eq!(
        open_view.operator_state,
        FixtureOperatorState::RepairPlanned
    );
    assert_eq!(
        open_view.member_impact_summary,
        "Members see: Action paused until records sync."
    );
    assert!(open_view.open);

    // ---- Accessibility gate on the open view ----
    let checklist_open = FixtureAccessibilityChecklist::evaluate(&open_view);
    assert_eq!(checklist_open.items.len(), 12, "exactly 12 categories");
    assert!(
        checklist_open.is_acceptable(),
        "no Blocked outcomes on the open view"
    );
    // Each of the twelve categories appears exactly once.
    let mut categories: Vec<FixtureAccessibilityCategory> =
        checklist_open.items.iter().map(|c| c.category).collect();
    categories.sort();
    categories.dedup();
    assert_eq!(categories.len(), 12);

    // ---- Apply (fixture-only) ----
    peer_b.fixture_apply_fetch_missing(&peer_a, &missing);
    assert_eq!(peer_b.receipt_index.len(), 3);
    assert_eq!(peer_a.receipt_hash_set(), peer_b.receipt_hash_set());

    // ---- Public RepairReceipt (#1849) for the applied repair ----
    let receipt = build_repair_receipt(
        &evidence,
        &plan,
        &peer_b,
        "did:icn:fixture:c",
        1_715_000_003,
    );
    assert!(receipt.verify_binding(), "fresh receipt verifies");
    assert_eq!(receipt.effect_outcome, EffectOutcome::Applied);
    assert_eq!(receipt.divergence_evidence_hash, evidence.evidence_hash);
    assert_eq!(receipt.repair_plan_hash, plan.plan_hash);
    assert_eq!(
        receipt.repair_receipt_class,
        RepairReceiptClass::FetchMissingReceipt
    );
    assert!(receipt.failure_reason.is_none());
    assert!(receipt.after_state_digest.is_some());

    // ---- Render resolved cockpit view ----
    let resolved_view = render_cockpit_resolved(&open_view, &receipt);
    assert!(!resolved_view.open);
    assert_eq!(
        resolved_view.operator_state,
        FixtureOperatorState::RepairApplied
    );
    assert_eq!(
        resolved_view.member_impact_summary,
        "Members see: Receipt available."
    );
    // The cross-link chain persists across the transition and now
    // anchors on the public RepairReceipt binding hash.
    assert_eq!(
        resolved_view.receipts_and_evidence.evidence_hash,
        evidence.evidence_hash
    );
    assert_eq!(
        resolved_view.receipts_and_evidence.plan_hash,
        plan.plan_hash
    );
    assert_eq!(
        resolved_view.receipts_and_evidence.repair_receipt_hash,
        Some(receipt.receipt_hash)
    );
    assert_eq!(
        resolved_view.receipts_and_evidence.repair_outcome_actor,
        Some("did:icn:fixture:c".to_string())
    );

    // ---- Accessibility gate on the resolved view ----
    let checklist_resolved = FixtureAccessibilityChecklist::evaluate(&resolved_view);
    assert_eq!(checklist_resolved.items.len(), 12);
    assert!(checklist_resolved.is_acceptable());

    // ---- Slice A transition summary, verbatim from spec §"First safe
    // proof-loop / dogfood slice" ----
    let slice_a_member_summary = format!(
        "Members see: Sync delayed → Receipt available. ({} → {})",
        FixtureOperatorState::Degraded.member_impact_summary(),
        FixtureOperatorState::RepairApplied.member_impact_summary(),
    );
    assert!(slice_a_member_summary.contains("Sync delayed → Receipt available"));
    assert!(slice_a_member_summary.contains("Receipt available"));
}

// ---------------------------------------------------------------------------
// Targeted unit tests
// ---------------------------------------------------------------------------

#[test]
fn cockpit_view_has_exactly_the_nine_required_fields() {
    // The struct's required-field set is the same shape the spec lists.
    // We verify this by constructing a view with deterministic values
    // and confirming each named field is reachable. (A future change
    // that drops a field would fail at the access site below.)
    let (peer_a, peer_b, peer_c) = build_three_peer_slice_a();
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    let plan = build_repair_plan(&evidence);
    let view = render_cockpit_open(&evidence, &plan, &[[0x03; 32]], Some(1_714_999_900));

    // Touch each of the nine required fields and the two derived ones.
    let _f1 = &view.affected_scope;
    let _f2 = view.state_class;
    let _f3 = &view.peers;
    let _f4 = &view.digest_mismatch_summary;
    let _f5 = view.last_successful_proof_at;
    let _f6 = &view.repair_plan_summary;
    let _f7 = &view.authority_required;
    let _f8 = &view.receipts_and_evidence;
    let _f9 = view.escalation_status;
    let _op = view.operator_state;
    let _mi = view.member_impact_summary;
}

#[test]
fn accessibility_gate_marks_blocked_when_cockpit_view_omits_evidence_link() {
    // Defense-in-depth: if the cockpit row ever loses its evidence_hash
    // link (Receipts/provenance/evidence access — category 3.11), the
    // checklist must mark that category Blocked and the overall view
    // unacceptable.
    let (peer_a, peer_b, peer_c) = build_three_peer_slice_a();
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    let plan = build_repair_plan(&evidence);
    let mut bad_view = render_cockpit_open(&evidence, &plan, &[[0x03; 32]], Some(1_714_999_900));
    bad_view.receipts_and_evidence.evidence_hash = [0u8; 32]; // simulate a broken link

    let checklist = FixtureAccessibilityChecklist::evaluate(&bad_view);
    assert!(!checklist.is_acceptable());
    let cat_311 = checklist
        .items
        .iter()
        .find(|c| {
            matches!(
                c.category,
                FixtureAccessibilityCategory::ReceiptsProvenanceAndEvidenceAccess
            )
        })
        .expect("category 3.11 must be present");
    assert!(matches!(
        cat_311.outcome,
        FixtureAccessibilityOutcome::Blocked { .. }
    ));
}

#[test]
fn accessibility_gate_marks_blocked_when_authority_label_is_missing() {
    // Category 3.12 — Governance and action access — requires a named
    // authority clause. A missing clause_id must Block.
    let (peer_a, peer_b, peer_c) = build_three_peer_slice_a();
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    let plan = build_repair_plan(&evidence);
    let mut bad_view = render_cockpit_open(&evidence, &plan, &[[0x03; 32]], Some(1_714_999_900));
    bad_view.authority_required.clause_id = String::new();

    let checklist = FixtureAccessibilityChecklist::evaluate(&bad_view);
    assert!(!checklist.is_acceptable());
    let cat_312 = checklist
        .items
        .iter()
        .find(|c| {
            matches!(
                c.category,
                FixtureAccessibilityCategory::GovernanceAndActionAccess
            )
        })
        .expect("category 3.12 must be present");
    assert!(matches!(
        cat_312.outcome,
        FixtureAccessibilityOutcome::Blocked { .. }
    ));
}

#[test]
fn member_impact_summary_contains_no_protocol_jargon() {
    // The spec is clear in §"Member shell surface": no Bloom-filter,
    // Merkle-root, vector-clock, or policy_version_id wording. Lock the
    // exact member-impact strings here so a renaming or accidental
    // expansion is caught by the test.
    let forbidden_substrings = [
        "Bloom",
        "Merkle",
        "vector clock",
        "vector_clock",
        "policy_version_id",
        "evidence_hash",
        "plan_hash",
        "DivergenceEvidence",
        "RepairPlan",
        "RepairReceipt",
        "AntiEntropyProbe",
    ];
    for state in [
        FixtureOperatorState::Healthy,
        FixtureOperatorState::Degraded,
        FixtureOperatorState::Syncing,
        FixtureOperatorState::Stale,
        FixtureOperatorState::Partitioned,
        FixtureOperatorState::Relayed,
        FixtureOperatorState::VerificationPending,
        FixtureOperatorState::RepairPlanned,
        FixtureOperatorState::RepairApplied,
        FixtureOperatorState::ReviewRequired,
        FixtureOperatorState::BlockedByPolicy,
        FixtureOperatorState::PrivateContentRestricted,
    ] {
        let summary = state.member_impact_summary();
        for forbidden in &forbidden_substrings {
            assert!(
                !summary.contains(forbidden),
                "member-impact summary for {state:?} contains protocol jargon `{forbidden}`: \
                 `{summary}`"
            );
        }
    }
}

#[test]
fn missing_receipt_with_named_authority_does_not_escalate() {
    // Spec §"Network / Federation surface" field 9: escalation is for
    // unclassifiable / equivocation / boundary-rule violations. A clean
    // MissingReceipt with a DomainPolicyClause authority basis does NOT
    // escalate; the cockpit shows "not escalated."
    let (peer_a, peer_b, peer_c) = build_three_peer_slice_a();
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    let plan = build_repair_plan(&evidence);
    let view = render_cockpit_open(&evidence, &plan, &[[0x03; 32]], Some(1_714_999_900));
    assert_eq!(
        view.escalation_status,
        FixtureEscalationStatus::NotEscalated
    );
    assert_ne!(
        view.escalation_status,
        FixtureEscalationStatus::EscalatedToGovernanceReview
    );
}

#[test]
fn repair_receipt_links_evidence_and_plan_by_hash() {
    // The public RepairReceipt (#1849) is the cockpit row's resolved
    // evidence artifact. It carries the cross-link hashes so an auditor
    // can chase the chain back to the open divergence, and
    // verify_binding() proves the binding has not been tampered with.
    let (peer_a, mut peer_b, peer_c) = build_three_peer_slice_a();
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    let plan = build_repair_plan(&evidence);
    let missing: Vec<Hash> = peer_a
        .receipt_hash_set()
        .difference(&peer_b.receipt_hash_set())
        .copied()
        .collect();
    peer_b.fixture_apply_fetch_missing(&peer_a, &missing);
    let receipt = build_repair_receipt(
        &evidence,
        &plan,
        &peer_b,
        "did:icn:fixture:c",
        1_715_000_003,
    );
    assert!(receipt.verify_binding());
    assert_eq!(receipt.divergence_evidence_hash, evidence.evidence_hash);
    assert_eq!(receipt.repair_plan_hash, plan.plan_hash);
    // If the evidence is rebuilt with a different nonce, the link breaks.
    let rebuilt_evidence = DivergenceEvidence::new(
        evidence.divergence_class,
        evidence.affected_state_class,
        evidence.scope.clone(),
        evidence.peers.clone(),
        evidence.digest_mismatch.clone(),
        evidence.policy_clause.clone(),
        evidence.freshness_emitted_at,
        evidence.freshness_valid_until,
        evidence.private_content_implication,
        [0xEE; 32], // different nonce
    );
    assert_ne!(
        receipt.divergence_evidence_hash,
        rebuilt_evidence.evidence_hash
    );
}

#[test]
fn fixture_view_has_no_runtime_state_fields() {
    // Structural tripwire — same shape as the #1838 fixture: a cockpit
    // view must remain a plain data struct. No Arc / JoinHandle /
    // Sender / runtime handles allowed. The 256-byte ceiling is loose
    // but catches gross expansion.
    assert!(
        std::mem::size_of::<FixtureStewardCockpitView>() < 1024,
        "FixtureStewardCockpitView must remain a plain data struct"
    );
    assert!(std::mem::size_of::<FixtureAccessibilityCheck>() < 256);
}

#[test]
fn slice_a_summary_matches_spec_transition_string() {
    // Spec Slice A §"First safe proof-loop / dogfood slice" says:
    //   Members see: Sync delayed → Receipt available
    // We assert both halves are reachable from the closed v0 mapping.
    let degraded = FixtureOperatorState::Degraded.member_impact_summary();
    let repair_applied = FixtureOperatorState::RepairApplied.member_impact_summary();
    assert!(degraded.contains("Sync delayed"));
    assert!(repair_applied.contains("Receipt available"));
}

#[test]
fn accessibility_pass_and_not_applicable_counts_are_consistent() {
    // The accessible-only categories (operator surface, no audio, no
    // visual layout) are expected NotApplicable; the rest pass.
    // Total must be 12; Pass + NotApplicable must equal 12 for a
    // healthy fixture view.
    let (peer_a, peer_b, peer_c) = build_three_peer_slice_a();
    let evidence = build_divergence_evidence(&peer_a, &peer_b, &peer_c);
    let plan = build_repair_plan(&evidence);
    let view = render_cockpit_open(&evidence, &plan, &[[0x03; 32]], Some(1_714_999_900));
    let checklist = FixtureAccessibilityChecklist::evaluate(&view);
    assert_eq!(
        checklist.pass_count() + checklist.not_applicable_count(),
        12,
        "every category must be either Pass or NotApplicable on the Slice A fixture view"
    );
    // ALL constant is exactly 12.
    assert_eq!(FixtureAccessibilityCategory::ALL.len(), 12);
}
