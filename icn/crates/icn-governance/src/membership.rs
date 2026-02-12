//! Membership configuration - who is eligible to vote

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Configuration for determining voting membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipConfig {
    /// Source of membership determination
    pub source: MembershipSource,
}

impl MembershipConfig {
    /// Create membership from a static list of DIDs
    pub fn static_list(members: Vec<Did>) -> Self {
        Self {
            source: MembershipSource::StaticList(members),
        }
    }

    /// Create membership from trust graph threshold
    ///
    /// Members are those with trust score >= threshold
    pub fn trust_threshold(threshold: f64) -> Self {
        Self {
            source: MembershipSource::TrustThreshold(threshold),
        }
    }
}

/// Source of membership determination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MembershipSource {
    /// Explicit list of member DIDs
    StaticList(Vec<Did>),

    /// Members determined by trust graph threshold
    /// Value is the minimum trust score required (0.0 - 1.0)
    TrustThreshold(f64),
}

impl MembershipSource {
    /// Check if a DID is in the static list (if applicable)
    pub fn contains_static(&self, did: &Did) -> bool {
        match self {
            MembershipSource::StaticList(members) => members.contains(did),
            _ => false,
        }
    }

    /// Get the trust threshold (if applicable)
    pub fn trust_threshold(&self) -> Option<f64> {
        match self {
            MembershipSource::TrustThreshold(threshold) => Some(*threshold),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_static_list_membership() {
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let did1 = kp1.did().clone();
        let did2 = kp2.did().clone();

        let config = MembershipConfig::static_list(vec![did1.clone(), did2.clone()]);

        assert!(config.source.contains_static(&did1));
        assert!(config.source.contains_static(&did2));

        let kp3 = KeyPair::generate().unwrap();
        let did3 = kp3.did().clone();
        assert!(!config.source.contains_static(&did3));
    }

    #[test]
    fn test_trust_threshold_membership() {
        let config = MembershipConfig::trust_threshold(0.5);

        assert_eq!(config.source.trust_threshold(), Some(0.5));
        assert!(!config
            .source
            .contains_static(KeyPair::generate().unwrap().did()));
    }
}
