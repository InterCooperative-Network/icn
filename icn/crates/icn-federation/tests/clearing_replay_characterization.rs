//! Characterization of `ClearingManager` replay, persistence and partial-failure behaviour.
//!
//! Test-first evidence for research findings NB-042, NB-043, NB-045, NB-046 and NB-051 (the
//! settlement-bookkeeping part), and the regression tests for the parts since repaired.
//!
//! # How to read this file
//!
//! * **Controls** run in normal CI and must pass. They prove the fixtures (real `ClearingManager`,
//!   real sled store, real reopen, deterministic fault injection) do what the RED tests rely on.
//! * **RED contracts** assert the invariant the system *should* hold and are `#[ignore]`d with a
//!   reason of the form `RED NB-0xx: …`, because current `main` violates them. Each one documents
//!   the observed value. Run them explicitly:
//!
//!   ```text
//!   cargo test -p icn-federation --test clearing_replay_characterization -- --ignored
//!   ```
//!
//!   A RED test that starts passing means the behaviour changed: re-classify it, do not delete it.
//! * **Repaired contracts** were RED contracts. They keep their name and invariant, run in normal
//!   CI, and say in their doc comment what closed them. `ClearingManager::confirm_transfer` now
//!   books a transfer with one write of its agreement's position record and decides under the
//!   manager's lock from durable state; that repairs NB-042, the settled re-booking part of NB-051
//!   and the confirmation half of NB-046.
//!
//! # Where each invariant comes from
//!
//! * Durability (NB-045, NB-046): ADR-0012 records clearing state as "Durable" for both the gateway
//!   store and the supervisor store, and `ClearingManager::terminate_agreement`'s own doc comment
//!   says it "removes it from the durable store".
//! * Counting an obligation once (NB-042, NB-043, NB-051): technical invariants of this component's
//!   own model — a transfer id is the unique key of its durable row (`TRANSFER_PREFIX` + id), and
//!   `TransferStatus` moves `Pending → Confirmed → Settled`. No domain owner has yet stated an
//!   idempotency rule for replays; where an expectation depends on such a rule, the test asserts
//!   only internal consistency and the open question is named.
//!
//! # Deliberately out of scope
//!
//! * NB-041 — `FederationServiceImpl::settle_clearing` clearing the position before the ledger
//!   commits. That is `icn-core` and needs a ledger stub.
//! * NB-044 — the imbalance-limit admission rule. Whether the limit bounds pending exposure or
//!   confirmed net is an owner decision; nothing here exercises `max_imbalance`.
//! * Finality, dispute handling and netting.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use icn_federation::{
    BilateralClearingAgreement, ClearingManager, ClearingPosition, CrossCoopTransfer,
    NettingResult, Obligation, SettlementInterval, TransferStatus,
};
use icn_identity::{Did, KeyPair};
use icn_store::{ContentHash, ReplicaMetadata, SledStore, Store};

// ── fixture ──────────────────────────────────────────────────────────────────────────────────

const OWN: &str = "coop-a";
const PARTNER: &str = "coop-b";
const AGREEMENT: &str = "agr-a-b";
const CURRENCY: &str = "CREDIT";
const AMOUNT: i64 = 4;

/// Durable key layout, copied from `clearing_manager.rs` (`TRANSFER_PREFIX`, `POSITION_PREFIX`).
/// Used only to *observe* what reached the store; the manager remains the only writer.
const TRANSFER_KEY_PREFIX: &[u8] = b"federation/clearing/transfers/";
const POSITION_KEY_PREFIX: &[u8] = b"federation/clearing/positions/";

fn did() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

fn agreement() -> BilateralClearingAgreement {
    let mut agreement = BilateralClearingAgreement::new(
        AGREEMENT.to_string(),
        OWN.to_string(),
        did(),
        PARTNER.to_string(),
        did(),
    )
    .with_rate(CURRENCY, CURRENCY, 1.0);
    // Manual: no scheduled settlement can run behind a test's back.
    agreement.settlement_interval = SettlementInterval::Manual;
    agreement
}

/// A transfer from this cooperative to its partner. `confirm_transfer` books it as
/// `coop_a_owes_b += dest_amount`, so the observable is `position.coop_a_owes_b`.
fn transfer(id: &str, amount: i64) -> CrossCoopTransfer {
    CrossCoopTransfer::new(
        id.to_string(),
        OWN.to_string(),
        did(),
        PARTNER.to_string(),
        did(),
        amount,
        CURRENCY.to_string(),
        amount,
        CURRENCY.to_string(),
    )
}

/// A sled directory that outlives any one manager, so "restart" is a real close and reopen of
/// the same on-disk store, not a second view of one open handle.
struct Node {
    dir: tempfile::TempDir,
}

impl Node {
    fn new() -> Self {
        Self {
            dir: tempfile::TempDir::new().unwrap(),
        }
    }

    fn open_store(&self) -> Arc<SledStore> {
        Arc::new(SledStore::open(self.dir.path()).unwrap())
    }

    /// Construct a manager over a freshly opened store. Every previous manager and store handle
    /// must already be dropped (sled holds an exclusive lock on the directory).
    fn manager(&self) -> (ClearingManager, Arc<SledStore>) {
        let store = self.open_store();
        let manager = ClearingManager::new(store.clone() as Arc<dyn Store>, OWN.to_string())
            .expect("manager must load from the store");
        (manager, store)
    }
}

/// Close everything, then reopen the same directory: `ClearingManager::new` → `load_from_store`.
fn restart(
    node: &Node,
    manager: ClearingManager,
    store: Arc<SledStore>,
) -> (ClearingManager, Arc<SledStore>) {
    store.flush().unwrap();
    drop(manager);
    assert_eq!(
        Arc::strong_count(&store),
        1,
        "fixture: a store handle outlived its manager; reopening would contend for the sled lock"
    );
    drop(store);
    node.manager()
}

fn owed(manager: &ClearingManager) -> i64 {
    manager.calculate_position(AGREEMENT).unwrap().coop_a_owes_b
}

fn pending_ids(manager: &ClearingManager) -> Vec<String> {
    manager
        .calculate_position(AGREEMENT)
        .unwrap()
        .pending_transfers
        .iter()
        .map(|t| t.id.clone())
        .collect()
}

/// The transfer row as persisted, independent of the manager's caches.
fn stored_transfer(store: &dyn Store, id: &str) -> Option<CrossCoopTransfer> {
    let mut key = TRANSFER_KEY_PREFIX.to_vec();
    key.extend(id.as_bytes());
    store
        .get(&key)
        .unwrap()
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
}

/// What the agreement's persisted position says is owed, independent of the manager's caches.
fn stored_owed(store: &dyn Store) -> i64 {
    let mut key = POSITION_KEY_PREFIX.to_vec();
    key.extend(AGREEMENT.as_bytes());
    let bytes = store
        .get(&key)
        .unwrap()
        .expect("the agreement's position is persisted");
    serde_json::from_slice::<ClearingPosition>(&bytes)
        .unwrap()
        .coop_a_owes_b
}

// ── deterministic fault injection ────────────────────────────────────────────────────────────

/// Delegates every operation to a real `SledStore`, except that it can be armed to fail the
/// *next* `put` whose key starts with a given prefix. The failure is a returned error, exactly as
/// a real write failure would surface; nothing is silently dropped, and the armed write is not
/// performed.
struct FaultyStore {
    inner: Arc<SledStore>,
    armed: Mutex<Option<Vec<u8>>>,
    failed: AtomicUsize,
}

impl FaultyStore {
    fn new(inner: Arc<SledStore>) -> Self {
        Self {
            inner,
            armed: Mutex::new(None),
            failed: AtomicUsize::new(0),
        }
    }

    fn fail_next_put_under(&self, prefix: &[u8]) {
        *self.armed.lock().unwrap() = Some(prefix.to_vec());
    }

    fn failures(&self) -> usize {
        self.failed.load(Ordering::SeqCst)
    }
}

impl Store for FaultyStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        let mut armed = self.armed.lock().unwrap();
        if armed
            .as_deref()
            .is_some_and(|prefix| key.starts_with(prefix))
        {
            *armed = None;
            self.failed.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("injected write failure (characterization fault)");
        }
        drop(armed);
        self.inner.put(key, value)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
        self.inner.list_replica_hashes()
    }
}

/// A manager over a fault-injecting view of a real store in `node`, with the agreement created.
fn faulty_manager(node: &Node) -> (ClearingManager, Arc<FaultyStore>, Arc<SledStore>) {
    let sled = node.open_store();
    let faulty = Arc::new(FaultyStore::new(sled.clone()));
    let manager = ClearingManager::new(faulty.clone() as Arc<dyn Store>, OWN.to_string()).unwrap();
    manager.create_agreement(agreement()).unwrap();
    (manager, faulty, sled)
}

/// Close a fault-injected manager and reopen the directory without fault injection.
fn reopen_clean(
    node: &Node,
    manager: ClearingManager,
    faulty: Arc<FaultyStore>,
    sled: Arc<SledStore>,
) -> (ClearingManager, Arc<SledStore>) {
    sled.flush().unwrap();
    drop(manager);
    drop(faulty);
    assert_eq!(Arc::strong_count(&sled), 1, "fixture: leaked store handle");
    drop(sled);
    node.manager()
}

// ── forced stale read (#2904 F1) ─────────────────────────────────────────────────────────────

/// How long a contender that has read a clearing record waits for the other contender to read
/// one too, counted from when the other *started* its call, before concluding that the
/// implementation serializes before its first read. A non-serializing implementation never waits
/// this long: its second contender reaches the store without blocking.
const SERIALIZATION_GRACE: Duration = Duration::from_secs(10);

thread_local! {
    /// Which contender (0 or 1) the current thread is. `None` on the test's own thread, whose
    /// setup calls pass through the store untouched.
    static CONTENDER: Cell<Option<usize>> = const { Cell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// Logged *before* the read is performed.
    Read,
    /// Logged *after* the write has completed.
    Write,
}

#[derive(Debug, Clone, Copy)]
struct Logged {
    contender: usize,
    op: Op,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interleaving {
    /// Both contenders read a clearing record before either could write one.
    Overlapped,
    /// The second contender could not reach the store while the first was inside its call.
    Serialized,
}

#[derive(Default)]
struct RendezvousState {
    started: [Option<Instant>; 2],
    has_read: [bool; 2],
    outcome: Option<Interleaving>,
    log: Vec<Logged>,
}

#[derive(Default)]
struct Rendezvous {
    state: Mutex<RendezvousState>,
    changed: Condvar,
}

impl Rendezvous {
    fn start(&self, contender: usize) {
        self.state.lock().unwrap().started[contender] = Some(Instant::now());
        self.changed.notify_all();
    }

    fn log(&self, contender: usize, op: Op) {
        self.state
            .lock()
            .unwrap()
            .log
            .push(Logged { contender, op });
    }

    /// Runs after `contender` has read a clearing record and before the value is returned to it.
    /// On its first such read, hold it until the other contender has read too, so both act on
    /// what they read before either can write; or until the other, having started, has been unable
    /// to read for [`SERIALIZATION_GRACE`], which means the implementation serializes first.
    fn after_read(&self, contender: usize) {
        let other = 1 - contender;
        let mut state = self.state.lock().unwrap();
        if state.has_read[contender] {
            return;
        }
        state.has_read[contender] = true;
        self.changed.notify_all();
        loop {
            if state.outcome.is_some() {
                return;
            }
            if state.has_read[other] {
                state.outcome = Some(Interleaving::Overlapped);
                self.changed.notify_all();
                return;
            }
            state = match state.started[other] {
                Some(started) if started.elapsed() >= SERIALIZATION_GRACE => {
                    state.outcome = Some(Interleaving::Serialized);
                    self.changed.notify_all();
                    return;
                }
                Some(started) => {
                    let remaining = SERIALIZATION_GRACE.saturating_sub(started.elapsed());
                    self.changed.wait_timeout(state, remaining).unwrap().0
                }
                None => self.changed.wait(state).unwrap(),
            };
        }
    }

    fn outcome(&self) -> (Interleaving, Vec<Logged>) {
        let state = self.state.lock().unwrap();
        (
            state
                .outcome
                .expect("both contenders read a clearing record"),
            state.log.clone(),
        )
    }
}

fn is_clearing_record(key: &[u8]) -> bool {
    key.starts_with(TRANSFER_KEY_PREFIX) || key.starts_with(POSITION_KEY_PREFIX)
}

/// Delegates every operation to a real `SledStore`. On a contender's thread it logs each read and
/// write of a clearing record and runs the [`Rendezvous`] after each read.
struct RendezvousStore {
    inner: Arc<SledStore>,
    rendezvous: Arc<Rendezvous>,
}

impl RendezvousStore {
    fn contender_for(key: &[u8]) -> Option<usize> {
        CONTENDER
            .with(Cell::get)
            .filter(|_| is_clearing_record(key))
    }

    fn wrote(&self, key: &[u8]) {
        if let Some(contender) = Self::contender_for(key) {
            self.rendezvous.log(contender, Op::Write);
        }
    }
}

impl Store for RendezvousStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        let contender = Self::contender_for(key);
        if let Some(contender) = contender {
            self.rendezvous.log(contender, Op::Read);
        }
        let value = self.inner.get(key)?;
        if let Some(contender) = contender {
            self.rendezvous.after_read(contender);
        }
        Ok(value)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        self.inner.put(key, value)?;
        self.wrote(key);
        Ok(())
    }
    fn compare_and_put(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new: &[u8],
    ) -> anyhow::Result<icn_store::CompareAndPut> {
        let outcome = self.inner.compare_and_put(key, expected, new)?;
        self.wrote(key);
        Ok(outcome)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)?;
        self.wrote(key);
        Ok(())
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)?;
        for key in keys {
            self.wrote(key);
        }
        Ok(())
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let contender = Self::contender_for(prefix);
        if let Some(contender) = contender {
            self.rendezvous.log(contender, Op::Read);
        }
        let rows = self.inner.scan(prefix)?;
        if let Some(contender) = contender {
            self.rendezvous.after_read(contender);
        }
        Ok(rows)
    }
    fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
        self.inner.list_replica_hashes()
    }
}

// ── position-lock witness (#2906 review FOLLOW_UP 1) ────────────────────────────────────────

/// How long a contender that has read the durable position waits for the other contender to read
/// or write it. With the manager's position lock in place the other cannot reach the record, so
/// the hold expires and the run is serial. It is a bound, not a sleep: an implementation without
/// the lock lets the other contender through at once.
const POSITION_HOLD: Duration = Duration::from_secs(2);

#[derive(Default)]
struct PositionHoldState {
    /// Contender has read or written the agreement's position record.
    reached: [bool; 2],
    held: [bool; 2],
    overlapped: Option<bool>,
}

/// Holds a contender *after* its first read of the position record and *before* it can write it
/// back, which is inside the read → modify → write that the position lock exists to protect.
/// Unlike [`Rendezvous`], it ignores the transfer-row read that precedes the lock.
#[derive(Default)]
struct PositionHold {
    state: Mutex<PositionHoldState>,
    changed: Condvar,
}

impl PositionHold {
    fn reached(&self, contender: usize) {
        self.state.lock().unwrap().reached[contender] = true;
        self.changed.notify_all();
    }

    fn after_position_read(&self, contender: usize) {
        let other = 1 - contender;
        let mut state = self.state.lock().unwrap();
        state.reached[contender] = true;
        self.changed.notify_all();
        if std::mem::replace(&mut state.held[contender], true) {
            return;
        }
        let deadline = Instant::now() + POSITION_HOLD;
        while !state.reached[other] {
            let now = Instant::now();
            if now >= deadline {
                state.overlapped.get_or_insert(false);
                return;
            }
            state = self.changed.wait_timeout(state, deadline - now).unwrap().0;
        }
        state.overlapped.get_or_insert(true);
    }

    fn wait_until_reached(&self, contender: usize) {
        let mut state = self.state.lock().unwrap();
        while !state.reached[contender] {
            state = self.changed.wait(state).unwrap();
        }
    }

    fn overlapped(&self) -> Option<bool> {
        self.state.lock().unwrap().overlapped
    }
}

/// Delegates every operation to a real `SledStore`, running [`PositionHold`] around a contender's
/// reads and writes of a position record.
struct PositionHoldStore {
    inner: Arc<SledStore>,
    hold: Arc<PositionHold>,
}

impl PositionHoldStore {
    fn contender_for(key: &[u8]) -> Option<usize> {
        CONTENDER
            .with(Cell::get)
            .filter(|_| key.starts_with(POSITION_KEY_PREFIX))
    }

    fn wrote(&self, key: &[u8]) {
        if let Some(contender) = Self::contender_for(key) {
            self.hold.reached(contender);
        }
    }
}

impl Store for PositionHoldStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        let value = self.inner.get(key)?;
        if let Some(contender) = Self::contender_for(key) {
            self.hold.after_position_read(contender);
        }
        Ok(value)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        self.inner.put(key, value)?;
        self.wrote(key);
        Ok(())
    }
    fn compare_and_put(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new: &[u8],
    ) -> anyhow::Result<icn_store::CompareAndPut> {
        let outcome = self.inner.compare_and_put(key, expected, new)?;
        self.wrote(key);
        Ok(outcome)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)?;
        self.wrote(key);
        Ok(())
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)?;
        for key in keys {
            self.wrote(key);
        }
        Ok(())
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn get_replica_metadata(&self, h: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
        self.inner.list_replica_hashes()
    }
}

fn position_hold_manager(
    node: &Node,
) -> (
    ClearingManager,
    Arc<PositionHoldStore>,
    Arc<SledStore>,
    Arc<PositionHold>,
) {
    let sled = node.open_store();
    let hold = Arc::new(PositionHold::default());
    let store = Arc::new(PositionHoldStore {
        inner: sled.clone(),
        hold: hold.clone(),
    });
    let manager = ClearingManager::new(store.clone() as Arc<dyn Store>, OWN.to_string()).unwrap();
    manager.create_agreement(agreement()).unwrap();
    (manager, store, sled, hold)
}

fn stored_entry_statuses(store: &dyn Store, id: &str) -> Vec<TransferStatus> {
    let mut key = POSITION_KEY_PREFIX.to_vec();
    key.extend(AGREEMENT.as_bytes());
    let bytes = store.get(&key).unwrap().expect("position is persisted");
    serde_json::from_slice::<ClearingPosition>(&bytes)
        .unwrap()
        .pending_transfers
        .iter()
        .filter(|t| t.id == id)
        .map(|t| t.status)
        .collect()
}

/// Two *different* obligations (4 and 9) on one agreement, confirmed concurrently on one manager.
/// Confirmation is a durable read → modify → write of the whole position record, so without the
/// position lock the second writer replays a stale base and the first obligation is lost while
/// both calls return `Ok`. A same-transfer race cannot show this: its two stale writes coincide.
/// The assertions are economic only, so the bounded hold cannot turn a lost update into a pass.
#[test]
fn nb042_concurrent_confirmations_of_different_transfers_keep_both_obligations() {
    let node = Node::new();
    let (manager, store, sled, hold) = position_hold_manager(&node);
    manager.propose_transfer(transfer("t-1", 4)).unwrap();
    manager.propose_transfer(transfer("t-2", 9)).unwrap();

    let results = std::thread::scope(|s| {
        let handles: Vec<_> = [(0usize, "t-1"), (1usize, "t-2")]
            .into_iter()
            .map(|(contender, id)| {
                let manager = &manager;
                s.spawn(move || {
                    CONTENDER.with(|cell| cell.set(Some(contender)));
                    manager.confirm_transfer(id).map_err(|e| e.to_string())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    eprintln!(
        "position-lock witness (confirm ‖ confirm): overlapped={:?} results={results:?}",
        hold.overlapped()
    );

    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(owed(&manager), 13, "live position lost an obligation");
    assert_eq!(
        stored_owed(sled.as_ref()),
        13,
        "durable position lost an obligation"
    );
    for id in ["t-1", "t-2"] {
        assert_eq!(
            stored_entry_statuses(sled.as_ref(), id),
            vec![TransferStatus::Confirmed],
            "{id} must be booked exactly once"
        );
    }

    sled.flush().unwrap();
    drop(manager);
    drop(store);
    assert_eq!(Arc::strong_count(&sled), 1, "fixture: leaked store handle");
    drop(sled);
    let (manager, _store) = node.manager();
    assert_eq!(owed(&manager), 13, "and durably, after a restart");
}

/// Confirmation of t-2 races settlement of already-confirmed t-1 on one manager. Whatever order
/// the lock imposes, what settlement reports discharged plus what is still owed must equal the two
/// unique obligations, and settled t-1 must not be owed again. Without the lock the confirmation
/// writes back the pre-settlement record and re-creates the discharged obligation.
#[test]
fn nb042_confirmation_racing_settlement_conserves_obligations() {
    let node = Node::new();
    let (manager, _store, sled, hold) = position_hold_manager(&node);
    manager.propose_transfer(transfer("t-1", 4)).unwrap();
    manager.confirm_transfer("t-1").unwrap();
    manager.propose_transfer(transfer("t-2", 9)).unwrap();

    let (confirmed, settled) = std::thread::scope(|s| {
        let (manager, hold) = (&manager, &hold);
        let confirm = s.spawn(move || {
            CONTENDER.with(|cell| cell.set(Some(0)));
            manager.confirm_transfer("t-2").map_err(|e| e.to_string())
        });
        let settle = s.spawn(move || {
            hold.wait_until_reached(0);
            CONTENDER.with(|cell| cell.set(Some(1)));
            manager
                .trigger_settlement(AGREEMENT)
                .map_err(|e| e.to_string())
        });
        (confirm.join().unwrap(), settle.join().unwrap())
    });
    eprintln!(
        "position-lock witness (confirm ‖ settle): overlapped={:?} confirm={confirmed:?}",
        hold.overlapped()
    );

    assert!(confirmed.is_ok(), "{confirmed:?}");
    let report = settled.unwrap();
    assert_eq!(
        report.coop_a_owed + stored_owed(sled.as_ref()),
        13,
        "settled + still owed must equal the two unique obligations"
    );
    assert!(
        stored_entry_statuses(sled.as_ref(), "t-1").is_empty(),
        "settled t-1 must not be tracked, and so owed, again"
    );
}

// ── controls (must pass) ─────────────────────────────────────────────────────────────────────

#[test]
fn control_one_confirmation_books_the_amount_once() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    manager.confirm_transfer("t-1").unwrap();

    assert_eq!(owed(&manager), AMOUNT);
    assert_eq!(pending_ids(&manager), vec!["t-1".to_string()]);
    assert_eq!(
        stored_transfer(store.as_ref(), "t-1").unwrap().status,
        TransferStatus::Confirmed
    );
}

#[test]
fn control_agreement_and_position_survive_a_real_reopen() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    manager.confirm_transfer("t-1").unwrap();

    let (manager, _store) = restart(&node, manager, store);

    assert!(manager.get_agreement(AGREEMENT).unwrap().is_some());
    assert_eq!(owed(&manager), AMOUNT);
    assert_eq!(pending_ids(&manager), vec!["t-1".to_string()]);
}

/// Settlement discharges confirmed transfers only; an unconfirmed one stays owed-to-be-confirmed,
/// before and after a reopen.
#[test]
fn control_settlement_keeps_unconfirmed_transfers_pending() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager
        .propose_transfer(transfer("t-confirmed", AMOUNT))
        .unwrap();
    manager.confirm_transfer("t-confirmed").unwrap();
    manager.propose_transfer(transfer("t-pending", 7)).unwrap();

    let report = manager.trigger_settlement(AGREEMENT).unwrap();
    assert_eq!(report.transfers_settled, 1);
    assert_eq!(report.coop_a_owed, AMOUNT);
    assert_eq!(owed(&manager), 0);
    assert_eq!(pending_ids(&manager), vec!["t-pending".to_string()]);

    let (manager, _store) = restart(&node, manager, store);
    assert_eq!(owed(&manager), 0);
    assert_eq!(pending_ids(&manager), vec!["t-pending".to_string()]);
}

/// The fault injector fails exactly the armed write, surfaces it as an error, and nothing else.
#[test]
fn control_fault_injection_fails_exactly_the_armed_write() {
    let node = Node::new();
    let (manager, faulty, _sled) = faulty_manager(&node);

    faulty.fail_next_put_under(TRANSFER_KEY_PREFIX);
    let err = manager
        .propose_transfer(transfer("t-1", AMOUNT))
        .unwrap_err();
    assert!(err.to_string().contains("injected write failure"), "{err}");
    assert_eq!(faulty.failures(), 1);
    // The transfer row was the first write, so nothing of this proposal reached the store.
    assert!(stored_transfer(faulty.as_ref(), "t-1").is_none());

    // Disarmed after one failure: the same call now succeeds.
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    assert_eq!(faulty.failures(), 1);
}

// ── A. duplicate confirmation (NB-042) ───────────────────────────────────────────────────────

/// REPAIRED (was RED NB-042, observed 8: `confirm_transfer` set `Confirmed` unconditionally and
/// added `dest_amount` again). It now books only if the agreement's durable position does not
/// already record the transfer as booked, so the second call changes nothing.
#[test]
fn nb042_repeated_confirmation_books_the_amount_once() {
    let node = Node::new();
    let (manager, _store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    manager.confirm_transfer("t-1").unwrap();
    let _ = manager.confirm_transfer("t-1");

    assert_eq!(
        owed(&manager),
        AMOUNT,
        "one transfer of {AMOUNT} confirmed twice must still owe {AMOUNT}"
    );
}

/// REPAIRED (was RED NB-042, observed 8), and rewritten because the old version could not tell a
/// real repair from a racy one (#2904 F1). It released both threads at a barrier *before*
/// `confirm_transfer`, so a guard that checks the transfer row's status without serializing
/// usually passed it, although both calls could still read `Pending` and both book.
///
/// This version forces the stale read. Each contender's first read of a clearing record goes
/// through [`RendezvousStore`], which performs the read and then holds the contender until the
/// other has read too. Both hold a `Pending` transfer row before either can write anything;
/// whatever each does next, the amount must be booked once, live and after a restart.
///
/// An implementation may instead serialize *before* its first read. Then the overlap is
/// impossible: the second contender is blocked while the first waits for it. The rendezvous does
/// not hang or guess. Once the other contender has started its call and still has not read within
/// [`SERIALIZATION_GRACE`], it records `Serialized` and releases the first, and the test checks
/// from the log that the second's first read began after the first's last write completed. A
/// non-serializing guard never blocks before its read, so it is forced into the overlap and
/// caught; it could escape only if a runnable thread were not scheduled for the whole grace
/// period. That would let this test pass an unsafe implementation; it can never fail a safe one.
#[test]
fn nb042_concurrent_confirmations_book_the_amount_once() {
    let node = Node::new();
    let sled = node.open_store();
    let rendezvous = Arc::new(Rendezvous::default());
    let store = Arc::new(RendezvousStore {
        inner: sled.clone(),
        rendezvous: rendezvous.clone(),
    });
    let manager = ClearingManager::new(store.clone() as Arc<dyn Store>, OWN.to_string()).unwrap();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();

    std::thread::scope(|s| {
        for contender in 0..2 {
            let (manager, rendezvous) = (&manager, &rendezvous);
            s.spawn(move || {
                CONTENDER.with(|cell| cell.set(Some(contender)));
                rendezvous.start(contender);
                let _ = manager.confirm_transfer("t-1");
            });
        }
    });

    let (interleaving, log) = rendezvous.outcome();
    eprintln!("nb042 concurrent witness: {interleaving:?}; log {log:?}");
    match interleaving {
        Interleaving::Overlapped => {
            let first_write = log
                .iter()
                .position(|entry| entry.op == Op::Write)
                .expect("one contender books the transfer");
            let mut readers: Vec<usize> = log[..first_write]
                .iter()
                .filter(|entry| entry.op == Op::Read)
                .map(|entry| entry.contender)
                .collect();
            readers.sort_unstable();
            readers.dedup();
            assert_eq!(
                readers,
                vec![0, 1],
                "both contenders read before either wrote: {log:?}"
            );
        }
        Interleaving::Serialized => {
            let first = log[0].contender;
            let second = 1 - first;
            let last_write_of_first = log
                .iter()
                .rposition(|entry| entry.contender == first && entry.op == Op::Write);
            let first_read_of_second = log
                .iter()
                .position(|entry| entry.contender == second && entry.op == Op::Read);
            assert!(
                matches!(
                    (last_write_of_first, first_read_of_second),
                    (Some(write), Some(read)) if read > write
                ),
                "a serializing implementation must finish one call before the other reads: {log:?}"
            );
        }
    }

    assert_eq!(
        owed(&manager),
        AMOUNT,
        "two concurrent confirmations of one transfer must owe {AMOUNT} once ({interleaving:?})"
    );

    sled.flush().unwrap();
    drop(manager);
    drop(store);
    assert_eq!(Arc::strong_count(&sled), 1, "fixture: leaked store handle");
    drop(sled);
    let (manager, store) = node.manager();
    assert_eq!(owed(&manager), AMOUNT, "and durably, after a restart");
    assert_eq!(
        stored_transfer(store.as_ref(), "t-1").unwrap().status,
        TransferStatus::Confirmed
    );
}

/// REPAIRED (was RED NB-042, observed 8). The replay arrives after a restart: the durable row
/// already says `Confirmed`, and the durable position already records the booking.
#[test]
fn nb042_confirmation_replayed_after_restart_books_the_amount_once() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    manager.confirm_transfer("t-1").unwrap();

    let (manager, store) = restart(&node, manager, store);
    assert_eq!(
        stored_transfer(store.as_ref(), "t-1").unwrap().status,
        TransferStatus::Confirmed,
        "precondition: the durable row already records the confirmation"
    );
    let _ = manager.confirm_transfer("t-1");

    assert_eq!(owed(&manager), AMOUNT);
}

/// The booking is the position write; the transfer row is written after it. Fail the row write:
/// the amount is booked, live and durably, while the row still says `Pending`. A retry must not
/// book it again, and must repair the row. A guard that trusted the row's `Pending` would book it
/// twice here; one that stopped at an error would leave the row stale.
#[test]
fn nb042_failed_row_write_after_booking_is_repaired_without_booking_again() {
    let node = Node::new();
    let (manager, faulty, sled) = faulty_manager(&node);
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();

    faulty.fail_next_put_under(TRANSFER_KEY_PREFIX);
    assert!(
        manager.confirm_transfer("t-1").is_err(),
        "precondition: the injected failure is reported to the caller"
    );
    assert_eq!(faulty.failures(), 1);
    assert_eq!(
        stored_transfer(faulty.as_ref(), "t-1").unwrap().status,
        TransferStatus::Pending,
        "precondition: the row write is the one that failed"
    );
    assert_eq!(owed(&manager), AMOUNT, "the position write landed: booked");
    assert_eq!(stored_owed(faulty.as_ref()), AMOUNT, "booked durably too");

    let _ = manager.confirm_transfer("t-1");

    assert_eq!(owed(&manager), AMOUNT, "a retry must not book it again");
    assert_eq!(
        stored_transfer(faulty.as_ref(), "t-1").unwrap().status,
        TransferStatus::Confirmed,
        "the retry repairs the row"
    );
    let (manager, store) = reopen_clean(&node, manager, faulty, sled);
    assert_eq!(owed(&manager), AMOUNT);
    assert_eq!(
        stored_transfer(store.as_ref(), "t-1").unwrap().status,
        TransferStatus::Confirmed
    );
}

// ── B. duplicate proposal (NB-043) ───────────────────────────────────────────────────────────

/// Byte-identical replay (one value, cloned). The store holds one row per transfer id, so the
/// position should hold one pending entry for it. Observed: two.
#[test]
#[ignore = "RED NB-043: an identical proposal replay adds a second pending entry (observed 2, expected 1); run with --ignored"]
fn nb043_identical_proposal_replay_keeps_one_pending_entry() {
    let node = Node::new();
    let (manager, _store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    let t = transfer("t-1", AMOUNT);
    manager.propose_transfer(t.clone()).unwrap();
    let _ = manager.propose_transfer(t);

    assert_eq!(
        pending_ids(&manager),
        vec!["t-1".to_string()],
        "one transfer id must appear once among pending transfers"
    );
}

/// Same transfer id, different amount. Whether the second proposal should be refused or should
/// replace the first is **not decided** by any owner, so this test does not assume either. It
/// asserts only that the durable row and the position agree about what transfer `t-1` is.
/// Observed: the row is overwritten with 9 while the position keeps the original 4 *and* adds the
/// 9 — two disagreeing records of one id.
#[test]
#[ignore = "RED NB-043: a conflicting proposal under one id leaves the row and the position disagreeing; run with --ignored"]
fn nb043_conflicting_proposal_under_one_id_keeps_row_and_position_consistent() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    let _ = manager.propose_transfer(transfer("t-1", 9));

    let row = stored_transfer(store.as_ref(), "t-1").unwrap();
    let in_position: Vec<i64> = manager
        .calculate_position(AGREEMENT)
        .unwrap()
        .pending_transfers
        .iter()
        .filter(|t| t.id == "t-1")
        .map(|t| t.dest_amount)
        .collect();

    assert_eq!(
        in_position,
        vec![row.dest_amount],
        "the position must hold exactly the transfer the durable row records"
    );
}

// ── C. termination and restart (NB-045) ──────────────────────────────────────────────────────

/// `terminate_agreement` documents that it "removes it from the durable store"; it removes only
/// the in-memory entries, and `load_from_store` reads the persisted rows back.
#[test]
#[ignore = "RED NB-045: a terminated agreement is active again after restart; run with --ignored"]
fn nb045_terminated_agreement_stays_terminated_after_restart() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.terminate_agreement(OWN, PARTNER).unwrap();
    assert!(
        manager.get_agreement(AGREEMENT).unwrap().is_none(),
        "precondition: termination takes effect in the running manager"
    );

    let (manager, _store) = restart(&node, manager, store);

    assert!(
        manager.get_agreement(AGREEMENT).unwrap().is_none(),
        "a terminated agreement must not be reconstructed as active by a restart"
    );
    assert!(
        manager
            .propose_transfer(transfer("t-after", AMOUNT))
            .is_err(),
        "no new transfer may be admitted under a terminated agreement"
    );
}

// ── D. partial-write failure (NB-046) ────────────────────────────────────────────────────────

/// `propose_transfer` writes the transfer row, then the position. Fail the position write.
/// The call reports the error (good), but the row is already durable while the position is not,
/// and the running manager's position already lists the transfer (it is mutated before the write).
#[test]
#[ignore = "RED NB-046: a failed position write during proposal leaves a durable transfer the position never records; run with --ignored"]
fn nb046_failed_position_write_during_proposal_leaves_consistent_state() {
    let node = Node::new();
    let (manager, faulty, sled) = faulty_manager(&node);

    faulty.fail_next_put_under(POSITION_KEY_PREFIX);
    assert!(
        manager.propose_transfer(transfer("t-1", AMOUNT)).is_err(),
        "precondition: the injected failure is reported to the caller"
    );
    assert_eq!(faulty.failures(), 1);

    let (manager, store) = reopen_clean(&node, manager, faulty, sled);
    let row = stored_transfer(store.as_ref(), "t-1");
    let pending = pending_ids(&manager);

    assert_eq!(
        row.is_some(),
        pending.contains(&"t-1".to_string()),
        "after reopen the durable transfer row and the durable position must agree on whether \
         t-1 exists (row present: {}, in position: {pending:?})",
        row.is_some()
    );
}

/// REPAIRED (was RED NB-046, observed: row `Confirmed`, nothing owed). `confirm_transfer` used to
/// write the row as `Confirmed` and then the position. It now books with one write of the
/// position record and writes the row only after that. Fail the position write: nothing may be
/// booked, durably or in the running manager. The live check is #2904 F2: an implementation that
/// added to its cache before the failed write would pass a reopen-only check while the live
/// manager reported the amount, and a retry would then book and persist it twice.
#[test]
fn nb046_failed_position_write_during_confirmation_leaves_consistent_state() {
    let node = Node::new();
    let (manager, faulty, sled) = faulty_manager(&node);
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();

    faulty.fail_next_put_under(POSITION_KEY_PREFIX);
    assert!(
        manager.confirm_transfer("t-1").is_err(),
        "precondition: the injected failure is reported to the caller"
    );
    assert_eq!(faulty.failures(), 1);

    assert_eq!(
        owed(&manager),
        0,
        "the running manager must not book a confirmation whose write failed"
    );
    assert_eq!(stored_owed(faulty.as_ref()), 0, "nor may the store");
    assert_eq!(
        stored_transfer(faulty.as_ref(), "t-1").unwrap().status,
        TransferStatus::Pending,
        "and the row is not marked confirmed"
    );

    // The fault is disarmed after one failure: a retry on the same manager books it once.
    manager.confirm_transfer("t-1").unwrap();
    assert_eq!(
        owed(&manager),
        AMOUNT,
        "a retry after a failed confirmation books it exactly once"
    );

    let (manager, store) = reopen_clean(&node, manager, faulty, sled);
    let durably_confirmed =
        stored_transfer(store.as_ref(), "t-1").unwrap().status == TransferStatus::Confirmed;

    assert_eq!(
        owed(&manager),
        if durably_confirmed { AMOUNT } else { 0 },
        "a transfer that is durably Confirmed must be booked in the durable position, and one \
         that is not must not be (row Confirmed: {durably_confirmed})"
    );
    assert!(durably_confirmed, "the retried confirmation is durable");
}

// ── E. settlement bookkeeping (NB-051, local part only) ──────────────────────────────────────

/// `trigger_settlement` marks confirmed transfers `Settled` only in the position's copy, drops
/// them, and zeroes the balances. The durable transfer rows still say `Confirmed`.
#[test]
#[ignore = "RED NB-051: settlement leaves the transfer row Confirmed (observed Confirmed, expected Settled); run with --ignored"]
fn nb051_settlement_records_settled_on_the_transfer_row() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    manager.confirm_transfer("t-1").unwrap();
    manager.trigger_settlement(AGREEMENT).unwrap();

    let (_manager, store) = restart(&node, manager, store);
    assert_eq!(
        stored_transfer(store.as_ref(), "t-1").unwrap().status,
        TransferStatus::Settled,
        "the durable record of a settled transfer must say so"
    );
}

/// REPAIRED (was RED NB-051/NB-042, observed 4: a transfer already discharged by settlement could
/// be confirmed again and re-entered the position). Settlement removes the position entry and the
/// row still says `Confirmed`; `confirm_transfer` reads an untracked `Confirmed` transfer as
/// booked and discharged, and books nothing. The row-status contract above stays RED: settlement
/// still does not write `Settled` to the row.
#[test]
fn nb051_settled_transfer_is_not_booked_again() {
    let node = Node::new();
    let (manager, _store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", AMOUNT)).unwrap();
    manager.confirm_transfer("t-1").unwrap();
    manager.trigger_settlement(AGREEMENT).unwrap();
    assert_eq!(owed(&manager), 0, "precondition: settlement discharged it");

    let _ = manager.confirm_transfer("t-1");

    assert_eq!(
        owed(&manager),
        0,
        "an obligation already discharged by settlement must not be booked again"
    );
}

/// #2906 late finding (Codex review thread r4624665467). Netting that cancels an agreement's
/// whole debt used to zero only the cached position and persist nothing for it, because only
/// agreements still owing after netting were written. `confirm_transfer` decides from the
/// durable record, so it read the stale pre-netting balance and booked the cancelled 4 again
/// (observed 13, `main` 9). Netting now persists every position it changes, including those
/// netted to zero.
#[test]
fn nb042_confirmation_after_full_netting_does_not_resurrect_the_cancelled_debt() {
    let node = Node::new();
    let (manager, store) = node.manager();
    manager.create_agreement(agreement()).unwrap();
    manager.propose_transfer(transfer("t-1", 4)).unwrap();
    manager.confirm_transfer("t-1").unwrap();
    manager.propose_transfer(transfer("t-2", 9)).unwrap();

    let cancelled = Obligation {
        from: OWN.to_string(),
        to: PARTNER.to_string(),
        amount: 4,
        currency: CURRENCY.to_string(),
    };
    manager
        .apply_multilateral_netting(&NettingResult {
            original: vec![cancelled],
            netted: vec![],
            cycles_canceled: vec![],
            amount_reduced: 4,
        })
        .unwrap();
    assert_eq!(owed(&manager), 0, "netting cancelled the whole debt");
    assert_eq!(
        stored_owed(store.as_ref()),
        0,
        "a position netted to zero must be persisted as zero"
    );

    manager.confirm_transfer("t-2").unwrap();
    assert_eq!(owed(&manager), 9, "only t-2 is owed after netting");
    assert_eq!(stored_owed(store.as_ref()), 9);

    let (manager, _store) = restart(&node, manager, store);
    assert_eq!(owed(&manager), 9, "and durably, after a restart");
}
