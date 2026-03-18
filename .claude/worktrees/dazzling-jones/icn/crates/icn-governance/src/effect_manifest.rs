//! Effect Manifest — normalized, hashable description of governance actions.
//!
//! Every governance mutation (proposal, amendment, parameter change) must
//! produce an EffectManifest before the system applies it.
//! The manifest is the input to the Invariant Gate and Power Diff.

use icn_kernel_api::invariants::BlockHeight;
use icn_kernel_api::Did;
use serde::{Deserialize, Serialize};

/// A normalized, deterministic description of what a governance action
/// does to system state. This is the "effect vector" — typed, hashable,
/// and exhaustive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectManifest {
    /// Version for deterministic replay.
    pub manifest_version: u16,
    /// Hash of the amendment/proposal this was derived from.
    pub change_hash: [u8; 32],
    /// Snapshot hash of the state this was diffed against.
    pub baseline_snapshot_hash: [u8; 32],
    /// DID of the manifest author.
    pub author: Did,
    /// Capability/authorization perimeter changes (if any).
    pub capability_effects: Vec<CapabilityEffect>,
    /// Economic policy changes (timelock-relevant).
    pub economic_effects: Vec<EconomicEffect>,
    /// Membership / identity-set changes (consent-relevant).
    pub membership_effects: Vec<MembershipEffect>,
    /// Protocol / governance parameter changes.
    pub protocol_effects: Vec<ProtocolEffect>,
    /// Block height when ratified (set after ratification).
    pub ratification_block: Option<BlockHeight>,
    /// Block height when activated (enforced by timelock invariant).
    pub activation_block: Option<BlockHeight>,
    /// Deterministic hash of this manifest.
    pub manifest_hash: [u8; 32],
}

/// Capability/authorization perimeter change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CapabilityEffect {
    Grant {
        subject: Did,
        capability: String,
        scope: String,
        resource: String,
    },
    Revoke {
        subject: Did,
        capability: String,
        scope: String,
        resource: String,
    },
}

/// Economic policy change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EconomicEffect {
    MutualCreditPolicyChange {
        entity_id: String,
        policy_key: String,
        before_hash: [u8; 32],
        after_hash: [u8; 32],
    },
    BudgetScopeChange {
        entity_id: String,
        envelope_id: String,
        before_hash: [u8; 32],
        after_hash: [u8; 32],
    },
    DemurrageChange {
        entity_id: String,
        old_rate_bps: u32,
        new_rate_bps: u32,
    },
    DisputeHookChange {
        entity_id: String,
        hook_id: String,
        before_hash: [u8; 32],
        after_hash: [u8; 32],
    },
    SettlementRuleChange {
        entity_id: String,
        rule_id: String,
        before_hash: [u8; 32],
        after_hash: [u8; 32],
    },
}

/// Membership/identity-set change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MembershipEffect {
    AddToEntity {
        entity_id: String,
        identity: Did,
        role: Option<String>,
    },
    RemoveFromEntity {
        entity_id: String,
        identity: Did,
    },
    AddObligation {
        entity_id: String,
        identity: Did,
        obligation_hash: [u8; 32],
    },
}

/// Protocol/governance parameter change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolEffect {
    ParameterChange {
        param_id: String,
        old_value: String,
        new_value: String,
        scope: String,
    },
    TimelockChange {
        entity_id: String,
        old_delay_secs: u64,
        new_delay_secs: u64,
    },
    AuthzModelChange {
        entity_id: String,
        before_hash: [u8; 32],
        after_hash: [u8; 32],
    },
    GovernanceExecutionChange {
        entity_id: String,
        before_hash: [u8; 32],
        after_hash: [u8; 32],
    },
}

impl EffectManifest {
    /// Compute deterministic hash over manifest content.
    pub fn compute_hash(
        manifest_version: u16,
        change_hash: [u8; 32],
        baseline_snapshot_hash: [u8; 32],
        author: &str,
        capability_effects: &[CapabilityEffect],
        economic_effects: &[EconomicEffect],
        membership_effects: &[MembershipEffect],
        protocol_effects: &[ProtocolEffect],
        ratification_block: Option<BlockHeight>,
        activation_block: Option<BlockHeight>,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&manifest_version.to_le_bytes());
        hasher.update(&change_hash);
        hasher.update(&baseline_snapshot_hash);
        hasher.update(author.as_bytes());
        // Hash each effect category deterministically via JSON
        for effects in [
            &serde_json::to_vec(capability_effects).unwrap_or_default(),
            &serde_json::to_vec(economic_effects).unwrap_or_default(),
            &serde_json::to_vec(membership_effects).unwrap_or_default(),
            &serde_json::to_vec(protocol_effects).unwrap_or_default(),
        ] {
            hasher.update(&(effects.len() as u32).to_le_bytes());
            hasher.update(effects);
        }
        if let Some(rb) = ratification_block {
            hasher.update(&[1u8]);
            hasher.update(&rb.to_le_bytes());
        } else {
            hasher.update(&[0u8]);
        }
        if let Some(ab) = activation_block {
            hasher.update(&[1u8]);
            hasher.update(&ab.to_le_bytes());
        } else {
            hasher.update(&[0u8]);
        }
        *hasher.finalize().as_bytes()
    }

    /// Build a manifest with computed hash.
    pub fn new(
        change_hash: [u8; 32],
        baseline_snapshot_hash: [u8; 32],
        author: Did,
        capability_effects: Vec<CapabilityEffect>,
        economic_effects: Vec<EconomicEffect>,
        membership_effects: Vec<MembershipEffect>,
        protocol_effects: Vec<ProtocolEffect>,
    ) -> Self {
        let manifest_hash = Self::compute_hash(
            1,
            change_hash,
            baseline_snapshot_hash,
            &author,
            &capability_effects,
            &economic_effects,
            &membership_effects,
            &protocol_effects,
            None,
            None,
        );
        Self {
            manifest_version: 1,
            change_hash,
            baseline_snapshot_hash,
            author,
            capability_effects,
            economic_effects,
            membership_effects,
            protocol_effects,
            ratification_block: None,
            activation_block: None,
            manifest_hash,
        }
    }

    /// Does this manifest touch the authorization perimeter?
    pub fn touches_authz(&self) -> bool {
        !self.capability_effects.is_empty()
            || self
                .protocol_effects
                .iter()
                .any(|e| matches!(e, ProtocolEffect::AuthzModelChange { .. }))
    }

    /// Does this manifest touch economic policy?
    pub fn touches_economics(&self) -> bool {
        !self.economic_effects.is_empty()
    }

    /// Is this manifest empty (no effects)?
    pub fn is_empty(&self) -> bool {
        self.capability_effects.is_empty()
            && self.economic_effects.is_empty()
            && self.membership_effects.is_empty()
            && self.protocol_effects.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manifest() {
        let m = EffectManifest::new(
            [0u8; 32],
            [0u8; 32],
            "did:icn:test".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        assert!(m.is_empty());
        assert!(!m.touches_authz());
        assert!(!m.touches_economics());
        assert_ne!(m.manifest_hash, [0u8; 32]);
    }

    #[test]
    fn test_manifest_with_capability_effect() {
        let m = EffectManifest::new(
            [1u8; 32],
            [0u8; 32],
            "did:icn:author".to_string(),
            vec![CapabilityEffect::Grant {
                subject: "did:icn:bob".into(),
                capability: "vote:operational".into(),
                scope: "coop:sunrise".into(),
                resource: "governance".into(),
            }],
            vec![],
            vec![],
            vec![],
        );
        assert!(m.touches_authz());
        assert!(!m.touches_economics());
    }

    #[test]
    fn test_manifest_hash_deterministic() {
        let m1 = EffectManifest::new(
            [1u8; 32],
            [2u8; 32],
            "did:icn:a".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let m2 = EffectManifest::new(
            [1u8; 32],
            [2u8; 32],
            "did:icn:a".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(m1.manifest_hash, m2.manifest_hash);
    }

    #[test]
    fn test_manifest_hash_changes_with_effects() {
        let m1 = EffectManifest::new(
            [1u8; 32],
            [0u8; 32],
            "did:icn:a".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let m2 = EffectManifest::new(
            [1u8; 32],
            [0u8; 32],
            "did:icn:a".to_string(),
            vec![],
            vec![EconomicEffect::DemurrageChange {
                entity_id: "coop:x".into(),
                old_rate_bps: 100,
                new_rate_bps: 200,
            }],
            vec![],
            vec![],
        );
        assert_ne!(m1.manifest_hash, m2.manifest_hash);
    }

    #[test]
    fn test_serde_roundtrip() {
        let m = EffectManifest::new(
            [42u8; 32],
            [0u8; 32],
            "did:icn:test".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let json = serde_json::to_string(&m).unwrap();
        let parsed: EffectManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.manifest_hash, m.manifest_hash);
    }
}
