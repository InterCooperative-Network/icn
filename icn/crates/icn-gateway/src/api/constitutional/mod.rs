//! Constitutional Governance API endpoints
//!
//! REST API for managing amendments and appeals within the ICN commons.
//! Implements v0.6.0 Constitutional Governance from the Commons Evolution RFC.
//!
//! # Amendment Endpoints
//! - POST /v1/constitutional/amendments - Create amendment
//! - GET /v1/constitutional/amendments - List amendments
//! - GET /v1/constitutional/amendments/{id} - Get amendment
//! - POST /v1/constitutional/amendments/{id}/changes - Add change to draft
//! - POST /v1/constitutional/amendments/{id}/submit - Submit for review
//! - POST /v1/constitutional/amendments/{id}/vote - Open voting
//! - POST /v1/constitutional/amendments/{id}/ratify - Add ratification
//! - POST /v1/constitutional/amendments/{id}/withdraw - Withdraw amendment
//!
//! # Voting Endpoints (UI-friendly)
//! - POST /v1/constitutional/amendments/{id}/votes - Cast a vote
//! - GET /v1/constitutional/amendments/{id}/votes - List all votes
//! - GET /v1/constitutional/amendments/{id}/results - Get voting results
//! - GET /v1/constitutional/amendments/{id}/my-vote - Get user's vote status
//!
//! # Appeal Endpoints
//! - POST /v1/constitutional/appeals - File appeal
//! - GET /v1/constitutional/appeals - List appeals
//! - GET /v1/constitutional/appeals/{id} - Get appeal
//! - POST /v1/constitutional/appeals/{id}/evidence - Add evidence
//! - POST /v1/constitutional/appeals/{id}/respond - Add response
//! - POST /v1/constitutional/appeals/{id}/review - Begin review
//! - POST /v1/constitutional/appeals/{id}/resolve - Resolve appeal
//! - POST /v1/constitutional/appeals/{id}/withdraw - Withdraw appeal
//!
//! # Appeal UI Endpoints
//! - GET /v1/constitutional/appeals/{id}/timeline - Appeal event timeline
//! - GET /v1/constitutional/appeals/{id}/status - Detailed status with next steps
//! - POST /v1/constitutional/appeals/{id}/assign-reviewer - Assign reviewer (admin)
//! - GET /v1/constitutional/appeals/dashboard - Appeals dashboard data

pub mod appeals_ui;
pub mod voting;

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};
use crate::pagination::{paginate_items, Cursored, PaginatedList, PaginationRequest};
use icn_governance::{
    Amendment, AmendmentChange, AmendmentId, AmendmentScope, AmendmentType, Appeal, AppealEvidence,
    AppealGrounds, AppealId, AppealOutcome, AppealRemedy, AppealResponse, AppealScope, AppealType,
    ChangeTarget, ChangeType, EvidenceType, Ratification, RatifierType, ResponseType,
};
use icn_identity::Did;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create an amendment
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAmendmentRequest {
    pub amendment_type: String,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub title: String,
    pub description: String,
    pub charter_id: Option<String>,
    pub changes: Vec<AmendmentChangeRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AmendmentChangeRequest {
    pub target: String,
    pub change_type: String,
    pub description: String,
    pub old_value: Option<String>,
    pub new_value: String,
}

/// Request to add a ratification
#[derive(Debug, Deserialize, ToSchema)]
pub struct RatifyRequest {
    pub ratifier_id: String,
    pub ratifier_type: String,
    pub approved: bool,
    pub comment: Option<String>,
    pub signature: String,
}

/// Request to file an appeal
#[derive(Debug, Deserialize, ToSchema)]
pub struct FileAppealRequest {
    pub appeal_type: AppealTypeRequest,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub grounds: Vec<AppealGroundsRequest>,
    pub statement: String,
    pub requested_remedy: String,
    pub remedy_details: Option<String>,
    pub respondent_did: Option<String>,
    pub original_decision_ref: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AppealTypeRequest {
    pub category: String,
    pub revocation_id: Option<String>,
    pub target_id: Option<String>,
    pub proposal_id: Option<String>,
    pub dispute_id: Option<String>,
    pub steward_did: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AppealGroundsRequest {
    pub ground_type: String,
    pub description: String,
}

/// Request to add evidence
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddEvidenceRequest {
    pub evidence_type: String,
    pub description: String,
    pub content_hash: Option<String>,
    pub uri: Option<String>,
}

/// Request to add response
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddResponseRequest {
    pub response_type: String,
    pub content: String,
}

/// Request to resolve appeal
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveAppealRequest {
    pub outcome: String,
    pub reason: String,
    pub remedy: Option<String>,
    pub remedy_details: Option<String>,
}

/// Amendment response
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AmendmentResponse {
    pub id: String,
    pub amendment_type: String,
    pub scope: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub proposer: String,
    pub sponsors: Vec<String>,
    pub changes: Vec<AmendmentChangeResponse>,
    pub ratifications_count: usize,
    pub approvals_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Cursored for AmendmentResponse {
    fn cursor_timestamp(&self) -> u64 {
        // Use updated_at in milliseconds for cursor
        self.updated_at * 1000
    }

    fn cursor_id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AmendmentChangeResponse {
    pub target: String,
    pub change_type: String,
    pub description: String,
    pub old_value: Option<String>,
    pub new_value: String,
}

/// Appeal response
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AppealResponse2 {
    pub id: String,
    pub appeal_type: String,
    pub scope: String,
    pub status: String,
    pub appellant: String,
    pub respondent: Option<String>,
    pub grounds: Vec<String>,
    pub statement: String,
    pub requested_remedy: String,
    pub evidence_count: usize,
    pub responses_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Cursored for AppealResponse2 {
    fn cursor_timestamp(&self) -> u64 {
        // Use updated_at in milliseconds for cursor
        self.updated_at * 1000
    }

    fn cursor_id(&self) -> &str {
        &self.id
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_amendment_type(s: &str) -> Result<AmendmentType> {
    match s.to_lowercase().as_str() {
        "charter" => Ok(AmendmentType::Charter),
        "constitutional" => Ok(AmendmentType::Constitutional),
        "policy" => Ok(AmendmentType::Policy),
        "economic" => Ok(AmendmentType::Economic),
        "governance" => Ok(AmendmentType::Governance),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid amendment type: {s}"
        ))),
    }
}

fn parse_amendment_scope(scope_type: &str, scope_id: Option<&str>) -> Result<AmendmentScope> {
    match scope_type.to_lowercase().as_str() {
        "jurisdiction" => {
            let id = scope_id.ok_or_else(|| {
                GatewayError::BadRequest("scope_id required for jurisdiction scope".to_string())
            })?;
            Ok(AmendmentScope::Jurisdiction {
                domain_id: id.to_string(),
            })
        }
        "federation" => {
            let id = scope_id.ok_or_else(|| {
                GatewayError::BadRequest("scope_id required for federation scope".to_string())
            })?;
            Ok(AmendmentScope::Federation {
                federation_id: id.to_string(),
            })
        }
        "network" => Ok(AmendmentScope::Network),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid scope type: {scope_type}"
        ))),
    }
}

fn parse_change_target(s: &str) -> Result<ChangeTarget> {
    match s.to_lowercase().as_str() {
        "governance_rules" | "governancerules" => Ok(ChangeTarget::GovernanceRules),
        "membership_policy" | "membershippolicy" => Ok(ChangeTarget::MembershipPolicy),
        "economic_policy" | "economicpolicy" => Ok(ChangeTarget::EconomicPolicy),
        "dispute_policy" | "disputepolicy" => Ok(ChangeTarget::DisputePolicy),
        "commons_rights" | "commonsrights" => Ok(ChangeTarget::CommonsRights),
        "steward_requirements" | "stewardrequirements" => Ok(ChangeTarget::StewardRequirements),
        "network_parameters" | "networkparameters" => Ok(ChangeTarget::NetworkParameters),
        other => Ok(ChangeTarget::Other {
            name: other.to_string(),
        }),
    }
}

fn parse_change_type(s: &str) -> Result<ChangeType> {
    match s.to_lowercase().as_str() {
        "add" => Ok(ChangeType::Add),
        "modify" => Ok(ChangeType::Modify),
        "remove" => Ok(ChangeType::Remove),
        "replace" => Ok(ChangeType::Replace),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid change type: {s}"
        ))),
    }
}

fn parse_ratifier_type(s: &str) -> Result<RatifierType> {
    match s.to_lowercase().as_str() {
        "commons_holder" | "commonsholder" => Ok(RatifierType::CommonsHolder),
        "jurisdiction" => Ok(RatifierType::Jurisdiction),
        "federation" => Ok(RatifierType::Federation),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid ratifier type: {s}"
        ))),
    }
}

fn parse_appeal_scope(scope_type: &str, scope_id: Option<&str>) -> Result<AppealScope> {
    match scope_type.to_lowercase().as_str() {
        "jurisdiction" => {
            let id = scope_id.ok_or_else(|| {
                GatewayError::BadRequest("scope_id required for jurisdiction scope".to_string())
            })?;
            Ok(AppealScope::Jurisdiction {
                domain_id: id.to_string(),
            })
        }
        "federation" => {
            let id = scope_id.ok_or_else(|| {
                GatewayError::BadRequest("scope_id required for federation scope".to_string())
            })?;
            Ok(AppealScope::Federation {
                federation_id: id.to_string(),
            })
        }
        "network" => Ok(AppealScope::Network),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid scope type: {scope_type}"
        ))),
    }
}

fn parse_appeal_type(req: &AppealTypeRequest) -> Result<AppealType> {
    match req.category.to_lowercase().as_str() {
        "revocation" => {
            let id_hex = req.revocation_id.as_ref().ok_or_else(|| {
                GatewayError::BadRequest("revocation_id required for revocation appeal".to_string())
            })?;
            let id_bytes = hex::decode(id_hex)
                .map_err(|e| GatewayError::BadRequest(format!("Invalid revocation_id hex: {e}")))?;
            let mut id = [0u8; 32];
            if id_bytes.len() != 32 {
                return Err(GatewayError::BadRequest(
                    "revocation_id must be 32 bytes".to_string(),
                ));
            }
            id.copy_from_slice(&id_bytes);
            Ok(AppealType::Revocation {
                revocation_id: id,
                revocation_type: req.details.clone().unwrap_or_else(|| "Unknown".to_string()),
            })
        }
        "suspension" => Ok(AppealType::Suspension {
            target_id: req
                .target_id
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            suspension_type: req.details.clone().unwrap_or_else(|| "Unknown".to_string()),
        }),
        "governance_decision" | "governancedecision" => Ok(AppealType::GovernanceDecision {
            proposal_id: req
                .proposal_id
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            decision: req.details.clone().unwrap_or_else(|| "Unknown".to_string()),
        }),
        "dispute_resolution" | "disputeresolution" => Ok(AppealType::DisputeResolution {
            dispute_id: req
                .dispute_id
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            resolution: req.details.clone().unwrap_or_else(|| "Unknown".to_string()),
        }),
        "membership_denial" | "membershipdenial" => Ok(AppealType::MembershipDenial {
            jurisdiction_id: req
                .target_id
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            application_id: req.details.clone(),
        }),
        "steward_action" | "stewardaction" => {
            let did: Did = req
                .steward_did
                .as_ref()
                .ok_or_else(|| {
                    GatewayError::BadRequest(
                        "steward_did required for steward action appeal".to_string(),
                    )
                })?
                .parse()
                .map_err(|e| GatewayError::BadRequest(format!("Invalid steward DID: {e}")))?;
            Ok(AppealType::StewardAction {
                steward_did: did,
                action: req.details.clone().unwrap_or_else(|| "Unknown".to_string()),
            })
        }
        other => Ok(AppealType::Other {
            category: other.to_string(),
            description: req
                .details
                .clone()
                .unwrap_or_else(|| "No description".to_string()),
        }),
    }
}

fn parse_appeal_grounds(req: &AppealGroundsRequest) -> Result<AppealGrounds> {
    match req.ground_type.to_lowercase().as_str() {
        "procedural_error" | "proceduralerror" => Ok(AppealGrounds::ProceduralError {
            description: req.description.clone(),
        }),
        "new_evidence" | "newevidence" => Ok(AppealGrounds::NewEvidence {
            description: req.description.clone(),
        }),
        "exceeded_authority" | "exceededauthority" => Ok(AppealGrounds::ExceededAuthority {
            description: req.description.clone(),
        }),
        "rights_violation" | "rightsviolation" => Ok(AppealGrounds::RightsViolation {
            right_violated: req.description.clone(),
        }),
        "factual_error" | "factualerror" => Ok(AppealGrounds::FactualError {
            description: req.description.clone(),
        }),
        "bias" => Ok(AppealGrounds::Bias {
            description: req.description.clone(),
        }),
        "disproportionate_penalty" | "disproportionatepenalty" => {
            Ok(AppealGrounds::DisproportionatePenalty {
                description: req.description.clone(),
            })
        }
        _ => Ok(AppealGrounds::Other {
            description: req.description.clone(),
        }),
    }
}

fn parse_appeal_remedy(remedy: &str, details: Option<&str>) -> AppealRemedy {
    match remedy.to_lowercase().as_str() {
        "reverse" => AppealRemedy::Reverse,
        "reinstate" => AppealRemedy::Reinstate,
        "modify" => AppealRemedy::Modify {
            modification: details.unwrap_or("").to_string(),
        },
        "compensation" => {
            // Parse "amount:currency" format
            if let Some(d) = details {
                let parts: Vec<&str> = d.split(':').collect();
                if parts.len() == 2 {
                    if let Ok(amount) = parts[0].parse() {
                        return AppealRemedy::Compensation {
                            amount,
                            currency: parts[1].to_string(),
                        };
                    }
                }
            }
            AppealRemedy::Custom {
                description: details.unwrap_or("Compensation requested").to_string(),
            }
        }
        "custom" => AppealRemedy::Custom {
            description: details.unwrap_or("").to_string(),
        },
        "none" | "" => AppealRemedy::None,
        _ => AppealRemedy::Custom {
            description: details.unwrap_or(remedy).to_string(),
        },
    }
}

fn parse_evidence_type(s: &str) -> EvidenceType {
    match s.to_lowercase().as_str() {
        "document" => EvidenceType::Document,
        "transaction" => EvidenceType::Transaction,
        "communication" => EvidenceType::Communication,
        "witness_statement" | "witnessstatement" => EvidenceType::WitnessStatement,
        "expert_opinion" | "expertopinion" => EvidenceType::ExpertOpinion,
        "technical" => EvidenceType::Technical,
        _ => EvidenceType::Other,
    }
}

fn parse_response_type(s: &str) -> ResponseType {
    match s.to_lowercase().as_str() {
        "initial_response" | "initialresponse" => ResponseType::InitialResponse,
        "reply" => ResponseType::Reply,
        "comment" => ResponseType::Comment,
        "question" => ResponseType::Question,
        "clarification" => ResponseType::Clarification,
        _ => ResponseType::Comment,
    }
}

fn parse_appeal_outcome(
    outcome: &str,
    reason: &str,
    remedy: Option<&str>,
    remedy_details: Option<&str>,
) -> Result<AppealOutcome> {
    match outcome.to_lowercase().as_str() {
        "upheld" => Ok(AppealOutcome::Upheld {
            reason: reason.to_string(),
            remedy: parse_appeal_remedy(remedy.unwrap_or("reverse"), remedy_details),
        }),
        "denied" => Ok(AppealOutcome::Denied {
            reason: reason.to_string(),
        }),
        "partially_upheld" | "partiallyupheld" => Ok(AppealOutcome::PartiallyUpheld {
            reason: reason.to_string(),
            remedy: parse_appeal_remedy(remedy.unwrap_or("modify"), remedy_details),
        }),
        "remanded" => Ok(AppealOutcome::Remanded {
            instructions: reason.to_string(),
            remanded_to: remedy_details.unwrap_or("original body").to_string(),
        }),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid appeal outcome: {outcome}"
        ))),
    }
}

fn amendment_to_response(amendment: &Amendment) -> AmendmentResponse {
    AmendmentResponse {
        id: amendment.id.to_hex(),
        amendment_type: format!("{}", amendment.amendment_type),
        scope: format!("{}", amendment.scope),
        status: format!("{:?}", amendment.status),
        title: amendment.title.clone(),
        description: amendment.description.clone(),
        proposer: amendment.proposer.to_string(),
        sponsors: amendment.sponsors.iter().map(|d| d.to_string()).collect(),
        changes: amendment
            .changes
            .iter()
            .map(|c| AmendmentChangeResponse {
                target: format!("{:?}", c.target),
                change_type: format!("{:?}", c.change_type),
                description: c.description.clone(),
                old_value: c.old_value.clone(),
                new_value: c.new_value.clone(),
            })
            .collect(),
        ratifications_count: amendment.ratifications.len(),
        approvals_count: amendment
            .ratifications
            .iter()
            .filter(|r| r.approved)
            .count(),
        created_at: amendment.created_at,
        updated_at: amendment.updated_at,
    }
}

fn appeal_to_response(appeal: &Appeal) -> AppealResponse2 {
    AppealResponse2 {
        id: appeal.id.to_hex(),
        appeal_type: format!("{}", appeal.appeal_type),
        scope: format!("{}", appeal.scope),
        status: format!("{:?}", appeal.status),
        appellant: appeal.appellant.to_string(),
        respondent: appeal.respondent.as_ref().map(|d| d.to_string()),
        grounds: appeal.grounds.iter().map(|g| format!("{g}")).collect(),
        statement: appeal.statement.clone(),
        requested_remedy: format!("{:?}", appeal.requested_remedy),
        evidence_count: appeal.evidence.len(),
        responses_count: appeal.responses.len(),
        created_at: appeal.created_at,
        updated_at: appeal.updated_at,
    }
}

// ============================================================================
// Amendment Endpoints
// ============================================================================

/// POST /constitutional/amendments - Create a new amendment
#[post("/amendments")]
pub async fn create_amendment(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    req: web::Json<CreateAmendmentRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let proposer: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Parse amendment type and scope
    let amendment_type = parse_amendment_type(&req.amendment_type)?;
    let scope = parse_amendment_scope(&req.scope_type, req.scope_id.as_deref())?;

    // Validate charter_id consistency before consuming `scope` into Amendment::new.
    //
    // When a charter_id is supplied alongside a Jurisdiction scope, the charter
    // must belong to exactly that domain.  Accepting a mismatched pair would make
    // the amendment's provenance ambiguous and produce a confusing substrate-level
    // error instead of a clear boundary-layer rejection.
    let validated_charter_id: Option<[u8; 32]> = if let Some(charter_hex) = &req.charter_id {
        let charter_bytes = hex::decode(charter_hex)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid charter_id hex: {e}")))?;
        if charter_bytes.len() != 32 {
            return Err(GatewayError::BadRequest(
                "charter_id must be 32 bytes".to_string(),
            ));
        }
        let mut charter_id_bytes = [0u8; 32];
        charter_id_bytes.copy_from_slice(&charter_bytes);

        if let AmendmentScope::Jurisdiction {
            domain_id: ref scope_domain,
        } = scope
        {
            let charter = commons_mgr
                .get_charter(charter_hex)
                .await
                .map_err(|e| GatewayError::InternalError(e.to_string()))?
                .ok_or_else(|| {
                    GatewayError::BadRequest(format!("charter_id '{charter_hex}' does not exist"))
                })?;
            if charter.domain_id != *scope_domain {
                return Err(GatewayError::BadRequest(format!(
                    "charter_id belongs to domain '{}' but amendment scope specifies '{}'; \
                     charter_id and scope_id must refer to the same domain",
                    charter.domain_id, scope_domain,
                )));
            }
        }

        Some(charter_id_bytes)
    } else {
        None
    };

    // Create amendment
    let mut amendment = Amendment::new(
        amendment_type,
        scope,
        req.title.clone(),
        req.description.clone(),
        proposer,
    );
    amendment.charter_id = validated_charter_id;

    // Add changes
    for change_req in &req.changes {
        let change = AmendmentChange {
            target: parse_change_target(&change_req.target)?,
            change_type: parse_change_type(&change_req.change_type)?,
            description: change_req.description.clone(),
            old_value: change_req.old_value.clone(),
            new_value: change_req.new_value.clone(),
        };
        amendment.add_change(change);
    }

    // Store amendment
    commons_mgr.store_amendment(amendment.clone()).await?;

    Ok(HttpResponse::Created().json(amendment_to_response(&amendment)))
}

/// Query parameters for listing amendments (with cursor pagination)
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListAmendmentsQuery {
    /// Filter by status
    pub status: Option<String>,
    /// Filter by scope
    pub scope: Option<String>,
    /// Filter by type
    #[serde(rename = "type")]
    pub amendment_type: Option<String>,
    /// Restrict results to amendments scoped to this domain (exact match).
    /// When supplied, only Jurisdiction-scoped amendments for this domain_id
    /// are returned.  Prevents cross-domain data leakage in multi-coop nodes.
    pub domain_id: Option<String>,
    /// Cursor for pagination
    pub cursor: Option<String>,
    /// Number of items per page (default: 20, max: 100)
    #[serde(default = "default_page_limit")]
    pub limit: usize,
}

fn default_page_limit() -> usize {
    crate::pagination::DEFAULT_PAGE_SIZE
}

/// GET /constitutional/amendments - List amendments
///
/// Supports cursor-based pagination for efficient navigation.
///
/// Query parameters:
/// - `status`: Filter by amendment status
/// - `scope`: Filter by scope
/// - `type`: Filter by amendment type
/// - `cursor`: Pagination cursor from previous response
/// - `limit`: Number of items per page (default: 20, max: 100)
#[get("/amendments")]
pub async fn list_amendments(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    query: web::Query<ListAmendmentsQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:read")?;

    let amendments = if let Some(ref domain) = query.domain_id {
        // Domain-scoped query: exact match, no cross-domain leakage.
        commons_mgr.list_amendments_by_domain(domain).await?
    } else {
        commons_mgr
            .list_amendments(
                query.status.as_deref(),
                query.scope.as_deref(),
                query.amendment_type.as_deref(),
            )
            .await?
    };

    // Convert to responses and sort by updated_at descending (most recent first)
    let mut responses: Vec<AmendmentResponse> =
        amendments.iter().map(amendment_to_response).collect();
    responses.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    // Apply cursor pagination
    let pagination_request = PaginationRequest {
        cursor: query.cursor.clone(),
        limit: query.limit,
        direction: crate::pagination::Direction::Forward,
    }
    .validate();

    let paginated: PaginatedList<AmendmentResponse> =
        paginate_items(responses, &pagination_request);

    Ok(HttpResponse::Ok().json(paginated))
}

/// GET /constitutional/amendments/{id} - Get amendment by ID
#[get("/amendments/{id}")]
pub async fn get_amendment(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:read")?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id = [0u8; 32];
    amendment_id.copy_from_slice(&id_bytes);

    let amendment = commons_mgr
        .get_amendment(&AmendmentId::new(amendment_id))
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("Amendment not found: {id_hex}")))?;

    Ok(HttpResponse::Ok().json(amendment_to_response(&amendment)))
}

/// POST /constitutional/amendments/{id}/submit - Submit amendment for review
#[post("/amendments/{id}/submit")]
pub async fn submit_amendment(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id = [0u8; 32];
    amendment_id.copy_from_slice(&id_bytes);

    let amendment = commons_mgr
        .submit_amendment(&AmendmentId::new(amendment_id), &caller)
        .await?;

    Ok(HttpResponse::Ok().json(amendment_to_response(&amendment)))
}

/// POST /constitutional/amendments/{id}/vote - Open voting period
#[post("/amendments/{id}/vote")]
pub async fn open_amendment_voting(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id = [0u8; 32];
    amendment_id.copy_from_slice(&id_bytes);

    let amendment = commons_mgr
        .open_amendment_voting(&AmendmentId::new(amendment_id), &caller)
        .await?;

    Ok(HttpResponse::Ok().json(amendment_to_response(&amendment)))
}

/// POST /constitutional/amendments/{id}/ratify - Add ratification
#[post("/amendments/{id}/ratify")]
pub async fn ratify_amendment(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<RatifyRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let authority: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id = [0u8; 32];
    amendment_id.copy_from_slice(&id_bytes);

    let signature = hex::decode(&req.signature)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature hex: {e}")))?;

    let ratification = Ratification {
        ratifier_id: req.ratifier_id.clone(),
        ratifier_type: parse_ratifier_type(&req.ratifier_type)?,
        authority_did: authority,
        approved: req.approved,
        timestamp: icn_time::current_timestamp_secs(),
        comment: req.comment.clone(),
        signature,
    };

    let amendment = commons_mgr
        .add_amendment_ratification(&AmendmentId::new(amendment_id), ratification)
        .await?;

    Ok(HttpResponse::Ok().json(amendment_to_response(&amendment)))
}

/// POST /constitutional/amendments/{id}/changes - Add a change to a draft amendment
#[post("/amendments/{id}/changes")]
pub async fn add_amendment_change(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<AmendmentChangeRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let id_hex = id.into_inner();

    // Verify caller is proposer (get amendment first)
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id_arr = [0u8; 32];
    amendment_id_arr.copy_from_slice(&id_bytes);
    let amendment_id_for_lookup = AmendmentId::new(amendment_id_arr);
    let existing = commons_mgr
        .get_amendment(&amendment_id_for_lookup)
        .await?
        .ok_or_else(|| GatewayError::NotFound("Amendment not found".to_string()))?;

    if existing.proposer != caller {
        return Err(GatewayError::AuthorizationFailed(
            "Only the proposer can add changes to an amendment".to_string(),
        ));
    }

    // Parse change fields
    let change = AmendmentChange {
        target: parse_change_target(&req.target)?,
        change_type: parse_change_type(&req.change_type)?,
        description: req.description.clone(),
        old_value: req.old_value.clone(),
        new_value: req.new_value.clone(),
    };

    // Add change
    let amendment = commons_mgr.add_amendment_change(&id_hex, change).await?;

    Ok(HttpResponse::Ok().json(amendment_to_response(&amendment)))
}

/// POST /constitutional/amendments/{id}/withdraw - Withdraw amendment
#[post("/amendments/{id}/withdraw")]
pub async fn withdraw_amendment(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let id_hex = id.into_inner();
    let id_bytes = hex::decode(&id_hex)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid amendment ID hex: {e}")))?;
    if id_bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Amendment ID must be 32 bytes".to_string(),
        ));
    }
    let mut amendment_id = [0u8; 32];
    amendment_id.copy_from_slice(&id_bytes);

    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("Withdrawn by proposer")
        .to_string();

    let amendment = commons_mgr
        .withdraw_amendment(&AmendmentId::new(amendment_id), &caller, reason)
        .await?;

    Ok(HttpResponse::Ok().json(amendment_to_response(&amendment)))
}

// ============================================================================
// Appeal Endpoints
// ============================================================================

/// POST /constitutional/appeals - File an appeal
#[post("/appeals")]
pub async fn file_appeal(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    req: web::Json<FileAppealRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let appellant: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Parse appeal type and scope
    let appeal_type = parse_appeal_type(&req.appeal_type)?;
    let scope = parse_appeal_scope(&req.scope_type, req.scope_id.as_deref())?;

    // Parse grounds
    let grounds: Result<Vec<AppealGrounds>> =
        req.grounds.iter().map(parse_appeal_grounds).collect();
    let grounds = grounds?;

    if grounds.is_empty() {
        return Err(GatewayError::BadRequest(
            "At least one ground for appeal is required".to_string(),
        ));
    }

    // Parse remedy
    let remedy = parse_appeal_remedy(&req.requested_remedy, req.remedy_details.as_deref());

    // Create appeal
    let mut appeal = Appeal::new(
        appeal_type,
        scope,
        appellant,
        grounds,
        req.statement.clone(),
        remedy,
    );

    // Set respondent if provided
    if let Some(respondent_str) = &req.respondent_did {
        let respondent: Did = respondent_str
            .parse()
            .map_err(|e| GatewayError::BadRequest(format!("Invalid respondent DID: {e}")))?;
        appeal.respondent = Some(respondent);
    }

    // Set decision reference if provided
    if let Some(ref decision_ref) = req.original_decision_ref {
        appeal.original_decision_ref = Some(decision_ref.clone());
    }

    // Store appeal
    commons_mgr.store_appeal(appeal.clone()).await?;

    Ok(HttpResponse::Created().json(appeal_to_response(&appeal)))
}

/// Query parameters for listing appeals (with cursor pagination)
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListAppealsQuery {
    /// Filter by status
    pub status: Option<String>,
    /// Filter by scope
    pub scope: Option<String>,
    /// Filter by appellant
    pub appellant: Option<String>,
    /// Restrict results to appeals scoped to this domain (exact match).
    /// When supplied, only Jurisdiction-scoped appeals for this domain_id
    /// are returned.  Prevents cross-domain data leakage in multi-coop nodes.
    pub domain_id: Option<String>,
    /// Cursor for pagination
    pub cursor: Option<String>,
    /// Number of items per page (default: 20, max: 100)
    #[serde(default = "default_page_limit")]
    pub limit: usize,
}

/// GET /constitutional/appeals - List appeals
///
/// Supports cursor-based pagination for efficient navigation.
///
/// Query parameters:
/// - `status`: Filter by appeal status
/// - `scope`: Filter by scope
/// - `appellant`: Filter by appellant DID
/// - `cursor`: Pagination cursor from previous response
/// - `limit`: Number of items per page (default: 20, max: 100)
#[get("/appeals")]
pub async fn list_appeals(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    query: web::Query<ListAppealsQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:read")?;

    let appeals = if let Some(ref domain) = query.domain_id {
        // Domain-scoped query: exact match, no cross-domain leakage.
        commons_mgr.list_appeals_by_domain(domain).await?
    } else {
        commons_mgr
            .list_appeals(
                query.status.as_deref(),
                query.scope.as_deref(),
                query.appellant.as_deref(),
            )
            .await?
    };

    // Convert to responses and sort by updated_at descending (most recent first)
    let mut responses: Vec<AppealResponse2> = appeals.iter().map(appeal_to_response).collect();
    responses.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    // Apply cursor pagination
    let pagination_request = PaginationRequest {
        cursor: query.cursor.clone(),
        limit: query.limit,
        direction: crate::pagination::Direction::Forward,
    }
    .validate();

    let paginated: PaginatedList<AppealResponse2> = paginate_items(responses, &pagination_request);

    Ok(HttpResponse::Ok().json(paginated))
}

/// GET /constitutional/appeals/{id} - Get appeal by ID
#[get("/appeals/{id}")]
pub async fn get_appeal(
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

    Ok(HttpResponse::Ok().json(appeal_to_response(&appeal)))
}

/// POST /constitutional/appeals/{id}/evidence - Add evidence
#[post("/appeals/{id}/evidence")]
pub async fn add_appeal_evidence(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<AddEvidenceRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let submitter: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

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

    // Parse content hash if provided
    let content_hash = if let Some(hash_hex) = &req.content_hash {
        let hash_bytes = hex::decode(hash_hex)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid content_hash hex: {e}")))?;
        if hash_bytes.len() != 32 {
            return Err(GatewayError::BadRequest(
                "content_hash must be 32 bytes".to_string(),
            ));
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&hash_bytes);
        Some(hash)
    } else {
        None
    };

    // Generate evidence ID
    let now = icn_time::current_timestamp_secs();
    let evidence_content = format!("{}:{}:{}", id_hex, req.description, now);
    let evidence_id = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(evidence_content.as_bytes());
        let result = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&result);
        id
    };

    let evidence = AppealEvidence {
        evidence_id,
        evidence_type: parse_evidence_type(&req.evidence_type),
        description: req.description.clone(),
        content_hash,
        uri: req.uri.clone(),
        submitted_by: submitter,
        submitted_at: now,
    };

    let appeal = commons_mgr
        .add_appeal_evidence(&AppealId::new(appeal_id), evidence)
        .await?;

    Ok(HttpResponse::Ok().json(appeal_to_response(&appeal)))
}

/// POST /constitutional/appeals/{id}/respond - Add response
#[post("/appeals/{id}/respond")]
pub async fn add_appeal_response(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<AddResponseRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let responder: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

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

    let now = icn_time::current_timestamp_secs();

    let response = AppealResponse {
        responder,
        response_type: parse_response_type(&req.response_type),
        content: req.content.clone(),
        evidence: Vec::new(),
        submitted_at: now,
    };

    let appeal = commons_mgr
        .add_appeal_response(&AppealId::new(appeal_id), response)
        .await?;

    Ok(HttpResponse::Ok().json(appeal_to_response(&appeal)))
}

/// POST /constitutional/appeals/{id}/review - Begin review
#[post("/appeals/{id}/review")]
pub async fn begin_appeal_review(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
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

    let appeal = commons_mgr
        .begin_appeal_review(&AppealId::new(appeal_id))
        .await?;

    Ok(HttpResponse::Ok().json(appeal_to_response(&appeal)))
}

/// POST /constitutional/appeals/{id}/resolve - Resolve appeal
#[post("/appeals/{id}/resolve")]
pub async fn resolve_appeal(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    req: web::Json<ResolveAppealRequest>,
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

    let outcome = parse_appeal_outcome(
        &req.outcome,
        &req.reason,
        req.remedy.as_deref(),
        req.remedy_details.as_deref(),
    )?;

    let appeal = commons_mgr
        .resolve_constitutional_appeal(&AppealId::new(appeal_id), outcome)
        .await?;

    Ok(HttpResponse::Ok().json(appeal_to_response(&appeal)))
}

/// POST /constitutional/appeals/{id}/withdraw - Withdraw appeal
#[post("/appeals/{id}/withdraw")]
pub async fn withdraw_appeal(
    http_req: HttpRequest,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    id: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "constitutional:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let caller: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

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

    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .map(String::from);

    let appeal = commons_mgr
        .withdraw_appeal(&AppealId::new(appeal_id), &caller, reason)
        .await?;

    Ok(HttpResponse::Ok().json(appeal_to_response(&appeal)))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure constitutional governance routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_amendment)
        .service(list_amendments)
        .service(get_amendment)
        .service(submit_amendment)
        .service(open_amendment_voting)
        .service(ratify_amendment)
        .service(add_amendment_change)
        .service(withdraw_amendment)
        .service(file_appeal)
        .service(list_appeals)
        .service(get_appeal)
        .service(add_appeal_evidence)
        .service(add_appeal_response)
        .service(begin_appeal_review)
        .service(resolve_appeal)
        .service(withdraw_appeal);

    // UI-friendly voting endpoints
    voting::configure(cfg);

    // UI-friendly appeals management endpoints
    appeals_ui::configure(cfg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_amendment_type() {
        assert!(matches!(
            parse_amendment_type("charter").unwrap(),
            AmendmentType::Charter
        ));
        assert!(matches!(
            parse_amendment_type("Constitutional").unwrap(),
            AmendmentType::Constitutional
        ));
        assert!(parse_amendment_type("invalid").is_err());
    }

    #[test]
    fn test_parse_amendment_scope() {
        let scope = parse_amendment_scope("jurisdiction", Some("coop:test")).unwrap();
        assert!(matches!(
            scope,
            AmendmentScope::Jurisdiction { domain_id } if domain_id == "coop:test"
        ));

        let scope = parse_amendment_scope("network", None).unwrap();
        assert!(matches!(scope, AmendmentScope::Network));

        assert!(parse_amendment_scope("jurisdiction", None).is_err());
    }

    #[test]
    fn test_parse_change_target() {
        assert!(matches!(
            parse_change_target("governance_rules").unwrap(),
            ChangeTarget::GovernanceRules
        ));
        assert!(matches!(
            parse_change_target("custom").unwrap(),
            ChangeTarget::Other { name } if name == "custom"
        ));
    }

    #[test]
    fn test_parse_appeal_remedy() {
        assert!(matches!(
            parse_appeal_remedy("reverse", None),
            AppealRemedy::Reverse
        ));
        assert!(matches!(
            parse_appeal_remedy("reinstate", None),
            AppealRemedy::Reinstate
        ));
        assert!(matches!(
            parse_appeal_remedy("compensation", Some("100:credits")),
            AppealRemedy::Compensation { amount: 100, currency } if currency == "credits"
        ));
    }
}
