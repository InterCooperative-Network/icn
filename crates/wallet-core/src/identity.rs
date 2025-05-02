use serde::{Serialize, Deserialize};
use serde_json::Value;
use uuid::Uuid;
use base64::{Engine, engine::general_purpose};
use crate::crypto::KeyPair;
use crate::error::{WalletResult, WalletError};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityId {
    did: String,
}

impl IdentityId {
    pub fn new(method: &str, identifier: &str) -> Self {
        Self {
            did: format!("did:{}:{}", method, identifier),
        }
    }
    
    pub fn from_did(did: &str) -> WalletResult<Self> {
        if !did.starts_with("did:") {
            return Err(WalletError::InvalidDidFormat(format!("DID must start with 'did:': {}", did)));
        }
        
        let parts: Vec<&str> = did.split(':').collect();
        if parts.len() < 3 {
            return Err(WalletError::InvalidDidFormat(format!("Invalid DID format: {}", did)));
        }
        
        Ok(Self { did: did.to_string() })
    }
    
    pub fn to_string(&self) -> String {
        self.did.clone()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum IdentityScope {
    Personal,
    Organization,
    Device,
    Service,
    Custom(String),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IdentityWallet {
    pub did: IdentityId,
    pub keypair: KeyPair,
    pub scope: IdentityScope,
    pub metadata: Option<Value>,
}

impl IdentityWallet {
    pub fn new(scope: IdentityScope, metadata: Option<Value>) -> Self {
        let keypair = KeyPair::generate();
        let public_key_bytes = keypair.public_key_bytes();
        let identifier = general_purpose::URL_SAFE_NO_PAD.encode(public_key_bytes);
        let did = IdentityId::new("icn", &identifier);
        
        Self {
            did,
            keypair,
            scope,
            metadata,
        }
    }
    
    pub fn sign_message(&self, message: &[u8]) -> Vec<u8> {
        self.keypair.sign(message).to_bytes().to_vec()
    }
    
    pub fn verify_message(&self, message: &[u8], signature: &[u8]) -> WalletResult<bool> {
        let signature = ed25519_dalek::Signature::try_from(signature)
            .map_err(|e| WalletError::CryptoError(format!("Invalid signature: {}", e)))?;
            
        Ok(self.keypair.verify(message, &signature))
    }
    
    pub fn to_document(&self) -> Value {
        let mut doc = serde_json::json!({
            "@context": ["https://www.w3.org/ns/did/v1"],
            "id": self.did.to_string(),
            "verificationMethod": [{
                "id": format!("{}#keys-1", self.did.to_string()),
                "type": "Ed25519VerificationKey2020",
                "controller": self.did.to_string(),
                "publicKeyBase64": general_purpose::STANDARD.encode(self.keypair.public_key_bytes())
            }],
            "authentication": [format!("{}#keys-1", self.did.to_string())],
            "assertionMethod": [format!("{}#keys-1", self.did.to_string())]
        });
        
        if let Some(metadata) = &self.metadata {
            doc["metadata"] = metadata.clone();
        }
        
        match self.scope {
            IdentityScope::Personal => { doc["scope"] = serde_json::json!("personal"); }
            IdentityScope::Organization => { doc["scope"] = serde_json::json!("organization"); }
            IdentityScope::Device => { doc["scope"] = serde_json::json!("device"); }
            IdentityScope::Service => { doc["scope"] = serde_json::json!("service"); }
            IdentityScope::Custom(ref s) => { doc["scope"] = serde_json::json!(s); }
        }
        
        doc
    }
} 