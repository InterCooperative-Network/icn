//! Device management API endpoints
//!
//! Endpoints for registering, listing, and revoking devices associated with a DID.
//! These endpoints enable multi-device support for mobile apps.

use actix_web::{delete, get, post, web, HttpMessage, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::TokenClaims;
use crate::error::{GatewayError, Result};
use crate::identity_mgr::{DeviceInfo, IdentityManager, RegisterDeviceRequest};

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to register a new device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRegisterDeviceRequest {
    /// Unique device identifier (e.g., "phone-1", "laptop-work")
    pub device_id: String,

    /// Human-readable label
    pub label: String,

    /// Ed25519 public key (hex-encoded)
    pub public_key: String,

    /// Optional X25519 encryption public key (hex-encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_public_key: Option<String>,

    /// Capabilities to grant: "sign", "add_device", "revoke_device", "rotate_key", "encrypt"
    pub capabilities: Vec<String>,

    /// Device ID of the device signing this request (must have AddDevice capability)
    pub signing_device_id: String,

    /// Hex-encoded signature of the registration request
    pub signature: String,
}

/// Request to revoke a device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRevokeDeviceRequest {
    /// Device ID of the device signing this request (must have RevokeDevice capability)
    pub signing_device_id: String,

    /// Hex-encoded signature
    pub signature: String,
}

/// Response for device registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRegisterDeviceResponse {
    pub device: DeviceInfo,
    pub message: String,
}

/// Response for device list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiListDevicesResponse {
    pub devices: Vec<DeviceInfo>,
    pub total: usize,
}

// ============================================================================
// API Endpoints
// ============================================================================

/// POST /v1/devices/{did} - Register a new device
///
/// Registers a new device for the authenticated user's DID.
/// The request must be signed by an existing device with AddDevice capability.
#[post("/{did}")]
pub async fn register_device(
    req: HttpRequest,
    identity_mgr: web::Data<Arc<IdentityManager>>,
    path: web::Path<String>,
    body: web::Json<ApiRegisterDeviceRequest>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();

    // Get authenticated user's DID from token
    let claims = req.extensions().get::<TokenClaims>().cloned().ok_or_else(|| {
        GatewayError::AuthenticationFailed("Missing authentication token".to_string())
    })?;

    // Verify the DID in path matches the authenticated user
    if claims.sub != did_str {
        return Err(GatewayError::AuthorizationFailed(
            "Cannot manage devices for another identity".to_string(),
        ));
    }

    // Parse the DID
    let did: icn_identity::Did = did_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID format: {e}")))?;

    // Decode signature
    let signature = hex::decode(&body.signature)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature hex: {e}")))?;

    // Create internal request
    let internal_request = RegisterDeviceRequest {
        device_id: body.device_id.clone(),
        label: body.label.clone(),
        public_key: body.public_key.clone(),
        encryption_public_key: body.encryption_public_key.clone(),
        capabilities: body.capabilities.clone(),
    };

    // Register the device
    let device = identity_mgr
        .register_device(&did, &internal_request, &body.signing_device_id, &signature)
        .await?;

    Ok(HttpResponse::Created().json(ApiRegisterDeviceResponse {
        device,
        message: "Device registered successfully".to_string(),
    }))
}

/// GET /v1/devices/{did} - List all devices
///
/// Lists all devices registered for the authenticated user's DID.
#[get("/{did}")]
pub async fn list_devices(
    req: HttpRequest,
    identity_mgr: web::Data<Arc<IdentityManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();

    // Get authenticated user's DID from token
    let claims = req.extensions().get::<TokenClaims>().cloned().ok_or_else(|| {
        GatewayError::AuthenticationFailed("Missing authentication token".to_string())
    })?;

    // Verify the DID in path matches the authenticated user
    if claims.sub != did_str {
        return Err(GatewayError::AuthorizationFailed(
            "Cannot view devices for another identity".to_string(),
        ));
    }

    // Parse the DID
    let did: icn_identity::Did = did_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID format: {e}")))?;

    // List devices
    let devices = identity_mgr.list_devices(&did).await?;
    let total = devices.len();

    Ok(HttpResponse::Ok().json(ApiListDevicesResponse { devices, total }))
}

/// DELETE /v1/devices/{did}/{device_id} - Revoke a device
///
/// Revokes a device for the authenticated user's DID.
/// The request must be signed by a device with RevokeDevice capability.
#[delete("/{did}/{device_id}")]
pub async fn revoke_device(
    req: HttpRequest,
    identity_mgr: web::Data<Arc<IdentityManager>>,
    path: web::Path<(String, String)>,
    body: web::Json<ApiRevokeDeviceRequest>,
) -> Result<HttpResponse> {
    let (did_str, device_id) = path.into_inner();

    // Get authenticated user's DID from token
    let claims = req.extensions().get::<TokenClaims>().cloned().ok_or_else(|| {
        GatewayError::AuthenticationFailed("Missing authentication token".to_string())
    })?;

    // Verify the DID in path matches the authenticated user
    if claims.sub != did_str {
        return Err(GatewayError::AuthorizationFailed(
            "Cannot manage devices for another identity".to_string(),
        ));
    }

    // Parse the DID
    let did: icn_identity::Did = did_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID format: {e}")))?;

    // Decode signature
    let signature = hex::decode(&body.signature)
        .map_err(|e| GatewayError::BadRequest(format!("Invalid signature hex: {e}")))?;

    // Revoke the device
    identity_mgr
        .revoke_device(&did, &device_id, &body.signing_device_id, &signature)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Device revoked successfully",
        "device_id": device_id
    })))
}

/// GET /v1/devices/{did}/{device_id} - Get a specific device
///
/// Gets details for a specific device.
#[get("/{did}/{device_id}")]
pub async fn get_device(
    req: HttpRequest,
    identity_mgr: web::Data<Arc<IdentityManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (did_str, device_id) = path.into_inner();

    // Get authenticated user's DID from token
    let claims = req.extensions().get::<TokenClaims>().cloned().ok_or_else(|| {
        GatewayError::AuthenticationFailed("Missing authentication token".to_string())
    })?;

    // Verify the DID in path matches the authenticated user
    if claims.sub != did_str {
        return Err(GatewayError::AuthorizationFailed(
            "Cannot view devices for another identity".to_string(),
        ));
    }

    // Parse the DID
    let did: icn_identity::Did = did_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID format: {e}")))?;

    // List devices and find the specific one
    let devices = identity_mgr.list_devices(&did).await?;
    let device = devices
        .into_iter()
        .find(|d| d.id == device_id)
        .ok_or_else(|| GatewayError::NotFound(format!("Device not found: {device_id}")))?;

    Ok(HttpResponse::Ok().json(device))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use actix_web_httpauth::middleware::HttpAuthentication;
    use icn_identity::KeyPair;

    fn test_keypair() -> KeyPair {
        KeyPair::generate().unwrap()
    }

    fn test_identity_mgr() -> Arc<IdentityManager> {
        Arc::new(IdentityManager::new())
    }

    // Note: More comprehensive tests would require setting up full auth middleware
    // These tests verify the basic endpoint structure

    #[actix_web::test]
    async fn test_list_devices_unauthorized() {
        let identity_mgr = test_identity_mgr();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(identity_mgr))
                .service(web::scope("/v1/devices").service(list_devices)),
        )
        .await;

        let kp = test_keypair();
        let req = test::TestRequest::get()
            .uri(&format!("/v1/devices/{}", kp.did()))
            .to_request();

        let resp = test::call_service(&app, req).await;
        // Without auth middleware, we expect Unauthorized
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }
}
