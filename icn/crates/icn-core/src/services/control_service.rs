//! Control service adapter for governance control effects.
//!
//! This module implements the `ControlService` trait from `icn-kernel-api`,
//! bridging the kernel-safe effect boundary to the actual governance actor.
//!
//! # Architecture
//!
//! ```text
//! [Kernel Effect]           [This Adapter]           [Governance Actor]
//! ControlEffect::Veto  -->  ControlServiceImpl  -->  GovernanceCommand::VetoProposal
//! ControlEffect::Force -->  ControlServiceImpl  -->  GovernanceCommand::ForceCloseProposal
//! ```

use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::governance::{ForcedOutcome, GovernanceHandle, ProposalId};
use icn_kernel_api::{
    ControlService, ForceCloseProposalRequest, ForceCloseProposalResult, VetoProposalRequest,
    VetoProposalResult,
};

/// Tracks provenance for control operations.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ControlProvenance {
    /// Decision receipt ID from governance
    pub decision_receipt_id: String,
    /// Decision hash for verification
    pub decision_hash: String,
    /// The proposal that was affected
    pub target_proposal_id: String,
    /// Operation type: "veto" or "force_close"
    pub operation_type: String,
    /// When the operation occurred
    pub timestamp: u64,
}

/// Adapter implementing `ControlService` for the governance actor.
///
/// This bridges kernel-safe control effects to the governance actor's
/// command interface, tracking provenance for audit purposes.
pub struct ControlServiceImpl {
    /// Handle to the governance actor
    gov_handle: GovernanceHandle,
    /// Provenance tracking (operation_key -> provenance)
    /// Key format: "{operation}:{target_proposal_id}"
    provenance: RwLock<HashMap<String, ControlProvenance>>,
}

impl ControlServiceImpl {
    /// Create a new control service adapter.
    pub fn new(gov_handle: GovernanceHandle) -> Self {
        Self {
            gov_handle,
            provenance: RwLock::new(HashMap::new()),
        }
    }

    /// Compute a state change hash for an operation.
    fn compute_state_change_hash(
        operation: &str,
        target_id: &str,
        decision_receipt_id: &str,
        timestamp: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(operation.as_bytes());
        hasher.update(b":");
        hasher.update(target_id.as_bytes());
        hasher.update(b":");
        hasher.update(decision_receipt_id.as_bytes());
        hasher.update(b":");
        hasher.update(timestamp.to_le_bytes());
        let hash = hasher.finalize();
        hex::encode(&hash[..20]) // First 20 bytes for reasonable length
    }

    /// Store provenance for an operation.
    #[allow(clippy::expect_used)]
    fn store_provenance(&self, key: String, provenance: ControlProvenance) {
        let mut map = self.provenance.write().expect("provenance lock poisoned");
        map.insert(key, provenance);
    }

    /// Get provenance for an operation.
    #[allow(dead_code, clippy::expect_used)]
    pub fn get_provenance(&self, operation: &str, target_id: &str) -> Option<ControlProvenance> {
        let key = format!("{operation}:{target_id}");
        let map = self.provenance.read().expect("provenance lock poisoned");
        map.get(&key).cloned()
    }
}

impl ControlService for ControlServiceImpl {
    fn veto_proposal(&self, request: VetoProposalRequest) -> Result<VetoProposalResult> {
        info!(
            target_proposal = %request.target_proposal_id,
            decision_receipt_id = %request.decision_receipt_id,
            "Executing veto via ControlService"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Submit the veto command to governance actor
        let proposal_id = ProposalId(request.target_proposal_id.clone());
        let reason = request.veto_reason.clone();

        // Use a blocking runtime to call the async governance handle
        // This is acceptable because control operations are rare and governance-internal
        let handle = self.gov_handle.clone();

        // Try to execute synchronously using tokio's current runtime
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                use crate::governance::GovernanceCommand;
                handle
                    .submit(GovernanceCommand::VetoProposal {
                        proposal_id: proposal_id.clone(),
                        reason,
                    })
                    .await
            })
        });

        match result {
            Ok(_) => {
                let state_change_hash = Self::compute_state_change_hash(
                    "veto",
                    &request.target_proposal_id,
                    &request.decision_receipt_id,
                    timestamp,
                );

                // Store provenance
                let key = format!("veto:{}", request.target_proposal_id);
                self.store_provenance(
                    key,
                    ControlProvenance {
                        decision_receipt_id: request.decision_receipt_id.clone(),
                        decision_hash: request.decision_hash.clone(),
                        target_proposal_id: request.target_proposal_id.clone(),
                        operation_type: "veto".to_string(),
                        timestamp,
                    },
                );

                info!(
                    target_proposal = %request.target_proposal_id,
                    state_change_hash = %state_change_hash,
                    "Veto succeeded"
                );

                Ok(VetoProposalResult {
                    success: true,
                    message: format!(
                        "Proposal {} vetoed successfully",
                        request.target_proposal_id
                    ),
                    state_change_hash: Some(state_change_hash),
                    vetoed_at: timestamp,
                })
            }
            Err(e) => {
                warn!(
                    target_proposal = %request.target_proposal_id,
                    error = %e,
                    "Veto failed"
                );

                Ok(VetoProposalResult {
                    success: false,
                    message: format!("Failed to veto proposal: {}", e),
                    state_change_hash: None,
                    vetoed_at: timestamp,
                })
            }
        }
    }

    fn force_close_proposal(
        &self,
        request: ForceCloseProposalRequest,
    ) -> Result<ForceCloseProposalResult> {
        info!(
            target_proposal = %request.target_proposal_id,
            forced_outcome = %request.forced_outcome,
            decision_receipt_id = %request.decision_receipt_id,
            "Executing force close via ControlService"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Parse the forced outcome string into ForcedOutcome enum
        let forced_outcome = match request.forced_outcome.to_lowercase().as_str() {
            "accept" | "accepted" => ForcedOutcome::Accept,
            "reject" | "rejected" => ForcedOutcome::Reject,
            "cancel" | "cancelled" | "expired" => ForcedOutcome::Cancel,
            other => {
                return Err(anyhow::anyhow!("Invalid forced outcome: {}", other))
                    .context("force_close_proposal");
            }
        };

        let proposal_id = ProposalId(request.target_proposal_id.clone());
        let reason = request.close_reason.clone();

        // Submit the force close command
        let handle = self.gov_handle.clone();

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                use crate::governance::GovernanceCommand;
                handle
                    .submit(GovernanceCommand::ForceCloseProposal {
                        proposal_id: proposal_id.clone(),
                        forced_outcome: forced_outcome.clone(),
                        reason,
                    })
                    .await
            })
        });

        match result {
            Ok(_) => {
                let state_change_hash = Self::compute_state_change_hash(
                    "force_close",
                    &request.target_proposal_id,
                    &request.decision_receipt_id,
                    timestamp,
                );

                // Store provenance
                let key = format!("force_close:{}", request.target_proposal_id);
                self.store_provenance(
                    key,
                    ControlProvenance {
                        decision_receipt_id: request.decision_receipt_id.clone(),
                        decision_hash: request.decision_hash.clone(),
                        target_proposal_id: request.target_proposal_id.clone(),
                        operation_type: "force_close".to_string(),
                        timestamp,
                    },
                );

                info!(
                    target_proposal = %request.target_proposal_id,
                    final_outcome = %request.forced_outcome,
                    state_change_hash = %state_change_hash,
                    "Force close succeeded"
                );

                Ok(ForceCloseProposalResult {
                    success: true,
                    message: format!(
                        "Proposal {} force closed as {}",
                        request.target_proposal_id, request.forced_outcome
                    ),
                    state_change_hash: Some(state_change_hash),
                    final_outcome: request.forced_outcome,
                    closed_at: timestamp,
                })
            }
            Err(e) => {
                warn!(
                    target_proposal = %request.target_proposal_id,
                    error = %e,
                    "Force close failed"
                );

                Ok(ForceCloseProposalResult {
                    success: false,
                    message: format!("Failed to force close proposal: {}", e),
                    state_change_hash: None,
                    final_outcome: request.forced_outcome,
                    closed_at: timestamp,
                })
            }
        }
    }

    fn can_veto(&self, target_proposal_id: &str, _domain_id: &str) -> bool {
        // For now, assume any proposal can be vetoed if it exists
        // A more sophisticated check would query the governance actor
        debug!(target_proposal = %target_proposal_id, "Checking if proposal can be vetoed");
        true
    }

    fn can_force_close(&self, target_proposal_id: &str, _domain_id: &str) -> bool {
        // For now, assume any proposal can be force closed if it exists
        debug!(target_proposal = %target_proposal_id, "Checking if proposal can be force closed");
        true
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_state_change_hash_deterministic() {
        let hash1 = ControlServiceImpl::compute_state_change_hash(
            "veto",
            "prop-123",
            "receipt-456",
            1700000000,
        );
        let hash2 = ControlServiceImpl::compute_state_change_hash(
            "veto",
            "prop-123",
            "receipt-456",
            1700000000,
        );
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_state_change_hash_varies_by_input() {
        let hash1 = ControlServiceImpl::compute_state_change_hash(
            "veto",
            "prop-123",
            "receipt-456",
            1700000000,
        );
        let hash2 = ControlServiceImpl::compute_state_change_hash(
            "force_close",
            "prop-123",
            "receipt-456",
            1700000000,
        );
        assert_ne!(hash1, hash2);
    }
}
