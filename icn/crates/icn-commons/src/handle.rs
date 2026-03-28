//! CommonsHandle — clone-safe actor handle for CommonsInner
//!
//! All operations are serialized through a `tokio::sync::RwLock`:
//! - mutations acquire a write lock
//! - reads acquire a read lock
//!
//! This is the single canonical owner of commons state. The gateway's
//! `CommonsManager` delegates all operations through this handle.

use crate::inner::CommonsInner;
use crate::store::{CommonsStoreBackend, InMemoryCommonsStore, SledCommonsStore};
use anyhow::{Context, Result};
use icn_governance::{
    Amendment, AmendmentChange, AmendmentId, Appeal, AppealEvidence, AppealId, AppealOutcome,
    AppealResponse, Charter, CharterStatus, OrgType, Ratification, StewardRecord,
};
use icn_identity::{
    Affiliation, AnchorStatus, CommonsHolderRecord, CommonsRevocationReason, Did, HolderStatus,
    JurisdictionId, MembershipCapability, MembershipStatus, POPAttestation, PersonhoodAnchor,
    RevocationRecord, RevocationScope, RevocationType,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Actor handle for [`CommonsInner`].
///
/// Clone-safe. All operations are serialized through an `RwLock`:
/// - mutations use write lock
/// - reads use read lock
///
/// This is the single canonical owner of commons state. The gateway's
/// `CommonsManager` delegates all operations through this handle.
#[derive(Clone)]
pub struct CommonsHandle {
    inner: Arc<RwLock<CommonsInner>>,
}

// ============================================================================
// Constructors
// ============================================================================

impl CommonsHandle {
    /// Create a handle backed by an in-memory store (testing/dev).
    pub fn new_in_memory() -> Self {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(InMemoryCommonsStore::new());
        CommonsHandle {
            inner: Arc::new(RwLock::new(CommonsInner::new(backend))),
        }
    }

    /// Create a handle backed by a sled store at the given path.
    pub fn with_sled_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let backend: Arc<dyn CommonsStoreBackend> =
            Arc::new(SledCommonsStore::open(path).context("Failed to open commons sled store")?);
        Ok(CommonsHandle {
            inner: Arc::new(RwLock::new(CommonsInner::new(backend))),
        })
    }

    /// Create a handle backed by a temporary sled store (useful in tests that
    /// need real persistence behavior without drop-and-reopen).
    pub fn with_sled_temporary() -> Result<Self> {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(
            SledCommonsStore::temporary()
                .context("Failed to create temporary commons sled store")?,
        );
        Ok(CommonsHandle {
            inner: Arc::new(RwLock::new(CommonsInner::new(backend))),
        })
    }
}

impl CommonsHandle {
    /// Flush pending writes to durable storage.
    pub async fn flush(&self) -> Result<()> {
        self.inner.read().await.flush()
    }
}

// ============================================================================
// PersonhoodAnchor Operations (Layer 0)
// ============================================================================

impl CommonsHandle {
    /// Create a new PersonhoodAnchor from enrollment completion.
    pub async fn create_anchor_from_enrollment(
        &self,
        did: &Did,
        steward_did: Option<&Did>,
    ) -> Result<PersonhoodAnchor> {
        self.inner
            .write()
            .await
            .create_anchor_from_enrollment(did, steward_did)
            .await
    }

    /// Get a PersonhoodAnchor by ID.
    pub async fn get_anchor(&self, anchor_id: &str) -> Result<Option<PersonhoodAnchor>> {
        self.inner.read().await.get_anchor(anchor_id).await
    }

    /// Get a PersonhoodAnchor by DID.
    pub async fn get_anchor_by_did(&self, did: &Did) -> Result<Option<PersonhoodAnchor>> {
        self.inner.read().await.get_anchor_by_did(did).await
    }

    /// Update anchor status.
    pub async fn update_anchor_status(&self, anchor_id: &str, status: AnchorStatus) -> Result<()> {
        self.inner
            .write()
            .await
            .update_anchor_status(anchor_id, status)
            .await
    }

    /// Add an attestation to an anchor.
    pub async fn add_attestation(
        &self,
        anchor_id: &str,
        attestation: POPAttestation,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .add_attestation(anchor_id, attestation)
            .await
    }
}

// ============================================================================
// CommonsHolderRecord Operations (Layer 1)
// ============================================================================

impl CommonsHandle {
    /// Create a CommonsHolderRecord from an anchor.
    pub async fn create_holder_from_anchor(
        &self,
        anchor_id: &str,
        did: &Did,
    ) -> Result<CommonsHolderRecord> {
        self.inner
            .write()
            .await
            .create_holder_from_anchor(anchor_id, did)
            .await
    }

    /// Create a CommonsHolderRecord from an anchor with an optional display name.
    pub async fn create_holder_from_anchor_with_name(
        &self,
        anchor_id: &str,
        did: &Did,
        display_name: Option<String>,
    ) -> Result<CommonsHolderRecord> {
        self.inner
            .write()
            .await
            .create_holder_from_anchor_with_name(anchor_id, did, display_name)
            .await
    }

    /// Get a CommonsHolderRecord by ID.
    pub async fn get_holder(&self, holder_id: &str) -> Result<Option<CommonsHolderRecord>> {
        self.inner.read().await.get_holder(holder_id).await
    }

    /// Get a CommonsHolderRecord by DID.
    pub async fn get_holder_by_did(&self, did: &Did) -> Result<Option<CommonsHolderRecord>> {
        self.inner.read().await.get_holder_by_did(did).await
    }

    /// Update the display name for a holder identified by DID.
    /// If no holder record exists yet, creates a minimal holder record directly.
    pub async fn update_display_name(&self, did: &Did, display_name: String) -> Result<()> {
        self.inner
            .write()
            .await
            .update_display_name(did, display_name)
            .await
    }

    /// Get or create a CommonsHolderRecord (idempotent).
    pub async fn get_or_create_holder(
        &self,
        anchor_id: &str,
        did: &Did,
        display_name: Option<String>,
    ) -> Result<CommonsHolderRecord> {
        self.inner
            .write()
            .await
            .get_or_create_holder(anchor_id, did, display_name)
            .await
    }

    /// Update holder status.
    pub async fn update_holder_status(&self, holder_id: &str, status: HolderStatus) -> Result<()> {
        self.inner
            .write()
            .await
            .update_holder_status(holder_id, status)
            .await
    }
}

// ============================================================================
// Affiliation Operations
// ============================================================================

impl CommonsHandle {
    /// Add an affiliation (join jurisdiction).
    ///
    /// Sybil resistance: requires at least `POPLevel::Strong` to join.
    pub async fn join_jurisdiction(
        &self,
        holder_id: &str,
        jurisdiction: JurisdictionId,
        initial_capabilities: Vec<MembershipCapability>,
    ) -> Result<Affiliation> {
        self.inner
            .write()
            .await
            .join_jurisdiction(holder_id, jurisdiction, initial_capabilities)
            .await
    }

    /// Leave a jurisdiction.
    pub async fn leave_jurisdiction(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .leave_jurisdiction(holder_id, jurisdiction)
            .await
    }

    /// Update affiliation status (e.g., Candidate -> Member).
    pub async fn update_affiliation_status(
        &self,
        holder_id: &str,
        jurisdiction: &JurisdictionId,
        status: MembershipStatus,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .update_affiliation_status(holder_id, jurisdiction, status)
            .await
    }

    /// List affiliations for a holder.
    pub async fn list_affiliations(&self, holder_id: &str) -> Result<Vec<Affiliation>> {
        self.inner.read().await.list_affiliations(holder_id).await
    }
}

// ============================================================================
// Charter Operations (Layer 2)
// ============================================================================

impl CommonsHandle {
    /// Store a charter.
    pub async fn store_charter(&self, charter: Charter) -> Result<()> {
        self.inner.write().await.store_charter(charter).await
    }

    /// Get a charter by ID.
    pub async fn get_charter(&self, charter_id: &str) -> Result<Option<Charter>> {
        self.inner.read().await.get_charter(charter_id).await
    }

    /// Get a charter by domain ID.
    pub async fn get_charter_by_domain(&self, domain_id: &str) -> Result<Option<Charter>> {
        self.inner
            .read()
            .await
            .get_charter_by_domain(domain_id)
            .await
    }

    /// List all charters with optional filters.
    pub async fn list_charters(
        &self,
        org_type: Option<OrgType>,
        status: Option<CharterStatus>,
    ) -> Result<Vec<Charter>> {
        self.inner
            .read()
            .await
            .list_charters(org_type, status)
            .await
    }

    /// Add a founder signature to a charter.
    pub async fn add_charter_signature(
        &self,
        charter_id: &str,
        signature: icn_governance::FounderSignature,
    ) -> Result<Charter> {
        self.inner
            .write()
            .await
            .add_charter_signature(charter_id, signature)
            .await
    }

    /// Update charter status.
    pub async fn update_charter_status(
        &self,
        charter_id: &str,
        status: CharterStatus,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .update_charter_status(charter_id, status)
            .await
    }
}

// ============================================================================
// Steward Operations
// ============================================================================

impl CommonsHandle {
    /// Register a new steward from a Commons Holder.
    #[allow(clippy::too_many_arguments)]
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
        self.inner
            .write()
            .await
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
        self.inner.write().await.store_steward(steward).await
    }

    /// Get a steward by ID.
    pub async fn get_steward(&self, steward_id: &str) -> Result<Option<StewardRecord>> {
        self.inner.read().await.get_steward(steward_id).await
    }

    /// Get a steward by DID.
    pub async fn get_steward_by_did(&self, did: &Did) -> Result<Option<StewardRecord>> {
        self.inner.read().await.get_steward_by_did(did).await
    }

    /// List all stewards with optional filters.
    pub async fn list_stewards(
        &self,
        active_only: bool,
        jurisdiction: Option<&str>,
    ) -> Result<Vec<StewardRecord>> {
        self.inner
            .read()
            .await
            .list_stewards(active_only, jurisdiction)
            .await
    }

    /// List stewards who can issue attestations.
    pub async fn list_attesters(&self) -> Result<Vec<StewardRecord>> {
        self.inner.read().await.list_attesters().await
    }

    /// Check if a DID belongs to an active steward.
    pub async fn is_active_steward(&self, did: &Did) -> Result<bool> {
        self.inner.read().await.is_active_steward(did).await
    }

    /// Suspend a steward.
    pub async fn suspend_steward(&self, steward_id: &str, reason: String) -> Result<()> {
        self.inner
            .write()
            .await
            .suspend_steward(steward_id, reason)
            .await
    }

    /// Reinstate a suspended steward.
    pub async fn reinstate_steward(&self, steward_id: &str) -> Result<bool> {
        self.inner.write().await.reinstate_steward(steward_id).await
    }

    /// Retire a steward.
    pub async fn retire_steward(&self, steward_id: &str) -> Result<()> {
        self.inner.write().await.retire_steward(steward_id).await
    }

    /// Revoke a steward for cause.
    pub async fn revoke_steward(
        &self,
        steward_id: &str,
        reason: String,
        evidence: Vec<[u8; 32]>,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .revoke_steward(steward_id, reason, evidence)
            .await
    }

    /// Record an attestation issued by a steward.
    pub async fn record_steward_attestation(&self, steward_id: &str) -> Result<()> {
        self.inner
            .write()
            .await
            .record_steward_attestation(steward_id)
            .await
    }

    /// Record a dispute against a steward's attestation.
    pub async fn record_steward_dispute(&self, steward_id: &str) -> Result<()> {
        self.inner
            .write()
            .await
            .record_steward_dispute(steward_id)
            .await
    }

    /// Record a dispute won by a steward.
    pub async fn record_steward_dispute_won(&self, steward_id: &str) -> Result<()> {
        self.inner
            .write()
            .await
            .record_steward_dispute_won(steward_id)
            .await
    }

    /// Extend a steward's term.
    pub async fn extend_steward_term(&self, steward_id: &str, new_term_end: u64) -> Result<()> {
        self.inner
            .write()
            .await
            .extend_steward_term(steward_id, new_term_end)
            .await
    }

    /// Add to a steward's bond.
    pub async fn add_steward_bond(&self, steward_id: &str, amount: u64) -> Result<()> {
        self.inner
            .write()
            .await
            .add_steward_bond(steward_id, amount)
            .await
    }

    /// Slash a steward's bond.
    pub async fn slash_steward_bond(&self, steward_id: &str, amount: u64) -> Result<u64> {
        self.inner
            .write()
            .await
            .slash_steward_bond(steward_id, amount)
            .await
    }
}

// ============================================================================
// Revocation Operations
// ============================================================================

impl CommonsHandle {
    /// Add a revocation record.
    pub async fn add_revocation(&self, record: RevocationRecord) -> Result<()> {
        self.inner.write().await.add_revocation(record).await
    }

    /// Check if a target is revoked.
    pub async fn check_revocation(&self, target_id: &str) -> icn_identity::RevocationCheck {
        self.inner.read().await.check_revocation(target_id).await
    }

    /// Check if a target is revoked at a specific scope.
    pub async fn check_revocation_at_scope(
        &self,
        target_id: &str,
        scope: &RevocationScope,
    ) -> icn_identity::RevocationCheck {
        self.inner
            .read()
            .await
            .check_revocation_at_scope(target_id, scope)
            .await
    }

    /// Get a revocation record by ID.
    pub async fn get_revocation(&self, revocation_id: &str) -> Option<RevocationRecord> {
        self.inner.read().await.get_revocation(revocation_id).await
    }

    /// List all revocations for a target.
    pub async fn list_revocations_for_target(&self, target_id: &str) -> Vec<RevocationRecord> {
        self.inner
            .read()
            .await
            .list_revocations_for_target(target_id)
            .await
    }

    /// List all revocations by type.
    pub async fn list_revocations_by_type(
        &self,
        target_type: RevocationType,
    ) -> Vec<RevocationRecord> {
        self.inner
            .read()
            .await
            .list_revocations_by_type(target_type)
            .await
    }

    /// List all pending appeals.
    pub async fn list_pending_appeals(&self) -> Vec<RevocationRecord> {
        self.inner.read().await.list_pending_appeals().await
    }

    /// File an appeal for a revocation.
    pub async fn file_appeal(&self, revocation_id: &str, reason: String) -> Result<()> {
        self.inner
            .write()
            .await
            .file_appeal(revocation_id, reason)
            .await
    }

    /// Resolve an appeal.
    pub async fn resolve_appeal(
        &self,
        revocation_id: &str,
        upheld: bool,
        resolution_notes: String,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .resolve_appeal(revocation_id, upheld, resolution_notes)
            .await
    }

    /// Revoke a membership with proper due process.
    pub async fn revoke_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        authority: Did,
        reason: CommonsRevocationReason,
        appeal_window_days: u64,
    ) -> Result<RevocationRecord> {
        self.inner
            .write()
            .await
            .revoke_membership(
                holder_id,
                jurisdiction_id,
                authority,
                reason,
                appeal_window_days,
            )
            .await
    }

    /// Ban a member immediately (severe violations).
    pub async fn ban_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        authority: Did,
        reason: CommonsRevocationReason,
    ) -> Result<RevocationRecord> {
        self.inner
            .write()
            .await
            .ban_member(holder_id, jurisdiction_id, authority, reason)
            .await
    }

    /// Check if a member is revoked in a jurisdiction.
    pub async fn is_member_revoked(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> bool {
        self.inner
            .read()
            .await
            .is_member_revoked(holder_id, jurisdiction_id)
            .await
    }
}

// ============================================================================
// Membership Management Operations
// ============================================================================

impl CommonsHandle {
    /// Apply for membership in a jurisdiction.
    ///
    /// Sybil resistance: requires at least `POPLevel::Strong` to apply.
    pub async fn apply_for_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: JurisdictionId,
        capabilities_requested: Vec<MembershipCapability>,
    ) -> Result<Affiliation> {
        self.inner
            .write()
            .await
            .apply_for_membership(holder_id, jurisdiction_id, capabilities_requested)
            .await
    }

    /// Approve a membership application (moves from Candidate to Provisional).
    pub async fn approve_membership(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .approve_membership(holder_id, jurisdiction_id)
            .await
    }

    /// Promote a member from Provisional to full Member.
    pub async fn promote_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .promote_member(holder_id, jurisdiction_id)
            .await
    }

    /// Suspend a member.
    pub async fn suspend_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .suspend_member(holder_id, jurisdiction_id)
            .await
    }

    /// Reinstate a suspended member.
    pub async fn reinstate_member(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .reinstate_member(holder_id, jurisdiction_id)
            .await
    }

    /// Grant a capability to a member.
    pub async fn grant_capability(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .grant_capability(holder_id, jurisdiction_id, capability)
            .await
    }

    /// Revoke a capability from a member.
    pub async fn revoke_capability(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .revoke_capability(holder_id, jurisdiction_id, capability)
            .await
    }

    /// Add a role to a member.
    pub async fn add_member_role(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        role: String,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .add_member_role(holder_id, jurisdiction_id, role)
            .await
    }

    /// Remove a role from a member.
    pub async fn remove_member_role(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        role: &str,
    ) -> Result<()> {
        self.inner
            .write()
            .await
            .remove_member_role(holder_id, jurisdiction_id, role)
            .await
    }

    /// Check if a member has a specific capability.
    pub async fn member_has_capability(
        &self,
        holder_id: &str,
        jurisdiction_id: &JurisdictionId,
        capability: MembershipCapability,
    ) -> Result<bool> {
        self.inner
            .read()
            .await
            .member_has_capability(holder_id, jurisdiction_id, capability)
            .await
    }

    /// List members of a jurisdiction by status.
    pub async fn list_members_by_status(
        &self,
        jurisdiction_id: &JurisdictionId,
        status: Option<MembershipStatus>,
    ) -> Vec<(String, Affiliation)> {
        self.inner
            .read()
            .await
            .list_members_by_status(jurisdiction_id, status)
            .await
    }
}

// ============================================================================
// Amendment Operations
// ============================================================================

impl CommonsHandle {
    /// Store an amendment.
    pub async fn store_amendment(&self, amendment: Amendment) -> Result<()> {
        self.inner.write().await.store_amendment(amendment).await
    }

    /// Add a change to a draft amendment.
    pub async fn add_amendment_change(
        &self,
        amendment_id: &str,
        change: AmendmentChange,
    ) -> Result<Amendment> {
        self.inner
            .write()
            .await
            .add_amendment_change(amendment_id, change)
            .await
    }

    /// Get an amendment by ID.
    pub async fn get_amendment(&self, id: &AmendmentId) -> Result<Option<Amendment>> {
        self.inner.read().await.get_amendment(id).await
    }

    /// List amendments with optional filters.
    pub async fn list_amendments(
        &self,
        status: Option<&str>,
        scope: Option<&str>,
        amendment_type: Option<&str>,
    ) -> Result<Vec<Amendment>> {
        self.inner
            .read()
            .await
            .list_amendments(status, scope, amendment_type)
            .await
    }

    /// Submit an amendment for review.
    pub async fn submit_amendment(&self, id: &AmendmentId, caller: &Did) -> Result<Amendment> {
        self.inner.write().await.submit_amendment(id, caller).await
    }

    /// Open voting on an amendment (skipping review for simple cases).
    pub async fn open_amendment_voting(&self, id: &AmendmentId, caller: &Did) -> Result<Amendment> {
        self.inner
            .write()
            .await
            .open_amendment_voting(id, caller)
            .await
    }

    /// Add a ratification to an amendment.
    pub async fn add_amendment_ratification(
        &self,
        id: &AmendmentId,
        ratification: Ratification,
    ) -> Result<Amendment> {
        self.inner
            .write()
            .await
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
        self.inner
            .write()
            .await
            .withdraw_amendment(id, caller, reason)
            .await
    }
}

// ============================================================================
// Appeal Operations
// ============================================================================

impl CommonsHandle {
    /// Store an appeal.
    pub async fn store_appeal(&self, appeal: Appeal) -> Result<()> {
        self.inner.write().await.store_appeal(appeal).await
    }

    /// Get an appeal by ID.
    pub async fn get_appeal(&self, id: &AppealId) -> Result<Option<Appeal>> {
        self.inner.read().await.get_appeal(id).await
    }

    /// List appeals with optional filters.
    pub async fn list_appeals(
        &self,
        status: Option<&str>,
        scope: Option<&str>,
        appellant: Option<&str>,
    ) -> Result<Vec<Appeal>> {
        self.inner
            .read()
            .await
            .list_appeals(status, scope, appellant)
            .await
    }

    /// Add evidence to an appeal.
    pub async fn add_appeal_evidence(
        &self,
        id: &AppealId,
        evidence: AppealEvidence,
    ) -> Result<Appeal> {
        self.inner
            .write()
            .await
            .add_appeal_evidence(id, evidence)
            .await
    }

    /// Add response to an appeal.
    pub async fn add_appeal_response(
        &self,
        id: &AppealId,
        response: AppealResponse,
    ) -> Result<Appeal> {
        self.inner
            .write()
            .await
            .add_appeal_response(id, response)
            .await
    }

    /// Begin review of an appeal.
    pub async fn begin_appeal_review(&self, id: &AppealId) -> Result<Appeal> {
        self.inner.write().await.begin_appeal_review(id).await
    }

    /// Resolve a constitutional appeal with outcome.
    pub async fn resolve_constitutional_appeal(
        &self,
        id: &AppealId,
        outcome: AppealOutcome,
    ) -> Result<Appeal> {
        self.inner
            .write()
            .await
            .resolve_constitutional_appeal(id, outcome)
            .await
    }

    /// Withdraw an appeal.
    pub async fn withdraw_appeal(
        &self,
        id: &AppealId,
        caller: &Did,
        reason: Option<String>,
    ) -> Result<Appeal> {
        self.inner
            .write()
            .await
            .withdraw_appeal(id, caller, reason)
            .await
    }
}
