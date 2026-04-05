//! Kernel-safe resource access grant store trait.
//!
//! The kernel records governance-authorized resource access grants here so that:
//! 1. The governance effect path persists grants with decision provenance.
//! 2. The `ResourceAccessEnforcerActor` can later query grants for idle-revocation.
//!
//! # Pattern
//!
//! Follows the same pattern as [`crate::budget::BudgetStore`]:
//! - Trait defined here (kernel API boundary).
//! - Sled-backed implementation lives in `apps/ledger`.
//! - Injected into `KernelGovernanceExecutor` via `with_resource_access_store()`.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A persisted resource access grant record.
///
/// Carries the governance decision hash for a complete audit trail from
/// proposal → effect → persisted grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAccessRecord {
    /// The resource type (opaque string; domain-defined by the app layer).
    pub resource_type: String,
    /// DID of the entity that was granted access.
    pub grantee_did: String,
    /// Blake3 hash of the access model (for verifiable audit without importing domain types).
    pub access_model_hash: String,
    /// When access was granted (seconds since epoch).
    pub granted_at: u64,
    /// Governance decision hash — links this grant to the proposal that authorized it.
    pub decision_hash: String,
    /// Whether this grant has been revoked.
    pub is_revoked: bool,
    /// When the grant was revoked (seconds since epoch), if applicable.
    pub revoked_at: Option<u64>,
    /// Reason for revocation, if revoked.
    pub revocation_reason: Option<String>,
}

/// Store for governance-authorized resource access grants.
///
/// # Idempotency
///
/// `grant()` is idempotent on `(resource_type, grantee_did, decision_hash)`:
/// if a record with the same key already exists with the same decision hash,
/// implementations MUST return `Ok(())` without modifying the record.
pub trait ResourceAccessStore: Send + Sync {
    /// Persist a new access grant.
    ///
    /// Idempotent: if a record already exists for `(resource_type, grantee_did)`
    /// with the same `decision_hash`, this is a no-op and returns `Ok(())`.
    fn grant(&self, record: &ResourceAccessRecord) -> Result<()>;

    /// Mark an existing grant as revoked.
    ///
    /// No-op if the grant does not exist or is already revoked.
    fn revoke(
        &self,
        resource_type: &str,
        grantee_did: &str,
        revoked_at: u64,
        reason: &str,
    ) -> Result<()>;

    /// Get the current record for a specific `(resource_type, grantee_did)` pair.
    fn get(&self, resource_type: &str, grantee_did: &str) -> Result<Option<ResourceAccessRecord>>;

    /// List all active (non-revoked) grants.
    ///
    /// Used by `LedgerService::list_enforceable_resources()` to feed the
    /// `ResourceAccessEnforcerActor` with governance-granted resources.
    fn list_active(&self) -> Result<Vec<ResourceAccessRecord>>;
}
