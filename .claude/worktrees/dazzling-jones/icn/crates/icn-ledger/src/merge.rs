//! Merge decision reporting for ledger synchronization visibility
//!
//! This module provides data structures for reporting what happened during ledger
//! merge operations, including conflicts, quarantines, and discarded entries.
//!
//! ## Purpose
//!
//! When gossip synchronization brings in new ledger entries, the ledger must
//! deterministically merge them into the canonical chain. This process may:
//! - Accept entries and extend the canonical chain
//! - Discard entries that are redundant or superseded
//! - Quarantine entries that violate invariants or limits
//! - Detect conflicts where multiple entries compete for the same position
//!
//! The `MergeDecision` captures all these outcomes, providing observability
//! for operators, auditing, and conflict resolution workflows.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let decision = ledger.merge_batch(incoming_entries)?;
//!
//! // Check for quarantined entries
//! if !decision.quarantined.is_empty() {
//!     for item in &decision.quarantined {
//!         warn!("Entry {} quarantined: {:?}", item.entry_id, item.reason);
//!     }
//! }
//!
//! // Check for conflicts
//! if !decision.conflicts.is_empty() {
//!     for conflict in &decision.conflicts {
//!         info!("Conflict resolved: {} (winner) vs {} (loser)",
//!               conflict.winner, conflict.loser);
//!     }
//! }
//! ```

use crate::types::{ContentHash, QuarantineReason};
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Decision record from merging a batch of entries into the ledger
///
/// This captures all outcomes: accepted, discarded, quarantined, and conflicts.
/// Emitted after each merge operation for observability and audit trails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeDecision {
    /// Current tip of the canonical chain after merge
    pub canonical_chain_tip: ContentHash,

    /// Entries discarded (redundant or superseded)
    pub discarded: Vec<ContentHash>,

    /// Entries quarantined (violate invariants or limits)
    pub quarantined: Vec<QuarantineItem>,

    /// Conflicts detected and resolved
    pub conflicts: Vec<ConflictPair>,

    /// Timestamp of the merge operation
    pub timestamp: u64,

    /// Number of entries successfully accepted
    pub accepted_count: u32,
}

impl MergeDecision {
    /// Create a new merge decision
    pub fn new(canonical_chain_tip: ContentHash) -> Self {
        MergeDecision {
            canonical_chain_tip,
            discarded: Vec::new(),
            quarantined: Vec::new(),
            conflicts: Vec::new(),
            timestamp: icn_time::current_timestamp_secs(),
            accepted_count: 0,
        }
    }

    /// Add a discarded entry
    pub fn add_discarded(&mut self, entry_id: ContentHash) {
        self.discarded.push(entry_id);
    }

    /// Add a quarantined entry
    pub fn add_quarantined(&mut self, item: QuarantineItem) {
        self.quarantined.push(item);
    }

    /// Add a conflict resolution
    pub fn add_conflict(&mut self, conflict: ConflictPair) {
        self.conflicts.push(conflict);
    }

    /// Increment accepted count
    pub fn increment_accepted(&mut self) {
        self.accepted_count += 1;
    }

    /// Check if there were any issues (quarantines or conflicts)
    pub fn has_issues(&self) -> bool {
        !self.quarantined.is_empty() || !self.conflicts.is_empty()
    }

    /// Total number of entries processed
    pub fn total_processed(&self) -> u32 {
        self.accepted_count + self.discarded.len() as u32 + self.quarantined.len() as u32
    }
}

/// Item quarantined during merge operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineItem {
    /// Content hash of the quarantined entry
    pub entry_id: ContentHash,

    /// Why it was quarantined
    pub reason: QuarantineReason,

    /// Author of the entry
    pub author: Did,

    /// When the entry was observed (UNIX timestamp)
    pub observed_at: u64,

    /// Optional metadata (e.g., "balance would be -150, limit -100")
    pub metadata: Option<String>,
}

impl QuarantineItem {
    /// Create a new quarantine item
    pub fn new(entry_id: ContentHash, reason: QuarantineReason, author: Did) -> Self {
        QuarantineItem {
            entry_id,
            reason,
            author,
            observed_at: icn_time::current_timestamp_secs(),
            metadata: None,
        }
    }

    /// Create with metadata
    pub fn with_metadata(mut self, metadata: String) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Conflict pair showing which entry won and which lost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictPair {
    /// Content hash of the losing entry (discarded or quarantined)
    pub loser: ContentHash,

    /// Content hash of the winning entry (kept in canonical chain)
    pub winner: ContentHash,

    /// Reason for the resolution (e.g., "earlier timestamp", "higher trust")
    pub resolution_reason: String,
}

impl ConflictPair {
    /// Create a new conflict pair
    pub fn new(loser: ContentHash, winner: ContentHash, resolution_reason: String) -> Self {
        ConflictPair {
            loser,
            winner,
            resolution_reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_merge_decision_creation() {
        let tip_hash = ContentHash::from_bytes([1u8; 32]);
        let decision = MergeDecision::new(tip_hash.clone());

        assert_eq!(decision.canonical_chain_tip, tip_hash);
        assert_eq!(decision.accepted_count, 0);
        assert!(!decision.has_issues());
        assert_eq!(decision.total_processed(), 0);
    }

    #[test]
    fn test_merge_decision_add_items() {
        let tip_hash = ContentHash::from_bytes([1u8; 32]);
        let mut decision = MergeDecision::new(tip_hash);

        // Add accepted entries
        decision.increment_accepted();
        decision.increment_accepted();
        assert_eq!(decision.accepted_count, 2);

        // Add discarded
        let discarded_hash = ContentHash::from_bytes([2u8; 32]);
        decision.add_discarded(discarded_hash.clone());
        assert_eq!(decision.discarded.len(), 1);

        // Add quarantined
        let quarantined_hash = ContentHash::from_bytes([3u8; 32]);
        let author = KeyPair::generate().unwrap().did().clone();
        let item = QuarantineItem::new(
            quarantined_hash,
            QuarantineReason::ExceedsCreditLimit,
            author,
        );
        decision.add_quarantined(item);
        assert_eq!(decision.quarantined.len(), 1);
        assert!(decision.has_issues());

        // Add conflict
        let winner_hash = ContentHash::from_bytes([4u8; 32]);
        let loser_hash = ContentHash::from_bytes([5u8; 32]);
        let conflict = ConflictPair::new(loser_hash, winner_hash, "earlier timestamp".to_string());
        decision.add_conflict(conflict);
        assert_eq!(decision.conflicts.len(), 1);

        // Check totals
        assert_eq!(decision.total_processed(), 4); // 2 accepted + 1 discarded + 1 quarantined
    }

    #[test]
    fn test_quarantine_item_with_metadata() {
        let entry_id = ContentHash::from_bytes([1u8; 32]);
        let author = KeyPair::generate().unwrap().did().clone();
        let item = QuarantineItem::new(
            entry_id.clone(),
            QuarantineReason::ExceedsCreditLimit,
            author.clone(),
        )
        .with_metadata("balance would be -150, limit -100".to_string());

        assert_eq!(item.entry_id, entry_id);
        assert_eq!(item.author, author);
        assert!(item.metadata.is_some());
        assert_eq!(item.metadata.unwrap(), "balance would be -150, limit -100");
    }

    #[test]
    fn test_conflict_pair() {
        let winner = ContentHash::from_bytes([1u8; 32]);
        let loser = ContentHash::from_bytes([2u8; 32]);
        let conflict = ConflictPair::new(loser.clone(), winner.clone(), "trust score".to_string());

        assert_eq!(conflict.winner, winner);
        assert_eq!(conflict.loser, loser);
        assert_eq!(conflict.resolution_reason, "trust score");
    }
}
