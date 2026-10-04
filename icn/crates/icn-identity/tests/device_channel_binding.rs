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
use icn_identity::device_authority::{
    sign_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal, DEVICE_ACT_DOMAIN,
};
use icn_identity::device_channel_binding::{
    bind_channel, verify_channel_binding, ChannelBindingError, ChannelBindingV1,
    CHANNEL_BINDING_DOMAIN, CHANNEL_BINDING_VERSION,
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
    let evidence = verify_channel_binding(&bound, s.subject, &store, 1, &CHANNEL).unwrap();
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
        verify_channel_binding(&bound, s.subject, &store, 1, &OTHER_CHANNEL).unwrap_err(),
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
        verify_channel_binding(&bound, s.subject, &store, 1, &CHANNEL),
        Err(ChannelBindingError::Act(DeviceActVerifyError::Refused(_)))
    ));
    // Authorized, then revoked.
    let auth = grant_present(&s, 1, s.genesis(), pi);
    let revoke = revoke_at(&s, 0, 2, auth.body.event_id(), pi);
    let store = store_of(&[s.inception.clone(), auth, revoke]);
    let bound = bind_channel(s.subject, pi, &pi_key, 2, CHANNEL, NONCE).unwrap();
    assert_eq!(
        verify_channel_binding(&bound, s.subject, &store, 2, &CHANNEL).unwrap_err(),
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
        verify_channel_binding(&signed, s.subject, &store, 1, &CHANNEL).unwrap_err(),
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
        verify_channel_binding(&signed, s.subject, &store, 1, &CHANNEL),
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
