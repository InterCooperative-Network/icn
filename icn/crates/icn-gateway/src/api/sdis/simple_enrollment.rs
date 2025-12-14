//! Simple SDIS Enrollment API
//!
//! Simplified enrollment flow for identity onboarding:
//! 1. Start enrollment → receive QR code
//! 2. Device scans QR → verifies possession
//! 3. Steward vouches → upgrades trust
//! 4. Complete enrollment → receive DID + recovery codes

use actix_web::{post, web, HttpRequest, HttpResponse};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::AuthManager;
use crate::error::{GatewayError, Result};
use crate::trust_mgr::TrustManager;

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
    /// Device proof containing ephemeral DID and signature
    pub device_proof: DeviceProof,
}

/// Device proof for Level 1 verification
#[derive(Debug, Deserialize)]
pub struct DeviceProof {
    /// Ephemeral DID generated on the device
    pub ephemeral_did: String,
    /// Hex-encoded Ed25519 signature over the challenge (enrollment_id)
    pub signature: String,
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
        qr_code: format!("data:image/png;base64,{qr_data}"),
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
        return Err(GatewayError::BadRequest("Enrollment expired".to_string()));
    }

    // Verify device_proof signature
    let ephemeral_did: Did = req
        .device_proof
        .ephemeral_did
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid ephemeral DID: {e}")))?;

    // Decode signature from hex
    let signature_bytes = hex::decode(&req.device_proof.signature)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature encoding: {e}")))?;

    // Validate signature length (Ed25519 = 64 bytes)
    if signature_bytes.len() != 64 {
        return Err(GatewayError::BadRequest(format!(
            "Invalid signature length: expected 64 bytes, got {}",
            signature_bytes.len()
        )));
    }

    // Extract verifying key from ephemeral DID
    let verifying_key: VerifyingKey = ephemeral_did
        .to_verifying_key()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID public key: {e}")))?;

    // Parse signature
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature format: {e}")))?;

    // Verify signature over enrollment_id (the challenge)
    verifying_key
        .verify(req.enrollment_id.as_bytes(), &signature)
        .map_err(|_| {
            GatewayError::AuthenticationFailed("Device proof signature verification failed".to_string())
        })?;

    // Signature verified - store ephemeral DID and upgrade level
    session.ephemeral_did = Some(req.device_proof.ephemeral_did.clone());
    session.level = 1;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "verified",
        "level": 1,
        "message": "Device verified successfully"
    })))
}

/// Minimum trust score required for stewards to vouch for enrollees
const STEWARD_MIN_TRUST_SCORE: f64 = 0.4;

/// POST /verify/level2
#[post("/verify/level2")]
pub async fn verify_level2(
    http_req: HttpRequest,
    store: web::Data<Arc<EnrollmentStore>>,
    auth: web::Data<Arc<AuthManager>>,
    trust_mgr: web::Data<Arc<TrustManager>>,
    req: web::Json<VerifyLevel2Request>,
) -> Result<HttpResponse> {
    // Extract steward DID from Bearer token
    let auth_header = http_req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| GatewayError::AuthenticationFailed("Missing Authorization header".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| GatewayError::AuthenticationFailed("Invalid Authorization format".to_string()))?;

    let claims = auth.verify_token(token)?;
    let steward_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::InternalError(format!("Invalid steward DID in token: {e}")))?;

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

    // Verify steward has sufficient trust
    // Stewards need a trust score of at least 0.4 (Partner level) to vouch
    // Self-trust is 1.0, so we check from steward's own perspective
    let steward_trust = trust_mgr.compute_trust_score(&steward_did, &steward_did);

    // For self-trust computation, we look at incoming edges to the steward
    // A new node with no incoming edges gets 0.0, bootstrapped stewards get higher
    let incoming_edges = trust_mgr.get_incoming_edges(&steward_did);
    let avg_incoming_trust = if incoming_edges.is_empty() {
        0.0
    } else {
        incoming_edges.iter().map(|e| e.score).sum::<f64>() / incoming_edges.len() as f64
    };

    // Use the higher of self-trust or avg incoming trust
    // This allows bootstrapping (first steward has self-trust) and growth (new stewards earn trust)
    let effective_trust = steward_trust.max(avg_incoming_trust);

    if effective_trust < STEWARD_MIN_TRUST_SCORE {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Insufficient trust to vouch: {effective_trust:.2} < {STEWARD_MIN_TRUST_SCORE:.2} required"
        )));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    session.level = 2;
    session.steward_vouch = Some(req.vouch_statement.clone());
    session.steward_did = Some(steward_did.to_string());
    session.vouched_at = Some(now);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "verified",
        "level": 2,
        "steward_did": steward_did.to_string(),
        "message": "Steward vouch recorded successfully"
    })))
}

/// POST /enrollment/complete
#[post("/enrollment/complete")]
pub async fn complete_enrollment(
    store: web::Data<Arc<EnrollmentStore>>,
    auth: web::Data<Arc<AuthManager>>,
    trust_mgr: web::Data<Arc<TrustManager>>,
    req: web::Json<CompleteEnrollmentRequest>,
) -> Result<HttpResponse> {
    let mut enrollments = store.enrollments.write().await;
    let session = enrollments
        .get_mut(&req.enrollment_id)
        .ok_or_else(|| GatewayError::NotFound("Enrollment not found".to_string()))?;

    // Must be level 2
    if session.level < 2 {
        return Err(GatewayError::BadRequest(
            "Must complete Level 2 verification first".to_string(),
        ));
    }

    // Verify the ephemeral_did matches the one from level 1
    if session.ephemeral_did.as_ref() != Some(&req.ephemeral_did) {
        return Err(GatewayError::BadRequest(
            "Ephemeral DID mismatch".to_string(),
        ));
    }

    // Verify the ephemeral signature (proves device still has the key)
    let ephemeral_did: Did = req
        .ephemeral_did
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid ephemeral DID: {e}")))?;

    let signature_bytes = hex::decode(&req.ephemeral_signature)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature encoding: {e}")))?;

    if signature_bytes.len() != 64 {
        return Err(GatewayError::BadRequest(format!(
            "Invalid signature length: expected 64 bytes, got {}",
            signature_bytes.len()
        )));
    }

    let verifying_key: VerifyingKey = ephemeral_did
        .to_verifying_key()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID public key: {e}")))?;

    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature format: {e}")))?;

    // The device signs a completion message
    let message = format!("complete:{}", req.enrollment_id);
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| {
            GatewayError::AuthenticationFailed("Completion signature verification failed".to_string())
        })?;

    // The DID is the ephemeral_did - in SDIS, keys are created on the device
    let did = req.ephemeral_did.clone();

    // Generate secure recovery codes (8 characters, alphanumeric)
    let recovery_codes: Vec<String> = generate_recovery_codes(5);

    // If there's a vouching steward, create an initial trust edge from steward to new member
    if let Some(ref steward_did_str) = session.steward_did {
        if let Ok(steward_did) = steward_did_str.parse::<Did>() {
            // Create initial trust edge: steward vouched for this member
            let edge = icn_trust::TrustEdge::new(
                steward_did,
                ephemeral_did.clone(),
                0.5, // Initial trust from vouch
            )
            .with_label("enrollment-vouch");
            let _ = trust_mgr.add_edge(edge);
        }
    }

    // Issue auth token for the new identity
    let auth_token = auth.issue_token(
        &ephemeral_did,
        &session.coop_id,
        vec!["ledger:read".to_string(), "ledger:write".to_string()],
    )?;

    // Mark enrollment complete by removing from store
    drop(enrollments); // Release lock before removing
    let mut enrollments = store.enrollments.write().await;
    enrollments.remove(&req.enrollment_id);

    Ok(HttpResponse::Ok().json(CompleteEnrollmentResponse {
        did,
        recovery_codes,
        auth_token,
    }))
}

/// Generate secure recovery codes
fn generate_recovery_codes(count: usize) -> Vec<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // Exclude confusing chars (0, 1, I, O)

    (0..count)
        .map(|_| {
            (0..8)
                .map(|_| {
                    let idx = rng.gen_range(0..chars.len());
                    chars[idx] as char
                })
                .collect::<String>()
        })
        .collect()
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
        .unwrap_or_else(Utc::now)
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
        .map(|s| {
            serde_json::json!({
                "enrollment_id": s.enrollment_id,
                "identity_name": s.identity_name,
                "coop_id": s.coop_id,
                "level": s.level,
                "created_at": format_timestamp(s.created_at),
                "expires_at": format_timestamp(s.expires_at),
            })
        })
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
    http_req: HttpRequest,
    store: web::Data<Arc<EnrollmentStore>>,
    auth: web::Data<Arc<AuthManager>>,
    trust_mgr: web::Data<Arc<TrustManager>>,
) -> Result<HttpResponse> {
    // Extract steward DID from Bearer token
    let auth_header = http_req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| GatewayError::AuthenticationFailed("Missing Authorization header".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| GatewayError::AuthenticationFailed("Invalid Authorization format".to_string()))?;

    let claims = auth.verify_token(token)?;
    let steward_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::InternalError(format!("Invalid steward DID in token: {e}")))?;

    let enrollments = store.enrollments.read().await;

    // Count vouched enrollments by THIS steward
    let vouched: Vec<_> = enrollments
        .values()
        .filter(|s| {
            s.vouched_at.is_some() && s.steward_did.as_ref() == Some(&steward_did.to_string())
        })
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
        .filter_map(|s| s.vouched_at.map(|v| v.saturating_sub(s.created_at)))
        .collect();

    let avg_response_hours = if response_times.is_empty() {
        0
    } else {
        let avg_secs: u64 = response_times.iter().sum::<u64>() / response_times.len() as u64;
        avg_secs / 3600 // Convert to hours
    };

    // Count rejections by this steward
    let total_rejections = enrollments
        .values()
        .filter(|s| s.rejected && s.rejected_by.as_ref() == Some(&steward_did.to_string()))
        .count();

    // Calculate reputation score from trust graph
    // Based on: average incoming trust * 100, weighted by number of edges
    let incoming_edges = trust_mgr.get_incoming_edges(&steward_did);
    let reputation_score = if incoming_edges.is_empty() {
        // No incoming edges - use base score based on vouch history
        // More vouches = higher initial reputation
        let base = 50.0;
        let vouch_bonus = (total_vouches as f64 * 2.0).min(30.0); // Up to 30 points for vouches
        (base + vouch_bonus) as u64
    } else {
        // Calculate weighted average of incoming trust
        let total_trust: f64 = incoming_edges.iter().map(|e| e.score).sum();
        let avg_trust = total_trust / incoming_edges.len() as f64;
        // Scale to 0-100 with edge count bonus
        let edge_bonus = (incoming_edges.len() as f64).ln().min(1.0) * 10.0;
        ((avg_trust * 90.0) + edge_bonus).round().min(100.0) as u64
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "total_vouches": total_vouches,
        "monthly_vouches": monthly_vouches,
        "total_rejections": total_rejections,
        "reputation_score": reputation_score,
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
        .map(|s| {
            serde_json::json!({
                "enrollment_id": s.enrollment_id,
                "identity_name": s.identity_name,
                "coop_id": s.coop_id,
                "vouch_statement": s.steward_vouch,
                "vouched_at": format_timestamp(s.vouched_at.unwrap_or(0)),
                "steward_did": s.steward_did,
            })
        })
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
