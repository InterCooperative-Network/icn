//! App-layer implementation of [`icn_kernel_api::DispatchEvidenceSink`].
//!
//! Closes the actor-path dispatch-evidence gap: when a proposal is
//! accepted through any path (gateway close, actor `CloseProposal`,
//! `ForceCloseProposal`, scheduler-driven close) the acceptance event
//! reaches `create_effect_subscription` → `create_decision_executor_callback`
//! in `icn-core`. Before this sink existed, per-effect results were
//! logged and discarded. With the sink wired, the same path writes
//! durable [`EffectDispatchEvidence`] against the matching
//! [`InstitutionalEffectRecord`], so acceptance-evidence parity no
//! longer depends on the gateway-close HTTP handler.
//!
//! The sink is **best-effort**: any failure to resolve the proposal or
//! write evidence logs and skips. The kernel does not retry, and
//! missing evidence never blocks governance.

use std::sync::Arc;

use icn_governance::ProposalId;
use icn_kernel_api::effects::{
    kernel_effect_subsystem, DispatchEvidenceSink, EffectResult, KernelEffect,
};

use crate::dispatch_evidence::EffectDispatchEvidence;
use crate::manager::GovernanceManager;

/// Prefix produced by `create_effect_subscription` when it formats
/// `decision_receipt_id = "gov:{domain_id}:{proposal_id}:receipt"`.
const RECEIPT_ID_PREFIX: &str = "gov:";
const RECEIPT_ID_SUFFIX: &str = ":receipt";

/// Derive the effect_kind label used by [`InstitutionalEffectRecord`]
/// from a kernel effect variant. This mirrors the mapping that
/// `payload_effect_kind` applies at emission time — same snake_case
/// labels keyed by the same variant shape.
///
/// Returns `None` for effects that do not correspond to a recorded
/// institutional effect (e.g. `NoOp`, pure treasury operations that
/// never emit an IER today).
fn kernel_effect_to_effect_kind(effect: &KernelEffect) -> Option<&'static str> {
    use icn_kernel_api::effects::{MembershipEffect, SdisEffect};

    match effect {
        KernelEffect::Sdis(SdisEffect::ApproveSteward { .. }) => Some("appoint_steward"),
        KernelEffect::Sdis(SdisEffect::RevokeSteward { .. }) => Some("revoke_steward"),
        KernelEffect::Sdis(SdisEffect::ReconfirmSteward { .. }) => Some("reconfirm_steward"),
        KernelEffect::Sdis(SdisEffect::ReinstateSteward { .. }) => Some("reinstate_steward"),
        KernelEffect::Sdis(SdisEffect::SuspendSteward { .. }) => Some("suspend_steward"),
        KernelEffect::Sdis(SdisEffect::SanctionSteward { .. }) => Some("sanction_steward"),
        KernelEffect::Membership(MembershipEffect::FreezeMember { .. }) => Some("freeze_member"),
        KernelEffect::Membership(MembershipEffect::UnfreezeMember { .. }) => {
            Some("unfreeze_member")
        }
        _ => None,
    }
}

/// Parse `proposal_id` out of a governance `decision_receipt_id` of the
/// canonical form `gov:{domain_id}:{proposal_id}:receipt`.
///
/// Returns the slice between the domain and the suffix. On any shape
/// deviation, returns `None` — callers must treat that as "no linkage"
/// and skip the write.
fn parse_proposal_id(decision_receipt_id: &str) -> Option<&str> {
    let stripped = decision_receipt_id
        .strip_prefix(RECEIPT_ID_PREFIX)?
        .strip_suffix(RECEIPT_ID_SUFFIX)?;
    // stripped = "{domain_id}:{proposal_id}"
    let (_domain, proposal) = stripped.split_once(':')?;
    if proposal.is_empty() {
        None
    } else {
        Some(proposal)
    }
}

/// Governance-side sink that persists dispatch evidence via the
/// receipt backend owned by [`GovernanceManager`].
///
/// Construct one per running daemon, after the manager has had its
/// receipt store installed. Kernel-clean: the kernel only sees the
/// trait; the manager type is confined to this app crate.
pub struct GovernanceDispatchEvidenceSink {
    manager: Arc<GovernanceManager>,
}

impl GovernanceDispatchEvidenceSink {
    pub fn new(manager: Arc<GovernanceManager>) -> Self {
        Self { manager }
    }

    /// Record evidence for a single (effect, result) pair. Separated from
    /// the batch entry point so tests can exercise the per-effect path
    /// directly.
    fn record_one(
        &self,
        decision_receipt_id: &str,
        proposal_id: &str,
        effect: &KernelEffect,
        result: &EffectResult,
        recorded_at: u64,
    ) {
        let Some(effect_kind) = kernel_effect_to_effect_kind(effect) else {
            // NoOp or effects that never emit an IER — nothing to link.
            return;
        };

        let records = match self
            .manager
            .list_institutional_effects(&ProposalId(proposal_id.to_string()))
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    proposal_id = %proposal_id,
                    receipt_id = %decision_receipt_id,
                    error = %e,
                    "dispatch-evidence sink: list_institutional_effects failed; skipping"
                );
                return;
            }
        };
        let Some(rec) = records.iter().find(|r| r.effect_kind == effect_kind) else {
            // No IER was emitted for this effect_kind (e.g. receipt store
            // wasn't installed when the acceptance ran). Nothing to link.
            tracing::debug!(
                proposal_id = %proposal_id,
                effect_kind = %effect_kind,
                "dispatch-evidence sink: no institutional effect record for effect_kind; skipping"
            );
            return;
        };

        let subsystem = kernel_effect_subsystem(effect).to_string();
        let error_message = if result.success {
            None
        } else {
            Some(result.message.clone())
        };
        let evidence = EffectDispatchEvidence::new(
            rec.record_id.clone(),
            proposal_id.to_string(),
            subsystem,
            // The kernel path does not mint a downstream receipt_ref;
            // leave None and let evidence-returning hooks (gateway path)
            // fill it in when they exist.
            None,
            result.success,
            error_message,
            recorded_at,
        );

        if let Err(e) = self.manager.record_dispatch_evidence(&evidence) {
            tracing::error!(
                proposal_id = %proposal_id,
                effect_kind = %effect_kind,
                error = %e,
                "dispatch-evidence sink: failed to persist evidence"
            );
        }
    }
}

impl DispatchEvidenceSink for GovernanceDispatchEvidenceSink {
    fn record_effects(
        &self,
        decision_receipt_id: &str,
        effects: &[KernelEffect],
        results: &[EffectResult],
        recorded_at: u64,
    ) {
        let Some(proposal_id) = parse_proposal_id(decision_receipt_id) else {
            tracing::debug!(
                receipt_id = %decision_receipt_id,
                "dispatch-evidence sink: non-governance receipt_id; skipping"
            );
            return;
        };

        let pairs = effects.len().min(results.len());
        for i in 0..pairs {
            self.record_one(
                decision_receipt_id,
                proposal_id,
                &effects[i],
                &results[i],
                recorded_at,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_proposal_id_extracts_middle_segment() {
        assert_eq!(
            parse_proposal_id("gov:domain-a:prop-42:receipt"),
            Some("prop-42")
        );
    }

    #[test]
    fn parse_proposal_id_rejects_missing_prefix() {
        assert_eq!(parse_proposal_id("domain:prop:receipt"), None);
    }

    #[test]
    fn parse_proposal_id_rejects_missing_suffix() {
        assert_eq!(parse_proposal_id("gov:domain:prop"), None);
    }

    #[test]
    fn parse_proposal_id_rejects_empty_proposal() {
        assert_eq!(parse_proposal_id("gov:domain::receipt"), None);
    }

    #[test]
    fn effect_kind_maps_sdis_variants() {
        use icn_kernel_api::effects::SdisEffect;
        assert_eq!(
            kernel_effect_to_effect_kind(&KernelEffect::Sdis(SdisEffect::RevokeSteward {
                steward_did: "did:icn:x".into(),
                reason: "r".into(),
            })),
            Some("revoke_steward")
        );
        assert_eq!(
            kernel_effect_to_effect_kind(&KernelEffect::NoOp { reason: "x".into() }),
            None
        );
    }
}
