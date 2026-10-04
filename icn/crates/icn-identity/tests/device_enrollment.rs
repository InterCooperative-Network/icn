//! N4-C — the enrollment ceremony, end to end, in a fixture.
//!
//! > A new device generates a key and presents a self-signed request naming the Subject it
//! > asks to join and the capabilities it wants. The person's authority edge checks possession,
//! > attenuates, and records an N1 `Authorize` fact. A relying party, handed the facts and an
//! > act in an N4-B bundle, accepts the device within its grant and refuses it outside it. A
//! > later `Revoke` refuses it entirely. The Subject's own establishment authority is untouched
//! > throughout, and a request that was never approved buys nothing.
//!
//! Fixture identities only. Every key is derived from a seed byte; nothing here is a real person.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{revoke_at, store_of, stranger, subject};
use icn_identity::authority_log::{
    derive, AuthorityStore, AuthorityView, CapabilitySet, CodecError, DeviceCapability,
    COMMITMENT_DOMAIN, DOMAIN, KDF_DOMAIN, SIGNATURE_DOMAIN,
};
use icn_identity::device_authority::{
    sign_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal, DEVICE_ACT_DOMAIN,
};
use icn_identity::device_enrollment::{
    approve_enrollment, sign_enrollment_request, verify_enrollment_request,
    EnrollmentApprovalError, EnrollmentRequestError, EnrollmentRequestSignature,
    EnrollmentRequestV1, EnrollmentVerifyError, SignedEnrollmentRequest, ENROLLMENT_REQUEST_DOMAIN,
    ENROLLMENT_REQUEST_VERSION, MAX_ENROLLMENT_LABEL,
};
use icn_identity::evidence_bundle::{
    verify_evidence_bundle, EvidenceBundle, EvidenceVerifyError, BUNDLE_DOMAIN,
};
use icn_identity::subject_context::GEN_CONTEXT_DOMAIN;

const PI_SEED: u8 = 0x33;
const NONCE: [u8; 32] = [0x77; 32];

fn sign_present() -> CapabilitySet {
    CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present])
}

fn live_state(store: &AuthorityStore, s: &authority_log_support::Subject) -> (Vec<[u8; 32]>, u64) {
    match derive(s.subject, store) {
        AuthorityView::Live { state, .. } => (
            state.authority.iter().map(|p| p.as_bytes()).collect(),
            state.generation,
        ),
        other => panic!("subject must be live, got {other:?}"),
    }
}

#[test]
fn the_ceremony_end_to_end_request_approve_act_revoke() {
    // The person: a Subject whose establishment key lives on the authority edge (the phone).
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);
    let mut store = AuthorityStore::new();
    store.ingest(&s.inception).unwrap();
    let before = live_state(&store, &s);

    // The new device: generates its own per-context key; presents a self-signed request.
    let (pi_key, pi) = stranger(PI_SEED);
    let request =
        EnrollmentRequestV1::new(s.subject, pi, sign_present(), "Katie desk Pi", NONCE).unwrap();
    let signed = sign_enrollment_request(&request, &pi_key).unwrap();

    // The edge: checks possession, then attenuates — only Present is granted.
    verify_enrollment_request(&signed).expect("the device holds the key it names");
    let granted = CapabilitySet::new([DeviceCapability::Present]);
    let fact = approve_enrollment(&signed, &edge, s.subject, 1, s.genesis(), granted, None)
        .expect("a bounded grant is approved");
    assert_eq!(fact.body.position(), 1);
    store.ingest(&fact).unwrap();

    // A relying party, handed the facts and an act: inside the grant, accepted …
    let present = DeviceActV1::new(
        s.subject,
        pi,
        DeviceCapability::Present,
        1,
        b"show".to_vec(),
    )
    .unwrap();
    let bundle = EvidenceBundle::assemble(
        &store_of(&[s.inception.clone(), fact.clone()]),
        &sign_device_act(&present, &pi_key).unwrap(),
    )
    .unwrap();
    let evidence = verify_evidence_bundle(&bundle, s.subject, 1).expect("Present was granted");
    assert_eq!(evidence.grant.granted_at, 1);
    assert_eq!(
        evidence.grant.capabilities,
        CapabilitySet::new([DeviceCapability::Present])
    );

    // … outside the grant, refused: the device asked for Sign, the edge did not give it.
    let sign = DeviceActV1::new(s.subject, pi, DeviceCapability::Sign, 1, b"act".to_vec()).unwrap();
    let bundle = EvidenceBundle::assemble(
        &store_of(&[s.inception.clone(), fact.clone()]),
        &sign_device_act(&sign, &pi_key).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        verify_evidence_bundle(&bundle, s.subject, 1),
        Err(EvidenceVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::CapabilityNotGranted(DeviceCapability::Sign)
        )))
    ));

    // The device is lost: the edge revokes it. Everything it signs from here on is refused.
    let revoke = revoke_at(&s, 0, 2, fact.body.event_id(), pi);
    store.ingest(&revoke).unwrap();
    let later = DeviceActV1::new(
        s.subject,
        pi,
        DeviceCapability::Present,
        2,
        b"show".to_vec(),
    )
    .unwrap();
    let bundle = EvidenceBundle::assemble(
        &store_of(&[s.inception.clone(), fact.clone(), revoke]),
        &sign_device_act(&later, &pi_key).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        verify_evidence_bundle(&bundle, s.subject, 2),
        Err(EvidenceVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        )))
    ));

    // The human is untouched: same identifier, same establishment authority, same generation.
    assert_eq!(live_state(&store, &s), before);
    assert_ne!(pi.as_bytes(), *s.subject.as_bytes());
}

#[test]
fn a_verified_request_that_was_never_approved_buys_nothing() {
    let s = subject(0x01, 4);
    let (pi_key, pi) = stranger(PI_SEED);
    let request =
        EnrollmentRequestV1::new(s.subject, pi, sign_present(), "Katie desk Pi", NONCE).unwrap();
    let signed = sign_enrollment_request(&request, &pi_key).unwrap();
    verify_enrollment_request(&signed).unwrap();

    // Possession proven; no fact recorded. The relying party sees a stranger.
    let act = DeviceActV1::new(
        s.subject,
        pi,
        DeviceCapability::Present,
        1,
        b"show".to_vec(),
    )
    .unwrap();
    let bundle = EvidenceBundle::assemble(
        &store_of(std::slice::from_ref(&s.inception)),
        &sign_device_act(&act, &pi_key).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        verify_evidence_bundle(&bundle, s.subject, 1),
        Err(EvidenceVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::PrefixIncomplete { .. }
        ))) | Err(EvidenceVerifyError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        )))
    ));
}

#[test]
fn approval_cannot_widen_what_was_requested() {
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);
    let (pi_key, pi) = stranger(PI_SEED);
    let request = EnrollmentRequestV1::new(
        s.subject,
        pi,
        CapabilitySet::new([DeviceCapability::Present]),
        "Katie desk Pi",
        NONCE,
    )
    .unwrap();
    let signed = sign_enrollment_request(&request, &pi_key).unwrap();
    let wider = CapabilitySet::new([DeviceCapability::Present, DeviceCapability::Sign]);
    assert_eq!(
        approve_enrollment(&signed, &edge, s.subject, 1, s.genesis(), wider, None).unwrap_err(),
        EnrollmentApprovalError::GrantExceedsRequest
    );
    let nothing = CapabilitySet::new([]);
    assert_eq!(
        approve_enrollment(&signed, &edge, s.subject, 1, s.genesis(), nothing, None).unwrap_err(),
        EnrollmentApprovalError::EmptyGrant
    );
}

#[test]
fn recover_is_never_requested_and_never_granted() {
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);
    let (pi_key, pi) = stranger(PI_SEED);
    assert_eq!(
        EnrollmentRequestV1::new(
            s.subject,
            pi,
            CapabilitySet::new([DeviceCapability::Recover]),
            "x",
            NONCE
        )
        .unwrap_err(),
        EnrollmentRequestError::RecoverRequested
    );
    let request =
        EnrollmentRequestV1::new(s.subject, pi, sign_present(), "Katie desk Pi", NONCE).unwrap();
    let signed = sign_enrollment_request(&request, &pi_key).unwrap();
    let recover = CapabilitySet::new([DeviceCapability::Recover]);
    assert_eq!(
        approve_enrollment(&signed, &edge, s.subject, 1, s.genesis(), recover, None).unwrap_err(),
        EnrollmentApprovalError::RecoverNeverGranted
    );
}

#[test]
fn a_request_for_another_subject_is_refused_at_approval() {
    let katie = subject(0x01, 4);
    let someone_else = subject(0x02, 4);
    let edge = katie.root.authority_signing_key(0);
    let (pi_key, pi) = stranger(PI_SEED);
    let request =
        EnrollmentRequestV1::new(someone_else.subject, pi, sign_present(), "Pi", NONCE).unwrap();
    let signed = sign_enrollment_request(&request, &pi_key).unwrap();
    assert_eq!(
        approve_enrollment(
            &signed,
            &edge,
            katie.subject,
            1,
            katie.genesis(),
            sign_present(),
            None
        )
        .unwrap_err(),
        EnrollmentApprovalError::RequestSubjectMismatch
    );
}

#[test]
fn a_tampered_request_fails_its_own_signature_and_is_not_approved() {
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);
    let (pi_key, pi) = stranger(PI_SEED);
    let request =
        EnrollmentRequestV1::new(s.subject, pi, sign_present(), "Katie desk Pi", NONCE).unwrap();
    let mut signed = sign_enrollment_request(&request, &pi_key).unwrap();
    signed.request.label = "Matt desk Pi".to_owned();
    assert_eq!(
        verify_enrollment_request(&signed).unwrap_err(),
        EnrollmentVerifyError::BadSignature
    );
    assert_eq!(
        approve_enrollment(
            &signed,
            &edge,
            s.subject,
            1,
            s.genesis(),
            sign_present(),
            None
        )
        .unwrap_err(),
        EnrollmentApprovalError::Possession(EnrollmentVerifyError::BadSignature)
    );
    // A signature by a different key over the same request is likewise refused.
    let (other_key, _) = stranger(0x44);
    let forged = SignedEnrollmentRequest {
        request: request.clone(),
        signature: EnrollmentRequestSignature::from_bytes(
            ed25519_dalek::Signer::sign(&other_key, &request.canonical_bytes()).to_bytes(),
        ),
    };
    assert!(verify_enrollment_request(&forged).is_err());
    assert!(sign_enrollment_request(&request, &other_key).is_err());
}

#[test]
fn the_establishment_key_cannot_enroll_itself_as_a_device() {
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);
    let edge_principal =
        icn_identity::authority_log::PrincipalKey::try_from_verifying_key(edge.verifying_key())
            .unwrap();
    let request =
        EnrollmentRequestV1::new(s.subject, edge_principal, sign_present(), "phone", NONCE)
            .unwrap();
    let signed = sign_enrollment_request(&request, &edge).unwrap();
    assert_eq!(
        approve_enrollment(
            &signed,
            &edge,
            s.subject,
            1,
            s.genesis(),
            sign_present(),
            None
        )
        .unwrap_err(),
        EnrollmentApprovalError::DeviceIsEstablishmentAuthority
    );
}

#[test]
fn request_bytes_are_strict_and_round_trip() {
    let s = subject(0x01, 4);
    let (_, pi) = stranger(PI_SEED);
    let request =
        EnrollmentRequestV1::new(s.subject, pi, sign_present(), "Katie desk Pi", NONCE).unwrap();
    let bytes = request.canonical_bytes();
    let decoded = EnrollmentRequestV1::decode(&bytes).unwrap();
    assert_eq!(decoded, request);
    assert_eq!(decoded.canonical_bytes(), bytes);
    assert_eq!(decoded.request_id(), request.request_id());

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        EnrollmentRequestV1::decode(&trailing),
        Err(EnrollmentRequestError::Codec(CodecError::TrailingBytes(1)))
    ));
    let mut wrong_domain = bytes.clone();
    wrong_domain[4] ^= 1;
    assert!(matches!(
        EnrollmentRequestV1::decode(&wrong_domain),
        Err(EnrollmentRequestError::Codec(CodecError::BadDomain))
    ));
    let mut wrong_version = bytes;
    wrong_version[4 + ENROLLMENT_REQUEST_DOMAIN.len() + 1] ^= 1;
    assert!(matches!(
        EnrollmentRequestV1::decode(&wrong_version),
        Err(EnrollmentRequestError::Codec(
            CodecError::UnsupportedVersion(_)
        ))
    ));
    assert_eq!(ENROLLMENT_REQUEST_VERSION, 1);
}

#[test]
fn label_and_ask_bounds_hold() {
    let s = subject(0x01, 4);
    let (_, pi) = stranger(PI_SEED);
    let long = "x".repeat(MAX_ENROLLMENT_LABEL + 1);
    assert_eq!(
        EnrollmentRequestV1::new(s.subject, pi, sign_present(), &long, NONCE).unwrap_err(),
        EnrollmentRequestError::LabelTooLong(MAX_ENROLLMENT_LABEL + 1)
    );
    assert_eq!(
        EnrollmentRequestV1::new(s.subject, pi, CapabilitySet::new([]), "Pi", NONCE).unwrap_err(),
        EnrollmentRequestError::EmptyRequest
    );
    // A label at the bound, with multibyte UTF-8, is fine and round-trips.
    let edge = "é".repeat(MAX_ENROLLMENT_LABEL / 2);
    let request = EnrollmentRequestV1::new(s.subject, pi, sign_present(), &edge, NONCE).unwrap();
    assert_eq!(
        EnrollmentRequestV1::decode(&request.canonical_bytes())
            .unwrap()
            .label,
        edge
    );
}

#[test]
fn request_domain_is_distinct_from_every_other_domain() {
    for other in [
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
        DEVICE_ACT_DOMAIN,
        BUNDLE_DOMAIN,
        GEN_CONTEXT_DOMAIN,
    ] {
        assert_ne!(ENROLLMENT_REQUEST_DOMAIN, other);
    }
}
