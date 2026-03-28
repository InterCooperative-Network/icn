//! Gossip state persistence proof — Layer 1.
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

use icn_gossip::{AccessControl, GossipActor, Topic};
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
