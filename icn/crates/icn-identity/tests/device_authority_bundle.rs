//! N4-B — the `(N1 facts, device act)` container and the stateless verification over it.
//!
//! N4-A left one boundary unbuilt (`N4A_DEVICE_AUTHORITY_EVALUATION.md` §9.2 item 2): a strict,
//! deterministic container a non-Rust client can carry to any relying party, holding the
//! Subject's retained canonical facts and one signed act. These tests pin that container and the
//! rule that verifying *through* it is exactly verifying the store it rehydrates:
//!
//! > The bundle is bytes, not authority. Every fact in it is re-admitted through the same gate a
//! > live event passes; one inadmissible fact fails the whole bundle rather than being dropped;
//! > withholding a fact can only shrink what the bundle proves; and the relying party still
//! > supplies the Subject and the position — the bundle never chooses either.
//!
//! Fixture identities only. Every key is derived from a seed byte; nothing here is a real person.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{authorize_at, revoke_at, store_of, stranger, subject, Subject};
use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    CodecError, DeviceCapability, PrincipalKey, SignedAuthorityEvent, SubjectId, COMMITMENT_DOMAIN,
    DOMAIN, KDF_DOMAIN, SIGNATURE_DOMAIN,
};
use icn_identity::device_authority::{
    sign_device_act, verify_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal,
    SignedDeviceAct, DEVICE_ACT_DOMAIN,
};
use icn_identity::device_authority_bundle::{
    verify_bundle, verify_bundle_bytes, BundleError, BundleVerifyError, DeviceAuthorityBundleV1,
    DEVICE_AUTHORITY_BUNDLE_DOMAIN, DEVICE_AUTHORITY_BUNDLE_VERSION, MAX_BUNDLE_FACTS,
};
use icn_identity::subject_context::GEN_CONTEXT_DOMAIN;

// ---------------------------------------------------------------------------------------------
// The fixture: S with A and B, then A revoked — the same four facts N4-A pins.
// ---------------------------------------------------------------------------------------------

const DEVICE_A_SEED: u8 = 0x11;
const DEVICE_B_SEED: u8 = 0x22;

struct Fixture {
    s: Subject,
    a_key: SigningKey,
    a: PrincipalKey,
    b_key: SigningKey,
    b: PrincipalKey,
    /// Positions: 1 = authorize A, 2 = authorize B, 3 = revoke A.
    facts: Vec<SignedAuthorityEvent>,
}

fn fixture() -> Fixture {
    let s = subject(0x01, 4);
    let (a_key, a) = stranger(DEVICE_A_SEED);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let revoke_a = revoke_at(&s, 0, 3, auth_b.body.event_id(), a);
    let facts = vec![s.inception.clone(), auth_a, auth_b, revoke_a];
    Fixture {
        s,
        a_key,
        a,
        b_key,
        b,
        facts,
    }
}

fn signed(
    subject: SubjectId,
    device: PrincipalKey,
    key: &SigningKey,
    position: u64,
) -> SignedDeviceAct {
    let act = DeviceActV1::new(
        subject,
        device,
        DeviceCapability::Sign,
        position,
        b"katie:open-workspace".to_vec(),
    )
    .expect("fixture act is well-formed");
    sign_device_act(&act, key).expect("signer holds the device key the act names")
}

fn bundle_of(facts: &[SignedAuthorityEvent], act: SignedDeviceAct) -> DeviceAuthorityBundleV1 {
    DeviceAuthorityBundleV1::new(facts.iter().cloned(), act).expect("fixture bundle is well-formed")
}

// A test-local framer written from the prose of the container contract, so a malformed bundle
// can be produced byte by byte without any help from the implementation under test.
fn lp(x: &[u8]) -> Vec<u8> {
    let mut v = (x.len() as u32).to_be_bytes().to_vec();
    v.extend_from_slice(x);
    v
}

fn frame(facts: &[(&[u8; 32], &[u8; 64], &[u8])], act: &SignedDeviceAct) -> Vec<u8> {
    let mut v = lp(DEVICE_AUTHORITY_BUNDLE_DOMAIN);
    v.extend_from_slice(&DEVICE_AUTHORITY_BUNDLE_VERSION.to_be_bytes());
    v.extend_from_slice(&(facts.len() as u32).to_be_bytes());
    for (event_id, witness, body) in facts {
        v.extend_from_slice(*event_id);
        v.extend_from_slice(*witness);
        v.extend_from_slice(&lp(body));
    }
    v.extend_from_slice(&lp(&act.act.canonical_bytes()));
    v.extend_from_slice(act.signature.as_bytes());
    v
}

fn record(f: &SignedAuthorityEvent) -> ([u8; 32], [u8; 64], Vec<u8>) {
    (
        *f.body.event_id().as_bytes(),
        *f.signature.as_bytes(),
        f.body.canonical_bytes(),
    )
}

// ---------------------------------------------------------------------------------------------
// The container is strict, canonical and order-independent.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_bundle_round_trips_strictly_and_its_bytes_do_not_depend_on_fact_order() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let bundle = bundle_of(&f.facts, act.clone());
    let bytes = bundle.canonical_bytes();

    let decoded = DeviceAuthorityBundleV1::decode(&bytes).expect("canonical bytes decode");
    assert_eq!(decoded, bundle);
    assert_eq!(
        decoded.canonical_bytes(),
        bytes,
        "encode/decode is a bijection"
    );
    assert_eq!(decoded.facts().len(), 4);
    assert_eq!(decoded.act(), &act);

    let mut reversed = f.facts.clone();
    reversed.reverse();
    let same = bundle_of(&reversed, act.clone());
    assert_eq!(
        same.canonical_bytes(),
        bytes,
        "fact order at construction leaves no trace"
    );

    let mut duplicated = f.facts.clone();
    duplicated.push(f.facts[1].clone());
    let same = bundle_of(&duplicated, act);
    assert_eq!(same.canonical_bytes(), bytes, "a repeated fact is one fact");
    assert_eq!(bundle.bundle_id(), decoded.bundle_id());
}

#[test]
fn trailing_bytes_a_wrong_version_and_a_wrong_domain_are_refused() {
    let f = fixture();
    let bundle = bundle_of(&f.facts, signed(f.s.subject, f.b, &f.b_key, 3));
    let bytes = bundle.canonical_bytes();

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&trailing),
        Err(BundleError::Codec(CodecError::TrailingBytes(1)))
    ));

    let domain_len = 4 + DEVICE_AUTHORITY_BUNDLE_DOMAIN.len();
    let mut wrong_version = bytes.clone();
    wrong_version[domain_len + 1] ^= 0x01;
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&wrong_version),
        Err(BundleError::Codec(CodecError::UnsupportedVersion(_)))
    ));

    let mut wrong_domain = bytes;
    wrong_domain[4] ^= 0x01;
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&wrong_domain),
        Err(BundleError::Codec(CodecError::BadDomain))
    ));
}

#[test]
fn facts_out_of_canonical_order_or_duplicated_on_the_wire_are_refused() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let canonical = bundle_of(&f.facts, act.clone());
    let ordered: Vec<_> = canonical.facts().iter().map(record).collect();
    let refs = |rs: &[(usize, usize)]| -> Vec<u8> {
        let picked: Vec<(&[u8; 32], &[u8; 64], &[u8])> = rs
            .iter()
            .map(|(i, _)| (&ordered[*i].0, &ordered[*i].1, ordered[*i].2.as_slice()))
            .collect();
        frame(&picked, &act)
    };

    // The canonical order, hand-framed, is accepted and is byte-identical to the implementation's.
    let hand = refs(&[(0, 0), (1, 0), (2, 0), (3, 0)]);
    assert_eq!(hand, canonical.canonical_bytes());

    // Two facts swapped.
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&refs(&[(1, 0), (0, 0), (2, 0), (3, 0)])),
        Err(BundleError::FactsNotCanonicallyOrdered { index: 1 })
    ));
    // One fact repeated.
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&refs(&[(0, 0), (1, 0), (1, 0), (2, 0)])),
        Err(BundleError::FactsNotCanonicallyOrdered { index: 2 })
    ));
}

#[test]
fn an_event_id_that_is_not_the_digest_of_its_body_is_refused() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let canonical = bundle_of(&f.facts, act.clone());
    let mut records: Vec<_> = canonical.facts().iter().map(record).collect();
    records[2].0[0] ^= 0xff;
    let picked: Vec<(&[u8; 32], &[u8; 64], &[u8])> = records
        .iter()
        .map(|r| (&r.0, &r.1, r.2.as_slice()))
        .collect();
    // The event id is part of the ordering key, so the mutated record may now be out of order;
    // whichever check fires, the bundle is refused and the index names the record.
    match DeviceAuthorityBundleV1::decode(&frame(&picked, &act)) {
        Err(BundleError::EventIdMismatch { index })
        | Err(BundleError::FactsNotCanonicallyOrdered { index }) => {
            assert!(
                index == 2 || index == 3,
                "the refused record is the mutated one or its successor"
            )
        }
        other => panic!("expected a refusal naming the mutated record, got {other:?}"),
    }
}

#[test]
fn a_non_canonical_body_on_the_wire_is_refused() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let canonical = bundle_of(&f.facts, act.clone());
    let mut records: Vec<_> = canonical.facts().iter().map(record).collect();
    // Append a byte to one body: its declared length still frames it, but the body is no longer
    // the canonical encoding of anything.
    records[1].2.push(0x00);
    let picked: Vec<(&[u8; 32], &[u8; 64], &[u8])> = records
        .iter()
        .map(|r| (&r.0, &r.1, r.2.as_slice()))
        .collect();
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&frame(&picked, &act)),
        Err(BundleError::Codec(_)) | Err(BundleError::NonCanonicalFact { index: 1 })
    ));
}

#[test]
fn a_fact_count_the_input_cannot_hold_is_refused_before_any_allocation() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let mut bytes = lp(DEVICE_AUTHORITY_BUNDLE_DOMAIN);
    bytes.extend_from_slice(&DEVICE_AUTHORITY_BUNDLE_VERSION.to_be_bytes());
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    bytes.extend_from_slice(&lp(&act.act.canonical_bytes()));
    bytes.extend_from_slice(act.signature.as_bytes());
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&bytes),
        Err(BundleError::Codec(CodecError::CountOverflow(u32::MAX)))
    ));
    // A count within the input but above the protocol bound is refused by the bound itself.
    let mut too_many = lp(DEVICE_AUTHORITY_BUNDLE_DOMAIN);
    too_many.extend_from_slice(&DEVICE_AUTHORITY_BUNDLE_VERSION.to_be_bytes());
    too_many.extend_from_slice(&(MAX_BUNDLE_FACTS + 1).to_be_bytes());
    too_many.extend(vec![0u8; (MAX_BUNDLE_FACTS as usize + 1) * 100]);
    assert!(matches!(
        DeviceAuthorityBundleV1::decode(&too_many),
        Err(BundleError::TooManyFacts(_))
    ));
}

#[test]
fn bundle_domain_is_distinct_from_every_other_domain() {
    for other in [
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
        DEVICE_ACT_DOMAIN,
        GEN_CONTEXT_DOMAIN,
    ] {
        assert_ne!(DEVICE_AUTHORITY_BUNDLE_DOMAIN, other);
    }
}

// ---------------------------------------------------------------------------------------------
// Verifying through the bundle is verifying the store it rehydrates — no more, no less.
// ---------------------------------------------------------------------------------------------

#[test]
fn verifying_through_the_bundle_equals_verifying_the_store() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let store = store_of(&f.facts);
    let direct = verify_device_act(&act, f.s.subject, &store, 3).expect("B is authorized at 3");

    let bundle = bundle_of(&f.facts, act);
    let through = verify_bundle(&bundle, f.s.subject, 3).expect("the bundle proves the same");
    assert_eq!(through, direct);
    assert_eq!(through.grant.granted_at, 2);

    let from_bytes = verify_bundle_bytes(&bundle.canonical_bytes(), f.s.subject, 3)
        .expect("bytes carry the same proof");
    assert_eq!(from_bytes, direct);
}

#[test]
fn a_revoked_device_is_refused_through_the_bundle() {
    let f = fixture();
    let bundle = bundle_of(&f.facts, signed(f.s.subject, f.a, &f.a_key, 3));
    assert!(matches!(
        verify_bundle(&bundle, f.s.subject, 3),
        Err(BundleVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        )))
    ));
}

#[test]
fn withholding_the_revocation_cannot_extend_the_revoked_devices_authority() {
    let f = fixture();
    let without_revoke = &f.facts[..3];
    // At the position of the withheld revoke, the prefix is incomplete: fail closed.
    let bundle = bundle_of(without_revoke, signed(f.s.subject, f.a, &f.a_key, 3));
    assert!(matches!(
        verify_bundle(&bundle, f.s.subject, 3),
        Err(BundleVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::PrefixIncomplete {
                frontier: 3,
                required: 3
            }
        )))
    ));
    // History before the revoke is intact: the same facts still prove A at 2.
    let bundle = bundle_of(without_revoke, signed(f.s.subject, f.a, &f.a_key, 2));
    let evidence = verify_bundle(&bundle, f.s.subject, 2).expect("A was authorized at 2");
    assert_eq!(evidence.grant.granted_at, 1);
}

#[test]
fn one_inadmissible_fact_fails_the_whole_bundle_rather_than_being_dropped() {
    let f = fixture();
    let act = signed(f.s.subject, f.b, &f.b_key, 3);
    let mut facts = f.facts.clone();
    // Corrupt the revoke fact's witness. A store that silently dropped it would still accept
    // B at 3; the bundle must refuse instead, naming the fact.
    let mut sig = *facts[3].signature.as_bytes();
    sig[5] ^= 0x01;
    facts[3] = SignedAuthorityEvent::new(
        facts[3].body.clone(),
        icn_identity::authority_log::WitnessSignature::from_bytes(sig),
    );
    let bundle = bundle_of(&facts, act);
    match verify_bundle(&bundle, f.s.subject, 3) {
        Err(BundleVerifyError::InadmissibleFact { index, .. }) => {
            let culprit = &bundle.facts()[index as usize];
            assert_eq!(
                culprit.body.position(),
                3,
                "the refused fact is the corrupted revoke"
            );
        }
        other => panic!("expected InadmissibleFact, got {other:?}"),
    }
}

#[test]
fn the_relying_party_supplies_the_subject_and_the_position_never_the_bundle() {
    let f = fixture();
    let bundle = bundle_of(&f.facts, signed(f.s.subject, f.b, &f.b_key, 3));
    let other = subject(0x02, 4).subject;
    assert!(matches!(
        verify_bundle(&bundle, other, 3),
        Err(BundleVerifyError::Act(
            DeviceActVerifyError::ActSubjectMismatch
        ))
    ));
    assert!(matches!(
        verify_bundle(&bundle, f.s.subject, 2),
        Err(BundleVerifyError::Act(
            DeviceActVerifyError::EvaluationPositionMismatch { act: 3, relying: 2 }
        ))
    ));
}

#[test]
fn verification_reads_no_wall_clock_and_no_environment() {
    // The module's only inputs are bytes and the relying party's (subject, position); the test
    // pins that it names nothing else. A reviewer can `grep` this list against the source.
    let src = include_str!("../src/device_authority_bundle.rs");
    // Code tokens, not words: prose in the module's own documentation may say "randomness".
    for forbidden in [
        "SystemTime",
        "Instant::now",
        "std::env",
        "std::fs",
        "std::net",
        "rand::",
        "OsRng",
        "thread_rng",
    ] {
        assert!(
            !src.contains(forbidden),
            "device_authority_bundle.rs must not use {forbidden}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Cross-implementation vector: the framing, over the N4-A act vector, with zero facts.
// `tests/reference/device_authority_bundle_reference.py` re-derives every byte from the
// contract's prose and compares against these literals.
// ---------------------------------------------------------------------------------------------

const VECTOR_SUBJECT_FIRST: u8 = 0x20;
const VECTOR_DEVICE_SEED_FIRST: u8 = 0x40;
const EXPECT_BUNDLE_BYTES_HEX: &str = "0000001e69636e2e6e342e6465766963652d617574686f726974792d62756e646c65000100000000000000790000001169636e2e6e342e6465766963652d6163740001202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f012543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d000000000000000301000000146b617469653a6f70656e2d776f726b73706163650e12ebbe9ab76d54de6a60385ecc27e4af8a601b8eb023f23726ae706f53b50b574342ba713987b5b09bdbe72dfb7d10497532e884cbc4e7751b69b918fe3306";
const EXPECT_BUNDLE_ID_HEX: &str =
    "a71dba711385b5940358a97ff8cb636b2d4276dfb0565fa40fc29433711722c2";

#[test]
fn canonical_bundle_vector_matches_the_independent_reference() {
    let mut subject_bytes = [0u8; 32];
    let mut seed = [0u8; 32];
    for i in 0..32u8 {
        subject_bytes[i as usize] = VECTOR_SUBJECT_FIRST + i;
        seed[i as usize] = VECTOR_DEVICE_SEED_FIRST + i;
    }
    let key = SigningKey::from_bytes(&seed);
    let device = PrincipalKey::try_from_verifying_key(key.verifying_key()).unwrap();
    let act = DeviceActV1::new(
        SubjectId::from_bytes(subject_bytes),
        device,
        DeviceCapability::Sign,
        3,
        b"katie:open-workspace".to_vec(),
    )
    .unwrap();
    let signed_act = sign_device_act(&act, &key).unwrap();
    let bundle = DeviceAuthorityBundleV1::new(std::iter::empty(), signed_act).unwrap();
    assert_eq!(
        hex::encode(bundle.canonical_bytes()),
        EXPECT_BUNDLE_BYTES_HEX
    );
    assert_eq!(hex::encode(bundle.bundle_id()), EXPECT_BUNDLE_ID_HEX);
    // A zero-fact bundle decodes, and verifying it refuses: no facts, no Subject.
    let decoded = DeviceAuthorityBundleV1::decode(&bundle.canonical_bytes()).unwrap();
    assert_eq!(decoded, bundle);
    assert!(matches!(
        verify_bundle(&decoded, SubjectId::from_bytes(subject_bytes), 3),
        Err(BundleVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::SubjectUnknown
        )))
    ));
}
