//! Simple SDIS Enrollment API
//!
//! Simplified enrollment flow for identity onboarding:
//! 1. Start enrollment → receive QR code
//! 2. Device scans QR → verifies possession
//! 3. Steward vouches → upgrades trust
//! 4. Complete enrollment → receive DID + recovery codes

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::{GatewayError, Result};

/// Enrollment state store
pub struct EnrollmentStore {
    enrollments: RwLock<std::collections::HashMap<String, EnrollmentSession>>,
}

impl EnrollmentStore {
    pub fn new() -> Self {
        Self {
            enrollments: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for EnrollmentStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Enrollment session state
#[derive(Debug, Clone)]
pub struct EnrollmentSession {
    pub enrollment_id: String,
    pub identity_name: String,
    pub coop_id: String,
    pub verification_code: String,
    pub level: u8,
    pub created_at: u64,
    pub expires_at: u64,
    pub ephemeral_did: Option<String>,
    pub steward_vouch: Option<String>,
    pub steward_did: Option<String>,
    pub vouched_at: Option<u64>,
    pub rejected: bool,
    pub rejection_reason: Option<String>,
    pub rejected_at: Option<u64>,
    pub rejected_by: Option<String>,
}

// ============================================================================
// Request/Response Models
// ============================================================================

/// Start enrollment request
#[derive(Debug, Deserialize)]
pub struct StartEnrollmentRequest {
    pub identity_name: String,
    pub coop_id: String,
}

/// Start enrollment response
#[derive(Debug, Serialize)]
pub struct StartEnrollmentResponse {
    pub enrollment_id: String,
    pub verification_code: String,
    pub qr_code: String,
    pub expires_at: String,
}

/// Level 1 verification request
#[derive(Debug, Deserialize)]
pub struct VerifyLevel1Request {
    pub enrollment_id: String,
    pub device_proof: String,
}

/// Level 2 verification request
#[derive(Debug, Deserialize)]
pub struct VerifyLevel2Request {
    pub enrollment_id: String,
    pub vouch_statement: String,
}

/// Complete enrollment request
#[derive(Debug, Deserialize)]
pub struct CompleteEnrollmentRequest {
    pub enrollment_id: String,
    pub ephemeral_did: String,
    pub ephemeral_signature: String,
    pub device_info: DeviceInfo,
}

#[derive(Debug, Deserialize)]
pub struct DeviceInfo {
    pub device_type: String,
    pub os: String,
    pub app_version: String,
}

/// Complete enrollment response
#[derive(Debug, Serialize)]
pub struct CompleteEnrollmentResponse {
    pub did: String,
    pub recovery_codes: Vec<String>,
    pub auth_token: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /enrollment/start
#[post("/enrollment/start")]
pub async fn start_enrollment(
    store: web::Data<Arc<EnrollmentStore>>,
    req: web::Json<StartEnrollmentRequest>,
) -> Result<HttpResponse> {
    let enrollment_id = Uuid::new_v4().to_string();
    let verification_code = generate_verification_code();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expires_at = now + 86400; // 24 hours

    // Generate QR code data
    let qr_data = serde_json::json!({
        "type": "icn-enrollment",
        "enrollment_id": enrollment_id,
        "challenge": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, enrollment_id.as_bytes()),
        "gateway_url": "http://10.8.10.40:30080"
    });

    // Create enrollment session
    let session = EnrollmentSession {
        enrollment_id: enrollment_id.clone(),
        identity_name: req.identity_name.clone(),
        coop_id: req.coop_id.clone(),
        verification_code: verification_code.clone(),
        level: 0,
        created_at: now,
        expires_at,
        ephemeral_did: None,
        steward_vouch: None,
        steward_did: None,
        vouched_at: None,
        rejected: false,
        rejection_reason: None,
        rejected_at: None,
        rejected_by: None,
    };

    store
        .enrollments
        .write()
        .await
        .insert(enrollment_id.clone(), session);

    Ok(HttpResponse::Ok().json(StartEnrollmentResponse {
        enrollment_id,
        verification_code,
        qr_code: format!("data:image/png;base64,{}", qr_data.to_string()),
        expires_at: format_timestamp(expires_at),
    }))
}

/// POST /verify/level1
#[post("/verify/level1")]
pub async fn verify_level1(
    store: web::Data<Arc<EnrollmentStore>>,
    req: web::Json<VerifyLevel1Request>,
) -> Result<HttpResponse> {
    let mut enrollments = store.enrollments.write().await;
    let session = enrollments
        .get_mut(&req.enrollment_id)
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    // Verify not expired
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now > session.expires_at {
        return Err(GatewayError::BadRequest(
            "Enrollment expired".to_string(),
        ));
    }

    // TODO: Verify device_proof signature
    // For now, just accept it
    session.level = 1;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "verified",
        "level": 1,
        "message": "Device verified successfully"
    })))
}

/// POST /verify/level2
#[post("/verify/level2")]
pub async fn verify_level2(
    store: web::Data<Arc<EnrollmentStore>>,
    req: web::Json<VerifyLevel2Request>,
    // TODO: Extract steward DID from Bearer token
) -> Result<HttpResponse> {
    let mut enrollments = store.enrollments.write().await;
    let session = enrollments
        .get_mut(&req.enrollment_id)
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    // Must be level 1 first
    if session.level < 1 {
        return Err(GatewayError::BadRequest(
            "Must complete Level 1 verification first".to_string(),
        ));
    }

    // TODO: Verify steward has sufficient trust
    // For now, just accept it
    session.level = 2;
    session.steward_vouch = Some(req.vouch_statement.clone());

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "verified",
        "level": 2,
        "message": "Steward vouch recorded successfully"
    })))
}

/// POST /enrollment/complete
#[post("/enrollment/complete")]
pub async fn complete_enrollment(
    store: web::Data<Arc<EnrollmentStore>>,
    req: web::Json<CompleteEnrollmentRequest>,
) -> Result<HttpResponse> {
    let enrollments = store.enrollments.read().await;
    let session = enrollments
        .get(&req.enrollment_id)
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    // Must be level 2
    if session.level < 2 {
        return Err(GatewayError::BadRequest(
            "Must complete Level 2 verification first".to_string(),
        ));
    }

    // TODO: Create actual DID and keystore
    // For now, return mock data
    let did = format!("did:icn:z{}", Uuid::new_v4().to_string().replace('-', ""));
    let recovery_codes: Vec<String> = (0..5)
        .map(|i| format!("RECOVERY-CODE-{:02}", i + 1))
        .collect();
    let auth_token = format!("Bearer {}", Uuid::new_v4());

    Ok(HttpResponse::Ok().json(CompleteEnrollmentResponse {
        did,
        recovery_codes,
        auth_token,
    }))
}

// ============================================================================
// Helpers
// ============================================================================

fn generate_verification_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("VERIFY-{:04}", rng.gen_range(1000..=9999))
}

fn format_timestamp(ts: u64) -> String {
    use chrono::{DateTime, Utc};
    DateTime::from_timestamp(ts as i64, 0)
        .unwrap_or_else(|| Utc::now())
        .to_rfc3339()
}

// ============================================================================
// Steward API Endpoints
// ============================================================================

/// GET /pending - List pending enrollments for stewards
#[actix_web::get("/pending")]
pub async fn list_pending_enrollments(
    store: web::Data<Arc<EnrollmentStore>>,
) -> Result<HttpResponse> {
    let enrollments = store.enrollments.read().await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Filter: level < 2, not rejected, not expired
    let pending: Vec<_> = enrollments
        .values()
        .filter(|s| s.level < 2 && !s.rejected && s.expires_at > now)
        .map(|s| serde_json::json!({
            "enrollment_id": s.enrollment_id,
            "identity_name": s.identity_name,
            "coop_id": s.coop_id,
            "level": s.level,
            "created_at": format_timestamp(s.created_at),
            "expires_at": format_timestamp(s.expires_at),
        }))
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "pending_count": pending.len(),
        "enrollments": pending
    })))
}

/// POST /vouch/{enrollment_id} - Steward vouches for an enrollment
#[post("/vouch/{enrollment_id}")]
pub async fn steward_vouch(
    store: web::Data<Arc<EnrollmentStore>>,
    enrollment_id: web::Path<String>,
    req: web::Json<StewardVouchRequest>,
) -> Result<HttpResponse> {
    let mut enrollments = store.enrollments.write().await;
    let session = enrollments
        .get_mut(enrollment_id.as_str())
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    // Must be level 1 first
    if session.level < 1 {
        return Err(GatewayError::BadRequest(
            "Enrollment must complete Level 1 verification first".to_string(),
        ));
    }

    // Check not expired
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now > session.expires_at {
        return Err(GatewayError::BadRequest("Enrollment expired".to_string()));
    }

    // Record the vouch
    session.level = 2;
    session.steward_vouch = Some(req.vouch_statement.clone());
    session.steward_did = req.steward_did.clone();
    session.vouched_at = Some(now);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "vouched",
        "enrollment_id": enrollment_id.as_str(),
        "level": 2,
        "message": "Steward vouch recorded. Enrollment ready for completion."
    })))
}

/// Steward vouch request
#[derive(Debug, Deserialize)]
pub struct StewardVouchRequest {
    pub vouch_statement: String,
    #[serde(default)]
    pub steward_did: Option<String>,
}

/// GET /status/{enrollment_id} - Get enrollment status
#[actix_web::get("/status/{enrollment_id}")]
pub async fn get_enrollment_status(
    store: web::Data<Arc<EnrollmentStore>>,
    enrollment_id: web::Path<String>,
) -> Result<HttpResponse> {
    let enrollments = store.enrollments.read().await;
    let session = enrollments
        .get(enrollment_id.as_str())
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    let status = if session.rejected {
        "rejected"
    } else {
        match session.level {
            0 => "pending_device_verification",
            1 => "pending_steward_vouch",
            2 => "ready_for_completion",
            _ => "unknown",
        }
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "enrollment_id": session.enrollment_id,
        "identity_name": session.identity_name,
        "coop_id": session.coop_id,
        "level": session.level,
        "status": status,
        "has_steward_vouch": session.steward_vouch.is_some(),
        "rejected": session.rejected,
        "rejection_reason": session.rejection_reason,
        "rejected_at": session.rejected_at.map(format_timestamp),
        "created_at": format_timestamp(session.created_at),
        "expires_at": format_timestamp(session.expires_at),
    })))
}

/// POST /reject/{enrollment_id} - Steward rejects an enrollment
#[post("/reject/{enrollment_id}")]
pub async fn reject_enrollment(
    store: web::Data<Arc<EnrollmentStore>>,
    enrollment_id: web::Path<String>,
    req: web::Json<RejectRequest>,
) -> Result<HttpResponse> {
    let mut enrollments = store.enrollments.write().await;
    let session = enrollments
        .get_mut(enrollment_id.as_str())
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    // Check not already rejected
    if session.rejected {
        return Err(GatewayError::BadRequest(
            "Enrollment already rejected".to_string(),
        ));
    }

    // Check not already completed (level 2)
    if session.level >= 2 {
        return Err(GatewayError::BadRequest(
            "Cannot reject a completed enrollment".to_string(),
        ));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Record the rejection
    session.rejected = true;
    session.rejection_reason = Some(req.reason.clone());
    session.rejected_at = Some(now);
    session.rejected_by = req.steward_did.clone();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "rejected",
        "enrollment_id": enrollment_id.as_str(),
        "reason": req.reason,
        "message": "Enrollment has been rejected."
    })))
}

/// Reject enrollment request
#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub reason: String,
    #[serde(default)]
    pub steward_did: Option<String>,
}

/// GET /steward/stats - Get steward statistics
#[actix_web::get("/steward/stats")]
pub async fn get_steward_stats(
    store: web::Data<Arc<EnrollmentStore>>,
) -> Result<HttpResponse> {
    let enrollments = store.enrollments.read().await;

    // Count vouched enrollments
    let vouched: Vec<_> = enrollments
        .values()
        .filter(|s| s.vouched_at.is_some())
        .collect();

    let total_vouches = vouched.len();

    // Calculate monthly vouches (last 30 days)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let month_ago = now.saturating_sub(30 * 24 * 60 * 60);

    let monthly_vouches = vouched
        .iter()
        .filter(|s| s.vouched_at.unwrap_or(0) >= month_ago)
        .count();

    // Calculate average response time (time from created_at to vouched_at)
    let response_times: Vec<u64> = vouched
        .iter()
        .filter_map(|s| {
            s.vouched_at.map(|v| v.saturating_sub(s.created_at))
        })
        .collect();

    let avg_response_hours = if response_times.is_empty() {
        0
    } else {
        let avg_secs: u64 = response_times.iter().sum::<u64>() / response_times.len() as u64;
        avg_secs / 3600 // Convert to hours
    };

    // Count rejections
    let total_rejections = enrollments
        .values()
        .filter(|s| s.rejected)
        .count();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "total_vouches": total_vouches,
        "monthly_vouches": monthly_vouches,
        "total_rejections": total_rejections,
        "reputation_score": 100, // TODO: Calculate from trust graph
        "avg_response_hours": avg_response_hours,
    })))
}

/// GET /steward/history - Get vouch history
#[actix_web::get("/steward/history")]
pub async fn get_vouch_history(
    store: web::Data<Arc<EnrollmentStore>>,
    query: web::Query<HistoryQuery>,
) -> Result<HttpResponse> {
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let enrollments = store.enrollments.read().await;

    // Get all vouched enrollments, sorted by vouched_at descending
    let mut vouched: Vec<_> = enrollments
        .values()
        .filter(|s| s.vouched_at.is_some())
        .collect();

    // Sort by vouched_at descending (most recent first)
    vouched.sort_by(|a, b| b.vouched_at.cmp(&a.vouched_at));

    let total = vouched.len();

    // Apply pagination
    let vouches: Vec<_> = vouched
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|s| serde_json::json!({
            "enrollment_id": s.enrollment_id,
            "identity_name": s.identity_name,
            "coop_id": s.coop_id,
            "vouch_statement": s.steward_vouch,
            "vouched_at": format_timestamp(s.vouched_at.unwrap_or(0)),
            "steward_did": s.steward_did,
        }))
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "vouches": vouches,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

/// History query parameters
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ============================================================================
// Configuration
// ============================================================================

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(start_enrollment)
        .service(verify_level1)
        .service(verify_level2)
        .service(complete_enrollment)
        .service(list_pending_enrollments)
        .service(steward_vouch)
        .service(get_enrollment_status)
        .service(reject_enrollment)
        .service(get_steward_stats)
        .service(get_vouch_history);
}
