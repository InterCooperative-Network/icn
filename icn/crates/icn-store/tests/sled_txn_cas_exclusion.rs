//! ADR-0091 S0a, test D3: does a sled transaction exclude a single-key `compare_and_put` on the
//! same key?
//!
//! **Evidence only.** Uses existing, already-shipped `SledStore` capabilities
//! (`TransactionalStore::transaction`, `Store::compare_and_put`); it does not implement or imply
//! the future object-safe create method.
//!
//! # Property exercised
//!
//! *Mutual exclusion and atomic visibility (class A).* Key `J` is "journal-like" and `R` is
//! "operation-like". Per round, one thread runs a transaction that creates `{J, R}` only if both
//! are absent; another runs `compare_and_put(J, None, v)`, released together through a barrier.
//! The only acceptable final states are:
//!
//! - **A (transaction wins):** `J == TX_J` and `R == TX_R`; the CAS reports `Conflict`.
//! - **B (CAS wins):** `J == CAS_J` and `R` absent; the transaction created nothing.
//!
//! Anything else (both writers succeed, neither succeeds, `J` from one writer with `R` from the
//! other, `R` without `J`) is a violation. A third thread reads `R` then `J` and `J` then `R`
//! throughout the round and must never observe a half-committed transaction.
//!
//! # What this does not establish
//!
//! - It is in-process only. Sled's directory lock excludes a second *process*; this does not test
//!   two processes (there cannot be two).
//! - Racing is scheduling-dependent. The deterministic `sequential_*` tests pin each outcome
//!   without a race; the racing test adds adversarial interleavings on top and reports how often
//!   each winner was reached. Reaching a winner is a statement about the scheduler, not the store.
//! - Throughput / lock cost is not measured.
//! - Nothing about durability or crash recovery (that is D2/D4).

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod s0a_support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Barrier;
use std::time::{Duration, Instant};

use icn_store::{CompareAndPut, SledStore, Store, TransactionalStore};
use s0a_support::{env_usize, strict, ENV_TRIALS};

const TX_J: &[u8] = b"journal-row-written-by-the-transaction";
const TX_R: &[u8] = b"operation-row-written-by-the-transaction";
const CAS_J: &[u8] = b"journal-row-written-by-the-cas";

const DEFAULT_TRIALS: usize = 5;
const ROUNDS_PER_TRIAL: u64 = 200;
/// If a winner has not been reached after the planned rounds, keep racing up to this many rounds
/// per trial before declaring the experiment underpowered.
const MAX_EXTRA_ROUNDS: u64 = 4_000;

fn key_j(trial: usize, round: u64) -> Vec<u8> {
    format!("s0a:d3:journal:{trial:04}:{round:08}").into_bytes()
}

fn key_r(trial: usize, round: u64) -> Vec<u8> {
    format!("s0a:d3:operation:{trial:04}:{round:08}").into_bytes()
}

/// Create `{J, R}` if and only if both are absent. Pure check-then-set, so safe if sled re-runs
/// the closure. Returns whether it created them. A typed outcome has to be an `Ok` value: the
/// store flattens a closure `Err` into a string.
fn tx_create(store: &SledStore, j: &[u8], r: &[u8]) -> bool {
    store
        .transaction(|tx| {
            if tx.get(j)?.is_some() || tx.get(r)?.is_some() {
                return Ok(false);
            }
            tx.set(j, TX_J)?;
            tx.set(r, TX_R)?;
            Ok(true)
        })
        .expect("transaction must not error")
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Final {
    TxWon,
    CasWon,
}

/// Classify the stored pair. This is the oracle for every round, so it has positive controls
/// (`classifier_*`): it must accept exactly the two valid shapes and reject mixes.
fn classify(j: Option<&[u8]>, r: Option<&[u8]>) -> Result<Final, String> {
    match (j, r) {
        (Some(TX_J), Some(TX_R)) => Ok(Final::TxWon),
        (Some(CAS_J), None) => Ok(Final::CasWon),
        other => Err(format!("invalid final state (J, R) = {other:?}")),
    }
}

fn spin(ns: i64) {
    let until = Instant::now() + Duration::from_nanos(ns.unsigned_abs());
    while Instant::now() < until {
        std::hint::spin_loop();
    }
}

#[derive(Default)]
struct Observation {
    samples: u64,
    saw_tx_pair: u64,
    violations: Vec<String>,
}

/// Read both keys in both orders until the writers finish. A transaction commits both rows at one
/// instant, so: `R` present => `J` is the transaction's; `J` is the transaction's => `R` present.
fn observe(store: &SledStore, j: &[u8], r: &[u8], finished: &AtomicUsize) -> Observation {
    let mut obs = Observation::default();
    let note = |obs: &mut Observation, msg: String| {
        if obs.violations.len() < 5 {
            obs.violations.push(msg);
        }
    };
    while finished.load(Ordering::SeqCst) < 2 {
        // R first, then J.
        let r1 = store.get(r).unwrap();
        let j1 = store.get(j).unwrap();
        if r1.is_some() && (r1.as_deref() != Some(TX_R) || j1.as_deref() != Some(TX_J)) {
            note(
                &mut obs,
                format!("read R then J: saw R={r1:?} with J={j1:?} (R without the tx's J)"),
            );
        }
        // J first, then R.
        let j2 = store.get(j).unwrap();
        let r2 = store.get(r).unwrap();
        if j2.as_deref() == Some(TX_J) {
            obs.saw_tx_pair += 1;
            if r2.as_deref() != Some(TX_R) {
                note(
                    &mut obs,
                    format!("read J then R: saw the tx's J with R={r2:?} (half-committed)"),
                );
            }
        }
        if !matches!(j2.as_deref(), None | Some(TX_J) | Some(CAS_J)) {
            note(&mut obs, format!("J held an unknown value: {j2:?}"));
        }
        obs.samples += 1;
        std::thread::yield_now();
    }
    obs
}

struct RoundOutcome {
    result: Result<Final, String>,
    observation: Observation,
}

/// One race. `skew_ns > 0` delays the transaction, `< 0` delays the CAS, after a shared barrier.
fn race(store: &SledStore, trial: usize, round: u64, skew_ns: i64) -> RoundOutcome {
    let (j, r) = (key_j(trial, round), key_r(trial, round));
    let barrier = Barrier::new(2);
    let finished = AtomicUsize::new(0);

    let (tx_created, cas, observation) = std::thread::scope(|s| {
        let tx = s.spawn(|| {
            barrier.wait();
            if skew_ns > 0 {
                spin(skew_ns);
            }
            let created = tx_create(store, &j, &r);
            finished.fetch_add(1, Ordering::SeqCst);
            created
        });
        let cas = s.spawn(|| {
            barrier.wait();
            if skew_ns < 0 {
                spin(skew_ns);
            }
            let res = store.compare_and_put(&j, None, CAS_J).unwrap();
            finished.fetch_add(1, Ordering::SeqCst);
            res
        });
        let obs = s.spawn(|| observe(store, &j, &r, &finished));
        (tx.join().unwrap(), cas.join().unwrap(), obs.join().unwrap())
    });

    let stored = classify(
        store.get(&j).unwrap().as_deref(),
        store.get(&r).unwrap().as_deref(),
    );
    RoundOutcome {
        result: reconcile(stored, tx_created, matches!(cas, CompareAndPut::Applied)),
        observation,
    }
}

/// The writers' own reports must agree with the stored state: exactly one of them won, and it is
/// the one whose rows are stored. Kept separate from `race` so it can be tested directly.
fn reconcile(
    stored: Result<Final, String>,
    tx_created: bool,
    cas_applied: bool,
) -> Result<Final, String> {
    match (&stored, tx_created, cas_applied) {
        (Ok(Final::TxWon), true, false) | (Ok(Final::CasWon), false, true) => stored,
        (Ok(_), tx, c) => Err(format!(
            "stored state is valid but the writers disagree with it: tx_created={tx}, \
             cas_applied={c}, stored={stored:?}"
        )),
        (Err(e), tx, c) => Err(format!("{e} (tx_created={tx}, cas_applied={c})")),
    }
}

/// Deterministic skew spread over roughly +-100 us, including zero, so both writers get the head
/// start in turn and some rounds are as simultaneous as a barrier release allows.
fn skew_for(trial: usize, round: u64) -> i64 {
    let x = (round.wrapping_mul(7_919) + (trial as u64).wrapping_mul(104_729)) % 200_000;
    x as i64 - 100_000
}

#[derive(Default)]
struct Tally {
    rounds: u64,
    tx_wins: u64,
    cas_wins: u64,
    violations: Vec<String>,
    observer_samples: u64,
    observer_saw_tx_pair: u64,
}

impl Tally {
    fn add(&mut self, trial: usize, round: u64, out: RoundOutcome) {
        self.rounds += 1;
        self.observer_samples += out.observation.samples;
        self.observer_saw_tx_pair += out.observation.saw_tx_pair;
        match out.result {
            Ok(Final::TxWon) => self.tx_wins += 1,
            Ok(Final::CasWon) => self.cas_wins += 1,
            Err(e) => self
                .violations
                .push(format!("trial {trial} round {round}: {e}")),
        }
        for v in out.observation.violations {
            self.violations
                .push(format!("trial {trial} round {round}: observer: {v}"));
        }
    }
}

#[test]
fn sled_transaction_excludes_single_key_cas_on_the_same_key() {
    let trials = env_usize(ENV_TRIALS, DEFAULT_TRIALS);
    let mut total = Tally::default();
    for trial in 0..trials {
        // A fresh real-path store per trial: independent trials, and the real code path (not
        // `temporary()`), although exclusion is in-memory and path-independent.
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SledStore::open(dir.path().join("db")).expect("open");
        let mut t = Tally::default();
        let mut round = 0u64;
        while round < ROUNDS_PER_TRIAL
            || ((t.tx_wins == 0 || t.cas_wins == 0) && round < ROUNDS_PER_TRIAL + MAX_EXTRA_ROUNDS)
        {
            let out = race(&store, trial, round, skew_for(trial, round));
            t.add(trial, round, out);
            round += 1;
        }
        println!(
            "S0A-D3-TRIAL trial={trial} rounds={} tx_wins={} cas_wins={} violations={} \
             observer_samples={} observer_saw_tx_pair={}",
            t.rounds,
            t.tx_wins,
            t.cas_wins,
            t.violations.len(),
            t.observer_samples,
            t.observer_saw_tx_pair
        );
        total.rounds += t.rounds;
        total.tx_wins += t.tx_wins;
        total.cas_wins += t.cas_wins;
        total.observer_samples += t.observer_samples;
        total.observer_saw_tx_pair += t.observer_saw_tx_pair;
        total.violations.extend(t.violations);
    }
    println!(
        "S0A-D3-SUMMARY trials={trials} rounds={} tx_wins={} cas_wins={} violations={} \
         observer_samples={} observer_saw_tx_pair={}",
        total.rounds,
        total.tx_wins,
        total.cas_wins,
        total.violations.len(),
        total.observer_samples,
        total.observer_saw_tx_pair
    );

    // The property. Every round must be a clean A or B, and the observer must never have seen a
    // half-committed transaction.
    assert!(
        total.violations.is_empty(),
        "transaction / single-key CAS exclusion violated in {} rounds; first:\n{}",
        total.violations.len(),
        total.violations[..total.violations.len().min(10)].join("\n")
    );
    assert_eq!(total.tx_wins + total.cas_wins, total.rounds);

    // Power: both winners reachable under racing. A statement about scheduling, so only gated
    // under ICN_S0A_STRICT; the sequential tests pin each outcome deterministically either way.
    if total.tx_wins == 0 || total.cas_wins == 0 {
        println!(
            "S0A-D3-POWER INCONCLUSIVE: only one winner was ever reached under racing \
             (tx_wins={}, cas_wins={}); the exclusion check ran, but one interleaving class was \
             never exercised here.",
            total.tx_wins, total.cas_wins
        );
        assert!(
            !strict(),
            "ICN_S0A_STRICT=1: one winner outcome was never reached under racing"
        );
    } else {
        println!(
            "S0A-D3-POWER OK: both winners reached (tx {}, cas {}).",
            total.tx_wins, total.cas_wins
        );
    }
    assert!(
        total.observer_samples > 0,
        "the observer never sampled, so atomic visibility was not exercised"
    );
}

/// Deterministic outcome A: the transaction runs to completion first; the CAS must lose and
/// report the transaction's value.
#[test]
fn sequential_transaction_then_cas_yields_the_transaction_pair() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SledStore::open(dir.path().join("db")).expect("open");
    let (j, r) = (key_j(0, 0), key_r(0, 0));
    assert!(tx_create(&store, &j, &r));
    match store.compare_and_put(&j, None, CAS_J).unwrap() {
        CompareAndPut::Conflict { current } => assert_eq!(current.as_deref(), Some(TX_J)),
        CompareAndPut::Applied => panic!("CAS applied over a committed transaction"),
    }
    assert_eq!(
        classify(
            store.get(&j).unwrap().as_deref(),
            store.get(&r).unwrap().as_deref()
        ),
        Ok(Final::TxWon)
    );
}

/// Deterministic outcome B: the CAS lands first; the transaction must create nothing, and in
/// particular must not write `R` for a `J` it did not create.
#[test]
fn sequential_cas_then_transaction_yields_the_cas_value_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SledStore::open(dir.path().join("db")).expect("open");
    let (j, r) = (key_j(0, 1), key_r(0, 1));
    assert!(matches!(
        store.compare_and_put(&j, None, CAS_J).unwrap(),
        CompareAndPut::Applied
    ));
    assert!(
        !tx_create(&store, &j, &r),
        "transaction must see J and stand down"
    );
    assert_eq!(store.get(&r).unwrap(), None, "R must remain absent");
    assert_eq!(
        classify(
            store.get(&j).unwrap().as_deref(),
            store.get(&r).unwrap().as_deref()
        ),
        Ok(Final::CasWon)
    );
}

/// Positive control for the oracle: it accepts exactly the two valid shapes and rejects the
/// mixes a broken exclusion would produce.
#[test]
fn classifier_rejects_every_mixed_or_partial_state() {
    assert_eq!(classify(Some(TX_J), Some(TX_R)), Ok(Final::TxWon));
    assert_eq!(classify(Some(CAS_J), None), Ok(Final::CasWon));
    for (j, r) in [
        (Some(TX_J), None),        // transaction half-committed
        (None, Some(TX_R)),        // R without J
        (Some(CAS_J), Some(TX_R)), // CAS value with the transaction's R
        (Some(TX_J), Some(b"x" as &[u8])),
        (None, None), // neither writer won
    ] {
        assert!(classify(j, r).is_err(), "({j:?}, {r:?}) must be rejected");
    }
}

/// Positive control for the race accounting: the writers' reports must agree with the stored
/// state. Both-succeed and neither-succeed are rejected even when the stored state looks valid.
#[test]
fn reconcile_rejects_writer_reports_that_contradict_the_stored_state() {
    // Agreement.
    assert_eq!(reconcile(Ok(Final::TxWon), true, false), Ok(Final::TxWon));
    assert_eq!(reconcile(Ok(Final::CasWon), false, true), Ok(Final::CasWon));
    // A valid stored state with the wrong writer credited, both credited, or neither.
    assert!(reconcile(Ok(Final::CasWon), true, false).is_err());
    assert!(reconcile(Ok(Final::TxWon), false, true).is_err());
    assert!(reconcile(Ok(Final::TxWon), true, true).is_err());
    assert!(reconcile(Ok(Final::CasWon), false, false).is_err());
    // An invalid stored state is never rescued by the writers' reports.
    assert!(reconcile(Err("mixed".into()), true, false).is_err());
}
