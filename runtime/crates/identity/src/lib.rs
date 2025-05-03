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

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use cid::Cid;
use rand::{rngs::OsRng, rngs::StdRng, SeedableRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json;
use sha2::{Sha256, Digest};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ssi::jwk::{Algorithm, JWK};
use ssi::did::DIDMethod;
use ssi::did_resolve::{DIDResolver as SsiResolver, ResolutionInputMetadata, ResolutionMetadata, DocumentMetadata};
use std::collections::HashMap;
use std::sync::{Arc, Mutex}; // Using Mutex for simple in-memory storage for now

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

impl fmt::Display for IdentityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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
    
    #[error("Key storage error: {0}")]
    KeyStorageError(String),
    
    #[error("Metadata storage error: {0}")]
    MetadataStorageError(String),
    
    #[error("DID resolution error: {0}")]
    DidResolutionError(String),
    
    #[error("Internal error: {0}")]
    InternalError(#[from] anyhow::Error), // Allow conversion from anyhow
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

/// Defines the interface for storing and retrieving cryptographic keys.
#[async_trait]
pub trait KeyStorage: Send + Sync {
    /// Stores a JWK securely, associated with a DID.
    async fn store_key(&self, did: &str, key: &JWK) -> Result<()>;
    /// Retrieves a JWK associated with a DID.
    async fn retrieve_key(&self, did: &str) -> Result<Option<JWK>>;
    /// Deletes a key associated with a DID.
    async fn delete_key(&self, did: &str) -> Result<()>;
}

/// Defines the interface for storing entity metadata.
#[async_trait]
pub trait MetadataStorage: Send + Sync {
    /// Stores metadata associated with a newly created entity.
    async fn store_entity_metadata(
        &self,
        entity_did: &str,
        parent_did: Option<&str>,
        genesis_cid: &Cid,
        entity_type: &str, // e.g., "Cooperative", "Community"
        metadata: Option<serde_json::Value>, // Optional extra metadata
    ) -> Result<()>;

    /// Retrieves metadata for a given entity DID.
    async fn retrieve_entity_metadata(&self, entity_did: &str) -> Result<Option<EntityMetadata>>;

    // Potentially add methods to query relationships, e.g., find children of a parent DID.
}

/// Represents stored metadata about an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMetadata {
    pub entity_did: String,
    pub parent_did: Option<String>,
    pub genesis_cid: String, // Store as string for easier serialization
    pub entity_type: String,
    pub creation_timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Manages identity creation, key storage, and metadata registration.
#[async_trait]
pub trait IdentityManager: Send + Sync {
    /// Generates a new Ed25519 keypair, derives the corresponding `did:key`,
    /// stores the keypair securely, and returns the DID string and public JWK.
    async fn generate_and_store_did_key(&self) -> Result<(String, JWK)>;

    /// Registers metadata about a newly created entity, linking its DID
    /// to its parent (if applicable), genesis node, and type.
    async fn register_entity_metadata(
        &self,
        entity_did: &str,
        parent_did: Option<&str>, // Optional: The DID of the parent entity (e.g., Federation)
        genesis_cid: &Cid,
        entity_type: &str, // e.g., "Cooperative", "Community"
        metadata: Option<serde_json::Value>, // Optional extra metadata
    ) -> Result<()>;

    /// Retrieves the JWK associated with a DID.
    async fn get_key(&self, did: &str) -> Result<Option<JWK>>;

    /// Retrieves the metadata associated with an entity DID.
    async fn get_entity_metadata(&self, did: &str) -> Result<Option<EntityMetadata>>;

     /// Resolve a DID using the ssi library's resolver trait.
     /// This might involve looking up keys in KeyStorage or using other resolution methods.
     async fn resolve_did(&self, did: &str) -> Result<(ResolutionMetadata, Option<serde_json::Value>, Option<DocumentMetadata>)>;
}

// --- Concrete Implementations (using simple in-memory storage for now) ---

/// Simple in-memory key storage using Mutex-protected HashMap.
#[derive(Debug, Default)]
pub struct InMemoryKeyStorage {
    keys: Mutex<HashMap<String, JWK>>,
}

#[async_trait]
impl KeyStorage for InMemoryKeyStorage {
    async fn store_key(&self, did: &str, key: &JWK) -> Result<()> {
        let mut keys = self.keys.lock().map_err(|_| anyhow!("Failed to lock key storage"))?;
        keys.insert(did.to_string(), key.clone());
        Ok(())
    }

    async fn retrieve_key(&self, did: &str) -> Result<Option<JWK>> {
        let keys = self.keys.lock().map_err(|_| anyhow!("Failed to lock key storage"))?;
        Ok(keys.get(did).cloned())
    }

     async fn delete_key(&self, did: &str) -> Result<()> {
        let mut keys = self.keys.lock().map_err(|_| anyhow!("Failed to lock key storage"))?;
        keys.remove(did);
        Ok(())
    }
}

/// Simple in-memory metadata storage using Mutex-protected HashMap.
#[derive(Debug, Default)]
pub struct InMemoryMetadataStorage {
    metadata: Mutex<HashMap<String, EntityMetadata>>,
}

#[async_trait]
impl MetadataStorage for InMemoryMetadataStorage {
    async fn store_entity_metadata(
        &self,
        entity_did: &str,
        parent_did: Option<&str>,
        genesis_cid: &Cid,
        entity_type: &str,
        metadata_val: Option<serde_json::Value>,
    ) -> Result<()> {
        let mut store = self.metadata.lock().map_err(|_| anyhow!("Failed to lock metadata storage"))?;
        let metadata_entry = EntityMetadata {
            entity_did: entity_did.to_string(),
            parent_did: parent_did.map(String::from),
            genesis_cid: genesis_cid.to_string(),
            entity_type: entity_type.to_string(),
            creation_timestamp: Utc::now(),
            metadata: metadata_val,
        };
        store.insert(entity_did.to_string(), metadata_entry);
        Ok(())
    }

    async fn retrieve_entity_metadata(&self, entity_did: &str) -> Result<Option<EntityMetadata>> {
        let store = self.metadata.lock().map_err(|_| anyhow!("Failed to lock metadata storage"))?;
        Ok(store.get(entity_did).cloned())
    }
}

/// Concrete implementation of IdentityManager using provided storage backends.
pub struct ConcreteIdentityManager {
    key_storage: Arc<dyn KeyStorage>,
    metadata_storage: Arc<dyn MetadataStorage>,
    did_method: ssi::did_key::DidKey, // Use ssi's did:key implementation
}

impl ConcreteIdentityManager {
    pub fn new(
        key_storage: Arc<dyn KeyStorage>,
        metadata_storage: Arc<dyn MetadataStorage>,
    ) -> Self {
        Self {
            key_storage,
            metadata_storage,
            did_method: ssi::did_key::DidKey {},
        }
    }
}

#[async_trait]
impl IdentityManager for ConcreteIdentityManager {
    async fn generate_and_store_did_key(&self) -> Result<(String, JWK)> {
        // 1. Generate Ed25519 keypair using ssi
        //    Note: ssi JWK generation often includes private key params (d).
        let keypair_jwk = JWK::generate_ed25519().map_err(|e| {
            IdentityError::KeypairGenerationFailed(format!("Failed to generate Ed25519 key: {}", e))
        })?;

        // 2. Derive the did:key string from the public key part
        let did_key_str = self.did_method.generate(&keypair_jwk).ok_or_else(|| {
            IdentityError::InvalidDid("Failed to generate did:key string from JWK".to_string())
        })?;

        // 3. Securely store the full keypair (including private parts)
        //    The KeyStorage trait needs to handle this securely.
        //    For InMemoryKeyStorage, it's just stored directly.
        self.key_storage.store_key(&did_key_str, &keypair_jwk).await?;

        // 4. Extract the public key JWK to return (remove private key 'd' parameter)
        let mut public_jwk = keypair_jwk.clone();
        public_jwk.params.d = None; // Ensure private key material is not returned

        Ok((did_key_str, public_jwk))
    }

    async fn register_entity_metadata(
        &self,
        entity_did: &str,
        parent_did: Option<&str>,
        genesis_cid: &Cid,
        entity_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        self.metadata_storage
            .store_entity_metadata(entity_did, parent_did, genesis_cid, entity_type, metadata)
            .await
    }

    async fn get_key(&self, did: &str) -> Result<Option<JWK>> {
        self.key_storage.retrieve_key(did).await
    }

    async fn get_entity_metadata(&self, did: &str) -> Result<Option<EntityMetadata>> {
        self.metadata_storage.retrieve_entity_metadata(did).await
    }

     /// Simple DID resolver implementation for did:key using the KeyStorage
     async fn resolve_did(&self, did: &str) -> Result<(ResolutionMetadata, Option<serde_json::Value>, Option<DocumentMetadata>)> {
         if !did.starts_with("did:key:") {
            // For now, only handle did:key. Could extend later.
            return Ok((
                ResolutionMetadata {
                    error: Some("unsupportedDidMethod".to_string()),
                    ..Default::default()
                },
                None, None
            ));
        }

        // Attempt to resolve using ssi's did:key method directly
        // This derives the public key from the DID string itself.
        let resolution_result = self.did_method.resolve(did, &ResolutionInputMetadata::default()).await;

        // We don't need to look up in KeyStorage for *resolving* did:key,
        // as the public key is embedded. KeyStorage is for holding private keys
        // for *signing*.

         match resolution_result {
            (res_meta, Some(doc), Some(doc_meta)) => Ok((res_meta, Some(doc.to_value()?), Some(doc_meta))),
            (res_meta, None, None) => Ok((res_meta, None, None)),
            _ => Err(anyhow!("Unexpected resolution result format for did:key")),
        }
     }
}

/// Represents a verifiable credential
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiableCredential {
    /// The ID of this credential
    pub id: String,
    
    /// The type(s) of this credential
    #[serde(rename = "type")]
    pub credential_type: Vec<String>,
    
    /// The issuer of this credential
    pub issuer: String,
    
    /// The issuance date of this credential
    pub issuanceDate: String,
    
    /// The subject of this credential
    pub credentialSubject: serde_json::Value,
    
    /// The proof of this credential (optional - for future JWS/ZK proofs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<serde_json::Value>,
    
    /// The expiration date of this credential (optional)
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
            id: format!("urn:uuid:{}", Uuid::new_v4()),
            credential_type: types,
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
    
    /// Verify the credential
    pub async fn verify(&self) -> IdentityResult<bool> {
        // Basic validation checks
        if self.issuer.is_empty() {
            return Err(IdentityError::InvalidCredential("Issuer is empty".to_string()));
        }
        
        // Check expiration
        if let Some(exp_date) = &self.expirationDate {
            if let Ok(date) = DateTime::parse_from_rfc3339(exp_date) {
                if date < Utc::now() {
                    return Err(IdentityError::InvalidCredential("Credential has expired".to_string()));
                }
            }
        }
        
        // If there's no proof, we can't verify signature
        if self.proof.is_none() {
            return Ok(false);
        }
        
        // Extract and validate proof
        let proof = self.proof.as_ref().unwrap();
        
        // Verify proof type
        let proof_type = proof.get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| IdentityError::InvalidCredential("Missing proof type".to_string()))?;
        
        if proof_type != "JsonWebSignature2020" {
            return Err(IdentityError::InvalidCredential(
                format!("Unsupported proof type: {}", proof_type)
            ));
        }
        
        // Extract verification method and JWS
        let verification_method = proof.get("verificationMethod")
            .and_then(|vm| vm.as_str())
            .ok_or_else(|| IdentityError::InvalidCredential("Missing verification method".to_string()))?;
        
        let jws = proof.get("jws")
            .and_then(|j| j.as_str())
            .ok_or_else(|| IdentityError::InvalidCredential("Missing JWS".to_string()))?;
        
        // Parse the JWS
        let jws_parts: Vec<&str> = jws.split('.').collect();
        if jws_parts.len() != 3 {
            return Err(IdentityError::InvalidCredential("Invalid JWS format".to_string()));
        }
        
        let (header_b64, payload_b64, signature_b64) = (jws_parts[0], jws_parts[1], jws_parts[2]);
        
        // Extract the DID from the verification method
        // Format is usually "did:method:id#keyId"
        let did_parts: Vec<&str> = verification_method.split('#').collect();
        if did_parts.is_empty() {
            return Err(IdentityError::InvalidDid(
                format!("Invalid verification method: {}", verification_method)
            ));
        }
        
        let did = did_parts[0];
        
        // In a production system, we'd resolve the DID to get the public key
        // For now, we'll use a simplified approach for the MVP
        
        // First, extract the multibase-encoded public key from the did:key
        // Format is did:key:z{base58_encoded_key}
        if !did.starts_with("did:key:z") {
            return Err(IdentityError::InvalidDid(
                format!("Only did:key method is supported at this time: {}", did)
            ));
        }
        
        // Extract the multibase encoded key
        let key_bytes = bs58::decode(&did[9..])
            .into_vec()
            .map_err(|e| IdentityError::InvalidDid(
                format!("Failed to decode key from DID: {}", e)
            ))?;
        
        // The first two bytes are the multicodec prefix for Ed25519 (0xed01)
        // The rest is the actual public key
        if key_bytes.len() < 3 {
            return Err(IdentityError::InvalidDid("Key bytes too short".to_string()));
        }
        
        let public_key = &key_bytes[2..];
        
        // Decode the signature from base64
        let signature_bytes = URL_SAFE_NO_PAD.decode(signature_b64)
            .map_err(|e| IdentityError::VerificationError(format!("Failed to decode signature: {}", e)))?;
        
        // Create a signature object
        let signature = Signature::new(signature_bytes);
        
        // Reconstruct signing input (header.payload)
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        
        // Create an identity ID from the DID
        let identity_id = IdentityId::new(did);
        
        // Verify the signature
        verify_signature(signing_input.as_bytes(), &signature, &identity_id)
    }
}

/// Signs a credential
pub async fn sign_credential(vc_data: VerifiableCredential, keypair: &KeyPair) -> IdentityResult<VerifiableCredential> {
    // Clone VC data to avoid modifying the original
    let mut vc_to_sign = vc_data.clone();
    
    // Ensure we remove any existing proof before signing
    vc_to_sign.proof = None;
    
    // Serialize the credential to canonical JSON
    let payload_bytes = serde_json::to_vec(&vc_to_sign)
        .map_err(|e| IdentityError::SerializationError(format!("Failed to serialize credential: {}", e)))?;
    
    // Extract DID from issuer field - this should be in the format "did:key:..."
    let issuer_did = &vc_to_sign.issuer;
    if !issuer_did.starts_with("did:") {
        return Err(IdentityError::InvalidDid(format!("Invalid issuer DID format: {}", issuer_did)));
    }
    
    // Simple JWS implementation
    // Create a header
    let header = serde_json::json!({
        "alg": "EdDSA",
        "typ": "JWT",
        "kid": format!("{}#key1", issuer_did),
    });
    
    // Base64url encode the header
    let header_encoded = URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&header)
            .map_err(|e| IdentityError::SerializationError(format!("Failed to serialize header: {}", e)))?
    );
    
    // Base64url encode the payload
    let payload_encoded = URL_SAFE_NO_PAD.encode(&payload_bytes);
    
    // Create the signing input (header.payload)
    let signing_input = format!("{}.{}", header_encoded, payload_encoded);
    
    // Sign the input string
    let signature = sign_message(signing_input.as_bytes(), keypair)?;
    
    // Base64url encode the signature
    let signature_encoded = URL_SAFE_NO_PAD.encode(signature.as_bytes());
    
    // Create the complete JWS (header.payload.signature)
    let jws = format!("{}.{}.{}", header_encoded, payload_encoded, signature_encoded);
    
    // Create JSON-LD proof object
    let proof = serde_json::json!({
        "type": "JsonWebSignature2020",
        "created": Utc::now().to_rfc3339(),
        "proofPurpose": "assertionMethod",
        "verificationMethod": format!("{}#key1", issuer_did),
        "jws": jws
    });
    
    // Attach proof to the credential
    vc_to_sign.proof = Some(proof);
    
    Ok(vc_to_sign)
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

/// Signs a message using an identity's keypair
pub fn sign_message(message: &[u8], keypair: &KeyPair) -> IdentityResult<Signature> {
    // Hash the message first with SHA-256
    let message_hash = Sha256::digest(message);
    
    // Sign the hash with the keypair
    let signature = keypair.sign(message_hash.as_slice())
        .map_err(|e| IdentityError::InvalidSignature(format!("Failed to sign message: {:?}", e)))?;
    
    Ok(Signature(signature))
}

/// Quorum proof that can be verified
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuorumProof {
    /// Signatures collected (Signer DID, Signature over content hash)
    pub votes: Vec<(IdentityId, Signature)>,
    
    /// The quorum configuration that must be met
    pub config: QuorumConfig,
}

/// Quorum configuration types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuorumConfig {
    /// Simple majority
    Majority,
    
    /// Threshold-based (percentage 0-100)
    Threshold(u8),
    
    /// Weighted votes with total required weight
    Weighted(Vec<(IdentityId, u32)>, u32),
}

impl QuorumProof {
    /// Verify that the quorum proof contains sufficient valid signatures according to the config
    /// 
    /// Only signatures from authorized guardians are counted towards meeting the quorum requirements.
    pub async fn verify(&self, content_hash: &[u8], authorized_guardians: &[IdentityId]) -> IdentityResult<bool> {
        let mut valid_signatures = 0u32;
        let mut weighted_sum = 0u32;
        let total_votes = self.votes.len() as u32;
        
        // Keep track of which DIDs have already provided a valid signature
        // This helps prevent duplicate signatures from the same DID
        let mut verified_dids = std::collections::HashSet::new();
        
        // Calculate total possible weight for Weighted quorum
        let _total_possible_weight = match &self.config {
            QuorumConfig::Weighted(weights, _) => {
                weights.iter().map(|(_, weight)| *weight).sum()
            },
            _ => 0u32
        };
        
        for (signer_did, signature) in &self.votes {
            // Prevent duplicate signatures from the same DID
            if verified_dids.contains(&signer_did.0) {
                tracing::warn!("Duplicate signature from DID {} detected and ignored", signer_did.0);
                continue;
            }
            
            // Check if the signer is an authorized guardian
            if !authorized_guardians.contains(signer_did) {
                tracing::warn!("Signature from unauthorized DID ({}) ignored in quorum proof", signer_did.0);
                continue;
            }
            
            match verify_signature(content_hash, signature, signer_did) {
                Ok(true) => {
                    valid_signatures += 1;
                    verified_dids.insert(signer_did.0.clone());
                    
                    // Handle weighted logic if applicable
                    if let QuorumConfig::Weighted(weights, _) = &self.config {
                        if let Some((_, weight)) = weights.iter().find(|(id, _)| id == signer_did) {
                            weighted_sum += *weight;
                        }
                    }
                }
                Ok(false) => {
                    tracing::warn!("Invalid signature found in quorum proof for DID: {}", signer_did.0);
                }
                Err(e) => {
                    tracing::error!("Error verifying signature for DID {}: {}", signer_did.0, e);
                    return Err(IdentityError::VerificationError(format!("Signature verification error: {}", e)));
                }
            }
        }
        
        // Check against quorum config
        let result = match &self.config {
            QuorumConfig::Majority => {
                // Simple majority of provided votes - must have more than half of valid signatures
                valid_signatures * 2 > total_votes
            },
            QuorumConfig::Threshold(threshold_percentage) => {
                // Ensure threshold is in valid range (0-100)
                let percentage = (*threshold_percentage).min(100) as f32 / 100.0;
                // Calculate the threshold count as a percentage of total votes
                let threshold_count = (total_votes as f32 * percentage).ceil() as u32;
                valid_signatures >= threshold_count
            },
            QuorumConfig::Weighted(_, required_weight) => {
                weighted_sum >= *required_weight
            },
        };
        
        tracing::debug!(
            "Quorum verification result: {} (valid: {}, total: {}, config: {:?})",
            result, valid_signatures, total_votes, self.config
        );
        
        Ok(result)
    }
}

/// Represents a trust bundle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBundle {
    /// The epoch ID of this trust bundle
    pub epoch_id: u64,
    
    /// The federation ID
    pub federation_id: String,
    
    /// The DAG roots in this trust bundle
    pub dag_roots: Vec<Cid>,
    
    /// The attestations in this trust bundle
    pub attestations: Vec<VerifiableCredential>,
    
    /// The proof of this trust bundle (optional - for quorum validation and signature verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<QuorumProof>,
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
            proof: None,
        }
    }
    
    /// Calculate a consistent hash for the trust bundle content
    /// 
    /// This provides a standardized way to create a hash over the trust bundle content
    /// for signing and verification purposes.
    pub fn calculate_hash(&self) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        
        // Ensure we hash all elements in a consistent order
        // Note: We don't include the proof field in the hash calculation since the proof
        // is created after the hash and would create a circular dependency
        
        // Hash the epoch_id
        let epoch_bytes = self.epoch_id.to_be_bytes();
        hasher.update(&epoch_bytes);
        
        // Hash the federation_id
        hasher.update(self.federation_id.as_bytes());
        
        // Hash each DAG root CID in order
        for cid in &self.dag_roots {
            hasher.update(cid.to_bytes());
        }
        
        // Hash each attestation in order
        for attestation in &self.attestations {
            // Serialize the attestation to JSON and hash the resulting bytes
            if let Ok(att_bytes) = serde_json::to_vec(attestation) {
                hasher.update(&att_bytes);
            }
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        
        hash
    }
    
    /// Verify the trust bundle
    /// 
    /// Validates the bundle's contents and verifies the quorum proof against the provided
    /// list of authorized guardians for the federation.
    pub async fn verify(&self, authorized_guardians: &[IdentityId]) -> IdentityResult<bool> {
        // Check basic validity
        if self.federation_id.is_empty() {
            return Err(IdentityError::InvalidCredential("Federation ID is empty".to_string()));
        }
        
        if self.dag_roots.is_empty() {
            return Err(IdentityError::InvalidCredential("DAG roots are empty".to_string()));
        }
        
        // Verify each attestation
        for attestation in &self.attestations {
            // Skip verification for attestations without proofs for now
            // In a production system, we might want to require proofs on all attestations
            if attestation.proof.is_none() {
                continue;
            }
            
            if !attestation.verify().await? {
                return Err(IdentityError::InvalidCredential(
                    format!("Invalid attestation in trust bundle: {}", attestation.id)
                ));
            }
        }
        
        // Verify the proof if present
        if let Some(proof) = &self.proof {
            // Calculate the hash of the bundle
            let bundle_hash = self.calculate_hash();
            
            // Verify the quorum proof with the provided authorized guardians
            proof.verify(&bundle_hash, authorized_guardians).await
        } else {
            // For full validation, a proof is required
            Err(IdentityError::VerificationError("Missing proof in TrustBundle".to_string()))
        }
    }

    /// Count the number of nodes with a specific role in this trust bundle
    /// 
    /// This method examines attestations and counts nodes with matching roles.
    /// It assumes that node roles are included in the credentialSubject of each attestation
    /// with a "role" field.
    pub fn count_nodes_by_role(&self, role: &str) -> usize {
        let mut count = 0;
        
        for attestation in &self.attestations {
            // Skip malformed attestations
            if !attestation.credentialSubject.is_object() {
                continue;
            }
            
            let subject = attestation.credentialSubject.as_object().unwrap();
            
            // If there's a "role" field matching our target role, count this node
            if let Some(node_role) = subject.get("role") {
                if let Some(role_str) = node_role.as_str() {
                    if role_str.to_lowercase() == role.to_lowercase() {
                        count += 1;
                    }
                }
            }
        }
        
        count
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
    use std::sync::Arc;

    // Helper to create a test IdentityManager instance
    fn test_identity_manager() -> Arc<dyn IdentityManager> {
        Arc::new(ConcreteIdentityManager::new(
            Arc::new(InMemoryKeyStorage::default()),
            Arc::new(InMemoryMetadataStorage::default()),
        ))
    }

    #[tokio::test]
    async fn test_generate_and_store_did_key() {
        let manager = test_identity_manager();
        let result = manager.generate_and_store_did_key().await;

        assert!(result.is_ok());
        let (did_key, public_jwk) = result.unwrap();

        // Check DID format
        assert!(did_key.starts_with("did:key:z"));

        // Check public JWK properties
        assert_eq!(public_jwk.key_type.as_deref(), Some("OKP")); // OKP for Ed25519/X25519
        assert_eq!(public_jwk.params.curve.as_deref(), Some("Ed25519"));
        assert!(public_jwk.params.x.is_some()); // Public key param 'x' should exist
        assert!(public_jwk.params.d.is_none()); // Private key param 'd' should NOT exist

        // Verify key was stored
        let stored_key = manager.get_key(&did_key).await.unwrap();
        assert!(stored_key.is_some());
        let stored_jwk = stored_key.unwrap();

        // Stored key SHOULD contain private part ('d')
        assert!(stored_jwk.params.d.is_some());
        assert_eq!(stored_jwk.params.x, public_jwk.params.x); // Public parts should match
    }

    #[tokio::test]
    async fn test_register_and_retrieve_metadata() {
        let manager = test_identity_manager();
        let (entity_did, _) = manager.generate_and_store_did_key().await.unwrap();
        let (parent_did, _) = manager.generate_and_store_did_key().await.unwrap();

        // Create a dummy CID
        let data = b"genesis";
        let digest = Sha256::digest(data);
        let mh = cid::multihash::Multihash::wrap(0x12, &digest).unwrap();
        let genesis_cid = Cid::new_v1(0x55, mh); // raw codec

        let entity_type = "Cooperative";
        let extra_meta = serde_json::json!({ "name": "Test Coop" });

        // Register metadata
        let register_result = manager.register_entity_metadata(
            &entity_did,
            Some(&parent_did),
            &genesis_cid,
            entity_type,
            Some(extra_meta.clone())
        ).await;
        assert!(register_result.is_ok());

        // Retrieve metadata
        let retrieved_meta_opt = manager.get_entity_metadata(&entity_did).await.unwrap();
        assert!(retrieved_meta_opt.is_some());
        let retrieved_meta = retrieved_meta_opt.unwrap();

        assert_eq!(retrieved_meta.entity_did, entity_did);
        assert_eq!(retrieved_meta.parent_did, Some(parent_did));
        assert_eq!(retrieved_meta.genesis_cid, genesis_cid.to_string());
        assert_eq!(retrieved_meta.entity_type, entity_type);
        assert!(retrieved_meta.creation_timestamp <= Utc::now());
        assert_eq!(retrieved_meta.metadata, Some(extra_meta));

         // Retrieve non-existent metadata
        let non_existent_meta = manager.get_entity_metadata("did:key:zNonExistent").await.unwrap();
        assert!(non_existent_meta.is_none());
    }

    #[tokio::test]
    async fn test_did_key_resolution() {
         let manager = test_identity_manager();
         let (did_key, _) = manager.generate_and_store_did_key().await.unwrap();

         let result = manager.resolve_did(&did_key).await;
         assert!(result.is_ok(), "Resolution failed: {:?}", result.err());

         let (res_meta, doc_opt, _doc_meta_opt) = result.unwrap();

         assert!(res_meta.error.is_none(), "Resolution returned error: {:?}", res_meta.error);
         assert!(doc_opt.is_some(), "DID Document was not returned");

         let doc = doc_opt.unwrap();
         println!("Resolved DID Document: {}", serde_json::to_string_pretty(&doc).unwrap());

         // Basic checks on the resolved document
         assert_eq!(doc.get("id").and_then(|v| v.as_str()), Some(did_key.as_str()));
         assert!(doc.get("verificationMethod").and_then(|v| v.as_array()).is_some());
         assert!(doc.get("authentication").and_then(|v| v.as_array()).is_some());
         // ... add more checks as needed based on ssi's did:key document structure
    }

     #[tokio::test]
     async fn test_unsupported_did_resolution() {
         let manager = test_identity_manager();
         let result = manager.resolve_did("did:example:123").await;
         assert!(result.is_ok()); // Resolution itself doesn't fail, but metadata indicates error

         let (res_meta, doc_opt, doc_meta_opt) = result.unwrap();
         assert_eq!(res_meta.error, Some("unsupportedDidMethod".to_string()));
         assert!(doc_opt.is_none());
         assert!(doc_meta_opt.is_none());
     }

    // TODO: Update other tests (sign/verify, VC, TrustBundle) to use the new manager
    // and proper crypto operations. The existing tests are likely broken now.
} 