//! SDIS Anchor Management API
//!
//! Endpoints for managing Anchors and their associated KeyBundles.
//! Anchors are permanent identity roots that survive key rotation.
//!
//! # Endpoints
//!
//! - GET /v1/sdis/anchor/:anchor_id - Get anchor details
//! - POST /v1/sdis/anchor/rotate-keys - Rotate keys while keeping anchor
//! - GET /v1/sdis/anchor/:anchor_id/history - Get key rotation history
//! - POST /v1/sdis/anchor/devices/add - Add a trusted device
//! - GET /v1/sdis/anchor/:anchor_id/devices - List trusted devices

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::enrollment::{AnchorDto, EnrollmentPathwayDto, KeyBundleDto};
use crate::error::{GatewayError, Result};

// ============================================================================
// Request/Response Models
// ============================================================================

/// Response for anchor details
#[derive(Debug, Serialize)]
pub struct AnchorDetailsResponse {
    /// Anchor ID (permanent)
    pub anchor_id: String,
    /// When the anchor was created
    pub created_at: u64,
    /// How the person was verified
    pub pathway: EnrollmentPathwayDto,
    /// Current DID (rotates with keys)
    pub current_did: String,
    /// Current keybundle version
    pub keybundle_version: u32,
    /// List of trusted devices
    pub devices: Vec<DeviceInfo>,
    /// Number of key rotations performed
    pub rotation_count: u32,
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device ID
    pub device_id: String,
    /// Device name (e.g., "iPhone 14", "Desktop")
    pub device_name: String,
    /// When device was added
    pub added_at: u64,
    /// Last seen timestamp
    pub last_seen: u64,
    /// Device public key
    pub device_pubkey: String,
}

/// Request to rotate keys
#[derive(Debug, Deserialize)]
pub struct RotateKeysRequest {
    /// Anchor ID
    pub anchor_id: String,
    /// Current DID (for verification)
    pub current_did: String,
    /// New keybundle
    pub new_keybundle: KeyBundleDto,
    /// Reason for rotation (optional)
    pub reason: Option<String>,
}

/// Response after key rotation
#[derive(Debug, Serialize)]
pub struct RotateKeysResponse {
    /// Anchor ID (unchanged)
    pub anchor_id: String,
    /// New DID with rotated keys
    pub new_did: String,
    /// New keybundle version
    pub keybundle_version: u32,
    /// Old DID (now revoked)
    pub revoked_did: String,
}

/// Key rotation history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationHistoryEntry {
    /// When rotation occurred
    pub rotated_at: u64,
    /// Old DID (revoked)
    pub old_did: String,
    /// New DID (active)
    pub new_did: String,
    /// Old version
    pub old_version: u32,
    /// New version
    pub new_version: u32,
    /// Reason for rotation
    pub reason: Option<String>,
}

/// Response for rotation history
#[derive(Debug, Serialize)]
pub struct RotationHistoryResponse {
    /// Anchor ID
    pub anchor_id: String,
    /// List of rotations (newest first)
    pub history: Vec<RotationHistoryEntry>,
    /// Total rotations
    pub total_count: u32,
}

/// Request to add a device
#[derive(Debug, Deserialize)]
pub struct AddDeviceRequest {
    /// Anchor ID
    pub anchor_id: String,
    /// Device name
    pub device_name: String,
    /// Device public key
    pub device_pubkey: String,
}

/// Response after adding device
#[derive(Debug, Serialize)]
pub struct AddDeviceResponse {
    /// Generated device ID
    pub device_id: String,
    /// Device info
    pub device: DeviceInfo,
}

// ============================================================================
// In-Memory Anchor Storage
// ============================================================================

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// In-memory storage for anchors and their metadata
pub struct AnchorStore {
    anchors: RwLock<HashMap<String, AnchorRecord>>,
}

impl Default for AnchorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AnchorStore {
    pub fn new() -> Self {
        Self {
            anchors: RwLock::new(HashMap::new()),
        }
    }

    pub fn store_anchor(&self, anchor_id: String, record: AnchorRecord) -> Result<()> {
        let mut anchors = self
            .anchors
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire write lock".to_string()))?;
        anchors.insert(anchor_id, record);
        Ok(())
    }

    pub fn get_anchor(&self, anchor_id: &str) -> Result<Option<AnchorRecord>> {
        let anchors = self
            .anchors
            .read()
            .map_err(|_| GatewayError::InternalError("Failed to acquire read lock".to_string()))?;
        Ok(anchors.get(anchor_id).cloned())
    }

    pub fn update_anchor(&self, anchor_id: &str, record: AnchorRecord) -> Result<()> {
        let mut anchors = self
            .anchors
            .write()
            .map_err(|_| GatewayError::InternalError("Failed to acquire write lock".to_string()))?;
        anchors.insert(anchor_id.to_string(), record);
        Ok(())
    }
}

/// Complete anchor record with metadata
#[derive(Debug, Clone)]
pub struct AnchorRecord {
    pub anchor: AnchorDto,
    pub current_did: String,
    pub keybundle_version: u32,
    pub devices: Vec<DeviceInfo>,
    pub rotation_history: Vec<RotationHistoryEntry>,
}

impl AnchorRecord {
    pub fn new(anchor: AnchorDto, initial_did: String) -> Self {
        Self {
            anchor,
            current_did: initial_did,
            keybundle_version: 1,
            devices: Vec::new(),
            rotation_history: Vec::new(),
        }
    }

    pub fn rotate_keys(&mut self, new_did: String, reason: Option<String>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let old_did = self.current_did.clone();
        let old_version = self.keybundle_version;
        let new_version = old_version + 1;

        // Add to history
        self.rotation_history.push(RotationHistoryEntry {
            rotated_at: now,
            old_did,
            new_did: new_did.clone(),
            old_version,
            new_version,
            reason,
        });

        // Update current state
        self.current_did = new_did;
        self.keybundle_version = new_version;
    }

    pub fn add_device(&mut self, device: DeviceInfo) {
        self.devices.push(device);
    }

    pub fn rotation_count(&self) -> u32 {
        self.rotation_history.len() as u32
    }
}

// ============================================================================
// API Handlers
// ============================================================================

/// Get anchor details
///
/// GET /v1/sdis/anchor/{anchor_id}
#[get("/{anchor_id}")]
pub async fn get_anchor(
    store: web::Data<Arc<AnchorStore>>,
    anchor_id: web::Path<String>,
) -> Result<HttpResponse> {
    let record = store
        .get_anchor(&anchor_id)?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    let response = AnchorDetailsResponse {
        anchor_id: anchor_id.to_string(),
        created_at: record.anchor.created_at,
        pathway: record.anchor.pathway.clone(),
        current_did: record.current_did.clone(),
        keybundle_version: record.keybundle_version,
        devices: record.devices.clone(),
        rotation_count: record.rotation_count(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Rotate keys while keeping the same anchor
///
/// POST /v1/sdis/anchor/rotate-keys
#[post("/rotate-keys")]
pub async fn rotate_keys(
    store: web::Data<Arc<AnchorStore>>,
    req: web::Json<RotateKeysRequest>,
) -> Result<HttpResponse> {
    // Get anchor record
    let mut record = store
        .get_anchor(&req.anchor_id)?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    // Verify current DID matches
    if record.current_did != req.current_did {
        return Err(GatewayError::BadRequest(
            "Current DID does not match".to_string(),
        ));
    }

    // Generate new DID
    let new_did = format!("did:icn:{}", Uuid::new_v4().to_string().replace("-", ""));

    // Perform rotation
    let old_did = record.current_did.clone();
    record.rotate_keys(new_did.clone(), req.reason.clone());

    // Save updated record
    store.update_anchor(&req.anchor_id, record.clone())?;

    let response = RotateKeysResponse {
        anchor_id: req.anchor_id.clone(),
        new_did,
        keybundle_version: record.keybundle_version,
        revoked_did: old_did,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Get key rotation history
///
/// GET /v1/sdis/anchor/{anchor_id}/history
#[get("/{anchor_id}/history")]
pub async fn get_rotation_history(
    store: web::Data<Arc<AnchorStore>>,
    anchor_id: web::Path<String>,
) -> Result<HttpResponse> {
    let record = store
        .get_anchor(&anchor_id)?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    let response = RotationHistoryResponse {
        anchor_id: anchor_id.to_string(),
        history: record.rotation_history.clone(),
        total_count: record.rotation_count(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Add a trusted device to an anchor
///
/// POST /v1/sdis/anchor/devices/add
#[post("/devices/add")]
pub async fn add_device(
    store: web::Data<Arc<AnchorStore>>,
    req: web::Json<AddDeviceRequest>,
) -> Result<HttpResponse> {
    // Get anchor record
    let mut record = store
        .get_anchor(&req.anchor_id)?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create device info
    let device_id = Uuid::new_v4().to_string();
    let device = DeviceInfo {
        device_id: device_id.clone(),
        device_name: req.device_name.clone(),
        added_at: now,
        last_seen: now,
        device_pubkey: req.device_pubkey.clone(),
    };

    // Add device
    record.add_device(device.clone());

    // Save updated record
    store.update_anchor(&req.anchor_id, record)?;

    let response = AddDeviceResponse { device_id, device };

    Ok(HttpResponse::Ok().json(response))
}

/// List devices for an anchor
///
/// GET /v1/sdis/anchor/{anchor_id}/devices
#[get("/{anchor_id}/devices")]
pub async fn list_devices(
    store: web::Data<Arc<AnchorStore>>,
    anchor_id: web::Path<String>,
) -> Result<HttpResponse> {
    let record = store
        .get_anchor(&anchor_id)?
        .ok_or_else(|| GatewayError::NotFound("Anchor not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "anchor_id": anchor_id.to_string(),
        "devices": record.devices,
        "device_count": record.devices.len(),
    })))
}

// ============================================================================
// Configuration Function
// ============================================================================

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/anchor")
            .service(get_anchor)
            .service(rotate_keys)
            .service(get_rotation_history)
            .service(add_device)
            .service(list_devices),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_anchor() -> AnchorDto {
        AnchorDto {
            anchor_id: "anchor_test_123".to_string(),
            created_at: 1702425600,
            pathway: EnrollmentPathwayDto::Genesis {
                reason: "Test".to_string(),
            },
        }
    }

    fn test_device() -> DeviceInfo {
        DeviceInfo {
            device_id: "device_1".to_string(),
            device_name: "Test Device".to_string(),
            added_at: 1702425600,
            last_seen: 1702425600,
            device_pubkey: "test_pubkey".to_string(),
        }
    }

    #[test]
    fn test_anchor_record_creation() {
        let anchor = test_anchor();
        let initial_did = "did:icn:test123".to_string();
        let record = AnchorRecord::new(anchor, initial_did.clone());

        assert_eq!(record.current_did, initial_did);
        assert_eq!(record.keybundle_version, 1);
        assert_eq!(record.devices.len(), 0);
        assert_eq!(record.rotation_history.len(), 0);
        assert_eq!(record.rotation_count(), 0);
    }

    #[test]
    fn test_key_rotation() {
        let anchor = test_anchor();
        let initial_did = "did:icn:test123".to_string();
        let mut record = AnchorRecord::new(anchor, initial_did.clone());

        let new_did = "did:icn:test456".to_string();
        record.rotate_keys(new_did.clone(), Some("Security update".to_string()));

        assert_eq!(record.current_did, new_did);
        assert_eq!(record.keybundle_version, 2);
        assert_eq!(record.rotation_history.len(), 1);
        assert_eq!(record.rotation_count(), 1);

        let history = &record.rotation_history[0];
        assert_eq!(history.old_did, initial_did);
        assert_eq!(history.new_did, new_did);
        assert_eq!(history.old_version, 1);
        assert_eq!(history.new_version, 2);
        assert_eq!(history.reason, Some("Security update".to_string()));
    }

    #[test]
    fn test_multiple_rotations() {
        let anchor = test_anchor();
        let mut record = AnchorRecord::new(anchor, "did:icn:v1".to_string());

        record.rotate_keys("did:icn:v2".to_string(), None);
        record.rotate_keys("did:icn:v3".to_string(), None);
        record.rotate_keys("did:icn:v4".to_string(), None);

        assert_eq!(record.current_did, "did:icn:v4");
        assert_eq!(record.keybundle_version, 4);
        assert_eq!(record.rotation_count(), 3);
        assert_eq!(record.rotation_history.len(), 3);
    }

    #[test]
    fn test_add_device() {
        let anchor = test_anchor();
        let mut record = AnchorRecord::new(anchor, "did:icn:test123".to_string());

        let device = test_device();
        record.add_device(device.clone());

        assert_eq!(record.devices.len(), 1);
        assert_eq!(record.devices[0].device_name, device.device_name);
    }

    #[test]
    fn test_multiple_devices() {
        let anchor = test_anchor();
        let mut record = AnchorRecord::new(anchor, "did:icn:test123".to_string());

        let device1 = DeviceInfo {
            device_id: "device_1".to_string(),
            device_name: "iPhone".to_string(),
            added_at: 1702425600,
            last_seen: 1702425600,
            device_pubkey: "key1".to_string(),
        };

        let device2 = DeviceInfo {
            device_id: "device_2".to_string(),
            device_name: "Desktop".to_string(),
            added_at: 1702425700,
            last_seen: 1702425700,
            device_pubkey: "key2".to_string(),
        };

        record.add_device(device1);
        record.add_device(device2);

        assert_eq!(record.devices.len(), 2);
        assert_eq!(record.devices[0].device_name, "iPhone");
        assert_eq!(record.devices[1].device_name, "Desktop");
    }
}
