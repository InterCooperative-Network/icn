//! SDIS Anchor - Permanent identity anchor
//!
//! An Anchor is a permanent, unchanging identity root in the SDIS system.
//! Unlike traditional DIDs where the identifier is derived from a public key,
//! an SDIS Anchor is derived from a VUI (Verifiable Unique Identifier) that
//! represents a verified unique individual.
//!
//! # Key Properties
//!
//! - **Permanent**: Anchor ID never changes, even when keys are rotated
//! - **Recoverable**: Identity can be recovered through steward network
//! - **Unique**: One anchor per verified person (Sybil resistance)
//! - **Privacy-preserving**: Anchor ID reveals nothing about the person
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Anchor                              │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │ ID: H(VUI || genesis)                                   ││
//! │  │ Created: timestamp                                       ││
//! │  │ Pathway: enrollment method                               ││
//! │  │ VUI Commitment: H(VUI)                                   ││
//! │  └─────────────────────────────────────────────────────────┘│
//! │                           │                                 │
//! │                           ▼                                 │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │              KeyBundle v1 (current)                     ││
//! │  │  Ed25519 + ML-DSA hybrid keys                           ││
//! │  │  X25519 encryption keys                                 ││
//! │  └─────────────────────────────────────────────────────────┘│
//! │                           │                                 │
//! │                           ▼                                 │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │              KeyBundle v2 (after rotation)              ││
//! │  │  New hybrid keys, same anchor                           ││
//! │  └─────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Did;

/// Permanent identity anchor
///
/// An Anchor is created once during enrollment and never changes.
/// Keys (KeyBundles) are bound to the Anchor and can be rotated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anchor {
    /// Unique anchor identifier: H(VUI || genesis_random)
    pub id: [u8; 32],
    /// Unix timestamp of anchor creation
    pub created_at: u64,
    /// How the person was verified during enrollment
    pub pathway: EnrollmentPathway,
    /// Commitment to the VUI (for recovery verification)
    pub vui_commitment: [u8; 32],
}

/// How a person's identity was verified during enrollment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum EnrollmentPathway {
    /// Government-issued ID verification
    GovernmentId {
        /// ISO 3166-1 alpha-2 country code
        country: [u8; 2],
        /// Hash of document type (passport, ID card, etc.)
        doc_type_hash: [u8; 8],
    },
    /// Sponsored by an organization with existing trust
    OrganizationalSponsorship {
        /// DID of the sponsoring organization
        org_did: Did,
        /// Hash of organization's internal ID for this person
        org_internal_id_hash: [u8; 16],
    },
    /// Biometric verification (face, fingerprint, etc.)
    BiometricCommitment {
        /// Hash of biometric template (never stored directly)
        template_hash: [u8; 32],
    },
    /// Web of trust vouching by existing members
    WebOfTrust {
        /// DIDs of vouchers (minimum threshold required)
        vouchers: Vec<Did>,
        /// When the vouching occurred
        vouched_at: u64,
    },
    /// Genesis anchor (for bootstrap/testing)
    Genesis {
        /// Reason for genesis anchor
        reason: String,
    },
}

impl Anchor {
    /// Create an anchor from a VUI and enrollment pathway
    ///
    /// The anchor ID is computed as: H(VUI || genesis_random)
    /// where genesis_random is fresh randomness that prevents
    /// rainbow table attacks on VUIs.
    pub fn from_vui(vui: &[u8; 32], pathway: EnrollmentPathway, genesis: &[u8; 32]) -> Self {
        // Compute anchor ID
        let mut hasher = Sha256::new();
        hasher.update(b"icn-anchor-v1");
        hasher.update(vui);
        hasher.update(genesis);
        let id_result = hasher.finalize();

        let mut id = [0u8; 32];
        id.copy_from_slice(&id_result);

        // Compute VUI commitment
        let mut hasher = Sha256::new();
        hasher.update(b"icn-vui-commitment");
        hasher.update(vui);
        let commitment_result = hasher.finalize();

        let mut vui_commitment = [0u8; 32];
        vui_commitment.copy_from_slice(&commitment_result);

        Self {
            id,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            pathway,
            vui_commitment,
        }
    }

    /// Create a genesis anchor (for bootstrap/testing)
    ///
    /// Genesis anchors don't require VUI verification and are used
    /// for initial network bootstrap or testing scenarios.
    pub fn genesis(reason: &str) -> Self {
        let mut id = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut id);

        Self {
            id,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            pathway: EnrollmentPathway::Genesis {
                reason: reason.to_string(),
            },
            vui_commitment: [0u8; 32], // No VUI for genesis
        }
    }

    /// Convert anchor to DID format
    ///
    /// SDIS DIDs use the format: `did:icn:<base58-anchor-id>`
    /// This maintains backward compatibility with legacy DIDs
    /// while allowing anchor-based identity.
    pub fn to_did(&self) -> Did {
        Did::from_anchor_id(&self.id)
    }

    /// Verify that a VUI matches this anchor's commitment
    ///
    /// Used during recovery to verify the person presenting
    /// a VUI is the owner of this anchor.
    pub fn verify_vui(&self, vui: &[u8; 32]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-vui-commitment");
        hasher.update(vui);
        let computed = hasher.finalize();

        computed[..] == self.vui_commitment
    }

    /// Check if this is a genesis anchor
    pub fn is_genesis(&self) -> bool {
        matches!(self.pathway, EnrollmentPathway::Genesis { .. })
    }

    /// Get the anchor ID as bytes
    pub fn id_bytes(&self) -> &[u8; 32] {
        &self.id
    }

    /// Get the anchor ID as a hex string
    pub fn id_hex(&self) -> String {
        hex::encode(&self.id)
    }
}

impl Did {
    /// Create a DID from an anchor ID (SDIS format)
    ///
    /// This creates a DID in the format `did:icn:<base58-anchor-id>`
    /// for SDIS anchor-based identities.
    pub fn from_anchor_id(anchor_id: &[u8; 32]) -> Self {
        let encoded = multibase::encode(multibase::Base::Base58Btc, anchor_id);
        Did::new_unchecked(format!("did:icn:{encoded}"))
    }

    /// Check if this DID is potentially an SDIS anchor-based DID
    ///
    /// Note: This only checks the format, not whether the DID
    /// actually corresponds to a registered anchor.
    pub fn is_anchor_did(&self) -> bool {
        // Both legacy and anchor DIDs have the same format
        // The difference is in what the 32 bytes represent
        // This method returns true if the format is valid
        // Actual anchor verification requires checking the anchor registry
        self.as_str().starts_with("did:icn:")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_from_vui() {
        let vui = [42u8; 32];
        let genesis = [1u8; 32];
        let pathway = EnrollmentPathway::Genesis {
            reason: "test".to_string(),
        };

        let anchor = Anchor::from_vui(&vui, pathway, &genesis);

        assert_eq!(anchor.id.len(), 32);
        assert!(anchor.created_at > 0);
        assert!(anchor.verify_vui(&vui));
    }

    #[test]
    fn test_anchor_vui_verification() {
        let vui = [42u8; 32];
        let wrong_vui = [43u8; 32];
        let genesis = [1u8; 32];
        let pathway = EnrollmentPathway::Genesis {
            reason: "test".to_string(),
        };

        let anchor = Anchor::from_vui(&vui, pathway, &genesis);

        assert!(anchor.verify_vui(&vui));
        assert!(!anchor.verify_vui(&wrong_vui));
    }

    #[test]
    fn test_anchor_to_did() {
        let anchor = Anchor::genesis("test");
        let did = anchor.to_did();

        assert!(did.as_str().starts_with("did:icn:"));
    }

    #[test]
    fn test_anchor_determinism() {
        let vui = [42u8; 32];
        let genesis = [1u8; 32];
        let pathway = EnrollmentPathway::Genesis {
            reason: "test".to_string(),
        };

        let anchor1 = Anchor::from_vui(&vui, pathway.clone(), &genesis);
        let anchor2 = Anchor::from_vui(&vui, pathway, &genesis);

        // Same inputs should produce same ID
        assert_eq!(anchor1.id, anchor2.id);
        assert_eq!(anchor1.vui_commitment, anchor2.vui_commitment);
    }

    #[test]
    fn test_anchor_different_genesis() {
        let vui = [42u8; 32];
        let genesis1 = [1u8; 32];
        let genesis2 = [2u8; 32];
        let pathway = EnrollmentPathway::Genesis {
            reason: "test".to_string(),
        };

        let anchor1 = Anchor::from_vui(&vui, pathway.clone(), &genesis1);
        let anchor2 = Anchor::from_vui(&vui, pathway, &genesis2);

        // Different genesis = different ID
        assert_ne!(anchor1.id, anchor2.id);
        // But same VUI commitment
        assert_eq!(anchor1.vui_commitment, anchor2.vui_commitment);
    }

    #[test]
    fn test_genesis_anchor() {
        let anchor = Anchor::genesis("bootstrap");

        assert!(anchor.is_genesis());
        assert_eq!(anchor.vui_commitment, [0u8; 32]);
    }

    #[test]
    fn test_government_id_pathway() {
        let vui = [42u8; 32];
        let genesis = [1u8; 32];
        let pathway = EnrollmentPathway::GovernmentId {
            country: [b'U', b'S'],
            doc_type_hash: [1, 2, 3, 4, 5, 6, 7, 8],
        };

        let anchor = Anchor::from_vui(&vui, pathway, &genesis);
        assert!(!anchor.is_genesis());
    }

    #[test]
    fn test_anchor_serialization() {
        let anchor = Anchor::genesis("test");

        let json = serde_json::to_string(&anchor).unwrap();
        let restored: Anchor = serde_json::from_str(&json).unwrap();

        assert_eq!(anchor.id, restored.id);
        assert_eq!(anchor.vui_commitment, restored.vui_commitment);
    }

    #[test]
    fn test_did_from_anchor_id() {
        let anchor_id = [42u8; 32];
        let did = Did::from_anchor_id(&anchor_id);

        assert!(did.as_str().starts_with("did:icn:"));
        assert!(did.is_anchor_did());
    }
}
