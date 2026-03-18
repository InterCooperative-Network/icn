//! State hash verification utilities for cross-node state comparison.
//!
//! This module provides utilities to extract and compare state hashes from
//! service execution outcomes. This enables verification that two nodes
//! replaying the same decision produce identical durable state.
//!
//! # Architecture
//!
//! Stage 7 (decision-receipt-replay) requires proof that:
//! ```text
//! Node A state == Node B state after replaying same decision
//! ```
//!
//! Each service returns a `state_change_hash` on successful operations:
//! - Ledger: entry_hash from TreasuryEntryResult
//! - Membership: state_change_hash from AddMemberResult, etc.
//! - Federation: state_change_hash from FederationJoinResult, etc.
//! - Control: state_change_hash from VetoProposalResult, etc.
//!
//! These hashes are aggregated into a `StateHashSet` for comparison.

use icn_kernel_api::effects::EffectResult;
use icn_kernel_api::governance::ExecutionOutcome;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Aggregated state hashes from execution outcomes.
///
/// Uses a BTreeMap to ensure deterministic ordering when computing
/// composite hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateHashSet {
    /// Map of effect_id → state_change_hash
    pub hashes: BTreeMap<String, String>,
}

impl StateHashSet {
    /// Create a new empty StateHashSet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a state hash for an effect.
    pub fn insert(&mut self, effect_id: String, state_change_hash: String) {
        self.hashes.insert(effect_id, state_change_hash);
    }

    /// Check if this set is empty.
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    /// Get the number of state hashes.
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// Compute a composite hash of all state hashes.
    ///
    /// This produces a single hash representing the aggregate state change.
    /// The hash is deterministic due to BTreeMap ordering.
    pub fn composite_hash(&self) -> String {
        if self.hashes.is_empty() {
            return String::new();
        }

        let mut hasher = Sha256::new();
        for (effect_id, hash) in &self.hashes {
            hasher.update(effect_id.as_bytes());
            hasher.update(b":");
            hasher.update(hash.as_bytes());
            hasher.update(b"\n");
        }
        format!("{:x}", hasher.finalize())
    }

    /// Compare with another StateHashSet.
    ///
    /// Returns true if all hashes match exactly.
    pub fn matches(&self, other: &StateHashSet) -> bool {
        self.hashes == other.hashes
    }

    /// Get differing hashes between two sets.
    ///
    /// Returns a list of (effect_id, self_hash, other_hash) for mismatches.
    pub fn diff(&self, other: &StateHashSet) -> Vec<(String, Option<String>, Option<String>)> {
        let mut diffs = Vec::new();

        // Check all keys in self
        for (effect_id, hash) in &self.hashes {
            match other.hashes.get(effect_id) {
                Some(other_hash) if other_hash == hash => {}
                Some(other_hash) => {
                    diffs.push((
                        effect_id.clone(),
                        Some(hash.clone()),
                        Some(other_hash.clone()),
                    ));
                }
                None => {
                    diffs.push((effect_id.clone(), Some(hash.clone()), None));
                }
            }
        }

        // Check keys only in other
        for (effect_id, hash) in &other.hashes {
            if !self.hashes.contains_key(effect_id) {
                diffs.push((effect_id.clone(), None, Some(hash.clone())));
            }
        }

        diffs
    }
}

impl From<Vec<EffectResult>> for StateHashSet {
    fn from(results: Vec<EffectResult>) -> Self {
        let mut set = StateHashSet::new();
        for result in results {
            if let Some(hash) = result.state_change_hash {
                if !hash.is_empty() {
                    set.insert(result.effect_id, hash);
                }
            }
        }
        set
    }
}

/// Extract state hashes from an ExecutionOutcome.
///
/// Parses the effects list for embedded state_hash markers in the format:
/// `-> state_hash=<hash>`
pub fn extract_state_hashes_from_outcome(outcome: &ExecutionOutcome) -> StateHashSet {
    match outcome {
        ExecutionOutcome::Success { effects, .. } => {
            let mut set = StateHashSet::new();
            for (i, effect) in effects.iter().enumerate() {
                if let Some(hash) = extract_state_hash_from_effect(effect) {
                    set.insert(format!("effect_{}", i), hash);
                }
            }
            set
        }
        ExecutionOutcome::Failed { .. } | ExecutionOutcome::Deferred { .. } => StateHashSet::new(),
    }
}

/// Extract state_change_hash from an effect string.
///
/// Looks for `state_hash=<hash>` pattern embedded in effect descriptions.
/// Hash values should be alphanumeric (typically hex).
fn extract_state_hash_from_effect(effect: &str) -> Option<String> {
    // Pattern: "state_hash=<hash>" or "-> state_hash=<hash>"
    if let Some(idx) = effect.find("state_hash=") {
        let start = idx + "state_hash=".len();
        let rest = &effect[start..];
        // Take until whitespace, comma, or end - allow alphanumeric chars
        let hash: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !hash.is_empty() {
            return Some(hash);
        }
    }
    None
}

/// Compare state hashes from two execution outcomes.
///
/// Returns true if both outcomes produced identical state changes.
pub fn verify_state_hash_match(outcome_a: &ExecutionOutcome, outcome_b: &ExecutionOutcome) -> bool {
    let set_a = extract_state_hashes_from_outcome(outcome_a);
    let set_b = extract_state_hashes_from_outcome(outcome_b);
    set_a.matches(&set_b)
}

/// Detailed comparison result between two execution outcomes.
#[derive(Debug, Clone)]
pub struct StateHashComparison {
    /// Whether all hashes match
    pub matches: bool,
    /// Composite hash from outcome A
    pub composite_a: String,
    /// Composite hash from outcome B
    pub composite_b: String,
    /// List of differing hashes
    pub differences: Vec<(String, Option<String>, Option<String>)>,
}

/// Compare state hashes with detailed results.
pub fn compare_state_hashes(
    outcome_a: &ExecutionOutcome,
    outcome_b: &ExecutionOutcome,
) -> StateHashComparison {
    let set_a = extract_state_hashes_from_outcome(outcome_a);
    let set_b = extract_state_hashes_from_outcome(outcome_b);

    StateHashComparison {
        matches: set_a.matches(&set_b),
        composite_a: set_a.composite_hash(),
        composite_b: set_b.composite_hash(),
        differences: set_a.diff(&set_b),
    }
}

/// Compare EffectResult vectors directly.
///
/// Useful when you have the raw effect results from service calls.
pub fn verify_effect_results_match(results_a: &[EffectResult], results_b: &[EffectResult]) -> bool {
    let set_a: StateHashSet = results_a.to_vec().into();
    let set_b: StateHashSet = results_b.to_vec().into();
    set_a.matches(&set_b)
}

/// Compare EffectResult vectors with detailed results.
pub fn compare_effect_results(
    results_a: &[EffectResult],
    results_b: &[EffectResult],
) -> StateHashComparison {
    let set_a: StateHashSet = results_a.to_vec().into();
    let set_b: StateHashSet = results_b.to_vec().into();

    StateHashComparison {
        matches: set_a.matches(&set_b),
        composite_a: set_a.composite_hash(),
        composite_b: set_b.composite_hash(),
        differences: set_a.diff(&set_b),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::governance::DecisionReceiptId;

    #[test]
    fn test_state_hash_set_empty() {
        let set = StateHashSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.composite_hash(), "");
    }

    #[test]
    fn test_state_hash_set_insert_and_len() {
        let mut set = StateHashSet::new();
        set.insert("effect_0".into(), "abc123".into());
        set.insert("effect_1".into(), "def456".into());
        assert!(!set.is_empty());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_state_hash_set_composite_deterministic() {
        let mut set1 = StateHashSet::new();
        set1.insert("effect_0".into(), "hash1".into());
        set1.insert("effect_1".into(), "hash2".into());

        let mut set2 = StateHashSet::new();
        // Insert in different order
        set2.insert("effect_1".into(), "hash2".into());
        set2.insert("effect_0".into(), "hash1".into());

        // BTreeMap ordering ensures deterministic hash
        assert_eq!(set1.composite_hash(), set2.composite_hash());
        assert!(!set1.composite_hash().is_empty());
    }

    #[test]
    fn test_state_hash_set_matches() {
        let mut set1 = StateHashSet::new();
        set1.insert("effect_0".into(), "abc123".into());

        let mut set2 = StateHashSet::new();
        set2.insert("effect_0".into(), "abc123".into());

        assert!(set1.matches(&set2));
    }

    #[test]
    fn test_state_hash_set_mismatch() {
        let mut set1 = StateHashSet::new();
        set1.insert("effect_0".into(), "abc123".into());

        let mut set2 = StateHashSet::new();
        set2.insert("effect_0".into(), "different".into());

        assert!(!set1.matches(&set2));
    }

    #[test]
    fn test_state_hash_set_diff() {
        let mut set1 = StateHashSet::new();
        set1.insert("effect_0".into(), "same".into());
        set1.insert("effect_1".into(), "different1".into());
        set1.insert("effect_2".into(), "only_in_a".into());

        let mut set2 = StateHashSet::new();
        set2.insert("effect_0".into(), "same".into());
        set2.insert("effect_1".into(), "different2".into());
        set2.insert("effect_3".into(), "only_in_b".into());

        let diff = set1.diff(&set2);
        assert_eq!(diff.len(), 3);

        // Check effect_1 mismatch
        assert!(diff.iter().any(|(id, a, b)| id == "effect_1"
            && a == &Some("different1".into())
            && b == &Some("different2".into())));

        // Check effect_2 only in A
        assert!(diff
            .iter()
            .any(|(id, a, b)| id == "effect_2" && a == &Some("only_in_a".into()) && b.is_none()));

        // Check effect_3 only in B
        assert!(diff
            .iter()
            .any(|(id, a, b)| id == "effect_3" && a.is_none() && b == &Some("only_in_b".into())));
    }

    #[test]
    fn test_extract_state_hash_from_effect() {
        // Basic format
        assert_eq!(
            extract_state_hash_from_effect("state_hash=abc123def"),
            Some("abc123def".into())
        );

        // With arrow prefix
        assert_eq!(
            extract_state_hash_from_effect("-> state_hash=fedcba987"),
            Some("fedcba987".into())
        );

        // With trailing content
        assert_eq!(
            extract_state_hash_from_effect("Executed -> state_hash=12345 successfully"),
            Some("12345".into())
        );

        // No state hash
        assert_eq!(
            extract_state_hash_from_effect("Just a regular effect"),
            None
        );
    }

    #[test]
    fn test_extract_state_hashes_from_outcome() {
        let outcome = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("test-receipt".into()),
            effects: vec![
                "Effect 1 -> state_hash=hash1".into(),
                "Effect 2 -> state_hash=hash2".into(),
                "Effect 3 with no hash".into(),
            ],
        };

        let set = extract_state_hashes_from_outcome(&outcome);
        assert_eq!(set.len(), 2);
        assert_eq!(set.hashes.get("effect_0"), Some(&"hash1".to_string()));
        assert_eq!(set.hashes.get("effect_1"), Some(&"hash2".to_string()));
    }

    #[test]
    fn test_extract_state_hashes_from_failed_outcome() {
        let outcome = ExecutionOutcome::Failed {
            receipt_id: DecisionReceiptId("test-receipt".into()),
            reason: "Some error".into(),
        };

        let set = extract_state_hashes_from_outcome(&outcome);
        assert!(set.is_empty());
    }

    #[test]
    fn test_verify_state_hash_match_identical() {
        let outcome_a = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("receipt-a".into()),
            effects: vec!["Effect -> state_hash=abc123".into()],
        };

        let outcome_b = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("receipt-b".into()), // Different receipt is OK
            effects: vec!["Effect -> state_hash=abc123".into()],
        };

        assert!(verify_state_hash_match(&outcome_a, &outcome_b));
    }

    #[test]
    fn test_verify_state_hash_match_different() {
        let outcome_a = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("receipt-a".into()),
            effects: vec!["Effect -> state_hash=abc123".into()],
        };

        let outcome_b = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("receipt-b".into()),
            effects: vec!["Effect -> state_hash=different".into()],
        };

        assert!(!verify_state_hash_match(&outcome_a, &outcome_b));
    }

    #[test]
    fn test_compare_state_hashes_detailed() {
        let outcome_a = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("receipt-a".into()),
            effects: vec![
                "Effect 1 -> state_hash=hash1".into(),
                "Effect 2 -> state_hash=hash2".into(),
            ],
        };

        let outcome_b = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId("receipt-b".into()),
            effects: vec![
                "Effect 1 -> state_hash=hash1".into(),
                "Effect 2 -> state_hash=DIFFERENT".into(),
            ],
        };

        let comparison = compare_state_hashes(&outcome_a, &outcome_b);
        assert!(!comparison.matches);
        assert_ne!(comparison.composite_a, comparison.composite_b);
        assert_eq!(comparison.differences.len(), 1);
        assert_eq!(comparison.differences[0].0, "effect_1");
    }

    #[test]
    fn test_from_effect_results() {
        let results = vec![
            EffectResult {
                effect_id: "treasury_0".into(),
                success: true,
                message: "OK".into(),
                state_change_hash: Some("hash_t".into()),
                ledger_entry_id: None,
            },
            EffectResult {
                effect_id: "membership_0".into(),
                success: true,
                message: "OK".into(),
                state_change_hash: Some("hash_m".into()),
                ledger_entry_id: None,
            },
            EffectResult {
                effect_id: "no_hash".into(),
                success: true,
                message: "OK".into(),
                state_change_hash: None,
                ledger_entry_id: None,
            },
        ];

        let set: StateHashSet = results.into();
        assert_eq!(set.len(), 2);
        assert_eq!(set.hashes.get("treasury_0"), Some(&"hash_t".to_string()));
        assert_eq!(set.hashes.get("membership_0"), Some(&"hash_m".to_string()));
    }

    #[test]
    fn test_verify_effect_results_match() {
        let results_a = vec![EffectResult {
            effect_id: "effect_0".into(),
            success: true,
            message: "OK".into(),
            state_change_hash: Some("same_hash".into()),
            ledger_entry_id: None,
        }];

        let results_b = vec![EffectResult {
            effect_id: "effect_0".into(),
            success: true,
            message: "Different message is OK".into(),
            state_change_hash: Some("same_hash".into()),
            ledger_entry_id: None,
        }];

        assert!(verify_effect_results_match(&results_a, &results_b));
    }

    #[test]
    fn test_compare_effect_results_mismatch() {
        let results_a = vec![EffectResult {
            effect_id: "effect_0".into(),
            success: true,
            message: "OK".into(),
            state_change_hash: Some("hash_a".into()),
            ledger_entry_id: None,
        }];

        let results_b = vec![EffectResult {
            effect_id: "effect_0".into(),
            success: true,
            message: "OK".into(),
            state_change_hash: Some("hash_b".into()),
            ledger_entry_id: None,
        }];

        let comparison = compare_effect_results(&results_a, &results_b);
        assert!(!comparison.matches);
        assert_eq!(comparison.differences.len(), 1);
    }
}
