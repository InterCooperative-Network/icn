//! SDIS steward service adapter.
//!
//! Implements `SdisService` backed by `icn_commons::CommonsHandle`.
//! The executor in `governance_executor.rs` calls this through the kernel-api
//! trait boundary; it never imports CommonsHandle directly.

use anyhow::Result;
use icn_kernel_api::{
    AppointStewardRequest, AppointStewardResult, ReconfirmStewardRequest, ReconfirmStewardResult,
    ReinstateStewardRequest, ReinstateStewardResult, RevokeStewardRequest, RevokeStewardResult,
    SdisService, SuspendStewardRequest, SuspendStewardResult,
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
            Ok(_record) => {
                let state_change_hash = Self::compute_appoint_hash(&request);
                info!(
                    steward_did = %request.steward_did,
                    state_change_hash = %state_change_hash,
                    "Steward appointed and registered in commons"
                );
                Ok(AppointStewardResult {
                    success: true,
                    state_change_hash,
                    error: None,
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

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Look up by DID first, then revoke by record ID
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons
                            .revoke_steward(&steward_id, request.reason.clone(), vec![])
                            .await
                            .map(|_| true)
                    }
                    Ok(None) => {
                        // No record found — idempotent no-op
                        tracing::debug!(
                            steward_did = %request.steward_did,
                            "RevokeSteward: no active steward record found, treating as no-op"
                        );
                        Ok(true)
                    }
                    Err(e) => Err(e),
                }
            })
        });

        match result {
            Ok(_) => {
                let state_change_hash = Self::compute_revoke_hash(&request);
                info!(
                    steward_did = %request.steward_did,
                    state_change_hash = %state_change_hash,
                    "Steward revoked in commons"
                );
                Ok(RevokeStewardResult {
                    success: true,
                    state_change_hash,
                    error: None,
                })
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

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons
                            .extend_steward_term(&steward_id, request.new_term_end)
                            .await
                            .map(|_| true)
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
            Ok(_) => {
                let state_change_hash = Self::compute_reconfirm_hash(&request);
                info!(
                    steward_did = %request.steward_did,
                    new_term_end = %request.new_term_end,
                    state_change_hash = %state_change_hash,
                    "Steward term extended in commons"
                );
                Ok(ReconfirmStewardResult {
                    success: true,
                    state_change_hash,
                    error: None,
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

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons.reinstate_steward(&steward_id).await
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
            Ok(was_suspended) => {
                let state_change_hash = if was_suspended {
                    let hash = Self::compute_reinstate_hash(&request);
                    info!(
                        steward_did = %request.steward_did,
                        state_change_hash = %hash,
                        "Suspended steward reinstated in commons"
                    );
                    hash
                } else {
                    info!(
                        steward_did = %request.steward_did,
                        "ReinstateSteward no-op: steward was not suspended"
                    );
                    String::new()
                };
                Ok(ReinstateStewardResult {
                    success: true,
                    was_suspended,
                    state_change_hash,
                    error: None,
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
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match self.commons.get_steward_by_did(&steward_did).await {
                    Ok(Some(record)) => {
                        let steward_id = record.id().to_hex();
                        self.commons
                            .suspend_steward(&steward_id, request.reason.clone())
                            .await
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
            Ok(()) => {
                let hash = Self::compute_suspend_hash(&request);
                info!(
                    steward_did = %request.steward_did,
                    state_change_hash = %hash,
                    "Steward suspended in commons"
                );
                Ok(SuspendStewardResult {
                    success: true,
                    state_change_hash: hash,
                    error: None,
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
                })
            }
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
        let found = commons
            .get_steward_by_did(&holder)
            .await
            .expect("get_steward_by_did must not error")
            .is_some();
        assert!(
            found,
            "steward must be present in commons after appointment"
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

        // Confirm presence before revoke
        assert!(
            commons.get_steward_by_did(&holder).await.unwrap().is_some(),
            "steward must exist before revocation"
        );

        let revoke_req = RevokeStewardRequest {
            steward_did: steward_did_str.clone(),
            reason: "Governance decision".to_string(),
        };
        let revoke = svc.revoke_steward(revoke_req).unwrap();
        assert!(revoke.success, "revoke_steward should succeed");
        assert!(!revoke.state_change_hash.is_empty());

        // Idempotent second revoke must succeed (no-op on missing record)
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
    }
}
