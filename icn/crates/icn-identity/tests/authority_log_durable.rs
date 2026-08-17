//! N1 durable-layer obligations (§19.2, items 1–5).
//!
//! The durable layer is two grow-only sets joined by union. These tests prove the join is a join
//! and that the admission filter commutes with it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use std::collections::BTreeSet;

use ed25519_dalek::{Signer, SigningKey};
use proptest::prelude::*;

use authority_log_support::{
    authorize_at, device, permutations, revoke_at, store_of, stranger, subject,
};
use icn_identity::authority_log::{
    admissible, AdmissionError, AuthorityStore, SignedAuthorityEvent, WitnessSignature,
    MAX_POSITION,
};

/// Build a small, structurally varied pool of admissible events plus some inadmissible ones.
fn event_pool() -> Vec<SignedAuthorityEvent> {
    let alpha = subject(1, 4);
    let beta = subject(2, 4);

    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let a2 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(11)); // conflicting @1
    let a3 = revoke_at(&alpha, 0, 2, a1.body.event_id(), device(10));
    let rotate = alpha
        .root
        .establish(1, alpha.subject, 3, a3.body.event_id())
        .expect("rotate must construct");
    let b1 = authorize_at(&beta, 0, 1, beta.genesis(), device(12));
    // Well-formed but wholly unauthorized: signed by a key the log never authorized.
    let (stranger_key, _) = stranger(200);
    let spam = icn_identity::authority_log::authorize_event(
        &stranger_key,
        alpha.subject,
        7,
        alpha.genesis(),
        device(13),
        authority_log_support::caps(),
        None,
    );

    vec![
        alpha.inception.clone(),
        beta.inception.clone(),
        a1,
        a2,
        a3,
        rotate,
        b1,
        spam,
    ]
}

/// Obligation 1 — `join(A, B) == join(B, A)`.
#[test]
fn join_is_commutative() {
    let pool = event_pool();
    let left = store_of(&pool[..4]);
    let right = store_of(&pool[3..]);

    assert_eq!(
        AuthorityStore::join(&left, &right),
        AuthorityStore::join(&right, &left),
        "join must be commutative"
    );
}

/// Obligation 2 — `join(join(A, B), C) == join(A, join(B, C))`.
#[test]
fn join_is_associative() {
    let pool = event_pool();
    let a = store_of(&pool[..3]);
    let b = store_of(&pool[2..5]);
    let c = store_of(&pool[4..]);

    let left = AuthorityStore::join(&AuthorityStore::join(&a, &b), &c);
    let right = AuthorityStore::join(&a, &AuthorityStore::join(&b, &c));

    assert_eq!(left, right, "join must be associative");
}

/// Obligation 3 — `join(A, A) == A`, including a re-signed identical body.
///
/// Re-signing produces the same body element and *may* produce a second witness. It must never
/// produce a second body: Ed25519 admits more than one valid signature over one message, and if
/// signatures were part of body identity, a hedged or retrying signer would manufacture duplicity
/// and halt the subject.
#[test]
fn join_is_idempotent_including_resigned_bodies() {
    let pool = event_pool();
    let store = store_of(&pool);

    assert_eq!(
        AuthorityStore::join(&store, &store),
        store,
        "join must be idempotent"
    );

    let alpha = subject(1, 4);
    let signing_key = alpha.root.authority_signing_key(0);
    let body = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10)).body;

    // Two genuinely different, genuinely valid signatures over the same canonical body: same
    // secret scalar, different nonce prefix. Both verify under the body's inline signer key.
    let first = SignedAuthorityEvent::new(body.clone(), sign_with(&signing_key, &body));
    let second = SignedAuthorityEvent::new(
        body.clone(),
        second_valid_signature(&signing_key, &body, 0x5a),
    );
    assert_ne!(
        first.signature, second.signature,
        "the two witnesses must actually differ, or this test proves nothing"
    );

    let mut left = AuthorityStore::new();
    assert!(left.ingest(&first).is_ok());
    let mut right = AuthorityStore::new();
    assert!(
        right.ingest(&second).is_ok(),
        "a second valid signature over one body is admissible"
    );

    let joined = AuthorityStore::join(&left, &right);
    assert_eq!(joined.body_count(), 1, "re-signing must not add a body");
    assert_eq!(
        joined.witness_count(),
        2,
        "the witness set may grow; that is not duplicity"
    );
    assert_eq!(
        joined.witnesses_for(body.event_id()).len(),
        2,
        "both witnesses name the same body digest"
    );

    // Idempotence still holds over the joined store, and joining in either order agrees.
    assert_eq!(AuthorityStore::join(&joined, &joined), joined);
    assert_eq!(joined, AuthorityStore::join(&right, &left));

    // A signature from a key that is not the body's inline signer is simply inadmissible.
    let intruder = SigningKey::from_bytes(&[77u8; 32]);
    let forged = SignedAuthorityEvent::new(body.clone(), sign_with(&intruder, &body));
    let mut store = joined.clone();
    assert!(matches!(
        store.ingest(&forged),
        Err(AdmissionError::SignatureInvalid)
    ));
    assert_eq!(store, joined, "a rejected event changes nothing");
}

fn sign_with(
    key: &SigningKey,
    body: &icn_identity::authority_log::AuthorityBody,
) -> WitnessSignature {
    WitnessSignature::from_bytes(key.sign(&body.signature_preimage()).to_bytes())
}

/// Produce a second valid Ed25519 signature over `body` under the same key.
///
/// The nonce prefix lives in the *expanded* secret, not in the public key, so changing it yields
/// a different `R` and therefore a different — but equally valid — signature. This is the
/// hedged-signer case the durable model must tolerate.
fn second_valid_signature(
    key: &SigningKey,
    body: &icn_identity::authority_log::AuthorityBody,
    prefix_byte: u8,
) -> WitnessSignature {
    use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
    use sha2::Sha512;

    let seed = key.to_bytes();
    let mut expanded = ExpandedSecretKey::from(&seed);
    expanded.hash_prefix = [prefix_byte; 32];
    let signature = raw_sign::<Sha512>(&expanded, &body.signature_preimage(), &key.verifying_key());
    WitnessSignature::from_bytes(signature.to_bytes())
}

/// Obligation 4 — permutation-invariant durable state.
///
/// Every delivery permutation of one event multiset, and every partition/merge pattern over it,
/// must produce byte-identical durable state.
#[test]
fn durable_state_is_permutation_invariant() {
    // Exhaustive over a five-event prefix (120 orders x 6 split points). The full eight-event
    // pool is covered by the randomized property below; 8! x 9 store builds would be gratuitous.
    let pool = event_pool()[..5].to_vec();
    let reference = store_of(&pool);

    for permutation in permutations(&pool) {
        // (a) sequential delivery in this order
        assert_eq!(
            store_of(&permutation),
            reference,
            "sequential delivery order must not change durable state"
        );

        // (b) every split into two partitions, merged afterwards
        for split in 0..=permutation.len() {
            let left = store_of(&permutation[..split]);
            let right = store_of(&permutation[split..]);
            assert_eq!(
                AuthorityStore::join(&left, &right),
                reference,
                "partition/merge pattern must not change durable state"
            );
        }
    }
}

/// Obligation 5 — `admissible` is state-independent, as a **differential** property.
///
/// One event is classified while surrounded by many different local states. The classification
/// must be identical every time. This test goes through [`AuthorityStore::ingest`] — the only
/// ingest path — precisely so that a future implementation which consulted the store, the
/// frontier, or the known-body set would fail it. Asserting the property from the function
/// signature alone would be unfalsifiable.
#[test]
fn admissibility_is_state_independent() {
    let alpha = subject(3, 4);
    let subject_under_test = authorize_at(&alpha, 0, 5, alpha.genesis(), device(20));

    // A gap: position 5 arrives with 1..4 absent. A frontier-relative bound would reject it here
    // and accept it once the gap filled.
    let baseline = {
        let mut store = AuthorityStore::new();
        store.ingest(&subject_under_test).is_ok()
    };
    assert!(baseline, "the event under test must be admissible at all");

    let pool = event_pool();
    let mut contexts: Vec<AuthorityStore> = vec![AuthorityStore::new()];
    for permutation in permutations(&pool[..4]) {
        contexts.push(store_of(&permutation));
    }
    // A context that already holds the whole chain up to and past position 5.
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(21));
    let a2 = revoke_at(&alpha, 0, 2, a1.body.event_id(), device(21));
    contexts.push(store_of(&[alpha.inception.clone(), a1, a2]));
    // A context that already holds the very event under test.
    contexts.push(store_of(std::slice::from_ref(&subject_under_test)));
    // A context holding a conflicting body at the same position.
    contexts.push(store_of(&[authorize_at(
        &alpha,
        0,
        5,
        alpha.genesis(),
        device(22),
    )]));

    for (index, context) in contexts.iter().enumerate() {
        let mut candidate = context.clone();
        let verdict = candidate.ingest(&subject_under_test).is_ok();
        assert_eq!(
            verdict, baseline,
            "admissibility changed in surrounding context #{index}: admission consulted state"
        );
        // And the free function must agree with the ingest path, in every context.
        assert_eq!(
            admissible(&subject_under_test.body, &subject_under_test.signature).is_ok(),
            baseline
        );
    }
}

/// The absolute position bound is absolute: it is not `local_frontier + C`.
#[test]
fn position_bound_is_an_absolute_constant() {
    let alpha = subject(4, 2);
    let ok = authorize_at(&alpha, 0, MAX_POSITION, alpha.genesis(), device(30));
    assert!(admissible(&ok.body, &ok.signature).is_ok());

    let too_far = authorize_at(&alpha, 0, MAX_POSITION + 1, alpha.genesis(), device(30));
    assert!(matches!(
        admissible(&too_far.body, &too_far.signature),
        Err(AdmissionError::PositionOutOfRange { .. })
    ));

    // The verdict does not change with a fuller store: an empty store and a store already holding
    // the whole prefix both reject it.
    let mut empty = AuthorityStore::new();
    let mut full = store_of(&event_pool());
    assert!(empty.ingest(&too_far).is_err());
    assert!(full.ingest(&too_far).is_err());
}

/// Malformed events never enter the durable set (obligation 9, durable half).
#[test]
fn malformed_events_are_never_durable() {
    let alpha = subject(5, 2);
    let good = authorize_at(&alpha, 0, 1, alpha.genesis(), device(40));

    // Corrupt the signature: the body is well-formed, the signature is not valid under the inline
    // signer key.
    let mut bad_sig = *good.signature.as_bytes();
    bad_sig[0] ^= 0xff;
    let forged =
        SignedAuthorityEvent::new(good.body.clone(), WitnessSignature::from_bytes(bad_sig));

    let mut store = AuthorityStore::new();
    assert!(matches!(
        store.ingest(&forged),
        Err(AdmissionError::SignatureInvalid)
    ));
    assert_eq!(store.body_count(), 0);
    assert_eq!(store.witness_count(), 0);
}

proptest! {
    /// Obligation 1/2/3 as properties over randomly partitioned event multisets.
    #[test]
    fn join_laws_hold_over_random_partitions(
        assignment in proptest::collection::vec(0usize..3, 8),
    ) {
        let pool = event_pool();
        let mut buckets: [Vec<SignedAuthorityEvent>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for (event, bucket) in pool.iter().zip(assignment.iter()) {
            buckets[*bucket].push(event.clone());
        }
        let a = store_of(&buckets[0]);
        let b = store_of(&buckets[1]);
        let c = store_of(&buckets[2]);

        prop_assert_eq!(AuthorityStore::join(&a, &b), AuthorityStore::join(&b, &a));
        prop_assert_eq!(
            AuthorityStore::join(&AuthorityStore::join(&a, &b), &c),
            AuthorityStore::join(&a, &AuthorityStore::join(&b, &c))
        );
        prop_assert_eq!(AuthorityStore::join(&a, &a), a.clone());

        // Monotone: the join never removes anything.
        let joined = AuthorityStore::join(&a, &b);
        for body in a.bodies() {
            prop_assert!(joined.contains_body(body));
        }
    }

    /// The admission filter commutes with union: `filter(A) ∪ filter(B) = filter(A ∪ B)`.
    ///
    /// This is the property state-independence exists to guarantee. It is stated over the *event*
    /// multiset rather than over stores, because a store already has the filter applied.
    #[test]
    fn admission_filter_commutes_with_union(
        left_mask in proptest::collection::vec(any::<bool>(), 8),
    ) {
        let pool = event_pool();
        let mut left = Vec::new();
        let mut right = Vec::new();
        for (event, in_left) in pool.iter().zip(left_mask.iter()) {
            if *in_left { left.push(event.clone()); } else { right.push(event.clone()); }
        }

        let separately = AuthorityStore::join(&store_of(&left), &store_of(&right));

        let mut together: Vec<SignedAuthorityEvent> = left;
        together.extend(right);
        let jointly = store_of(&together);

        prop_assert_eq!(separately, jointly);
    }
}

/// The durable set really is grow-only: nothing in the public API removes an element.
#[test]
fn durable_state_is_grow_only() {
    let pool = event_pool();
    let mut store = AuthorityStore::new();
    let mut seen: BTreeSet<_> = BTreeSet::new();

    for event in &pool {
        if store.ingest(event).is_ok() {
            seen.insert(event.body.clone());
        }
        for body in &seen {
            assert!(
                store.contains_body(body),
                "a previously admitted body disappeared"
            );
        }
    }
}
