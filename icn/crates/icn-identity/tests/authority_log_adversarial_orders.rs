//! The ten adversarial delivery orders of §9.2.2, each asserted end to end.
//!
//! The requirement in every case is the same: **for one eventual fact set, all honest replicas
//! hold the same durable joined state and compute the same pure derived verdict**, regardless of
//! the order in which they learned the facts or how they were partitioned.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use authority_log_support::{
    authorize_at, caps, device, permutations, revoke_at, store_of, stranger, subject,
};
use icn_identity::authority_log::{
    authorize_event, derive, AuthorityStore, AuthorityView, SignedAuthorityEvent, SubjectId,
    WitnessSignature,
};

fn view(store: &AuthorityStore, s: SubjectId) -> AuthorityView {
    derive(s, store)
}

/// Assert convergence: every permutation and every two-way partition of `events` yields the same
/// durable state and the same derived verdict. Returns the converged verdict.
fn assert_converges(events: &[SignedAuthorityEvent], s: SubjectId) -> AuthorityView {
    let reference_store = store_of(events);
    let reference_view = view(&reference_store, s);

    for permutation in permutations(events) {
        let sequential = store_of(&permutation);
        assert_eq!(
            sequential, reference_store,
            "durable state depends on delivery order"
        );
        assert_eq!(
            view(&sequential, s),
            reference_view,
            "derived verdict depends on delivery order"
        );

        for split in 0..=permutation.len() {
            let merged = AuthorityStore::join(
                &store_of(&permutation[..split]),
                &store_of(&permutation[split..]),
            );
            assert_eq!(
                merged, reference_store,
                "durable state depends on partition"
            );
            assert_eq!(
                view(&merged, s),
                reference_view,
                "derived verdict depends on partition"
            );
        }
    }
    reference_view
}

fn frontier(v: &AuthorityView) -> Option<u64> {
    match v {
        AuthorityView::Live { frontier, .. } => Some(*frontier),
        _ => None,
    }
}

fn disputed(v: &AuthorityView) -> Option<u64> {
    match v {
        AuthorityView::Halted { disputed_at, .. } => Some(*disputed_at),
        _ => None,
    }
}

/// Cases 1 and 2 — `n` before `n−1`, and `n−1` before `n`, reach identical state.
#[test]
fn case_1_and_2_out_of_order_positions_converge() {
    let alpha = subject(100, 4);
    let at_one = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let at_two = authorize_at(&alpha, 0, 2, at_one.body.event_id(), device(11));

    let verdict = assert_converges(&[alpha.inception.clone(), at_one, at_two], alpha.subject);
    assert_eq!(frontier(&verdict), Some(3));
}

/// Cases 3, 4 and 5 — `X@n` then `Y@n`, `Y@n` then `X@n`, and independent replicas merging.
#[test]
fn case_3_4_5_conflicting_same_position_converge_on_halted() {
    let alpha = subject(101, 4);
    let x = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let y = authorize_at(&alpha, 0, 1, alpha.genesis(), device(11));

    let verdict = assert_converges(&[alpha.inception.clone(), x, y], alpha.subject);
    assert_eq!(disputed(&verdict), Some(1));
}

/// Case 6 — an identical event delivered repeatedly is idempotent, including re-signings.
#[test]
fn case_6_repeated_delivery_is_idempotent() {
    use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
    use sha2::Sha512;

    let alpha = subject(102, 4);
    let event = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    let key = alpha.root.authority_signing_key(0);
    let seed = key.to_bytes();
    let mut expanded = ExpandedSecretKey::from(&seed);
    expanded.hash_prefix = [0x77; 32];
    let resigned = SignedAuthorityEvent::new(
        event.body.clone(),
        WitnessSignature::from_bytes(
            raw_sign::<Sha512>(
                &expanded,
                &event.body.signature_preimage(),
                &key.verifying_key(),
            )
            .to_bytes(),
        ),
    );

    let events = vec![
        alpha.inception.clone(),
        event.clone(),
        event.clone(),
        resigned,
    ];
    let verdict = assert_converges(&events, alpha.subject);
    assert_eq!(frontier(&verdict), Some(2));

    let store = store_of(&events);
    assert_eq!(store.bodies_for(alpha.subject).len(), 2);
    assert_eq!(store.witnesses_for(event.body.event_id()).len(), 2);
}

/// Case 7 — an invalid event arriving before the valid one blocks nothing.
#[test]
fn case_7_invalid_before_valid_blocks_nothing() {
    let alpha = subject(103, 4);
    let valid = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    // Malformed: the signature does not verify under the inline signer key. Never durable.
    let mut broken = *valid.signature.as_bytes();
    broken[7] ^= 0xff;
    let malformed =
        SignedAuthorityEvent::new(valid.body.clone(), WitnessSignature::from_bytes(broken));

    // Unauthorized but well-formed: durable, never a candidate.
    let (stranger_key, _) = stranger(210);
    let unauthorized = authorize_event(
        &stranger_key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );

    let malformed_first = store_of(&[
        alpha.inception.clone(),
        malformed.clone(),
        unauthorized.clone(),
        valid.clone(),
    ]);
    let valid_first = store_of(&[
        alpha.inception.clone(),
        valid.clone(),
        malformed,
        unauthorized.clone(),
    ]);

    assert_eq!(malformed_first, valid_first);
    assert!(malformed_first.contains_body(&unauthorized.body));
    assert_eq!(frontier(&view(&malformed_first, alpha.subject)), Some(2));
    assert_eq!(
        view(&malformed_first, alpha.subject),
        view(&valid_first, alpha.subject)
    );
}

/// Case 8 — a rotation without the pre-committed material is retained and ignored.
#[test]
fn case_8_rotation_without_reveal_is_retained_and_ignored() {
    use icn_identity::authority_log::{ContextNonce, ContinuityRoot};

    let alpha = subject(104, 4);
    let impostor = ContinuityRoot::new([250u8; 32], ContextNonce::from_bytes([250u8; 32]), 4);
    let bogus = impostor
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("constructs");
    let legit = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    let verdict = assert_converges(
        &[alpha.inception.clone(), bogus.clone(), legit],
        alpha.subject,
    );
    assert_eq!(
        frontier(&verdict),
        Some(2),
        "an unauthorized establishment must not supersede the legitimate event"
    );
    assert!(store_of(std::slice::from_ref(&bogus)).contains_body(&bogus.body));
}

/// Case 9 — a compromised current key forks a non-establishment event, unilaterally.
#[test]
fn case_9_compromised_current_key_forks_unilaterally() {
    let alpha = subject(105, 4);
    let key = alpha.root.authority_signing_key(0);
    let first = authorize_event(
        &key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(10),
        caps(),
        None,
    );
    let second = authorize_event(
        &key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );

    let verdict = assert_converges(&[alpha.inception.clone(), first, second], alpha.subject);
    assert_eq!(
        disputed(&verdict),
        Some(1),
        "current-key compromise is an instant DoS; the model converges ON A HALTED SUBJECT"
    );
}

/// Case 10 — a legitimate superseding establishment arriving afterwards recovers the subject,
/// deterministically, in every delivery order.
#[test]
fn case_10_superseding_recovery_converges() {
    let alpha = subject(106, 4);
    let key = alpha.root.authority_signing_key(0);
    let attacker = authorize_event(
        &key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(10),
        caps(),
        None,
    );
    let victim = authorize_at(&alpha, 0, 1, alpha.genesis(), device(11));
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");

    let verdict = assert_converges(
        &[alpha.inception.clone(), attacker, victim, rotation.clone()],
        alpha.subject,
    );
    assert_eq!(frontier(&verdict), Some(2));

    // And the recovered chain continues under the new authority in any order.
    let after = authorize_at(&alpha, 1, 2, rotation.body.event_id(), device(12));
    let verdict = assert_converges(&[alpha.inception.clone(), rotation, after], alpha.subject);
    assert_eq!(frontier(&verdict), Some(3));
}

/// A longer mixed scenario: gaps, spam, a fork, and recovery, all converging.
#[test]
fn mixed_scenario_converges_under_every_order() {
    let alpha = subject(107, 4);
    let key = alpha.root.authority_signing_key(0);

    let one = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let fork_a = authorize_event(
        &key,
        alpha.subject,
        2,
        one.body.event_id(),
        device(11),
        caps(),
        None,
    );
    let fork_b = authorize_event(
        &key,
        alpha.subject,
        2,
        one.body.event_id(),
        device(12),
        caps(),
        None,
    );
    let (stranger_key, _) = stranger(220);
    let spam = authorize_event(
        &stranger_key,
        alpha.subject,
        2,
        one.body.event_id(),
        device(13),
        caps(),
        None,
    );
    let buffered = authorize_at(&alpha, 0, 9, alpha.genesis(), device(14));

    let halted = assert_converges(
        &[
            alpha.inception.clone(),
            one.clone(),
            fork_a,
            fork_b,
            spam,
            buffered.clone(),
        ],
        alpha.subject,
    );
    assert_eq!(disputed(&halted), Some(2));

    // Recovery at the disputed position.
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 2, one.body.event_id())
        .expect("rotate");
    let recovered = assert_converges(
        &[alpha.inception.clone(), one, rotation, buffered],
        alpha.subject,
    );
    assert_eq!(frontier(&recovered), Some(3));
}

/// Revocation of a device is an ordinary non-establishment event and converges the same way.
#[test]
fn revocation_converges() {
    let alpha = subject(108, 4);
    let grant = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let revoke = revoke_at(&alpha, 0, 2, grant.body.event_id(), device(10));

    let verdict = assert_converges(&[alpha.inception.clone(), grant, revoke], alpha.subject);
    let state = verdict.state().expect("state");
    assert!(state.devices.is_empty());
}
