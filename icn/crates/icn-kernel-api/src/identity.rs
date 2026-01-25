//! Identity Primitive
//!
//! Provides DID operations, signing, verification, and keystore access.
//!
//! # Design
//!
//! The identity primitive handles the "who" of the system without
//! interpreting what that identity means. Membership, trust, and
//! reputation are app-layer concerns.
//!
//! # Non-Goals
//!
//! - Credential semantics (what credentials mean)
//! - Membership (who's in an org)
//! - Trust/reputation scores
//! - Steward ceremonies (that's an app)

use crate::types::{Did, KeyId, Signature};

/// DID method for creating identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DidMethod {
    /// ICN-native DID method (did:icn:...)
    Icn,
    /// Key-based DID (did:key:...)
    Key,
    /// Web-based DID (did:web:...) - requires network resolution
    Web,
}

/// DID Document containing public keys and service endpoints.
#[derive(Clone, Debug)]
pub struct DidDocument {
    /// The DID this document describes
    pub id: Did,
    /// Verification methods (public keys)
    pub verification_methods: Vec<VerificationMethod>,
    /// Service endpoints
    pub services: Vec<ServiceEndpoint>,
    /// Controller DIDs (who can update this document)
    pub controllers: Vec<Did>,
    /// Document creation timestamp
    pub created: u64,
    /// Document last updated timestamp
    pub updated: u64,
}

/// Verification method (public key) in a DID document.
#[derive(Clone, Debug)]
pub struct VerificationMethod {
    /// Unique identifier for this key
    pub id: String,
    /// Key type (e.g., "Ed25519VerificationKey2020")
    pub key_type: String,
    /// Controller of this key
    pub controller: Did,
    /// Public key material (multibase encoded)
    pub public_key_multibase: String,
}

/// Service endpoint in a DID document.
#[derive(Clone, Debug)]
pub struct ServiceEndpoint {
    /// Unique identifier for this service
    pub id: String,
    /// Service type (e.g., "ICNNode", "CredentialRegistry")
    pub service_type: String,
    /// Service endpoint URI
    pub service_endpoint: String,
}

/// Bundle of keys associated with an identity.
#[derive(Clone, Debug)]
pub struct KeyBundle {
    /// Primary signing key
    pub signing_key: KeyId,
    /// Key for key agreement (X25519)
    pub key_agreement: Option<KeyId>,
    /// Additional keys for specific purposes
    pub additional_keys: Vec<(String, KeyId)>,
}

/// Core identity operations.
///
/// This trait defines the kernel's identity capabilities.
/// Implementations may use different key storage backends
/// (file, TPM, HSM, etc.).
pub trait IdentityService: Send + Sync {
    /// Create a new DID with the specified method.
    ///
    /// Returns the new DID and its document.
    fn did_create(&self, method: DidMethod) -> Result<(Did, DidDocument), IdentityError>;

    /// Rotate keys for an existing DID.
    ///
    /// The old keys are revoked and new keys are generated.
    fn did_rotate(&self, did: &Did, new_keys: KeyBundle) -> Result<DidDocument, IdentityError>;

    /// Deactivate a DID.
    ///
    /// Once deactivated, a DID cannot be used for signing.
    fn did_deactivate(&self, did: &Did) -> Result<(), IdentityError>;

    /// Sign arbitrary bytes with a DID's key.
    ///
    /// The specific key used depends on the implementation
    /// (typically the primary signing key).
    fn sign(&self, did: &Did, payload: &[u8]) -> Result<Signature, IdentityError>;

    /// Verify a signature against a DID.
    ///
    /// Returns true if the signature is valid.
    fn verify(&self, did: &Did, payload: &[u8], signature: &Signature) -> Result<bool, IdentityError>;

    /// Get the current DID document.
    fn get_document(&self, did: &Did) -> Result<DidDocument, IdentityError>;

    /// List all DIDs managed by this identity service.
    fn list_dids(&self) -> Result<Vec<Did>, IdentityError>;
}

/// DID resolution from various sources.
///
/// Different DID methods require different resolution strategies:
/// - did:key - Can be resolved locally from the DID itself
/// - did:icn - Resolved via ICN network
/// - did:web - Resolved via HTTPS (requires network)
pub trait DidResolver: Send + Sync {
    /// Resolve a DID to its document.
    ///
    /// This may involve network requests for some DID methods.
    fn resolve(&self, did: &Did) -> Result<DidDocument, IdentityError>;

    /// Check if this resolver can handle the given DID method.
    fn supports(&self, did: &Did) -> bool;
}

/// Keystore interface for cryptographic operations.
///
/// The keystore provides signing capabilities WITHOUT exposing
/// private key material. This allows for secure key management
/// with different backends (file, TPM, HSM).
pub trait Keystore: Send + Sync {
    /// Sign data with a specific key.
    ///
    /// The private key never leaves the keystore.
    fn sign(&self, key_id: &KeyId, payload: &[u8]) -> Result<Signature, IdentityError>;

    /// Verify a signature with a specific key.
    fn verify(&self, key_id: &KeyId, payload: &[u8], signature: &Signature) -> Result<bool, IdentityError>;

    /// Generate a new key pair.
    ///
    /// Returns the key ID for the new key.
    fn generate_key(&self, key_type: KeyType) -> Result<KeyId, IdentityError>;

    /// Delete a key from the keystore.
    fn delete_key(&self, key_id: &KeyId) -> Result<(), IdentityError>;

    /// List all key IDs in the keystore.
    fn list_keys(&self) -> Result<Vec<KeyId>, IdentityError>;

    /// Get the public key for a key ID.
    fn get_public_key(&self, key_id: &KeyId) -> Result<Vec<u8>, IdentityError>;
}

/// Supported key types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyType {
    /// Ed25519 signing key
    Ed25519,
    /// X25519 key agreement key
    X25519,
    /// ECDSA P-256 signing key
    EcdsaP256,
    /// ECDSA P-384 signing key
    EcdsaP384,
}

/// Errors from identity operations.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// DID not found
    #[error("DID not found: {0}")]
    DidNotFound(String),

    /// DID already exists
    #[error("DID already exists: {0}")]
    DidAlreadyExists(String),

    /// DID has been deactivated
    #[error("DID has been deactivated: {0}")]
    DidDeactivated(String),

    /// Key not found in keystore
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    /// Invalid signature
    #[error("Invalid signature")]
    InvalidSignature,

    /// Unsupported DID method
    #[error("Unsupported DID method: {0}")]
    UnsupportedMethod(String),

    /// Resolution failed (network error, etc.)
    #[error("Failed to resolve DID: {0}")]
    ResolutionFailed(String),

    /// Keystore error
    #[error("Keystore error: {0}")]
    KeystoreError(String),

    /// Internal error
    #[error("Internal identity error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_method() {
        assert_eq!(DidMethod::Icn, DidMethod::Icn);
        assert_ne!(DidMethod::Key, DidMethod::Web);
    }

    #[test]
    fn test_key_bundle() {
        let bundle = KeyBundle {
            signing_key: KeyId::new("key-1"),
            key_agreement: Some(KeyId::new("key-2")),
            additional_keys: vec![("backup".to_string(), KeyId::new("key-3"))],
        };
        assert_eq!(bundle.signing_key.0, "key-1");
        assert!(bundle.key_agreement.is_some());
        assert_eq!(bundle.additional_keys.len(), 1);
    }
}
