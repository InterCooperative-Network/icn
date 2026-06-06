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
                return Err(e);
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
                    return Err(e);
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
}

impl CloseJournalEntry {
    /// Idempotently (re)persist every artifact, then the terminal proposal
    /// state.
    ///
    /// Order mirrors the pre-journal close path so behavior is unchanged on the
    /// happy path: Db-A receipts (v3 → v1 → allocation) → proof bytes (Db-B) →
    /// terminal `save_proposal` (Db-B). Every write is idempotent, so calling
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
        if let Some(proof) = &self.proof_bytes {
            state.save_proof_bytes(&self.proposal.id, proof)?;
        }
        state.save_proposal(&self.proposal)?;
        Ok(())
    }
}
