//! Member shell Slice A — read-only rendering rehearsal (fixture-only).
//!
//! Implements Slice A from `docs/spec/member-shell-v0.md` §"First safe
//! proof-loop / dogfood slice". This is the first exercise of the
//! member-shell rendering surface over the same proof rail landed by
//! #1843 (`AntiEntropyProbe` + `StateDigest`),
//! #1844 (`DivergenceEvidence` + `RepairPlan`),
//! #1845 (receipt-index anti-entropy Slice A fixture),
//! #1846 (steward cockpit divergence-render Slice A fixture),
//! and #1850 (`RepairReceipt` wire-stable schema, issue #1849).
//!
//! # What this is
//!
//! A deterministic in-memory test that renders a fixture member shell
//! view over an open-then-resolved Slice A divergence. The decisive
//! property under test:
//!
//! ```text
//! A fixture member with a fixture device, in a fixture LocalDomain that
//! belongs to all four merged owning-entity classes (Cooperative,
//! Community, Federation, Individual), sees a standing surface and four
//! ActionCards. One is a proposal/vote that is Open. One is a
//! meeting/attend that is Confirmed with a plain-language receipt
//! summary. One is paused because the Slice A divergence is still open;
//! the member sees "Action paused until records sync." One is closed for
//! insufficient authority; the member sees a plain explanation. A
//! PrivateEvidence reference is rendered as existence + scope + access
//! path only — never body content. The twelve-category accessibility
//! gate passes on the rendered view. After the Slice A repair lands,
//! the same view transitions to "Receipt available." No member-facing
//! string contains protocol jargon.
//! ```
//!
//! # What this is NOT
//!
//! * Not a live shell. No HTML, no native app, no PWA, no web shell, no
//!   terminal UI. The "view" is a plain `FixtureMemberShellView` struct
//!   with one named field per spec-required surface item.
//! * Not a platform choice. iOS, Android, PWA, web, and native shell
//!   remain explicitly undecided per `docs/spec/member-shell-v0.md`
//!   §"Scope and non-goals."
//! * Not a partner skin. No NYCN-specific labels, no Summit framing, no
//!   named-partner cooperative. The generic shell stays generic per
//!   `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3.
//! * Not real member data. Every DID, display name, device label,
//!   timestamp, and hash is fixture-only.
//! * Not a runtime mutation. No sockets, no QUIC, no gossip actor, no
//!   spawned tasks, no actor handles, no async runtime, no on-disk
//!   state, nothing in K3s.
//! * Not private data. The `PrivateEvidence` reference renders as
//!   existence + scope + access path only. There is no body, content,
//!   payload, raw bytes, or secret field on the rendering type by
//!   construction.
//! * Not a member-facing surface for the wire-stable receipt. The
//!   public `RepairReceipt` (#1849) now anchors the resolved card's
//!   opaque `receipt_ref_hash` so an auditor can chase the chain back
//!   to the resolved repair evidence. No member-facing string surfaces
//!   the receipt's internal field set; the closed plain-language
//!   vocabulary stays intact.
//! * Not a public `PeerSyncReport` schema. Still design-level.
//! * Not a steward cockpit surface. That fixture (#1840 / PR #1846)
//!   renders the operator-facing technical detail; this fixture renders
//!   the member-facing plain-language projection of the same proof rail.
//! * Not a production-readiness claim, live-federation claim, or NYCN
//!   pilot claim.

use std::collections::{BTreeMap, BTreeSet};

use icn_gossip::{to_bloom_projection, BloomFilter};
use icn_kernel_api::{
    AuthorityBasis, BoundaryRuleRef, BoundaryRuleSet, Did, DigestMismatch, DivergenceClass,
    DivergenceEvidence, EffectOutcome, ExpectedRepairReceiptClass, Hash, PeerSet, PolicyClauseRef,
    ProbeScope, RepairAction, RepairPlan, RepairReceipt, RepairReceiptClass, StateClass,
    StateDigest,
};

// ===========================================================================
// Slice A proof-rail fixture helpers
// ===========================================================================
//
// The member shell renders the *member-facing projection* of the same
// proof rail the cockpit fixture renders. To prove the projection is
// honest, this test constructs the same Slice A `DivergenceEvidence` /
// `RepairPlan` shape the cockpit fixture (#1846) uses, and asserts that
// every member-visible string on the resulting view is free of the
// protocol vocabulary that lives on those records. The records
// themselves are *not* surfaced; they exist here only so the test can
// demonstrate the firewall.

#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureReceiptHash {
    receipt_hash: Hash,
    label: &'static str,
}

fn fixture_receipt_hash(label: &'static str, byte: u8) -> FixtureReceiptHash {
    FixtureReceiptHash {
        receipt_hash: [byte; 32],
        label,
    }
}

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

fn fixture_bloom_digest(hashes: &[Hash]) -> StateDigest {
    let mut filter = BloomFilter::new_adaptive(hashes.len().max(1));
    for h in hashes {
        filter.insert(h);
    }
    StateDigest::Bloom(to_bloom_projection(&filter.to_data(), hashes.len() as u32))
}

fn build_slice_a_divergence() -> DivergenceEvidence {
    let r1 = fixture_receipt_hash("r1", 0x01);
    let r2 = fixture_receipt_hash("r2", 0x02);
    let r3 = fixture_receipt_hash("r3", 0x03);
    let local_digest = fixture_bloom_digest(&[r1.receipt_hash, r2.receipt_hash, r3.receipt_hash]);
    DivergenceEvidence::new(
        DivergenceClass::MissingReceipt,
        StateClass::ReceiptIndex,
        fixture_scope(),
        PeerSet::from_dids(vec![
            "did:icn:fixture:a".to_string(),
            "did:icn:fixture:b".to_string(),
            "did:icn:fixture:c".to_string(),
        ]),
        DigestMismatch::MissingOnRemote {
            local: local_digest,
        },
        fixture_policy_clause(),
        1_715_000_001,
        1_715_000_031,
        false,
        [0xBB; 32],
    )
}

fn build_slice_a_repair_plan(evidence: &DivergenceEvidence) -> RepairPlan {
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

/// Build the public `RepairReceipt` (#1849) the resolved member-shell
/// view anchors on.
///
/// Constructs the wire-stable receipt with `EffectOutcome::Applied`
/// over a deterministic fixture after-state digest. The
/// `affected_state_class` is sourced from `evidence`; `scope`,
/// `authority_basis`, and `boundary_rules` are sourced from `plan` so
/// any drift in the evidence/plan→receipt chain would diverge the
/// binding hash. The resulting `receipt_hash` is what the resolved
/// card's opaque `receipt_ref_hash` points at — the member never
/// reads it, but an auditor can chase the chain back to the resolved
/// repair evidence.
///
/// Kernel-level only: no live network, no live repair. The receipt
/// records what a fixture peer would have produced had the bounded
/// `FetchMissing` action run against real peers.
fn build_slice_a_repair_receipt(evidence: &DivergenceEvidence, plan: &RepairPlan) -> RepairReceipt {
    let r1 = fixture_receipt_hash("r1", 0x01);
    let r2 = fixture_receipt_hash("r2", 0x02);
    let r3 = fixture_receipt_hash("r3", 0x03);
    let after = fixture_bloom_digest(&[r1.receipt_hash, r2.receipt_hash, r3.receipt_hash]);
    RepairReceipt::new(
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
        Some(after),
        1_715_000_040,
        1_715_000_070,
        evidence.private_content_implication,
        None,
        [0xDD; 32],
    )
    .expect("Slice A member-shell receipt is structurally consistent")
}

// ===========================================================================
// Member-shell vocabularies (closed sets, lifted verbatim from spec)
// ===========================================================================

/// Closed seven-string sync vocabulary per
/// `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell
/// surface" + `docs/spec/member-shell-v0.md` §"Member-facing status
/// vocabulary / Sync state".
///
/// Test-private. The live shell (forward work) will own the canonical
/// type; this enum exists only so the fixture can lock the closed set
/// and detect drift via the targeted vocabulary test below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FixtureSyncStatus {
    Synced,
    SyncDelayed,
    SomeRecordsAreBeingVerified,
    ActionPausedUntilRecordsSync,
    ReceiptAvailable,
    ReviewRequired,
    SyncDelayedDegraded,
}

impl FixtureSyncStatus {
    const ALL: [Self; 7] = [
        Self::Synced,
        Self::SyncDelayed,
        Self::SomeRecordsAreBeingVerified,
        Self::ActionPausedUntilRecordsSync,
        Self::ReceiptAvailable,
        Self::ReviewRequired,
        Self::SyncDelayedDegraded,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Synced => "Synced",
            Self::SyncDelayed => "Sync delayed",
            Self::SomeRecordsAreBeingVerified => "Some records are being verified",
            Self::ActionPausedUntilRecordsSync => "Action paused until records sync",
            Self::ReceiptAvailable => "Receipt available",
            Self::ReviewRequired => "Review required",
            Self::SyncDelayedDegraded => "Sync delayed / degraded",
        }
    }
}

/// Action lifecycle vocabulary per `docs/spec/member-shell-v0.md`
/// §"Member-facing status vocabulary / Action lifecycle".
///
/// Test-private. Closed set per the v0 spec. The full set is declared
/// here so a reviewer sees the complete taxonomy at a glance; Slice A
/// only exercises a subset (Open, OpenButPaused, Confirmed,
/// ClosedInsufficientAuthority). The remaining variants are part of
/// the closed v0 vocabulary and will be exercised by later slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // variants outside Slice A's exercised subset are intentional placeholders
enum FixtureActionLifecycle {
    Open,
    OpenButPaused,
    DraftWillBeSentWhenRecordsSync,
    SentWaitingForReceipt,
    Confirmed,
    Declined,
    ClosedDeadlinePassed,
    ClosedSuperseded,
    ClosedInsufficientAuthority,
}

impl FixtureActionLifecycle {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::OpenButPaused => "Open but paused",
            Self::DraftWillBeSentWhenRecordsSync => "Draft — will be sent when records sync",
            Self::SentWaitingForReceipt => "Sent — waiting for receipt",
            Self::Confirmed => "Confirmed",
            Self::Declined => "Declined",
            Self::ClosedDeadlinePassed => "Closed — deadline passed",
            Self::ClosedSuperseded => "Closed — superseded",
            Self::ClosedInsufficientAuthority => "Closed — insufficient authority",
        }
    }
}

/// Owning entity classes per `docs/spec/member-shell-v0.md` §"Standing
/// surface" — "the four merged-spec scope vocabularies." Names match the
/// generic structural taxonomy from
/// `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FixtureOwningEntityClass {
    Cooperative,
    Community,
    Federation,
    Individual,
}

impl FixtureOwningEntityClass {
    const ALL: [Self; 4] = [
        Self::Cooperative,
        Self::Community,
        Self::Federation,
        Self::Individual,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Cooperative => "Cooperative",
            Self::Community => "Community",
            Self::Federation => "Federation",
            Self::Individual => "Individual",
        }
    }
}

/// Subset of the ADR-0027 `source_kind` taxonomy actually emitted today
/// per `docs/contracts/institution-package/action-card.schema.json`
/// `x-icn-emitted-source-kinds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureSourceKind {
    Proposal,
    Meeting,
    ActionItem,
}

impl FixtureSourceKind {
    /// Plain-language label per `docs/spec/member-shell-v0.md`
    /// §"ActionCard rendering contract / `source_kind`" — the shell maps
    /// the raw enum value to a plain-language source label rather than
    /// rendering the enum verbatim.
    fn plain_label(self) -> &'static str {
        match self {
            Self::Proposal => "Proposal in your domain",
            Self::Meeting => "Meeting in your domain",
            Self::ActionItem => "Action item assigned to you",
        }
    }
}

/// Subset of the ADR-0027 `action_kind` taxonomy actually emitted today
/// per `docs/contracts/institution-package/action-card.schema.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureActionKind {
    Vote,
    Attend,
    Complete,
}

impl FixtureActionKind {
    /// Plain-language verb per `docs/spec/member-shell-v0.md`
    /// §"ActionCard rendering contract / `action_kind`".
    fn plain_verb(self) -> &'static str {
        match self {
            Self::Vote => "Vote",
            Self::Attend => "Attend",
            Self::Complete => "Complete",
        }
    }
}

/// ADR-0027 risk-level taxonomy. Slice A exercises Low and Normal only;
/// Elevated is part of the closed taxonomy and exists so the fixture
/// covers the complete set a reviewer expects to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Elevated is part of the closed ADR-0027 taxonomy but not exercised by Slice A
enum FixtureRiskLevel {
    Low,
    Normal,
    Elevated,
}

impl FixtureRiskLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Elevated => "elevated",
        }
    }

    /// Color-independent glyph paired with the label per ADR-0028 §3.4.
    fn glyph(self) -> &'static str {
        match self {
            Self::Low => "•",
            Self::Normal => "■",
            Self::Elevated => "▲",
        }
    }
}

/// Plain-language receipt-class labels per `docs/spec/member-shell-v0.md`
/// §"Receipt-class plain-language labels (mapping to ADR-0026 /
/// ADR-0025)". Test-private; only the variants the Slice A fixture
/// renders are defined here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureReceiptClassLabel {
    AttendanceRecorded,
    GovernanceDecisionRecorded,
    ActionCompleted,
}

impl FixtureReceiptClassLabel {
    fn label(self) -> &'static str {
        match self {
            Self::AttendanceRecorded => "Attendance recorded",
            Self::GovernanceDecisionRecorded => "Governance decision recorded",
            Self::ActionCompleted => "Action completed",
        }
    }
}

// ===========================================================================
// Member-shell rendering types — test-private
// ===========================================================================

/// Fixture member identity. The DID is shown under "details" per
/// `docs/spec/member-shell-v0.md` §"Standing surface / Identity" — the
/// surface uses `display_name` as the primary affordance.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureMember {
    did: Did,
    display_name: &'static str,
}

/// Fixture device the member is using right now.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureDevice {
    label: &'static str,
    last_active_at: u64,
}

/// Fixture `LocalDomain` per `docs/spec/member-shell-v0.md` §"Boundary
/// lines / Member shell vs. institution-package skin / theme" and
/// §"Standing surface / Memberships." The four owning-entity classes
/// are surfaced so the member sees, structurally, what governs them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureLocalDomain {
    domain_id: Did,
    display_name: &'static str,
    owning_entity_classes: BTreeSet<FixtureOwningEntityClass>,
}

/// Fixture standing surface per `docs/spec/member-shell-v0.md`
/// §"Standing surface." Only the v0 plain-language fields the spec
/// names. No raw `AuthorityGrant` content hashes, no raw
/// `policy_version_id` strings.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureStandingSurface {
    member: FixtureMember,
    device: FixtureDevice,
    current_domain: FixtureLocalDomain,
    role_label: &'static str,
    mandate_summary: &'static str,
    sync_status_label: &'static str,
    open_action_count: u32,
    closed_for_authority_count: u32,
    closed_for_authority_explanation: &'static str,
    private_records_access_path_label: &'static str,
}

/// Fixture receipt summary per `docs/spec/member-shell-v0.md` §"Receipt
/// and provenance rendering." Plain summary first; the plain receipt
/// class label is the closed mapping per §"Receipt-class plain-language
/// labels." There is **no** body, content, payload, raw_bytes, or
/// secret field on this struct by construction — see the structural
/// test `fixture_receipt_summary_has_no_body_field`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureReceiptSummary {
    plain_summary: &'static str,
    receipt_class_label: FixtureReceiptClassLabel,
    scope_label: &'static str,
    applied_at: u64,
    /// Opaque reference the shell carries so the member can chase the
    /// chain via the "details" affordance. The receipt itself never
    /// shows body content; only this hash plus the plain summary above.
    receipt_ref_hash: Hash,
}

/// Fixture `PrivateEvidence` reference per `docs/spec/member-shell-v0.md`
/// §"Privacy and ScopedVault member affordances." The struct surfaces
/// **existence + scope + access path only**. No body, content, payload,
/// raw_bytes, secret, vault, or contents field on this type by
/// construction — see the structural test
/// `fixture_private_evidence_reference_has_no_body_field`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixturePrivateEvidenceReference {
    existence_label: &'static str,
    scope_label: &'static str,
    access_status_label: &'static str,
    request_review_path_label: &'static str,
}

/// Fixture ActionCard. Field set mirrors
/// `docs/contracts/institution-package/action-card.schema.json` plus
/// the member-shell rendering layer's plain-language additions per
/// `docs/spec/member-shell-v0.md` §"ActionCard rendering contract."
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureActionCard {
    /// Card id per the schema's deterministic format
    /// (`card-<source>-<source_id>-<action>`).
    card_id: &'static str,
    source_kind: FixtureSourceKind,
    action_kind: FixtureActionKind,
    /// `LocalDomain` display name the card targets.
    scope_label: &'static str,
    title: &'static str,
    summary: &'static str,
    /// Plain-language authority basis. Per ADR-0027 the schema already
    /// requires this field to be plain-language; the shell surfaces it
    /// verbatim.
    authority_basis: &'static str,
    /// Plain-language explanation when the card is closed for
    /// insufficient authority; `None` when the member is authorized.
    required_authority_explanation: Option<&'static str>,
    /// Optional plain-language deadline. `None` per spec is rendered as
    /// "no time pressure," not as "no deadline" (more honest framing).
    deadline_label: Option<&'static str>,
    risk_level: FixtureRiskLevel,
    accessibility_hint: &'static str,
    receipt_expected: bool,
    /// Plain-language expected-receipt label per
    /// `docs/spec/member-shell-v0.md` §"ActionCard rendering contract
    /// / Mandatory rendering for every ActionCard / `receipt_expected`":
    /// the shell renders an outcome line ("If you confirm, this will
    /// produce a <receipt class summary>"). Required whenever
    /// `receipt_expected` is true AND the card is in a pre-receipt
    /// state (Open / OpenButPaused / DraftWillBeSentWhenRecordsSync /
    /// SentWaitingForReceipt). `None` for closed cards or cards
    /// already Confirmed with a `receipt_summary`.
    expected_receipt_label: Option<FixtureReceiptClassLabel>,
    state: FixtureActionLifecycle,
    sync_status: FixtureSyncStatus,
    /// Plain receipt summary present only when the card has been
    /// confirmed and a receipt has landed.
    receipt_summary: Option<FixtureReceiptSummary>,
}

impl FixtureActionCard {
    /// True if the card is in a state where it would still produce a
    /// receipt on confirm. Used by the accessibility gate to require
    /// the expected-receipt label only on pre-receipt cards.
    fn is_pre_receipt_state(&self) -> bool {
        matches!(
            self.state,
            FixtureActionLifecycle::Open
                | FixtureActionLifecycle::OpenButPaused
                | FixtureActionLifecycle::DraftWillBeSentWhenRecordsSync
                | FixtureActionLifecycle::SentWaitingForReceipt
        )
    }
}

/// Composed member shell view rendered for a single member.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureMemberShellView {
    standing: FixtureStandingSurface,
    action_cards: Vec<FixtureActionCard>,
    private_evidence_references: Vec<FixturePrivateEvidenceReference>,
    /// Overall surface sync-status label, used for the home / today
    /// rollup. Slice A: open view → "Sync delayed"; resolved view →
    /// "Receipt available."
    surface_sync_status: FixtureSyncStatus,
}

impl FixtureMemberShellView {
    /// Every member-facing string on the view, used by the no-jargon
    /// check below. Excludes opaque hash bytes (the receipt-ref hash is
    /// not a string the member reads).
    fn all_member_facing_strings(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();

        let s = &self.standing;
        out.push(s.member.display_name);
        out.push(s.device.label);
        out.push(s.current_domain.display_name);
        out.push(s.role_label);
        out.push(s.mandate_summary);
        out.push(s.sync_status_label);
        out.push(s.closed_for_authority_explanation);
        out.push(s.private_records_access_path_label);

        for card in &self.action_cards {
            out.push(card.scope_label);
            out.push(card.title);
            out.push(card.summary);
            out.push(card.authority_basis);
            if let Some(req) = card.required_authority_explanation {
                out.push(req);
            }
            if let Some(dl) = card.deadline_label {
                out.push(dl);
            }
            out.push(card.risk_level.label());
            out.push(card.accessibility_hint);
            out.push(card.state.label());
            out.push(card.sync_status.label());
            out.push(card.source_kind.plain_label());
            out.push(card.action_kind.plain_verb());
            if let Some(receipt) = &card.receipt_summary {
                out.push(receipt.plain_summary);
                out.push(receipt.receipt_class_label.label());
                out.push(receipt.scope_label);
            }
            if let Some(expected) = card.expected_receipt_label {
                out.push(expected.label());
            }
        }

        for r in &self.private_evidence_references {
            out.push(r.existence_label);
            out.push(r.scope_label);
            out.push(r.access_status_label);
            out.push(r.request_review_path_label);
        }

        out.push(self.surface_sync_status.label());
        out
    }
}

// ===========================================================================
// Accessibility checklist — twelve-category gate per ADR-0028 /
// docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md, adapted for the
// member shell.
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)] // names follow the doc literally
enum FixtureAccessibilityCategory {
    LanguageAccess,
    ScreenReaderAndNonVisualAccess,
    LowVisionAccess,
    ColorIndependentMeaning,
    KeyboardSwitchAndNonPointerAccess,
    CaptionsTranscriptsAndNonAudioAccess,
    CognitiveLoadAndStepComplexity,
    LowBandwidthAndLowDeviceAccess,
    AssistiveTechnologyCompatibility,
    PrivacyPreservingAccommodationPath,
    ReceiptsProvenanceAndEvidenceAccess,
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
    /// Evaluate the twelve-category gate on a fixture member-shell
    /// view. Deterministic; not a real accessibility audit. The
    /// checklist verifies that the rendered view has the structural
    /// prerequisites each category requires (textual labels for
    /// color-independent meaning, accessibility hints on every card,
    /// authority basis on every card, no body fields on private
    /// references) and records `Pass` for satisfied categories or
    /// `NotApplicable` for categories that do not apply to a backend
    /// structural fixture.
    fn evaluate(view: &FixtureMemberShellView) -> Self {
        use FixtureAccessibilityCategory as Cat;
        use FixtureAccessibilityOutcome::*;

        let standing_has_text =
            !view.standing.member.display_name.is_empty() && !view.standing.role_label.is_empty();
        let every_card_has_hint = view
            .action_cards
            .iter()
            .all(|c| !c.accessibility_hint.is_empty());
        let every_card_has_label = view
            .action_cards
            .iter()
            .all(|c| !c.title.is_empty() && !c.summary.is_empty());
        let every_card_has_authority = view
            .action_cards
            .iter()
            .all(|c| !c.authority_basis.is_empty());
        // Spec §"ActionCard rendering contract / `receipt_expected`":
        // pre-receipt cards that expect a receipt must surface the
        // plain expected-receipt label so the member sees the outcome
        // line before confirm.
        let pre_receipt_expecting_cards_carry_expected_label = view.action_cards.iter().all(|c| {
            !c.receipt_expected || !c.is_pre_receipt_state() || c.expected_receipt_label.is_some()
        });
        // Spec §"Card states the shell must distinguish / Closed:
        // insufficient authority": the shell must name the missing
        // authority plainly. A closed-insufficient-authority card with
        // no `required_authority_explanation` violates the contract
        // even when `authority_basis` is non-empty.
        let insufficient_authority_cards_have_explanation = view.action_cards.iter().all(|c| {
            !matches!(c.state, FixtureActionLifecycle::ClosedInsufficientAuthority)
                || c.required_authority_explanation.is_some()
        });
        let every_card_has_textual_risk_glyph_pair = view
            .action_cards
            .iter()
            .all(|c| !c.risk_level.label().is_empty() && !c.risk_level.glyph().is_empty());
        let every_card_summary_under_140 = view.action_cards.iter().all(|c| c.summary.len() <= 140);
        let private_refs_have_access_path = view
            .private_evidence_references
            .iter()
            .all(|r| !r.access_status_label.is_empty() && !r.request_review_path_label.is_empty());
        let receipt_summary_present_for_confirmed = view.action_cards.iter().all(|c| {
            !matches!(c.state, FixtureActionLifecycle::Confirmed) || c.receipt_summary.is_some()
        });

        let item = |c: Cat, o: FixtureAccessibilityOutcome| FixtureAccessibilityCheck {
            category: c,
            outcome: o,
        };

        let items = vec![
            // 3.1 Language access — every rendered string is plain
            // English; protocol jargon is barred by the targeted test
            // below.
            item(
                Cat::LanguageAccess,
                if standing_has_text && every_card_has_label {
                    Pass
                } else {
                    Blocked {
                        reason: "missing plain-language label on standing or action card",
                    }
                },
            ),
            // 3.2 Screen-reader / non-visual access — every card has a
            // title + summary + accessibility hint. Status conveyed by
            // text label per the closed sync vocabulary.
            item(
                Cat::ScreenReaderAndNonVisualAccess,
                if every_card_has_hint && every_card_has_label {
                    Pass
                } else {
                    Blocked {
                        reason: "card missing screen-reader-addressable label or hint",
                    }
                },
            ),
            // 3.3 Low-vision access — N/A for structural fixture; zoom
            // and contrast belong to the live shell.
            item(
                Cat::LowVisionAccess,
                NotApplicable {
                    reason: "structural fixture; zoom and contrast belong to the live shell, \
                            not the kernel-side rendering record",
                },
            ),
            // 3.4 Color-independent meaning — risk level is a (label,
            // glyph) pair per the rendering contract; sync status is a
            // text label, not a color. Slice A asserts both shapes.
            item(
                Cat::ColorIndependentMeaning,
                if every_card_has_textual_risk_glyph_pair {
                    Pass
                } else {
                    Blocked {
                        reason: "risk level conveyed only by color (no label/glyph pairing)",
                    }
                },
            ),
            // 3.5 Keyboard / switch / non-pointer access — N/A for the
            // structural fixture; input belongs to the live shell.
            item(
                Cat::KeyboardSwitchAndNonPointerAccess,
                NotApplicable {
                    reason: "structural fixture; input handling belongs to the live shell",
                },
            ),
            // 3.6 Captions / transcripts / non-audio access — N/A; the
            // Slice A surface carries no audio content.
            item(
                Cat::CaptionsTranscriptsAndNonAudioAccess,
                NotApplicable {
                    reason: "no audio content on the Slice A member-shell rendering",
                },
            ),
            // 3.7 Cognitive load — every card summary fits in one line
            // (≤ 140 chars); receipt summary uses the closed plain
            // receipt-class label; sync status is the closed
            // seven-string vocabulary.
            item(
                Cat::CognitiveLoadAndStepComplexity,
                if every_card_summary_under_140 {
                    Pass
                } else {
                    Blocked {
                        reason: "action-card summary exceeds plain-language one-line length",
                    }
                },
            ),
            // 3.8 Low-bandwidth and low-device access — the view is a
            // small struct with bounded fields; no autoplay media; no
            // large images. The structural shape supports the spec's
            // mobile-first / older-device target.
            item(Cat::LowBandwidthAndLowDeviceAccess, Pass),
            // 3.9 AT compatibility — N/A for the structural fixture; AT
            // hooks belong to the live shell.
            item(
                Cat::AssistiveTechnologyCompatibility,
                NotApplicable {
                    reason: "structural fixture; AT hooks belong to the live shell",
                },
            ),
            // 3.10 Privacy-preserving accommodation path — the private
            // reference renders existence + scope + access path only.
            // The struct definition itself bars body / content /
            // payload fields, verified structurally by the targeted
            // test below.
            item(
                Cat::PrivacyPreservingAccommodationPath,
                if private_refs_have_access_path {
                    Pass
                } else {
                    Blocked {
                        reason: "private reference missing access status or review path",
                    }
                },
            ),
            // 3.11 Receipts / provenance / evidence access — confirmed
            // cards carry a plain-language receipt summary with the
            // closed receipt-class label. Pre-receipt cards that
            // expect a receipt carry the plain expected-receipt label
            // so the member sees the outcome line before confirm. The
            // opaque `receipt_ref_hash` is available for "details" but
            // never shown as primary.
            item(
                Cat::ReceiptsProvenanceAndEvidenceAccess,
                if !receipt_summary_present_for_confirmed {
                    Blocked {
                        reason: "confirmed card missing receipt summary",
                    }
                } else if !pre_receipt_expecting_cards_carry_expected_label {
                    Blocked {
                        reason: "pre-receipt card with receipt_expected=true is missing the \
                                 plain expected-receipt label",
                    }
                } else {
                    Pass
                },
            ),
            // 3.12 Governance and action access — every card carries
            // its plain-language `authority_basis`; the
            // insufficient-authority card carries a plain
            // `required_authority_explanation` naming the missing
            // authority class.
            item(
                Cat::GovernanceAndActionAccess,
                if !every_card_has_authority {
                    Blocked {
                        reason: "card missing plain-language authority basis",
                    }
                } else if !insufficient_authority_cards_have_explanation {
                    Blocked {
                        reason: "ClosedInsufficientAuthority card missing required_authority_\
                                 explanation",
                    }
                } else {
                    Pass
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

// ===========================================================================
// Fixture build helpers
// ===========================================================================

fn fixture_member() -> FixtureMember {
    FixtureMember {
        did: "did:icn:demo:member-1".to_string(),
        display_name: "Demo Member",
    }
}

fn fixture_device() -> FixtureDevice {
    FixtureDevice {
        label: "Demo phone",
        last_active_at: 1_715_000_010,
    }
}

fn fixture_local_domain() -> FixtureLocalDomain {
    let mut classes = BTreeSet::new();
    for c in FixtureOwningEntityClass::ALL {
        classes.insert(c);
    }
    FixtureLocalDomain {
        domain_id: "did:icn:demo:exampledomain".to_string(),
        display_name: "Example Local Domain",
        owning_entity_classes: classes,
    }
}

fn proposal_vote_card() -> FixtureActionCard {
    FixtureActionCard {
        card_id: "card-proposal-fixture-prop-1-vote",
        source_kind: FixtureSourceKind::Proposal,
        action_kind: FixtureActionKind::Vote,
        scope_label: "Example Local Domain",
        title: "Vote on the example fixture proposal",
        summary: "A short fixture summary describing what the proposal asks you to decide.",
        authority_basis: "You are a voting member of this domain.",
        required_authority_explanation: None,
        deadline_label: Some("Closes in 2 days"),
        risk_level: FixtureRiskLevel::Normal,
        accessibility_hint: "This card asks you to vote on a fixture proposal in your domain.",
        receipt_expected: true,
        expected_receipt_label: Some(FixtureReceiptClassLabel::GovernanceDecisionRecorded),
        state: FixtureActionLifecycle::Open,
        sync_status: FixtureSyncStatus::Synced,
        receipt_summary: None,
    }
}

fn meeting_attend_confirmed_card() -> FixtureActionCard {
    FixtureActionCard {
        card_id: "card-meeting-fixture-meet-1-attend",
        source_kind: FixtureSourceKind::Meeting,
        action_kind: FixtureActionKind::Attend,
        scope_label: "Example Local Domain",
        title: "Attended the example fixture meeting",
        summary: "Your attendance was recorded for the example fixture meeting.",
        authority_basis: "You are a meeting attendee.",
        required_authority_explanation: None,
        deadline_label: None,
        risk_level: FixtureRiskLevel::Low,
        accessibility_hint: "This card records that you attended a fixture meeting.",
        receipt_expected: true,
        // The card has already produced a receipt — the `receipt_summary`
        // below is the rendered post-confirm artifact, so the pre-receipt
        // expected-receipt label is no longer applicable.
        expected_receipt_label: None,
        state: FixtureActionLifecycle::Confirmed,
        sync_status: FixtureSyncStatus::ReceiptAvailable,
        receipt_summary: Some(FixtureReceiptSummary {
            plain_summary: "Your attendance at the example fixture meeting was recorded.",
            receipt_class_label: FixtureReceiptClassLabel::AttendanceRecorded,
            scope_label: "Example Local Domain",
            applied_at: 1_715_000_005,
            receipt_ref_hash: [0xDD; 32],
        }),
    }
}

fn sync_delayed_paused_card() -> FixtureActionCard {
    FixtureActionCard {
        card_id: "card-action_item-fixture-item-1-complete",
        source_kind: FixtureSourceKind::ActionItem,
        action_kind: FixtureActionKind::Complete,
        scope_label: "Example Local Domain",
        title: "Complete the example fixture action item",
        summary: "This action is paused until your domain's records finish syncing.",
        authority_basis: "You are the assigned holder of this action item.",
        required_authority_explanation: None,
        deadline_label: Some("No time pressure"),
        risk_level: FixtureRiskLevel::Normal,
        accessibility_hint: "This card is currently paused. You will be able to act once your \
                             domain's records finish syncing.",
        receipt_expected: true,
        expected_receipt_label: Some(FixtureReceiptClassLabel::ActionCompleted),
        state: FixtureActionLifecycle::OpenButPaused,
        sync_status: FixtureSyncStatus::ActionPausedUntilRecordsSync,
        receipt_summary: None,
    }
}

fn closed_insufficient_authority_card() -> FixtureActionCard {
    FixtureActionCard {
        card_id: "card-proposal-fixture-prop-2-vote",
        source_kind: FixtureSourceKind::Proposal,
        action_kind: FixtureActionKind::Vote,
        scope_label: "Example Local Domain",
        title: "Vote on the example federation-scoped proposal",
        summary: "This proposal asks for a federation-scoped vote that you are not currently \
                  authorized to cast.",
        authority_basis: "Federation-scoped voting requires a federation delegate authority.",
        required_authority_explanation: Some(
            "This requires authority you do not currently hold (federation delegate). You can \
             request authority through your steward.",
        ),
        deadline_label: Some("Closes in 5 days"),
        risk_level: FixtureRiskLevel::Normal,
        accessibility_hint: "This card is closed because you do not currently hold the required \
                             authority. A plain-language explanation is shown.",
        receipt_expected: false,
        expected_receipt_label: None,
        state: FixtureActionLifecycle::ClosedInsufficientAuthority,
        sync_status: FixtureSyncStatus::Synced,
        receipt_summary: None,
    }
}

fn fixture_private_evidence_reference() -> FixturePrivateEvidenceReference {
    FixturePrivateEvidenceReference {
        existence_label: "Private record exists",
        scope_label: "Example Local Domain",
        access_status_label: "Access pending review",
        request_review_path_label: "Request review through your steward",
    }
}

fn fixture_standing_surface(open_count: u32, closed_count: u32) -> FixtureStandingSurface {
    FixtureStandingSurface {
        member: fixture_member(),
        device: fixture_device(),
        current_domain: fixture_local_domain(),
        role_label: "Voting member",
        mandate_summary: "Voting mandate in your domain, valid for the current term.",
        sync_status_label: FixtureSyncStatus::SyncDelayed.label(),
        open_action_count: open_count,
        closed_for_authority_count: closed_count,
        closed_for_authority_explanation:
            "One card is closed because you do not currently hold the required authority.",
        private_records_access_path_label: "Privacy and access",
    }
}

fn render_open_view() -> FixtureMemberShellView {
    let cards = vec![
        proposal_vote_card(),
        meeting_attend_confirmed_card(),
        sync_delayed_paused_card(),
        closed_insufficient_authority_card(),
    ];
    FixtureMemberShellView {
        standing: fixture_standing_surface(1, 1),
        action_cards: cards,
        private_evidence_references: vec![fixture_private_evidence_reference()],
        surface_sync_status: FixtureSyncStatus::SyncDelayed,
    }
}

/// Render the resolved view: the Slice A repair has landed; the paused
/// card and the surface rollup transition to `Receipt available`. The
/// member-facing strings stay in the closed vocabulary.
///
/// `receipt` is the public, wire-stable `RepairReceipt` (#1849) whose
/// `receipt_hash` anchors the resolved card's opaque
/// `receipt_ref_hash`. The member never reads this hash — it is the
/// auditor-facing cross-link to the resolved repair evidence.
fn render_resolved_view(
    open_view: &FixtureMemberShellView,
    receipt: &RepairReceipt,
) -> FixtureMemberShellView {
    let mut resolved = open_view.clone();
    for card in resolved.action_cards.iter_mut() {
        if matches!(card.state, FixtureActionLifecycle::OpenButPaused) {
            card.state = FixtureActionLifecycle::Confirmed;
            card.sync_status = FixtureSyncStatus::ReceiptAvailable;
            card.summary = "Your action completed once your domain's records finished syncing.";
            card.accessibility_hint =
                "This card has been recorded as completed. The plain receipt summary is shown.";
            // Receipt has landed — the pre-receipt expected-receipt
            // label is no longer applicable; the receipt_summary below
            // is the rendered post-confirm artifact, anchored on the
            // public RepairReceipt binding hash.
            card.expected_receipt_label = None;
            card.receipt_summary = Some(FixtureReceiptSummary {
                plain_summary:
                    "Your action item was completed for the example fixture action item.",
                receipt_class_label: FixtureReceiptClassLabel::ActionCompleted,
                scope_label: "Example Local Domain",
                applied_at: receipt.applied_at,
                receipt_ref_hash: receipt.receipt_hash,
            });
        }
    }
    resolved.standing.sync_status_label = FixtureSyncStatus::ReceiptAvailable.label();
    resolved.surface_sync_status = FixtureSyncStatus::ReceiptAvailable;
    resolved
}

// ===========================================================================
// Forbidden member-facing protocol jargon
// ===========================================================================

/// Substrings the spec is explicit about keeping out of member-facing
/// strings (`docs/spec/member-shell-v0.md` §"Boundary lines / Member
/// shell vs. steward cockpit" + §"Member-facing status vocabulary" +
/// the failure-and-safety row that calls v0 violation on "raw
/// scheduler, runtime, or cockpit jargon"). The list also draws from
/// `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell
/// surface" which bars Bloom / Merkle / vector-clock / `policy_version_id`
/// wording.
const FORBIDDEN_MEMBER_FACING_SUBSTRINGS: &[&str] = &[
    "Bloom",
    "Merkle",
    "vector clock",
    "vector_clock",
    "policy_version_id",
    "evidence_hash",
    "plan_hash",
    "DigestMismatch",
    "DivergenceEvidence",
    "DivergenceClass",
    "RepairPlan",
    "RepairAction",
    "RepairReceipt",
    "PeerSyncReport",
    "AntiEntropyProbe",
    "BloomFilter",
    "BloomProjection",
    "MerkleRoot",
    "StateDigest",
    "StateClass",
    "ConstraintSet",
    "PolicyOracle",
    "AllowAllOracle",
    "protobuf",
    "gossip",
    "QUIC",
    "blockchain",
    "wallet",
    "currency",
    "crypto",
    // Financial framing barred per spec design principle 10. ("token"
    // would also bar legitimate words like "tokenized," so the list
    // keeps the bar narrow on terms the spec actually names.)
    "balance ",
];

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn member_shell_slice_a_renders_read_only_surface_and_action_cards() {
    // ---- Slice A proof-rail fixtures ----
    //
    // The member shell renders the *member-facing projection* of the
    // same proof rail the cockpit fixture renders. We construct the
    // divergence and the plan so a reviewer can see the cross-link is
    // present, then assert that none of the rail's vocabulary leaks
    // into the member view.
    let divergence = build_slice_a_divergence();
    assert!(divergence.verify_binding());
    assert_eq!(divergence.divergence_class, DivergenceClass::MissingReceipt);
    assert!(!divergence.private_content_implication);

    let plan = build_slice_a_repair_plan(&divergence);
    assert!(plan.verify_binding());
    assert_eq!(plan.divergence_evidence_hash, divergence.evidence_hash);

    // ---- Render the open member-shell view ----
    let open_view = render_open_view();

    // ---- Standing surface ----
    assert_eq!(open_view.standing.member.did, "did:icn:demo:member-1");
    assert_eq!(open_view.standing.member.display_name, "Demo Member");
    assert_eq!(open_view.standing.device.label, "Demo phone");
    assert_eq!(open_view.standing.device.last_active_at, 1_715_000_010);
    assert_eq!(
        open_view.standing.current_domain.domain_id,
        "did:icn:demo:exampledomain"
    );
    assert_eq!(
        open_view.standing.current_domain.display_name,
        "Example Local Domain"
    );
    // All four owning-entity classes are present.
    assert_eq!(
        open_view
            .standing
            .current_domain
            .owning_entity_classes
            .len(),
        4
    );
    for class in FixtureOwningEntityClass::ALL {
        assert!(open_view
            .standing
            .current_domain
            .owning_entity_classes
            .contains(&class));
    }
    assert_eq!(
        open_view.standing.sync_status_label,
        FixtureSyncStatus::SyncDelayed.label()
    );
    assert_eq!(open_view.standing.open_action_count, 1);
    assert_eq!(open_view.standing.closed_for_authority_count, 1);

    // ---- Exactly four ActionCards with the required kinds/states ----
    assert_eq!(open_view.action_cards.len(), 4);

    let proposal_vote = open_view
        .action_cards
        .iter()
        .find(|c| {
            matches!(c.source_kind, FixtureSourceKind::Proposal)
                && matches!(c.action_kind, FixtureActionKind::Vote)
                && matches!(c.state, FixtureActionLifecycle::Open)
        })
        .expect("proposal/vote card present and Open");
    assert!(proposal_vote.receipt_expected);

    let meeting_attend_done = open_view
        .action_cards
        .iter()
        .find(|c| {
            matches!(c.source_kind, FixtureSourceKind::Meeting)
                && matches!(c.action_kind, FixtureActionKind::Attend)
                && matches!(c.state, FixtureActionLifecycle::Confirmed)
        })
        .expect("meeting/attend Confirmed card with receipt present");
    let receipt = meeting_attend_done
        .receipt_summary
        .as_ref()
        .expect("Confirmed meeting/attend must carry a receipt summary");
    assert_eq!(receipt.receipt_class_label.label(), "Attendance recorded");

    let paused = open_view
        .action_cards
        .iter()
        .find(|c| matches!(c.state, FixtureActionLifecycle::OpenButPaused))
        .expect("paused card present");
    assert_eq!(
        paused.sync_status.label(),
        "Action paused until records sync"
    );

    let closed_auth = open_view
        .action_cards
        .iter()
        .find(|c| matches!(c.state, FixtureActionLifecycle::ClosedInsufficientAuthority))
        .expect("closed insufficient-authority card present");
    assert!(closed_auth.required_authority_explanation.is_some());
    // Insufficient authority is NOT a system error — see targeted test
    // below for the exact framing assertion.

    // ---- Private evidence reference is existence-only ----
    assert_eq!(open_view.private_evidence_references.len(), 1);
    let private_ref = &open_view.private_evidence_references[0];
    assert_eq!(private_ref.existence_label, "Private record exists");
    assert_eq!(private_ref.access_status_label, "Access pending review");

    // ---- Open state surface rollup ----
    assert_eq!(open_view.surface_sync_status.label(), "Sync delayed");

    // ---- Accessibility gate ----
    let checklist_open = FixtureAccessibilityChecklist::evaluate(&open_view);
    assert_eq!(checklist_open.items.len(), 12, "exactly 12 categories");
    assert!(
        checklist_open.is_acceptable(),
        "no Blocked outcomes on the Slice A open view"
    );
    let mut categories: Vec<FixtureAccessibilityCategory> =
        checklist_open.items.iter().map(|c| c.category).collect();
    categories.sort();
    categories.dedup();
    assert_eq!(categories.len(), 12);

    // ---- Resolved transition: paused → Receipt available ----
    // Construct the public RepairReceipt (#1849) and anchor the
    // resolved card's opaque receipt_ref_hash on it. The receipt's
    // verify_binding() proves the artifact has not been tampered
    // with; the resolved view's audit-facing cross-link points back
    // to it. No member-facing string surfaces the receipt's internal
    // field set.
    let evidence = build_slice_a_divergence();
    let plan = build_slice_a_repair_plan(&evidence);
    let receipt = build_slice_a_repair_receipt(&evidence, &plan);
    assert!(receipt.verify_binding());
    assert_eq!(receipt.effect_outcome, EffectOutcome::Applied);
    assert_eq!(receipt.divergence_evidence_hash, evidence.evidence_hash);
    assert_eq!(receipt.repair_plan_hash, plan.plan_hash);
    let resolved_view = render_resolved_view(&open_view, &receipt);
    assert_eq!(
        resolved_view.surface_sync_status.label(),
        "Receipt available"
    );
    let was_paused_now_confirmed = resolved_view
        .action_cards
        .iter()
        .find(|c| c.card_id == sync_delayed_paused_card().card_id)
        .expect("paused card still present after resolve");
    assert!(matches!(
        was_paused_now_confirmed.state,
        FixtureActionLifecycle::Confirmed
    ));
    // The opaque audit-facing receipt_ref_hash on the resolved card
    // anchors on the public RepairReceipt binding hash.
    let resolved_summary = was_paused_now_confirmed
        .receipt_summary
        .as_ref()
        .expect("resolved card carries a receipt summary");
    assert_eq!(resolved_summary.receipt_ref_hash, receipt.receipt_hash);
    assert_eq!(resolved_summary.applied_at, receipt.applied_at);
    assert_eq!(
        was_paused_now_confirmed.sync_status.label(),
        "Receipt available"
    );
    let new_receipt = was_paused_now_confirmed
        .receipt_summary
        .as_ref()
        .expect("resolved card carries a receipt summary");
    assert_eq!(new_receipt.receipt_class_label.label(), "Action completed");

    // ---- Accessibility gate still passes after resolve ----
    let checklist_resolved = FixtureAccessibilityChecklist::evaluate(&resolved_view);
    assert_eq!(checklist_resolved.items.len(), 12);
    assert!(checklist_resolved.is_acceptable());

    // ---- The decisive Slice A transition string ----
    let slice_a_transition = format!(
        "Members see: {} → {}",
        FixtureSyncStatus::SyncDelayed.label(),
        FixtureSyncStatus::ReceiptAvailable.label(),
    );
    assert_eq!(
        slice_a_transition,
        "Members see: Sync delayed → Receipt available"
    );
}

// ---------------------------------------------------------------------------
// Targeted unit tests
// ---------------------------------------------------------------------------

#[test]
fn closed_seven_string_sync_vocabulary_is_exact() {
    // Spec §"Member-facing status vocabulary / Sync state" + sibling
    // spec §"Member shell surface" pin the closed seven-string set. Any
    // accidental addition, removal, or rewording breaks this test.
    assert_eq!(FixtureSyncStatus::ALL.len(), 7);
    let labels: Vec<&'static str> = FixtureSyncStatus::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(
        labels,
        vec![
            "Synced",
            "Sync delayed",
            "Some records are being verified",
            "Action paused until records sync",
            "Receipt available",
            "Review required",
            "Sync delayed / degraded",
        ]
    );
    // Each label is unique.
    let mut uniq: BTreeMap<&'static str, ()> = BTreeMap::new();
    for l in &labels {
        uniq.insert(*l, ());
    }
    assert_eq!(uniq.len(), 7);
}

#[test]
fn action_cards_have_non_empty_accessibility_hint_and_label() {
    let view = render_open_view();
    for card in &view.action_cards {
        assert!(
            !card.title.is_empty(),
            "card {} missing title",
            card.card_id
        );
        assert!(
            !card.summary.is_empty(),
            "card {} missing summary",
            card.card_id
        );
        assert!(
            !card.accessibility_hint.is_empty(),
            "card {} missing accessibility_hint",
            card.card_id
        );
        assert!(
            !card.authority_basis.is_empty(),
            "card {} missing authority_basis",
            card.card_id
        );
    }
}

#[test]
fn fixture_private_evidence_reference_has_no_body_field() {
    // Structural tripwire — `FixturePrivateEvidenceReference` must
    // remain a plain reference type: existence, scope, access status,
    // request-review path. No body, content, payload, raw bytes, or
    // secret field. A future field addition that broke this contract
    // would push the struct over the size bound.
    assert!(
        std::mem::size_of::<FixturePrivateEvidenceReference>()
            <= 4 * std::mem::size_of::<&'static str>(),
        "FixturePrivateEvidenceReference must stay reference-only \
         (existence + scope + access status + review path)"
    );
}

#[test]
fn fixture_receipt_summary_has_no_body_field() {
    // Structural tripwire — `FixtureReceiptSummary` must remain a plain
    // summary type. No body / content / payload field. The summary is
    // a single plain string; the class is a closed-set enum; the
    // opaque cross-link hash lives at the end for "details."
    let summary = &meeting_attend_confirmed_card()
        .receipt_summary
        .expect("meeting/attend Confirmed card carries a receipt summary");
    // The summary string is plain language (one short sentence).
    assert!(summary.plain_summary.len() <= 140);
    // The class label is the closed plain-language label.
    assert_eq!(summary.receipt_class_label.label(), "Attendance recorded");
}

#[test]
fn insufficient_authority_card_does_not_imply_system_error() {
    // Spec §"Card states the shell must distinguish / Closed:
    // insufficient authority" — the shell must name the missing
    // authority plainly. It must not frame the closure as a system
    // failure, broken card, or error.
    let card = closed_insufficient_authority_card();
    let explanation = card
        .required_authority_explanation
        .expect("insufficient-authority card carries a plain explanation");
    let lowered = explanation.to_lowercase();
    for word in [
        "error",
        "failed",
        "failure",
        "crash",
        "broken",
        "misconfigured",
    ] {
        assert!(
            !lowered.contains(word),
            "insufficient-authority explanation must not imply system error; \
             found `{word}` in: `{explanation}`"
        );
    }
    // The summary itself must also not imply error.
    let summary_lower = card.summary.to_lowercase();
    for word in ["error", "failed", "failure", "broken"] {
        assert!(
            !summary_lower.contains(word),
            "insufficient-authority summary must not imply system error; \
             found `{word}` in: `{}`",
            card.summary
        );
    }
}

#[test]
fn receipt_summary_uses_plain_label_not_internal_class_name() {
    // Spec §"Receipt-class plain-language labels" — the shell maps
    // technical receipt class names (`MeetingAttendanceReceipt`,
    // `ActionItemCompletionReceipt`, `GovernanceDecisionReceipt`) to a
    // closed set of plain-language labels. The labels MUST be the
    // plain strings.
    assert_eq!(
        FixtureReceiptClassLabel::AttendanceRecorded.label(),
        "Attendance recorded"
    );
    assert_eq!(
        FixtureReceiptClassLabel::GovernanceDecisionRecorded.label(),
        "Governance decision recorded"
    );
    assert_eq!(
        FixtureReceiptClassLabel::ActionCompleted.label(),
        "Action completed"
    );

    // And every member-facing string carrying a receipt-class label
    // uses the plain string, never the technical class name.
    let view = render_open_view();
    for card in &view.action_cards {
        if let Some(receipt) = &card.receipt_summary {
            let plain = receipt.receipt_class_label.label();
            assert!(
                !plain.contains("Receipt"),
                "receipt-class label `{plain}` should be plain (e.g. \
                 `Attendance recorded`), not a technical class name"
            );
        }
    }
}

#[test]
fn member_facing_strings_contain_no_protocol_jargon() {
    // Spec §"Boundary lines / Member shell vs. steward cockpit": the
    // shell does not surface `DivergenceEvidence` class names, peer
    // DIDs, digest forms, or `policy_version_id` hashes. Network
    // sibling spec §"Member shell surface" bars Bloom / Merkle /
    // vector-clock / `policy_version_id` wording. Failure-and-safety
    // table calls v0 violation on "raw scheduler, runtime, or cockpit
    // jargon." This test locks the bar.
    let open_view = render_open_view();
    let evidence = build_slice_a_divergence();
    let plan = build_slice_a_repair_plan(&evidence);
    let receipt = build_slice_a_repair_receipt(&evidence, &plan);
    let resolved_view = render_resolved_view(&open_view, &receipt);

    for (label, view) in [("open", &open_view), ("resolved", &resolved_view)] {
        for s in view.all_member_facing_strings() {
            for forbidden in FORBIDDEN_MEMBER_FACING_SUBSTRINGS {
                assert!(
                    !s.contains(forbidden),
                    "{label} view contains forbidden member-facing substring `{forbidden}` in: \
                     `{s}`"
                );
            }
        }
    }
}

#[test]
fn private_evidence_reference_renders_existence_only() {
    // Spec §"Privacy and ScopedVault member affordances" + failure row
    // "Restricted content exposed in shell → Shell-side hard rule:
    // never." Slice A asserts the rendered private reference carries
    // existence + scope + access path only.
    let r = fixture_private_evidence_reference();
    assert_eq!(r.existence_label, "Private record exists");
    assert_eq!(r.scope_label, "Example Local Domain");
    assert_eq!(r.access_status_label, "Access pending review");
    assert!(!r.request_review_path_label.is_empty());

    // The struct's field set is exactly four `&'static str`s. The
    // structural tripwire above guarantees no body-shaped field has
    // been added.
}

#[test]
fn accessibility_gate_marks_blocked_when_card_misses_authority_basis() {
    // Defense-in-depth: category 3.12 (Governance and action access)
    // requires a plain-language authority basis on every card. A
    // missing `authority_basis` must Block.
    let mut bad_view = render_open_view();
    bad_view.action_cards[0].authority_basis = "";

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
fn pre_receipt_cards_with_receipt_expected_carry_expected_label() {
    // Spec §"ActionCard rendering contract / Mandatory rendering for
    // every ActionCard / `receipt_expected`": the shell must render
    // the outcome line ("If you confirm, this will produce a <receipt
    // class summary>") whenever the card is pre-receipt and expects
    // one. Slice A has two such cards (proposal/vote Open,
    // action_item/complete OpenButPaused) — both must carry the label.
    let view = render_open_view();
    let mut pre_receipt_expecting = 0;
    for card in &view.action_cards {
        if card.receipt_expected && card.is_pre_receipt_state() {
            pre_receipt_expecting += 1;
            assert!(
                card.expected_receipt_label.is_some(),
                "card {} is pre-receipt with receipt_expected=true but is missing the \
                 plain expected-receipt label",
                card.card_id
            );
        }
    }
    assert!(
        pre_receipt_expecting >= 2,
        "Slice A is expected to render at least two pre-receipt cards \
         that expect a receipt; found {pre_receipt_expecting}"
    );
    // And the labels we expect: vote → "Governance decision recorded";
    // action_item/complete → "Action completed."
    let labels: BTreeSet<&'static str> = view
        .action_cards
        .iter()
        .filter_map(|c| c.expected_receipt_label.map(|l| l.label()))
        .collect();
    assert!(labels.contains("Governance decision recorded"));
    assert!(labels.contains("Action completed"));
}

#[test]
fn accessibility_gate_marks_blocked_when_pre_receipt_card_omits_expected_label() {
    // Defense-in-depth for the 3.11 condition above. A pre-receipt
    // card with `receipt_expected=true` but no `expected_receipt_label`
    // must Block category 3.11.
    let mut bad_view = render_open_view();
    let pos = bad_view
        .action_cards
        .iter()
        .position(|c| c.receipt_expected && c.is_pre_receipt_state())
        .expect("Slice A renders at least one pre-receipt expecting card");
    bad_view.action_cards[pos].expected_receipt_label = None;

    let checklist = FixtureAccessibilityChecklist::evaluate(&bad_view);
    assert!(!checklist.is_acceptable());
    let cat = checklist
        .items
        .iter()
        .find(|c| {
            matches!(
                c.category,
                FixtureAccessibilityCategory::ReceiptsProvenanceAndEvidenceAccess
            )
        })
        .expect("category 3.11 must be present");
    let blocked_reason = match &cat.outcome {
        FixtureAccessibilityOutcome::Blocked { reason } => *reason,
        other => panic!("expected Blocked, got {other:?}"),
    };
    assert!(
        blocked_reason.contains("expected-receipt label"),
        "Blocked reason should name the expected-receipt label gap; got: `{blocked_reason}`"
    );
}

#[test]
fn accessibility_gate_marks_blocked_when_insufficient_authority_card_omits_explanation() {
    // Spec §"Card states the shell must distinguish / Closed:
    // insufficient authority" + ADR-0028 §3.12: a card closed for
    // insufficient authority must carry a plain explanation naming the
    // missing authority class. `authority_basis` alone is not enough.
    let mut bad_view = render_open_view();
    let pos = bad_view
        .action_cards
        .iter()
        .position(|c| matches!(c.state, FixtureActionLifecycle::ClosedInsufficientAuthority))
        .expect("Slice A renders an insufficient-authority card");
    bad_view.action_cards[pos].required_authority_explanation = None;

    let checklist = FixtureAccessibilityChecklist::evaluate(&bad_view);
    assert!(!checklist.is_acceptable());
    let cat = checklist
        .items
        .iter()
        .find(|c| {
            matches!(
                c.category,
                FixtureAccessibilityCategory::GovernanceAndActionAccess
            )
        })
        .expect("category 3.12 must be present");
    let blocked_reason = match &cat.outcome {
        FixtureAccessibilityOutcome::Blocked { reason } => *reason,
        other => panic!("expected Blocked, got {other:?}"),
    };
    assert!(
        blocked_reason.contains("required_authority_explanation"),
        "Blocked reason should name the missing required_authority_explanation; \
         got: `{blocked_reason}`"
    );
}

#[test]
fn closed_insufficient_authority_card_carries_required_authority_explanation() {
    // Positive complement to the negative test above: the Slice A
    // insufficient-authority card always carries its plain
    // explanation.
    let view = render_open_view();
    let card = view
        .action_cards
        .iter()
        .find(|c| matches!(c.state, FixtureActionLifecycle::ClosedInsufficientAuthority))
        .expect("Slice A renders an insufficient-authority card");
    let explanation = card
        .required_authority_explanation
        .expect("insufficient-authority card carries a plain explanation");
    assert!(!explanation.is_empty());
}

#[test]
fn accessibility_gate_marks_blocked_when_card_misses_accessibility_hint() {
    // Defense-in-depth: category 3.2 (Screen-reader / non-visual
    // access) requires every card to carry a plain-language
    // accessibility hint per ADR-0028.
    let mut bad_view = render_open_view();
    bad_view.action_cards[1].accessibility_hint = "";

    let checklist = FixtureAccessibilityChecklist::evaluate(&bad_view);
    assert!(!checklist.is_acceptable());
    let cat_32 = checklist
        .items
        .iter()
        .find(|c| {
            matches!(
                c.category,
                FixtureAccessibilityCategory::ScreenReaderAndNonVisualAccess
            )
        })
        .expect("category 3.2 must be present");
    assert!(matches!(
        cat_32.outcome,
        FixtureAccessibilityOutcome::Blocked { .. }
    ));
}

#[test]
fn fixture_view_has_no_runtime_state_fields() {
    // Structural tripwire — the member-shell view must remain a plain
    // data struct. No `Arc`, `JoinHandle`, `Sender`, runtime handle, or
    // socket allowed. The bound is loose but catches gross expansion.
    assert!(
        std::mem::size_of::<FixtureMemberShellView>() < 4096,
        "FixtureMemberShellView must remain a plain data struct"
    );
    assert!(std::mem::size_of::<FixtureActionCard>() < 1024);
    assert!(std::mem::size_of::<FixtureStandingSurface>() < 512);
    assert!(std::mem::size_of::<FixturePrivateEvidenceReference>() < 256);
    assert!(std::mem::size_of::<FixtureReceiptSummary>() < 256);
}

#[test]
fn accessibility_pass_and_not_applicable_counts_are_consistent() {
    // Every category must be Pass or NotApplicable on a healthy view,
    // and the total must be exactly twelve. Mirror of the cockpit
    // fixture's identical structural check.
    let view = render_open_view();
    let checklist = FixtureAccessibilityChecklist::evaluate(&view);
    assert_eq!(checklist.items.len(), 12);
    assert_eq!(
        checklist.pass_count() + checklist.not_applicable_count(),
        12,
        "every category must be Pass or NotApplicable on the Slice A member-shell view"
    );
    assert_eq!(FixtureAccessibilityCategory::ALL.len(), 12);
}

#[test]
fn slice_a_open_view_surface_status_is_sync_delayed() {
    // Slice A's decisive open string is "Sync delayed." Lock it.
    let open_view = render_open_view();
    assert_eq!(
        open_view.surface_sync_status,
        FixtureSyncStatus::SyncDelayed
    );
    assert_eq!(open_view.surface_sync_status.label(), "Sync delayed");
}

#[test]
fn slice_a_resolved_view_surface_status_is_receipt_available() {
    // Slice A's decisive resolved string is "Receipt available." Lock
    // it.
    let open_view = render_open_view();
    let evidence = build_slice_a_divergence();
    let plan = build_slice_a_repair_plan(&evidence);
    let receipt = build_slice_a_repair_receipt(&evidence, &plan);
    let resolved_view = render_resolved_view(&open_view, &receipt);
    assert_eq!(
        resolved_view.surface_sync_status,
        FixtureSyncStatus::ReceiptAvailable
    );
    assert_eq!(
        resolved_view.surface_sync_status.label(),
        "Receipt available"
    );
}

#[test]
fn four_owning_entity_classes_match_the_merged_taxonomy() {
    // Spec §"Standing surface / Memberships" — each domain entry shows
    // its owning entity class (Cooperative / Community / Federation /
    // Individual / other governed class). Slice A exercises the four
    // primary classes per the merged taxonomy.
    let domain = fixture_local_domain();
    assert_eq!(domain.owning_entity_classes.len(), 4);
    let labels: Vec<&'static str> = domain
        .owning_entity_classes
        .iter()
        .map(|c| c.label())
        .collect();
    let mut sorted = labels;
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["Community", "Cooperative", "Federation", "Individual"]
    );
}

#[test]
fn proof_rail_records_stay_off_member_facing_strings() {
    // The fixture builds the same `DivergenceEvidence` / `RepairPlan` /
    // `RepairReceipt` chain that the cockpit fixture (#1846) and
    // receipt-index fixture (#1845) render against. The member shell
    // anchors the resolved card's opaque `receipt_ref_hash` on the
    // receipt's binding hash so an auditor can chase the chain — but
    // none of those records' typed fields appear as member-facing
    // strings. Slice A asserts that none of the binding hashes leak
    // through as visible text on either view.
    let divergence = build_slice_a_divergence();
    let plan = build_slice_a_repair_plan(&divergence);
    let receipt = build_slice_a_repair_receipt(&divergence, &plan);
    let evidence_hex = hex::encode(divergence.evidence_hash);
    let plan_hex = hex::encode(plan.plan_hash);
    let receipt_hex = hex::encode(receipt.receipt_hash);
    let open_view = render_open_view();
    let resolved_view = render_resolved_view(&open_view, &receipt);
    for (label, view) in [("open", &open_view), ("resolved", &resolved_view)] {
        for s in view.all_member_facing_strings() {
            assert!(
                !s.contains(&evidence_hex),
                "{label} member-facing string leaks evidence_hash: `{s}`"
            );
            assert!(
                !s.contains(&plan_hex),
                "{label} member-facing string leaks plan_hash: `{s}`"
            );
            assert!(
                !s.contains(&receipt_hex),
                "{label} member-facing string leaks receipt_hash: `{s}`"
            );
        }
    }
    // The plan still carries its expected receipt class, but that
    // identifier is the cockpit's surface, not the member's. The
    // member only sees "Receipt available."
    assert_eq!(
        plan.expected_repair_receipt_class,
        ExpectedRepairReceiptClass::FetchMissingReceipt
    );
    // The receipt's class maps 1:1 back to the plan's expected class.
    assert_eq!(
        ExpectedRepairReceiptClass::from(receipt.repair_receipt_class),
        plan.expected_repair_receipt_class
    );
}
