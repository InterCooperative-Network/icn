//! P4: the successor cutover commitment and genesis-roster commitment (S17 layout; EXPERIMENTAL).
//!
//! Golden values come from the independent reference
//! `tests/reference/cutover_commitment_reference.py` (standard-library Python, no shared code).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_authz::effect_v0::{ContextRef, MAX_INPUTS, MAX_ROLE_LEN};
use icn_governance::cutover::{
    genesis_roster_commitment, CutoverCommitmentV1, CutoverError, CUTOVER_DOMAIN,
    CUTOVER_ROSTER_DOMAIN,
};
use icn_governance::domain::GovernanceDomainId;
use icn_identity::authority_log::SubjectId;

const C: ContextRef = ContextRef([0xC1; 32]);
const H1: [u8; 32] = [0xB1; 32];
const M: [u8; 32] = [0x4D; 32];
const GOLDEN_R_EMPTY: &str = "296e3e56e1ddd67826fe8cc1511358669158544b3d654d86b8c40acf2454918c";
const GOLDEN_R_ONE: &str = "bfdb751c71d95f7db52de3559bf3c473123aa37ebb3104a6e21a12d36666ec15";
const GOLDEN_R_THREE: &str = "c76438699ba705283321e5fd451416db99e077315671aad3a0edffa98eb2fda3";
const GOLDEN_ENCODING: &str = "0000001969636e2e676f7665726e616e63652e6375746f7665722e76310001000000136c65676163792e636f6f702e6578616d706c6500000016737563636573736f722e636f6f702e6578616d706c65c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1c76438699ba705283321e5fd451416db99e077315671aad3a0edffa98eb2fda34d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d";
const GOLDEN_COMMITMENT: &str = "06861915a106305ac815ca86ed25262bb28f22ce1096670f5a7e1058a7f5d77c";

fn subject(b: u8) -> SubjectId {
    SubjectId::from_bytes([b; 32])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn three() -> Vec<(SubjectId, String)> {
    vec![
        (subject(1), "elector".into()),
        (subject(1), "member".into()),
        (subject(2), "socia-ñ".into()),
    ]
}

fn domain(s: &str) -> GovernanceDomainId {
    GovernanceDomainId(s.into())
}

fn example() -> CutoverCommitmentV1 {
    CutoverCommitmentV1::new(
        domain("legacy.coop.example"),
        domain("successor.coop.example"),
        C,
        H1,
        genesis_roster_commitment(C, &three()).unwrap(),
        M,
    )
    .unwrap()
}

#[test]
fn roster_golden_vectors_match_the_independent_reference() {
    assert_eq!(
        hex(&genesis_roster_commitment(C, &[]).unwrap()),
        GOLDEN_R_EMPTY
    );
    assert_eq!(
        hex(&genesis_roster_commitment(C, &[(subject(1), "member".into())]).unwrap()),
        GOLDEN_R_ONE
    );
    // One Subject with two distinct roles, and a non-ASCII role.
    assert_eq!(
        hex(&genesis_roster_commitment(C, &three()).unwrap()),
        GOLDEN_R_THREE
    );
}

#[test]
fn cutover_golden_encoding_and_commitment_match_the_independent_reference() {
    let value = example();
    assert_eq!(hex(&value.encode().unwrap()), GOLDEN_ENCODING);
    assert_eq!(hex(&value.commitment().unwrap()), GOLDEN_COMMITMENT);
}

#[test]
fn roster_refuses_noncanonical_input_and_never_sorts() {
    let mut unsorted = three();
    unsorted.swap(0, 2);
    assert_eq!(
        genesis_roster_commitment(C, &unsorted),
        Err(CutoverError::NotStrictlyAscending)
    );
    let mut duplicate = three();
    duplicate.insert(1, duplicate[0].clone());
    assert_eq!(
        genesis_roster_commitment(C, &duplicate),
        Err(CutoverError::NotStrictlyAscending)
    );
    let mut reversed_roles = three();
    reversed_roles.swap(0, 1);
    assert_eq!(
        genesis_roster_commitment(C, &reversed_roles),
        Err(CutoverError::NotStrictlyAscending)
    );
}

fn many(n: usize) -> Vec<(SubjectId, String)> {
    (0..n)
        .map(|i| {
            let mut bytes = [0u8; 32];
            bytes[30..].copy_from_slice(&(i as u16).to_be_bytes());
            (SubjectId::from_bytes(bytes), "member".to_string())
        })
        .collect()
}

#[test]
fn roster_size_and_role_bounds_are_the_genesis_bounds() {
    assert!(genesis_roster_commitment(C, &many(MAX_INPUTS)).is_ok());
    assert_eq!(
        genesis_roster_commitment(C, &many(MAX_INPUTS + 1)),
        Err(CutoverError::TooManyPairs)
    );
    let at_limit = vec![(subject(1), "r".repeat(MAX_ROLE_LEN))];
    assert!(genesis_roster_commitment(C, &at_limit).is_ok());
    let over = vec![(subject(1), "r".repeat(MAX_ROLE_LEN + 1))];
    assert_eq!(
        genesis_roster_commitment(C, &over),
        Err(CutoverError::RoleTooLong)
    );
    // Genesis permits an empty role and an empty standing; so does the commitment layer.
    assert!(genesis_roster_commitment(C, &[(subject(1), String::new())]).is_ok());
    assert!(genesis_roster_commitment(C, &[]).is_ok());
}

#[test]
fn roster_framing_and_context_separation() {
    let a = genesis_roster_commitment(C, &[(subject(1), "a".into()), (subject(2), "b".into())]);
    let b = genesis_roster_commitment(C, &[(subject(1), "ab".into()), (subject(2), String::new())]);
    assert_ne!(a.unwrap(), b.unwrap());
    let other_context = ContextRef([0xC2; 32]);
    assert_ne!(
        genesis_roster_commitment(C, &three()).unwrap(),
        genesis_roster_commitment(other_context, &three()).unwrap()
    );
    // The empty roster is context-bound too.
    assert_ne!(
        genesis_roster_commitment(C, &[]).unwrap(),
        genesis_roster_commitment(other_context, &[]).unwrap()
    );
    let mut flipped = three();
    let mut bytes = *flipped[2].0.as_bytes();
    bytes[31] ^= 1;
    flipped[2].0 = SubjectId::from_bytes(bytes);
    assert_ne!(
        genesis_roster_commitment(C, &three()).unwrap(),
        genesis_roster_commitment(C, &flipped).unwrap()
    );
}

#[test]
fn cutover_validation_refuses_structurally_invalid_commitments() {
    let r = genesis_roster_commitment(C, &three()).unwrap();
    let new = |l: &str, s: &str, r: [u8; 32], m: [u8; 32]| {
        CutoverCommitmentV1::new(domain(l), domain(s), C, H1, r, m)
    };
    assert_eq!(new("", "s", r, M), Err(CutoverError::EmptyDomain));
    assert_eq!(new("l", "", r, M), Err(CutoverError::EmptyDomain));
    assert_eq!(new("same", "same", r, M), Err(CutoverError::SameDomain));
    // The empty founding electorate for this C (D-R3, enforced at the cutover boundary).
    let empty = genesis_roster_commitment(C, &[]).unwrap();
    assert_eq!(new("l", "s", empty, M), Err(CutoverError::EmptyRoster));
    // Another context's empty roster is not this context's: R is context-bound.
    let foreign_empty = genesis_roster_commitment(ContextRef([0xC2; 32]), &[]).unwrap();
    assert!(new("l", "s", foreign_empty, M).is_ok());
    assert_eq!(
        new("l", "s", r, [0; 32]),
        Err(CutoverError::ZeroRecognitionMap)
    );
    // Case or whitespace differences are different domains: no normalization.
    assert!(new("Coop", "coop", r, M).is_ok());
}

#[test]
fn cutover_round_trips_and_decoding_is_strict() {
    let value = example();
    let bytes = value.encode().unwrap();
    assert_eq!(CutoverCommitmentV1::decode(&bytes).unwrap(), value);
    for cut in 0..bytes.len() {
        assert!(
            CutoverCommitmentV1::decode(&bytes[..cut]).is_err(),
            "prefix {cut}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        CutoverCommitmentV1::decode(&trailing),
        Err(CutoverError::Encoding("trailing bytes"))
    );
    // Wrong tag: the roster tag in the cutover position.
    let mut wrong_tag = (CUTOVER_ROSTER_DOMAIN.len() as u32).to_be_bytes().to_vec();
    wrong_tag.extend_from_slice(CUTOVER_ROSTER_DOMAIN);
    wrong_tag.extend_from_slice(&bytes[4 + CUTOVER_DOMAIN.len()..]);
    assert_eq!(
        CutoverCommitmentV1::decode(&wrong_tag),
        Err(CutoverError::Encoding("wrong domain tag"))
    );
    let version_at = 4 + CUTOVER_DOMAIN.len();
    for version in [0u16, 2, u16::MAX] {
        let mut other = bytes.clone();
        other[version_at..version_at + 2].copy_from_slice(&version.to_be_bytes());
        assert_eq!(
            CutoverCommitmentV1::decode(&other),
            Err(CutoverError::Encoding("unsupported version"))
        );
    }
    // An oversize length prefix for L is truncation, never a large allocation or a wrap.
    let mut oversize = bytes.clone();
    oversize[version_at + 2..version_at + 6].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        CutoverCommitmentV1::decode(&oversize),
        Err(CutoverError::Encoding("truncated"))
    );
    // Invalid UTF-8 in L.
    let mut invalid = bytes.clone();
    invalid[version_at + 6] = 0xFF;
    assert_eq!(
        CutoverCommitmentV1::decode(&invalid),
        Err(CutoverError::Encoding("invalid UTF-8"))
    );
    // Invalid UTF-8 in S (S's first content byte follows L's frame).
    let s_content = version_at + 6 + "legacy.coop.example".len() + 4;
    let mut invalid_s = bytes.clone();
    invalid_s[s_content] = 0xFF;
    assert_eq!(
        CutoverCommitmentV1::decode(&invalid_s),
        Err(CutoverError::Encoding("invalid UTF-8"))
    );
    // Decoding re-validates the empty-roster and zero-M placeholders.
    let n = bytes.len();
    let mut empty_roster = bytes.clone();
    empty_roster[n - 64..n - 32].copy_from_slice(&genesis_roster_commitment(C, &[]).unwrap());
    assert_eq!(
        CutoverCommitmentV1::decode(&empty_roster),
        Err(CutoverError::EmptyRoster)
    );
    let mut zero_m = bytes.clone();
    zero_m[n - 32..].copy_from_slice(&[0; 32]);
    assert_eq!(
        CutoverCommitmentV1::decode(&zero_m),
        Err(CutoverError::ZeroRecognitionMap)
    );
    // Decoding re-validates: an encoding of L == S is refused.
    let mut same = Vec::new();
    same.extend_from_slice(&bytes[..version_at + 2]);
    for _ in 0..2 {
        same.extend_from_slice(&4u32.to_be_bytes());
        same.extend_from_slice(b"same");
    }
    same.extend_from_slice(&bytes[bytes.len() - 128..]);
    assert_eq!(
        CutoverCommitmentV1::decode(&same),
        Err(CutoverError::SameDomain)
    );
}

#[test]
fn every_committed_field_changes_the_commitment_and_framing_is_unambiguous() {
    let base = example();
    let commitment = base.commitment().unwrap();
    let mut variants = Vec::new();
    let mut v = base.clone();
    v.predecessor = domain("legacy.coop.examplf");
    variants.push(v);
    let mut v = base.clone();
    v.successor = domain("successor.coop.examplf");
    variants.push(v);
    let mut v = base.clone();
    v.context = ContextRef([0xC2; 32]);
    variants.push(v);
    let mut v = base.clone();
    v.genesis_head[0] ^= 1;
    variants.push(v);
    let mut v = base.clone();
    v.roster_commitment[31] ^= 1;
    variants.push(v);
    let mut v = base.clone();
    v.recognition_map_commitment[16] ^= 1;
    variants.push(v);
    // Moving a byte across the L/S boundary.
    let mut v = base.clone();
    v.predecessor = domain("legacy.coop.examples");
    v.successor = domain("uccessor.coop.example");
    variants.push(v);
    // Swapping predecessor and successor.
    let mut v = base.clone();
    std::mem::swap(&mut v.predecessor, &mut v.successor);
    variants.push(v);
    for (i, variant) in variants.iter().enumerate() {
        assert_ne!(variant.commitment().unwrap(), commitment, "variant {i}");
    }
}

#[test]
fn the_two_tags_are_distinct_and_neither_is_a_prefix_of_the_other() {
    assert_ne!(CUTOVER_DOMAIN, CUTOVER_ROSTER_DOMAIN);
    assert!(!CUTOVER_ROSTER_DOMAIN.starts_with(CUTOVER_DOMAIN));
    assert!(!CUTOVER_DOMAIN.starts_with(CUTOVER_ROSTER_DOMAIN));
}
