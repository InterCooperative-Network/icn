//! P5 for the experimental authority-to-effect spine v0: golden byte vectors for every v0
//! encoding, decode∘encode as the identity, and strict-decode refusals.
//!
//! Fixture identities only: every key is derived from a seed byte.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    roster_hash, target_id, AuthorizationV0, AuthzCodecError, CanonicalV0, ContextInceptionV0,
    ContextRef, EffectV0, InputActV0, OutcomeReceiptV0, RuleAdoptionV0, Signed, SignedDesignation,
    StandingV0, Verdict, WriterDesignationV0,
};
use icn_identity::authority_log::{PrincipalKey, SubjectId};
use sha2::{Digest, Sha256};

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn principal(seed: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(key(seed).verifying_key()).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn inception() -> ContextInceptionV0 {
    ContextInceptionV0::new(
        vec![principal(0xF3), principal(0xF1), principal(0xF2)],
        2,
        [0x1a; 32],
    )
    .unwrap()
}

fn ctx() -> ContextRef {
    inception().context_ref()
}

fn designation() -> WriterDesignationV0 {
    WriterDesignationV0 {
        context: ctx(),
        epoch: 1,
        writer: principal(0xA1),
        start_head: [0x0e; 32],
    }
}

fn standing() -> StandingV0 {
    StandingV0 {
        context: ctx(),
        epoch: 1,
        subject: SubjectId::from_bytes([0x5b; 32]),
        role: "member".into(),
        not_after: 2_000,
    }
}

fn adoption() -> RuleAdoptionV0 {
    RuleAdoptionV0 {
        context: ctx(),
        epoch: 1,
        rule_hash: [0x7e; 32],
    }
}

fn authorization() -> AuthorizationV0 {
    AuthorizationV0 {
        context: ctx(),
        epoch: 1,
        subject_matter: [0x50; 32],
        rule_hash: [0x7e; 32],
        verdict: Verdict::Pass,
        inputs: vec![
            InputActV0 {
                act_id: [0x01; 32],
                subject: SubjectId::from_bytes([0x5b; 32]),
                position: 1,
            },
            InputActV0 {
                act_id: [0x02; 32],
                subject: SubjectId::from_bytes([0x5c; 32]),
                position: 3,
            },
        ],
        roster_hash: [0x40; 32],
        effect: EffectV0 {
            target: target_id(&principal(0xC1)),
            action: "noop".into(),
            params_hash: [0xee; 32],
        },
        operation_id: [0x09; 32],
        not_after: 1_500,
    }
}

fn receipt() -> OutcomeReceiptV0 {
    OutcomeReceiptV0 {
        operation_id: [0x09; 32],
        authorization_hash: authorization().hash(),
        custodian: principal(0xC1),
        result_hash: [0x4e; 32],
    }
}

fn roundtrip<T: CanonicalV0 + PartialEq + std::fmt::Debug>(value: &T) -> Vec<u8> {
    let bytes = value.canonical_bytes();
    let decoded = T::decode(&bytes).unwrap();
    assert_eq!(&decoded, value);
    assert_eq!(decoded.canonical_bytes(), bytes);
    bytes
}

/// Golden vectors: `SHA-256` of each canonical encoding. A change to any v0 byte layout, domain
/// string or field order changes one of these.
#[test]
fn p5_golden_vectors_and_roundtrip() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("inception", roundtrip(&inception())),
        ("designation", roundtrip(&designation())),
        ("standing", roundtrip(&standing())),
        ("rule_adoption", roundtrip(&adoption())),
        ("authorization", roundtrip(&authorization())),
        ("outcome_receipt", roundtrip(&receipt())),
    ];
    let got: Vec<(&str, String)> = cases.iter().map(|(n, b)| (*n, digest(b))).collect();
    let expected: Vec<(&str, &str)> = vec![
        ("inception", GOLDEN_INCEPTION),
        ("designation", GOLDEN_DESIGNATION),
        ("standing", GOLDEN_STANDING),
        ("rule_adoption", GOLDEN_ADOPTION),
        ("authorization", GOLDEN_AUTHORIZATION),
        ("outcome_receipt", GOLDEN_RECEIPT),
    ];
    // Print every value before asserting, so a deliberate format change shows all drift at once.
    for (name, g) in &got {
        eprintln!("{name}: {g}");
    }
    eprintln!("context_ref: {}", hex(&ctx().0));
    eprintln!("target_id: {}", hex(&target_id(&principal(0xC1))));
    for ((name, g), (_, e)) in got.iter().zip(expected.iter()) {
        assert_eq!(g, e, "golden vector drifted for {name}");
    }
    assert_eq!(hex(&ctx().0), GOLDEN_CONTEXT_REF);
    assert_eq!(hex(&target_id(&principal(0xC1))), GOLDEN_TARGET_ID);
    let commitment = authorization().effect.commitment(ctx(), [0x09; 32]);
    let roster = roster_hash(&[standing()]);
    eprintln!("subject_matter_commitment: {}", hex(&commitment));
    eprintln!("roster_hash: {}", hex(&roster));
    assert_eq!(hex(&commitment), GOLDEN_COMMITMENT);
    assert_eq!(hex(&roster), GOLDEN_ROSTER);
}

const GOLDEN_INCEPTION: &str = "89157bf0ccf76c3ba8c5a8022b4cff978193fcfca6a5a2444bd2460d192a0484";
const GOLDEN_DESIGNATION: &str = "363514c28c2d29d0e0c7226bd228000607a3f5f3916584631793a29099faea86";
const GOLDEN_STANDING: &str = "8b1fa50420daaf296d9a66f10c4a9cf22af24968839474b607f60a43657bebbd";
const GOLDEN_ADOPTION: &str = "17eda2e6fabc1b33cfd9fcc48d1e1c5be7efa8547a9d756d62998ba487ef2d25";
const GOLDEN_AUTHORIZATION: &str =
    "2c5a1beda5e8683698f9df56668ea9c2436be05ae418a1340a1dd6fa81050d03";
const GOLDEN_RECEIPT: &str = "330b2f5ea03ed4196b98155faea8ac0fed308227ec8c94ac0fd37e29db811392";
const GOLDEN_CONTEXT_REF: &str = "2c2bbef23bc25517ee855dee68c17aa8e83459405d5d04ecbacae9e862c92e25";
const GOLDEN_COMMITMENT: &str = "71330935128bae2a44acb67a385b07aa52dbacd034bbfc043c31c3108122f8c3";
const GOLDEN_ROSTER: &str = "aec9f99c10181201d4fd6c4f62aa5a05e4f33413c2cd7b70a09ba5aa2e722b52";
const GOLDEN_TARGET_ID: &str = "fc85bfcec4faa7b0ec82d393b492174c798f4dc1b669a263309181327c2131f7";

/// The standing layout, built by hand without calling any v0 encoder, so the golden above is
/// anchored to the documented layout and not only to the implementation's own output.
#[test]
fn p5_standing_layout_by_hand() {
    let mut expected = Vec::new();
    let domain = b"icn.authz.v0.standing";
    expected.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    expected.extend_from_slice(domain);
    expected.extend_from_slice(&0u16.to_be_bytes());
    expected.extend_from_slice(&ctx().0);
    expected.extend_from_slice(&1u64.to_be_bytes());
    expected.extend_from_slice(&[0x5b; 32]);
    expected.extend_from_slice(&6u32.to_be_bytes());
    expected.extend_from_slice(b"member");
    expected.extend_from_slice(&2_000u64.to_be_bytes());
    assert_eq!(standing().canonical_bytes(), expected);
}

/// Every v0 body begins with its own domain string, so no two kinds share a prefix.
#[test]
fn p5_every_body_is_domain_separated() {
    let bodies = [
        inception().canonical_bytes(),
        designation().canonical_bytes(),
        standing().canonical_bytes(),
        adoption().canonical_bytes(),
        authorization().canonical_bytes(),
        receipt().canonical_bytes(),
    ];
    for (i, a) in bodies.iter().enumerate() {
        for b in bodies.iter().skip(i + 1) {
            assert_ne!(a[..28], b[..28], "two body kinds share a domain prefix");
        }
    }
    // A standing body does not decode as an authorization and vice versa.
    assert!(matches!(
        AuthorizationV0::decode(&standing().canonical_bytes()),
        Err(AuthzCodecError::WrongDomain { .. }) | Err(AuthzCodecError::TooLong { .. })
    ));
}

#[test]
fn p5_signed_wrappers_roundtrip() {
    let signed = Signed::sign(authorization(), &key(0xA1));
    let bytes = signed.encode();
    let decoded = Signed::<AuthorizationV0>::decode(&bytes).unwrap();
    assert_eq!(decoded, signed);
    assert!(decoded.verifies_under(&principal(0xA1)));
    assert!(!decoded.verifies_under(&principal(0xA2)));

    let des = SignedDesignation::sign(designation(), &[&key(0xF2), &key(0xF1)]).unwrap();
    let decoded = SignedDesignation::decode(&des.encode()).unwrap();
    assert_eq!(decoded, des);
    assert_eq!(decoded.encode(), des.encode());
}

#[test]
fn strict_decode_refusals() {
    // Trailing bytes.
    let mut bytes = standing().canonical_bytes();
    bytes.push(0);
    assert_eq!(
        StandingV0::decode(&bytes),
        Err(AuthzCodecError::TrailingBytes)
    );

    // Unsorted inputs.
    let mut auth = authorization();
    auth.inputs.reverse();
    assert_eq!(
        AuthorizationV0::decode(&auth.canonical_bytes()),
        Err(AuthzCodecError::NotStrictlyAscending { field: "inputs" })
    );

    // Unknown verdict tag (its offset: domain lp + version + ctx + epoch + matter + rule).
    let mut bytes = authorization().canonical_bytes();
    let verdict_at = 4 + b"icn.authz.v0.authorization".len() + 2 + 32 + 8 + 32 + 32;
    bytes[verdict_at] = 9;
    assert_eq!(
        AuthorizationV0::decode(&bytes),
        Err(AuthzCodecError::InvalidValue { field: "verdict" })
    );

    // Epoch 0 designation.
    let mut d = designation();
    d.epoch = 0;
    assert_eq!(
        WriterDesignationV0::decode(&d.canonical_bytes()),
        Err(AuthzCodecError::InvalidValue { field: "epoch" })
    );

    // Threshold above the founder count, and an empty founder set.
    assert!(ContextInceptionV0::new(vec![principal(1)], 2, [0; 32]).is_err());
    assert!(ContextInceptionV0::new(vec![], 1, [0; 32]).is_err());
    // Duplicate founder.
    assert_eq!(
        ContextInceptionV0::new(vec![principal(1), principal(1)], 1, [0; 32]),
        Err(AuthzCodecError::NotStrictlyAscending { field: "founders" })
    );

    // Oversized role.
    let mut s = standing();
    s.role = "x".repeat(65);
    assert!(matches!(
        StandingV0::decode(&s.canonical_bytes()),
        Err(AuthzCodecError::TooLong { field: "role", .. })
    ));
}
