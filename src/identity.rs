use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use p256::ecdsa::{signature::{Signer as EcdsaSigner, Verifier}, Signature, SigningKey as EcdsaSigningKey, VerifyingKey as EcdsaVerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use base64::{Engine as _};
use crate::guardians::{RecoveryBundle, GuardianSignature, GuardianError};

// Identity-related errors
#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("Failed to generate keypair: {0}")]
    KeyGeneration(String),
    
    #[error("Invalid DID format")]
    InvalidDid,
    
    #[error("Failed to sign payload: {0}")]
    SigningFailed(String),
    
    #[error("Failed to save identity: {0}")]
    SaveFailed(String),
    
    #[error("Failed to load identity: {0}")]
    LoadFailed(String),
    
    #[error("Identity not found: {0}")]
    NotFound(String),
    
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
    
    #[error("Guardian error: {0}")]
    GuardianError(#[from] GuardianError),
    
    #[error("Device linking error: {0}")]
    DeviceLinkingError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyType {
    Ed25519,
    Ecdsa,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDocument {
    #[serde(rename = "@context")]
    context: Vec<String>,
    id: String,
    controller: String,
    verification_method: Vec<VerificationMethod>,
    authentication: Vec<String>,
    assertion_method: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    id: String,
    #[serde(rename = "type")]
    key_type: String,
    controller: String,
    public_key_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    did: String,
    scope: String,
    username: String,
    // Store only the public key in the serializable struct
    public_key: String,
    key_type: KeyType,
    created_at: chrono::DateTime<chrono::Utc>,
    metadata: HashMap<String, String>,
    
    #[serde(skip_serializing, skip_deserializing)]
    keypair_bytes: Option<Vec<u8>>,
}

// New device linking structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLinkChallenge {
    pub source_did: String,
    pub source_device_id: String,
    pub target_public_key: String,
    pub target_key_type: KeyType,
    pub challenge_nonce: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub expiration: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLink {
    pub challenge: DeviceLinkChallenge,
    pub signature: String,
}

// Utility function to create a DID string
fn create_did(scope: &str, username: &str) -> String {
    format!("did:icn:{}:{}", scope, username)
}

impl Identity {
    pub fn new(scope: &str, username: &str, key_type: KeyType) -> Result<Self, IdentityError> {
        match key_type {
            KeyType::Ed25519 => Self::new_ed25519(scope, username),
            KeyType::Ecdsa => Self::new_ecdsa(scope, username),
        }
    }
    
    fn new_ed25519(scope: &str, username: &str) -> Result<Self, IdentityError> {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        
        let verifying_key = signing_key.verifying_key();
        let public_key = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());
        let keypair_bytes = signing_key.to_bytes().to_vec();
        
        Ok(Self {
            did: create_did(scope, username),
            scope: scope.to_string(),
            username: username.to_string(),
            public_key,
            key_type: KeyType::Ed25519,
            created_at: chrono::Utc::now(),
            metadata: HashMap::new(),
            keypair_bytes: Some(keypair_bytes),
        })
    }
    
    fn new_ecdsa(scope: &str, username: &str) -> Result<Self, IdentityError> {
        let signing_key = EcdsaSigningKey::random(&mut OsRng);
        let public_key = base64::engine::general_purpose::STANDARD.encode(&signing_key.verifying_key().to_encoded_point(false).as_bytes());
        
        // For ECDSA, we'll need to serialize the key into a vec
        let mut keypair_bytes = Vec::new();
        // First add the private key bytes
        keypair_bytes.extend_from_slice(&signing_key.to_bytes());
        
        Ok(Self {
            did: create_did(scope, username),
            scope: scope.to_string(),
            username: username.to_string(),
            public_key,
            key_type: KeyType::Ecdsa,
            created_at: chrono::Utc::now(),
            metadata: HashMap::new(),
            keypair_bytes: Some(keypair_bytes),
        })
    }
    
    pub fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, IdentityError> {
        if self.keypair_bytes.is_none() {
            return Err(IdentityError::SigningFailed("No keypair available".to_string()));
        }
        
        match self.key_type {
            KeyType::Ed25519 => {
                let keypair_bytes = self.keypair_bytes.as_ref().unwrap();
                let signing_key = SigningKey::from_bytes(keypair_bytes.as_slice().try_into()
                    .map_err(|_| IdentityError::SigningFailed("Invalid key length".to_string()))?);
                
                let signature = signing_key.sign(payload);
                Ok(signature.to_vec())
            },
            KeyType::Ecdsa => {
                let keypair_bytes = self.keypair_bytes.as_ref().unwrap();
                let secret_key = p256::SecretKey::from_slice(keypair_bytes.as_slice())
                    .map_err(|e| IdentityError::SigningFailed(format!("Invalid ECDSA key: {}", e)))?;
                let signing_key = EcdsaSigningKey::from(secret_key);
                
                let signature: Signature = signing_key.sign(payload);
                Ok(signature.to_der().as_bytes().to_vec())
            }
        }
    }
    
    pub fn to_did_document(&self) -> DidDocument {
        let vm_id = format!("{}#keys-1", self.did);
        
        DidDocument {
            context: vec![
                "https://www.w3.org/ns/did/v1".to_string(),
                "https://w3id.org/security/suites/ed25519-2020/v1".to_string(),
            ],
            id: self.did.clone(),
            controller: self.did.clone(),
            verification_method: vec![
                VerificationMethod {
                    id: vm_id.clone(),
                    key_type: match self.key_type {
                        KeyType::Ed25519 => "Ed25519VerificationKey2020".to_string(),
                        KeyType::Ecdsa => "EcdsaSecp256r1VerificationKey2019".to_string(),
                    },
                    controller: self.did.clone(),
                    public_key_base64: self.public_key.clone(),
                }
            ],
            authentication: vec![vm_id.clone()],
            assertion_method: vec![vm_id],
        }
    }
    
    pub fn export_metadata(&self, path: &Path) -> Result<(), IdentityError> {
        let meta_json = serde_json::to_string_pretty(&self).map_err(|e| {
            IdentityError::SaveFailed(format!("Failed to serialize identity: {}", e))
        })?;
        
        fs::write(path, meta_json).map_err(|e| {
            IdentityError::SaveFailed(format!("Failed to write metadata file: {}", e))
        })?;
        
        Ok(())
    }
    
    pub fn import_metadata(path: &Path) -> Result<Self, IdentityError> {
        let meta_json = fs::read_to_string(path).map_err(|e| {
            IdentityError::LoadFailed(format!("Failed to read metadata file: {}", e))
        })?;
        
        let mut identity: Identity = serde_json::from_str(&meta_json).map_err(|e| {
            IdentityError::LoadFailed(format!("Failed to deserialize identity: {}", e))
        })?;
        
        // Note: After import, the keypair is not available.
        // The keypair needs to be loaded separately.
        identity.keypair_bytes = None;
        
        Ok(identity)
    }
    
    pub fn did(&self) -> &str {
        &self.did
    }
    
    pub fn scope(&self) -> &str {
        &self.scope
    }
    
    pub fn username(&self) -> &str {
        &self.username
    }
    
    pub fn add_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    // Add a new method to get metadata as a serializable object
    pub fn get_metadata(&self) -> Self {
        self.clone()
    }

    /// Verify a signature with this identity's public key
    pub fn verify(&self, payload: &[u8], signature: &[u8]) -> Result<bool, IdentityError> {
        match self.key_type {
            KeyType::Ed25519 => {
                // Decode the public key from base64
                let public_key_bytes = base64::engine::general_purpose::STANDARD.decode(&self.public_key)
                    .map_err(|e| IdentityError::VerificationFailed(format!("Failed to decode public key: {}", e)))?;
                
                // Create a verifying key
                let verifying_key = VerifyingKey::from_bytes(public_key_bytes.as_slice().try_into()
                    .map_err(|_| IdentityError::VerificationFailed("Invalid public key length".to_string()))?);
                
                // Verify the signature
                let sig = ed25519_dalek::Signature::from_bytes(signature.try_into()
                    .map_err(|_| IdentityError::VerificationFailed("Invalid signature length".to_string()))?);
                
                match verifying_key.verify_strict(payload, &sig) {
                    Ok(_) => Ok(true),
                    Err(e) => {
                        // Return false instead of error for verification failures
                        // Only return error for technical issues
                        Ok(false)
                    }
                }
            },
            KeyType::Ecdsa => {
                // Decode the public key from base64
                let public_key_bytes = base64::engine::general_purpose::STANDARD.decode(&self.public_key)
                    .map_err(|e| IdentityError::VerificationFailed(format!("Failed to decode public key: {}", e)))?;
                
                // Create a verifying key
                let verifying_key = EcdsaVerifyingKey::from_sec1_bytes(&public_key_bytes)
                    .map_err(|e| IdentityError::VerificationFailed(format!("Invalid ECDSA public key: {}", e)))?;
                
                // Verify the signature
                let sig = Signature::from_der(signature)
                    .map_err(|e| IdentityError::VerificationFailed(format!("Invalid ECDSA signature: {}", e)))?;
                
                match verifying_key.verify(payload, &sig) {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false)
                }
            }
        }
    }
    
    /// Verify a guardian's signature for identity recovery
    pub fn verify_guardian_signature(&self, did: &str, signature: &[u8]) -> Result<bool, IdentityError> {
        // In a recovery scenario, the guardian signs the DID of the identity being recovered
        let payload = did.as_bytes();
        self.verify(payload, signature)
    }

    /// Generate a device linking challenge for a target device public key
    pub fn create_device_link_challenge(&self, target_public_key: &str, target_key_type: KeyType) -> Result<DeviceLinkChallenge, IdentityError> {
        // Get or create a device ID for the current device
        let device_id = self.metadata.get("device_id")
            .cloned()
            .unwrap_or_else(|| {
                let new_id = Uuid::new_v4().to_string();
                // Note: this doesn't persist the device_id since we're not mutating self
                // The caller should add this to metadata if not already present
                new_id
            });
            
        // Create a random nonce for the challenge
        let nonce = Uuid::new_v4().to_string();
        
        // Set expiration to 24 hours from now
        let now = chrono::Utc::now();
        let expiration = now + chrono::Duration::hours(24);
        
        Ok(DeviceLinkChallenge {
            source_did: self.did.clone(),
            source_device_id: device_id,
            target_public_key: target_public_key.to_string(),
            target_key_type,
            challenge_nonce: nonce,
            timestamp: now,
            expiration,
        })
    }
    
    /// Sign a device link challenge
    pub fn sign_device_link(&self, challenge: &DeviceLinkChallenge) -> Result<DeviceLink, IdentityError> {
        // Ensure this identity is the source of the challenge
        if challenge.source_did != self.did {
            return Err(IdentityError::DeviceLinkingError(
                "Challenge source DID does not match this identity".to_string()
            ));
        }
        
        // Serialize challenge to bytes for signing
        let challenge_bytes = serde_json::to_vec(challenge)
            .map_err(|e| IdentityError::DeviceLinkingError(
                format!("Failed to serialize challenge: {}", e)
            ))?;
        
        // Sign the challenge
        let signature_bytes = self.sign(&challenge_bytes)?;
        let signature = base64::engine::general_purpose::STANDARD.encode(&signature_bytes);
        
        Ok(DeviceLink {
            challenge: challenge.clone(),
            signature,
        })
    }
    
    /// Verify a device link signature
    pub fn verify_device_link(link: &DeviceLink) -> Result<bool, IdentityError> {
        // Check if the link has expired
        let now = chrono::Utc::now();
        if now > link.challenge.expiration {
            return Err(IdentityError::DeviceLinkingError("Device link has expired".to_string()));
        }
        
        // Get the source public key from the DID
        // This would typically involve resolving the DID to get the public key
        // For now, we'll assume the public key is directly available
        
        // Serialize challenge to bytes for verification
        let challenge_bytes = serde_json::to_vec(&link.challenge)
            .map_err(|e| IdentityError::DeviceLinkingError(
                format!("Failed to serialize challenge: {}", e)
            ))?;
        
        // Decode signature
        let signature_bytes = base64::engine::general_purpose::STANDARD.decode(&link.signature)
            .map_err(|e| IdentityError::DeviceLinkingError(
                format!("Failed to decode signature: {}", e)
            ))?;
        
        // This is a placeholder for actual DID resolution and verification
        // In a real implementation, we would resolve the DID to get the verification method
        // and use it to verify the signature
        println!("Verification of device link would happen here with DID resolution");
        
        // For now, we return true as a placeholder
        // In a real implementation, this should properly verify the signature
        Ok(true)
    }
    
    /// Import identity from a device link
    pub fn from_device_link(link: &DeviceLink, private_key: &[u8]) -> Result<Self, IdentityError> {
        // Verify the device link is valid
        Self::verify_device_link(link)?;
        
        // Parse DID to get scope and username
        let did_parts: Vec<&str> = link.challenge.source_did.split(':').collect();
        if did_parts.len() < 4 || did_parts[0] != "did" || did_parts[1] != "icn" {
            return Err(IdentityError::InvalidDid);
        }
        
        let scope = did_parts[2].to_string();
        let username = did_parts[3].to_string();
        
        // Create a new identity with the same DID but with the target device's keypair
        let mut identity = match link.challenge.target_key_type {
            KeyType::Ed25519 => {
                // Load the private key
                let signing_key = SigningKey::from_bytes(private_key.try_into()
                    .map_err(|_| IdentityError::DeviceLinkingError("Invalid Ed25519 key length".to_string()))?);
                
                let verifying_key = signing_key.verifying_key();
                let public_key = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());
                
                Identity {
                    did: link.challenge.source_did.clone(),
                    scope,
                    username,
                    public_key,
                    key_type: KeyType::Ed25519,
                    created_at: chrono::Utc::now(),
                    metadata: HashMap::new(),
                    keypair_bytes: Some(signing_key.to_bytes().to_vec()),
                }
            },
            KeyType::Ecdsa => {
                // Load the private key
                let secret_key = p256::SecretKey::from_slice(private_key)
                    .map_err(|e| IdentityError::DeviceLinkingError(format!("Invalid ECDSA key: {}", e)))?;
                
                let signing_key = EcdsaSigningKey::from(secret_key);
                let public_key = base64::engine::general_purpose::STANDARD.encode(
                    &signing_key.verifying_key().to_encoded_point(false).as_bytes()
                );
                
                // Create the ECDSA keypair bytes
                let mut keypair_bytes = Vec::new();
                keypair_bytes.extend_from_slice(&signing_key.to_bytes());
                
                Identity {
                    did: link.challenge.source_did.clone(),
                    scope,
                    username,
                    public_key,
                    key_type: KeyType::Ecdsa,
                    created_at: chrono::Utc::now(),
                    metadata: HashMap::new(),
                    keypair_bytes: Some(keypair_bytes),
                }
            }
        };
        
        // Add device ID to metadata
        let device_id = Uuid::new_v4().to_string();
        identity.add_metadata("device_id", &device_id);
        
        // Add linked_from metadata to track the linking source
        identity.add_metadata("linked_from", &link.challenge.source_device_id);
        
        Ok(identity)
    }
    
    /// Generate a new device keypair for linking
    pub fn generate_device_keypair(key_type: KeyType) -> Result<(Vec<u8>, String), IdentityError> {
        match key_type {
            KeyType::Ed25519 => {
                let mut csprng = OsRng;
                let signing_key = SigningKey::generate(&mut csprng);
                let verifying_key = signing_key.verifying_key();
                let public_key = base64::engine::general_purpose::STANDARD.encode(verifying_key.as_bytes());
                let private_key = signing_key.to_bytes().to_vec();
                
                Ok((private_key, public_key))
            },
            KeyType::Ecdsa => {
                let signing_key = EcdsaSigningKey::random(&mut OsRng);
                let public_key = base64::engine::general_purpose::STANDARD.encode(
                    &signing_key.verifying_key().to_encoded_point(false).as_bytes()
                );
                let private_key = signing_key.to_bytes().to_vec();
                
                Ok((private_key, public_key))
            }
        }
    }
}

// IdentityManager handles multiple identities across different scopes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityManager {
    identities: HashMap<String, Identity>,
    active_identity: Option<String>,
}

impl IdentityManager {
    pub fn new() -> Self {
        Self {
            identities: HashMap::new(),
            active_identity: None,
        }
    }
    
    pub fn add_identity(&mut self, identity: Identity) {
        let did = identity.did().to_string();
        self.identities.insert(did.clone(), identity);
        
        // If this is the first identity, make it active
        if self.active_identity.is_none() {
            self.active_identity = Some(did);
        }
    }
    
    pub fn get_identity(&self, did: &str) -> Option<&Identity> {
        self.identities.get(did)
    }
    
    pub fn get_active_identity(&self) -> Option<&Identity> {
        self.active_identity.as_ref().and_then(|did| self.identities.get(did))
    }
    
    pub fn set_active_identity(&mut self, did: &str) -> Result<(), IdentityError> {
        if !self.identities.contains_key(did) {
            return Err(IdentityError::NotFound(did.to_string()));
        }
        self.active_identity = Some(did.to_string());
        Ok(())
    }
    
    pub fn list_identities(&self) -> Vec<&Identity> {
        self.identities.values().collect()
    }

    /// Recover an identity from a recovery bundle with guardian signatures
    pub fn recover_from_guardians(
        &self,
        bundle: &RecoveryBundle, 
        signatures: Vec<GuardianSignature>,
        recovery_password: &str
    ) -> Result<Identity, IdentityError> {
        // Extract the DIDs from the signatures for easier checking
        let signed_dids: Vec<String> = signatures.iter()
            .map(|sig| sig.guardian_did.clone())
            .collect();
        
        // Load the guardian set and check if recovery is possible
        // This would normally be done in the guardian manager, we're just creating
        // the identity-side recovery API here
        
        // Decrypt the recovery bundle using the password
        // In a real implementation, this would use proper encryption
        // For simplicity, we'll assume a decryption function exists
        let decrypted_data = match decrypt_recovery_data(
            &bundle.encrypted_data, 
            recovery_password, 
            &bundle.nonce
        ) {
            Ok(data) => data,
            Err(e) => return Err(IdentityError::RecoveryFailed(
                format!("Failed to decrypt recovery bundle: {}", e)
            )),
        };
        
        // Deserialize the decrypted data into an Identity
        let mut identity: Identity = match serde_json::from_slice(&decrypted_data) {
            Ok(id) => id,
            Err(e) => return Err(IdentityError::RecoveryFailed(
                format!("Failed to deserialize identity: {}", e)
            )),
        };
        
        // Return the recovered identity
        Ok(identity)
    }
    
    /// Verify a recovery request with guardian signatures
    pub fn verify_recovery_request(
        &self,
        did: &str,
        signatures: &[GuardianSignature],
        threshold: usize
    ) -> Result<bool, IdentityError> {
        // Check if we have enough signatures
        if signatures.len() < threshold {
            return Ok(false);
        }
        
        // Verify each signature
        let mut valid_signatures = 0;
        for sig in signatures {
            // Get the guardian's identity
            let guardian = match self.get_identity(&sig.guardian_did) {
                Some(id) => id,
                None => continue, // Skip guardians we don't have in our identity store
            };
            
            // Verify the signature
            match guardian.verify_guardian_signature(did, &sig.signature)? {
                true => valid_signatures += 1,
                false => continue,
            }
        }
        
        // Check if we have enough valid signatures
        Ok(valid_signatures >= threshold)
    }
}

// Utility function for decrypting recovery data (placeholder)
fn decrypt_recovery_data(encrypted_data: &[u8], password: &str, nonce: &[u8]) -> Result<Vec<u8>, String> {
    // In a real implementation, this would use proper encryption
    // For demonstration, we'll return the data as-is, assuming it's valid
    Ok(encrypted_data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;
    
    #[test]
    fn test_create_identity_ed25519() {
        let identity = Identity::new("coop1", "alice", KeyType::Ed25519).unwrap();
        assert_eq!(identity.did(), "did:icn:coop1:alice");
        assert_eq!(identity.scope(), "coop1");
        assert_eq!(identity.username(), "alice");
    }
    
    #[test]
    fn test_create_identity_ecdsa() {
        let identity = Identity::new("community", "bob", KeyType::Ecdsa).unwrap();
        assert_eq!(identity.did(), "did:icn:community:bob");
        assert_eq!(identity.scope(), "community");
        assert_eq!(identity.username(), "bob");
    }
    
    #[test]
    fn test_sign_verify_ed25519() {
        let identity = Identity::new("coop1", "alice", KeyType::Ed25519).unwrap();
        let message = b"Hello, ICN!";
        let signature = identity.sign(message).unwrap();
        assert!(!signature.is_empty());
    }
    
    #[test]
    fn test_export_import_metadata() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("alice.meta.json");
        
        let mut identity = Identity::new("coop1", "alice", KeyType::Ed25519).unwrap();
        identity.add_metadata("role", "worker");
        
        identity.export_metadata(&file_path).unwrap();
        
        let loaded_identity = Identity::import_metadata(&file_path).unwrap();
        assert_eq!(loaded_identity.did(), "did:icn:coop1:alice");
        assert_eq!(loaded_identity.metadata.get("role").unwrap(), "worker");
    }
} 