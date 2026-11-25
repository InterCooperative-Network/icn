//! P5b slice 1a: lifecycle record, witness, keys and basis (EXPERIMENTAL; pure codec).
//!
//! Golden values come from the independent reference
//! `tests/reference/predecessor_lifecycle_reference.py` (standard-library Python; it builds C with
//! the independent P4 reference), stored in `tests/reference/predecessor_lifecycle_vectors.txt`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_authz::effect_v0::ContextRef;
use icn_governance::cutover::{genesis_roster_commitment, CutoverCommitmentV1};
use icn_governance::domain::GovernanceDomainId;
use icn_governance::predecessor_lifecycle::{
    check_witness, domain_record_pin, lifecycle_basis, lifecycle_key, next_attempt, next_revision,
    witness_key, LifecycleCodecError as E, LifecyclePresence, LifecycleRecordV1,
    LifecycleWitnessV1, StoredLifecycle, LIFECYCLE_KEY_PREFIX, MAX_COMMITMENT_LEN,
    WITNESS_KEY_PREFIX,
};
use icn_identity::authority_log::SubjectId;

const VECTORS: &str = include_str!("reference/predecessor_lifecycle_vectors.txt");
const L: &str = "legacy.coop.example";
const L2: &str = "co:op/ñ-legacy";
const S: &str = "successor.coop.example";
const EL: [u8; 32] = [0xE1; 32];
const F: [u8; 32] = [0xF5; 32];
const STORED_DOMAIN: &[u8] = br#"{"id":"legacy.coop.example","name":"Legacy"}"#;

fn vector(name: &str) -> Vec<u8> {
    let line = VECTORS
        .lines()
        .find(|l| l.split(' ').next() == Some(name))
        .unwrap_or_else(|| panic!("vector {name}"));
    hex::decode(line.split(' ').nth(1).unwrap()).unwrap()
}

fn d(s: &str) -> GovernanceDomainId {
    GovernanceDomainId(s.to_string())
}

fn commitment(l: &str) -> CutoverCommitmentV1 {
    let c = ContextRef([0xC1; 32]);
    let standing = vec![
        (SubjectId::from_bytes([1; 32]), "elector".to_string()),
        (SubjectId::from_bytes([1; 32]), "member".to_string()),
        (SubjectId::from_bytes([2; 32]), "socia-ñ".to_string()),
    ];
    CutoverCommitmentV1::new(
        d(l),
        d(S),
        c,
        [0xB1; 32],
        genesis_roster_commitment(c, &standing).unwrap(),
        [0x4D; 32],
    )
    .unwrap()
}

fn k(l: &str) -> [u8; 32] {
    commitment(l).commitment().unwrap()
}

fn basis() -> [u8; 32] {
    lifecycle_basis(&d(L), 1, &EL, &domain_record_pin(STORED_DOMAIN)).unwrap()
}

fn pending(revision: u64, attempt: u64) -> LifecycleRecordV1 {
    LifecycleRecordV1 {
        predecessor: d(L),
        revision,
        state: StoredLifecycle::Pending {
            attempt,
            commitment: commitment(L),
            basis: basis(),
            electorate: EL,
        },
    }
}

fn sealed_l2() -> LifecycleRecordV1 {
    LifecycleRecordV1 {
        predecessor: d(L2),
        revision: u64::MAX,
        state: StoredLifecycle::Sealed {
            attempt: 1,
            commitment: commitment(L2),
            basis: basis(),
            electorate: EL,
            final_set: F,
        },
    }
}

fn legacy_aborted() -> LifecycleRecordV1 {
    LifecycleRecordV1 {
        predecessor: d(L),
        revision: 3,
        state: StoredLifecycle::Legacy {
            last_attempt: 1,
            aborted: Some(k(L)),
        },
    }
}

fn witness() -> LifecycleWitnessV1 {
    LifecycleWitnessV1 {
        predecessor: d(L),
        first_attempt: 1,
        first_commitment_hash: k(L),
    }
}

#[test]
fn golden_vectors_match_the_independent_reference_and_round_trip() {
    assert_eq!(domain_record_pin(STORED_DOMAIN).to_vec(), vector("PIN"));
    assert_eq!(basis().to_vec(), vector("BASIS"));
    assert_eq!(lifecycle_key(&d(L)).unwrap(), vector("KEY"));
    assert_eq!(witness_key(&d(L2)).unwrap(), vector("WITNESS_KEY"));
    for (name, record, l) in [
        ("RECORD_LEGACY_ABORTED", legacy_aborted(), L),
        ("RECORD_PENDING", pending(1, 1), L),
        ("RECORD_PENDING_AFTER_ABORT", pending(4, 2), L),
        ("RECORD_SEALED_MAX_REVISION", sealed_l2(), L2),
    ] {
        let bytes = record.encode().unwrap();
        assert_eq!(bytes, vector(name), "{name}");
        assert_eq!(
            LifecycleRecordV1::decode_for(&d(l), &bytes).unwrap(),
            record,
            "{name}"
        );
    }
    let w = witness().encode().unwrap();
    assert_eq!(w, vector("WITNESS"));
    assert_eq!(
        LifecycleWitnessV1::decode_for(&d(L), &w).unwrap(),
        witness()
    );
}

#[test]
fn every_truncation_of_every_vector_is_refused() {
    for (name, l) in [
        ("RECORD_LEGACY_ABORTED", L),
        ("RECORD_PENDING", L),
        ("RECORD_SEALED_MAX_REVISION", L2),
    ] {
        let bytes = vector(name);
        for cut in 0..bytes.len() {
            assert!(
                LifecycleRecordV1::decode_for(&d(l), &bytes[..cut]).is_err(),
                "{name} prefix {cut}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            LifecycleRecordV1::decode_for(&d(l), &trailing),
            Err(E::TrailingBytes),
            "{name}"
        );
    }
    let w = vector("WITNESS");
    for cut in 0..w.len() {
        assert!(
            LifecycleWitnessV1::decode_for(&d(L), &w[..cut]).is_err(),
            "witness {cut}"
        );
    }
    let mut trailing = w.clone();
    trailing.push(0);
    assert_eq!(
        LifecycleWitnessV1::decode_for(&d(L), &trailing),
        Err(E::TrailingBytes)
    );
}

/// Byte offsets inside `RECORD_PENDING` (L = 19 bytes).
const TAG_AT: usize = 4;
const VERSION_AT: usize = 4 + 39;
const L_AT: usize = VERSION_AT + 2;
const REV_AT: usize = L_AT + 4 + 19;
const STATE_AT: usize = REV_AT + 8;
const BODY_AT: usize = STATE_AT + 1;

fn mutated(f: impl FnOnce(&mut Vec<u8>)) -> Result<LifecycleRecordV1, E> {
    let mut b = vector("RECORD_PENDING");
    f(&mut b);
    LifecycleRecordV1::decode_for(&d(L), &b)
}

#[test]
fn structural_corruption_is_refused_with_its_reason() {
    assert_eq!(mutated(|b| b[TAG_AT] ^= 1), Err(E::WrongTag));
    assert_eq!(
        mutated(|b| b[VERSION_AT + 1] = 2),
        Err(E::UnsupportedVersion)
    );
    assert_eq!(
        mutated(|b| b[REV_AT..REV_AT + 8].copy_from_slice(&[0; 8])),
        Err(E::ZeroRevision)
    );
    assert_eq!(mutated(|b| b[STATE_AT] = 0), Err(E::UnknownState));
    assert_eq!(mutated(|b| b[STATE_AT] = 4), Err(E::UnknownState));
    // Pending attempt 0.
    assert_eq!(
        mutated(|b| b[BODY_AT..BODY_AT + 8].copy_from_slice(&[0; 8])),
        Err(E::ZeroAttempt)
    );
    // A wrong K (first byte after lp(C)).
    let c_len = vector("RECORD_PENDING")[BODY_AT + 8..BODY_AT + 12]
        .try_into()
        .map(u32::from_be_bytes)
        .unwrap() as usize;
    let k_at = BODY_AT + 12 + c_len;
    assert_eq!(mutated(|b| b[k_at] ^= 1), Err(E::CommitmentHashMismatch));
    // A byte inside C: no longer the committed value (C decodes but K disagrees, or C is invalid).
    assert!(mutated(|b| b[BODY_AT + 12 + c_len - 1] ^= 1).is_err());
    // Non-canonical C: one extra byte inside lp(C).
    assert_eq!(
        mutated(|b| {
            let n = (c_len + 1) as u32;
            b[BODY_AT + 8..BODY_AT + 12].copy_from_slice(&n.to_be_bytes());
            b.insert(BODY_AT + 12 + c_len, 0);
        }),
        Err(E::InvalidCommitment)
    );
    // An lp(C) longer than the cap, or than the buffer.
    assert_eq!(
        mutated(|b| b[BODY_AT + 8..BODY_AT + 12]
            .copy_from_slice(&((MAX_COMMITMENT_LEN + 1) as u32).to_be_bytes())),
        Err(E::CommitmentTooLong)
    );
    assert_eq!(
        mutated(|b| b[BODY_AT + 8..BODY_AT + 12]
            .copy_from_slice(&((MAX_COMMITMENT_LEN) as u32).to_be_bytes())),
        Err(E::Truncated)
    );
}

#[test]
fn a_record_or_witness_for_another_domain_is_refused() {
    // Stored under L but naming L2 inside (or vice versa): a transplant.
    assert_eq!(
        LifecycleRecordV1::decode_for(&d("other.coop"), &vector("RECORD_PENDING")),
        Err(E::WrongDomain)
    );
    assert_eq!(
        LifecycleWitnessV1::decode_for(&d("other.coop"), &vector("WITNESS")),
        Err(E::WrongDomain)
    );
    // A record whose cutover commitment names a different predecessor.
    let mut foreign = pending(1, 1);
    if let StoredLifecycle::Pending { commitment, .. } = &mut foreign.state {
        *commitment = self::commitment(L2);
    }
    assert_eq!(foreign.encode(), Err(E::ForeignCommitment));
}

#[test]
fn legacy_rules_and_the_initial_state_are_enforced() {
    let mut never = legacy_aborted();
    never.state = StoredLifecycle::Legacy {
        last_attempt: 0,
        aborted: None,
    };
    assert_eq!(never.encode(), Err(E::InitialStateNotStored));
    let mut unflagged = legacy_aborted();
    unflagged.state = StoredLifecycle::Legacy {
        last_attempt: 1,
        aborted: None,
    };
    assert_eq!(unflagged.encode(), Err(E::AttemptFlagMismatch));
    // Decoder side: flag 2, flag 0 with an attempt, and attempt 0.
    let flag_at = STATE_AT + 1 + 8;
    let mut b = vector("RECORD_LEGACY_ABORTED");
    b[flag_at] = 2;
    assert_eq!(
        LifecycleRecordV1::decode_for(&d(L), &b),
        Err(E::InvalidAbortFlag)
    );
    let mut b = vector("RECORD_LEGACY_ABORTED");
    b[flag_at] = 0;
    b.truncate(flag_at + 1);
    assert_eq!(
        LifecycleRecordV1::decode_for(&d(L), &b),
        Err(E::AttemptFlagMismatch)
    );
    let mut b = vector("RECORD_LEGACY_ABORTED");
    b[STATE_AT + 1..STATE_AT + 9].copy_from_slice(&[0; 8]);
    assert_eq!(
        LifecycleRecordV1::decode_for(&d(L), &b),
        Err(E::InitialStateNotStored)
    );
}

#[test]
fn domain_ids_are_bounded_and_keys_are_disjoint_and_unambiguous() {
    let at = "l".repeat(1024);
    let over = "l".repeat(1025);
    assert!(lifecycle_key(&d(&at)).is_ok());
    assert_eq!(lifecycle_key(&d(&over)), Err(E::InvalidDomain));
    assert_eq!(witness_key(&d("")), Err(E::InvalidDomain));
    assert_eq!(
        lifecycle_basis(&d(&over), 1, &EL, &[0; 32]),
        Err(E::InvalidDomain)
    );
    // Neither prefix is a prefix of the other, and length framing separates "a:b" from "a".
    assert!(!WITNESS_KEY_PREFIX.starts_with(LIFECYCLE_KEY_PREFIX));
    assert!(!LIFECYCLE_KEY_PREFIX.starts_with(WITNESS_KEY_PREFIX));
    assert!(!lifecycle_key(&d("a:b"))
        .unwrap()
        .starts_with(&lifecycle_key(&d("a")).unwrap()));
    // A record's L must be a valid, bounded UTF-8 id in the bytes too.
    let mut b = vector("RECORD_PENDING");
    b[L_AT + 4] = 0xFF;
    assert_eq!(
        LifecycleRecordV1::decode_for(&d(L), &b),
        Err(E::InvalidDomain)
    );
}

#[test]
fn every_basis_input_changes_the_basis_and_the_pin_is_category_separated() {
    let pin = domain_record_pin(STORED_DOMAIN);
    let base = lifecycle_basis(&d(L), 1, &EL, &pin).unwrap();
    assert_ne!(lifecycle_basis(&d(L2), 1, &EL, &pin).unwrap(), base);
    assert_ne!(lifecycle_basis(&d(L), 2, &EL, &pin).unwrap(), base);
    assert_ne!(lifecycle_basis(&d(L), 1, &[0xE2; 32], &pin).unwrap(), base);
    assert_ne!(lifecycle_basis(&d(L), 1, &EL, &[0; 32]).unwrap(), base);
    // Swapping E and the pin changes the basis: the categories are not interchangeable.
    assert_ne!(lifecycle_basis(&d(L), 1, &pin, &EL).unwrap(), base);
    assert_ne!(domain_record_pin(b"x"), domain_record_pin(b"y"));
}

#[test]
fn record_and_witness_must_correspond() {
    let l = d(L);
    let rec = |r: &LifecycleRecordV1| r.encode().unwrap();
    let w = witness().encode().unwrap();
    assert_eq!(
        check_witness(&l, None, None),
        Ok(LifecyclePresence::NeverBegun)
    );
    // Missing either one: fail closed, never Legacy.
    assert_eq!(
        check_witness(&l, Some(&rec(&pending(1, 1))), None),
        Err(E::WitnessMismatch)
    );
    assert_eq!(check_witness(&l, None, Some(&w)), Err(E::WitnessMismatch));
    // Both, corresponding: at the first attempt K must equal K_first.
    for r in [pending(1, 1), legacy_aborted(), pending(4, 2)] {
        assert_eq!(
            check_witness(&l, Some(&rec(&r)), Some(&w)),
            Ok(LifecyclePresence::Established(Box::new(r)))
        );
    }
    // A witness whose K_first is not the first attempt's K.
    let mut wrong_k = witness();
    wrong_k.first_commitment_hash = [9; 32];
    assert_eq!(
        check_witness(
            &l,
            Some(&rec(&pending(1, 1))),
            Some(&wrong_k.encode().unwrap())
        ),
        Err(E::WitnessMismatch)
    );
    // A record older than the witness's first attempt.
    let mut later = witness();
    later.first_attempt = 3;
    assert_eq!(
        check_witness(
            &l,
            Some(&rec(&pending(4, 2))),
            Some(&later.encode().unwrap())
        ),
        Err(E::WitnessMismatch)
    );
    // A witness copied from another domain.
    let mut other = witness();
    other.predecessor = d(L2);
    assert_eq!(
        check_witness(
            &l,
            Some(&rec(&pending(1, 1))),
            Some(&other.encode().unwrap())
        ),
        Err(E::WrongDomain)
    );
    // Witness from the first Pending record.
    assert_eq!(
        LifecycleWitnessV1::for_first_pending(&pending(1, 1)).unwrap(),
        witness()
    );
    assert_eq!(
        LifecycleWitnessV1::for_first_pending(&legacy_aborted()),
        Err(E::WitnessMismatch)
    );
    // Witness structural refusals.
    let mut zero = witness();
    zero.first_attempt = 0;
    assert_eq!(zero.encode(), Err(E::WitnessZeroAttempt));
    let mut b = w.clone();
    let at = 4 + 47 + 2 + 4 + 19;
    b[at..at + 8].copy_from_slice(&[0; 8]);
    assert_eq!(
        LifecycleWitnessV1::decode_for(&l, &b),
        Err(E::WitnessZeroAttempt)
    );
    let mut b = w.clone();
    b[4] ^= 1;
    assert_eq!(LifecycleWitnessV1::decode_for(&l, &b), Err(E::WrongTag));
}

#[test]
fn counters_refuse_overflow_and_a_max_revision_is_frozen() {
    assert_eq!(next_revision(1), Ok(2));
    assert_eq!(next_revision(u64::MAX), Err(E::RevisionOverflow));
    assert_eq!(next_attempt(u64::MAX), Err(E::AttemptOverflow));
    assert_eq!(next_attempt(0), Ok(1));
    // u64::MAX decodes (it is a vector) but cannot advance.
    let r = sealed_l2();
    assert_eq!(next_revision(r.revision), Err(E::RevisionOverflow));
    // Revision 0 is never encoded.
    let mut zero = pending(1, 1);
    zero.revision = 0;
    assert_eq!(zero.encode(), Err(E::ZeroRevision));
}
