//! Frozen-core invariant types for governance safety.
//!
//! These types are kernel-safe: the kernel verifies attestation presence,
//! format, and hash linkage. It never evaluates the predicates themselves.
//! Predicate evaluation lives in the governance app layer.

use serde::{Deserialize, Serialize};

/// Block height for ordering governance transitions.
pub type BlockHeight = u64;

/// Identifies which frozen-core invariant was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantId {
    /// #1: No rule can prevent an identity from leaving an entity.
    RightOfExit,
    /// #2: Rules only apply to actions after activation block.
    NoRetroactiveObligations,
    /// #3: Adding an identity requires their cryptographic signature.
    ExplicitCryptographicEntry,
    /// #4: Authz changes must include a computable capability delta.
    NoSilentPowerChanges,
    /// #5: No wildcard capabilities across all resources.
    BoundedAuthority,
    /// #6: Economic/capability changes require timelock delay.
    MandatoryExecutionDelay,
    /// #7: Cannot delegate capabilities you don't possess.
    CapabilityConservation,
    /// #8: Mutual credit loops must net to zero.
    MutualCreditConservation,
    /// #9: Funds move only via owner signature or pre-consented dispute hook.
    SovereignSignatureRequirement,
    /// #10: Every state transition emits an immutable receipt.
    UniversalReceiptGeneration,
}

/// Domain classification for invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantDomain {
    /// Consent & exit invariants (#1-#3).
    SovereigntyCore,
    /// Power & governance invariants (#4-#7).
    AntiCaptureCore,
    /// Economic invariants (#8-#10).
    EconomicCore,
}

impl InvariantId {
    /// Which domain does this invariant belong to?
    pub fn domain(&self) -> InvariantDomain {
        match self {
            Self::RightOfExit
            | Self::NoRetroactiveObligations
            | Self::ExplicitCryptographicEntry => InvariantDomain::SovereigntyCore,
            Self::NoSilentPowerChanges
            | Self::BoundedAuthority
            | Self::MandatoryExecutionDelay
            | Self::CapabilityConservation => InvariantDomain::AntiCaptureCore,
            Self::MutualCreditConservation
            | Self::SovereignSignatureRequirement
            | Self::UniversalReceiptGeneration => InvariantDomain::EconomicCore,
        }
    }

    /// All frozen-core invariant IDs.
    pub fn all() -> &'static [InvariantId] {
        &[
            Self::RightOfExit,
            Self::NoRetroactiveObligations,
            Self::ExplicitCryptographicEntry,
            Self::NoSilentPowerChanges,
            Self::BoundedAuthority,
            Self::MandatoryExecutionDelay,
            Self::CapabilityConservation,
            Self::MutualCreditConservation,
            Self::SovereignSignatureRequirement,
            Self::UniversalReceiptGeneration,
        ]
    }
}

/// A single invariant violation with machine-readable details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantViolation {
    pub id: InvariantId,
    pub domain: InvariantDomain,
    pub message: String,
    /// Machine-readable key-value details for logging/UX.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<(String, String)>,
}

/// Report from running all invariants against a manifest.
///
/// The kernel checks `passed` and `report_hash` -- it never inspects
/// individual violations (meaning firewall).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantReport {
    /// Gate version for deterministic replay.
    pub report_version: u16,
    /// Hash of the manifest that was checked.
    pub manifest_hash: [u8; 32],
    /// Hash of the power diff (if present).
    pub power_diff_hash: Option<[u8; 32]>,
    /// Did all invariants pass?
    pub passed: bool,
    /// Violations found (empty if passed).
    pub violations: Vec<InvariantViolation>,
    /// Deterministic hash over the full report content.
    pub report_hash: [u8; 32],
}

impl InvariantReport {
    /// Compute the deterministic report hash.
    pub fn compute_hash(
        report_version: u16,
        manifest_hash: [u8; 32],
        power_diff_hash: Option<[u8; 32]>,
        passed: bool,
        violations: &[InvariantViolation],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&report_version.to_le_bytes());
        hasher.update(&manifest_hash);
        if let Some(pdh) = power_diff_hash {
            hasher.update(&[1u8]);
            hasher.update(&pdh);
        } else {
            hasher.update(&[0u8]);
        }
        hasher.update(&[passed as u8]);
        // Hash each violation deterministically
        for v in violations {
            let v_json = serde_json::to_vec(v).unwrap_or_default();
            hasher.update(&(v_json.len() as u32).to_le_bytes());
            hasher.update(&v_json);
        }
        *hasher.finalize().as_bytes()
    }

    /// Build a report with computed hash.
    pub fn new(
        manifest_hash: [u8; 32],
        power_diff_hash: Option<[u8; 32]>,
        violations: Vec<InvariantViolation>,
    ) -> Self {
        let passed = violations.is_empty();
        let report_hash =
            Self::compute_hash(1, manifest_hash, power_diff_hash, passed, &violations);
        Self {
            report_version: 1,
            manifest_hash,
            power_diff_hash,
            passed,
            violations,
            report_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_id_domain_mapping() {
        assert_eq!(
            InvariantId::RightOfExit.domain(),
            InvariantDomain::SovereigntyCore
        );
        assert_eq!(
            InvariantId::NoSilentPowerChanges.domain(),
            InvariantDomain::AntiCaptureCore
        );
        assert_eq!(
            InvariantId::MutualCreditConservation.domain(),
            InvariantDomain::EconomicCore
        );
    }

    #[test]
    fn test_all_invariants_returns_10() {
        assert_eq!(InvariantId::all().len(), 10);
    }

    #[test]
    fn test_report_passing() {
        let report = InvariantReport::new([0u8; 32], None, vec![]);
        assert!(report.passed);
        assert!(report.violations.is_empty());
        assert_ne!(report.report_hash, [0u8; 32]);
    }

    #[test]
    fn test_report_failing() {
        let violation = InvariantViolation {
            id: InvariantId::BoundedAuthority,
            domain: InvariantDomain::AntiCaptureCore,
            message: "Wildcard scope detected".into(),
            details: vec![("scope".into(), "*".into())],
        };
        let report = InvariantReport::new([0u8; 32], None, vec![violation]);
        assert!(!report.passed);
        assert_eq!(report.violations.len(), 1);
    }

    #[test]
    fn test_report_hash_deterministic() {
        let v = InvariantViolation {
            id: InvariantId::MandatoryExecutionDelay,
            domain: InvariantDomain::AntiCaptureCore,
            message: "Timelock too short".into(),
            details: vec![],
        };
        let r1 = InvariantReport::new([1u8; 32], Some([2u8; 32]), vec![v.clone()]);
        let r2 = InvariantReport::new([1u8; 32], Some([2u8; 32]), vec![v]);
        assert_eq!(r1.report_hash, r2.report_hash);
    }

    #[test]
    fn test_serde_roundtrip() {
        let report = InvariantReport::new([42u8; 32], None, vec![]);
        let json = serde_json::to_string(&report).unwrap();
        let parsed: InvariantReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.report_hash, report.report_hash);
        assert!(parsed.passed);
    }
}
