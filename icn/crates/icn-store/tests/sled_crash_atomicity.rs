//! ADR-0091 S0a, test D2: does a sled multi-key transaction recover all-or-none after SIGKILL?
//!
//! **Evidence only.** No production code changes, no new `Store` API.
//!
//! # Property exercised
//!
//! *Atomic recovery of a multi-key transaction (class B).* `SledStore::transaction` writes a
//! pair of rows `(a_i, b_i)` under different key prefixes. After the writer is SIGKILLed at an
//! arbitrary moment and the directory is reopened, every pair index `i` must be present under
//! both prefixes or under neither. A single orphan row is a violation.
//!
//! # What this does not establish
//!
//! - **Not a proof.** N independent kills with no violation is evidence, not a proof of crash
//!   atomicity; the kill instants are sampled, not enumerated.
//! - **Not durability.** Nothing here asserts that an *acknowledged* commit survives: a
//!   transaction does not flush, so sled may legitimately lose the tail. Acked-vs-recovered is
//!   reported, never asserted (see `sled_flush_barrier.rs` for the barrier).
//! - **Not power loss.** SIGKILL leaves the kernel page cache intact. A process kill cannot
//!   distinguish a data `fsync` from a plain `write`, so this says nothing about power failure.
//! - **Not the adjacent-key layout, other page shapes, I/O errors, or a poisoned instance.**
//!
//! # Why a control
//!
//! "No torn pair found" passes vacuously if the harness could never observe one. The same loop
//! is therefore run with two ordinary `put`s, which carry no atomicity promise. If *that* tears
//! at least once the harness demonstrably sees partial outcomes; if it never does, the
//! transaction result has reduced power and the test says so (and fails under
//! `ICN_S0A_STRICT=1`). A second, deterministic control (`checker_flags_*`) proves the
//! classifier itself reports a hand-built torn state.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod s0a_support;

use std::time::Duration;

use icn_store::{SledStore, Store, TransactionalStore};
use s0a_support::*;

const CHILD_ENTRY: &str = "s0a_d2_child_entry";
const DEFAULT_TRIALS: usize = 10;
/// The child idles after this many pairs rather than filling the disk if the parent is slow.
const MAX_PAIRS: u64 = 400_000;
const ACK_EVERY: u64 = 64;

/// Child half. Run on its own (no `ICN_S0A_CHILD_*`) it returns at once.
#[test]
fn s0a_d2_child_entry() {
    let Some((mode, dir)) = child_config() else {
        return;
    };
    child_arm_watchdog();
    let store = SledStore::open(&dir).expect("child: sled must open");
    child_ready();
    let mut i = 0u64;
    loop {
        match mode.as_str() {
            "tx" => store
                .transaction(|tx| {
                    tx.set(&key_a(i), &value(i, b'a'))?;
                    tx.set(&key_b(i), &value(i, b'b'))?;
                    Ok(())
                })
                .expect("child: transaction must commit"),
            "plain" => {
                store.put(&key_a(i), &value(i, b'a')).expect("child: put a");
                store.put(&key_b(i), &value(i, b'b')).expect("child: put b");
            }
            other => panic!("unknown child mode {other}"),
        }
        if i % ACK_EVERY == ACK_EVERY - 1 {
            child_ack(i);
        }
        i += 1;
        if i >= MAX_PAIRS {
            child_park();
        }
    }
}

struct Trial {
    index: usize,
    delay_ms: u64,
    acked: Option<u64>,
    reopen_attempts: u32,
    report: PairReport,
}

/// Kill delays are a deterministic spread over ~[60 ms, 1.4 s] so a series covers instants before
/// the first buffer seal, around sled's 500 ms background flush, and well after it. Sub-ms jitter
/// keeps successive trials from landing on the same phase of the flusher.
fn kill_delay(trial: usize) -> Duration {
    let t = trial as u64;
    Duration::from_micros(60_000 + (t * 211_379) % 1_340_000 + (t * 7_919) % 1_000)
}

fn run_trial(mode: &str, index: usize) -> Trial {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("db");
    let run = ChildRun::spawn(CHILD_ENTRY, mode, &path);
    assert!(run.wait_ready(), "child never reported ready ({mode})");
    let delay = kill_delay(index);
    std::thread::sleep(delay);
    let killed = run.kill();
    assert_eq!(
        killed.signal,
        Some(9),
        "child must die by SIGKILL, not exit ({mode} trial {index})"
    );
    let (store, reopen_attempts) = reopen(&path);
    let report = analyze_pairs(&store);
    drop(store);
    Trial {
        index,
        delay_ms: delay.as_millis() as u64,
        acked: killed.last_ack,
        reopen_attempts,
        report,
    }
}

fn run_series(mode: &str, n: usize) -> Vec<Trial> {
    (0..n)
        .map(|i| {
            let t = run_trial(mode, i);
            println!(
                "S0A-D2-TRIAL mode={mode} trial={} delay_ms={} acked={} a={} b={} complete={} \
                 torn={} corrupt={} unparsable={} prefix_contiguous={} reopen_attempts={}",
                t.index,
                t.delay_ms,
                t.acked.map_or("none".into(), |a| a.to_string()),
                t.report.a_rows,
                t.report.b_rows,
                t.report.complete,
                t.report.torn.len(),
                t.report.corrupt.len(),
                t.report.unparsable,
                t.report.prefix_contiguous,
                t.reopen_attempts,
            );
            t
        })
        .collect()
}

struct Summary {
    trials: usize,
    torn_trials: usize,
    corrupt_trials: usize,
    trials_with_data: usize,
    complete_total: usize,
    torn_rows_total: usize,
    acked_but_missing: usize,
    non_prefix: usize,
}

fn summarize(trials: &[Trial]) -> Summary {
    Summary {
        trials: trials.len(),
        torn_trials: trials.iter().filter(|t| !t.report.torn.is_empty()).count(),
        corrupt_trials: trials
            .iter()
            .filter(|t| !t.report.corrupt.is_empty() || t.report.unparsable > 0)
            .count(),
        trials_with_data: trials.iter().filter(|t| t.report.complete > 0).count(),
        complete_total: trials.iter().map(|t| t.report.complete).sum(),
        torn_rows_total: trials.iter().map(|t| t.report.torn.len()).sum(),
        // An ACK the parent read, whose pair index is beyond what recovery found complete.
        // Expected to be non-zero sometimes (no barrier); informational only.
        acked_but_missing: trials
            .iter()
            .filter(|t| {
                t.acked
                    .is_some_and(|a| t.report.max_complete.is_none_or(|m| m < a))
            })
            .count(),
        non_prefix: trials
            .iter()
            .filter(|t| !t.report.prefix_contiguous)
            .count(),
    }
}

fn print_summary(mode: &str, s: &Summary) {
    println!(
        "S0A-D2-SUMMARY mode={mode} trials={} torn_trials={} torn_rows_total={} \
         corrupt_trials={} trials_with_data={} complete_pairs_total={} \
         acked_but_missing_trials={} non_prefix_trials={}",
        s.trials,
        s.torn_trials,
        s.torn_rows_total,
        s.corrupt_trials,
        s.trials_with_data,
        s.complete_total,
        s.acked_but_missing,
        s.non_prefix,
    );
}

#[test]
fn sled_transaction_is_all_or_none_across_sigkill() {
    let _serial = serialize_kill_tests();
    let n = env_usize(ENV_TRIALS, DEFAULT_TRIALS);

    let tx = run_series("tx", n);
    let plain = run_series("plain", n);
    let tx_s = summarize(&tx);
    let plain_s = summarize(&plain);
    print_summary("tx", &tx_s);
    print_summary("plain-control", &plain_s);

    // The property under test. Collect every violation before failing, so a RED run reports the
    // whole picture rather than the first trial.
    let violations: Vec<String> = tx
        .iter()
        .filter(|t| !t.report.torn.is_empty() || !t.report.corrupt.is_empty())
        .map(|t| {
            format!(
                "trial {} (kill after {} ms): torn indices {:?}, corrupt indices {:?}",
                t.index,
                t.delay_ms,
                &t.report.torn[..t.report.torn.len().min(8)],
                &t.report.corrupt[..t.report.corrupt.len().min(8)],
            )
        })
        .collect();
    assert!(
        violations.is_empty(),
        "sled transaction was NOT all-or-none across SIGKILL; ADR-0091 option B is invalid \
         until explained:\n{}",
        violations.join("\n")
    );
    assert_eq!(
        tx_s.corrupt_trials, 0,
        "a recovered transaction row was unparsable or damaged"
    );

    // Non-vacuity of the transaction series: if every trial recovered an empty database, "no
    // torn pair" says nothing.
    assert!(
        tx_s.complete_total > 0,
        "no transaction trial recovered a single complete pair, so atomicity was never exercised \
         (kill delays too short for sled to have written anything?)"
    );

    // Power of the experiment: did the control ever tear?
    if plain_s.torn_trials == 0 {
        println!(
            "S0A-D2-POWER INCONCLUSIVE: the two-plain-put control never tore in {} trials; the \
             transaction result has REDUCED POWER (harness may be unable to observe partial \
             outcomes under this timing).",
            plain_s.trials
        );
        assert!(
            !strict(),
            "ICN_S0A_STRICT=1: control never tore, so the transaction result is underpowered"
        );
    } else {
        println!(
            "S0A-D2-POWER OK: the plain-put control tore in {}/{} trials ({} orphan rows), so the \
             harness can observe partial outcomes.",
            plain_s.torn_trials, plain_s.trials, plain_s.torn_rows_total
        );
    }
}

/// Positive control for the classifier: a hand-built torn state must be flagged, an intact pair
/// must not. Deterministic, no child process.
#[test]
fn checker_flags_a_deliberately_torn_state() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SledStore::open(dir.path().join("db")).expect("open");
    // 0: intact. 1: only `a`. 2: only `b`. 3: both, but `b` carries the wrong bytes.
    store.put(&key_a(0), &value(0, b'a')).unwrap();
    store.put(&key_b(0), &value(0, b'b')).unwrap();
    store.put(&key_a(1), &value(1, b'a')).unwrap();
    store.put(&key_b(2), &value(2, b'b')).unwrap();
    store.put(&key_a(3), &value(3, b'a')).unwrap();
    store.put(&key_b(3), b"not the expected bytes").unwrap();

    let report = analyze_pairs(&store);
    assert_eq!(report.torn, vec![1, 2], "orphans must be reported");
    assert_eq!(report.corrupt, vec![3], "a damaged value must be reported");
    assert_eq!(report.complete, 2, "indices 0 and 3 have both rows");
    assert_eq!(report.max_complete, Some(3));
    assert!(
        !report.prefix_contiguous,
        "complete {{0,3}} is not a prefix"
    );

    // And an intact store reports clean.
    let clean = SledStore::open(dir.path().join("clean")).expect("open");
    for i in 0..5 {
        clean.put(&key_a(i), &value(i, b'a')).unwrap();
        clean.put(&key_b(i), &value(i, b'b')).unwrap();
    }
    let clean_report = analyze_pairs(&clean);
    assert!(clean_report.torn.is_empty() && clean_report.corrupt.is_empty());
    assert_eq!(clean_report.complete, 5);
    assert!(clean_report.prefix_contiguous);
}
