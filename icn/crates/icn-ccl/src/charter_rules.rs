//! Charter rule enforcement via CCL

use crate::ast::{BinOp, Expr};
use crate::types::Value;
use serde::{Deserialize, Serialize};

/// Charter rule that can be evaluated via CCL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CharterRule {
    MembershipEligibility(Expr),
    TransactionLimit(Expr),
    ProposalEligibility(Expr),
    VotingEligibility(Expr),
    Custom { name: String, rule: Expr },
}

impl CharterRule {
    pub fn name(&self) -> &str {
        match self {
            CharterRule::MembershipEligibility(_) => "membership_eligibility",
            CharterRule::TransactionLimit(_) => "transaction_limit",
            CharterRule::ProposalEligibility(_) => "proposal_eligibility",
            CharterRule::VotingEligibility(_) => "voting_eligibility",
            CharterRule::Custom { name, .. } => name,
        }
    }

    pub fn expr(&self) -> &Expr {
        match self {
            CharterRule::MembershipEligibility(e) => e,
            CharterRule::TransactionLimit(e) => e,
            CharterRule::ProposalEligibility(e) => e,
            CharterRule::VotingEligibility(e) => e,
            CharterRule::Custom { rule, .. } => rule,
        }
    }

    pub fn membership_trust_threshold(min_trust_bp: i64) -> Self {
        CharterRule::MembershipEligibility(Expr::BinOp {
            op: BinOp::Ge,
            left: Box::new(Expr::Var("trust_score".to_string())),
            right: Box::new(Expr::Literal(Value::Int(min_trust_bp))),
        })
    }

    pub fn transaction_credit_limit() -> Self {
        CharterRule::TransactionLimit(Expr::BinOp {
            op: BinOp::Le,
            left: Box::new(Expr::Var("amount".to_string())),
            right: Box::new(Expr::Call {
                name: "credit_limit".to_string(),
                args: vec![Expr::Var("sender".to_string())],
            }),
        })
    }

    pub fn proposal_min_membership_days(days: i64) -> Self {
        CharterRule::ProposalEligibility(Expr::BinOp {
            op: BinOp::Ge,
            left: Box::new(Expr::Var("membership_duration_days".to_string())),
            right: Box::new(Expr::Literal(Value::Int(days))),
        })
    }

    pub fn voting_active_members_only() -> Self {
        CharterRule::VotingEligibility(Expr::BinOp {
            op: BinOp::And,
            left: Box::new(Expr::Var("is_active_member".to_string())),
            right: Box::new(Expr::Var("in_good_standing".to_string())),
        })
    }
}

/// Charter rule validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub rule_name: String,
    pub reason: Option<String>,
}

impl ValidationResult {
    pub fn pass(rule_name: impl Into<String>) -> Self {
        Self {
            passed: true,
            rule_name: rule_name.into(),
            reason: None,
        }
    }

    pub fn fail(rule_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            passed: false,
            rule_name: rule_name.into(),
            reason: Some(reason.into()),
        }
    }
}

/// Charter rule set for a jurisdiction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharterRuleSet {
    pub membership_rules: Vec<CharterRule>,
    pub transaction_rules: Vec<CharterRule>,
    pub governance_rules: Vec<CharterRule>,
}

impl CharterRuleSet {
    pub fn new() -> Self {
        Self {
            membership_rules: Vec::new(),
            transaction_rules: Vec::new(),
            governance_rules: Vec::new(),
        }
    }

    pub fn cooperative_default(min_trust_bp: i64) -> Self {
        Self {
            membership_rules: vec![CharterRule::membership_trust_threshold(min_trust_bp)],
            transaction_rules: vec![CharterRule::transaction_credit_limit()],
            governance_rules: vec![
                CharterRule::proposal_min_membership_days(30),
                CharterRule::voting_active_members_only(),
            ],
        }
    }

    pub fn add_membership_rule(&mut self, rule: CharterRule) {
        self.membership_rules.push(rule);
    }

    pub fn add_transaction_rule(&mut self, rule: CharterRule) {
        self.transaction_rules.push(rule);
    }

    pub fn add_governance_rule(&mut self, rule: CharterRule) {
        self.governance_rules.push(rule);
    }
}

impl Default for CharterRuleSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_trust_threshold() {
        let rule = CharterRule::membership_trust_threshold(500);
        assert_eq!(rule.name(), "membership_eligibility");
    }

    #[test]
    fn test_charter_rule_set_creation() {
        let ruleset = CharterRuleSet::cooperative_default(500);
        assert_eq!(ruleset.membership_rules.len(), 1);
        assert_eq!(ruleset.transaction_rules.len(), 1);
        assert_eq!(ruleset.governance_rules.len(), 2);
    }

    #[test]
    fn test_validation_result() {
        let pass = ValidationResult::pass("test_rule");
        assert!(pass.passed);
        let fail = ValidationResult::fail("test_rule", "failed");
        assert!(!fail.passed);
    }
}
