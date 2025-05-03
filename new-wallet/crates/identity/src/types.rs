use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::error::{IdentityError, IdentityResult};
use ed25519_dalek::{Signature, Signer, Verifier, SigningKey, VerifyingKey};
use std::fmt::{self, Display, Formatter};

/// Scope for an identity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityScope {
    /// Individual person
    Individual,
    
    /// Community group
    Community,
    
    /// Cooperative organization
    Cooperative,
    
    /// Federation node
    Federation,
    
    /// Guardian role
    Guardian,
}

impl Display for IdentityScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IdentityScope::Individual => write!(f, "individual"),
            IdentityScope::Community => write!(f, "community"),
            IdentityScope::Cooperative => write!(f, "cooperative"),
            IdentityScope::Federation => write!(f, "federation"),
            IdentityScope::Guardian => write!(f, "guardian"),
        }
    }
}

impl TryFrom<&str> for IdentityScope {
    type Error = IdentityError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "individual" => Ok(IdentityScope::Individual),
            "community" => Ok(IdentityScope::Community),
            "cooperative" => Ok(IdentityScope::Cooperative),
            "federation" => Ok(IdentityScope::Federation),
            "guardian" => Ok(IdentityScope::Guardian),
            _ => Err(IdentityError::InvalidScope(format!("Unknown scope: {}", s))),
        }
    }
}

/// Decentralized Identity (DID)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Did {
    /// Full DID string, e.g. "did:icn:123456789abcdef"
    pub did_string: String,
    
    /// Method name, e.g. "icn"
    pub method: String,
    
    /// Method-specific identifier
    pub id: String,
}

impl Did {
    /// Create a new DID from its components
    pub fn new(method: &str, id: &str) -> Self {
        let did_string = format!("did:{}:{}", method, id);
        Self {
            did_string,
            method: method.to_string(),
            id: id.to_string(),
        }
    }
    
    /// Try to parse a DID string
    pub fn parse(did_string: &str) -> IdentityResult<Self> {
        let parts: Vec<&str> = did_string.split(':').collect();
        
        if parts.len() < 3 || parts[0] != "did" {
            return Err(IdentityError::InvalidDid(format!("Invalid DID format: {}", did_string)));
        }
        
        let method = parts[1].to_string();
        let id = parts[2..].join(":");
        
        Ok(Self {
            did_string: did_string.to_string(),
            method,
            id,
        })
    }
}

impl Display for Did {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.did_string)
    }
}

/// Identity wallet that manages a user's identity
#[derive(Clone, Serialize, Deserialize)]
pub struct IdentityWallet {
    /// Decentralized Identity (DID)
    pub did: Did,
    
    /// Human-readable name for the identity
    pub name: String,
    
    /// Scope of the identity
    pub scope: IdentityScope,
    
    /// When the identity was created
    pub created_at: DateTime<Utc>,
    
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    
    /// Public key for verification (serialized)
    pub public_key: String,
    
    /// Private key for signing (serialized, only for local identities)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
}

impl IdentityWallet {
    /// Create a new identity wallet with a generated key pair
    pub fn new(name: &str, scope: IdentityScope) -> IdentityResult<Self> {
        // Generate a new Ed25519 key pair
        let mut rng = rand::thread_rng();
        let signing_key = SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        
        // Generate the ID from the public key
        let public_key_bytes = verifying_key.to_bytes();
        let id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key_bytes);
        
        // Create the DID
        let did = Did::new("icn", &id);
        
        // Serialize keys
        let public_key = base64::engine::general_purpose::STANDARD.encode(public_key_bytes);
        let private_key = base64::engine::general_purpose::STANDARD.encode(signing_key.to_bytes());
        
        Ok(Self {
            did,
            name: name.to_string(),
            scope,
            created_at: Utc::now(),
            metadata: HashMap::new(),
            public_key,
            private_key: Some(private_key),
        })
    }
    
    /// Create an identity wallet from existing components
    pub fn from_components(
        did: Did,
        name: &str,
        scope: IdentityScope,
        public_key: &str,
        private_key: Option<&str>,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            did,
            name: name.to_string(),
            scope,
            created_at: Utc::now(),
            metadata,
            public_key: public_key.to_string(),
            private_key: private_key.map(|s| s.to_string()),
        }
    }
    
    /// Sign a message using the identity's private key
    pub fn sign(&self, message: &[u8]) -> IdentityResult<Signature> {
        let private_key = self.private_key.as_deref()
            .ok_or_else(|| IdentityError::KeyError("No private key available".to_string()))?;
            
        let key_bytes = base64::engine::general_purpose::STANDARD.decode(private_key)
            .map_err(|e| IdentityError::KeyError(format!("Failed to decode private key: {}", e)))?;
            
        let signing_key = SigningKey::from_bytes(key_bytes.as_slice().try_into()
            .map_err(|_| IdentityError::KeyError("Invalid private key length".to_string()))?);
            
        Ok(signing_key.sign(message))
    }
    
    /// Verify a signature against this identity's public key
    pub fn verify(&self, message: &[u8], signature: &Signature) -> IdentityResult<()> {
        let key_bytes = base64::engine::general_purpose::STANDARD.decode(&self.public_key)
            .map_err(|e| IdentityError::KeyError(format!("Failed to decode public key: {}", e)))?;
            
        let verifying_key = VerifyingKey::from_bytes(key_bytes.as_slice().try_into()
            .map_err(|_| IdentityError::KeyError("Invalid public key length".to_string()))?)?;
            
        verifying_key.verify(message, signature)
            .map_err(|e| IdentityError::VerificationFailed(format!("Signature verification failed: {}", e)))
    }
    
    /// Check if this wallet has a private key (can sign)
    pub fn can_sign(&self) -> bool {
        self.private_key.is_some()
    }
}

impl Debug for IdentityWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentityWallet")
            .field("did", &self.did)
            .field("name", &self.name)
            .field("scope", &self.scope)
            .field("created_at", &self.created_at)
            .field("metadata", &self.metadata)
            .field("public_key", &"[redacted]")
            .field("has_private_key", &self.private_key.is_some())
            .finish()
    }
}

use std::fmt::Debug; 