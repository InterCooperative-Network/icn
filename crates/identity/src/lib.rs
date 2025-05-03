use serde::{Serialize, Deserialize};
use thiserror::Error;
use ed25519_dalek::{Keypair, Signature, Signer, Verifier, PublicKey, SecretKey};
use rand::rngs::OsRng;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

/// Error type for identity operations
#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("Key generation error: {0}")]
    KeyGeneration(String),
    
    #[error("Signing error: {0}")]
    Signing(String),
    
    #[error("Verification error: {0}")]
    Verification(String),
    
    #[error("Encoding error: {0}")]
    Encoding(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Result type for identity operations
pub type IdentityResult<T> = Result<T, IdentityError>;

/// DID document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDocument {
    /// The DID identifier
    pub id: String,
    
    /// Controller DID
    pub controller: Option<String>,
    
    /// Public key in base64
    pub public_key: String,
    
    /// Verification methods
    pub verification_method: Vec<VerificationMethod>,
    
    /// Service endpoints
    pub service: Vec<Service>,
}

/// Verification method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    /// Verification method ID
    pub id: String,
    
    /// Verification method type
    #[serde(rename = "type")]
    pub method_type: String,
    
    /// Controller DID
    pub controller: String,
    
    /// Public key in base64
    pub public_key_base64: String,
}

/// Service endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    /// Service ID
    pub id: String,
    
    /// Service type
    #[serde(rename = "type")]
    pub service_type: String,
    
    /// Service endpoint URL
    pub service_endpoint: String,
}

/// Identity manager
pub struct IdentityManager {
    /// Active keypair
    keypair: Option<Keypair>,
    
    /// Active DID
    did: Option<String>,
    
    /// DID document
    did_document: Option<DidDocument>,
}

impl IdentityManager {
    /// Create a new identity manager
    pub fn new() -> Self {
        Self {
            keypair: None,
            did: None,
            did_document: None,
        }
    }
    
    /// Generate a new identity
    pub fn generate_identity(&mut self) -> IdentityResult<String> {
        // Generate a keypair
        let mut csprng = OsRng;
        let keypair = Keypair::generate(&mut csprng);
        
        // Create a DID
        let public_key_bytes = keypair.public.as_bytes();
        let public_key_b64 = BASE64.encode(public_key_bytes);
        let did = format!("did:icn:{}", &public_key_b64[0..16]);
        
        // Create verification method
        let verification_method = VerificationMethod {
            id: format!("{}#keys-1", &did),
            method_type: "Ed25519VerificationKey2020".to_string(),
            controller: did.clone(),
            public_key_base64: public_key_b64.clone(),
        };
        
        // Create DID document
        let did_document = DidDocument {
            id: did.clone(),
            controller: None,
            public_key: public_key_b64,
            verification_method: vec![verification_method],
            service: vec![],
        };
        
        // Store the identity
        self.keypair = Some(keypair);
        self.did = Some(did.clone());
        self.did_document = Some(did_document);
        
        Ok(did)
    }
    
    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> IdentityResult<String> {
        let keypair = self.keypair.as_ref().ok_or_else(|| {
            IdentityError::Signing("No keypair available".to_string())
        })?;
        
        let signature = keypair.sign(message);
        let signature_b64 = BASE64.encode(signature.to_bytes());
        
        Ok(signature_b64)
    }
    
    /// Verify a signature
    pub fn verify(&self, message: &[u8], signature_b64: &str) -> IdentityResult<bool> {
        let keypair = self.keypair.as_ref().ok_or_else(|| {
            IdentityError::Verification("No keypair available".to_string())
        })?;
        
        let signature_bytes = BASE64.decode(signature_b64).map_err(|e| {
            IdentityError::Encoding(format!("Failed to decode signature: {}", e))
        })?;
        
        let signature = Signature::from_bytes(&signature_bytes).map_err(|e| {
            IdentityError::Verification(format!("Invalid signature: {}", e))
        })?;
        
        match keypair.public.verify(message, &signature) {
            Ok(_) => Ok(true),
            Err(e) => {
                // Return false for verification failure rather than an error
                Ok(false)
            }
        }
    }
    
    /// Get the current DID
    pub fn get_did(&self) -> Option<String> {
        self.did.clone()
    }
    
    /// Get the DID document
    pub fn get_did_document(&self) -> Option<DidDocument> {
        self.did_document.clone()
    }
} 