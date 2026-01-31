//! Protocol upgrade coordination system
//!
//! Enables governance-driven protocol upgrades with:
//! - Version tracking and adoption monitoring
//! - Migration guide distribution
//! - Deadline enforcement for deprecated versions
//! - Metrics for upgrade progress

use icn_kernel_api::Version;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Current protocol version (updated with each release)
pub const CURRENT_VERSION: (u32, u32, u32) = (0, 1, 0);

/// Upgrade coordinator tracks protocol versions across the network
pub struct UpgradeCoordinator {
    /// Current node version
    current_version: Version,

    /// Pending upgrade proposals (approved but not yet at deadline)
    pending_upgrades: Arc<RwLock<Vec<PendingUpgrade>>>,

    /// Version adoption tracking (DID -> Version)
    peer_versions: Arc<RwLock<HashMap<Did, PeerVersionInfo>>>,

    /// Minimum required version (enforced after upgrade deadline)
    min_required_version: Arc<RwLock<Option<Version>>>,
}

/// Information about a pending upgrade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUpgrade {
    /// Target version
    pub version: Version,

    /// Upgrade deadline (Unix timestamp)
    pub deadline: u64,

    /// Breaking changes description
    pub breaking_changes: Vec<String>,

    /// Migration guide URL
    pub migration_guide: Option<String>,

    /// Minimum version required after deadline
    pub min_required_version: Option<Version>,

    /// When this upgrade was approved
    pub approved_at: u64,
}

/// Peer version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerVersionInfo {
    /// Peer's protocol version
    pub version: Version,

    /// When this version was last reported
    pub last_seen: u64,

    /// Whether peer is below minimum required version
    pub is_deprecated: bool,
}

/// Statistics about network upgrade adoption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeAdoptionStats {
    /// Total number of peers
    pub total_peers: usize,

    /// Peers at target version
    pub at_target_version: usize,

    /// Peers at compatible versions
    pub at_compatible_version: usize,

    /// Peers at deprecated versions
    pub at_deprecated_version: usize,

    /// Adoption percentage (0.0 to 1.0)
    pub adoption_rate: f64,

    /// Days until deadline (if upgrade pending)
    pub days_until_deadline: Option<i64>,
}

impl UpgradeCoordinator {
    /// Create a new upgrade coordinator
    pub fn new() -> Self {
        let (major, minor, patch) = CURRENT_VERSION;
        Self {
            current_version: Version::new(major, minor, patch),
            pending_upgrades: Arc::new(RwLock::new(Vec::new())),
            peer_versions: Arc::new(RwLock::new(HashMap::new())),
            min_required_version: Arc::new(RwLock::new(None)),
        }
    }

    /// Get current node version
    pub fn current_version(&self) -> Version {
        self.current_version.clone()
    }

    /// Register an approved upgrade proposal
    pub fn register_upgrade(&self, upgrade: PendingUpgrade) -> Result<(), String> {
        let mut upgrades = self
            .pending_upgrades
            .write()
            .map_err(|e| format!("Lock error: {e}"))?;

        // Check if upgrade already exists
        if upgrades.iter().any(|u| u.version == upgrade.version) {
            return Err(format!("Upgrade to {} already registered", upgrade.version));
        }

        // Validate deadline is in the future
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        if upgrade.deadline <= now {
            return Err("Upgrade deadline must be in the future".to_string());
        }

        upgrades.push(upgrade);
        Ok(())
    }

    /// Update peer version information
    pub fn update_peer_version(&self, did: Did, version: Version) -> Result<(), String> {
        let mut versions = self
            .peer_versions
            .write()
            .map_err(|e| format!("Lock error: {e}"))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        let min_version = self
            .min_required_version
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;

        let is_deprecated = if let Some(ref min) = *min_version {
            version < *min
        } else {
            false
        };

        versions.insert(
            did,
            PeerVersionInfo {
                version,
                last_seen: now,
                is_deprecated,
            },
        );

        Ok(())
    }

    /// Check if a peer's version is acceptable
    pub fn is_peer_version_acceptable(&self, version: &Version) -> Result<bool, String> {
        let min_version = self
            .min_required_version
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;

        if let Some(ref min) = *min_version {
            Ok(version >= min)
        } else {
            // No minimum enforced yet
            Ok(true)
        }
    }

    /// Process upgrades that have reached their deadline
    pub fn check_upgrade_deadlines(&self) -> Result<Vec<String>, String> {
        let mut upgrades = self
            .pending_upgrades
            .write()
            .map_err(|e| format!("Lock error: {e}"))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        let mut enforced = Vec::new();

        // Check each upgrade
        upgrades.retain(|upgrade| {
            if upgrade.deadline <= now {
                // Deadline reached - enforce minimum version
                if let Some(ref min_version) = upgrade.min_required_version {
                    let mut min_req =
                        self.min_required_version
                            .write()
                            .unwrap_or_else(|poisoned| {
                                warn!("Min required version lock poisoned, recovering");
                                poisoned.into_inner()
                            });

                    // Update minimum required version if higher
                    if min_req.as_ref().is_none_or(|v| min_version > v) {
                        *min_req = Some(min_version.clone());
                        enforced.push(format!(
                            "Enforcing minimum version {} (upgrade {} deadline reached)",
                            min_version, upgrade.version
                        ));
                    }
                }
                false // Remove from pending
            } else {
                true // Keep in pending
            }
        });

        // Update peer deprecation status
        if !enforced.is_empty() {
            let min_version = self
                .min_required_version
                .read()
                .unwrap_or_else(|poisoned| {
                    warn!("Min required version lock poisoned, recovering");
                    poisoned.into_inner()
                })
                .clone();
            if let Some(min) = min_version {
                let mut versions = self.peer_versions.write().unwrap_or_else(|poisoned| {
                    warn!("Peer versions lock poisoned, recovering");
                    poisoned.into_inner()
                });
                for info in versions.values_mut() {
                    info.is_deprecated = info.version < min;
                }
            }
        }

        Ok(enforced)
    }

    /// Get adoption statistics for current upgrade
    pub fn get_adoption_stats(&self) -> Result<Option<UpgradeAdoptionStats>, String> {
        let upgrades = self
            .pending_upgrades
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;

        let versions = self
            .peer_versions
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;

        // Get the latest pending upgrade
        let latest_upgrade = upgrades.last();

        // Use if-let pattern to avoid unwrap
        let upgrade = match latest_upgrade {
            Some(u) => u,
            None => return Ok(None),
        };
        let target_version = &upgrade.version;

        let total_peers = versions.len();
        if total_peers == 0 {
            return Ok(None);
        }

        let mut at_target = 0;
        let mut at_compatible = 0;
        let mut at_deprecated = 0;

        for info in versions.values() {
            if info.version == *target_version {
                at_target += 1;
            } else if info.version.is_compatible_with(target_version) {
                at_compatible += 1;
            }

            if info.is_deprecated {
                at_deprecated += 1;
            }
        }

        let adoption_rate = at_target as f64 / total_peers as f64;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        let days_until_deadline = if upgrade.deadline > now {
            Some(((upgrade.deadline - now) / 86400) as i64)
        } else {
            Some(0)
        };

        Ok(Some(UpgradeAdoptionStats {
            total_peers,
            at_target_version: at_target,
            at_compatible_version: at_compatible,
            at_deprecated_version: at_deprecated,
            adoption_rate,
            days_until_deadline,
        }))
    }

    /// Get list of peers with deprecated versions
    pub fn get_deprecated_peers(&self) -> Result<Vec<(Did, Version)>, String> {
        let versions = self
            .peer_versions
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;

        Ok(versions
            .iter()
            .filter(|(_, info)| info.is_deprecated)
            .map(|(did, info)| (did.clone(), info.version.clone()))
            .collect())
    }

    /// Get pending upgrades
    pub fn get_pending_upgrades(&self) -> Result<Vec<PendingUpgrade>, String> {
        let upgrades = self
            .pending_upgrades
            .read()
            .map_err(|e| format!("Lock error: {e}"))?;
        Ok(upgrades.clone())
    }
}

impl Default for UpgradeCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a `PendingUpgrade` from primitive values.
///
/// This replaces the former `proposal_to_pending_upgrade` which took
/// `ProposalPayload` (an icn-governance type). Callers that have a
/// `ProposalPayload` should destructure it and pass the fields directly.
pub fn create_pending_upgrade(
    version: Version,
    deadline: u64,
    breaking_changes: Vec<String>,
    migration_guide: Option<String>,
    min_required_version: Option<Version>,
    approved_at: u64,
) -> PendingUpgrade {
    PendingUpgrade {
        version,
        deadline,
        breaking_changes,
        migration_guide,
        min_required_version,
        approved_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::Did;

    fn test_did(n: u8) -> Did {
        let mut anchor_id = [0u8; 32];
        anchor_id[0] = n;
        Did::from_anchor_id(&anchor_id)
    }

    #[test]
    fn test_version_tracking() {
        let coordinator = UpgradeCoordinator::new();

        let v1 = Version::new(0, 1, 0);
        let v2 = Version::new(0, 2, 0);

        coordinator
            .update_peer_version(test_did(1), v1.clone())
            .unwrap();
        coordinator
            .update_peer_version(test_did(2), v2.clone())
            .unwrap();

        let versions = coordinator.peer_versions.read().unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions.get(&test_did(1)).unwrap().version, v1);
        assert_eq!(versions.get(&test_did(2)).unwrap().version, v2);
    }

    #[test]
    fn test_upgrade_registration() {
        let coordinator = UpgradeCoordinator::new();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let upgrade = PendingUpgrade {
            version: Version::new(1, 0, 0),
            deadline: now + 86400 * 90, // 90 days
            breaking_changes: vec!["Breaking change 1".to_string()],
            migration_guide: Some("https://example.com/migration".to_string()),
            min_required_version: Some(Version::new(1, 0, 0)),
            approved_at: now,
        };

        coordinator.register_upgrade(upgrade.clone()).unwrap();

        let upgrades = coordinator.get_pending_upgrades().unwrap();
        assert_eq!(upgrades.len(), 1);
        assert_eq!(upgrades[0].version, Version::new(1, 0, 0));
    }

    #[test]
    fn test_version_acceptance() {
        let coordinator = UpgradeCoordinator::new();

        // Initially, all versions are acceptable
        assert!(coordinator
            .is_peer_version_acceptable(&Version::new(0, 1, 0))
            .unwrap());

        // Set minimum required version
        *coordinator.min_required_version.write().unwrap() = Some(Version::new(1, 0, 0));

        // Old version should be rejected
        assert!(!coordinator
            .is_peer_version_acceptable(&Version::new(0, 1, 0))
            .unwrap());

        // New version should be accepted
        assert!(coordinator
            .is_peer_version_acceptable(&Version::new(1, 0, 0))
            .unwrap());
        assert!(coordinator
            .is_peer_version_acceptable(&Version::new(1, 1, 0))
            .unwrap());
    }

    #[test]
    fn test_adoption_stats() {
        let coordinator = UpgradeCoordinator::new();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Register upgrade
        let upgrade = PendingUpgrade {
            version: Version::new(1, 0, 0),
            deadline: now + 86400 * 30, // 30 days
            breaking_changes: vec![],
            migration_guide: None,
            min_required_version: Some(Version::new(1, 0, 0)),
            approved_at: now,
        };
        coordinator.register_upgrade(upgrade).unwrap();

        // Add peer versions
        coordinator
            .update_peer_version(test_did(1), Version::new(1, 0, 0))
            .unwrap(); // At target
        coordinator
            .update_peer_version(test_did(2), Version::new(1, 0, 0))
            .unwrap(); // At target
        coordinator
            .update_peer_version(test_did(3), Version::new(0, 9, 0))
            .unwrap(); // Old version

        let stats = coordinator.get_adoption_stats().unwrap().unwrap();
        assert_eq!(stats.total_peers, 3);
        assert_eq!(stats.at_target_version, 2);
        assert!((stats.adoption_rate - 0.666).abs() < 0.01);
        assert!(stats.days_until_deadline.unwrap() > 0);
    }
}
