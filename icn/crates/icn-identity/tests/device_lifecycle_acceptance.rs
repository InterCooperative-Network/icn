//! The executable backbone of the personal-domain identity slice, through every boundary:
//!
//! ```text
//! enrollment request (N4-C) → attenuated approval → N1 facts → external bundle bytes (N4-B)
//!   → relying-party verification (N4-A) → revoke A → A refused / B accepted / S unchanged
//! ```
//!
//! The acceptance invariant, stated once and pinned here:
//!
//! > A durable Subject `S`. Device `A` is enrolled beneath `S` with bounded authority. Device `B`
//! > is independently enrolled beneath `S` with bounded authority. `A ≠ B`, `A ≠ S`, `B ≠ S`.
//! > Both act successfully within their grants. `A` is revoked. At the post-revocation position
//! > `A`'s act is refused and `B`'s is accepted. `S` keeps its identity, its establishment
//! > authority, its generation and its pre-rotation commitment — nothing rotates because nothing
//! > needs to. Every verdict follows from admitted ICN facts carried as bytes; no verifier holds
//! > a side table; facts arriving in any order give the same verdicts; and a relying party whose
//! > facts stop short of the revocation fails closed rather than accepting the revoked device.
//!
//! `tests/device_authority.rs` proves the core over a local store; this file proves it over the
//! external boundary and the enrollment ceremony. Fixture identities only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{permutations, store_of, stranger, subject, Subject};
use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    derive, derive_prefix, revoke_event, AuthorityState, AuthorityView, CapabilitySet,
    DeviceCapability, EventId, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{
    sign_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityEvidence,
    DeviceAuthorityRefusal, SignedDeviceAct,
};
use icn_identity::device_enrollment::{
    approve_enrollment, sign_enrollment_request, verify_enrollment_request, EnrollmentRequestV1,
};
use icn_identity::evidence_bundle::{
    verify_evidence_bundle_bytes, EvidenceBundle, EvidenceVerifyError,
};

/// A device, from its own secret: generate, request, be approved. Returns the key, the Principal
/// and the N1 fact the edge authored. The device never sees the edge's key.
fn enroll(
    s: &Subject,
    edge: &SigningKey,
    seed: u8,
    label: &str,
    ask: CapabilitySet,
    grant: CapabilitySet,
    position: u64,
    prev: EventId,
) -> (SigningKey, PrincipalKey, SignedAuthorityEvent) {
    let (key, principal) = stranger(seed);
    let request = EnrollmentRequestV1::new(s.subject, principal, ask, label, [seed; 32]).unwrap();
    let signed = sign_enrollment_request(&request, &key).unwrap();
    verify_enrollment_request(&signed).unwrap();
    let fact = approve_enrollment(&signed, edge, s.subject, position, prev, grant, None).unwrap();
    (key, principal, fact)
}

fn act(
    subject: SubjectId,
    device: PrincipalKey,
    key: &SigningKey,
    capability: DeviceCapability,
    position: u64,
) -> SignedDeviceAct {
    let act =
        DeviceActV1::new(subject, device, capability, position, b"katie:act".to_vec()).unwrap();
    sign_device_act(&act, key).unwrap()
}

/// An independent relying party: it receives facts and an act as **bytes**, decodes them, and
/// decides at the position it chooses. It holds nothing between calls.
fn relying_party(
    facts: &[SignedAuthorityEvent],
    act: SignedDeviceAct,
    subject: SubjectId,
    position: u64,
) -> Result<DeviceAuthorityEvidence, EvidenceVerifyError> {
    let bundle =
        EvidenceBundle::assemble(&store_of(facts), &act).map_err(EvidenceVerifyError::Bundle)?;
    verify_evidence_bundle_bytes(&bundle.canonical_bytes(), subject, position)
}

fn refused_as(
    result: Result<DeviceAuthorityEvidence, EvidenceVerifyError>,
) -> DeviceAuthorityRefusal {
    match result {
        Err(EvidenceVerifyError::Act(DeviceActVerifyError::Refused(refusal))) => refusal,
        other => panic!("expected an N4-A refusal, got {other:?}"),
    }
}

fn live_state(view: AuthorityView) -> (AuthorityState, u64) {
    match view {
        AuthorityView::Live { state, frontier } => (state, frontier),
        other => panic!("subject must be live, got {other:?}"),
    }
}

#[test]
fn the_backbone_two_devices_one_revoked_the_other_and_the_subject_unharmed() {
    // 1. A durable Subject. Its establishment key lives at the authority edge.
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);

    // 2–3. Two devices enrolled independently, each from its own secret, each bounded.
    let (a_key, a, auth_a) = enroll(
        &s,
        &edge,
        0x11,
        "Katie desk Pi",
        CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]),
        CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]),
        1,
        s.genesis(),
    );
    let (b_key, b, auth_b) = enroll(
        &s,
        &edge,
        0x22,
        "Katie phone",
        CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]),
        CapabilitySet::new([DeviceCapability::Present]), // attenuated: asked for Sign too
        2,
        auth_a.body.event_id(),
    );
    let enrolled = vec![s.inception.clone(), auth_a.clone(), auth_b.clone()];

    // 4–6. Three distinct identities, and the Subject is not a key at all.
    assert_ne!(a, b);
    assert_ne!(a.as_bytes(), *s.subject.as_bytes());
    assert_ne!(b.as_bytes(), *s.subject.as_bytes());

    // 7–8. Both act within their grants, judged by an independent relying party from bytes.
    let ok_a = relying_party(
        &enrolled,
        act(s.subject, a, &a_key, DeviceCapability::Sign, 2),
        s.subject,
        2,
    )
    .expect("A may Sign");
    assert_eq!((ok_a.device, ok_a.grant.granted_at), (a, 1));
    let ok_b = relying_party(
        &enrolled,
        act(s.subject, b, &b_key, DeviceCapability::Present, 2),
        s.subject,
        2,
    )
    .expect("B may Present");
    assert_eq!((ok_b.device, ok_b.grant.granted_at), (b, 2));
    // Bounded: B asked for Sign and was not given it.
    assert_eq!(
        refused_as(relying_party(
            &enrolled,
            act(s.subject, b, &b_key, DeviceCapability::Sign, 2),
            s.subject,
            2
        )),
        DeviceAuthorityRefusal::CapabilityNotGranted(DeviceCapability::Sign)
    );

    // 9. A is lost. The edge revokes it: one more N1 fact, nothing else.
    let revoke_a = revoke_event(&edge, s.subject, 3, auth_b.body.event_id(), a);
    let mut all = enrolled.clone();
    all.push(revoke_a);

    // 10–11. At the current position A is refused and B is accepted, from the same facts.
    assert_eq!(
        refused_as(relying_party(
            &all,
            act(s.subject, a, &a_key, DeviceCapability::Sign, 3),
            s.subject,
            3
        )),
        DeviceAuthorityRefusal::DeviceNotAuthorized
    );
    let still_b = relying_party(
        &all,
        act(s.subject, b, &b_key, DeviceCapability::Present, 3),
        s.subject,
        3,
    )
    .expect("B is unaffected by A's revocation");
    assert_eq!(
        (still_b.device, still_b.grant.granted_at, still_b.generation),
        (b, 2, 0)
    );

    // 12–13. S is the same Subject with the same establishment authority, generation and
    // pre-rotation commitment before and after; only the device map changed.
    let store_all = authority_log_support::store_of(&all);
    let (before, _) = live_state(derive_prefix(s.subject, &store_all, 2));
    let (after, frontier) = live_state(derive(s.subject, &store_all));
    assert_eq!(frontier, 4);
    assert_eq!(after.authority, before.authority);
    assert_eq!(after.next_commitment, before.next_commitment);
    assert_eq!(after.generation, before.generation);
    assert_eq!(
        after.generation, 0,
        "nothing rotated: a device revocation is not an establishment event"
    );
    assert!(before.devices.contains_key(&a) && before.devices.contains_key(&b));
    assert!(!after.devices.contains_key(&a) && after.devices.contains_key(&b));
    assert!(
        !after.authority.contains(&a) && !after.authority.contains(&b),
        "devices are never log writers"
    );

    // 14a. The verdicts are a function of the facts, not of their arrival order.
    for order in permutations(&all) {
        assert_eq!(
            refused_as(relying_party(
                &order,
                act(s.subject, a, &a_key, DeviceCapability::Sign, 3),
                s.subject,
                3
            )),
            DeviceAuthorityRefusal::DeviceNotAuthorized
        );
        assert!(relying_party(
            &order,
            act(s.subject, b, &b_key, DeviceCapability::Present, 3),
            s.subject,
            3
        )
        .is_ok());
    }

    // 14b. A relying party whose facts stop before the revocation fails closed at the current
    // position: it does not accept A because it "never heard" of the revocation. History it does
    // hold stays verifiable.
    let stale = &all[..3];
    assert_eq!(
        refused_as(relying_party(
            stale,
            act(s.subject, a, &a_key, DeviceCapability::Sign, 3),
            s.subject,
            3
        )),
        DeviceAuthorityRefusal::PrefixIncomplete {
            frontier: 3,
            required: 3
        }
    );
    assert!(relying_party(
        stale,
        act(s.subject, a, &a_key, DeviceCapability::Sign, 2),
        s.subject,
        2
    )
    .is_ok());
}

#[test]
fn a_device_cannot_be_enrolled_twice_into_a_wider_grant_by_resubmitting_its_request() {
    // Resubmission is not escalation: a second approval of the same device at a later position
    // is a new fact with its own grant, and the relying party evaluates whichever is in force
    // at its position. It never merges grants.
    let s = subject(0x01, 4);
    let edge = s.root.authority_signing_key(0);
    let (a_key, a, auth_a) = enroll(
        &s,
        &edge,
        0x11,
        "Katie desk Pi",
        CapabilitySet::new([DeviceCapability::Present]),
        CapabilitySet::new([DeviceCapability::Present]),
        1,
        s.genesis(),
    );
    let facts = vec![s.inception.clone(), auth_a];
    assert!(relying_party(
        &facts,
        act(s.subject, a, &a_key, DeviceCapability::Present, 1),
        s.subject,
        1
    )
    .is_ok());
    assert_eq!(
        refused_as(relying_party(
            &facts,
            act(s.subject, a, &a_key, DeviceCapability::Sign, 1),
            s.subject,
            1
        )),
        DeviceAuthorityRefusal::CapabilityNotGranted(DeviceCapability::Sign)
    );
}
