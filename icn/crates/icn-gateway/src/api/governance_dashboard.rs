//! Governance Dashboard API
//!
//! Aggregate governance statistics and activity for domain dashboards.

use actix_web::{get, web, HttpRequest, HttpResponse};
use icn_governance::{Amendment, AmendmentStatus, Appeal, AppealStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::commons_mgr::CommonsManager;
use crate::error::Result;
use crate::middleware::require_scope;

/// Governance dashboard data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GovernanceDashboard {
    /// Charter ID (hex)
    pub charter_id: Option<String>,
    /// Pending amendments count
    pub pending_amendments: usize,
    /// Open appeals count
    pub open_appeals: usize,
    /// Recent activity events
    pub recent_activity: Vec<ActivityEvent>,
    /// Amendments breakdown
    pub amendments_breakdown: AmendmentsBreakdown,
    /// Appeals breakdown
    pub appeals_breakdown: AppealsBreakdown,
}

/// Amendments breakdown by status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AmendmentsBreakdown {
    pub draft: usize,
    pub submitted: usize,
    pub voting: usize,
    pub ratified: usize,
    pub rejected: usize,
    pub withdrawn: usize,
}

/// Appeals breakdown by status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppealsBreakdown {
    pub filed: usize,
    pub under_review: usize,
    pub hearing: usize,
    pub resolved: usize,
    pub dismissed: usize,
    pub withdrawn: usize,
}

/// Recent activity event
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivityEvent {
    /// Event type
    pub event_type: String,
    /// Event description
    pub description: String,
    /// Timestamp
    pub timestamp: u64,
    /// Resource ID (proposal/amendment/appeal)
    pub resource_id: String,
    /// Resource type
    pub resource_type: String,
}

/// GET /governance/{charter_id}/dashboard - Governance dashboard
#[get("/governance/{charter_id}/dashboard")]
pub async fn get_governance_dashboard(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    charter_id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let charter_id_str = charter_id.into_inner();

    // Get all amendments (no filter by charter since Amendment doesn't have charter_id field)
    let amendments = commons_mgr.list_amendments(None, None, None).await?;

    // Get all appeals
    let appeals = commons_mgr.list_appeals(None, None, None).await?;

    // Build dashboard
    let dashboard = build_dashboard(&charter_id_str, &amendments, &appeals);

    Ok(HttpResponse::Ok().json(dashboard))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Build dashboard from governance data
fn build_dashboard(
    charter_id: &str,
    amendments: &[Amendment],
    appeals: &[Appeal],
) -> GovernanceDashboard {
    // Amendments breakdown
    let mut amendments_breakdown = AmendmentsBreakdown {
        draft: 0,
        submitted: 0,
        voting: 0,
        ratified: 0,
        rejected: 0,
        withdrawn: 0,
    };

    let mut pending_amendments = 0;

    for amendment in amendments {
        match &amendment.status {
            AmendmentStatus::Draft => {
                amendments_breakdown.draft += 1;
                pending_amendments += 1;
            }
            AmendmentStatus::Submitted { .. } | AmendmentStatus::UnderReview { .. } => {
                amendments_breakdown.submitted += 1;
                pending_amendments += 1;
            }
            AmendmentStatus::Voting { .. } | AmendmentStatus::Ratifying { .. } => {
                amendments_breakdown.voting += 1;
                pending_amendments += 1;
            }
            AmendmentStatus::Ratified { .. } => amendments_breakdown.ratified += 1,
            AmendmentStatus::Rejected { .. } => amendments_breakdown.rejected += 1,
            AmendmentStatus::Withdrawn { .. } => amendments_breakdown.withdrawn += 1,
            _ => {}
        }
    }

    // Appeals breakdown
    let mut appeals_breakdown = AppealsBreakdown {
        filed: 0,
        under_review: 0,
        hearing: 0,
        resolved: 0,
        dismissed: 0,
        withdrawn: 0,
    };

    let mut open_appeals = 0;

    for appeal in appeals {
        match &appeal.status {
            AppealStatus::Filed { .. } => {
                appeals_breakdown.filed += 1;
                open_appeals += 1;
            }
            AppealStatus::UnderReview { .. } => {
                appeals_breakdown.under_review += 1;
                open_appeals += 1;
            }
            AppealStatus::Hearing { .. } => {
                appeals_breakdown.hearing += 1;
                open_appeals += 1;
            }
            AppealStatus::Resolved { .. } => appeals_breakdown.resolved += 1,
            AppealStatus::Dismissed { .. } => appeals_breakdown.dismissed += 1,
            AppealStatus::Withdrawn { .. } => appeals_breakdown.withdrawn += 1,
        }
    }

    // Recent activity (last 10 events)
    let mut activity = Vec::new();

    for amendment in amendments.iter().take(5) {
        let timestamp = match &amendment.status {
            AmendmentStatus::Draft => amendment.created_at,
            AmendmentStatus::Submitted { submitted_at, .. } => *submitted_at,
            AmendmentStatus::Voting {
                voting_started_at, ..
            } => *voting_started_at,
            AmendmentStatus::Ratified { ratified_at, .. } => *ratified_at,
            AmendmentStatus::Rejected { rejected_at, .. } => *rejected_at,
            AmendmentStatus::Withdrawn { withdrawn_at, .. } => *withdrawn_at,
            _ => amendment.created_at,
        };

        activity.push(ActivityEvent {
            event_type: "amendment".to_string(),
            description: format!("Amendment: {}", amendment.title),
            timestamp,
            resource_id: hex::encode(amendment.id.as_bytes()),
            resource_type: "amendment".to_string(),
        });
    }

    for appeal in appeals.iter().take(5) {
        let timestamp = appeal.created_at;

        activity.push(ActivityEvent {
            event_type: "appeal".to_string(),
            description: format!("Appeal filed: {:?}", appeal.appeal_type),
            timestamp,
            resource_id: hex::encode(appeal.id.as_bytes()),
            resource_type: "appeal".to_string(),
        });
    }

    // Sort by timestamp descending
    activity.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
    activity.truncate(10);

    GovernanceDashboard {
        charter_id: Some(charter_id.to_string()),
        pending_amendments,
        open_appeals,
        recent_activity: activity,
        amendments_breakdown,
        appeals_breakdown,
    }
}

/// Configure governance dashboard routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_governance_dashboard);
}
