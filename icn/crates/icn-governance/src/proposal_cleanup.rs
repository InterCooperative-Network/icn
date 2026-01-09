//! Proposal cleanup and archival
//!
//! Provides automatic cleanup of expired proposals to prevent unbounded storage growth.
//! Expired proposals are archived to summaries for audit trail, then removed from
//! active indices.
//!
//! # Retention Policy
//!
//! Proposals go through three lifecycle phases:
//! 1. **Active**: Open for voting, stored with full details
//! 2. **Archived**: Closed proposals, summarized for audit trail
//! 3. **Deleted**: Removed after archive retention period
//!
//! # Example
//!
//! ```ignore
//! use icn_governance::proposal_cleanup::{ProposalCleanupTask, ProposalRetention};
//!
//! let retention = ProposalRetention::default();
//! let task = ProposalCleanupTask::new(store, retention);
//!
//! // Run cleanup (typically called periodically)
//! let stats = task.run_cleanup()?;
//! println!("Archived {} proposals, deleted {} archives", stats.archived, stats.deleted);
//! ```

use crate::{GovernanceDomainId, Proposal, ProposalId, ProposalOutcome, ProposalState, Timestamp};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Default active TTL: 30 days (max voting period)
pub const DEFAULT_ACTIVE_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Default archive TTL: 1 year
pub const DEFAULT_ARCHIVE_TTL_SECS: u64 = 365 * 24 * 60 * 60;

/// Default delete after: 2 years
pub const DEFAULT_DELETE_AFTER_SECS: u64 = 730 * 24 * 60 * 60;

/// Retention policy for proposals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRetention {
    /// Maximum time a closed proposal remains in full form (seconds)
    /// After this, it's archived to summary
    pub active_ttl_secs: u64,

    /// How long to keep archived summaries (seconds)
    /// After this, they can be fully deleted
    pub archive_ttl_secs: u64,

    /// Total time before full deletion (seconds)
    /// Must be >= active_ttl + archive_ttl
    pub delete_after_secs: u64,
}

impl Default for ProposalRetention {
    fn default() -> Self {
        Self {
            active_ttl_secs: DEFAULT_ACTIVE_TTL_SECS,     // 30 days
            archive_ttl_secs: DEFAULT_ARCHIVE_TTL_SECS,   // 1 year
            delete_after_secs: DEFAULT_DELETE_AFTER_SECS, // 2 years
        }
    }
}

impl ProposalRetention {
    /// Create a retention policy for testing with shorter periods
    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self {
            active_ttl_secs: 60,    // 1 minute
            archive_ttl_secs: 120,  // 2 minutes
            delete_after_secs: 180, // 3 minutes
        }
    }

    /// Check if a closed proposal should be archived
    pub fn should_archive(&self, proposal: &Proposal, now: Timestamp) -> bool {
        if let Some(closed_at) = proposal.closed_at() {
            now.saturating_sub(closed_at) >= self.active_ttl_secs
        } else {
            false
        }
    }

    /// Check if an archive should be deleted
    ///
    /// Deletion is based on total time since the proposal was closed
    /// (`delete_after_secs`), not since it was archived. This ensures
    /// consistent retention regardless of when archiving runs.
    pub fn should_delete(&self, archive: &ProposalArchive, now: Timestamp) -> bool {
        now.saturating_sub(archive.closed_at) >= self.delete_after_secs
    }

    /// Validate that retention constraints are satisfied
    ///
    /// Returns an error if `delete_after_secs < active_ttl_secs + archive_ttl_secs`,
    /// which would cause archives to be deleted before their expected retention.
    pub fn validate(&self) -> Result<(), String> {
        let min_delete_after = self.active_ttl_secs.saturating_add(self.archive_ttl_secs);
        if self.delete_after_secs < min_delete_after {
            return Err(format!(
                "delete_after_secs ({}) must be >= active_ttl_secs ({}) + archive_ttl_secs ({})",
                self.delete_after_secs, self.active_ttl_secs, self.archive_ttl_secs
            ));
        }
        Ok(())
    }
}

/// Archived summary of a closed proposal
///
/// Contains minimal information for audit trail while removing bulk data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalArchive {
    /// Original proposal ID
    pub id: ProposalId,

    /// Domain the proposal belonged to
    pub domain_id: GovernanceDomainId,

    /// Proposal title
    pub title: String,

    /// Who created the proposal
    pub proposer: Did,

    /// Type of proposal (e.g., "text", "budget", "membership")
    pub proposal_type: String,

    /// Final outcome
    pub outcome: ProposalOutcome,

    /// Final vote tally
    pub tally: ArchivedTally,

    /// When the proposal was created
    pub created_at: Timestamp,

    /// When the proposal was closed
    pub closed_at: Timestamp,

    /// When this archive was created
    pub archived_at: Timestamp,
}

/// Minimal vote tally for archives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedTally {
    /// Votes in favor
    pub for_votes: usize,
    /// Votes against
    pub against_votes: usize,
    /// Abstentions
    pub abstain_votes: usize,
}

impl ProposalArchive {
    /// Create an archive from a closed proposal
    ///
    /// Returns an error if the proposal is not in a closed state.
    pub fn from_proposal(proposal: &Proposal, tally: &crate::VoteTally) -> Result<Self, String> {
        let closed_at = proposal.closed_at().ok_or_else(|| {
            format!(
                "Proposal {} is not closed (state: {:?}), cannot archive",
                proposal.id, proposal.state
            )
        })?;
        let outcome = proposal.outcome().ok_or_else(|| {
            format!(
                "Proposal {} has no outcome despite being closed, cannot archive",
                proposal.id
            )
        })?;

        Ok(Self {
            id: proposal.id.clone(),
            domain_id: proposal.domain_id.clone(),
            title: proposal.title.clone(),
            proposer: proposal.proposer.clone(),
            proposal_type: proposal.payload.type_name().to_string(),
            outcome,
            tally: ArchivedTally {
                for_votes: tally.for_votes,
                against_votes: tally.against_votes,
                abstain_votes: tally.abstain_votes,
            },
            created_at: proposal.created_at,
            closed_at,
            archived_at: icn_time::current_timestamp_secs(),
        })
    }
}

/// Statistics from a cleanup run
#[derive(Debug, Clone, Default)]
pub struct CleanupStats {
    /// Number of proposals scanned
    pub scanned: usize,
    /// Number of proposals archived
    pub archived: usize,
    /// Number of archives deleted
    pub deleted: usize,
    /// Errors encountered (non-fatal)
    pub errors: usize,
}

/// Proposal cleanup task
///
/// Periodically scans for expired proposals and performs cleanup according
/// to the retention policy.
pub struct ProposalCleanupTask<S> {
    store: S,
    retention: ProposalRetention,
}

impl<S: crate::GovernanceStore> ProposalCleanupTask<S> {
    /// Create a new cleanup task
    pub fn new(store: S, retention: ProposalRetention) -> Self {
        Self { store, retention }
    }

    /// Run a cleanup cycle
    ///
    /// 1. Finds closed proposals past active TTL and archives them
    /// 2. Finds archives past archive TTL and deletes them
    /// 3. Returns statistics about what was cleaned up
    pub fn run_cleanup(&self) -> anyhow::Result<CleanupStats> {
        let now = icn_time::current_timestamp_secs();
        let mut stats = CleanupStats::default();

        // Get all domains to scan their proposals
        let domains = self.store.list_domains()?;

        for domain in domains {
            let proposals = self.store.list_proposals(&domain.id)?;
            stats.scanned += proposals.len();

            for proposal in proposals {
                // Skip proposals that aren't closed
                if !proposal.state.is_closed() {
                    continue;
                }

                // Check if should be archived
                if self.retention.should_archive(&proposal, now) {
                    match self.archive_proposal(&proposal) {
                        Ok(()) => {
                            stats.archived += 1;
                            debug!(
                                proposal_id = %proposal.id,
                                "Archived proposal"
                            );
                        }
                        Err(e) => {
                            stats.errors += 1;
                            tracing::warn!(
                                proposal_id = %proposal.id,
                                error = %e,
                                "Failed to archive proposal"
                            );
                        }
                    }
                }
            }
        }

        // Check archives for deletion
        let archives = self.list_archives()?;
        for archive in archives {
            if self.retention.should_delete(&archive, now) {
                match self.delete_archive(&archive.id) {
                    Ok(()) => {
                        stats.deleted += 1;
                        debug!(
                            proposal_id = %archive.id,
                            "Deleted archive"
                        );
                    }
                    Err(e) => {
                        stats.errors += 1;
                        tracing::warn!(
                            proposal_id = %archive.id,
                            error = %e,
                            "Failed to delete archive"
                        );
                    }
                }
            }
        }

        if stats.archived > 0 || stats.deleted > 0 {
            info!(
                scanned = stats.scanned,
                archived = stats.archived,
                deleted = stats.deleted,
                errors = stats.errors,
                "Proposal cleanup completed"
            );
        }

        // TODO: Add metrics when icn-obs governance metrics are extended
        // icn_obs::metrics::governance::proposals_archived_inc_by(stats.archived as u64);
        // icn_obs::metrics::governance::archives_deleted_inc_by(stats.deleted as u64);

        Ok(stats)
    }

    /// Archive a single proposal
    fn archive_proposal(&self, proposal: &Proposal) -> anyhow::Result<()> {
        // Compute final tally
        let tally = self.store.compute_tally(&proposal.id)?;

        // Create archive
        let archive =
            ProposalArchive::from_proposal(proposal, &tally).map_err(|e| anyhow::anyhow!(e))?;

        // Store archive (currently a stub - see store_archive docs)
        self.store_archive(&archive)?;

        // Remove original proposal from store
        //
        // Note: We don't remove votes since they may be referenced by:
        // - Delegation verification (checking if delegatee voted)
        // - Member participation metrics (voting history)
        // - Dispute evidence (proving how someone voted)
        //
        // Vote cleanup could be a separate scheduled task that runs after
        // a longer retention period (e.g., 5 years) or when all referencing
        // systems have been updated. This is documented as a future
        // consideration for storage optimization.
        //
        // TODO: Once store_archive is fully implemented, the proposal index
        // should be updated there to maintain consistency.
        self.remove_proposal(&proposal.id)?;

        Ok(())
    }

    /// Store an archive (implementation depends on store type)
    ///
    /// **Note: This is currently a stub implementation.** A full implementation
    /// requires extending `GovernanceStore` trait with archive-specific methods:
    /// - `store_archive(archive: &ProposalArchive) -> Result<()>`
    /// - `get_archive(id: &ProposalId) -> Result<Option<ProposalArchive>>`
    /// - `list_archives(domain: &GovernanceDomainId) -> Result<Vec<ProposalArchive>>`
    /// - `delete_archive(id: &ProposalId) -> Result<()>`
    fn store_archive(&self, archive: &ProposalArchive) -> anyhow::Result<()> {
        let key = format!("archive:{}", archive.id.0);
        let value = serde_json::to_vec(archive)?;

        // Stub: log the archive that would be stored
        debug!(key = %key, "Would store archive (size: {} bytes)", value.len());

        Ok(())
    }

    /// Remove a proposal from the store
    ///
    /// **Note: This is currently a stub implementation.** A full implementation
    /// requires adding `delete_proposal(id: &ProposalId) -> Result<()>` to
    /// the `GovernanceStore` trait. Until then, proposals remain in the store
    /// after archival.
    fn remove_proposal(&self, _id: &ProposalId) -> anyhow::Result<()> {
        // Stub: proposals are not actually removed yet
        // Returning Ok(()) to allow the archival workflow to complete,
        // but proposals will remain in the store until trait is extended.
        Ok(())
    }

    /// List all archives
    ///
    /// **Note: This is currently a stub implementation.** Returns empty until
    /// `GovernanceStore` trait is extended with archive storage methods.
    fn list_archives(&self) -> anyhow::Result<Vec<ProposalArchive>> {
        // Stub: no archive storage implemented yet
        // A full implementation would scan the archive prefix
        Ok(Vec::new())
    }

    /// Delete an archive
    ///
    /// **Note: This is currently a stub implementation.** A full implementation
    /// requires adding `delete_archive(id: &ProposalId) -> Result<()>` to
    /// the `GovernanceStore` trait.
    fn delete_archive(&self, _id: &ProposalId) -> anyhow::Result<()> {
        // Stub: archives are not actually deleted yet
        Ok(())
    }
}

// Extension methods for Proposal to support cleanup operations.
//
// These methods are defined here rather than in `proposal.rs` because they are
// only needed for the cleanup workflow. If cleanup becomes more integrated with
// the core Proposal type, consider moving these to the main impl block.
impl Proposal {
    /// Get the timestamp when the proposal was closed
    pub fn closed_at(&self) -> Option<Timestamp> {
        match &self.state {
            ProposalState::Accepted { closed_at } => Some(*closed_at),
            ProposalState::Rejected { closed_at } => Some(*closed_at),
            ProposalState::NoQuorum { closed_at } => Some(*closed_at),
            ProposalState::Cancelled { cancelled_at } => Some(*cancelled_at),
            ProposalState::Vetoed { vetoed_at, .. } => Some(*vetoed_at),
            ProposalState::ForceClosed { closed_at, .. } => Some(*closed_at),
            _ => None,
        }
    }

    /// Get the outcome of a closed proposal
    ///
    /// # Design Note: Cancelled and Vetoed Mapping
    ///
    /// The `Cancelled` and `Vetoed` states are mapped to `ProposalOutcome::Rejected`
    /// because `ProposalOutcome` only has three variants: `Accepted`, `Rejected`,
    /// and `NoQuorum`. This is a deliberate simplification for archival purposes:
    ///
    /// - **Cancelled**: The proposer withdrew the proposal (voluntary rejection)
    /// - **Vetoed**: Emergency override by authorized party (administrative rejection)
    /// - **Rejected**: Democratic vote result
    ///
    /// While these are semantically different, they all result in the proposal not
    /// being enacted. If audit trails need to distinguish between these outcomes,
    /// consider extending `ProposalOutcome` to include `Cancelled` and `Vetoed`
    /// variants in a future iteration.
    pub fn outcome(&self) -> Option<ProposalOutcome> {
        match &self.state {
            ProposalState::Accepted { .. } => Some(ProposalOutcome::Accepted),
            ProposalState::Rejected { .. } => Some(ProposalOutcome::Rejected),
            ProposalState::NoQuorum { .. } => Some(ProposalOutcome::NoQuorum),
            // Cancelled and Vetoed map to Rejected (see doc comment above)
            ProposalState::Cancelled { .. } => Some(ProposalOutcome::Rejected),
            ProposalState::Vetoed { .. } => Some(ProposalOutcome::Rejected),
            ProposalState::ForceClosed { outcome, .. } => Some(outcome.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        store::{GovernanceStore, InMemoryGovernanceStore},
        GovernanceDomainId, ProposalPayload,
    };
    use icn_identity::KeyPair;

    fn test_proposal(domain_id: &GovernanceDomainId) -> Proposal {
        let kp = KeyPair::generate().unwrap();
        Proposal::new(
            domain_id.clone(),
            kp.did().clone(),
            "Test Proposal".to_string(),
            "A test proposal".to_string(),
            ProposalPayload::Text {
                body: "Should we do this?".to_string(),
            },
        )
    }

    #[test]
    fn test_retention_default() {
        let retention = ProposalRetention::default();
        assert_eq!(retention.active_ttl_secs, DEFAULT_ACTIVE_TTL_SECS);
        assert_eq!(retention.archive_ttl_secs, DEFAULT_ARCHIVE_TTL_SECS);
        assert_eq!(retention.delete_after_secs, DEFAULT_DELETE_AFTER_SECS);
    }

    #[test]
    fn test_should_archive_not_closed() {
        let retention = ProposalRetention::for_testing();
        let domain_id = GovernanceDomainId::new("test");
        let proposal = test_proposal(&domain_id);

        // Draft proposal should not be archived
        let now = icn_time::current_timestamp_secs();
        assert!(!retention.should_archive(&proposal, now));
    }

    #[test]
    fn test_should_archive_recently_closed() {
        let retention = ProposalRetention::for_testing();
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        // Open and close the proposal
        proposal.open(3600).unwrap();
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Accepted { closed_at: now })
            .unwrap();

        // Just closed, should not be archived yet
        assert!(!retention.should_archive(&proposal, now));
    }

    #[test]
    fn test_should_archive_past_ttl() {
        let retention = ProposalRetention::for_testing();
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        // Open and close the proposal
        proposal.open(3600).unwrap();
        let closed_at = icn_time::current_timestamp_secs() - 120; // 2 minutes ago
        proposal
            .close(ProposalState::Accepted { closed_at })
            .unwrap();

        // Closed 2 minutes ago, active TTL is 1 minute, should be archived
        let now = icn_time::current_timestamp_secs();
        assert!(retention.should_archive(&proposal, now));
    }

    #[test]
    fn test_proposal_closed_at() {
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        // Draft has no closed_at
        assert!(proposal.closed_at().is_none());

        // Open and close
        proposal.open(3600).unwrap();
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Rejected { closed_at: now })
            .unwrap();

        assert_eq!(proposal.closed_at(), Some(now));
    }

    #[test]
    fn test_proposal_outcome() {
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        // Draft has no outcome
        assert!(proposal.outcome().is_none());

        // Open and accept
        proposal.open(3600).unwrap();
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Accepted { closed_at: now })
            .unwrap();

        assert_eq!(proposal.outcome(), Some(ProposalOutcome::Accepted));
    }

    #[test]
    fn test_archive_from_proposal() {
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        // Open and close
        proposal.open(3600).unwrap();
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Accepted { closed_at: now })
            .unwrap();

        let tally = crate::VoteTally::new(10, 3, 2);

        let archive = ProposalArchive::from_proposal(&proposal, &tally).unwrap();

        assert_eq!(archive.id, proposal.id);
        assert_eq!(archive.domain_id, domain_id);
        assert_eq!(archive.title, "Test Proposal");
        assert_eq!(archive.outcome, ProposalOutcome::Accepted);
        assert_eq!(archive.tally.for_votes, 10);
        assert_eq!(archive.tally.against_votes, 3);
    }

    #[test]
    fn test_cleanup_task_empty() {
        let store = InMemoryGovernanceStore::new();
        let retention = ProposalRetention::for_testing();
        let task = ProposalCleanupTask::new(store, retention);

        let stats = task.run_cleanup().unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.archived, 0);
        assert_eq!(stats.deleted, 0);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_cleanup_task_skips_open() {
        let store = InMemoryGovernanceStore::new();
        let retention = ProposalRetention::for_testing();

        // Create a domain and an open proposal
        let config = crate::GovernanceConfig::cooperative_default();
        let domain = crate::GovernanceDomain::new("Test".to_string(), config);
        store.store_domain(&domain).unwrap();

        let mut proposal = test_proposal(&domain.id);
        proposal.open(3600).unwrap();
        store.store_proposal(&proposal).unwrap();

        let task = ProposalCleanupTask::new(store, retention);
        let stats = task.run_cleanup().unwrap();

        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.archived, 0); // Open proposals are not archived
    }

    #[test]
    fn test_cleanup_task_archives_expired() {
        let store = InMemoryGovernanceStore::new();
        let retention = ProposalRetention::for_testing();

        // Create a domain and a closed proposal
        let config = crate::GovernanceConfig::cooperative_default();
        let domain = crate::GovernanceDomain::new("Test".to_string(), config);
        store.store_domain(&domain).unwrap();

        let mut proposal = test_proposal(&domain.id);
        proposal.open(3600).unwrap();

        // Close it 2 minutes ago (past the 1-minute active TTL)
        let closed_at = icn_time::current_timestamp_secs() - 120;
        proposal
            .close(ProposalState::Accepted { closed_at })
            .unwrap();
        store.store_proposal(&proposal).unwrap();

        let task = ProposalCleanupTask::new(store, retention);
        let stats = task.run_cleanup().unwrap();

        assert_eq!(stats.scanned, 1);
        assert_eq!(stats.archived, 1);
    }

    #[test]
    fn test_archive_serialization() {
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        proposal.open(3600).unwrap();
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Accepted { closed_at: now })
            .unwrap();

        let tally = crate::VoteTally::new(5, 2, 1);

        let archive = ProposalArchive::from_proposal(&proposal, &tally).unwrap();

        // Serialize and deserialize
        let json = serde_json::to_string(&archive).unwrap();
        let parsed: ProposalArchive = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id.0, archive.id.0);
        assert_eq!(parsed.outcome, archive.outcome);
        assert_eq!(parsed.tally.for_votes, archive.tally.for_votes);
    }

    #[test]
    fn test_retention_validation_valid() {
        let retention = ProposalRetention::default();
        assert!(retention.validate().is_ok());

        let retention = ProposalRetention::for_testing();
        assert!(retention.validate().is_ok());
    }

    #[test]
    fn test_retention_validation_invalid() {
        let retention = ProposalRetention {
            active_ttl_secs: 100,
            archive_ttl_secs: 100,
            delete_after_secs: 150, // Less than 100 + 100 = 200
        };
        let result = retention.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be >="));
    }

    #[test]
    fn test_from_proposal_error_on_open() {
        let domain_id = GovernanceDomainId::new("test");
        let mut proposal = test_proposal(&domain_id);

        // Open but not closed
        proposal.open(3600).unwrap();

        let tally = crate::VoteTally::new(0, 0, 0);
        let result = ProposalArchive::from_proposal(&proposal, &tally);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not closed"));
    }

    #[test]
    fn test_should_delete_uses_closed_at() {
        let retention = ProposalRetention {
            active_ttl_secs: 60,
            archive_ttl_secs: 120,
            delete_after_secs: 300, // 5 minutes from close
        };

        let now = icn_time::current_timestamp_secs();

        // Archive created 2 minutes ago, closed 4 minutes ago
        let archive = ProposalArchive {
            id: ProposalId("test".to_string()),
            domain_id: GovernanceDomainId::new("test"),
            title: "Test".to_string(),
            proposer: icn_identity::KeyPair::generate().unwrap().did().clone(),
            proposal_type: "text".to_string(),
            outcome: ProposalOutcome::Accepted,
            tally: ArchivedTally {
                for_votes: 5,
                against_votes: 2,
                abstain_votes: 0,
            },
            created_at: now - 600,
            closed_at: now - 240, // 4 minutes ago (< 5 min delete_after)
            archived_at: now - 120,
        };

        // Should NOT be deleted - only 4 minutes since close, need 5
        assert!(!retention.should_delete(&archive, now));

        // Archive closed 6 minutes ago
        let archive_old = ProposalArchive {
            closed_at: now - 360, // 6 minutes ago (> 5 min delete_after)
            ..archive.clone()
        };

        // SHOULD be deleted - 6 minutes since close, delete_after is 5
        assert!(retention.should_delete(&archive_old, now));
    }

    #[test]
    fn test_cleanup_task_delete_phase_stub() {
        // This test verifies the deletion phase exists but is a stub.
        // Once list_archives is implemented, this test should be updated
        // to verify actual deletion behavior.
        let store = InMemoryGovernanceStore::new();
        let retention = ProposalRetention::for_testing();

        // No domains/proposals, but run_cleanup should still process
        // the (empty) archive deletion phase without errors
        let task = ProposalCleanupTask::new(store, retention);
        let stats = task.run_cleanup().unwrap();

        // Currently list_archives returns empty, so no deletions
        assert_eq!(stats.deleted, 0);
    }
}
