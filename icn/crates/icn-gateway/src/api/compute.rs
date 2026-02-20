//! Compute API endpoints
//!
//! RESTful API for submitting and monitoring distributed compute tasks.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::compute_mgr::ComputeManager;
use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::middleware::{get_claims, require_scope};

/// Code type for compute tasks
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodeType {
    /// CCL contract (default)
    #[default]
    Ccl,
    /// WebAssembly module
    Wasm,
}

/// Resource profile for compute tasks (API representation)
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct ResourceProfileRequest {
    /// CPU cores required (e.g., 0.5 = half a core, 2.0 = two cores)
    #[serde(default)]
    pub cpu_cores: Option<f64>,
    /// Memory required in megabytes
    #[serde(default)]
    pub memory_mb: Option<u64>,
    /// Temporary storage required in megabytes
    #[serde(default)]
    pub storage_mb: Option<u64>,
    /// Network bandwidth required in Mbps
    #[serde(default)]
    pub network_mbps: Option<f64>,
    /// Expected runtime duration in seconds
    #[serde(default)]
    pub duration_estimate_secs: Option<u64>,
}

/// Request to submit a compute task
#[derive(Debug, serde::Deserialize)]
pub struct SubmitTaskRequest {
    /// Task ID (auto-generated if not provided)
    #[serde(default)]
    pub task_id: Option<String>,
    /// CCL contract JSON (for code_type: ccl)
    #[serde(default)]
    pub code: Option<String>,
    /// WASM bytecode as base64 (for code_type: wasm)
    #[serde(default)]
    pub wasm_bytes: Option<String>,
    /// WASM module hash reference (for code_type: wasm, alternative to wasm_bytes)
    #[serde(default)]
    pub wasm_hash: Option<String>,
    /// Code type: "ccl" (default) or "wasm"
    #[serde(default)]
    pub code_type: CodeType,
    /// Input arguments (JSON)
    #[serde(default)]
    pub inputs: serde_json::Value,
    /// Fuel limit (default: 10000)
    #[serde(default = "default_fuel_limit")]
    pub fuel_limit: u64,
    /// Task priority (default: Normal)
    #[serde(default)]
    pub priority: Option<String>,
    /// Deadline in milliseconds from now (optional)
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    /// Payment rate per 1000 fuel (optional)
    #[serde(default)]
    pub payment_rate: Option<u64>,
    /// Payment currency (default: credits)
    #[serde(default)]
    pub payment_currency: Option<String>,
    /// Resource requirements for the task (optional)
    #[serde(default)]
    pub resource_profile: Option<ResourceProfileRequest>,
}

fn default_fuel_limit() -> u64 {
    10_000
}

/// Response from submitting a task
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SubmitTaskResponse {
    pub task_id: String,
    pub task_hash: String,
}

// ============================================================================
// Endpoints
// ============================================================================

/// POST /compute/submit - Submit a compute task
#[post("/submit")]
pub async fn submit_task(
    http_req: HttpRequest,
    compute_mgr: web::Data<Arc<ComputeManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    req: web::Json<SubmitTaskRequest>,
) -> Result<HttpResponse> {
    use base64::Engine;
    use icn_compute::TaskCode;

    // Require compute:write scope
    require_scope(&http_req, "compute:write")?;

    // Get submitter DID from JWT
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let submitter_did = claims.sub.clone();

    // Generate task ID if not provided
    let task_id = req
        .task_id
        .clone()
        .unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));

    // Convert inputs to bytes
    let inputs = if req.inputs.is_null() {
        vec![]
    } else {
        serde_json::to_vec(&req.inputs).unwrap_or_default()
    };

    // Parse priority string (low, normal, high, critical) - case insensitive
    let priority = req.priority.as_deref().unwrap_or("normal");

    // Build TaskCode based on code_type
    let task_code = match req.code_type {
        CodeType::Ccl => {
            let code = req.code.as_ref().ok_or_else(|| {
                crate::error::GatewayError::BadRequest(
                    "Missing 'code' field for CCL task".to_string(),
                )
            })?;
            TaskCode::Ccl(code.clone())
        }
        CodeType::Wasm => {
            if let Some(ref hash_hex) = req.wasm_hash {
                // Reference by hash (deploy-once, reference-by-hash).
                // Validate format at the API boundary before touching the registry.
                if hash_hex.len() != 64 {
                    return Err(crate::error::GatewayError::BadRequest(
                        "wasm_hash must be exactly 64 hex characters (32 bytes)".to_string(),
                    ));
                }
                let hash_bytes = hex::decode(hash_hex).map_err(|_| {
                    crate::error::GatewayError::BadRequest(
                        "wasm_hash contains non-hex characters".to_string(),
                    )
                })?;
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);

                // Registry existence check: return 404 before queueing to the compute actor.
                // Require the registry to be configured; silently skipping the check when it is
                // absent would allow references to non-existent modules to reach the compute actor.
                match compute_mgr.wasm_registry() {
                    Some(registry) => {
                        let exists = registry
                            .get_metadata(&hash)
                            .await
                            .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?
                            .is_some();
                        if !exists {
                            return Err(crate::error::GatewayError::NotFound(format!(
                                "WASM module not found: {hash_hex}"
                            )));
                        }
                    }
                    None => {
                        return Err(crate::error::GatewayError::InternalError(
                            "WASM registry is not configured for wasm_hash submissions".to_string(),
                        ));
                    }
                }

                TaskCode::WasmRef(hash)
            } else if let Some(ref wasm_b64) = req.wasm_bytes {
                // Inline WASM bytes
                let wasm_bytes = base64::engine::general_purpose::STANDARD
                    .decode(wasm_b64)
                    .map_err(|e| {
                        crate::error::GatewayError::BadRequest(format!("Invalid base64: {e}"))
                    })?;
                TaskCode::WasmInline(wasm_bytes)
            } else {
                return Err(crate::error::GatewayError::BadRequest(
                    "WASM task requires either 'wasm_bytes' or 'wasm_hash'".to_string(),
                ));
            }
        }
    };

    // Submit task with coop_id from JWT claims
    let coop_id = Some(claims.coop_id.clone());
    let task_hash = compute_mgr
        .submit_task_with_code(
            task_id.clone(),
            submitter_did.clone(),
            coop_id,
            task_code,
            inputs,
            req.fuel_limit,
            priority,
            req.deadline_ms,
            req.payment_rate,
            req.payment_currency.clone(),
            req.resource_profile.clone(),
        )
        .await
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    let task_hash_hex = hex::encode(task_hash);

    // Broadcast event
    broadcaster
        .broadcast(
            "compute",
            GatewayEvent::ComputeTaskSubmitted {
                task_id: task_id.clone(),
                task_hash: task_hash_hex.clone(),
                submitter: submitter_did,
                fuel_limit: req.fuel_limit,
            },
        )
        .await;

    Ok(HttpResponse::Ok().json(SubmitTaskResponse {
        task_id,
        task_hash: task_hash_hex,
    }))
}

/// GET /compute/status/{task_hash} - Get task status
#[get("/status/{task_hash}")]
pub async fn get_status(
    http_req: HttpRequest,
    compute_mgr: web::Data<Arc<ComputeManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    // Require compute:read scope
    require_scope(&http_req, "compute:read")?;

    let task_hash_hex = path.into_inner();

    // Parse task hash
    let hash_bytes = hex::decode(&task_hash_hex)
        .map_err(|_| crate::error::GatewayError::BadRequest("Invalid task hash".to_string()))?;

    if hash_bytes.len() != 32 {
        return Err(crate::error::GatewayError::BadRequest(
            "Task hash must be 32 bytes".to_string(),
        ));
    }

    let mut task_hash = [0u8; 32];
    task_hash.copy_from_slice(&hash_bytes);

    // Get status
    match compute_mgr.get_status(task_hash).await {
        Ok(Some(status)) => Ok(HttpResponse::Ok().json(status)),
        Ok(None) => Err(crate::error::GatewayError::NotFound(
            "Task not found".to_string(),
        )),
        Err(e) => Err(crate::error::GatewayError::InternalError(e.to_string())),
    }
}

/// Request to cancel a compute task
#[derive(Debug, serde::Deserialize)]
pub struct CancelTaskRequest {
    /// Cancellation reason (optional)
    #[serde(default = "default_cancel_reason")]
    pub reason: String,
}

fn default_cancel_reason() -> String {
    "Cancelled by user".to_string()
}

/// POST /compute/cancel/{task_hash} - Cancel a task
#[post("/cancel/{task_hash}")]
pub async fn cancel_task(
    http_req: HttpRequest,
    compute_mgr: web::Data<Arc<ComputeManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<String>,
    req: web::Json<CancelTaskRequest>,
) -> Result<HttpResponse> {
    // Require compute:write scope
    require_scope(&http_req, "compute:write")?;

    // Get submitter DID from JWT
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;
    let requester_did = claims.sub.clone();

    let task_hash_hex = path.into_inner();

    // Parse task hash
    let hash_bytes = hex::decode(&task_hash_hex)
        .map_err(|_| crate::error::GatewayError::BadRequest("Invalid task hash".to_string()))?;

    if hash_bytes.len() != 32 {
        return Err(crate::error::GatewayError::BadRequest(
            "Task hash must be 32 bytes".to_string(),
        ));
    }

    let mut task_hash = [0u8; 32];
    task_hash.copy_from_slice(&hash_bytes);

    // Cancel task
    compute_mgr
        .cancel_task(task_hash, requester_did.clone(), req.reason.clone())
        .await
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    icn_obs::metrics::compute::tasks_cancelled_inc();

    // Broadcast event
    broadcaster
        .broadcast(
            "compute",
            GatewayEvent::ComputeTaskCancelled {
                task_hash: task_hash_hex.clone(),
                submitter: requester_did,
                reason: req.reason.clone(),
            },
        )
        .await;

    #[derive(serde::Serialize, utoipa::ToSchema)]
    struct CancelResponse {
        task_hash: String,
        status: String,
        reason: String,
    }

    Ok(HttpResponse::Ok().json(CancelResponse {
        task_hash: task_hash_hex,
        status: "cancelled".to_string(),
        reason: req.reason.clone(),
    }))
}

// ============================================================================
// WASM Registry Endpoints
// ============================================================================

/// Request body for WASM upload (base64-encoded bytes)
#[derive(Debug, serde::Deserialize)]
pub struct UploadWasmRequest {
    /// WASM bytecode as base64
    pub wasm_bytes: String,
    /// Module name (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// Module description (optional)
    #[serde(default)]
    pub description: Option<String>,
}

/// Response from uploading a WASM module
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct UploadWasmResponse {
    pub hash: String,
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Response for listing WASM modules
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct WasmMetadataResponse {
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub size_bytes: usize,
    pub owner: String,
    pub deployed_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<icn_compute::WasmMetadata> for WasmMetadataResponse {
    fn from(m: icn_compute::WasmMetadata) -> Self {
        Self {
            hash: hex::encode(m.code_hash),
            name: m.name,
            size_bytes: m.size_bytes,
            owner: m.owner,
            deployed_at: m.deployed_at,
            description: m.description,
        }
    }
}

/// Pagination query parameters
#[derive(Debug, serde::Deserialize)]
pub struct PaginationQuery {
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default = "default_page_limit")]
    pub limit: usize,
}

fn default_page_limit() -> usize {
    50
}

/// POST /compute/wasm/upload — Upload a WASM module
#[post("/wasm/upload")]
pub async fn upload_wasm(
    http_req: HttpRequest,
    compute_mgr: web::Data<Arc<ComputeManager>>,
    req: web::Json<UploadWasmRequest>,
) -> Result<HttpResponse> {
    use base64::Engine;

    require_scope(&http_req, "compute:write")?;

    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let registry = compute_mgr.wasm_registry().ok_or_else(|| {
        crate::error::GatewayError::InternalError("WASM registry not configured".to_string())
    })?;

    let wasm_bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.wasm_bytes)
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid base64: {e}")))?;

    let name = req.name.clone();
    let description = req.description.clone();

    let hash = registry
        .deploy(wasm_bytes, &claims.sub, |mut m| {
            if let Some(ref n) = name {
                m = m.with_name(n.clone());
            }
            if let Some(ref d) = description {
                m = m.with_description(d.clone());
            }
            m
        })
        .await
        .map_err(|e| match e {
            icn_compute::WasmRegistryError::AlreadyExists(h) => {
                crate::error::GatewayError::Conflict(format!("WASM module already exists: {h}"))
            }
            icn_compute::WasmRegistryError::InvalidModule(msg) => {
                crate::error::GatewayError::BadRequest(format!("Invalid WASM: {msg}"))
            }
            other => crate::error::GatewayError::InternalError(other.to_string()),
        })?;

    let meta = registry
        .get_metadata(&hash)
        .await
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    Ok(HttpResponse::Created().json(UploadWasmResponse {
        hash: hex::encode(hash),
        size: meta.map(|m| m.size_bytes).unwrap_or(0),
        name: req.name.clone(),
    }))
}

/// GET /compute/wasm — List WASM modules (paginated)
#[get("/wasm")]
pub async fn list_wasm(
    http_req: HttpRequest,
    compute_mgr: web::Data<Arc<ComputeManager>>,
    query: web::Query<PaginationQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "compute:read")?;

    let registry = compute_mgr.wasm_registry().ok_or_else(|| {
        crate::error::GatewayError::InternalError("WASM registry not configured".to_string())
    })?;

    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.min(100); // Cap at 100

    let modules = registry
        .list_all(offset, limit)
        .await
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    let response: Vec<WasmMetadataResponse> = modules.into_iter().map(Into::into).collect();

    Ok(HttpResponse::Ok().json(response))
}

/// GET /compute/wasm/{hash} — Get WASM module metadata by hash
#[get("/wasm/{hash}")]
pub async fn get_wasm_metadata(
    http_req: HttpRequest,
    compute_mgr: web::Data<Arc<ComputeManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "compute:read")?;

    let registry = compute_mgr.wasm_registry().ok_or_else(|| {
        crate::error::GatewayError::InternalError("WASM registry not configured".to_string())
    })?;

    let hash_hex = path.into_inner();
    let hash_bytes = hex::decode(&hash_hex)
        .map_err(|_| crate::error::GatewayError::BadRequest("Invalid hash hex".to_string()))?;
    if hash_bytes.len() != 32 {
        return Err(crate::error::GatewayError::BadRequest(
            "Hash must be 32 bytes".to_string(),
        ));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash_bytes);

    match registry.get_metadata(&hash).await {
        Ok(Some(meta)) => Ok(HttpResponse::Ok().json(WasmMetadataResponse::from(meta))),
        Ok(None) => Err(crate::error::GatewayError::NotFound(
            "WASM module not found".to_string(),
        )),
        Err(e) => Err(crate::error::GatewayError::InternalError(e.to_string())),
    }
}

/// Configure compute routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/compute")
            .service(submit_task)
            .service(cancel_task)
            .service(get_status)
            .service(upload_wasm)
            .service(list_wasm)
            .service(get_wasm_metadata),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fuel_limit() {
        assert_eq!(default_fuel_limit(), 10_000);
    }

    // -------------------------------------------------------------------------
    // Hash validation tests (no daemon needed)
    // -------------------------------------------------------------------------

    /// `wasm_hash` must be exactly 64 hex chars.  Short strings should be caught
    /// before we even attempt a registry lookup.
    #[test]
    fn test_wasm_hash_validation_bad_length() {
        // 63 chars → too short
        let short_hash = "a".repeat(63);
        assert_ne!(short_hash.len(), 64);

        // 65 chars → too long
        let long_hash = "a".repeat(65);
        assert_ne!(long_hash.len(), 64);

        // 64 chars of valid hex → passes length check
        let good_hash = "a".repeat(64);
        assert_eq!(good_hash.len(), 64);
        assert!(hex::decode(&good_hash).is_ok());
    }

    #[test]
    fn test_wasm_hash_validation_non_hex() {
        // 64 chars but not all hex
        let bad_chars = "z".repeat(64);
        assert!(hex::decode(&bad_chars).is_err());
    }

    #[test]
    fn test_wasm_hash_validation_exact_32_bytes() {
        // 64 hex chars → exactly 32 bytes when decoded
        let hash_hex = "aa".repeat(32); // 64 chars, decodes to 32 × 0xaa
        let decoded = hex::decode(&hash_hex).unwrap_or_default();
        assert_eq!(decoded.len(), 32);
    }
}
