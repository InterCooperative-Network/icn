//! `DurableConsumedSet` at the storage boundary: persistence across reopen, strict record
//! validation, atomic first-writer-wins under concurrency, and each failure window between the
//! insert, the durability barrier and the acknowledgement.
//!
//! Entries are signed statements built directly here (consumption checks the writer's signature,
//! not the votes); admission-level replay and conflict across a restart are tested in
//! `icn-governance/tests/context_log.rs` with real authorizations.
//!
//! Letters refer to the slice's test matrix.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    AuthorizationV0, CheckpointedAuthorizationV0, ConsumedAuthorization, ConsumedEntry,
    ConsumedSet, ConsumedSetError, ContextRef, EffectV0, LogCheckpointV0, Signed, Verdict,
};
use icn_authz_store::{consumed_key, DurableBackend, DurableConsumedSet, KEY_PREFIX};
use icn_identity::authority_log::PrincipalKey;
use icn_store::{CompareAndPut, SledStore, Store};

const CTX: ContextRef = ContextRef([0x11; 32]);
const OP: [u8; 32] = [0x22; 32];

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pk(k: &SigningKey) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap()
}

fn effect() -> EffectV0 {
    EffectV0 {
        target: [0x33; 32],
        action: "reserve".into(),
        params_hash: [0x44; 32],
    }
}

/// A checkpointed authorization of `(ctx, op)` signed by `writer`; `seq` varies the body.
fn checkpointed(ctx: ContextRef, op: [u8; 32], writer: &SigningKey, seq: u64) -> ConsumedEntry {
    let effect = effect();
    ConsumedEntry {
        authorization: ConsumedAuthorization::Checkpointed(Signed::sign(
            CheckpointedAuthorizationV0 {
                context: ctx,
                checkpoint: LogCheckpointV0 {
                    seq,
                    head: [seq as u8; 32],
                },
                subject_matter: effect.commitment(ctx, op),
                rule_hash: [0x55; 32],
                verdict: Verdict::Pass,
                inputs: Vec::new(),
                effect,
                operation_id: op,
                not_after: 1_500,
            },
            writer,
        )),
        writer: pk(writer),
    }
}

/// A legacy authorization of `(ctx, op)` signed by `writer`.
fn legacy(ctx: ContextRef, op: [u8; 32], writer: &SigningKey) -> ConsumedEntry {
    let effect = effect();
    ConsumedEntry {
        authorization: ConsumedAuthorization::Legacy(Signed::sign(
            AuthorizationV0 {
                context: ctx,
                epoch: 1,
                subject_matter: effect.commitment(ctx, op),
                rule_hash: [0x55; 32],
                verdict: Verdict::Pass,
                inputs: Vec::new(),
                roster_hash: [0x66; 32],
                effect,
                operation_id: op,
                not_after: 1_500,
            },
            writer,
        )),
        writer: pk(writer),
    }
}

fn entry() -> ConsumedEntry {
    checkpointed(CTX, OP, &key(0xA1), 1)
}

fn open(dir: &Path) -> DurableConsumedSet {
    DurableConsumedSet::open(dir).unwrap()
}

/// Raw store access to the same directory (the set must be closed first: sled locks it).
fn raw(dir: &Path) -> SledStore {
    SledStore::open(dir).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Persistence across a clean close and reopen
// ---------------------------------------------------------------------------------------------

/// A, O, R: a consumption is acknowledged, survives close/reopen with its variant, writer and
/// signed statement intact, and the same key is never consumed again.
#[test]
fn r_acknowledged_consumption_survives_clean_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let first = entry();
    {
        let mut set = open(dir.path());
        assert_eq!(set.consume(CTX, OP, first.clone()), Ok(None));
    }
    let mut set = open(dir.path());
    assert_eq!(set.inspect(CTX, OP), Ok(Some(first.clone())));
    // Identical and conflicting attempts both return the original, unchanged.
    assert_eq!(set.consume(CTX, OP, first.clone()), Ok(Some(first.clone())));
    let rival = checkpointed(CTX, OP, &key(0xA1), 2);
    assert_eq!(set.consume(CTX, OP, rival), Ok(Some(first.clone())));
    drop(set);
    assert_eq!(open(dir.path()).inspect(CTX, OP), Ok(Some(first)));
}

/// D (storage level), O: both variants persist and come back as themselves, and either kind
/// blocks the other on one key.
#[test]
fn d_both_kinds_share_one_key_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let w = key(0xA1);
    let ops = [[0x01; 32], [0x02; 32]];
    {
        let mut set = open(dir.path());
        assert_eq!(set.consume(CTX, ops[0], legacy(CTX, ops[0], &w)), Ok(None));
        assert_eq!(
            set.consume(CTX, ops[1], checkpointed(CTX, ops[1], &w, 1)),
            Ok(None)
        );
    }
    let mut set = open(dir.path());
    assert_eq!(
        set.consume(CTX, ops[0], checkpointed(CTX, ops[0], &w, 1)),
        Ok(Some(legacy(CTX, ops[0], &w)))
    );
    assert_eq!(
        set.consume(CTX, ops[1], legacy(CTX, ops[1], &w)),
        Ok(Some(checkpointed(CTX, ops[1], &w, 1)))
    );
}

/// E: two operation ids in one context are two keys.
#[test]
fn e_distinct_operations_in_one_context_both_persist() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = ([0x01; 32], [0x02; 32]);
    {
        let mut set = open(dir.path());
        assert_eq!(
            set.consume(CTX, a, checkpointed(CTX, a, &key(1), 1)),
            Ok(None)
        );
        assert_eq!(
            set.consume(CTX, b, checkpointed(CTX, b, &key(1), 1)),
            Ok(None)
        );
    }
    let set = open(dir.path());
    assert!(set.inspect(CTX, a).unwrap().is_some());
    assert!(set.inspect(CTX, b).unwrap().is_some());
}

/// F: one operation id in two contexts is two keys.
#[test]
fn f_one_operation_in_two_contexts_is_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let other = ContextRef([0x12; 32]);
    {
        let mut set = open(dir.path());
        assert_eq!(
            set.consume(CTX, OP, checkpointed(CTX, OP, &key(1), 1)),
            Ok(None)
        );
        assert_eq!(
            set.consume(other, OP, checkpointed(other, OP, &key(1), 1)),
            Ok(None)
        );
    }
    let set = open(dir.path());
    assert_eq!(
        set.inspect(other, OP)
            .unwrap()
            .unwrap()
            .authorization
            .context(),
        other
    );
}

/// The key is fixed-length and carries the full context and operation id.
#[test]
fn keys_are_fixed_length_and_injective() {
    let k = consumed_key(CTX, OP);
    assert_eq!(k.len(), KEY_PREFIX.len() + 64);
    assert_eq!(&k[KEY_PREFIX.len()..KEY_PREFIX.len() + 32], &CTX.0);
    assert_eq!(&k[KEY_PREFIX.len() + 32..], &OP);
    // Moving a byte from the context into the operation id changes the key.
    let mut ctx2 = CTX.0;
    ctx2[31] = 0x22;
    let mut op2 = OP;
    op2[0] = 0x11;
    assert_ne!(consumed_key(ContextRef(ctx2), op2), k);
}

// ---------------------------------------------------------------------------------------------
// Concurrency
// ---------------------------------------------------------------------------------------------

/// G: many different entries race for one key; exactly one is acknowledged and every other
/// caller is handed that one; the winner is what persists.
#[test]
fn g_concurrent_attempts_on_one_key_acknowledge_once() {
    let dir = tempfile::tempdir().unwrap();
    let set = open(dir.path());
    let results: Vec<(
        ConsumedEntry,
        Result<Option<ConsumedEntry>, ConsumedSetError>,
    )> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..16u64)
            .map(|i| {
                let set = &set;
                s.spawn(move || {
                    let e = checkpointed(CTX, OP, &key(0xA1), i + 1);
                    (e.clone(), set.consume_shared(CTX, OP, e))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let winners: Vec<&ConsumedEntry> = results
        .iter()
        .filter(|(_, r)| r == &Ok(None))
        .map(|(e, _)| e)
        .collect();
    assert_eq!(winners.len(), 1);
    let winner = winners[0].clone();
    for (_, r) in &results {
        assert!(r == &Ok(None) || r == &Ok(Some(winner.clone())), "{r:?}");
    }
    drop(set);
    assert_eq!(open(dir.path()).inspect(CTX, OP), Ok(Some(winner)));
}

/// H: concurrent consumptions of different keys are all acknowledged and all persist.
#[test]
fn h_concurrent_attempts_on_distinct_keys_all_persist() {
    let dir = tempfile::tempdir().unwrap();
    let set = open(dir.path());
    std::thread::scope(|s| {
        for i in 0..16u8 {
            let set = &set;
            s.spawn(move || {
                let op = [i; 32];
                assert_eq!(
                    set.consume_shared(CTX, op, checkpointed(CTX, op, &key(0xA1), 1)),
                    Ok(None)
                );
            });
        }
    });
    drop(set);
    let set = open(dir.path());
    for i in 0..16u8 {
        assert!(set.inspect(CTX, [i; 32]).unwrap().is_some(), "op {i} lost");
    }
}

// ---------------------------------------------------------------------------------------------
// Fault injection between insert, barrier and acknowledgement (S)
// ---------------------------------------------------------------------------------------------

/// A sled backend with switchable faults.
#[derive(Default)]
struct Faults {
    /// `insert_if_absent` fails without writing (the `Store` contract for an `Err`).
    insert_fails: AtomicBool,
    /// `insert_if_absent` writes, then reports an error anyway (a backend breaking the contract).
    insert_applies_then_fails: AtomicBool,
    /// `sync` fails.
    sync_fails: AtomicBool,
    /// `get` fails.
    get_fails: AtomicBool,
}

struct Faulty {
    inner: SledStore,
    faults: Arc<Faults>,
}

impl DurableBackend for Faulty {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        if self.faults.get_fails.load(Ordering::SeqCst) {
            return Err("injected read failure".into());
        }
        DurableBackend::get(&self.inner, key)
    }

    fn insert_if_absent(&self, key: &[u8], value: &[u8]) -> Result<CompareAndPut, String> {
        if self.faults.insert_fails.load(Ordering::SeqCst) {
            return Err("injected insert failure (nothing written)".into());
        }
        let r = self.inner.insert_if_absent(key, value)?;
        if self.faults.insert_applies_then_fails.load(Ordering::SeqCst) {
            return Err("injected failure after the insert applied".into());
        }
        Ok(r)
    }

    fn sync(&self) -> Result<(), String> {
        if self.faults.sync_fails.load(Ordering::SeqCst) {
            return Err("injected barrier failure".into());
        }
        self.inner.sync()
    }
}

fn faulty(dir: &Path) -> (DurableConsumedSet<Faulty>, Arc<Faults>) {
    let faults = Arc::new(Faults::default());
    let set = DurableConsumedSet::with_backend(Faulty {
        inner: raw(dir),
        faults: faults.clone(),
    });
    (set, faults)
}

/// I: an insert that fails cannot acknowledge, records nothing, and the retry is admitted.
#[test]
fn i_insert_failure_never_acknowledges() {
    let dir = tempfile::tempdir().unwrap();
    let (mut set, faults) = faulty(dir.path());
    faults.insert_fails.store(true, Ordering::SeqCst);
    assert!(matches!(
        set.consume(CTX, OP, entry()),
        Err(ConsumedSetError::Unavailable(_))
    ));
    assert_eq!(set.inspect(CTX, OP), Ok(None));
    faults.insert_fails.store(false, Ordering::SeqCst);
    assert_eq!(set.consume(CTX, OP, entry()), Ok(None));
}

/// J, K: the insert applied but the barrier failed. The caller is told the commit is uncertain
/// (never an acknowledgement); the record is not rolled back; a retry — identical or
/// conflicting — is handed the stored record, never acknowledged and never written over; and
/// the record that survives the reopen is exactly the original.
#[test]
fn j_k_barrier_failure_is_uncertain_and_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let first = entry();
    {
        let (mut set, faults) = faulty(dir.path());
        faults.sync_fails.store(true, Ordering::SeqCst);
        assert!(matches!(
            set.consume(CTX, OP, first.clone()),
            Err(ConsumedSetError::CommitUncertain(_))
        ));
        // Not erased.
        assert_eq!(set.inspect(CTX, OP), Ok(Some(first.clone())));
        // Retries, with the barrier still failing and once it recovers.
        assert_eq!(set.consume(CTX, OP, first.clone()), Ok(Some(first.clone())));
        faults.sync_fails.store(false, Ordering::SeqCst);
        assert_eq!(set.consume(CTX, OP, first.clone()), Ok(Some(first.clone())));
        let rival = checkpointed(CTX, OP, &key(0xA1), 2);
        assert_eq!(set.consume(CTX, OP, rival), Ok(Some(first.clone())));
    }
    // After a clean close the record is the original: there was never a second acknowledgement.
    assert_eq!(open(dir.path()).inspect(CTX, OP), Ok(Some(first)));
}

/// S: a backend that applies the insert and then reports an error (violating its contract)
/// still cannot produce an acknowledgement, and the retry cannot overwrite.
#[test]
fn s_contract_violating_backend_still_cannot_double_acknowledge() {
    let dir = tempfile::tempdir().unwrap();
    let (mut set, faults) = faulty(dir.path());
    faults
        .insert_applies_then_fails
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        set.consume(CTX, OP, entry()),
        Err(ConsumedSetError::Unavailable(_))
    ));
    faults
        .insert_applies_then_fails
        .store(false, Ordering::SeqCst);
    let rival = checkpointed(CTX, OP, &key(0xA1), 2);
    assert_eq!(set.consume(CTX, OP, rival), Ok(Some(entry())));
}

/// A read failure during inspection is reported, not mistaken for "not consumed".
#[test]
fn read_failure_is_not_absence() {
    let dir = tempfile::tempdir().unwrap();
    let (set, faults) = faulty(dir.path());
    faults.get_fails.store(true, Ordering::SeqCst);
    assert!(matches!(
        set.inspect(CTX, OP),
        Err(ConsumedSetError::Unavailable(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// Strict records (L, M) and refused entries
// ---------------------------------------------------------------------------------------------

/// Store `bytes` under `(CTX, OP)` directly, then try both reading and consuming through the set.
fn corrupt_with(bytes: &[u8]) -> (Result<Option<ConsumedEntry>, ConsumedSetError>, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = raw(dir.path());
        Store::put(&store, &consumed_key(CTX, OP), bytes).unwrap();
        store.flush().unwrap();
    }
    let mut set = open(dir.path());
    let inspected = set.inspect(CTX, OP);
    assert_eq!(
        inspected,
        set.consume(CTX, OP, entry()).map(|_| None),
        "inspect and consume disagree"
    );
    drop(set);
    // Nothing overwrote the corrupt bytes.
    let after = Store::get(&raw(dir.path()), &consumed_key(CTX, OP))
        .unwrap()
        .unwrap();
    (inspected, after)
}

fn assert_corrupt(bytes: &[u8]) {
    let (r, after) = corrupt_with(bytes);
    assert!(
        matches!(r, Err(ConsumedSetError::Corrupt(_))),
        "{r:?} for {} bytes",
        bytes.len()
    );
    assert_eq!(after, bytes);
}

/// L: arbitrary bytes.
#[test]
fn l_malformed_record_fails_closed() {
    assert_corrupt(b"not a record");
    assert_corrupt(&[]);
}

/// M: truncated, extended, oversized, unknown version or kind, a record stored under another
/// key, and a record whose signature does not verify under its writer.
#[test]
fn m_damaged_or_misplaced_records_fail_closed() {
    let good = entry().encode();
    assert_corrupt(&good[..good.len() - 1]);
    let mut longer = good.clone();
    longer.push(0);
    assert_corrupt(&longer);
    // Larger than any record the codec can produce (one statement is bounded by 64 KiB).
    assert_corrupt(&vec![0u8; 80 * 1024]);

    // Layout: lp(domain) = 4 + len, then u16 version, then u8 kind.
    let domain_len = u32::from_be_bytes(good[..4].try_into().unwrap()) as usize;
    let version_at = 4 + domain_len;
    let mut version = good.clone();
    version[version_at + 1] ^= 0xFF;
    assert_corrupt(&version);
    let mut kind = good.clone();
    kind[version_at + 2] = 9;
    assert_corrupt(&kind);

    // A valid record of another operation, stored under this key.
    assert_corrupt(&checkpointed(CTX, [0x99; 32], &key(0xA1), 1).encode());
    // A valid statement attributed to a writer that did not sign it.
    let mut forged = entry();
    forged.writer = pk(&key(0xB2));
    assert_corrupt(&forged.encode());
    // One flipped signature bit.
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    assert_corrupt(&flipped);
}

/// Entries that could never be read back are refused before any write.
#[test]
fn invalid_entries_are_refused_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let mut set = open(dir.path());
    // Consumed under a key it does not name.
    assert!(matches!(
        set.consume(CTX, [0x77; 32], entry()),
        Err(ConsumedSetError::InvalidEntry(_))
    ));
    // Attributed to a writer that did not sign it.
    let mut forged = entry();
    forged.writer = pk(&key(0xB2));
    assert!(matches!(
        set.consume(CTX, OP, forged),
        Err(ConsumedSetError::InvalidEntry(_))
    ));
    // A genuinely signed statement that the codec will not read back (an action label one byte
    // over `MAX_ACTION_LEN`): persisting it would turn into a permanent `Corrupt`.
    let mut unreadable = entry();
    if let ConsumedAuthorization::Checkpointed(s) = &mut unreadable.authorization {
        let mut body = s.body.clone();
        body.effect.action = "x".repeat(icn_authz::effect_v0::MAX_ACTION_LEN + 1);
        *s = Signed::sign(body, &key(0xA1));
    }
    assert!(unreadable.is_authentic());
    assert!(matches!(
        set.consume(CTX, OP, unreadable),
        Err(ConsumedSetError::InvalidEntry(_))
    ));
    assert_eq!(set.inspect(CTX, [0x77; 32]), Ok(None));
    assert_eq!(set.inspect(CTX, OP), Ok(None));
}

/// A backend that reports a conflict yet no current value (impossible for an insert-if-absent)
/// is treated as corrupt state, never as an acknowledgement.
#[test]
fn impossible_conflict_is_corrupt_not_acknowledged() {
    struct Liar(SledStore);
    impl DurableBackend for Liar {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
            DurableBackend::get(&self.0, key)
        }
        fn insert_if_absent(&self, _: &[u8], _: &[u8]) -> Result<CompareAndPut, String> {
            Ok(CompareAndPut::Conflict { current: None })
        }
        fn sync(&self) -> Result<(), String> {
            self.0.sync()
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut set = DurableConsumedSet::with_backend(Liar(raw(dir.path())));
    assert!(matches!(
        set.consume(CTX, OP, entry()),
        Err(ConsumedSetError::Corrupt(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// Unsupported backends (N)
// ---------------------------------------------------------------------------------------------

/// N: a backend whose insert is the `Store` default `compare_and_put` (no atomic CAS) fails
/// closed: no acknowledgement and nothing written.
#[test]
fn n_store_without_atomic_insert_fails_closed() {
    /// A real sled store that hides its compare-and-put, as a non-CAS backend would.
    struct NoCas(SledStore);
    impl Store for NoCas {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            Store::get(&self.0, key)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            Store::put(&self.0, key, value)
        }
        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            Store::delete(&self.0, key)
        }
        fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
            Store::delete_atomic(&self.0, keys)
        }
        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Store::scan(&self.0, prefix)
        }
        fn get_replica_metadata(
            &self,
            h: &[u8; 32],
        ) -> anyhow::Result<Option<icn_store::ReplicaMetadata>> {
            Store::get_replica_metadata(&self.0, h)
        }
        fn put_replica_metadata(&self, m: &icn_store::ReplicaMetadata) -> anyhow::Result<()> {
            Store::put_replica_metadata(&self.0, m)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<[u8; 32]>> {
            Store::list_replica_hashes(&self.0)
        }
    }
    struct Via(NoCas);
    impl DurableBackend for Via {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
            Store::get(&self.0, key).map_err(|e| e.to_string())
        }
        fn insert_if_absent(&self, key: &[u8], value: &[u8]) -> Result<CompareAndPut, String> {
            Store::compare_and_put(&self.0, key, None, value).map_err(|e| e.to_string())
        }
        fn sync(&self) -> Result<(), String> {
            Store::flush(&self.0).map_err(|e| e.to_string())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut set = DurableConsumedSet::with_backend(Via(NoCas(raw(dir.path()))));
    match set.consume(CTX, OP, entry()) {
        Err(ConsumedSetError::Unavailable(msg)) => {
            assert!(msg.contains("compare-and-put"), "{msg}")
        }
        other => panic!("expected a fail-closed refusal, got {other:?}"),
    }
    assert_eq!(set.inspect(CTX, OP), Ok(None));
}

// ---------------------------------------------------------------------------------------------
// Process crash (T): a child process is aborted, the parent reopens
// ---------------------------------------------------------------------------------------------

const CHILD_DIR: &str = "ICN_AUTHZ_STORE_CHILD_DIR";
const CHILD_MODE: &str = "ICN_AUTHZ_STORE_CHILD_MODE";
/// `std::process::abort` raises SIGABRT on Linux.
const SIGABRT: i32 = 6;

/// The child half of [`t_abort_after_acknowledgement_keeps_the_consumption`] and
/// [`t_abort_between_insert_and_barrier_is_all_or_nothing`]. Inert unless run by the parent.
#[test]
#[ignore = "child process of the crash tests; run only by them"]
fn crash_child() {
    let Some(dir) = std::env::var_os(CHILD_DIR) else {
        return;
    };
    let mode = std::env::var(CHILD_MODE).unwrap();
    match mode.as_str() {
        // Acknowledged, then the process dies without unwinding or closing the database.
        "after-ack" => {
            let mut set = open(Path::new(&dir));
            assert_eq!(set.consume(CTX, OP, entry()), Ok(None));
            println!("ACK");
            std::process::abort();
        }
        // The insert applies, then the process dies inside the barrier, before it runs.
        "before-barrier" => {
            struct DieInSync(SledStore);
            impl DurableBackend for DieInSync {
                fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
                    DurableBackend::get(&self.0, key)
                }
                fn insert_if_absent(&self, k: &[u8], v: &[u8]) -> Result<CompareAndPut, String> {
                    self.0.insert_if_absent(k, v)
                }
                fn sync(&self) -> Result<(), String> {
                    println!("APPLIED");
                    std::process::abort();
                }
            }
            let mut set = DurableConsumedSet::with_backend(DieInSync(raw(Path::new(&dir))));
            let _ = set.consume(CTX, OP, entry());
            unreachable!("the barrier aborts");
        }
        other => panic!("unknown child mode {other}"),
    }
}

/// Run [`crash_child`] in a separate process with `mode`; return its stdout and the signal that
/// terminated it, if any.
fn run_child(dir: &Path, mode: &str) -> (String, Option<i32>) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "crash_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_DIR, dir)
        .env(CHILD_MODE, mode)
        .output()
        .unwrap();
    use std::os::unix::process::ExitStatusExt;
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.signal(),
    )
}

/// T: a real process abort (`SIGABRT`, no unwinding, no close) after the acknowledgement. The
/// reopened set holds the consumption and refuses the operation again. Process crash only: no
/// OS crash or power loss is simulated.
#[test]
fn t_abort_after_acknowledgement_keeps_the_consumption() {
    let dir = tempfile::tempdir().unwrap();
    let (stdout, signal) = run_child(dir.path(), "after-ack");
    assert!(stdout.contains("ACK"), "child never acknowledged: {stdout}");
    assert_eq!(signal, Some(SIGABRT), "the child must die of SIGABRT");
    let mut set = open(dir.path());
    assert_eq!(set.inspect(CTX, OP), Ok(Some(entry())));
    let rival = checkpointed(CTX, OP, &key(0xA1), 2);
    assert_eq!(set.consume(CTX, OP, rival), Ok(Some(entry())));
}

/// T: a process abort after the insert applied but before the barrier ran. Nothing was
/// acknowledged, so either outcome is allowed — the record is absent, or it is exactly the
/// original — and in both the operation can be acknowledged at most once afterwards.
#[test]
fn t_abort_between_insert_and_barrier_is_all_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (stdout, signal) = run_child(dir.path(), "before-barrier");
    assert!(stdout.contains("APPLIED"), "child never applied: {stdout}");
    assert_eq!(signal, Some(SIGABRT), "the child must die of SIGABRT");
    let mut set = open(dir.path());
    match set.inspect(CTX, OP) {
        Ok(None) => {
            println!("before-barrier outcome: record absent after the abort");
            assert_eq!(set.consume(CTX, OP, entry()), Ok(None));
            assert_eq!(set.consume(CTX, OP, entry()), Ok(Some(entry())));
        }
        Ok(Some(found)) => {
            println!("before-barrier outcome: record present after the abort");
            assert_eq!(found, entry());
            assert_eq!(set.consume(CTX, OP, entry()), Ok(Some(entry())));
        }
        Err(e) => panic!("a torn record after the abort: {e:?}"),
    }
}
