//! GEN-B — `SubjectRecognitionV1` obligations.
//!
//! # Where the golden vectors come from
//!
//! The `EXPECT_*` literals below were computed by `tests/reference/subject_recognition_reference.py`,
//! written from `docs/architecture/GEN_SUBJECT_RECOGNITION.md` alone. It shares no code with
//! `icn-governance` and starts from the *published* GEN-A §8 values (SubjectId,
//! `subject_context_ref`), not from Rust output. Run it to compare:
//!
//! ```text
//! python3 icn/crates/icn-governance/tests/reference/subject_recognition_reference.py
//! ```
//!
//! What the vectors validate: the byte layout, field order, framing and tags of the basis hash and
//! recognition hash. What they do not validate: GEN-A itself (pinned by its own vectors), any
//! institutional decision, or membership legitimacy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::any::TypeId;

use ed25519_dalek::SigningKey;
use icn_governance::domain::GovernanceDomainId;
use icn_governance::subject_recognition::{
    bootstrap_rehearsal_basis_hash, compute_recognition_hash, RecognitionBasisHash,
    RecognitionBasisKind, RecognitionError, StandingClass, SubjectRecognitionV1,
    BOOTSTRAP_REHEARSAL_BASIS_TAG, SUBJECT_RECOGNITION_TAG,
};
use icn_identity::authority_log::{ContinuityRoot, PrincipalKey, SubjectId};
use icn_identity::subject_context::{
    incept_subject_context_v1, verify_subject_context_genesis_v1, ContextSalt, GenesisVerifyError,
    SubjectContextDescriptor, SubjectContextGenesisV1, SubjectContextKind,
};
use icn_kernel_api::canonical::tags_are_prefix_free;

// ---------------------------------------------------------------------------------------------
// Inputs: identical to the GEN-A §8 vector inputs.
// ---------------------------------------------------------------------------------------------

const DOMAIN: &str = "coop.example.governance";
const OTHER_DOMAIN: &str = "coop.example.other";

fn seq(start: u8) -> [u8; 32] {
    let mut b = [0u8; 32];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = start.wrapping_add(i as u8);
    }
    b
}

fn device_for(seed_start: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(SigningKey::from_bytes(&seq(seed_start)).verifying_key())
        .unwrap()
}

fn bundle(domain: &str, secret_start: u8, device_seed: u8) -> SubjectContextGenesisV1 {
    let d = SubjectContextDescriptor::governance_domain_v1(domain, ContextSalt::from_bytes(seq(0)))
        .unwrap();
    let root = ContinuityRoot::new(seq(secret_start), d.context_nonce(), 4);
    incept_subject_context_v1(&d, &root, device_for(device_seed)).unwrap()
}

/// The exact GEN-A §8 vector bundle.
fn vector_bundle() -> SubjectContextGenesisV1 {
    bundle(DOMAIN, 0x80, 0x40)
}

fn domain() -> GovernanceDomainId {
    GovernanceDomainId::new(DOMAIN)
}

fn recognise(b: &SubjectContextGenesisV1) -> SubjectRecognitionV1 {
    SubjectRecognitionV1::recognize_bootstrap_rehearsal(&domain(), b).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Expected values from the independent Python reference.
// ---------------------------------------------------------------------------------------------

/// GEN-A §8, published.
const GEN_A_SUBJECT_ID: &str = "3b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a";
const GEN_A_SUBJECT_CONTEXT_REF: &str =
    "5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86";

const EXPECT_BASIS_PREIMAGE: &str = "69636e3a676f763a7265636f676e6974696f6e2d62617369733a626f6f7473747261702d72656865617273616c3a76311700000000000000636f6f702e6578616d706c652e676f7665726e616e63653b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86";
const EXPECT_BASIS_HASH: &str = "327aab543a13a4bda082ff2e921919b53e4b95b6c5ca4db965e5cde0a6dd252e";
const EXPECT_RECOGNITION_PREIMAGE: &str = "69636e3a676f763a7375626a6563742d7265636f676e6974696f6e3a76311700000000000000636f6f702e6578616d706c652e676f7665726e616e63653b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a0101327aab543a13a4bda082ff2e921919b53e4b95b6c5ca4db965e5cde0a6dd252e";
const EXPECT_RECOGNITION_HASH: &str =
    "916ce2f5420ef322d570f07d76e57abce8085fde05cb66a46b2a0d2485b3139f";
const EXPECT_OTHER_DOMAIN_BASIS_HASH: &str =
    "a9676bd71551d9d26bcaf4a20e3ba79b902b6c0658c4af390087243726fe8b75";
const EXPECT_OTHER_DOMAIN_RECOGNITION_HASH: &str =
    "5e7bc9db8ef7246acd3e3d569ce14d885ee9e2a2bd255515bae8b06383d2c916";

fn subject_from_hex(h: &str) -> SubjectId {
    let mut b = [0u8; 32];
    b.copy_from_slice(&hex::decode(h).unwrap());
    SubjectId::from_bytes(b)
}

fn context_ref_from_hex(h: &str) -> icn_identity::subject_context::SubjectContextRef {
    let mut b = [0u8; 32];
    b.copy_from_slice(&hex::decode(h).unwrap());
    icn_identity::subject_context::SubjectContextRef::from_bytes(b)
}

// ---------------------------------------------------------------------------------------------
// Positive obligations
// ---------------------------------------------------------------------------------------------

#[test]
fn vector_matches_the_independent_reference() {
    let rec = recognise(&vector_bundle());
    assert_eq!(hex::encode(rec.subject().as_bytes()), GEN_A_SUBJECT_ID);
    assert_eq!(hex::encode(rec.basis_hash().as_bytes()), EXPECT_BASIS_HASH);
    assert_eq!(
        hex::encode(rec.recognition_hash().as_bytes()),
        EXPECT_RECOGNITION_HASH
    );
}

#[test]
fn preimages_follow_the_documented_field_order() {
    // Rebuild both preimages by hand from the spec and hash them with the raw BLAKE3 crate, so
    // neither CanonicalHasher nor the module under test participates.
    let subject = hex::decode(GEN_A_SUBJECT_ID).unwrap();
    let cref = hex::decode(GEN_A_SUBJECT_CONTEXT_REF).unwrap();
    let dom = DOMAIN.as_bytes();

    let mut basis = Vec::new();
    basis.extend_from_slice(b"icn:gov:recognition-basis:bootstrap-rehearsal:v1");
    basis.extend_from_slice(&(dom.len() as u64).to_le_bytes());
    basis.extend_from_slice(dom);
    basis.extend_from_slice(&subject);
    basis.extend_from_slice(&cref);
    assert_eq!(hex::encode(&basis), EXPECT_BASIS_PREIMAGE);
    let basis_hash = blake3::hash(&basis);
    assert_eq!(basis_hash.to_hex().as_str(), EXPECT_BASIS_HASH);

    let mut rec = Vec::new();
    rec.extend_from_slice(b"icn:gov:subject-recognition:v1");
    rec.extend_from_slice(&(dom.len() as u64).to_le_bytes());
    rec.extend_from_slice(dom);
    rec.extend_from_slice(&subject);
    rec.push(0x01); // standing_class = Member
    rec.push(0x01); // basis kind = BootstrapRehearsalV1
    rec.extend_from_slice(basis_hash.as_bytes());
    assert_eq!(hex::encode(&rec), EXPECT_RECOGNITION_PREIMAGE);
    assert_eq!(
        blake3::hash(&rec).to_hex().as_str(),
        EXPECT_RECOGNITION_HASH
    );
}

#[test]
fn helper_functions_agree_with_the_vector() {
    let subject = subject_from_hex(GEN_A_SUBJECT_ID);
    let cref = context_ref_from_hex(GEN_A_SUBJECT_CONTEXT_REF);
    let bh = bootstrap_rehearsal_basis_hash(DOMAIN, &subject, &cref);
    assert_eq!(hex::encode(bh.as_bytes()), EXPECT_BASIS_HASH);
    let rh = compute_recognition_hash(
        DOMAIN,
        &subject,
        StandingClass::Member,
        RecognitionBasisKind::BootstrapRehearsalV1,
        &bh,
    );
    assert_eq!(hex::encode(rh.as_bytes()), EXPECT_RECOGNITION_HASH);

    // Same Subject and context ref under a different domain string: pinned second vector.
    let bh2 = bootstrap_rehearsal_basis_hash(OTHER_DOMAIN, &subject, &cref);
    assert_eq!(hex::encode(bh2.as_bytes()), EXPECT_OTHER_DOMAIN_BASIS_HASH);
    let rh2 = compute_recognition_hash(
        OTHER_DOMAIN,
        &subject,
        StandingClass::Member,
        RecognitionBasisKind::BootstrapRehearsalV1,
        &bh2,
    );
    assert_eq!(
        hex::encode(rh2.as_bytes()),
        EXPECT_OTHER_DOMAIN_RECOGNITION_HASH
    );
}

#[test]
fn accepted_recognition_retains_domain_subject_and_semantics() {
    let b = vector_bundle();
    let verified =
        verify_subject_context_genesis_v1(&b, SubjectContextKind::GovernanceDomainV1, DOMAIN)
            .unwrap();
    let rec = recognise(&b);
    assert_eq!(rec.domain_id().0.as_bytes(), DOMAIN.as_bytes());
    assert_eq!(rec.subject(), &verified.subject);
    assert_eq!(rec.standing_class(), StandingClass::Member);
    assert_eq!(rec.basis_kind(), RecognitionBasisKind::BootstrapRehearsalV1);
}

#[test]
fn equal_inputs_give_equal_hashes_across_repeats() {
    let a = recognise(&vector_bundle());
    for _ in 0..5 {
        assert_eq!(a, recognise(&vector_bundle()));
    }
    assert!(a.verify_against_genesis(&vector_bundle()).is_ok());
}

// ---------------------------------------------------------------------------------------------
// Refusals at the construction boundary
// ---------------------------------------------------------------------------------------------

#[test]
fn wrong_governance_domain_is_refused() {
    let err = SubjectRecognitionV1::recognize_bootstrap_rehearsal(
        &GovernanceDomainId::new(OTHER_DOMAIN),
        &vector_bundle(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        RecognitionError::Genesis(GenesisVerifyError::ContextIdMismatch)
    );
}

#[test]
fn empty_domain_is_refused() {
    let err = SubjectRecognitionV1::recognize_bootstrap_rehearsal(
        &GovernanceDomainId::new(""),
        &vector_bundle(),
    )
    .unwrap_err();
    assert_eq!(err, RecognitionError::EmptyDomain);
}

#[test]
fn alias_and_renormalised_spellings_are_refused() {
    for alias in [
        "Coop.Example.Governance",  // case
        "coop.example.governance ", // trailing space
        " coop.example.governance", // leading space
        "coop.example.governance\0",
        "coop.example.governance/", // trailing delimiter
    ] {
        let err = SubjectRecognitionV1::recognize_bootstrap_rehearsal(
            &GovernanceDomainId::new(alias),
            &vector_bundle(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            RecognitionError::Genesis(GenesisVerifyError::ContextIdMismatch),
            "alias {alias:?} must not match"
        );
    }
}

#[test]
fn unicode_normalisation_forms_are_distinct_domains() {
    // "é" precomposed (NFC) vs "e" + U+0301 (NFD): different bytes, so different domains. No
    // normalisation happens anywhere; a bundle for one cannot recognise in the other.
    let nfc = "coop.caf\u{e9}";
    let nfd = "coop.cafe\u{301}";
    assert_ne!(nfc.as_bytes(), nfd.as_bytes());
    let b = bundle(nfc, 0x80, 0x40);
    assert!(
        SubjectRecognitionV1::recognize_bootstrap_rehearsal(&GovernanceDomainId::new(nfc), &b)
            .is_ok()
    );
    assert!(
        SubjectRecognitionV1::recognize_bootstrap_rehearsal(&GovernanceDomainId::new(nfd), &b)
            .is_err()
    );
}

#[test]
fn tampered_genesis_is_refused() {
    let mut b = vector_bundle();
    b.version = 2;
    let err = SubjectRecognitionV1::recognize_bootstrap_rehearsal(&domain(), &b).unwrap_err();
    assert_eq!(
        err,
        RecognitionError::Genesis(GenesisVerifyError::UnsupportedVersion(2))
    );

    let mut b = vector_bundle();
    let mut salt = *b.context_salt.as_bytes();
    salt[0] ^= 1;
    b.context_salt = ContextSalt::from_bytes(salt);
    let err = SubjectRecognitionV1::recognize_bootstrap_rehearsal(&domain(), &b).unwrap_err();
    assert_eq!(
        err,
        RecognitionError::Genesis(GenesisVerifyError::ContextNonceMismatch)
    );
}

#[test]
fn unsupported_standing_and_basis_tags_are_refused() {
    assert_eq!(StandingClass::from_tag(0x01), Ok(StandingClass::Member));
    assert_eq!(
        RecognitionBasisKind::from_tag(0x01),
        Ok(RecognitionBasisKind::BootstrapRehearsalV1)
    );
    for t in [0x00u8, 0x02, 0x7f, 0xff] {
        assert_eq!(
            StandingClass::from_tag(t),
            Err(RecognitionError::UnsupportedStandingClass(t))
        );
        assert_eq!(
            RecognitionBasisKind::from_tag(t),
            Err(RecognitionError::UnsupportedBasisKind(t))
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Identity substitution, reuse, and authority confusion
// ---------------------------------------------------------------------------------------------

#[test]
fn recognition_of_one_subject_does_not_verify_against_another_subjects_genesis() {
    // Same domain, different continuity secret => a different Subject.
    let other = bundle(DOMAIN, 0x11, 0x40);
    let rec_a = recognise(&vector_bundle());
    let rec_b = recognise(&other);
    assert_ne!(rec_a.subject(), rec_b.subject());
    assert_ne!(rec_a.recognition_hash(), rec_b.recognition_hash());
    assert_eq!(
        rec_a.verify_against_genesis(&other),
        Err(RecognitionError::Mismatch)
    );
}

#[test]
fn recognition_cannot_be_reused_in_another_context() {
    let rec = recognise(&vector_bundle());
    // A genesis incepted for a different domain cannot back a recognition in this one.
    let elsewhere = bundle(OTHER_DOMAIN, 0x80, 0x40);
    assert_eq!(
        rec.verify_against_genesis(&elsewhere),
        Err(RecognitionError::Genesis(
            GenesisVerifyError::ContextIdMismatch
        ))
    );
    // And the basis commits the domain: same Subject + context ref in another domain differs.
    let subject = subject_from_hex(GEN_A_SUBJECT_ID);
    let cref = context_ref_from_hex(GEN_A_SUBJECT_CONTEXT_REF);
    assert_ne!(
        bootstrap_rehearsal_basis_hash(DOMAIN, &subject, &cref),
        bootstrap_rehearsal_basis_hash(OTHER_DOMAIN, &subject, &cref)
    );
}

#[test]
fn a_principal_is_not_a_subject() {
    // Type level: the constructor derives the Subject from a verified genesis and offers no
    // parameter for one, so a device Principal cannot be substituted. The two types are distinct.
    assert_ne!(TypeId::of::<SubjectId>(), TypeId::of::<PrincipalKey>());

    // Value level: even the raw bytes of the device key, forced into a SubjectId, would commit a
    // different recognition than the real one, and no genesis can verify it.
    let real = recognise(&vector_bundle());
    let device_bytes = vector_bundle_device().as_bytes();
    let forged_subject = SubjectId::from_bytes(device_bytes);
    let forged = compute_recognition_hash(
        DOMAIN,
        &forged_subject,
        StandingClass::Member,
        RecognitionBasisKind::BootstrapRehearsalV1,
        real.basis_hash(),
    );
    assert_ne!(&forged, real.recognition_hash());
}

fn vector_bundle_device() -> PrincipalKey {
    device_for(0x40)
}

#[test]
fn recognition_is_independent_of_the_current_device() {
    // Same Subject (same secret/salt/domain), different initial device => identical recognition.
    // Recognition names the Subject; it neither contains nor depends on any device authority, and
    // GEN-A verification is historical, so it cannot claim the device is authorized now.
    let a = recognise(&bundle(DOMAIN, 0x80, 0x40));
    let b = recognise(&bundle(DOMAIN, 0x80, 0x60));
    assert_eq!(a, b);
}

#[test]
fn rehearsal_basis_is_never_production_grade() {
    assert!(!RecognitionBasisKind::BootstrapRehearsalV1.is_production_grade());
    let rec = recognise(&vector_bundle());
    assert!(!rec.basis_kind().is_production_grade());
}

// ---------------------------------------------------------------------------------------------
// Mutation, framing, and domain separation
// ---------------------------------------------------------------------------------------------

#[test]
fn mutating_any_committed_field_changes_the_hash() {
    let rec = recognise(&vector_bundle());
    let base = *rec.recognition_hash();
    let kind = RecognitionBasisKind::BootstrapRehearsalV1;
    let standing = StandingClass::Member;

    // domain
    assert_ne!(
        compute_recognition_hash(
            OTHER_DOMAIN,
            rec.subject(),
            standing,
            kind,
            rec.basis_hash()
        ),
        base
    );
    // subject: flip each of a few bit positions
    for i in [0usize, 15, 31] {
        let mut s = *rec.subject().as_bytes();
        s[i] ^= 0x01;
        assert_ne!(
            compute_recognition_hash(
                DOMAIN,
                &SubjectId::from_bytes(s),
                standing,
                kind,
                rec.basis_hash()
            ),
            base
        );
    }
    // basis hash: any different basis hash. The only public producer is the basis derivation, so
    // use the other domain's basis hash as an arbitrary different value.
    let different_basis = bootstrap_rehearsal_basis_hash(
        OTHER_DOMAIN,
        rec.subject(),
        &context_ref_from_hex(GEN_A_SUBJECT_CONTEXT_REF),
    );
    assert_ne!(
        compute_recognition_hash(DOMAIN, rec.subject(), standing, kind, &different_basis),
        base
    );
    // standing and basis kind have exactly one v1 variant: mutation is unrepresentable, and the
    // tag-parsing test above covers every other byte value.
}

#[test]
fn length_prefix_prevents_trading_bytes_between_domain_and_subject() {
    // Without a length prefix, domain "X" + subject S and domain "X||S[0]" + subject S[1..]||?
    // could collide. Subject is fixed-width, so build the nearest adversarial pair and require
    // different hashes.
    let s = subject_from_hex(GEN_A_SUBJECT_ID);
    let kind = RecognitionBasisKind::BootstrapRehearsalV1;
    let bh = bootstrap_rehearsal_basis_hash(
        DOMAIN,
        &s,
        &context_ref_from_hex(GEN_A_SUBJECT_CONTEXT_REF),
    );
    let longer = format!("{DOMAIN}\u{3b}"); // 0x3b is the first byte of the Subject
    let h1 = compute_recognition_hash(DOMAIN, &s, StandingClass::Member, kind, &bh);
    let mut shifted = *s.as_bytes();
    shifted.rotate_left(1);
    let h2 = compute_recognition_hash(
        &longer,
        &SubjectId::from_bytes(shifted),
        StandingClass::Member,
        kind,
        &bh,
    );
    assert_ne!(h1, h2);
}

#[test]
fn new_tags_are_prefix_free_and_distinct_from_each_other_and_gen_a() {
    assert!(tags_are_prefix_free(&[
        SUBJECT_RECOGNITION_TAG,
        BOOTSTRAP_REHEARSAL_BASIS_TAG
    ]));
    // GEN-A domains use a different namespace ("icn.gen." with u32be length framing, SHA-256);
    // none can equal or prefix a printable "icn:" tag.
    for gen_a in [
        icn_identity::subject_context::GEN_CONTEXT_DOMAIN,
        icn_identity::subject_context::SUBJECT_CONTEXT_REF_DOMAIN,
        icn_identity::subject_context::INITIAL_DEVICE_BINDING_REF_DOMAIN,
    ] {
        assert!(!SUBJECT_RECOGNITION_TAG.as_bytes().starts_with(gen_a));
        assert!(!BOOTSTRAP_REHEARSAL_BASIS_TAG.as_bytes().starts_with(gen_a));
        assert!(!gen_a.starts_with(SUBJECT_RECOGNITION_TAG.as_bytes()));
    }
}

#[test]
fn the_two_tags_separate_identical_field_material() {
    // The basis hash and recognition hash commit different layouts, but separate them by tag too:
    // identical field bytes hashed under each tag must differ.
    use icn_kernel_api::canonical::CanonicalHasher;
    let mut a = CanonicalHasher::new(SUBJECT_RECOGNITION_TAG);
    a.str(DOMAIN);
    let mut b = CanonicalHasher::new(BOOTSTRAP_REHEARSAL_BASIS_TAG);
    b.str(DOMAIN);
    assert_ne!(a.finish(), b.finish());
}

#[test]
fn basis_hash_type_has_no_public_constructor() {
    // `RecognitionBasisHash` can only be obtained from a basis derivation, so an arbitrary or
    // all-zero 32 bytes cannot be presented as a basis. This is a compile-time property; the
    // assertion below only pins that the type is the one the producer returns.
    let _: RecognitionBasisHash = bootstrap_rehearsal_basis_hash(
        DOMAIN,
        &subject_from_hex(GEN_A_SUBJECT_ID),
        &context_ref_from_hex(GEN_A_SUBJECT_CONTEXT_REF),
    );
}
