//! Contract Management API endpoints
//!
//! RESTful API for deploying, managing, and querying CCL contracts.

use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

// Re-export types from icn_ccl for contract operations
use icn_ccl::{
    Contract, ContractMetadata, ContractRegistryHandle, ContractStatus, ContractSummary,
    DeploymentMetadata, Visibility,
};

/// Helper to get the contract registry or return 503 Service Unavailable
fn get_registry(
    registry: &web::Data<Option<Arc<ContractRegistryHandle>>>,
) -> Result<Arc<ContractRegistryHandle>> {
    registry.as_ref().as_ref().cloned().ok_or_else(|| {
        GatewayError::ServiceUnavailable(
            "Contract registry not configured. Contracts API requires daemon integration."
                .to_string(),
        )
    })
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to deploy a new contract
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeployContractRequest {
    /// CCL contract source (JSON format)
    pub source: String,
    /// Visibility: "private", "coop", or "public"
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// Coop ID (required if visibility is "coop")
    #[serde(default)]
    pub coop_id: Option<String>,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
}

fn default_visibility() -> String {
    "private".to_string()
}

/// Response from deploying a contract
#[derive(Debug, Serialize, ToSchema)]
pub struct DeployContractResponse {
    /// Content hash of the deployed contract (hex)
    pub hash: String,
    /// Contract name
    pub name: String,
    /// Version number
    pub version: u32,
}

/// Query parameters for listing contracts
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListContractsQuery {
    /// Filter by visibility: "private", "coop", "public"
    #[serde(default)]
    pub visibility: Option<String>,
    /// Filter by owner DID
    #[serde(default)]
    pub owner: Option<String>,
    /// Filter by name prefix
    #[serde(default)]
    pub name_prefix: Option<String>,
    /// Maximum number of results (default: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// Contract summary for list responses
#[derive(Debug, Serialize, ToSchema)]
pub struct ContractListItem {
    /// Content hash (hex)
    pub hash: String,
    /// Contract name
    pub name: String,
    /// Owner DID
    pub owner: String,
    /// Version number
    pub version: u32,
    /// Deployment timestamp (Unix seconds)
    pub deployed_at: u64,
    /// Rule names in the contract
    pub rules: Vec<String>,
    /// Whether the contract is revoked
    pub revoked: bool,
    /// Contract lifecycle status: "active", "deprecated", or "revoked"
    pub status: String,
}

impl From<ContractSummary> for ContractListItem {
    fn from(summary: ContractSummary) -> Self {
        let status = match &summary.status {
            ContractStatus::Active => "active".to_string(),
            ContractStatus::Deprecated { .. } => "deprecated".to_string(),
            ContractStatus::Revoked { .. } => "revoked".to_string(),
        };
        Self {
            hash: hex::encode(summary.hash),
            name: summary.name,
            owner: summary.owner,
            version: summary.version,
            deployed_at: summary.deployed_at,
            rules: summary.rules,
            revoked: summary.revoked,
            status,
        }
    }
}

/// Response for get contract by hash
#[derive(Debug, Serialize, ToSchema)]
pub struct ContractDetailResponse {
    /// Content hash (hex)
    pub hash: String,
    /// Full contract source (JSON)
    pub source: String,
    /// Contract metadata
    pub metadata: ContractMetadataResponse,
}

/// Contract metadata response
#[derive(Debug, Serialize, ToSchema)]
pub struct ContractMetadataResponse {
    /// Contract name
    pub name: String,
    /// Owner DID
    pub owner: String,
    /// Version number
    pub version: u32,
    /// Deployment timestamp
    pub deployed_at: u64,
    /// Rule names
    pub rules: Vec<String>,
    /// Visibility level
    pub visibility: String,
    /// Optional description
    pub description: Option<String>,
}

impl From<ContractMetadata> for ContractMetadataResponse {
    fn from(meta: ContractMetadata) -> Self {
        let visibility = match &meta.visibility {
            Visibility::Private => "private".to_string(),
            Visibility::Coop(id) => format!("coop:{id}"),
            Visibility::Public => "public".to_string(),
        };

        Self {
            name: meta.name,
            owner: meta.owner,
            version: meta.version,
            deployed_at: meta.deployed_at,
            rules: meta.rules,
            visibility,
            description: meta.description,
        }
    }
}

/// Request to revoke a contract
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeContractRequest {
    /// Reason for revocation
    pub reason: String,
}

/// Response from revoking a contract
#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeContractResponse {
    /// Contract hash that was revoked
    pub hash: String,
    /// Status
    pub status: String,
    /// Reason for revocation
    pub reason: String,
}

/// Request to deprecate a contract
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeprecateContractRequest {
    /// Reason for deprecation
    pub reason: String,
    /// Hash of the successor contract (optional)
    #[serde(default)]
    pub successor: Option<String>,
}

/// Response from deprecating a contract
#[derive(Debug, Serialize, ToSchema)]
pub struct DeprecateContractResponse {
    /// Contract hash that was deprecated
    pub hash: String,
    /// Status
    pub status: String,
    /// Successor hash (if specified)
    pub successor: Option<String>,
    /// Reason for deprecation
    pub reason: String,
}

/// Contract status response
#[derive(Debug, Serialize, ToSchema)]
pub struct ContractStatusResponse {
    /// Contract hash
    pub hash: String,
    /// Status type: "active", "deprecated", or "revoked"
    pub status: String,
    /// Successor hash for deprecated contracts
    pub successor: Option<String>,
    /// Reason for deprecation/revocation
    pub reason: Option<String>,
    /// When the status changed (Unix millis)
    pub changed_at: Option<u64>,
}

impl From<(String, ContractStatus)> for ContractStatusResponse {
    fn from((hash, status): (String, ContractStatus)) -> Self {
        match status {
            ContractStatus::Active => Self {
                hash,
                status: "active".to_string(),
                successor: None,
                reason: None,
                changed_at: None,
            },
            ContractStatus::Deprecated {
                successor,
                reason,
                deprecated_at,
            } => Self {
                hash,
                status: "deprecated".to_string(),
                successor: successor.map(hex::encode),
                reason: Some(reason),
                changed_at: Some(deprecated_at),
            },
            ContractStatus::Revoked { reason, revoked_at } => Self {
                hash,
                status: "revoked".to_string(),
                successor: None,
                reason: Some(reason),
                changed_at: Some(revoked_at),
            },
        }
    }
}

/// Version history item
#[derive(Debug, Serialize, ToSchema)]
pub struct VersionHistoryItem {
    /// Version number
    pub version: u32,
    /// Contract hash (hex)
    pub hash: String,
}

// ============================================================================
// Helpers
// ============================================================================

/// Parse visibility string to Visibility enum
fn parse_visibility(s: &str, coop_id: Option<&str>) -> Result<Visibility> {
    match s.to_lowercase().as_str() {
        "private" => Ok(Visibility::Private),
        "coop" => {
            let coop = coop_id.ok_or_else(|| {
                GatewayError::BadRequest("coop_id is required for coop visibility".to_string())
            })?;
            Ok(Visibility::Coop(coop.to_string()))
        }
        "public" => Ok(Visibility::Public),
        _ => Err(GatewayError::BadRequest(format!(
            "Invalid visibility '{s}'. Must be 'private', 'coop', or 'public'"
        ))),
    }
}

/// Parse hex hash string to 32-byte array
fn parse_hash(hash_hex: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hash_hex)
        .map_err(|_| GatewayError::BadRequest("Invalid hex hash".to_string()))?;

    if bytes.len() != 32 {
        return Err(GatewayError::BadRequest(
            "Hash must be 32 bytes (64 hex chars)".to_string(),
        ));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

// ============================================================================
// Endpoints
// ============================================================================

/// POST /contracts - Deploy a new contract
///
/// Requires `contracts:write` scope.
#[post("")]
pub async fn deploy_contract(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    req: web::Json<DeployContractRequest>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:write scope
    require_scope(&http_req, "contracts:write")?;

    // Get deployer DID from JWT
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let owner_did = claims.sub.clone();

    // Parse the contract source
    let contract: Contract = serde_json::from_str(&req.source)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid contract JSON: {e}")))?;

    // Validate contract structure
    contract
        .validate()
        .map_err(|e| GatewayError::BadRequest(format!("Contract validation failed: {e}")))?;

    // Parse visibility
    let visibility = parse_visibility(&req.visibility, req.coop_id.as_deref())?;

    // Create deployment metadata
    let mut metadata = DeploymentMetadata::new(owner_did).with_visibility(visibility);

    if let Some(ref desc) = req.description {
        metadata = metadata.with_description(desc.clone());
    }

    // Register the contract
    let hash = registry
        .register_contract(contract.clone(), metadata)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to register contract: {e}")))?;

    // Get the version from metadata
    let version = registry
        .get_metadata(&hash)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get metadata: {e}")))?
        .map(|m| m.version)
        .unwrap_or(1);

    Ok(HttpResponse::Created().json(DeployContractResponse {
        hash: hex::encode(hash),
        name: contract.name,
        version,
    }))
}

/// GET /contracts - List contracts
///
/// Requires `contracts:read` scope.
#[get("")]
pub async fn list_contracts(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    query: web::Query<ListContractsQuery>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:read scope
    require_scope(&http_req, "contracts:read")?;

    // Build filter
    let mut filter = icn_ccl::registry_actor::types::ContractFilter::new();

    if let Some(ref owner) = query.owner {
        filter = filter.with_owner(owner.clone());
    }

    if let Some(ref prefix) = query.name_prefix {
        filter = filter.with_name_prefix(prefix.clone());
    }

    if let Some(ref vis) = query.visibility {
        let visibility = parse_visibility(vis, None)?;
        filter = filter.with_visibility(visibility);
    }

    filter = filter.with_limit(query.limit);

    // List contracts
    let summaries = registry
        .list_contracts(filter)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to list contracts: {e}")))?;

    let items: Vec<ContractListItem> = summaries.into_iter().map(ContractListItem::from).collect();

    Ok(HttpResponse::Ok().json(items))
}

/// GET /contracts/{hash} - Get contract by hash
///
/// Requires `contracts:read` scope.
#[get("/{hash}")]
pub async fn get_contract(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:read scope
    require_scope(&http_req, "contracts:read")?;

    let hash_hex = path.into_inner();
    let hash = parse_hash(&hash_hex)?;

    // Get contract
    let contract = registry
        .get_contract(&hash)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get contract: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Contract {hash_hex} not found")))?;

    // Get metadata
    let metadata = registry
        .get_metadata(&hash)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get metadata: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Contract {hash_hex} not found")))?;

    // Serialize contract back to JSON
    let source = serde_json::to_string_pretty(&contract)
        .map_err(|e| GatewayError::InternalError(format!("Failed to serialize contract: {e}")))?;

    Ok(HttpResponse::Ok().json(ContractDetailResponse {
        hash: hash_hex,
        source,
        metadata: ContractMetadataResponse::from(metadata),
    }))
}

/// DELETE /contracts/{hash} - Revoke a contract
///
/// Requires `contracts:write` scope. Only the contract owner can revoke.
#[delete("/{hash}")]
pub async fn revoke_contract(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    path: web::Path<String>,
    req: web::Json<RevokeContractRequest>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:write scope
    require_scope(&http_req, "contracts:write")?;

    // Get requester DID from JWT
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let requester_did = claims.sub.clone();

    let hash_hex = path.into_inner();
    let hash = parse_hash(&hash_hex)?;

    // Revoke the contract
    registry
        .revoke_contract(&hash, &requester_did, req.reason.clone())
        .await
        .map_err(|e| {
            // Check if it's an authorization error
            let err_str = e.to_string();
            if err_str.contains("not authorized") || err_str.contains("not the owner") {
                GatewayError::Forbidden("Only the contract owner can revoke".to_string())
            } else if err_str.contains("not found") {
                GatewayError::NotFound(format!("Contract {hash_hex} not found"))
            } else {
                GatewayError::InternalError(format!("Failed to revoke contract: {e}"))
            }
        })?;

    Ok(HttpResponse::Ok().json(RevokeContractResponse {
        hash: hash_hex,
        status: "revoked".to_string(),
        reason: req.reason.clone(),
    }))
}

/// GET /contracts/{hash}/metadata - Get contract metadata only
///
/// Requires `contracts:read` scope.
#[get("/{hash}/metadata")]
pub async fn get_contract_metadata(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:read scope
    require_scope(&http_req, "contracts:read")?;

    let hash_hex = path.into_inner();
    let hash = parse_hash(&hash_hex)?;

    // Get metadata
    let metadata = registry
        .get_metadata(&hash)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get metadata: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Contract {hash_hex} not found")))?;

    Ok(HttpResponse::Ok().json(ContractMetadataResponse::from(metadata)))
}

/// POST /contracts/{hash}/deprecate - Deprecate a contract
///
/// Requires `contracts:write` scope. Only the contract owner can deprecate.
#[post("/{hash}/deprecate")]
pub async fn deprecate_contract(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    path: web::Path<String>,
    req: web::Json<DeprecateContractRequest>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:write scope
    require_scope(&http_req, "contracts:write")?;

    // Get requester DID from JWT
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;
    let requester_did = claims.sub.clone();

    let hash_hex = path.into_inner();
    let hash = parse_hash(&hash_hex)?;

    // Parse successor hash if provided
    let successor = match &req.successor {
        Some(s) => Some(parse_hash(s)?),
        None => None,
    };

    // Deprecate the contract
    registry
        .deprecate_contract(&hash, &requester_did, successor, req.reason.clone())
        .await
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("not authorized") || err_str.contains("Only owner") {
                GatewayError::Forbidden("Only the contract owner can deprecate".to_string())
            } else if err_str.contains("not found") {
                GatewayError::NotFound(format!("Contract {hash_hex} not found"))
            } else {
                GatewayError::InternalError(format!("Failed to deprecate contract: {e}"))
            }
        })?;

    Ok(HttpResponse::Ok().json(DeprecateContractResponse {
        hash: hash_hex,
        status: "deprecated".to_string(),
        successor: req.successor.clone(),
        reason: req.reason.clone(),
    }))
}

/// GET /contracts/{hash}/status - Get contract lifecycle status
///
/// Requires `contracts:read` scope.
#[get("/{hash}/status")]
pub async fn get_contract_status(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:read scope
    require_scope(&http_req, "contracts:read")?;

    let hash_hex = path.into_inner();
    let hash = parse_hash(&hash_hex)?;

    // Get status
    let status = registry
        .get_status(&hash)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get status: {e}")))?
        .ok_or_else(|| GatewayError::NotFound(format!("Contract {hash_hex} not found")))?;

    Ok(HttpResponse::Ok().json(ContractStatusResponse::from((hash_hex, status))))
}

/// GET /contracts/name/{name}/versions - Get version history for a contract
///
/// Requires `contracts:read` scope.
#[get("/name/{name}/versions")]
pub async fn get_version_history(
    http_req: HttpRequest,
    registry_opt: web::Data<Option<Arc<ContractRegistryHandle>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Get the registry or return 503
    let registry = get_registry(&registry_opt)?;

    // Require contracts:read scope
    require_scope(&http_req, "contracts:read")?;

    let name = path.into_inner();

    // Get version history
    let versions = registry
        .get_version_history(&name)
        .await
        .map_err(|e| GatewayError::InternalError(format!("Failed to get version history: {e}")))?;

    if versions.is_empty() {
        return Err(GatewayError::NotFound(format!(
            "No contracts found with name '{name}'"
        )));
    }

    let items: Vec<VersionHistoryItem> = versions
        .into_iter()
        .map(|(version, hash)| VersionHistoryItem {
            version,
            hash: hex::encode(hash),
        })
        .collect();

    Ok(HttpResponse::Ok().json(items))
}

/// Configure contract routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(deploy_contract)
        .service(list_contracts)
        .service(get_contract)
        .service(revoke_contract)
        .service(deprecate_contract)
        .service(get_contract_status)
        .service(get_contract_metadata)
        .service(get_version_history);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_visibility() {
        assert!(matches!(
            parse_visibility("private", None).unwrap(),
            Visibility::Private
        ));
        assert!(matches!(
            parse_visibility("public", None).unwrap(),
            Visibility::Public
        ));
        assert!(matches!(
            parse_visibility("coop", Some("coop-123")).unwrap(),
            Visibility::Coop(_)
        ));

        // Error case: coop without coop_id
        assert!(parse_visibility("coop", None).is_err());

        // Error case: invalid visibility
        assert!(parse_visibility("invalid", None).is_err());
    }

    #[test]
    fn test_parse_hash() {
        let valid_hex = "a".repeat(64);
        let hash = parse_hash(&valid_hex).unwrap();
        assert_eq!(hash, [0xaa; 32]);

        // Error case: too short
        assert!(parse_hash("abcd").is_err());

        // Error case: invalid hex
        assert!(parse_hash("gg".repeat(32).as_str()).is_err());
    }
}
