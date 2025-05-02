/*!
# ICN Identity System

This crate implements the identity system for the ICN Runtime, including DIDs,
Verifiable Credentials, TrustBundles, and ZK disclosure.

## Architectural Tenets
- Identity = Scoped DIDs (Coop/Community/Individual/Node/etc)
- DID-signed VCs; ZK Disclosure support
- Traceable reputation
- TrustBundles for federation anchoring
*/

use chrono::{DateTime, Utc};
use cid::Cid;
use multihash::{Code, MultihashDigest};
use rand::rngs::OsRng;
use rand::{RngCore, SeedableRng};
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Represents an identity ID (DID)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityId(pub String);

impl IdentityId {
    /// Create a new IdentityId from a DID string
    pub fn new(did: impl Into<String>) -> Self {
        Self(did.into())
    }
    
    /// Get the DID as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Represents a signature
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(pub Vec<u8>);

impl Signature {
    /// Create a new signature from bytes
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    
    /// Get the signature as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Simple DID resolver trait that will be expanded later
pub trait DIDResolver {
    /// Resolve a DID to its DID Document
    fn resolve(&self, did: &str) -> Result<serde_json::Value, String>;
}

/// Errors that can occur during identity operations
#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("Invalid DID: {0}")]
    InvalidDid(String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("Invalid credential: {0}")]
    InvalidCredential(String),
    
    #[error("Scope violation: {0}")]
    ScopeViolation(String),
    
    #[error("ZK verification failed: {0}")]
    ZkVerificationFailed(String),
    
    #[error("Keypair generation failed: {0}")]
    KeypairGenerationFailed(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Result type for identity operations
pub type IdentityResult<T> = Result<T, IdentityError>;

/// Scopes for identity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityScope {
    Individual,
    Cooperative,
    Community,
    Federation,
    Node,
    Guardian,
}

/// Keypair type used for operations (abstraction over the actual implementation)
pub struct KeyPair {
    /// The private key bytes
    private_key: Vec<u8>,
    /// The public key bytes
    public_key: Vec<u8>,
}

impl KeyPair {
    /// Create a new keypair from private and public key bytes
    pub fn new(private_key: Vec<u8>, public_key: Vec<u8>) -> Self {
        Self {
            private_key,
            public_key,
        }
    }
    
    /// Sign a message using the private key
    pub fn sign(&self, message: &[u8]) -> IdentityResult<Vec<u8>> {
        // This is a simplified implementation - in reality, the signature
        // would be created using specific cryptographic operations for the
        // chosen key type (e.g., Ed25519)
        
        // For now, we simulate signing by using the private key and message
        let mut hasher = Sha256::new();
        hasher.update(&self.private_key);
        hasher.update(message);
        let signature = hasher.finalize().to_vec();
        
        Ok(signature)
    }
    
    /// Get the public key bytes
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }
}

/// Generates a keypair for a DID
pub fn generate_did_keypair() -> IdentityResult<(String, KeyPair)> {
    // Generate a random seed
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    
    // Create a deterministic RNG from the seed
    let mut rng = StdRng::from_seed(seed);
    
    // Generate a private key
    let mut private_key = [0u8; 32];
    rng.fill_bytes(&mut private_key);
    
    // Derive a public key (simplified)
    let mut hasher = Sha256::new();
    hasher.update(private_key);
    let public_key = hasher.finalize().to_vec();
    
    // Create DID string in did:key format
    // This is simplified - in a real implementation we'd use actual
    // multicodec and multibase encoding
    let mut public_key_bytes = vec![0xed, 0x01]; // Ed25519 prefix
    public_key_bytes.extend_from_slice(&public_key);
    
    let did = format!("did:key:z{}", bs58::encode(public_key_bytes).into_string());
    
    // Create keypair
    let keypair = KeyPair::new(private_key.to_vec(), public_key);
    
    Ok((did, keypair))
}

/// Signs a message using an identity's keypair
pub fn sign_message(message: &[u8], keypair: &KeyPair) -> IdentityResult<Signature> {
    // Hash the message first with SHA-256
    let message_hash = Sha256::digest(message);
    
    // Sign the hash with the keypair
    let signature = keypair.sign(message_hash.as_slice())
        .map_err(|e| IdentityError::InvalidSignature(format!("Failed to sign message: {:?}", e)))?;
    
    Ok(Signature(signature))
}

/// Verifies a signature
pub fn verify_signature(message: &[u8], signature: &Signature, did: &IdentityId) -> IdentityResult<bool> {
    // In a real implementation, we would:
    // 1. Extract the public key from the DID string
    // 2. Verify the signature using the public key
    
    // This is a simplified implementation for the MVP
    // In a real implementation, we would properly validate
    // the signature cryptographically
    
    // Hash the message with SHA-256 (same as in sign_message)
    let _message_hash = Sha256::digest(message);
    
    // For now, just check that the DID and signature are not empty
    if did.0.is_empty() {
        return Err(IdentityError::InvalidDid("Empty DID".to_string()));
    }
    
    if signature.0.is_empty() {
        return Err(IdentityError::InvalidSignature("Empty signature".to_string()));
    }
    
    // Mock verification - in a real implementation this would cryptographically verify
    Ok(true)
}

/// Represents a verifiable credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableCredential {
    /// The context of the credential
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    
    /// The id of the credential
    pub id: String,
    
    /// The types of the credential
    #[serde(rename = "type")]
    pub types: Vec<String>,
    
    /// The issuer of the credential
    pub issuer: String,
    
    /// The issuance date of the credential
    pub issuanceDate: String,
    
    /// The subject of the credential
    pub credentialSubject: serde_json::Value,
    
    /// The proof of the credential (optional - for future JWS/ZK proofs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<serde_json::Value>,
    
    /// The expiration date of the credential (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expirationDate: Option<String>,
}

impl VerifiableCredential {
    /// Create a new verifiable credential
    pub fn new(
        types: Vec<String>,
        issuer: &IdentityId,
        subject_id: &IdentityId,
        claims: serde_json::Value,
    ) -> Self {
        // Create a subject with id and claims
        let mut subject_map = serde_json::Map::new();
        subject_map.insert("id".to_string(), serde_json::Value::String(subject_id.0.clone()));
        
        // Add all claims to the subject
        if let serde_json::Value::Object(claims_map) = claims {
            for (key, value) in claims_map {
                subject_map.insert(key, value);
            }
        }
        
        // Current timestamp in ISO 8601 format
        let now: DateTime<Utc> = Utc::now();
        let issuance_date = now.to_rfc3339();
        
        Self {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://icn.coop/credentials/v1".to_string(),
            ],
            id: format!("urn:uuid:{}", Uuid::new_v4()),
            types,
            issuer: issuer.0.clone(),
            issuanceDate: issuance_date,
            credentialSubject: serde_json::Value::Object(subject_map),
            proof: None,
            expirationDate: None,
        }
    }
    
    /// Set an expiration date for the credential
    pub fn with_expiration(mut self, expiration_date: DateTime<Utc>) -> Self {
        self.expirationDate = Some(expiration_date.to_rfc3339());
        self
    }
    
    /// Verify the credential (stub for future implementation)
    pub fn verify(&self) -> IdentityResult<bool> {
        // This is a stub - the actual implementation would:
        // 1. Verify the issuer's DID is valid
        // 2. Verify the credential hasn't expired
        // 3. Verify the proof if present
        
        // For now, just check basic validity
        if self.issuer.is_empty() {
            return Err(IdentityError::InvalidCredential("Issuer is empty".to_string()));
        }
        
        if let Some(exp_date) = &self.expirationDate {
            if let Ok(date) = DateTime::parse_from_rfc3339(exp_date) {
                if date < Utc::now() {
                    return Err(IdentityError::InvalidCredential("Credential has expired".to_string()));
                }
            }
        }
        
        Ok(true)
    }
}

/// Signs a credential
pub fn sign_credential(vc: VerifiableCredential, issuer_did: &str, _keypair: &KeyPair) -> IdentityResult<VerifiableCredential> {
    // Ensure the issuer matches the DID
    if vc.issuer != issuer_did {
        return Err(IdentityError::InvalidCredential("Issuer DID doesn't match credential issuer".to_string()));
    }
    
    // For MVP, we'll just return the VC with issuer, id, and issuanceDate set
    // In the future, this would add a proper JWS or ZK proof
    
    Ok(vc)
}

/// Represents a trust bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    /// The epoch ID of this trust bundle
    pub epoch_id: u64,
    
    /// The federation ID
    pub federation_id: String,
    
    /// The DAG roots in this trust bundle
    pub dag_roots: Vec<Cid>,
    
    /// The attestations in this trust bundle
    pub attestations: Vec<VerifiableCredential>,
    
    /// The signature of this trust bundle (optional - for future multi-sig from Guardians)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl TrustBundle {
    /// Create a new trust bundle
    pub fn new(
        epoch_id: u64,
        federation_id: String,
        dag_roots: Vec<Cid>,
        attestations: Vec<VerifiableCredential>,
    ) -> Self {
        Self {
            epoch_id,
            federation_id,
            dag_roots,
            attestations,
            signature: None,
        }
    }
    
    /// Verify the trust bundle (stub for future implementation)
    pub fn verify(&self) -> IdentityResult<bool> {
        // This is a stub - the actual implementation would:
        // 1. Verify each attestation
        // 2. Verify the signature if present
        
        // For now, just check basic validity
        if self.federation_id.is_empty() {
            return Err(IdentityError::InvalidCredential("Federation ID is empty".to_string()));
        }
        
        if self.dag_roots.is_empty() {
            return Err(IdentityError::InvalidCredential("DAG roots are empty".to_string()));
        }
        
        Ok(true)
    }
}

/// Represents an anchor subject
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorSubject {
    /// The epoch ID
    pub epoch_id: u64,
    
    /// The trust bundle CID
    pub trust_bundle_cid: Cid,
    
    /// Optional Guardian mandate reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate: Option<String>,
}

/// Represents an anchor credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorCredential {
    /// The ID of this anchor credential
    pub id: String,
    
    /// The issuer of this anchor credential
    pub issuer: String,
    
    /// The issuance date of this anchor credential
    pub issuanceDate: String,
    
    /// The subject of this anchor credential
    pub credentialSubject: AnchorSubject,
    
    /// The proof of this anchor credential (optional - for future JWS/ZK proofs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<serde_json::Value>,
}

impl AnchorCredential {
    /// Create a new anchor credential
    pub fn new(
        issuer: &IdentityId,
        epoch_id: u64,
        trust_bundle_cid: Cid,
        mandate: Option<String>,
    ) -> Self {
        // Current timestamp in ISO 8601 format
        let now: DateTime<Utc> = Utc::now();
        
        Self {
            id: format!("urn:uuid:{}", Uuid::new_v4()),
            issuer: issuer.0.clone(),
            issuanceDate: now.to_rfc3339(),
            credentialSubject: AnchorSubject {
                epoch_id,
                trust_bundle_cid,
                mandate,
            },
            proof: None,
        }
    }
    
    /// Verify the anchor credential (stub for future implementation)
    pub fn verify(&self) -> IdentityResult<bool> {
        // This is a stub - the actual implementation would:
        // 1. Verify the issuer's DID is valid
        // 2. Verify the credential hasn't expired
        // 3. Verify the proof if present
        
        // For now, just check basic validity
        if self.issuer.is_empty() {
            return Err(IdentityError::InvalidCredential("Issuer is empty".to_string()));
        }
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_did_keypair() {
        let result = generate_did_keypair();
        assert!(result.is_ok());
        
        let (did, _keypair) = result.unwrap();
        assert!(did.starts_with("did:key:z"));
    }
    
    #[test]
    fn test_sign_and_verify() {
        // Generate a keypair
        let (did, keypair) = generate_did_keypair().unwrap();
        let identity_id = IdentityId(did);
        
        // Sign a message
        let message = b"Hello, world!";
        let signature = sign_message(message, &keypair).unwrap();
        
        // Verify the signature
        let result = verify_signature(message, &signature, &identity_id).unwrap();
        assert!(result);
    }
    
    #[test]
    fn test_verifiable_credential() {
        // Generate identities for issuer and subject
        let (issuer_did, _) = generate_did_keypair().unwrap();
        let (subject_did, _) = generate_did_keypair().unwrap();
        
        let issuer_id = IdentityId(issuer_did);
        let subject_id = IdentityId(subject_did);
        
        // Create claims
        let claims = serde_json::json!({
            "name": "Test User",
            "role": "Developer"
        });
        
        // Create credential
        let vc = VerifiableCredential::new(
            vec!["VerifiableCredential".to_string(), "DeveloperCredential".to_string()],
            &issuer_id,
            &subject_id,
            claims,
        );
        
        // Verify basic fields
        assert_eq!(vc.types, vec!["VerifiableCredential".to_string(), "DeveloperCredential".to_string()]);
        assert_eq!(vc.issuer, issuer_id.0);
        
        // Check subject
        if let serde_json::Value::Object(subject) = &vc.credentialSubject {
            assert_eq!(subject.get("id").unwrap().as_str().unwrap(), subject_id.0);
            assert_eq!(subject.get("name").unwrap().as_str().unwrap(), "Test User");
            assert_eq!(subject.get("role").unwrap().as_str().unwrap(), "Developer");
        } else {
            panic!("Subject is not an object");
        }
        
        // Verify the credential
        assert!(vc.verify().is_ok());
    }
    
    #[test]
    fn test_anchor_credential() {
        // Generate identity for issuer
        let (issuer_did, _) = generate_did_keypair().unwrap();
        let issuer_id = IdentityId(issuer_did);
        
        // Create a sample CID
        let mh = Code::Sha2_256.digest(b"test");
        let cid = Cid::new_v1(0x55, mh);
        
        // Create anchor credential
        let anchor = AnchorCredential::new(
            &issuer_id,
            123, // epoch_id
            cid,
            Some("Guardian Mandate 1".to_string()),
        );
        
        // Verify basic fields
        assert_eq!(anchor.issuer, issuer_id.0);
        assert_eq!(anchor.credentialSubject.epoch_id, 123);
        assert_eq!(anchor.credentialSubject.trust_bundle_cid, cid);
        assert_eq!(anchor.credentialSubject.mandate, Some("Guardian Mandate 1".to_string()));
        
        // Verify the credential
        assert!(anchor.verify().is_ok());
    }
} 