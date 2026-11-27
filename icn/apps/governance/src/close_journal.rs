//! Write-ahead close journal for cross-store-atomic governance proposal close.
//!
//! Governance close persists provenance artifacts across two independent
//! sled-backed stores: the receipt store (`GovernanceReceiptBackend`, here
//! "Db-A") and the proposal state store (`GovernanceStateStore`, "Db-B"). The
//! two are separate `sled::Db` instances, so a single sled transaction cannot
//! span them. Without coordination, a terminal `save_proposal` (Db-B) failure
//! *after* the v1 governance receipt (Db-A) is durable leaves a permanent
//! phantom-accepted half-close: `GET /v1/receipts/chain` can observe an
//! accepted governance/allocation receipt while the proposal stays `Open`.
//!
//! This module provides a write-ahead-log discipline instead of cross-store
//! transactions:
//!
//! 1. Build the terminal-region artifacts in memory (v3 receipt,
//!    `GovernanceProofV2` bytes, v1 governance receipt, allocation receipt) and
//!    the closed [`Proposal`].
//! 2. Persist a self-contained [`CloseJournalEntry`] to Db-B *before* any
//!    durable artifact write (`save_close_intent`).
//! 3. [`CloseJournalEntry::commit`] writes every artifact idempotently, then
//!    the terminal proposal state.
//! 4. Delete the journal entry only after every artifact is durable.
//!
//! On the next actor startup, `GovernanceActor::recover_incomplete_closes`
//! replays any lingering entry through [`CloseJournalEntry::commit`] and clears
//! it. Because every artifact write is idempotent (receipts are hash-keyed /
//! append-only first-write-wins; `save_proof_bytes` / `save_proposal` overwrite
//! in place), replay can never duplicate or corrupt provenance.
//!
//! The entry carries the already-built, already-signed artifacts so recovery
//! replays exact bytes — preserving DTOs and signatures rather than re-deriving
//! them (which could yield a different `decision_hash` or attestation). No
//! append-only audit/provenance receipt is ever deleted, and the two stores are
//! never merged, so the meaning-firewall storage separation is preserved.
//!
//! # Commitment boundary
//!
//! For a journaled close, the fsynced [`CloseJournalEntry`] is the decision's
//! commitment point. Before it is durable nothing of the close is written.
//! Once it is durable the decision stands: veto, force-close and close each
//! complete a pending intent before acting (and are then refused, or report
//! the same close), and startup recovery completes it.
//!
//! When [`CloseJournalEntry::execution_closure`] is set, `commit` writes the
//! institutional-effect record and the ADR-0014 mandate (with its grants and
//! any SDIS lifecycle revocations) *before* the terminal `save_proposal`.
//! The mandate gate (`crate::mandate_gate::DefaultMandateGate::new`) resolves
//! a mandate only when its proposal's committed decision — this durable
//! accepted intent, or the terminal accepted proposal and its single v1
//! receipt — carries the mandate's decision and payload hash
//! (`crate::decision_commitment`). So, between the mandate write and the
//! terminal save, the mandate resolves on the strength of the durable intent
//! above, never on a decision that could still be refused; a mandate with no
//! such commitment (written by an older binary, or contradicted) does not
//! resolve at all.
//!
//! # Incomplete intents are not discarded
//!
//! An intent whose completion keeps failing (a persistent receipt-store write
//! error, or a [`crate::grant_minting::MANDATE_IDENTITY_CONFLICT`]) stays in
//! place. Every veto, force-close and close of that proposal first tries to
//! complete it and is refused while that fails; startup recovery retries it
//! and logs the failure. Nothing drops it automatically: resolving it needs
//! the cause fixed, or an authorized reconciliation.
//!
//! # Stored format and downgrade
//!
//! An entry without an execution closure is stored as the plain JSON object,
//! exactly as before the field existed, so older binaries replay it
//! unchanged. An entry *with* one is stored inside a one-key envelope
//! (`{"execution_closure_intent_v1": {..}}`). A binary that predates the
//! field cannot parse that envelope: it does not have the required top-level
//! `proposal` field. Such a binary therefore refuses the entry instead of
//! replaying it as a decision without its closure (and, for a force-accept,
//! as a rejection). Its recovery scan and every close/veto/force-close of
//! that proposal then fail closed until a binary that understands the
//! envelope completes the entry. Note that such a binary's
//! `list_close_intents` fails as a whole on one unparseable entry, so its
//! startup recovery skips ALL pending intents, not only the enveloped one;
//! an unrelated proposal's pending close still completes when that proposal
//! is next closed, vetoed or force-closed.
//!
//! Release note / runbook: do not roll back past this format while any
//! enveloped intent is pending. Since every accepted, execution-required
//! close (not only a force-accept) now journals its execution closure, that
//! includes routine closes interrupted before completion. Check that the
//! close journal is empty (or roll forward) before downgrading.

use serde::{Deserialize, Serialize};

use icn_governance::{GovernanceDecisionReceipt, GovernanceDecisionReceiptV3, Proposal};
use icn_kernel_api::AllocationReceipt;

use crate::receipt_backend::GovernanceReceiptBackend;
use crate::state_store::GovernanceStateStore;

/// A [`GovernanceDecisionReceiptV3`] plus the close timestamp it is ordered
/// under in opaque storage. `recorded_at` only orders the opaque entry; it is
/// not part of the canonical `decision_hash`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionV3Entry {
    /// The #1868 process-authorized decision receipt.
    pub receipt: GovernanceDecisionReceiptV3,
    /// Proposal close time, passed to `put_governance_decision_v3`.
    pub recorded_at: u64,
}

/// The receipt-store (Db-A) provenance artifacts produced by a close.
///
/// Shared by the actor close path, the standalone [`GovernanceManager`] close
/// path, and write-ahead recovery, so receipt emission cannot drift between
/// them.
///
/// [`GovernanceManager`]: crate::manager::GovernanceManager
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloseReceipts {
    /// #1868 process-authorized decision receipt — present only for scoped
    /// closes (a capability scope was actually presented).
    pub decision_v3: Option<DecisionV3Entry>,
    /// v1 governance decision receipt — the chain-observable accepted receipt
    /// that `GET /v1/receipts/chain` and `icnctl audit verify` index by
    /// `decision_hash`.
    pub governance_receipt: Option<GovernanceDecisionReceipt>,
    /// governance→economics allocation/contribution receipt (best-effort).
    pub allocation_receipt: Option<AllocationReceipt>,
}

impl CloseReceipts {
    /// True when there are no Db-A artifacts to write (e.g. no receipt backend
    /// was wired, or a non-execution-required close with no presented scope).
    pub fn is_empty(&self) -> bool {
        self.decision_v3.is_none()
            && self.governance_receipt.is_none()
            && self.allocation_receipt.is_none()
    }

    /// Idempotently write the Db-A provenance artifacts to the receipt backend
    /// in canonical order: v3 → v1 → allocation.
    ///
    /// `governance_receipt_fatal` mirrors the existing close-path policy: the v1
    /// `put_governance` write is fatal only when the proposal requires execution
    /// closure (PS-3 provenance chain). The actor path only ever carries a v1
    /// receipt under that condition, so it passes `true`; the standalone
    /// manager passes `requires_execution_closure`. The v3 write is always
    /// fatal. The allocation write is always best-effort: a missing allocation
    /// yields a *detectably incomplete* chain that `icnctl audit verify` flags,
    /// never a falsely-verified one — so no audit/provenance check is weakened.
    pub fn apply(
        &self,
        backend: &dyn GovernanceReceiptBackend,
        governance_receipt_fatal: bool,
    ) -> Result<(), String> {
        // v3 is always fail-closed: a scoped close emits it as required decision
        // evidence, and it must be durable before the v1 receipt below.
        if let Some(v3) = &self.decision_v3 {
            if let Err(e) = backend.put_governance_decision_v3(&v3.receipt, v3.recorded_at) {
                tracing::error!(
                    proposal_id = %v3.receipt.proposal_id,
                    error = %e,
                    "Failed to store v3 decision receipt — process-authority evidence not persisted"
                );
                return Err(format!(
                    "v3 decision receipt persistence failed for proposal '{}': {e}",
                    v3.receipt.proposal_id
                ));
            }
        }
        if let Some(receipt) = &self.governance_receipt {
            if let Err(e) = backend.put_governance(receipt) {
                // Log unconditionally (matches the pre-shared close paths), then
                // bail only when the v1 receipt is a hard provenance-chain
                // requirement (execution closure).
                tracing::error!(
                    proposal_id = %receipt.proposal_id,
                    error = %e,
                    "Failed to store governance decision receipt — provenance chain broken"
                );
                if governance_receipt_fatal {
                    return Err(format!(
                        "governance decision receipt persistence failed for proposal '{}': {e}",
                        receipt.proposal_id
                    ));
                }
            }
        }
        if let Some(allocation) = &self.allocation_receipt {
            if let Err(e) = backend.put_allocation(allocation) {
                tracing::error!(
                    error = %e,
                    "Failed to store allocation receipt — economics binding broken (best-effort)"
                );
            }
        }
        Ok(())
    }
}

/// A durable close intent that contradicts the proposal's stored terminal state.
///
/// The intent records a decided close (its receipts may already be durable), yet
/// the stored proposal is already closed with a *different* value. That can only
/// arise from data written before veto and force-close completed a pending close
/// first. Neither record is chosen automatically: recovery leaves both in place,
/// every further terminal operation on the proposal is refused, and the
/// contradiction is reported here until an authorized reconciliation resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseConflict {
    /// The proposal both records describe.
    pub proposal_id: String,
    /// The terminal state the durable close intent would commit.
    pub journaled_state: String,
    /// The terminal state actually stored.
    pub stored_state: String,
}

impl std::fmt::Display for CloseConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Proposal '{}' has a durable close intent ({}) that contradicts its stored \
             terminal state ({}); both are retained and the proposal is refused further \
             terminal changes until an authorized reconciliation",
            self.proposal_id, self.journaled_state, self.stored_state
        )
    }
}

impl std::error::Error for CloseConflict {}

/// Short, stable name of a proposal state for conflict reports (no payload data).
pub(crate) fn state_name(state: &icn_governance::ProposalState) -> String {
    let debug = format!("{state:?}");
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `Some(conflict)` when `stored` is closed and differs from the journaled
/// terminal value; `None` when completing the intent is safe (the row is not yet
/// closed, or already holds exactly the journaled value).
pub(crate) fn detect_conflict(
    entry: &CloseJournalEntry,
    stored: Option<&Proposal>,
) -> anyhow::Result<Option<CloseConflict>> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if !stored.state.is_closed()
        || serde_json::to_value(stored)? == serde_json::to_value(&entry.proposal)?
    {
        return Ok(None);
    }
    Ok(Some(CloseConflict {
        proposal_id: entry.proposal.id.0.clone(),
        journaled_state: state_name(&entry.proposal.state),
        stored_state: state_name(&stored.state),
    }))
}

/// A self-contained write-ahead record of an in-flight proposal close.
///
/// Persisted to the proposal state store (Db-B) keyed by `proposal.id` *before*
/// any terminal-region provenance write, and deleted only after every artifact
/// is durable. See the module docs for the full discipline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseJournalEntry {
    /// The proposal in its terminal (closed) state — the terminal commit value.
    pub proposal: Proposal,
    /// Serialized `GovernanceProofV2` bytes (Db-B), when a signing key is wired.
    pub proof_bytes: Option<Vec<u8>>,
    /// Db-A receipt-store artifacts.
    pub receipts: CloseReceipts,
    /// Close timestamp (`now`) used for the downstream event + side effects, so
    /// recovery reproduces the same `decided_at` rather than using restart time.
    pub decided_at: u64,
    /// Hex-encoded accepted decision hash (when the outcome is Accepted). Lets
    /// recovery replay the downstream `ProposalAccepted` event and the
    /// non-execution-required institutional-effect / mandate emission without
    /// re-deriving the hash from votes/tally.
    pub governance_decision_hash: Option<String>,
    /// Whether [`Self::commit`] also writes the close's execution closure —
    /// the institutional-effect record and the ADR-0014 mandate (with its
    /// derived grants and any SDIS lifecycle revocations) — for an accepted,
    /// execution-required proposal.
    ///
    /// Those writes are execution authority: the mandate gate resolves grants
    /// from them and lifecycle revocations end other grants. When this is
    /// `true` they happen only behind this durable intent, so a decision that
    /// never became durable cannot leave authority behind, and an interrupted
    /// one is completed by retry or startup recovery. Every write is
    /// idempotent (`AlreadyEmitted` / `AlreadyMinted`), so replay never adds a
    /// second mandate.
    ///
    /// Set by the ordinary close and the force-accept path for an accepted,
    /// execution-required proposal. Entries written before this field existed
    /// deserialize as `false`, which matches what they did: their close wrote
    /// those artifacts itself, before the intent.
    #[serde(default)]
    pub execution_closure: bool,
}

/// True when `state` records an accepted outcome — an ordinary accepted close
/// or a force-close whose forced outcome is acceptance.
pub(crate) fn is_accepted(state: &icn_governance::ProposalState) -> bool {
    matches!(
        state,
        icn_governance::ProposalState::Accepted { .. }
            | icn_governance::ProposalState::ForceClosed {
                outcome: icn_governance::ProposalOutcome::Accepted,
                ..
            }
    )
}

/// Envelope key for an entry that carries an execution closure. See the
/// module docs ("Stored format and downgrade").
const EXECUTION_CLOSURE_ENVELOPE: &str = "execution_closure_intent_v1";

/// Fail unless an institutional effect that the emission seam reported as
/// already stored is bound to exactly `decision_hash`.
///
/// `emit_accepted_effect` deduplicates by `(proposal_id, effect_kind)` only.
/// A record that already exists for the proposal but names another decision
/// (or none) is not this decision's effect; adopting it would let the
/// proposal's terminal record and its effect disagree about which decision
/// produced it. A fresh emission (`Emitted`) or `NoEffect` passes.
pub(crate) fn require_effect_bound_to_decision(
    backend: &dyn GovernanceReceiptBackend,
    proposal_id: &str,
    outcome: &crate::institutional_effect::AcceptanceEmissionOutcome,
    decision_hash: Option<icn_kernel_api::Hash>,
) -> Result<(), String> {
    let crate::institutional_effect::AcceptanceEmissionOutcome::AlreadyEmitted { record_id } =
        outcome
    else {
        return Ok(());
    };
    let prior = backend
        .list_institutional_effects_by_proposal(proposal_id)
        .map_err(|e| format!("could not read the existing institutional effect: {e}"))?
        .into_iter()
        .find(|r| &r.record_id == record_id);
    match prior {
        Some(r) if r.decision_hash == decision_hash => Ok(()),
        Some(_) => Err(
            "an institutional effect for this proposal is already bound to a different decision"
                .to_string(),
        ),
        None => Err(format!(
            "the existing institutional effect '{record_id}' could not be read back"
        )),
    }
}

impl CloseJournalEntry {
    /// Serialize for the state store. An entry with an execution closure is
    /// wrapped in the [`EXECUTION_CLOSURE_ENVELOPE`] so a binary that does not
    /// know the field refuses it rather than misreading it; any other entry
    /// keeps the legacy plain shape.
    pub fn to_stored_bytes(&self) -> serde_json::Result<Vec<u8>> {
        if self.execution_closure {
            let mut envelope = serde_json::Map::new();
            envelope.insert(
                EXECUTION_CLOSURE_ENVELOPE.to_string(),
                serde_json::to_value(self)?,
            );
            serde_json::to_vec(&serde_json::Value::Object(envelope))
        } else {
            serde_json::to_vec(self)
        }
    }

    /// Parse bytes written by [`Self::to_stored_bytes`] (or by an older binary,
    /// which wrote the plain shape). An envelope must be the only key and must
    /// carry an entry with an execution closure; anything else is refused.
    pub fn from_stored_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        if let Some(object) = value.as_object() {
            if let Some(inner) = object.get(EXECUTION_CLOSURE_ENVELOPE) {
                if object.len() != 1 {
                    anyhow::bail!("close-journal envelope carries unexpected extra keys");
                }
                let entry: Self = serde_json::from_value(inner.clone())?;
                if !entry.execution_closure {
                    anyhow::bail!(
                        "close-journal envelope for proposal '{}' does not carry an execution closure",
                        entry.proposal.id.0
                    );
                }
                return Ok(entry);
            }
        }
        Ok(serde_json::from_value(value)?)
    }

    /// Idempotently (re)persist every artifact, then the terminal proposal
    /// state.
    ///
    /// Order mirrors the pre-journal close path so behavior is unchanged on the
    /// happy path: Db-A receipts (v3 → v1 → allocation) → execution closure
    /// (Db-A, only when [`Self::execution_closure`] is set) → proof bytes
    /// (Db-B) → terminal `save_proposal` (Db-B). Every write is idempotent, so calling
    /// `commit` again (on recovery, or after a transient partial failure)
    /// cannot duplicate or corrupt provenance.
    ///
    /// The v1 receipt is treated as fatal here (`apply(.., true)`): the actor
    /// close path and recovery only ever carry a v1 receipt for an
    /// execution-required close, where it is a hard provenance-chain
    /// requirement.
    pub fn commit(
        &self,
        backend: Option<&dyn GovernanceReceiptBackend>,
        state: &dyn GovernanceStateStore,
    ) -> anyhow::Result<()> {
        if !self.receipts.is_empty() {
            let backend = backend.ok_or_else(|| {
                anyhow::anyhow!(
                    "close journal for proposal '{}' carries receipt artifacts but no receipt backend is wired",
                    self.proposal.id.0
                )
            })?;
            self.receipts
                .apply(backend, true)
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        if self.execution_closure {
            let backend = backend.ok_or_else(|| {
                anyhow::anyhow!(
                    "close journal for proposal '{}' carries an execution closure but no receipt backend is wired",
                    self.proposal.id.0
                )
            })?;
            self.apply_execution_closure(backend)?;
        }
        if let Some(proof) = &self.proof_bytes {
            state.save_proof_bytes(&self.proposal.id, proof)?;
        }
        state.save_proposal(&self.proposal)?;
        Ok(())
    }

    /// Idempotently write the execution closure: institutional-effect record,
    /// then the ADR-0014 mandate (and its grants / lifecycle revocations).
    ///
    /// Uses `decided_at` as the write time so a replay reproduces the same
    /// records rather than restamping them with the restart time. Fails closed
    /// unless the entry records an accepted outcome with a well-formed decision
    /// hash — a mandate must never be minted for anything else.
    fn apply_execution_closure(
        &self,
        backend: &dyn GovernanceReceiptBackend,
    ) -> anyhow::Result<()> {
        let proposal_id = &self.proposal.id.0;
        if !is_accepted(&self.proposal.state) {
            anyhow::bail!(
                "close journal for proposal '{proposal_id}' carries an execution closure but does not record an accepted outcome"
            );
        }
        let decision_hash: icn_kernel_api::Hash = self
            .governance_decision_hash
            .as_deref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|v| <[u8; 32]>::try_from(v.as_slice()).ok())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "close journal for proposal '{proposal_id}' carries an execution closure without a valid decision hash"
                )
            })?;
        // The journal alone does not authorize: the decision it carries must be
        // an accepted one and internally consistent. A v1 governance receipt in
        // the same entry must name this proposal, record acceptance and carry
        // exactly the decision hash the closure is bound to.
        if let Some(receipt) = &self.receipts.governance_receipt {
            if receipt.proposal_id != *proposal_id
                || receipt.decision_hash != decision_hash
                || !matches!(
                    receipt.outcome,
                    icn_governance::proof::ProofOutcome::Accepted
                )
            {
                anyhow::bail!(
                    "close journal for proposal '{proposal_id}' carries an execution closure whose \
                     decision does not match its governance receipt"
                );
            }
        }
        let effect = crate::institutional_effect::emit_accepted_effect(
            backend,
            proposal_id,
            &self.proposal.domain_id.0,
            Some(decision_hash),
            &self.proposal.payload,
            self.decided_at,
        )
        .map_err(|e| {
            anyhow::anyhow!(
                "execution closure for proposal '{proposal_id}' failed: institutional effect: {e}"
            )
        })?;
        // An effect record that already exists must belong to THIS decision.
        // One bound to another decision hash (or to none) is refused, never
        // adopted: the intent stays for an authorized reconciliation.
        require_effect_bound_to_decision(backend, proposal_id, &effect, Some(decision_hash))
            .map_err(|e| {
                anyhow::anyhow!("execution closure for proposal '{proposal_id}' failed: {e}")
            })?;
        match crate::grant_minting::mint_and_persist_for_accepted(
            backend,
            proposal_id,
            &self.proposal.domain_id,
            decision_hash,
            &self.proposal.payload,
            self.decided_at,
        ) {
            Ok(crate::grant_minting::MandateMintOutcome::Minted { .. })
            | Ok(crate::grant_minting::MandateMintOutcome::AlreadyMinted { .. }) => Ok(()),
            Ok(crate::grant_minting::MandateMintOutcome::HashFailed) => anyhow::bail!(
                "execution closure for proposal '{proposal_id}' failed: mandate payload hash failed"
            ),
            Err(e) => {
                anyhow::bail!("execution closure for proposal '{proposal_id}' failed: mandate: {e}")
            }
        }
    }
}
