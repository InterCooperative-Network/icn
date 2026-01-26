//! Economics schema definitions.
//!
//! Defines capital structure, surplus allocation, and credit policies.

use super::SchemaError;
use serde::{Deserialize, Serialize};

/// Member equity configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberEquity {
    /// Minimum equity contribution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i64>,

    /// Maximum equity contribution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<i64>,

    /// Interest rate on equity (0 = no interest)
    #[serde(default)]
    pub interest_rate: f64,

    /// Refund policy on exit
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refund_on_exit: Option<RefundPolicy>,
}

/// Refund policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundPolicy {
    /// Whether full refund is given
    #[serde(default)]
    pub full: bool,

    /// Time limit for refund
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<super::governance::Duration>,
}

/// Capital configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapitalConfig {
    /// Member equity rules
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_equity: Option<MemberEquity>,
}

/// A single surplus allocation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationRule {
    /// Target for allocation (reserves, patronage_refund, etc.)
    pub target: AllocationTarget,

    /// Fraction expression (e.g., "0.20")
    pub fraction: String,

    /// Condition for this allocation to apply
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,

    /// How to distribute (for patronage)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proportional_to: Option<String>,

    /// Condition for this allocation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// Allocation targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationTarget {
    Reserves,
    PatronageRefund,
    CommunityFund,
    WorkerBonus,
    EducationFund,
    IndivisibleReserves,
    Custom(String),
}

/// Surplus allocation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurplusConfig {
    /// Allocation rules (applied in order)
    #[serde(default)]
    pub allocation: Vec<AllocationRule>,
}

/// Credit eligibility and terms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditConfig {
    /// Eligibility expression
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<CreditEligibility>,

    /// Credit limit expression
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,

    /// Payment terms
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<PaymentTerms>,
}

/// Credit eligibility condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditEligibility {
    /// Field to check
    pub field: String,

    /// Comparison operator
    pub op: super::entity::ComparisonOp,

    /// Value to compare against
    pub value: serde_yaml::Value,
}

/// Payment terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentTerms {
    Net7,
    Net15,
    Net30,
    Net60,
    Net90,
}

/// Complete economics schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsSchema {
    /// Capital configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capital: Option<CapitalConfig>,

    /// Surplus allocation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surplus: Option<SurplusConfig>,

    /// Credit configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit: Option<CreditConfig>,
}

impl EconomicsSchema {
    /// Validate the economics schema.
    pub fn validate(&self) -> Result<(), SchemaError> {
        // Validate surplus allocation fractions sum to <= 1.0
        if let Some(surplus) = &self.surplus {
            let mut total = 0.0;
            for rule in &surplus.allocation {
                // Parse fraction - for now, simple float parsing
                // In production, would parse expressions properly
                let f = rule.fraction.parse::<f64>().map_err(|e| {
                    SchemaError::Validation(format!(
                        "Invalid surplus allocation fraction '{}': {}",
                        rule.fraction, e
                    ))
                })?;
                total += f;
            }
            if total > 1.0 + f64::EPSILON {
                return Err(SchemaError::Validation(format!(
                    "Surplus allocation fractions sum to {} (> 1.0)",
                    total
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_economics() {
        let yaml = r#"
capital:
  member_equity:
    minimum: 100
    maximum: 10000
    interest_rate: 0
    refund_on_exit:
      full: true
      within:
        days: 90

surplus:
  allocation:
    - target: reserves
      fraction: "0.20"
      until: "reserves >= 6 * monthly_operating"
    - target: patronage_refund
      fraction: "0.60"
      proportional_to: purchases
    - target: community_fund
      fraction: "0.10"
    - target: worker_bonus
      fraction: "0.10"
      condition: "worker_owners_exist"

credit:
  eligibility:
    field: membership_months
    op: ">="
    value: 6
  limit: "min(1000, patronage_history * 0.5 * trust_score)"
  terms: net30
"#;
        let econ: EconomicsSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(econ.capital.is_some());
        assert!(econ.surplus.is_some());
        assert!(econ.credit.is_some());
        econ.validate().expect("Validation failed");
    }

    #[test]
    fn test_validate_excess_allocation() {
        let yaml = r#"
surplus:
  allocation:
    - target: reserves
      fraction: "0.60"
    - target: patronage_refund
      fraction: "0.60"
"#;
        let econ: EconomicsSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(econ.validate().is_err());
    }

    #[test]
    fn test_member_equity() {
        let yaml = r#"
capital:
  member_equity:
    minimum: 50
    maximum: 500
"#;
        let econ: EconomicsSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        let equity = econ.capital.unwrap().member_equity.unwrap();
        assert_eq!(equity.minimum, Some(50));
        assert_eq!(equity.maximum, Some(500));
    }
}
