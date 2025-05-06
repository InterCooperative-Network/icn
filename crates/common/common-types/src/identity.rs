//! Common identity types and traits for the ICN project
//!
//! This module provides shared identity definitions used across components.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Identity represents a unique identifier for an entity in the ICN system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identity {
    /// The unique identifier string (could be a DID, public key hash, etc.)
    pub id: String,
    /// The type of identity
    pub id_type: IdentityType,
}

/// Types of identities supported in the ICN system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IdentityType {
    /// A Decentralized Identifier
    Did,
    /// A public key identity
    PublicKey,
    /// A federation identifier
    Federation,
    /// A mesh node identifier
    MeshNode,
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.id_type, self.id)
    }
}

impl fmt::Display for IdentityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityType::Did => write!(f, "did"),
            IdentityType::PublicKey => write!(f, "pubkey"),
            IdentityType::Federation => write!(f, "fed"),
            IdentityType::MeshNode => write!(f, "node"),
        }
    }
}

impl Identity {
    /// Create a new identity with the given ID and type
    pub fn new(id: impl Into<String>, id_type: IdentityType) -> Self {
        Self {
            id: id.into(),
            id_type,
        }
    }

    /// Create a new DID-based identity
    pub fn did(did: impl Into<String>) -> Self {
        Self::new(did, IdentityType::Did)
    }

    /// Create a new public key-based identity
    pub fn pubkey(key: impl Into<String>) -> Self {
        Self::new(key, IdentityType::PublicKey)
    }

    /// Create a new federation identity
    pub fn federation(id: impl Into<String>) -> Self {
        Self::new(id, IdentityType::Federation)
    }

    /// Create a new mesh node identity
    pub fn mesh_node(id: impl Into<String>) -> Self {
        Self::new(id, IdentityType::MeshNode)
    }
} 