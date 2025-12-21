//! QR Code Encoding
//!
//! Compact binary encoding for ephemeral proofs suitable for QR codes.
//!
//! Target size: <1KB for QR Version 15-20 (binary mode).
//! Actual base encoding: ~135 bytes (fits in Version 5).

use super::ephemeral::{Channel, EphemeralError, EphemeralProof, EPOCH_2024};
use icn_zkp::ProofType;

/// Magic bytes for ICN ephemeral proof
const MAGIC: [u8; 2] = [0x49, 0x43]; // "IC"

/// Encode an ephemeral proof for QR code display
///
/// Format (135 bytes base):
/// ```text
/// [0-1]   Magic bytes "IC" (2)
/// [2]     Version (1)
/// [3]     Proof type byte (1)
/// [4-19]  Truncated anchor (16)
/// [20-51] Ephemeral public key (32)
/// [52-55] Relative expiry (4) - seconds since EPOCH_2024
/// [56-71] Nonce (16)
/// [72-135] Signature (64)
/// [136]   Channels bitmap (1)
/// ```
/// Total: 137 bytes
pub fn encode_for_qr(proof: &EphemeralProof) -> Result<Vec<u8>, EphemeralError> {
    let mut encoded = Vec::with_capacity(140);

    // Magic bytes
    encoded.extend_from_slice(&MAGIC);

    // Version (1 byte)
    encoded.push(proof.v);

    // Proof type (1 byte)
    encoded.push(proof_type_to_byte(&proof.t));

    // Truncated anchor (16 bytes)
    encoded.extend_from_slice(&proof.a);

    // Ephemeral public key (32 bytes)
    encoded.extend_from_slice(&proof.k);

    // Relative expiry (4 bytes - seconds since EPOCH_2024)
    let relative_expiry = proof.x.saturating_sub(EPOCH_2024) as u32;
    encoded.extend_from_slice(&relative_expiry.to_le_bytes());

    // Nonce (16 bytes)
    encoded.extend_from_slice(&proof.n);

    // Signature (64 bytes)
    encoded.extend_from_slice(&proof.signature_bytes());

    // Channels bitmap (1 byte)
    encoded.push(Channel::to_bitmap(&proof.c));

    Ok(encoded)
}

/// Decode an ephemeral proof from QR code data
pub fn decode_from_qr(data: &[u8]) -> Result<EphemeralProof, EphemeralError> {
    if data.len() < 137 {
        return Err(EphemeralError::InvalidFormat(format!(
            "too short: {} bytes (need 137)",
            data.len()
        )));
    }

    // Check magic
    if data[0..2] != MAGIC {
        return Err(EphemeralError::InvalidFormat("invalid magic bytes".into()));
    }

    // Version
    let v = data[2];
    if v != 1 {
        return Err(EphemeralError::InvalidFormat(format!(
            "unsupported version: {v}"
        )));
    }

    // Proof type
    let t = byte_to_proof_type(data[3])?;

    // Truncated anchor
    let mut a = [0u8; 16];
    a.copy_from_slice(&data[4..20]);

    // Ephemeral public key
    let mut k = [0u8; 32];
    k.copy_from_slice(&data[20..52]);

    // Relative expiry -> absolute
    let relative_expiry = u32::from_le_bytes([data[52], data[53], data[54], data[55]]);
    let x = EPOCH_2024 + (relative_expiry as u64);

    // Nonce
    let mut n = [0u8; 16];
    n.copy_from_slice(&data[56..72]);

    // Signature (as Vec<u8>)
    let s = data[72..136].to_vec();

    // Channels
    let c = Channel::from_bitmap(data[136]);

    Ok(EphemeralProof {
        v,
        t,
        a,
        k,
        x,
        n,
        s,
        c,
    })
}

/// Convert proof type to single byte
fn proof_type_to_byte(pt: &ProofType) -> u8 {
    match pt {
        ProofType::AgeAtLeast { threshold } => {
            // 0x00-0x7F: Age proofs (threshold in lower 7 bits)
            (*threshold).min(127)
        }
        ProofType::Citizenship { country_code: _ } => {
            // 0x80: Citizenship (country code in extended data if needed)
            0x80
        }
        ProofType::Membership { .. } => {
            // 0x81: Membership
            0x81
        }
        ProofType::NonRevocation => {
            // 0x82: Non-revocation
            0x82
        }
        ProofType::Custom { .. } => {
            // 0xFF: Custom
            0xFF
        }
    }
}

/// Convert byte to proof type
///
/// NOTE: Some proof types (Citizenship, Membership, Custom) require extended data
/// that cannot be encoded in a single byte. These use placeholder values and
/// require protocol extension to properly support in ephemeral QR codes.
/// See: https://github.com/InterCooperative-Network/icn/issues/133
fn byte_to_proof_type(byte: u8) -> Result<ProofType, EphemeralError> {
    match byte {
        0x00..=0x7F => Ok(ProofType::AgeAtLeast { threshold: byte }),
        0x80 => Ok(ProofType::Citizenship {
            // Extended QR format needed to encode country code
            country_code: [b'?', b'?'],
        }),
        0x81 => Ok(ProofType::Membership {
            // Extended QR format needed to encode org DID
            org_did: icn_identity::Did::from_anchor_id(&[0u8; 32]),
        }),
        0x82 => Ok(ProofType::NonRevocation),
        0xFF => Ok(ProofType::Custom {
            // Extended QR format needed to encode circuit ID
            circuit_id: [0u8; 32],
        }),
        _ => Err(EphemeralError::InvalidFormat(format!(
            "unknown proof type: 0x{byte:02x}"
        ))),
    }
}

/// Calculate QR code version needed for given data size
pub fn qr_version_for_size(bytes: usize) -> u8 {
    // Binary mode capacities (error correction level M)
    // Version 5: 106 bytes
    // Version 10: 271 bytes
    // Version 15: 520 bytes
    // Version 20: 858 bytes

    match bytes {
        0..=106 => 5,
        107..=271 => 10,
        272..=520 => 15,
        521..=858 => 20,
        _ => 25, // Larger versions exist but less common
    }
}

/// Estimate QR code complexity
#[derive(Debug, Clone)]
pub struct QrEstimate {
    pub data_size: usize,
    pub qr_version: u8,
    pub modules: u16, // QR code size in modules
}

impl QrEstimate {
    pub fn for_proof(_proof: &EphemeralProof) -> Self {
        let data_size = 137; // Base encoding size
        let qr_version = qr_version_for_size(data_size);
        let modules = 17 + 4 * (qr_version as u16); // QR version formula

        Self {
            data_size,
            qr_version,
            modules,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::{Anchor, KeyBundle};
    use std::time::Duration;

    fn test_keybundle() -> KeyBundle {
        let anchor = Anchor::genesis("test");
        KeyBundle::generate(anchor, 1).unwrap()
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let keybundle = test_keybundle();
        let (original, _) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 21 },
            Duration::from_secs(3600),
            vec![Channel::Nfc, Channel::Ble],
        )
        .unwrap();

        let encoded = encode_for_qr(&original).unwrap();
        assert_eq!(encoded.len(), 137);
        assert_eq!(&encoded[0..2], &MAGIC);

        let decoded = decode_from_qr(&encoded).unwrap();

        assert_eq!(decoded.v, original.v);
        assert_eq!(decoded.a, original.a);
        assert_eq!(decoded.k, original.k);
        assert_eq!(decoded.x, original.x);
        assert_eq!(decoded.n, original.n);
        assert_eq!(decoded.s, original.s);
        assert_eq!(decoded.c, original.c);
    }

    #[test]
    fn test_proof_type_encoding() {
        // Age proofs
        assert_eq!(
            proof_type_to_byte(&ProofType::AgeAtLeast { threshold: 18 }),
            18
        );
        assert_eq!(
            proof_type_to_byte(&ProofType::AgeAtLeast { threshold: 21 }),
            21
        );

        // Special types
        assert_eq!(
            proof_type_to_byte(&ProofType::Citizenship {
                country_code: [b'U', b'S']
            }),
            0x80
        );
        assert_eq!(proof_type_to_byte(&ProofType::NonRevocation), 0x82);
    }

    #[test]
    fn test_qr_version_estimation() {
        assert_eq!(qr_version_for_size(100), 5);
        assert_eq!(qr_version_for_size(137), 10); // Our base size
        assert_eq!(qr_version_for_size(500), 15);
        assert_eq!(qr_version_for_size(800), 20);
    }

    #[test]
    fn test_qr_estimate() {
        let keybundle = test_keybundle();
        let (proof, _) = EphemeralProof::generate(
            &keybundle,
            ProofType::AgeAtLeast { threshold: 18 },
            Duration::from_secs(3600),
            vec![],
        )
        .unwrap();

        let estimate = QrEstimate::for_proof(&proof);
        assert_eq!(estimate.data_size, 137);
        assert!(estimate.qr_version <= 15); // Should fit in small QR
    }

    #[test]
    fn test_decode_invalid_magic() {
        let data = vec![0x00; 137]; // Wrong magic
        let result = decode_from_qr(&data);
        assert!(matches!(result, Err(EphemeralError::InvalidFormat(_))));
    }

    #[test]
    fn test_decode_too_short() {
        let data = vec![0x49, 0x43, 0x01]; // Only 3 bytes
        let result = decode_from_qr(&data);
        assert!(matches!(result, Err(EphemeralError::InvalidFormat(_))));
    }
}
