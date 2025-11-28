//! Network Partition Detection and Healing
//!
//! Phase 18 Week 3: Detects network partitions and heals them with conflict resolution
//!
//! When nodes lose connectivity and later reconnect, their vector clocks may have diverged,
//! causing conflicts. This module detects partitions and resolves conflicts when they heal.

use crate::vector_clock::VectorClock;
use anyhow::Result;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, info, warn};

/// Content hash type (32-byte SHA-256)
pub type ContentHash = [u8; 32];

/// Threshold configuration for partition detection
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Duration of no contact before suspecting partition (default: 5 minutes)
    pub partition_threshold: Duration,
    /// Interval for checking partition status (default: 30 seconds)
    pub check_interval: Duration,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            partition_threshold: Duration::from_secs(300), // 5 minutes
            check_interval: Duration::from_secs(30),       // 30 seconds
        }
    }
}

/// Detects network partitions by tracking last-seen timestamps
pub struct PartitionDetector {
    last_seen: HashMap<Did, Instant>,
    config: PartitionConfig,
}

impl PartitionDetector {
    pub fn new(config: PartitionConfig) -> Self {
        Self {
            last_seen: HashMap::new(),
            config,
        }
    }

    /// Record that we've seen a peer (call on every message received)
    pub fn record_contact(&mut self, did: &Did) {
        self.last_seen.insert(did.clone(), Instant::now());
    }

    /// Check if a peer is suspected to be partitioned
    pub fn is_partitioned(&self, did: &Did) -> bool {
        match self.last_seen.get(did) {
            Some(last) => last.elapsed() > self.config.partition_threshold,
            None => false, // Unknown peers aren't considered partitioned
        }
    }

    /// Get all peers that are suspected to be partitioned
    pub fn get_partitioned_peers(&self) -> Vec<Did> {
        let threshold = self.config.partition_threshold;
        self.last_seen
            .iter()
            .filter(|(_, last)| last.elapsed() > threshold)
            .map(|(did, _)| did.clone())
            .collect()
    }

    /// Get time since last contact with peer
    pub fn time_since_contact(&self, did: &Did) -> Option<Duration> {
        self.last_seen.get(did).map(|last| last.elapsed())
    }

    /// Remove a peer from tracking (call when peer explicitly disconnects)
    pub fn remove_peer(&mut self, did: &Did) {
        self.last_seen.remove(did);
    }
}

/// Type of data that can have conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    LedgerEntry,
    Contract,
    TrustEdge,
    GossipEntry,
    Other,
}

/// A conflict detected during vector clock merge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub data_type: DataType,
    pub content_hash: ContentHash,
    pub local_version: u64,
    pub remote_version: u64,
    pub local_timestamp: SystemTime,
    pub remote_timestamp: SystemTime,
    pub local_data: Vec<u8>,
    pub remote_data: Vec<u8>,
}

/// Strategy for resolving conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep local version (prefer our data)
    KeepLocal,
    /// Keep remote version (prefer their data)
    KeepRemote,
    /// Keep both versions (for append-only data)
    KeepBoth,
    /// Use timestamp (Last-Write-Wins)
    LastWriteWins,
    /// Requires manual resolution (for critical data like ledger)
    RequiresManual,
}

/// Result of conflict resolution
#[derive(Debug, Clone)]
pub enum ResolutionOutcome {
    /// Conflict was automatically resolved
    Resolved {
        strategy: ConflictResolution,
        kept_version: String, // "local", "remote", or "both"
    },
    /// Conflict requires manual intervention
    RequiresManual { conflict: Conflict, reason: String },
}

/// Merges vector clocks and detects causality violations
pub struct VectorClockMerger {
    local_clock: VectorClock,
}

impl VectorClockMerger {
    pub fn new(local_clock: VectorClock) -> Self {
        Self { local_clock }
    }

    /// Merge remote clock and return conflicts (concurrent updates)
    pub fn merge(
        &mut self,
        _peer: &Did,
        remote_clock: VectorClock,
    ) -> Result<Vec<(Did, u64, u64)>> {
        let mut conflicts = Vec::new();

        // For each peer in remote clock
        for (remote_did, &remote_version) in &remote_clock.clock {
            let local_version = self.local_clock.get(remote_did);

            // Check for conflict (concurrent updates)
            if remote_version > local_version {
                // Remote is ahead - we need their updates
                debug!(
                    "Detected version gap for {}: local={}, remote={}",
                    remote_did, local_version, remote_version
                );
                conflicts.push((remote_did.clone(), local_version, remote_version));
            } else if remote_version < local_version {
                // We're ahead - they need our updates (not a conflict from our perspective)
                debug!(
                    "We are ahead of remote for {}: local={}, remote={}",
                    remote_did, local_version, remote_version
                );
            }
            // If equal, no conflict
        }

        // Check for peers in our clock that aren't in remote clock
        for (local_did, &local_version) in &self.local_clock.clock {
            if remote_clock.get(local_did) == 0 && local_version > 0 {
                debug!(
                    "Peer {} in local clock but not remote: version={}",
                    local_did, local_version
                );
            }
        }

        // Merge the clocks (take max of each peer's version)
        self.local_clock.merge(&remote_clock);

        Ok(conflicts)
    }

    pub fn get_clock(&self) -> &VectorClock {
        &self.local_clock
    }
}

/// Resolves conflicts based on data type and policy
pub struct ConflictResolver {
    // Policies for different data types
    ledger_policy: ConflictResolution,
    contract_policy: ConflictResolution,
    trust_policy: ConflictResolution,
    gossip_policy: ConflictResolution,
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self {
            ledger_policy: ConflictResolution::RequiresManual, // Ledger conflicts are critical
            contract_policy: ConflictResolution::LastWriteWins, // Contracts use timestamps
            trust_policy: ConflictResolution::LastWriteWins,   // Trust edges use timestamps
            gossip_policy: ConflictResolution::KeepBoth,       // Gossip is append-only
        }
    }
}

impl ConflictResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve a conflict based on data type and policy
    pub fn resolve(&self, conflict: &Conflict) -> Result<ResolutionOutcome> {
        let policy = match conflict.data_type {
            DataType::LedgerEntry => self.ledger_policy,
            DataType::Contract => self.contract_policy,
            DataType::TrustEdge => self.trust_policy,
            DataType::GossipEntry => self.gossip_policy,
            DataType::Other => ConflictResolution::LastWriteWins,
        };

        match policy {
            ConflictResolution::RequiresManual => {
                warn!(
                    "Conflict requires manual resolution: {:?} hash={:?}",
                    conflict.data_type,
                    hex::encode(&conflict.content_hash[..8])
                );
                Ok(ResolutionOutcome::RequiresManual {
                    conflict: conflict.clone(),
                    reason: format!(
                        "Policy requires manual resolution for {:?}",
                        conflict.data_type
                    ),
                })
            }
            ConflictResolution::LastWriteWins => {
                let kept = if conflict.remote_timestamp > conflict.local_timestamp {
                    "remote"
                } else {
                    "local"
                };
                info!("Resolved conflict using Last-Write-Wins: kept {}", kept);
                Ok(ResolutionOutcome::Resolved {
                    strategy: ConflictResolution::LastWriteWins,
                    kept_version: kept.to_string(),
                })
            }
            ConflictResolution::KeepBoth => {
                info!("Resolved conflict by keeping both versions (append-only)");
                Ok(ResolutionOutcome::Resolved {
                    strategy: ConflictResolution::KeepBoth,
                    kept_version: "both".to_string(),
                })
            }
            ConflictResolution::KeepLocal => Ok(ResolutionOutcome::Resolved {
                strategy: ConflictResolution::KeepLocal,
                kept_version: "local".to_string(),
            }),
            ConflictResolution::KeepRemote => Ok(ResolutionOutcome::Resolved {
                strategy: ConflictResolution::KeepRemote,
                kept_version: "remote".to_string(),
            }),
        }
    }

    /// Set policy for ledger conflicts
    pub fn set_ledger_policy(&mut self, policy: ConflictResolution) {
        self.ledger_policy = policy;
    }

    /// Set policy for contract conflicts
    pub fn set_contract_policy(&mut self, policy: ConflictResolution) {
        self.contract_policy = policy;
    }

    /// Set policy for trust edge conflicts
    pub fn set_trust_policy(&mut self, policy: ConflictResolution) {
        self.trust_policy = policy;
    }
}

/// Heals network partitions by merging clocks and resolving conflicts
pub struct PartitionHealer {
    merger: VectorClockMerger,
    resolver: ConflictResolver,
}

impl PartitionHealer {
    pub fn new(local_clock: VectorClock) -> Self {
        Self {
            merger: VectorClockMerger::new(local_clock),
            resolver: ConflictResolver::new(),
        }
    }

    /// When partition heals, merge vector clocks and resolve conflicts
    pub async fn heal_partition(
        &mut self,
        peer: &Did,
        their_clock: VectorClock,
        conflicts: Vec<Conflict>,
    ) -> Result<Vec<ResolutionOutcome>> {
        info!("Healing partition with peer {}", peer);

        // Merge vector clocks (detect causality violations)
        let version_gaps = self.merger.merge(peer, their_clock)?;

        if !version_gaps.is_empty() {
            info!(
                "Detected {} version gaps during partition healing",
                version_gaps.len()
            );
        }

        // Resolve each conflict
        let mut outcomes = Vec::new();
        for conflict in conflicts {
            let outcome = self.resolve_conflict(conflict).await?;
            outcomes.push(outcome);
        }

        info!(
            "Partition healing complete: {} conflicts resolved",
            outcomes.len()
        );

        Ok(outcomes)
    }

    /// Resolve a single conflict
    async fn resolve_conflict(&self, conflict: Conflict) -> Result<ResolutionOutcome> {
        match conflict.data_type {
            DataType::LedgerEntry => self.resolve_ledger_conflict(conflict).await,
            DataType::Contract => self.resolve_contract_conflict(conflict).await,
            DataType::TrustEdge => self.resolve_trust_conflict(conflict).await,
            DataType::GossipEntry => self.resolve_gossip_conflict(conflict).await,
            DataType::Other => self.log_and_keep_both(conflict).await,
        }
    }

    async fn resolve_ledger_conflict(&self, conflict: Conflict) -> Result<ResolutionOutcome> {
        // Ledger conflicts are critical - require manual resolution
        warn!(
            "Ledger conflict detected: hash={:?}",
            hex::encode(&conflict.content_hash[..8])
        );
        self.resolver.resolve(&conflict)
    }

    async fn resolve_contract_conflict(&self, conflict: Conflict) -> Result<ResolutionOutcome> {
        // Contracts use Last-Write-Wins
        debug!(
            "Contract conflict: using Last-Write-Wins for hash={:?}",
            hex::encode(&conflict.content_hash[..8])
        );
        self.resolver.resolve(&conflict)
    }

    async fn resolve_trust_conflict(&self, conflict: Conflict) -> Result<ResolutionOutcome> {
        // Trust edges use Last-Write-Wins
        debug!(
            "Trust edge conflict: using Last-Write-Wins for hash={:?}",
            hex::encode(&conflict.content_hash[..8])
        );
        self.resolver.resolve(&conflict)
    }

    async fn resolve_gossip_conflict(&self, conflict: Conflict) -> Result<ResolutionOutcome> {
        // Gossip is append-only - keep both
        debug!(
            "Gossip conflict: keeping both versions for hash={:?}",
            hex::encode(&conflict.content_hash[..8])
        );
        self.resolver.resolve(&conflict)
    }

    async fn log_and_keep_both(&self, conflict: Conflict) -> Result<ResolutionOutcome> {
        info!(
            "Unknown data type conflict: keeping both versions for hash={:?}",
            hex::encode(&conflict.content_hash[..8])
        );
        Ok(ResolutionOutcome::Resolved {
            strategy: ConflictResolution::KeepBoth,
            kept_version: "both".to_string(),
        })
    }

    /// Get reference to merged vector clock
    pub fn get_clock(&self) -> &VectorClock {
        self.merger.get_clock()
    }

    /// Get mutable reference to conflict resolver for policy changes
    pub fn resolver_mut(&mut self) -> &mut ConflictResolver {
        &mut self.resolver
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use std::thread;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_partition_detector_basic() {
        let config = PartitionConfig {
            partition_threshold: Duration::from_millis(100),
            check_interval: Duration::from_secs(1),
        };
        let mut detector = PartitionDetector::new(config);

        let peer1 = make_test_did();
        let peer2 = make_test_did();

        // Record contact
        detector.record_contact(&peer1);
        detector.record_contact(&peer2);

        // Should not be partitioned immediately
        assert!(!detector.is_partitioned(&peer1));
        assert!(!detector.is_partitioned(&peer2));

        // Wait for threshold
        thread::sleep(Duration::from_millis(150));

        // Should now be partitioned
        assert!(detector.is_partitioned(&peer1));
        assert!(detector.is_partitioned(&peer2));

        // Record new contact for peer1
        detector.record_contact(&peer1);

        // peer1 should no longer be partitioned
        assert!(!detector.is_partitioned(&peer1));
        assert!(detector.is_partitioned(&peer2)); // peer2 still partitioned
    }

    #[test]
    fn test_partition_detector_get_partitioned_peers() {
        let config = PartitionConfig {
            partition_threshold: Duration::from_millis(50),
            check_interval: Duration::from_secs(1),
        };
        let mut detector = PartitionDetector::new(config);

        let peer1 = make_test_did();
        let peer2 = make_test_did();
        let peer3 = make_test_did();

        detector.record_contact(&peer1);
        detector.record_contact(&peer2);
        detector.record_contact(&peer3);

        // Wait for partition threshold
        thread::sleep(Duration::from_millis(100));

        let partitioned = detector.get_partitioned_peers();
        assert_eq!(partitioned.len(), 3);

        // Update peer1
        detector.record_contact(&peer1);

        let partitioned = detector.get_partitioned_peers();
        assert_eq!(partitioned.len(), 2); // Only peer2 and peer3
    }

    #[test]
    fn test_vector_clock_merger() {
        let did1 = make_test_did();
        let did2 = make_test_did();
        let did3 = make_test_did();

        let mut local_clock = VectorClock::new();
        local_clock.increment(&did1);
        local_clock.clock.insert(did2.clone(), 5);

        let mut remote_clock = VectorClock::new();
        remote_clock.increment(&did2);
        remote_clock.increment(&did2);
        remote_clock.clock.insert(did1.clone(), 1);
        remote_clock.clock.insert(did3.clone(), 3);

        let mut merger = VectorClockMerger::new(local_clock);
        let conflicts = merger.merge(&did2, remote_clock).unwrap();

        // Should detect version gap for did2 (remote has 2, local has 5 - no conflict)
        // and did3 (remote has 3, local has 0 - conflict)
        assert!(!conflicts.is_empty());

        // Merged clock should have max versions
        assert_eq!(merger.get_clock().get(&did1), 1);
        assert_eq!(merger.get_clock().get(&did2), 5);
        assert_eq!(merger.get_clock().get(&did3), 3);
    }

    #[test]
    fn test_conflict_resolver_policies() {
        let resolver = ConflictResolver::default();

        let ledger_conflict = Conflict {
            data_type: DataType::LedgerEntry,
            content_hash: [0u8; 32],
            local_version: 1,
            remote_version: 2,
            local_timestamp: SystemTime::now(),
            remote_timestamp: SystemTime::now(),
            local_data: vec![1, 2, 3],
            remote_data: vec![4, 5, 6],
        };

        let gossip_conflict = Conflict {
            data_type: DataType::GossipEntry,
            content_hash: [1u8; 32],
            local_version: 1,
            remote_version: 2,
            local_timestamp: SystemTime::now(),
            remote_timestamp: SystemTime::now(),
            local_data: vec![1, 2, 3],
            remote_data: vec![4, 5, 6],
        };

        // Ledger should require manual resolution
        let outcome = resolver.resolve(&ledger_conflict).unwrap();
        assert!(matches!(outcome, ResolutionOutcome::RequiresManual { .. }));

        // Gossip should keep both
        let outcome = resolver.resolve(&gossip_conflict).unwrap();
        if let ResolutionOutcome::Resolved {
            strategy,
            kept_version,
        } = outcome
        {
            assert_eq!(strategy, ConflictResolution::KeepBoth);
            assert_eq!(kept_version, "both");
        } else {
            panic!("Expected Resolved outcome");
        }
    }

    #[test]
    fn test_conflict_resolver_last_write_wins() {
        let resolver = ConflictResolver::default();

        let now = SystemTime::now();
        let earlier = now - Duration::from_secs(10);

        let conflict_remote_newer = Conflict {
            data_type: DataType::Contract,
            content_hash: [2u8; 32],
            local_version: 1,
            remote_version: 2,
            local_timestamp: earlier,
            remote_timestamp: now,
            local_data: vec![1, 2, 3],
            remote_data: vec![4, 5, 6],
        };

        let outcome = resolver.resolve(&conflict_remote_newer).unwrap();
        if let ResolutionOutcome::Resolved {
            strategy,
            kept_version,
        } = outcome
        {
            assert_eq!(strategy, ConflictResolution::LastWriteWins);
            assert_eq!(kept_version, "remote");
        } else {
            panic!("Expected Resolved outcome");
        }

        let conflict_local_newer = Conflict {
            data_type: DataType::TrustEdge,
            content_hash: [3u8; 32],
            local_version: 2,
            remote_version: 1,
            local_timestamp: now,
            remote_timestamp: earlier,
            local_data: vec![1, 2, 3],
            remote_data: vec![4, 5, 6],
        };

        let outcome = resolver.resolve(&conflict_local_newer).unwrap();
        if let ResolutionOutcome::Resolved {
            strategy,
            kept_version,
        } = outcome
        {
            assert_eq!(strategy, ConflictResolution::LastWriteWins);
            assert_eq!(kept_version, "local");
        } else {
            panic!("Expected Resolved outcome");
        }
    }

    #[tokio::test]
    async fn test_partition_healer_basic() {
        let _did1 = make_test_did();
        let did2 = make_test_did();

        let local_clock = VectorClock::new();
        let mut remote_clock = VectorClock::new();
        remote_clock.increment(&did2);

        let mut healer = PartitionHealer::new(local_clock);

        // No conflicts to resolve
        let outcomes = healer
            .heal_partition(&did2, remote_clock, vec![])
            .await
            .unwrap();

        assert_eq!(outcomes.len(), 0);

        // Clock should be merged
        assert_eq!(healer.get_clock().get(&did2), 1);
    }

    #[tokio::test]
    async fn test_partition_healer_with_conflicts() {
        let _did1 = make_test_did();
        let did2 = make_test_did();

        let local_clock = VectorClock::new();
        let remote_clock = VectorClock::new();

        let mut healer = PartitionHealer::new(local_clock);

        let conflicts = vec![
            Conflict {
                data_type: DataType::GossipEntry,
                content_hash: [0u8; 32],
                local_version: 1,
                remote_version: 1,
                local_timestamp: SystemTime::now(),
                remote_timestamp: SystemTime::now(),
                local_data: vec![1],
                remote_data: vec![2],
            },
            Conflict {
                data_type: DataType::Contract,
                content_hash: [1u8; 32],
                local_version: 1,
                remote_version: 2,
                local_timestamp: SystemTime::now() - Duration::from_secs(10),
                remote_timestamp: SystemTime::now(),
                local_data: vec![3],
                remote_data: vec![4],
            },
        ];

        let outcomes = healer
            .heal_partition(&did2, remote_clock, conflicts)
            .await
            .unwrap();

        assert_eq!(outcomes.len(), 2);

        // First conflict (gossip) should keep both
        if let ResolutionOutcome::Resolved { strategy, .. } = &outcomes[0] {
            assert_eq!(*strategy, ConflictResolution::KeepBoth);
        } else {
            panic!("Expected Resolved outcome for gossip");
        }

        // Second conflict (contract) should use Last-Write-Wins (remote newer)
        if let ResolutionOutcome::Resolved {
            strategy,
            kept_version,
        } = &outcomes[1]
        {
            assert_eq!(*strategy, ConflictResolution::LastWriteWins);
            assert_eq!(kept_version, "remote");
        } else {
            panic!("Expected Resolved outcome for contract");
        }
    }
}
