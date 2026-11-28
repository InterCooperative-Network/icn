//! Governance evidence initialization.
//!
//! On a node with an identity the supervisor, not the gateway, establishes the
//! institutional evidence path that terminal governance transitions write to,
//! and settles the governance actor's evidence readiness from the outcome.
//! It runs after the governance actor is spawned, after the decision executor
//! has registered the governance effect subscription (so recovered acceptances
//! reach the executor), and before in-flight decision recovery, the RPC server,
//! or the gateway accepts requests.
//!
//! The daemon opens `<data_dir>/gateway_store` once and builds the receipt
//! store over it (whose constructor runs the legacy by-proposal index
//! migration). Over that store, in this order:
//!
//! - E3 by-grantee index backfill — before recovery, because a recovered
//!   acceptance's lifecycle (RemoveSteward, SuspendSteward, RevokeAuthority,
//!   reinstate, reconfirm) looks grants up by grantee; a stale index would
//!   skip legacy grants permanently;
//! - E4 receipt store installed on the actor (also binds decision commitments);
//! - E5 receipt store installed as the dispatch-evidence backend — before
//!   recovery, because the deferred sink drops evidence until installed;
//! - E6 close-journal recovery, which sets the actor `Ready`;
//! - E7 #1987 dispatch-evidence backfill over the execution-record store;
//! - E8 execution-record retention cleanup, only if E7 succeeded, so pruning
//!   cannot delete a terminal record whose evidence has not been healed.
//!
//! Readiness is keyed to this outcome alone: `Established` leaves governance
//! `Ready` whatever later happens to the gateway or its HTTP bind; `Failed`
//! holds scheduled closes and refuses terminal transitions. A journal that
//! cannot be scanned is still `Established`, with the unknown residual
//! surfaced by log and metric; it is replayed at the next startup.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use icn_governance_actor::dispatch_evidence_backfill::{
    backfill_pending_dispatch_evidence, EvidenceBackfillReport,
};
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
use icn_governance_actor::{
    CloseRecoveryReport, DeferredDispatchEvidenceSink, EvidenceReadiness, GovernanceHandle,
};

/// Idempotent rebuild of the receipt store's ADR-0014 by-grantee grant index.
/// Returns the number of index entries written.
pub type ByGranteeIndexBackfill = Arc<dyn Fn() -> Result<usize, String> + Send + Sync>;

/// The governance view of the daemon's single receipt store.
#[derive(Clone)]
pub struct GovernanceEvidenceStore {
    /// The receipt store, installed on the actor and the dispatch-evidence sink.
    pub receipt_backend: Arc<dyn GovernanceReceiptBackend>,
    /// The receipt store's by-grantee index backfill (E3).
    pub backfill_by_grantee_index: ByGranteeIndexBackfill,
}

/// What the daemon injects: one open of `<data_dir>/gateway_store`, seen as
/// the governance evidence store by the supervisor and as the concrete shared
/// store by the gateway.
#[derive(Clone)]
pub struct GovernanceStoreInjection {
    /// Governance view, consumed by [`init_governance_evidence`].
    pub evidence: GovernanceEvidenceStore,
    /// Gateway view of the same database and receipt store.
    pub gateway: icn_gateway::shared_store::SharedGatewayStore,
}

/// Inputs to [`init_governance_evidence`].
pub struct GovernanceEvidenceInputs {
    /// The spawned governance actor.
    pub governance: GovernanceHandle,
    /// The injected store: `None` when the daemon injected none, `Err` when
    /// opening it failed.
    pub store: Option<Result<GovernanceEvidenceStore, String>>,
    /// The deferred dispatch-evidence sink the decision executor writes through.
    pub dispatch_evidence_sink: Option<Arc<DeferredDispatchEvidenceSink>>,
    /// The decision executor's execution-record store (E7 scans it).
    pub execution_store: Option<Arc<dyn icn_store::Store>>,
    /// Execution-record retention cleanup (E8).
    pub post_backfill_cleanup: Option<Box<dyn FnOnce() + Send>>,
}

/// How governance evidence initialization ended.
#[derive(Debug)]
pub enum GovernanceEvidenceInit {
    /// The evidence path is installed and close recovery has run.
    Established(EstablishedEvidence),
    /// No evidence path: governance is held.
    Failed(EvidenceInitFailure),
}

/// What an established evidence initialization did.
#[derive(Debug)]
pub struct EstablishedEvidence {
    /// E3: by-grantee index entries written, or why the backfill failed
    /// (non-fatal; logged).
    pub by_grantee_backfill: Result<usize, String>,
    /// E6: the close-journal replay.
    pub close_recovery: CloseRecoveryReport,
    /// E7: the dispatch-evidence backfill; `None` when no execution store or
    /// dispatch sink was provided.
    pub dispatch_evidence_backfill: Option<Result<EvidenceBackfillReport, String>>,
    /// E8: whether the retention cleanup ran.
    pub retention_cleanup_ran: bool,
}

/// Why no evidence path was established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceInitFailure {
    /// The daemon could not open the store (for example: locked by another holder).
    StoreOpen(String),
    /// The daemon injected no store.
    NotInjected,
}

/// Run E3–E8 over the injected store. Does not settle readiness on failure;
/// see [`settle_governance_evidence`] and [`establish_governance_evidence`].
pub async fn init_governance_evidence(inputs: GovernanceEvidenceInputs) -> GovernanceEvidenceInit {
    let GovernanceEvidenceInputs {
        governance,
        store,
        dispatch_evidence_sink,
        execution_store,
        post_backfill_cleanup,
    } = inputs;
    let store = match store {
        None => return GovernanceEvidenceInit::Failed(EvidenceInitFailure::NotInjected),
        Some(Err(cause)) => {
            return GovernanceEvidenceInit::Failed(EvidenceInitFailure::StoreOpen(cause))
        }
        Some(Ok(store)) => store,
    };

    // E3: by-grantee index backfill, before recovery reads the index.
    // Synchronous sled work runs on the blocking pool; it is awaited before E4.
    let backfill = store.backfill_by_grantee_index.clone();
    let by_grantee_backfill = blocking_step("by-grantee index backfill", move || backfill())
        .await
        .and_then(|written| written);
    match &by_grantee_backfill {
        Ok(0) => debug!("ADR-0014 by-grantee index backfill: no legacy grants found"),
        Ok(written) => info!(
            written,
            "ADR-0014 by-grantee index backfill: recovered legacy grants"
        ),
        Err(e) => error!(
            error = %e,
            "ADR-0014 by-grantee index backfill failed at startup; legacy grants may be \
             invisible to by-grantee readers (including recovered acceptance lifecycles) \
             until the next successful run"
        ),
    }

    // E4: receipt store on the actor (actor-path closes write their evidence).
    governance
        .install_receipt_store(store.receipt_backend.clone())
        .await;
    info!("Receipt store installed on governance actor");

    // E5: dispatch-evidence backend, before recovery can drive effects.
    if let Some(sink) = dispatch_evidence_sink.as_ref() {
        sink.install_backend(store.receipt_backend.clone());
        info!("Dispatch-evidence sink backend installed");
    }

    // E6: replay incomplete closes; the actor becomes Ready.
    let close_recovery = governance.recover_incomplete_closes().await;
    surface_close_recovery(&close_recovery);

    // E7: heal dispatch evidence lost in the async execution window. Blocking
    // pool, awaited before E8.
    let dispatch_evidence_backfill = match (execution_store, dispatch_evidence_sink) {
        (Some(execution_store), Some(sink)) => {
            let result = blocking_step("dispatch-evidence backfill", move || {
                backfill_pending_dispatch_evidence(&execution_store, &sink)
                    .map_err(|e| e.to_string())
            })
            .await
            .and_then(|report| report);
            match &result {
                Ok(report) => info!(
                    scanned = report.scanned,
                    redriven = report.redriven,
                    skipped = report.skipped,
                    unparsable = report.unparsable,
                    "Dispatch-evidence backfill complete"
                ),
                Err(e) => warn!(
                    error = %e,
                    "Dispatch-evidence backfill failed; evidence may remain incomplete \
                     until the next startup, and retention cleanup is skipped"
                ),
            }
            Some(result)
        }
        _ => None,
    };

    // E8: retention cleanup only after a successful backfill. Blocking pool.
    let retention_cleanup_ran = match (&dispatch_evidence_backfill, post_backfill_cleanup) {
        (Some(Ok(_)), Some(cleanup)) => match blocking_step("retention cleanup", cleanup).await {
            Ok(()) => true,
            Err(e) => {
                warn!(error = %e, "Execution-record retention cleanup did not complete");
                false
            }
        },
        (_, Some(_)) => {
            info!("Execution-record retention cleanup skipped: dispatch-evidence backfill did not succeed");
            false
        }
        (_, None) => false,
    };

    GovernanceEvidenceInit::Established(EstablishedEvidence {
        by_grantee_backfill,
        close_recovery,
        dispatch_evidence_backfill,
        retention_cleanup_ran,
    })
}

/// Run one synchronous startup step on the blocking pool and wait for it. A
/// panic or cancellation inside the step is returned as that step's failure;
/// it never unwinds into the supervisor.
async fn blocking_step<T, F>(step: &'static str, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("{step} did not complete: {e}"))
}

/// Log each incomplete journaled close and publish the recovery metrics.
fn surface_close_recovery(report: &CloseRecoveryReport) {
    for residual in &report.residual {
        error!(
            proposal_id = %residual.proposal_id,
            cause = %residual.cause,
            "Journaled governance close did not complete at startup; it stays in the \
             journal for the next replay"
        );
    }
    if let Some(e) = &report.scan_error {
        error!(
            error = %e,
            "Governance close journal could not be scanned at startup; incomplete closes, \
             if any, are unknown and will be replayed at the next startup"
        );
    }
    icn_obs::metrics::governance::close_recovery_residual_set(report.residual.len());
    icn_obs::metrics::governance::close_recovery_completed_inc(report.completed);
    icn_obs::metrics::governance::close_recovery_scan_failed_set(report.scan_error.is_some());
    info!(
        scanned = report.scanned,
        completed = report.completed,
        residual = report.residual.len(),
        scan_failed = report.scan_error.is_some(),
        "Governance close-journal recovery complete"
    );
}

/// Settle the actor's evidence readiness from the evidence-init outcome.
///
/// `Established`: recovery already set `Ready`; this reads it back.
/// `Failed`: hold scheduled closes and refuse terminal transitions.
pub fn settle_governance_evidence(
    init: &GovernanceEvidenceInit,
    governance: &GovernanceHandle,
) -> EvidenceReadiness {
    match init {
        GovernanceEvidenceInit::Established(_) => governance.evidence_readiness(),
        GovernanceEvidenceInit::Failed(cause) => {
            let state = governance.mark_evidence_unavailable();
            icn_obs::metrics::supervisor::error_inc("governance_evidence_unavailable");
            error!(
                ?cause,
                ?state,
                "Governance evidence storage was not established: scheduled governance work \
                 is held and close/veto/force-close are refused until a restart with a working \
                 evidence store"
            );
            state
        }
    }
}

/// Proof that governance evidence initialization has completed and the
/// actor's readiness has been settled (`Ready` or `Failed`).
///
/// Only [`establish_governance_evidence`] constructs it, and only after its
/// whole sequence (E3–E8 and settlement) has been awaited. The ingress
/// starters require it: `init_rpc::spawn_rpc_server` takes it by reference
/// (so on the identity path the RPC server cannot be started before evidence
/// init completes), and `init_gateway::spawn_gateway` refuses to start a
/// gateway that has a governance actor but no settlement.
///
/// It does not identify which actor it settled; the supervisor uses one
/// actor per node.
#[derive(Debug)]
pub struct GovernanceEvidenceSettled {
    readiness: EvidenceReadiness,
}

impl GovernanceEvidenceSettled {
    /// The readiness settled by evidence initialization.
    pub fn readiness(&self) -> EvidenceReadiness {
        self.readiness
    }

    /// Whether in-flight decision recovery must defer records that drive the
    /// actor's terminal transitions (veto / force-close): true unless
    /// evidence is `Ready`.
    pub fn defers_actor_control_recovery(&self) -> bool {
        self.readiness != EvidenceReadiness::Ready
    }
}

impl PartialEq<EvidenceReadiness> for GovernanceEvidenceSettled {
    fn eq(&self, other: &EvidenceReadiness) -> bool {
        self.readiness == *other
    }
}

/// Initialize governance evidence and settle readiness. The supervisor's step.
pub async fn establish_governance_evidence(
    inputs: GovernanceEvidenceInputs,
) -> (GovernanceEvidenceInit, GovernanceEvidenceSettled) {
    let governance = inputs.governance.clone();
    let init = init_governance_evidence(inputs).await;
    let readiness = settle_governance_evidence(&init, &governance);
    (init, GovernanceEvidenceSettled { readiness })
}

/// Split the daemon's injected store into the governance view (for
/// [`init_governance_evidence`]) and the gateway view. An open failure goes
/// to evidence init only (which then reports `Failed(StoreOpen)`); the
/// gateway gets no store and, having an actor, refuses to start. Never
/// fails, so a store open failure cannot stop the node.
pub fn split_governance_store(
    injected: Option<Result<GovernanceStoreInjection, String>>,
) -> (
    Option<Result<GovernanceEvidenceStore, String>>,
    Option<icn_gateway::shared_store::SharedGatewayStore>,
) {
    match injected {
        None => (None, None),
        Some(Err(cause)) => (Some(Err(cause)), None),
        Some(Ok(injection)) => (Some(Ok(injection.evidence)), Some(injection.gateway)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M-C1 S2a: only Ready admits in-flight recovery of veto/force-close
    /// decisions. The settled production path returns Ready or Failed, and
    /// other readiness values remain fail-closed.
    #[test]
    fn actor_control_recovery_is_deferred_unless_evidence_admits_it() {
        for (readiness, defers) in [
            (EvidenceReadiness::Pending, true),
            (EvidenceReadiness::Ready, false),
            (EvidenceReadiness::NotProvisioned, true),
            (EvidenceReadiness::Failed, true),
        ] {
            let settled = GovernanceEvidenceSettled { readiness };
            assert_eq!(
                settled.defers_actor_control_recovery(),
                defers,
                "{readiness:?}"
            );
        }
    }
}
