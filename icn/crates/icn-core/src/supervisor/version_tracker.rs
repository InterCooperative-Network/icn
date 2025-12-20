//! Protocol version tracking and upgrade coordination
//!
//! Tracks peer versions, monitors adoption rates, and manages protocol upgrades.

use icn_governance::proposal::Version;
use icn_identity::Did;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

/// Get current Unix timestamp in seconds
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Tracks protocol versions across the network
pub struct VersionTracker {
    /// Current node's protocol version
    current_version: Version,

    /// Peer versions (DID -> Version)
    peer_versions: HashMap<Did, PeerVersionInfo>,

    /// Pending upgrade (if governance approved)
    pending_upgrade: Option<PendingUpgrade>,
}

/// Version info for a specific peer
#[derive(Debug, Clone)]
pub struct PeerVersionInfo {
    /// Protocol version reported by peer
    pub version: Version,
    /// When we last saw this version from this peer
    pub last_seen: u64,
    /// First time we saw this peer
    pub first_seen: u64,
}

/// Pending protocol upgrade (governance-approved)
#[derive(Debug, Clone)]
pub struct PendingUpgrade {
    /// Proposal ID that approved this upgrade
    pub proposal_id: String,
    /// Target version to upgrade to
    pub target_version: Version,
    /// When the upgrade was approved (Unix timestamp)
    pub approved_at: u64,
    /// Deadline for upgrade (Unix timestamp)
    pub deadline: u64,
    /// Minimum version required after deadline
    pub min_required_version: Option<Version>,
    /// Breaking changes description
    pub breaking_changes: Vec<String>,
    /// Migration guide URL
    pub migration_guide: Option<String>,
}

impl VersionTracker {
    /// Create a new version tracker
    pub fn new(current_version: Version) -> Self {
        info!(
            "Version tracker initialized with version {}",
            current_version
        );

        Self {
            current_version,
            peer_versions: HashMap::new(),
            pending_upgrade: None,
        }
    }

    /// Get current node version
    pub fn current_version(&self) -> &Version {
        &self.current_version
    }

    /// Record a peer's protocol version
    pub fn record_peer_version(&mut self, peer: Did, version: Version) {
        let now = now_secs();

        self.peer_versions
            .entry(peer.clone())
            .and_modify(|info| {
                if info.version != version {
                    info!(
                        "Peer {} upgraded from {} to {}",
                        peer, info.version, version
                    );
                }
                info.version = version.clone();
                info.last_seen = now;
            })
            .or_insert_with(|| PeerVersionInfo {
                version: version.clone(),
                last_seen: now,
                first_seen: now,
            });
    }

    /// Get adoption rate for a specific version
    ///
    /// Returns percentage (0.0 to 1.0) of peers on the given version
    pub fn adoption_rate(&self, version: &Version) -> f64 {
        if self.peer_versions.is_empty() {
            return 0.0;
        }

        let count = self
            .peer_versions
            .values()
            .filter(|info| &info.version == version)
            .count();

        count as f64 / self.peer_versions.len() as f64
    }

    /// Get peers below a minimum version
    pub fn peers_below_version(&self, min_version: &Version) -> Vec<Did> {
        self.peer_versions
            .iter()
            .filter(|(_, info)| info.version < *min_version)
            .map(|(did, _)| did.clone())
            .collect()
    }

    /// Get peers on a specific version
    pub fn peers_on_version(&self, version: &Version) -> Vec<Did> {
        self.peer_versions
            .iter()
            .filter(|(_, info)| &info.version == version)
            .map(|(did, _)| did.clone())
            .collect()
    }

    /// Check if a peer version is compatible with current node
    pub fn is_peer_compatible(&self, peer_version: &Version) -> bool {
        self.current_version.is_compatible_with(peer_version)
    }

    /// Set pending upgrade (called when governance proposal passes)
    pub fn set_pending_upgrade(&mut self, upgrade: PendingUpgrade) {
        info!(
            "Protocol upgrade approved: {} -> {} (deadline: {})",
            self.current_version, upgrade.target_version, upgrade.deadline
        );

        if !upgrade.breaking_changes.is_empty() {
            info!("Breaking changes:");
            for change in &upgrade.breaking_changes {
                info!("  - {}", change);
            }
        }

        if let Some(ref guide) = upgrade.migration_guide {
            info!("Migration guide: {}", guide);
        }

        self.pending_upgrade = Some(upgrade);
    }

    /// Get pending upgrade
    pub fn pending_upgrade(&self) -> Option<&PendingUpgrade> {
        self.pending_upgrade.as_ref()
    }

    /// Clear pending upgrade (after applied)
    pub fn clear_pending_upgrade(&mut self) {
        self.pending_upgrade = None;
    }

    /// Check if upgrade deadline has passed
    pub fn is_upgrade_deadline_passed(&self) -> bool {
        if let Some(upgrade) = &self.pending_upgrade {
            now_secs() >= upgrade.deadline
        } else {
            false
        }
    }

    /// Get time remaining until upgrade deadline (seconds)
    pub fn upgrade_deadline_remaining(&self) -> Option<u64> {
        if let Some(upgrade) = &self.pending_upgrade {
            let now = now_secs();
            if now < upgrade.deadline {
                Some(upgrade.deadline - now)
            } else {
                Some(0)
            }
        } else {
            None
        }
    }

    /// Get version statistics
    pub fn version_stats(&self) -> VersionStats {
        let mut version_counts: HashMap<Version, usize> = HashMap::new();

        for info in self.peer_versions.values() {
            *version_counts.entry(info.version.clone()).or_insert(0) += 1;
        }

        let total_peers = self.peer_versions.len();

        // Find most common version
        let most_common_version = version_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(version, _)| version.clone());

        VersionStats {
            total_peers,
            version_counts,
            most_common_version,
            current_version: self.current_version.clone(),
        }
    }

    /// Prune stale peer entries (not seen in 1 hour)
    pub fn prune_stale_peers(&mut self) {
        let stale_threshold = 60 * 60; // 1 hour
        let now = now_secs();

        let before = self.peer_versions.len();

        self.peer_versions
            .retain(|_, info| now - info.last_seen < stale_threshold);

        let removed = before - self.peer_versions.len();
        if removed > 0 {
            info!("Pruned {} stale peer version entries", removed);
        }
    }
}

/// Version statistics across the network
#[derive(Debug, Clone)]
pub struct VersionStats {
    /// Total number of peers tracked
    pub total_peers: usize,
    /// Count of peers per version
    pub version_counts: HashMap<Version, usize>,
    /// Most common version in the network
    pub most_common_version: Option<Version>,
    /// Current node's version
    pub current_version: Version,
}

impl VersionStats {
    /// Get adoption rate for the current version
    pub fn current_version_adoption(&self) -> f64 {
        if self.total_peers == 0 {
            return 0.0;
        }

        let count = self
            .version_counts
            .get(&self.current_version)
            .copied()
            .unwrap_or(0);

        count as f64 / self.total_peers as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_version_tracker_basic() {
        let tracker = VersionTracker::new(Version::new(1, 0, 0));

        assert_eq!(tracker.current_version(), &Version::new(1, 0, 0));
        assert_eq!(tracker.adoption_rate(&Version::new(1, 0, 0)), 0.0);
    }

    #[test]
    fn test_record_peer_versions() {
        let mut tracker = VersionTracker::new(Version::new(1, 0, 0));

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();
        let peer3 = KeyPair::generate().unwrap().did().clone();

        tracker.record_peer_version(peer1.clone(), Version::new(1, 0, 0));
        tracker.record_peer_version(peer2.clone(), Version::new(1, 0, 0));
        tracker.record_peer_version(peer3.clone(), Version::new(1, 1, 0));

        assert_eq!(tracker.adoption_rate(&Version::new(1, 0, 0)), 2.0 / 3.0);
        assert_eq!(tracker.adoption_rate(&Version::new(1, 1, 0)), 1.0 / 3.0);
    }

    #[test]
    fn test_peers_below_version() {
        let mut tracker = VersionTracker::new(Version::new(2, 0, 0));

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();
        let peer3 = KeyPair::generate().unwrap().did().clone();

        tracker.record_peer_version(peer1.clone(), Version::new(1, 0, 0));
        tracker.record_peer_version(peer2.clone(), Version::new(1, 5, 0));
        tracker.record_peer_version(peer3.clone(), Version::new(2, 0, 0));

        let below = tracker.peers_below_version(&Version::new(2, 0, 0));
        assert_eq!(below.len(), 2);
        assert!(below.contains(&peer1));
        assert!(below.contains(&peer2));
    }

    #[test]
    fn test_pending_upgrade() {
        let mut tracker = VersionTracker::new(Version::new(1, 0, 0));

        assert!(tracker.pending_upgrade().is_none());

        let upgrade = PendingUpgrade {
            proposal_id: "prop-123".to_string(),
            target_version: Version::new(2, 0, 0),
            approved_at: now_secs(),
            deadline: now_secs() + 86400, // 24 hours
            min_required_version: Some(Version::new(2, 0, 0)),
            breaking_changes: vec!["Breaking change 1".to_string()],
            migration_guide: Some("https://docs.example.com/migrate".to_string()),
        };

        tracker.set_pending_upgrade(upgrade.clone());
        assert!(tracker.pending_upgrade().is_some());
        assert_eq!(
            tracker.pending_upgrade().unwrap().target_version,
            Version::new(2, 0, 0)
        );
    }

    #[test]
    fn test_version_stats() {
        let mut tracker = VersionTracker::new(Version::new(1, 0, 0));

        let peer1 = KeyPair::generate().unwrap().did().clone();
        let peer2 = KeyPair::generate().unwrap().did().clone();
        let peer3 = KeyPair::generate().unwrap().did().clone();

        tracker.record_peer_version(peer1, Version::new(1, 0, 0));
        tracker.record_peer_version(peer2, Version::new(1, 0, 0));
        tracker.record_peer_version(peer3, Version::new(1, 1, 0));

        let stats = tracker.version_stats();
        assert_eq!(stats.total_peers, 3);
        assert_eq!(stats.most_common_version, Some(Version::new(1, 0, 0)));
        assert_eq!(stats.current_version_adoption(), 2.0 / 3.0);
    }
}
