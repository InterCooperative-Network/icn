//! Cross-process gossip persistence proof helper binary.
//!
//! Used exclusively by the Layer 4 integration test in
//! `crates/icn-gossip/tests/gossip_persistence.rs` to prove that gossip
//! coordination state written through `GossipActor` survives a true OS
//! process boundary — not just a same-runtime drop-and-reopen.
//!
//! ## Usage
//!
//! ```text
//! gossip_restart_helper write <data_dir>
//!     Creates a GossipHandle, mutates gossip coordination state (topic,
//!     subscriber, vector clock), exports via read().await.export_state(),
//!     persists via save_snapshot(). Prints "own_did,subscriber_did" to
//!     stdout, exits 0. Exits 1 on error.
//!
//! gossip_restart_helper read <data_dir> <own_did> <subscriber_did>
//!     Loads snapshot from data_dir, restores into a fresh GossipActor,
//!     asserts exact invariants (topic name, subscriber DID, vector clock
//!     count). Exits 0 on success, 1 on failure.
//! ```
//!
//! ## Architecture note
//!
//! Gossip persistence is snapshot-based (JSON via `icn-snapshot`), not sled.
//! Gossip entries are intentionally NOT persisted — they are re-fetched from
//! peers via anti-entropy after restart. What persists:
//! - Vector clock (causal ordering continuity)
//! - Topic metadata (name, ACL, scope)
//! - Topic subscriptions (which DIDs are subscribed to which topics)
//!
//! ## Key difference from ledger/governance restart helpers
//!
//! - No sled file lock to release — the JSON snapshot is written atomically
//!   and closed on drop.
//! - `new_current_thread()` runtime is sufficient — no `block_in_place` call
//!   in the gossip path (unlike ledger's `submit_treasury_entry`).
//! - DIDs generated in the write phase are printed to stdout and passed to
//!   the read phase as CLI args (parallel to ledger's entry hash handoff).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use icn_gossip::{AccessControl, GossipActor, GossipHandle, Topic};
use icn_identity::KeyPair;
use icn_snapshot::{save_snapshot, StateSnapshot};
use std::{path::PathBuf, process};

const PROOF_TOPIC: &str = "layer-4-cross-process-proof";

fn main() {
    // Current-thread runtime is sufficient: no block_in_place in gossip path.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let args: Vec<String> = std::env::args().collect();
    let exit_code = match args.get(1).map(String::as_str) {
        Some("write") => {
            let data_dir = PathBuf::from(args.get(2).expect("write: missing data_dir"));
            rt.block_on(run_write(data_dir))
        }
        Some("read") => {
            let data_dir = PathBuf::from(args.get(2).expect("read: missing data_dir"));
            let own_did_str = args.get(3).expect("read: missing own_did").clone();
            let subscriber_did_str = args.get(4).expect("read: missing subscriber_did").clone();
            run_read(data_dir, own_did_str, subscriber_did_str)
        }
        _ => {
            eprintln!(
                "usage: gossip_restart_helper <write|read> <data_dir> [own_did] [subscriber_did]"
            );
            1
        }
    };

    // Drop runtime before exit so all async cleanup (snapshot file handles) runs.
    drop(rt);
    process::exit(exit_code);
}

/// Write phase: build gossip coordination state through the handle path,
/// persist snapshot, print "own_did,subscriber_did" to stdout.
async fn run_write(data_dir: PathBuf) -> i32 {
    let own_kp = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: own KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let own_did = own_kp.did().clone();

    let subscriber_kp = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: subscriber KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let subscriber_did = subscriber_kp.did().clone();

    // Build state through GossipHandle — the production path.
    let handle: GossipHandle = GossipActor::spawn(own_did.clone(), None);

    {
        let mut g = handle.write().await;
        g.create_topic(Topic::new(PROOF_TOPIC.to_string(), AccessControl::Public));

        if let Err(e) = g
            .publish(PROOF_TOPIC, b"layer-4-cross-process-payload".to_vec())
            .await
        {
            eprintln!("write: publish failed: {e}");
            return 1;
        }

        if let Err(e) = g.subscribe(PROOF_TOPIC, subscriber_did.clone()).await {
            eprintln!("write: subscribe failed: {e}");
            return 1;
        }
        // write guard drops here
    }

    // Export via read lock — same path as supervisor shutdown.rs.
    let gossip_state = handle.read().await.export_state();

    let mut snapshot = StateSnapshot::new();
    snapshot.gossip_state = Some(gossip_state);

    if let Err(e) = save_snapshot(&snapshot, &data_dir) {
        eprintln!("write: save_snapshot failed: {e}");
        return 1;
    }

    // Print own_did and subscriber_did for the read phase to verify.
    // Format: "own_did,subscriber_did" — a single line.
    println!("{},{}", own_did, subscriber_did);
    0
}

/// Read phase: load snapshot, restore into fresh GossipActor, assert invariants.
fn run_read(data_dir: PathBuf, own_did_str: String, subscriber_did_str: String) -> i32 {
    use icn_identity::Did;

    let own_did: Did = match Did::from_str(&own_did_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read: invalid own_did {own_did_str:?}: {e}");
            return 1;
        }
    };
    let subscriber_did: Did = match Did::from_str(&subscriber_did_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read: invalid subscriber_did {subscriber_did_str:?}: {e}");
            return 1;
        }
    };

    // Load snapshot from disk — no shared memory from the write process.
    let snapshot = match icn_snapshot::load_snapshot(&data_dir) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("read: no snapshot found at {}", data_dir.display());
            return 1;
        }
        Err(e) => {
            eprintln!("read: load_snapshot failed: {e}");
            return 1;
        }
    };

    let gossip_state = match snapshot.gossip_state {
        Some(s) => s,
        None => {
            eprintln!("read: snapshot has no gossip_state");
            return 1;
        }
    };

    // Fresh actor — no prior state.
    let mut actor = GossipActor::new(own_did.clone(), None);

    if let Err(e) = actor.restore_state(gossip_state) {
        eprintln!("read: restore_state failed: {e}");
        return 1;
    }

    // Assert 1: topic name survives process boundary.
    let topics = actor.get_topics();
    if !topics.contains(&PROOF_TOPIC.to_string()) {
        eprintln!(
            "read: topic {PROOF_TOPIC:?} not found after cross-process restart, got: {topics:?}"
        );
        return 1;
    }

    // Assert 2: subscriber DID survives process boundary.
    let subscribers = actor.get_subscribers(PROOF_TOPIC);
    if !subscribers.iter().any(|d| d == &subscriber_did) {
        eprintln!(
            "read: subscriber DID {subscriber_did_str:?} not found after restart, got: {subscribers:?}"
        );
        return 1;
    }

    // Assert 3: vector clock for own_did survives with exact count.
    let clock_val = actor.get_clock().get(&own_did);
    if clock_val != 1 {
        eprintln!("read: vector clock for own_did must be 1 after one publish, got: {clock_val}");
        return 1;
    }

    0
}
