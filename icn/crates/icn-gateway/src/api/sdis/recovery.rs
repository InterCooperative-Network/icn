//! SDIS Recovery API
//!
//! Endpoints for recovering access to an Anchor after key loss.
//! Recovery allows rotating to a new KeyBundle while keeping the same Anchor.
//!
//! # Flow
//!
//! 1. Client submits recovery request with Anchor ID or VUI hint
//! 2. Client provides identity verification proof
//! 3. Stewards verify the proof
//! 4. Once threshold reached, new KeyBundle is authorized
//! 5. Client receives new DID (same Anchor, new keys)

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::enrollment::KeyBundleDto;
use crate::error::{GatewayError, Result};

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to start a recovery ceremony
#[derive(Debug, Deserialize)]
pub struct StartRecoveryRequest {
    /// Anchor ID (if known)
    pub anchor_id: Option<String>,
    /// VUI hint (partial information to help locate anchor)
    pub vui_hint: Option<String>,
    /// Identity verification data
    pub verification_data: serde_json::Value,
    /// New keybundle to rotate to
    pub new_keybundle: KeyBundleDto,
}

/// Response after starting recovery
#[derive(Debug, Serialize)]
pub struct StartRecoveryResponse {
    /// Unique recovery ceremony ID
    pub recovery_id: String,
    /// Current status
    pub status: RecoveryStatus,
    /// Number of steward approvals required
    pub required_stewards: u32,
}

/// Recovery ceremony status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    PendingStewardVerification,
    Approved,
    Rejected,
    Completed,
    Expired,
}

/// Response for recovery status query
#[derive(Debug, Serialize)]
pub struct RecoveryStatusResponse {
    /// Recovery ID
    pub recovery_id: String,
    /// Current status
    pub status: RecoveryStatus,
    /// Number of steward approvals received
    pub steward_approvals: u32,
    /// Number of steward approvals required
    pub required_stewards: u32,
    /// Reason for rejection (if rejected)
    pub rejection_reason: Option<String>,
}

/// Request to complete recovery
#[derive(Debug, Deserialize)]
pub struct CompleteRecoveryRequest {
    /// Optional: Recovery share from trusted contacts
    pub recovery_share: Option<String>,
}

/// Response after completing recovery
#[derive(Debug, Serialize)]
pub struct CompleteRecoveryResponse {
    /// Original anchor ID (unchanged)
    pub anchor_id: String,
    /// New DID with rotated keys
    pub new_did: String,
    /// New keybundle version
    pub keybundle_version: u32,
    /// Instructions for updating other devices
    pub update_instructions: String,
}

// ============================================================================
// In-Memory Recovery Storage
// ============================================================================

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// In-memory storage for recovery ceremonies
pub struct RecoveryStore {
    ceremonies: RwLock<HashMap<String, RecoveryCeremony>>,
}

impl Default for RecoveryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RecoveryStore {
    pub fn new() -> Self {
        Self {
            ceremonies: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_ceremony(&self, ceremony: RecoveryCeremony) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let mut ceremonies = self
            .ceremonies
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire write lock".to_string()))?;
        ceremonies.insert(id.clone(), ceremony);
        Ok(id)
    }

    pub fn get_ceremony(&self, id: &str) -> Result<Option<RecoveryCeremony>> {
        let ceremonies = self
            .ceremonies
            .read()
            .map_err(|_| GatewayError::InternalError("Failed to acquire read lock".to_string()))?;
        Ok(ceremonies.get(id).cloned())
    }

    pub fn update_ceremony(&self, id: &str, ceremony: RecoveryCeremony) -> Result<()> {
        let mut ceremonies = self
            .ceremonies
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire write lock".to_string()))?;
        ceremonies.insert(id.to_string(), ceremony);
        Ok(())
    }
}

/// Recovery ceremony state
#[derive(Debug, Clone)]
pub struct RecoveryCeremony {
    pub anchor_id: Option<String>,
    pub vui_hint: Option<String>,
    pub verification_data: serde_json::Value,
    pub new_keybundle: KeyBundleDto,
    pub status: RecoveryStatus,
    pub steward_approvals: u32,
    pub required_stewards: u32,
    pub rejection_reason: Option<String>,
    pub created_at: u64,
    pub old_keybundle_version: Option<u32>,
}

impl RecoveryCeremony {
    pub fn new(
        anchor_id: Option<String>,
        vui_hint: Option<String>,
        verification_data: serde_json::Value,
        new_keybundle: KeyBundleDto,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            anchor_id,
            vui_hint,
            verification_data,
            new_keybundle,
            status: RecoveryStatus::PendingStewardVerification,
            steward_approvals: 0,
            required_stewards: 3, // Same threshold as enrollment
            rejection_reason: None,
            created_at: now,
            old_keybundle_version: None,
        }
    }

    pub fn is_approved(&self) -> bool {
        self.steward_approvals >= self.required_stewards
    }

    pub fn approve_by_steward(&mut self) {
        self.steward_approvals += 1;
        if self.is_approved() {
            self.status = RecoveryStatus::Approved;
        }
    }

    pub fn reject(&mut self, reason: String) {
        self.status = RecoveryStatus::Rejected;
        self.rejection_reason = Some(reason);
    }

    pub fn complete(&mut self) {
        self.status = RecoveryStatus::Completed;
    }
}

// ============================================================================
// API Handlers
// ============================================================================

/// Start a new recovery ceremony
///
/// POST /v1/sdis/recovery/start
#[post("/start")]
pub async fn start_recovery(
    store: web::Data<Arc<RecoveryStore>>,
    req: web::Json<StartRecoveryRequest>,
) -> Result<HttpResponse> {
    // Validate request
    validate_recovery_request(&req)?;

    // Create ceremony
    let ceremony = RecoveryCeremony::new(
        req.anchor_id.clone(),
        req.vui_hint.clone(),
        req.verification_data.clone(),
        req.new_keybundle.clone(),
    );

    let recovery_id = store.create_ceremony(ceremony.clone())?;

    // In a real implementation, this would:
    // 1. Look up the anchor by ID or VUI hint
    // 2. Submit verification data to steward network
    // 3. Stewards verify identity proofs
    // 4. Stewards vote on recovery approval

    let response = StartRecoveryResponse {
        recovery_id,
        status: ceremony.status.clone(),
        required_stewards: ceremony.required_stewards,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Get recovery ceremony status
///
/// GET /v1/sdis/recovery/{recovery_id}
#[get("/{recovery_id}")]
pub async fn get_recovery_status(
    store: web::Data<Arc<RecoveryStore>>,
    recovery_id: web::Path<String>,
) -> Result<HttpResponse> {
    let ceremony = store
        .get_ceremony(&recovery_id)?
        .ok_or_else(|| GatewayError::NotFound("Recovery ceremony not found".to_string()))?;

    let response = RecoveryStatusResponse {
        recovery_id: recovery_id.to_string(),
        status: ceremony.status.clone(),
        steward_approvals: ceremony.steward_approvals,
        required_stewards: ceremony.required_stewards,
        rejection_reason: ceremony.rejection_reason.clone(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Complete recovery and receive new DID
///
/// POST /v1/sdis/recovery/{recovery_id}/complete
#[post("/{recovery_id}/complete")]
pub async fn complete_recovery(
    store: web::Data<Arc<RecoveryStore>>,
    recovery_id: web::Path<String>,
    _req: web::Json<CompleteRecoveryRequest>,
) -> Result<HttpResponse> {
    let mut ceremony = store
        .get_ceremony(&recovery_id)?
        .ok_or_else(|| GatewayError::NotFound("Recovery ceremony not found".to_string()))?;

    // Check if approved
    if ceremony.status != RecoveryStatus::Approved {
        return Err(GatewayError::BadRequest(
            "Recovery not approved yet".to_string(),
        ));
    }

    // Complete the recovery
    ceremony.complete();

    // In a real implementation:
    // 1. Rotate the keybundle for this anchor
    // 2. Generate new DID from anchor + new keybundle
    // 3. Revoke old DID
    // 4. Update anchor → DID mapping
    // 5. Notify all devices

    let anchor_id = ceremony
        .anchor_id
        .clone()
        .unwrap_or_else(|| "anchor_recovered".to_string());
    let new_did = format!("did:icn:{}", Uuid::new_v4().to_string().replace("-", ""));
    let old_version = ceremony.old_keybundle_version.unwrap_or(1);
    let new_version = old_version + 1;

    store.update_ceremony(&recovery_id, ceremony)?;

    let response = CompleteRecoveryResponse {
        anchor_id,
        new_did,
        keybundle_version: new_version,
        update_instructions:
            "Use the new DID on all your devices. Your Anchor ID remains the same.".to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Simulate steward approval (for testing/development only)
///
/// POST /v1/sdis/recovery/{recovery_id}/approve
///
/// **WARNING**: This endpoint should NOT be enabled in production.
/// Set environment variable `ICN_ENABLE_ADMIN_ENDPOINTS=false` to disable.
#[post("/{recovery_id}/approve")]
pub async fn approve_recovery(
    store: web::Data<Arc<RecoveryStore>>,
    recovery_id: web::Path<String>,
) -> Result<HttpResponse> {
    // Production guard: check environment variable
    let admin_enabled = std::env::var("ICN_ENABLE_ADMIN_ENDPOINTS")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase() == "true";
    
    if !admin_enabled {
        return Err(GatewayError::Forbidden(
            "Admin endpoints are disabled in production".to_string(),
        ));
    }

    let mut ceremony = store
        .get_ceremony(&recovery_id)?
        .ok_or_else(|| GatewayError::NotFound("Recovery ceremony not found".to_string()))?;

    ceremony.approve_by_steward();
    store.update_ceremony(&recovery_id, ceremony.clone())?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "approved",
        "approvals": ceremony.steward_approvals,
        "required": ceremony.required_stewards
    })))
}

// ============================================================================
// Helper Functions
// ============================================================================

fn validate_recovery_request(req: &StartRecoveryRequest) -> Result<()> {
    // Must provide either anchor_id or vui_hint
    if req.anchor_id.is_none() && req.vui_hint.is_none() {
        return Err(GatewayError::BadRequest(
            "Must provide either anchor_id or vui_hint".to_string(),
        ));
    }

    // Validate new keybundle
    if req.new_keybundle.ed25519_pub.is_empty()
        || req.new_keybundle.ml_dsa_pub.is_empty()
        || req.new_keybundle.x25519_pub.is_empty()
    {
        return Err(GatewayError::BadRequest(
            "All public keys must be provided in new keybundle".to_string(),
        ));
    }

    Ok(())
}

// ============================================================================
// Configuration Function
// ============================================================================

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/recovery")
            .service(start_recovery)
            .service(get_recovery_status)
            .service(complete_recovery)
            .service(approve_recovery), // Guarded by ICN_ENABLE_ADMIN_ENDPOINTS env var
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keybundle() -> KeyBundleDto {
        KeyBundleDto {
            ed25519_pub: "test_ed25519_new".to_string(),
            ml_dsa_pub: "test_ml_dsa_new".to_string(),
            x25519_pub: "test_x25519_new".to_string(),
        }
    }

    #[test]
    fn test_recovery_ceremony_creation() {
        let ceremony = RecoveryCeremony::new(
            Some("anchor_123".to_string()),
            None,
            serde_json::json!({"type": "selfie"}),
            test_keybundle(),
        );

        assert_eq!(ceremony.status, RecoveryStatus::PendingStewardVerification);
        assert_eq!(ceremony.steward_approvals, 0);
        assert_eq!(ceremony.required_stewards, 3);
        assert_eq!(ceremony.anchor_id, Some("anchor_123".to_string()));
    }

    #[test]
    fn test_recovery_approval() {
        let mut ceremony = RecoveryCeremony::new(
            Some("anchor_123".to_string()),
            None,
            serde_json::json!({}),
            test_keybundle(),
        );

        assert!(!ceremony.is_approved());

        ceremony.approve_by_steward();
        ceremony.approve_by_steward();
        assert!(!ceremony.is_approved());

        ceremony.approve_by_steward();
        assert!(ceremony.is_approved());
        assert_eq!(ceremony.status, RecoveryStatus::Approved);
    }

    #[test]
    fn test_recovery_rejection() {
        let mut ceremony = RecoveryCeremony::new(
            Some("anchor_123".to_string()),
            None,
            serde_json::json!({}),
            test_keybundle(),
        );

        ceremony.reject("Invalid verification proof".to_string());
        assert_eq!(ceremony.status, RecoveryStatus::Rejected);
        assert_eq!(
            ceremony.rejection_reason,
            Some("Invalid verification proof".to_string())
        );
    }

    #[test]
    fn test_recovery_completion() {
        let mut ceremony = RecoveryCeremony::new(
            Some("anchor_123".to_string()),
            None,
            serde_json::json!({}),
            test_keybundle(),
        );

        ceremony.approve_by_steward();
        ceremony.approve_by_steward();
        ceremony.approve_by_steward();
        assert_eq!(ceremony.status, RecoveryStatus::Approved);

        ceremony.complete();
        assert_eq!(ceremony.status, RecoveryStatus::Completed);
    }

    #[test]
    fn test_validation_requires_identifier() {
        let req = StartRecoveryRequest {
            anchor_id: None,
            vui_hint: None,
            verification_data: serde_json::json!({}),
            new_keybundle: test_keybundle(),
        };

        assert!(validate_recovery_request(&req).is_err());
    }

    #[test]
    fn test_validation_accepts_anchor_id() {
        let req = StartRecoveryRequest {
            anchor_id: Some("anchor_123".to_string()),
            vui_hint: None,
            verification_data: serde_json::json!({}),
            new_keybundle: test_keybundle(),
        };

        assert!(validate_recovery_request(&req).is_ok());
    }

    #[test]
    fn test_validation_accepts_vui_hint() {
        let req = StartRecoveryRequest {
            anchor_id: None,
            vui_hint: Some("partial_vui".to_string()),
            verification_data: serde_json::json!({}),
            new_keybundle: test_keybundle(),
        };

        assert!(validate_recovery_request(&req).is_ok());
    }
}
