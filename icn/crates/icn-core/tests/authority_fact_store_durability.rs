//! N1-D: what an acknowledged authority fact is worth (icn#2799, ADR-0087).
//!
//! Two kinds of evidence, kept apart because they prove different things.
//!
//! - **The acknowledgment boundary, deterministically.** A backend that records every call and can
//!   be told to fail shows that `put_fact` returns `Ok` only after a successful flush, that a
//!   failed flush is a failed write, and that a retry after one is safe. Nothing here depends on
//!   timing or on what a storage engine happens to do in the background.
//! - **A process that dies.** A child persists facts, reports the acknowledgment, and is SIGKILLed
//!   with no chance to drop a handle or run a destructor; the parent reopens the directory. That
//!   shows acknowledged facts outlive the process that wrote them. It is **not** a power-loss
//!   test: the kernel's page cache survives a process kill, so it cannot tell an fsync from a
//!   plain write, and sled's flush is only best-effort against power loss anyway (see
//!   `DurableStore for SledStore`).
//!
//! The kill test also guards the barrier only probabilistically. Sled flushes in the background
//! (every 500 ms by default), so without the barrier an acknowledged write is lost only if the
//! kill lands before that tick. Here the kill follows the acknowledgment within milliseconds, and
//! removing the barrier failed it every time on icn-dev, but a slow enough runner could let the
//! mutant through. The recording-backend tests above are the timing-independent guard.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use icn_core::authority_facts::{DurableStore, SledAuthorityFactStore, AUTHORITY_FACT_PREFIX};
use icn_identity::authority_log::{
    authorize_event, derive, rehydrate, AuthorityFactStore, AuthorityStore, AuthorityView,
    CapabilitySet, ContextNonce, ContinuityRoot, DeviceCapability, FactStoreError, PersistedFact,
    PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

// ── fixtures: deterministic, so a child process and its parent build identical events ──────────

struct Fixture {
    root: ContinuityRoot,
    subject: SubjectId,
    inception: SignedAuthorityEvent,
}

fn fixture(seed: u8) -> Fixture {
    let root = ContinuityRoot::new([seed; 32], ContextNonce::from_bytes([seed ^ 0xa5; 32]), 4);
    let inception = root.incept().expect("inception must construct");
    let subject = inception.body.subject();
    Fixture {
        root,
        subject,
        inception,
    }
}

fn device(seed: u8) -> PrincipalKey {
    let key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
    PrincipalKey::try_from_verifying_key(key.verifying_key())
        .expect("a real verifying key must convert")
}

fn authorize(
    f: &Fixture,
    position: u64,
    prev: icn_identity::authority_log::EventId,
    dev: u8,
) -> SignedAuthorityEvent {
    authorize_event(
        &f.root.authority_signing_key(0),
        f.subject,
        position,
        prev,
        device(dev),
        CapabilitySet::new([DeviceCapability::Sign]),
        None,
    )
}

/// A non-trivial history: inception plus two authorizations, so `derive` answers `Live`.
fn history(f: &Fixture) -> Vec<SignedAuthorityEvent> {
    let a1 = authorize(f, 1, f.inception.body.event_id(), 10);
    let a2 = authorize(f, 2, a1.body.event_id(), 11);
    vec![f.inception.clone(), a1, a2]
}

// ── the deterministic boundary ───────────────────────────────────────────────────────────────

/// A durable-claiming backend that records each call and can fail `put` or its barrier on demand.
///
/// It implements [`DurableStore`] only so the adapter will accept it; what it proves is the
/// adapter's call discipline, not any storage property.
#[derive(Default)]
struct RecordingStore {
    rows: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    calls: Mutex<Vec<&'static str>>,
    fail_put: Mutex<bool>,
    fail_sync: Mutex<bool>,
}

impl RecordingStore {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
    fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }
    fn fail_sync(&self, fail: bool) {
        *self.fail_sync.lock().unwrap() = fail;
    }
    fn fail_put(&self, fail: bool) {
        *self.fail_put.lock().unwrap() = fail;
    }
}

impl Store for RecordingStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.rows.lock().unwrap().get(key).cloned())
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push("put");
        if *self.fail_put.lock().unwrap() {
            anyhow::bail!("injected put failure");
        }
        self.rows
            .lock()
            .unwrap()
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.rows.lock().unwrap().remove(key);
        Ok(())
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        for key in keys {
            rows.remove(key);
        }
        Ok(())
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }
    fn get_replica_metadata(&self, _: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
        Ok(None)
    }
    fn put_replica_metadata(&self, _: &ReplicaMetadata) -> anyhow::Result<()> {
        Ok(())
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
        Ok(Vec::new())
    }
}

impl DurableStore for RecordingStore {
    fn sync_durable(&self) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push("sync");
        if *self.fail_sync.lock().unwrap() {
            anyhow::bail!("injected sync failure");
        }
        Ok(())
    }
}

fn adapter_over(store: &Arc<RecordingStore>) -> SledAuthorityFactStore {
    SledAuthorityFactStore::new(Arc::clone(store) as Arc<dyn DurableStore>)
}

#[test]
fn a_fact_is_acknowledged_only_after_the_flush_that_follows_its_write() {
    let backend = Arc::new(RecordingStore::default());
    let facts = adapter_over(&backend);
    let f = fixture(3);

    for event in history(&f) {
        backend.clear_calls();
        facts
            .put_fact(&PersistedFact::from_event(&event))
            .expect("a healthy backend must acknowledge");
        assert_eq!(
            backend.calls(),
            ["put", "sync"],
            "each acknowledgment must come after exactly one write and then one flush"
        );
    }
}

#[test]
fn a_failed_flush_is_a_failed_write_and_a_retry_is_safe() {
    let backend = Arc::new(RecordingStore::default());
    let facts = adapter_over(&backend);
    let f = fixture(4);
    let fact = PersistedFact::from_event(&f.inception);

    backend.fail_sync(true);
    let refused = facts
        .put_fact(&fact)
        .expect_err("a failed flush must not acknowledge");
    assert!(
        matches!(&refused, FactStoreError::Backend(why) if why.contains("injected sync failure")),
        "the flush failure must be what the caller sees: {refused:?}"
    );
    assert_eq!(backend.calls(), ["put", "sync"]);

    // Not acknowledged, so the caller writes it again once the medium recovers. The record's key
    // is its identity, so the retry leaves one record, not two.
    backend.fail_sync(false);
    facts.put_fact(&fact).expect("the retry must acknowledge");
    assert_eq!(backend.scan(AUTHORITY_FACT_PREFIX).unwrap().len(), 1);
    let rehydrated = rehydrate(&facts).expect("the retried record must rehydrate");
    assert_eq!(rehydrated.body_count(), 1);
}

#[test]
fn a_failed_write_is_neither_flushed_nor_acknowledged() {
    let backend = Arc::new(RecordingStore::default());
    let facts = adapter_over(&backend);
    let f = fixture(5);

    backend.fail_put(true);
    let refused = facts
        .put_fact(&PersistedFact::from_event(&f.inception))
        .expect_err("a failed write must not acknowledge");
    assert!(
        matches!(&refused, FactStoreError::Backend(why) if why.contains("injected put failure")),
        "{refused:?}"
    );
    assert_eq!(
        backend.calls(),
        ["put"],
        "nothing to flush after a failed write"
    );
    assert!(backend.scan(AUTHORITY_FACT_PREFIX).unwrap().is_empty());
}

// ── the process that dies ────────────────────────────────────────────────────────────────────

/// Set by the crash test for its child; unset, the child half below does nothing.
const CHILD_DIR: &str = "ICN_N1D_CRASH_CHILD_DIR";
const CHILD_SEED: u8 = 9;
/// What the child prints once every fact is acknowledged, followed by how many.
///
/// Looked for anywhere in a line, not at its start: libtest prints `test <name> ... ` without a
/// newline before the test body runs, so the child's first line of output begins with that.
const CHILD_ACK: &str = "N1D-CHILD-ACKNOWLEDGED ";
/// How long the parent waits for the acknowledgment before killing the child and failing, so a
/// child that never acknowledges fails the test instead of hanging it.
const CHILD_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

/// The child half of [`acknowledged_facts_survive_the_writer_being_killed`]. Run on its own (no
/// [`CHILD_DIR`]) it returns at once.
///
/// Persists the history through the real sled-backed adapter, prints the acknowledgment, then
/// waits to be killed. It never returns from that wait, so no handle is ever dropped and sled's
/// drop-time flush cannot run.
#[test]
fn crash_child_persists_then_waits_to_be_killed() {
    let Ok(dir) = std::env::var(CHILD_DIR) else {
        return;
    };
    let sled = SledStore::open(&dir).expect("child: sled must open");
    let facts = SledAuthorityFactStore::new(Arc::new(sled) as Arc<dyn DurableStore>);
    let events = history(&fixture(CHILD_SEED));
    for event in &events {
        facts
            .put_fact(&PersistedFact::from_event(event))
            .expect("child: persisting must succeed");
    }
    {
        use std::io::Write as _;
        let mut out = std::io::stdout().lock();
        writeln!(out, "{CHILD_ACK}{}", events.len()).unwrap();
        out.flush().unwrap();
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

#[cfg(unix)]
#[test]
fn acknowledged_facts_survive_the_writer_being_killed() {
    use std::io::BufRead as _;
    use std::os::unix::process::ExitStatusExt as _;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("authority-facts");

    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "crash_child_persists_then_waits_to_be_killed",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_DIR, &path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the child must start");

    // Read the child's output on its own thread, so waiting for the acknowledgment has a deadline.
    let stdout = child.stdout.take().unwrap();
    let (ack_tx, ack_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { return };
            if let Some((_, count)) = line.split_once(CHILD_ACK) {
                let _ = ack_tx.send(count.trim().parse::<usize>());
                return;
            }
        }
    });

    // Kill the child the moment it has acknowledged, before anything else can run in it. On
    // timeout or an early exit, kill it all the same so nothing is left running, then fail.
    let acked = ack_rx.recv_timeout(CHILD_DEADLINE);
    child.kill().expect("SIGKILL the child");
    let status = child.wait().expect("reap the child");
    let acked = acked
        .expect("the child must acknowledge before the deadline")
        .expect("the acknowledgment must carry a count");
    assert_eq!(
        status.signal(),
        Some(9),
        "the child must have died by SIGKILL, not exited: {status:?}"
    );

    let events = history(&fixture(CHILD_SEED));
    assert_eq!(acked, events.len());
    let mut expected = AuthorityStore::new();
    expected.ingest_all(&events);

    let reopened = SledAuthorityFactStore::new(Arc::new(
        SledStore::open(&path).expect("sled must reopen after the kill"),
    ) as Arc<dyn DurableStore>);
    let after = rehydrate(&reopened).expect("rehydration after the kill must succeed");

    assert_eq!(
        after.bodies(),
        expected.bodies(),
        "every acknowledged body must survive"
    );
    assert_eq!(
        after.witnesses(),
        expected.witnesses(),
        "every acknowledged witness must survive"
    );
    let subject = fixture(CHILD_SEED).subject;
    assert_eq!(derive(subject, &after), derive(subject, &expected));
    assert_eq!(after.body_count(), 3);
    assert!(
        matches!(derive(subject, &after), AuthorityView::Live { .. }),
        "the derived state must be non-trivial, or the equalities above prove nothing"
    );
}
