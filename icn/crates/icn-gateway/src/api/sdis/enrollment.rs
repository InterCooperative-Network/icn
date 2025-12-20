//! SDIS Enrollment API
//!
//! Endpoints for enrolling new identities into the SDIS system.
//! Enrollment involves creating a permanent Anchor that can survive key rotation.
//!
//! # Flow
//!
//! 1. Client generates keypair locally
//! 2. Client submits enrollment request with pathway proof
//! 3. Stewards verify the proof
//! 4. Once threshold reached, Anchor is created
//! 5. Client receives Anchor ID and initial DID

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{GatewayError, Result};

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to start an enrollment ceremony
#[derive(Debug, Deserialize)]
pub struct StartEnrollmentRequest {
    /// How the person will be verified
    pub pathway: EnrollmentPathwayDto,
    /// Pathway-specific proof data
    pub proof_data: serde_json::Value,
    /// Client-generated initial keybundle (public keys only)
    pub initial_keybundle: KeyBundleDto,
}

/// Enrollment pathway DTO
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnrollmentPathwayDto {
    GovernmentId {
        country: String,
        doc_type: String,
        doc_hash: String,
    },
    OrganizationalSponsorship {
        org_did: String,
        org_internal_id: String,
    },
    BiometricCommitment {
        template_hash: String,
    },
    WebOfTrust {
        vouchers: Vec<String>,
    },
    Genesis {
        reason: String,
    },
}

/// KeyBundle DTO (public keys only)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeyBundleDto {
    /// Ed25519 public key (base58)
    pub ed25519_pub: String,
    /// ML-DSA public key (base64)
    pub ml_dsa_pub: String,
    /// X25519 public key (base58)
    pub x25519_pub: String,
}

/// Response after starting enrollment
#[derive(Debug, Serialize)]
pub struct StartEnrollmentResponse {
    /// Unique ceremony ID
    pub ceremony_id: String,
    /// Current status
    pub status: CeremonyStatus,
    /// Number of steward approvals required
    pub required_stewards: u32,
    /// Estimated completion time
    pub estimated_completion: Option<String>,
}

/// Ceremony status
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CeremonyStatus {
    PendingStewardVerification,
    Approved,
    Rejected,
    Expired,
}

/// Response for ceremony status query
#[derive(Debug, Serialize)]
pub struct EnrollmentStatusResponse {
    /// Ceremony ID
    pub ceremony_id: String,
    /// Current status
    pub status: CeremonyStatus,
    /// Number of steward approvals received
    pub steward_approvals: u32,
    /// Number of steward approvals required
    pub required_stewards: u32,
    /// Anchor data (only present when approved)
    pub anchor: Option<AnchorDto>,
    /// Reason for rejection (if rejected)
    pub rejection_reason: Option<String>,
}

/// Anchor DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorDto {
    /// Anchor ID (32 bytes, hex-encoded)
    pub anchor_id: String,
    /// Unix timestamp of creation
    pub created_at: u64,
    /// Enrollment pathway
    pub pathway: EnrollmentPathwayDto,
}

/// Request to finalize enrollment
#[derive(Debug, Deserialize)]
pub struct FinalizeEnrollmentRequest {
    /// VUI commitment (if applicable)
    pub vui_commitment: Option<String>,
}

/// Response after finalizing enrollment
#[derive(Debug, Serialize)]
pub struct FinalizeEnrollmentResponse {
    /// Permanent anchor ID
    pub anchor_id: String,
    /// Initial DID
    pub did: String,
    /// KeyBundle version
    pub keybundle_version: u32,
    /// Recovery codes for future key rotation
    pub recovery_codes: Vec<String>,
}

// ============================================================================
// In-Memory Ceremony Storage
// NOTE: Persistent storage is available via CommonsStore.put_ceremony()
// This API is currently disabled; simple_enrollment.rs uses CommonsManager
// for persistent storage.
// ============================================================================

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// In-memory storage for enrollment ceremonies
pub struct EnrollmentStore {
    ceremonies: RwLock<HashMap<String, EnrollmentCeremony>>,
}

impl Default for EnrollmentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EnrollmentStore {
    pub fn new() -> Self {
        Self {
            ceremonies: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_ceremony(&self, ceremony: EnrollmentCeremony) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let mut ceremonies = self
            .ceremonies
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire write lock".to_string()))?;
        ceremonies.insert(id.clone(), ceremony);
        Ok(id)
    }

    pub fn get_ceremony(&self, id: &str) -> Result<Option<EnrollmentCeremony>> {
        let ceremonies = self
            .ceremonies
            .read()
            .map_err(|_| GatewayError::InternalError("Failed to acquire read lock".to_string()))?;
        Ok(ceremonies.get(id).cloned())
    }

    pub fn update_ceremony(&self, id: &str, ceremony: EnrollmentCeremony) -> Result<()> {
        let mut ceremonies = self
            .ceremonies
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire write lock".to_string()))?;
        ceremonies.insert(id.to_string(), ceremony);
        Ok(())
    }
}

/// Enrollment ceremony state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentCeremony {
    pub pathway: EnrollmentPathwayDto,
    pub proof_data: serde_json::Value,
    pub initial_keybundle: KeyBundleDto,
    pub status: CeremonyStatus,
    pub steward_approvals: u32,
    pub required_stewards: u32,
    pub anchor: Option<AnchorDto>,
    pub created_at: u64,
    pub rejection_reason: Option<String>,
}

impl EnrollmentCeremony {
    pub fn new(
        pathway: EnrollmentPathwayDto,
        proof_data: serde_json::Value,
        initial_keybundle: KeyBundleDto,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            pathway,
            proof_data,
            initial_keybundle,
            status: CeremonyStatus::PendingStewardVerification,
            steward_approvals: 0,
            required_stewards: 3, // Default threshold
            anchor: None,
            created_at: now,
            rejection_reason: None,
        }
    }

    pub fn is_approved(&self) -> bool {
        self.steward_approvals >= self.required_stewards
    }

    pub fn approve_by_steward(&mut self) {
        self.steward_approvals += 1;
        if self.is_approved() {
            self.status = CeremonyStatus::Approved;
        }
    }

    pub fn reject(&mut self, reason: String) {
        self.status = CeremonyStatus::Rejected;
        self.rejection_reason = Some(reason);
    }
}

// ============================================================================
// API Handlers
// ============================================================================

/// Start a new enrollment ceremony
///
/// POST /v1/sdis/enrollment/start
#[post("/start")]
pub async fn start_enrollment(
    store: web::Data<Arc<EnrollmentStore>>,
    req: web::Json<StartEnrollmentRequest>,
) -> Result<HttpResponse> {
    // Validate request
    validate_enrollment_request(&req)?;

    // Create ceremony
    let ceremony = EnrollmentCeremony::new(
        req.pathway.clone(),
        req.proof_data.clone(),
        req.initial_keybundle.clone(),
    );

    let ceremony_id = store.create_ceremony(ceremony.clone())?;

    // In a real implementation, this would:
    // 1. Submit to steward network via gossip
    // 2. Stewards would verify the proof_data
    // 3. Stewards would vote on approval
    // For now, we'll auto-approve after a simulated delay

    let response = StartEnrollmentResponse {
        ceremony_id,
        status: ceremony.status.clone(),
        required_stewards: ceremony.required_stewards,
        estimated_completion: Some("2025-12-13T12:00:00Z".to_string()),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Get enrollment ceremony status
///
/// GET /v1/sdis/enrollment/{ceremony_id}
#[get("/{ceremony_id}")]
pub async fn get_enrollment_status(
    store: web::Data<Arc<EnrollmentStore>>,
    ceremony_id: web::Path<String>,
) -> Result<HttpResponse> {
    let ceremony = store
        .get_ceremony(&ceremony_id)?
        .ok_or_else(|| GatewayError::NotFound("Ceremony not found".to_string()))?;

    let response = EnrollmentStatusResponse {
        ceremony_id: ceremony_id.to_string(),
        status: ceremony.status.clone(),
        steward_approvals: ceremony.steward_approvals,
        required_stewards: ceremony.required_stewards,
        anchor: ceremony.anchor.clone(),
        rejection_reason: ceremony.rejection_reason.clone(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Finalize enrollment and receive anchor
///
/// POST /v1/sdis/enrollment/{ceremony_id}/finalize
#[post("/{ceremony_id}/finalize")]
pub async fn finalize_enrollment(
    store: web::Data<Arc<EnrollmentStore>>,
    ceremony_id: web::Path<String>,
    _req: web::Json<FinalizeEnrollmentRequest>,
) -> Result<HttpResponse> {
    let mut ceremony = store
        .get_ceremony(&ceremony_id)?
        .ok_or_else(|| GatewayError::NotFound("Ceremony not found".to_string()))?;

    // Check if approved
    if ceremony.status != CeremonyStatus::Approved {
        return Err(GatewayError::BadRequest(
            "Ceremony not approved yet".to_string(),
        ));
    }

    // Generate anchor if not already created
    if ceremony.anchor.is_none() {
        let anchor = generate_anchor(&ceremony)?;
        ceremony.anchor = Some(anchor);
        store.update_ceremony(&ceremony_id, ceremony.clone())?;
    }

    let anchor = ceremony.anchor.as_ref().unwrap();

    // Generate DID from anchor
    let did = format!("did:icn:{}", anchor.anchor_id);

    // Generate recovery codes
    let recovery_codes = generate_recovery_codes();

    let response = FinalizeEnrollmentResponse {
        anchor_id: anchor.anchor_id.clone(),
        did,
        keybundle_version: 1,
        recovery_codes,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Simulate steward approval (for testing/development only)
///
/// POST /v1/sdis/enrollment/{ceremony_id}/approve
///
/// **WARNING**: This endpoint should NOT be enabled in production.
/// Set environment variable `ICN_ENABLE_ADMIN_ENDPOINTS=false` to disable.
#[post("/{ceremony_id}/approve")]
pub async fn approve_ceremony(
    store: web::Data<Arc<EnrollmentStore>>,
    ceremony_id: web::Path<String>,
) -> Result<HttpResponse> {
    // Production guard: check environment variable
    let admin_enabled = std::env::var("ICN_ENABLE_ADMIN_ENDPOINTS")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase()
        == "true";

    if !admin_enabled {
        return Err(GatewayError::Forbidden(
            "Admin endpoints are disabled in production".to_string(),
        ));
    }

    let mut ceremony = store
        .get_ceremony(&ceremony_id)?
        .ok_or_else(|| GatewayError::NotFound("Ceremony not found".to_string()))?;

    ceremony.approve_by_steward();
    store.update_ceremony(&ceremony_id, ceremony.clone())?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "approved",
        "approvals": ceremony.steward_approvals,
        "required": ceremony.required_stewards
    })))
}

// ============================================================================
// Helper Functions
// ============================================================================

fn validate_enrollment_request(req: &StartEnrollmentRequest) -> Result<()> {
    // Validate pathway-specific data
    match &req.pathway {
        EnrollmentPathwayDto::GovernmentId { country, .. } => {
            if country.len() != 2 {
                return Err(GatewayError::BadRequest(
                    "Country code must be 2 characters".to_string(),
                ));
            }
        }
        EnrollmentPathwayDto::WebOfTrust { vouchers } => {
            if vouchers.len() < 3 {
                return Err(GatewayError::BadRequest(
                    "Web of trust requires at least 3 vouchers".to_string(),
                ));
            }
        }
        _ => {}
    }

    // Validate keybundle
    if req.initial_keybundle.ed25519_pub.is_empty()
        || req.initial_keybundle.ml_dsa_pub.is_empty()
        || req.initial_keybundle.x25519_pub.is_empty()
    {
        return Err(GatewayError::BadRequest(
            "All public keys must be provided".to_string(),
        ));
    }

    Ok(())
}

fn generate_anchor(ceremony: &EnrollmentCeremony) -> Result<AnchorDto> {
    // Generate a unique anchor ID
    // In production, this would use proper VUI computation with steward threshold
    let anchor_id = Uuid::new_v4().to_string().replace("-", "");

    Ok(AnchorDto {
        anchor_id,
        created_at: ceremony.created_at,
        pathway: ceremony.pathway.clone(),
    })
}

fn generate_recovery_codes() -> Vec<String> {
    // Generate 12 recovery codes
    // In production, these would be derived from steward shares
    (0..12)
        .map(|i| {
            format!(
                "RECOVERY-{}-{}",
                i + 1,
                Uuid::new_v4().to_string()[..8].to_uppercase()
            )
        })
        .collect()
}

// ============================================================================
// Configuration Function
// ============================================================================

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/enrollment")
            .service(start_enrollment)
            .service(get_enrollment_status)
            .service(finalize_enrollment)
            .service(approve_ceremony), // Guarded by ICN_ENABLE_ADMIN_ENDPOINTS env var
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ceremony_creation() {
        let pathway = EnrollmentPathwayDto::Genesis {
            reason: "Test".to_string(),
        };
        let keybundle = KeyBundleDto {
            ed25519_pub: "test_ed25519".to_string(),
            ml_dsa_pub: "test_ml_dsa".to_string(),
            x25519_pub: "test_x25519".to_string(),
        };

        let ceremony = EnrollmentCeremony::new(pathway, serde_json::json!({}), keybundle);

        assert_eq!(ceremony.status, CeremonyStatus::PendingStewardVerification);
        assert_eq!(ceremony.steward_approvals, 0);
        assert_eq!(ceremony.required_stewards, 3);
    }

    #[test]
    fn test_ceremony_approval() {
        let pathway = EnrollmentPathwayDto::Genesis {
            reason: "Test".to_string(),
        };
        let keybundle = KeyBundleDto {
            ed25519_pub: "test_ed25519".to_string(),
            ml_dsa_pub: "test_ml_dsa".to_string(),
            x25519_pub: "test_x25519".to_string(),
        };

        let mut ceremony = EnrollmentCeremony::new(pathway, serde_json::json!({}), keybundle);

        assert!(!ceremony.is_approved());

        ceremony.approve_by_steward();
        assert_eq!(ceremony.steward_approvals, 1);
        assert!(!ceremony.is_approved());

        ceremony.approve_by_steward();
        assert_eq!(ceremony.steward_approvals, 2);
        assert!(!ceremony.is_approved());

        ceremony.approve_by_steward();
        assert_eq!(ceremony.steward_approvals, 3);
        assert!(ceremony.is_approved());
        assert_eq!(ceremony.status, CeremonyStatus::Approved);
    }

    #[test]
    fn test_ceremony_rejection() {
        let pathway = EnrollmentPathwayDto::Genesis {
            reason: "Test".to_string(),
        };
        let keybundle = KeyBundleDto {
            ed25519_pub: "test_ed25519".to_string(),
            ml_dsa_pub: "test_ml_dsa".to_string(),
            x25519_pub: "test_x25519".to_string(),
        };

        let mut ceremony = EnrollmentCeremony::new(pathway, serde_json::json!({}), keybundle);

        ceremony.reject("Invalid documentation".to_string());
        assert_eq!(ceremony.status, CeremonyStatus::Rejected);
        assert_eq!(
            ceremony.rejection_reason,
            Some("Invalid documentation".to_string())
        );
    }
}
