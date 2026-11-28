//! ADR-0091 S0a, test D4: what `Store::flush` does and does not give, on a real path.
//!
//! **Evidence only.** No production code changes.
//!
//! # Properties exercised
//!
//! 1. **Process-crash durability after a barrier (class C).** A child writes rows to a
//!    `SledStore::open`ed directory, calls `Store::flush` through `&dyn Store`, and is SIGKILLed
//!    the instant it acknowledges. After reopen, every acknowledged row must be present with its
//!    exact bytes.
//! 2. **The default `Store::flush` is a silent no-op.** A store that does not override `flush`
//!    returns `Ok(())` while holding its writes only in volatile memory. `Ok(())` therefore does
//!    not by itself imply a durable barrier happened. (Characterization of today's trait default;
//!    it should flip if the default ever becomes fail-closed.)
//!
//! # What this does not establish
//!
//! - **Not power-loss safety.** SIGKILL leaves the kernel page cache intact, so this cannot tell
//!   a data `fsync` from a plain `write`, and sled's Linux barrier is `sync_file_range`, not a
//!   verified `fsync`-class operation (packet section 14.2 C1). Nothing here makes a power-loss claim.
//! - **Not that the barrier is what saved the data.** The kill follows the acknowledgment within
//!   milliseconds, but sled also flushes in the background every 500 ms. The unflushed series
//!   below exists to show whether *loss is observable at all* in this harness; if it never loses
//!   anything, the flushed series has reduced power and says so.
//! - **Unflushed rows are never asserted.** Whether they survive or vanish is observed and
//!   recorded only. Sled makes no promise either way.
//! - `SledStore::temporary()` is deliberately not used: it skips the sync entirely.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod s0a_support;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};
use s0a_support::*;

const CHILD_ENTRY: &str = "s0a_d4_child_entry";
const DEFAULT_TRIALS: usize = 10;
const ROWS: u64 = 200;

fn row_key(i: u64) -> Vec<u8> {
    format!("s0a:d4:row:{i:06}").into_bytes()
}

fn row_value(i: u64) -> Vec<u8> {
    value(i, b'd')
}

/// Child half. Modes: `flushed` (write, `Store::flush`, ack) and `unflushed` (write, ack, no
/// flush). The ack is printed only after the last call returns.
#[test]
fn s0a_d4_child_entry() {
    let Some((mode, dir)) = child_config() else {
        return;
    };
    child_arm_watchdog();
    let sled = SledStore::open(&dir).expect("child: sled must open");
    // Everything below goes through the trait object, the way a ledger holding `Arc<dyn Store>`
    // would call it.
    let store: &dyn Store = &sled;
    child_ready();
    for i in 0..ROWS {
        store
            .put(&row_key(i), &row_value(i))
            .expect("child: put must succeed");
    }
    match mode.as_str() {
        "flushed" => store.flush().expect("child: flush must succeed"),
        "unflushed" => {}
        other => panic!("unknown child mode {other}"),
    }
    child_ack(ROWS);
    child_park();
}

struct Trial {
    present: usize,
    damaged: usize,
    acked: Option<u64>,
}

fn run_trial(mode: &str, index: usize) -> Trial {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("db");
    let run = ChildRun::spawn(CHILD_ENTRY, mode, &path);
    assert!(run.wait_ready(), "child never reported ready ({mode})");
    // Kill the instant the acknowledgment is read: the strongest case for the barrier, because
    // nothing but the barrier has had time to persist the rows.
    assert!(
        run.wait_ack_at_least(ROWS),
        "child never acknowledged ({mode} trial {index})"
    );
    let killed = run.kill();
    assert_eq!(
        killed.signal,
        Some(9),
        "child must die by SIGKILL, not exit ({mode} trial {index})"
    );
    let (store, _) = reopen(&path);
    let mut present = 0;
    let mut damaged = 0;
    for i in 0..ROWS {
        match store.get(&row_key(i)).unwrap() {
            Some(v) if v == row_value(i) => present += 1,
            Some(_) => damaged += 1,
            None => {}
        }
    }
    drop(store);
    Trial {
        present,
        damaged,
        acked: killed.last_ack,
    }
}

#[test]
fn flush_barrier_survives_sigkill_and_default_flush_is_a_no_op() {
    let _serial = serialize_kill_tests();
    let n = env_usize(ENV_TRIALS, DEFAULT_TRIALS);

    // (a) Flushed: every acknowledged row must survive the kill. This is the asserted property.
    let flushed: Vec<Trial> = (0..n).map(|i| run_trial("flushed", i)).collect();
    for (i, t) in flushed.iter().enumerate() {
        println!(
            "S0A-D4-TRIAL mode=flushed trial={i} acked={} present={}/{ROWS} damaged={}",
            t.acked.map_or("none".into(), |a| a.to_string()),
            t.present,
            t.damaged
        );
    }
    let flushed_failures: Vec<String> = flushed
        .iter()
        .enumerate()
        .filter(|(_, t)| t.present != ROWS as usize || t.damaged != 0)
        .map(|(i, t)| {
            format!(
                "trial {i}: acked={:?} present={}/{ROWS} damaged={}",
                t.acked, t.present, t.damaged
            )
        })
        .collect();

    // (b) Unflushed: observed, NOT asserted. Shows whether the harness can see loss at all.
    let unflushed: Vec<Trial> = (0..n).map(|i| run_trial("unflushed", i)).collect();
    for (i, t) in unflushed.iter().enumerate() {
        println!(
            "S0A-D4-TRIAL mode=unflushed trial={i} acked={} present={}/{ROWS} damaged={}",
            t.acked.map_or("none".into(), |a| a.to_string()),
            t.present,
            t.damaged
        );
    }
    let lost_trials = unflushed
        .iter()
        .filter(|t| t.present < ROWS as usize)
        .count();
    let min_present = unflushed.iter().map(|t| t.present).min().unwrap_or(0);
    let max_present = unflushed.iter().map(|t| t.present).max().unwrap_or(0);

    println!(
        "S0A-D4-SUMMARY trials={n} flushed_failures={} flushed_total_rows_present={} \
         unflushed_trials_with_loss={lost_trials} unflushed_present_min={min_present} \
         unflushed_present_max={max_present} rows_per_trial={ROWS}",
        flushed_failures.len(),
        flushed.iter().map(|t| t.present).sum::<usize>(),
    );

    assert!(
        flushed_failures.is_empty(),
        "rows acknowledged after Store::flush did not survive SIGKILL:\n{}",
        flushed_failures.join("\n")
    );

    // Power of the flushed result.
    if lost_trials == 0 {
        println!(
            "S0A-D4-POWER INCONCLUSIVE: the unflushed control never lost a row in {n} trials, so \
             this harness could not tell a flush barrier from sled's background flusher; the \
             flushed series shows survival, not that the barrier caused it."
        );
        assert!(
            !strict(),
            "ICN_S0A_STRICT=1: loss was never observable, so the flushed result is underpowered"
        );
    } else {
        println!(
            "S0A-D4-POWER OK: unflushed rows were lost in {lost_trials}/{n} trials (present \
             {min_present}..{max_present} of {ROWS}), so loss is observable and the flushed \
             series' survival is attributable to the barrier within this timing."
        );
    }
}

// ── the default `Store::flush`, characterized ──────────────────────────────────────────────────

/// Two-level toy store: writes land in `volatile`; only `sync_to_durable` moves them to
/// `durable`, which is what a crash would preserve. `barrier_calls` counts real barriers.
#[derive(Default)]
struct Mem {
    volatile: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    durable: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    barrier_calls: AtomicUsize,
}

impl Mem {
    fn sync_to_durable(&self) {
        let v = self.volatile.lock().unwrap().clone();
        *self.durable.lock().unwrap() = v;
        self.barrier_calls.fetch_add(1, Ordering::SeqCst);
    }
    /// What survives a crash.
    fn crash_image(&self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        self.durable.lock().unwrap().clone()
    }
}

macro_rules! store_over_mem {
    ($ty:ident $(, flush = $flush:expr)?) => {
        impl Store for $ty {
            fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
                Ok(self.0.volatile.lock().unwrap().get(key).cloned())
            }
            fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
                self.0
                    .volatile
                    .lock()
                    .unwrap()
                    .insert(key.to_vec(), value.to_vec());
                Ok(())
            }
            fn delete(&self, key: &[u8]) -> Result<()> {
                self.0.volatile.lock().unwrap().remove(key);
                Ok(())
            }
            fn delete_atomic(&self, keys: &[Vec<u8>]) -> Result<()> {
                let mut m = self.0.volatile.lock().unwrap();
                for k in keys {
                    m.remove(k);
                }
                Ok(())
            }
            fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(self
                    .0
                    .volatile
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(k, _)| k.starts_with(prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect())
            }
            fn get_replica_metadata(&self, _h: &ContentHash) -> Result<Option<ReplicaMetadata>> {
                Ok(None)
            }
            fn put_replica_metadata(&self, _m: &ReplicaMetadata) -> Result<()> {
                Ok(())
            }
            fn list_replica_hashes(&self) -> Result<Vec<ContentHash>> {
                Ok(Vec::new())
            }
            $(
                fn flush(&self) -> Result<()> {
                    let f: fn(&Mem) -> Result<()> = $flush;
                    f(&self.0)
                }
            )?
        }
    };
}

/// Does **not** override `flush`: it inherits the trait default.
#[derive(Default)]
struct DefaultFlushStore(Mem);
store_over_mem!(DefaultFlushStore);

/// Overrides `flush` with a real (simulated) barrier. The positive control: it proves the probe
/// *can* see a barrier, so its silence for the default store means something.
#[derive(Default)]
struct BarrierFlushStore(Mem);
store_over_mem!(
    BarrierFlushStore,
    flush = |m| {
        m.sync_to_durable();
        Ok(())
    }
);

#[test]
fn default_flush_returns_ok_without_any_durable_barrier() {
    let store = DefaultFlushStore::default();
    let dyn_store: &dyn Store = &store;
    dyn_store.put(b"k", b"v").unwrap();

    assert!(
        dyn_store.flush().is_ok(),
        "the default flush returns Ok(())"
    );
    assert_eq!(
        store.0.barrier_calls.load(Ordering::SeqCst),
        0,
        "no barrier ran"
    );
    assert!(
        store.0.crash_image().is_empty(),
        "after an Ok flush, the row exists only in volatile memory: a crash here loses it"
    );
    assert_eq!(dyn_store.get(b"k").unwrap().as_deref(), Some(&b"v"[..]));
}

#[test]
fn overriding_flush_is_observably_a_barrier() {
    let store = BarrierFlushStore::default();
    let dyn_store: &dyn Store = &store;
    dyn_store.put(b"k", b"v").unwrap();
    assert!(
        store.0.crash_image().is_empty(),
        "nothing durable before flush"
    );

    dyn_store.flush().unwrap();
    assert_eq!(store.0.barrier_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.0.crash_image().get(&b"k"[..]).map(Vec::as_slice),
        Some(&b"v"[..]),
        "the probe sees the barrier when a store really implements one"
    );
}
