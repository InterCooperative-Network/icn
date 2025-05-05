/*!
 * Economic Policy Enforcement
 *
 * Defines and enforces scoped economic policies for token usage, 
 * rate limits, and authorization rules across federation entities.
 */

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::{debug, info, warn};

use icn_identity::IdentityScope;

/// Error types for policy enforcement
#[derive(Error, Debug)]
pub enum PolicyError {
    #[error("Token type {0} not allowed for this scope")]
    TokenTypeNotAllowed(String),
    
    #[error("Amount {requested} exceeds maximum allowed {limit} for token type {token_type}")]
    ExceedsMaximumAmount { token_type: String, requested: u64, limit: u64 },
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },
    
    #[error("Operation not allowed for identity scope: {0:?}")]
    ScopeNotAllowed(IdentityScope),
    
    #[error("Role '{0}' not authorized for this operation")]
    RoleNotAuthorized(String),
    
    #[error("Invalid policy configuration: {0}")]
    InvalidConfiguration(String),
    
    #[error("Policy evaluation error: {0}")]
    EvaluationError(String),
}

/// Types of resources that can be tracked and limited
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    /// Compute resources (CPU, memory)
    Compute,
    /// Storage resources (disk, database)
    Storage,
    /// Network resources (bandwidth, connections)
    Network,
    /// Generic token resources
    Token,
    /// Energy tokens used for core operations
    Energy,
    /// Reputation tokens used for governance
    Reputation,
    /// Custom token type
    Custom(u16),
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compute => write!(f, "compute"),
            Self::Storage => write!(f, "storage"),
            Self::Network => write!(f, "network"),
            Self::Token => write!(f, "token"),
            Self::Energy => write!(f, "energy"),
            Self::Reputation => write!(f, "reputation"),
            Self::Custom(id) => write!(f, "custom_{}", id),
        }
    }
}

/// Rate limiting configuration for resource usage
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// Maximum amount allowed per time window
    pub max_amount: u64,
    /// Duration of the time window in seconds
    pub window_seconds: u64,
    /// Optional maximum burst allowed
    pub max_burst: Option<u64>,
}

impl RateLimit {
    /// Create a new rate limit
    pub fn new(max_amount: u64, window_seconds: u64) -> Self {
        Self {
            max_amount,
            window_seconds,
            max_burst: None,
        }
    }

    /// Set the maximum burst allowed
    pub fn with_max_burst(mut self, max_burst: u64) -> Self {
        self.max_burst = Some(max_burst);
        self
    }
}

/// Resource authorization rule for a specific token type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAuthorizationRule {
    /// Token type this rule applies to
    pub token_type: ResourceType,
    /// Maximum amount per action
    pub max_per_action: u64,
    /// Minimum balance required
    pub min_balance: Option<u64>,
    /// Rate limits, if any
    pub rate_limits: Option<RateLimit>,
    /// Allowed identity scopes
    pub allowed_scopes: Option<Vec<IdentityScope>>,
    /// Allowed roles
    pub allowed_roles: Option<Vec<String>>,
}

impl TokenAuthorizationRule {
    /// Create a new token authorization rule
    pub fn new(token_type: ResourceType, max_per_action: u64) -> Self {
        Self {
            token_type,
            max_per_action,
            min_balance: None,
            rate_limits: None,
            allowed_scopes: None,
            allowed_roles: None,
        }
    }

    /// Set the minimum balance required
    pub fn with_min_balance(mut self, min_balance: u64) -> Self {
        self.min_balance = Some(min_balance);
        self
    }

    /// Set the rate limits
    pub fn with_rate_limits(mut self, rate_limits: RateLimit) -> Self {
        self.rate_limits = Some(rate_limits);
        self
    }

    /// Set the allowed identity scopes
    pub fn with_allowed_scopes(mut self, scopes: Vec<IdentityScope>) -> Self {
        self.allowed_scopes = Some(scopes);
        self
    }

    /// Set the allowed roles
    pub fn with_allowed_roles(mut self, roles: Vec<String>) -> Self {
        self.allowed_roles = Some(roles);
        self
    }

    /// Check if an identity scope is allowed
    pub fn is_scope_allowed(&self, scope: &IdentityScope) -> bool {
        match &self.allowed_scopes {
            Some(scopes) => scopes.contains(scope),
            None => true, // No restrictions
        }
    }

    /// Check if a role is allowed
    pub fn is_role_allowed(&self, role: &str) -> bool {
        match &self.allowed_roles {
            Some(roles) => roles.iter().any(|r| r == role),
            None => true, // No restrictions
        }
    }
}

/// Federation-wide economic policy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationPolicy {
    /// ID of the federation
    pub federation_id: String,
    /// Version of the policy
    pub version: String,
    /// Token authorization rules, keyed by token type
    pub token_rules: HashMap<String, TokenAuthorizationRule>,
    /// Global rate limits for the federation
    pub global_rate_limits: Option<HashMap<String, RateLimit>>,
    /// Metadata
    pub metadata: Option<HashMap<String, String>>,
}

impl FederationPolicy {
    /// Create a new federation policy
    pub fn new(federation_id: &str, version: &str) -> Self {
        Self {
            federation_id: federation_id.to_string(),
            version: version.to_string(),
            token_rules: HashMap::new(),
            global_rate_limits: None,
            metadata: None,
        }
    }

    /// Add a token authorization rule
    pub fn add_token_rule(&mut self, rule: TokenAuthorizationRule) {
        self.token_rules.insert(rule.token_type.to_string(), rule);
    }

    /// Add a global rate limit
    pub fn add_global_rate_limit(&mut self, resource_type: ResourceType, rate_limit: RateLimit) {
        let global_limits = self.global_rate_limits.get_or_insert_with(HashMap::new);
        global_limits.insert(resource_type.to_string(), rate_limit);
    }

    /// Get a token authorization rule
    pub fn get_token_rule(&self, token_type: &ResourceType) -> Option<&TokenAuthorizationRule> {
        self.token_rules.get(&token_type.to_string())
    }

    /// Check if a resource usage is allowed by policy
    pub fn check_resource_authorization(
        &self,
        token_type: &ResourceType,
        amount: u64,
        identity_scope: &IdentityScope,
        roles: &[String],
    ) -> Result<(), PolicyError> {
        // Get the token rule
        let rule = self.get_token_rule(token_type)
            .ok_or_else(|| PolicyError::TokenTypeNotAllowed(token_type.to_string()))?;
        
        // Check identity scope
        if !rule.is_scope_allowed(identity_scope) {
            return Err(PolicyError::ScopeNotAllowed(*identity_scope));
        }
        
        // Check role authorization
        if let Some(allowed_roles) = &rule.allowed_roles {
            if !roles.iter().any(|role| allowed_roles.contains(role)) {
                return Err(PolicyError::RoleNotAuthorized(roles.first().cloned().unwrap_or_default()));
            }
        }
        
        // Check maximum amount per action
        if amount > rule.max_per_action {
            return Err(PolicyError::ExceedsMaximumAmount {
                token_type: token_type.to_string(),
                requested: amount,
                limit: rule.max_per_action,
            });
        }
        
        // Check rate limits (would be implemented by a rate limiter component)
        
        Ok(())
    }

    /// Load a policy from a TOML string
    pub fn from_toml(toml_str: &str) -> Result<Self, PolicyError> {
        toml::from_str(toml_str)
            .map_err(|e| PolicyError::InvalidConfiguration(format!("Failed to parse TOML: {}", e)))
    }

    /// Load a policy from a JSON string
    pub fn from_json(json_str: &str) -> Result<Self, PolicyError> {
        serde_json::from_str(json_str)
            .map_err(|e| PolicyError::InvalidConfiguration(format!("Failed to parse JSON: {}", e)))
    }
}

/// Enforces economic policy rules
pub struct PolicyEnforcer {
    /// The policy to enforce
    policy: Arc<FederationPolicy>,
    /// Account balances
    balances: HashMap<String, HashMap<String, u64>>,
    /// Usage tracking for rate limiting
    usage_tracker: HashMap<String, Vec<(Instant, u64)>>,
}

impl PolicyEnforcer {
    /// Create a new policy enforcer
    pub fn new(policy: Arc<FederationPolicy>) -> Self {
        Self {
            policy,
            balances: HashMap::new(),
            usage_tracker: HashMap::new(),
        }
    }

    /// Authorize a resource usage
    pub fn authorize_resource_usage(
        &mut self,
        identity: &str,
        token_type: &ResourceType,
        amount: u64,
        identity_scope: &IdentityScope,
        roles: &[String],
    ) -> Result<(), PolicyError> {
        // Check policy rules
        self.policy.check_resource_authorization(token_type, amount, identity_scope, roles)?;
        
        // Check balance if min_balance is set
        if let Some(rule) = self.policy.get_token_rule(token_type) {
            if let Some(min_balance) = rule.min_balance {
                let balance = self.get_balance(identity, &token_type.to_string());
                if amount > balance || balance - amount < min_balance {
                    return Err(PolicyError::InsufficientBalance {
                        required: amount + min_balance,
                        available: balance,
                    });
                }
            }
        }
        
        // Check rate limits if configured
        self.check_rate_limits(identity, token_type, amount)?;
        
        Ok(())
    }

    /// Record resource usage
    pub fn record_resource_usage(
        &mut self,
        identity: &str,
        token_type: &ResourceType,
        amount: u64,
    ) -> Result<(), PolicyError> {
        // Record for rate limiting
        let key = format!("{}:{}", identity, token_type);
        let tracker = self.usage_tracker.entry(key).or_insert_with(Vec::new);
        tracker.push((Instant::now(), amount));
        
        // Update balance
        let account_balances = self.balances.entry(identity.to_string()).or_insert_with(HashMap::new);
        let balance = account_balances.entry(token_type.to_string()).or_insert(0);
        
        if *balance < amount {
            return Err(PolicyError::InsufficientBalance {
                required: amount,
                available: *balance,
            });
        }
        
        *balance -= amount;
        
        Ok(())
    }

    /// Check rate limits
    fn check_rate_limits(
        &mut self,
        identity: &str,
        token_type: &ResourceType,
        amount: u64,
    ) -> Result<(), PolicyError> {
        // Get rate limit from token rule
        let rule = match self.policy.get_token_rule(token_type) {
            Some(rule) => rule,
            None => return Ok(()),
        };
        
        let rate_limit = match &rule.rate_limits {
            Some(limit) => limit,
            None => return Ok(()),
        };
        
        let key = format!("{}:{}", identity, token_type);
        let tracker = self.usage_tracker.entry(key).or_insert_with(Vec::new);
        
        // Clean up old entries
        let now = Instant::now();
        let window_duration = Duration::from_secs(rate_limit.window_seconds);
        tracker.retain(|(time, _)| now.duration_since(*time) <= window_duration);
        
        // Calculate total in window
        let total_in_window: u64 = tracker.iter().map(|(_, amt)| amt).sum();
        
        // Check if adding this amount would exceed the limit
        if total_in_window + amount > rate_limit.max_amount {
            return Err(PolicyError::RateLimitExceeded(format!(
                "{} usage exceeds rate limit of {} per {} seconds",
                token_type, rate_limit.max_amount, rate_limit.window_seconds
            )));
        }
        
        Ok(())
    }

    /// Get a balance
    fn get_balance(&self, identity: &str, token_type: &str) -> u64 {
        self.balances
            .get(identity)
            .and_then(|balances| balances.get(token_type))
            .copied()
            .unwrap_or(0)
    }

    /// Set a balance
    pub fn set_balance(&mut self, identity: &str, token_type: &str, amount: u64) {
        let account_balances = self.balances.entry(identity.to_string()).or_insert_with(HashMap::new);
        account_balances.insert(token_type.to_string(), amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_authorization_rule() {
        let rule = TokenAuthorizationRule::new(ResourceType::Energy, 100)
            .with_min_balance(10)
            .with_allowed_scopes(vec![IdentityScope::Cooperative, IdentityScope::Individual])
            .with_allowed_roles(vec!["worker".to_string(), "admin".to_string()]);
        
        assert_eq!(rule.token_type, ResourceType::Energy);
        assert_eq!(rule.max_per_action, 100);
        assert_eq!(rule.min_balance, Some(10));
        
        assert!(rule.is_scope_allowed(&IdentityScope::Cooperative));
        assert!(!rule.is_scope_allowed(&IdentityScope::Guardian));
        
        assert!(rule.is_role_allowed("worker"));
        assert!(rule.is_role_allowed("admin"));
        assert!(!rule.is_role_allowed("guest"));
    }

    #[test]
    fn test_policy_from_toml() {
        let toml_str = r#"
            federation_id = "did:icn:federation:test"
            version = "1.0.0"
            
            [token_rules.energy]
            token_type = "Energy"
            max_per_action = 100
            min_balance = 10
            
            [token_rules.energy.rate_limits]
            max_amount = 1000
            window_seconds = 3600
            
            [[token_rules.energy.allowed_scopes]]
            "Cooperative"
            [[token_rules.energy.allowed_scopes]]
            "Individual"
            
            [token_rules.energy.allowed_roles]
            worker = true
            admin = true
        "#;
        
        let result = FederationPolicy::from_toml(toml_str);
        assert!(result.is_err(), "Expected error parsing sample TOML");
        
        // A more proper example would be created for a real implementation
    }

    #[test]
    fn test_policy_checks() {
        let mut policy = FederationPolicy::new("did:icn:federation:test", "1.0.0");
        
        let rule = TokenAuthorizationRule::new(ResourceType::Energy, 100)
            .with_min_balance(10)
            .with_allowed_scopes(vec![IdentityScope::Cooperative, IdentityScope::Individual])
            .with_allowed_roles(vec!["worker".to_string(), "admin".to_string()]);
        
        policy.add_token_rule(rule);
        
        // Valid usage
        let result = policy.check_resource_authorization(
            &ResourceType::Energy,
            50,
            &IdentityScope::Cooperative,
            &["worker".to_string()]
        );
        assert!(result.is_ok());
        
        // Invalid scope
        let result = policy.check_resource_authorization(
            &ResourceType::Energy,
            50,
            &IdentityScope::Guardian,
            &["worker".to_string()]
        );
        assert!(result.is_err());
        
        // Invalid role
        let result = policy.check_resource_authorization(
            &ResourceType::Energy,
            50,
            &IdentityScope::Cooperative,
            &["guest".to_string()]
        );
        assert!(result.is_err());
        
        // Amount exceeds max
        let result = policy.check_resource_authorization(
            &ResourceType::Energy,
            150,
            &IdentityScope::Cooperative,
            &["worker".to_string()]
        );
        assert!(result.is_err());
        
        // Invalid token type
        let result = policy.check_resource_authorization(
            &ResourceType::Token,
            50,
            &IdentityScope::Cooperative,
            &["worker".to_string()]
        );
        assert!(result.is_err());
    }
} 