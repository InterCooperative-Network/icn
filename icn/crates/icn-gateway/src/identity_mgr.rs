//! Identity Manager
//!
//! Manages DID documents and device registration for multi-device support.

use icn_identity::{Capability, Did, DidDocument, KeyType};
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};

const DID_DOCUMENT_PREFIX: &str = "did_doc:";

/// Manager for identity and device operations
pub struct IdentityManager {
    store: Arc<dyn Store>,
    /// Cache of recently accessed DID documents
    cache: RwLock<lru::LruCache<String, DidDocument>>,
}

impl IdentityManager {
    /// Create a new identity manager with temporary storage (for testing)
    pub fn new() -> Self {
        // SAFETY: SledStore::temporary() only fails on I/O errors which are unrecoverable
        #[allow(clippy::expect_used)]
        let store =
            Arc::new(icn_store::SledStore::temporary().expect("Failed to create temp store"));

        // SAFETY: 1000 is always non-zero
        #[allow(clippy::unwrap_used)]
        let capacity = std::num::NonZeroUsize::new(1000).unwrap();

        Self {
            store,
            cache: RwLock::new(lru::LruCache::new(capacity)),
        }
    }

    /// Create a new identity manager with persistent storage
    pub fn new_with_storage(data_dir: std::path::PathBuf) -> Result<Self> {
        let store_path = data_dir.join("identity_store");
        let store = Arc::new(icn_store::SledStore::open(&store_path).map_err(|e| {
            GatewayError::InternalError(format!("Failed to open identity store: {e}"))
        })?);

        // SAFETY: 1000 is always non-zero
        #[allow(clippy::unwrap_used)]
        let capacity = std::num::NonZeroUsize::new(1000).unwrap();

        Ok(Self {
            store,
            cache: RwLock::new(lru::LruCache::new(capacity)),
        })
    }

    /// Get or create a DID document
    pub async fn get_or_create_document(
        &self,
        did: &Did,
        initial_key: &[u8; 32],
        initial_x25519_key: &[u8; 32],
    ) -> Result<DidDocument> {
        // Check cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(doc) = cache.get(did.as_str()) {
                return Ok(doc.clone());
            }
        }

        // Check storage
        let key = format!("{DID_DOCUMENT_PREFIX}{}", did.as_str());
        if let Some(data) = self.store.get(key.as_bytes())? {
            let doc: DidDocument = serde_json::from_slice(&data).map_err(|e| {
                GatewayError::InternalError(format!("Failed to parse DID doc: {e}"))
            })?;

            // Update cache
            let mut cache = self.cache.write().await;
            cache.put(did.as_str().to_string(), doc.clone());

            return Ok(doc);
        }

        // Create new document
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(initial_key)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid public key: {e}")))?;

        let doc = DidDocument::new(did.clone(), &verifying_key, initial_x25519_key);

        // Store it
        self.store_document(&doc).await?;

        info!(did = %did, "Created new DID document");
        Ok(doc)
    }

    /// Get a DID document by DID
    pub async fn get_document(&self, did: &Did) -> Result<Option<DidDocument>> {
        // Check cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(doc) = cache.get(did.as_str()) {
                return Ok(Some(doc.clone()));
            }
        }

        // Check storage
        let key = format!("{DID_DOCUMENT_PREFIX}{}", did.as_str());
        if let Some(data) = self.store.get(key.as_bytes())? {
            let doc: DidDocument = serde_json::from_slice(&data).map_err(|e| {
                GatewayError::InternalError(format!("Failed to parse DID doc: {e}"))
            })?;

            // Update cache
            let mut cache = self.cache.write().await;
            cache.put(did.as_str().to_string(), doc.clone());

            return Ok(Some(doc));
        }

        Ok(None)
    }

    /// Store a DID document
    async fn store_document(&self, doc: &DidDocument) -> Result<()> {
        let key = format!("{DID_DOCUMENT_PREFIX}{}", doc.id.as_str());
        let data = serde_json::to_vec(doc).map_err(|e| {
            GatewayError::InternalError(format!("Failed to serialize DID doc: {e}"))
        })?;

        self.store.put(key.as_bytes(), &data)?;

        // Update cache
        let mut cache = self.cache.write().await;
        cache.put(doc.id.as_str().to_string(), doc.clone());

        Ok(())
    }

    /// Register a new device for a DID
    ///
    /// The caller must provide a signature proving they have authority to add devices.
    pub async fn register_device(
        &self,
        did: &Did,
        request: &RegisterDeviceRequest,
        signing_device_id: &str,
        signature: &[u8],
    ) -> Result<DeviceInfo> {
        // Get the existing document
        let mut doc = self
            .get_document(did)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("DID document not found: {did}")))?;

        // Verify the signing device has AddDevice capability
        if !doc.has_capability(signing_device_id, Capability::AddDevice) {
            return Err(GatewayError::AuthorizationFailed(
                "Device does not have AddDevice capability".to_string(),
            ));
        }

        // Verify the signature
        let vm = doc
            .get_verification_method(signing_device_id)
            .ok_or_else(|| GatewayError::NotFound("Signing device not found".to_string()))?;

        if vm.key_type != KeyType::Ed25519 {
            return Err(GatewayError::BadRequest(
                "Signing device must use Ed25519".to_string(),
            ));
        }

        let verifying_key_bytes: [u8; 32] = vm
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| GatewayError::InternalError("Invalid key length".to_string()))?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&verifying_key_bytes)
            .map_err(|e| GatewayError::InternalError(format!("Invalid key: {e}")))?;

        // Build message to verify
        let message = build_add_device_message(did, &request.device_id, &request.label);
        let sig: ed25519_dalek::Signature = signature
            .try_into()
            .map_err(|_| GatewayError::BadRequest("Invalid signature format".to_string()))?;

        use ed25519_dalek::Verifier;
        verifying_key.verify(&message, &sig).map_err(|_| {
            GatewayError::AuthorizationFailed("Signature verification failed".to_string())
        })?;

        // Parse capabilities
        let capabilities = parse_capabilities(&request.capabilities)?;

        // Decode the new device's public key
        let public_key = hex::decode(&request.public_key)
            .map_err(|e| GatewayError::BadRequest(format!("Invalid public key hex: {e}")))?;

        // If encryption key provided, add both
        if let Some(ref enc_key) = request.encryption_public_key {
            let x25519_key = hex::decode(enc_key).map_err(|e| {
                GatewayError::BadRequest(format!("Invalid encryption key hex: {e}"))
            })?;

            doc.add_device_with_encryption_key(
                request.device_id.clone(),
                request.label.clone(),
                public_key.clone(),
                x25519_key,
                capabilities.clone(),
            )
            .map_err(|e| GatewayError::BadRequest(e.to_string()))?;
        } else {
            doc.add_device(
                request.device_id.clone(),
                request.label.clone(),
                public_key.clone(),
                KeyType::Ed25519,
                capabilities.clone(),
            )
            .map_err(|e| GatewayError::BadRequest(e.to_string()))?;
        }

        // Store updated document
        self.store_document(&doc).await?;

        info!(
            did = %did,
            device_id = %request.device_id,
            "Registered new device"
        );

        Ok(DeviceInfo {
            id: request.device_id.clone(),
            label: request.label.clone(),
            key_type: "Ed25519".to_string(),
            capabilities: request.capabilities.clone(),
            added_at: doc
                .get_verification_method(&request.device_id)
                .map(|vm| vm.added_at)
                .unwrap_or(0),
            revoked: false,
        })
    }

    /// List all devices for a DID
    pub async fn list_devices(&self, did: &Did) -> Result<Vec<DeviceInfo>> {
        let doc = self
            .get_document(did)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("DID document not found: {did}")))?;

        let devices = doc
            .verification_method
            .iter()
            .filter(|vm| vm.key_type == KeyType::Ed25519) // Only signing keys are "devices"
            .map(|vm| DeviceInfo {
                id: vm.id.clone(),
                label: vm.label.clone(),
                key_type: format!("{:?}", vm.key_type),
                capabilities: vm
                    .capabilities
                    .iter()
                    .map(|c| format!("{c:?}").to_lowercase())
                    .collect(),
                added_at: vm.added_at,
                revoked: vm.revoked_at.is_some(),
            })
            .collect();

        Ok(devices)
    }

    /// Revoke a device
    pub async fn revoke_device(
        &self,
        did: &Did,
        device_id: &str,
        signing_device_id: &str,
        signature: &[u8],
    ) -> Result<()> {
        // Get the existing document
        let mut doc = self
            .get_document(did)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("DID document not found: {did}")))?;

        // Verify the signing device has RevokeDevice capability
        if !doc.has_capability(signing_device_id, Capability::RevokeDevice) {
            return Err(GatewayError::AuthorizationFailed(
                "Device does not have RevokeDevice capability".to_string(),
            ));
        }

        // Verify the signature
        let vm = doc
            .get_verification_method(signing_device_id)
            .ok_or_else(|| GatewayError::NotFound("Signing device not found".to_string()))?;

        let verifying_key_bytes: [u8; 32] = vm
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| GatewayError::InternalError("Invalid key length".to_string()))?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&verifying_key_bytes)
            .map_err(|e| GatewayError::InternalError(format!("Invalid key: {e}")))?;

        // Build message to verify
        let message = build_revoke_device_message(did, device_id);
        let sig: ed25519_dalek::Signature = signature
            .try_into()
            .map_err(|_| GatewayError::BadRequest("Invalid signature format".to_string()))?;

        use ed25519_dalek::Verifier;
        verifying_key.verify(&message, &sig).map_err(|_| {
            GatewayError::AuthorizationFailed("Signature verification failed".to_string())
        })?;

        // Prevent self-revocation if it's the last device with management capabilities
        let devices_with_revoke_cap: Vec<_> = doc
            .verification_method
            .iter()
            .filter(|vm| {
                vm.revoked_at.is_none() && vm.capabilities.contains(&Capability::RevokeDevice)
            })
            .collect();

        if devices_with_revoke_cap.len() == 1 && devices_with_revoke_cap[0].id == device_id {
            return Err(GatewayError::BadRequest(
                "Cannot revoke the last device with management capabilities".to_string(),
            ));
        }

        // Revoke the device
        doc.revoke_device(device_id)
            .map_err(|e| GatewayError::BadRequest(e.to_string()))?;

        // Also revoke the encryption key if it exists
        let enc_key_id = format!("{device_id}-enc");
        if doc.get_verification_method(&enc_key_id).is_some() {
            let _ = doc.revoke_device(&enc_key_id); // Ignore error if it doesn't exist
        }

        // Store updated document
        self.store_document(&doc).await?;

        info!(
            did = %did,
            device_id = %device_id,
            "Revoked device"
        );

        Ok(())
    }

    /// Verify a signature from any authorized device
    pub async fn verify_device_signature(
        &self,
        did: &Did,
        message: &[u8],
        signature: &[u8],
    ) -> Result<String> {
        let doc = self
            .get_document(did)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("DID document not found: {did}")))?;

        let sig: ed25519_dalek::Signature = signature
            .try_into()
            .map_err(|_| GatewayError::BadRequest("Invalid signature format".to_string()))?;

        // Try each signing key
        for vm in doc.verification_method.iter() {
            if vm.key_type != KeyType::Ed25519 || vm.revoked_at.is_some() {
                continue;
            }

            if !vm.capabilities.contains(&Capability::Sign) {
                continue;
            }

            let key_bytes: [u8; 32] = match vm.public_key.as_slice().try_into() {
                Ok(b) => b,
                Err(_) => continue,
            };

            let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&key_bytes) {
                Ok(k) => k,
                Err(_) => continue,
            };

            use ed25519_dalek::Verifier;
            if verifying_key.verify(message, &sig).is_ok() {
                debug!(did = %did, device_id = %vm.id, "Signature verified");
                return Ok(vm.id.clone());
            }
        }

        Err(GatewayError::AuthorizationFailed(
            "No authorized device signed this message".to_string(),
        ))
    }
}

impl Default for IdentityManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to register a new device
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterDeviceRequest {
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
}

/// Device information in responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceInfo {
    pub id: String,
    pub label: String,
    pub key_type: String,
    pub capabilities: Vec<String>,
    pub added_at: u64,
    pub revoked: bool,
}

/// Request to revoke a device
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeDeviceRequest {
    /// Device ID of the device performing the revocation
    pub signing_device_id: String,

    /// Hex-encoded signature
    pub signature: String,
}

/// Response for device registration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterDeviceResponse {
    pub device: DeviceInfo,
    pub document_version: u64,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_capabilities(caps: &[String]) -> Result<Vec<Capability>> {
    caps.iter()
        .map(|c| match c.to_lowercase().as_str() {
            "sign" => Ok(Capability::Sign),
            "add_device" | "adddevice" => Ok(Capability::AddDevice),
            "revoke_device" | "revokedevice" => Ok(Capability::RevokeDevice),
            "rotate_key" | "rotatekey" => Ok(Capability::RotateKey),
            "recover" => Ok(Capability::Recover),
            "encrypt" => Ok(Capability::Encrypt),
            _ => Err(GatewayError::BadRequest(format!("Unknown capability: {c}"))),
        })
        .collect()
}

fn build_add_device_message(did: &Did, device_id: &str, label: &str) -> Vec<u8> {
    format!("ICN_ADD_DEVICE:{}:{}:{}", did.as_str(), device_id, label).into_bytes()
}

fn build_revoke_device_message(did: &Did, device_id: &str) -> Vec<u8> {
    format!("ICN_REVOKE_DEVICE:{}:{}", did.as_str(), device_id).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did_and_keys() -> (Did, [u8; 32], [u8; 32]) {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let pub_key = *kp.verifying_key().as_bytes();
        let x25519_key = [0u8; 32]; // Dummy encryption key
        (did, pub_key, x25519_key)
    }

    #[tokio::test]
    async fn test_get_or_create_document() {
        let mgr = IdentityManager::new();
        let (did, pub_key, x25519_key) = test_did_and_keys();

        let doc = mgr
            .get_or_create_document(&did, &pub_key, &x25519_key)
            .await
            .unwrap();

        assert_eq!(doc.id, did);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.verification_method.len(), 2); // Ed25519 + X25519

        // Get again should return same document
        let doc2 = mgr.get_document(&did).await.unwrap().unwrap();
        assert_eq!(doc2.version, doc.version);
    }

    #[tokio::test]
    async fn test_list_devices() {
        let mgr = IdentityManager::new();
        let (did, pub_key, x25519_key) = test_did_and_keys();

        mgr.get_or_create_document(&did, &pub_key, &x25519_key)
            .await
            .unwrap();

        let devices = mgr.list_devices(&did).await.unwrap();

        assert_eq!(devices.len(), 1); // Only one signing device
        assert_eq!(devices[0].id, "device-1");
        assert!(!devices[0].revoked);
    }

    #[tokio::test]
    async fn test_parse_capabilities() {
        let caps = vec![
            "sign".to_string(),
            "add_device".to_string(),
            "encrypt".to_string(),
        ];
        let parsed = parse_capabilities(&caps).unwrap();

        assert!(parsed.contains(&Capability::Sign));
        assert!(parsed.contains(&Capability::AddDevice));
        assert!(parsed.contains(&Capability::Encrypt));
    }

    #[tokio::test]
    async fn test_parse_invalid_capability() {
        let caps = vec!["invalid_cap".to_string()];
        let result = parse_capabilities(&caps);
        assert!(result.is_err());
    }
}
