//! SDIS steward service adapter.
//!
//! Implements `SdisService` backed by `icn_commons::CommonsHandle`.
//! The executor in `governance_executor.rs` calls this through the kernel-api
//! trait boundary; it never imports CommonsHandle directly.

use anyhow::Result;
use icn_kernel_api::{
    AppointStewardRequest, AppointStewardResult, ReconfirmStewardRequest, ReconfirmStewardResult,
    ReinstateStewardRequest, ReinstateStewardResult, RevokeStewardRequest, RevokeStewardResult,
    SanctionStewardRequest, SanctionStewardResult, SdisService, SuspendStewardRequest,
    SuspendStewardResult,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};

pub struct SdisServiceImpl {
    commons: Arc<icn_commons::CommonsHandle>,
}

impl SdisServiceImpl {
    pub fn new(commons: Arc<icn_commons::CommonsHandle>) -> Self {
        Self { commons }
    }

    fn compute_appoint_hash(request: &AppointStewardRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"sdis:appoint:");
        hasher.update(request.steward_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.jurisdiction_id.as_bytes());
        hasher.update(b":");
        hasher.update(request.proposal_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn compute_revoke_hash(request: &RevokeStewardRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"sdis:revoke:");
        hasher.update(request.steward_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.reason.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn compute_reconfirm_hash(request: &ReconfirmStewardRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"sdis:reconfirm:");
        hasher.update(request.steward_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.new_term_end.to_le_bytes());
        hasher.update(b":");
        hasher.update(request.proposal_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn compute_reinstate_hash(request: &ReinstateStewardRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"sdis:reinstate:");
        hasher.update(request.steward_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.proposal_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn compute_suspend_hash(request: &SuspendStewardRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"sdis:suspend:");
        hasher.update(request.steward_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.proposal_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl SdisService for SdisServiceImpl {
    fn appoint_steward(&self, request: AppointStewardRequest) -> Result<AppointStewardResult> {
        info!(
            steward_did = %request.steward_did,
            jurisdiction_id = %request.jurisdiction_id,
            term_length_seconds = %request.term_length_seconds,
            bond_amount = %request.bond_amount,
            proposal_id = %request.proposal_id,
            "Appointing steward via governance dispatch"
        );

        let term_days = (request.term_length_seconds / 86_400) as u64;
        let bond = request.bond_amount.max(0) as u64;
        let steward_did = icn_identity::Did::from_str(&request.steward_did)
            .map_err(|e| anyhow::anyhow!("Invalid steward DID '{}': {}", request.steward_did, e))?;
        // Empty jurisdiction_id means no scoped jurisdiction — treated as None (global steward).
        let jurisdiction = if request.jurisdiction_id.is_empty() {
            None
        } else {
            Some(request.jurisdiction_id.clone())
        };

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.commons
                    .register_steward(
                        &steward_did,
                        &steward_did,
                        term_days,
                        bond,
                        request.proposal_id.clone(),
                        jurisdiction,
                        vec![],
                    )
                    .await
            })
        });

        match result {
            Ok(record) => {
                let state_change_hash = Self::compute_appoint_hash(&request);
                let steward_id = record.id().to_hex();
                info!(
                    steward_did = %request.steward_did,
                    state_change_hash = %state_change_hash,
                    steward_id = %steward_id,
                    "Steward appointed and registered in commons"
                );
                Ok(AppointStewardResult {
                    success: true,
                    state_change_hash,
                    error: None,
                    receipt_ref: Some(steward_id),
                })
            }
            Err(e) => {
                tracing::warn!(
                    steward_did = %request.steward_did,
                    error = %e,
                    "Failed to appoint steward in commons"
                );
                Ok(AppointStewardResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                    receipt_ref: None,
                })
            }
        }
    }

    fn revoke_steward(&self, request: RevokeStewardRequest) -> Result<RevokeStewardResult> {
        info!(
            steward_did = %request.steward_did,
            reason = %request.reason,
            "Revoking steward via governance dispatch"
        );

        let steward_did = icn_identity::Did::from_str(&request.steward_did)
            .map_err(|e| anyhow::anyhow!("Invalid steward DID '{}': {}", request.steward_did, e))?;

        // Returns `Some(steward_id)` when the revoke was routed through a
        // real commons handle; `None` when no active record existed and this
        // is an idempotent no-op. The inner `Result` disambiguates success
        // from commons failure — we do not need a separate bool.
        let result: Result<Option<String>> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons
                            .revoke_steward(&steward_id, request.reason.clone(), vec![])
                            .await
                            .map(|_| Some(steward_id))
                    }
                    Ok(None) => {
                        // No record found — idempotent no-op. No steward_id
                        // to attribute to this revoke.
                        tracing::debug!(
                            steward_did = %request.steward_did,
                            "RevokeSteward: no active steward record found, treating as no-op"
                        );
                        Ok(None)
                    }
                    Err(e) => Err(e),
                }
            })
        });

        match result {
            Ok(receipt_ref) => {
                // Only count a real revoke as state-changing. The no-op
                // (no active record) branch keeps `state_change_hash`
                // empty so downstream audit surfaces can distinguish a
                // genuine revocation from an idempotent repeat.
                if receipt_ref.is_some() {
                    let state_change_hash = Self::compute_revoke_hash(&request);
                    info!(
                        steward_did = %request.steward_did,
                        state_change_hash = %state_change_hash,
                        receipt_ref = ?receipt_ref,
                        "Steward revoked in commons"
                    );
                    Ok(RevokeStewardResult {
                        success: true,
                        state_change_hash,
                        error: None,
                        receipt_ref,
                    })
                } else {
                    info!(
                        steward_did = %request.steward_did,
                        "Steward revoke was a no-op in commons (no active record)"
                    );
                    Ok(RevokeStewardResult {
                        success: true,
                        state_change_hash: String::new(),
                        error: None,
                        receipt_ref: None,
                    })
                }
            }
            Err(e) => {
                tracing::warn!(
                    steward_did = %request.steward_did,
                    error = %e,
                    "Failed to revoke steward in commons"
                );
                Ok(RevokeStewardResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                    receipt_ref: None,
                })
            }
        }
    }

    fn reconfirm_steward(
        &self,
        request: ReconfirmStewardRequest,
    ) -> Result<ReconfirmStewardResult> {
        info!(
            steward_did = %request.steward_did,
            new_term_end = %request.new_term_end,
            proposal_id = %request.proposal_id,
            "Reconfirming steward term via governance dispatch"
        );

        let steward_did = icn_identity::Did::from_str(&request.steward_did)
            .map_err(|e| anyhow::anyhow!("Invalid steward DID '{}': {}", request.steward_did, e))?;

        let result: Result<Option<String>> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons
                            .extend_steward_term(&steward_id, request.new_term_end)
                            .await
                            .map(|_| Some(steward_id))
                    }
                    Ok(None) => {
                        tracing::warn!(
                            steward_did = %request.steward_did,
                            "ReconfirmSteward: no active steward record found"
                        );
                        Err(anyhow::anyhow!(
                            "Steward '{}' not found — cannot reconfirm",
                            request.steward_did
                        ))
                    }
                    Err(e) => Err(e),
                }
            })
        });

        match result {
            Ok(receipt_ref) => {
                let state_change_hash = Self::compute_reconfirm_hash(&request);
                info!(
                    steward_did = %request.steward_did,
                    new_term_end = %request.new_term_end,
                    state_change_hash = %state_change_hash,
                    receipt_ref = ?receipt_ref,
                    "Steward term extended in commons"
                );
                Ok(ReconfirmStewardResult {
                    success: true,
                    state_change_hash,
                    error: None,
                    receipt_ref,
                })
            }
            Err(e) => {
                tracing::warn!(
                    steward_did = %request.steward_did,
                    error = %e,
                    "Failed to reconfirm steward in commons"
                );
                Ok(ReconfirmStewardResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                    receipt_ref: None,
                })
            }
        }
    }

    fn reinstate_steward(
        &self,
        request: ReinstateStewardRequest,
    ) -> Result<ReinstateStewardResult> {
        info!(
            steward_did = %request.steward_did,
            proposal_id = %request.proposal_id,
            "Reinstating steward via governance dispatch"
        );

        let steward_did = icn_identity::Did::from_str(&request.steward_did)
            .map_err(|e| anyhow::anyhow!("Invalid steward DID '{}': {}", request.steward_did, e))?;

        // Returns (steward_id, was_suspended): capture the commons handle
        // the reinstate was routed through so we can forward it into
        // durable dispatch evidence. Populated on both active and no-op
        // paths since the commons call succeeded against a real record;
        // `state_change_hash` still distinguishes the two.
        let result: Result<(String, bool)> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        let was_suspended = self.commons.reinstate_steward(&steward_id).await?;
                        Ok((steward_id, was_suspended))
                    }
                    Ok(None) => {
                        tracing::warn!(
                            steward_did = %request.steward_did,
                            "ReinstateSteward: no steward record found"
                        );
                        Err(anyhow::anyhow!(
                            "Steward '{}' not found — cannot reinstate",
                            request.steward_did
                        ))
                    }
                    Err(e) => Err(e),
                }
            })
        });

        match result {
            Ok((steward_id, was_suspended)) => {
                let state_change_hash = if was_suspended {
                    let hash = Self::compute_reinstate_hash(&request);
                    info!(
                        steward_did = %request.steward_did,
                        state_change_hash = %hash,
                        receipt_ref = %steward_id,
                        "Suspended steward reinstated in commons"
                    );
                    hash
                } else {
                    info!(
                        steward_did = %request.steward_did,
                        receipt_ref = %steward_id,
                        "ReinstateSteward no-op: steward was not suspended"
                    );
                    String::new()
                };
                Ok(ReinstateStewardResult {
                    success: true,
                    was_suspended,
                    state_change_hash,
                    error: None,
                    receipt_ref: Some(steward_id),
                })
            }
            Err(e) => {
                tracing::warn!(
                    steward_did = %request.steward_did,
                    error = %e,
                    "Failed to reinstate steward in commons"
                );
                Ok(ReinstateStewardResult {
                    success: false,
                    was_suspended: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                    receipt_ref: None,
                })
            }
        }
    }

    fn suspend_steward(&self, request: SuspendStewardRequest) -> Result<SuspendStewardResult> {
        info!(
            steward_did = %request.steward_did,
            proposal_id = %request.proposal_id,
            duration_seconds = %request.duration_seconds,
            "Suspending steward via governance dispatch \
             (duration advisory — timed reinstatement not enforced)"
        );
        let steward_did = icn_identity::Did::from_str(&request.steward_did)
            .map_err(|e| anyhow::anyhow!("Invalid steward DID '{}': {}", request.steward_did, e))?;
        // Returns the steward_id the suspend was routed through, so it
        // can be forwarded into durable dispatch evidence.
        let result: Result<String> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons
                            .suspend_steward(&steward_id, request.reason.clone())
                            .await?;
                        Ok(steward_id)
                    }
                    Ok(None) => Err(anyhow::anyhow!(
                        "Steward '{}' not found — cannot suspend",
                        request.steward_did
                    )),
                    Err(e) => Err(e),
                }
            })
        });
        match result {
            Ok(steward_id) => {
                let hash = Self::compute_suspend_hash(&request);
                info!(
                    steward_did = %request.steward_did,
                    state_change_hash = %hash,
                    receipt_ref = %steward_id,
                    "Steward suspended in commons"
                );
                Ok(SuspendStewardResult {
                    success: true,
                    state_change_hash: hash,
                    error: None,
                    receipt_ref: Some(steward_id),
                })
            }
            Err(e) => {
                warn!(
                    steward_did = %request.steward_did,
                    error = %e,
                    "Failed to suspend steward in commons"
                );
                Ok(SuspendStewardResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                    receipt_ref: None,
                })
            }
        }
    }

    fn sanction_steward(&self, request: SanctionStewardRequest) -> Result<SanctionStewardResult> {
        info!(
            steward_did = %request.steward_did,
            bond_slash_amount = %request.bond_slash_amount,
            proposal_id = %request.proposal_id,
            "Sanctioning steward via governance dispatch (bond slash)"
        );
        let steward_did = icn_identity::Did::from_str(&request.steward_did)
            .map_err(|e| anyhow::anyhow!("Invalid steward DID '{}': {}", request.steward_did, e))?;

        // Sanction is a two-step mutation: (1) slash_steward_bond, then optionally
        // (2) suspend_steward. Step 1 is irreversible once committed. If step 2
        // fails after step 1 has persisted, we MUST preserve the downstream handle
        // (receipt_ref) and state_change_hash — otherwise dispatch evidence would
        // hide a real commons mutation behind `success=false, receipt_ref=None`.
        //
        // We therefore split the async sequence so the pre-slash failure path and
        // the post-slash partial-failure path are distinguishable to the caller.
        let pre_slash = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let record = self
                    .commons
                    .get_steward_by_did(&steward_did)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Steward '{}' not found — cannot sanction",
                            request.steward_did
                        )
                    })?;
                let steward_id = record.id().to_hex();
                // Idempotent per governance decision (the sanction proposal's
                // receipt id): a crash-recovery re-dispatch of the same
                // SanctionSteward effect must not slash the bond twice, while
                // distinct decisions still slash independently.
                let remaining = self
                    .commons
                    .slash_steward_bond_for_decision(
                        &steward_id,
                        request.bond_slash_amount,
                        &request.proposal_id,
                    )
                    .await?;
                Ok::<_, anyhow::Error>((steward_id, remaining))
            })
        });

        let (steward_id, remaining_bond) = match pre_slash {
            Ok(pair) => pair,
            Err(e) => {
                warn!(
                    steward_did = %request.steward_did,
                    error = %e,
                    "Failed to sanction steward (pre-slash); no durable mutation"
                );
                return Ok(SanctionStewardResult {
                    success: false,
                    remaining_bond: 0,
                    suspended: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                    receipt_ref: None,
                });
            }
        };

        // Bond slash committed; compute state_change_hash once. From here on,
        // receipt_ref and state_change_hash MUST be preserved in every return
        // path because commons state has already changed durably.
        let mut hasher = Sha256::new();
        hasher.update(b"sdis:sanction:");
        hasher.update(request.steward_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.proposal_id.as_bytes());
        hasher.update(b":");
        hasher.update(request.bond_slash_amount.to_le_bytes());
        let state_change_hash = format!("{:x}", hasher.finalize());

        // Step 2: optional suspend. Any failure here is a partial mutation.
        if !request.suspend_reason.is_empty() {
            let suspend_res = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    self.commons
                        .suspend_steward(&steward_id, request.suspend_reason.clone())
                        .await
                })
            });
            match suspend_res {
                Ok(()) => {
                    info!(
                        steward_did = %request.steward_did,
                        remaining_bond = %remaining_bond,
                        suspended = true,
                        state_change_hash = %state_change_hash,
                        receipt_ref = %steward_id,
                        "Steward sanctioned (bond slashed + suspended) in commons"
                    );
                    Ok(SanctionStewardResult {
                        success: true,
                        remaining_bond,
                        suspended: true,
                        state_change_hash,
                        error: None,
                        receipt_ref: Some(steward_id),
                    })
                }
                Err(e) => {
                    // Partial mutation: slash persisted, suspend failed.
                    // Preserve receipt_ref + state_change_hash for honest audit.
                    warn!(
                        steward_did = %request.steward_did,
                        remaining_bond = %remaining_bond,
                        error = %e,
                        state_change_hash = %state_change_hash,
                        receipt_ref = %steward_id,
                        "Sanction partially applied: bond slashed, suspend failed; preserving receipt_ref"
                    );
                    Ok(SanctionStewardResult {
                        success: false,
                        remaining_bond,
                        suspended: false,
                        state_change_hash,
                        error: Some(format!("bond slashed but suspend failed: {e}")),
                        receipt_ref: Some(steward_id),
                    })
                }
            }
        } else {
            info!(
                steward_did = %request.steward_did,
                remaining_bond = %remaining_bond,
                suspended = false,
                state_change_hash = %state_change_hash,
                receipt_ref = %steward_id,
                "Steward sanctioned (bond slashed) in commons"
            );
            Ok(SanctionStewardResult {
                success: true,
                remaining_bond,
                suspended: false,
                state_change_hash,
                error: None,
                receipt_ref: Some(steward_id),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn test_did(seed: u8) -> icn_identity::Did {
        let signing_key = SigningKey::from_bytes(&[seed; 32]);
        icn_identity::Did::from_public_key(&signing_key.verifying_key())
    }

    async fn create_strong_holder(
        commons: &icn_commons::CommonsHandle,
        holder: &icn_identity::Did,
        sponsor: &icn_identity::Did,
    ) {
        let anchor = commons
            .create_anchor_from_enrollment(holder, Some(sponsor))
            .await
            .expect("create anchor");
        let anchor_id = anchor.id_hex();
        commons
            .get_or_create_holder(&anchor_id, holder, None)
            .await
            .expect("create holder");
    }

    fn make_service_with_commons() -> (SdisServiceImpl, Arc<icn_commons::CommonsHandle>) {
        let commons = Arc::new(icn_commons::CommonsHandle::new_in_memory());
        let svc = SdisServiceImpl::new(commons.clone());
        (svc, commons)
    }

    /// AppointSteward → durable steward record exists in commons.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_appoint_steward_durable_record() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(1);
        let sponsor = test_did(2);

        // Commons setup: holder needs Strong POP for steward registration.
        // Empty jurisdiction_id → global steward (no charter required).
        create_strong_holder(&commons, &holder, &sponsor).await;

        let request = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86400 * 365,
            bond_amount: 1000,
            region: Some("northeast".to_string()),
            proposal_id: "gov:coop-alpha:prop-001:receipt".to_string(),
        };

        let result = svc.appoint_steward(request.clone()).unwrap();

        assert!(result.success, "appoint_steward should succeed");
        assert!(
            !result.state_change_hash.is_empty(),
            "state_change_hash must be non-empty"
        );
        assert!(result.error.is_none());

        // Verify the steward record is now present in commons
        let record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("get_steward_by_did must not error")
            .expect("steward must be present in commons after appointment");

        // Evidence-fidelity seam: the service must publish the commons
        // StewardId::to_hex() as receipt_ref so the dispatch-evidence sink
        // can persist it verbatim (see
        // `apps/governance/tests/actor_path_dispatch_evidence_sink.rs`).
        let expected_receipt_ref = record.id().to_hex();
        assert_eq!(
            result.receipt_ref,
            Some(expected_receipt_ref),
            "AppointStewardResult.receipt_ref must equal the commons StewardId::to_hex() \
             so kernel-path dispatch evidence carries the real downstream handle"
        );
    }

    /// SanctionSteward is idempotent per governance decision: re-dispatching the
    /// same sanction (same `proposal_id`) — as crash recovery does — slashes the
    /// bond only once, while a distinct decision slashes again. Guards the
    /// replay window where a hard crash leaves the execution record non-terminal
    /// and `recover_in_flight` re-runs the effect.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sanction_steward_idempotent_per_decision() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(40);
        let sponsor = test_did(41);
        create_strong_holder(&commons, &holder, &sponsor).await;

        svc.appoint_steward(AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86400 * 365,
            bond_amount: 1000,
            region: None,
            proposal_id: "gov:coop:appoint:receipt".to_string(),
        })
        .unwrap();

        let sanction = |proposal_id: &str| SanctionStewardRequest {
            steward_did: holder.to_string(),
            bond_slash_amount: 300,
            suspend_reason: String::new(),
            reason: "misbehavior".to_string(),
            proposal_id: proposal_id.to_string(),
        };
        let bond_now = |c: Arc<icn_commons::CommonsHandle>, did: icn_identity::Did| async move {
            c.get_steward_by_did(&did)
                .await
                .expect("get_steward_by_did must not error")
                .expect("steward present")
                .bond_amount
        };

        // First dispatch of decision A slashes 300 (1000 -> 700).
        assert!(
            svc.sanction_steward(sanction("gov:coop:sanction-A:receipt"))
                .unwrap()
                .success
        );
        assert_eq!(bond_now(commons.clone(), holder.clone()).await, 700);

        // Re-dispatch of the SAME decision A (crash-recovery replay) must NOT
        // slash again — the bond stays 700.
        assert!(
            svc.sanction_steward(sanction("gov:coop:sanction-A:receipt"))
                .unwrap()
                .success
        );
        assert_eq!(
            bond_now(commons.clone(), holder.clone()).await,
            700,
            "same sanction decision must not double-slash the bond on replay"
        );

        // A DISTINCT decision B still slashes (700 -> 400).
        assert!(
            svc.sanction_steward(sanction("gov:coop:sanction-B:receipt"))
                .unwrap()
                .success
        );
        assert_eq!(
            bond_now(commons.clone(), holder.clone()).await,
            400,
            "a distinct sanction decision must still slash"
        );
    }

    /// AppointSteward → RevokeSteward → steward record removed from commons.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_revoke_steward_removes_durable_record() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(3);
        let sponsor = test_did(4);

        create_strong_holder(&commons, &holder, &sponsor).await;
        // No jurisdiction — None path bypasses charter requirement
        let steward_did_str = holder.to_string();

        let appoint_req = AppointStewardRequest {
            steward_did: steward_did_str.clone(),
            jurisdiction_id: String::new(), // empty → None in adapter
            term_length_seconds: 86400 * 180,
            bond_amount: 500,
            region: None,
            proposal_id: "gov:coop-beta:prop-002:receipt".to_string(),
        };
        let appoint = svc.appoint_steward(appoint_req).unwrap();
        assert!(appoint.success, "appoint must succeed");
        let appoint_receipt_ref = appoint
            .receipt_ref
            .clone()
            .expect("AppointStewardResult must publish receipt_ref on success");

        // Confirm presence before revoke
        let record_before = commons
            .get_steward_by_did(&holder)
            .await
            .unwrap()
            .expect("steward must exist before revocation");
        assert_eq!(
            record_before.id().to_hex(),
            appoint_receipt_ref,
            "appoint receipt_ref must match the commons StewardId of the record the service created"
        );

        let revoke_req = RevokeStewardRequest {
            steward_did: steward_did_str.clone(),
            reason: "Governance decision".to_string(),
        };
        let revoke = svc.revoke_steward(revoke_req).unwrap();
        assert!(revoke.success, "revoke_steward should succeed");
        assert!(!revoke.state_change_hash.is_empty());
        // Evidence-fidelity seam: revoke publishes the same steward_id it
        // routed through — the handle the revoke operation actually used.
        assert_eq!(
            revoke.receipt_ref,
            Some(appoint_receipt_ref),
            "RevokeStewardResult.receipt_ref must be the steward_id the revoke was routed through"
        );

        // Idempotent second revoke must succeed. Because commons retains
        // revoked records (get_steward_by_did does not filter by active
        // status), the service still has a real handle to attribute the
        // repeat revoke to — so receipt_ref remains Some(steward_id).
        // The honestly-None path is exercised by
        // `test_revoke_missing_steward_is_noop_with_no_receipt_ref` below,
        // where no record ever existed.
        let idempotent = svc
            .revoke_steward(RevokeStewardRequest {
                steward_did: steward_did_str,
                reason: "duplicate revoke".to_string(),
            })
            .unwrap();
        assert!(
            idempotent.success,
            "second revoke is a no-op and must succeed"
        );
        assert!(
            idempotent.receipt_ref.is_some(),
            "idempotent repeat revoke still routes through the retained record; \
             receipt_ref must still point at that steward_id"
        );
    }

    /// RevokeSteward against a DID that was never registered → no active
    /// record → service returns success (idempotent no-op) but must truthfully
    /// leave `receipt_ref` as `None` because there is no downstream handle
    /// to attribute.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_revoke_missing_steward_is_noop_with_no_receipt_ref() {
        let (svc, _commons) = make_service_with_commons();
        let unknown = test_did(77);

        let revoke = svc
            .revoke_steward(RevokeStewardRequest {
                steward_did: unknown.to_string(),
                reason: "never appointed".to_string(),
            })
            .unwrap();

        assert!(
            revoke.success,
            "revoke of a non-existent steward must succeed as an idempotent no-op"
        );
        assert_eq!(
            revoke.receipt_ref, None,
            "no active record → no handle to attribute → receipt_ref must remain None"
        );
        assert!(
            revoke.state_change_hash.is_empty(),
            "no-op revoke must not claim a state_change_hash — audit must be able \
             to distinguish a real revocation from an idempotent no-op"
        );
        assert!(
            revoke.error.is_none(),
            "no-op revoke is a success, not a failure; error must be None"
        );
    }

    /// ReconfirmSteward → `term_end` is extended in the durable commons record.
    ///
    /// Proves the full ReconfirmSteward dispatch chain:
    /// `SdisService::reconfirm_steward` → `CommonsHandle::extend_steward_term`
    /// → durable mutation visible via `get_steward_by_did`.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_reconfirm_steward_extends_term() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(5);
        let sponsor = test_did(6);

        // Commons setup: appoint a steward with a known initial term.
        create_strong_holder(&commons, &holder, &sponsor).await;
        let initial_term_days: u64 = 180;
        let initial_term_end_secs: u64 = initial_term_days * 86_400;

        let appoint_req = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(), // global steward (no charter required)
            term_length_seconds: initial_term_end_secs as i64,
            bond_amount: 500,
            region: None,
            proposal_id: "gov:coop-gamma:prop-010:receipt".to_string(),
        };
        let appoint = svc.appoint_steward(appoint_req).unwrap();
        assert!(appoint.success, "initial appointment must succeed");

        // Record the initial term_end from the durable record.
        let initial_record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("get_steward_by_did must not error")
            .expect("steward must exist after appointment");
        let initial_term_end = initial_record.term_end;

        // Reconfirm with a term_end that is 365 days past the initial value.
        let extended_term_end = initial_term_end + 365 * 86_400;
        let reconfirm_req = ReconfirmStewardRequest {
            steward_did: holder.to_string(),
            new_term_end: extended_term_end,
            proposal_id: "gov:coop-gamma:prop-011:receipt".to_string(),
        };
        let reconfirm = svc.reconfirm_steward(reconfirm_req).unwrap();
        assert!(reconfirm.success, "reconfirm_steward must succeed");
        assert!(
            !reconfirm.state_change_hash.is_empty(),
            "state_change_hash must be non-empty"
        );
        assert!(reconfirm.error.is_none());

        // Verify the durable record has the extended term_end.
        let updated_record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("get_steward_by_did must not error after reconfirmation")
            .expect("steward must still exist after reconfirmation");
        assert_eq!(
            updated_record.term_end, extended_term_end,
            "term_end must equal the new value after reconfirmation"
        );
        assert!(
            updated_record.term_end > initial_term_end,
            "term_end must be later than the initial value"
        );
    }

    /// ReconfirmSteward fails when the steward DID is not found.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_reconfirm_steward_fails_for_unknown_did() {
        let (svc, _commons) = make_service_with_commons();
        let unknown_did = test_did(99);
        let req = ReconfirmStewardRequest {
            steward_did: unknown_did.to_string(),
            new_term_end: 99_999_999,
            proposal_id: "gov:unknown:prop-000:receipt".to_string(),
        };
        let result = svc.reconfirm_steward(req).unwrap();
        assert!(
            !result.success,
            "reconfirm for unknown DID must return success=false"
        );
        assert!(
            result.error.is_some(),
            "error field must be populated on failure"
        );
    }

    /// ReinstateSteward → suspended steward becomes active again.
    ///
    /// Proves the full ReinstateSteward dispatch chain:
    /// `SdisService::reinstate_steward` → `CommonsHandle::reinstate_steward`
    /// → durable status change visible via `is_active_steward`.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_reinstate_steward_after_suspension() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(10);
        let sponsor = test_did(11);

        // Commons setup: appoint a steward, then suspend them directly.
        create_strong_holder(&commons, &holder, &sponsor).await;

        let appoint_req = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86400 * 365,
            bond_amount: 500,
            region: None,
            proposal_id: "gov:coop-delta:prop-020:receipt".to_string(),
        };
        let appoint = svc.appoint_steward(appoint_req).unwrap();
        assert!(appoint.success, "initial appointment must succeed");

        // Suspend directly via CommonsHandle (bypasses governance — test setup only).
        let record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("lookup must not error")
            .expect("steward must exist");
        commons
            .suspend_steward(&record.id().to_hex(), "test suspension".to_string())
            .await
            .expect("suspend must succeed");

        // Verify steward is now inactive.
        assert!(
            !commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error"),
            "steward must be inactive after suspension"
        );

        // Reinstate via governance dispatch.
        let reinstate_req = ReinstateStewardRequest {
            steward_did: holder.to_string(),
            proposal_id: "gov:coop-delta:prop-021:receipt".to_string(),
        };
        let result = svc.reinstate_steward(reinstate_req).unwrap();

        assert!(result.success, "reinstate_steward must succeed");
        assert!(result.was_suspended, "was_suspended must be true");
        assert!(
            !result.state_change_hash.is_empty(),
            "state_change_hash must be non-empty when suspended was true"
        );
        assert!(result.error.is_none());
        // Evidence-fidelity seam: reinstate publishes the same steward_id
        // it routed the commons call through — the real downstream handle.
        assert_eq!(
            result.receipt_ref.as_deref(),
            Some(record.id().to_hex().as_str()),
            "ReinstateStewardResult.receipt_ref must be the steward_id the reinstate was routed through"
        );

        // Verify the steward is now active again.
        assert!(
            commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error after reinstatement"),
            "steward must be active after reinstatement"
        );
    }

    /// ReinstateSteward on an active (not suspended) steward is a no-op.
    ///
    /// Proves idempotent behavior: `was_suspended = false`, `success = true`,
    /// `state_change_hash` is empty.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_reinstate_steward_no_op_when_not_suspended() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(12);
        let sponsor = test_did(13);

        create_strong_holder(&commons, &holder, &sponsor).await;

        let appoint_req = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86400 * 180,
            bond_amount: 200,
            region: None,
            proposal_id: "gov:coop-epsilon:prop-030:receipt".to_string(),
        };
        let appoint = svc.appoint_steward(appoint_req).unwrap();
        assert!(appoint.success);

        // Reinstate an active (not suspended) steward — must be idempotent.
        let reinstate_req = ReinstateStewardRequest {
            steward_did: holder.to_string(),
            proposal_id: "gov:coop-epsilon:prop-031:receipt".to_string(),
        };
        let result = svc.reinstate_steward(reinstate_req).unwrap();

        assert!(result.success, "reinstate no-op must return success=true");
        assert!(
            !result.was_suspended,
            "was_suspended must be false for active steward"
        );
        assert!(
            result.state_change_hash.is_empty(),
            "state_change_hash must be empty on no-op path"
        );
        assert!(result.error.is_none());
        // No-op but record existed and commons call succeeded: the
        // downstream handle is real, so receipt_ref is truthful.
        // state_change_hash (empty) already distinguishes no-op from
        // active reinstatement.
        let record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("lookup must not error")
            .expect("record must exist");
        assert_eq!(
            result.receipt_ref.as_deref(),
            Some(record.id().to_hex().as_str()),
            "reinstate no-op still routed through a real steward_id; receipt_ref must reflect it"
        );

        // Steward still active.
        assert!(
            commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error"),
            "steward must remain active after no-op reinstatement"
        );
    }

    /// ReinstateSteward fails when the steward DID is not found.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_reinstate_steward_fails_for_unknown_did() {
        let (svc, _commons) = make_service_with_commons();
        let unknown_did = test_did(98);
        let req = ReinstateStewardRequest {
            steward_did: unknown_did.to_string(),
            proposal_id: "gov:unknown:prop-000:receipt".to_string(),
        };
        let result = svc.reinstate_steward(req).unwrap();
        assert!(
            !result.success,
            "reinstate for unknown DID must return success=false"
        );
        assert!(
            result.error.is_some(),
            "error field must be populated on failure"
        );
        assert_eq!(
            result.receipt_ref, None,
            "no commons record → no downstream handle → receipt_ref must remain None"
        );
    }

    // ─── SuspendSteward proof tests ────────────────────────────────────────────

    /// SuspendSteward → active steward becomes suspended with reason persisted.
    ///
    /// Proves the full SuspendSteward dispatch chain:
    /// `SdisService::suspend_steward` → `CommonsHandle::suspend_steward`
    /// → durable `Suspended { reason }` status visible via `is_active_steward`.
    ///
    /// Duration is advisory: it is carried through the request but CommonsHandle
    /// stores `Suspended { reason }` only — no timed auto-reinstatement.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_suspend_active_steward() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(20);
        let sponsor = test_did(21);

        create_strong_holder(&commons, &holder, &sponsor).await;
        let appoint_req = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86_400,
            bond_amount: 100,
            region: None,
            proposal_id: "gov:coop:prop-030:receipt".to_string(),
        };
        let appoint = svc.appoint_steward(appoint_req).unwrap();
        assert!(
            appoint.success,
            "appointment must succeed before suspension test"
        );

        // Verify steward is active before suspending.
        assert!(
            commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error"),
            "steward must be active before suspension"
        );

        // Suspend via governance dispatch.
        let req = SuspendStewardRequest {
            steward_did: holder.to_string(),
            reason: "governance investigation".to_string(),
            duration_seconds: 86_400, // advisory only — not enforced by CommonsHandle
            proposal_id: "gov:coop:prop-031:receipt".to_string(),
        };
        let result = svc.suspend_steward(req).unwrap();
        assert!(result.success, "suspension must succeed");
        assert!(
            !result.state_change_hash.is_empty(),
            "state_change_hash must be populated on success"
        );
        assert!(result.error.is_none());
        // Evidence-fidelity seam: suspend publishes the steward_id it
        // routed the commons call through.
        let record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("lookup must not error")
            .expect("record must exist");
        assert_eq!(
            result.receipt_ref.as_deref(),
            Some(record.id().to_hex().as_str()),
            "SuspendStewardResult.receipt_ref must be the steward_id the suspend was routed through"
        );

        // Read back durable state — steward must now be inactive (suspended).
        assert!(
            !commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error"),
            "steward must be inactive after governance suspension"
        );

        // Verify the record shows Suspended status (reason stored, not duration).
        let record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("get_steward_by_did must not error")
            .expect("steward record must exist");
        assert!(
            record.is_suspended(),
            "steward record must show Suspended status after governance suspension"
        );
    }

    /// SuspendSteward on an already-suspended steward is idempotent — updates reason.
    ///
    /// CommonsHandle::suspend_steward overwrites the reason on re-suspension.
    /// This is safe and expected: no error is returned.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_suspend_already_suspended_is_idempotent() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(22);
        let sponsor = test_did(23);

        create_strong_holder(&commons, &holder, &sponsor).await;
        let appoint_req = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86_400,
            bond_amount: 100,
            region: None,
            proposal_id: "gov:coop:prop-032:receipt".to_string(),
        };
        svc.appoint_steward(appoint_req).unwrap();

        // First suspension via service.
        let req1 = SuspendStewardRequest {
            steward_did: holder.to_string(),
            reason: "first suspension".to_string(),
            duration_seconds: 3_600,
            proposal_id: "gov:coop:prop-033:receipt".to_string(),
        };
        let r1 = svc.suspend_steward(req1).unwrap();
        assert!(r1.success, "first suspension must succeed");

        // Second suspension (already suspended) — must succeed and update reason.
        let req2 = SuspendStewardRequest {
            steward_did: holder.to_string(),
            reason: "updated reason".to_string(),
            duration_seconds: 7_200,
            proposal_id: "gov:coop:prop-034:receipt".to_string(),
        };
        let r2 = svc.suspend_steward(req2).unwrap();
        assert!(
            r2.success,
            "re-suspension of already-suspended steward must succeed"
        );
        assert!(
            !r2.state_change_hash.is_empty(),
            "state_change_hash must be populated on re-suspension"
        );

        // Steward remains suspended.
        assert!(
            !commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error"),
            "steward must remain suspended after re-suspension"
        );
    }

    /// SuspendSteward fails when the steward DID is not found.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_suspend_steward_fails_for_unknown_did() {
        let (svc, _commons) = make_service_with_commons();
        let unknown_did = test_did(97);
        let req = SuspendStewardRequest {
            steward_did: unknown_did.to_string(),
            reason: "test".to_string(),
            duration_seconds: 3_600,
            proposal_id: "gov:unknown:prop-000:receipt".to_string(),
        };
        let result = svc.suspend_steward(req).unwrap();
        assert!(
            !result.success,
            "suspend for unknown DID must return success=false"
        );
        assert!(
            result.error.is_some(),
            "error field must be populated on failure"
        );
        assert!(
            result.state_change_hash.is_empty(),
            "state_change_hash must be empty on failure"
        );
        assert_eq!(
            result.receipt_ref, None,
            "no commons record → no downstream handle → receipt_ref must remain None"
        );
    }

    // ─── SanctionSteward proof tests ───────────────────────────────────────────

    /// SanctionSteward → bond slashed in the durable commons record, and
    /// the service-level receipt_ref carries the real commons
    /// `StewardId::to_hex()` the slash was routed through.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sanction_steward_slashes_bond_and_publishes_receipt_ref() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(30);
        let sponsor = test_did(31);

        create_strong_holder(&commons, &holder, &sponsor).await;
        let appoint_req = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86_400 * 30,
            bond_amount: 1_000,
            region: None,
            proposal_id: "gov:coop:prop-040:receipt".to_string(),
        };
        svc.appoint_steward(appoint_req).unwrap();

        let record = commons
            .get_steward_by_did(&holder)
            .await
            .expect("lookup must not error")
            .expect("record must exist");
        let steward_id_hex = record.id().to_hex();

        // Pure slash (no suspend).
        let req = SanctionStewardRequest {
            steward_did: holder.to_string(),
            bond_slash_amount: 250,
            suspend_reason: String::new(),
            reason: "minor infraction".to_string(),
            proposal_id: "gov:coop:prop-041:receipt".to_string(),
        };
        let result = svc.sanction_steward(req).unwrap();
        assert!(result.success, "sanction must succeed");
        // Commons slash_steward_bond returns the post-slash bond value
        // from the commons layer; we don't pin an exact number here
        // because the commons bond accounting semantics (starting bond,
        // fees) live in icn-commons, not this service. What this test
        // pins is that (a) the sanction succeeded, (b) a real receipt_ref
        // was published, and (c) the suspended flag reflects the request.
        assert!(
            !result.suspended,
            "empty suspend_reason → steward must not be suspended"
        );
        assert!(
            !result.state_change_hash.is_empty(),
            "state_change_hash must be populated on success"
        );
        assert_eq!(
            result.receipt_ref.as_deref(),
            Some(steward_id_hex.as_str()),
            "SanctionStewardResult.receipt_ref must be the steward_id the slash was routed through"
        );
    }

    /// SanctionSteward with `suspend_reason` also suspends the steward,
    /// and still publishes the same `receipt_ref`.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sanction_steward_with_suspend_publishes_receipt_ref() {
        let (svc, commons) = make_service_with_commons();
        let holder = test_did(32);
        let sponsor = test_did(33);

        create_strong_holder(&commons, &holder, &sponsor).await;
        svc.appoint_steward(AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: String::new(),
            term_length_seconds: 86_400 * 30,
            bond_amount: 500,
            region: None,
            proposal_id: "gov:coop:prop-050:receipt".to_string(),
        })
        .unwrap();

        let record = commons
            .get_steward_by_did(&holder)
            .await
            .unwrap()
            .expect("record must exist");
        let steward_id_hex = record.id().to_hex();

        let req = SanctionStewardRequest {
            steward_did: holder.to_string(),
            bond_slash_amount: 100,
            suspend_reason: "serious breach".to_string(),
            reason: "serious breach".to_string(),
            proposal_id: "gov:coop:prop-051:receipt".to_string(),
        };
        let result = svc.sanction_steward(req).unwrap();
        assert!(result.success);
        assert!(
            result.suspended,
            "non-empty suspend_reason → steward must be suspended"
        );
        assert_eq!(
            result.receipt_ref.as_deref(),
            Some(steward_id_hex.as_str()),
            "receipt_ref must carry the same steward_id the slash+suspend were routed through"
        );

        // Durable state: suspension visible.
        assert!(
            !commons
                .is_active_steward(&holder)
                .await
                .expect("is_active must not error"),
            "steward must be inactive after sanction-with-suspend"
        );
    }

    /// SanctionSteward fails for an unknown DID: no record → no handle →
    /// receipt_ref must remain None.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sanction_steward_fails_for_unknown_did() {
        // Pre-slash failure path: lookup fails, so no commons mutation occurs.
        // receipt_ref and state_change_hash must both be empty/None — this is
        // the ONLY sanction failure path where dropping attribution is honest.
        let (svc, _commons) = make_service_with_commons();
        let unknown = test_did(96);
        let req = SanctionStewardRequest {
            steward_did: unknown.to_string(),
            bond_slash_amount: 100,
            suspend_reason: String::new(),
            reason: "no record".to_string(),
            proposal_id: "gov:unknown:prop-000:receipt".to_string(),
        };
        let result = svc.sanction_steward(req).unwrap();
        assert!(!result.success, "sanction for unknown DID must fail");
        assert!(result.error.is_some());
        assert_eq!(
            result.remaining_bond, 0,
            "pre-slash failure must not leak a nonzero remaining_bond"
        );
        assert!(
            result.state_change_hash.is_empty(),
            "pre-slash failure must leave state_change_hash empty"
        );
        assert_eq!(
            result.receipt_ref, None,
            "no commons mutation occurred → receipt_ref must remain None"
        );
    }

    /// Pins the partial-mutation DTO contract for `sanction_steward`.
    ///
    /// A real slash-then-suspend partial failure requires injecting a commons
    /// storage fault between step 1 (slash, irreversible) and step 2 (suspend).
    /// `icn-commons` does not currently expose a fault-injection seam, so we
    /// pin the contract at the DTO level: a partial-mutation result MUST carry
    /// both `receipt_ref = Some(_)` and a non-empty `state_change_hash` even
    /// when `success == false`. Dropping attribution would hide a real durable
    /// commons mutation behind a "nothing happened" evidence row.
    #[test]
    fn sanction_partial_mutation_dto_contract() {
        let partial = SanctionStewardResult {
            success: false,
            remaining_bond: 250,
            suspended: false,
            state_change_hash: "abc123".to_string(),
            error: Some("bond slashed but suspend failed: storage i/o".to_string()),
            receipt_ref: Some("deadbeef".to_string()),
        };
        assert!(!partial.success);
        assert!(
            partial.receipt_ref.is_some(),
            "partial mutation must preserve receipt_ref"
        );
        assert!(
            !partial.state_change_hash.is_empty(),
            "partial mutation must preserve state_change_hash (slash really happened)"
        );
        assert!(partial.error.is_some());
    }
}
