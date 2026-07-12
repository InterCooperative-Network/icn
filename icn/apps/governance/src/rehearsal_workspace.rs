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
//! The preview→confirm binding lives here: [`plan_for_row`] computes
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
    /// Set the moment the real action item is created, BEFORE the
    /// mutation-applied receipt is recorded. If that recording fails, a
    /// retried confirm resumes with this item instead of creating a second
    /// one; review mutations are blocked while it is set.
    pub pending_item_id: Option<String>,
    pub execution: Option<ExecutionRecord>,
}

/// The per-domain rehearsal workspace.
#[derive(Debug)]
pub struct Workspace {
    /// Restart-safe run scope for every PERSISTENT identifier this workspace
    /// mints (session/decision/activation/plan/application ids). The receipt
    /// store outlives this in-memory workspace, so ids must never repeat
    /// across daemon restarts: a fresh run id per reset guarantees a new id
    /// space while the seed CONTENT stays deterministic.
    pub run_id: String,
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
    fn seeded(
        run_id: String,
        generation: u64,
        decision_seq: u64,
        rows: Vec<PendingPublishRow>,
    ) -> Self {
        let mut bindings = BTreeMap::new();
        for row in &rows {
            if let Some(label) = &row.assignee_label {
                bindings.insert(label.clone(), String::new());
            }
        }
        Workspace {
            run_id,
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
                    pending_item_id: None,
                    execution: None,
                })
                .collect(),
            bindings,
            decisions: Vec::new(),
        }
    }

    /// The process-session id for this workspace run (restart-safe).
    pub fn session_id(&self) -> String {
        format!("rehearsal-{}-gen{:04}", self.run_id, self.generation)
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
        format!(
            "rehearsal-decision-{}-{:06}",
            self.run_id, self.decision_seq
        )
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
    /// generation (starts at 1) plus the action-item ids created by the
    /// PRIOR workspace (executed or interrupted), so the caller can retire
    /// them through the normal status machinery. Mints a fresh restart-safe
    /// run id: persistent receipt identifiers are never reused across
    /// resets or daemon restarts even though the seed content is
    /// deterministic.
    pub fn reset(&self, domain_id: &str, rows: Vec<PendingPublishRow>) -> (u64, Vec<String>) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut inner = self.lock_inner();
        let (generation, decision_seq, prior_items) = match inner.get(domain_id) {
            Some(ws) => (
                ws.generation + 1,
                ws.decision_seq,
                ws.rows
                    .iter()
                    .filter_map(|r| {
                        r.execution
                            .as_ref()
                            .map(|e| e.action_item_id.clone())
                            .or_else(|| r.pending_item_id.clone())
                    })
                    .collect(),
            ),
            None => (1, 0, Vec::new()),
        };
        let run_id = format!("{nanos:x}-g{generation:04}");
        inner.insert(
            domain_id.to_string(),
            Workspace::seeded(run_id, generation, decision_seq, rows),
        );
        (generation, prior_items)
    }

    /// Whether a workspace exists for this domain (drives the reset
    /// designation gate: first initialization is a setup act).
    pub fn is_initialized(&self, domain_id: &str) -> bool {
        self.lock_inner().contains_key(domain_id)
    }

    /// Lock with poisoned-lock recovery: rehearsal state is fictional and
    /// resettable, so recovering the inner value (with a warning) beats
    /// panicking a request thread forever after one panic under the lock.
    fn lock_inner(&self) -> std::sync::MutexGuard<'_, HashMap<String, Workspace>> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "rehearsal state mutex was poisoned; recovering (workspace is resettable)"
            );
            poisoned.into_inner()
        })
    }

    /// Run `f` with the domain's workspace, if initialized.
    pub fn with_workspace<T>(
        &self,
        domain_id: &str,
        f: impl FnOnce(&mut Workspace) -> T,
    ) -> Option<T> {
        let mut inner = self.lock_inner();
        inner.get_mut(domain_id).map(f)
    }

    /// The initialized workspace domains, sorted. The summary handler
    /// filters this list by the CALLER's own domain standing before serving
    /// any rows — workspace existence in one domain must never be observable
    /// from another.
    pub fn initialized_domains(&self) -> Vec<String> {
        let inner = self.lock_inner();
        let mut domains: Vec<String> = inner.keys().cloned().collect();
        domains.sort();
        domains
    }

    /// Snapshot the rows of ONE domain's workspace for the summary read
    /// model. Returns `None` when that domain has no workspace.
    pub fn domain_rows(&self, domain_id: &str) -> Option<Vec<PendingPublishRow>> {
        let inner = self.lock_inner();
        inner
            .get(domain_id)
            .map(|ws| ws.rows.iter().map(|r| r.base.clone()).collect())
    }
}

/// Canonical field framing for the domain-separated digests: fields are
/// appended in a fixed order, each length-prefixed (u64 LE); an `Option` adds
/// a 0/1 presence marker before the framed value. No serde in the hash path —
/// framing is explicit, deterministic, and cannot fail (the ICN receipt
/// convention: hash fields directly, never a serialization).
fn frame(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn frame_opt(hasher: &mut blake3::Hasher, value: Option<&str>) {
    match value {
        None => {
            hasher.update(&[0u8]);
        }
        Some(s) => {
            hasher.update(&[1u8]);
            frame(hasher, s.as_bytes());
        }
    }
}

fn frame_opt_u64(hasher: &mut blake3::Hasher, value: Option<u64>) {
    match value {
        None => {
            hasher.update(&[0u8]);
        }
        Some(v) => {
            hasher.update(&[1u8]);
            hasher.update(&v.to_le_bytes());
        }
    }
}

/// The plan document + digest for an action-item row in its current state.
pub struct PlannedMutation {
    pub digest: blake3::Hash,
    pub title: String,
    pub description: String,
    pub assignee_did: Option<String>,
    pub due_date: Option<u64>,
}

/// Build the canonical plan for a row and compute its digest
/// (`urn:icn:rehearsal-plan:v1`, tag-separated, explicit field framing).
/// Exactly the values that determine the mutation are bound — including the
/// workspace run/generation, the row version, and the label's currently
/// bound DID (part of the digest, never exported), so ANY intervening change
/// makes an outstanding preview stale. Returns `None` for row kinds that are
/// not executable in this slice.
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

    let mut h = blake3::Hasher::new();
    h.update(PLAN_DIGEST_TAG);
    frame(&mut h, b"urn:icn:rehearsal-plan:v1");
    frame(&mut h, domain_id.as_bytes());
    frame(&mut h, row.base.id.as_bytes());
    frame(&mut h, ws.run_id.as_bytes());
    h.update(&ws.generation.to_le_bytes());
    h.update(&row.version.to_le_bytes());
    frame(&mut h, b"create_action_item");
    frame(&mut h, title.as_bytes());
    frame(&mut h, description.as_bytes());
    frame_opt(&mut h, row.base.assignee_label.as_deref());
    frame_opt(&mut h, assignee_did.as_deref());
    frame_opt_u64(&mut h, due_date);
    frame(&mut h, b"medium");
    frame(&mut h, row.base.authority_basis.as_bytes());
    frame(&mut h, risk_str(&row.base).as_bytes());
    frame(&mut h, receipt_category_str(&row.base).as_bytes());
    frame(&mut h, provenance_str(&row.base).as_bytes());
    frame(&mut h, b"rehearsal_runtime");
    h.update(&[0u8]); // reversible: false
    Some(PlannedMutation {
        digest: h.finalize(),
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

/// Canonical body hash for a review decision (`DecisionRecordedReceipt`
/// input; `urn:icn:rehearsal-review-decision:v1`, explicit field framing).
/// Takes the generation explicitly so callers holding a mutable row borrow
/// need no simultaneous workspace borrow.
pub fn decision_body_hash_at(
    domain_id: &str,
    generation: u64,
    row: &RehearsalRow,
    decision: ReviewDecision,
    note: Option<&str>,
) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(DECISION_BODY_TAG);
    frame(&mut h, b"urn:icn:rehearsal-review-decision:v1");
    frame(&mut h, domain_id.as_bytes());
    h.update(&generation.to_le_bytes());
    frame(&mut h, row.base.id.as_bytes());
    h.update(&row.version.to_le_bytes());
    frame(&mut h, decision.as_str().as_bytes());
    frame(&mut h, status_str(decision.status_after()).as_bytes());
    frame(&mut h, row.base.plain_summary.as_bytes());
    frame_opt(&mut h, row.base.assignee_label.as_deref());
    frame_opt(&mut h, note);
    *h.finalize().as_bytes()
}

/// Result hash binding the applied mutation to the created record
/// (`urn:icn:rehearsal-apply-result:v1`, explicit field framing).
pub fn apply_result_hash(
    domain_id: &str,
    action_item_id: &str,
    title: &str,
    assignee_did: Option<&str>,
    created_by_did: &str,
) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(RESULT_HASH_TAG);
    frame(&mut h, b"urn:icn:rehearsal-apply-result:v1");
    frame(&mut h, domain_id.as_bytes());
    frame(&mut h, action_item_id.as_bytes());
    frame(&mut h, title.as_bytes());
    frame_opt(&mut h, assignee_did);
    frame(&mut h, created_by_did.as_bytes());
    *h.finalize().as_bytes()
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

/// Domain-separation tag for the evidence packet hash (BLAKE3, the ICN
/// receipt convention).
const EVIDENCE_TAG: &[u8] = b"icn:gov:rehearsal_workflow_evidence:v1";

/// Hash the canonical evidence-packet bytes. Returns
/// `(packet_hash, packet_hash_sha256)`:
///
/// - `packet_hash` — domain-separated BLAKE3 (primary, ICN convention);
/// - `packet_hash_sha256` — plain SHA-256 of the same bytes, carried as a
///   mirror so a browser steward view can verify tamper-evidence with
///   WebCrypto (which cannot compute BLAKE3) without any dependency.
pub fn evidence_packet_hashes(canonical: &[u8]) -> (String, String) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(EVIDENCE_TAG);
    hasher.update(canonical);
    let blake = hasher.finalize().to_hex().to_string();
    use sha2::Digest as _;
    let sha = format!("{:x}", sha2::Sha256::digest(canonical));
    (blake, sha)
}

/// Reusable evidence-packet tamper validator.
///
/// Canonicalization contract: the packet's own JSON serialization with the
/// two hash fields (`packet_hash`, `packet_hash_sha256`) removed. EVERY other
/// exported field — rows, decisions, receipt hashes, generation, run id,
/// bindings, non-claims, and `generated_at` — is bound by the hashes, so any
/// post-export change fails verification.
pub fn validate_evidence_packet(packet: &serde_json::Value) -> Result<(), String> {
    let obj = packet
        .as_object()
        .ok_or_else(|| "evidence packet must be a JSON object".to_string())?;
    let claimed_blake = obj
        .get("packet_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "evidence packet is missing packet_hash".to_string())?;
    let claimed_sha = obj
        .get("packet_hash_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "evidence packet is missing packet_hash_sha256".to_string())?;
    let mut content = obj.clone();
    content.remove("packet_hash");
    content.remove("packet_hash_sha256");
    let canonical = serde_json::to_vec(&content)
        .map_err(|e| format!("evidence packet does not serialize: {e}"))?;
    let (blake, sha) = evidence_packet_hashes(&canonical);
    if blake != claimed_blake {
        return Err("evidence packet failed BLAKE3 verification: content was altered".to_string());
    }
    if sha != claimed_sha {
        return Err("evidence packet failed SHA-256 verification: content was altered".to_string());
    }
    Ok(())
}
