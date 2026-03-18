//! SDIS API Endpoints
//!
//! Sovereign Digital Identity System API for credential presentation.
//!
//! # Endpoints
//!
//! ## Enrollment
//! - `POST /v1/sdis/enrollment/start` - Start enrollment ceremony
//! - `GET /v1/sdis/enrollment/{id}` - Get ceremony status
//! - `POST /v1/sdis/enrollment/{id}/finalize` - Finalize enrollment
//!
//! ## Recovery
//! - `POST /v1/sdis/recovery/start` - Start recovery ceremony
//! - `GET /v1/sdis/recovery/{id}` - Get recovery status
//! - `POST /v1/sdis/recovery/{id}/complete` - Complete recovery
//!
//! ## Anchor Management
//! - `GET /v1/sdis/anchor/{id}` - Get anchor details
//! - `POST /v1/sdis/anchor/rotate-keys` - Rotate keys
//! - `GET /v1/sdis/anchor/{id}/history` - Get rotation history
//! - `POST /v1/sdis/anchor/devices/add` - Add device
//! - `GET /v1/sdis/anchor/{id}/devices` - List devices
//!
//! ## Ephemeral Proofs
//! - `POST /v1/sdis/ephemeral/generate` - Generate ephemeral proof
//! - `POST /v1/sdis/ephemeral/refresh` - Refresh existing proof
//!
//! ## Verification
//! - `POST /v1/sdis/verify/level1` - QR scan verification (no network)
//! - `POST /v1/sdis/verify/level2` - Binding verification (hybrid)
//! - `POST /v1/sdis/verify/level3` - Full STARK verification (network)
//!
//! ## Binding
//! - `GET /v1/sdis/binding/{session}` - Get binding for session
//!
//! ## Health
//! - `GET /v1/sdis/health` - Service health check

pub mod anchor;
pub mod enrollment;
pub mod ephemeral;
pub mod qr;
pub mod recovery;
pub mod simple_enrollment;
pub mod verify;

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
pub use ephemeral::{Channel, EphemeralBinding, EphemeralProof, VerifyResult};
pub use qr::{decode_from_qr, encode_for_qr, QrEstimate};
pub use verify::EphemeralVerifier;

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to generate an ephemeral proof
#[derive(Debug, Deserialize, ToSchema)]
pub struct GenerateEphemeralRequest {
    /// Type of proof to generate
    pub proof_type: String,
    /// Proof-specific parameters
    #[serde(default)]
    pub params: serde_json::Value,
    /// Validity duration in seconds (default: 3600)
    #[serde(default = "default_validity")]
    pub validity_secs: u64,
    /// Available upgrade channels
    #[serde(default)]
    pub channels: Vec<String>,
}

fn default_validity() -> u64 {
    3600
}

/// Response for ephemeral proof generation
#[derive(Debug, Serialize, ToSchema)]
pub struct GenerateEphemeralResponse {
    /// Base64-encoded QR data
    pub qr_data: String,
    /// Expiry timestamp
    pub expires_at: u64,
    /// Session ID for binding retrieval
    pub session_id: String,
    /// QR code size estimate
    pub qr_estimate: QrEstimateResponse,
}

/// QR estimate in response
#[derive(Debug, Serialize, ToSchema)]
pub struct QrEstimateResponse {
    pub data_size: usize,
    pub qr_version: u8,
    pub modules: u16,
}

impl From<QrEstimate> for QrEstimateResponse {
    fn from(e: QrEstimate) -> Self {
        Self {
            data_size: e.data_size,
            qr_version: e.qr_version,
            modules: e.modules,
        }
    }
}

/// Request for Level 1 verification
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyLevel1Request {
    /// Base64-encoded QR data
    pub qr_data: String,
}

/// Request for Level 2 verification
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyLevel2Request {
    /// Base64-encoded QR data
    pub qr_data: String,
    /// Base64-encoded binding data (optional if cached)
    #[serde(default)]
    pub binding: Option<String>,
}

/// Verification response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyResponse {
    pub valid: bool,
    pub level: u8,
    pub proof_type: Option<String>,
    pub warnings: Vec<String>,
    pub verified_at: u64,
}

impl From<VerifyResult> for VerifyResponse {
    fn from(r: VerifyResult) -> Self {
        Self {
            valid: r.valid,
            level: r.level,
            proof_type: r.proof_type.map(|pt| format!("{pt:?}")),
            warnings: r.warnings,
            verified_at: r.verified_at,
        }
    }
}

/// SDIS service state
pub struct SdisState {
    pub verifier: EphemeralVerifier,
}

impl Default for SdisState {
    fn default() -> Self {
        Self::new()
    }
}

impl SdisState {
    pub fn new() -> Self {
        Self {
            verifier: EphemeralVerifier::new(),
        }
    }
}

// ============================================================================
// Health Endpoint
// ============================================================================

/// GET /v1/sdis/health - Health check
#[get("/health")]
pub async fn sdis_health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "service": "sdis",
        "version": "1.0"
    }))
}

// ============================================================================
// Verification Endpoints
// ============================================================================

/// POST /v1/sdis/verify/level1 - Level 1 (QR only) verification
#[post("/verify/level1")]
pub async fn verify_level1(
    state: web::Data<Arc<SdisState>>,
    req: web::Json<VerifyLevel1Request>,
) -> Result<HttpResponse> {
    // Decode QR data
    let qr_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.qr_data)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid base64: {e}")))?;

    // Decode proof
    let proof = decode_from_qr(&qr_bytes)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid QR data: {e}")))?;

    // Verify
    let result = state.verifier.verify_level1(&proof);

    Ok(HttpResponse::Ok().json(VerifyResponse::from(result)))
}

/// POST /v1/sdis/verify/level2 - Level 2 (with binding) verification
#[post("/verify/level2")]
pub async fn verify_level2(
    state: web::Data<Arc<SdisState>>,
    req: web::Json<VerifyLevel2Request>,
) -> Result<HttpResponse> {
    // Decode QR data
    let qr_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.qr_data)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid base64: {e}")))?;

    let proof = decode_from_qr(&qr_bytes)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid QR data: {e}")))?;

    // Get binding
    let result = if let Some(binding_b64) = &req.binding {
        let binding_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, binding_b64)
                .map_err(|e| GatewayError::BadRequest(format!("Invalid binding base64: {e}")))?;

        let binding: EphemeralBinding = icn_encoding::decode(&binding_bytes)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid binding: {e}")))?;

        state.verifier.verify_level2(&proof, &binding)
    } else {
        // Try cached binding
        state.verifier.verify_level2_cached(&proof)
    };

    Ok(HttpResponse::Ok().json(VerifyResponse::from(result)))
}

// ============================================================================
// Ephemeral Proof Generation Endpoints
// ============================================================================

/// Request to generate an ephemeral proof
#[derive(Debug, Deserialize, ToSchema)]
pub struct GenerateProofRequest {
    /// Type of proof to generate
    pub proof_type: ProofTypeRequest,
    /// Validity duration in seconds (default: 3600)
    #[serde(default = "default_validity")]
    pub validity_secs: u64,
    /// Available upgrade channels
    #[serde(default)]
    pub channels: Vec<String>,
}

/// Proof type from client request
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProofTypeRequest {
    Age {
        threshold: u8,
    },
    Citizenship {
        country_code: String,
    },
    #[serde(rename = "non_revocation")]
    NonRevocation,
}

impl From<ProofTypeRequest> for icn_zkp::ProofType {
    fn from(req: ProofTypeRequest) -> Self {
        match req {
            ProofTypeRequest::Age { threshold } => icn_zkp::ProofType::AgeAtLeast { threshold },
            ProofTypeRequest::Citizenship { country_code } => {
                // Convert country code string to 2-byte array
                let mut code = [0u8; 2];
                let bytes = country_code.as_bytes();
                if bytes.len() >= 2 {
                    code[0] = bytes[0];
                    code[1] = bytes[1];
                }
                icn_zkp::ProofType::Citizenship { country_code: code }
            }
            ProofTypeRequest::NonRevocation => icn_zkp::ProofType::NonRevocation,
        }
    }
}

/// Response for ephemeral proof generation
#[derive(Debug, Serialize, ToSchema)]
pub struct GenerateProofResponse {
    /// Base64-encoded QR data
    pub qr_data: String,
    /// Expiry timestamp (unix seconds)
    pub expires_at: u64,
    /// What was proved
    pub proof_type: String,
}

/// POST /v1/sdis/ephemeral/generate - Generate an ephemeral proof
#[post("/ephemeral/generate")]
pub async fn generate_ephemeral(req: web::Json<GenerateProofRequest>) -> Result<HttpResponse> {
    use base64::Engine;
    use icn_identity::{Anchor, KeyBundle};
    use std::time::Duration;

    // For demo purposes, create a temporary keybundle
    // In production, this would use the authenticated user's keys
    let anchor = Anchor::genesis("demo-user");
    let keybundle = KeyBundle::generate(anchor, 1)
        .map_err(|e| GatewayError::InternalError(format!("Failed to generate keys: {e}")))?;

    // Parse channels
    let channels: Vec<Channel> = req
        .channels
        .iter()
        .filter_map(|c| match c.to_lowercase().as_str() {
            "nfc" => Some(Channel::Nfc),
            "ble" => Some(Channel::Ble),
            "wifi" | "wifi_direct" => Some(Channel::WifiDirect),
            "http" => Some(Channel::Http),
            _ => None,
        })
        .collect();

    // Generate proof
    let proof_type: icn_zkp::ProofType = req.proof_type.clone().into();
    let validity = Duration::from_secs(req.validity_secs.min(86400)); // Max 24 hours

    let (proof, _binding) =
        EphemeralProof::generate(&keybundle, proof_type.clone(), validity, channels)
            .map_err(|e| GatewayError::InternalError(format!("Failed to generate proof: {e}")))?;

    // Encode for QR
    let qr_bytes = encode_for_qr(&proof)
        .map_err(|e| GatewayError::InternalError(format!("Failed to encode proof: {e}")))?;
    let qr_data = base64::engine::general_purpose::STANDARD.encode(&qr_bytes);

    Ok(HttpResponse::Ok().json(GenerateProofResponse {
        qr_data,
        expires_at: proof.x,
        proof_type: format!("{proof_type:?}"),
    }))
}

// ============================================================================
// Configuration
// ============================================================================

/// Configure SDIS routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/sdis")
            .service(sdis_health)
            .service(verify_level1)
            .service(verify_level2)
            .service(generate_ephemeral)
            .configure(simple_enrollment::configure),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use base64::Engine;
    use icn_identity::{Anchor, KeyBundle};
    use icn_zkp::ProofType;
    use std::time::Duration;

    fn test_keybundle() -> KeyBundle {
        let anchor = Anchor::genesis("test");
        KeyBundle::generate(anchor, 1).unwrap()
    }

    #[actix_web::test]
    async fn test_sdis_health() {
        let state = Arc::new(SdisState::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(web::scope("/v1/sdis").service(sdis_health)),
        )
        .await;

        let req = test::TestRequest::get().uri("/v1/sdis/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "sdis");
    }

    #[actix_web::test]
    async fn test_verify_level1_valid() {
        let state = Arc::new(SdisState::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(web::scope("/v1/sdis").service(verify_level1)),
        )
        .await;

        // Generate a valid proof
        let keybundle = test_keybundle();
        let (proof, _) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 18 },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let qr_data = encode_for_qr(&proof).unwrap();
        let qr_b64 = base64::engine::general_purpose::STANDARD.encode(&qr_data);

        let req = test::TestRequest::post()
            .uri("/v1/sdis/verify/level1")
            .set_json(VerifyLevel1Request { qr_data: qr_b64 })
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: VerifyResponse = test::read_body_json(resp).await;
        assert!(body.valid);
        assert_eq!(body.level, 1);
    }

    #[actix_web::test]
    async fn test_verify_level1_invalid_base64() {
        let state = Arc::new(SdisState::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(web::scope("/v1/sdis").service(verify_level1)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/v1/sdis/verify/level1")
            .set_json(VerifyLevel1Request {
                qr_data: "not-valid-base64!!!".to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_verify_level2_with_binding() {
        let state = Arc::new(SdisState::new());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(web::scope("/v1/sdis").service(verify_level2)),
        )
        .await;

        // Generate proof and binding
        let keybundle = test_keybundle();
        let (proof, binding) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 21 },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let qr_data = encode_for_qr(&proof).unwrap();
        let qr_b64 = base64::engine::general_purpose::STANDARD.encode(&qr_data);

        let binding_bytes = icn_encoding::encode(&binding).unwrap();
        let binding_b64 = base64::engine::general_purpose::STANDARD.encode(&binding_bytes);

        let req = test::TestRequest::post()
            .uri("/v1/sdis/verify/level2")
            .set_json(VerifyLevel2Request {
                qr_data: qr_b64,
                binding: Some(binding_b64),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: VerifyResponse = test::read_body_json(resp).await;
        assert!(body.valid);
        assert_eq!(body.level, 2);
    }

    #[actix_web::test]
    async fn test_generate_ephemeral() {
        let app = test::init_service(
            App::new().service(web::scope("/v1/sdis").service(generate_ephemeral)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/v1/sdis/ephemeral/generate")
            .set_json(serde_json::json!({
                "proof_type": { "type": "age", "threshold": 18 },
                "validity_secs": 3600,
                "channels": []
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(
            resp.status().is_success(),
            "Expected 200, got {}",
            resp.status()
        );
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(body["qr_data"].as_str().is_some());
        assert!(body["expires_at"].as_u64().is_some());
    }
}
