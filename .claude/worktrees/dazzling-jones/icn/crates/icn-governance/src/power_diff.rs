//! Power Diff — computed delta showing how an EffectManifest changes
//! the authorization/capability landscape.
//!
//! Attached to every proposal so voters see power implications before voting.
//! Three output layers: human-readable summary, structured data, cryptographic receipt.

use icn_kernel_api::Did;
use serde::{Deserialize, Serialize};

/// Computed power delta for a governance action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerDiff {
    /// Version for deterministic replay.
    pub diff_version: u16,
    /// Hash of the manifest this diff was computed from.
    pub manifest_hash: [u8; 32],
    /// Hash of the baseline state snapshot.
    pub baseline_snapshot_hash: [u8; 32],
    /// Per-subject changes.
    pub subject_deltas: Vec<SubjectDelta>,
    /// Aggregate summary.
    pub summary: DiffSummary,
    /// Risk indicators.
    pub warnings: Vec<PowerWarning>,
    /// Deterministic hash of the full diff.
    pub diff_hash: [u8; 32],
}

/// Change in capabilities for a single subject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectDelta {
    pub subject: Did,
    pub capabilities_gained: Vec<CapabilityChange>,
    pub capabilities_lost: Vec<CapabilityChange>,
    pub scope_widened: Vec<ScopeChange>,
    pub scope_narrowed: Vec<ScopeChange>,
}

/// A single capability change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityChange {
    pub capability: String,
    pub scope: String,
    pub resource: String,
}

/// A scope change (widening or narrowing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeChange {
    pub capability: String,
    pub old_scope: String,
    pub new_scope: String,
}

/// Aggregate diff metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub total_grants: u32,
    pub total_revocations: u32,
    pub subjects_affected: u32,
    /// Positive = more concentrated. Negative = more distributed.
    pub concentration_delta: f64,
    pub exit_rights_affected: bool,
    pub appeal_rights_affected: bool,
}

/// Risk warnings that trigger tier escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerWarning {
    /// Power concentration spike for a single subject.
    ConcentrationSpike {
        subject: Did,
        old_score: f64,
        new_score: f64,
    },
    /// Exit rights narrowed for an entity.
    ExitRightNarrowed { entity_id: String },
    /// An appeal path was removed.
    AppealPathRemoved { decision_type: String },
    /// A subject gains unilateral capability (no quorum required).
    UnilateralCapability { subject: Did, capability: String },
    /// An irreversible change was detected.
    IrreversibleChange { effect_index: usize, reason: String },
}

impl PowerWarning {
    /// Minimum governance tier required when this warning is present.
    pub fn min_tier(&self) -> u8 {
        match self {
            Self::ConcentrationSpike { .. } => 3,
            Self::ExitRightNarrowed { .. } => 3,
            Self::AppealPathRemoved { .. } => 4,
            Self::UnilateralCapability { .. } => 4,
            Self::IrreversibleChange { .. } => 3,
        }
    }
}

impl PowerDiff {
    /// Compute deterministic hash of the diff.
    pub fn compute_hash(
        diff_version: u16,
        manifest_hash: [u8; 32],
        baseline_snapshot_hash: [u8; 32],
        subject_deltas: &[SubjectDelta],
        summary: &DiffSummary,
        warnings: &[PowerWarning],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&diff_version.to_le_bytes());
        hasher.update(&manifest_hash);
        hasher.update(&baseline_snapshot_hash);
        let deltas_json = serde_json::to_vec(subject_deltas).unwrap_or_default();
        hasher.update(&(deltas_json.len() as u32).to_le_bytes());
        hasher.update(&deltas_json);
        let summary_json = serde_json::to_vec(summary).unwrap_or_default();
        hasher.update(&summary_json);
        let warnings_json = serde_json::to_vec(warnings).unwrap_or_default();
        hasher.update(&warnings_json);
        *hasher.finalize().as_bytes()
    }

    /// Build a diff with computed hash.
    pub fn new(
        manifest_hash: [u8; 32],
        baseline_snapshot_hash: [u8; 32],
        subject_deltas: Vec<SubjectDelta>,
        summary: DiffSummary,
        warnings: Vec<PowerWarning>,
    ) -> Self {
        let diff_hash = Self::compute_hash(
            1,
            manifest_hash,
            baseline_snapshot_hash,
            &subject_deltas,
            &summary,
            &warnings,
        );
        Self {
            diff_version: 1,
            manifest_hash,
            baseline_snapshot_hash,
            subject_deltas,
            summary,
            warnings,
            diff_hash,
        }
    }

    /// Maximum tier escalation required by warnings.
    /// Returns 0 if no warnings.
    pub fn max_escalation_tier(&self) -> u8 {
        self.warnings
            .iter()
            .map(|w| w.min_tier())
            .max()
            .unwrap_or(0)
    }
}

/// Self-authenticating cryptographic receipt for a Power Diff.
/// Same pattern as GovernanceDecisionReceipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerDiffReceipt {
    pub diff_hash: [u8; 32],
    pub manifest_hash: [u8; 32],
    pub baseline_snapshot_hash: [u8; 32],
    pub report_hash: [u8; 32],
    pub computed_by: Did,
    pub timestamp: u64,
    /// Ed25519 signature over the receipt hash.
    pub signature: Vec<u8>,
    /// Deterministic hash of the receipt (signed content).
    pub receipt_hash: [u8; 32],
}

impl PowerDiffReceipt {
    /// Compute the receipt hash (the content that gets signed).
    pub fn compute_receipt_hash(
        diff_hash: [u8; 32],
        manifest_hash: [u8; 32],
        baseline_snapshot_hash: [u8; 32],
        report_hash: [u8; 32],
        computed_by: &str,
        timestamp: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&diff_hash);
        hasher.update(&manifest_hash);
        hasher.update(&baseline_snapshot_hash);
        hasher.update(&report_hash);
        hasher.update(computed_by.as_bytes());
        hasher.update(&timestamp.to_le_bytes());
        *hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_diff() {
        let d = PowerDiff::new([0u8; 32], [0u8; 32], vec![], DiffSummary::default(), vec![]);
        assert_eq!(d.max_escalation_tier(), 0);
        assert_ne!(d.diff_hash, [0u8; 32]);
    }

    #[test]
    fn test_warning_escalation() {
        let w = vec![
            PowerWarning::ConcentrationSpike {
                subject: "did:icn:alice".into(),
                old_score: 0.1,
                new_score: 0.5,
            },
            PowerWarning::AppealPathRemoved {
                decision_type: "expulsion".into(),
            },
        ];
        let d = PowerDiff::new([0u8; 32], [0u8; 32], vec![], DiffSummary::default(), w);
        assert_eq!(d.max_escalation_tier(), 4);
    }

    #[test]
    fn test_diff_hash_deterministic() {
        let d1 = PowerDiff::new([1u8; 32], [2u8; 32], vec![], DiffSummary::default(), vec![]);
        let d2 = PowerDiff::new([1u8; 32], [2u8; 32], vec![], DiffSummary::default(), vec![]);
        assert_eq!(d1.diff_hash, d2.diff_hash);
    }

    #[test]
    fn test_serde_roundtrip() {
        let d = PowerDiff::new([0u8; 32], [0u8; 32], vec![], DiffSummary::default(), vec![]);
        let json = serde_json::to_string(&d).unwrap();
        let parsed: PowerDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.diff_hash, d.diff_hash);
    }

    #[test]
    fn test_receipt_hash_deterministic() {
        let h1 = PowerDiffReceipt::compute_receipt_hash(
            [1u8; 32],
            [2u8; 32],
            [3u8; 32],
            [4u8; 32],
            "did:icn:a",
            12345,
        );
        let h2 = PowerDiffReceipt::compute_receipt_hash(
            [1u8; 32],
            [2u8; 32],
            [3u8; 32],
            [4u8; 32],
            "did:icn:a",
            12345,
        );
        assert_eq!(h1, h2);
        assert_ne!(h1, [0u8; 32]);
    }
}
