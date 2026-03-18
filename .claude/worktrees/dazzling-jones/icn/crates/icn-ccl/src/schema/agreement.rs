//! Federation agreement schema definitions.
//!
//! Defines agreements between federations, including boundary protocols,
//! credential recognition, and dispute resolution.
//!
//! # Key Insight
//!
//! Federation A uses consensus internally. Federation B uses majority vote.
//! The agreement doesn't care. It only asks:
//! "Did Federation A approve? Did Federation B approve?"
//!
//! Internal process is sovereign. Boundary outcomes are interoperable.
//! This is how pluralism works: agree on interfaces, not implementations.

use super::SchemaError;
use serde::{Deserialize, Serialize};

/// Party to an agreement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    /// Party type
    #[serde(rename = "type")]
    pub party_type: PartyType,

    /// Party identifier (DID)
    pub id: String,
}

/// Types of parties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyType {
    Federation,
    Cooperative,
    Community,
    Individual,
}

/// Value transfer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    /// Whether transfers are allowed
    #[serde(default)]
    pub allowed: bool,

    /// Accepted denominations by party
    #[serde(default)]
    pub denominations: Vec<PartyDenomination>,

    /// Exchange rates
    #[serde(default)]
    pub exchange_rates: Vec<ExchangeRate>,

    /// Settlement configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement: Option<SettlementConfig>,
}

/// Denominations accepted by a party.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyDenomination {
    /// Party ID
    pub party: String,

    /// Accepted currency types
    pub accepts: Vec<String>,
}

/// Exchange rate between currencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// Source currency
    pub from: String,

    /// Target currency
    pub to: String,

    /// Exchange rate
    pub rate: f64,

    /// Last updated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
}

/// Settlement configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementConfig {
    /// Settlement cycle
    pub cycle: SettlementCycle,

    /// Settlement method
    pub method: SettlementMethod,
}

/// Settlement cycle frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementCycle {
    Daily,
    Weekly,
    Biweekly,
    Monthly,
}

/// Settlement method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMethod {
    NetSettlement,
    GrossSettlement,
    RealTime,
}

/// Credential recognition rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialConfig {
    /// Mutual recognition rules
    #[serde(default)]
    pub mutual_recognition: Vec<CredentialRecognition>,
}

/// A single credential recognition rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRecognition {
    /// Credential type
    #[serde(rename = "type")]
    pub credential_type: String,

    /// Recognition scope
    pub scope: RecognitionScope,
}

/// Scope of credential recognition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionScope {
    /// Can verify but not issue
    VerificationOnly,
    /// Informational, not authoritative
    Advisory,
    /// Full authority
    Full,
}

/// Joint decision process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointDecision {
    /// Decision name
    pub name: String,

    /// What approval is required
    pub requires: String,
}

/// Boundary outcome type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryOutcomeType {
    /// Simple approved/rejected
    Binary,
    /// Graduated (e.g., approved with conditions)
    Graduated,
}

/// Joint governance configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointGovernance {
    /// Joint decision processes
    #[serde(default)]
    pub process: Vec<JointDecision>,

    /// Boundary outcome type
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_outcome: Option<BoundaryOutcome>,
}

/// Boundary outcome configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryOutcome {
    /// Outcome type
    #[serde(rename = "type")]
    pub outcome_type: BoundaryOutcomeType,
}

/// Boundary protocol (what crosses federation lines).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryProtocol {
    /// Value transfer rules
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfers: Option<TransferConfig>,

    /// Credential recognition
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<CredentialConfig>,

    /// Joint governance
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint_decisions: Option<JointGovernance>,
}

/// Dispute resolution stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeStage {
    /// Stage name
    pub stage: DisputeStageName,

    /// Duration limit
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<super::governance::Duration>,

    /// Who handles this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parties: Option<String>,

    /// How mediator/arbitrator is selected
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mediator: Option<MediatorSelection>,

    /// Panel size (for arbitration)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<u32>,

    /// Whether decision is binding
    #[serde(default)]
    pub binding: bool,
}

/// Dispute stage names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeStageName {
    Negotiation,
    Mediation,
    Arbitration,
    Adjudication,
}

/// Mediator selection method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediatorSelection {
    /// Selection method
    pub selected_by: SelectionMethod,
}

/// Selection methods for mediators/arbitrators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMethod {
    MutualAgreement,
    Rotation,
    RandomFromPool,
    ThirdParty,
}

/// Dispute resolution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeResolution {
    /// Resolution ladder (stages in order)
    #[serde(default)]
    pub ladder: Vec<DisputeStage>,

    /// Prohibited resolution methods
    #[serde(default)]
    pub prohibited: Vec<String>,
}

/// Exit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitConfig {
    /// Notice period required
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice_period: Option<super::governance::Duration>,

    /// Obligations that survive exit
    #[serde(default)]
    pub obligations_survive: Vec<String>,
}

/// Incompatibility handling action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompatibilityAction {
    RejectTransfer,
    EscalateToDispute,
    RequestAttestation,
    TreatAsUnverified,
}

/// Handling for a specific incompatibility type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncompatibilityHandler {
    /// Action to take
    pub action: IncompatibilityAction,

    /// Notification behavior
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<String>,

    /// Logging configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<LogConfig>,

    /// Timeout before escalation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<super::governance::Duration>,

    /// Default if no resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_if_no_resolution: Option<String>,

    /// Fallback action
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<IncompatibilityAction>,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Reason code
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Include details
    #[serde(default)]
    pub details: bool,
}

/// Incompatibility handling configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncompatibilityHandling {
    /// Value incompatibility handler
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_incompatible: Option<IncompatibilityHandler>,

    /// Governance deadlock handler
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance_deadlock: Option<IncompatibilityHandler>,

    /// Unknown credential handler
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_unknown: Option<IncompatibilityHandler>,
}

/// Complete agreement schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgreementSchema {
    /// Agreement name
    pub name: String,

    /// Parties to the agreement
    #[serde(default)]
    pub parties: Vec<Party>,

    /// Boundary protocol
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary: Option<BoundaryProtocol>,

    /// Dispute resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disputes: Option<DisputeResolution>,

    /// Exit configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitConfig>,

    /// Incompatibility handling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incompatibility_handling: Option<IncompatibilityHandling>,
}

impl AgreementSchema {
    /// Validate the agreement schema.
    pub fn validate(&self) -> Result<(), SchemaError> {
        if self.name.is_empty() {
            return Err(SchemaError::Validation(
                "Agreement name cannot be empty".to_string(),
            ));
        }

        if self.parties.len() < 2 {
            return Err(SchemaError::Validation(
                "Agreement must have at least 2 parties".to_string(),
            ));
        }

        // Validate party references in boundary protocol
        if let Some(boundary) = &self.boundary {
            if let Some(transfers) = &boundary.transfers {
                let party_ids: Vec<_> = self.parties.iter().map(|p| &p.id).collect();
                for denom in &transfers.denominations {
                    if !party_ids.contains(&&denom.party) {
                        return Err(SchemaError::Reference(format!(
                            "Transfer denomination references unknown party: {}",
                            denom.party
                        )));
                    }
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
    fn test_parse_agreement() {
        let yaml = r#"
name: "Finger Lakes Mutual Aid Compact"
parties:
  - type: federation
    id: "did:icn:finger-lakes-coops"
  - type: federation
    id: "did:icn:rochester-communities"

boundary:
  transfers:
    allowed: true
    denominations:
      - party: "did:icn:finger-lakes-coops"
        accepts: [labor_hours, usd_equivalent]
      - party: "did:icn:rochester-communities"
        accepts: [labor_hours, community_credits]
    exchange_rates:
      - from: labor_hours
        to: usd_equivalent
        rate: 15.00
        updated: "2025-01-01"
    settlement:
      cycle: weekly
      method: net_settlement

  credentials:
    mutual_recognition:
      - type: membership
        scope: verification_only
      - type: skill_attestation
        scope: advisory

  joint_decisions:
    process:
      - name: "joint_operational"
        requires: "approval from each party by their own rules"
      - name: "agreement_amendment"
        requires: "approval from each party, threshold: constitutional"
    boundary_outcome:
      type: binary

disputes:
  ladder:
    - stage: negotiation
      duration:
        days: 30
      parties: direct
    - stage: mediation
      duration:
        days: 30
      mediator:
        selected_by: mutual_agreement
    - stage: arbitration
      duration:
        days: 60
      panel: 3
      binding: true
  prohibited: [litigation]

exit:
  notice_period:
    days: 90
  obligations_survive: [pending_settlements, active_disputes]
"#;
        let agreement: AgreementSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(agreement.name, "Finger Lakes Mutual Aid Compact");
        assert_eq!(agreement.parties.len(), 2);
        agreement.validate().expect("Validation failed");
    }

    #[test]
    fn test_incompatibility_handling() {
        let yaml = r#"
name: "Test Agreement"
parties:
  - type: federation
    id: "did:icn:a"
  - type: federation
    id: "did:icn:b"

incompatibility_handling:
  value_incompatible:
    action: reject_transfer
    notify: both_parties
    log:
      reason: "no_exchange_rate"
      details: true

  governance_deadlock:
    action: escalate_to_dispute
    timeout:
      days: 14
    default_if_no_resolution: status_quo

  credential_unknown:
    action: request_attestation
    fallback: treat_as_unverified
"#;
        let agreement: AgreementSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(agreement.incompatibility_handling.is_some());
        agreement.validate().expect("Validation failed");
    }

    #[test]
    fn test_validate_minimum_parties() {
        let yaml = r#"
name: "Invalid Agreement"
parties:
  - type: federation
    id: "did:icn:lonely"
"#;
        let agreement: AgreementSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(agreement.validate().is_err());
    }

    #[test]
    fn test_binary_boundary_outcome() {
        let yaml = r#"
name: "Simple Agreement"
parties:
  - type: cooperative
    id: "did:icn:coop-a"
  - type: cooperative
    id: "did:icn:coop-b"

boundary:
  joint_decisions:
    boundary_outcome:
      type: binary
"#;
        let agreement: AgreementSchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        let joint = agreement.boundary.unwrap().joint_decisions.unwrap();
        assert_eq!(
            joint.boundary_outcome.unwrap().outcome_type,
            BoundaryOutcomeType::Binary
        );
    }
}
