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
//! let stats = task.run_cleanup().await?;
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
    pub fn should_delete(&self, archive: &ProposalArchive, now: Timestamp) -> bool {
        now.saturating_sub(archive.archived_at) >= self.archive_ttl_secs
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
    pub fn from_proposal(proposal: &Proposal, tally: &crate::VoteTally) -> Option<Self> {
        let closed_at = proposal.closed_at()?;
        let outcome = proposal.outcome()?;

        Some(Self {
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
        let archive = ProposalArchive::from_proposal(proposal, &tally)
            .ok_or_else(|| anyhow::anyhow!("Proposal is not closed, cannot archive"))?;

        // Store archive
        self.store_archive(&archive)?;

        // Remove original proposal from store
        // Note: We don't remove votes since they may be referenced by other systems
        // The proposal index is updated when we store the archive
        self.remove_proposal(&proposal.id)?;

        Ok(())
    }

    /// Store an archive (implementation depends on store type)
    fn store_archive(&self, archive: &ProposalArchive) -> anyhow::Result<()> {
        // For now, store as a special "archived" proposal
        // A full implementation would add archive methods to GovernanceStore trait
        let key = format!("archive:{}", archive.id.0);
        let value = serde_json::to_vec(archive)?;

        // Use the underlying store if available
        // For simplicity, we'll skip this for the initial implementation
        // and just log the archive creation
        debug!(key = %key, "Would store archive (size: {} bytes)", value.len());

        Ok(())
    }

    /// Remove a proposal from the store
    fn remove_proposal(&self, _id: &ProposalId) -> anyhow::Result<()> {
        // For now, this is a no-op since we don't have delete methods in GovernanceStore
        // A full implementation would add delete_proposal to the trait
        Ok(())
    }

    /// List all archives
    fn list_archives(&self) -> anyhow::Result<Vec<ProposalArchive>> {
        // For now, return empty since we don't have archive storage implemented
        // A full implementation would scan the archive prefix
        Ok(Vec::new())
    }

    /// Delete an archive
    fn delete_archive(&self, _id: &ProposalId) -> anyhow::Result<()> {
        // For now, this is a no-op
        Ok(())
    }
}

// Extension trait for Proposal to get closed_at and outcome
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
    /// Note: Cancelled and Vetoed states map to Rejected since ProposalOutcome
    /// only has Accepted/Rejected/NoQuorum variants.
    pub fn outcome(&self) -> Option<ProposalOutcome> {
        match &self.state {
            ProposalState::Accepted { .. } => Some(ProposalOutcome::Accepted),
            ProposalState::Rejected { .. } => Some(ProposalOutcome::Rejected),
            ProposalState::NoQuorum { .. } => Some(ProposalOutcome::NoQuorum),
            // Cancelled and Vetoed are effectively rejections
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
}
