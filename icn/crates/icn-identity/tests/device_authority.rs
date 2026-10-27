//! N4-A — relying-party evaluation of delegated device authority (#2694 §7, #2599).
//!
//! The forcing scenario, stated once and pinned here:
//!
//! > One durable human Subject `S`. Device Principal `A` and device Principal `B` are each
//! > independently authorized for a bounded scope. `A` is revoked. An act signed by `A` after the
//! > revocation is refused; an equivalent act signed by `B` is accepted; `S` is the same Subject
//! > throughout; revoking `A` neither revokes nor replaces `S`; the verifier never pretends that
//! > `A == S`; and authority is evaluated at a position the **relying party** supplies, never one
//! > the signer picks.
//!
//! Fixture identities only. Every key is derived from a seed byte; nothing here is a real person.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{
    authorize_at, authorize_span_at, device, permutations, principal, revoke_at, store_of,
    stranger, subject, subject_with_plan, Subject,
};
use ed25519_dalek::Signer as _;
use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    authorize_event, derive, derive_prefix, sign_body, AuthorityBody, AuthorityStore,
    AuthorityView, CapabilitySet, DeviceCapability, EstablishmentKind, PrincipalKey,
    SignedAuthorityEvent, SubjectId, ValiditySpan, COMMITMENT_DOMAIN, DOMAIN, KDF_DOMAIN,
    MAX_POSITION, SIGNATURE_DOMAIN,
};
use icn_identity::device_authority::{
    evaluate_device_authority, sign_device_act, verify_device_act, DeviceActError,
    DeviceActSignature, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal, SignedDeviceAct,
    DEVICE_ACT_DOMAIN, DEVICE_ACT_VERSION, MAX_DEVICE_ACT_PAYLOAD,
};
use icn_identity::subject_context::{
    GEN_CONTEXT_DOMAIN, INITIAL_DEVICE_BINDING_REF_DOMAIN, SUBJECT_CONTEXT_REF_DOMAIN,
};

// ---------------------------------------------------------------------------------------------
// The fixture: S with A and B, then A revoked.
// ---------------------------------------------------------------------------------------------

/// Seeds for the two device keypairs. Distinct from every subject seed used below.
const DEVICE_A_SEED: u8 = 0x11;
const DEVICE_B_SEED: u8 = 0x22;

struct Fixture {
    s: Subject,
    a_key: SigningKey,
    a: PrincipalKey,
    b_key: SigningKey,
    b: PrincipalKey,
    /// Positions: 1 = authorize A, 2 = authorize B, 3 = revoke A.
    store: AuthorityStore,
}

fn fixture() -> Fixture {
    let s = subject(0x01, 4);
    let (a_key, a) = stranger(DEVICE_A_SEED);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let revoke_a = revoke_at(&s, 0, 3, auth_b.body.event_id(), a);
    let store = store_of(&[s.inception.clone(), auth_a, auth_b, revoke_a]);
    Fixture {
        s,
        a_key,
        a,
        b_key,
        b,
        store,
    }
}

fn act(subject: SubjectId, device: PrincipalKey, position: u64, payload: &[u8]) -> DeviceActV1 {
    DeviceActV1::new(
        subject,
        device,
        DeviceCapability::Sign,
        position,
        payload.to_vec(),
    )
    .expect("fixture act is well-formed")
}

fn signed(
    subject: SubjectId,
    device: PrincipalKey,
    key: &SigningKey,
    position: u64,
) -> SignedDeviceAct {
    sign_device_act(
        &act(subject, device, position, b"fixture:open-workspace"),
        key,
    )
    .expect("signer holds the device key the act names")
}

// ---------------------------------------------------------------------------------------------
// The acceptance scenario.
// ---------------------------------------------------------------------------------------------

#[test]
fn revoked_device_is_refused_while_sibling_device_and_subject_survive() {
    let f = fixture();
    let subject_before = f.s.subject;

    // The relying party evaluates at the last position it retained: 3, the revocation.
    let a_after = signed(f.s.subject, f.a, &f.a_key, 3);
    let b_after = signed(f.s.subject, f.b, &f.b_key, 3);

    let refused = verify_device_act(&a_after, f.s.subject, &f.store, 3).unwrap_err();
    assert_eq!(
        refused,
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::DeviceNotAuthorized)
    );

    let verified = verify_device_act(&b_after, f.s.subject, &f.store, 3).unwrap();
    assert_eq!(verified.act(), &b_after.act);
    assert_eq!(verified.act_id(), b_after.act.act_id());
    let evidence = verified.evidence();
    assert_eq!(evidence.subject, f.s.subject);
    assert_eq!(evidence.device, f.b);
    assert_eq!(evidence.capability, DeviceCapability::Sign);
    assert_eq!(evidence.evaluation_position, 3);
    assert_eq!(evidence.grant.granted_at, 2);
    assert_eq!(evidence.generation, 0);

    // The Subject is untouched by the revocation: same identifier, same establishment authority,
    // same generation, same pre-rotation commitment, before and after.
    assert_eq!(f.s.subject, subject_before);
    let before = match derive_prefix(f.s.subject, &f.store, 2) {
        AuthorityView::Live { state, frontier } => {
            assert_eq!(frontier, 3);
            state
        }
        other => panic!("expected live prefix through 2, got {other:?}"),
    };
    let after = match derive(f.s.subject, &f.store) {
        AuthorityView::Live { state, frontier } => {
            assert_eq!(frontier, 4);
            state
        }
        other => panic!("expected live frontier, got {other:?}"),
    };
    assert_eq!(before.authority, after.authority);
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.next_commitment, after.next_commitment);
    assert!(before.devices.contains_key(&f.a));
    assert!(!after.devices.contains_key(&f.a));
    assert!(after.devices.contains_key(&f.b));

    // The verifier never equates a device with the Subject: the device is not in the authority
    // set, and the Subject is not a key.
    assert!(!after.authority.contains(&f.b));
    assert_ne!(evidence.device.as_bytes(), *evidence.subject.as_bytes());
}

#[test]
fn later_revocation_does_not_erase_earlier_authorized_authorship() {
    let f = fixture();
    // A acted at position 2, before it was revoked at 3.
    let a_then = signed(f.s.subject, f.a, &f.a_key, 2);
    let verified = verify_device_act(&a_then, f.s.subject, &f.store, 2).unwrap();
    let evidence = verified.evidence();
    assert_eq!(evidence.device, f.a);
    assert_eq!(evidence.grant.granted_at, 1);

    // The same bytes evaluated at the revocation position are refused — not because the
    // signature changed, but because authority is a function of the evaluated prefix.
    let a_now = signed(f.s.subject, f.a, &f.a_key, 3);
    assert_eq!(
        verify_device_act(&a_now, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::DeviceNotAuthorized)
    );
}

#[test]
fn evaluation_position_is_supplied_by_the_relying_party_not_the_signer() {
    let f = fixture();
    // The act claims 2 (where A was still authorized). The relying party evaluates at 3.
    let a_claims_two = signed(f.s.subject, f.a, &f.a_key, 2);
    assert_eq!(
        verify_device_act(&a_claims_two, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::EvaluationPositionMismatch { act: 2, relying: 3 }
    );
}

#[test]
fn a_position_beyond_the_retained_prefix_fails_closed() {
    let f = fixture();
    let b_future = signed(f.s.subject, f.b, &f.b_key, 4);
    assert_eq!(
        verify_device_act(&b_future, f.s.subject, &f.store, 4).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::PrefixIncomplete {
            frontier: 4,
            required: 4,
        })
    );
}

#[test]
fn an_act_naming_a_different_subject_than_the_relying_party_expects_is_refused() {
    let f = fixture();
    let other = subject(0x02, 4);
    let b_elsewhere = signed(other.subject, f.b, &f.b_key, 3);
    assert_eq!(
        verify_device_act(&b_elsewhere, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::ActSubjectMismatch
    );
}

#[test]
fn a_signature_by_another_device_over_the_same_act_is_refused() {
    let f = fixture();
    // B's key signs an act that names A as the device. Signing refuses at construction...
    let a_act = act(f.s.subject, f.a, 3, b"x");
    assert!(sign_device_act(&a_act, &f.b_key).is_err());
    // ...and a hand-assembled envelope is refused at verification.
    let forged = SignedDeviceAct {
        act: a_act.clone(),
        signature: sign_device_act(&act(f.s.subject, f.b, 3, b"x"), &f.b_key)
            .unwrap()
            .signature,
    };
    assert_eq!(
        verify_device_act(&forged, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::BadSignature
    );
}

#[test]
fn a_mutated_payload_breaks_the_signature() {
    let f = fixture();
    let good = signed(f.s.subject, f.b, &f.b_key, 3);
    let mut tampered = good.clone();
    tampered.act.payload.push(0x00);
    assert_eq!(
        verify_device_act(&tampered, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::BadSignature
    );
}

#[test]
fn a_capability_the_grant_does_not_carry_is_refused() {
    let f = fixture();
    let present = DeviceActV1::new(
        f.s.subject,
        f.b,
        DeviceCapability::Present,
        3,
        b"show".to_vec(),
    )
    .unwrap();
    let signed_present = sign_device_act(&present, &f.b_key).unwrap();
    assert_eq!(
        verify_device_act(&signed_present, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::CapabilityNotGranted(
            DeviceCapability::Present
        ))
    );
}

#[test]
fn a_grant_outside_its_validity_span_is_refused_without_a_clock() {
    let s = subject(0x03, 4);
    let (c_key, c) = stranger(0x33);
    let span = ValiditySpan::new(1, 2).unwrap();
    let auth_c = authorize_span_at(&s, 0, 1, s.genesis(), c, span);
    let auth_d = authorize_at(&s, 0, 2, auth_c.body.event_id(), device(0x44));
    let auth_e = authorize_at(&s, 0, 3, auth_d.body.event_id(), device(0x55));
    let store = store_of(&[s.inception.clone(), auth_c, auth_d, auth_e]);

    assert!(verify_device_act(&signed(s.subject, c, &c_key, 2), s.subject, &store, 2).is_ok());
    assert_eq!(
        verify_device_act(&signed(s.subject, c, &c_key, 3), s.subject, &store, 3).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::GrantNotInForce {
            granted_at: 1,
            validity: Some(span),
        })
    );
}

#[test]
fn a_device_never_authorized_is_refused_even_with_a_valid_signature() {
    let f = fixture();
    let (z_key, z) = stranger(0x99);
    let z_act = signed(f.s.subject, z, &z_key, 3);
    assert_eq!(
        verify_device_act(&z_act, f.s.subject, &f.store, 3).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::DeviceNotAuthorized)
    );
}

#[test]
fn the_establishment_authority_acting_as_a_device_is_refused() {
    // N1 permits authorizing the generation-0 authority key as a device. The Alpha profile
    // refuses to accept acts from it as *device* acts, for the same reason GEN-A refuses it at
    // genesis: a credential that is also the log-writer key has no bounded scope.
    let s = subject(0x04, 4);
    let authority_key = s.root.authority_signing_key(0);
    let authority = principal(&authority_key);
    let auth_self = authorize_at(&s, 0, 1, s.genesis(), authority);
    let store = store_of(&[s.inception.clone(), auth_self]);
    assert!(matches!(
        derive(s.subject, &store),
        AuthorityView::Live { frontier: 2, .. }
    ));
    let self_act = signed(s.subject, authority, &authority_key, 1);
    assert_eq!(
        verify_device_act(&self_act, s.subject, &store, 1).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::DeviceIsEstablishmentAuthority)
    );
}

// ---------------------------------------------------------------------------------------------
// Fail-closed on fork, gap, and unknown subject.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_fork_at_or_before_the_evaluated_position_fails_closed() {
    let s = subject(0x05, 4);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_b = authorize_at(&s, 0, 1, s.genesis(), b);
    // Two equally-authorized, distinct candidates at position 2.
    let fork_1 = authorize_at(&s, 0, 2, auth_b.body.event_id(), device(0x61));
    let fork_2 = authorize_at(&s, 0, 2, auth_b.body.event_id(), device(0x62));
    let store = store_of(&[s.inception.clone(), auth_b, fork_1, fork_2]);

    // Before the fork, B's authority is evaluable.
    assert!(verify_device_act(&signed(s.subject, b, &b_key, 1), s.subject, &store, 1).is_ok());
    // At the fork, nothing is.
    assert_eq!(
        verify_device_act(&signed(s.subject, b, &b_key, 2), s.subject, &store, 2).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::AuthorityHalted { disputed_at: 2 })
    );
}

#[test]
fn a_gap_before_the_evaluated_position_fails_closed() {
    let s = subject(0x06, 4);
    let (b_key, b) = stranger(DEVICE_B_SEED);
    let auth_b = authorize_at(&s, 0, 1, s.genesis(), b);
    // Position 2 is missing; position 3 names a parent nobody retained.
    let orphan = authorize_at(&s, 0, 3, auth_b.body.event_id(), device(0x63));
    let store = store_of(&[s.inception.clone(), auth_b, orphan]);
    assert_eq!(
        verify_device_act(&signed(s.subject, b, &b_key, 3), s.subject, &store, 3).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::PrefixIncomplete {
            frontier: 2,
            required: 3,
        })
    );
}

#[test]
fn an_unknown_subject_fails_closed() {
    let f = fixture();
    let ghost = subject(0x07, 4);
    let (b_key, b) = (f.b_key, f.b);
    let ghost_act = signed(ghost.subject, b, &b_key, 1);
    assert_eq!(
        verify_device_act(&ghost_act, ghost.subject, &f.store, 1).unwrap_err(),
        DeviceActVerifyError::Refused(DeviceAuthorityRefusal::SubjectUnknown)
    );
}

#[test]
fn position_zero_and_positions_above_the_bound_are_refused() {
    let f = fixture();
    assert!(DeviceActV1::new(f.s.subject, f.b, DeviceCapability::Sign, 0, vec![]).is_err());
    assert!(DeviceActV1::new(
        f.s.subject,
        f.b,
        DeviceCapability::Sign,
        MAX_POSITION + 1,
        vec![]
    )
    .is_err());
    assert_eq!(
        evaluate_device_authority(f.s.subject, &f.store, f.b, DeviceCapability::Sign, 0)
            .unwrap_err(),
        DeviceAuthorityRefusal::PositionOutOfRange(0)
    );
}

// ---------------------------------------------------------------------------------------------
// Prefix derivation is the existing N1 fold over the retained prefix — no new selector.
// ---------------------------------------------------------------------------------------------

#[test]
fn derive_prefix_through_the_bound_equals_derive() {
    let f = fixture();
    assert_eq!(
        derive_prefix(f.s.subject, &f.store, MAX_POSITION),
        derive(f.s.subject, &f.store)
    );
    // ...and so does any bound at or past the frontier.
    assert_eq!(
        derive_prefix(f.s.subject, &f.store, 3),
        derive(f.s.subject, &f.store)
    );
    assert_eq!(
        derive_prefix(f.s.subject, &f.store, 7),
        derive(f.s.subject, &f.store)
    );
}

#[test]
fn derive_prefix_reports_state_as_of_the_bound() {
    let f = fixture();
    match derive_prefix(f.s.subject, &f.store, 1) {
        AuthorityView::Live { state, frontier } => {
            assert_eq!(frontier, 2);
            assert!(state.devices.contains_key(&f.a));
            assert!(!state.devices.contains_key(&f.b));
        }
        other => panic!("expected live prefix through 1, got {other:?}"),
    }
    match derive_prefix(f.s.subject, &f.store, 0) {
        AuthorityView::Live { state, frontier } => {
            assert_eq!(frontier, 1);
            assert!(state.devices.is_empty());
            assert_eq!(state.generation, 0);
        }
        other => panic!("expected inception-only prefix, got {other:?}"),
    }
}

#[test]
fn derive_prefix_hides_a_later_fork_and_reveals_an_earlier_one() {
    let s = subject(0x08, 4);
    let auth_1 = authorize_at(&s, 0, 1, s.genesis(), device(0x71));
    let fork_a = authorize_at(&s, 0, 2, auth_1.body.event_id(), device(0x72));
    let fork_b = authorize_at(&s, 0, 2, auth_1.body.event_id(), device(0x73));
    let store = store_of(&[s.inception.clone(), auth_1, fork_a, fork_b]);
    assert!(matches!(
        derive_prefix(s.subject, &store, 1),
        AuthorityView::Live { frontier: 2, .. }
    ));
    assert!(matches!(
        derive_prefix(s.subject, &store, 2),
        AuthorityView::Halted { disputed_at: 2, .. }
    ));
}

#[test]
fn a_superseding_rotation_inside_the_prefix_is_honoured() {
    let s = subject(0x09, 4);
    let auth_1 = authorize_at(&s, 0, 1, s.genesis(), device(0x81));
    let rotate = s
        .root
        .establish(1, s.subject, 2, auth_1.body.event_id())
        .expect("rotation constructs");
    let (k_key, k) = stranger(0x82);
    let auth_after = authorize_event(
        &s.root.authority_signing_key(1),
        s.subject,
        3,
        rotate.body.event_id(),
        k,
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    );
    let store = store_of(&[s.inception.clone(), auth_1, rotate, auth_after]);
    let verified =
        verify_device_act(&signed(s.subject, k, &k_key, 3), s.subject, &store, 3).unwrap();
    let ev = verified.evidence();
    assert_eq!(ev.generation, 1);
    assert_eq!(ev.grant.granted_at, 3);
    // The pre-rotation device survives a rotate (N1: rotate preserves grants).
    let ev_old =
        evaluate_device_authority(s.subject, &store, device(0x81), DeviceCapability::Sign, 3)
            .unwrap();
    assert_eq!(ev_old.grant.granted_at, 1);
}

#[test]
fn verdicts_are_independent_of_ingest_order() {
    let f = fixture();
    let events: Vec<_> = f.store.bodies_for(f.s.subject).into_iter().collect();
    assert_eq!(events.len(), 4);
    let witnesses: Vec<_> = events
        .iter()
        .map(|body| {
            let sigs = f.store.witnesses_for(body.event_id());
            (
                body.clone(),
                sigs.into_iter().next().expect("one witness per body"),
            )
        })
        .collect();
    let reference_a = verify_device_act(
        &signed(f.s.subject, f.a, &f.a_key, 3),
        f.s.subject,
        &f.store,
        3,
    );
    let reference_b = verify_device_act(
        &signed(f.s.subject, f.b, &f.b_key, 3),
        f.s.subject,
        &f.store,
        3,
    );
    for order in permutations(&witnesses) {
        let mut store = AuthorityStore::new();
        for (body, witness) in &order {
            let event = SignedAuthorityEvent::new(body.clone(), witness.signature);
            assert!(store.ingest(&event).is_ok());
        }
        assert_eq!(
            verify_device_act(
                &signed(f.s.subject, f.a, &f.a_key, 3),
                f.s.subject,
                &store,
                3
            ),
            reference_a
        );
        assert_eq!(
            verify_device_act(
                &signed(f.s.subject, f.b, &f.b_key, 3),
                f.s.subject,
                &store,
                3
            ),
            reference_b
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Canonical act bytes.
// ---------------------------------------------------------------------------------------------

#[test]
fn act_domain_is_distinct_from_every_n1_and_gen_domain() {
    let domains: [&[u8]; 8] = [
        DEVICE_ACT_DOMAIN,
        DOMAIN,
        SIGNATURE_DOMAIN,
        COMMITMENT_DOMAIN,
        KDF_DOMAIN,
        GEN_CONTEXT_DOMAIN,
        SUBJECT_CONTEXT_REF_DOMAIN,
        INITIAL_DEVICE_BINDING_REF_DOMAIN,
    ];
    for (i, a) in domains.iter().enumerate() {
        for (j, b) in domains.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "domain separators must be pairwise distinct");
            }
        }
    }
    assert_eq!(DEVICE_ACT_DOMAIN, b"icn.n4.device-act");
    assert_eq!(DEVICE_ACT_VERSION, 1);
}

#[test]
fn canonical_act_bytes_are_strict_and_round_trip() {
    let f = fixture();
    let a = act(f.s.subject, f.b, 3, b"payload");
    let bytes = a.canonical_bytes();
    // LP(domain) || u16 version || b32 subject || (tag || 32 key) || u64 position || u8 cap || LP(payload)
    assert_eq!(bytes.len(), 4 + 17 + 2 + 32 + 33 + 8 + 1 + 4 + 7);
    assert_eq!(&bytes[..4], &17u32.to_be_bytes());
    assert_eq!(&bytes[4..21], b"icn.n4.device-act");
    assert_eq!(DeviceActV1::decode(&bytes).unwrap(), a);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(DeviceActV1::decode(&trailing).is_err());
    // A SubjectId in the device slot is a decode error, not a 32-byte blob (I2).
    let mut subject_in_principal_slot = bytes.clone();
    subject_in_principal_slot[4 + 17 + 2 + 32] = 0x00;
    assert!(DeviceActV1::decode(&subject_in_principal_slot).is_err());
    assert_eq!(a.act_id(), act(f.s.subject, f.b, 3, b"payload").act_id());
    assert_ne!(a.act_id(), act(f.s.subject, f.b, 3, b"payload!").act_id());
}

#[test]
fn a_device_act_is_not_an_admissible_authority_body() {
    // The act domain is not the N1 body domain, so act bytes can never be mistaken for a body.
    let f = fixture();
    let a = act(f.s.subject, f.b, 3, b"payload");
    assert!(AuthorityBody::decode(&a.canonical_bytes()).is_err());
    // And an N1 body re-signed under the act domain is not a device act either.
    let inception = &f.s.inception;
    assert!(DeviceActV1::decode(&inception.body.canonical_bytes()).is_err());
    let _ = sign_body; // keep the import honest: constructing N1 bodies is N1's job, not ours
    let _ = EstablishmentKind::Rotate;
}

// ---------------------------------------------------------------------------------------------
// The relying-party layer reads no clock. Scoped to this one file, mirroring the N1 guard in
// `authority_log_no_clock.rs` without widening it.
// ---------------------------------------------------------------------------------------------

#[test]
fn device_authority_module_reads_no_wall_clock() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/device_authority.rs");
    let text = std::fs::read_to_string(&path).expect("device_authority.rs must exist");
    for forbidden in [
        "SystemTime",
        "UNIX_EPOCH",
        "Instant",
        "current_timestamp",
        "try_current_timestamp",
        "icn_time",
        "Utc::now",
        "chrono",
        "Local::now",
        "OffsetDateTime",
        "elapsed(",
        "std::time",
        "OsRng",
        "rand::",
        "thread_rng",
    ] {
        assert!(
            !text.contains(forbidden),
            "device_authority.rs must not reference `{forbidden}`"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Cross-implementation vector. The expected bytes below were produced by an independent
// reference written from the specification prose alone
// (`tests/reference/device_act_reference.py`), which reads these literals back purely as a
// comparison target. Inputs are stated in full so a third implementation can reproduce them.
// ---------------------------------------------------------------------------------------------

/// `subject[i] = 0x20 + i` for i in 0..32 — an opaque 32-byte `SubjectId`.
fn vector_subject() -> SubjectId {
    let mut b = [0u8; 32];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = 0x20u8.wrapping_add(i as u8);
    }
    SubjectId::from_bytes(b)
}
/// `device_seed[i] = 0x40 + i` for i in 0..32.
fn vector_device_key() -> SigningKey {
    let mut b = [0u8; 32];
    for (i, slot) in b.iter_mut().enumerate() {
        *slot = 0x40u8.wrapping_add(i as u8);
    }
    SigningKey::from_bytes(&b)
}
const VECTOR_EVALUATION_POSITION: u64 = 3;
const VECTOR_PAYLOAD: &[u8] = b"fixture:open-workspace";

const EXPECT_DEVICE_PUBKEY_HEX: &str =
    "2543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d";
const EXPECT_ACT_BYTES_HEX: &str = "0000001169636e2e6e342e6465766963652d6163740001202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f012543b92ff1095511476adc8369db6ddc933665a11978dda1404ee1066ca9559d00000000000000030100000016666978747572653a6f70656e2d776f726b7370616365";
const EXPECT_ACT_ID_HEX: &str = "0bab45ffa00c413d8c566d7d2a77b8ef6990c281c50bcf884ec7238301067e3d";
const EXPECT_ACT_SIGNATURE_HEX: &str = "6453bc439a5d7b2124fd72f8637768601f5c7e4ecb3377b4c2413d8aa8d4f933a119dc8be1d53304306c043c7c4833f982d7db12fbc4f5a3386d8a8baa256206";

#[test]
fn canonical_act_vector_matches_the_independent_reference() {
    let key = vector_device_key();
    let device = principal(&key);
    assert_eq!(hex::encode(device.as_bytes()), EXPECT_DEVICE_PUBKEY_HEX);
    let a = DeviceActV1::new(
        vector_subject(),
        device,
        DeviceCapability::Sign,
        VECTOR_EVALUATION_POSITION,
        VECTOR_PAYLOAD.to_vec(),
    )
    .unwrap();
    assert_eq!(hex::encode(a.canonical_bytes()), EXPECT_ACT_BYTES_HEX);
    assert_eq!(hex::encode(a.act_id()), EXPECT_ACT_ID_HEX);
    let signed_act = sign_device_act(&a, &key).unwrap();
    // RFC 8032 signatures are deterministic, so the reference can verify this exact signature
    // over its own independently derived bytes.
    assert_eq!(
        hex::encode(signed_act.signature.as_bytes()),
        EXPECT_ACT_SIGNATURE_HEX
    );
    assert_eq!(
        DeviceActV1::decode(&hex::decode(EXPECT_ACT_BYTES_HEX).unwrap()).unwrap(),
        a
    );
}

// ---------------------------------------------------------------------------------------------
// Verified act versus bare authority evaluation.
// ---------------------------------------------------------------------------------------------

/// Authority alone is not authorship. For a device that *is* authorized, every act whose signature
/// fails is refused, so no unsigned or wrongly signed act reaches a `VerifiedDeviceAct`, even though
/// `evaluate_device_authority` succeeds for the same device at the same position.
#[test]
fn only_a_valid_signature_over_this_act_yields_a_verified_act() {
    let f = fixture();
    let authority =
        evaluate_device_authority(f.s.subject, &f.store, f.b, DeviceCapability::Sign, 3).unwrap();

    let good = signed(f.s.subject, f.b, &f.b_key, 3);
    let unsigned = SignedDeviceAct {
        act: good.act.clone(),
        signature: DeviceActSignature::from_bytes([0u8; 64]),
    };
    let by_another_key = SignedDeviceAct {
        act: good.act.clone(),
        signature: DeviceActSignature::from_bytes(
            f.a_key.sign(&good.act.canonical_bytes()).to_bytes(),
        ),
    };
    let mut other_act = good.clone();
    other_act.act.payload = b"fixture:another-act".to_vec();
    for forged in [&unsigned, &by_another_key, &other_act] {
        assert_eq!(
            verify_device_act(forged, f.s.subject, &f.store, 3),
            Err(DeviceActVerifyError::BadSignature)
        );
    }

    let verified = verify_device_act(&good, f.s.subject, &f.store, 3).unwrap();
    assert_eq!(verified.act(), &good.act);
    assert_eq!(verified.act_id(), good.act.act_id());
    assert_eq!(verified.evidence(), &authority);
}

/// An act whose public fields were changed outside the bounds after construction is never signed
/// and never verified, even with a raw signature over its bytes; the bound itself still signs.
#[test]
fn an_act_moved_outside_its_bounds_is_neither_signed_nor_verified() {
    let f = fixture();
    let valid = act(f.s.subject, f.b, 3, b"fixture:open-workspace");

    let mut oversized = valid.clone();
    oversized.payload = vec![0u8; MAX_DEVICE_ACT_PAYLOAD + 1];
    let mut position_zero = valid.clone();
    position_zero.evaluation_position = 0;
    let mut beyond_bound = valid.clone();
    beyond_bound.evaluation_position = MAX_POSITION + 1;

    for (bad, why) in [
        (
            &oversized,
            DeviceActError::PayloadTooLarge(MAX_DEVICE_ACT_PAYLOAD + 1),
        ),
        (&position_zero, DeviceActError::PositionOutOfRange(0)),
        (
            &beyond_bound,
            DeviceActError::PositionOutOfRange(MAX_POSITION + 1),
        ),
    ] {
        assert_eq!(bad.validate(), Err(why.clone()));
        assert_eq!(sign_device_act(bad, &f.b_key), Err(why.clone()));
        let raw = SignedDeviceAct {
            act: bad.clone(),
            signature: DeviceActSignature::from_bytes(
                f.b_key.sign(&bad.canonical_bytes()).to_bytes(),
            ),
        };
        assert_eq!(
            verify_device_act(&raw, f.s.subject, &f.store, bad.evaluation_position),
            Err(DeviceActVerifyError::MalformedAct(why))
        );
        assert!(DeviceActV1::decode(&bad.canonical_bytes()).is_err());
    }

    // Exactly at the bound: signs, verifies and round-trips.
    let mut at_bound = valid;
    at_bound.payload = vec![0x5a; MAX_DEVICE_ACT_PAYLOAD];
    assert_eq!(at_bound.validate(), Ok(()));
    let signed_at_bound = sign_device_act(&at_bound, &f.b_key).unwrap();
    assert!(verify_device_act(&signed_at_bound, f.s.subject, &f.store, 3).is_ok());
    assert_eq!(
        DeviceActV1::decode(&at_bound.canonical_bytes()),
        Ok(at_bound)
    );
}

// ---------------------------------------------------------------------------------------------
// A verdict at a pinned position is relative to the retained facts, not final.
// ---------------------------------------------------------------------------------------------

/// A accepted at `E = 2`; then a `recover` arrives at position 2, competing with B's grant there.
/// The establishment body supersedes it and a recovery clears every device grant, so the same act
/// at the same `E` is now refused. Pinning `E` did not freeze the facts at or below it.
#[test]
fn a_superseding_recovery_inside_the_prefix_withdraws_an_earlier_verdict() {
    let s = subject_with_plan(0x31, vec![EstablishmentKind::Recover]);
    let (a_key, a) = stranger(DEVICE_A_SEED);
    let (_, b) = stranger(DEVICE_B_SEED);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let a_act = signed(s.subject, a, &a_key, 2);

    let before = store_of(&[s.inception.clone(), auth_a.clone(), auth_b.clone()]);
    let earlier = verify_device_act(&a_act, s.subject, &before, 2).unwrap();
    assert_eq!(earlier.evidence().grant.granted_at, 1);

    let recover = s
        .root
        .establish(1, s.subject, 2, auth_a.body.event_id())
        .expect("recover");
    let after = store_of(&[s.inception.clone(), auth_a, auth_b, recover]);
    assert_eq!(
        verify_device_act(&a_act, s.subject, &after, 2),
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
    match derive_prefix(s.subject, &after, 2) {
        AuthorityView::Live { state, frontier } => {
            assert_eq!(frontier, 3);
            assert_eq!(state.generation, 1);
            assert!(state.devices.is_empty());
        }
        other => panic!("expected the recovery to be selected at 2, got {other:?}"),
    }
}

/// A accepted at `E = 2`; then a second grant, equally authorized, arrives at position 2. The
/// Subject is halted at 2, so the same act at the same `E` is now refused rather than read from
/// either branch.
#[test]
fn a_competing_candidate_inside_the_prefix_halts_an_earlier_verdict() {
    let s = subject(0x32, 2);
    let (a_key, a) = stranger(DEVICE_A_SEED);
    let (_, b) = stranger(DEVICE_B_SEED);
    let auth_a = authorize_at(&s, 0, 1, s.genesis(), a);
    let auth_b = authorize_at(&s, 0, 2, auth_a.body.event_id(), b);
    let a_act = signed(s.subject, a, &a_key, 2);

    let before = store_of(&[s.inception.clone(), auth_a.clone(), auth_b.clone()]);
    assert!(verify_device_act(&a_act, s.subject, &before, 2).is_ok());

    let competing = authorize_at(&s, 0, 2, auth_a.body.event_id(), device(0x77));
    let after = store_of(&[s.inception.clone(), auth_a, auth_b, competing]);
    assert_eq!(
        verify_device_act(&a_act, s.subject, &after, 2),
        Err(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::AuthorityHalted { disputed_at: 2 }
        ))
    );
    // Below the fork, history is still answerable: A's act at 1 still verifies.
    let below = verify_device_act(&signed(s.subject, a, &a_key, 1), s.subject, &after, 1).unwrap();
    assert_eq!(below.evidence().grant.granted_at, 1);
}
