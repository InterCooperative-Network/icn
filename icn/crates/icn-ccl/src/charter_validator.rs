//! Charter validator - Integrates charter rules with ledger validation
//!
//! This module provides a wrapper around CharterRuleSet that can be used
//! as a validation hook in the ledger. It uses simple rule evaluation
//! without requiring full CCL runtime integration.

use crate::charter_rules::{CharterRule, CharterRuleSet, ValidationResult};
use anyhow::Result;
use icn_ledger::JournalEntry;
use std::collections::HashMap;
use std::sync::Arc;

/// Validator for charter-based transaction rules
pub struct CharterValidator {
    /// Charter rules to enforce
    rules: CharterRuleSet,
    /// Domain ID for this validator
    _domain_id: String,
}

impl CharterValidator {
    /// Create a new charter validator
    pub fn new(domain_id: String, rules: CharterRuleSet) -> Self {
        Self {
            _domain_id: domain_id,
            rules,
        }
    }

    /// Create a default validator for cooperatives
    pub fn cooperative_default(domain_id: String, min_trust_bp: i64) -> Self {
        Self {
            _domain_id: domain_id,
            rules: CharterRuleSet::cooperative_default(min_trust_bp),
        }
    }

    /// Validate a journal entry against charter rules
    ///
    /// Returns Ok(()) if all rules pass, Err with reason if any rule fails.
    /// This method is designed to be used as a ledger validation hook.
    pub fn validate_entry(&self, entry: &JournalEntry) -> Result<()> {
        let results = self.validate_entry_detailed(entry)?;

        if Self::has_failures(&results) {
            let failures = Self::get_failure_reasons(&results);
            let reason = failures.join("; ");
            anyhow::bail!("Charter validation failed: {reason}");
        }

        Ok(())
    }

    /// Validate and return detailed results for each rule
    ///
    /// Note: This is a simplified implementation that validates structural
    /// properties. Full CCL evaluation would require interpreter integration.
    pub fn validate_entry_detailed(&self, entry: &JournalEntry) -> Result<Vec<ValidationResult>> {
        let mut results = Vec::new();

        // Extract transaction summary for validation
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

        // Validate against transaction rules
        // For now, we perform basic structural validation
        // Full CCL expression evaluation can be added when needed

        for rule in &self.rules.transaction_rules {
            let result = self.evaluate_rule_basic(rule, entry, &total_debits, &total_credits);
            results.push(result);
        }

        Ok(results)
    }

    /// Basic rule evaluation without full CCL runtime
    ///
    /// This validates common charter rule patterns:
    /// - Credit limit checks
    /// - Transaction amount limits
    /// - Account eligibility
    fn evaluate_rule_basic(
        &self,
        rule: &CharterRule,
        _entry: &JournalEntry,
        _total_debits: &HashMap<String, i64>,
        _total_credits: &HashMap<String, i64>,
    ) -> ValidationResult {
        // For now, we pass all rules (optimistic validation)
        // In production, you would:
        // 1. Parse the rule expression AST
        // 2. Check against ledger state
        // 3. Evaluate conditions

        // This is a hook point for future full CCL integration
        ValidationResult::pass(rule.name())
    }

    /// Check if any validation result failed
    pub fn has_failures(results: &[ValidationResult]) -> bool {
        results.iter().any(|r| !r.passed)
    }

    /// Get failure reasons from validation results
    pub fn get_failure_reasons(results: &[ValidationResult]) -> Vec<String> {
        results
            .iter()
            .filter(|r| !r.passed)
            .filter_map(|r| r.reason.clone())
            .collect()
    }

    /// Get the charter rule set
    pub fn rules(&self) -> &CharterRuleSet {
        &self.rules
    }

    /// Update transaction rules
    pub fn set_transaction_rules(&mut self, rules: Vec<CharterRule>) {
        self.rules.transaction_rules = rules;
    }

    /// Add a transaction rule
    pub fn add_transaction_rule(&mut self, rule: CharterRule) {
        self.rules.add_transaction_rule(rule);
    }

    /// Create a validation hook closure for use with ledger
    ///
    /// Returns a closure that can be passed to `ledger.set_validation_hook()`
    pub fn create_hook(self: Arc<Self>) -> impl Fn(&JournalEntry) -> Result<()> + Send + Sync {
        move |entry: &JournalEntry| self.validate_entry(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;
    use icn_ledger::{entry::JournalEntryBuilder, JournalEntry};

    fn create_test_entry(author: icn_identity::Did, debit: i64, credit: i64) -> JournalEntry {
        let receiver = IdentityBundle::generate().unwrap().did().clone();

        JournalEntryBuilder::new(author.clone())
            .debit(author, "hours".to_string(), debit)
            .credit(receiver, "hours".to_string(), credit)
            .build()
            .unwrap()
    }

    #[test]
    fn test_charter_validator_creation() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);
        assert_eq!(validator._domain_id, "test-coop");
        assert!(!validator.rules.transaction_rules.is_empty());
    }

    #[test]
    fn test_validate_entry_basic() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

        let author = IdentityBundle::generate().unwrap().did().clone();
        let entry = create_test_entry(author, 10, 10);

        let results = validator.validate_entry_detailed(&entry);
        assert!(results.is_ok());
    }

    #[test]
    fn test_validate_entry_passes() {
        let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

        let author = IdentityBundle::generate().unwrap().did().clone();
        let entry = create_test_entry(author, 10, 10);

        // Should pass (optimistic validation)
        let result = validator.validate_entry(&entry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_has_failures() {
        let pass = ValidationResult::pass("test");
        let fail = ValidationResult::fail("test", "failed");

        assert!(!CharterValidator::has_failures(&[pass.clone()]));
        assert!(CharterValidator::has_failures(&[fail.clone()]));
        assert!(CharterValidator::has_failures(&[pass, fail]));
    }

    #[test]
    fn test_get_failure_reasons() {
        let pass = ValidationResult::pass("test1");
        let fail1 = ValidationResult::fail("test2", "reason1");
        let fail2 = ValidationResult::fail("test3", "reason2");

        let reasons = CharterValidator::get_failure_reasons(&[pass, fail1, fail2]);
        assert_eq!(reasons.len(), 2);
        assert!(reasons.contains(&"reason1".to_string()));
        assert!(reasons.contains(&"reason2".to_string()));
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

        // Hook should be callable
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
