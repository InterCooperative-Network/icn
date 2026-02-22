//! Input validation for governance HTTP requests.
//!
//! Mirrors the governance-specific subset of `icn-gateway/src/validation.rs`.
//! Non-governance validators (coop IDs, payments, history limits, etc.) are
//! not included here.

use icn_http_kit::error::ApiError;

// ============================================================================
// Constants
// ============================================================================

pub const MAX_DOMAIN_ID_LEN: usize = 128;
pub const MAX_DOMAIN_NAME_LEN: usize = 256;
pub const MAX_GOVERNANCE_MODEL_LEN: usize = 64;
pub const MAX_PROPOSAL_TITLE_LEN: usize = 256;
pub const MAX_PROPOSAL_DESCRIPTION_LEN: usize = 10_000;
pub const MAX_VOTE_COMMENT_LEN: usize = 2_000;
pub const MAX_DOMAIN_MEMBERS: usize = 10_000;
pub const MAX_VOTING_PERIOD_SECONDS: u64 = 31_536_000; // 1 year

pub const MAX_FEDERATION_ID_LEN: usize = 128;
pub const MAX_REASON_LEN: usize = 2_000;
pub const MAX_EVIDENCE_LEN: usize = 5_000;
pub const MAX_CONTEXT_LEN: usize = 256;
pub const MAX_GRACE_PERIOD_DAYS: u32 = 365;
pub const MAX_CURRENCY_LEN: usize = 32;
pub const MAX_PAYMENT_AMOUNT: i64 = 1_000_000_000_000;

// ============================================================================
// Domain validators
// ============================================================================

pub fn validate_domain_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() {
        return Err(ApiError::BadRequest("Domain ID cannot be empty".into()));
    }
    if id.len() > MAX_DOMAIN_ID_LEN {
        return Err(ApiError::BadRequest(format!(
            "Domain ID exceeds maximum length of {MAX_DOMAIN_ID_LEN} characters"
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':')
    {
        return Err(ApiError::BadRequest(
            "Domain ID must contain only alphanumeric characters, hyphens, underscores, and colons"
                .into(),
        ));
    }
    Ok(())
}

pub fn validate_domain_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Domain name cannot be empty or whitespace-only".into(),
        ));
    }
    if name.len() > MAX_DOMAIN_NAME_LEN {
        return Err(ApiError::BadRequest(format!(
            "Domain name exceeds maximum length of {MAX_DOMAIN_NAME_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_governance_model(model: &str) -> Result<(), ApiError> {
    if model.is_empty() || model.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Governance model cannot be empty or whitespace-only".into(),
        ));
    }
    if model.len() > MAX_GOVERNANCE_MODEL_LEN {
        return Err(ApiError::BadRequest(format!(
            "Governance model exceeds maximum length of {MAX_GOVERNANCE_MODEL_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_domain_members(members: &[String]) -> Result<(), ApiError> {
    if members.is_empty() {
        return Err(ApiError::BadRequest(
            "Domain must have at least one member".into(),
        ));
    }
    if members.len() > MAX_DOMAIN_MEMBERS {
        return Err(ApiError::BadRequest(format!(
            "Number of members exceeds maximum of {MAX_DOMAIN_MEMBERS}"
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for m in members {
        if !seen.insert(m) {
            return Err(ApiError::BadRequest(format!(
                "Duplicate member DID not allowed: {m}"
            )));
        }
    }
    Ok(())
}

pub fn validate_governance_params(
    quorum_percent: u8,
    approval_percent: u8,
    voting_period_seconds: u64,
) -> Result<(), ApiError> {
    if quorum_percent > 100 {
        return Err(ApiError::BadRequest(
            "Quorum percentage must be between 0 and 100".into(),
        ));
    }
    if approval_percent > 100 {
        return Err(ApiError::BadRequest(
            "Approval percentage must be between 0 and 100".into(),
        ));
    }
    if voting_period_seconds == 0 {
        return Err(ApiError::BadRequest(
            "Voting period must be greater than 0".into(),
        ));
    }
    if voting_period_seconds > MAX_VOTING_PERIOD_SECONDS {
        return Err(ApiError::BadRequest(format!(
            "Voting period exceeds maximum of {MAX_VOTING_PERIOD_SECONDS} seconds (1 year)"
        )));
    }
    Ok(())
}

// ============================================================================
// Proposal validators
// ============================================================================

pub fn validate_proposal_title(title: &str) -> Result<(), ApiError> {
    if title.is_empty() || title.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Proposal title cannot be empty or whitespace-only".into(),
        ));
    }
    if title.len() > MAX_PROPOSAL_TITLE_LEN {
        return Err(ApiError::BadRequest(format!(
            "Proposal title exceeds maximum length of {MAX_PROPOSAL_TITLE_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_proposal_description(description: &str) -> Result<(), ApiError> {
    if description.is_empty() || description.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Proposal description cannot be empty or whitespace-only".into(),
        ));
    }
    if description.len() > MAX_PROPOSAL_DESCRIPTION_LEN {
        return Err(ApiError::BadRequest(format!(
            "Proposal description exceeds maximum length of {MAX_PROPOSAL_DESCRIPTION_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_vote_comment(comment: &Option<String>) -> Result<(), ApiError> {
    if let Some(text) = comment {
        if !text.is_empty() && text.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Vote comment cannot be whitespace-only".into(),
            ));
        }
        if text.len() > MAX_VOTE_COMMENT_LEN {
            return Err(ApiError::BadRequest(format!(
                "Vote comment exceeds maximum length of {MAX_VOTE_COMMENT_LEN} characters"
            )));
        }
    }
    Ok(())
}

// ============================================================================
// Federation validators
// ============================================================================

pub fn validate_federation_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.trim().is_empty() {
        return Err(ApiError::BadRequest("Federation ID cannot be empty".into()));
    }
    if id.len() > MAX_FEDERATION_ID_LEN {
        return Err(ApiError::BadRequest(format!(
            "Federation ID exceeds maximum length of {MAX_FEDERATION_ID_LEN} characters"
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ':')
    {
        return Err(ApiError::BadRequest(
            "Federation ID must contain only alphanumeric characters, hyphens, underscores, and colons".into(),
        ));
    }
    Ok(())
}

pub fn validate_trust_score(score: f64) -> Result<(), ApiError> {
    if !score.is_finite() {
        return Err(ApiError::BadRequest(
            "Trust score must be a finite number".into(),
        ));
    }
    if !(0.0..=1.0).contains(&score) {
        return Err(ApiError::BadRequest(
            "Trust score must be between 0.0 and 1.0".into(),
        ));
    }
    Ok(())
}

pub fn validate_grace_period_days(days: u32) -> Result<(), ApiError> {
    if days == 0 {
        return Err(ApiError::BadRequest(
            "Grace period must be at least 1 day".into(),
        ));
    }
    if days > MAX_GRACE_PERIOD_DAYS {
        return Err(ApiError::BadRequest(format!(
            "Grace period cannot exceed {MAX_GRACE_PERIOD_DAYS} days"
        )));
    }
    Ok(())
}

pub fn validate_settlement_interval(interval: &str) -> Result<String, ApiError> {
    match interval.to_lowercase().as_str() {
        "daily" | "weekly" | "monthly" | "manual" => Ok(interval.to_lowercase()),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid settlement interval: '{interval}'. Valid values: daily, weekly, monthly, manual"
        ))),
    }
}

pub fn validate_data_sharing_level(level: &str) -> Result<String, ApiError> {
    match level.to_lowercase().as_str() {
        "none" | "metadata_only" | "full" => Ok(level.to_lowercase()),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid data sharing level: '{level}'. Valid values: none, metadata_only, full"
        ))),
    }
}

pub fn validate_dispute_resolution(method: &str) -> Result<String, ApiError> {
    let lower = method.to_lowercase();
    match lower.as_str() {
        "federation_mediation" | "federation_vote" => Ok(lower),
        s if s.starts_with("arbitrator:") => {
            let arbitrator_id = s.strip_prefix("arbitrator:").unwrap_or("");
            if arbitrator_id.is_empty() || arbitrator_id.trim().is_empty() {
                return Err(ApiError::BadRequest(
                    "Arbitrator ID cannot be empty in dispute resolution".into(),
                ));
            }
            Ok(method.to_string())
        }
        _ => Err(ApiError::BadRequest(format!(
            "Invalid dispute resolution: '{method}'. Valid values: federation_mediation, federation_vote, arbitrator:<coop_id>"
        ))),
    }
}

pub fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if reason.is_empty() || reason.trim().is_empty() {
        return Err(ApiError::BadRequest("Reason cannot be empty".into()));
    }
    if reason.len() > MAX_REASON_LEN {
        return Err(ApiError::BadRequest(format!(
            "Reason exceeds maximum length of {MAX_REASON_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_context(context: &str) -> Result<(), ApiError> {
    if context.is_empty() || context.trim().is_empty() {
        return Err(ApiError::BadRequest("Context cannot be empty".into()));
    }
    if context.len() > MAX_CONTEXT_LEN {
        return Err(ApiError::BadRequest(format!(
            "Context exceeds maximum length of {MAX_CONTEXT_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_evidence(evidence: &Option<String>) -> Result<(), ApiError> {
    if let Some(ev) = evidence {
        if ev.is_empty() || ev.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Evidence cannot be empty if provided".into(),
            ));
        }
        if ev.len() > MAX_EVIDENCE_LEN {
            return Err(ApiError::BadRequest(format!(
                "Evidence exceeds maximum length of {MAX_EVIDENCE_LEN} characters"
            )));
        }
    }
    Ok(())
}

pub fn validate_max_imbalance(max_imbalance: i64) -> Result<(), ApiError> {
    if max_imbalance <= 0 {
        return Err(ApiError::BadRequest(
            "Max imbalance must be positive".into(),
        ));
    }
    Ok(())
}

pub fn validate_currency(currency: &str) -> Result<(), ApiError> {
    if currency.is_empty() || currency.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Currency cannot be empty or whitespace-only".into(),
        ));
    }
    if currency.len() > MAX_CURRENCY_LEN {
        return Err(ApiError::BadRequest(format!(
            "Currency identifier exceeds maximum length of {MAX_CURRENCY_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_payment_amount(amount: i64) -> Result<(), ApiError> {
    if amount <= 0 {
        return Err(ApiError::BadRequest(
            "Payment amount must be positive".into(),
        ));
    }
    if amount > MAX_PAYMENT_AMOUNT {
        return Err(ApiError::BadRequest(format!(
            "Payment amount exceeds maximum of {MAX_PAYMENT_AMOUNT}"
        )));
    }
    Ok(())
}

pub fn validate_auto_accept_threshold(threshold: Option<f64>) -> Result<(), ApiError> {
    if let Some(t) = threshold {
        if !t.is_finite() {
            return Err(ApiError::BadRequest(
                "Auto-accept threshold must be a finite number".into(),
            ));
        }
        if t != -1.0 && !(0.0..=1.0).contains(&t) {
            return Err(ApiError::BadRequest(
                "Auto-accept threshold must be -1.0 (disabled) or between 0.0 and 1.0".into(),
            ));
        }
    }
    Ok(())
}

pub fn validate_trust_decay_factor(factor: Option<f64>) -> Result<(), ApiError> {
    if let Some(f) = factor {
        validate_trust_score(f)?;
    }
    Ok(())
}

pub fn validate_max_attestations_per_minute(rate: Option<u32>) -> Result<(), ApiError> {
    if let Some(r) = rate {
        if r == 0 {
            return Err(ApiError::BadRequest(
                "Max attestations per minute must be greater than 0".into(),
            ));
        }
    }
    Ok(())
}

// ============================================================================
// Action item validators (inline in the gateway; extracted here)
// ============================================================================

pub const MAX_ACTION_ITEM_TITLE_LEN: usize = 200;
pub const MAX_ACTION_ITEM_DESC_LEN: usize = 5_000;
pub const MAX_TAGS: usize = 10;
pub const MAX_TAG_LEN: usize = 50;
pub const MAX_MEETING_CONTEXT_LEN: usize = 500;

pub fn validate_action_item_title(title: &str) -> Result<(), ApiError> {
    if title.is_empty() {
        return Err(ApiError::BadRequest("Title cannot be empty".into()));
    }
    if title.len() > MAX_ACTION_ITEM_TITLE_LEN {
        return Err(ApiError::BadRequest(format!(
            "Title exceeds maximum length of {MAX_ACTION_ITEM_TITLE_LEN} characters"
        )));
    }
    Ok(())
}

pub fn validate_action_item_description(description: &Option<String>) -> Result<(), ApiError> {
    if let Some(desc) = description {
        if desc.len() > MAX_ACTION_ITEM_DESC_LEN {
            return Err(ApiError::BadRequest(format!(
                "Description exceeds maximum length of {MAX_ACTION_ITEM_DESC_LEN} characters"
            )));
        }
    }
    Ok(())
}

pub fn validate_tags(tags: &[String]) -> Result<(), ApiError> {
    if tags.len() > MAX_TAGS {
        return Err(ApiError::BadRequest(format!(
            "Too many tags (maximum {MAX_TAGS})"
        )));
    }
    for tag in tags {
        if tag.is_empty() {
            return Err(ApiError::BadRequest("Tags cannot be empty strings".into()));
        }
        if tag.len() > MAX_TAG_LEN {
            return Err(ApiError::BadRequest(format!(
                "Tag '{tag}' exceeds maximum length of {MAX_TAG_LEN} characters"
            )));
        }
    }
    Ok(())
}

pub fn validate_meeting_context(ctx: &Option<String>) -> Result<(), ApiError> {
    if let Some(context) = ctx {
        if context.len() > MAX_MEETING_CONTEXT_LEN {
            return Err(ApiError::BadRequest(format!(
                "Meeting context exceeds maximum length of {MAX_MEETING_CONTEXT_LEN} characters"
            )));
        }
    }
    Ok(())
}

pub fn validate_due_date(due_date: Option<u64>, allow_past: bool) -> Result<(), ApiError> {
    if let Some(ts) = due_date {
        if !allow_past {
            let now = current_time_secs();
            let grace = 5 * 60;
            if ts < now.saturating_sub(grace) {
                return Err(ApiError::BadRequest(
                    "Due date cannot be in the past".into(),
                ));
            }
        }
    }
    Ok(())
}

// ============================================================================
// Shared helper
// ============================================================================

pub fn current_time_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
