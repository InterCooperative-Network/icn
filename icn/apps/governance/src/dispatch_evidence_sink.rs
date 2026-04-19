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
//!
//! ## Deferred installation
//!
//! Daemon bootstrap constructs [`DeferredDispatchEvidenceSink`] *before*
//! the gateway creates the backing receipt store. The deferred sink is
//! plumbed into the kernel via `BootstrapHandles.dispatch_evidence_sink`;
//! later, the gateway calls [`DeferredDispatchEvidenceSink::install_backend`]
//! with the concrete `ReceiptStore` once it is open. Any acceptance that
//! completes before installation logs a debug line and writes no
//! evidence — honest best-effort behavior during the startup window.

use std::sync::{Arc, OnceLock};

use icn_kernel_api::effects::{
    kernel_effect_subsystem, DispatchEvidenceSink, EffectResult, KernelEffect,
};

use crate::dispatch_evidence::EffectDispatchEvidence;
use crate::receipt_backend::GovernanceReceiptBackend;

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
    //
    // Split from the RIGHT: domain_id is allowed to contain ':' (e.g.
    // `did:icn:coop-alpha`), so `split_once(':')` would grab the first
    // colon inside the domain and mis-extract proposal_id. The proposal
    // id itself must not contain ':' (enforced at proposal creation).
    let (domain, proposal) = stripped.rsplit_once(':')?;
    if domain.is_empty() || proposal.is_empty() {
        None
    } else {
        Some(proposal)
    }
}

/// Governance-side sink that persists dispatch evidence directly via a
/// [`GovernanceReceiptBackend`]. The backend is the same
/// `Arc<ReceiptStore>` the gateway installs on the governance actor, so
/// evidence written here lands in the same sled tree the gateway-close
/// hook already uses.
///
/// Kernel-clean: the kernel only sees the `DispatchEvidenceSink` trait;
/// the concrete backend and manager types are confined to this app
/// crate. No `GovernanceManager` coupling — the sink only needs the
/// two persistence operations on the backend.
pub struct GovernanceDispatchEvidenceSink {
    backend: Arc<dyn GovernanceReceiptBackend>,
}

impl GovernanceDispatchEvidenceSink {
    pub fn new(backend: Arc<dyn GovernanceReceiptBackend>) -> Self {
        Self { backend }
    }

    /// Persist evidence for one (effect, result) pair given the
    /// pre-resolved `InstitutionalEffectRecord.record_id`. Kept
    /// separate so the batch entry point can amortize the IER lookup
    /// across all pairs.
    fn write_one(
        &self,
        proposal_id: &str,
        effect_record_id: &str,
        effect_kind: &str,
        effect: &KernelEffect,
        result: &EffectResult,
        recorded_at: u64,
    ) {
        let subsystem = kernel_effect_subsystem(effect).to_string();
        let error_message = if result.success {
            None
        } else {
            Some(result.message.clone())
        };
        let evidence = EffectDispatchEvidence::new(
            effect_record_id.to_string(),
            proposal_id.to_string(),
            subsystem,
            // Honest forwarding of whatever downstream handle the executing
            // service published on `EffectResult.receipt_ref`. For SDIS
            // AppointSteward / RevokeSteward / ReconfirmSteward this is the
            // content-addressed `StewardId::to_hex()` minted by the commons
            // layer. `None` when the service layer had no downstream handle
            // to attribute (failure, no-op revoke, non-evidence-producing
            // effects). No synthesis — what you see here is what the
            // executor actually produced.
            result.receipt_ref.clone(),
            result.success,
            error_message,
            // Explicit outcome class — forwarded verbatim from the service
            // layer. `None` when the service didn't classify (non-SDIS
            // effects today); classified rows let auditors distinguish
            // applied / no_op / partial / failed without inferring from
            // success + receipt_ref + state_change_hash combinations.
            result.outcome,
            recorded_at,
        );

        if let Err(e) = self.backend.put_effect_dispatch_evidence(&evidence) {
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

        // Fetch the IER list ONCE per batch and index by effect_kind.
        // Previous implementation re-fetched per effect which scaled
        // poorly for multi-effect decisions.
        let records = match self
            .backend
            .list_institutional_effects_by_proposal(proposal_id)
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    proposal_id = %proposal_id,
                    receipt_id = %decision_receipt_id,
                    error = %e,
                    "dispatch-evidence sink: list_institutional_effects_by_proposal failed; skipping batch"
                );
                return;
            }
        };

        let pairs = effects.len().min(results.len());
        for i in 0..pairs {
            let effect = &effects[i];
            let result = &results[i];
            let Some(effect_kind) = kernel_effect_to_effect_kind(effect) else {
                // NoOp or effects that never emit an IER — nothing to link.
                continue;
            };
            let Some(rec) = records.iter().find(|r| r.effect_kind == effect_kind) else {
                tracing::debug!(
                    proposal_id = %proposal_id,
                    effect_kind = %effect_kind,
                    "dispatch-evidence sink: no institutional effect record for effect_kind; skipping"
                );
                continue;
            };
            self.write_one(
                proposal_id,
                &rec.record_id,
                effect_kind,
                effect,
                result,
                recorded_at,
            );
        }
    }
}

/// Dispatch-evidence sink with deferred backend installation.
///
/// Daemon bootstrap must provide a [`DispatchEvidenceSink`] to the
/// kernel before the gateway has opened its sled-backed `ReceiptStore`.
/// This type fills that window: construct it at bootstrap (empty),
/// hand `Arc<Self>` to the kernel as the sink, and later call
/// [`install_backend`] once the real backend is available.
///
/// Writes before installation are silent no-ops with a debug log —
/// honest best-effort semantics, not a silent data loss hole:
/// the only realistic window is the narrow slice between supervisor
/// spawn and gateway startup, during which no proposal acceptance
/// should be running anyway.
///
/// [`install_backend`]: DeferredDispatchEvidenceSink::install_backend
pub struct DeferredDispatchEvidenceSink {
    inner: OnceLock<GovernanceDispatchEvidenceSink>,
}

impl Default for DeferredDispatchEvidenceSink {
    fn default() -> Self {
        Self::new()
    }
}

impl DeferredDispatchEvidenceSink {
    pub fn new() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }

    /// Install the concrete receipt backend. Idempotent: a second call
    /// is ignored (first install wins) and logs a warning. The daemon
    /// calls this exactly once, immediately after `ReceiptStore`
    /// initialization.
    pub fn install_backend(&self, backend: Arc<dyn GovernanceReceiptBackend>) {
        let sink = GovernanceDispatchEvidenceSink::new(backend);
        if self.inner.set(sink).is_err() {
            tracing::warn!(
                "DeferredDispatchEvidenceSink::install_backend called twice; ignoring second install"
            );
        } else {
            tracing::info!(
                "DeferredDispatchEvidenceSink: backend installed; actor-path dispatch evidence is now durable"
            );
        }
    }

    /// Whether a backend has been installed. Primarily for tests and
    /// health checks.
    pub fn is_installed(&self) -> bool {
        self.inner.get().is_some()
    }
}

impl DispatchEvidenceSink for DeferredDispatchEvidenceSink {
    fn record_effects(
        &self,
        decision_receipt_id: &str,
        effects: &[KernelEffect],
        results: &[EffectResult],
        recorded_at: u64,
    ) {
        match self.inner.get() {
            Some(inner) => inner.record_effects(decision_receipt_id, effects, results, recorded_at),
            None => {
                tracing::debug!(
                    receipt_id = %decision_receipt_id,
                    effect_count = effects.len(),
                    "DeferredDispatchEvidenceSink: no backend installed yet; dropping batch"
                );
            }
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
    fn parse_proposal_id_handles_colon_containing_domain() {
        // domain_id can legitimately contain ':' (e.g. a DID).
        // rsplit_once must pick off only the trailing proposal segment.
        assert_eq!(
            parse_proposal_id("gov:did:icn:coop-alpha:prop-123:receipt"),
            Some("prop-123")
        );
    }

    #[test]
    fn parse_proposal_id_rejects_empty_domain() {
        assert_eq!(parse_proposal_id("gov::prop-1:receipt"), None);
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

    #[test]
    fn deferred_sink_drops_when_no_backend_installed() {
        let sink = DeferredDispatchEvidenceSink::new();
        assert!(!sink.is_installed());
        // Should not panic; returns silently.
        sink.record_effects("gov:d:p:receipt", &[], &[], 0);
    }
}
