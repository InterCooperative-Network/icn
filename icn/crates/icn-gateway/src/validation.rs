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
pub const MAX_SCOPES: usize = 20;

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
        return Err(GatewayError::BadRequest("Domain ID cannot be empty".to_string()));
    }

    if id.len() > MAX_DOMAIN_ID_LEN {
        return Err(GatewayError::BadRequest(
            format!("Domain ID exceeds maximum length of {MAX_DOMAIN_ID_LEN} characters")
        ));
    }

    // Validate characters (alphanumeric, hyphens, underscores, colons for namespacing)
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':') {
        return Err(GatewayError::BadRequest(
            "Domain ID must contain only alphanumeric characters, hyphens, underscores, and colons".to_string()
        ));
    }

    Ok(())
}

/// Validate governance domain name
pub fn validate_domain_name(name: &str) -> Result<()> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(GatewayError::BadRequest("Domain name cannot be empty or whitespace-only".to_string()));
    }

    if name.len() > MAX_DOMAIN_NAME_LEN {
        return Err(GatewayError::BadRequest(
            format!("Domain name exceeds maximum length of {MAX_DOMAIN_NAME_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate cooperative ID
pub fn validate_coop_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(GatewayError::BadRequest("Cooperative ID cannot be empty".to_string()));
    }

    if id.len() > MAX_COOP_ID_LEN {
        return Err(GatewayError::BadRequest(
            format!("Cooperative ID exceeds maximum length of {MAX_COOP_ID_LEN} characters")
        ));
    }

    // Validate characters (alphanumeric, hyphens, underscores)
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(GatewayError::BadRequest(
            "Cooperative ID must contain only alphanumeric characters, hyphens, and underscores".to_string()
        ));
    }

    Ok(())
}

/// Validate cooperative name
pub fn validate_coop_name(name: &str) -> Result<()> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(GatewayError::BadRequest("Cooperative name cannot be empty or whitespace-only".to_string()));
    }

    if name.len() > MAX_COOP_NAME_LEN {
        return Err(GatewayError::BadRequest(
            format!("Cooperative name exceeds maximum length of {MAX_COOP_NAME_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate currency identifier
pub fn validate_currency(currency: &str) -> Result<()> {
    if currency.is_empty() || currency.trim().is_empty() {
        return Err(GatewayError::BadRequest("Currency cannot be empty or whitespace-only".to_string()));
    }

    if currency.len() > MAX_CURRENCY_LEN {
        return Err(GatewayError::BadRequest(
            format!("Currency identifier exceeds maximum length of {MAX_CURRENCY_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate optional memo field
pub fn validate_memo(memo: &Option<String>) -> Result<()> {
    if let Some(memo_text) = memo {
        if memo_text.len() > MAX_MEMO_LEN {
            return Err(GatewayError::BadRequest(
                format!("Memo exceeds maximum length of {MAX_MEMO_LEN} characters")
            ));
        }
    }

    Ok(())
}

/// Validate governance model string
pub fn validate_governance_model(model: &str) -> Result<()> {
    if model.is_empty() || model.trim().is_empty() {
        return Err(GatewayError::BadRequest("Governance model cannot be empty or whitespace-only".to_string()));
    }

    if model.len() > MAX_GOVERNANCE_MODEL_LEN {
        return Err(GatewayError::BadRequest(
            format!("Governance model exceeds maximum length of {MAX_GOVERNANCE_MODEL_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate credit policy string
pub fn validate_credit_policy(policy: &str) -> Result<()> {
    if policy.is_empty() || policy.trim().is_empty() {
        return Err(GatewayError::BadRequest("Credit policy cannot be empty or whitespace-only".to_string()));
    }

    if policy.len() > MAX_CREDIT_POLICY_LEN {
        return Err(GatewayError::BadRequest(
            format!("Credit policy exceeds maximum length of {MAX_CREDIT_POLICY_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate member count doesn't exceed limit
pub fn validate_member_count(current_count: usize) -> Result<()> {
    if current_count >= MAX_MEMBERS_PER_COOP {
        return Err(GatewayError::BadRequest(
            format!("Cooperative has reached maximum member limit of {MAX_MEMBERS_PER_COOP}")
        ));
    }

    Ok(())
}

/// Validate scopes list
pub fn validate_scopes(scopes: &[String]) -> Result<()> {
    if scopes.len() > MAX_SCOPES {
        return Err(GatewayError::BadRequest(
            format!("Number of scopes exceeds maximum of {MAX_SCOPES}")
        ));
    }

    Ok(())
}

/// Validate payment amount
pub fn validate_payment_amount(amount: i64) -> Result<()> {
    if amount <= 0 {
        return Err(GatewayError::BadRequest("Amount must be positive".to_string()));
    }

    if amount > MAX_PAYMENT_AMOUNT {
        return Err(GatewayError::BadRequest(
            format!("Amount exceeds maximum of {MAX_PAYMENT_AMOUNT}")
        ));
    }

    Ok(())
}

/// Validate cooperative count doesn't exceed global limit
pub fn validate_coop_count(current_count: usize) -> Result<()> {
    if current_count >= MAX_COOPERATIVES {
        return Err(GatewayError::BadRequest(
            format!("Gateway has reached maximum cooperative limit of {MAX_COOPERATIVES}")
        ));
    }

    Ok(())
}

/// Validate history limit parameter
pub fn validate_history_limit(limit: usize) -> Result<usize> {
    if limit == 0 {
        return Err(GatewayError::BadRequest("Limit must be greater than 0".to_string()));
    }

    if limit > MAX_HISTORY_LIMIT {
        return Err(GatewayError::BadRequest(
            format!("Limit exceeds maximum of {MAX_HISTORY_LIMIT}")
        ));
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
            "Offset exceeds maximum allowed value".to_string()
        ));
    }

    Ok(offset)
}

/// Validate proposal title
pub fn validate_proposal_title(title: &str) -> Result<()> {
    if title.is_empty() || title.trim().is_empty() {
        return Err(GatewayError::BadRequest("Proposal title cannot be empty or whitespace-only".to_string()));
    }

    if title.len() > MAX_PROPOSAL_TITLE_LEN {
        return Err(GatewayError::BadRequest(
            format!("Proposal title exceeds maximum length of {MAX_PROPOSAL_TITLE_LEN} characters")
        ));
    }

    Ok(())
}

/// Validate proposal description
pub fn validate_proposal_description(description: &str) -> Result<()> {
    if description.is_empty() || description.trim().is_empty() {
        return Err(GatewayError::BadRequest("Proposal description cannot be empty or whitespace-only".to_string()));
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
                "Vote comment cannot be whitespace-only".to_string()
            ));
        }

        if comment_text.len() > MAX_VOTE_COMMENT_LEN {
            return Err(GatewayError::BadRequest(
                format!("Vote comment exceeds maximum length of {MAX_VOTE_COMMENT_LEN} characters")
            ));
        }
    }

    Ok(())
}

/// Validate domain members list
pub fn validate_domain_members(members: &[String]) -> Result<()> {
    if members.is_empty() {
        return Err(GatewayError::BadRequest("Domain must have at least one member".to_string()));
    }

    if members.len() > MAX_DOMAIN_MEMBERS {
        return Err(GatewayError::BadRequest(
            format!("Number of members exceeds maximum of {MAX_DOMAIN_MEMBERS}")
        ));
    }

    // Check for duplicates (prevents quorum calculation bugs)
    let mut seen = std::collections::HashSet::new();
    for member in members {
        if !seen.insert(member) {
            return Err(GatewayError::BadRequest(
                format!("Duplicate member DID not allowed: {member}")
            ));
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
            "Quorum percentage must be between 0 and 100".to_string()
        ));
    }

    // Approval percentage must be 0-100
    if approval_percent > 100 {
        return Err(GatewayError::BadRequest(
            "Approval percentage must be between 0 and 100".to_string()
        ));
    }

    // Voting period must be reasonable (not zero, not more than 1 year)
    if voting_period_seconds == 0 {
        return Err(GatewayError::BadRequest(
            "Voting period must be greater than 0".to_string()
        ));
    }

    if voting_period_seconds > MAX_VOTING_PERIOD_SECONDS {
        return Err(GatewayError::BadRequest(
            format!("Voting period exceeds maximum of {MAX_VOTING_PERIOD_SECONDS} seconds (1 year)")
        ));
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
        assert!(validate_scopes(&["ledger:read".to_string()]).is_ok());
        let many_scopes: Vec<String> = (0..20).map(|i| format!("scope:{i}")).collect();
        assert!(validate_scopes(&many_scopes).is_ok());
        let too_many_scopes: Vec<String> = (0..21).map(|i| format!("scope:{i}")).collect();
        assert!(validate_scopes(&too_many_scopes).is_err());
    }

    #[test]
    fn test_validate_governance_model() {
        assert!(validate_governance_model("consensus").is_ok());
        assert!(validate_governance_model("majority").is_ok());
        assert!(validate_governance_model(&"a".repeat(MAX_GOVERNANCE_MODEL_LEN)).is_ok());
        assert!(validate_governance_model("").is_err()); // Empty
        assert!(validate_governance_model("   ").is_err()); // Whitespace-only
        assert!(validate_governance_model(&"a".repeat(MAX_GOVERNANCE_MODEL_LEN + 1)).is_err()); // Too long
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
        assert!(validate_proposal_title(&"a".repeat(MAX_PROPOSAL_TITLE_LEN + 1)).is_err()); // Too long
    }

    #[test]
    fn test_validate_proposal_description() {
        assert!(validate_proposal_description("Test description").is_ok());
        assert!(validate_proposal_description(&"a".repeat(MAX_PROPOSAL_DESCRIPTION_LEN)).is_ok());
        assert!(validate_proposal_description("").is_err()); // Empty
        assert!(validate_proposal_description("   ").is_err()); // Whitespace-only
        assert!(validate_proposal_description(&"a".repeat(MAX_PROPOSAL_DESCRIPTION_LEN + 1)).is_err()); // Too long
    }

    #[test]
    fn test_validate_vote_comment() {
        assert!(validate_vote_comment(&None).is_ok());
        assert!(validate_vote_comment(&Some("Great proposal!".to_string())).is_ok());
        assert!(validate_vote_comment(&Some("a".repeat(MAX_VOTE_COMMENT_LEN))).is_ok());
        assert!(validate_vote_comment(&Some("".to_string())).is_ok()); // Empty is allowed
        assert!(validate_vote_comment(&Some("   ".to_string())).is_err()); // Whitespace-only rejected
        assert!(validate_vote_comment(&Some("a".repeat(MAX_VOTE_COMMENT_LEN + 1))).is_err()); // Too long
    }

    #[test]
    fn test_validate_domain_name() {
        assert!(validate_domain_name("Food Coop").is_ok());
        assert!(validate_domain_name("x").is_ok());
        assert!(validate_domain_name(&"a".repeat(MAX_DOMAIN_NAME_LEN)).is_ok());
        assert!(validate_domain_name("").is_err()); // Empty
        assert!(validate_domain_name("   ").is_err()); // Whitespace-only
        assert!(validate_domain_name("\t\n  ").is_err()); // Whitespace-only (tabs/newlines)
        assert!(validate_domain_name(&"a".repeat(MAX_DOMAIN_NAME_LEN + 1)).is_err()); // Too long
    }

    #[test]
    fn test_validate_domain_members() {
        assert!(validate_domain_members(&["did:icn:alice".to_string()]).is_ok());
        assert!(validate_domain_members(&["did:icn:alice".to_string(), "did:icn:bob".to_string()]).is_ok());
        assert!(validate_domain_members(&[]).is_err()); // Empty
        let too_many: Vec<String> = (0..MAX_DOMAIN_MEMBERS + 1).map(|i| format!("did:icn:{i}")).collect();
        assert!(validate_domain_members(&too_many).is_err()); // Too many

        // Duplicate detection
        assert!(validate_domain_members(&vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:alice".to_string() // Duplicate!
        ]).is_err());
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
}
