//! N4-D — binding a transport connection to a device Principal, by composition.
//!
//! A Home connection must be able to say "this connection is acting as device `K` for Subject
//! `S`" without trusting an IP address, a hostname, an RDP login, TLS certificate ownership or
//! possession of an enrollment request. The binding is **not** a new kernel primitive: it is a
//! `DeviceActV1` with capability `Present` whose payload is a domain-separated channel binding
//! `(channel digest, nonce)`. A relying party that terminates the connection verifies the act
//! with N4-A (authorship, then authorization over retained facts) and then checks one equality:
//! the bound channel digest is the one it sees. TLS stays TLS; ICN authority stays ICN's; the
//! two meet only at that equality.
//!
//! Fixture identities only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{revoke_at, store_of, stranger, subject, Subject};
use icn_identity::authority_log::{
    authorize_event, CapabilitySet, EventId, PrincipalKey, SignedAuthorityEvent,
};
use icn_identity::authority_log::{
    CodecError, DeviceCapability, COMMITMENT_DOMAIN, DOMAIN, KDF_DOMAIN, SIGNATURE_DOMAIN,
};
use icn_identity::device_admission::AdmissionRefusal;
use icn_identity::device_authority::{
    sign_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal, DEVICE_ACT_DOMAIN,
};
use icn_identity::device_authority::{DeviceActError, MAX_DEVICE_ACT_PAYLOAD};
use icn_identity::device_channel_binding::{
    admit_channel_binding, bind_channel, verify_channel_binding_at, ChannelBindingError,
    ChannelBindingV1, CHANNEL_BINDING_DOMAIN, CHANNEL_BINDING_VERSION,
};
use icn_identity::device_enrollment::ENROLLMENT_REQUEST_DOMAIN;
use icn_identity::evidence_bundle::BUNDLE_DOMAIN;
use icn_identity::subject_context::GEN_CONTEXT_DOMAIN;

const CHANNEL: [u8; 32] = [0xc1; 32]; // e.g. SHA-256 of the server certificate the client saw
const OTHER_CHANNEL: [u8; 32] = [0xc2; 32];
const NONCE: [u8; 32] = [0x11; 32];

/// A grant that includes `Present` (the support fixture's default grant does not).
fn grant_present(
    s: &Subject,
    position: u64,
    prev: EventId,
    device: PrincipalKey,
) -> SignedAuthorityEvent {
    authorize_event(
        &s.root.authority_signing_key(0),
        s.subject,
        position,
        prev,
        device,
        CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]),
        None,
    )
}

#[test]
fn an_authorized_device_binds_the_channel_it_actually_has() {
    let s = subject(0x01, 4);
    let (pi_key, pi) = stranger(0x33);
    let store = store_of(&[s.inception.clone(), grant_present(&s, 1, s.genesis(), pi)]);
    let bound = bind_channel(s.subject, pi, &pi_key, 1, CHANNEL, NONCE).unwrap();
    let verified = verify_channel_binding_at(&bound, s.subject, &store, 1, &CHANNEL).unwrap();
    assert_eq!(verified.act(), &bound.act);
    let evidence = verified.evidence();
    assert_eq!(evidence.device, pi);
    assert_eq!(evidence.capability, DeviceCapability::Present);
}

#[test]
fn a_binding_to_another_channel_is_refused_even_for_an_authorized_device() {
    // A relaying party in the middle sees a different certificate: the device's signature is
    // genuine and its grant is in force, and the connection is still refused.
    let s = subject(0x01, 4);
    let (pi_key, pi) = stranger(0x33);
    let store = store_of(&[s.inception.clone(), grant_present(&s, 1, s.genesis(), pi)]);
    let bound = bind_channel(s.subject, pi, &pi_key, 1, CHANNEL, NONCE).unwrap();
    assert_eq!(
        verify_channel_binding_at(&bound, s.subject, &store, 1, &OTHER_CHANNEL).unwrap_err(),
        ChannelBindingError::ChannelMismatch
    );
}

#[test]
fn an_unauthorized_or_revoked_device_cannot_bind_any_channel() {
    let s = subject(0x01, 4);
    let (pi_key, pi) = stranger(0x33);
    // Never authorized.
    let store = store_of(std::slice::from_ref(&s.inception));
    let bound = bind_channel(s.subject, pi, &pi_key, 1, CHANNEL, NONCE).unwrap();
    assert!(matches!(
        verify_channel_binding_at(&bound, s.subject, &store, 1, &CHANNEL),
        Err(ChannelBindingError::Act(DeviceActVerifyError::Refused(_)))
    ));
    // Authorized, then revoked.
    let auth = grant_present(&s, 1, s.genesis(), pi);
    let revoke = revoke_at(&s, 0, 2, auth.body.event_id(), pi);
    let store = store_of(&[s.inception.clone(), auth, revoke]);
    let bound = bind_channel(s.subject, pi, &pi_key, 2, CHANNEL, NONCE).unwrap();
    assert_eq!(
        verify_channel_binding_at(&bound, s.subject, &store, 2, &CHANNEL).unwrap_err(),
        ChannelBindingError::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
}

#[test]
fn a_binding_needs_the_present_capability_not_merely_a_grant() {
    let s = subject(0x01, 4);
    let (pi_key, pi) = stranger(0x33);
    // The support fixture grants {Sign, Present}; an act that claims Present but whose grant
    // lacked it would be CapabilityNotGranted. Here we check the other direction: a plain
    // Sign act carrying channel-binding bytes is not a binding.
    let store = store_of(&[s.inception.clone(), grant_present(&s, 1, s.genesis(), pi)]);
    let payload = ChannelBindingV1::new(CHANNEL, NONCE).canonical_bytes();
    let act = DeviceActV1::new(s.subject, pi, DeviceCapability::Sign, 1, payload).unwrap();
    let signed = sign_device_act(&act, &pi_key).unwrap();
    assert_eq!(
        verify_channel_binding_at(&signed, s.subject, &store, 1, &CHANNEL).unwrap_err(),
        ChannelBindingError::NotAPresentation
    );
}

#[test]
fn a_binding_whose_payload_is_not_a_channel_binding_is_refused() {
    let s = subject(0x01, 4);
    let (pi_key, pi) = stranger(0x33);
    let store = store_of(&[s.inception.clone(), grant_present(&s, 1, s.genesis(), pi)]);
    let act = DeviceActV1::new(
        s.subject,
        pi,
        DeviceCapability::Present,
        1,
        b"hello".to_vec(),
    )
    .unwrap();
    let signed = sign_device_act(&act, &pi_key).unwrap();
    assert!(matches!(
        verify_channel_binding_at(&signed, s.subject, &store, 1, &CHANNEL),
        Err(ChannelBindingError::Payload(_))
    ));
}

#[test]
fn channel_binding_bytes_are_strict_and_round_trip() {
    let binding = ChannelBindingV1::new(CHANNEL, NONCE);
    let bytes = binding.canonical_bytes();
    assert_eq!(ChannelBindingV1::decode(&bytes).unwrap(), binding);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        ChannelBindingV1::decode(&trailing),
        Err(CodecError::TrailingBytes(1))
    ));
    let mut wrong_version = bytes;
    wrong_version[4 + CHANNEL_BINDING_DOMAIN.len() + 1] ^= 1;
    assert!(matches!(
        ChannelBindingV1::decode(&wrong_version),
        Err(CodecError::UnsupportedVersion(_))
    ));
    assert_eq!(CHANNEL_BINDING_VERSION, 1);
    // Framing, written from the contract: LP(domain) || u16(1) || b32(channel) || b32(nonce).
    let mut expected = (CHANNEL_BINDING_DOMAIN.len() as u32).to_be_bytes().to_vec();
    expected.extend_from_slice(CHANNEL_BINDING_DOMAIN);
    expected.extend_from_slice(&1u16.to_be_bytes());
    expected.extend_from_slice(&CHANNEL);
    expected.extend_from_slice(&NONCE);
    assert_eq!(binding.canonical_bytes(), expected);
}

#[test]
fn channel_binding_domain_is_distinct_from_every_other_domain() {
    for other in [
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
        DEVICE_ACT_DOMAIN,
        BUNDLE_DOMAIN,
        ENROLLMENT_REQUEST_DOMAIN,
        GEN_CONTEXT_DOMAIN,
    ] {
        assert_ne!(CHANNEL_BINDING_DOMAIN, other);
    }
}

// ---------------------------------------------------------------------------------------------
// Admitting a connection now (N4-E) versus the historical question (N4-A at a chosen E).
// ---------------------------------------------------------------------------------------------

const P_SEED: u8 = 0x33;
const Q_SEED: u8 = 0x44;

/// S: P granted at 1, Q at 2, P revoked at 3. The relying party's admission position is 3.
fn admission_facts(s: &Subject, p: PrincipalKey, q: PrincipalKey) -> Vec<SignedAuthorityEvent> {
    let grant_p = grant_present(s, 1, s.genesis(), p);
    let grant_q = grant_present(s, 2, grant_p.body.event_id(), q);
    let revoke_p = revoke_at(s, 0, 3, grant_q.body.event_id(), p);
    vec![s.inception.clone(), grant_p, grant_q, revoke_p]
}

fn admission_refused(err: ChannelBindingError) -> AdmissionRefusal {
    match err {
        ChannelBindingError::Admission(refusal) => refusal,
        other => panic!("expected a current-admission refusal, got {other:?}"),
    }
}

/// A connection is admitted only by an act bound to the relying party's own current tip. The
/// same device's binding at an earlier position is a valid *historical* answer and is still not
/// admitted now; one bound past the tip means the relying party is behind.
#[test]
fn a_connection_is_admitted_only_at_the_relying_partys_current_tip() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let store = store_of(&admission_facts(&s, p, q));

    let at_tip = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    let admitted = admit_channel_binding(&at_tip, s.subject, &store, &CHANNEL).unwrap();
    assert_eq!(admitted.act(), &at_tip.act);
    assert_eq!(admitted.act_id(), at_tip.act.act_id());
    assert_eq!(admitted.evidence().evaluation_position, 3);
    assert_eq!(admitted.evidence().device, q);

    let earlier = bind_channel(s.subject, q, &q_key, 2, CHANNEL, NONCE).unwrap();
    assert!(verify_channel_binding_at(&earlier, s.subject, &store, 2, &CHANNEL).is_ok());
    assert_eq!(
        admission_refused(
            admit_channel_binding(&earlier, s.subject, &store, &CHANNEL).unwrap_err()
        ),
        AdmissionRefusal::StaleAct {
            act: 2,
            admission: 3
        }
    );

    let ahead = bind_channel(s.subject, q, &q_key, 4, CHANNEL, NONCE).unwrap();
    assert_eq!(
        admission_refused(admit_channel_binding(&ahead, s.subject, &store, &CHANNEL).unwrap_err()),
        AdmissionRefusal::RelyingPartyBehind {
            act: 4,
            admission: 3
        }
    );
}

/// P was authorized when it bound the channel at 2, so the historical question says yes. P is
/// revoked in the facts the relying party holds, so no connection is admitted for P now.
#[test]
fn a_revoked_devices_valid_historical_binding_never_admits_a_connection() {
    let s = subject(0x01, 4);
    let (p_key, p) = stranger(P_SEED);
    let (_, q) = stranger(Q_SEED);
    let store = store_of(&admission_facts(&s, p, q));

    let then = bind_channel(s.subject, p, &p_key, 2, CHANNEL, NONCE).unwrap();
    assert!(verify_channel_binding_at(&then, s.subject, &store, 2, &CHANNEL).is_ok());
    assert_eq!(
        admission_refused(admit_channel_binding(&then, s.subject, &store, &CHANNEL).unwrap_err()),
        AdmissionRefusal::StaleAct {
            act: 2,
            admission: 3
        }
    );
    let now = bind_channel(s.subject, p, &p_key, 3, CHANNEL, NONCE).unwrap();
    assert_eq!(
        admission_refused(admit_channel_binding(&now, s.subject, &store, &CHANNEL).unwrap_err()),
        AdmissionRefusal::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
}

/// An admission is relative to the facts held when it was made: once the relying party retains
/// a later revoke of Q, Q's binding at the old tip is stale and a binding at the new tip refused.
#[test]
fn a_later_fact_withdraws_an_earlier_admission() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let mut facts = admission_facts(&s, p, q);
    let at_tip = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    assert!(admit_channel_binding(&at_tip, s.subject, &store_of(&facts), &CHANNEL).is_ok());

    let revoke_q = revoke_at(&s, 0, 4, facts[3].body.event_id(), q);
    facts.push(revoke_q);
    let later = store_of(&facts);
    assert_eq!(
        admission_refused(admit_channel_binding(&at_tip, s.subject, &later, &CHANNEL).unwrap_err()),
        AdmissionRefusal::StaleAct {
            act: 3,
            admission: 4
        }
    );
    let at_new_tip = bind_channel(s.subject, q, &q_key, 4, CHANNEL, NONCE).unwrap();
    assert_eq!(
        admission_refused(
            admit_channel_binding(&at_new_tip, s.subject, &later, &CHANNEL).unwrap_err()
        ),
        AdmissionRefusal::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
}

/// Missing facts move the tip back, never forward: a relying party that lacks the revoke of P
/// admits what it can justify from what it holds, and refuses a binding past its own tip.
#[test]
fn missing_facts_hold_the_tip_back_and_never_admit_past_it() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let facts = admission_facts(&s, p, q);
    let behind = store_of(&facts[..3]);
    let at_three = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    assert_eq!(
        admission_refused(
            admit_channel_binding(&at_three, s.subject, &behind, &CHANNEL).unwrap_err()
        ),
        AdmissionRefusal::RelyingPartyBehind {
            act: 3,
            admission: 2
        }
    );
}

/// A fork in the retained prefix halts current authority: no connection is admitted.
#[test]
fn a_fork_in_the_retained_facts_admits_no_connection() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let mut facts = admission_facts(&s, p, q);
    let competing = grant_present(&s, 2, facts[1].body.event_id(), stranger(0x55).1);
    facts.push(competing);
    let bound = bind_channel(s.subject, q, &q_key, 1, CHANNEL, NONCE).unwrap();
    assert!(matches!(
        admission_refused(
            admit_channel_binding(&bound, s.subject, &store_of(&facts), &CHANNEL).unwrap_err()
        ),
        AdmissionRefusal::AuthorityHalted { .. }
    ));
}

/// A genuine, current binding on the wrong channel is still refused at admission.
#[test]
fn admission_still_refuses_a_binding_to_another_channel() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let store = store_of(&admission_facts(&s, p, q));
    let bound = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    assert_eq!(
        admit_channel_binding(&bound, s.subject, &store, &OTHER_CHANNEL).unwrap_err(),
        ChannelBindingError::ChannelMismatch
    );
}

/// Context confusion: Q holds a current grant in Subject T too, and the relying party admits for
/// T. Q's binding names S, so it is refused for T rather than read across contexts.
#[test]
fn a_binding_for_one_subject_never_admits_a_connection_for_another() {
    let s = subject(0x01, 4);
    let t = subject(0x02, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let mut facts = admission_facts(&s, p, q);
    facts.extend(admission_facts(&t, p, q));
    let store = store_of(&facts);
    let for_s = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    assert!(admit_channel_binding(&for_s, s.subject, &store, &CHANNEL).is_ok());
    assert_eq!(
        admission_refused(admit_channel_binding(&for_s, t.subject, &store, &CHANNEL).unwrap_err()),
        AdmissionRefusal::Act(DeviceActVerifyError::ActSubjectMismatch)
    );
}

/// Replay is the transport profile's to refuse (its nonce policy); what this layer guarantees is a
/// stable key for it. The same binding has one `act_id`; a fresh nonce gives a different one.
#[test]
fn a_replayed_binding_carries_the_same_act_id_and_a_fresh_nonce_a_new_one() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let store = store_of(&admission_facts(&s, p, q));
    let first = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    let replayed = first.clone();
    let fresh = bind_channel(s.subject, q, &q_key, 3, CHANNEL, [0x12; 32]).unwrap();
    let a = admit_channel_binding(&first, s.subject, &store, &CHANNEL).unwrap();
    let b = admit_channel_binding(&replayed, s.subject, &store, &CHANNEL).unwrap();
    let c = admit_channel_binding(&fresh, s.subject, &store, &CHANNEL).unwrap();
    assert_eq!(a.act_id(), b.act_id());
    assert_ne!(a.act_id(), c.act_id());
}

/// An act whose payload was enlarged after construction is refused as malformed, by both
/// operations, before its payload is decoded or any authority is consulted.
#[test]
fn a_binding_act_moved_outside_its_bounds_is_refused_as_malformed() {
    let s = subject(0x01, 4);
    let (_, p) = stranger(P_SEED);
    let (q_key, q) = stranger(Q_SEED);
    let store = store_of(&admission_facts(&s, p, q));
    let mut bound = bind_channel(s.subject, q, &q_key, 3, CHANNEL, NONCE).unwrap();
    bound.act.payload = vec![0; MAX_DEVICE_ACT_PAYLOAD + 1];
    let malformed = ChannelBindingError::Act(DeviceActVerifyError::MalformedAct(
        DeviceActError::PayloadTooLarge(MAX_DEVICE_ACT_PAYLOAD + 1),
    ));
    assert_eq!(
        admit_channel_binding(&bound, s.subject, &store, &CHANNEL).unwrap_err(),
        malformed
    );
    assert_eq!(
        verify_channel_binding_at(&bound, s.subject, &store, 3, &CHANNEL).unwrap_err(),
        malformed
    );
}
