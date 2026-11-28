//! Shared harness for the ADR-0091 S0a storage-capability tests (evidence only).
//!
//! Not a test target: `tests/<dir>/mod.rs` is compiled only where a test file declares
//! `mod s0a_support;`. Everything here is test scaffolding. Nothing in `icn-store/src` is touched.
//!
//! # The child-process protocol
//!
//! A kill test re-executes *this test binary* (`std::env::current_exe`) with libtest's `--exact`
//! filter pointing at a `*_child_entry` test, plus [`ENV_CHILD_MODE`] / [`ENV_CHILD_DIR`]. Run
//! without those variables the entry test returns at once, so a plain `cargo test` is unaffected.
//! The child talks to the parent over stdout, one marker per line:
//!
//! - `S0A-READY`: the store is open and the child is about to do its work.
//! - `S0A-ACK <n>`: the child's own claim that everything up to `n` completed. It is printed
//!   **after** the operation returns, so an ACK the parent has read is a lower bound on what the
//!   child finished, never an upper bound (the kill can land between an operation and its ACK).
//!
//! Markers are searched for anywhere in a line, not at its start: libtest prints
//! `test <name> ... ` without a newline before the body runs, so the first line is prefixed.
//!
//! # No orphans
//!
//! [`ChildRun`] kills and reaps its child on drop, so a panicking parent does not leave one
//! behind. A child also arms a watchdog that aborts it after [`CHILD_WATCHDOG`], which covers the
//! one case drop cannot: the parent itself being SIGKILLed.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::io::{BufRead as _, Write as _};
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use icn_store::{SledStore, Store};

pub const ENV_CHILD_MODE: &str = "ICN_S0A_CHILD_MODE";
pub const ENV_CHILD_DIR: &str = "ICN_S0A_CHILD_DIR";
/// Number of independent kill trials per scenario. Defaults are small so a routine `cargo test`
/// stays quick; the evidence runs raise it (>= 30).
pub const ENV_TRIALS: &str = "ICN_S0A_TRIALS";
/// `ICN_S0A_STRICT=1` turns *reduced experimental power* (a control that never tore, a winner
/// that was never reached) into a failure. Off by default because those outcomes are
/// scheduling- and timing-dependent, so gating a routine run on them would be a flake, not a
/// safety signal. The atomicity assertions themselves are never gated by this switch.
pub const ENV_STRICT: &str = "ICN_S0A_STRICT";

const READY: &str = "S0A-READY";
const ACK: &str = "S0A-ACK ";

/// A child aborts itself after this long, so nothing outlives a dead parent.
pub const CHILD_WATCHDOG: Duration = Duration::from_secs(180);
/// How long the parent waits for a child to become ready / acknowledge before failing.
pub const CHILD_DEADLINE: Duration = Duration::from_secs(90);

pub fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

pub fn strict() -> bool {
    std::env::var(ENV_STRICT).is_ok_and(|v| v == "1")
}

static SERIAL: Mutex<()> = Mutex::new(());

/// Kill tests in one binary run one at a time. libtest runs tests in parallel by default, and
/// two sets of children hammering one disk would blur exactly the timing these tests study.
pub fn serialize_kill_tests() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

// ── child side ─────────────────────────────────────────────────────────────────────────────────

/// `Some((mode, dir))` when this process is a re-executed child; `None` in a normal test run.
pub fn child_config() -> Option<(String, PathBuf)> {
    let mode = std::env::var(ENV_CHILD_MODE).ok()?;
    let dir = std::env::var(ENV_CHILD_DIR).ok()?;
    Some((mode, PathBuf::from(dir)))
}

fn child_say(line: &str) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}").unwrap();
    out.flush().unwrap();
}

pub fn child_ready() {
    child_say(READY);
}

pub fn child_ack(n: u64) {
    child_say(&format!("{ACK}{n}"));
}

pub fn child_arm_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(CHILD_WATCHDOG);
        std::process::abort();
    });
}

/// Wait to be killed. Never returns, so no handle is dropped and sled's drop-time flush cannot run.
pub fn child_park() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

// ── parent side ────────────────────────────────────────────────────────────────────────────────

pub struct Killed {
    /// The signal that ended the child. The tests require `Some(9)`; anything else means the
    /// child died or exited some other way and the trial proves nothing about SIGKILL.
    pub signal: Option<i32>,
    /// Highest `S0A-ACK` the parent read before the child's pipe closed (`None` if none arrived).
    pub last_ack: Option<u64>,
}

pub struct ChildRun {
    child: Option<Child>,
    ready_rx: Receiver<()>,
    /// Highest ACK seen, stored as `n + 1` so that 0 means "none".
    last_ack: Arc<AtomicU64>,
    reader: Option<JoinHandle<()>>,
}

impl ChildRun {
    pub fn spawn(entry_test: &str, mode: &str, dir: &Path) -> Self {
        let mut child = Command::new(std::env::current_exe().expect("current_exe"))
            .args([entry_test, "--exact", "--nocapture", "--test-threads=1"])
            .env(ENV_CHILD_MODE, mode)
            .env(ENV_CHILD_DIR, dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the child must start");
        let stdout = child.stdout.take().expect("child stdout");
        let (ready_tx, ready_rx) = mpsc::channel();
        let last_ack = Arc::new(AtomicU64::new(0));
        let ack_writer = Arc::clone(&last_ack);
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if line.contains(READY) {
                    let _ = ready_tx.send(());
                }
                if let Some((_, n)) = line.split_once(ACK) {
                    if let Ok(n) = n.trim().parse::<u64>() {
                        ack_writer.fetch_max(n + 1, Ordering::SeqCst);
                    }
                }
            }
        });
        Self {
            child: Some(child),
            ready_rx,
            last_ack,
            reader: Some(reader),
        }
    }

    pub fn wait_ready(&self) -> bool {
        self.ready_rx.recv_timeout(CHILD_DEADLINE).is_ok()
    }

    /// Block until the child has acknowledged at least `n`, or the deadline passes.
    pub fn wait_ack_at_least(&self, n: u64) -> bool {
        let start = Instant::now();
        while start.elapsed() < CHILD_DEADLINE {
            if self.last_ack.load(Ordering::SeqCst) > n {
                return true;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        false
    }

    /// SIGKILL (`Child::kill` on Unix), reap, and collect what the child managed to say.
    pub fn kill(mut self) -> Killed {
        let mut child = self.child.take().expect("child already reaped");
        child.kill().expect("SIGKILL the child");
        let status = child.wait().expect("reap the child");
        if let Some(reader) = self.reader.take() {
            // The pipe's only writer is dead, so the reader sees EOF and ends.
            let _ = reader.join();
        }
        let raw = self.last_ack.load(Ordering::SeqCst);
        Killed {
            signal: status.signal(),
            last_ack: raw.checked_sub(1),
        }
    }
}

impl Drop for ChildRun {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Reopen a store after its writer died. The dead process's directory lock is released by the
/// kernel as soon as the child is reaped, so the first attempt should succeed. A short retry is
/// kept only so a loaded runner reports a lock problem as a lock problem; the attempt count is
/// returned so the evidence shows if it was ever needed.
pub fn reopen(path: &Path) -> (SledStore, u32) {
    let mut last = None;
    for attempt in 1..=20u32 {
        match SledStore::open(path) {
            Ok(store) => return (store, attempt),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
    panic!("sled would not reopen after the writer was killed: {last:?}");
}

// ── the two-key pair workload ──────────────────────────────────────────────────────────────────
//
// Pair `i` is two rows under different prefixes, `a` and `b`. The prefixes are deliberate: the
// future design writes a journal row and an operation row under different keyspaces, so the two
// rows of a pair sort far apart and, once the tree has grown, sit on different leaf pages. A pair
// adjacent in key order would be the easier case for any engine.

pub const PAIR_A: &str = "s0a:pair:a:";
pub const PAIR_B: &str = "s0a:pair:b:";

pub fn key_a(i: u64) -> Vec<u8> {
    format!("{PAIR_A}{i:010}").into_bytes()
}

pub fn key_b(i: u64) -> Vec<u8> {
    format!("{PAIR_B}{i:010}").into_bytes()
}

/// Deterministic, variable-length (48..208 bytes) payload. The length varies so that sled's
/// buffer boundaries do not line up with pair boundaries at a fixed parity, and the content is
/// derived from `(i, tag)` so a reader can recompute it and catch a damaged value, not just a
/// missing one.
pub fn value(i: u64, tag: u8) -> Vec<u8> {
    let len = 48 + ((i.wrapping_mul(2_654_435_761) >> 7) % 160) as usize;
    (0..len)
        .map(|n| tag ^ (i as u8).wrapping_add(n as u8))
        .collect()
}

#[derive(Debug, Default, Clone)]
pub struct PairReport {
    pub a_rows: usize,
    pub b_rows: usize,
    /// Indices present under both prefixes.
    pub complete: usize,
    /// Indices present under exactly one prefix. **Any entry here is an atomicity violation.**
    pub torn: Vec<u64>,
    /// Indices whose stored bytes differ from `value(i, tag)`.
    pub corrupt: Vec<u64>,
    /// Rows whose key did not parse as `<prefix><index>`.
    pub unparsable: usize,
    pub max_complete: Option<u64>,
    /// Whether the complete indices are exactly `0..=max_complete` (a prefix of the history).
    /// Reported, never asserted: this is not part of the contract being tested.
    pub prefix_contiguous: bool,
}

fn rows_by_index(
    store: &dyn Store,
    prefix: &str,
    tag: u8,
    corrupt: &mut Vec<u64>,
    unparsable: &mut usize,
) -> BTreeMap<u64, ()> {
    let mut out = BTreeMap::new();
    for (k, v) in store.scan(prefix.as_bytes()).expect("scan") {
        let idx = std::str::from_utf8(&k[prefix.len()..])
            .ok()
            .and_then(|s| s.parse::<u64>().ok());
        let Some(i) = idx else {
            *unparsable += 1;
            continue;
        };
        if v != value(i, tag) {
            corrupt.push(i);
        }
        out.insert(i, ());
    }
    out
}

/// Classify every pair in `store`. This is the observer the kill tests depend on, so it has its
/// own positive-control test (`checker_flags_*`): an observer that can never report a tear would
/// make "no tears found" meaningless.
pub fn analyze_pairs(store: &dyn Store) -> PairReport {
    let mut report = PairReport::default();
    let a = rows_by_index(
        store,
        PAIR_A,
        b'a',
        &mut report.corrupt,
        &mut report.unparsable,
    );
    let b = rows_by_index(
        store,
        PAIR_B,
        b'b',
        &mut report.corrupt,
        &mut report.unparsable,
    );
    report.a_rows = a.len();
    report.b_rows = b.len();
    let mut complete = Vec::new();
    for i in a.keys().chain(b.keys()) {
        match (a.contains_key(i), b.contains_key(i)) {
            (true, true) => complete.push(*i),
            _ => report.torn.push(*i),
        }
    }
    complete.sort_unstable();
    complete.dedup();
    report.torn.sort_unstable();
    report.torn.dedup();
    report.corrupt.sort_unstable();
    report.corrupt.dedup();
    report.complete = complete.len();
    report.max_complete = complete.last().copied();
    report.prefix_contiguous = match report.max_complete {
        None => true,
        Some(max) => complete.len() as u64 == max + 1,
    };
    report
}
