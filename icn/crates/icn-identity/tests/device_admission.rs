//! N4-E — current admission: the relying party's position, computed from its own facts.
//!
//! N4-A answers a *historical* question: was this device covered at position `E`? That must
//! stay answerable. But a relying party admitting a class-2 act *now* must not let the device
//! choose an arbitrarily old `E` at which it was still authorized: a device revoked at 3 could
//! otherwise keep signing acts bound to 2 forever. The admission position is therefore computed
//! from the relying party's **own retained facts** — the last position it holds a clean prefix
//! through, `frontier − 1` — and the act must bind exactly that position.
//!
//! > The retained current position is local evidence, not universal finality. No clock, no
//! > registrar, no "latest globally": a relying party that is behind refuses and must obtain
//! > facts; a relying party that holds an old prefix will, honestly, admit at its old position.
//!
//! Fixture identities only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{
    authorize_at, permutations, revoke_at, store_of, stranger, subject, Subject,
};
use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{AuthorityStore, DeviceCapability, PrincipalKey, SubjectId};
use icn_identity::device_admission::{
    admission_position, verify_device_act_at_admission, AdmissionRefusal,
};
use icn_identity::device_authority::{
    sign_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal, SignedDeviceAct,
};

struct Fixture {
    s: Subject,
    a_key: SigningKey,
    a: PrincipalKey,
    b_key: SigningKey,
    b: PrincipalKey,
    /// inception, A@1, B@2, revoke A@3
    facts: Vec<icn_identity::authority_log::SignedAuthorityEvent>,
}

fn fixture() -> Fixture {
    let s = subject(0x01, 4);
    let (a_key, a) = stranger(0x11);
    let (b_key, b) = stranger(0x22);
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
        b"act".to_vec(),
    )
    .unwrap();
    sign_device_act(&act, key).unwrap()
}

#[test]
fn an_up_to_date_relying_party_admits_at_its_clean_tip() {
    let f = fixture();
    let store = store_of(&f.facts); // frontier 4
    assert_eq!(admission_position(f.s.subject, &store), Ok(3));
    // B, bound to the admission position: admitted, with its grant as evidence.
    let ev =
        verify_device_act_at_admission(&signed(f.s.subject, f.b, &f.b_key, 3), f.s.subject, &store)
            .expect("B is authorized at the admission position");
    assert_eq!(
        (ev.evaluation_position, ev.grant.granted_at, ev.generation),
        (3, 2, 0)
    );
    // A, revoked at 3, bound to the admission position: refused on authority.
    assert_eq!(
        verify_device_act_at_admission(&signed(f.s.subject, f.a, &f.a_key, 3), f.s.subject, &store)
            .unwrap_err(),
        AdmissionRefusal::Act(DeviceActVerifyError::Refused(
            DeviceAuthorityRefusal::DeviceNotAuthorized
        ))
    );
}

#[test]
fn a_revoked_device_replaying_an_older_position_is_refused_for_current_admission() {
    // The historical verifier can still explain that A was valid at 2; current admission must
    // refuse an act that binds 2 when the relying party's tip is 3.
    let f = fixture();
    let store = store_of(&f.facts);
    let stale_act = signed(f.s.subject, f.a, &f.a_key, 2);
    assert!(
        icn_identity::device_authority::verify_device_act(&stale_act, f.s.subject, &store, 2)
            .is_ok(),
        "history: A was authorized at 2"
    );
    assert_eq!(
        verify_device_act_at_admission(&stale_act, f.s.subject, &store).unwrap_err(),
        AdmissionRefusal::StaleAct {
            act: 2,
            admission: 3
        }
    );
}

#[test]
fn an_act_bound_beyond_the_relying_partys_facts_means_it_is_behind_and_refuses() {
    let f = fixture();
    let behind = store_of(&f.facts[..3]); // holds through 2; frontier 3
    assert_eq!(admission_position(f.s.subject, &behind), Ok(2));
    assert_eq!(
        verify_device_act_at_admission(
            &signed(f.s.subject, f.b, &f.b_key, 3),
            f.s.subject,
            &behind
        )
        .unwrap_err(),
        AdmissionRefusal::RelyingPartyBehind {
            act: 3,
            admission: 2
        }
    );
}

#[test]
fn a_relying_party_that_has_not_received_the_revocation_admits_at_its_old_tip_and_says_so() {
    // The honest class-2 limit, pinned rather than hidden: a relying party whose facts stop
    // before the revocation holds a clean prefix through 2 and admits A there. It is not wrong
    // about its facts; it is behind. Obtaining facts is the only cure (N3), never a clock.
    let f = fixture();
    let behind = store_of(&f.facts[..3]);
    let ev = verify_device_act_at_admission(
        &signed(f.s.subject, f.a, &f.a_key, 2),
        f.s.subject,
        &behind,
    )
    .expect("admitted under the facts it holds");
    assert_eq!(ev.evaluation_position, 2);
}

#[test]
fn two_simultaneously_valid_devices_bind_the_same_current_position() {
    let f = fixture();
    let before_revoke = store_of(&f.facts[..3]); // A and B both in force; tip 2
    assert!(verify_device_act_at_admission(
        &signed(f.s.subject, f.a, &f.a_key, 2),
        f.s.subject,
        &before_revoke
    )
    .is_ok());
    assert!(verify_device_act_at_admission(
        &signed(f.s.subject, f.b, &f.b_key, 2),
        f.s.subject,
        &before_revoke
    )
    .is_ok());
}

#[test]
fn unknown_halted_and_inception_only_subjects_have_no_admission_position() {
    let f = fixture();
    // Unknown: an empty store.
    assert_eq!(
        admission_position(f.s.subject, &AuthorityStore::new()),
        Err(AdmissionRefusal::SubjectUnknown)
    );
    // Inception only: no delegation exists; position 0 is inception, never an admission.
    let inception_only = store_of(std::slice::from_ref(&f.s.inception));
    assert_eq!(
        admission_position(f.s.subject, &inception_only),
        Err(AdmissionRefusal::NoDelegationYet)
    );
    // Halted: two authorized candidates at position 1 (A and a second grant with a different
    // device) fork the log; current authority is disputed, so nothing is admitted at or above.
    let (_, c) = stranger(0x33);
    let fork = authorize_at(&f.s, 0, 1, f.s.genesis(), c);
    let halted = store_of(&[f.s.inception.clone(), f.facts[1].clone(), fork]);
    assert_eq!(
        admission_position(f.s.subject, &halted),
        Err(AdmissionRefusal::AuthorityHalted { disputed_at: 1 })
    );
    assert!(matches!(
        verify_device_act_at_admission(
            &signed(f.s.subject, f.a, &f.a_key, 1),
            f.s.subject,
            &halted
        ),
        Err(AdmissionRefusal::AuthorityHalted { disputed_at: 1 })
    ));
}

#[test]
fn admission_is_a_function_of_the_facts_not_their_arrival_order() {
    let f = fixture();
    for order in permutations(&f.facts) {
        let store = store_of(&order);
        assert_eq!(admission_position(f.s.subject, &store), Ok(3));
        assert!(verify_device_act_at_admission(
            &signed(f.s.subject, f.b, &f.b_key, 3),
            f.s.subject,
            &store
        )
        .is_ok());
        assert!(verify_device_act_at_admission(
            &signed(f.s.subject, f.a, &f.a_key, 3),
            f.s.subject,
            &store
        )
        .is_err());
    }
}

#[test]
fn admission_reads_no_wall_clock_and_no_environment() {
    let src = include_str!("../src/device_admission.rs");
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
            "device_admission.rs must not use {forbidden}"
        );
    }
}
