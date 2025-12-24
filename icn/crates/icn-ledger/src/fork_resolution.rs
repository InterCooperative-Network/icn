//! Ledger Fork Resolution
//!
//! Phase 18 Week 5: Detects and resolves ledger forks when conflicting entries reference
//! the same parent(s) but have different content hashes.
//!
//! A fork occurs when two nodes create different entries building on the same parent during
//! network partition or concurrent operations. This module provides multiple resolution
//! strategies based on trust, timestamps, and participant consensus.

use crate::types::{ContentHash, JournalEntry};
use anyhow::{anyhow, Result};
use icn_trust::TrustGraph;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info};

/// Strategy for resolving ledger forks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ForkResolutionStrategy {
    /// Prefer entry with earlier timestamp (first-write-wins)
    /// Best for: Simple coordination, low-trust environments
    TimestampPreference,

    /// Prefer entry from higher-trust participant
    /// Best for: Communities with established trust relationships
    TrustWeighted,

    /// Prefer entry with more co-signatures (participant consensus)
    /// Best for: Multi-party transactions requiring agreement
    MajoritySignatures,

    /// Combination strategy (trust-weighted + timestamp + signatures)
    /// Best for: Production deployments needing robust resolution
    #[default]
    Hybrid,
}

/// Detected fork with conflicting entries
#[derive(Debug, Clone)]
pub struct Fork {
    /// Common parent(s) that both entries reference
    pub common_parents: Vec<ContentHash>,

    /// First conflicting entry
    pub entry1: JournalEntry,

    /// Second conflicting entry
    pub entry2: JournalEntry,

    /// When the fork was detected
    pub detected_at: SystemTime,
}

/// Result of fork resolution
#[derive(Debug, Clone, PartialEq)]
pub enum ForkResolution {
    /// Keep entry1, discard entry2
    KeepFirst,

    /// Keep entry2, discard entry1
    KeepSecond,

    /// Cannot automatically resolve, requires manual intervention
    RequiresManual { reason: String },
}

/// Fork resolution system
pub struct ForkResolver {
    strategy: ForkResolutionStrategy,
    trust_graph: Option<Arc<tokio::sync::RwLock<TrustGraph>>>,
}

impl ForkResolver {
    /// Create a new fork resolver with the given strategy
    pub fn new(strategy: ForkResolutionStrategy) -> Self {
        Self {
            strategy,
            trust_graph: None,
        }
    }

    /// Set the trust graph for trust-weighted resolution
    pub fn set_trust_graph(&mut self, trust_graph: Arc<tokio::sync::RwLock<TrustGraph>>) {
        self.trust_graph = Some(trust_graph);
    }

    /// Detect if two entries form a fork (same parents, different hashes)
    pub fn is_fork(&self, entry1: &JournalEntry, entry2: &JournalEntry) -> bool {
        // Different entries (different hashes)
        if entry1.id == entry2.id {
            return false;
        }

        // Check if they share parent(s)
        let parents1: HashSet<_> = entry1.parents.iter().collect();
        let parents2: HashSet<_> = entry2.parents.iter().collect();

        // Fork if they have any common parents
        !parents1.is_disjoint(&parents2)
    }

    /// Resolve a detected fork using the configured strategy
    pub fn resolve_fork(&self, fork: &Fork) -> Result<ForkResolution> {
        info!(
            "Resolving fork with strategy {:?}: {} vs {}",
            self.strategy, fork.entry1.author, fork.entry2.author
        );

        match self.strategy {
            ForkResolutionStrategy::TimestampPreference => {
                self.resolve_by_timestamp(&fork.entry1, &fork.entry2)
            }
            ForkResolutionStrategy::TrustWeighted => {
                self.resolve_by_trust(&fork.entry1, &fork.entry2)
            }
            ForkResolutionStrategy::MajoritySignatures => {
                self.resolve_by_signatures(&fork.entry1, &fork.entry2)
            }
            ForkResolutionStrategy::Hybrid => self.resolve_hybrid(&fork.entry1, &fork.entry2),
        }
    }

    /// Resolve by timestamp (first-write-wins)
    fn resolve_by_timestamp(
        &self,
        entry1: &JournalEntry,
        entry2: &JournalEntry,
    ) -> Result<ForkResolution> {
        debug!(
            "Resolving by timestamp: {} vs {}",
            entry1.timestamp, entry2.timestamp
        );

        if entry1.timestamp < entry2.timestamp {
            Ok(ForkResolution::KeepFirst)
        } else if entry2.timestamp < entry1.timestamp {
            Ok(ForkResolution::KeepSecond)
        } else {
            // Timestamps are equal - use author DID as tiebreaker (deterministic)
            if entry1.author.to_string() < entry2.author.to_string() {
                Ok(ForkResolution::KeepFirst)
            } else {
                Ok(ForkResolution::KeepSecond)
            }
        }
    }

    /// Resolve by trust score
    fn resolve_by_trust(
        &self,
        entry1: &JournalEntry,
        entry2: &JournalEntry,
    ) -> Result<ForkResolution> {
        let trust_graph = self
            .trust_graph
            .as_ref()
            .ok_or_else(|| anyhow!("Trust graph required for trust-weighted resolution"))?;

        // Compute trust scores for both authors (using block_in_place for sync context)
        let trust_graph_clone = trust_graph.clone();
        let author1 = entry1.author.clone();
        let author2 = entry2.author.clone();
        let (trust1, trust2) = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let graph = trust_graph_clone.read().await;
                let t1 = graph.compute_trust_score(&author1).unwrap_or(0.0);
                let t2 = graph.compute_trust_score(&author2).unwrap_or(0.0);
                (t1, t2)
            })
        });

        debug!(
            "Resolving by trust: {} (trust={:.2}) vs {} (trust={:.2})",
            entry1.author, trust1, entry2.author, trust2
        );

        if trust1 > trust2 {
            Ok(ForkResolution::KeepFirst)
        } else if trust2 > trust1 {
            Ok(ForkResolution::KeepSecond)
        } else {
            // Equal trust - fall back to timestamp
            self.resolve_by_timestamp(entry1, entry2)
        }
    }

    /// Resolve by signature count (participant consensus)
    fn resolve_by_signatures(
        &self,
        entry1: &JournalEntry,
        entry2: &JournalEntry,
    ) -> Result<ForkResolution> {
        // Count unique signers for each entry
        // Note: Current JournalEntry only has author signature
        // In future, multi-party transactions would have multiple signatures
        let signers1 = self.count_signers(entry1);
        let signers2 = self.count_signers(entry2);

        debug!(
            "Resolving by signatures: {} signers vs {} signers",
            signers1, signers2
        );

        if signers1 > signers2 {
            Ok(ForkResolution::KeepFirst)
        } else if signers2 > signers1 {
            Ok(ForkResolution::KeepSecond)
        } else {
            // Equal signature count - fall back to timestamp
            self.resolve_by_timestamp(entry1, entry2)
        }
    }

    /// Resolve using hybrid strategy (combines all factors)
    fn resolve_hybrid(
        &self,
        entry1: &JournalEntry,
        entry2: &JournalEntry,
    ) -> Result<ForkResolution> {
        debug!("Resolving with hybrid strategy");

        // Calculate scores for each entry
        let score1 = self.calculate_hybrid_score(entry1)?;
        let score2 = self.calculate_hybrid_score(entry2)?;

        debug!("Hybrid scores: entry1={:.2}, entry2={:.2}", score1, score2);

        if score1 > score2 {
            Ok(ForkResolution::KeepFirst)
        } else if score2 > score1 {
            Ok(ForkResolution::KeepSecond)
        } else {
            // Scores are equal - use author DID as tiebreaker
            if entry1.author.to_string() < entry2.author.to_string() {
                Ok(ForkResolution::KeepFirst)
            } else {
                Ok(ForkResolution::KeepSecond)
            }
        }
    }

    /// Calculate hybrid score (trust + timestamp + signatures)
    fn calculate_hybrid_score(&self, entry: &JournalEntry) -> Result<f64> {
        let mut score = 0.0;

        // Trust component (40% weight)
        if let Some(ref trust_graph) = self.trust_graph {
            // Acquire read lock on trust graph (using block_in_place for sync context)
            let trust_graph_clone = trust_graph.clone();
            let author_clone = entry.author.clone();
            let trust = tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let graph = trust_graph_clone.read().await;
                    graph.compute_trust_score(&author_clone).unwrap_or(0.0)
                })
            });
            score += trust * 0.4;
        }

        // Timestamp component (30% weight) - favor earlier entries
        // Normalize timestamp to 0-1 range (assuming recent entries)
        let now = icn_time::current_timestamp_secs();
        let age_hours = (now.saturating_sub(entry.timestamp)) / 3600;
        let recency_score = if age_hours == 0 {
            1.0
        } else {
            1.0 / (age_hours as f64)
        };
        score += recency_score.min(1.0) * 0.3;

        // Signature component (30% weight)
        let signers = self.count_signers(entry);
        score += (signers as f64).min(10.0) / 10.0 * 0.3;

        Ok(score)
    }

    /// Count unique signers for an entry
    fn count_signers(&self, _entry: &JournalEntry) -> usize {
        // Current implementation: only author signature
        // Future: multi-party signatures would be counted here
        1
    }
}

/// Fork detector that scans the ledger for forks
pub struct ForkDetector {
    /// Map of parent hash -> list of entries that reference it
    parent_index: HashMap<ContentHash, Vec<ContentHash>>,
}

impl ForkDetector {
    /// Create a new fork detector
    pub fn new() -> Self {
        Self {
            parent_index: HashMap::new(),
        }
    }

    /// Index an entry (call this for each new entry)
    pub fn index_entry(&mut self, entry: &JournalEntry) {
        if let Some(entry_hash) = &entry.id {
            for parent in &entry.parents {
                self.parent_index
                    .entry(parent.clone())
                    .or_default()
                    .push(entry_hash.clone());
            }
        }
    }

    /// Detect forks by finding parents with multiple children
    pub fn detect_forks(&self) -> Vec<(ContentHash, Vec<ContentHash>)> {
        let mut forks = Vec::new();

        for (parent, children) in &self.parent_index {
            if children.len() > 1 {
                debug!(
                    "Detected fork: parent {} has {} children",
                    hex::encode(parent.0),
                    children.len()
                );
                forks.push((parent.clone(), children.clone()));
            }
        }

        forks
    }

    /// Check if a specific parent has a fork
    pub fn has_fork(&self, parent: &ContentHash) -> bool {
        self.parent_index
            .get(parent)
            .map(|children| children.len() > 1)
            .unwrap_or(false)
    }

    /// Get all children of a parent
    pub fn get_children(&self, parent: &ContentHash) -> Vec<ContentHash> {
        self.parent_index.get(parent).cloned().unwrap_or_default()
    }
}

impl Default for ForkDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AccountDelta;
    use icn_identity::{Did, KeyPair};

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn make_test_entry(author: Did, parents: Vec<ContentHash>, timestamp: u64) -> JournalEntry {
        // Create unique ID based on timestamp + author (simple hash)
        let mut id = [0u8; 32];
        id[0..8].copy_from_slice(&timestamp.to_le_bytes());
        id[8..16].copy_from_slice(&author.to_string().as_bytes()[0..8]);

        JournalEntry {
            id: Some(ContentHash(id)),
            timestamp,
            author,
            contract_ref: None,
            accounts: vec![AccountDelta {
                account_id: make_test_did(),
                debit: None,
                credit: Some(100),
                currency: "hours".to_string(),
            }],
            parents,
            signature: None,
        }
    }

    #[test]
    fn test_fork_detection() {
        let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);

        let parent = ContentHash([0u8; 32]);
        let author1 = make_test_did();
        let author2 = make_test_did();

        let entry1 = make_test_entry(author1, vec![parent.clone()], 1000);
        let entry2 = make_test_entry(author2, vec![parent.clone()], 2000);

        // Same parent, different hashes = fork
        assert!(resolver.is_fork(&entry1, &entry2));

        // Same entry = not a fork
        assert!(!resolver.is_fork(&entry1, &entry1));
    }

    #[test]
    fn test_timestamp_resolution() {
        let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);

        let parent = ContentHash([0u8; 32]);
        let author1 = make_test_did();
        let author2 = make_test_did();

        let entry1 = make_test_entry(author1.clone(), vec![parent.clone()], 1000);
        let entry2 = make_test_entry(author2.clone(), vec![parent.clone()], 2000);

        let fork = Fork {
            common_parents: vec![parent.clone()],
            entry1: entry1.clone(),
            entry2: entry2.clone(),
            detected_at: SystemTime::now(),
        };

        // Earlier timestamp should win
        let resolution = resolver.resolve_fork(&fork).unwrap();
        assert_eq!(resolution, ForkResolution::KeepFirst);

        // Reverse order
        let fork2 = Fork {
            common_parents: vec![parent.clone()],
            entry1: entry2,
            entry2: entry1,
            detected_at: SystemTime::now(),
        };

        let resolution2 = resolver.resolve_fork(&fork2).unwrap();
        assert_eq!(resolution2, ForkResolution::KeepSecond);
    }

    #[test]
    fn test_fork_detector_indexing() {
        let mut detector = ForkDetector::new();

        let parent = ContentHash([0u8; 32]);
        let author1 = make_test_did();
        let author2 = make_test_did();

        let entry1 = make_test_entry(author1, vec![parent.clone()], 1000);
        let entry2 = make_test_entry(author2, vec![parent.clone()], 2000);

        // Index both entries
        detector.index_entry(&entry1);
        detector.index_entry(&entry2);

        // Should detect fork
        assert!(detector.has_fork(&parent));

        let forks = detector.detect_forks();
        assert_eq!(forks.len(), 1);
        assert_eq!(forks[0].0, parent);
        assert_eq!(forks[0].1.len(), 2);
    }

    #[test]
    fn test_fork_detector_no_fork() {
        let mut detector = ForkDetector::new();

        let parent1 = ContentHash([0u8; 32]);
        let parent2 = ContentHash([1u8; 32]);
        let author = make_test_did();

        let entry1 = make_test_entry(author.clone(), vec![parent1.clone()], 1000);
        let entry2 = make_test_entry(author, vec![parent2.clone()], 2000);

        // Index both entries (different parents)
        detector.index_entry(&entry1);
        detector.index_entry(&entry2);

        // Should not detect fork
        assert!(!detector.has_fork(&parent1));
        assert!(!detector.has_fork(&parent2));

        let forks = detector.detect_forks();
        assert_eq!(forks.len(), 0);
    }

    #[test]
    fn test_nway_fork_detection() {
        let mut detector = ForkDetector::new();

        let parent = ContentHash([0u8; 32]);
        let author1 = make_test_did();
        let author2 = make_test_did();
        let author3 = make_test_did();
        let author4 = make_test_did();

        // Create 4 entries all referencing the same parent (4-way fork)
        let entry1 = make_test_entry(author1, vec![parent.clone()], 1000);
        let entry2 = make_test_entry(author2, vec![parent.clone()], 2000);
        let entry3 = make_test_entry(author3, vec![parent.clone()], 3000);
        let entry4 = make_test_entry(author4, vec![parent.clone()], 4000);

        // Index all entries
        detector.index_entry(&entry1);
        detector.index_entry(&entry2);
        detector.index_entry(&entry3);
        detector.index_entry(&entry4);

        // Should detect fork with 4 children
        assert!(detector.has_fork(&parent));

        let forks = detector.detect_forks();
        assert_eq!(forks.len(), 1);
        assert_eq!(forks[0].0, parent);
        assert_eq!(forks[0].1.len(), 4);
    }

    #[test]
    fn test_nway_fork_resolution_tournament() {
        let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);

        let parent = ContentHash([0u8; 32]);
        let author1 = make_test_did();
        let author2 = make_test_did();
        let author3 = make_test_did();

        // Create 3 entries with different timestamps
        // Entry1 (ts=1000) should win as earliest
        let entry1 = make_test_entry(author1, vec![parent.clone()], 1000);
        let entry2 = make_test_entry(author2, vec![parent.clone()], 2000);
        let entry3 = make_test_entry(author3, vec![parent.clone()], 3000);

        // Tournament round 1: entry1 vs entry2
        let fork1 = Fork {
            common_parents: vec![parent.clone()],
            entry1: entry1.clone(),
            entry2: entry2.clone(),
            detected_at: SystemTime::now(),
        };
        let resolution1 = resolver.resolve_fork(&fork1).unwrap();
        assert_eq!(resolution1, ForkResolution::KeepFirst); // entry1 wins (earlier timestamp)

        // Tournament round 2: entry1 (winner) vs entry3
        let fork2 = Fork {
            common_parents: vec![parent.clone()],
            entry1: entry1.clone(),
            entry2: entry3.clone(),
            detected_at: SystemTime::now(),
        };
        let resolution2 = resolver.resolve_fork(&fork2).unwrap();
        assert_eq!(resolution2, ForkResolution::KeepFirst); // entry1 wins again

        // Final result: entry1 is the overall winner, entry2 and entry3 should be quarantined
    }

    #[test]
    fn test_nway_fork_resolution_challenger_wins() {
        let resolver = ForkResolver::new(ForkResolutionStrategy::TimestampPreference);

        let parent = ContentHash([0u8; 32]);
        let author1 = make_test_did();
        let author2 = make_test_did();
        let author3 = make_test_did();

        // Create 3 entries where later entry has earliest timestamp
        // Entry3 (ts=500) should ultimately win
        let entry1 = make_test_entry(author1, vec![parent.clone()], 2000);
        let entry2 = make_test_entry(author2, vec![parent.clone()], 1500);
        let entry3 = make_test_entry(author3, vec![parent.clone()], 500);

        // Tournament round 1: entry1 vs entry2
        let fork1 = Fork {
            common_parents: vec![parent.clone()],
            entry1: entry1.clone(),
            entry2: entry2.clone(),
            detected_at: SystemTime::now(),
        };
        let resolution1 = resolver.resolve_fork(&fork1).unwrap();
        assert_eq!(resolution1, ForkResolution::KeepSecond); // entry2 wins (earlier)

        // Tournament round 2: entry2 (new winner) vs entry3
        let fork2 = Fork {
            common_parents: vec![parent.clone()],
            entry1: entry2.clone(),
            entry2: entry3.clone(),
            detected_at: SystemTime::now(),
        };
        let resolution2 = resolver.resolve_fork(&fork2).unwrap();
        assert_eq!(resolution2, ForkResolution::KeepSecond); // entry3 wins (earliest)

        // Final result: entry3 is the overall winner
    }
}
