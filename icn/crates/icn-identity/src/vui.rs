//! SDIS VUI - Verifiable Unique Identifier
//!
//! A VUI is a privacy-preserving identifier that proves uniqueness
//! without revealing the underlying identity data. It's computed via
//! a threshold PRF (Pseudo-Random Function) by the steward network.
//!
//! # Protocol
//!
//! 1. User provides identity data (ID document, biometrics, etc.) to stewards
//! 2. Each steward computes a PRF partial: partial_i = HMAC(pepper_share_i, data_hash)
//! 3. User combines partials to get VUI = H(combine(partials))
//! 4. VUI is stored as commitment in the anchor
//!
//! # Properties
//!
//! - **Unique**: Same person always gets same VUI (deterministic)
//! - **Private**: VUI reveals nothing about the person
//! - **Unlinkable**: Without pepper, can't reverse VUI to identity
//! - **Distributed**: No single steward knows the full pepper

use icn_crypto_pq::{combine_prf_partials, PrfPartial};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::anchor::EnrollmentPathway;

/// Verifiable Unique Identifier
///
/// A 32-byte identifier that uniquely identifies a person while
/// preserving their privacy. Computed via threshold PRF.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vui([u8; 32]);

impl Vui {
    /// Create a VUI from raw bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Vui(bytes)
    }

    /// Get the VUI as bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Compute a commitment to this VUI
    ///
    /// Used to store VUI commitment in anchors without revealing the VUI itself.
    pub fn commitment(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-vui-commitment");
        hasher.update(self.0);
        let result = hasher.finalize();

        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&result);
        commitment
    }

    /// Combine PRF partials from stewards to compute VUI
    ///
    /// This is the client-side computation after receiving partials
    /// from threshold number of stewards.
    pub fn from_partials(partials: &[PrfPartial]) -> anyhow::Result<Self> {
        let combined = combine_prf_partials(partials)
            .map_err(|e| anyhow::anyhow!("Failed to combine PRF partials: {e}"))?;
        Ok(Vui(combined))
    }

    /// Get as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string
    pub fn from_hex(s: &str) -> anyhow::Result<Self> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            anyhow::bail!("VUI must be 32 bytes, got {}", bytes.len());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Vui(arr))
    }
}

/// Identity data hash for VUI computation
///
/// This is what gets sent to stewards for PRF evaluation.
/// The hash ensures the actual identity data is never transmitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdDataHash {
    /// Hash of the identity data
    pub hash: [u8; 32],
    /// Enrollment pathway used
    pub pathway: EnrollmentPathway,
    /// Timestamp of hash computation
    pub computed_at: u64,
}

impl IdDataHash {
    /// Create from identity data
    ///
    /// The data is hashed locally before being sent to stewards.
    /// This ensures PII is never transmitted.
    pub fn from_data(data: &[u8], pathway: EnrollmentPathway) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-id-data-v1");
        hasher.update(data);
        let result = hasher.finalize();

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);

        Self {
            hash,
            pathway,
            computed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Get the hash bytes for PRF input
    pub fn as_prf_input(&self) -> &[u8; 32] {
        &self.hash
    }
}

/// Request to steward for VUI computation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VuiRequest {
    /// Hashed identity data
    pub id_data_hash: IdDataHash,
    /// Requester's temporary public key for response encryption
    pub ephemeral_public: [u8; 32],
    /// Nonce for freshness
    pub nonce: [u8; 16],
}

impl VuiRequest {
    /// Create a new VUI request
    pub fn new(id_data_hash: IdDataHash) -> Self {
        let mut ephemeral_public = [0u8; 32];
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut ephemeral_public);
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);

        Self {
            id_data_hash,
            ephemeral_public,
            nonce,
        }
    }
}

/// Response from steward with PRF partial
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VuiResponse {
    /// PRF partial from this steward
    pub partial: SerializedPartial,
    /// Steward index (for combining)
    pub steward_index: u8,
    /// Original nonce (for correlation)
    pub nonce: [u8; 16],
    /// Commitment to the pepper share (for verification)
    pub share_commitment: [u8; 32],
}

/// Serializable PRF partial for threshold combination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedPartial {
    /// Shard index (1-based)
    pub index: u8,
    /// Partial PRF value from this shard
    pub value: [u8; 32],
}

impl SerializedPartial {
    /// Convert to PrfPartial for combination
    pub fn to_prf_partial(&self) -> PrfPartial {
        PrfPartial {
            index: self.index,
            value: self.value,
        }
    }

    /// Create from PrfPartial
    pub fn from_prf_partial(partial: &PrfPartial) -> Self {
        Self {
            index: partial.index,
            value: partial.value,
        }
    }
}

/// VUI registry entry for uniqueness checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VuiRegistryEntry {
    /// VUI commitment (not the VUI itself)
    pub vui_commitment: [u8; 32],
    /// Anchor this VUI is bound to
    pub anchor_id: [u8; 32],
    /// When this entry was created
    pub registered_at: u64,
    /// Steward attestations (threshold required)
    pub attestations: Vec<VuiAttestation>,
}

/// Steward attestation that a VUI is unique
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VuiAttestation {
    /// Steward's DID
    pub steward_did: String,
    /// Steward's signature on (vui_commitment, anchor_id)
    pub signature: Vec<u8>,
    /// When attested
    pub attested_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_crypto_pq::PepperShare;

    #[test]
    fn test_vui_from_bytes() {
        let bytes = [42u8; 32];
        let vui = Vui::from_bytes(bytes);

        assert_eq!(vui.as_bytes(), &bytes);
    }

    #[test]
    fn test_vui_commitment() {
        let vui = Vui::from_bytes([42u8; 32]);
        let commitment = vui.commitment();

        // Commitment should be different from VUI
        assert_ne!(commitment, *vui.as_bytes());

        // Commitment should be deterministic
        assert_eq!(commitment, vui.commitment());
    }

    #[test]
    fn test_vui_from_partials() {
        let share1 = PepperShare::new(1, [1u8; 32]);
        let share2 = PepperShare::new(2, [2u8; 32]);
        let input = [42u8; 32];

        let partial1 = icn_crypto_pq::threshold::PrfPartial::compute(&share1, &input);
        let partial2 = icn_crypto_pq::threshold::PrfPartial::compute(&share2, &input);

        let vui = Vui::from_partials(&[partial1, partial2]).unwrap();

        assert_eq!(vui.as_bytes().len(), 32);
    }

    #[test]
    fn test_vui_hex_roundtrip() {
        let vui = Vui::from_bytes([42u8; 32]);
        let hex = vui.to_hex();
        let restored = Vui::from_hex(&hex).unwrap();

        assert_eq!(vui, restored);
    }

    #[test]
    fn test_id_data_hash() {
        let data = b"identity document data";
        let pathway = EnrollmentPathway::Genesis {
            reason: "test".to_string(),
        };

        let hash = IdDataHash::from_data(data, pathway);

        assert_eq!(hash.hash.len(), 32);
        assert!(hash.computed_at > 0);
    }

    #[test]
    fn test_id_data_hash_determinism() {
        let data = b"identity document data";
        let pathway = EnrollmentPathway::Genesis {
            reason: "test".to_string(),
        };

        let hash1 = IdDataHash::from_data(data, pathway.clone());
        let hash2 = IdDataHash::from_data(data, pathway);

        // Same data should produce same hash
        assert_eq!(hash1.hash, hash2.hash);
    }

    #[test]
    fn test_vui_request() {
        let hash = IdDataHash::from_data(
            b"test data",
            EnrollmentPathway::Genesis {
                reason: "test".to_string(),
            },
        );

        let request = VuiRequest::new(hash);

        // Should have randomness
        assert_ne!(request.ephemeral_public, [0u8; 32]);
        assert_ne!(request.nonce, [0u8; 16]);
    }

    #[test]
    fn test_serialized_partial_roundtrip() {
        let original = PrfPartial {
            index: 5,
            value: [42u8; 32],
        };

        let serialized = SerializedPartial::from_prf_partial(&original);
        let restored = serialized.to_prf_partial();

        assert_eq!(original.index, restored.index);
        assert_eq!(original.value, restored.value);
    }
}
