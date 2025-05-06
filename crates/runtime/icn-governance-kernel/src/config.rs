use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use icn_identity::IdentityScope;
use crate::ast::{Node, Value};
use anyhow::{anyhow, Result};
use icn_economics::{ResourceType, BudgetRulesConfig, CategoryRule};
use chrono;

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

/// Configuration for participatory budgeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Name of the budget
    pub name: String,
    
    /// Scope type (e.g., Community, Cooperative)
    pub scope_type: IdentityScope,
    
    /// Scope identifier (e.g., DID)
    pub scope_id: String,
    
    /// Resource allocations
    pub resource_allocations: HashMap<ResourceType, u64>,
    
    /// Start date (Unix timestamp)
    pub start_timestamp: i64,
    
    /// End date (Unix timestamp)
    pub end_timestamp: i64,
    
    /// Governance rules
    pub rules: BudgetRulesConfig,
}

impl BudgetConfig {
    /// Try to parse a participatory budget config from CCL AST
    pub fn try_from_ast(node: &Node) -> Result<Self> {
        if let Node::Block { name, properties, .. } = node {
            if name != "participatory_budget:v1" {
                return Err(anyhow!("Expected participatory_budget:v1 block, found {}", name));
            }
            
            // Parse identity section
            let identity = properties.get("identity")
                .ok_or_else(|| anyhow!("Missing identity section in budget config"))?;
            
            let name = match identity.get_property("name") {
                Some(Node::Property { value: Value::String(s), .. }) => s.clone(),
                _ => return Err(anyhow!("Missing or invalid name in budget identity")),
            };
            
            let scope_type = match identity.get_property("scope") {
                Some(Node::Property { value: Value::Identifier(s), .. }) => {
                    match s.as_str() {
                        "Individual" => IdentityScope::Individual,
                        "Cooperative" => IdentityScope::Cooperative,
                        "Community" => IdentityScope::Community,
                        "Federation" => IdentityScope::Federation,
                        "Node" => IdentityScope::Node,
                        "Guardian" => IdentityScope::Guardian,
                        _ => return Err(anyhow!("Invalid scope type: {}", s)),
                    }
                },
                _ => return Err(anyhow!("Missing or invalid scope in budget identity")),
            };
            
            let scope_id = match identity.get_property("scope_id") {
                Some(Node::Property { value: Value::String(s), .. }) => s.clone(),
                _ => return Err(anyhow!("Missing or invalid scope_id in budget identity")),
            };
            
            // Parse resource allocations
            let resource_allocations = if let Some(Node::Property { value: Value::Array(resources), .. }) = 
                identity.get_property("total_resources") {
                let mut allocations = HashMap::new();
                
                for resource in resources {
                    if let Node::Object { properties, .. } = &**resource {
                        let resource_type = match properties.get("type") {
                            Some(Value::String(s)) => {
                                match s.as_str() {
                                    "Compute" => ResourceType::Compute,
                                    "Storage" => ResourceType::Storage,
                                    "Network" => ResourceType::NetworkBandwidth,
                                    "Labor" => ResourceType::LaborHours { skill: "general".to_string() },
                                    _ => {
                                        // For unrecognized types, use custom
                                        ResourceType::Custom { 
                                            identifier: s.clone() 
                                        }
                                    }
                                }
                            },
                            _ => continue, // Skip invalid resource types
                        };
                        
                        let amount = match properties.get("amount") {
                            Some(Value::Number(n)) => {
                                n.parse::<u64>().unwrap_or(0)
                            },
                            _ => continue, // Skip invalid amounts
                        };
                        
                        allocations.insert(resource_type, amount);
                    }
                }
                
                allocations
            } else {
                HashMap::new()
            };
            
            // Parse timeframe
            let (start_timestamp, end_timestamp) = if let Some(timeframe) = identity.get_property("timeframe") {
                if let Node::Property { value: Value::Object(timeframe_obj), .. } = timeframe {
                    let start_date = match timeframe_obj.get("start_date") {
                        Some(Value::String(s)) => {
                            // Parse date string to timestamp
                            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
                                .unwrap_or_else(|_| chrono::Utc::now().timestamp())
                        },
                        _ => chrono::Utc::now().timestamp(),
                    };
                    
                    let end_date = match timeframe_obj.get("end_date") {
                        Some(Value::String(s)) => {
                            // Parse date string to timestamp
                            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                .map(|date| date.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp())
                                .unwrap_or_else(|_| start_date + 31536000) // Default: 1 year
                        },
                        _ => start_date + 31536000, // Default: 1 year
                    };
                    
                    (start_date, end_date)
                } else {
                    (chrono::Utc::now().timestamp(), chrono::Utc::now().timestamp() + 31536000)
                }
            } else {
                (chrono::Utc::now().timestamp(), chrono::Utc::now().timestamp() + 31536000)
            };
            
            // Parse governance rules
            let rules = if let Some(governance) = properties.get("governance") {
                // Extract voting method
                let voting_method = governance.get_property("decision_method")
                    .and_then(|node| {
                        if let Node::Property { value: Value::String(s), .. } = node {
                            match s.as_str() {
                                "simple_majority" => Some(icn_economics::VotingMethod::SimpleMajority),
                                "quadratic_voting" => Some(icn_economics::VotingMethod::Quadratic),
                                "threshold" => Some(icn_economics::VotingMethod::Threshold),
                                _ => None, // Unknown voting method
                            }
                        } else {
                            None
                        }
                    });
                
                // Extract minimum participants
                let min_participants = governance.get_property("phases")
                    .and_then(|node| {
                        if let Node::Property { value: Value::Object(phases), .. } = node {
                            phases.get("voting")
                                .and_then(|voting| {
                                    if let Value::Object(voting_obj) = voting {
                                        voting_obj.get("min_participants")
                                            .and_then(|v| {
                                                if let Value::Number(n) = v {
                                                    n.parse::<u32>().ok()
                                                } else {
                                                    None
                                                }
                                            })
                                    } else {
                                        None
                                    }
                                })
                        } else {
                            None
                        }
                    });
                
                // Extract quorum percentage
                let quorum_percentage = governance.get_property("quorum_percentage")
                    .and_then(|node| {
                        if let Node::Property { value: Value::Number(n), .. } = node {
                            n.parse::<u8>().ok().filter(|&v| v <= 100)
                        } else {
                            None
                        }
                    });
                    
                // Extract threshold percentage
                let threshold_percentage = governance.get_property("threshold_percentage")
                    .and_then(|node| {
                        if let Node::Property { value: Value::Number(n), .. } = node {
                            n.parse::<u8>().ok().filter(|&v| v <= 100)
                        } else {
                            None
                        }
                    });
                
                // Extract categories from resources section
                let categories = if let Some(resources) = properties.get("resources") {
                    if let Some(categories_node) = resources.get_property("categories") {
                        if let Node::Property { value: Value::Object(categories_map), .. } = categories_node {
                            let mut rules_map = HashMap::new();
                            
                            for (name, value) in categories_map {
                                if let Value::Object(category_obj) = value {
                                    let mut rule = CategoryRule {
                                        min_allocation: None,
                                        max_allocation: None,
                                        description: None,
                                    };
                                    
                                    // Parse description
                                    if let Some(Value::String(desc)) = category_obj.get("description") {
                                        rule.description = Some(desc.clone());
                                    }
                                    
                                    // Parse min_allocation
                                    if let Some(Value::String(min)) = category_obj.get("min_allocation") {
                                        // Parse percentage (e.g., "30%")
                                        if let Some(percent) = min.strip_suffix('%') {
                                            if let Ok(value) = percent.trim().parse::<u8>() {
                                                if value <= 100 {
                                                    rule.min_allocation = Some(value);
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Parse max_allocation
                                    if let Some(Value::String(max)) = category_obj.get("max_allocation") {
                                        // Parse percentage (e.g., "40%")
                                        if let Some(percent) = max.strip_suffix('%') {
                                            if let Ok(value) = percent.trim().parse::<u8>() {
                                                if value <= 100 {
                                                    rule.max_allocation = Some(value);
                                                }
                                            }
                                        }
                                    }
                                    
                                    rules_map.insert(name.clone(), rule);
                                }
                            }
                            
                            Some(rules_map)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                // Extract custom rules
                let custom_rules = governance.get_property("custom_rules")
                    .and_then(|node| {
                        if let Node::Property { value: Value::Object(obj), .. } = node {
                            // Convert to JSON
                            serde_json::to_value(obj).ok()
                        } else {
                            None
                        }
                    });
                
                BudgetRulesConfig {
                    voting_method,
                    categories,
                    min_participants,
                    quorum_percentage,
                    threshold_percentage,
                    custom_rules,
                }
            } else {
                // Default empty rules
                BudgetRulesConfig {
                    voting_method: None,
                    categories: None,
                    min_participants: None,
                    quorum_percentage: Some(50), // Default 50%
                    threshold_percentage: Some(50), // Default 50%
                    custom_rules: None,
                }
            };
            
            Ok(BudgetConfig {
                name,
                scope_type,
                scope_id,
                resource_allocations,
                start_timestamp,
                end_timestamp,
                rules,
            })
        } else {
            Err(anyhow!("Expected block node for participatory budget config"))
        }
    }
    
    /// Convert to a budget rules config
    pub fn to_budget_rules(&self) -> BudgetRulesConfig {
        self.rules.clone()
    }
} 