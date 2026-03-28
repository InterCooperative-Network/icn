//! Gossip state persistence proof — Layers 1–4.
//!
//! ## Architecture note: gossip persistence is NOT sled-based
//!
//! Unlike ledger and governance, gossip state is persisted via the
//! `icn-snapshot` JSON file mechanism — not sled:
//!
//! - Write: `GossipActor::export_state()` → `icn_snapshot::StateSnapshot` →
//!   `save_snapshot(&snapshot, data_dir)` → atomic JSON file on disk
//! - Read: `load_snapshot(data_dir)` → `GossipActor::restore_state(state)`
//!
//! Gossip entries are intentionally NOT persisted — they are re-fetched from
//! peers via anti-entropy after restart. What persists for continuity:
//! - Vector clock (causal ordering state)
//! - Topic metadata (name, ACL, scope, max_entries)
//! - Topic subscriptions (which DIDs are subscribed to which topics)
//!
//! ## The proof
//!
//! 1. `GossipActor::new(own_did, None)` — no oracle or keypair needed.
//! 2. `create_topic()` — register a named topic with Public ACL.
//! 3. `publish()` — increments vector clock to 1; no send_callback needed.
//! 4. `subscribe()` — adds a second DID to the topic's subscriber list.
//! 5. `export_state()` → `save_snapshot()` — persist to disk.
//! 6. Drop actor — all in-memory state gone.
//! 7. `load_snapshot()` → `restore_state()` into fresh actor.
//! 8. Assert exact field values for topic, subscriber, and vector clock.
//!
//! ## What is NOT proven
//!
//! - Cross-process restart: requires subprocess (Layer 4 target).
//! - Gossip entry re-gossip correctness: entries are NOT persisted by design.
//! - Anti-entropy resync after restart: requires multi-node integration test.
//! - Actor-backed path through GossipHandle: same-runtime close+reopen (Layer 3).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gossip::{AccessControl, GossipActor, GossipHandle, Topic};
use icn_identity::KeyPair;
use icn_snapshot::{load_snapshot, save_snapshot, StateSnapshot};

const PROOF_TOPIC: &str = "layer-1-persistence-proof";

/// Layer 1 — GossipActor state snapshot persistence proof.
///
/// Proves that topic metadata, topic subscriptions, and the vector clock
/// written through the canonical `export_state()` → `save_snapshot()` path
/// survive a drop-and-reload boundary with exact field values when restored
/// via `restore_state()`.
///
/// No oracle, no keypair, no network layer needed — this exercises the pure
/// state serialization path used by the production snapshot mechanism.
#[tokio::test]
async fn test_gossip_state_survives_export_snapshot_restore() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_path_buf();

    let own_kp = KeyPair::generate().expect("own KeyPair");
    let own_did = own_kp.did().clone();

    let subscriber_kp = KeyPair::generate().expect("subscriber KeyPair");
    let subscriber_did = subscriber_kp.did().clone();

    // ── Phase 1: build state, export, persist ────────────────────────────────
    {
        let mut actor = GossipActor::new(own_did.clone(), None);

        // Register topic with Public ACL — any DID can subscribe or publish.
        actor.create_topic(Topic::new(PROOF_TOPIC.to_string(), AccessControl::Public));

        // Publish one entry — increments vector clock for own_did to 1.
        // No keypair or send_callback needed: entry is stored locally only.
        actor
            .publish(PROOF_TOPIC, b"layer-1-proof-payload".to_vec())
            .await
            .expect("Phase1: publish");

        // Subscribe a second DID. Public ACL bypasses trust gating; no oracle
        // check fires because oracle is None.
        actor
            .subscribe(PROOF_TOPIC, subscriber_did.clone())
            .await
            .expect("Phase1: subscribe");

        // Export state and persist via icn-snapshot (atomic JSON file write).
        let gossip_state = actor.export_state();
        let mut snapshot = StateSnapshot::new();
        snapshot.gossip_state = Some(gossip_state);
        save_snapshot(&snapshot, &data_dir).expect("Phase1: save_snapshot");

        // actor drops here — all in-memory state released.
    }

    // ── Phase 2: load snapshot and restore into fresh actor ──────────────────
    let snapshot = load_snapshot(&data_dir)
        .expect("Phase2: load_snapshot")
        .expect("snapshot must be present after save");

    let gossip_state = snapshot
        .gossip_state
        .expect("gossip_state must be present in loaded snapshot");

    let mut actor2 = GossipActor::new(own_did.clone(), None);
    actor2
        .restore_state(gossip_state)
        .expect("Phase2: restore_state");

    // ── Exact assertions ─────────────────────────────────────────────────────

    // 1. Topic name survives exact round-trip.
    let topics = actor2.get_topics();
    assert!(
        topics.contains(&PROOF_TOPIC.to_string()),
        "topic {PROOF_TOPIC:?} must survive snapshot round-trip, got: {topics:?}"
    );

    // 2. Subscriber DID survives in the topic's subscription list.
    //    restore_state inserts directly (no ACL recheck — trusts persisted state).
    let subscribers = actor2.get_subscribers(PROOF_TOPIC);
    assert!(
        subscribers.iter().any(|d| d == &subscriber_did),
        "subscriber DID must survive snapshot round-trip, got: {subscribers:?}"
    );

    // 3. Vector clock entry for own_did survives with exact count.
    //    One publish = one increment = count 1.
    let clock_val = actor2.get_clock().get(&own_did);
    assert_eq!(
        clock_val, 1,
        "vector clock for own_did must be 1 after one publish, got: {clock_val}"
    );
}

/// Layer 2 — GossipHandle (Arc<RwLock<GossipActor>>) snapshot persistence proof.
///
/// Proves that topic metadata, topic subscriptions, and the vector clock
/// written through the production handle path
/// (`GossipHandle` = `Arc<RwLock<GossipActor>>`) survive a drop-and-reload
/// boundary with exact field values when restored via `restore_state()`.
///
/// This is the actual path used by the supervisor:
/// - mutations: `gossip_handle.write().await.method()`
/// - export:    `gossip_handle.read().await.export_state()`
/// - restore:   `gossip_handle.write().await.restore_state(state)`
///
/// No oracle, no keypair, no network layer needed — this exercises the
/// handle-backed access pattern used in every production supervisor path.
#[tokio::test]
async fn test_gossip_handle_state_survives_snapshot_restore() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_path_buf();

    let own_kp = KeyPair::generate().expect("own KeyPair");
    let own_did = own_kp.did().clone();

    let subscriber_kp = KeyPair::generate().expect("subscriber KeyPair");
    let subscriber_did = subscriber_kp.did().clone();

    // ── Phase 1: mutate through handle, export, persist ─────────────────────
    {
        // Production path: GossipActor::spawn returns Arc<RwLock<GossipActor>>.
        let handle: GossipHandle = GossipActor::spawn(own_did.clone(), None);

        // All mutations go through the write guard — exactly as the supervisor does.
        {
            let mut g = handle.write().await;
            g.create_topic(Topic::new(PROOF_TOPIC.to_string(), AccessControl::Public));
            g.publish(PROOF_TOPIC, b"layer-2-proof-payload".to_vec())
                .await
                .expect("Phase1: publish");
            g.subscribe(PROOF_TOPIC, subscriber_did.clone())
                .await
                .expect("Phase1: subscribe");
            // g drops here — write lock released before export.
        }

        // Export via read lock — the path used by supervisor shutdown.rs.
        let gossip_state = handle.read().await.export_state();
        let mut snapshot = StateSnapshot::new();
        snapshot.gossip_state = Some(gossip_state);
        save_snapshot(&snapshot, &data_dir).expect("Phase1: save_snapshot");

        // handle drops here — all in-memory state released.
    }

    // ── Phase 2: load snapshot and restore into fresh actor ──────────────────
    let snapshot = load_snapshot(&data_dir)
        .expect("Phase2: load_snapshot")
        .expect("snapshot must be present after save");

    let gossip_state = snapshot
        .gossip_state
        .expect("gossip_state must be present in loaded snapshot");

    let mut actor2 = GossipActor::new(own_did.clone(), None);
    actor2
        .restore_state(gossip_state)
        .expect("Phase2: restore_state");

    // ── Exact assertions (same invariants as Layer 1) ────────────────────────

    // 1. Topic name survives exact round-trip.
    let topics = actor2.get_topics();
    assert!(
        topics.contains(&PROOF_TOPIC.to_string()),
        "topic {PROOF_TOPIC:?} must survive handle-backed snapshot round-trip, got: {topics:?}"
    );

    // 2. Subscriber DID survives in the topic's subscription list.
    let subscribers = actor2.get_subscribers(PROOF_TOPIC);
    assert!(
        subscribers.iter().any(|d| d == &subscriber_did),
        "subscriber DID must survive handle-backed snapshot round-trip, got: {subscribers:?}"
    );

    // 3. Vector clock entry for own_did survives with exact count.
    let clock_val = actor2.get_clock().get(&own_did);
    assert_eq!(
        clock_val, 1,
        "vector clock for own_did must be 1 after one publish via handle, got: {clock_val}"
    );
}

/// Layer 3 — Same-runtime handle drop + fresh handle restore proof.
///
/// Proves that gossip coordination state (vector clock, topic metadata,
/// subscriptions) survives a same-runtime lifecycle boundary:
///
/// 1. Mutate state through a `GossipHandle`.
/// 2. Export and persist snapshot.
/// 3. **Drop all Arc refs to the original handle** — actor memory fully
///    reclaimed.  No in-memory continuity; the snapshot on disk is the only
///    bridge.
/// 4. In the **same Tokio runtime**, create a brand-new `GossipHandle` via
///    `GossipActor::spawn()`.
/// 5. Restore state into the fresh handle exactly as the supervisor does at
///    boot (`restore_gossip_snapshot` → `gossip_handle.write().await.restore_state()`).
/// 6. Assert exact invariants through the fresh handle's read lock.
///
/// This is the real supervisor lifecycle: the daemon shuts down, the handle
/// drops, and at restart a fresh handle is created and restored from snapshot
/// before accepting any new work.
#[tokio::test]
async fn test_gossip_handle_survives_same_runtime_drop_and_recreate() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_path_buf();

    let own_kp = KeyPair::generate().expect("own KeyPair");
    let own_did = own_kp.did().clone();

    let subscriber_kp = KeyPair::generate().expect("subscriber KeyPair");
    let subscriber_did = subscriber_kp.did().clone();

    // ── Phase 1: mutate through handle, export, persist, DROP handle ─────────
    {
        let handle: GossipHandle = GossipActor::spawn(own_did.clone(), None);

        {
            let mut g = handle.write().await;
            g.create_topic(Topic::new(PROOF_TOPIC.to_string(), AccessControl::Public));
            g.publish(PROOF_TOPIC, b"layer-3-proof-payload".to_vec())
                .await
                .expect("Phase1: publish");
            g.subscribe(PROOF_TOPIC, subscriber_did.clone())
                .await
                .expect("Phase1: subscribe");
            // write guard drops here
        }

        // Export via read lock — same path as supervisor shutdown.rs.
        let gossip_state = handle.read().await.export_state();

        // Persist to disk.
        let mut snapshot = StateSnapshot::new();
        snapshot.gossip_state = Some(gossip_state);
        save_snapshot(&snapshot, &data_dir).expect("Phase1: save_snapshot");

        // handle drops at end of this block — all Arc refs released.
        // Actor memory is fully reclaimed; snapshot on disk is the only bridge.
    }

    // ── Boundary: load snapshot from disk only (no in-memory remnant) ────────
    let snapshot = load_snapshot(&data_dir)
        .expect("Phase2: load_snapshot")
        .expect("snapshot must be present after save");
    let restored_state = snapshot
        .gossip_state
        .expect("gossip_state must be present in loaded snapshot");

    // ── Phase 2: brand-new handle in the SAME Tokio runtime ──────────────────
    // GossipActor::spawn() creates a completely empty actor with no prior state.
    let handle2: GossipHandle = GossipActor::spawn(own_did.clone(), None);

    // Restore via write lock — the exact path used by restore_gossip_snapshot
    // in supervisor/init_gossip.rs.
    handle2
        .write()
        .await
        .restore_state(restored_state)
        .expect("Phase2: restore_state");

    // ── Exact assertions through the fresh handle's read lock ─────────────────

    let g = handle2.read().await;

    // 1. Topic name survives the full lifecycle boundary.
    let topics = g.get_topics();
    assert!(
        topics.contains(&PROOF_TOPIC.to_string()),
        "topic {PROOF_TOPIC:?} must survive same-runtime drop+recreate, got: {topics:?}"
    );

    // 2. Subscriber DID survives in the topic's subscription list.
    let subscribers = g.get_subscribers(PROOF_TOPIC);
    assert!(
        subscribers.iter().any(|d| d == &subscriber_did),
        "subscriber DID must survive same-runtime drop+recreate, got: {subscribers:?}"
    );

    // 3. Vector clock entry for own_did survives with exact count.
    let clock_val = g.get_clock().get(&own_did);
    assert_eq!(
        clock_val, 1,
        "vector clock for own_did must be 1 after same-runtime restore, got: {clock_val}"
    );
}

/// Layer 4 — Cross-process gossip snapshot persistence proof.
///
/// Proves that gossip coordination state (vector clock, topic metadata,
/// subscriptions) written in one OS process is readable in a completely
/// fresh OS process. No shared memory. No shared runtime. True process-
/// boundary restart.
///
/// Implementation:
/// - Helper binary: `crates/icn-gossip/src/bin/gossip_restart_helper.rs`
///   - `write <data_dir>` — builds coordination state through GossipHandle,
///     persists snapshot, prints "own_did,subscriber_did" to stdout, exits 0.
///   - `read <data_dir> <own_did> <subscriber_did>` — loads snapshot, restores
///     into fresh GossipActor, asserts exact invariants, exits 0 or 1.
///
/// Architecture note: gossip uses JSON snapshot files (not sled). No file
/// lock release is needed between processes — the snapshot is written
/// atomically and closed on drop.
#[test]
fn test_gossip_state_survives_cross_process_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_str().expect("data_dir to str");
    let helper = env!("CARGO_BIN_EXE_gossip_restart_helper");

    // ── Write subprocess: build + persist gossip coordination state ───────────
    let write_out = std::process::Command::new(helper)
        .args(["write", data_dir])
        .output()
        .expect("failed to spawn write subprocess");

    assert!(
        write_out.status.success(),
        "write subprocess failed (exit {:?}):\nstdout: {}\nstderr: {}",
        write_out.status.code(),
        String::from_utf8_lossy(&write_out.stdout),
        String::from_utf8_lossy(&write_out.stderr)
    );

    // Write subprocess prints "own_did,subscriber_did" on one line.
    let stdout = String::from_utf8(write_out.stdout)
        .expect("write stdout must be valid UTF-8")
        .trim()
        .to_string();

    let (own_did_str, subscriber_did_str) = stdout
        .split_once(',')
        .expect("write stdout must be 'own_did,subscriber_did'");

    assert!(
        own_did_str.starts_with("did:icn:"),
        "own_did must be a valid DID, got: {own_did_str:?}"
    );
    assert!(
        subscriber_did_str.starts_with("did:icn:"),
        "subscriber_did must be a valid DID, got: {subscriber_did_str:?}"
    );

    // ── Read subprocess: fresh process, no shared memory ─────────────────────
    let read_out = std::process::Command::new(helper)
        .args(["read", data_dir, own_did_str, subscriber_did_str])
        .output()
        .expect("failed to spawn read subprocess");

    assert!(
        read_out.status.success(),
        "read subprocess failed (exit {:?}):\nstdout: {}\nstderr: {}",
        read_out.status.code(),
        String::from_utf8_lossy(&read_out.stdout),
        String::from_utf8_lossy(&read_out.stderr)
    );
}
