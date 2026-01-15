//! Inter-Cooperative Agreement API endpoints
//!
//! RESTful API for managing inter-cooperative agreements including:
//! - Agreement lifecycle (create, propose, sign, suspend, resume, terminate)
//! - Party management
//! - Amendment workflow
//! - Status queries

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to create a new agreement draft
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAgreementRequest {
    /// Agreement title
    pub title: String,
    /// Agreement description
    pub description: String,
    /// Type of agreement
    pub agreement_type: AgreementTypeRequest,
}

/// Agreement type configuration
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgreementTypeRequest {
    Trade {
        items: Vec<TradeItemRequest>,
        currency: String,
    },
    Credit {
        credit_limit: i64,
        interest_rate_bps: u32,
        currency: String,
    },
    ResourceSharing {
        resource_type: String,
        duration_days: u32,
        compensation_model: String,
    },
    FederationMembership {
        federation_id: String,
        min_trust_threshold: f64,
        governance_binding: bool,
    },
    Custom {
        agreement_type_name: String,
        terms_json: String,
    },
}

/// Trade item in a trade agreement
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TradeItemRequest {
    pub description: String,
    pub quantity: u32,
    pub unit: String,
    pub unit_price: i64,
    pub currency: String,
}

/// Request to add a party to an agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddPartyRequest {
    /// DID of the party
    pub did: String,
    /// Cooperative ID
    pub coop_id: String,
    /// Role: proposer, counterparty, witness, guarantor
    pub role: String,
    /// Optional display name
    pub display_name: Option<String>,
}

/// Request to set agreement terms
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetTermsRequest {
    /// Effective date (Unix timestamp)
    pub effective_date: Option<u64>,
    /// Expiration date (Unix timestamp)
    pub expiration_date: Option<u64>,
    /// Termination notice period in days
    pub termination_notice_days: Option<u32>,
    /// Dispute resolution method
    pub dispute_resolution: Option<String>,
    /// Additional terms
    pub additional_terms: Option<String>,
}

/// Request to suspend an agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct SuspendRequest {
    /// Reason for suspension
    pub reason: String,
}

/// Request to terminate an agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct TerminateRequest {
    /// Termination reason type: expired, mutual_consent, breach, withdrawal, force_majeure
    pub reason_type: String,
    /// Additional details
    pub details: Option<String>,
}

/// Request to propose an amendment
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProposeAmendmentRequest {
    /// Description of the amendment
    pub description: String,
    /// List of changes
    pub changes: Vec<AmendmentChangeRequest>,
}

/// Amendment change specification
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "change_type", rename_all = "snake_case")]
pub enum AmendmentChangeRequest {
    ExtendDuration {
        new_expiration: u64,
    },
    UpdateTerm {
        field: String,
        new_value: String,
    },
    AddParty {
        did: String,
        coop_id: String,
        role: String,
    },
    RemoveParty {
        party_did: String,
    },
}

/// Agreement list response
#[derive(Debug, Serialize, ToSchema)]
pub struct AgreementListResponse {
    pub agreements: Vec<AgreementSummary>,
    pub total: usize,
}

/// Agreement summary for lists
#[derive(Debug, Serialize, ToSchema)]
pub struct AgreementSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub agreement_type: String,
    pub parties_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Agreement detail response
#[derive(Debug, Serialize, ToSchema)]
pub struct AgreementDetailResponse {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub status_details: serde_json::Value,
    pub agreement_type: String,
    pub agreement_type_details: serde_json::Value,
    pub parties: Vec<PartyInfo>,
    pub signatures: Vec<SignatureInfo>,
    pub terms: serde_json::Value,
    pub version: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Party information
#[derive(Debug, Serialize, ToSchema)]
pub struct PartyInfo {
    pub did: String,
    pub coop_id: String,
    pub role: String,
    pub display_name: Option<String>,
    pub has_signed: bool,
}

/// Signature information
#[derive(Debug, Serialize, ToSchema)]
pub struct SignatureInfo {
    pub signer_did: String,
    pub coop_id: String,
    pub signed_at: u64,
    pub version_signed: u32,
}

/// Amendment response
#[derive(Debug, Serialize, ToSchema)]
pub struct AmendmentResponse {
    pub id: String,
    pub agreement_id: String,
    pub description: String,
    pub status: String,
    pub proposed_by: String,
    pub proposed_at: u64,
    pub signatures_count: usize,
    pub required_signatures: usize,
}

// ============================================================================
// Helper Functions
// ============================================================================

use icn_federation::agreement::{
    Agreement, AgreementId, AgreementParty, AgreementTerms, AgreementType, AmendmentChange,
    CompensationModel, DataSharingLevel, FederationMembershipTerms, PartyRole, ResourceType,
    TerminationReason, TradeItem,
};
use icn_identity::Did;

/// Convert API agreement type to domain type
fn to_domain_agreement_type(req_type: AgreementTypeRequest) -> Result<AgreementType> {
    match req_type {
        AgreementTypeRequest::Trade { items, currency } => {
            let trade_items: Vec<TradeItem> = items
                .into_iter()
                .map(|item| TradeItem {
                    description: item.description,
                    quantity: item.quantity,
                    unit: item.unit,
                    unit_price: item.unit_price,
                    currency: item.currency,
                })
                .collect();
            Ok(AgreementType::Trade {
                items: trade_items,
                currency,
            })
        }
        AgreementTypeRequest::Credit {
            credit_limit,
            interest_rate_bps,
            currency,
        } => Ok(AgreementType::Credit {
            credit_limit,
            interest_rate_bps,
            currency,
        }),
        AgreementTypeRequest::ResourceSharing {
            resource_type,
            duration_days,
            compensation_model: _,  // TODO: Parse compensation model
        } => {
            // Parse resource_type string to enum
            let resource_type_enum = match resource_type.as_str() {
                "equipment" => ResourceType::Equipment,
                "facility" => ResourceType::Facility,
                "compute" => ResourceType::Compute,
                "storage" => ResourceType::Storage,
                _ => ResourceType::Equipment, // Default
            };
            
            // Use fixed rate compensation model (simplified)
            let compensation = CompensationModel::FixedRate {
                amount: 0, // TODO: Extract from compensation_model string
                currency: "USD".to_string(),
                period_days: 30,
            };

            Ok(AgreementType::ResourceSharing {
                resource_type: resource_type_enum,
                duration_days,
                compensation,
            })
        }
        AgreementTypeRequest::FederationMembership {
            federation_id,
            min_trust_threshold,
            governance_binding,
        } => {
            let terms = FederationMembershipTerms {
                min_trust_threshold,
                governance_binding,
                data_sharing_level: DataSharingLevel::MetadataOnly, // Default
                contribution_requirements: None,
            };
            Ok(AgreementType::FederationMembership {
                federation_id,
                terms,
            })
        }
        AgreementTypeRequest::Custom {
            agreement_type_name,
            terms_json,
        } => Ok(AgreementType::Custom {
            agreement_type_name,
            terms_json,
        }),
    }
}

/// Convert domain agreement to API response
fn to_api_agreement(agreement: &Agreement) -> AgreementDetailResponse {
    let parties: Vec<PartyInfo> = agreement
        .parties
        .iter()
        .map(|party| {
            let has_signed = agreement
                .signatures
                .iter()
                .any(|sig| sig.signer == party.did);
            PartyInfo {
                did: party.did.to_string(),
                coop_id: party.coop_id.clone(),
                role: format!("{:?}", party.role),
                display_name: party.display_name.clone(),
                has_signed,
            }
        })
        .collect();

    let signatures: Vec<SignatureInfo> = agreement
        .signatures
        .iter()
        .map(|sig| SignatureInfo {
            signer_did: sig.signer.to_string(),
            coop_id: sig.coop_id.clone(),
            signed_at: sig.signed_at,
            version_signed: sig.version_signed,
        })
        .collect();

    AgreementDetailResponse {
        id: agreement.id.0.clone(),
        title: agreement.title.clone(),
        description: agreement.description.clone(),
        status: format!("{:?}", agreement.status),
        status_details: serde_json::to_value(&agreement.status).unwrap_or(serde_json::Value::Null),
        agreement_type: format!("{:?}", agreement.agreement_type),
        agreement_type_details: serde_json::to_value(&agreement.agreement_type)
            .unwrap_or(serde_json::Value::Null),
        parties,
        signatures,
        terms: serde_json::to_value(&agreement.terms).unwrap_or(serde_json::Value::Null),
        version: agreement.version,
        created_at: agreement.created_at,
        updated_at: agreement.updated_at,
    }
}

/// Convert domain agreement to API summary
fn to_api_summary(agreement: &Agreement) -> AgreementSummary {
    let agreement_type_str = match &agreement.agreement_type {
        AgreementType::Trade { .. } => "Trade",
        AgreementType::Credit { .. } => "Credit",
        AgreementType::ResourceSharing { .. } => "ResourceSharing",
        AgreementType::FederationMembership { .. } => "FederationMembership",
        AgreementType::Custom { .. } => "Custom",
    };

    AgreementSummary {
        id: agreement.id.0.clone(),
        title: agreement.title.clone(),
        status: format!("{:?}", agreement.status),
        agreement_type: agreement_type_str.to_string(),
        parties_count: agreement.parties.len(),
        created_at: agreement.created_at,
        updated_at: agreement.updated_at,
    }
}

/// Convert amendment changes from API to domain
fn to_domain_amendment_changes(changes: Vec<AmendmentChangeRequest>) -> Result<Vec<AmendmentChange>> {
    changes
        .into_iter()
        .map(|change| match change {
            AmendmentChangeRequest::ExtendDuration { new_expiration } => {
                Ok(AmendmentChange::ExtendDuration { new_expiration })
            }
            AmendmentChangeRequest::UpdateTerm { field, new_value } => {
                Ok(AmendmentChange::UpdateTerm {
                    field,
                    old_value: String::new(), // TODO: Fetch from current agreement
                    new_value,
                })
            }
            AmendmentChangeRequest::AddParty { did, coop_id, role } => {
                let role_enum = match role.as_str() {
                    "proposer" => PartyRole::Proposer,
                    "counterparty" => PartyRole::Counterparty,
                    "witness" => PartyRole::Witness,
                    "guarantor" => PartyRole::Guarantor,
                    _ => {
                        return Err(GatewayError::BadRequest(format!(
                            "Invalid role: {}",
                            role
                        )))
                    }
                };
                let party = AgreementParty {
                    did: Did::from_str(&did)
                        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {}", e)))?,
                    coop_id,
                    role: role_enum,
                    display_name: None,
                };
                Ok(AmendmentChange::AddParty { party })
            }
            AmendmentChangeRequest::RemoveParty { party_did } => {
                Ok(AmendmentChange::RemoveParty {
                    party_did: Did::from_str(&party_did)
                        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {}", e)))?,
                })
            }
        })
        .collect()
}

// ============================================================================
// API Endpoints
// ============================================================================

/// GET /agreements - List all agreements
///
/// Query parameters:
/// - status: Filter by status (draft, proposed, active, suspended, terminated)
/// - party: Filter by party DID
#[utoipa::path(
    get,
    path = "/agreements",
    tag = "agreements",
    responses(
        (status = 200, description = "List of agreements", body = AgreementListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - insufficient scope")
    ),
    security(("bearer_auth" = []))
)]
#[get("")]
pub async fn list_agreements(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    query: web::Query<ListAgreementsQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:read")?;

    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    // Get agreements based on query filters
    let agreements = if let Some(status) = &query.status {
        manager
            .list_by_status(status)
            .map_err(|e| GatewayError::InternalError(format!("Failed to list agreements: {}", e)))?
    } else {
        manager
            .list_agreements()
            .map_err(|e| GatewayError::InternalError(format!("Failed to list agreements: {}", e)))?
    };

    // Filter by party if requested
    let filtered_agreements: Vec<_> = if let Some(party_did_str) = &query.party {
        let party_did = Did::from_str(party_did_str)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid party DID: {}", e)))?;
        agreements
            .into_iter()
            .filter(|a| a.parties.iter().any(|p| p.did == party_did))
            .collect()
    } else {
        agreements
    };

    let summaries: Vec<AgreementSummary> = filtered_agreements
        .iter()
        .map(to_api_summary)
        .collect();

    let response = AgreementListResponse {
        total: summaries.len(),
        agreements: summaries,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
pub struct ListAgreementsQuery {
    pub status: Option<String>,
    pub party: Option<String>,
}

/// POST /agreements - Create a new agreement draft
#[utoipa::path(
    post,
    path = "/agreements",
    tag = "agreements",
    request_body = CreateAgreementRequest,
    responses(
        (status = 201, description = "Agreement created", body = AgreementDetailResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    ),
    security(("bearer_auth" = []))
)]
#[post("")]
pub async fn create_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    req: web::Json<CreateAgreementRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    // Convert API types to domain types
    let agreement_type = to_domain_agreement_type(req.agreement_type.clone())?;

    // Create the agreement draft
    let agreement = manager
        .create_draft(req.title.clone(), req.description.clone(), agreement_type)
        .map_err(|e| GatewayError::InternalError(format!("Failed to create agreement: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Created().json(response))
}

/// GET /agreements/{id} - Get agreement details
#[utoipa::path(
    get,
    path = "/agreements/{id}",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement details", body = AgreementDetailResponse),
        (status = 404, description = "Agreement not found"),
        (status = 401, description = "Unauthorized")
    ),
    security(("bearer_auth" = []))
)]
#[get("/{id}")]
pub async fn get_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:read")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let agreement = manager
        .get_agreement(&AgreementId(agreement_id.clone()))
        .map_err(|e| GatewayError::InternalError(format!("Failed to get agreement: {}", e)))?
        .ok_or_else(|| GatewayError::NotFound(format!("Agreement not found: {}", agreement_id)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// DELETE /agreements/{id} - Delete a draft agreement
#[utoipa::path(
    delete,
    path = "/agreements/{id}",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 204, description = "Agreement deleted"),
        (status = 404, description = "Agreement not found"),
        (status = 400, description = "Cannot delete non-draft agreement")
    ),
    security(("bearer_auth" = []))
)]
#[delete("/{id}")]
pub async fn delete_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    manager
        .delete_draft(&AgreementId(agreement_id.clone()))
        .map_err(|e| GatewayError::BadRequest(format!("Failed to delete agreement: {}", e)))?;

    Ok(HttpResponse::NoContent().finish())
}

/// POST /agreements/{id}/parties - Add a party to draft agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/parties",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = AddPartyRequest,
    responses(
        (status = 200, description = "Party added", body = AgreementDetailResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Agreement not found")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/parties")]
pub async fn add_party(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
    req: web::Json<AddPartyRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    // Parse role
    let role = match req.role.as_str() {
        "proposer" => PartyRole::Proposer,
        "counterparty" => PartyRole::Counterparty,
        "witness" => PartyRole::Witness,
        "guarantor" => PartyRole::Guarantor,
        _ => return Err(GatewayError::BadRequest(format!("Invalid role: {}", req.role))),
    };

    // Parse DID
    let did = Did::from_str(&req.did)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    // Create party
    let party = AgreementParty {
        did,
        coop_id: req.coop_id.clone(),
        role,
        display_name: req.display_name.clone(),
    };

    let agreement = manager
        .add_party(&AgreementId(agreement_id.clone()), party)
        .map_err(|e| GatewayError::BadRequest(format!("Failed to add party: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// PUT /agreements/{id}/terms - Set agreement terms
#[utoipa::path(
    put,
    path = "/agreements/{id}/terms",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = SetTermsRequest,
    responses(
        (status = 200, description = "Terms set", body = AgreementDetailResponse),
        (status = 400, description = "Bad request")
    ),
    security(("bearer_auth" = []))
)]
#[put("/{id}/terms")]
pub async fn set_terms(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
    req: web::Json<SetTermsRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    // Build terms object
    let terms = AgreementTerms {
        effective_date: req.effective_date,
        expiration_date: req.expiration_date,
        auto_renewal: None, // TODO: Add to API request if needed
        termination_notice_days: req.termination_notice_days,
        dispute_resolution: req.dispute_resolution.clone(),
        governing_law: None, // TODO: Add to API request if needed
        additional_terms: req.additional_terms.clone(),
    };

    let agreement = manager
        .set_terms(&AgreementId(agreement_id.clone()), terms)
        .map_err(|e| GatewayError::BadRequest(format!("Failed to set terms: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// POST /agreements/{id}/propose - Propose the agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/propose",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement proposed", body = AgreementDetailResponse),
        (status = 400, description = "Cannot propose - invalid state or missing parties")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/propose")]
pub async fn propose_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let agreement = manager
        .propose(&AgreementId(agreement_id.clone()))
        .map_err(|e| GatewayError::BadRequest(format!("Failed to propose agreement: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// POST /agreements/{id}/sign - Sign the agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/sign",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement signed", body = AgreementDetailResponse),
        (status = 400, description = "Cannot sign - invalid state or not a party")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/sign")]
pub async fn sign_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let agreement = manager
        .sign(&AgreementId(agreement_id.clone()))
        .map_err(|e| GatewayError::BadRequest(format!("Failed to sign agreement: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// POST /agreements/{id}/suspend - Suspend an active agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/suspend",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = SuspendRequest,
    responses(
        (status = 200, description = "Agreement suspended", body = AgreementDetailResponse),
        (status = 400, description = "Cannot suspend")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/suspend")]
pub async fn suspend_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
    req: web::Json<SuspendRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:admin")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let agreement = manager
        .suspend(&AgreementId(agreement_id.clone()), req.reason.clone())
        .map_err(|e| GatewayError::BadRequest(format!("Failed to suspend agreement: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// POST /agreements/{id}/resume - Resume a suspended agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/resume",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement resumed", body = AgreementDetailResponse),
        (status = 400, description = "Cannot resume")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/resume")]
pub async fn resume_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:admin")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let agreement = manager
        .resume(&AgreementId(agreement_id.clone()))
        .map_err(|e| GatewayError::BadRequest(format!("Failed to resume agreement: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

/// POST /agreements/{id}/terminate - Terminate an agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/terminate",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = TerminateRequest,
    responses(
        (status = 200, description = "Agreement terminated", body = AgreementDetailResponse),
        (status = 400, description = "Cannot terminate")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/terminate")]
pub async fn terminate_agreement(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
    req: web::Json<TerminateRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:admin")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    // Parse termination reason
    let reason = match req.reason_type.as_str() {
        "expired" => TerminationReason::Expired,
        "mutual_consent" => TerminationReason::MutualConsent {
            explanation: req.details.clone(),
        },
        "breach" => TerminationReason::Breach {
            breaching_party: Did::from_str("did:icn:placeholder")
                .map_err(|e| GatewayError::InternalError(format!("Invalid DID: {}", e)))?,  // TODO: Get from request
            breach_description: req.details.clone().unwrap_or_default(),
        },
        "withdrawal" => TerminationReason::Withdrawal {
            withdrawing_party: Did::from_str("did:icn:placeholder")
                .map_err(|e| GatewayError::InternalError(format!("Invalid DID: {}", e)))?, // TODO: Get from claims
            notice_days: 0, // TODO: Extract from request
        },
        "force_majeure" => TerminationReason::ForceMajeure {
            description: req.details.clone().unwrap_or_default(),
        },
        _ => {
            return Err(GatewayError::BadRequest(format!(
                "Invalid termination reason: {}",
                req.reason_type
            )))
        }
    };

    let agreement = manager
        .terminate(&AgreementId(agreement_id.clone()), reason)
        .map_err(|e| GatewayError::BadRequest(format!("Failed to terminate agreement: {}", e)))?;

    let response = to_api_agreement(&agreement);
    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Amendment Endpoints
// ============================================================================

/// GET /agreements/{id}/amendments - List amendments for an agreement
#[utoipa::path(
    get,
    path = "/agreements/{id}/amendments",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "List of amendments", body = Vec<AmendmentResponse>)
    ),
    security(("bearer_auth" = []))
)]
#[get("/{id}/amendments")]
pub async fn list_amendments(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:read")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let amendments = manager
        .get_amendments(&AgreementId(agreement_id.clone()))
        .map_err(|e| GatewayError::InternalError(format!("Failed to list amendments: {}", e)))?;

    let responses: Vec<AmendmentResponse> = amendments
        .iter()
        .map(|amendment| AmendmentResponse {
            id: amendment.id.clone(),
            agreement_id: amendment.agreement_id.0.clone(),
            description: amendment.description.clone(),
            status: format!("{:?}", amendment.status),
            proposed_by: amendment.proposed_by.to_string(),
            proposed_at: amendment.proposed_at,
            signatures_count: amendment.signatures.len(),
            required_signatures: 0, // TODO: Calculate based on agreement parties
        })
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

/// POST /agreements/{id}/amendments - Propose an amendment
#[utoipa::path(
    post,
    path = "/agreements/{id}/amendments",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = ProposeAmendmentRequest,
    responses(
        (status = 201, description = "Amendment proposed", body = AmendmentResponse),
        (status = 400, description = "Cannot amend")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/amendments")]
pub async fn propose_amendment(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<String>,
    req: web::Json<ProposeAmendmentRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let agreement_id = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    // Convert changes
    let changes = to_domain_amendment_changes(req.changes.clone())?;

    let amendment = manager
        .propose_amendment(&AgreementId(agreement_id.clone()), req.description.clone(), changes)
        .map_err(|e| GatewayError::BadRequest(format!("Failed to propose amendment: {}", e)))?;

    let response = AmendmentResponse {
        id: amendment.id.clone(),
        agreement_id: amendment.agreement_id.0.clone(),
        description: amendment.description.clone(),
        status: format!("{:?}", amendment.status),
        proposed_by: amendment.proposed_by.to_string(),
        proposed_at: amendment.proposed_at,
        signatures_count: amendment.signatures.len(),
        required_signatures: 0, // TODO: Calculate
    };

    Ok(HttpResponse::Created().json(response))
}

/// POST /agreements/{id}/amendments/{amendment_id}/sign - Sign an amendment
#[utoipa::path(
    post,
    path = "/agreements/{id}/amendments/{amendment_id}/sign",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID"),
        ("amendment_id" = String, Path, description = "Amendment ID")
    ),
    responses(
        (status = 200, description = "Amendment signed", body = AmendmentResponse),
        (status = 400, description = "Cannot sign")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/amendments/{amendment_id}/sign")]
pub async fn sign_amendment(
    http_req: HttpRequest,
    agreement_mgr: web::Data<Option<icn_federation::agreement::AgreementManagerHandle>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let (agreement_id, amendment_id) = path.into_inner();

    let manager = agreement_mgr
        .as_ref()
        .as_ref()
        .ok_or_else(|| GatewayError::ServiceUnavailable("Agreement manager not configured".into()))?;

    let amendment = manager
        .sign_amendment(&AgreementId(agreement_id.clone()), &amendment_id)
        .map_err(|e| GatewayError::BadRequest(format!("Failed to sign amendment: {}", e)))?;

    let response = AmendmentResponse {
        id: amendment.id.clone(),
        agreement_id: amendment.agreement_id.0.clone(),
        description: amendment.description.clone(),
        status: format!("{:?}", amendment.status),
        proposed_by: amendment.proposed_by.to_string(),
        proposed_at: amendment.proposed_at,
        signatures_count: amendment.signatures.len(),
        required_signatures: 0, // TODO: Calculate
    };

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure agreement routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/agreements")
            .service(list_agreements)
            .service(create_agreement)
            .service(get_agreement)
            .service(delete_agreement)
            .service(add_party)
            .service(set_terms)
            .service(propose_agreement)
            .service(sign_agreement)
            .service(suspend_agreement)
            .service(resume_agreement)
            .service(terminate_agreement)
            .service(list_amendments)
            .service(propose_amendment)
            .service(sign_amendment),
    );
}
