//! Charter validator — Integrates charter rules with ledger validation
//!
//! This module provides a wrapper around CharterRuleSet that can be used
//! as a validation hook in the ledger.
//!
//! # Honesty contract
//!
//! `evaluate_rule_basic()` returns `ValidationResult::deferred()` for all
//! rule kinds.  This is intentional: full CCL interpreter integration is not
//! yet wired here, and the validator MUST NOT silently claim rules passed when
//! they have not been evaluated.
//!
//! Deferred ≠ Pass.  `validate_entry()` warns on deferred rules but does not
//! fail — the system is permissive about rules it cannot yet enforce.
//! `validate_entry_detailed()` surfaces every result so callers can audit them.
//!
//! When full CCL evaluation is wired, replace `deferred()` returns with actual
//! interpreter calls and update this comment.

use crate::charter_rules::{CharterRule, CharterRuleSet, ValidationResult};
use anyhow::Result;
use icn_ledger::JournalEntry;
use std::collections::HashMap;
use std::sync::Arc;

/// Validator for charter-based transaction rules.
pub struct CharterValidator {
    /// Charter rules to enforce
    rules: CharterRuleSet,
    /// Domain ID for this validator
    _domain_id: String,
}

impl CharterValidator {
    /// Create a new charter validator.
    pub fn new(domain_id: String, rules: CharterRuleSet) -> Self {
        Self {
            _domain_id: domain_id,
            rules,
        }
    }

    /// Create a default validator for cooperatives.
    pub fn cooperative_default(domain_id: String, min_trust_bp: i64) -> Self {
        Self {
            _domain_id: domain_id,
            rules: CharterRuleSet::cooperative_default(min_trust_bp),
        }
    }

    /// Validate a journal entry against charter rules.
    ///
    /// Returns `Ok(())` when all evaluated rules pass.
    ///
    /// Deferred rules are warned but do NOT cause failure — the system is
    /// permissive about rules it cannot yet enforce.  Deferred is not silence:
    /// a `tracing::warn!` is emitted listing every deferred rule name.
    ///
    /// Returns `Err` only when a rule is explicitly evaluated and FAILS.
    pub fn validate_entry(&self, entry: &JournalEntry) -> Result<()> {
        let results = self.validate_entry_detailed(entry)?;

        // Surface deferred rules as warnings — they are unenforced, not approved.
        let deferred: Vec<&str> = results
            .iter()
            .filter(|r| r.is_deferred())
            .map(|r| r.rule_name.as_str())
            .collect();
        if !deferred.is_empty() {
            tracing::warn!(
                rules = ?deferred,
                "Charter rules deferred (not enforced): full CCL interpreter not wired. \
                 These rules are unenforced — they did not pass evaluation."
            );
        }

        if Self::has_failures(&results) {
            let failures = Self::get_failure_reasons(&results);
            let reason = failures.join("; ");
            anyhow::bail!("Charter validation failed: {reason}");
        }

        Ok(())
    }

    /// Validate and return detailed results for each transaction rule.
    ///
    /// Results carry `RuleStatus::Deferred` for all rules until full CCL
    /// interpreter evaluation is wired.  Callers should inspect `is_deferred()`
    /// to distinguish "not evaluated" from "evaluated and passed".
    pub fn validate_entry_detailed(&self, entry: &JournalEntry) -> Result<Vec<ValidationResult>> {
        let mut results = Vec::new();

        // Extract transaction summary for future full evaluation.
        let mut total_debits: HashMap<String, i64> = HashMap::new();
        let mut total_credits: HashMap<String, i64> = HashMap::new();

        for delta in &entry.accounts {
            let currency = &delta.currency;
            if let Some(debit) = delta.debit {
                *total_debits.entry(currency.clone()).or_insert(0) += debit;
            }
            if let Some(credit) = delta.credit {
                *total_credits.entry(currency.clone()).or_insert(0) += credit;
            }
        }

        for rule in &self.rules.transaction_rules {
            let result = self.evaluate_rule_basic(rule, entry, &total_debits, &total_credits);
            results.push(result);
        }

        Ok(results)
    }

    /// Evaluate a single charter rule against a transaction.
    ///
    /// # STUB — all rules return Deferred
    ///
    /// Full CCL interpreter evaluation is not yet wired to this validator.
    /// Returns `ValidationResult::deferred()` for every rule kind.
    ///
    /// This is intentionally honest: the rule is not enforced, but it is not
    /// silently approved either.  When this function returns `deferred`, the
    /// rule has been SKIPPED — the transaction should not be considered as
    /// having satisfied the rule.
    ///
    /// To close this gap, wire `crate::interpreter::Interpreter` here and
    /// evaluate `rule.expr()` against a context built from `entry` data.
    fn evaluate_rule_basic(
        &self,
        rule: &CharterRule,
        _entry: &JournalEntry,
        _total_debits: &HashMap<String, i64>,
        _total_credits: &HashMap<String, i64>,
    ) -> ValidationResult {
        // STUB: all rule kinds deferred until CCL interpreter is wired.
        // Do NOT change this to ValidationResult::pass() — that would claim
        // rules evaluated when they have not been.
        ValidationResult::deferred(rule.name())
    }

    /// True if any result is an explicit `Fail` (not deferred).
    ///
    /// Deferred results are NOT counted as failures — they are unenforced
    /// but permissive.
    pub fn has_failures(results: &[ValidationResult]) -> bool {
        results.iter().any(|r| r.failed())
    }

    /// True if any result is `Deferred` (rule was skipped, not evaluated).
    ///
    /// Use this to surface unenforced rules in audit trails.
    pub fn has_deferred(results: &[ValidationResult]) -> bool {
        results.iter().any(|r| r.is_deferred())
    }

    /// Get failure reasons (excludes deferred).
    pub fn get_failure_reasons(results: &[ValidationResult]) -> Vec<String> {
        results
            .iter()
            .filter(|r| r.failed())
            .filter_map(|r| r.reason.clone())
            .collect()
    }

    /// Get names of deferred (unenforced) rules.
    pub fn get_deferred_names(results: &[ValidationResult]) -> Vec<String> {
        results
            .iter()
            .filter(|r| r.is_deferred())
            .map(|r| r.rule_name.clone())
            .collect()
    }

    /// Get the charter rule set.
    pub fn rules(&self) -> &CharterRuleSet {
        &self.rules
    }

    /// Update transaction rules.
    pub fn set_transaction_rules(&mut self, rules: Vec<CharterRule>) {
        self.rules.transaction_rules = rules;
    }

    /// Add a transaction rule.
    pub fn add_transaction_rule(&mut self, rule: CharterRule) {
        self.rules.add_transaction_rule(rule);
    }

    /// Create a validation hook closure for use with ledger.
    pub fn create_hook(self: Arc<Self>) -> impl Fn(&JournalEntry) -> Result<()> + Send + Sync {
        move |entry: &JournalEntry| self.validate_entry(entry)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;
    use icn_ledger::{entry::JournalEntryBuilder, JournalEntry};

    fn create_test_entry(author: icn_identity::Did, debit: i64, credit: i64) -> JournalEntry {
        let receiver = IdentityBundle::generate().unwrap().did().clone();

        JournalEntryBuilder::new(author.clone())
            .debit(author, "hours".to_string(), debit)
            .credit(receiver, "hours".to_string(), credit)
            .with_system_provenance("test")
            .build()
            .unwrap()
    }

    #[test]
    fn test_charter_validator_creation() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);
        assert_eq!(validator._domain_id, "test-coop");
        assert!(!validator.rules.transaction_rules.is_empty());
    }

    /// Rules are deferred, not passed — validate_entry_detailed surfaces deferred results.
    #[test]
    fn test_validate_entry_detailed_returns_deferred_not_pass() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

        let author = IdentityBundle::generate().unwrap().did().clone();
        let entry = create_test_entry(author, 10, 10);

        let results = validator.validate_entry_detailed(&entry).unwrap();
        assert!(
            !results.is_empty(),
            "cooperative_default has transaction rules"
        );

        // All results must be Deferred, not Pass — the stub is honest.
        for r in &results {
            assert!(
                r.is_deferred(),
                "Rule '{}' should be Deferred, got {:?}",
                r.rule_name,
                r.status
            );
            assert!(!r.passed(), "Deferred must not be treated as passed");
        }
    }

    /// validate_entry succeeds (permissive) even with deferred rules.
    /// Deferred = unenforced, not failed.
    #[test]
    fn test_validate_entry_is_permissive_for_deferred_rules() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

        let author = IdentityBundle::generate().unwrap().did().clone();
        let entry = create_test_entry(author, 10, 10);

        // Must succeed — deferred rules are permissive, not blocking.
        let result = validator.validate_entry(&entry);
        assert!(
            result.is_ok(),
            "Deferred rules must not cause validate_entry to fail"
        );
    }

    /// has_failures returns false for deferred results.
    #[test]
    fn test_has_failures_excludes_deferred() {
        let deferred = ValidationResult::deferred("rule_a");
        let pass = ValidationResult::pass("rule_b");

        // Deferred is not a failure.
        assert!(!CharterValidator::has_failures(std::slice::from_ref(
            &deferred
        )));
        assert!(!CharterValidator::has_failures(std::slice::from_ref(&pass)));
        assert!(!CharterValidator::has_failures(&[deferred, pass]));
    }

    #[test]
    fn test_has_failures_detects_explicit_fail() {
        let fail = ValidationResult::fail("rule_a", "limit exceeded");
        assert!(CharterValidator::has_failures(std::slice::from_ref(&fail)));
    }

    /// has_deferred correctly identifies unenforced rules.
    #[test]
    fn test_has_deferred() {
        let deferred = ValidationResult::deferred("rule_a");
        let pass = ValidationResult::pass("rule_b");
        let fail = ValidationResult::fail("rule_c", "reason");

        assert!(CharterValidator::has_deferred(std::slice::from_ref(
            &deferred
        )));
        assert!(!CharterValidator::has_deferred(std::slice::from_ref(&pass)));
        assert!(!CharterValidator::has_deferred(std::slice::from_ref(&fail)));
    }

    /// get_deferred_names surfaces rule names for audit trails.
    #[test]
    fn test_get_deferred_names() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);
        let author = IdentityBundle::generate().unwrap().did().clone();
        let entry = create_test_entry(author, 10, 10);
        let results = validator.validate_entry_detailed(&entry).unwrap();

        let names = CharterValidator::get_deferred_names(&results);
        assert!(
            !names.is_empty(),
            "Should report deferred rule names for audit"
        );
        // All deferred names should come from the cooperative_default transaction rules.
        assert!(names.iter().any(|n| n == "transaction_limit"));
    }

    #[test]
    fn test_get_failure_reasons_excludes_deferred() {
        let deferred = ValidationResult::deferred("rule_a");
        let fail = ValidationResult::fail("rule_b", "reason1");

        let reasons = CharterValidator::get_failure_reasons(&[deferred, fail]);
        // Only explicit failures, not deferred.
        assert_eq!(reasons, vec!["reason1".to_string()]);
    }

    #[test]
    fn test_create_hook() {
        let validator = Arc::new(CharterValidator::cooperative_default(
            "test-coop".to_string(),
            500,
        ));
        let hook = validator.create_hook();

        let author = IdentityBundle::generate().unwrap().did().clone();
        let entry = create_test_entry(author, 10, 10);

        // Hook must succeed even with deferred rules (permissive).
        let result = hook(&entry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_transaction_rule() {
        let mut validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);
        let initial_count = validator.rules.transaction_rules.len();
        validator.add_transaction_rule(CharterRule::transaction_credit_limit());
        assert_eq!(validator.rules.transaction_rules.len(), initial_count + 1);
    }
}
