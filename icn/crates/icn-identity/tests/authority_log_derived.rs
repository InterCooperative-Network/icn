//! N1 derived-layer obligations (§19.2, items 6–19).
//!
//! The derived view is a pure function of `Bodies(σ)`. These tests prove determinism, gap
//! buffering, halt-on-duplicity, superseding recovery, suffix orphaning, and the absence of any
//! content-based branch selection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod authority_log_support;

use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;

use authority_log_support::{
    authorize_at, authorize_span_at, caps, caps_with_recover, device, permutations, principal,
    revoke_at, store_of, stranger, subject, subject_with_plan, Subject,
};
use icn_identity::authority_log::{
    authorize_event, derive, resolve, sign_body, supersede, AuthorityBody, AuthorityStore,
    AuthorityView, Commitment, ContextNonce, ContinuityRoot, DeviceCapability, EstablishmentBody,
    EstablishmentKind, EventHeader, EventId, PrincipalKey, PrincipalSet, Resolution,
    SignedAuthorityEvent, SubjectId, ValiditySpan, WitnessSignature,
};

fn view(store: &AuthorityStore, s: SubjectId) -> AuthorityView {
    derive(s, store)
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

/// An unknown subject derives to `Unknown`, never to a partial state.
#[test]
fn unknown_subject_derives_unknown() {
    let alpha = subject(1, 4);
    let store = AuthorityStore::new();
    assert_eq!(view(&store, alpha.subject), AuthorityView::Unknown);

    // A store holding only non-inception bodies for σ is still `Unknown`: nothing anchors them.
    let orphan = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let store = store_of(&[orphan]);
    assert_eq!(view(&store, alpha.subject), AuthorityView::Unknown);
}

/// Obligation 6 — the derived view is deterministic across permutations and replicas.
#[test]
fn derived_view_is_deterministic() {
    let alpha = subject(2, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let a2 = revoke_at(&alpha, 0, 2, a1.body.event_id(), device(10));
    let rotate = alpha
        .root
        .establish(1, alpha.subject, 3, a2.body.event_id())
        .expect("rotate");
    let a4 = authorize_at(&alpha, 1, 4, rotate.body.event_id(), device(11));
    let events = vec![alpha.inception.clone(), a1, a2, rotate, a4];

    let reference = view(&store_of(&events), alpha.subject);
    assert_eq!(frontier(&reference), Some(5));

    for permutation in permutations(&events) {
        // Same replica, different arrival order.
        assert_eq!(view(&store_of(&permutation), alpha.subject), reference);

        // Two independent replicas that partition the events and merge later.
        for split in 0..=permutation.len() {
            let merged = AuthorityStore::join(
                &store_of(&permutation[..split]),
                &store_of(&permutation[split..]),
            );
            assert_eq!(view(&merged, alpha.subject), reference);
        }
    }
}

/// Obligation 7 — gaps buffer. An out-of-order event is retained, unused, never rejected, and
/// never wedges the subject once the gap fills.
#[test]
fn gaps_buffer_and_never_wedge() {
    let alpha = subject(3, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let a2 = authorize_at(&alpha, 0, 2, a1.body.event_id(), device(11));

    // Position 2 arrives before position 1.
    let mut store = store_of(&[alpha.inception.clone(), a2.clone()]);
    assert!(
        store.contains_body(&a2.body),
        "the future event must be durably retained, not rejected"
    );
    assert_eq!(
        frontier(&view(&store, alpha.subject)),
        Some(1),
        "derive halts advancement at the gap"
    );

    // The gap fills; derive advances straight through it.
    assert!(store.ingest(&a1).is_ok());
    assert_eq!(frontier(&view(&store, alpha.subject)), Some(3));

    // The reverse order reaches exactly the same place (§9.2.2 cases 1 and 2).
    let in_order = store_of(&[alpha.inception.clone(), a1, a2]);
    assert_eq!(store, in_order);
    assert_eq!(view(&store, alpha.subject), view(&in_order, alpha.subject));
}

/// Obligation 8 — conflicting same-position events halt, identically in both arrival orders and
/// after a replica merge (§9.2.2 cases 3, 4 and 5).
#[test]
fn conflicting_same_position_events_halt() {
    let alpha = subject(4, 4);
    let x = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let y = authorize_at(&alpha, 0, 1, alpha.genesis(), device(11));
    assert_ne!(x.body, y.body);

    let xy = store_of(&[alpha.inception.clone(), x.clone(), y.clone()]);
    let yx = store_of(&[alpha.inception.clone(), y.clone(), x.clone()]);
    let merged = AuthorityStore::join(
        &store_of(&[alpha.inception.clone(), x.clone()]),
        &store_of(&[alpha.inception.clone(), y.clone()]),
    );

    assert_eq!(xy, yx, "durable state is order-independent");
    assert_eq!(xy, merged, "durable state is partition-independent");

    let verdict = view(&xy, alpha.subject);
    assert_eq!(disputed(&verdict), Some(1));
    assert_eq!(view(&yx, alpha.subject), verdict);
    assert_eq!(view(&merged, alpha.subject), verdict);

    match verdict {
        AuthorityView::Halted { candidates, .. } => {
            assert_eq!(candidates.len(), 2);
            assert!(candidates.contains(&x.body));
            assert!(candidates.contains(&y.body));
        }
        other => panic!("expected Halted, got {other:?}"),
    }
}

/// Obligation 9 — unauthorized or malformed events cannot fork or block (§9.2.2 case 7).
#[test]
fn spam_cannot_fork_or_block() {
    let alpha = subject(5, 4);
    let legit = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    // A stranger signs a well-formed body at the same position, correctly naming the prefix.
    let (stranger_key, _) = stranger(200);
    let spam = authorize_event(
        &stranger_key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );

    let store = store_of(&[alpha.inception.clone(), legit.clone(), spam.clone()]);
    assert!(
        store.contains_body(&spam.body),
        "authenticated-but-unauthorized bodies ARE durably known"
    );

    let verdict = view(&store, alpha.subject);
    assert_eq!(
        frontier(&verdict),
        Some(2),
        "spam must neither fork nor block; the legitimate event advances"
    );

    // Flooding position 1 with a thousand distinct spam bodies changes nothing.
    let mut flooded = store.clone();
    for seed in 0u16..1000 {
        let key = SigningKey::from_bytes(&[(seed % 251) as u8 + 1; 32]);
        let junk = authorize_event(
            &key,
            alpha.subject,
            1,
            alpha.genesis(),
            device((seed % 200) as u8),
            caps(),
            None,
        );
        let _ = flooded.ingest(&junk);
    }
    assert!(flooded.body_count() > store.body_count());
    assert_eq!(
        view(&flooded, alpha.subject),
        verdict,
        "unauthorized bodies never enter the candidate set"
    );
}

/// Obligation 10 — pre-rotation. A correct reveal is authorized; a wrong or missing reveal stays
/// durably known but never becomes an authorized establishment candidate (§9.2.2 case 8).
#[test]
fn pre_rotation_reveal_gates_establishment() {
    let alpha = subject(6, 4);
    let good = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");

    let store = store_of(&[alpha.inception.clone(), good.clone()]);
    assert_eq!(frontier(&view(&store, alpha.subject)), Some(2));

    // A rotation whose revealed material is not the pre-committed material. It is well-formed and
    // self-signed, so it is admissible — but it is not authorized.
    let impostor_root = ContinuityRoot::new([99u8; 32], ContextNonce::from_bytes([99u8; 32]), 4);
    let forged = impostor_root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("forged rotate constructs");

    let mut store = store_of(std::slice::from_ref(&alpha.inception));
    assert!(
        store.ingest(&forged).is_ok(),
        "a wrong reveal is still admissible"
    );
    assert!(store.contains_body(&forged.body));
    assert_eq!(
        frontier(&view(&store, alpha.subject)),
        Some(1),
        "a wrong reveal must never become an authorized establishment candidate"
    );

    // And it never becomes one even alongside the real rotation: supersede sees exactly one
    // *authorized* establishment.
    assert!(store.ingest(&good).is_ok());
    assert_eq!(frontier(&view(&store, alpha.subject)), Some(2));

    // Once the chain is exhausted, nothing further is armed.
    let short = subject(7, 1);
    let only = short
        .root
        .establish(1, short.subject, 1, short.genesis())
        .expect("rotate");
    let store = store_of(&[short.inception.clone(), only.clone()]);
    let state = match view(&store, short.subject) {
        AuthorityView::Live { state, .. } => state,
        other => panic!("expected Live, got {other:?}"),
    };
    assert_eq!(state.next_commitment, Commitment::TERMINAL);
    assert!(short
        .root
        .establish(2, short.subject, 2, only.body.event_id())
        .is_err());
}

/// Obligation 11 — a compromised *current* key can fork a non-establishment event. Pre-rotation
/// does **not** prevent this; the model detects it (§9.2.2 case 9). N1 does not claim otherwise.
#[test]
fn compromised_current_key_forks_non_establishment_events() {
    let alpha = subject(8, 4);
    let compromised = alpha.root.authority_signing_key(0);

    let victim = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let attacker = authorize_event(
        &compromised,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );

    let store = store_of(&[alpha.inception.clone(), victim, attacker]);
    assert_eq!(
        disputed(&view(&store, alpha.subject)),
        Some(1),
        "the fork is DETECTED; pre-rotation does not prevent it"
    );

    // The compromised key cannot rotate: it holds no pre-committed material for generation 1.
    let attempted_rotation = AuthorityBody::Rotate(EstablishmentBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 1,
            prev_digest: alpha.genesis(),
            signer: principal(&compromised),
        },
        revealed_authority: PrincipalSet::single(principal(&compromised)),
        next_commitment: Commitment::from_bytes([7u8; 32]),
    });
    let signed = sign_body(attempted_rotation, &compromised);
    let mut store = store.clone();
    assert!(store.ingest(&signed).is_ok(), "admissible");
    let attempted_only = store_of(&[alpha.inception.clone(), signed]);
    assert_eq!(
        frontier(&view(&attempted_only, alpha.subject)),
        Some(1),
        "a compromised current key cannot rotate"
    );
}

/// Obligation 12a — a legitimate establishment event supersedes conflicting non-establishment
/// candidates at the same position and advances (§9.2.2 case 10).
#[test]
fn superseding_recovery_advances() {
    let alpha = subject(9, 4);
    let compromised = alpha.root.authority_signing_key(0);
    let attacker = authorize_event(
        &compromised,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );
    let victim = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    let forked = store_of(&[alpha.inception.clone(), attacker.clone(), victim.clone()]);
    assert_eq!(disputed(&view(&forked, alpha.subject)), Some(1));

    // Recovery must rotate AT the disputed position, not the next one.
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");
    let mut recovered = forked.clone();
    assert!(recovered.ingest(&rotation).is_ok());

    let verdict = view(&recovered, alpha.subject);
    assert_eq!(
        frontier(&verdict),
        Some(2),
        "the establishment event supersedes both non-establishment candidates"
    );

    // The victim's own event at the disputed position is filtered out too. That is a real cost of
    // recovery, not a bug (§9.2.2, operational consequence 1).
    let state = verdict.state().expect("state");
    assert!(!state.devices.contains_key(&device(10)));
    assert!(!state.devices.contains_key(&device(11)));

    // Superseding recovery terminates: the compromised key is no longer in the authority set.
    assert!(!state.authority.contains(&principal(&compromised)));
    let replay = authorize_event(
        &compromised,
        alpha.subject,
        2,
        rotation.body.event_id(),
        device(12),
        caps(),
        None,
    );
    let mut after = recovered.clone();
    assert!(after.ingest(&replay).is_ok());
    assert_eq!(
        frontier(&view(&after, alpha.subject)),
        Some(2),
        "the rotated-out key cannot author again"
    );
}

/// Obligation 12b — two authorized establishment bodies at one position must **halt**, not choose.
///
/// The commitment scheme makes that state unreachable in practice (see
/// [`two_authorized_establishment_bodies_are_unconstructible`]), so the rule is asserted against
/// [`resolve`] — the single decision point `derive` itself calls, not a reimplementation of it.
#[test]
fn two_establishment_candidates_halt_rather_than_choose() {
    let alpha = subject(10, 4);
    let beta = subject(11, 4);

    let first = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");
    // A structurally identical establishment body from a different chain, at the same position.
    let second = beta
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");
    assert_ne!(first.body, second.body);
    assert!(first.body.is_establishment() && second.body.is_establishment());

    let candidates: BTreeSet<AuthorityBody> = [first.body.clone(), second.body.clone()]
        .into_iter()
        .collect();

    // `supersede` keeps both: authority class cannot distinguish them.
    let survivors = supersede(candidates.clone());
    assert_eq!(survivors.len(), 2);

    // And `resolve` halts rather than selecting one.
    match resolve(candidates) {
        Resolution::Halt(set) => {
            assert_eq!(set.len(), 2);
            assert!(set.contains(&first.body));
            assert!(set.contains(&second.body));
        }
        other => panic!("expected Halt, got {other:?}"),
    }
}

/// §9.2.1 constraint 3 — the reachable-state half of obligation 12b.
///
/// Because the derived candidate filter enforces canonical construction and each generation
/// commits to exactly one next authority, no honest retry and no malicious re-issue can produce a
/// *second* authorized establishment body at a position. Each variant is admitted alongside the
/// canonical body, but none can make the public derived view halt or choose it.
#[test]
fn two_authorized_establishment_bodies_are_unconstructible() {
    let alpha = subject(12, 4);
    let canonical = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");
    let revealed = match &canonical.body {
        AuthorityBody::Rotate(e) => e.revealed_authority.clone(),
        other => panic!("expected Rotate, got {other:?}"),
    };
    let signer = revealed.canonical_signer().expect("non-empty");
    let signing_key = alpha.root.authority_signing_key(1);

    let cannot_compete = |variant: SignedAuthorityEvent, reason: &str| {
        assert_ne!(
            variant.body, canonical.body,
            "{reason}: variant must differ"
        );
        let store = store_of(&[alpha.inception.clone(), canonical.clone(), variant.clone()]);
        assert!(store.contains_body(&variant.body), "{reason}: admitted");
        match view(&store, alpha.subject) {
            AuthorityView::Live { state, frontier } => {
                assert_eq!(frontier, 2, "{reason}: canonical body must advance alone");
                assert_eq!(state.generation, 1, "{reason}: one establishment applied");
                assert_eq!(state.authority, revealed.members().clone());
            }
            other => panic!("{reason}: alternate body became a candidate: {other:?}"),
        }
    };

    // Context fields are selected by the derived chain, not by the state-only authority rule.
    cannot_compete(
        alpha
            .root
            .establish(1, SubjectId::from_bytes([0x5c; 32]), 1, alpha.genesis())
            .expect("foreign-subject shape"),
        "foreign subject",
    );
    cannot_compete(
        alpha
            .root
            .establish(1, alpha.subject, 2, alpha.genesis())
            .expect("wrong-position shape"),
        "wrong position",
    );
    cannot_compete(
        alpha
            .root
            .establish(1, alpha.subject, 1, EventId::from_bytes([0xa7; 32]))
            .expect("wrong-parent shape"),
        "wrong parent",
    );

    // (a) The holder retries with a *fresh* next commitment — the §9.2.1 constraint 2 scenario.
    let fresh_commitment = AuthorityBody::Rotate(EstablishmentBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 1,
            prev_digest: alpha.genesis(),
            signer,
        },
        revealed_authority: revealed.clone(),
        next_commitment: Commitment::from_bytes([0xab; 32]),
    });
    cannot_compete(
        sign_body(fresh_commitment, &signing_key),
        "fresh next commitment",
    );

    // (b) The same revealed material re-labelled as a `recover` rather than a `rotate`. The kind
    //     is inside the commitment, so exactly one of the two is ever armed.
    let other_kind = match &canonical.body {
        AuthorityBody::Rotate(e) => AuthorityBody::Recover(e.clone()),
        other => panic!("expected Rotate, got {other:?}"),
    };
    cannot_compete(sign_body(other_kind, &signing_key), "alternate kind");

    // (c) A non-canonical signer drawn from the revealed set is rejected even though it is
    //     legitimately part of that set's material.
    let extra_key = SigningKey::from_bytes(&[3u8; 32]);
    let extra = principal(&extra_key);
    let mut widened: BTreeSet<PrincipalKey> = revealed.members().clone();
    widened.insert(extra);
    let widened_set = PrincipalSet::new(widened, "revealed_authority").expect("non-empty");
    let widened_body = AuthorityBody::Rotate(EstablishmentBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 1,
            prev_digest: alpha.genesis(),
            signer,
        },
        revealed_authority: widened_set,
        next_commitment: match &canonical.body {
            AuthorityBody::Rotate(e) => e.next_commitment,
            other => panic!("expected Rotate, got {other:?}"),
        },
    });
    cannot_compete(
        sign_body(widened_body, &signing_key),
        "widened authority set",
    );

    // (d) The construction API itself is deterministic and refuses off-chain generations.
    let retry = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");
    assert_eq!(
        retry.body.canonical_bytes(),
        canonical.body.canonical_bytes()
    );
    assert!(sign_body(canonical.body.clone(), &signing_key).body == canonical.body);
}

/// An establishment generation cannot be skipped, replayed, or reused at a later position.
#[test]
fn establishment_generation_cannot_be_skipped_or_reused() {
    let alpha = subject(13, 4);
    let skipped = alpha
        .root
        .establish(2, alpha.subject, 1, alpha.genesis())
        .expect("generation two constructs as bytes");
    let skipped_store = store_of(&[alpha.inception.clone(), skipped]);
    assert_eq!(frontier(&view(&skipped_store, alpha.subject)), Some(1));

    let first = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("first establishment");
    let replay = alpha
        .root
        .establish(1, alpha.subject, 2, first.body.event_id())
        .expect("replay-shaped body constructs");
    let replay_store = store_of(&[alpha.inception.clone(), first.clone(), replay]);
    let replay_view = view(&replay_store, alpha.subject);
    assert_eq!(frontier(&replay_view), Some(2));
    assert_eq!(replay_view.state().expect("state").generation, 1);

    let second = alpha
        .root
        .establish(2, alpha.subject, 2, first.body.event_id())
        .expect("next generation");
    let sequential = store_of(&[alpha.inception.clone(), first, second]);
    let sequential_view = view(&sequential, alpha.subject);
    assert_eq!(frontier(&sequential_view), Some(3));
    assert_eq!(sequential_view.state().expect("state").generation, 2);
}

/// Obligation 13a — the verdict is invariant under relabeling of body digests.
///
/// The scenario is fixed and the *instantiation* varies: different roots and different device
/// keys give completely different `event_id`s while preserving every structural and authority
/// relation. Any lowest-hash, greatest-hash or ordering-based tiebreak would let the winner's
/// structural role drift between instantiations. The verdict must not move.
#[test]
fn verdict_is_invariant_under_event_id_relabeling() {
    for seed in 20u8..60 {
        let s = subject(seed, 4);
        // Two structurally distinct roles at one position: "alpha device" and "beta device".
        let alpha_role = authorize_at(&s, 0, 1, s.genesis(), device(seed.wrapping_add(100)));
        let beta_role = authorize_at(&s, 0, 1, s.genesis(), device(seed.wrapping_add(150)));

        let store = store_of(&[s.inception.clone(), alpha_role.clone(), beta_role.clone()]);
        let verdict = view(&store, s.subject);

        assert_eq!(
            disputed(&verdict),
            Some(1),
            "instantiation {seed} produced a non-Halted verdict: a content tiebreak is present"
        );
        match verdict {
            AuthorityView::Halted { candidates, .. } => {
                // Both roles survive, in every instantiation. A hash-order selector would drop one.
                assert!(candidates.contains(&alpha_role.body));
                assert!(candidates.contains(&beta_role.body));
            }
            other => panic!("expected Halted, got {other:?}"),
        }
    }
}

/// Obligation 13b — every candidate set that survives `supersede` with size ≥ 2 halts.
#[test]
fn every_multi_candidate_set_halts() {
    let alpha = subject(61, 4);
    let signer = alpha.root.authority_signing_key(0);
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");

    // Build candidate sets of every shape: all non-establishment, mixed, all establishment.
    let non_establishment: Vec<AuthorityBody> = (0u8..4)
        .map(|i| {
            authorize_event(
                &signer,
                alpha.subject,
                1,
                alpha.genesis(),
                device(70 + i),
                caps(),
                None,
            )
            .body
        })
        .collect();
    let establishment: Vec<AuthorityBody> = vec![rotation.body.clone(), {
        let beta = subject(62, 4);
        beta.root
            .establish(1, alpha.subject, 1, alpha.genesis())
            .expect("rotate")
            .body
    }];

    for take_non in 0..=non_establishment.len() {
        for take_est in 0..=establishment.len() {
            let candidates: BTreeSet<AuthorityBody> = non_establishment[..take_non]
                .iter()
                .chain(establishment[..take_est].iter())
                .cloned()
                .collect();
            let survivors = supersede(candidates.clone());
            let resolution = resolve(candidates);

            match survivors.len() {
                0 => assert_eq!(resolution, Resolution::Exhausted),
                1 => assert!(matches!(resolution, Resolution::Advance(_))),
                _ => match resolution {
                    Resolution::Halt(set) => assert_eq!(set, survivors),
                    other => panic!("|supersede(C)| >= 2 must halt, got {other:?}"),
                },
            }
        }
    }
}

/// `supersede` selects strictly by authority class, and never by content.
#[test]
fn supersede_selects_only_by_authority_class() {
    let alpha = subject(63, 4);
    let signer = alpha.root.authority_signing_key(0);
    let ordinary = authorize_event(
        &signer,
        alpha.subject,
        1,
        alpha.genesis(),
        device(80),
        caps_with_recover(),
        None,
    )
    .body;
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate")
        .body;

    // A `Recover` *capability* does not make an event an establishment event (F13/F13a guard).
    assert!(!ordinary.is_establishment());
    assert!(rotation.is_establishment());

    let mixed: BTreeSet<AuthorityBody> = [ordinary.clone(), rotation.clone()].into_iter().collect();
    let survivors = supersede(mixed);
    assert_eq!(survivors.len(), 1);
    assert!(survivors.contains(&rotation));

    // With no establishment present, everything survives — supersede never breaks ties.
    let others: BTreeSet<AuthorityBody> = [
        ordinary.clone(),
        authorize_event(
            &signer,
            alpha.subject,
            1,
            alpha.genesis(),
            device(81),
            caps(),
            None,
        )
        .body,
    ]
    .into_iter()
    .collect();
    assert_eq!(supersede(others.clone()), others);
}

/// A `Recover` capability grants nothing at the authority layer.
#[test]
fn recover_capability_cannot_mint_establishment_authority() {
    let alpha = subject(64, 4);
    let signer = alpha.root.authority_signing_key(0);
    let holder = SigningKey::from_bytes(&[123u8; 32]);
    let holder_principal = principal(&holder);

    let grant = authorize_event(
        &signer,
        alpha.subject,
        1,
        alpha.genesis(),
        holder_principal,
        caps_with_recover(),
        None,
    );
    let store = store_of(&[alpha.inception.clone(), grant.clone()]);
    let state = match view(&store, alpha.subject) {
        AuthorityView::Live { state, .. } => state,
        other => panic!("expected Live, got {other:?}"),
    };
    assert!(state
        .devices
        .get(&holder_principal)
        .is_some_and(|g| g.capabilities.contains(DeviceCapability::Recover)));

    // The capability holder cannot author anything at all: it is not in the authority set.
    let attempt = authorize_event(
        &holder,
        alpha.subject,
        2,
        grant.body.event_id(),
        device(90),
        caps(),
        None,
    );

    // Nor can it produce an establishment event.
    let forged = AuthorityBody::Recover(EstablishmentBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 2,
            prev_digest: grant.body.event_id(),
            signer: holder_principal,
        },
        revealed_authority: PrincipalSet::single(holder_principal),
        next_commitment: Commitment::from_bytes([1u8; 32]),
    });
    let forged = sign_body(forged, &holder);
    let attempted = store_of(&[alpha.inception.clone(), grant.clone(), attempt, forged]);
    let after_attempts = view(&attempted, alpha.subject);
    assert_eq!(frontier(&after_attempts), Some(2));
    let after_state = after_attempts.state().expect("state");
    assert!(!after_state.devices.contains_key(&device(90)));
    assert_eq!(after_state.authority, state.authority);
}

/// Obligation 14 — a re-signed identical body does not fork and does not halt.
#[test]
fn resigned_body_does_not_fork_the_derived_view() {
    use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
    use icn_identity::authority_log::WitnessSignature;
    use sha2::Sha512;

    let alpha = subject(65, 4);
    let key = alpha.root.authority_signing_key(0);
    let event = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));

    let seed = key.to_bytes();
    let mut expanded = ExpandedSecretKey::from(&seed);
    expanded.hash_prefix = [0x33; 32];
    let second = SignedAuthorityEvent::new(
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
    assert_ne!(event.signature, second.signature);

    let store = store_of(&[alpha.inception.clone(), event.clone(), second]);
    assert_eq!(
        store.bodies_for(alpha.subject).len(),
        2,
        "inception + one body"
    );
    assert_eq!(store.witnesses_for(event.body.event_id()).len(), 2);

    let verdict = view(&store, alpha.subject);
    assert!(!verdict.is_halted(), "a re-signing is not duplicity");
    assert_eq!(frontier(&verdict), Some(2));
}

/// Obligation 15 — establishment bodies are deterministic.
#[test]
fn establishment_construction_is_deterministic() {
    let alpha = subject(66, 4);
    let first = alpha
        .root
        .establish(1, alpha.subject, 3, alpha.genesis())
        .expect("rotate");
    let second = alpha
        .root
        .establish(1, alpha.subject, 3, alpha.genesis())
        .expect("rotate");

    assert_eq!(first.body.canonical_bytes(), second.body.canonical_bytes());
    assert_eq!(first.body.event_id(), second.body.event_id());
    assert_eq!(first.signature, second.signature);

    // A freshly rebuilt root with the same secret and nonce reproduces the same bytes: an honest
    // crash and restart cannot mint a second authorized body.
    let rebuilt = subject(66, 4);
    let after_restart = rebuilt
        .root
        .establish(1, rebuilt.subject, 3, rebuilt.genesis())
        .expect("rotate");
    assert_eq!(after_restart.body.event_id(), first.body.event_id());

    // The safe entry point refuses when the state's armed commitment is not this root's.
    let stranger_subject = subject(67, 4);
    let state = match view(
        &store_of(std::slice::from_ref(&stranger_subject.inception)),
        stranger_subject.subject,
    ) {
        AuthorityView::Live { state, .. } => state,
        other => panic!("expected Live, got {other:?}"),
    };
    assert!(alpha
        .root
        .establish_from_state(
            &state,
            stranger_subject.subject,
            1,
            stranger_subject.genesis()
        )
        .is_err());

    // There is no entropy parameter on the construction path, and generation bounds are enforced.
    assert!(alpha
        .root
        .establish(0, alpha.subject, 1, alpha.genesis())
        .is_err());
    assert!(alpha
        .root
        .establish(99, alpha.subject, 1, alpha.genesis())
        .is_err());
}

/// Obligation 16 — `σ`, `prev_digest` and dedup all use `event_id = H(canonical_body)`.
#[test]
fn digest_roles_all_use_the_body_digest() {
    let alpha = subject(68, 4);
    let inception_body = &alpha.inception.body;

    // σ == event_id(inception body).
    assert_eq!(
        alpha.subject.as_bytes(),
        inception_body.event_id().as_bytes()
    );
    assert_eq!(inception_body.subject(), alpha.subject);

    // The digest is over the body alone: it does not move when the signature changes.
    let other_signature = {
        use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
        use icn_identity::authority_log::WitnessSignature;
        use sha2::Sha512;
        let key = alpha.root.authority_signing_key(0);
        let seed = key.to_bytes();
        let mut expanded = ExpandedSecretKey::from(&seed);
        expanded.hash_prefix = [0x11; 32];
        WitnessSignature::from_bytes(
            raw_sign::<Sha512>(
                &expanded,
                &inception_body.signature_preimage(),
                &key.verifying_key(),
            )
            .to_bytes(),
        )
    };
    let resigned = SignedAuthorityEvent::new(inception_body.clone(), other_signature);
    assert_ne!(resigned.signature, alpha.inception.signature);
    assert_eq!(
        resigned.body.subject(),
        alpha.subject,
        "a re-signed inception must NOT mint a second SubjectId"
    );

    // A child naming a digest of the whole *signed event* cannot extend the chain.
    let whole_event_digest = {
        let mut preimage = inception_body.canonical_bytes();
        preimage.extend_from_slice(alpha.inception.signature.as_bytes());
        EventId::from_bytes(<[u8; 32]>::from(<sha2::Sha256 as sha2::Digest>::digest(
            &preimage,
        )))
    };
    assert_ne!(whole_event_digest, inception_body.event_id());

    let wedged = authorize_at(&alpha, 0, 1, whole_event_digest, device(10));
    let store = store_of(&[alpha.inception.clone(), wedged.clone()]);
    assert!(
        store.contains_body(&wedged.body),
        "admissible, just useless"
    );
    assert_eq!(
        frontier(&view(&store, alpha.subject)),
        Some(1),
        "a whole-signed-event digest must fail to extend the canonical chain"
    );

    // Dedup keys on the body digest too: same body, one element.
    let duplicate = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let twice = store_of(&[
        alpha.inception.clone(),
        duplicate.clone(),
        duplicate.clone(),
    ]);
    assert_eq!(twice.bodies_for(alpha.subject).len(), 2);
    assert_eq!(duplicate.body.event_id(), duplicate.body.event_id());
}

/// Obligation 17 — a fork behind the frontier halts there, and superseding recovery **orphans**
/// the suffix. The test asserts the loss (§9.2.2 case 11, O-N9).
#[test]
fn fork_behind_the_frontier_orphans_the_suffix() {
    let alpha = subject(69, 4);
    let mut chain: Vec<SignedAuthorityEvent> = vec![alpha.inception.clone()];
    let mut prev = alpha.genesis();

    // Positions 1..=5, each authorizing a distinct device.
    for position in 1u64..=5 {
        let event = authorize_at(&alpha, 0, position, prev, device(100 + position as u8));
        prev = event.body.event_id();
        chain.push(event);
    }
    // Build a valid 6..=9 continuation, then withhold 6 and 7 so 8 and 9 are genuinely buffered
    // rather than permanently mis-parented.
    let at_six = authorize_at(&alpha, 0, 6, prev, device(106));
    let at_seven = authorize_at(&alpha, 0, 7, at_six.body.event_id(), device(107));
    let at_eight = authorize_at(&alpha, 0, 8, at_seven.body.event_id(), device(120));
    let at_nine = authorize_at(&alpha, 0, 9, at_eight.body.event_id(), device(121));
    chain.push(at_eight.clone());
    chain.push(at_nine.clone());

    let clean = store_of(&chain);
    assert_eq!(
        frontier(&view(&clean, alpha.subject)),
        Some(6),
        "the buffered 8/9 pair sits unused beyond the gap"
    );

    let mut counterfactual = clean.clone();
    assert!(counterfactual.ingest(&at_six).is_ok());
    assert!(counterfactual.ingest(&at_seven).is_ok());
    assert_eq!(
        frontier(&view(&counterfactual, alpha.subject)),
        Some(10),
        "without the later fork, filling the gap must make the buffered suffix reachable"
    );

    // Introduce a fork at position 3.
    let parent_of_three = chain[2].body.event_id();
    let fork = authorize_at(&alpha, 0, 3, parent_of_three, device(130));
    let mut forked = clean.clone();
    assert!(forked.ingest(&fork).is_ok());
    assert_eq!(disputed(&view(&forked, alpha.subject)), Some(3));

    // A superseding establishment at the disputed position recovers the subject.
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 3, parent_of_three)
        .expect("rotate");
    let mut recovered = forked.clone();
    assert!(recovered.ingest(&rotation).is_ok());

    let verdict = view(&recovered, alpha.subject);
    assert_eq!(
        frontier(&verdict),
        Some(4),
        "recovery advances past the disputed position and stops: the suffix is gone"
    );

    // ASSERT THE LOSS. Devices granted at 3, 4, 5, 8 and 9 are no longer in the derived state.
    let state = verdict.state().expect("state");
    for orphaned in [103u8, 104, 105, 120, 121, 130] {
        assert!(
            !state.devices.contains_key(&device(orphaned)),
            "device {orphaned} must be orphaned from the effective history"
        );
    }
    // Grants from before the fork survive.
    for kept in [101u8, 102] {
        assert!(state.devices.contains_key(&device(kept)));
    }

    // The raw bodies remain durable facts (O-N9: "authored on a superseded branch", not
    // "authorized in the accepted history").
    for orphan in [&chain[3], &chain[4], &at_eight, &at_nine, &fork] {
        assert!(
            recovered.contains_body(&orphan.body),
            "orphaned bodies stay in the durable set"
        );
    }
}

/// Obligation 18 — unilateral self-equivocation. One authorized key, no second party, no
/// Byzantine replica: the subject halts. This preserves O16′ rather than papering over it.
#[test]
fn one_key_can_halt_its_own_subject() {
    let alpha = subject(70, 4);
    let only_key = alpha.root.authority_signing_key(0);

    let first = authorize_event(
        &only_key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(10),
        caps(),
        None,
    );
    let second = authorize_event(
        &only_key,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );

    let store = store_of(&[alpha.inception.clone(), first.clone(), second.clone()]);
    let verdict = view(&store, alpha.subject);
    assert_eq!(
        disputed(&verdict),
        Some(1),
        "a single authorized key halts its own subject with no victim action"
    );

    // If it authors only one, the chain simply advances — a stolen-key action, not a fork.
    let single = store_of(&[alpha.inception.clone(), first]);
    assert_eq!(frontier(&view(&single, alpha.subject)), Some(2));
}

/// Obligation 19 — authority principals are raw keys, never subject identifiers.
#[test]
fn subject_id_is_rejected_as_an_authority_principal() {
    use icn_identity::authority_log::{admissible_bytes, CodecError, PROTOCOL_VERSION};

    let alpha = subject(71, 4);
    let event = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let canonical = event.body.canonical_bytes();

    // Locate the device principal field and retag it as something other than an Ed25519 key —
    // the wire-level shape a "subject named as a principal" would have. Decoding must reject it.
    let device_tag_offset = canonical
        .windows(33)
        .position(|w| w[0] == 0x01 && w[1..] == device(10).as_bytes())
        .expect("device principal must appear in the canonical body");
    let mut retagged = canonical.clone();
    retagged[device_tag_offset] = 0x02;
    assert!(matches!(
        AuthorityBody::decode(&retagged),
        Err(CodecError::BadPrincipalTag(0x02))
    ));
    assert!(admissible_bytes(&retagged, &event.signature).is_err());

    // The subject's own 32 bytes placed in the principal slot: rejected either because they are
    // not a valid Ed25519 point, or — if they happen to be one — because the resulting principal
    // is simply an unrelated key with no authority. Either way `derive` never consults another
    // subject's log.
    let mut substituted = canonical.clone();
    substituted[device_tag_offset + 1..device_tag_offset + 33]
        .copy_from_slice(alpha.subject.as_bytes());
    match AuthorityBody::decode(&substituted) {
        Err(CodecError::InvalidPublicKey) => {}
        Err(other) => panic!("unexpected rejection: {other:?}"),
        Ok(decoded) => {
            // If the subject bytes happen to be a valid point, the body is a *different* body,
            // its signature no longer verifies, and the named principal holds no authority.
            assert!(admissible_bytes(&substituted, &event.signature).is_err());
            let state = match view(
                &store_of(std::slice::from_ref(&alpha.inception)),
                alpha.subject,
            ) {
                AuthorityView::Live { state, .. } => state,
                other => panic!("expected Live, got {other:?}"),
            };
            let named = match &decoded {
                AuthorityBody::Authorize(b) => b.device,
                other => panic!("expected Authorize, got {other:?}"),
            };
            assert!(!state.authority.contains(&named));
        }
    }

    // There is no API path either: `PrincipalKey` is constructible only from a verifying key, a
    // `Did` that decodes to one, or 32 bytes that validate as one. A DID minted from subject
    // bytes fails unless those bytes are coincidentally a valid key, and even then it is an
    // unrelated principal, never the subject.
    // `[0x02; 32]` is not a valid compressed Edwards point. N1 also rejects mathematically valid
    // points presented with non-canonical encodings; fixed aliases are covered by the encoding
    // suite.
    assert!(PrincipalKey::try_from_bytes([0x02u8; 32]).is_err());
    let _ = PROTOCOL_VERSION;
}

/// The establishment class and the authorization rule are kept in step.
///
/// An event is an establishment event **iff** candidate authorization requires the pre-rotation
/// reveal. The public derived path must therefore accept an armed reveal signed by the future key
/// and ordinary events signed by the current key, while rejecting those rules in reverse.
#[test]
fn establishment_class_matches_authorization_rule() {
    let alpha = subject(72, 4);
    let current = alpha.root.authority_signing_key(0);
    let future = alpha.root.authority_signing_key(1);
    let rotation = alpha
        .root
        .establish(1, alpha.subject, 1, alpha.genesis())
        .expect("rotate");
    let authorize = authorize_event(
        &current,
        alpha.subject,
        1,
        alpha.genesis(),
        device(10),
        caps(),
        None,
    );
    assert!(rotation.body.is_establishment());
    assert!(!authorize.body.is_establishment());
    assert_eq!(
        frontier(&view(
            &store_of(&[alpha.inception.clone(), rotation]),
            alpha.subject
        )),
        Some(2),
        "an armed reveal is authorized even though its signer is not the current key"
    );
    assert_eq!(
        frontier(&view(
            &store_of(&[alpha.inception.clone(), authorize]),
            alpha.subject
        )),
        Some(2),
        "an ordinary event is authorized by the current authority set"
    );

    let unarmed_establishment = AuthorityBody::Rotate(EstablishmentBody {
        header: EventHeader {
            subject: alpha.subject,
            position: 1,
            prev_digest: alpha.genesis(),
            signer: principal(&current),
        },
        revealed_authority: PrincipalSet::single(principal(&current)),
        next_commitment: Commitment::from_bytes([0xcd; 32]),
    });
    let ordinary_from_future = authorize_event(
        &future,
        alpha.subject,
        1,
        alpha.genesis(),
        device(11),
        caps(),
        None,
    );
    for rejected in [
        sign_body(unarmed_establishment, &current),
        ordinary_from_future,
    ] {
        assert_eq!(
            frontier(&view(
                &store_of(&[alpha.inception.clone(), rejected]),
                alpha.subject
            )),
            Some(1),
            "establishment and ordinary authorization rules must not collapse"
        );
    }
}

/// `recover` clears device grants; `rotate` preserves them. Both replace the authority set.
#[test]
fn rotate_preserves_devices_and_recover_clears_them() {
    for (plan, expect_devices) in [
        (vec![EstablishmentKind::Rotate], true),
        (vec![EstablishmentKind::Recover], false),
    ] {
        let s = subject_with_plan(80, plan);
        let grant = authorize_at(&s, 0, 1, s.genesis(), device(10));
        let establishment = s
            .root
            .establish(1, s.subject, 2, grant.body.event_id())
            .expect("establish");

        let store = store_of(&[s.inception.clone(), grant, establishment]);
        let state = match view(&store, s.subject) {
            AuthorityView::Live { state, .. } => state,
            other => panic!("expected Live, got {other:?}"),
        };
        assert_eq!(state.generation, 1);
        assert_eq!(
            state.devices.contains_key(&device(10)),
            expect_devices,
            "rotate preserves device grants; recover replaces rather than extends"
        );
    }
}

/// Validity spans are position-denominated and are applied purely.
#[test]
fn validity_spans_are_position_denominated() {
    let alpha = subject(81, 4);
    let span = ValiditySpan::new(2, 4).expect("valid span");
    let grant = authorize_span_at(&alpha, 0, 1, alpha.genesis(), device(10), span);

    let store = store_of(&[alpha.inception.clone(), grant]);
    let state = match view(&store, alpha.subject) {
        AuthorityView::Live { state, .. } => state,
        other => panic!("expected Live, got {other:?}"),
    };

    assert!(!state.device_in_force_at(&device(10), 1));
    assert!(state.device_in_force_at(&device(10), 2));
    assert!(state.device_in_force_at(&device(10), 4));
    assert!(!state.device_in_force_at(&device(10), 5));

    assert!(
        ValiditySpan::new(5, 4).is_err(),
        "inverted spans are rejected"
    );
}

/// A device grant is never effective before the authority event that created it, even when the
/// optional validity span is absent or begins earlier than the grant position.
#[test]
fn grants_are_not_retroactive() {
    for explicit_span in [false, true] {
        let alpha = subject(85 + u8::from(explicit_span), 4);
        let first = authorize_at(&alpha, 0, 1, alpha.genesis(), device(20));
        let target = if explicit_span {
            authorize_span_at(
                &alpha,
                0,
                2,
                first.body.event_id(),
                device(10),
                ValiditySpan::new(0, 4).expect("span"),
            )
        } else {
            authorize_at(&alpha, 0, 2, first.body.event_id(), device(10))
        };
        let store = store_of(&[alpha.inception.clone(), first, target]);
        let state = view(&store, alpha.subject)
            .state()
            .expect("derived state")
            .clone();

        assert!(!state.device_in_force_at(&device(10), 0));
        assert!(!state.device_in_force_at(&device(10), 1));
        assert!(state.device_in_force_at(&device(10), 2));
    }
}

/// The public derivation boundary accepts only an [`AuthorityStore`], so caller-assembled unsigned
/// bodies cannot enter authority state while the pure fold remains separate from admission.
#[test]
fn unsigned_body_cannot_reach_public_derivation() {
    let alpha = subject(82, 4);
    let forged_body = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10)).body;
    let forged = SignedAuthorityEvent::new(forged_body, WitnessSignature::from_bytes([0u8; 64]));
    let mut store = store_of(std::slice::from_ref(&alpha.inception));

    assert!(store.ingest(&forged).is_err());
    let verdict = view(&store, alpha.subject);
    assert_eq!(frontier(&verdict), Some(1));
    assert!(verdict.state().expect("inception state").devices.is_empty());
}

/// A body belonging to another subject never influences this subject's view.
#[test]
fn cross_subject_bodies_are_ignored() {
    let alpha: Subject = subject(83, 4);
    let beta: Subject = subject(84, 4);
    let a1 = authorize_at(&alpha, 0, 1, alpha.genesis(), device(10));
    let b1 = authorize_at(&beta, 0, 1, beta.genesis(), device(11));

    let mixed = store_of(&[
        alpha.inception.clone(),
        beta.inception.clone(),
        a1.clone(),
        b1,
    ]);
    let isolated = store_of(&[alpha.inception.clone(), a1]);

    assert_eq!(view(&mixed, alpha.subject), view(&isolated, alpha.subject));
}
