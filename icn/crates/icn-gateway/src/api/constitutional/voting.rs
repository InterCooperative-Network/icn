//! Amendment Voting API endpoints
//!
//! Provides UI-friendly voting endpoints for constitutional amendments.
//! This supplements the existing ratification system with simpler vote
//! operations for mobile/web apps.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};
use icn_governance::{AmendmentId, AmendmentStatus, Ratification, RatifierType};
use icn_identity::Did;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Vote choice
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum VoteChoice {
    /// Approve the amendment
    Approve,
    /// Reject the amendment
    Reject,
    /// Abstain from voting (still counts for quorum)
    Abstain,
}

impl VoteChoice {
    pub fn to_approved(&self) -> bool {
        matches!(self, VoteChoice::Approve)
    }
}

/// Request to cast a vote
#[derive(Debug, Deserialize, ToSchema)]
pub struct CastVoteRequest {
    /// Vote choice: approve, reject, or abstain
    pub vote: VoteChoice,
    /// Optional comment explaining the vote
    pub comment: Option<String>,
    /// Optional vote weight (for weighted voting systems)
    pub weight: Option<u32>,
}

/// Individual vote response
#[derive(Debug, Serialize, ToSchema)]
pub struct VoteResponse {
    /// Voter DID
    pub voter: String,
    /// Vote choice
    pub vote: String,
    /// Weight (if applicable)
    pub weight: Option<u32>,
    /// Comment
    pub comment: Option<String>,
    /// Vote timestamp
    pub timestamp: u64,
    /// Human-readable time
    pub voted_at: String,
}

/// Vote results response
#[derive(Debug, Serialize, ToSchema)]
pub struct VoteResultsResponse {
    /// Amendment ID
    pub amendment_id: String,
    /// Amendment title
    pub title: String,
    /// Current status
    pub status: String,
    /// Total votes cast
    pub total_votes: usize,
    /// Approve votes
    pub approve_count: usize,
    /// Reject votes
    pub reject_count: usize,
    /// Abstain votes
    pub abstain_count: usize,
    /// Approval percentage (of non-abstain votes)
    pub approval_percentage: f64,
    /// Quorum requirement (percentage)
    pub quorum_required: u32,
    /// Quorum achieved
    pub quorum_achieved: bool,
    /// Approval threshold (percentage)
    pub approval_threshold: u32,
    /// Has passed (meets quorum and threshold)
    pub has_passed: bool,
    /// Voting ends at (Unix timestamp)
    pub voting_ends_at: Option<u64>,
    /// Time remaining in seconds
    pub time_remaining_secs: Option<u64>,
    /// All votes (if include_votes=true)
    pub votes: Option<Vec<VoteResponse>>,
}

/// List votes response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListVotesResponse {
    pub amendment_id: String,
    pub total_votes: usize,
    pub votes: Vec<VoteResponse>,
}

/// User's vote status response
#[derive(Debug, Serialize, ToSchema)]
pub struct MyVoteResponse {
    pub amendment_id: String,
    /// Whether user has voted
    pub has_voted: bool,
    /// The vote if cast
    pub vote: Option<VoteResponse>,
    /// Whether user can vote (is eligible)
    pub can_vote: bool,
    /// Reason if can't vote
    pub reason: Option<String>,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn format_timestamp(timestamp: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    let duration_since_epoch = datetime.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration_since_epoch.as_secs();

    let days = secs / 86400;
    let years = 1970 + (days / 365);
    let days_in_year = days % 365;
    let months = days_in_year / 30 + 1;
    let day = days_in_year % 30 + 1;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    format!("{years:04}-{months:02}-{day:02} {hours:02}:{mins:02} UTC")
}

fn ratification_to_vote(r: &Ratification) -> VoteResponse {
    let vote = if r.approved { "approve" } else { "reject" };
    VoteResponse {
        voter: r.authority_did.to_string(),
        vote: vote.to_string(),
        weight: None, // Ratifications don't have weight
        comment: r.comment.clone(),
        timestamp: r.timestamp,
        voted_at: format_timestamp(r.timestamp),
    }
}

fn format_status(status: &AmendmentStatus) -> String {
    match status {
        AmendmentStatus::Draft | AmendmentStatus::DraftWithArtifacts { .. } => "draft".to_string(),
        AmendmentStatus::Submitted { .. } => "submitted".to_string(),
        AmendmentStatus::UnderReview { .. } => "under_review".to_string(),
        AmendmentStatus::Voting { .. } => "voting".to_string(),
        AmendmentStatus::Ratifying { .. } => "ratifying".to_string(),
        AmendmentStatus::Ratified { .. } => "ratified".to_string(),
        AmendmentStatus::Rejected { .. } => "rejected".to_string(),
        AmendmentStatus::Withdrawn { .. } => "withdrawn".to_string(),
        AmendmentStatus::Expired { .. } => "expired".to_string(),
    }
}

fn parse_amendment_id(id_hex: &str) -> Result<AmendmentId> {
    let id_bytes = hex::decode(id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id = [0u8; 32];
    amendment_id.copy_from_slice(&id_bytes);
    Ok(AmendmentId::new(amendment_id))
}

// ============================================================================
// Endpoints
// ============================================================================

/// POST /v1/constitutional/amendments/{id}/votes - Cast a vote
///
/// Cast a vote on an amendment. The caller must be an eligible voter
/// (member, steward, etc. depending on the amendment scope).
#[post("/amendments/{id}/votes")]
pub async fn cast_vote(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<CastVoteRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let voter: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let amendment_id = parse_amendment_id(&id)?;

    // Get amendment to verify it's in voting period
    let amendment = commons_mgr
        .get_amendment(&amendment_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Amendment not found".to_string()))?;

    // Check voting status
    if !matches!(amendment.status, AmendmentStatus::Voting { .. }) {
        return Err(GatewayError::BadRequest(
            "Amendment is not currently in voting period".to_string(),
        ));
    }

    // Check if already voted
    let already_voted = amendment
        .ratifications
        .iter()
        .any(|r| r.authority_did == voter);
    if already_voted {
        return Err(GatewayError::BadRequest(
            "You have already voted on this amendment".to_string(),
        ));
    }

    // Handle abstain as a special case
    if req.vote == VoteChoice::Abstain {
        // For abstain, we still record it but with approved=false and a special comment
        let comment = Some(format!(
            "[ABSTAIN] {}",
            req.comment.clone().unwrap_or_default()
        ));
        let ratification = Ratification {
            ratifier_id: voter.to_string(),
            ratifier_type: RatifierType::CommonsHolder,
            authority_did: voter.clone(),
            approved: false, // Abstain doesn't count toward approval
            timestamp: icn_time::current_timestamp_secs(),
            comment,
            signature: Vec::new(), // Simplified - real impl would require signature
        };

        let updated = commons_mgr
            .add_amendment_ratification(&amendment_id, ratification)
            .await?;

        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "vote": "abstain",
            "total_votes": updated.ratifications.len(),
        })));
    }

    // Create ratification from vote
    let ratification = Ratification {
        ratifier_id: voter.to_string(),
        ratifier_type: RatifierType::CommonsHolder,
        authority_did: voter.clone(),
        approved: req.vote.to_approved(),
        timestamp: icn_time::current_timestamp_secs(),
        comment: req.comment.clone(),
        signature: Vec::new(), // Simplified - real impl would require signature
    };

    let updated = commons_mgr
        .add_amendment_ratification(&amendment_id, ratification)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "vote": if req.vote.to_approved() { "approve" } else { "reject" },
        "total_votes": updated.ratifications.len(),
    })))
}

/// GET /v1/constitutional/amendments/{id}/votes - List all votes
///
/// Returns all votes cast on an amendment.
#[get("/amendments/{id}/votes")]
pub async fn list_votes(
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    let amendment_id = parse_amendment_id(&id)?;

    let amendment = commons_mgr
        .get_amendment(&amendment_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Amendment not found".to_string()))?;

    let votes: Vec<VoteResponse> = amendment
        .ratifications
        .iter()
        .map(ratification_to_vote)
        .collect();

    Ok(HttpResponse::Ok().json(ListVotesResponse {
        amendment_id: hex::encode(amendment.id.as_bytes()),
        total_votes: votes.len(),
        votes,
    }))
}

/// GET /v1/constitutional/amendments/{id}/results - Get vote results
///
/// Returns the current voting results including approval percentage,
/// quorum status, and pass/fail determination.
#[get("/amendments/{id}/results")]
pub async fn get_results(
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    query: web::Query<ResultsQuery>,
) -> Result<HttpResponse> {
    let amendment_id = parse_amendment_id(&id)?;

    let amendment = commons_mgr
        .get_amendment(&amendment_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Amendment not found".to_string()))?;

    // Count votes
    let mut approve_count = 0;
    let mut reject_count = 0;
    let mut abstain_count = 0;

    for r in &amendment.ratifications {
        if r.comment
            .as_ref()
            .is_some_and(|c| c.starts_with("[ABSTAIN]"))
        {
            abstain_count += 1;
        } else if r.approved {
            approve_count += 1;
        } else {
            reject_count += 1;
        }
    }

    let total_votes = amendment.ratifications.len();
    let non_abstain = approve_count + reject_count;

    // Calculate approval percentage
    let approval_percentage = if non_abstain > 0 {
        (approve_count as f64 / non_abstain as f64) * 100.0
    } else {
        0.0
    };

    // Get voting end time and time remaining
    let (voting_ends_at_ts, time_remaining_secs) = match &amendment.status {
        AmendmentStatus::Voting { voting_ends_at, .. } => {
            let now = icn_time::current_timestamp_secs();
            let remaining = if *voting_ends_at > now {
                voting_ends_at - now
            } else {
                0
            };
            (Some(*voting_ends_at), Some(remaining))
        }
        _ => (None, None),
    };

    // Check quorum (simplified - would need total eligible voters)
    let quorum_required = amendment.requirements.quorum_percent;
    let quorum_achieved = total_votes >= 3; // Simplified: at least 3 votes

    // Check approval threshold
    let approval_threshold = amendment.requirements.approval_percent;
    let has_passed = quorum_achieved && approval_percentage >= approval_threshold as f64;

    // Include votes if requested
    let votes = if query.include_votes.unwrap_or(false) {
        Some(
            amendment
                .ratifications
                .iter()
                .map(ratification_to_vote)
                .collect(),
        )
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(VoteResultsResponse {
        amendment_id: hex::encode(amendment.id.as_bytes()),
        title: amendment.title.clone(),
        status: format_status(&amendment.status),
        total_votes,
        approve_count,
        reject_count,
        abstain_count,
        approval_percentage,
        quorum_required,
        quorum_achieved,
        approval_threshold,
        has_passed,
        voting_ends_at: voting_ends_at_ts,
        time_remaining_secs,
        votes,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResultsQuery {
    /// Include individual votes in response
    pub include_votes: Option<bool>,
}

/// GET /v1/constitutional/amendments/{id}/my-vote - Get user's vote status
///
/// Returns whether the authenticated user has voted on this amendment
/// and their vote if cast.
#[get("/amendments/{id}/my-vote")]
pub async fn get_my_vote(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let voter: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let amendment_id = parse_amendment_id(&id)?;

    let amendment = commons_mgr
        .get_amendment(&amendment_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Amendment not found".to_string()))?;

    // Find user's vote
    let my_vote = amendment
        .ratifications
        .iter()
        .find(|r| r.authority_did == voter)
        .map(ratification_to_vote);

    // Determine if user can vote
    let (can_vote, reason) = match &amendment.status {
        AmendmentStatus::Voting { voting_ends_at, .. } => {
            let now = icn_time::current_timestamp_secs();

            if my_vote.is_some() {
                (false, Some("Already voted".to_string()))
            } else if now > *voting_ends_at {
                (false, Some("Voting period has ended".to_string()))
            } else {
                (true, None)
            }
        }
        _ => (false, Some("Amendment is not in voting period".to_string())),
    };

    Ok(HttpResponse::Ok().json(MyVoteResponse {
        amendment_id: hex::encode(amendment.id.as_bytes()),
        has_voted: my_vote.is_some(),
        vote: my_vote,
        can_vote,
        reason,
    }))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure voting routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(cast_vote)
        .service(list_votes)
        .service(get_results)
        .service(get_my_vote);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vote_choice_conversion() {
        assert!(VoteChoice::Approve.to_approved());
        assert!(!VoteChoice::Reject.to_approved());
        assert!(!VoteChoice::Abstain.to_approved());
    }

    #[test]
    fn test_format_timestamp() {
        let ts = 1734000000; // Dec 12, 2024 approximately
        let formatted = format_timestamp(ts);
        assert!(formatted.contains("UTC"));
    }
}
