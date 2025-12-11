//! Enrollment Token - Blind signature-based enrollment tokens
//!
//! Enrollment tokens allow privacy-preserving enrollment:
//! 1. User blinds their enrollment request
//! 2. Steward signs the blinded request (cannot see actual data)
//! 3. User unblinds to get a valid token
//! 4. Token used to complete enrollment (unlinkable to issuance)

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Enrollment token for SDIS enrollment
///
/// This token proves a person passed verification without revealing
/// which steward verified them or when (unlinkability via blind signatures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentToken {
    /// Unique token identifier
    pub token_id: [u8; 32],

    /// Unblinded signature from steward
    pub signature: Vec<u8>,

    /// DID of the issuing steward (public knowledge)
    pub issuing_steward: Did,

    /// Unix timestamp when token was issued
    pub issued_at: u64,

    /// Unix timestamp when token expires
    pub expires_at: u64,

    /// Commitment to the VUI this token is for
    pub vui_commitment: [u8; 32],

    /// Token version for future compatibility
    pub version: u8,
}

impl EnrollmentToken {
    /// Token version
    pub const VERSION: u8 = 1;

    /// Default validity period (7 days)
    pub const DEFAULT_VALIDITY_SECS: u64 = 7 * 24 * 60 * 60;

    /// Create a new enrollment token
    pub fn new(
        signature: Vec<u8>,
        issuing_steward: Did,
        vui_commitment: [u8; 32],
        validity_secs: Option<u64>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate token ID from signature and timestamp
        let mut hasher = Sha256::new();
        hasher.update(b"icn-enrollment-token-v1");
        hasher.update(&signature);
        hasher.update(now.to_le_bytes());
        let hash = hasher.finalize();

        let mut token_id = [0u8; 32];
        token_id.copy_from_slice(&hash);

        Self {
            token_id,
            signature,
            issuing_steward,
            issued_at: now,
            expires_at: now + validity_secs.unwrap_or(Self::DEFAULT_VALIDITY_SECS),
            vui_commitment,
            version: Self::VERSION,
        }
    }

    /// Check if the token has expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.expires_at
    }

    /// Check if the token is currently valid
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }

    /// Get remaining validity time in seconds
    pub fn remaining_validity(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.expires_at.saturating_sub(now)
    }

    /// Get token ID as hex string
    pub fn token_id_hex(&self) -> String {
        hex::encode(self.token_id)
    }

    /// Create the message that was signed (for verification)
    pub fn signing_message(&self) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(b"icn-enrollment-token-v1");
        msg.extend_from_slice(&self.vui_commitment);
        msg.extend_from_slice(&self.issued_at.to_le_bytes());
        msg.extend_from_slice(&self.expires_at.to_le_bytes());
        msg
    }
}

/// Record of token issuance (stored by steward)
///
/// This record cannot be correlated to the actual token due to blind signatures.
/// It's used for auditing and rate-limiting purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenIssuanceRecord {
    /// Hash of the blinded token (cannot correlate to actual token)
    pub blinded_token_hash: [u8; 32],

    /// VUI commitment that was checked
    pub vui_checked: [u8; 32],

    /// Unix timestamp when issued
    pub issued_at: u64,

    /// DID of the issuing steward
    pub steward_did: Did,

    /// Enrollment pathway used
    pub pathway_hash: [u8; 8],

    /// Jurisdiction tier at time of issuance
    pub jurisdiction_tier: u8,
}

impl TokenIssuanceRecord {
    /// Create a new issuance record
    pub fn new(
        blinded_token_hash: [u8; 32],
        vui_checked: [u8; 32],
        steward_did: Did,
        pathway_hash: [u8; 8],
        jurisdiction_tier: u8,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            blinded_token_hash,
            vui_checked,
            issued_at: now,
            steward_did,
            pathway_hash,
            jurisdiction_tier,
        }
    }
}

/// Token request from user to steward
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    /// Blinded message to sign
    pub blinded_message: Vec<u8>,

    /// VUI commitment
    pub vui_commitment: [u8; 32],

    /// Requested validity period in seconds
    pub validity_secs: Option<u64>,

    /// Enrollment pathway being used
    pub pathway: EnrollmentPathwayProof,

    /// Nonce for freshness
    pub nonce: [u8; 16],

    /// Timestamp of request
    pub requested_at: u64,
}

/// Proof of enrollment pathway (hashed for privacy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentPathwayProof {
    /// Hash of pathway type and evidence
    pub pathway_hash: [u8; 8],

    /// Zero-knowledge proof of valid enrollment (placeholder for Phase S4)
    pub zk_proof: Option<Vec<u8>>,
}

impl TokenRequest {
    /// Create a new token request
    pub fn new(blinded_message: Vec<u8>, vui_commitment: [u8; 32], pathway_hash: [u8; 8]) -> Self {
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            blinded_message,
            vui_commitment,
            validity_secs: None,
            pathway: EnrollmentPathwayProof {
                pathway_hash,
                zk_proof: None,
            },
            nonce,
            requested_at: now,
        }
    }
}

/// Response from steward with signed token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Signed blinded message
    pub blinded_signature: Vec<u8>,

    /// Steward's DID
    pub steward_did: Did,

    /// Original nonce (for correlation)
    pub nonce: [u8; 16],

    /// Unix timestamp of response
    pub responded_at: u64,
}

impl TokenResponse {
    /// Create a new token response
    pub fn new(blinded_signature: Vec<u8>, steward_did: Did, nonce: [u8; 16]) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            blinded_signature,
            steward_did,
            nonce,
            responded_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_enrollment_token_creation() {
        let steward_did = test_did();
        let vui_commitment = [42u8; 32];
        let signature = vec![1, 2, 3, 4];

        let token = EnrollmentToken::new(signature, steward_did, vui_commitment, None);

        assert_eq!(token.version, EnrollmentToken::VERSION);
        assert!(!token.is_expired());
        assert!(token.is_valid());
        assert!(token.remaining_validity() > 0);
    }

    #[test]
    fn test_token_expiration() {
        let steward_did = test_did();
        let vui_commitment = [42u8; 32];
        let signature = vec![1, 2, 3, 4];

        // Create token that's already expired (set expires_at to past)
        let mut token = EnrollmentToken::new(signature, steward_did, vui_commitment, Some(0));
        token.expires_at = token.issued_at.saturating_sub(1); // Set to 1 second before issued_at

        assert!(token.is_expired());
        assert!(!token.is_valid());
        assert_eq!(token.remaining_validity(), 0);
    }

    #[test]
    fn test_token_id_hex() {
        let steward_did = test_did();
        let vui_commitment = [42u8; 32];
        let signature = vec![1, 2, 3, 4];

        let token = EnrollmentToken::new(signature, steward_did, vui_commitment, None);

        let hex_id = token.token_id_hex();
        assert_eq!(hex_id.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_token_issuance_record() {
        let steward_did = test_did();
        let blinded_hash = [1u8; 32];
        let vui_checked = [2u8; 32];
        let pathway_hash = [3u8; 8];

        let record =
            TokenIssuanceRecord::new(blinded_hash, vui_checked, steward_did, pathway_hash, 1);

        assert_eq!(record.blinded_token_hash, blinded_hash);
        assert_eq!(record.vui_checked, vui_checked);
        assert!(record.issued_at > 0);
    }

    #[test]
    fn test_token_request() {
        let blinded_message = vec![1, 2, 3, 4];
        let vui_commitment = [42u8; 32];
        let pathway_hash = [1u8; 8];

        let request = TokenRequest::new(blinded_message.clone(), vui_commitment, pathway_hash);

        assert_eq!(request.blinded_message, blinded_message);
        assert_eq!(request.vui_commitment, vui_commitment);
        assert_ne!(request.nonce, [0u8; 16]); // Should be random
    }

    #[test]
    fn test_token_response() {
        let steward_did = test_did();
        let blinded_signature = vec![5, 6, 7, 8];
        let nonce = [9u8; 16];

        let response = TokenResponse::new(blinded_signature.clone(), steward_did, nonce);

        assert_eq!(response.blinded_signature, blinded_signature);
        assert_eq!(response.nonce, nonce);
        assert!(response.responded_at > 0);
    }
}
