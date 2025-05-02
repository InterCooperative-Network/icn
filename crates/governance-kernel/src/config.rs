use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use icn_identity::IdentityScope;

/// A governance configuration parsed from CCL
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// The template type (e.g., "coop_bylaws", "community_charter")
    pub template_type: String,
    
    /// The template version (e.g., "v1")
    pub template_version: String,
    
    /// The identity scope this configuration applies to
    pub governing_scope: IdentityScope,
    
    /// Basic identity information
    pub identity: Option<IdentityInfo>,
    
    /// Governance structure
    pub governance: Option<GovernanceStructure>,
    
    /// Membership rules
    pub membership: Option<MembershipRules>,
    
    /// Proposal process
    pub proposals: Option<ProposalProcess>,
    
    /// Working groups structure
    pub working_groups: Option<WorkingGroups>,
    
    /// Dispute resolution process
    pub dispute_resolution: Option<DisputeResolution>,
    
    /// Economic model
    pub economic_model: Option<EconomicModel>,
}

/// Basic identity information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentityInfo {
    /// Name of the organization
    pub name: Option<String>,
    
    /// Description of the organization
    pub description: Option<String>,
    
    /// Founding date of the organization
    pub founding_date: Option<String>,
    
    /// Mission statement of the organization
    pub mission_statement: Option<String>,
}

/// Governance structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceStructure {
    /// Decision making method (e.g., "consensus", "consent", "majority")
    pub decision_making: Option<String>,
    
    /// Quorum required for decisions (fraction of members)
    pub quorum: Option<f64>,
    
    /// Required majority for decisions (fraction of votes)
    pub majority: Option<f64>,
    
    /// Term length for elected positions (in days)
    pub term_length: Option<u64>,
    
    /// Defined roles in the organization
    pub roles: Option<Vec<Role>>,
}

/// A role in the organization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    /// Name of the role
    pub name: String,
    
    /// Permissions granted to this role
    pub permissions: Vec<String>,
}

/// Membership rules
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MembershipRules {
    /// Onboarding process
    pub onboarding: Option<Onboarding>,
    
    /// Membership dues
    pub dues: Option<Dues>,
    
    /// Offboarding process
    pub offboarding: Option<Offboarding>,
}

/// Onboarding process
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Onboarding {
    /// Whether a sponsor is required
    pub requires_sponsor: Option<bool>,
    
    /// Trial period in days
    pub trial_period_days: Option<u64>,
    
    /// Requirements for joining
    pub requirements: Option<Vec<String>>,
}

/// Membership dues
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dues {
    /// Amount in tokens
    pub amount: Option<u64>,
    
    /// Frequency of payment
    pub frequency: Option<String>,
    
    /// Variable options for dues
    pub variable_options: Option<Vec<DuesOption>>,
}

/// A dues option
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuesOption {
    /// Amount in tokens
    pub amount: u64,
    
    /// Description of this option
    pub description: String,
}

/// Offboarding process
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offboarding {
    /// Notice period in days
    pub notice_period_days: Option<u64>,
    
    /// Maximum inactive days before automatic removal
    pub max_inactive_days: Option<u64>,
}

/// Proposal process
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalProcess {
    /// Types of proposals
    pub types: Option<Vec<ProposalType>>,
}

/// A proposal type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalType {
    /// Name of this proposal type
    pub name: String,
    
    /// Modifier for quorum requirement
    pub quorum_modifier: Option<f64>,
    
    /// Modifier for majority requirement
    pub majority_modifier: Option<f64>,
    
    /// Discussion period in days
    pub discussion_period_days: Option<u64>,
}

/// Working groups structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkingGroups {
    /// Minimum members to form a group
    pub formation_threshold: Option<u64>,
    
    /// Minimum members to maintain a group
    pub dissolution_threshold: Option<u64>,
    
    /// Resource allocation for working groups
    pub resource_allocation: Option<ResourceAllocation>,
}

/// Resource allocation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// Default budget allocation
    pub default_budget: Option<u64>,
    
    /// Whether budget changes need approval
    pub requires_approval: Option<bool>,
}

/// Dispute resolution process
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisputeResolution {
    /// Steps in the dispute resolution process
    pub process: Option<Vec<String>>,
    
    /// Size of the dispute resolution committee
    pub committee_size: Option<u64>,
}

/// Economic model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EconomicModel {
    /// Method of surplus distribution
    pub surplus_distribution: Option<String>,
    
    /// Compensation policy
    pub compensation_policy: Option<CompensationPolicy>,
}

/// Compensation policy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompensationPolicy {
    /// Hourly rates for different types of work
    pub hourly_rates: Option<HashMap<String, u64>>,
    
    /// Whether to track hours
    pub track_hours: Option<bool>,
    
    /// Whether volunteer options are available
    pub volunteer_options: Option<bool>,
} 