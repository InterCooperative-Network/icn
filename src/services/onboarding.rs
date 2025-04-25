use crate::federation::{FederationError, FederationRuntime, FinalizationReceipt, FederationManifest};
use crate::identity::{Identity, IdentityManager};
use crate::storage::{StorageManager, StorageError};
use crate::services::{FederationSyncService, FederationSyncError};
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use std::io;
use thiserror::Error;
use qrcode::{QrCode, render::unicode};
use uuid::Uuid;

/// Errors that can occur in the onboarding process
#[derive(Debug, Error)]
pub enum OnboardingError {
    #[error("Federation error: {0}")]
    FederationError(#[from] FederationError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Sync error: {0}")]
    SyncError(#[from] FederationSyncError),
    
    #[error("QR code generation error: {0}")]
    QrCodeError(#[from] qrcode::types::QrError),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    
    #[error("Invalid invite format: {0}")]
    InvalidFormat(String),
    
    #[error("Onboarding error: {0}")]
    OnboardingError(String),
}

/// Supported output formats for QR codes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QrFormat {
    Terminal,
    Svg,
    Png,
}

impl QrFormat {
    /// Parse format from string
    pub fn from_str(format: &str) -> Option<Self> {
        match format.to_lowercase().as_str() {
            "terminal" => Some(QrFormat::Terminal),
            "svg" => Some(QrFormat::Svg),
            "png" => Some(QrFormat::Png),
            _ => None,
        }
    }
}

/// Federation invite payload for QR codes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationInvitePayload {
    /// ID of the federation to join
    pub federation_id: String,
    
    /// Optional name of the federation (for display)
    pub name: Option<String>,
    
    /// Federation manifest (if embedded)
    pub manifest: Option<FederationManifest>,
    
    /// Hash of the manifest (if not embedded)
    pub manifest_hash: Option<String>,
    
    /// Endpoint to fetch the manifest from (if not embedded)
    pub manifest_endpoint: Option<String>,
    
    /// Root credential/receipt to bootstrap trust
    pub root_credential: Option<FinalizationReceipt>,
    
    /// Bootstrap peer address for direct connection
    pub bootstrap_peer: Option<String>,
    
    /// Invite creator DID
    pub creator_did: String,
    
    /// Timestamp when the invite was created
    pub created: chrono::DateTime<chrono::Utc>,
    
    /// Expiration time (optional)
    pub expires: Option<chrono::DateTime<chrono::Utc>>,
}

/// Onboarding service for handling federation invites
pub struct OnboardingService {
    federation_runtime: FederationRuntime,
    identity_manager: IdentityManager,
    storage_manager: StorageManager,
    sync_service: FederationSyncService,
}

impl OnboardingService {
    /// Create a new onboarding service
    pub fn new(
        federation_runtime: FederationRuntime,
        identity_manager: IdentityManager,
        storage_manager: StorageManager,
        sync_service: FederationSyncService,
    ) -> Self {
        Self {
            federation_runtime,
            identity_manager,
            storage_manager,
            sync_service,
        }
    }
    
    /// Create a federation invite
    pub fn create_invite(&self, federation_id: &str) -> Result<FederationInvitePayload, OnboardingError> {
        // Get the active identity
        let identity = self.identity_manager.get_active_identity()
            .ok_or_else(|| OnboardingError::OnboardingError("No active identity found".to_string()))?;
        
        // Get the federation manifest
        let manifest = self.federation_runtime.get_federation_manifest(federation_id)?;
        
        // Get one root credential (admin receipt) to bootstrap trust
        let root_receipts = self.federation_runtime.get_finalized_receipts_by_did(&identity.did())?;
        let root_credential = root_receipts.into_iter()
            .find(|r| r.federation_id == federation_id && r.receipt_type == "member_credential");
        
        // Create the invite payload
        let invite = FederationInvitePayload {
            federation_id: federation_id.to_string(),
            name: Some(manifest.name.clone()),
            manifest: Some(manifest),
            manifest_hash: None, // Only used if manifest is not embedded
            manifest_endpoint: None, // Only used if manifest is not embedded
            root_credential,
            bootstrap_peer: None, // Optional, set if available
            creator_did: identity.did().to_string(),
            created: chrono::Utc::now(),
            expires: Some(chrono::Utc::now() + chrono::Duration::days(7)), // Default 7-day expiration
        };
        
        Ok(invite)
    }
    
    /// Process a federation invite
    pub fn process_invite(&self, invite: FederationInvitePayload) -> Result<(), OnboardingError> {
        // Validate the invite
        self.validate_invite(&invite)?;
        
        // Check if we already have this federation
        if let Ok(existing_manifest) = self.federation_runtime.get_federation_manifest(&invite.federation_id) {
            // We already have this federation, check if it's the same version
            if let Some(manifest) = &invite.manifest {
                if existing_manifest.version >= manifest.version {
                    // We already have this version or newer, nothing to do
                    return Ok(());
                }
            }
        }
        
        // Store the manifest if provided
        if let Some(manifest) = invite.manifest {
            // Store the manifest in local storage
            let manifests_dir = self.storage_manager.get_data_dir().join("federation_manifests");
            if !manifests_dir.exists() {
                fs::create_dir_all(&manifests_dir)?;
            }
            
            let manifest_path = manifests_dir.join(format!("{}.json", invite.federation_id));
            let manifest_json = serde_json::to_string_pretty(&manifest)?;
            fs::write(manifest_path, manifest_json)?;
        } else if let (Some(manifest_hash), Some(manifest_endpoint)) = (&invite.manifest_hash, &invite.manifest_endpoint) {
            // TODO: Fetch the manifest from the endpoint and verify its hash
            return Err(OnboardingError::OnboardingError(
                "Fetching manifests from endpoints not yet implemented".to_string()
            ));
        } else {
            return Err(OnboardingError::InvalidFormat(
                "Invite missing both manifest and endpoint information".to_string()
            ));
        }
        
        // Store the root credential if provided
        if let Some(receipt) = invite.root_credential {
            // The receipt needs to be stored, but process_receipt is private
            // Let's create a credential sync data entry manually
            let credential_id = format!("cred-import-{}", uuid::Uuid::new_v4());
            let receipt_id = receipt.id.clone();
            let federation_id = receipt.federation_id.clone();
            
            let credential_data = crate::services::CredentialSyncData {
                credential_id: credential_id.clone(),
                receipt_id,
                receipt,
                federation_id,
                status: crate::services::CredentialStatus::Pending,
                trust_score: None,
                last_verified: chrono::Utc::now(),
                verifiable_credential: None, // We'd need to convert it, but that's a private method too
            };
            
            // Add it to the sync service's data
            let mut sync_data = self.sync_service.sync_data.lock().unwrap();
            sync_data.insert(credential_id.clone(), credential_data);
            drop(sync_data);
            
            println!("Imported root credential with ID: {}", credential_id);
        }
        
        // If a bootstrap peer is provided, add it to the federation configuration
        if let Some(peer) = invite.bootstrap_peer {
            // TODO: Configure the federation runtime to use this peer
            println!("Bootstrap peer available: {}", peer);
        }
        
        Ok(())
    }
    
    /// Validate a federation invite
    fn validate_invite(&self, invite: &FederationInvitePayload) -> Result<(), OnboardingError> {
        // Check if the invite has expired
        if let Some(expires) = invite.expires {
            if chrono::Utc::now() > expires {
                return Err(OnboardingError::OnboardingError("Invite has expired".to_string()));
            }
        }
        
        // If manifest is provided, validate it
        if let Some(manifest) = &invite.manifest {
            if manifest.federation_id != invite.federation_id {
                return Err(OnboardingError::InvalidFormat(
                    "Federation ID mismatch between invite and manifest".to_string()
                ));
            }
            
            // Additional validation could be done here, like verifying signatures
        }
        
        Ok(())
    }
    
    /// Encode an invite payload for QR code
    pub fn encode_invite_for_qr(&self, invite: &FederationInvitePayload) -> Result<String, OnboardingError> {
        // Serialize to JSON
        let json = serde_json::to_string(invite)?;
        
        // Base64url encode
        let encoded = URL_SAFE.encode(json.as_bytes());
        
        // Add a prefix to identify this as an ICN federation invite
        let prefixed = format!("icn:fed:{}", encoded);
        
        Ok(prefixed)
    }
    
    /// Decode a QR code string back to an invite payload
    pub fn decode_invite_from_qr(&self, qr_content: &str) -> Result<FederationInvitePayload, OnboardingError> {
        // Check prefix
        if !qr_content.starts_with("icn:fed:") {
            return Err(OnboardingError::InvalidFormat("Invalid QR code format".to_string()));
        }
        
        // Remove prefix
        let encoded = qr_content.trim_start_matches("icn:fed:");
        
        // Base64url decode
        let decoded = URL_SAFE.decode(encoded.as_bytes())
            .map_err(|_| OnboardingError::InvalidFormat("Invalid base64 encoding".to_string()))?;
        
        // Parse JSON
        let json = String::from_utf8(decoded)
            .map_err(|_| OnboardingError::InvalidFormat("Invalid UTF-8 encoding".to_string()))?;
        
        // Deserialize into the invite payload
        let invite: FederationInvitePayload = serde_json::from_str(&json)?;
        
        Ok(invite)
    }
    
    /// Generate a QR code for a federation invite
    pub fn generate_invite_qr(
        &self,
        invite: &FederationInvitePayload,
        format: QrFormat,
        output_path: Option<&Path>,
    ) -> Result<String, OnboardingError> {
        // Encode invite for QR
        let encoded = self.encode_invite_for_qr(invite)?;
        
        // Generate QR code
        let code = QrCode::new(encoded)?;
        
        match format {
            QrFormat::Terminal => {
                // For terminal output, use unicode renderer
                let qr_string = code
                    .render::<unicode::Dense1x2>()
                    .build();
                Ok(qr_string)
            },
            QrFormat::Svg => {
                // Generate SVG manually
                let width = code.width();
                let modules = code.to_colors();
                
                let mut svg = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n\
                     <svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" viewBox=\"0 0 ");
                svg.push_str(&(width + 2).to_string());
                svg.push_str(" ");
                svg.push_str(&(width + 2).to_string());
                svg.push_str("\" stroke=\"none\">\n\
                              <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
                
                // Draw modules as squares
                for (i, color) in modules.iter().enumerate() {
                    if *color {
                        let x = (i % width) + 1;
                        let y = (i / width) + 1;
                        svg.push_str("<rect x=\"");
                        svg.push_str(&x.to_string());
                        svg.push_str("\" y=\"");
                        svg.push_str(&y.to_string());
                        svg.push_str("\" width=\"1\" height=\"1\" fill=\"black\"/>\n");
                    }
                }
                
                svg.push_str("</svg>");
                
                // Save to file if path provided
                if let Some(path) = output_path {
                    fs::write(path, svg.as_bytes())?;
                    Ok(format!("QR code saved to {}", path.display()))
                } else {
                    Ok(svg)
                }
            },
            QrFormat::Png => {
                // For PNG, use the image crate
                if let Some(path) = output_path {
                    // Image size in pixels
                    let size = 250;
                    let width = code.width() as u32;
                    let module_size = size / width;
                    
                    // Create a new image
                    let mut imgbuf = image::RgbImage::new(size, size);
                    
                    // Fill with white
                    for pixel in imgbuf.pixels_mut() {
                        *pixel = image::Rgb([255, 255, 255]);
                    }
                    
                    // Draw QR code
                    let modules = code.to_colors();
                    for (i, color) in modules.iter().enumerate() {
                        if *color {
                            let x = (i % code.width()) as u32;
                            let y = (i / code.width()) as u32;
                            
                            // Draw a black square for each module
                            for dy in 0..module_size {
                                for dx in 0..module_size {
                                    let px = (x * module_size) + dx;
                                    let py = (y * module_size) + dy;
                                    if px < size && py < size {
                                        imgbuf.put_pixel(px, py, image::Rgb([0, 0, 0]));
                                    }
                                }
                            }
                        }
                    }
                    
                    // Save to file
                    imgbuf.save(path).map_err(|e| OnboardingError::IoError(e.into()))?;
                    Ok(format!("QR code saved to {}", path.display()))
                } else {
                    Err(OnboardingError::OnboardingError("PNG format requires an output path".to_string()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // TODO: Add tests for onboarding functionality
} 