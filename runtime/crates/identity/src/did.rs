use serde::{Deserialize, Serialize};
use std::fmt;

// Re-export necessary types if they were moved from here previously
// pub use crate::error::{IdentityError, IdentityResult}; 
// pub use crate::keypair::Signature; 

/// Represents an identity ID (DID)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

// Define DidDocument and VerificationMethod locally if they are not part of ssi or need customization
// For now, assuming they might be defined elsewhere or imported directly where needed.
// If they were meant to be defined *here*, their definitions should be added.

// Example placeholder definitions if needed:
/*
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
   // ... fields based on ssi::did::VerificationMethod or W3C spec ...
   pub id: String,
   #[serde(rename = "type")]
   pub type_: String,
   pub controller: String,
   #[serde(rename = "publicKeyJwk")]
   pub public_key_jwk: Option<ssi::jwk::JWK>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidDocument {
   // ... fields based on ssi::did::Document or W3C spec ...
   #[serde(rename = "@context")]
   pub context: serde_json::Value,
   pub id: String,
   #[serde(rename = "verificationMethod")]
   pub verification_method: Option<Vec<VerificationMethod>>,
   // ... other DID document fields like authentication, assertionMethod etc.
}
*/
