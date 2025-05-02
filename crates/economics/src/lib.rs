/*!
# ICN Economic System

This crate implements the economic system for the ICN Runtime, including scoped tokens,
metering logic, treasury operations, and budgeting primitives.

## Architectural Tenets
- Economics = Scoped Resource Tokens (icn:resource/...) represent capabilities/access, not currency
- No speculation
- Includes primitives for Participatory Budgeting
- Metering via explicit ResourceAuthorization
*/

use icn_identity::IdentityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use std::collections::HashMap;

// New budget operations module
pub mod budget_ops;

/// Errors that can occur during economic operations
#[derive(Debug, Error)]
pub enum EconomicsError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    
    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),
    
    #[error("Invalid budget: {0}")]
    InvalidBudget(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Authorization expired at {0}")]
    AuthorizationExpired(i64),
    
    #[error("Insufficient authorization: requested {requested}, available {available}")]
    InsufficientAuthorization { requested: u64, available: u64 },
    
    #[error("Invalid resource type: {0}")]
    InvalidResourceType(String),
    
    #[error("Authorization not found with ID: {0}")]
    AuthorizationNotFound(Uuid),
    
    #[error("Token not found with ID: {0}")]
    TokenNotFound(Uuid),
}

/// Result type for economic operations
pub type EconomicsResult<T> = Result<T, EconomicsError>;

/// Types of resources
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    /// Compute resources (CPU time, etc.)
    Compute,
    
    /// Storage resources (disk space, etc.)
    Storage,
    
    /// Network bandwidth
    NetworkBandwidth,
    
    /// Labor hours contributed to a project
    LaborHours { skill: String },
    
    /// Credits issued by a community
    CommunityCredit { community_did: String },
    
    /// Custom resource type
    Custom { identifier: String },
}

/// Represents a scoped resource token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedResourceToken {
    /// Unique identifier for this token instance
    pub token_id: Uuid,
    
    /// DID of the current owner
    pub owner_did: String,
    
    /// The resource type
    pub resource_type: ResourceType,
    
    /// The amount of the resource
    pub amount: u64,
    
    /// The scope of this token
    pub scope: IdentityScope,
    
    /// Arbitrary metadata (e.g., source proposal CID)
    pub metadata: Option<serde_json::Value>,
    
    /// Unix timestamp of issuance
    pub issuance_date: i64,
}

impl ScopedResourceToken {
    /// Create a new scoped resource token
    pub fn new(
        owner_did: String,
        resource_type: ResourceType,
        amount: u64,
        scope: IdentityScope,
        metadata: Option<serde_json::Value>,
        issuance_date: i64,
    ) -> Self {
        Self {
            token_id: Uuid::new_v4(),
            owner_did,
            resource_type,
            amount,
            scope,
            metadata,
            issuance_date,
        }
    }
    
    /// Check if this token is valid for a specific scope
    pub fn is_valid_for_scope(&self, scope: &IdentityScope) -> bool {
        match (&self.scope, scope) {
            // Individual tokens can only be used by that individual
            (IdentityScope::Individual, IdentityScope::Individual) => true,
            
            // Cooperative tokens can be used in cooperative contexts
            (IdentityScope::Cooperative, IdentityScope::Cooperative) => true,
            
            // Community tokens can be used in community contexts
            (IdentityScope::Community, IdentityScope::Community) => true,
            
            // Federation tokens can be used in federation contexts
            (IdentityScope::Federation, IdentityScope::Federation) => true,
            
            // Node-specific tokens
            (IdentityScope::Node, IdentityScope::Node) => true,
            
            // Guardian tokens can be used in guardian contexts
            (IdentityScope::Guardian, IdentityScope::Guardian) => true,
            
            // Other scope combinations are not valid
            _ => false,
        }
    }
}

/// Represents an authorization to use resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAuthorization {
    /// Unique identifier for this authorization
    pub auth_id: Uuid,
    
    /// DID granting the authorization
    pub grantor_did: String,
    
    /// DID receiving the authorization
    pub grantee_did: String,
    
    /// The resource type
    pub resource_type: ResourceType,
    
    /// Maximum amount authorized for use
    pub authorized_amount: u64,
    
    /// Amount already used (default 0)
    pub consumed_amount: u64,
    
    /// The scope of this authorization
    pub scope: IdentityScope,
    
    /// Optional expiry timestamp (Unix timestamp)
    pub expiry_timestamp: Option<i64>,
    
    /// Arbitrary metadata (e.g., link to governing proposal)
    pub metadata: Option<serde_json::Value>,
}

impl ResourceAuthorization {
    /// Create a new resource authorization
    pub fn new(
        grantor_did: String,
        grantee_did: String,
        resource_type: ResourceType,
        authorized_amount: u64,
        scope: IdentityScope,
        expiry_timestamp: Option<i64>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            auth_id: Uuid::new_v4(),
            grantor_did,
            grantee_did,
            resource_type,
            authorized_amount,
            consumed_amount: 0,
            scope,
            expiry_timestamp,
            metadata,
        }
    }
    
    /// Check if this authorization is valid at the given timestamp
    pub fn is_valid(&self, current_timestamp: i64) -> bool {
        if let Some(expiry) = self.expiry_timestamp {
            current_timestamp < expiry
        } else {
            true // No expiry = always valid
        }
    }
    
    /// Get the remaining amount available in this authorization
    pub fn remaining_amount(&self) -> u64 {
        if self.consumed_amount > self.authorized_amount {
            0 // Should not happen, but fail safe
        } else {
            self.authorized_amount - self.consumed_amount
        }
    }
}

/// Create a new resource authorization
pub fn create_authorization(
    grantor_did: String,
    grantee_did: String,
    resource_type: ResourceType,
    authorized_amount: u64,
    scope: IdentityScope,
    expiry_timestamp: Option<i64>,
    metadata: Option<serde_json::Value>,
) -> ResourceAuthorization {
    ResourceAuthorization::new(
        grantor_did,
        grantee_did,
        resource_type,
        authorized_amount,
        scope,
        expiry_timestamp,
        metadata,
    )
}

/// Validate if a resource authorization can be used for the requested amount at the current time
pub fn validate_authorization_usage(
    auth: &ResourceAuthorization,
    requested_amount: u64,
    current_timestamp: i64,
) -> EconomicsResult<()> {
    // Check if authorization has expired
    if !auth.is_valid(current_timestamp) {
        if let Some(expiry) = auth.expiry_timestamp {
            return Err(EconomicsError::AuthorizationExpired(expiry));
        }
    }
    
    // Check if there's enough remaining amount
    let remaining = auth.remaining_amount();
    if requested_amount > remaining {
        return Err(EconomicsError::InsufficientAuthorization {
            requested: requested_amount,
            available: remaining,
        });
    }
    
    Ok(())
}

/// Consume some amount from a resource authorization
pub fn consume_authorization(
    auth: &mut ResourceAuthorization,
    consumed_amount: u64,
    current_timestamp: i64,
) -> EconomicsResult<()> {
    // First validate the usage
    validate_authorization_usage(auth, consumed_amount, current_timestamp)?;
    
    // Update the consumed amount
    auth.consumed_amount += consumed_amount;
    
    Ok(())
}

/// Host ABI functions for token operations
pub mod token_ops {
    use super::*;
    
    /// Mint a new token
    pub fn mint_token(
        owner_did: String,
        resource_type: ResourceType,
        amount: u64,
        scope: IdentityScope,
        metadata: Option<serde_json::Value>,
    ) -> EconomicsResult<ScopedResourceToken> {
        // Create token with current timestamp
        let now = chrono::Utc::now().timestamp();
        let token = ScopedResourceToken::new(
            owner_did,
            resource_type,
            amount,
            scope,
            metadata,
            now,
        );
        
        // TODO(V3-MVP): Add token to state/storage
        
        Ok(token)
    }
    
    /// Transfer a token
    pub fn transfer_token(
        _token_id: Uuid,
        _from_did: &str,
        _to_did: &str,
    ) -> EconomicsResult<()> {
        // TODO(V3-MVP): Implement token transfer logic with state/storage
        Err(EconomicsError::InvalidToken("Not implemented".to_string()))
    }
    
    /// Burn a token
    pub fn burn_token(
        _token_id: Uuid,
        _owner_did: &str,
    ) -> EconomicsResult<()> {
        // TODO(V3-MVP): Implement token burning logic with state/storage
        Err(EconomicsError::InvalidToken("Not implemented".to_string()))
    }
}

/// Represents a participatory budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipatoryBudget {
    /// Unique ID for this budget instance (e.g., derived from scope_id + timeframe)
    pub id: String,
    
    /// The name of this budget
    pub name: String,
    
    /// The DID of the governing Coop/Community
    pub scope_id: String,
    
    /// The scope of this budget
    pub scope_type: IdentityScope,
    
    /// The total allocated resources for each resource type
    pub total_allocated: HashMap<ResourceType, u64>,
    
    /// Amount spent by proposal for each resource type
    pub spent_by_proposal: HashMap<Uuid, HashMap<ResourceType, u64>>,
    
    /// The proposals in this budget
    pub proposals: HashMap<Uuid, BudgetProposal>,
    
    /// Rules from the CCL configuration
    pub rules: Option<BudgetRulesConfig>,
    
    /// Start timestamp (Unix timestamp)
    pub start_timestamp: i64,
    
    /// End timestamp (Unix timestamp)
    pub end_timestamp: i64,
}

/// Configuration rules for budget governance derived from CCL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRulesConfig {
    /// Voting method (e.g., "quadratic_voting", "simple_majority")
    pub voting_method: Option<String>,
    
    /// Resource categories with allocation constraints
    pub categories: Option<HashMap<String, CategoryRule>>,
    
    /// Minimum participants needed for decisions
    pub min_participants: Option<u32>,
    
    /// Other custom rules specific to this budget
    pub custom_rules: Option<serde_json::Value>,
}

/// Rules for a specific budget category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRule {
    /// Minimum allocation percentage (0-100)
    pub min_allocation: Option<u8>,
    
    /// Maximum allocation percentage (0-100)
    pub max_allocation: Option<u8>,
    
    /// Description of this category
    pub description: Option<String>,
}

/// Status of a budget proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    /// Proposal has been submitted but not yet approved
    Proposed,
    
    /// Proposal has been approved for implementation
    Approved,
    
    /// Proposal has been rejected
    Rejected,
    
    /// Proposal has been implemented and completed
    Completed,
    
    /// Proposal has been cancelled
    Cancelled,
}

/// Represents a budget proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetProposal {
    /// Unique identifier for this proposal
    pub id: Uuid,
    
    /// The title of this proposal
    pub title: String,
    
    /// The description of this proposal
    pub description: String,
    
    /// The DID of the proposer
    pub proposer_did: String,
    
    /// Resources requested per resource type
    pub requested_resources: HashMap<ResourceType, u64>,
    
    /// Current status of the proposal
    pub status: ProposalStatus,
    
    /// Optional category from budget rules
    pub category: Option<String>,
    
    /// Votes for this proposal (DID -> vote value)
    pub votes: HashMap<String, i32>,
    
    /// Unix timestamp when this proposal was created
    pub creation_timestamp: i64,
    
    /// Additional metadata for this proposal
    pub metadata: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resource_type() {
        let compute = ResourceType::Compute;
        let storage = ResourceType::Storage;
        let labor = ResourceType::LaborHours { skill: "Programming".to_string() };
        let community = ResourceType::CommunityCredit { community_did: "did:icn:community123".to_string() };
        let custom = ResourceType::Custom { identifier: "special-resource".to_string() };
        
        assert_ne!(compute, storage);
        assert_ne!(labor, community);
        assert_ne!(custom, compute);
    }
    
    #[test]
    fn test_scoped_resource_token() {
        let token = ScopedResourceToken::new(
            "did:icn:alice".to_string(),
            ResourceType::Compute,
            100,
            IdentityScope::Individual,
            Some(serde_json::json!({"note": "Test token"})),
            chrono::Utc::now().timestamp(),
        );
        
        assert_eq!(token.owner_did, "did:icn:alice");
        assert_eq!(token.amount, 100);
        assert!(matches!(token.resource_type, ResourceType::Compute));
        assert!(token.is_valid_for_scope(&IdentityScope::Individual));
        assert!(!token.is_valid_for_scope(&IdentityScope::Community));
    }
    
    #[test]
    fn test_resource_authorization() {
        let now = chrono::Utc::now().timestamp();
        let future = now + 3600; // 1 hour in the future
        
        let auth = ResourceAuthorization::new(
            "did:icn:system".to_string(),
            "did:icn:bob".to_string(),
            ResourceType::Storage,
            1000,
            IdentityScope::Individual,
            Some(future),
            None,
        );
        
        assert_eq!(auth.authorized_amount, 1000);
        assert_eq!(auth.consumed_amount, 0);
        assert_eq!(auth.remaining_amount(), 1000);
        assert!(auth.is_valid(now));
        
        // Test with expired authorization
        let past = now - 3600; // 1 hour in the past
        let expired_auth = ResourceAuthorization::new(
            "did:icn:system".to_string(),
            "did:icn:bob".to_string(),
            ResourceType::Storage,
            1000,
            IdentityScope::Individual,
            Some(past),
            None,
        );
        
        assert!(!expired_auth.is_valid(now));
    }
    
    #[test]
    fn test_authorization_helpers() {
        let now = chrono::Utc::now().timestamp();
        let future = now + 3600; // 1 hour in the future
        
        let mut auth = ResourceAuthorization::new(
            "did:icn:system".to_string(),
            "did:icn:bob".to_string(),
            ResourceType::Compute,
            1000,
            IdentityScope::Individual,
            Some(future),
            None,
        );
        
        // Test validation
        assert!(validate_authorization_usage(&auth, 500, now).is_ok());
        assert!(validate_authorization_usage(&auth, 1001, now).is_err());
        
        // Test consumption
        assert!(consume_authorization(&mut auth, 300, now).is_ok());
        assert_eq!(auth.consumed_amount, 300);
        assert_eq!(auth.remaining_amount(), 700);
        
        // Test overconsumption
        assert!(consume_authorization(&mut auth, 800, now).is_err());
        assert_eq!(auth.consumed_amount, 300); // Should remain unchanged
        
        // Test consuming with expired auth
        let past = now - 3600; // 1 hour in the past
        let mut expired_auth = ResourceAuthorization::new(
            "did:icn:system".to_string(),
            "did:icn:bob".to_string(),
            ResourceType::Storage,
            1000,
            IdentityScope::Individual,
            Some(past),
            None,
        );
        
        assert!(consume_authorization(&mut expired_auth, 100, now).is_err());
        assert_eq!(expired_auth.consumed_amount, 0); // Should remain unchanged
    }
    
    #[test]
    fn test_token_creation() {
        let token = token_ops::mint_token(
            "did:icn:charlie".to_string(),
            ResourceType::NetworkBandwidth,
            5000,
            IdentityScope::Community,
            Some(serde_json::json!({"community": "developers"})),
        ).unwrap();
        
        assert_eq!(token.owner_did, "did:icn:charlie");
        assert!(matches!(token.resource_type, ResourceType::NetworkBandwidth));
        assert_eq!(token.amount, 5000);
        assert!(matches!(token.scope, IdentityScope::Community));
    }
} 