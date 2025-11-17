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

/// Maximum number of scopes in a token request
pub const MAX_SCOPES: usize = 20;

/// Maximum payment amount (prevent overflow and unrealistic values)
/// Set to 1 trillion to allow large legitimate transactions while preventing abuse
pub const MAX_PAYMENT_AMOUNT: i64 = 1_000_000_000_000;

/// Validate cooperative ID
pub fn validate_coop_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(GatewayError::BadRequest("Cooperative ID cannot be empty".to_string()));
    }

    if id.len() > MAX_COOP_ID_LEN {
        return Err(GatewayError::BadRequest(
            format!("Cooperative ID exceeds maximum length of {} characters", MAX_COOP_ID_LEN)
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
    if name.is_empty() {
        return Err(GatewayError::BadRequest("Cooperative name cannot be empty".to_string()));
    }

    if name.len() > MAX_COOP_NAME_LEN {
        return Err(GatewayError::BadRequest(
            format!("Cooperative name exceeds maximum length of {} characters", MAX_COOP_NAME_LEN)
        ));
    }

    Ok(())
}

/// Validate currency identifier
pub fn validate_currency(currency: &str) -> Result<()> {
    if currency.is_empty() {
        return Err(GatewayError::BadRequest("Currency cannot be empty".to_string()));
    }

    if currency.len() > MAX_CURRENCY_LEN {
        return Err(GatewayError::BadRequest(
            format!("Currency identifier exceeds maximum length of {} characters", MAX_CURRENCY_LEN)
        ));
    }

    Ok(())
}

/// Validate optional memo field
pub fn validate_memo(memo: &Option<String>) -> Result<()> {
    if let Some(memo_text) = memo {
        if memo_text.len() > MAX_MEMO_LEN {
            return Err(GatewayError::BadRequest(
                format!("Memo exceeds maximum length of {} characters", MAX_MEMO_LEN)
            ));
        }
    }

    Ok(())
}

/// Validate governance model string
pub fn validate_governance_model(model: &str) -> Result<()> {
    if model.is_empty() {
        return Err(GatewayError::BadRequest("Governance model cannot be empty".to_string()));
    }

    if model.len() > MAX_GOVERNANCE_MODEL_LEN {
        return Err(GatewayError::BadRequest(
            format!("Governance model exceeds maximum length of {} characters", MAX_GOVERNANCE_MODEL_LEN)
        ));
    }

    Ok(())
}

/// Validate credit policy string
pub fn validate_credit_policy(policy: &str) -> Result<()> {
    if policy.is_empty() {
        return Err(GatewayError::BadRequest("Credit policy cannot be empty".to_string()));
    }

    if policy.len() > MAX_CREDIT_POLICY_LEN {
        return Err(GatewayError::BadRequest(
            format!("Credit policy exceeds maximum length of {} characters", MAX_CREDIT_POLICY_LEN)
        ));
    }

    Ok(())
}

/// Validate member count doesn't exceed limit
pub fn validate_member_count(current_count: usize) -> Result<()> {
    if current_count >= MAX_MEMBERS_PER_COOP {
        return Err(GatewayError::BadRequest(
            format!("Cooperative has reached maximum member limit of {}", MAX_MEMBERS_PER_COOP)
        ));
    }

    Ok(())
}

/// Validate scopes list
pub fn validate_scopes(scopes: &[String]) -> Result<()> {
    if scopes.len() > MAX_SCOPES {
        return Err(GatewayError::BadRequest(
            format!("Number of scopes exceeds maximum of {}", MAX_SCOPES)
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
            format!("Amount exceeds maximum of {}", MAX_PAYMENT_AMOUNT)
        ));
    }

    Ok(())
}

/// Validate cooperative count doesn't exceed global limit
pub fn validate_coop_count(current_count: usize) -> Result<()> {
    if current_count >= MAX_COOPERATIVES {
        return Err(GatewayError::BadRequest(
            format!("Gateway has reached maximum cooperative limit of {}", MAX_COOPERATIVES)
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
        assert!(validate_coop_name(&"a".repeat(257)).is_err()); // Too long
    }

    #[test]
    fn test_validate_currency() {
        assert!(validate_currency("hours").is_ok());
        assert!(validate_currency("USD").is_ok());
        assert!(validate_currency("").is_err()); // Empty
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
        assert!(validate_scopes(&vec!["ledger:read".to_string()]).is_ok());
        let many_scopes: Vec<String> = (0..20).map(|i| format!("scope:{}", i)).collect();
        assert!(validate_scopes(&many_scopes).is_ok());
        let too_many_scopes: Vec<String> = (0..21).map(|i| format!("scope:{}", i)).collect();
        assert!(validate_scopes(&too_many_scopes).is_err());
    }

    #[test]
    fn test_validate_governance_model() {
        assert!(validate_governance_model("consensus").is_ok());
        assert!(validate_governance_model("majority").is_ok());
        assert!(validate_governance_model("").is_err()); // Empty
        assert!(validate_governance_model(&"a".repeat(65)).is_err()); // Too long
    }

    #[test]
    fn test_validate_credit_policy() {
        assert!(validate_credit_policy("conservative").is_ok());
        assert!(validate_credit_policy("permissive").is_ok());
        assert!(validate_credit_policy("").is_err()); // Empty
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
}
