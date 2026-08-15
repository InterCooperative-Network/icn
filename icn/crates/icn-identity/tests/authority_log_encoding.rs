//! N1 encoding obligations (§19.2, item 20).
//!
//! Round trip plus fixed cross-implementation vectors over a versioned, domain-separated,
//! length-prefixed canonical preimage. The vectors pin the wire format: any change to field
//! order, integer width, tag values, domain separators or set ordering breaks them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{
    authorize_span_at, caps_with_recover, device, principal, revoke_at, subject,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use icn_identity::authority_log::{
    admissible, admissible_bytes, authorize_event, sign_body, AdmissionError, AuthorityBody,
    AuthorityStore, AuthorizeBody, CapabilitySet, CodecError, Commitment, ContextNonce,
    ContinuityRoot, DeviceCapability, EstablishmentBody, EventHeader, EventId, PrincipalKey,
    PrincipalSet, SubjectId, ValiditySpan, WitnessSignature, DOMAIN, PROTOCOL_VERSION,
    SIGNATURE_DOMAIN,
};

/// Every body kind, built deterministically, for round-trip and vector coverage.
fn corpus() -> Vec<(&'static str, AuthorityBody)> {
    let alpha = subject(200, 3);
    let genesis = alpha.genesis();

    let rotate = alpha
        .root
        .establish(1, alpha.subject, 1, genesis)
        .expect("rotate")
        .body;

    let authorize = authorize_event(
        &alpha.root.authority_signing_key(0),
        alpha.subject,
        2,
        rotate.event_id(),
        device(210),
        caps_with_recover(),
        None,
    )
    .body;

    let authorize_span = authorize_span_at(
        &alpha,
        0,
        3,
        authorize.event_id(),
        device(211),
        ValiditySpan::new(3, 9).expect("span"),
    )
    .body;

    let revoke = revoke_at(&alpha, 0, 4, authorize_span.event_id(), device(210)).body;

    // A `recover` body reusing the rotate's shape, so both establishment kinds are pinned.
    let recover = match &rotate {
        AuthorityBody::Rotate(e) => AuthorityBody::Recover(e.clone()),
        other => panic!("expected Rotate, got {other:?}"),
    };

    vec![
        ("inception", alpha.inception.body.clone()),
        ("rotate", rotate),
        ("authorize", authorize),
        ("authorize_span", authorize_span),
        ("revoke", revoke),
        ("recover", recover),
    ]
}

/// Obligation 20a — encode/decode round trip over every body kind.
#[test]
fn canonical_encoding_round_trips() {
    for (name, body) in corpus() {
        let bytes = body.canonical_bytes();
        let decoded = AuthorityBody::decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(decoded, body, "{name}: round trip changed the body");
        assert_eq!(
            decoded.canonical_bytes(),
            bytes,
            "{name}: re-encoding is not byte-stable"
        );
        assert_eq!(decoded.event_id(), body.event_id(), "{name}: digest moved");

        // The encoding opens with the length-prefixed domain separator and the version.
        let mut expected_prefix = (DOMAIN.len() as u32).to_be_bytes().to_vec();
        expected_prefix.extend_from_slice(DOMAIN);
        expected_prefix.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        expected_prefix.push(body.kind_tag());
        assert!(
            bytes.starts_with(&expected_prefix),
            "{name}: missing versioned, domain-separated, length-prefixed header"
        );

        // The signature preimage is the body under its own separate domain.
        let preimage = body.signature_preimage();
        let mut expected_sig_prefix = (SIGNATURE_DOMAIN.len() as u32).to_be_bytes().to_vec();
        expected_sig_prefix.extend_from_slice(SIGNATURE_DOMAIN);
        assert!(preimage.starts_with(&expected_sig_prefix));
        assert!(preimage.ends_with(&bytes));
        assert_ne!(
            preimage, bytes,
            "{name}: signing and digest preimages must not coincide"
        );
    }
}

/// Obligation 20b — the encoding is strict, so one logical body has exactly one encoding.
#[test]
fn decoding_rejects_non_canonical_input() {
    let (_, body) = corpus().into_iter().next().expect("corpus is non-empty");
    let canonical = body.canonical_bytes();

    // Trailing bytes.
    let mut trailing = canonical.clone();
    trailing.push(0x00);
    assert!(matches!(
        AuthorityBody::decode(&trailing),
        Err(CodecError::TrailingBytes(1))
    ));

    // Truncation.
    assert!(matches!(
        AuthorityBody::decode(&canonical[..canonical.len() - 1]),
        Err(CodecError::UnexpectedEnd { .. })
    ));

    // Wrong domain separator.
    let mut bad_domain = canonical.clone();
    bad_domain[4] ^= 0xff;
    assert!(matches!(
        AuthorityBody::decode(&bad_domain),
        Err(CodecError::BadDomain)
    ));

    // Unsupported version.
    let mut bad_version = canonical.clone();
    let version_at = 4 + DOMAIN.len();
    bad_version[version_at] = 0xff;
    assert!(matches!(
        AuthorityBody::decode(&bad_version),
        Err(CodecError::UnsupportedVersion(_))
    ));

    // Unknown kind tag.
    let mut bad_kind = canonical.clone();
    bad_kind[version_at + 2] = 0x7f;
    assert!(matches!(
        AuthorityBody::decode(&bad_kind),
        Err(CodecError::UnknownKind(0x7f))
    ));

    // Empty input.
    assert!(AuthorityBody::decode(&[]).is_err());
}

/// Non-ascending or duplicated set members are rejected, so a set has one encoding.
#[test]
fn decoding_rejects_unordered_and_duplicate_sets() {
    let alpha = subject(201, 2);
    let a_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
    let b_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
    let a = principal(&a_key);
    let b = principal(&b_key);
    let (low, high) = if a.as_bytes() < b.as_bytes() {
        (a, b)
    } else {
        (b, a)
    };

    let body = AuthorityBody::Rotate(EstablishmentBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 1,
            prev_digest: alpha.genesis(),
            signer: low,
        },
        revealed_authority: PrincipalSet::new([low, high], "revealed_authority").expect("set"),
        next_commitment: Commitment::from_bytes([9u8; 32]),
    });
    let canonical = body.canonical_bytes();
    assert_eq!(AuthorityBody::decode(&canonical).expect("decodes"), body);

    // Swap the two members so the set is descending.
    let first_at = canonical
        .windows(33)
        .position(|w| w[0] == 0x01 && w[1..] == low.as_bytes())
        .expect("low key present");
    // The set members are the last two principals before the 32-byte commitment.
    let set_start = canonical.len() - 32 - 66;
    let mut swapped = canonical.clone();
    let (left, right) = canonical[set_start..set_start + 66].split_at(33);
    swapped[set_start..set_start + 33].copy_from_slice(right);
    swapped[set_start + 33..set_start + 66].copy_from_slice(left);
    assert!(matches!(
        AuthorityBody::decode(&swapped),
        Err(CodecError::UnorderedSet { .. })
    ));

    // Duplicate the same member twice.
    let mut duplicated = canonical.clone();
    duplicated[set_start + 33..set_start + 66]
        .copy_from_slice(&canonical[set_start..set_start + 33]);
    assert!(matches!(
        AuthorityBody::decode(&duplicated),
        Err(CodecError::UnorderedSet { .. })
    ));

    let _ = first_at;
}

/// A `Did` is canonicalized to its decoded key, so two DID string encodings of one key produce
/// one preimage (absorbs O10).
#[test]
fn did_canonicalization_collapses_string_encodings() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]).verifying_key();
    let principal = PrincipalKey::try_from_verifying_key(key).expect("generated key is valid");
    let did = principal.to_did();

    let round_tripped = PrincipalKey::from_did(&did).expect("valid did");
    assert_eq!(principal, round_tripped);
    assert_eq!(principal.as_bytes(), round_tripped.as_bytes());

    // A DID carrying a canonical, strong key maps back to exactly the same protocol principal.
    let bogus = icn_identity::Did::from_public_key(&key);
    assert!(PrincipalKey::from_did(&bogus).is_ok());
    // `[0x02; 32]` is not a valid compressed Edwards point.
    assert!(PrincipalKey::try_from_bytes([0x02u8; 32]).is_err());
}

/// N1 accepts exactly one RFC 8032 encoding for each mathematical authority principal.
///
/// `ed25519-dalek` intentionally accepts ZIP-215 encodings. These fixed values decode to the
/// same Edwards point there, but only the reduced `y = 18` encoding is canonical for N1.
#[test]
fn non_canonical_public_key_alias_is_rejected() {
    let non_canonical = [0xff; 32];
    let mut canonical = [0u8; 32];
    canonical[0] = 18;
    canonical[31] = 0x80;

    let alias = VerifyingKey::from_bytes(&non_canonical).expect("dalek accepts ZIP-215 alias");
    let unique = VerifyingKey::from_bytes(&canonical).expect("canonical point");
    assert_eq!(alias.to_montgomery(), unique.to_montgomery());
    assert_ne!(alias.to_bytes(), unique.to_bytes());

    assert!(PrincipalKey::try_from_bytes(canonical).is_ok());
    assert!(matches!(
        PrincipalKey::try_from_bytes(non_canonical),
        Err(CodecError::NonCanonicalPublicKey)
    ));
    assert!(matches!(
        PrincipalKey::try_from_verifying_key(alias),
        Err(CodecError::NonCanonicalPublicKey)
    ));

    // Legacy `Did` remains permissive for compatibility; the N1 conversion is the scoped
    // hardening boundary.
    let encoded = multibase::encode(multibase::Base::Base58Btc, non_canonical);
    let legacy_did = icn_identity::Did::from_str(&format!("did:icn:{encoded}"))
        .expect("legacy DID accepts ZIP-215 key bytes");
    assert!(matches!(
        PrincipalKey::from_did(&legacy_did),
        Err(CodecError::NonCanonicalPublicKey)
    ));
}

/// Weak Ed25519 principals and the signatures they make acceptable to ordinary verification are
/// rejected at both N1 key construction and wire admission.
#[test]
fn weak_public_key_and_signature_are_rejected() {
    // Compressed Edwards identity: a small-order public key.
    let mut weak_bytes = [0u8; 32];
    weak_bytes[0] = 1;
    let weak_key = VerifyingKey::from_bytes(&weak_bytes).expect("identity decodes");
    assert!(weak_key.is_weak());

    // R = identity and s = 0. Ordinary Ed25519 verification accepts this pair for the identity
    // public key, while strict verification correctly rejects it.
    let mut signature_bytes = [0u8; 64];
    signature_bytes[0] = 1;
    let signature = Signature::from_bytes(&signature_bytes);
    let message = b"n1 weak-key regression";
    assert!(weak_key.verify(message, &signature).is_ok());
    assert!(weak_key.verify_strict(message, &signature).is_err());
    assert!(matches!(
        PrincipalKey::try_from_bytes(weak_bytes),
        Err(CodecError::WeakPublicKey)
    ));
    assert!(matches!(
        PrincipalKey::try_from_verifying_key(weak_key),
        Err(CodecError::WeakPublicKey)
    ));

    let weak_encoded = multibase::encode(multibase::Base::Base58Btc, weak_bytes);
    let legacy_weak_did = icn_identity::Did::from_str(&format!("did:icn:{weak_encoded}"))
        .expect("legacy DID accepts weak point bytes");
    assert!(matches!(
        PrincipalKey::from_did(&legacy_weak_did),
        Err(CodecError::WeakPublicKey)
    ));

    // Preserve the wire-boundary regression too: replace both inception principals so the only
    // reason decoding fails is the weak key, not initial-set membership.
    let root = ContinuityRoot::new([0x31; 32], ContextNonce::from_bytes([0x41; 32]), 2);
    let mut canonical_body = root.incept().expect("inception").body.canonical_bytes();
    let signer_at = 4 + DOMAIN.len() + 2 + 1;
    canonical_body[signer_at + 1..signer_at + 33].copy_from_slice(&weak_bytes);
    let initial_authority_at = signer_at + 33 + 32 + 4;
    canonical_body[initial_authority_at + 1..initial_authority_at + 33]
        .copy_from_slice(&weak_bytes);
    let witness = WitnessSignature::from_bytes(signature_bytes);
    assert!(matches!(
        admissible_bytes(&canonical_body, &witness),
        Err(AdmissionError::Codec(CodecError::WeakPublicKey))
    ));
}

/// A non-inception body may not claim position 0.
#[test]
fn position_zero_is_reserved_for_inception() {
    let alpha = subject(202, 2);
    let body = AuthorityBody::Revoke(icn_identity::authority_log::RevokeBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 0,
            prev_digest: alpha.genesis(),
            signer: device(10),
        },
        device: device(11),
    });
    assert!(matches!(
        AuthorityBody::decode(&body.canonical_bytes()),
        Err(CodecError::ReservedPosition)
    ));
}

/// An inception body whose signer is outside its own initial authority set is rejected.
#[test]
fn inception_signer_must_be_in_the_initial_authority_set() {
    let root = ContinuityRoot::new([5u8; 32], ContextNonce::from_bytes([6u8; 32]), 2);
    let inception = root.incept().expect("incept");
    let canonical = inception.body.canonical_bytes();

    // Replace the signer field (first principal after the header) with an unrelated key.
    let stranger_key = ed25519_dalek::SigningKey::from_bytes(&[200u8; 32]);
    let stranger = principal(&stranger_key);
    let signer_at = 4 + DOMAIN.len() + 2 + 1;
    let mut tampered = canonical.clone();
    tampered[signer_at + 1..signer_at + 33].copy_from_slice(&stranger.as_bytes());
    assert!(matches!(
        AuthorityBody::decode(&tampered),
        Err(CodecError::SignerNotInInitialAuthority)
    ));
}

/// Obligation 20c — fixed cross-implementation vectors.
///
/// `(kind, canonical body hex, event_id hex)`. These are pinned bytes: a second implementation
/// that produces different bytes for the same logical body is not interoperable, and a change to
/// this repo's encoder that alters them is a wire-format break.
const VECTORS: &[(&str, &str, &str)] = &[
    (
        "inception",
        "0000001169636e2e617574686f726974792d6c6f67000101012b5ccad4adea89cce4ec4e3a32fc12cf349e1f18f3b5581efbe3e7b26283a5ea6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d00000001012b5ccad4adea89cce4ec4e3a32fc12cf349e1f18f3b5581efbe3e7b26283a5ea3d684d9169ab06f642a280b1231f6158889a772acbc74965ae31ed00c2ef4db4",
        "173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce",
    ),
    (
        "rotate",
        "0000001169636e2e617574686f726974792d6c6f67000102173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce0000000000000001173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce01eb35f579dd9369e15645da1a243da9f30aa42efe770ad1689009c7cd50873e800000000101eb35f579dd9369e15645da1a243da9f30aa42efe770ad1689009c7cd50873e80db0b887cf2b2025ae7c173020c99ceae634c5d609629927701a9370e2b29f39b",
        "2dfd2db30bc2d28689b7564a66332cca4b8dbbb6cec67b0399d6e126056cf1c7",
    ),
    (
        "authorize",
        "0000001169636e2e617574686f726974792d6c6f67000103173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce00000000000000022dfd2db30bc2d28689b7564a66332cca4b8dbbb6cec67b0399d6e126056cf1c7012b5ccad4adea89cce4ec4e3a32fc12cf349e1f18f3b5581efbe3e7b26283a5ea0141e8fde132ad670e534cd8b275d2cd7eec77733c66f8db48a1cada7fabfc455500000002010400",
        "d646ca12c89eb617ef46bc098e726c32f965b989a3ff6c2ba8ed085cfde7459f",
    ),
    (
        "authorize_span",
        "0000001169636e2e617574686f726974792d6c6f67000103173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce0000000000000003d646ca12c89eb617ef46bc098e726c32f965b989a3ff6c2ba8ed085cfde7459f012b5ccad4adea89cce4ec4e3a32fc12cf349e1f18f3b5581efbe3e7b26283a5ea01b4740ba4e0f7e0576f11e8149dbbb2529c212213db679831fb5d1b221e84552d00000001010100000000000000030000000000000009",
        "cf572c61873192c3fe9fed22b1316e1349f4fbf6abe5900871b6f880ec60062e",
    ),
    (
        "revoke",
        "0000001169636e2e617574686f726974792d6c6f67000104173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce0000000000000004cf572c61873192c3fe9fed22b1316e1349f4fbf6abe5900871b6f880ec60062e012b5ccad4adea89cce4ec4e3a32fc12cf349e1f18f3b5581efbe3e7b26283a5ea0141e8fde132ad670e534cd8b275d2cd7eec77733c66f8db48a1cada7fabfc4555",
        "89c293f7e1c0762a826d5ceb3b3a75b61c1e23863136459c9f5c9e51f034704f",
    ),
    (
        "recover",
        "0000001169636e2e617574686f726974792d6c6f67000105173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce0000000000000001173581ef059dfe89120b0f386cf2417517494f73c98a9e95866dc01eda7151ce01eb35f579dd9369e15645da1a243da9f30aa42efe770ad1689009c7cd50873e800000000101eb35f579dd9369e15645da1a243da9f30aa42efe770ad1689009c7cd50873e80db0b887cf2b2025ae7c173020c99ceae634c5d609629927701a9370e2b29f39b",
        "6434ea2f02b8c3ee4ef3e6aeba6c600659caa79dee31337be89b1e05e8390410",
    ),
];

/// Regenerate the vector table above by running:
/// `cargo test -p icn-identity --test authority_log_encoding -- --ignored print_vectors --nocapture`
#[test]
#[ignore = "generator, not an assertion"]
fn print_vectors() {
    for (name, body) in corpus() {
        println!(
            "    (\n        \"{name}\",\n        \"{}\",\n        \"{}\",\n    ),",
            hex::encode(body.canonical_bytes()),
            hex::encode(body.event_id().as_bytes()),
        );
    }
}

#[test]
fn fixed_vectors_match() {
    let corpus = corpus();
    assert_eq!(
        VECTORS.len(),
        corpus.len(),
        "the vector table must cover every body kind"
    );
    for ((expected_name, expected_body, expected_id), (name, body)) in VECTORS.iter().zip(&corpus) {
        assert_eq!(expected_name, name);
        assert_eq!(
            &hex::encode(body.canonical_bytes()),
            expected_body,
            "{name}: canonical bytes drifted from the pinned vector"
        );
        assert_eq!(
            &hex::encode(body.event_id().as_bytes()),
            expected_id,
            "{name}: event_id drifted from the pinned vector"
        );
        // Vectors decode back to the same body.
        let raw = hex::decode(expected_body).expect("vector hex");
        assert_eq!(&AuthorityBody::decode(&raw).expect("vector decodes"), body);
    }
}

/// Signature vectors: the preimage and a deterministic signature over it.
#[test]
fn signature_preimage_is_pinned() {
    let alpha = subject(200, 3);
    let body = &alpha.inception.body;
    let preimage = body.signature_preimage();

    let mut expected = (SIGNATURE_DOMAIN.len() as u32).to_be_bytes().to_vec();
    expected.extend_from_slice(SIGNATURE_DOMAIN);
    expected.extend_from_slice(&body.canonical_bytes());
    assert_eq!(preimage, expected);

    assert!(admissible(body, &alpha.inception.signature).is_ok());
}

/// Sanity: `SubjectId` and `EventId` are distinct types over the same digest, and the subject of
/// an inception body is its own digest.
#[test]
fn subject_of_inception_is_its_own_digest() {
    let alpha = subject(203, 2);
    let expected = SubjectId::from_bytes(*alpha.inception.body.event_id().as_bytes());
    assert_eq!(alpha.subject, expected);
    let _: EventId = alpha.inception.body.event_id();
}

/// A body built by struct literal, bypassing the validating constructors, is caught at admission.
///
/// Several body fields are public, so `ValiditySpan { from_position: 9, until_position: 2 }` is
/// constructible even though [`ValiditySpan::new`] rejects it. The round-trip step inside
/// `admissible` is what stops such a body entering the durable set.
#[test]
fn admission_rejects_bodies_that_bypass_validating_constructors() {
    let alpha = subject(204, 2);
    let key = alpha.root.authority_signing_key(0);

    let body = AuthorityBody::Authorize(AuthorizeBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 1,
            prev_digest: alpha.genesis(),
            signer: principal(&key),
        },
        device: device(10),
        capabilities: CapabilitySet::new([DeviceCapability::Sign]),
        validity: Some(ValiditySpan {
            from_position: 9,
            until_position: 2,
        }),
    });
    let signed = sign_body(body, &key);

    assert!(matches!(
        admissible(&signed.body, &signed.signature),
        Err(AdmissionError::Codec(CodecError::InvertedSpan { .. }))
    ));

    let mut store = AuthorityStore::new();
    assert!(store.ingest(&signed).is_err());
    assert_eq!(
        store.body_count(),
        0,
        "an invalid body must not become durable"
    );
}
