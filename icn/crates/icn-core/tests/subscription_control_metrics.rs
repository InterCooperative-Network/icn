#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Behaviour-level tests for the subscription-control outcome counter (issue #2482).
//!
//! These drive the **production** handler
//! (`icn_core::supervisor::init_network::create_incoming_handler`) against a real
//! `GossipActor` and assert on what the metrics recorder actually observed. Asserting
//! only that the label enums map to the right strings would not catch a counter call
//! deleted, duplicated, or wired into the wrong match arm — which is the whole point.
//!
//! ## Why a manually built current-thread runtime
//!
//! The handler dispatches onto `tokio::spawn`, and `metrics::with_local_recorder`
//! installs a **thread-local** recorder. On a multi-thread runtime the spawned task runs
//! on a worker thread that cannot see it, and every assertion would read zero. A
//! current-thread runtime polls spawned tasks on the calling thread, inside the
//! `with_local_recorder` closure, so the recorder is in scope when the counter fires.
//!
//! The recorder is local, not global, so these tests do not interfere with each other.

use std::sync::Arc;
use std::time::Duration;

use icn_core::supervisor::init_network::{create_incoming_handler, MessageHandlerDeps};
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_identity::{Did, KeyPair};
use icn_net::{IncomingMessageHandler, NetworkMessage};
use metrics::with_local_recorder;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use tokio::sync::RwLock;

const OUTCOME_METRIC: &str = "icn_gossip_subscription_control_outcome_total";
const SPOOF_METRIC: &str = "icn_gossip_subscription_control_spoof_rejected_total";
const SUBSCRIBES_RECEIVED: &str = "icn_gossip_subscribes_received_total";
const UNSUBSCRIBES_RECEIVED: &str = "icn_gossip_unsubscribes_received_total";

const TOPIC: &str = "coop:updates";
/// Never created on the actor, so subscribing/unsubscribing to it fails for a
/// non-spoof reason.
const ABSENT_TOPIC: &str = "coop:does-not-exist";

fn did() -> Did {
    KeyPair::generate().expect("keypair").did().clone()
}

/// Sum a counter restricted to one `(action, outcome)` label pair.
///
/// Label-aware on purpose: summing across all label sets (as the icn-ledger helper does)
/// would let a call recorded under the wrong labels still satisfy the assertion.
fn outcome_count(snapshotter: &Snapshotter, action: &str, outcome: &str) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| {
            let k = key.key();
            if k.name() != OUTCOME_METRIC {
                return None;
            }
            let mut got_action = None;
            let mut got_outcome = None;
            for label in k.labels() {
                match label.key() {
                    "action" => got_action = Some(label.value()),
                    "outcome" => got_outcome = Some(label.value()),
                    _ => {}
                }
            }
            if got_action == Some(action) && got_outcome == Some(outcome) {
                match value {
                    DebugValue::Counter(v) => Some(v),
                    _ => None,
                }
            } else {
                None
            }
        })
        .sum()
}

/// Total for an unlabelled counter, across every label set.
fn plain_count(snapshotter: &Snapshotter, name: &str) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| {
            if key.key().name() == name {
                match value {
                    DebugValue::Counter(v) => Some(v),
                    _ => None,
                }
            } else {
                None
            }
        })
        .sum()
}

/// Every emitted series for the outcome metric, as `(action, outcome, value)`.
///
/// Used to assert that a scenario produced *nothing except* what was expected — a plain
/// per-label assertion would pass even if a stray call landed under other labels.
fn all_outcome_series(snapshotter: &Snapshotter) -> Vec<(String, String, u64)> {
    let mut out: Vec<(String, String, u64)> = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| {
            let k = key.key();
            if k.name() != OUTCOME_METRIC {
                return None;
            }
            let mut a = String::new();
            let mut o = String::new();
            for label in k.labels() {
                match label.key() {
                    "action" => a = label.value().to_string(),
                    "outcome" => o = label.value().to_string(),
                    _ => {}
                }
            }
            match value {
                DebugValue::Counter(v) => Some((a, o, v)),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out
}

async fn harness(own: &Did) -> (Arc<RwLock<GossipActor>>, IncomingMessageHandler) {
    let gossip = Arc::new(RwLock::new(GossipActor::new(own.clone(), None)));
    gossip
        .write()
        .await
        .create_topic(Topic::new(TOPIC.to_string(), AccessControl::Public));

    let handler = create_incoming_handler(MessageHandlerDeps {
        gossip_handle: gossip.clone(),
        network_handle_holder: Arc::new(RwLock::new(None)),
        own_did: own.clone(),
        federation_enabled: false,
    });

    (gossip, handler)
}

/// Run `body` with a thread-local recorder installed and a current-thread runtime, then
/// hand back the snapshotter.
///
/// `body` gets the handler and the actor. After it returns, the runtime is driven a
/// little longer so the handler's spawned task reaches its counter call.
fn with_recorder<F>(body: F) -> Snapshotter
where
    F: FnOnce(&Did, &Did, IncomingMessageHandler, Arc<RwLock<GossipActor>>),
{
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    with_local_recorder(&recorder, || {
        rt.block_on(async {
            let own = did();
            let peer = did();
            let (gossip, handler) = harness(&own).await;
            body(&own, &peer, handler, gossip);
            // Let the spawned task run to completion. 200 * 5ms mirrors the settle()
            // budget in gossip_subscription_control.rs.
            for _ in 0..200 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
    });

    snapshotter
}

// ---------------------------------------------------------------------------
// Subscribe: all three outcomes
// ---------------------------------------------------------------------------

#[test]
fn accepted_subscribe_emits_exactly_one_processed() {
    let snap = with_recorder(|own, peer, handler, _g| {
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(outcome_count(&snap, "subscribe", "processed"), 1);
    assert_eq!(
        all_outcome_series(&snap),
        vec![("subscribe".into(), "processed".into(), 1)],
        "a successful subscribe must emit exactly one series and nothing else"
    );
}

#[test]
fn forged_own_did_subscribe_emits_exactly_one_rejected_own_did() {
    let snap = with_recorder(|own, _peer, handler, _g| {
        // `from` claims the receiving node's own DID — the #2471 attack.
        handler(NetworkMessage::subscribe(
            own.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(outcome_count(&snap, "subscribe", "rejected_own_did"), 1);
    assert_eq!(outcome_count(&snap, "subscribe", "processed"), 0);
    // The #2474 security counter must still fire, independently of this one.
    assert_eq!(
        plain_count(&snap, SPOOF_METRIC),
        1,
        "the pre-existing spoof counter must remain intact and independent"
    );
}

#[test]
fn non_spoof_subscribe_failure_emits_exactly_one_rejected_or_error() {
    let snap = with_recorder(|own, peer, handler, _g| {
        // Topic was never created on this actor: a genuine, non-spoof refusal.
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![ABSENT_TOPIC.to_string()],
        ));
    });

    assert_eq!(outcome_count(&snap, "subscribe", "rejected_or_error"), 1);
    assert_eq!(outcome_count(&snap, "subscribe", "rejected_own_did"), 0);
    assert_eq!(
        plain_count(&snap, SPOOF_METRIC),
        0,
        "a non-spoof failure must not touch the spoof counter"
    );
}

// ---------------------------------------------------------------------------
// Unsubscribe: all three outcomes
// ---------------------------------------------------------------------------

#[test]
fn accepted_unsubscribe_emits_exactly_one_processed() {
    let snap = with_recorder(|own, peer, handler, _g| {
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
        handler(NetworkMessage::unsubscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(outcome_count(&snap, "unsubscribe", "processed"), 1);
    assert_eq!(outcome_count(&snap, "subscribe", "processed"), 1);
}

#[test]
fn forged_own_did_unsubscribe_emits_exactly_one_rejected_own_did() {
    let snap = with_recorder(|own, _peer, handler, _g| {
        handler(NetworkMessage::unsubscribe(
            own.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(outcome_count(&snap, "unsubscribe", "rejected_own_did"), 1);
    assert_eq!(outcome_count(&snap, "unsubscribe", "processed"), 0);
    assert_eq!(plain_count(&snap, SPOOF_METRIC), 1);
}

#[test]
fn non_spoof_unsubscribe_failure_emits_exactly_one_rejected_or_error() {
    let snap = with_recorder(|own, peer, handler, _g| {
        // `unsubscribe` errors with "Topic not found" for a topic that was never created.
        handler(NetworkMessage::unsubscribe(
            peer.clone(),
            own.clone(),
            vec![ABSENT_TOPIC.to_string()],
        ));
    });

    assert_eq!(outcome_count(&snap, "unsubscribe", "rejected_or_error"), 1);
    assert_eq!(outcome_count(&snap, "unsubscribe", "rejected_own_did"), 0);
    assert_eq!(plain_count(&snap, SPOOF_METRIC), 0);
}

// ---------------------------------------------------------------------------
// Per-topic accounting, isolation, and non-interference
// ---------------------------------------------------------------------------

#[test]
fn multi_topic_request_increments_once_per_topic() {
    let snap = with_recorder(|own, peer, handler, _g| {
        // Three topics in ONE message: one that exists, two that do not.
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![
                TOPIC.to_string(),
                ABSENT_TOPIC.to_string(),
                "coop:also-absent".to_string(),
            ],
        ));
    });

    assert_eq!(
        outcome_count(&snap, "subscribe", "processed"),
        1,
        "one existing topic -> exactly one processed"
    );
    assert_eq!(
        outcome_count(&snap, "subscribe", "rejected_or_error"),
        2,
        "two absent topics -> exactly two refusals; the counter is per-topic, not per-message"
    );
    assert_eq!(
        plain_count(&snap, SUBSCRIBES_RECEIVED),
        1,
        "arrival is per-message and must NOT be inflated to per-topic by this change"
    );
}

#[test]
fn subscribe_ack_and_unrelated_payloads_do_not_touch_the_outcome_counter() {
    let snap = with_recorder(|own, peer, handler, _g| {
        handler(NetworkMessage::subscribe_ack(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
        handler(NetworkMessage::ping(peer.clone(), own.clone()));
    });

    assert!(
        all_outcome_series(&snap).is_empty(),
        "SubscribeAck and Ping must emit no subscription-control outcome series, got {:?}",
        all_outcome_series(&snap)
    );
}

#[test]
fn arrival_counters_are_not_double_counted() {
    let snap = with_recorder(|own, peer, handler, _g| {
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
        handler(NetworkMessage::unsubscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    // Each arrival counter fires exactly once per received message, unchanged by #2482.
    assert_eq!(plain_count(&snap, SUBSCRIBES_RECEIVED), 1);
    assert_eq!(plain_count(&snap, UNSUBSCRIBES_RECEIVED), 1);
    // And the new counter is additional, not a replacement.
    assert_eq!(outcome_count(&snap, "subscribe", "processed"), 1);
    assert_eq!(outcome_count(&snap, "unsubscribe", "processed"), 1);
}
