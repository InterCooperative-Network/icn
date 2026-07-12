//! Isolated rehearsal review workspace (#1726 / #1728 / #2386).
//!
//! Domain state for the organizer pending-publish review surface mounted only
//! in [`GovernanceContextBuildMode::Rehearsal`](crate::http::GovernanceContextBuildMode).
//! It holds FICTIONAL, deterministic, generic rehearsal rows (seeded from the
//! same in-code fixture generator the read-only summary serves) plus the
//! organizer's review state over them. It is:
//!
//! - **domain-scoped** — one workspace per governance domain, initialized only
//!   by an explicit reset; no implicit fallback;
//! - **node-lifetime, resettable** — the review WORKSPACE is deliberately not
//!   durable storage (a reset or restart recreates the deterministic seed);
//!   everything that must outlive it (created action items, ADR-0026 process
//!   receipts) is persisted through the real governance manager machinery and
//!   is explicitly NOT stored here;
//! - **label-only on read surfaces** — assignment uses human-readable labels;
//!   a label may be bound to a fictional DID for the completion loop, but no
//!   read surface (rows, bindings, previews, evidence) ever exposes the DID.
//!
//! The preview→confirm binding lives here: [`canonical_plan_digest`] computes
//! a domain-separated BLAKE3 digest over the canonical mutation document
//! (`urn:icn:rehearsal-plan:v1`). Any change to the row, its review state,
//! its assignment, the label's identity binding, or the workspace generation
//! changes the digest, so a previously issued preview fails closed at
//! confirm. The digest bytes are what `record_mutation_plan_recorded`
//! persists as the plan `body_hash`, binding the approved preview to the
//! applied mutation through the real receipt ladder.
//!
//! Receipts record process facts; nothing in this module grants authority.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use serde::Serialize;

use crate::http::models::{PendingPublishRow, PendingPublishRowKind, PendingPublishRowStatus};

/// Domain-separation tag for the preview/plan digest (`urn:icn:rehearsal-plan:v1`).
const PLAN_DIGEST_TAG: &[u8] = b"icn:gov:rehearsal_plan:v1";
/// Domain-separation tag for review-decision body hashes.
const DECISION_BODY_TAG: &[u8] = b"icn:gov:rehearsal_review_decision:v1";
/// Domain-separation tag for the applied-mutation result hash.
const RESULT_HASH_TAG: &[u8] = b"icn:gov:rehearsal_apply_result:v1";

/// Bounded input limits (enforced at the HTTP layer; mirrored here for reuse).
pub const MAX_PLAIN_SUMMARY: usize = 256;
pub const MAX_NOTE: usize = 2000;
pub const MAX_LABEL: usize = 120;

/// A review decision an organizer can record. Closed set; anything else is a
/// 400 at the HTTP layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Approve,
    Reject,
    NeedsEdit,
    NeedsMoreInfo,
}

impl ReviewDecision {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            "needs_edit" => Some(Self::NeedsEdit),
            "needs_more_info" => Some(Self::NeedsMoreInfo),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::NeedsEdit => "needs_edit",
            Self::NeedsMoreInfo => "needs_more_info",
        }
    }

    pub fn status_after(self) -> PendingPublishRowStatus {
        match self {
            Self::Approve => PendingPublishRowStatus::ApprovedForPublish,
            Self::Reject => PendingPublishRowStatus::Rejected,
            Self::NeedsEdit => PendingPublishRowStatus::NeedsEdit,
            Self::NeedsMoreInfo => PendingPublishRowStatus::NeedsMoreInfo,
        }
    }
}

/// Reference to the DecisionRecorded receipt that approved a row, carried
/// into the activation step of the confirm ladder.
#[derive(Debug, Clone)]
pub struct ApproveRef {
    pub decision_id: String,
    pub record_hash: [u8; 32],
}

/// One recorded review decision (audit log for the evidence packet).
#[derive(Debug, Clone)]
pub struct DecisionLogEntry {
    pub seq: u64,
    pub row_id: String,
    pub row_version: u64,
    pub decision: ReviewDecision,
    pub decision_id: String,
    pub record_hash: [u8; 32],
    pub note_present: bool,
}

/// Execution record for a confirmed row: every id/hash returned by the real
/// receipt ladder plus the created action item. Hashes are copies of what the
/// machinery persisted; the receipts themselves live in the receipt store.
#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    pub preview_digest: [u8; 32],
    pub action_item_id: String,
    pub session_id: String,
    pub decision_id: String,
    pub decision_record_hash: [u8; 32],
    pub gate_record_hash: [u8; 32],
    pub activation_id: String,
    pub activation_record_hash: [u8; 32],
    pub plan_id: String,
    pub plan_record_hash: [u8; 32],
    pub application_id: String,
    pub application_record_hash: [u8; 32],
    pub result_hash: [u8; 32],
}

/// One rehearsal row: the generic fixture row plus mutable review state.
#[derive(Debug, Clone)]
pub struct RehearsalRow {
    pub base: PendingPublishRow,
    /// Bumps on every successful review/edit/assign mutation; part of the
    /// digest input, so any change invalidates outstanding previews.
    pub version: u64,
    pub note: Option<String>,
    /// Set when the current version was approved; cleared by any edit,
    /// re-assignment, or non-approve decision.
    pub approve_ref: Option<ApproveRef>,
    pub execution: Option<ExecutionRecord>,
}

/// The per-domain rehearsal workspace.
#[derive(Debug)]
pub struct Workspace {
    pub generation: u64,
    /// Monotonic sequence for decision ids within this workspace lifetime
    /// (NOT reset per generation, so decision ids never collide).
    pub decision_seq: u64,
    pub session_opened: bool,
    /// Record hash of the `ProcessSessionOpenedReceipt` once opened.
    pub session_record_hash: Option<[u8; 32]>,
    pub rows: Vec<RehearsalRow>,
    /// Human label → fictional DID string. DIDs never leave read surfaces.
    pub bindings: BTreeMap<String, String>,
    pub decisions: Vec<DecisionLogEntry>,
}

impl Workspace {
    /// Deterministic seed: fixture rows enter at version 0 with their labels
    /// registered (unbound). Prior bindings are cleared — a reset returns the
    /// whole fictional scenario to its clean state.
    fn seeded(generation: u64, decision_seq: u64, rows: Vec<PendingPublishRow>) -> Self {
        let mut bindings = BTreeMap::new();
        for row in &rows {
            if let Some(label) = &row.assignee_label {
                bindings.insert(label.clone(), String::new());
            }
        }
        Workspace {
            generation,
            decision_seq,
            session_opened: false,
            session_record_hash: None,
            rows: rows
                .into_iter()
                .map(|base| RehearsalRow {
                    base,
                    version: 0,
                    note: None,
                    approve_ref: None,
                    execution: None,
                })
                .collect(),
            bindings,
            decisions: Vec::new(),
        }
    }

    /// The process-session id for this workspace generation.
    pub fn session_id(&self) -> String {
        format!("rehearsal-review-gen{:04}", self.generation)
    }

    pub fn row(&self, row_id: &str) -> Option<&RehearsalRow> {
        self.rows.iter().find(|r| r.base.id == row_id)
    }

    pub fn row_mut(&mut self, row_id: &str) -> Option<&mut RehearsalRow> {
        self.rows.iter_mut().find(|r| r.base.id == row_id)
    }

    /// Whether a label is bound to an identity. Empty string = registered but
    /// unbound (seed state).
    pub fn label_bound(&self, label: &str) -> Option<bool> {
        self.bindings.get(label).map(|did| !did.is_empty())
    }

    /// The bound DID for a label, if any. Internal use only — never exported.
    pub fn bound_did(&self, label: &str) -> Option<&str> {
        self.bindings
            .get(label)
            .map(String::as_str)
            .filter(|d| !d.is_empty())
    }

    pub fn next_decision_id(&mut self) -> String {
        self.decision_seq += 1;
        format!("rehearsal-decision-{:06}", self.decision_seq)
    }
}

/// Shared container: one workspace per initialized domain. Owned by the
/// `GovernanceManager` (one instance per node), never a process-global.
/// All operations run under one `Mutex` with no awaits inside critical
/// sections — the entire review surface is serialized, which is the honest
/// concurrency model for a single-facilitator rehearsal and makes the
/// check-then-execute in confirm atomic.
#[derive(Debug, Default)]
pub struct RehearsalReviewState {
    inner: Mutex<HashMap<String, Workspace>>,
}

impl RehearsalReviewState {
    /// Initialize or re-seed the workspace for a domain. Returns the new
    /// generation (starts at 1).
    pub fn reset(&self, domain_id: &str, rows: Vec<PendingPublishRow>) -> u64 {
        let mut inner = self.inner.lock().expect("rehearsal state poisoned");
        let (generation, decision_seq) = match inner.get(domain_id) {
            Some(ws) => (ws.generation + 1, ws.decision_seq),
            None => (1, 0),
        };
        inner.insert(
            domain_id.to_string(),
            Workspace::seeded(generation, decision_seq, rows),
        );
        generation
    }

    /// Run `f` with the domain's workspace, if initialized.
    pub fn with_workspace<T>(
        &self,
        domain_id: &str,
        f: impl FnOnce(&mut Workspace) -> T,
    ) -> Option<T> {
        let mut inner = self.inner.lock().expect("rehearsal state poisoned");
        inner.get_mut(domain_id).map(f)
    }

    /// Whether any workspace has been initialized (drives the summary origin).
    pub fn any_initialized(&self) -> bool {
        !self
            .inner
            .lock()
            .expect("rehearsal state poisoned")
            .is_empty()
    }

    /// Snapshot every initialized workspace's rows for the self-scoped
    /// summary read model (single workspace in practice).
    pub fn summary_rows(&self) -> Vec<PendingPublishRow> {
        let inner = self.inner.lock().expect("rehearsal state poisoned");
        let mut domains: Vec<&String> = inner.keys().collect();
        domains.sort();
        domains
            .into_iter()
            .flat_map(|d| inner[d].rows.iter().map(|r| r.base.clone()))
            .collect()
    }
}

/// Canonical plan document (`urn:icn:rehearsal-plan:v1`) — exactly the values
/// that determine the mutation. Serialized with fixed field order (struct
/// declaration order) and hashed under [`PLAN_DIGEST_TAG`]. The bound DID is
/// part of the digest (so a rebinding invalidates previews) but the document
/// itself is never exported — the browser receives only the digest and the
/// human-readable fields.
#[derive(Debug, Serialize)]
pub struct CanonicalPlanV1<'a> {
    pub contract: &'static str,
    pub domain_id: &'a str,
    pub row_id: &'a str,
    pub generation: u64,
    pub version: u64,
    pub action: &'static str,
    pub title: &'a str,
    pub description: &'a str,
    pub assignee_label: Option<&'a str>,
    pub assignee_did: Option<&'a str>,
    pub due_date: Option<u64>,
    pub priority: &'static str,
    pub authority_basis: &'a str,
    pub risk_level: &'a str,
    pub receipt_expected_category: &'a str,
    pub source_provenance: &'a str,
    pub origin: &'static str,
    pub reversible: bool,
}

/// The plan document + digest for an action-item row in its current state.
pub struct PlannedMutation {
    pub digest: blake3::Hash,
    pub title: String,
    pub description: String,
    pub assignee_did: Option<String>,
    pub due_date: Option<u64>,
}

/// Build the canonical plan for a row and compute its digest. Returns `None`
/// for row kinds that are not executable in this slice.
pub fn plan_for_row(
    domain_id: &str,
    ws: &Workspace,
    row: &RehearsalRow,
) -> Option<PlannedMutation> {
    if row.base.kind != PendingPublishRowKind::ActionItem {
        return None;
    }
    let title = row.base.plain_summary.clone();
    let description = format!(
        "Rehearsal-confirmed proposed work {} (generation {}). Authority basis: {}. \
         Source: {}. Fictional rehearsal record — grants no authority.",
        row.base.id,
        ws.generation,
        row.base.authority_basis,
        provenance_str(&row.base),
    );
    let assignee_did = row
        .base
        .assignee_label
        .as_deref()
        .and_then(|l| ws.bound_did(l))
        .map(str::to_string);
    let due_date = row_due_date(row);

    let doc = CanonicalPlanV1 {
        contract: "urn:icn:rehearsal-plan:v1",
        domain_id,
        row_id: &row.base.id,
        generation: ws.generation,
        version: row.version,
        action: "create_action_item",
        title: &title,
        description: &description,
        assignee_label: row.base.assignee_label.as_deref(),
        assignee_did: assignee_did.as_deref(),
        due_date,
        priority: "medium",
        authority_basis: &row.base.authority_basis,
        risk_level: risk_str(&row.base),
        receipt_expected_category: receipt_category_str(&row.base),
        source_provenance: provenance_str(&row.base),
        origin: "rehearsal_runtime",
        reversible: false,
    };
    let bytes = serde_json::to_vec(&doc).expect("canonical plan serializes");
    let mut hasher = blake3::Hasher::new();
    hasher.update(PLAN_DIGEST_TAG);
    hasher.update(&bytes);
    Some(PlannedMutation {
        digest: hasher.finalize(),
        title,
        description,
        assignee_did,
        due_date,
    })
}

/// Rehearsal rows carry no due date today; edits may add one later. Kept as a
/// helper so the digest input and the created item can never disagree.
fn row_due_date(_row: &RehearsalRow) -> Option<u64> {
    None
}

/// Canonical body hash for a review decision (`DecisionRecordedReceipt` input).
/// Takes the generation explicitly so callers holding a mutable row borrow
/// need no simultaneous workspace borrow.
pub fn decision_body_hash_at(
    domain_id: &str,
    generation: u64,
    row: &RehearsalRow,
    decision: ReviewDecision,
    note: Option<&str>,
) -> [u8; 32] {
    #[derive(Serialize)]
    struct CanonicalDecisionV1<'a> {
        contract: &'static str,
        domain_id: &'a str,
        generation: u64,
        row_id: &'a str,
        row_version: u64,
        decision: &'static str,
        status_after: &'a str,
        plain_summary: &'a str,
        assignee_label: Option<&'a str>,
        note: Option<&'a str>,
    }
    let status_after = status_str(decision.status_after());
    let doc = CanonicalDecisionV1 {
        contract: "urn:icn:rehearsal-review-decision:v1",
        domain_id,
        generation,
        row_id: &row.base.id,
        row_version: row.version,
        decision: decision.as_str(),
        status_after,
        plain_summary: &row.base.plain_summary,
        assignee_label: row.base.assignee_label.as_deref(),
        note,
    };
    let bytes = serde_json::to_vec(&doc).expect("canonical decision serializes");
    let mut hasher = blake3::Hasher::new();
    hasher.update(DECISION_BODY_TAG);
    hasher.update(&bytes);
    *hasher.finalize().as_bytes()
}

/// Result hash binding the applied mutation to the created record.
pub fn apply_result_hash(
    domain_id: &str,
    action_item_id: &str,
    title: &str,
    assignee_did: Option<&str>,
    created_by_did: &str,
) -> [u8; 32] {
    #[derive(Serialize)]
    struct CanonicalResultV1<'a> {
        contract: &'static str,
        domain_id: &'a str,
        action_item_id: &'a str,
        title: &'a str,
        assignee_did: Option<&'a str>,
        created_by_did: &'a str,
    }
    let doc = CanonicalResultV1 {
        contract: "urn:icn:rehearsal-apply-result:v1",
        domain_id,
        action_item_id,
        title,
        assignee_did,
        created_by_did,
    };
    let bytes = serde_json::to_vec(&doc).expect("canonical result serializes");
    let mut hasher = blake3::Hasher::new();
    hasher.update(RESULT_HASH_TAG);
    hasher.update(&bytes);
    *hasher.finalize().as_bytes()
}

// ── Closed-enum wire strings (mirror the serde renames on the models; kept
// here so digest inputs never depend on serde_json enum tagging) ────────────

pub fn status_str(s: PendingPublishRowStatus) -> &'static str {
    match s {
        PendingPublishRowStatus::PendingReview => "pending_review",
        PendingPublishRowStatus::ApprovedForPublish => "approved_for_publish",
        PendingPublishRowStatus::Rejected => "rejected",
        PendingPublishRowStatus::NeedsEdit => "needs_edit",
        PendingPublishRowStatus::NeedsMoreInfo => "needs_more_info",
    }
}

fn risk_str(row: &PendingPublishRow) -> &'static str {
    use crate::http::models::PendingPublishRiskLevel as R;
    match row.risk_level {
        R::Low => "low",
        R::Normal => "normal",
        R::Elevated => "elevated",
    }
}

fn receipt_category_str(row: &PendingPublishRow) -> &'static str {
    use crate::http::models::PendingPublishReceiptCategory as C;
    match row.receipt_expected.category {
        C::GovernanceReceipt => "governance_receipt",
        C::AttendanceReceipt => "attendance_receipt",
        C::ActionItemCompletionReceipt => "action_item_completion_receipt",
        C::SettlementReceipt => "settlement_receipt",
        C::None => "none",
    }
}

fn provenance_str(row: &PendingPublishRow) -> &'static str {
    use crate::http::models::PendingPublishProvenance as P;
    match row.source_provenance {
        P::CommittedFixture => "committed_fixture",
        P::MeetingRecord => "meeting_record",
        P::GovernanceRecord => "governance_record",
        P::ExampleSnippet => "example_snippet",
        P::RepoSafePaste => "repo_safe_paste",
        P::PriorEvidencePacket => "prior_evidence_packet",
    }
}

/// Hex encoding for record hashes (lowercase, 64 chars).
pub fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
