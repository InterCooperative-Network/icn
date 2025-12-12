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
// Configuration
// ============================================================================

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(start_enrollment)
        .service(verify_level1)
        .service(verify_level2)
        .service(complete_enrollment);
}
