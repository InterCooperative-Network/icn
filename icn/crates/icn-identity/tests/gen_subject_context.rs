//! GEN-A — context-scoped Subject genesis obligations (#2695).
//!
//! # Where the golden vectors come from
//!
//! The fixed hex below was **not** produced by this implementation. It was computed by an
//! independent reference written in Python directly from the specification prose in
//! `docs/architecture/GEN_SUBJECT_CONTEXT_GENESIS.md`, sharing no code with `icn-identity`.
//! A vector that a Rust test generates and then compares against itself proves only that the
//! code is deterministic; these prove the *specification* is reproducible by a second
//! implementation, which is the property GEN-A actually needs.
//!
//! To regenerate them in another language, implement the document and feed it the inputs in
//! [`vector_inputs`] below.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    authorize_event, sign_body, AdmissionError, AuthorityBody, CapabilitySet, CodecError,
    ContextNonce, ContinuityRoot, DeviceCapability, EventId, PrincipalKey, SubjectId, ValiditySpan,
    WitnessSignature,
};
use icn_identity::subject_context::{
    alpha_initial_device_capabilities, incept_subject_context_v1, initial_device_binding_ref,
    verify_subject_context_genesis_v1, ContextSalt, GenesisVerifyError, SubjectContextDescriptor,
    SubjectContextError, SubjectContextGenesisV1, SubjectContextKind, GEN_CONTEXT_DOMAIN,
    INITIAL_DEVICE_BINDING_REF_DOMAIN, SUBJECT_CONTEXT_GENESIS_VERSION, SUBJECT_CONTEXT_REF_DOMAIN,
};

// ---------------------------------------------------------------------------------------------
// Fixed vector inputs. Stated here in full so another implementation can reproduce them.
// ---------------------------------------------------------------------------------------------

/// `context_id` — the exact UTF-8 string whose bytes enter the preimage.
const VECTOR_CONTEXT_ID: &str = "coop.example.governance";
/// `context_salt[i] = i` for i in 0..32.
fn vector_salt() -> ContextSalt {
    let mut b = [0u8; 32];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = i as u8;
    }
    ContextSalt::from_bytes(b)
}
/// `continuity_secret[i] = 0x80 + i` for i in 0..32.
fn vector_secret() -> [u8; 32] {
    let mut b = [0u8; 32];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = 0x80u8.wrapping_add(i as u8);
    }
    b
}
/// `device_seed[i] = 0x40 + i` for i in 0..32.
fn vector_device_key() -> SigningKey {
    let mut b = [0u8; 32];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = 0x40u8.wrapping_add(i as u8);
    }
    SigningKey::from_bytes(&b)
}
/// The establishment plan the vectors pin: `[Rotate; 4]`, i.e. horizon 4.
const VECTOR_HORIZON: usize = 4;

/// Human-readable restatement of every vector input, for cross-implementation use.
#[test]
fn vector_inputs() {
    assert_eq!(VECTOR_CONTEXT_ID, "coop.example.governance");
    assert_eq!(hex::encode(vector_salt().as_bytes()), VEC_SALT_HEX);
    assert_eq!(hex::encode(vector_secret()), VEC_SECRET_HEX);
    assert_eq!(VECTOR_HORIZON, 4);
}

const VEC_SALT_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const VEC_SECRET_HEX: &str = "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f";

// ---------------------------------------------------------------------------------------------
// Expected outputs, from the independent Python reference.
// ---------------------------------------------------------------------------------------------

const EXPECT_CONTEXT_PREIMAGE: &str = "0000001769636e2e67656e2e7375626a6563742d636f6e7465787400010100000017636f6f702e6578616d706c652e676f7665726e616e6365000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const EXPECT_CONTEXT_NONCE: &str =
    "208c2b372130c8479460ddc56a0d581caa5016a47df54ed08f1357c45041e3cf";
const EXPECT_INITIAL_AUTHORITY_PUB: &str =
    "6438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636ba";
const EXPECT_NEXT_COMMITMENT_C1: &str =
    "d04408dd67899efb069fbb5930860464f34dde35816d5d469dcafc2639fcfa1a";
const EXPECT_DEVICE_PUB: &str = "2543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d";
const EXPECT_INCEPTION_BODY: &str = "0000001169636e2e617574686f726974792d6c6f67000101016438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636ba208c2b372130c8479460ddc56a0d581caa5016a47df54ed08f1357c45041e3cf00000001016438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636bad04408dd67899efb069fbb5930860464f34dde35816d5d469dcafc2639fcfa1a";
const EXPECT_SUBJECT_ID: &str = "3b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a";
const EXPECT_AUTHORIZE_BODY: &str = "0000001169636e2e617574686f726974792d6c6f670001033b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a00000000000000013b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a016438475a7044bca357e049c1fcb255ce9f791ec05b44ddb06d9566b9a01636ba012543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d00000002010300";
const EXPECT_AUTHORIZE_EVENT_ID: &str =
    "e600a622c3a727fd48b55f31ee7deb3d91bc1be518ee38f0211b4d575f61af40";
const EXPECT_SUBJECT_CONTEXT_REF: &str =
    "5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86";
const EXPECT_INITIAL_DEVICE_BINDING_REF: &str =
    "b2ff12700d18618947fdc6a6b02310580dd95c7b5e3b91f4cc7de338a197d3ad";

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn vector_descriptor() -> SubjectContextDescriptor {
    SubjectContextDescriptor::governance_domain_v1(VECTOR_CONTEXT_ID, vector_salt()).unwrap()
}

fn vector_root(descriptor: &SubjectContextDescriptor) -> ContinuityRoot {
    ContinuityRoot::new(vector_secret(), descriptor.context_nonce(), VECTOR_HORIZON)
}

fn vector_bundle() -> SubjectContextGenesisV1 {
    let d = vector_descriptor();
    let root = vector_root(&d);
    incept_subject_context_v1(&d, &root, device_principal()).unwrap()
}

fn verify_vector(bundle: &SubjectContextGenesisV1) -> GenesisVerifyError {
    verify_subject_context_genesis_v1(
        bundle,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .expect_err("bundle should have been rejected")
}

/// The device Principal for the vector device key.
fn device_principal() -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(vector_device_key().verifying_key()).unwrap()
}

/// A second *valid* signature over one body: same scalar, different nonce prefix.
fn alternate_witness(key: &SigningKey, body: &AuthorityBody, prefix: u8) -> WitnessSignature {
    use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
    use sha2::Sha512;

    let seed = key.to_bytes();
    let mut expanded = ExpandedSecretKey::from(&seed);
    expanded.hash_prefix = [prefix; 32];
    let sig = raw_sign::<Sha512>(&expanded, &body.signature_preimage(), &key.verifying_key());
    WitnessSignature::from_bytes(sig.to_bytes())
}

// =============================================================================================
// 1. Golden vectors — cross-implementation reproducibility
// =============================================================================================

#[test]
fn context_preimage_and_nonce_match_the_independent_reference() {
    let d = vector_descriptor();
    assert_eq!(hex::encode(d.context_preimage()), EXPECT_CONTEXT_PREIMAGE);
    assert_eq!(
        hex::encode(d.context_nonce().as_bytes()),
        EXPECT_CONTEXT_NONCE
    );
}

/// The three intermediate values the spec's vector table lists are asserted directly, not only
/// transitively via the body bytes that contain them.
#[test]
fn component_keys_and_commitment_match_the_independent_reference() {
    let d = vector_descriptor();
    let root = vector_root(&d);

    let authority = root.authority_set(0).canonical_signer().unwrap();
    assert_eq!(
        hex::encode(authority.as_bytes()),
        EXPECT_INITIAL_AUTHORITY_PUB
    );
    assert_eq!(
        hex::encode(root.commitment(1).as_bytes()),
        EXPECT_NEXT_COMMITMENT_C1
    );
    assert_eq!(
        hex::encode(device_principal().as_bytes()),
        EXPECT_DEVICE_PUB
    );

    // And they really are the bytes embedded in the canonical bodies.
    let bundle = vector_bundle();
    assert!(hex::encode(&bundle.inception_body_bytes).contains(EXPECT_INITIAL_AUTHORITY_PUB));
    assert!(hex::encode(&bundle.inception_body_bytes).contains(EXPECT_NEXT_COMMITMENT_C1));
    assert!(hex::encode(&bundle.authorize_body_bytes).contains(EXPECT_DEVICE_PUB));
}

#[test]
fn inception_bytes_and_subject_id_match_the_independent_reference() {
    let bundle = vector_bundle();
    assert_eq!(
        hex::encode(&bundle.inception_body_bytes),
        EXPECT_INCEPTION_BODY
    );

    let verified = verify_subject_context_genesis_v1(
        &bundle,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();
    assert_eq!(hex::encode(verified.subject.as_bytes()), EXPECT_SUBJECT_ID);
}

#[test]
fn authorize_bytes_and_both_references_match_the_independent_reference() {
    let bundle = vector_bundle();
    assert_eq!(
        hex::encode(&bundle.authorize_body_bytes),
        EXPECT_AUTHORIZE_BODY
    );

    let v = verify_subject_context_genesis_v1(
        &bundle,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();
    assert_eq!(
        hex::encode(v.initial_authorize_event_id.as_bytes()),
        EXPECT_AUTHORIZE_EVENT_ID
    );
    assert_eq!(
        hex::encode(v.subject_context_ref.as_bytes()),
        EXPECT_SUBJECT_CONTEXT_REF
    );
    assert_eq!(
        hex::encode(v.initial_device_binding_ref.as_bytes()),
        EXPECT_INITIAL_DEVICE_BINDING_REF
    );
}

#[test]
fn a_valid_bundle_verifies_and_reports_the_expected_shape() {
    let bundle = vector_bundle();
    let v = verify_subject_context_genesis_v1(
        &bundle,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();

    assert_eq!(v.frontier, 2, "both events must be folded into the chain");
    assert_eq!(v.authority.generation, 0, "no rotation has occurred");
    assert_eq!(v.authority.devices.len(), 1);
    assert_eq!(v.device, device_principal());
    let grant = v.authority.devices.get(&v.device).unwrap();
    assert_eq!(grant.capabilities, alpha_initial_device_capabilities());
    assert!(grant.validity.is_none());
    assert_eq!(grant.granted_at, 1);
}

// =============================================================================================
// 2. Determinism, context separation, and the continuity-plan invariant
// =============================================================================================

#[test]
fn the_same_full_configuration_reproduces_a_byte_identical_genesis() {
    let first = vector_bundle();
    let second = vector_bundle();
    assert_eq!(
        first, second,
        "same secret + plan + context + salt must reproduce the same Subject"
    );
}

#[test]
fn a_different_context_id_yields_a_different_subject() {
    let a = vector_descriptor();
    let b = SubjectContextDescriptor::governance_domain_v1("coop.other.governance", vector_salt())
        .unwrap();
    assert_ne!(a.context_nonce(), b.context_nonce());

    let sa = vector_root(&a).subject_id().unwrap();
    let sb = ContinuityRoot::new(vector_secret(), b.context_nonce(), VECTOR_HORIZON)
        .subject_id()
        .unwrap();
    assert_ne!(sa, sb, "context is part of the Subject's genesis input");
}

#[test]
fn a_different_salt_yields_a_different_subject() {
    let a = vector_descriptor();
    let b = SubjectContextDescriptor::governance_domain_v1(
        VECTOR_CONTEXT_ID,
        ContextSalt::from_bytes([0xff; 32]),
    )
    .unwrap();
    assert_ne!(a.context_nonce(), b.context_nonce());

    let sa = vector_root(&a).subject_id().unwrap();
    let sb = ContinuityRoot::new(vector_secret(), b.context_nonce(), VECTOR_HORIZON)
        .subject_id()
        .unwrap();
    assert_ne!(sa, sb);
}

/// The salt is what satisfies invariant I4b: two people in one governance domain must not
/// publish the same nonce.
#[test]
fn two_subjects_in_one_context_publish_unlinkable_nonces() {
    let a = SubjectContextDescriptor::governance_domain_v1(
        VECTOR_CONTEXT_ID,
        ContextSalt::from_bytes([0x01; 32]),
    )
    .unwrap();
    let b = SubjectContextDescriptor::governance_domain_v1(
        VECTOR_CONTEXT_ID,
        ContextSalt::from_bytes([0x02; 32]),
    )
    .unwrap();
    assert_ne!(a.context_nonce(), b.context_nonce());
}

/// A recovery path that reconstructs a *different* establishment plan has not reconstructed the
/// same Subject, even with the same secret, context and salt.
#[test]
fn a_different_plan_horizon_changes_the_subject() {
    let d = vector_descriptor();
    let same = ContinuityRoot::new(vector_secret(), d.context_nonce(), VECTOR_HORIZON)
        .subject_id()
        .unwrap();
    let other = ContinuityRoot::new(vector_secret(), d.context_nonce(), VECTOR_HORIZON + 1)
        .subject_id()
        .unwrap();
    assert_ne!(
        same, other,
        "next_commitment is inside the inception body, so the plan is part of genesis"
    );
}

#[test]
fn a_root_built_with_a_foreign_nonce_is_refused() {
    let d = vector_descriptor();
    let foreign = ContinuityRoot::new(
        vector_secret(),
        ContextNonce::from_bytes([0x77; 32]),
        VECTOR_HORIZON,
    );
    assert_eq!(
        incept_subject_context_v1(&d, &foreign, device_principal()).unwrap_err(),
        SubjectContextError::ContextNonceMismatch
    );
}

// =============================================================================================
// 3. Reference stability — device independence and witness independence
// =============================================================================================

#[test]
fn changing_the_initial_device_leaves_the_subject_context_reference_unchanged() {
    let d = vector_descriptor();
    let root = vector_root(&d);
    let first = incept_subject_context_v1(&d, &root, device_principal()).unwrap();
    let second = incept_subject_context_v1(
        &d,
        &root,
        PrincipalKey::try_from_verifying_key(SigningKey::from_bytes(&[0x09; 32]).verifying_key())
            .unwrap(),
    )
    .unwrap();

    let a = verify_subject_context_genesis_v1(
        &first,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();
    let b = verify_subject_context_genesis_v1(
        &second,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();

    assert_eq!(a.subject, b.subject);
    assert_eq!(
        a.subject_context_ref, b.subject_context_ref,
        "recognition must survive device replacement"
    );
    assert_ne!(a.initial_device_binding_ref, b.initial_device_binding_ref);
}

#[test]
fn a_second_valid_witness_changes_neither_semantic_reference() {
    let d = vector_descriptor();
    let root = vector_root(&d);
    let bundle = incept_subject_context_v1(&d, &root, device_principal()).unwrap();

    let inception_body = AuthorityBody::decode(&bundle.inception_body_bytes).unwrap();
    let authorize_body = AuthorityBody::decode(&bundle.authorize_body_bytes).unwrap();
    let key = root.authority_signing_key(0);

    let resigned = SubjectContextGenesisV1 {
        inception_witness: alternate_witness(&key, &inception_body, 0x5a),
        authorize_witness: alternate_witness(&key, &authorize_body, 0xa5),
        ..bundle.clone()
    };
    assert_ne!(resigned.inception_witness, bundle.inception_witness);

    let a = verify_subject_context_genesis_v1(
        &bundle,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();
    let b = verify_subject_context_genesis_v1(
        &resigned,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();

    assert_eq!(a.subject, b.subject);
    assert_eq!(a.subject_context_ref, b.subject_context_ref);
    assert_eq!(a.initial_device_binding_ref, b.initial_device_binding_ref);
}

// =============================================================================================
// 4. Domain separation and encoding hygiene
// =============================================================================================

#[test]
fn the_three_gen_domains_are_distinct_from_each_other_and_from_n1() {
    use icn_identity::authority_log::{COMMITMENT_DOMAIN, DOMAIN, KDF_DOMAIN, SIGNATURE_DOMAIN};
    let all = [
        GEN_CONTEXT_DOMAIN,
        SUBJECT_CONTEXT_REF_DOMAIN,
        INITIAL_DEVICE_BINDING_REF_DOMAIN,
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "domain separators must be pairwise distinct");
        }
    }
}

/// Every GEN preimage is length-prefixed with its own domain, so no GEN preimage can be a
/// prefix of, or equal to, an N1 canonical body.
#[test]
fn a_gen_preimage_is_not_a_valid_n1_body() {
    let d = vector_descriptor();
    assert!(AuthorityBody::decode(&d.context_preimage()).is_err());
}

#[test]
fn no_did_string_spelling_enters_the_canonical_bytes() {
    let bundle = vector_bundle();
    for bytes in [&bundle.inception_body_bytes, &bundle.authorize_body_bytes] {
        let as_text = String::from_utf8_lossy(bytes);
        assert!(
            !as_text.contains("did:"),
            "principals are raw keys, not DID strings"
        );
    }
    // The raw device key is present verbatim.
    let device = device_principal().as_bytes();
    assert!(bundle.authorize_body_bytes.windows(32).any(|w| w == device));
}

// =============================================================================================
// 5. Rejection behaviour — every one of these must fail closed
// =============================================================================================

#[test]
fn an_empty_context_id_is_rejected_at_construction() {
    assert_eq!(
        SubjectContextDescriptor::governance_domain_v1("", vector_salt()).unwrap_err(),
        SubjectContextError::EmptyContextId
    );
}

#[test]
fn an_empty_context_id_is_rejected_at_verification() {
    let mut bundle = vector_bundle();
    bundle.context_id = String::new();
    assert_eq!(
        verify_subject_context_genesis_v1(&bundle, SubjectContextKind::GovernanceDomainV1, ""),
        Err(GenesisVerifyError::EmptyContextId)
    );
}

#[test]
fn an_unsupported_version_is_rejected() {
    let mut bundle = vector_bundle();
    bundle.version = SUBJECT_CONTEXT_GENESIS_VERSION + 1;
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::UnsupportedVersion(2)
    );
}

#[test]
fn a_claim_about_a_different_context_is_rejected() {
    let bundle = vector_bundle();
    assert_eq!(
        verify_subject_context_genesis_v1(
            &bundle,
            SubjectContextKind::GovernanceDomainV1,
            "coop.other.governance"
        ),
        Err(GenesisVerifyError::ContextIdMismatch)
    );
}

/// Replay across contexts: relabelling a genuine bundle cannot move it to another context,
/// because the inception body commits the nonce the original context derived.
#[test]
fn a_bundle_cannot_be_relabelled_into_another_context() {
    let bundle = vector_bundle();
    let relabelled = SubjectContextGenesisV1 {
        context_id: "coop.other.governance".to_string(),
        ..bundle
    };
    assert_eq!(
        verify_subject_context_genesis_v1(
            &relabelled,
            SubjectContextKind::GovernanceDomainV1,
            "coop.other.governance"
        ),
        Err(GenesisVerifyError::ContextNonceMismatch)
    );
}

/// Swapping the salt alone cannot make the bundle mean something else either.
#[test]
fn a_mutated_salt_is_rejected() {
    let bundle = vector_bundle();
    let mutated = SubjectContextGenesisV1 {
        context_salt: ContextSalt::from_bytes([0xab; 32]),
        ..bundle
    };
    assert_eq!(
        verify_vector(&mutated),
        GenesisVerifyError::ContextNonceMismatch
    );
}

#[test]
fn trailing_bytes_on_a_canonical_body_are_rejected() {
    let mut bundle = vector_bundle();
    bundle.inception_body_bytes.push(0x00);
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::InceptionNotAdmissible(AdmissionError::Codec(
            CodecError::TrailingBytes(1)
        ))
    );
}

#[test]
fn a_bad_inception_witness_is_rejected() {
    let mut bundle = vector_bundle();
    let mut sig = *bundle.inception_witness.as_bytes();
    sig[0] ^= 0xff;
    bundle.inception_witness = WitnessSignature::from_bytes(sig);
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::InceptionNotAdmissible(AdmissionError::SignatureInvalid)
    );
}

#[test]
fn a_bad_authorize_witness_is_rejected() {
    let mut bundle = vector_bundle();
    let mut sig = *bundle.authorize_witness.as_bytes();
    sig[0] ^= 0xff;
    bundle.authorize_witness = WitnessSignature::from_bytes(sig);
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::AuthorizeNotAdmissible(AdmissionError::SignatureInvalid)
    );
}

/// Build a bundle whose authorize event differs from the Alpha profile in exactly one way.
fn bundle_with_authorize(
    subject: SubjectId,
    position: u64,
    prev: EventId,
    caps: CapabilitySet,
    validity: Option<ValiditySpan>,
) -> SubjectContextGenesisV1 {
    let d = vector_descriptor();
    let root = vector_root(&d);
    let base = incept_subject_context_v1(&d, &root, device_principal()).unwrap();
    let event = authorize_event(
        &root.authority_signing_key(0),
        subject,
        position,
        prev,
        device_principal(),
        caps,
        validity,
    );
    SubjectContextGenesisV1 {
        authorize_body_bytes: event.body.canonical_bytes(),
        authorize_witness: event.signature,
        ..base
    }
}

fn vector_subject_and_inception() -> (SubjectId, EventId) {
    let d = vector_descriptor();
    let event = vector_root(&d).incept().unwrap();
    (event.body.subject(), event.body.event_id())
}

#[test]
fn an_authorize_naming_another_subject_is_rejected() {
    let (_, prev) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(
        SubjectId::from_bytes([0x33; 32]),
        1,
        prev,
        alpha_initial_device_capabilities(),
        None,
    );
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::AuthorizeSubjectMismatch
    );
}

#[test]
fn an_authorize_at_the_wrong_position_is_rejected() {
    let (subject, prev) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(subject, 2, prev, alpha_initial_device_capabilities(), None);
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::AuthorizePositionMismatch(2)
    );
}

#[test]
fn an_authorize_naming_the_wrong_parent_is_rejected() {
    let (subject, _) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(
        subject,
        1,
        EventId::from_bytes([0x44; 32]),
        alpha_initial_device_capabilities(),
        None,
    );
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::AuthorizeParentMismatch
    );
}

#[test]
fn an_extra_capability_is_rejected() {
    let (subject, prev) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(
        subject,
        1,
        prev,
        CapabilitySet::new([
            DeviceCapability::Sign,
            DeviceCapability::Present,
            DeviceCapability::Encrypt,
        ]),
        None,
    );
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::CapabilityMismatch
    );
}

#[test]
fn a_missing_capability_is_rejected() {
    let (subject, prev) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(
        subject,
        1,
        prev,
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    );
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::CapabilityMismatch
    );
}

/// `Recover` is an app-layer label, not establishment authority — but it is still not part of
/// the Alpha genesis profile, and the verifier must say so.
#[test]
fn a_recover_capability_is_rejected_by_the_alpha_profile() {
    let (subject, prev) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(
        subject,
        1,
        prev,
        CapabilitySet::new([
            DeviceCapability::Sign,
            DeviceCapability::Present,
            DeviceCapability::Recover,
        ]),
        None,
    );
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::CapabilityMismatch
    );
}

#[test]
fn a_validity_span_is_rejected_by_the_alpha_profile() {
    let (subject, prev) = vector_subject_and_inception();
    let bundle = bundle_with_authorize(
        subject,
        1,
        prev,
        alpha_initial_device_capabilities(),
        Some(ValiditySpan::new(1, 10).unwrap()),
    );
    assert_eq!(
        verify_vector(&bundle),
        GenesisVerifyError::UnexpectedValiditySpan
    );
}

/// The load-bearing one: an authorize event signed by a key the Subject never established is
/// perfectly *admissible* — some key signed those bytes — and must still be refused, because
/// only N1's derived fold can say whether that key held authority.
#[test]
fn an_authorize_signed_by_an_unauthorized_key_is_rejected() {
    let d = vector_descriptor();
    let root = vector_root(&d);
    let base = incept_subject_context_v1(&d, &root, device_principal()).unwrap();
    let (subject, prev) = vector_subject_and_inception();

    let impostor = SigningKey::from_bytes(&[0x66; 32]);
    let event = authorize_event(
        &impostor,
        subject,
        1,
        prev,
        device_principal(),
        alpha_initial_device_capabilities(),
        None,
    );
    // Admission accepts it: the signature is valid under the inline signer key.
    assert!(icn_identity::authority_log::admissible(&event.body, &event.signature).is_ok());

    let bundle = SubjectContextGenesisV1 {
        authorize_body_bytes: event.body.canonical_bytes(),
        authorize_witness: event.signature,
        ..base
    };
    assert_eq!(verify_vector(&bundle), GenesisVerifyError::AuthorityNotLive);
}

#[test]
fn an_inception_body_in_the_authorize_slot_is_rejected() {
    let d = vector_descriptor();
    let root = vector_root(&d);
    let base = incept_subject_context_v1(&d, &root, device_principal()).unwrap();
    let bundle = SubjectContextGenesisV1 {
        authorize_body_bytes: base.inception_body_bytes.clone(),
        authorize_witness: base.inception_witness,
        ..base
    };
    assert_eq!(verify_vector(&bundle), GenesisVerifyError::NotAnAuthorize);
}

#[test]
fn an_authorize_body_in_the_inception_slot_is_rejected() {
    let base = vector_bundle();
    let bundle = SubjectContextGenesisV1 {
        inception_body_bytes: base.authorize_body_bytes.clone(),
        inception_witness: base.authorize_witness,
        ..base
    };
    assert_eq!(verify_vector(&bundle), GenesisVerifyError::NotAnInception);
}

// =============================================================================================
// 6. Sanity: the reference helper agrees with the verifier
// =============================================================================================

#[test]
fn the_device_binding_reference_helper_agrees_with_the_verifier() {
    let bundle = vector_bundle();
    let v = verify_subject_context_genesis_v1(
        &bundle,
        SubjectContextKind::GovernanceDomainV1,
        VECTOR_CONTEXT_ID,
    )
    .unwrap();
    assert_eq!(
        initial_device_binding_ref(v.subject_context_ref, v.initial_authorize_event_id),
        v.initial_device_binding_ref
    );
    let _ = sign_body; // re-exported for downstream construction; referenced to keep the import honest
}
