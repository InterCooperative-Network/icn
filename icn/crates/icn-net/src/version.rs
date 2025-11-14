//! Protocol version negotiation and capability detection
//!
//! This module implements version negotiation for ICN, enabling nodes running different
//! software versions to communicate safely and discover each other's capabilities.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Version capabilities announced during handshake
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionInfo {
    /// Current protocol version this node is running
    pub current_version: u32,

    /// Minimum protocol version this node supports
    pub min_supported: u32,

    /// Maximum protocol version this node supports
    pub max_supported: u32,

    /// Optional capabilities bitmap for feature detection
    pub capabilities: CapabilityFlags,

    /// Software version string (e.g., "icnd-0.1.0")
    pub software_version: String,
}

impl VersionInfo {
    /// Create new version info with default ICN protocol version
    pub fn new(software_version: String) -> Self {
        Self {
            current_version: crate::protocol::PROTOCOL_VERSION,
            min_supported: crate::protocol::MIN_SUPPORTED_VERSION,
            max_supported: crate::protocol::MAX_SUPPORTED_VERSION,
            capabilities: CapabilityFlags::current(),
            software_version,
        }
    }

    /// Check if this node supports a specific capability
    pub fn has_capability(&self, capability: CapabilityFlags) -> bool {
        self.capabilities.contains(capability)
    }
}

bitflags::bitflags! {
    /// Feature capability flags
    ///
    /// These flags indicate which optional features a node supports.
    /// Nodes can check peer capabilities to determine which features to use.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CapabilityFlags: u64 {
        /// Supports end-to-end encryption (Phase 10)
        const E2E_ENCRYPTION = 0b00000001;

        /// Supports signed envelopes (Phase 9)
        const SIGNED_MESSAGES = 0b00000010;

        /// Supports graceful restart / state snapshots
        const GRACEFUL_RESTART = 0b00000100;

        /// Supports topology-aware networking
        const TOPOLOGY_AWARE = 0b00001000;

        /// Supports trust-gated rate limiting
        const TRUST_RATE_LIMITING = 0b00010000;

        /// Supports gossip pull protocol
        const GOSSIP_PULL = 0b00100000;

        /// Supports multi-device identity (Phase 11)
        const MULTI_DEVICE = 0b01000000;

        /// Supports economic safety rails (Phase 12)
        const ECONOMIC_SAFETY = 0b10000000;

        // Future capabilities can be added here
        // Reserve high bits for future use
    }
}

// Manual serde implementation for CapabilityFlags
impl Serialize for CapabilityFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CapabilityFlags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bits = u64::deserialize(deserializer)?;
        Ok(CapabilityFlags::from_bits_truncate(bits))
    }
}

impl CapabilityFlags {
    /// Get current capabilities supported by this build
    pub fn current() -> Self {
        Self::E2E_ENCRYPTION
            | Self::SIGNED_MESSAGES
            | Self::GRACEFUL_RESTART
            | Self::TOPOLOGY_AWARE
            | Self::TRUST_RATE_LIMITING
            | Self::GOSSIP_PULL
            | Self::MULTI_DEVICE
            | Self::ECONOMIC_SAFETY
    }

    /// Get a human-readable list of capabilities
    pub fn describe(&self) -> Vec<&'static str> {
        let mut features = Vec::new();
        if self.contains(Self::E2E_ENCRYPTION) {
            features.push("e2e_encryption");
        }
        if self.contains(Self::SIGNED_MESSAGES) {
            features.push("signed_messages");
        }
        if self.contains(Self::GRACEFUL_RESTART) {
            features.push("graceful_restart");
        }
        if self.contains(Self::TOPOLOGY_AWARE) {
            features.push("topology_aware");
        }
        if self.contains(Self::TRUST_RATE_LIMITING) {
            features.push("trust_rate_limiting");
        }
        if self.contains(Self::GOSSIP_PULL) {
            features.push("gossip_pull");
        }
        if self.contains(Self::MULTI_DEVICE) {
            features.push("multi_device");
        }
        if self.contains(Self::ECONOMIC_SAFETY) {
            features.push("economic_safety");
        }
        features
    }
}

/// Negotiate protocol version between two nodes
///
/// This function finds the highest mutually supported protocol version.
/// It ensures both nodes can communicate by finding the overlap between
/// their supported version ranges.
///
/// # Arguments
///
/// * `local` - This node's version information
/// * `remote` - The remote peer's version information
///
/// # Returns
///
/// The negotiated protocol version to use for this connection
///
/// # Errors
///
/// Returns an error if there is no compatible version (no overlap in ranges)
pub fn negotiate_version(local: &VersionInfo, remote: &VersionInfo) -> Result<u32> {
    // Find the overlap between supported versions
    // Use the minimum of the two max_supported versions
    let negotiated = std::cmp::min(local.max_supported, remote.max_supported);

    // Verify it's within both nodes' supported range
    if negotiated < local.min_supported || negotiated < remote.min_supported {
        bail!(
            "No compatible version. Local: [{}-{}], Remote: [{}-{}]",
            local.min_supported,
            local.max_supported,
            remote.min_supported,
            remote.max_supported
        );
    }

    Ok(negotiated)
}

/// Calculate the common capabilities between two nodes
///
/// This returns the intersection of capabilities, representing features
/// that both nodes support.
pub fn common_capabilities(local: &VersionInfo, remote: &VersionInfo) -> CapabilityFlags {
    local.capabilities & remote.capabilities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_creation() {
        let info = VersionInfo::new("icnd-0.1.0".to_string());
        assert_eq!(info.current_version, crate::protocol::PROTOCOL_VERSION);
        assert_eq!(info.min_supported, crate::protocol::MIN_SUPPORTED_VERSION);
        assert_eq!(info.max_supported, crate::protocol::MAX_SUPPORTED_VERSION);
        assert_eq!(info.software_version, "icnd-0.1.0");
    }

    #[test]
    fn test_capability_checking() {
        let info = VersionInfo::new("icnd-0.1.0".to_string());
        assert!(info.has_capability(CapabilityFlags::E2E_ENCRYPTION));
        assert!(info.has_capability(CapabilityFlags::SIGNED_MESSAGES));
        assert!(info.has_capability(CapabilityFlags::GRACEFUL_RESTART));
    }

    #[test]
    fn test_negotiate_same_version() {
        let local = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 1,
            capabilities: CapabilityFlags::SIGNED_MESSAGES,
            software_version: "icnd-0.1.0".to_string(),
        };

        let remote = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 1,
            capabilities: CapabilityFlags::SIGNED_MESSAGES,
            software_version: "icnd-0.1.0".to_string(),
        };

        let negotiated = negotiate_version(&local, &remote).unwrap();
        assert_eq!(negotiated, 1);
    }

    #[test]
    fn test_negotiate_backward_compatible() {
        // Local node (running v1)
        let local = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 1,
            capabilities: CapabilityFlags::SIGNED_MESSAGES | CapabilityFlags::GRACEFUL_RESTART,
            software_version: "icnd-0.1.0".to_string(),
        };

        // Remote node (running v2, backward compatible with v1)
        let remote = VersionInfo {
            current_version: 2,
            min_supported: 1,
            max_supported: 2,
            capabilities: CapabilityFlags::all(),
            software_version: "icnd-0.2.0".to_string(),
        };

        // Should negotiate to version 1 (the highest both support)
        let negotiated = negotiate_version(&local, &remote).unwrap();
        assert_eq!(negotiated, 1);
    }

    #[test]
    fn test_negotiate_forward_compatible() {
        // Older node
        let old = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 2,
            capabilities: CapabilityFlags::SIGNED_MESSAGES,
            software_version: "icnd-0.1.0".to_string(),
        };

        // Newer node
        let new = VersionInfo {
            current_version: 3,
            min_supported: 2,
            max_supported: 3,
            capabilities: CapabilityFlags::all(),
            software_version: "icnd-0.3.0".to_string(),
        };

        // Should negotiate to version 2 (overlap)
        let negotiated = negotiate_version(&old, &new).unwrap();
        assert_eq!(negotiated, 2);
    }

    #[test]
    fn test_negotiate_incompatible_versions() {
        // Old node that only supports v1
        let old = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 1,
            capabilities: CapabilityFlags::empty(),
            software_version: "icnd-0.1.0".to_string(),
        };

        // New node that dropped v1 support
        let new = VersionInfo {
            current_version: 3,
            min_supported: 2, // No longer supports v1
            max_supported: 3,
            capabilities: CapabilityFlags::all(),
            software_version: "icnd-0.3.0".to_string(),
        };

        // Should fail - no overlap
        let result = negotiate_version(&old, &new);
        assert!(result.is_err());

        let error = result.unwrap_err().to_string();
        assert!(error.contains("No compatible version"));
        assert!(error.contains("Local: [1-1]"));
        assert!(error.contains("Remote: [2-3]"));
    }

    #[test]
    fn test_common_capabilities() {
        let local = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 1,
            capabilities: CapabilityFlags::SIGNED_MESSAGES
                | CapabilityFlags::GRACEFUL_RESTART
                | CapabilityFlags::E2E_ENCRYPTION,
            software_version: "icnd-0.1.0".to_string(),
        };

        let remote = VersionInfo {
            current_version: 1,
            min_supported: 1,
            max_supported: 1,
            capabilities: CapabilityFlags::SIGNED_MESSAGES | CapabilityFlags::TOPOLOGY_AWARE,
            software_version: "icnd-0.1.0".to_string(),
        };

        let common = common_capabilities(&local, &remote);

        // Only SIGNED_MESSAGES is common
        assert!(common.contains(CapabilityFlags::SIGNED_MESSAGES));
        assert!(!common.contains(CapabilityFlags::GRACEFUL_RESTART));
        assert!(!common.contains(CapabilityFlags::E2E_ENCRYPTION));
        assert!(!common.contains(CapabilityFlags::TOPOLOGY_AWARE));
    }

    #[test]
    fn test_capability_describe() {
        let caps = CapabilityFlags::E2E_ENCRYPTION
            | CapabilityFlags::SIGNED_MESSAGES
            | CapabilityFlags::GRACEFUL_RESTART;

        let described = caps.describe();
        assert_eq!(described.len(), 3);
        assert!(described.contains(&"e2e_encryption"));
        assert!(described.contains(&"signed_messages"));
        assert!(described.contains(&"graceful_restart"));
    }

    #[test]
    fn test_capability_serialization() {
        let caps = CapabilityFlags::E2E_ENCRYPTION | CapabilityFlags::SIGNED_MESSAGES;

        // Serialize
        let json = serde_json::to_string(&caps).unwrap();

        // Deserialize
        let deserialized: CapabilityFlags = serde_json::from_str(&json).unwrap();

        assert_eq!(caps, deserialized);
    }

    #[test]
    fn test_version_info_serialization() {
        let info = VersionInfo {
            current_version: 2,
            min_supported: 1,
            max_supported: 3,
            capabilities: CapabilityFlags::all(),
            software_version: "icnd-0.2.0".to_string(),
        };

        // Serialize
        let json = serde_json::to_string(&info).unwrap();

        // Deserialize
        let deserialized: VersionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_current_capabilities() {
        let caps = CapabilityFlags::current();

        // Should include all currently implemented features
        assert!(caps.contains(CapabilityFlags::E2E_ENCRYPTION));
        assert!(caps.contains(CapabilityFlags::SIGNED_MESSAGES));
        assert!(caps.contains(CapabilityFlags::GRACEFUL_RESTART));
        assert!(caps.contains(CapabilityFlags::TOPOLOGY_AWARE));
        assert!(caps.contains(CapabilityFlags::TRUST_RATE_LIMITING));
        assert!(caps.contains(CapabilityFlags::GOSSIP_PULL));
        assert!(caps.contains(CapabilityFlags::MULTI_DEVICE));
        assert!(caps.contains(CapabilityFlags::ECONOMIC_SAFETY));
    }
}
