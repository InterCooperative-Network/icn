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
use cid::Cid;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use futures::lock::Mutex;
use crate::token_storage::{TokenStorage, AuthorizationStorage};

// New budget operations module
pub mod budget_ops;

// New token storage module
pub mod token_storage;

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
pub async fn create_authorization(
    grantor_did: String,
    grantee_did: String,
    resource_type: ResourceType,
    authorized_amount: u64,
    scope: IdentityScope,
    expiry_timestamp: Option<i64>,
    metadata: Option<serde_json::Value>,
    storage: &mut impl token_storage::AuthorizationStorage,
) -> EconomicsResult<ResourceAuthorization> {
    // Create the authorization
    let auth = ResourceAuthorization::new(
        grantor_did,
        grantee_did,
        resource_type,
        authorized_amount,
        scope,
        expiry_timestamp,
        metadata,
    );
    
    // Store in persistent storage
    storage.store_authorization(&auth).await?;
    
    Ok(auth)
}

/// Validate if a resource authorization can be used for the requested amount at the current time
pub async fn validate_authorization_usage(
    auth_id: &Uuid,
    requested_amount: u64,
    current_timestamp: i64,
    storage: &impl token_storage::AuthorizationStorage,
) -> EconomicsResult<()> {
    // Retrieve the authorization from storage
    let auth = storage.get_authorization(auth_id).await?
        .ok_or_else(|| EconomicsError::AuthorizationNotFound(*auth_id))?;
    
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
pub async fn consume_authorization(
    auth_id: &Uuid,
    consumed_amount: u64,
    current_timestamp: i64,
    storage: &mut impl token_storage::AuthorizationStorage,
) -> EconomicsResult<ResourceAuthorization> {
    // First validate the usage
    validate_authorization_usage(auth_id, consumed_amount, current_timestamp, storage).await?;
    
    // Retrieve the authorization from storage
    let mut auth = storage.get_authorization(auth_id).await?
        .ok_or_else(|| EconomicsError::AuthorizationNotFound(*auth_id))?;
    
    // Update the consumed amount
    auth.consumed_amount += consumed_amount;
    
    // Store the updated authorization
    storage.update_authorization(&auth).await?;
    
    Ok(auth)
}

/// Host ABI functions for token operations
pub mod token_ops {
    use super::*;
    use crate::token_storage::TokenStorage;
    use std::sync::Arc;
    use futures::lock::Mutex;
    
    /// Mint a new token
    pub async fn mint_token(
        owner_did: String,
        resource_type: ResourceType,
        amount: u64,
        scope: IdentityScope,
        metadata: Option<serde_json::Value>,
        storage: &mut impl TokenStorage,
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
        
        // Store the token in persistent storage
        storage.store_token(&token).await?;
        
        Ok(token)
    }
    
    /// Transfer a token from one owner to another
    pub async fn transfer_token(
        token_id: Uuid,
        from_did: &str,
        to_did: &str,
        storage: &mut impl TokenStorage,
    ) -> EconomicsResult<()> {
        // Retrieve the token
        let token_opt = storage.get_token(&token_id).await?;
        
        // Check if token exists
        let mut token = token_opt.ok_or_else(|| 
            EconomicsError::TokenNotFound(token_id)
        )?;
        
        // Verify ownership
        if token.owner_did != from_did {
            return Err(EconomicsError::Unauthorized(
                format!("Token {} is not owned by {}", token_id, from_did)
            ));
        }
        
        // Update ownership
        token.owner_did = to_did.to_string();
        
        // Store the updated token
        storage.store_token(&token).await?;
        
        Ok(())
    }
    
    /// Burn a token (remove it from circulation)
    pub async fn burn_token(
        token_id: Uuid,
        owner_did: &str,
        storage: &mut impl TokenStorage,
    ) -> EconomicsResult<()> {
        // Retrieve the token
        let token_opt = storage.get_token(&token_id).await?;
        
        // Check if token exists
        let token = token_opt.ok_or_else(|| 
            EconomicsError::TokenNotFound(token_id)
        )?;
        
        // Verify ownership
        if token.owner_did != owner_did {
            return Err(EconomicsError::Unauthorized(
                format!("Token {} is not owned by {}", token_id, owner_did)
            ));
        }
        
        // Delete the token from storage
        storage.delete_token(&token_id).await?;
        
        Ok(())
    }
    
    /// Get all tokens owned by a specific DID
    pub async fn get_tokens_by_owner(
        owner_did: &str, 
        storage: &impl TokenStorage
    ) -> EconomicsResult<Vec<ScopedResourceToken>> {
        storage.list_tokens_by_owner(owner_did).await
    }
    
    /// Get token by ID
    pub async fn get_token_by_id(
        token_id: &Uuid,
        storage: &impl TokenStorage
    ) -> EconomicsResult<Option<ScopedResourceToken>> {
        storage.get_token(token_id).await
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

/// Voting method for budget proposals
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VotingMethod {
    /// Simple majority voting (>50%)
    SimpleMajority,
    
    /// Quadratic voting (votes weighted by square root of stake)
    Quadratic,
    
    /// Threshold voting (requires specific % of yes votes)
    Threshold,
}

/// Configuration rules for budget governance derived from CCL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRulesConfig {
    /// Voting method for this budget
    pub voting_method: Option<VotingMethod>,
    
    /// Resource categories with allocation constraints
    pub categories: Option<HashMap<String, CategoryRule>>,
    
    /// Minimum participants needed for decisions (quorum)
    pub min_participants: Option<u32>,
    
    /// Percentage of votes needed for quorum (0-100)
    pub quorum_percentage: Option<u8>,
    
    /// Percentage of votes needed for approval threshold (0-100)
    pub threshold_percentage: Option<u8>,
    
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
    /// Proposal has been submitted but not yet opened for voting
    Proposed,
    
    /// Proposal is open for voting
    VotingOpen,
    
    /// Voting period has closed but not yet tallied
    VotingClosed,
    
    /// Proposal has been approved
    Approved,
    
    /// Proposal has been rejected
    Rejected,
    
    /// Proposal has been executed (resources allocated)
    Executed,
    
    /// Proposal execution has failed
    Failed,
    
    /// Proposal has been cancelled
    Cancelled,
}

/// Type of vote that can be cast on a budget proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteChoice {
    /// Vote to approve the proposal
    Approve,
    
    /// Vote to reject the proposal
    Reject,
    
    /// Abstain from voting (counts for quorum but not for approval/rejection)
    Abstain,
    
    /// Quadratic vote with specific weight (for quadratic voting method)
    Quadratic(u32),
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
    
    /// Votes for this proposal (voter DID -> vote choice)
    pub votes: HashMap<String, VoteChoice>,
    
    /// Unix timestamp when this proposal was created
    pub creation_timestamp: i64,
    
    /// Additional metadata for this proposal
    pub metadata: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_storage::{MockTokenStorage, MockAuthorizationStorage, MockEconomicsStorage};
    use crate::token_storage::{TokenStorage, AuthorizationStorage};
    
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
    
    #[tokio::test]
    async fn test_resource_authorization() {
        let mut storage = MockEconomicsStorage::new();
        let now = chrono::Utc::now().timestamp();
        let future = now + 3600; // 1 hour in the future
        
        // Create authorization
        let auth = create_authorization(
            "did:icn:system".to_string(),
            "did:icn:bob".to_string(),
            ResourceType::Compute,
            1000,
            IdentityScope::Individual,
            Some(future),
            None,
            &mut storage,
        ).await.unwrap();
        
        // Verify the authorization was stored
        let stored_auth = storage.get_authorization(&auth.auth_id).await.unwrap().unwrap();
        assert_eq!(stored_auth.auth_id, auth.auth_id);
        assert_eq!(stored_auth.authorized_amount, 1000);
        assert_eq!(stored_auth.consumed_amount, 0);
        
        // Test validation
        assert!(validate_authorization_usage(&auth.auth_id, 500, now, &storage).await.is_ok());
        assert!(validate_authorization_usage(&auth.auth_id, 1001, now, &storage).await.is_err());
        
        // Test consumption
        let updated_auth = consume_authorization(&auth.auth_id, 300, now, &mut storage).await.unwrap();
        assert_eq!(updated_auth.consumed_amount, 300);
        assert_eq!(updated_auth.remaining_amount(), 700);
        
        // Verify the update was persisted
        let stored_auth = storage.get_authorization(&auth.auth_id).await.unwrap().unwrap();
        assert_eq!(stored_auth.consumed_amount, 300);
        
        // Test overconsumption
        let result = consume_authorization(&auth.auth_id, 800, now, &mut storage).await;
        assert!(result.is_err());
        
        // Create an expired authorization
        let past = now - 3600; // 1 hour in the past
        let expired_auth = create_authorization(
            "did:icn:system".to_string(),
            "did:icn:bob".to_string(),
            ResourceType::Storage,
            1000,
            IdentityScope::Individual,
            Some(past),
            None,
            &mut storage,
        ).await.unwrap();
        
        // Test consuming with expired auth
        let result = consume_authorization(&expired_auth.auth_id, 100, now, &mut storage).await;
        assert!(result.is_err());
        
        // List authorizations for grantee
        let bob_auths = storage.list_authorizations_by_grantee("did:icn:bob").await.unwrap();
        assert_eq!(bob_auths.len(), 2);
    }
    
    #[test]
    fn test_token_creation() {
        // This test doesn't test persistence, so we don't create a storage instance
        // We're just verifying the ScopedResourceToken struct works as expected
        let token = ScopedResourceToken::new(
            "did:icn:charlie".to_string(),
            ResourceType::NetworkBandwidth,
            5000,
            IdentityScope::Community,
            Some(serde_json::json!({"community": "developers"})),
            chrono::Utc::now().timestamp(),
        );
        
        assert_eq!(token.owner_did, "did:icn:charlie");
        assert!(matches!(token.resource_type, ResourceType::NetworkBandwidth));
        assert_eq!(token.amount, 5000);
        assert!(matches!(token.scope, IdentityScope::Community));
    }
    
    #[tokio::test]
    async fn test_token_operations() {
        // Create test storage
        let mut storage = MockTokenStorage::new();
        
        // Test minting a token
        let token = token_ops::mint_token(
            "did:icn:alice".to_string(),
            ResourceType::Compute,
            100,
            IdentityScope::Individual,
            Some(serde_json::json!({"purpose": "testing"})),
            &mut storage
        ).await.unwrap();
        
        // Verify token was stored
        let stored_token = storage.get_token(&token.token_id).await.unwrap().unwrap();
        assert_eq!(stored_token.token_id, token.token_id);
        assert_eq!(stored_token.amount, 100);
        
        // Test transferring the token
        token_ops::transfer_token(
            token.token_id,
            "did:icn:alice",
            "did:icn:bob",
            &mut storage
        ).await.unwrap();
        
        // Verify the transfer
        let transferred_token = token_ops::get_token_by_id(&token.token_id, &storage).await.unwrap().unwrap();
        assert_eq!(transferred_token.owner_did, "did:icn:bob");
        
        // Test burning the token
        token_ops::burn_token(
            token.token_id,
            "did:icn:bob",
            &mut storage
        ).await.unwrap();
        
        // Verify the token was burned (deleted)
        let burned_token = token_ops::get_token_by_id(&token.token_id, &storage).await.unwrap();
        assert!(burned_token.is_none());
    }
    
    #[tokio::test]
    async fn test_token_unauthorized_operations() {
        // Create test storage
        let mut storage = MockTokenStorage::new();
        
        // Mint a token
        let token = token_ops::mint_token(
            "did:icn:alice".to_string(),
            ResourceType::Storage,
            200,
            IdentityScope::Individual,
            None,
            &mut storage
        ).await.unwrap();
        
        // Test unauthorized transfer
        let result = token_ops::transfer_token(
            token.token_id,
            "did:icn:bob", // Not the owner
            "did:icn:charlie",
            &mut storage
        ).await;
        
        assert!(result.is_err());
        if let Err(EconomicsError::Unauthorized(_)) = result {
            // Expected error
        } else {
            panic!("Expected Unauthorized error, got: {:?}", result);
        }
        
        // Test unauthorized burn
        let result = token_ops::burn_token(
            token.token_id,
            "did:icn:eve", // Not the owner
            &mut storage
        ).await;
        
        assert!(result.is_err());
        if let Err(EconomicsError::Unauthorized(_)) = result {
            // Expected error
        } else {
            panic!("Expected Unauthorized error, got: {:?}", result);
        }
    }
    
    #[tokio::test]
    async fn test_tokens_by_owner() {
        // Create test storage
        let mut storage = MockTokenStorage::new();
        
        // Mint three tokens with different owners
        let _token1 = token_ops::mint_token(
            "did:icn:alice".to_string(),
            ResourceType::Compute,
            100,
            IdentityScope::Individual,
            None,
            &mut storage
        ).await.unwrap();
        
        let _token2 = token_ops::mint_token(
            "did:icn:alice".to_string(),
            ResourceType::Storage,
            200,
            IdentityScope::Individual,
            None,
            &mut storage
        ).await.unwrap();
        
        let token3 = token_ops::mint_token(
            "did:icn:bob".to_string(),
            ResourceType::NetworkBandwidth,
            300,
            IdentityScope::Individual,
            None,
            &mut storage
        ).await.unwrap();
        
        // Test listing tokens by owner
        let alice_tokens = token_ops::get_tokens_by_owner("did:icn:alice", &storage).await.unwrap();
        assert_eq!(alice_tokens.len(), 2);
        
        let bob_tokens = token_ops::get_tokens_by_owner("did:icn:bob", &storage).await.unwrap();
        assert_eq!(bob_tokens.len(), 1);
        assert_eq!(bob_tokens[0].token_id, token3.token_id);
        
        let charlie_tokens = token_ops::get_tokens_by_owner("did:icn:charlie", &storage).await.unwrap();
        assert_eq!(charlie_tokens.len(), 0);
    }
} 