//! Commons Manager for Gateway API
//!
//! Thin gateway facade that delegates all substrate operations through
//! [`CommonsHandle`] (defined in `icn-commons`).
//!
//! Gateway-local concerns (enrollment sessions, governance DTOs) remain here.

use anyhow::Result;
use hex;
use icn_commons::CommonsHandle;
use icn_governance::{
    Amendment, AmendmentChange, AmendmentId, AmendmentStatus, Appeal, AppealEvidence, AppealId,
    AppealOutcome, AppealResponse, AppealStatus, Charter, CharterStatus, FounderSignature, OrgType,
    Ratification, StewardRecord, StewardStatus,
};
use icn_identity::{
    Affiliation, AnchorStatus, CommonsHolderRecord, CommonsRevocationReason, Did, HolderStatus,
    JurisdictionId, MembershipCapability, MembershipStatus, POPAttestation, PersonhoodAnchor,
    RevocationRecord, RevocationScope, RevocationType,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::models::{
    AmendmentsBreakdown, AppealsBreakdown, GovernanceActivityEvent, GovernanceDashboard,
    StewardDetailResponse, StewardSummaryResponse,
};

// ============================================================================
// CommonsManager struct
// ============================================================================

/// Thin gateway facade for commons operations.
///
/// All substrate operations (anchors, holders, charters, stewards, amendments,
/// appeals, revocations, membership, affiliations) are delegated to the
/// [`CommonsHandle`] from `icn-commons`. Enrollment sessions are managed
/// in-memory as a gateway-local concern (not persisted via the substrate).
pub struct CommonsManager {
    handle: CommonsHandle,
    /// Gateway-local: enrollment sessions (in-memory; not substrate state).
    enrollment_sessions:
        Arc<RwLock<HashMap<String, crate::api::sdis::simple_enrollment::EnrollmentSession>>>,
}

impl CommonsManager {
    /// Create with an in-memory handle (testing / no data_dir configured).
    pub fn new() -> Self {
        CommonsManager {
            handle: CommonsHandle::new_in_memory(),
            enrollment_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with a Sled-backed handle for durable persistence.
    pub fn with_sled_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Ok(CommonsManager {
            handle: CommonsHandle::with_sled_path(path)?,
            enrollment_sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create with a temporary Sled-backed handle (persistence testing).
    pub fn with_sled_temporary() -> Result<Self> {
        Ok(CommonsManager {
            handle: CommonsHandle::with_sled_temporary()?,
            enrollment_sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create from an existing [`CommonsHandle`] (for sharing with actors).
    pub fn with_handle(handle: CommonsHandle) -> Self {
        CommonsManager {
            handle,
            enrollment_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Flush pending writes to durable storage.
    pub async fn flush(&self) -> Result<()> {
        self.handle.flush().await
    }
}

impl Default for CommonsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PersonhoodAnchor Operations (Layer 0)
// ============================================================================

impl CommonsManager {
    /// Create a new PersonhoodAnchor from enrollment completion.
    pub async fn create_anchor_from_enrollment(
        &self,
        did: &Did,
        steward_did: Option<&Did>,
    ) -> Result<PersonhoodAnchor> {
        self.handle
            .create_anchor_from_enrollment(did, steward_did)
            .await
    }

    /// Get a PersonhoodAnchor by ID.
    pub async fn get_anchor(&self, anchor_id: &str) -> Result<Option<PersonhoodAnchor>> {
        self.handle.get_anchor(anchor_id).await
    }

    /// Get a PersonhoodAnchor by DID.
    pub async fn get_anchor_by_did(&self, did: &Did) -> Result<Option<PersonhoodAnchor>> {
        self.handle.get_anchor_by_did(did).await
    }

    /// Update anchor status.
    pub async fn update_anchor_status(&self, anchor_id: &str, status: AnchorStatus) -> Result<()> {
        self.handle.update_anchor_status(anchor_id, status).await
    }

    /// Add an attestation to an anchor.
    pub async fn add_attestation(
        &self,
        anchor_id: &str,
        attestation: POPAttestation,
    ) -> Result<()> {
        self.handle.add_attestation(anchor_id, attestation).await
    }
}

// ============================================================================
// CommonsHolderRecord Operations (Layer 1)
// ============================================================================

impl CommonsManager {
    /// Create a CommonsHolderRecord from an anchor.
    pub async fn create_holder_from_anchor(
        &self,
        anchor_id: &str,
        did: &Did,
    ) -> Result<CommonsHolderRecord> {
        self.handle.create_holder_from_anchor(anchor_id, did).await
    }

    /// Create a CommonsHolderRecord from an anchor with an optional display name.
    pub async fn create_holder_from_anchor_with_name(
        &self,
        anchor_id: &str,
        did: &Did,
        display_name: Option<String>,
    ) -> Result<CommonsHolderRecord> {
        self.handle
            .create_holder_from_anchor_with_name(anchor_id, did, display_name)
            .await
    }

    /// Get a CommonsHolderRecord by ID.
    pub async fn get_holder(&self, holder_id: &str) -> Result<Option<CommonsHolderRecord>> {
        self.handle.get_holder(holder_id).await
    }

    /// Get a CommonsHolderRecord by DID.
    pub async fn get_holder_by_did(&self, did: &Did) -> Result<Option<CommonsHolderRecord>> {
        self.handle.get_holder_by_did(did).await
    }

    /// Update the display name for a holder identified by DID.
    pub async fn update_display_name(&self, did: &Did, display_name: String) -> Result<()> {
        self.handle.update_display_name(did, display_name).await
    }

    /// Get or create a CommonsHolderRecord (idempotent).
    pub async fn get_or_create_holder(
        &self,
        anchor_id: &str,
        did: &Did,
        display_name: Option<String>,
    ) -> Result<CommonsHolderRecord> {
        self.handle
            .get_or_create_holder(anchor_id, did, display_name)
            .await
    }

    /// Update holder status.
    pub async fn update_holder_status(&self, holder_id: &str, status: HolderStatus) -> Result<()> {
        self.handle.update_holder_status(holder_id, status).await
    }
}

// ============================================================================
// Affiliation Operations
// ============================================================================

impl CommonsManager {
    /// Add an affiliation (join jurisdiction).
    pub async fn join_jurisdiction(
        &self,
        holder_id: &str,
        jurisdiction: JurisdictionId,
        initial_capabilities: Vec<MembershipCapability>,
    ) -> Result<Affiliation> {
        self.handle
            .join_jurisdiction(holder_id, jurisdiction, initial_capabilities)
            .await
    }

    /// Leave a jurisdiction.
    pub async fn leave_jurisdiction(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
    ) -> Result<()> {
        self.handle
            .leave_jurisdiction(holder_id, jurisdiction)
            .await
    }

    /// Update affiliation status.
    pub async fn update_affiliation_status(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        status: MembershipStatus,
    ) -> Result<()> {
        self.handle
            .update_affiliation_status(holder_id, jurisdiction, status)
            .await
    }

    /// List affiliations for a holder.
    pub async fn list_affiliations(&self, holder_id: &str) -> Result<Vec<Affiliation>> {
        self.handle.list_affiliations(holder_id).await
    }
}

// ============================================================================
// Charter Operations (Layer 2)
// ============================================================================

impl CommonsManager {
    /// Store a charter.
    pub async fn store_charter(&self, charter: Charter) -> Result<()> {
        self.handle.store_charter(charter).await
    }

    /// Get a charter by ID.
    pub async fn get_charter(&self, charter_id: &str) -> Result<Option<Charter>> {
        self.handle.get_charter(charter_id).await
    }

    /// Get a charter by domain ID.
    pub async fn get_charter_by_domain(&self, domain_id: &str) -> Result<Option<Charter>> {
        self.handle.get_charter_by_domain(domain_id).await
    }

    /// List charters with optional filters.
    pub async fn list_charters(
        &self,
        org_type: Option<OrgType>,
        status: Option<CharterStatus>,
    ) -> Result<Vec<Charter>> {
        self.handle.list_charters(org_type, status).await
    }

    /// Add a charter signature.
    pub async fn add_charter_signature(
        &self,
        charter_id: &str,
        signature: FounderSignature,
    ) -> Result<Charter> {
        self.handle
            .add_charter_signature(charter_id, signature)
            .await
    }

    /// Update charter status.
    pub async fn update_charter_status(
        &self,
        charter_id: &str,
        status: CharterStatus,
    ) -> Result<()> {
        self.handle.update_charter_status(charter_id, status).await
    }
}

// ============================================================================
// Steward Operations
// ============================================================================

impl CommonsManager {
    /// Register a new steward.
    pub async fn register_steward(
        &self,
        holder_did: &Did,
        steward_did: &Did,
        term_duration_days: u64,
        bond_amount: u64,
        governance_approval: String,
        jurisdiction: Option<String>,
        specializations: Vec<String>,
    ) -> Result<StewardRecord> {
        self.handle
            .register_steward(
                holder_did,
                steward_did,
                term_duration_days,
                bond_amount,
                governance_approval,
                jurisdiction,
                specializations,
            )
            .await
    }

    /// Store a steward record.
    pub async fn store_steward(&self, steward: StewardRecord) -> Result<()> {
        self.handle.store_steward(steward).await
    }

    /// Get a steward by ID.
    pub async fn get_steward(&self, steward_id: &str) -> Result<Option<StewardRecord>> {
        self.handle.get_steward(steward_id).await
    }

    /// Get a steward by DID.
    pub async fn get_steward_by_did(&self, did: &Did) -> Result<Option<StewardRecord>> {
        self.handle.get_steward_by_did(did).await
    }

    /// List stewards with optional filters.
    pub async fn list_stewards(
        &self,
        active_only: bool,
        jurisdiction: Option<&str>,
    ) -> Result<Vec<StewardRecord>> {
        self.handle.list_stewards(active_only, jurisdiction).await
    }

    /// List active attesters.
    pub async fn list_attesters(&self) -> Result<Vec<StewardRecord>> {
        self.handle.list_attesters().await
    }

    /// Check if a DID is an active steward.
    pub async fn is_active_steward(&self, did: &Did) -> Result<bool> {
        self.handle.is_active_steward(did).await
    }

    /// Suspend a steward.
    pub async fn suspend_steward(&self, steward_id: &str, reason: String) -> Result<()> {
        self.handle.suspend_steward(steward_id, reason).await
    }

    /// Reinstate a suspended steward.
    pub async fn reinstate_steward(&self, steward_id: &str) -> Result<bool> {
        self.handle.reinstate_steward(steward_id).await
    }

    /// Retire a steward.
    pub async fn retire_steward(&self, steward_id: &str) -> Result<()> {
        self.handle.retire_steward(steward_id).await
    }

    /// Revoke a steward.
    pub async fn revoke_steward(
        &self,
        steward_id: &str,
        reason: String,
        evidence: Vec<[u8; 32]>,
    ) -> Result<()> {
        self.handle
            .revoke_steward(steward_id, reason, evidence)
            .await
    }

    /// Record a steward attestation event.
    pub async fn record_steward_attestation(&self, steward_id: &str) -> Result<()> {
        self.handle.record_steward_attestation(steward_id).await
    }

    /// Record a dispute against a steward.
    pub async fn record_steward_dispute(&self, steward_id: &str) -> Result<()> {
        self.handle.record_steward_dispute(steward_id).await
    }

    /// Record a dispute won by a steward.
    pub async fn record_steward_dispute_won(&self, steward_id: &str) -> Result<()> {
        self.handle.record_steward_dispute_won(steward_id).await
    }

    /// Extend a steward's term.
    pub async fn extend_steward_term(&self, steward_id: &str, new_term_end: u64) -> Result<()> {
        self.handle
            .extend_steward_term(steward_id, new_term_end)
            .await
    }

    /// Add bond to a steward.
    pub async fn add_steward_bond(&self, steward_id: &str, amount: u64) -> Result<()> {
        self.handle.add_steward_bond(steward_id, amount).await
    }

    /// Slash a steward's bond.
    pub async fn slash_steward_bond(&self, steward_id: &str, amount: u64) -> Result<u64> {
        self.handle.slash_steward_bond(steward_id, amount).await
    }
}

// ============================================================================
// Revocation Operations
// ============================================================================

impl CommonsManager {
    /// Add a revocation record.
    pub async fn add_revocation(&self, record: RevocationRecord) -> Result<()> {
        self.handle.add_revocation(record).await
    }

    /// Check revocation status for a target.
    pub async fn check_revocation(&self, target_id: &str) -> icn_identity::RevocationCheck {
        self.handle.check_revocation(target_id).await
    }

    /// Check revocation status at a given scope.
    pub async fn check_revocation_at_scope(
        &self,
        target_id: &str,
        scope: &RevocationScope,
    ) -> icn_identity::RevocationCheck {
        self.handle
            .check_revocation_at_scope(target_id, scope)
            .await
    }

    /// Get a revocation record.
    pub async fn get_revocation(&self, revocation_id: &str) -> Option<RevocationRecord> {
        self.handle.get_revocation(revocation_id).await
    }

    /// List revocations for a target.
    pub async fn list_revocations_for_target(&self, target_id: &str) -> Vec<RevocationRecord> {
        self.handle.list_revocations_for_target(target_id).await
    }

    /// List revocations by type.
    pub async fn list_revocations_by_type(
        &self,
        revocation_type: RevocationType,
    ) -> Vec<RevocationRecord> {
        self.handle.list_revocations_by_type(revocation_type).await
    }

    /// List revocations with pending appeals.
    pub async fn list_pending_appeals(&self) -> Vec<RevocationRecord> {
        self.handle.list_pending_appeals().await
    }

    /// File an appeal for a revocation.
    pub async fn file_appeal(&self, revocation_id: &str, reason: String) -> Result<()> {
        self.handle.file_appeal(revocation_id, reason).await
    }

    /// Resolve an appeal.
    pub async fn resolve_appeal(
        &self,
        revocation_id: &str,
        upheld: bool,
        resolution_notes: String,
    ) -> Result<()> {
        self.handle
            .resolve_appeal(revocation_id, upheld, resolution_notes)
            .await
    }

    /// Revoke a member's access.
    pub async fn revoke_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        authority: Did,
        reason: CommonsRevocationReason,
        appeal_window_days: u64,
    ) -> Result<RevocationRecord> {
        self.handle
            .revoke_membership(
                holder_id,
                jurisdiction_id,
                authority,
                reason,
                appeal_window_days,
            )
            .await
    }

    /// Ban a member entirely (immediate, severe violations).
    pub async fn ban_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        authority: Did,
        reason: CommonsRevocationReason,
    ) -> Result<RevocationRecord> {
        self.handle
            .ban_member(holder_id, jurisdiction_id, authority, reason)
            .await
    }

    /// Check if a member is revoked.
    pub async fn is_member_revoked(&self, holder_id: &str, jurisdiction: &JurisdictionId) -> bool {
        self.handle.is_member_revoked(holder_id, jurisdiction).await
    }
}

// ============================================================================
// Membership Management Operations
// ============================================================================

impl CommonsManager {
    /// Apply for membership in a jurisdiction.
    pub async fn apply_for_membership(
        &self,
        holder_id: &str,
        jurisdiction: JurisdictionId,
        initial_capabilities: Vec<MembershipCapability>,
    ) -> Result<Affiliation> {
        self.handle
            .apply_for_membership(holder_id, jurisdiction, initial_capabilities)
            .await
    }

    /// Approve membership.
    pub async fn approve_membership(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
    ) -> Result<()> {
        self.handle
            .approve_membership(holder_id, jurisdiction)
            .await
    }

    /// Promote a member from Provisional to full Member.
    pub async fn promote_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        self.handle.promote_member(holder_id, jurisdiction_id).await
    }

    /// Suspend a member.
    pub async fn suspend_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        self.handle.suspend_member(holder_id, jurisdiction_id).await
    }

    /// Reinstate a suspended member.
    pub async fn reinstate_member(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
    ) -> Result<()> {
        self.handle.reinstate_member(holder_id, jurisdiction).await
    }

    /// Grant a capability to a member.
    pub async fn grant_capability(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<()> {
        self.handle
            .grant_capability(holder_id, jurisdiction, capability)
            .await
    }

    /// Revoke a capability from a member.
    pub async fn revoke_capability(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<()> {
        self.handle
            .revoke_capability(holder_id, jurisdiction, capability)
            .await
    }

    /// Add a role to a member.
    pub async fn add_member_role(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        role: String,
    ) -> Result<()> {
        self.handle
            .add_member_role(holder_id, jurisdiction, role)
            .await
    }

    /// Remove a role from a member.
    pub async fn remove_member_role(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        role: &str,
    ) -> Result<()> {
        self.handle
            .remove_member_role(holder_id, jurisdiction, role)
            .await
    }

    /// Check if a member has a capability.
    pub async fn member_has_capability(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<bool> {
        self.handle
            .member_has_capability(holder_id, jurisdiction, capability)
            .await
    }

    /// List members by status.
    pub async fn list_members_by_status(
        &self,
        jurisdiction: &JurisdictionId,
        status: Option<MembershipStatus>,
    ) -> Vec<(String, Affiliation)> {
        self.handle
            .list_members_by_status(jurisdiction, status)
            .await
    }
}

// ============================================================================
// Amendment Operations (v0.6.0 Constitutional Governance)
// ============================================================================

impl CommonsManager {
    /// Store an amendment.
    pub async fn store_amendment(&self, amendment: Amendment) -> Result<()> {
        self.handle.store_amendment(amendment).await
    }

    /// Add a change to an amendment.
    pub async fn add_amendment_change(
        &self,
        amendment_id: &str,
        change: AmendmentChange,
    ) -> Result<Amendment> {
        self.handle.add_amendment_change(amendment_id, change).await
    }

    /// Get an amendment by ID.
    pub async fn get_amendment(&self, id: &AmendmentId) -> Result<Option<Amendment>> {
        self.handle.get_amendment(id).await
    }

    /// List amendments with optional filters.
    pub async fn list_amendments(
        &self,
        status: Option<&str>,
        scope: Option<&str>,
        amendment_type: Option<&str>,
    ) -> Result<Vec<Amendment>> {
        self.handle
            .list_amendments(status, scope, amendment_type)
            .await
    }

    /// Submit an amendment for review.
    pub async fn submit_amendment(&self, id: &AmendmentId, caller: &Did) -> Result<Amendment> {
        self.handle.submit_amendment(id, caller).await
    }

    /// Open voting on an amendment.
    pub async fn open_amendment_voting(&self, id: &AmendmentId, caller: &Did) -> Result<Amendment> {
        self.handle.open_amendment_voting(id, caller).await
    }

    /// Add a ratification to an amendment.
    pub async fn add_amendment_ratification(
        &self,
        id: &AmendmentId,
        ratification: Ratification,
    ) -> Result<Amendment> {
        self.handle
            .add_amendment_ratification(id, ratification)
            .await
    }

    /// Withdraw an amendment.
    pub async fn withdraw_amendment(
        &self,
        id: &AmendmentId,
        caller: &Did,
        reason: String,
    ) -> Result<Amendment> {
        self.handle.withdraw_amendment(id, caller, reason).await
    }
}

// ============================================================================
// Appeal Operations (v0.6.0 Constitutional Governance)
// ============================================================================

impl CommonsManager {
    /// Store an appeal.
    pub async fn store_appeal(&self, appeal: Appeal) -> Result<()> {
        self.handle.store_appeal(appeal).await
    }

    /// Get an appeal by ID.
    pub async fn get_appeal(&self, id: &AppealId) -> Result<Option<Appeal>> {
        self.handle.get_appeal(id).await
    }

    /// List appeals with optional filters.
    pub async fn list_appeals(
        &self,
        status: Option<&str>,
        scope: Option<&str>,
        appellant: Option<&str>,
    ) -> Result<Vec<Appeal>> {
        self.handle.list_appeals(status, scope, appellant).await
    }

    /// Add evidence to an appeal.
    pub async fn add_appeal_evidence(
        &self,
        id: &AppealId,
        evidence: AppealEvidence,
    ) -> Result<Appeal> {
        self.handle.add_appeal_evidence(id, evidence).await
    }

    /// Add a response to an appeal.
    pub async fn add_appeal_response(
        &self,
        id: &AppealId,
        response: AppealResponse,
    ) -> Result<Appeal> {
        self.handle.add_appeal_response(id, response).await
    }

    /// Begin review of an appeal.
    pub async fn begin_appeal_review(&self, id: &AppealId) -> Result<Appeal> {
        self.handle.begin_appeal_review(id).await
    }

    /// Resolve a constitutional appeal.
    pub async fn resolve_constitutional_appeal(
        &self,
        id: &AppealId,
        outcome: AppealOutcome,
    ) -> Result<Appeal> {
        self.handle.resolve_constitutional_appeal(id, outcome).await
    }

    /// Withdraw an appeal.
    pub async fn withdraw_appeal(
        &self,
        id: &AppealId,
        caller: &Did,
        reason: Option<String>,
    ) -> Result<Appeal> {
        self.handle.withdraw_appeal(id, caller, reason).await
    }
}

// ============================================================================
// Enrollment Session Operations (gateway-local, in-memory)
// ============================================================================

impl CommonsManager {
    /// Store an enrollment session.
    pub async fn put_enrollment_session(
        &self,
        id: &str,
        session: &crate::api::sdis::simple_enrollment::EnrollmentSession,
    ) -> Result<()> {
        self.enrollment_sessions
            .write()
            .await
            .insert(id.to_string(), session.clone());
        Ok(())
    }

    /// Get an enrollment session by ID.
    pub async fn get_enrollment_session(
        &self,
        id: &str,
    ) -> Result<Option<crate::api::sdis::simple_enrollment::EnrollmentSession>> {
        Ok(self.enrollment_sessions.read().await.get(id).cloned())
    }

    /// Update an enrollment session.
    pub async fn update_enrollment_session(
        &self,
        id: &str,
        session: &crate::api::sdis::simple_enrollment::EnrollmentSession,
    ) -> Result<()> {
        self.enrollment_sessions
            .write()
            .await
            .insert(id.to_string(), session.clone());
        Ok(())
    }

    /// Delete an enrollment session.
    pub async fn delete_enrollment_session(&self, id: &str) -> Result<bool> {
        Ok(self.enrollment_sessions.write().await.remove(id).is_some())
    }

    /// List all enrollment sessions.
    pub async fn list_enrollment_sessions(
        &self,
    ) -> Result<
        Vec<(
            String,
            crate::api::sdis::simple_enrollment::EnrollmentSession,
        )>,
    > {
        Ok(self
            .enrollment_sessions
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

// ============================================================================
// Governance Dashboard (gateway-local DTO)
// ============================================================================

impl CommonsManager {
    /// Build a governance dashboard from stored amendments and appeals.
    ///
    /// Returns a gateway-level DTO so callers don't need icn_governance types.
    pub async fn build_governance_dashboard(
        &self,
        charter_id: &str,
    ) -> Result<GovernanceDashboard> {
        let amendments = self.list_amendments(None, None, None).await?;
        let appeals = self.list_appeals(None, None, None).await?;

        let mut ab = AmendmentsBreakdown {
            draft: 0,
            submitted: 0,
            voting: 0,
            ratified: 0,
            rejected: 0,
            withdrawn: 0,
        };
        let mut pending_amendments = 0usize;
        for amendment in &amendments {
            match &amendment.status {
                AmendmentStatus::Draft => {
                    ab.draft += 1;
                    pending_amendments += 1;
                }
                AmendmentStatus::Submitted { .. } | AmendmentStatus::UnderReview { .. } => {
                    ab.submitted += 1;
                    pending_amendments += 1;
                }
                AmendmentStatus::Voting { .. } | AmendmentStatus::Ratifying { .. } => {
                    ab.voting += 1;
                    pending_amendments += 1;
                }
                AmendmentStatus::Ratified { .. } => ab.ratified += 1,
                AmendmentStatus::Rejected { .. } => ab.rejected += 1,
                AmendmentStatus::Withdrawn { .. } => ab.withdrawn += 1,
                _ => {}
            }
        }

        let mut apb = AppealsBreakdown {
            filed: 0,
            under_review: 0,
            hearing: 0,
            resolved: 0,
            dismissed: 0,
            withdrawn: 0,
        };
        let mut open_appeals = 0usize;
        for appeal in &appeals {
            match &appeal.status {
                AppealStatus::Filed { .. } => {
                    apb.filed += 1;
                    open_appeals += 1;
                }
                AppealStatus::UnderReview { .. } => {
                    apb.under_review += 1;
                    open_appeals += 1;
                }
                AppealStatus::Hearing { .. } => {
                    apb.hearing += 1;
                    open_appeals += 1;
                }
                AppealStatus::Resolved { .. } => apb.resolved += 1,
                AppealStatus::Dismissed { .. } => apb.dismissed += 1,
                AppealStatus::Withdrawn { .. } => apb.withdrawn += 1,
            }
        }

        let mut activity = Vec::new();
        for amendment in amendments.iter().take(5) {
            let timestamp = match &amendment.status {
                AmendmentStatus::Draft => amendment.created_at,
                AmendmentStatus::Submitted { submitted_at, .. } => *submitted_at,
                AmendmentStatus::Voting {
                    voting_started_at, ..
                } => *voting_started_at,
                AmendmentStatus::Ratified { ratified_at, .. } => *ratified_at,
                AmendmentStatus::Rejected { rejected_at, .. } => *rejected_at,
                AmendmentStatus::Withdrawn { withdrawn_at, .. } => *withdrawn_at,
                _ => amendment.created_at,
            };
            activity.push(GovernanceActivityEvent {
                event_type: "amendment".to_string(),
                description: format!("Amendment: {}", amendment.title),
                timestamp,
                resource_id: hex::encode(amendment.id.as_bytes()),
                resource_type: "amendment".to_string(),
            });
        }
        for appeal in appeals.iter().take(5) {
            activity.push(GovernanceActivityEvent {
                event_type: "appeal".to_string(),
                description: format!("Appeal filed: {:?}", appeal.appeal_type),
                timestamp: appeal.created_at,
                resource_id: hex::encode(appeal.id.as_bytes()),
                resource_type: "appeal".to_string(),
            });
        }
        activity.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        activity.truncate(10);

        Ok(GovernanceDashboard {
            charter_id: Some(charter_id.to_string()),
            pending_amendments,
            open_appeals,
            recent_activity: activity,
            amendments_breakdown: ab,
            appeals_breakdown: apb,
        })
    }
}

// ============================================================================
// Steward DTO Helpers (gateway-local free functions)
// ============================================================================

/// Convert a `StewardRecord` to a gateway-local summary DTO.
pub fn steward_to_summary(s: &StewardRecord) -> StewardSummaryResponse {
    StewardSummaryResponse {
        steward_id: s.steward_id.to_hex(),
        steward_did: s.steward_did.to_string(),
        holder_did: s.holder_did.to_string(),
        status: format_steward_status(&s.status),
        jurisdiction: s.jurisdiction.clone(),
        reputation_score: s.reputation_score,
        attestations_issued: s.attestations_issued,
        can_attest: s.can_attest(),
        term_end: s.term_end,
    }
}

/// Convert a `StewardRecord` to a gateway-local detail DTO.
pub fn steward_to_detail(s: &StewardRecord) -> StewardDetailResponse {
    StewardDetailResponse {
        steward_id: s.steward_id.to_hex(),
        steward_did: s.steward_did.to_string(),
        holder_did: s.holder_did.to_string(),
        status: format_steward_status(&s.status),
        jurisdiction: s.jurisdiction.clone(),
        term_start: s.term_start,
        term_end: s.term_end,
        bond_amount: s.bond_amount,
        reputation_score: s.reputation_score,
        effectiveness_score: s.effectiveness_score(),
        attestations_issued: s.attestations_issued,
        attestations_disputed: s.attestations_disputed,
        disputes_against: s.disputes_against,
        disputes_won: s.disputes_won,
        specializations: s.specializations.clone(),
        can_attest: s.can_attest(),
        is_term_expired: s.is_term_expired(),
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

fn format_steward_status(s: &StewardStatus) -> String {
    match s {
        StewardStatus::Active => "active".to_string(),
        StewardStatus::Suspended { .. } => "suspended".to_string(),
        StewardStatus::Retired => "retired".to_string(),
        StewardStatus::Revoked { .. } => "revoked".to_string(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::POPLevel;

    fn test_did() -> Did {
        Did::from_anchor_id(&[42u8; 32])
    }

    fn steward_did() -> Did {
        Did::from_anchor_id(&[99u8; 32])
    }

    #[tokio::test]
    async fn test_create_anchor_with_steward() {
        let mgr = CommonsManager::new();
        let did = test_did();
        let steward = steward_did();

        let anchor = mgr
            .create_anchor_from_enrollment(&did, Some(&steward))
            .await
            .unwrap();

        assert!(anchor.is_active());
        assert_eq!(anchor.pop_attestations.len(), 1);
        assert_eq!(anchor.pop_level(), Some(POPLevel::Strong));
    }

    #[tokio::test]
    async fn test_create_holder_from_anchor() {
        let mgr = CommonsManager::new();
        let did = test_did();

        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        let anchor_id = hex::encode(anchor.id());

        let holder = mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();

        assert!(holder.is_active());
        assert!(holder.affiliations.is_empty());
    }

    #[tokio::test]
    async fn test_join_and_leave_jurisdiction() {
        let mgr = CommonsManager::new();
        let did = test_did();
        let steward = steward_did();

        let anchor = mgr
            .create_anchor_from_enrollment(&did, Some(&steward))
            .await
            .unwrap();
        let anchor_id = hex::encode(anchor.id());

        let holder = mgr
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();
        let holder_id = hex::encode(holder.id());

        let jurisdiction = JurisdictionId::new("coop:test-coop");

        let affiliation = mgr
            .join_jurisdiction(&holder_id, jurisdiction.clone(), vec![])
            .await
            .unwrap();
        assert_eq!(affiliation.membership_status, MembershipStatus::Candidate);

        let affiliations = mgr.list_affiliations(&holder_id).await.unwrap();
        assert_eq!(affiliations.len(), 1);

        mgr.leave_jurisdiction(&holder_id, &jurisdiction)
            .await
            .unwrap();

        let affiliations = mgr.list_affiliations(&holder_id).await.unwrap();
        assert_eq!(affiliations[0].membership_status, MembershipStatus::Exited);
    }

    #[tokio::test]
    async fn test_get_by_did() {
        let mgr = CommonsManager::new();
        let did = test_did();

        let anchor = mgr.create_anchor_from_enrollment(&did, None).await.unwrap();
        let anchor_id = hex::encode(anchor.id());

        mgr.create_holder_from_anchor(&anchor_id, &did)
            .await
            .unwrap();

        let found_anchor = mgr.get_anchor_by_did(&did).await.unwrap();
        assert!(found_anchor.is_some());

        let found_holder = mgr.get_holder_by_did(&did).await.unwrap();
        assert!(found_holder.is_some());
    }
}
