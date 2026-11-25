//! P1 (context→domain commitment): the EXPERIMENTAL `ContextInceptionV1` encoding, its strict
//! decoding, V0 byte-compatibility through the versioned [`ContextInception`] carrier, and V0/V1
//! context separation.
//!
//! Golden V1 bytes were computed independently (Python `hashlib` over the specified layout; the
//! same model reproduces the existing V0 goldens), not by this implementation. Fixture identities
//! only: every key is derived from a seed byte.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    AuthzCodecError, CanonicalV0, ContextInception, ContextInceptionRef, ContextInceptionV0,
    ContextInceptionV1, INCEPTION_V1_VERSION,
};
use icn_identity::authority_log::PrincipalKey;
use sha2::{Digest, Sha256};

fn principal(seed: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(SigningKey::from_bytes(&[seed; 32]).verifying_key())
        .unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

const SERVED: [u8; 32] = [0x5e; 32];

fn founders() -> Vec<PrincipalKey> {
    vec![principal(0xF3), principal(0xF1), principal(0xF2)]
}

/// The V0 fixture of `effect_v0_encoding.rs`, unchanged.
fn v0() -> ContextInceptionV0 {
    ContextInceptionV0::new(founders(), 2, [0x1a; 32]).unwrap()
}

/// The same founders, threshold and label, serving [`SERVED`].
fn v1() -> ContextInceptionV1 {
    ContextInceptionV1::new(founders(), 2, [0x1a; 32], SERVED).unwrap()
}

// V0 goldens: identical to `effect_v0_encoding.rs` (GOLDEN_INCEPTION / GOLDEN_CONTEXT_REF).
const GOLDEN_V0_INCEPTION_SHA: &str =
    "89157bf0ccf76c3ba8c5a8022b4cff978193fcfca6a5a2444bd2460d192a0484";
const GOLDEN_V0_CONTEXT_REF: &str =
    "2c2bbef23bc25517ee855dee68c17aa8e83459405d5d04ecbacae9e862c92e25";

// V1 goldens (independent reference; see the module doc).
const GOLDEN_V1_BYTES: &str = concat!(
    "0000001669636e2e617574687a2e76312e696e63657074696f6e000100000003",
    "70082ae9f98861b514d4846bc5e02fb096fb36ca953c92830909816dcf38a0a1",
    "b8fc9d70b330b5d377a521047bce772144a747caa203c453a91385de86e177cc",
    "dcbe4fdaf43c6146889b82b0e690fc88c12866052ea3ea042b1f7988bca5ae08",
    "021a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a",
    "1a5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e",
    "5e",
);
const GOLDEN_V1_INCEPTION_SHA: &str =
    "6073b901bec9aeee8e595f9d1a7a1f092cc0232cd688c68728db7f640ca4848b";
const GOLDEN_V1_CONTEXT_REF: &str =
    "3d5e3c4a0a86e4a20ea62dc681b6a56841fd52be5666a8e32017a1ed656e253e";

/// Offset of the version field: `u32be(22) || "icn.authz.v1.inception"`.
const VERSION_AT: usize = 4 + 22;
/// Offset of `served_subject`: the last 32 bytes.
const V1_LEN: usize = 193;
const SERVED_AT: usize = V1_LEN - 32;

// ------------------------------------------------------------------------------------------
// V0 compatibility
// ------------------------------------------------------------------------------------------

#[test]
fn v0_bytes_and_context_ref_are_unchanged() {
    let v0 = v0();
    assert_eq!(digest(&v0.canonical_bytes()), GOLDEN_V0_INCEPTION_SHA);
    assert_eq!(hex(&v0.context_ref().0), GOLDEN_V0_CONTEXT_REF);
}

#[test]
fn v0_through_the_versioned_carrier_is_byte_identical() {
    let v0 = v0();
    let carried = ContextInception::from(v0.clone());
    assert_eq!(carried.canonical_bytes(), v0.canonical_bytes());
    assert_eq!(hex(&carried.context_ref().0), GOLDEN_V0_CONTEXT_REF);
    assert_eq!(carried.served_subject(), None);
    assert_eq!(carried.threshold(), 2);
    assert_eq!(carried.founders(), v0.founders.as_slice());

    let view = ContextInceptionRef::from(&v0);
    assert_eq!(view.context_ref(), v0.context_ref());
    assert_eq!(view.canonical_bytes(), v0.canonical_bytes());
    assert_eq!(view.served_subject(), None);

    // Carrier decode dispatches V0 bytes to V0, byte for byte.
    let decoded = ContextInception::decode(&v0.canonical_bytes()).unwrap();
    assert_eq!(decoded, ContextInception::V0(v0));
}

// ------------------------------------------------------------------------------------------
// V1 encoding
// ------------------------------------------------------------------------------------------

#[test]
fn v1_golden_bytes_hash_and_context_ref() {
    let v1 = v1();
    let bytes = v1.canonical_bytes();
    assert_eq!(bytes.len(), V1_LEN);
    assert_eq!(hex(&bytes), GOLDEN_V1_BYTES);
    assert_eq!(digest(&bytes), GOLDEN_V1_INCEPTION_SHA);
    assert_eq!(hex(&v1.context_ref().0), GOLDEN_V1_CONTEXT_REF);
    assert_eq!(&bytes[SERVED_AT..], &SERVED);
    assert_eq!(
        u16::from_be_bytes([bytes[VERSION_AT], bytes[VERSION_AT + 1]]),
        INCEPTION_V1_VERSION
    );
}

#[test]
fn v1_round_trips_directly_and_through_the_carrier() {
    let v1 = v1();
    let bytes = v1.canonical_bytes();
    assert_eq!(ContextInceptionV1::decode(&bytes).unwrap(), v1);
    let carried = ContextInception::decode(&bytes).unwrap();
    assert_eq!(carried, ContextInception::V1(v1.clone()));
    assert_eq!(carried.canonical_bytes(), bytes);
    assert_eq!(carried.context_ref(), v1.context_ref());
    assert_eq!(carried.served_subject(), Some(SERVED));
    assert_eq!(
        ContextInceptionRef::from(&carried),
        ContextInceptionRef::V1(&v1)
    );
}

#[test]
fn new_sorts_founders_like_v0() {
    let v1 = v1();
    assert!(v1
        .founders
        .windows(2)
        .all(|w| w[0].as_bytes() < w[1].as_bytes()));
    assert_eq!(v1.founders, v0().founders);
}

// ------------------------------------------------------------------------------------------
// V0 / V1 separation
// ------------------------------------------------------------------------------------------

#[test]
fn v0_and_v1_contexts_never_collide() {
    let (v0, v1) = (v0(), v1());
    assert_ne!(v0.context_ref(), v1.context_ref());
    assert_ne!(v0.canonical_bytes(), v1.canonical_bytes());
    // Different served subjects are different contexts.
    let other = ContextInceptionV1::new(founders(), 2, [0x1a; 32], [0x5f; 32]).unwrap();
    assert_ne!(other.context_ref(), v1.context_ref());
}

#[test]
fn v0_decoder_refuses_v1_and_v1_decoder_refuses_v0() {
    assert_eq!(
        ContextInceptionV0::decode(&v1().canonical_bytes()),
        Err(AuthzCodecError::WrongDomain { field: "domain" })
    );
    assert_eq!(
        ContextInceptionV1::decode(&v0().canonical_bytes()),
        Err(AuthzCodecError::WrongDomain { field: "domain" })
    );
}

/// A V1 body re-tagged as V0 (or the reverse) does not decode: the tag, version and length all
/// disagree, so no inception can be read as the other version.
#[test]
fn retagging_does_not_cross_versions() {
    let mut as_v0 = v1().canonical_bytes();
    as_v0[4..26].copy_from_slice(b"icn.authz.v0.inception");
    assert!(ContextInceptionV0::decode(&as_v0).is_err());
    assert!(ContextInception::decode(&as_v0).is_err());

    let mut as_v1 = v0().canonical_bytes();
    as_v1[4..26].copy_from_slice(b"icn.authz.v1.inception");
    assert!(ContextInceptionV1::decode(&as_v1).is_err());
    assert!(ContextInception::decode(&as_v1).is_err());
}

#[test]
fn carrier_refuses_unknown_domains() {
    for bytes in [
        Vec::new(),
        vec![0, 0, 0],
        b"\x00\x00\x00\x16icn.authz.v2.inception\x00\x02".to_vec(),
        b"\x00\x00\x00\x14icn.authz.v0.context".to_vec(),
    ] {
        assert_eq!(
            ContextInception::decode(&bytes),
            Err(AuthzCodecError::WrongDomain { field: "domain" }),
            "{bytes:?}"
        );
    }
}

// ------------------------------------------------------------------------------------------
// V1 strictness
// ------------------------------------------------------------------------------------------

#[test]
fn v1_refuses_wrong_versions() {
    for version in [0u16, 2, 0x0100, u16::MAX] {
        let mut bytes = v1().canonical_bytes();
        bytes[VERSION_AT..VERSION_AT + 2].copy_from_slice(&version.to_be_bytes());
        assert_eq!(
            ContextInceptionV1::decode(&bytes),
            Err(AuthzCodecError::UnsupportedVersion(version))
        );
        assert_eq!(
            ContextInception::decode(&bytes),
            Err(AuthzCodecError::UnsupportedVersion(version))
        );
    }
}

#[test]
fn v1_refuses_trailing_bytes() {
    for extra in [&[0u8][..], &[0x5e], &[0; 32]] {
        let mut bytes = v1().canonical_bytes();
        bytes.extend_from_slice(extra);
        assert_eq!(
            ContextInceptionV1::decode(&bytes),
            Err(AuthzCodecError::TrailingBytes)
        );
        assert_eq!(
            ContextInception::decode(&bytes),
            Err(AuthzCodecError::TrailingBytes)
        );
    }
}

#[test]
fn v1_refuses_every_truncation() {
    let bytes = v1().canonical_bytes();
    for len in 0..bytes.len() {
        assert!(
            ContextInceptionV1::decode(&bytes[..len]).is_err(),
            "prefix of {len} bytes decoded"
        );
        assert!(ContextInception::decode(&bytes[..len]).is_err());
    }
}

#[test]
fn v1_refuses_a_zero_served_subject_at_every_door() {
    let refusal = AuthzCodecError::InvalidValue {
        field: "served_subject",
    };
    // Construction.
    assert_eq!(
        ContextInceptionV1::new(founders(), 2, [0x1a; 32], [0; 32]),
        Err(refusal.clone())
    );
    // Decode.
    let mut bytes = v1().canonical_bytes();
    bytes[SERVED_AT..].fill(0);
    assert_eq!(ContextInceptionV1::decode(&bytes), Err(refusal.clone()));
    assert_eq!(ContextInception::decode(&bytes), Err(refusal.clone()));
    // A value assembled field by field (the fields are public) fails validation, which every
    // entry point runs first.
    let mut assembled = v1();
    assembled.served_subject = [0; 32];
    assert_eq!(assembled.validate(), Err(refusal.clone()));
    assert_eq!(ContextInception::V1(assembled).validate(), Err(refusal));
    // One non-zero byte anywhere is a commitment.
    for i in 0..32 {
        let mut s = [0u8; 32];
        s[i] = 1;
        assert!(ContextInceptionV1::new(founders(), 2, [0x1a; 32], s).is_ok());
    }
}

#[test]
fn v1_keeps_the_v0_founder_rules() {
    // Out-of-range thresholds: 0, non-intersecting (2·t <= n), above n.
    for t in [0u8, 1, 4] {
        assert_eq!(
            ContextInceptionV1::new(founders(), t, [0x1a; 32], SERVED),
            Err(AuthzCodecError::InvalidValue { field: "threshold" })
        );
    }
    assert_eq!(
        ContextInceptionV1::new(Vec::new(), 1, [0x1a; 32], SERVED),
        Err(AuthzCodecError::InvalidValue { field: "founders" })
    );
    assert_eq!(
        ContextInceptionV1::new(vec![principal(1), principal(1)], 2, [0x1a; 32], SERVED),
        Err(AuthzCodecError::NotStrictlyAscending { field: "founders" })
    );
    // Unsorted founders on the wire: swap the first two founder keys.
    let mut bytes = v1().canonical_bytes();
    let first = VERSION_AT + 2 + 4;
    let (a, b) = (
        bytes[first..first + 32].to_vec(),
        bytes[first + 32..first + 64].to_vec(),
    );
    bytes[first..first + 32].copy_from_slice(&b);
    bytes[first + 32..first + 64].copy_from_slice(&a);
    assert_eq!(
        ContextInceptionV1::decode(&bytes),
        Err(AuthzCodecError::NotStrictlyAscending { field: "founders" })
    );
}

#[test]
fn v1_refuses_tampered_framing() {
    // Domain length prefix.
    let mut bytes = v1().canonical_bytes();
    bytes[3] = 21;
    assert!(ContextInceptionV1::decode(&bytes).is_err());
    // Founder count larger / smaller than the founders present.
    for count in [2u32, 4, u32::MAX] {
        let mut bytes = v1().canonical_bytes();
        bytes[VERSION_AT + 2..VERSION_AT + 6].copy_from_slice(&count.to_be_bytes());
        assert!(
            ContextInceptionV1::decode(&bytes).is_err(),
            "count {count} decoded"
        );
    }
}

// ------------------------------------------------------------------------------------------
// Bounded mutation: every single-bit flip of the V1 encoding
// ------------------------------------------------------------------------------------------

/// Each of the 193 × 8 single-bit mutants of the canonical V1 bytes is either refused, or decodes
/// to a different inception with a different [`ContextRef`] whose re-encoding is exactly the
/// mutant. No mutant is accepted as the original context, and none decodes as V0.
#[test]
fn every_single_bit_flip_is_refused_or_names_a_different_context() {
    let v1 = v1();
    let original = v1.canonical_bytes();
    let (mut refused, mut different) = (0usize, 0usize);
    for i in 0..original.len() {
        for bit in 0..8 {
            let mut m = original.clone();
            m[i] ^= 1 << bit;
            assert!(
                ContextInceptionV0::decode(&m).is_err(),
                "byte {i} bit {bit}"
            );
            match ContextInception::decode(&m) {
                Err(_) => refused += 1,
                Ok(decoded) => {
                    assert_eq!(decoded.canonical_bytes(), m, "byte {i} bit {bit}");
                    assert_ne!(decoded.context_ref(), v1.context_ref());
                    assert_ne!(decoded, ContextInception::V1(v1.clone()));
                    different += 1;
                }
            }
        }
    }
    assert_eq!(refused + different, V1_LEN * 8);
    // The tag, version and count/threshold bytes are refused; label and served flips survive
    // as different contexts (both are opaque, so any non-zero value is well formed).
    assert!(refused > 0 && different > 0, "{refused} / {different}");
}
