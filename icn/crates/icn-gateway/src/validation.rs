//! Input validation for API requests
//!
//! Validates request inputs to prevent DoS attacks via:
//! - Excessively long strings
//! - Resource exhaustion
//! - Invalid formats

use crate::error::{GatewayError, Result};

/// Maximum length for cooperative ID
pub const MAX_COOP_ID_LEN: usize = 64;

/// Maximum length for cooperative name
pub const MAX_COOP_NAME_LEN: usize = 256;

/// Maximum length for currency identifier
pub const MAX_CURRENCY_LEN: usize = 32;

/// Maximum length for transaction memo
pub const MAX_MEMO_LEN: usize = 1024;

/// Maximum length for governance model string
pub const MAX_GOVERNANCE_MODEL_LEN: usize = 64;

/// Maximum length for credit policy string
pub const MAX_CREDIT_POLICY_LEN: usize = 64;

/// Maximum number of members per cooperative
pub const MAX_MEMBERS_PER_COOP: usize = 10_000;

/// Maximum number of cooperatives per gateway instance
/// This prevents unbounded memory growth from cooperative creation DoS
pub const MAX_COOPERATIVES: usize = 1_000;

/// Maximum number of WebSocket subscribers per cooperative
/// This prevents unbounded memory growth from subscription DoS
pub const MAX_SUBSCRIBERS_PER_COOP: usize = 1_000;

/// Maximum number of scopes in a token request
pub const MAX_SCOPES: usize = 30;

/// Allowed scopes for gateway tokens
/// These are the only scopes that can be requested during authentication
pub const ALLOWED_SCOPES: &[&str] = &[
    // Ledger operations
    "ledger:read",
    "ledger:write",
    // Cooperative operations
    "coop:read",
    "coop:write",
    "coop:admin",
    // Governance operations
    "gov:read",
    "gov:write",
    "governance:read",
    // Payment operations
    "payments:read",
    "payments:write",
    // Federation operations
    "federation:read",
    "federation:write",
    "federation:admin",
    // Compute operations
    "compute:read",
    "compute:write",
    // Constitutional operations
    "constitutional:read",
    "constitutional:write",
    "constitutional:admin",
    // Entity operations
    "entity:read",
    "entity:write",
    "entity:audit",
    // Admin operations
    "admin",
];

/// Maximum payment amount (prevent overflow and unrealistic values)
/// Set to 1 trillion to allow large legitimate transactions while preventing abuse
pub const MAX_PAYMENT_AMOUNT: i64 = 1_000_000_000_000;

/// Maximum number of history entries to return per request
/// This prevents OOM from loading millions of transactions into memory
pub const MAX_HISTORY_LIMIT: usize = 1_000;

/// Default number of history entries to return if not specified
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// Maximum WebSocket message size (64KB)
/// Prevents memory exhaustion from extremely large JSON payloads
pub const MAX_WEBSOCKET_MESSAGE_SIZE: usize = 65_536;

/// Maximum total active WebSocket connections across all cooperatives
/// Prevents resource exhaustion from unlimited connection attacks
/// With MAX_SUBSCRIBERS_PER_COOP=1,000 and MAX_COOPERATIVES=1,000,
/// theoretical max is 1 million, but we limit to 10,000 total connections
pub const MAX_TOTAL_WEBSOCKET_CONNECTIONS: u64 = 10_000;

/// Maximum length for governance domain ID
pub const MAX_DOMAIN_ID_LEN: usize = 128;

/// Maximum length for governance domain name
pub const MAX_DOMAIN_NAME_LEN: usize = 256;

/// Maximum length for proposal title
pub const MAX_PROPOSAL_TITLE_LEN: usize = 256;

/// Maximum length for proposal description
pub const MAX_PROPOSAL_DESCRIPTION_LEN: usize = 10_000;

/// Maximum length for vote comment
pub const MAX_VOTE_COMMENT_LEN: usize = 2_000;

/// Maximum number of members in governance domain
pub const MAX_DOMAIN_MEMBERS: usize = 10_000;

/// Maximum voting period (1 year in seconds = 365 * 24 * 3600)
pub const MAX_VOTING_PERIOD_SECONDS: u64 = 31_536_000;

/// Validate governance domain ID
pub fn validate_domain_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(GatewayError::BadRequest(
            "Domain ID cannot be empty".to_string(),
        ));
    }

    if id.len() > MAX_DOMAIN_ID_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Domain ID exceeds maximum length of {MAX_DOMAIN_ID_LEN} characters"
        )));
    }

    // Validate characters (alphanumeric, hyphens, underscores, colons for namespacing)
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':')
    {
        return Err(GatewayError::BadRequest(
            "Domain ID must contain only alphanumeric characters, hyphens, underscores, and colons"
                .to_string(),
        ));
    }

    Ok(())
}

/// Validate governance domain name
pub fn validate_domain_name(name: &str) -> Result<()> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Domain name cannot be empty or whitespace-only".to_string(),
        ));
    }

    if name.len() > MAX_DOMAIN_NAME_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Domain name exceeds maximum length of {MAX_DOMAIN_NAME_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate cooperative ID
pub fn validate_coop_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(GatewayError::BadRequest(
            "Cooperative ID cannot be empty".to_string(),
        ));
    }

    if id.len() > MAX_COOP_ID_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Cooperative ID exceeds maximum length of {MAX_COOP_ID_LEN} characters"
        )));
    }

    // Validate characters (alphanumeric, hyphens, underscores)
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(GatewayError::BadRequest(
            "Cooperative ID must contain only alphanumeric characters, hyphens, and underscores"
                .to_string(),
        ));
    }

    Ok(())
}

/// Validate cooperative name
pub fn validate_coop_name(name: &str) -> Result<()> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Cooperative name cannot be empty or whitespace-only".to_string(),
        ));
    }

    if name.len() > MAX_COOP_NAME_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Cooperative name exceeds maximum length of {MAX_COOP_NAME_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate currency identifier
pub fn validate_currency(currency: &str) -> Result<()> {
    if currency.is_empty() || currency.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Currency cannot be empty or whitespace-only".to_string(),
        ));
    }

    if currency.len() > MAX_CURRENCY_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Currency identifier exceeds maximum length of {MAX_CURRENCY_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate optional memo field
pub fn validate_memo(memo: &Option<String>) -> Result<()> {
    if let Some(memo_text) = memo {
        if memo_text.len() > MAX_MEMO_LEN {
            return Err(GatewayError::BadRequest(format!(
                "Memo exceeds maximum length of {MAX_MEMO_LEN} characters"
            )));
        }
    }

    Ok(())
}

/// Validate governance model string
pub fn validate_governance_model(model: &str) -> Result<()> {
    if model.is_empty() || model.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Governance model cannot be empty or whitespace-only".to_string(),
        ));
    }

    if model.len() > MAX_GOVERNANCE_MODEL_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Governance model exceeds maximum length of {MAX_GOVERNANCE_MODEL_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate credit policy string
pub fn validate_credit_policy(policy: &str) -> Result<()> {
    if policy.is_empty() || policy.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Credit policy cannot be empty or whitespace-only".to_string(),
        ));
    }

    if policy.len() > MAX_CREDIT_POLICY_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Credit policy exceeds maximum length of {MAX_CREDIT_POLICY_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate role string (for invite system)
pub fn validate_role(role: &str) -> Result<()> {
    if role.is_empty() || role.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Role cannot be empty or whitespace-only".to_string(),
        ));
    }

    if role.len() > 32 {
        return Err(GatewayError::BadRequest(
            "Role exceeds maximum length of 32 characters".to_string(),
        ));
    }

    // Optional: Validate against known roles
    let valid_roles = ["member", "admin", "participant", "facilitator"];
    if !valid_roles.contains(&role) {
        return Err(GatewayError::BadRequest(format!(
            "Invalid role '{}'. Must be one of: {}",
            role,
            valid_roles.join(", ")
        )));
    }

    Ok(())
}

/// Validate member count doesn't exceed limit
pub fn validate_member_count(current_count: usize) -> Result<()> {
    if current_count >= MAX_MEMBERS_PER_COOP {
        return Err(GatewayError::BadRequest(format!(
            "Cooperative has reached maximum member limit of {MAX_MEMBERS_PER_COOP}"
        )));
    }

    Ok(())
}

/// Validate scopes list
pub fn validate_scopes(scopes: &[String]) -> Result<()> {
    if scopes.len() > MAX_SCOPES {
        return Err(GatewayError::BadRequest(format!(
            "Number of scopes exceeds maximum of {MAX_SCOPES}"
        )));
    }

    // Check that all requested scopes are in the allowlist
    for scope in scopes {
        if !ALLOWED_SCOPES.contains(&scope.as_str()) {
            return Err(GatewayError::BadRequest(format!(
                "Invalid scope: '{}'. Allowed scopes are: {}",
                scope,
                ALLOWED_SCOPES.join(", ")
            )));
        }
    }

    Ok(())
}

/// Validate payment amount
pub fn validate_payment_amount(amount: i64) -> Result<()> {
    if amount <= 0 {
        return Err(GatewayError::BadRequest(
            "Amount must be positive".to_string(),
        ));
    }

    if amount > MAX_PAYMENT_AMOUNT {
        return Err(GatewayError::BadRequest(format!(
            "Amount exceeds maximum of {MAX_PAYMENT_AMOUNT}"
        )));
    }

    Ok(())
}

/// Validate cooperative count doesn't exceed global limit
pub fn validate_coop_count(current_count: usize) -> Result<()> {
    if current_count >= MAX_COOPERATIVES {
        return Err(GatewayError::BadRequest(format!(
            "Gateway has reached maximum cooperative limit of {MAX_COOPERATIVES}"
        )));
    }

    Ok(())
}

/// Validate history limit parameter
pub fn validate_history_limit(limit: usize) -> Result<usize> {
    if limit == 0 {
        return Err(GatewayError::BadRequest(
            "Limit must be greater than 0".to_string(),
        ));
    }

    if limit > MAX_HISTORY_LIMIT {
        return Err(GatewayError::BadRequest(format!(
            "Limit exceeds maximum of {MAX_HISTORY_LIMIT}"
        )));
    }

    Ok(limit)
}

/// Validate history offset parameter
/// Prevents integer overflow attacks in pagination arithmetic
pub fn validate_history_offset(offset: usize) -> Result<usize> {
    // Maximum safe offset to prevent overflow when added to MAX_HISTORY_LIMIT
    // Using usize::MAX / 2 as a reasonable upper bound (still allows huge offsets)
    const MAX_HISTORY_OFFSET: usize = usize::MAX / 2;

    if offset > MAX_HISTORY_OFFSET {
        return Err(GatewayError::BadRequest(
            "Offset exceeds maximum allowed value".to_string(),
        ));
    }

    Ok(offset)
}

/// Validate proposal title
pub fn validate_proposal_title(title: &str) -> Result<()> {
    if title.is_empty() || title.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Proposal title cannot be empty or whitespace-only".to_string(),
        ));
    }

    if title.len() > MAX_PROPOSAL_TITLE_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Proposal title exceeds maximum length of {MAX_PROPOSAL_TITLE_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate proposal description
pub fn validate_proposal_description(description: &str) -> Result<()> {
    if description.is_empty() || description.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Proposal description cannot be empty or whitespace-only".to_string(),
        ));
    }

    if description.len() > MAX_PROPOSAL_DESCRIPTION_LEN {
        return Err(GatewayError::BadRequest(
            format!("Proposal description exceeds maximum length of {MAX_PROPOSAL_DESCRIPTION_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate vote comment
pub fn validate_vote_comment(comment: &Option<String>) -> Result<()> {
    if let Some(comment_text) = comment {
        // Allow empty comments (user may choose to not provide one)
        // But reject whitespace-only comments as they provide no value
        if !comment_text.is_empty() && comment_text.trim().is_empty() {
            return Err(GatewayError::BadRequest(
                "Vote comment cannot be whitespace-only".to_string(),
            ));
        }

        if comment_text.len() > MAX_VOTE_COMMENT_LEN {
            return Err(GatewayError::BadRequest(format!(
                "Vote comment exceeds maximum length of {MAX_VOTE_COMMENT_LEN} characters"
            )));
        }
    }

    Ok(())
}

/// Validate domain members list
pub fn validate_domain_members(members: &[String]) -> Result<()> {
    if members.is_empty() {
        return Err(GatewayError::BadRequest(
            "Domain must have at least one member".to_string(),
        ));
    }

    if members.len() > MAX_DOMAIN_MEMBERS {
        return Err(GatewayError::BadRequest(format!(
            "Number of members exceeds maximum of {MAX_DOMAIN_MEMBERS}"
        )));
    }

    // Check for duplicates (prevents quorum calculation bugs)
    let mut seen = std::collections::HashSet::new();
    for member in members {
        if !seen.insert(member) {
            return Err(GatewayError::BadRequest(format!(
                "Duplicate member DID not allowed: {member}"
            )));
        }
    }

    Ok(())
}

/// Validate governance parameters
pub fn validate_governance_params(
    quorum_percent: u8,
    approval_percent: u8,
    voting_period_seconds: u64,
) -> Result<()> {
    // Quorum percentage must be 0-100
    if quorum_percent > 100 {
        return Err(GatewayError::BadRequest(
            "Quorum percentage must be between 0 and 100".to_string(),
        ));
    }

    // Approval percentage must be 0-100
    if approval_percent > 100 {
        return Err(GatewayError::BadRequest(
            "Approval percentage must be between 0 and 100".to_string(),
        ));
    }

    // Voting period must be reasonable (not zero, not more than 1 year)
    if voting_period_seconds == 0 {
        return Err(GatewayError::BadRequest(
            "Voting period must be greater than 0".to_string(),
        ));
    }

    if voting_period_seconds > MAX_VOTING_PERIOD_SECONDS {
        return Err(GatewayError::BadRequest(format!(
            "Voting period exceeds maximum of {MAX_VOTING_PERIOD_SECONDS} seconds (1 year)"
        )));
    }

    Ok(())
}

// === Federation Proposal Validation (Issue #518) ===

/// Maximum length for federation ID
pub const MAX_FEDERATION_ID_LEN: usize = 128;

/// Maximum length for reason strings (leave, terminate, revoke)
pub const MAX_REASON_LEN: usize = 2000;

/// Maximum length for evidence string
pub const MAX_EVIDENCE_LEN: usize = 5000;

/// Maximum length for context string
pub const MAX_CONTEXT_LEN: usize = 256;

/// Maximum grace period in days
pub const MAX_GRACE_PERIOD_DAYS: u32 = 365;

/// Validate federation ID
pub fn validate_federation_id(id: &str) -> Result<()> {
    if id.is_empty() || id.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Federation ID cannot be empty".to_string(),
        ));
    }

    if id.len() > MAX_FEDERATION_ID_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Federation ID exceeds maximum length of {MAX_FEDERATION_ID_LEN} characters"
        )));
    }

    // Validate characters (alphanumeric, hyphens, underscores, colons)
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':')
    {
        return Err(GatewayError::BadRequest(
            "Federation ID must contain only alphanumeric characters, hyphens, underscores, and colons"
                .to_string(),
        ));
    }

    Ok(())
}

/// Validate trust score (must be in [0.0, 1.0])
pub fn validate_trust_score(score: f64) -> Result<()> {
    if !score.is_finite() {
        return Err(GatewayError::BadRequest(
            "Trust score must be a finite number".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&score) {
        return Err(GatewayError::BadRequest(
            "Trust score must be between 0.0 and 1.0".to_string(),
        ));
    }
    Ok(())
}

/// Validate grace period in days (must be 1-365)
pub fn validate_grace_period_days(days: u32) -> Result<()> {
    if days == 0 {
        return Err(GatewayError::BadRequest(
            "Grace period must be at least 1 day".to_string(),
        ));
    }

    if days > MAX_GRACE_PERIOD_DAYS {
        return Err(GatewayError::BadRequest(format!(
            "Grace period cannot exceed {MAX_GRACE_PERIOD_DAYS} days"
        )));
    }

    Ok(())
}

/// Validate settlement interval string and return parsed enum
pub fn validate_settlement_interval(interval: &str) -> Result<String> {
    match interval.to_lowercase().as_str() {
        "daily" | "weekly" | "monthly" | "manual" => Ok(interval.to_lowercase()),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid settlement interval: '{interval}'. Valid values: daily, weekly, monthly, manual"
        ))),
    }
}

/// Validate data sharing level string and return parsed enum
pub fn validate_data_sharing_level(level: &str) -> Result<String> {
    match level.to_lowercase().as_str() {
        "none" | "metadata_only" | "full" => Ok(level.to_lowercase()),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid data sharing level: '{level}'. Valid values: none, metadata_only, full"
        ))),
    }
}

/// Validate dispute resolution string and return normalized version
pub fn validate_dispute_resolution(method: &str) -> Result<String> {
    let lower = method.to_lowercase();
    match lower.as_str() {
        "federation_mediation" | "federation_vote" => Ok(lower),
        s if s.starts_with("arbitrator:") => {
            let arbitrator_id = s.strip_prefix("arbitrator:").unwrap_or("");
            if arbitrator_id.is_empty() || arbitrator_id.trim().is_empty() {
                return Err(GatewayError::BadRequest(
                    "Arbitrator ID cannot be empty in dispute resolution".to_string(),
                ));
            }
            Ok(method.to_string())
        }
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid dispute resolution: '{method}'. Valid values: federation_mediation, federation_vote, arbitrator:<coop_id>"
        ))),
    }
}

/// Validate reason string (for leave, terminate, revoke)
pub fn validate_reason(reason: &str) -> Result<()> {
    if reason.is_empty() || reason.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Reason cannot be empty".to_string(),
        ));
    }

    if reason.len() > MAX_REASON_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Reason exceeds maximum length of {MAX_REASON_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate context string (for vouch)
pub fn validate_context(context: &str) -> Result<()> {
    if context.is_empty() || context.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "Context cannot be empty".to_string(),
        ));
    }

    if context.len() > MAX_CONTEXT_LEN {
        return Err(GatewayError::BadRequest(format!(
            "Context exceeds maximum length of {MAX_CONTEXT_LEN} characters"
        )));
    }

    Ok(())
}

/// Validate optional evidence string
pub fn validate_evidence(evidence: &Option<String>) -> Result<()> {
    if let Some(ev) = evidence {
        if ev.is_empty() || ev.trim().is_empty() {
            return Err(GatewayError::BadRequest(
                "Evidence cannot be empty if provided".to_string(),
            ));
        }
        if ev.len() > MAX_EVIDENCE_LEN {
            return Err(GatewayError::BadRequest(format!(
                "Evidence exceeds maximum length of {MAX_EVIDENCE_LEN} characters"
            )));
        }
    }

    Ok(())
}

/// Validate max imbalance (must be positive)
pub fn validate_max_imbalance(max_imbalance: i64) -> Result<()> {
    if max_imbalance <= 0 {
        return Err(GatewayError::BadRequest(
            "Max imbalance must be positive".to_string(),
        ));
    }

    Ok(())
}

/// Validate auto-accept vouch threshold (-1.0 to disable, or 0.0-1.0)
pub fn validate_auto_accept_threshold(threshold: Option<f64>) -> Result<()> {
    if let Some(t) = threshold {
        if !t.is_finite() {
            return Err(GatewayError::BadRequest(
                "Auto-accept threshold must be a finite number".to_string(),
            ));
        }
        // -1.0 means disabled, otherwise must be in [0.0, 1.0]
        if t != -1.0 && !(0.0..=1.0).contains(&t) {
            return Err(GatewayError::BadRequest(
                "Auto-accept threshold must be -1.0 (disabled) or between 0.0 and 1.0".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate trust decay factor (0.0-1.0)
pub fn validate_trust_decay_factor(factor: Option<f64>) -> Result<()> {
    if let Some(f) = factor {
        validate_trust_score(f)?;
    }
    Ok(())
}

/// Validate max attestations per minute (must be > 0)
pub fn validate_max_attestations_per_minute(rate: Option<u32>) -> Result<()> {
    if let Some(r) = rate {
        if r == 0 {
            return Err(GatewayError::BadRequest(
                "Max attestations per minute must be greater than 0".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_coop_id() {
        // Valid IDs
        assert!(validate_coop_id("test-coop").is_ok());
        assert!(validate_coop_id("coop123").is_ok());
        assert!(validate_coop_id("my_coop_2024").is_ok());

        // Invalid IDs
        assert!(validate_coop_id("").is_err()); // Empty
        assert!(validate_coop_id("a".repeat(65).as_str()).is_err()); // Too long
        assert!(validate_coop_id("test coop").is_err()); // Space
        assert!(validate_coop_id("test@coop").is_err()); // Special char
    }

    #[test]
    fn test_validate_coop_name() {
        assert!(validate_coop_name("Test Cooperative").is_ok());
        assert!(validate_coop_name("").is_err()); // Empty
        assert!(validate_coop_name("   ").is_err()); // Whitespace-only
        assert!(validate_coop_name(&"a".repeat(257)).is_err()); // Too long
    }

    #[test]
    fn test_validate_currency() {
        assert!(validate_currency("hours").is_ok());
        assert!(validate_currency("USD").is_ok());
        assert!(validate_currency("").is_err()); // Empty
        assert!(validate_currency("   ").is_err()); // Whitespace-only
        assert!(validate_currency(&"a".repeat(33)).is_err()); // Too long
    }

    #[test]
    fn test_validate_memo() {
        assert!(validate_memo(&None).is_ok());
        assert!(validate_memo(&Some("Test memo".to_string())).is_ok());
        assert!(validate_memo(&Some("a".repeat(1024))).is_ok());
        assert!(validate_memo(&Some("a".repeat(1025))).is_err()); // Too long
    }

    #[test]
    fn test_validate_member_count() {
        assert!(validate_member_count(0).is_ok());
        assert!(validate_member_count(9_999).is_ok());
        assert!(validate_member_count(10_000).is_err()); // At limit
    }

    #[test]
    fn test_validate_scopes() {
        // Valid scopes
        assert!(validate_scopes(&["ledger:read".to_string()]).is_ok());
        assert!(validate_scopes(&["ledger:write".to_string(), "coop:read".to_string()]).is_ok());

        // Valid admin scope (now allowed)
        assert!(validate_scopes(&["admin".to_string()]).is_ok());

        // Invalid scope
        assert!(validate_scopes(&["invalid:scope".to_string()]).is_err());
        assert!(validate_scopes(&["not:allowed".to_string()]).is_err());

        // Too many valid scopes
        let many_scopes: Vec<String> = (0..30).map(|_| "ledger:read".to_string()).collect();
        assert!(validate_scopes(&many_scopes).is_ok());

        let too_many_scopes: Vec<String> = (0..31).map(|_| "ledger:read".to_string()).collect();
        assert!(validate_scopes(&too_many_scopes).is_err());
    }

    #[test]
    fn test_validate_governance_model() {
        assert!(validate_governance_model("consensus").is_ok());
        assert!(validate_governance_model("majority").is_ok());
        assert!(validate_governance_model(&"a".repeat(MAX_GOVERNANCE_MODEL_LEN)).is_ok());
        assert!(validate_governance_model("").is_err()); // Empty
        assert!(validate_governance_model("   ").is_err()); // Whitespace-only
        assert!(validate_governance_model(&"a".repeat(MAX_GOVERNANCE_MODEL_LEN + 1)).is_err());
        // Too long
    }

    #[test]
    fn test_validate_credit_policy() {
        assert!(validate_credit_policy("conservative").is_ok());
        assert!(validate_credit_policy("permissive").is_ok());
        assert!(validate_credit_policy("").is_err()); // Empty
        assert!(validate_credit_policy("   ").is_err()); // Whitespace-only
        assert!(validate_credit_policy(&"a".repeat(65)).is_err()); // Too long
    }

    #[test]
    fn test_validate_payment_amount() {
        assert!(validate_payment_amount(1).is_ok());
        assert!(validate_payment_amount(1000).is_ok());
        assert!(validate_payment_amount(MAX_PAYMENT_AMOUNT).is_ok());
        assert!(validate_payment_amount(0).is_err()); // Zero
        assert!(validate_payment_amount(-1).is_err()); // Negative
        assert!(validate_payment_amount(MAX_PAYMENT_AMOUNT + 1).is_err()); // Too large
        assert!(validate_payment_amount(i64::MAX).is_err()); // Way too large
    }

    #[test]
    fn test_validate_coop_count() {
        assert!(validate_coop_count(0).is_ok());
        assert!(validate_coop_count(500).is_ok());
        assert!(validate_coop_count(999).is_ok());
        assert!(validate_coop_count(MAX_COOPERATIVES).is_err()); // At limit
        assert!(validate_coop_count(MAX_COOPERATIVES + 1).is_err()); // Over limit
    }

    #[test]
    fn test_validate_history_limit() {
        assert!(validate_history_limit(1).is_ok());
        assert!(validate_history_limit(100).is_ok());
        assert!(validate_history_limit(MAX_HISTORY_LIMIT).is_ok());
        assert!(validate_history_limit(0).is_err()); // Zero
        assert!(validate_history_limit(MAX_HISTORY_LIMIT + 1).is_err()); // Too large
    }

    #[test]
    fn test_validate_history_offset() {
        assert!(validate_history_offset(0).is_ok());
        assert!(validate_history_offset(1000).is_ok());
        assert!(validate_history_offset(1_000_000).is_ok());
        assert!(validate_history_offset(usize::MAX / 2).is_ok()); // At limit
        assert!(validate_history_offset(usize::MAX / 2 + 1).is_err()); // Over limit
        assert!(validate_history_offset(usize::MAX).is_err()); // Way over limit
    }

    #[test]
    fn test_validate_proposal_title() {
        assert!(validate_proposal_title("Test Proposal").is_ok());
        assert!(validate_proposal_title("x").is_ok());
        assert!(validate_proposal_title(&"a".repeat(MAX_PROPOSAL_TITLE_LEN)).is_ok());
        assert!(validate_proposal_title("").is_err()); // Empty
        assert!(validate_proposal_title("   ").is_err()); // Whitespace-only
        assert!(validate_proposal_title("\t\n  ").is_err()); // Whitespace-only (tabs/newlines)
        assert!(validate_proposal_title(&"a".repeat(MAX_PROPOSAL_TITLE_LEN + 1)).is_err());
        // Too long
    }

    #[test]
    fn test_validate_proposal_description() {
        assert!(validate_proposal_description("Test description").is_ok());
        assert!(validate_proposal_description(&"a".repeat(MAX_PROPOSAL_DESCRIPTION_LEN)).is_ok());
        assert!(validate_proposal_description("").is_err()); // Empty
        assert!(validate_proposal_description("   ").is_err()); // Whitespace-only
        assert!(
            validate_proposal_description(&"a".repeat(MAX_PROPOSAL_DESCRIPTION_LEN + 1)).is_err()
        ); // Too long
    }

    #[test]
    fn test_validate_vote_comment() {
        assert!(validate_vote_comment(&None).is_ok());
        assert!(validate_vote_comment(&Some("Great proposal!".to_string())).is_ok());
        assert!(validate_vote_comment(&Some("a".repeat(MAX_VOTE_COMMENT_LEN))).is_ok());
        assert!(validate_vote_comment(&Some("".to_string())).is_ok()); // Empty is allowed
        assert!(validate_vote_comment(&Some("   ".to_string())).is_err()); // Whitespace-only rejected
        assert!(validate_vote_comment(&Some("a".repeat(MAX_VOTE_COMMENT_LEN + 1))).is_err());
        // Too long
    }

    #[test]
    fn test_validate_domain_name() {
        assert!(validate_domain_name("Food Coop").is_ok());
        assert!(validate_domain_name("x").is_ok());
        assert!(validate_domain_name(&"a".repeat(MAX_DOMAIN_NAME_LEN)).is_ok());
        assert!(validate_domain_name("").is_err()); // Empty
        assert!(validate_domain_name("   ").is_err()); // Whitespace-only
        assert!(validate_domain_name("\t\n  ").is_err()); // Whitespace-only (tabs/newlines)
        assert!(validate_domain_name(&"a".repeat(MAX_DOMAIN_NAME_LEN + 1)).is_err());
        // Too long
    }

    #[test]
    fn test_validate_domain_members() {
        assert!(validate_domain_members(&["did:icn:alice".to_string()]).is_ok());
        assert!(
            validate_domain_members(&["did:icn:alice".to_string(), "did:icn:bob".to_string()])
                .is_ok()
        );
        assert!(validate_domain_members(&[]).is_err()); // Empty
        let too_many: Vec<String> = (0..MAX_DOMAIN_MEMBERS + 1)
            .map(|i| format!("did:icn:{i}"))
            .collect();
        assert!(validate_domain_members(&too_many).is_err()); // Too many

        // Duplicate detection
        let members_with_dup = vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:alice".to_string(), // Duplicate!
        ];
        assert!(validate_domain_members(&members_with_dup).is_err());
    }

    #[test]
    fn test_validate_governance_params() {
        // Valid params
        assert!(validate_governance_params(50, 66, 86400).is_ok());
        assert!(validate_governance_params(0, 0, 1).is_ok());
        assert!(validate_governance_params(100, 100, MAX_VOTING_PERIOD_SECONDS).is_ok());

        // Invalid quorum
        assert!(validate_governance_params(101, 66, 86400).is_err());

        // Invalid approval
        assert!(validate_governance_params(50, 101, 86400).is_err());

        // Invalid voting period (zero)
        assert!(validate_governance_params(50, 66, 0).is_err());

        // Invalid voting period (too long)
        assert!(validate_governance_params(50, 66, MAX_VOTING_PERIOD_SECONDS + 1).is_err());
    }

    // ============================================================================
    // Federation Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_federation_id() {
        // Valid IDs
        assert!(validate_federation_id("test-federation").is_ok());
        assert!(validate_federation_id("regional-food-fed").is_ok());
        assert!(validate_federation_id("coop123").is_ok());

        // Invalid IDs
        assert!(validate_federation_id("").is_err()); // Empty
        assert!(validate_federation_id("   ").is_err()); // Whitespace-only
        assert!(validate_federation_id(&"a".repeat(MAX_FEDERATION_ID_LEN + 1)).is_err()); // Too long
        assert!(validate_federation_id("test federation").is_err()); // Space
        assert!(validate_federation_id("test@federation").is_err()); // Special char
    }

    #[test]
    fn test_validate_trust_score() {
        // Valid scores
        assert!(validate_trust_score(0.0).is_ok());
        assert!(validate_trust_score(0.5).is_ok());
        assert!(validate_trust_score(1.0).is_ok());

        // Invalid scores
        assert!(validate_trust_score(-0.1).is_err()); // Negative
        assert!(validate_trust_score(1.1).is_err()); // Too high
        assert!(validate_trust_score(f64::NAN).is_err()); // NaN
        assert!(validate_trust_score(f64::INFINITY).is_err()); // Infinity
        assert!(validate_trust_score(f64::NEG_INFINITY).is_err()); // Negative infinity
    }

    #[test]
    fn test_validate_grace_period_days() {
        // Valid periods
        assert!(validate_grace_period_days(1).is_ok());
        assert!(validate_grace_period_days(30).is_ok());
        assert!(validate_grace_period_days(365).is_ok());

        // Invalid periods
        assert!(validate_grace_period_days(0).is_err()); // Zero
        assert!(validate_grace_period_days(366).is_err()); // > 365
        assert!(validate_grace_period_days(1000).is_err()); // Way too long
    }

    #[test]
    fn test_validate_settlement_interval() {
        // Valid intervals
        assert!(validate_settlement_interval("daily").is_ok());
        assert!(validate_settlement_interval("DAILY").is_ok()); // Case insensitive
        assert!(validate_settlement_interval("weekly").is_ok());
        assert!(validate_settlement_interval("monthly").is_ok());
        assert!(validate_settlement_interval("manual").is_ok());

        // Returns normalized lowercase
        assert_eq!(
            validate_settlement_interval("WEEKLY").unwrap(),
            "weekly".to_string()
        );

        // Invalid intervals
        assert!(validate_settlement_interval("yearly").is_err());
        assert!(validate_settlement_interval("hourly").is_err());
        assert!(validate_settlement_interval("").is_err());
    }

    #[test]
    fn test_validate_data_sharing_level() {
        // Valid levels
        assert!(validate_data_sharing_level("none").is_ok());
        assert!(validate_data_sharing_level("NONE").is_ok()); // Case insensitive
        assert!(validate_data_sharing_level("metadata_only").is_ok());
        assert!(validate_data_sharing_level("full").is_ok());

        // Returns normalized lowercase
        assert_eq!(
            validate_data_sharing_level("FULL").unwrap(),
            "full".to_string()
        );

        // Invalid levels
        assert!(validate_data_sharing_level("partial").is_err());
        assert!(validate_data_sharing_level("").is_err());
    }

    #[test]
    fn test_validate_dispute_resolution() {
        // Valid methods
        assert!(validate_dispute_resolution("federation_mediation").is_ok());
        assert!(validate_dispute_resolution("FEDERATION_MEDIATION").is_ok()); // Case insensitive
        assert!(validate_dispute_resolution("federation_vote").is_ok());
        assert!(validate_dispute_resolution("arbitrator:neutral-coop").is_ok());

        // Arbitrator with empty ID
        assert!(validate_dispute_resolution("arbitrator:").is_err());
        assert!(validate_dispute_resolution("arbitrator:   ").is_err());

        // Invalid methods
        assert!(validate_dispute_resolution("court").is_err());
        assert!(validate_dispute_resolution("").is_err());
    }

    #[test]
    fn test_validate_reason() {
        // Valid reasons
        assert!(validate_reason("Strategic realignment").is_ok());
        assert!(validate_reason("a").is_ok()); // Single char

        // Invalid reasons
        assert!(validate_reason("").is_err()); // Empty
        assert!(validate_reason("   ").is_err()); // Whitespace-only
        assert!(validate_reason(&"a".repeat(MAX_REASON_LEN + 1)).is_err()); // Too long
    }

    #[test]
    fn test_validate_context() {
        // Valid contexts
        assert!(validate_context("trade").is_ok());
        assert!(validate_context("governance").is_ok());

        // Invalid contexts
        assert!(validate_context("").is_err()); // Empty
        assert!(validate_context("   ").is_err()); // Whitespace-only
        assert!(validate_context(&"a".repeat(MAX_CONTEXT_LEN + 1)).is_err()); // Too long
    }

    #[test]
    fn test_validate_evidence() {
        // Valid evidence
        assert!(validate_evidence(&None).is_ok());
        assert!(validate_evidence(&Some("Supporting documentation".to_string())).is_ok());

        // Invalid evidence
        assert!(validate_evidence(&Some("".to_string())).is_err()); // Empty string
        assert!(validate_evidence(&Some("   ".to_string())).is_err()); // Whitespace-only
        assert!(validate_evidence(&Some("a".repeat(MAX_EVIDENCE_LEN + 1))).is_err());
        // Too long
    }

    #[test]
    fn test_validate_max_imbalance() {
        // Valid imbalances
        assert!(validate_max_imbalance(1).is_ok());
        assert!(validate_max_imbalance(1000).is_ok());
        assert!(validate_max_imbalance(1_000_000).is_ok());

        // Invalid imbalances
        assert!(validate_max_imbalance(0).is_err()); // Zero
        assert!(validate_max_imbalance(-100).is_err()); // Negative
    }

    #[test]
    fn test_validate_auto_accept_threshold() {
        // Valid thresholds
        assert!(validate_auto_accept_threshold(None).is_ok());
        assert!(validate_auto_accept_threshold(Some(0.0)).is_ok());
        assert!(validate_auto_accept_threshold(Some(0.5)).is_ok());
        assert!(validate_auto_accept_threshold(Some(1.0)).is_ok());
        assert!(validate_auto_accept_threshold(Some(-1.0)).is_ok()); // Disabled

        // Invalid thresholds
        assert!(validate_auto_accept_threshold(Some(-0.5)).is_err()); // Not -1.0 and negative
        assert!(validate_auto_accept_threshold(Some(1.1)).is_err()); // Too high
        assert!(validate_auto_accept_threshold(Some(f64::NAN)).is_err()); // NaN
    }

    #[test]
    fn test_validate_trust_decay_factor() {
        // Valid factors
        assert!(validate_trust_decay_factor(None).is_ok());
        assert!(validate_trust_decay_factor(Some(0.0)).is_ok());
        assert!(validate_trust_decay_factor(Some(0.05)).is_ok());
        assert!(validate_trust_decay_factor(Some(1.0)).is_ok());

        // Invalid factors
        assert!(validate_trust_decay_factor(Some(-0.1)).is_err()); // Negative
        assert!(validate_trust_decay_factor(Some(1.1)).is_err()); // Too high
        assert!(validate_trust_decay_factor(Some(f64::NAN)).is_err()); // NaN
    }

    #[test]
    fn test_validate_max_attestations_per_minute() {
        // Valid rates
        assert!(validate_max_attestations_per_minute(None).is_ok());
        assert!(validate_max_attestations_per_minute(Some(1)).is_ok());
        assert!(validate_max_attestations_per_minute(Some(100)).is_ok());

        // Invalid rates
        assert!(validate_max_attestations_per_minute(Some(0)).is_err()); // Zero
    }
}
