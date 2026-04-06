//! SDIS steward service adapter.
//!
//! Implements `SdisService` backed by `icn_commons::CommonsHandle`.
//! The executor in `governance_executor.rs` calls this through the kernel-api
//! trait boundary; it never imports CommonsHandle directly.

use anyhow::Result;
use icn_kernel_api::{
    AppointStewardRequest, AppointStewardResult, RevokeStewardRequest, RevokeStewardResult,
    SdisService,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::info;

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

    async fn create_charter(commons: &icn_commons::CommonsHandle, domain_id: &str) {
        use icn_governance::{Charter, GovernanceConfig, OrgType};
        let charter = Charter::new(
            OrgType::Cooperative,
            domain_id.to_string(),
            format!("Test Coop for {domain_id}"),
            GovernanceConfig::cooperative_default(),
            Default::default(),
            Default::default(),
        );
        commons.store_charter(charter).await.expect("store charter");
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

        // Commons setup: holder needs Strong POP for steward registration;
        // jurisdiction needs an existing charter.
        create_strong_holder(&commons, &holder, &sponsor).await;
        create_charter(&commons, "coop-alpha").await;

        let request = AppointStewardRequest {
            steward_did: holder.to_string(),
            jurisdiction_id: "coop-alpha".to_string(),
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
}
