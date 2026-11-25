//! P5a: the pure predecessor-seal lifecycle model (S19; EXPERIMENTAL, unratified).
//!
//! Golden values come from `tests/reference/predecessor_final_set_reference.py` (independent,
//! standard-library Python). These tests exercise a PURE validator: nothing here is durable.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_authz::effect_v0::ContextRef;
use icn_governance::cutover::{genesis_roster_commitment, CutoverCommitmentV1};
use icn_governance::domain::GovernanceDomainId;
use icn_governance::predecessor_seal::{
    classify, evaluate_drain, final_set_commitment, validate_transition, CutoverTransition,
    DrainStatus, PredecessorLifecycle, SealRefusal, TransitionOutcome, MAX_FINAL_SET,
    MAX_PROPOSAL_ID_LEN, PREDECESSOR_FINAL_SET_DOMAIN,
};
use icn_governance::proposal::{ProposalPayload, ProposalState};
use icn_governance::ProposalOutcome;
use icn_identity::authority_log::SubjectId;

const L: &str = "legacy.coop.example";
const GOLDEN_F_ONE: &str = "6549e29e1553320a7d3f52b6ce1662909ea816e84e93c3412b2bdb9e6de44948";
const GOLDEN_F_THREE: &str = "ad89fc5e757ac1c9e4cccbcdf98c921b4190c5c378378a9ed2d49aad2ddbfb17";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn l() -> GovernanceDomainId {
    GovernanceDomainId(L.into())
}

fn commitment(successor: &str, head: u8) -> CutoverCommitmentV1 {
    let c = ContextRef([0xC1; 32]);
    let standing = vec![(SubjectId::from_bytes([1; 32]), "member".to_string())];
    CutoverCommitmentV1::new(
        l(),
        GovernanceDomainId(successor.into()),
        c,
        [head; 32],
        genesis_roster_commitment(c, &standing).unwrap(),
        [0x4D; 32],
    )
    .unwrap()
}

fn k(c: &CutoverCommitmentV1) -> [u8; 32] {
    c.commitment().unwrap()
}

fn legacy(last: u64, aborted: Option<[u8; 32]>) -> PredecessorLifecycle {
    PredecessorLifecycle::Legacy {
        last_attempt: last,
        aborted,
    }
}

fn pending(attempt: u64, c: &CutoverCommitmentV1) -> PredecessorLifecycle {
    PredecessorLifecycle::CutoverPending {
        attempt,
        commitment: c.clone(),
    }
}

fn begin(attempt: u64, c: &CutoverCommitmentV1) -> CutoverTransition {
    CutoverTransition::Begin {
        attempt,
        commitment: c.clone(),
    }
}

fn abort(attempt: u64, c: &CutoverCommitmentV1) -> CutoverTransition {
    CutoverTransition::Abort {
        attempt,
        commitment_hash: k(c),
    }
}

fn seal(attempt: u64, c: &CutoverCommitmentV1, f: [u8; 32]) -> CutoverTransition {
    CutoverTransition::Seal {
        attempt,
        commitment_hash: k(c),
        final_set: f,
    }
}

fn next(state: &PredecessorLifecycle, t: &CutoverTransition) -> PredecessorLifecycle {
    match validate_transition(&l(), state, t).unwrap() {
        TransitionOutcome::Validated(next) => next,
        TransitionOutcome::Replayed => panic!("expected a new state, got a replay"),
    }
}

fn refused(state: &PredecessorLifecycle, t: &CutoverTransition) -> SealRefusal {
    validate_transition(&l(), state, t).unwrap_err()
}

#[test]
fn begin_abort_retry_and_seal_follow_the_attempt_counter() {
    let first = commitment("successor.coop.example", 0xB1);
    let second = commitment("successor.coop.example", 0xB2);
    let s0 = PredecessorLifecycle::initial();
    assert_eq!(s0, legacy(0, None));
    assert_eq!(s0.attempt(), 0);

    let s1 = next(&s0, &begin(1, &first));
    assert_eq!(s1, pending(1, &first));
    let s2 = next(&s1, &abort(1, &first));
    assert_eq!(s2, legacy(1, Some(k(&first))));
    // A retry needs a strictly greater attempt; the aborted one is never reused.
    assert_eq!(
        refused(&s2, &begin(1, &second)),
        SealRefusal::StaleAttempt {
            attempt: 1,
            latest: 1
        }
    );
    let s3 = next(&s2, &begin(2, &second));
    assert_eq!(s3, pending(2, &second));
    let f = [0xF0; 32];
    let s4 = next(&s3, &seal(2, &second, f));
    assert_eq!(
        s4,
        PredecessorLifecycle::Sealed {
            attempt: 2,
            commitment: second.clone(),
            final_set: f
        }
    );
    assert_eq!(s4.attempt(), 2);
}

#[test]
fn exact_replay_is_idempotent_and_conflicting_replay_is_refused() {
    let c = commitment("successor.coop.example", 0xB1);
    let other = commitment("successor.coop.example", 0xB9);
    let f = [0xF0; 32];
    let pend = pending(1, &c);
    assert_eq!(
        validate_transition(&l(), &pend, &begin(1, &c)),
        Ok(TransitionOutcome::Replayed)
    );
    assert_eq!(
        refused(&pend, &begin(1, &other)),
        SealRefusal::AlreadyPending
    );
    assert_eq!(
        refused(&pend, &begin(2, &other)),
        SealRefusal::AlreadyPending
    );

    let aborted = legacy(1, Some(k(&c)));
    assert_eq!(
        validate_transition(&l(), &aborted, &abort(1, &c)),
        Ok(TransitionOutcome::Replayed)
    );
    assert_eq!(
        refused(&aborted, &abort(1, &other)),
        SealRefusal::StaleAttempt {
            attempt: 1,
            latest: 1
        }
    );

    let sealed = next(&pend, &seal(1, &c, f));
    assert_eq!(
        validate_transition(&l(), &sealed, &seal(1, &c, f)),
        Ok(TransitionOutcome::Replayed)
    );
    assert_eq!(
        refused(&sealed, &seal(1, &c, [0xF1; 32])),
        SealRefusal::SealedIsFinal
    );
    assert_eq!(
        refused(&sealed, &seal(1, &other, f)),
        SealRefusal::SealedIsFinal
    );
}

#[test]
fn transition_table_refuses_every_invalid_pair() {
    let c = commitment("successor.coop.example", 0xB1);
    let other = commitment("successor.coop.example", 0xB9);
    let f = [0xF0; 32];
    let sealed = PredecessorLifecycle::Sealed {
        attempt: 3,
        commitment: c.clone(),
        final_set: f,
    };
    let cases: Vec<(PredecessorLifecycle, CutoverTransition, SealRefusal)> = vec![
        // Legacy
        (
            legacy(0, None),
            begin(0, &c),
            SealRefusal::StaleAttempt {
                attempt: 0,
                latest: 0,
            },
        ),
        (
            legacy(0, None),
            begin(2, &c),
            SealRefusal::AttemptGap {
                attempt: 2,
                expected: 1,
            },
        ),
        (
            legacy(2, Some(k(&c))),
            begin(2, &c),
            SealRefusal::StaleAttempt {
                attempt: 2,
                latest: 2,
            },
        ),
        (
            legacy(2, Some(k(&c))),
            begin(4, &c),
            SealRefusal::AttemptGap {
                attempt: 4,
                expected: 3,
            },
        ),
        (legacy(0, None), abort(1, &c), SealRefusal::NotPending),
        (
            legacy(2, Some(k(&c))),
            abort(1, &c),
            SealRefusal::StaleAttempt {
                attempt: 1,
                latest: 2,
            },
        ),
        (
            legacy(2, Some(k(&c))),
            abort(3, &c),
            SealRefusal::NotPending,
        ),
        (
            legacy(1, Some(k(&c))),
            seal(1, &c, f),
            SealRefusal::NotPending,
        ),
        (legacy(0, None), seal(1, &c, f), SealRefusal::NotPending),
        // CutoverPending
        (
            pending(2, &c),
            begin(1, &c),
            SealRefusal::StaleAttempt {
                attempt: 1,
                latest: 2,
            },
        ),
        (pending(2, &c), begin(3, &c), SealRefusal::AlreadyPending),
        (
            pending(2, &c),
            abort(1, &c),
            SealRefusal::StaleAttempt {
                attempt: 1,
                latest: 2,
            },
        ),
        (pending(2, &c), abort(3, &c), SealRefusal::MismatchedAttempt),
        (
            pending(2, &c),
            abort(2, &other),
            SealRefusal::MismatchedAttempt,
        ),
        (
            pending(2, &c),
            seal(1, &c, f),
            SealRefusal::StaleAttempt {
                attempt: 1,
                latest: 2,
            },
        ),
        (
            pending(2, &c),
            seal(3, &c, f),
            SealRefusal::MismatchedAttempt,
        ),
        (
            pending(2, &c),
            seal(2, &other, f),
            SealRefusal::MismatchedAttempt,
        ),
        // Sealed: no exit
        (sealed.clone(), begin(4, &other), SealRefusal::SealedIsFinal),
        (sealed.clone(), begin(3, &c), SealRefusal::SealedIsFinal),
        (sealed.clone(), abort(3, &c), SealRefusal::SealedIsFinal),
        (sealed.clone(), seal(4, &c, f), SealRefusal::SealedIsFinal),
    ];
    for (state, transition, why) in cases {
        assert_eq!(
            refused(&state, &transition),
            why,
            "{state:?} / {transition:?}"
        );
    }
}

#[test]
fn an_old_message_never_affects_a_later_attempt() {
    let first = commitment("successor.coop.example", 0xB1);
    let second = commitment("successor.coop.example", 0xB2);
    let retry = pending(2, &second);
    // The first attempt's abort and seal cannot touch the second attempt.
    assert_eq!(
        refused(&retry, &abort(1, &first)),
        SealRefusal::StaleAttempt {
            attempt: 1,
            latest: 2
        }
    );
    assert_eq!(
        refused(&retry, &seal(1, &first, [0xF0; 32])),
        SealRefusal::StaleAttempt {
            attempt: 1,
            latest: 2
        }
    );
    // The second attempt's own number with the first commitment is a mismatch.
    assert_eq!(
        refused(&retry, &abort(2, &first)),
        SealRefusal::MismatchedAttempt
    );
    assert_eq!(
        refused(&retry, &begin(1, &first)),
        SealRefusal::StaleAttempt {
            attempt: 1,
            latest: 2
        }
    );
}

#[test]
fn the_attempt_counter_overflow_fails_closed() {
    let c = commitment("successor.coop.example", 0xB1);
    let exhausted = legacy(u64::MAX, Some(k(&c)));
    assert_eq!(
        refused(&exhausted, &begin(u64::MAX, &c)),
        SealRefusal::AttemptOverflow
    );
    // The last representable attempt can still begin, abort and seal.
    let near = legacy(u64::MAX - 1, Some(k(&c)));
    let last = next(&near, &begin(u64::MAX, &c));
    assert_eq!(next(&last, &abort(u64::MAX, &c)), exhausted);
}

#[test]
fn foreign_or_malformed_commitments_and_states_are_refused() {
    let foreign = CutoverCommitmentV1::new(
        GovernanceDomainId("another.coop.example".into()),
        GovernanceDomainId("successor.coop.example".into()),
        ContextRef([0xC1; 32]),
        [0xB1; 32],
        genesis_roster_commitment(
            ContextRef([0xC1; 32]),
            &[(SubjectId::from_bytes([1; 32]), "member".to_string())],
        )
        .unwrap(),
        [0x4D; 32],
    )
    .unwrap();
    assert_eq!(
        refused(&legacy(0, None), &begin(1, &foreign)),
        SealRefusal::ForeignPredecessor
    );
    // A stored pending state for another predecessor is refused before any transition.
    assert_eq!(
        refused(&pending(1, &foreign), &abort(1, &foreign)),
        SealRefusal::ForeignPredecessor
    );
    let c = commitment("successor.coop.example", 0xB1);
    for malformed in [
        legacy(0, Some(k(&c))),
        legacy(3, None),
        pending(0, &c),
        PredecessorLifecycle::Sealed {
            attempt: 0,
            commitment: c.clone(),
            final_set: [0; 32],
        },
    ] {
        assert_eq!(
            refused(&malformed, &begin(1, &c)),
            SealRefusal::MalformedState,
            "{malformed:?}"
        );
    }
}

#[test]
fn final_set_golden_vectors_match_the_independent_reference() {
    let k = [0x4B; 32];
    assert_eq!(
        hex(&final_set_commitment(&l(), 1, &k, &["cutover-proposal"]).unwrap()),
        GOLDEN_F_ONE
    );
    assert_eq!(
        hex(&final_set_commitment(&l(), 2, &k, &["0f8c-a", "7b1e-b", "ñ-proposal"]).unwrap()),
        GOLDEN_F_THREE
    );
}

#[test]
fn final_set_refuses_malformed_input_and_never_sorts() {
    let k = [0x4B; 32];
    let refuse = |domain: &GovernanceDomainId, attempt: u64, ids: &[&str]| {
        matches!(
            final_set_commitment(domain, attempt, &k, ids),
            Err(SealRefusal::FinalSet(_))
        )
    };
    assert!(refuse(&l(), 1, &[]));
    assert!(refuse(&l(), 1, &["b", "a"]));
    assert!(refuse(&l(), 1, &["a", "a"]));
    assert!(refuse(&l(), 1, &["a", ""]));
    let long = "x".repeat(MAX_PROPOSAL_ID_LEN + 1);
    assert!(refuse(&l(), 1, &[long.as_str()]));
    let at_limit = "x".repeat(MAX_PROPOSAL_ID_LEN);
    assert!(!refuse(&l(), 1, &[at_limit.as_str()]));
    assert!(refuse(&l(), 0, &["a"]));
    assert!(refuse(&GovernanceDomainId(String::new()), 1, &["a"]));
    // L is capped at the shared 1024-byte bound, measured in UTF-8 bytes (D-S20-11).
    let cap = icn_governance::cutover::MAX_CUTOVER_DOMAIN_ID_BYTES;
    assert!(!refuse(&GovernanceDomainId("l".repeat(cap)), 1, &["a"]));
    assert!(refuse(&GovernanceDomainId("l".repeat(cap + 1)), 1, &["a"]));
    assert!(refuse(
        &GovernanceDomainId("ñ".repeat(cap / 2 + 1)),
        1,
        &["a"]
    ));
}

#[test]
fn final_set_size_bound_is_enforced() {
    let ids: Vec<String> = (0..=MAX_FINAL_SET).map(|i| format!("{i:08x}")).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert!(final_set_commitment(&l(), 1, &[0; 32], &refs[..MAX_FINAL_SET]).is_ok());
    assert_eq!(
        final_set_commitment(&l(), 1, &[0; 32], &refs),
        Err(SealRefusal::FinalSet("too many entries"))
    );
}

#[test]
fn final_set_is_bound_to_domain_attempt_cutover_and_exact_set() {
    let k = [0x4B; 32];
    let base = final_set_commitment(&l(), 1, &k, &["a", "b"]).unwrap();
    let variants = [
        final_set_commitment(
            &GovernanceDomainId("legacy.coop.examplf".into()),
            1,
            &k,
            &["a", "b"],
        ),
        final_set_commitment(&l(), 2, &k, &["a", "b"]),
        final_set_commitment(&l(), 1, &[0x4C; 32], &["a", "b"]),
        final_set_commitment(&l(), 1, &k, &["a"]),
        final_set_commitment(&l(), 1, &k, &["a", "c"]),
        final_set_commitment(&l(), 1, &k, &["ab"]),
        final_set_commitment(&l(), 1, &k, &["a", "b", "c"]),
    ];
    for (i, v) in variants.iter().enumerate() {
        assert_ne!(*v.as_ref().unwrap(), base, "variant {i}");
    }
    assert!(!PREDECESSOR_FINAL_SET_DOMAIN.starts_with(b"icn.governance.cutover"));
}

#[test]
fn classification_uses_the_real_proposal_states() {
    use DrainStatus::*;
    // ExecutionRequired (every payload but Text) and Declarative (Text), per the real payload.
    let req = ProposalPayload::VetoProposal {
        target_proposal_id: "t".into(),
        reason: "r".into(),
    };
    let decl = ProposalPayload::Text { body: "b".into() };
    let t = 1_000;
    assert_eq!(
        classify(&ProposalState::Draft, &req, false, true),
        InactiveDraft
    );
    for active in [
        ProposalState::Deliberation {
            started_at: t,
            ends_at: t,
        },
        ProposalState::Open {
            opened_at: t,
            closes_at: t,
        },
    ] {
        assert_eq!(classify(&active, &decl, true, false), Active);
    }
    let accepted = ProposalState::Accepted { closed_at: t };
    assert_eq!(classify(&accepted, &decl, false, false), Terminal);
    assert_eq!(classify(&accepted, &req, true, false), Terminal);
    assert_eq!(
        classify(&accepted, &req, false, false),
        TerminalEffectsPending
    );
    assert_eq!(
        classify(&accepted, &req, false, true),
        TerminalEffectsPending
    );
    assert_eq!(
        classify(&accepted, &req, true, true),
        TerminalChangesCutoverBasis
    );
    assert_eq!(
        classify(&accepted, &decl, true, true),
        TerminalChangesCutoverBasis
    );
    let forced = |outcome| ProposalState::ForceClosed {
        closed_at: t,
        outcome,
        reason: "r".into(),
    };
    assert_eq!(
        classify(&forced(ProposalOutcome::Accepted), &req, false, false),
        TerminalEffectsPending
    );
    assert_eq!(
        classify(&forced(ProposalOutcome::Rejected), &req, false, true),
        Terminal
    );
    for closed in [
        ProposalState::Rejected { closed_at: t },
        ProposalState::NoQuorum { closed_at: t },
        ProposalState::Cancelled { cancelled_at: t },
        ProposalState::Vetoed {
            vetoed_at: t,
            reason: "r".into(),
        },
    ] {
        // Not accepted: no effect to close and no basis change, whatever the caller says.
        assert_eq!(classify(&closed, &req, false, true), Terminal);
    }
}

#[test]
fn drain_seals_over_terminal_ids_and_refuses_unfinished_work() {
    use DrainStatus::*;
    let c = commitment("successor.coop.example", 0xB1);
    let state = pending(1, &c);
    let ok = evaluate_drain(
        &l(),
        &state,
        &[("a", Terminal), ("b", InactiveDraft), ("c", Terminal)],
    )
    .unwrap();
    let expected_f = final_set_commitment(&l(), 1, &k(&c), &["a", "c"]).unwrap();
    assert_eq!(ok, seal(1, &c, expected_f));
    // The produced transition is valid against the same state.
    assert!(matches!(
        validate_transition(&l(), &state, &ok),
        Ok(TransitionOutcome::Validated(
            PredecessorLifecycle::Sealed { .. }
        ))
    ));
    for (evidence, why) in [
        (
            vec![("a", Terminal), ("b", Active)],
            SealRefusal::ActiveProposal("b".into()),
        ),
        (
            vec![("a", TerminalEffectsPending)],
            SealRefusal::UnresolvedEffects("a".into()),
        ),
        (
            vec![("a", TerminalChangesCutoverBasis)],
            SealRefusal::ChangesCutoverBasis("a".into()),
        ),
    ] {
        assert_eq!(evaluate_drain(&l(), &state, &evidence), Err(why));
    }
    // Only drafts (or nothing): no terminal proposal, so no final set.
    assert!(matches!(
        evaluate_drain(&l(), &state, &[("a", InactiveDraft)]),
        Err(SealRefusal::FinalSet(_))
    ));
    assert!(matches!(
        evaluate_drain(&l(), &state, &[]),
        Err(SealRefusal::FinalSet(_))
    ));
    // Unsorted or duplicate evidence is refused, never sorted.
    for bad in [
        vec![("b", Terminal), ("a", Terminal)],
        vec![("a", Terminal), ("a", Terminal)],
    ] {
        assert!(matches!(
            evaluate_drain(&l(), &state, &bad),
            Err(SealRefusal::FinalSet(_))
        ));
    }
    // Drain is meaningless outside a pending attempt.
    assert_eq!(
        evaluate_drain(&l(), &legacy(0, None), &[("a", Terminal)]),
        Err(SealRefusal::NotPending)
    );
    let sealed = next(&state, &ok);
    assert_eq!(
        evaluate_drain(&l(), &sealed, &[("a", Terminal)]),
        Err(SealRefusal::SealedIsFinal)
    );
}

#[test]
fn a_final_set_from_another_domain_or_attempt_does_not_seal_this_one() {
    let c = commitment("successor.coop.example", 0xB1);
    let state = pending(2, &c);
    // F computed for attempt 1, or for another predecessor, is a different value; the validator
    // records whatever F it is given, so the binding lives in F itself (checked by recomputation).
    let f_other_attempt = final_set_commitment(&l(), 1, &k(&c), &["a"]).unwrap();
    let f_this = final_set_commitment(&l(), 2, &k(&c), &["a"]).unwrap();
    assert_ne!(f_other_attempt, f_this);
    let produced = evaluate_drain(&l(), &state, &[("a", DrainStatus::Terminal)]).unwrap();
    assert_eq!(produced, seal(2, &c, f_this));
}

#[test]
fn drain_evidence_order_is_checked_over_every_entry_including_drafts() {
    use DrainStatus::*;
    let c = commitment("successor.coop.example", 0xB1);
    let state = pending(1, &c);
    // The terminal ids alone would be ascending; the full evidence is not.
    for bad in [
        vec![("b", Terminal), ("a", InactiveDraft)],
        vec![("a", Terminal), ("a", InactiveDraft)],
    ] {
        assert!(matches!(
            evaluate_drain(&l(), &state, &bad),
            Err(SealRefusal::FinalSet(_))
        ));
    }
}

#[test]
fn abort_replay_edges_and_sealed_foreign_state() {
    let c = commitment("successor.coop.example", 0xB1);
    let other = commitment("successor.coop.example", 0xB9);
    assert_eq!(
        refused(&legacy(0, None), &abort(0, &c)),
        SealRefusal::StaleAttempt {
            attempt: 0,
            latest: 0
        }
    );
    assert_eq!(
        refused(&legacy(2, Some(k(&c))), &abort(2, &other)),
        SealRefusal::StaleAttempt {
            attempt: 2,
            latest: 2
        }
    );
    let foreign = CutoverCommitmentV1::new(
        GovernanceDomainId("another.coop.example".into()),
        GovernanceDomainId("successor.coop.example".into()),
        c.context,
        c.genesis_head,
        c.roster_commitment,
        c.recognition_map_commitment,
    )
    .unwrap();
    let sealed_foreign = PredecessorLifecycle::Sealed {
        attempt: 1,
        commitment: foreign,
        final_set: [0xF0; 32],
    };
    assert_eq!(
        refused(&sealed_foreign, &abort(1, &c)),
        SealRefusal::ForeignPredecessor
    );
}

/// Pins the documented trust gap: the pure validator records whatever F a hand-built Seal
/// carries. Only `evaluate_drain` computes F; P5b must recompute it under its fence.
#[test]
fn a_hand_built_seal_records_its_f_unchecked() {
    let c = commitment("successor.coop.example", 0xB1);
    let arbitrary = [0u8; 32];
    assert_eq!(
        next(&pending(1, &c), &seal(1, &c, arbitrary)),
        PredecessorLifecycle::Sealed {
            attempt: 1,
            commitment: c.clone(),
            final_set: arbitrary
        }
    );
}
