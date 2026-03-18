//! Appeals Management UI Support
//!
//! UI-friendly endpoints for managing appeals workflow.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use icn_governance::{Appeal, AppealId, AppealStatus};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

/// Timeline event for appeal history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppealTimelineEvent {
    /// Event type
    pub event_type: String,
    /// Event description
    pub description: String,
    /// Timestamp
    pub timestamp: u64,
    /// Actor DID (if applicable)
    pub actor: Option<String>,
}

/// Detailed appeal status with next steps
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppealStatusDetail {
    /// Current status
    pub status: String,
    /// Human-readable status description
    pub description: String,
    /// Is appeal in terminal state?
    pub is_terminal: bool,
    /// Is appeal active?
    pub is_active: bool,
    /// Next steps for appellant
    pub next_steps: Vec<String>,
    /// Deadlines
    pub deadlines: Vec<AppealDeadline>,
}

/// Deadline information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppealDeadline {
    /// Deadline type
    pub deadline_type: String,
    /// Deadline timestamp
    pub timestamp: u64,
    /// Is deadline passed?
    pub is_passed: bool,
    /// Time remaining (seconds)
    pub time_remaining: Option<i64>,
}

/// Request to assign reviewer
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssignReviewerRequest {
    /// Reviewer DID
    pub reviewer_did: String,
}

/// Dashboard data for appeals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppealsDashboard {
    /// Total appeals count
    pub total_appeals: usize,
    /// Active appeals count
    pub active_appeals: usize,
    /// Filed appeals awaiting review
    pub filed_count: usize,
    /// Under review count
    pub under_review_count: usize,
    /// Hearing count
    pub hearing_count: usize,
    /// Resolved count
    pub resolved_count: usize,
    /// Dismissed count
    pub dismissed_count: usize,
    /// Withdrawn count
    pub withdrawn_count: usize,
    /// Average resolution time (seconds)
    pub avg_resolution_time: Option<u64>,
    /// Recent appeals (last 5)
    pub recent_appeals: Vec<RecentAppeal>,
}

/// Recent appeal summary
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecentAppeal {
    /// Appeal ID
    pub id: String,
    /// Appeal type
    pub appeal_type: String,
    /// Appellant DID
    pub appellant: String,
    /// Current status
    pub status: String,
    /// Created timestamp
    pub created_at: u64,
}

/// GET /constitutional/appeals/{id}/timeline - Appeal event timeline
#[get("/appeals/{id}/timeline")]
pub async fn get_appeal_timeline(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:read")?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid appeal ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Appeal ID must be 32 bytes".to_string(),
        ));
    }
    let mut appeal_id = [0u8; 32];
    appeal_id.copy_from_slice(&id_bytes);

    let appeal = commons_mgr
        .get_appeal(&AppealId::new(appeal_id))
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("Appeal not found: {id_hex}")))?;

    let timeline = build_timeline(&appeal);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "appeal_id": id_hex,
        "timeline": timeline,
        "count": timeline.len()
    })))
}

/// GET /constitutional/appeals/{id}/status - Detailed status with next steps
#[get("/appeals/{id}/status")]
pub async fn get_appeal_status(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:read")?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid appeal ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Appeal ID must be 32 bytes".to_string(),
        ));
    }
    let mut appeal_id = [0u8; 32];
    appeal_id.copy_from_slice(&id_bytes);

    let appeal = commons_mgr
        .get_appeal(&AppealId::new(appeal_id))
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("Appeal not found: {id_hex}")))?;

    let claims = get_claims(&http_req);
    let caller_did = claims.and_then(|c| c.sub.parse::<Did>().ok());

    let status_detail = build_status_detail(&appeal, caller_did.as_ref());

    Ok(HttpResponse::Ok().json(status_detail))
}

/// POST /constitutional/appeals/{id}/assign-reviewer - Assign reviewer (admin only)
#[post("/appeals/{id}/assign-reviewer")]
pub async fn assign_reviewer(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<AssignReviewerRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:admin")?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid appeal ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Appeal ID must be 32 bytes".to_string(),
        ));
    }
    let mut appeal_id = [0u8; 32];
    appeal_id.copy_from_slice(&id_bytes);

    let reviewer_did: Did = req
        .reviewer_did
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid reviewer DID: {e}")))?;

    // Get current appeal
    let mut appeal = commons_mgr
        .get_appeal(&AppealId::new(appeal_id))
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("Appeal not found: {id_hex}")))?;

    // Add to appeal body if not already present
    if !appeal.appeal_body.contains(&reviewer_did) {
        appeal.appeal_body.push(reviewer_did.clone());
        appeal.updated_at = icn_time::current_timestamp_secs();

        // Store updated appeal
        commons_mgr.store_appeal(appeal.clone()).await?;
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "appeal_id": id_hex,
        "reviewer_did": reviewer_did.to_string(),
        "appeal_body": appeal.appeal_body.iter().map(|d| d.to_string()).collect::<Vec<_>>()
    })))
}

/// GET /constitutional/appeals/dashboard - Appeals dashboard data
#[get("/appeals/dashboard")]
pub async fn get_appeals_dashboard(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:read")?;

    // Get all appeals
    let all_appeals = commons_mgr.list_appeals(None, None, None).await?;

    let dashboard = build_dashboard(&all_appeals);

    Ok(HttpResponse::Ok().json(dashboard))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Build timeline from appeal events
fn build_timeline(appeal: &Appeal) -> Vec<AppealTimelineEvent> {
    let mut timeline = Vec::new();

    // Filed event - always present
    let filed_at = match &appeal.status {
        AppealStatus::Filed { filed_at }
        | AppealStatus::UnderReview { filed_at, .. }
        | AppealStatus::Hearing { filed_at, .. }
        | AppealStatus::Resolved { filed_at, .. }
        | AppealStatus::Dismissed { filed_at, .. }
        | AppealStatus::Withdrawn { filed_at, .. } => *filed_at,
    };

    timeline.push(AppealTimelineEvent {
        event_type: "filed".to_string(),
        description: format!("Appeal filed by {}", appeal.appellant),
        timestamp: filed_at,
        actor: Some(appeal.appellant.to_string()),
    });

    // Review started
    if let AppealStatus::UnderReview {
        review_started_at, ..
    }
    | AppealStatus::Hearing {
        hearing_started_at: review_started_at,
        ..
    }
    | AppealStatus::Resolved {
        resolved_at: review_started_at,
        ..
    } = &appeal.status
    {
        if matches!(
            appeal.status,
            AppealStatus::UnderReview { .. } | AppealStatus::Hearing { .. }
        ) {
            timeline.push(AppealTimelineEvent {
                event_type: "review_started".to_string(),
                description: "Appeal review began".to_string(),
                timestamp: *review_started_at,
                actor: None,
            });
        }
    }

    // Hearing started
    if let AppealStatus::Hearing {
        hearing_started_at, ..
    } = &appeal.status
    {
        timeline.push(AppealTimelineEvent {
            event_type: "hearing_started".to_string(),
            description: "Hearing period opened".to_string(),
            timestamp: *hearing_started_at,
            actor: None,
        });
    }

    // Evidence submissions
    for evidence in &appeal.evidence {
        timeline.push(AppealTimelineEvent {
            event_type: "evidence_added".to_string(),
            description: format!("Evidence submitted: {}", evidence.description),
            timestamp: evidence.submitted_at,
            actor: Some(evidence.submitted_by.to_string()),
        });
    }

    // Responses
    for response in &appeal.responses {
        timeline.push(AppealTimelineEvent {
            event_type: "response_added".to_string(),
            description: format!("Response filed by {}", response.responder),
            timestamp: response.submitted_at,
            actor: Some(response.responder.to_string()),
        });
    }

    // Resolution
    if let AppealStatus::Resolved {
        resolved_at,
        outcome,
        ..
    } = &appeal.status
    {
        timeline.push(AppealTimelineEvent {
            event_type: "resolved".to_string(),
            description: format!("Appeal resolved: {outcome:?}"),
            timestamp: *resolved_at,
            actor: None,
        });
    }

    // Dismissed
    if let AppealStatus::Dismissed {
        dismissed_at,
        reason,
        ..
    } = &appeal.status
    {
        timeline.push(AppealTimelineEvent {
            event_type: "dismissed".to_string(),
            description: format!("Appeal dismissed: {reason}"),
            timestamp: *dismissed_at,
            actor: None,
        });
    }

    // Withdrawn
    if let AppealStatus::Withdrawn {
        withdrawn_at,
        reason,
        ..
    } = &appeal.status
    {
        let desc = if let Some(r) = reason {
            format!("Appeal withdrawn: {r}")
        } else {
            "Appeal withdrawn".to_string()
        };
        timeline.push(AppealTimelineEvent {
            event_type: "withdrawn".to_string(),
            description: desc,
            timestamp: *withdrawn_at,
            actor: Some(appeal.appellant.to_string()),
        });
    }

    // Sort by timestamp
    timeline.sort_by_key(|e| e.timestamp);

    timeline
}

/// Build detailed status with next steps
fn build_status_detail(appeal: &Appeal, _caller: Option<&Did>) -> AppealStatusDetail {
    let now = icn_time::current_timestamp_secs();

    let (status, description, next_steps) = match &appeal.status {
        AppealStatus::Filed { .. } => (
            "filed".to_string(),
            "Appeal filed, awaiting review".to_string(),
            vec![
                "Wait for appeal body to begin review".to_string(),
                "You may submit additional evidence".to_string(),
            ],
        ),
        AppealStatus::UnderReview { .. } => (
            "under_review".to_string(),
            "Appeal under review by appeal body".to_string(),
            vec![
                "Wait for hearing to begin".to_string(),
                "You may submit additional evidence".to_string(),
            ],
        ),
        AppealStatus::Hearing {
            hearing_ends_at, ..
        } => {
            let time_left = (*hearing_ends_at as i64) - (now as i64);
            let desc = if time_left > 0 {
                format!("Hearing period active ({time_left} seconds remaining)")
            } else {
                "Hearing period ended, awaiting decision".to_string()
            };
            (
                "hearing".to_string(),
                desc,
                vec![
                    "Submit evidence before hearing closes".to_string(),
                    "Respond to any questions from appeal body".to_string(),
                ],
            )
        }
        AppealStatus::Resolved { outcome, .. } => (
            "resolved".to_string(),
            format!("Appeal resolved: {outcome:?}"),
            vec!["No further action required".to_string()],
        ),
        AppealStatus::Dismissed { reason, .. } => (
            "dismissed".to_string(),
            format!("Appeal dismissed: {reason}"),
            vec!["Appeal closed - cannot resubmit".to_string()],
        ),
        AppealStatus::Withdrawn { .. } => (
            "withdrawn".to_string(),
            "Appeal withdrawn by appellant".to_string(),
            vec!["No further action required".to_string()],
        ),
    };

    // Build deadlines
    let mut deadlines = Vec::new();

    if let AppealStatus::Hearing {
        hearing_ends_at, ..
    } = &appeal.status
    {
        let time_remaining = (*hearing_ends_at as i64) - (now as i64);
        deadlines.push(AppealDeadline {
            deadline_type: "hearing_end".to_string(),
            timestamp: *hearing_ends_at,
            is_passed: time_remaining <= 0,
            time_remaining: if time_remaining > 0 {
                Some(time_remaining)
            } else {
                None
            },
        });
    }

    if let Some(filing_deadline) = appeal.filing_deadline {
        let time_remaining = (filing_deadline as i64) - (now as i64);
        deadlines.push(AppealDeadline {
            deadline_type: "filing".to_string(),
            timestamp: filing_deadline,
            is_passed: time_remaining <= 0,
            time_remaining: if time_remaining > 0 {
                Some(time_remaining)
            } else {
                None
            },
        });
    }

    AppealStatusDetail {
        status,
        description,
        is_terminal: appeal.status.is_terminal(),
        is_active: appeal.status.is_active(),
        next_steps,
        deadlines,
    }
}

/// Build dashboard statistics
fn build_dashboard(appeals: &[Appeal]) -> AppealsDashboard {
    let total = appeals.len();
    let mut filed = 0;
    let mut under_review = 0;
    let mut hearing = 0;
    let mut resolved = 0;
    let mut dismissed = 0;
    let mut withdrawn = 0;

    let mut resolution_times = Vec::new();

    for appeal in appeals {
        match &appeal.status {
            AppealStatus::Filed { .. } => filed += 1,
            AppealStatus::UnderReview { .. } => under_review += 1,
            AppealStatus::Hearing { .. } => hearing += 1,
            AppealStatus::Resolved {
                filed_at,
                resolved_at,
                ..
            } => {
                resolved += 1;
                resolution_times.push(resolved_at - filed_at);
            }
            AppealStatus::Dismissed { .. } => dismissed += 1,
            AppealStatus::Withdrawn { .. } => withdrawn += 1,
        }
    }

    let active = filed + under_review + hearing;

    let avg_resolution_time = if !resolution_times.is_empty() {
        Some(resolution_times.iter().sum::<u64>() / resolution_times.len() as u64)
    } else {
        None
    };

    // Recent appeals (last 5)
    let mut sorted = appeals.to_vec();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    let recent_appeals = sorted
        .iter()
        .take(5)
        .map(|a| RecentAppeal {
            id: hex::encode(a.id.as_bytes()),
            appeal_type: format!("{:?}", a.appeal_type),
            appellant: a.appellant.to_string(),
            status: match &a.status {
                AppealStatus::Filed { .. } => "filed".to_string(),
                AppealStatus::UnderReview { .. } => "under_review".to_string(),
                AppealStatus::Hearing { .. } => "hearing".to_string(),
                AppealStatus::Resolved { .. } => "resolved".to_string(),
                AppealStatus::Dismissed { .. } => "dismissed".to_string(),
                AppealStatus::Withdrawn { .. } => "withdrawn".to_string(),
            },
            created_at: a.created_at,
        })
        .collect();

    AppealsDashboard {
        total_appeals: total,
        active_appeals: active,
        filed_count: filed,
        under_review_count: under_review,
        hearing_count: hearing,
        resolved_count: resolved,
        dismissed_count: dismissed,
        withdrawn_count: withdrawn,
        avg_resolution_time,
        recent_appeals,
    }
}

/// Configure appeals UI routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_appeal_timeline)
        .service(get_appeal_status)
        .service(assign_reviewer)
        .service(get_appeals_dashboard);
}
