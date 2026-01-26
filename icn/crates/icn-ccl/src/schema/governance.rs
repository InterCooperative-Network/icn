//! Governance schema definitions.
//!
//! Defines governance bodies, decision types, voting rules, and delegation.

use super::SchemaError;
use serde::{Deserialize, Serialize};

/// Convening schedule for a body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConveneSchedule {
    /// Regular meeting frequency
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regular: Option<MeetingFrequency>,

    /// Special meeting trigger
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub special: Option<SpecialMeetingTrigger>,
}

/// Meeting frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingFrequency {
    Weekly,
    Biweekly,
    Monthly,
    Quarterly,
    Annually,
}

/// Trigger for special meetings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialMeetingTrigger {
    /// What triggers the special meeting
    pub trigger: TriggerType,

    /// Threshold expression (e.g., "0.10 * members")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<String>,
}

/// Types of triggers for special meetings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Member petition
    Petition,
    /// Board request
    BoardRequest,
    /// Emergency
    Emergency,
}

/// Term duration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermDuration {
    /// Years in term
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub years: Option<u32>,

    /// Months in term
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub months: Option<u32>,

    /// Whether terms are staggered
    #[serde(default)]
    pub staggered: bool,
}

/// Recall procedure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallProcedure {
    /// Petition threshold (e.g., "0.25 * members")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub petition: Option<String>,

    /// Vote threshold
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vote: Option<VoteThreshold>,
}

/// A governance body (e.g., general assembly, board).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceBody {
    /// Body name
    pub name: String,

    /// Composition (who is in this body)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<String>,

    /// Number of seats (for elected bodies)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<u32>,

    /// Who elects this body
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elected_by: Option<String>,

    /// Term duration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<TermDuration>,

    /// Recall procedure
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recall: Option<RecallProcedure>,

    /// Convening schedule
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convenes: Option<ConveneSchedule>,
}

/// Voting threshold types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VoteThreshold {
    /// Named threshold
    Named(ThresholdName),
    /// Fraction (e.g., { fraction: "2/3" })
    Fraction { fraction: String },
    /// Expression (e.g., "0.67 * votes")
    Expression(String),
}

/// Named voting thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdName {
    SimpleMajority,
    Supermajority,
    Unanimous,
    Consensus,
}

/// Duration in various units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Duration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<u32>,
}

/// Ratification requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatificationRequirement {
    /// Body that must ratify
    pub must_ratify: String,

    /// Time limit for ratification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<Duration>,
}

/// A decision type (e.g., operational, policy, constitutional).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionType {
    /// Decision type name
    pub name: String,

    /// Which body has authority
    pub authority: String,

    /// Vote threshold
    pub threshold: VoteThreshold,

    /// Quorum requirement (expression like "0.5 * members")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quorum: Option<String>,

    /// Ratification period
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratification_period: Option<Duration>,

    /// Constraint (must be ratified by another body)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<RatificationRequirement>,
}

/// Delegation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationConfig {
    /// Whether delegation is allowed
    #[serde(default)]
    pub allowed: bool,

    /// Whether delegation is transitive
    #[serde(default)]
    pub transitive: bool,

    /// How quickly delegation can be revoked
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocable: Option<RevocationType>,

    /// What can be delegated
    #[serde(default)]
    pub scope: Vec<DelegationScope>,
}

/// Types of revocation speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationType {
    Instant,
    EndOfSession,
    EndOfTerm,
}

/// What can be delegated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationScope {
    Vote,
    Propose,
    Speak,
    Candidacy,
}

/// Complete governance schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceSchema {
    /// Governance bodies
    #[serde(default)]
    pub bodies: Vec<GovernanceBody>,

    /// Decision types
    #[serde(default)]
    pub decisions: Vec<DecisionType>,

    /// Delegation configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<DelegationConfig>,
}

impl GovernanceSchema {
    /// Validate the governance schema.
    pub fn validate(&self) -> Result<(), SchemaError> {
        let body_names: Vec<_> = self.bodies.iter().map(|b| &b.name).collect();

        // Validate decision authority references
        for decision in &self.decisions {
            if !body_names.contains(&&decision.authority) {
                return Err(SchemaError::Reference(format!(
                    "Decision '{}' references unknown body: {}",
                    decision.name, decision.authority
                )));
            }

            // Validate constraint references
            if let Some(constraint) = &decision.constraint {
                if !body_names.contains(&&constraint.must_ratify) {
                    return Err(SchemaError::Reference(format!(
                        "Decision '{}' constraint references unknown body: {}",
                        decision.name, constraint.must_ratify
                    )));
                }
            }
        }

        // Validate elected_by references
        for body in &self.bodies {
            if let Some(elected_by) = &body.elected_by {
                if !body_names.contains(&elected_by) {
                    return Err(SchemaError::Reference(format!(
                        "Body '{}' elected_by references unknown body: {}",
                        body.name, elected_by
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_governance() {
        let yaml = r#"
bodies:
  - name: general_assembly
    composition: all_members
    convenes:
      regular: quarterly
      special:
        trigger: petition
        threshold: "0.10 * members"

  - name: board
    seats: 7
    elected_by: general_assembly
    term:
      years: 2
      staggered: true
    recall:
      petition: "0.25 * members"
      vote: simple_majority

decisions:
  - name: operational
    authority: board
    threshold: simple_majority
    quorum: "0.5 * board.seats"

  - name: policy
    authority: general_assembly
    threshold: simple_majority
    quorum: "0.25 * members"

  - name: constitutional
    authority: general_assembly
    threshold:
      fraction: "2/3"
    quorum: "0.5 * members"
    ratification_period:
      days: 30

delegation:
  allowed: true
  transitive: false
  revocable: instant
  scope: [vote, propose]
"#;
        let gov: GovernanceSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(gov.bodies.len(), 2);
        assert_eq!(gov.decisions.len(), 3);
        gov.validate().expect("Validation failed");
    }

    #[test]
    fn test_validate_unknown_authority() {
        let yaml = r#"
bodies:
  - name: board
decisions:
  - name: operational
    authority: nonexistent_body
    threshold: simple_majority
"#;
        let gov: GovernanceSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(gov.validate().is_err());
    }

    #[test]
    fn test_emergency_decision() {
        let yaml = r#"
bodies:
  - name: board
  - name: general_assembly

decisions:
  - name: emergency
    authority: board
    threshold: unanimous
    constraint:
      must_ratify: general_assembly
      within:
        days: 60
"#;
        let gov: GovernanceSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        gov.validate().expect("Validation failed");

        let emergency = &gov.decisions[0];
        assert!(emergency.constraint.is_some());
    }
}
