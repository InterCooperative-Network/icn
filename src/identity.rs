use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use p256::ecdsa::{signature::Signer as EcdsaSigner, Signature, SigningKey as EcdsaSigningKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;
use base64::{Engine as _};

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