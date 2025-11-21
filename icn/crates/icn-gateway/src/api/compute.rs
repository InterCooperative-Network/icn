//! Compute API endpoints
//!
//! RESTful API for submitting and monitoring distributed compute tasks.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::compute_mgr::ComputeManager;
use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::middleware::{get_claims, require_scope};

/// Request to submit a compute task
#[derive(Debug, serde::Deserialize)]
pub struct SubmitTaskRequest {
    /// Task ID (auto-generated if not provided)
    #[serde(default)]
    pub task_id: Option<String>,
    /// CCL contract JSON
    pub code: String,
    /// Input arguments (JSON)
    #[serde(default)]
    pub inputs: serde_json::Value,
    /// Fuel limit (default: 10000)
    #[serde(default = "default_fuel_limit")]
    pub fuel_limit: u64,
    /// Deadline in milliseconds from now (optional)
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    /// Payment rate per 1000 fuel (optional)
    #[serde(default)]
    pub payment_rate: Option<u64>,
    /// Payment currency (default: credits)
    #[serde(default)]
    pub payment_currency: Option<String>,
}

fn default_fuel_limit() -> u64 {
    10_000
}

/// Response from submitting a task
#[derive(Debug, serde::Serialize)]
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
    // Require compute:write scope
    require_scope(&http_req, "compute:write")?;

    // Get submitter DID from JWT
    let claims = get_claims(&http_req)
        .ok_or_else(|| crate::error::GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let submitter_did = claims.sub.clone();

    // Generate task ID if not provided
    let task_id = req.task_id.clone()
        .unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));

    // Convert inputs to bytes
    let inputs = if req.inputs.is_null() {
        vec![]
    } else {
        serde_json::to_vec(&req.inputs).unwrap_or_default()
    };

    // Submit task
    let task_hash = compute_mgr.submit_task(
        task_id.clone(),
        submitter_did.clone(),
        req.code.clone(),
        inputs,
        req.fuel_limit,
        req.deadline_ms,
        req.payment_rate,
        req.payment_currency.clone(),
    ).await
        .map_err(|e| crate::error::GatewayError::InternalError(e.to_string()))?;

    let task_hash_hex = hex::encode(task_hash);

    // Broadcast event
    broadcaster.broadcast("compute", GatewayEvent::ComputeTaskSubmitted {
        task_id: task_id.clone(),
        task_hash: task_hash_hex.clone(),
        submitter: submitter_did,
        fuel_limit: req.fuel_limit,
    }).await;

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
            "Task hash must be 32 bytes".to_string()
        ));
    }

    let mut task_hash = [0u8; 32];
    task_hash.copy_from_slice(&hash_bytes);

    // Get status
    match compute_mgr.get_status(task_hash).await {
        Ok(Some(status)) => Ok(HttpResponse::Ok().json(status)),
        Ok(None) => Err(crate::error::GatewayError::NotFound("Task not found".to_string())),
        Err(e) => Err(crate::error::GatewayError::InternalError(e.to_string())),
    }
}

/// Configure compute routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/compute")
            .service(submit_task)
            .service(get_status)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fuel_limit() {
        assert_eq!(default_fuel_limit(), 10_000);
    }
}
