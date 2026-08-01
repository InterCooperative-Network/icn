#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Behaviour-level tests for the subscription-control outcome counter (issue #2482).
//!
//! These drive the **production** handler
//! (`icn_core::supervisor::init_network::create_incoming_handler`) against a real
//! `GossipActor` and assert on what the metrics recorder actually observed. Asserting
//! only that the enums map to the right label strings would not catch a counter call
//! deleted, duplicated, or wired into the wrong match arm — which is the point.
//!
//! ## Exact-series assertions
//!
//! Every scenario asserts the **complete** set of emitted outcome series, not just that
//! the expected one equals 1. A per-label assertion would pass even if a stray call
//! landed under different labels; `assert_eq!` on the whole sorted vector cannot.
//!
//! ## Why a manually built current-thread runtime
//!
//! The handler dispatches onto `tokio::spawn`, and `metrics::with_local_recorder`
//! installs a **thread-local** recorder. On a multi-thread runtime the spawned task runs
//! on a worker thread that cannot see it, and every assertion would read zero. A
//! current-thread runtime polls spawned tasks on the calling thread, inside the
//! `with_local_recorder` closure, so the recorder is in scope when the counter fires.
//!
//! The recorder is local, not global, so tests do not interfere with each other.

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
/// non-spoof reason (`Topic not found`).
const ABSENT_TOPIC: &str = "coop:does-not-exist";

/// Bounded settle budget: the handler dispatches onto `tokio::spawn`, so the counter
/// fires after the call returns. 200 * 5ms mirrors the `settle()` budget already used by
/// `gossip_subscription_control.rs`, but unlike that helper this one exits as soon as the
/// expected number of events is observed rather than always sleeping the full budget.
const SETTLE_TICKS: u32 = 200;
const SETTLE_TICK: Duration = Duration::from_millis(5);

fn did() -> Did {
    KeyPair::generate().expect("keypair").did().clone()
}

/// Every emitted series for the outcome metric, as sorted `(action, outcome, value)`.
fn outcome_series(snapshotter: &Snapshotter) -> Vec<(String, String, u64)> {
    let mut out: Vec<(String, String, u64)> = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| {
            let k = key.key();
            if k.name() != OUTCOME_METRIC {
                return None;
            }
            let mut action = String::new();
            let mut outcome = String::new();
            for label in k.labels() {
                match label.key() {
                    "action" => action = label.value().to_string(),
                    "outcome" => outcome = label.value().to_string(),
                    _ => {}
                }
            }
            match value {
                DebugValue::Counter(v) => Some((action, outcome, v)),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out
}

/// Total outcome increments observed, across every label set.
fn outcome_event_total(snapshotter: &Snapshotter) -> u64 {
    outcome_series(snapshotter).iter().map(|(_, _, v)| v).sum()
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

/// Convenience for building the expected exact-series vector.
fn series(items: &[(&str, &str, u64)]) -> Vec<(String, String, u64)> {
    let mut v: Vec<(String, String, u64)> = items
        .iter()
        .map(|(a, o, n)| (a.to_string(), o.to_string(), *n))
        .collect();
    v.sort();
    v
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

/// Run `act` under a thread-local recorder on a current-thread runtime, waiting until
/// `expected_events` outcome increments have been observed.
///
/// `seed_subscribed` subscribes `peer` to `TOPIC` through the **actor API** rather than
/// the network handler. That path records no outcome (the counter lives only in the
/// supervisor handler), so a test needing pre-existing subscription state still observes
/// exactly one series — its own. That is why unsubscribe tests do not have to assert a
/// two-series vector containing an unrelated setup subscribe.
///
/// When `expected_events` is 0 the full budget is waited out, because "nothing happened"
/// cannot be detected early.
fn run_observed(
    seed_subscribed: bool,
    expected_events: u64,
    act: impl FnOnce(&Did, &Did, &IncomingMessageHandler),
) -> Snapshotter {
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

            if seed_subscribed {
                gossip
                    .write()
                    .await
                    .subscribe(TOPIC, peer.clone())
                    .await
                    .expect("seed subscribe");
                assert_eq!(
                    outcome_event_total(&snapshotter),
                    0,
                    "seeding via the actor API must not emit an outcome series"
                );
            }

            act(&own, &peer, &handler);

            for _ in 0..SETTLE_TICKS {
                if expected_events > 0 && outcome_event_total(&snapshotter) >= expected_events {
                    return;
                }
                tokio::time::sleep(SETTLE_TICK).await;
            }

            if expected_events > 0 {
                panic!(
                    "timed out waiting for {expected_events} outcome event(s); observed {:?}",
                    outcome_series(&snapshotter)
                );
            }
        });
    });

    snapshotter
}

// ---------------------------------------------------------------------------
// Subscribe: all three outcomes, each asserted as the COMPLETE emitted set
// ---------------------------------------------------------------------------

#[test]
fn subscribe_success_emits_only_one_processed_series() {
    let snap = run_observed(false, 1, |own, peer, handler| {
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(
        outcome_series(&snap),
        series(&[("subscribe", "processed", 1)])
    );
    assert_eq!(plain_count(&snap, SPOOF_METRIC), 0);
}

#[test]
fn subscribe_forged_own_did_emits_only_one_rejected_own_did_series() {
    let snap = run_observed(false, 1, |own, _peer, handler| {
        // `from` claims the receiving node's own DID — the #2471 attack.
        handler(NetworkMessage::subscribe(
            own.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(
        outcome_series(&snap),
        series(&[("subscribe", "rejected_own_did", 1)])
    );
    // The #2474 security counter must still fire, independently of this one.
    assert_eq!(
        plain_count(&snap, SPOOF_METRIC),
        1,
        "the pre-existing spoof counter must remain intact and independent"
    );
}

#[test]
fn subscribe_non_spoof_failure_emits_only_one_rejected_or_error_series() {
    let snap = run_observed(false, 1, |own, peer, handler| {
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![ABSENT_TOPIC.to_string()],
        ));
    });

    assert_eq!(
        outcome_series(&snap),
        series(&[("subscribe", "rejected_or_error", 1)])
    );
    assert_eq!(
        plain_count(&snap, SPOOF_METRIC),
        0,
        "a non-spoof failure must not touch the spoof counter"
    );
}

// ---------------------------------------------------------------------------
// Unsubscribe: all three outcomes, each asserted as the COMPLETE emitted set
// ---------------------------------------------------------------------------

#[test]
fn unsubscribe_success_emits_only_one_processed_series() {
    // Seeded through the actor API, so the recorder observes only the unsubscribe.
    let snap = run_observed(true, 1, |own, peer, handler| {
        handler(NetworkMessage::unsubscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(
        outcome_series(&snap),
        series(&[("unsubscribe", "processed", 1)])
    );
    assert_eq!(plain_count(&snap, SPOOF_METRIC), 0);
}

#[test]
fn unsubscribe_forged_own_did_emits_only_one_rejected_own_did_series() {
    let snap = run_observed(false, 1, |own, _peer, handler| {
        handler(NetworkMessage::unsubscribe(
            own.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    });

    assert_eq!(
        outcome_series(&snap),
        series(&[("unsubscribe", "rejected_own_did", 1)])
    );
    assert_eq!(plain_count(&snap, SPOOF_METRIC), 1);
}

#[test]
fn unsubscribe_non_spoof_failure_emits_only_one_rejected_or_error_series() {
    let snap = run_observed(false, 1, |own, peer, handler| {
        // `unsubscribe` errors with "Topic not found" for a topic never created.
        handler(NetworkMessage::unsubscribe(
            peer.clone(),
            own.clone(),
            vec![ABSENT_TOPIC.to_string()],
        ));
    });

    assert_eq!(
        outcome_series(&snap),
        series(&[("unsubscribe", "rejected_or_error", 1)])
    );
    assert_eq!(plain_count(&snap, SPOOF_METRIC), 0);
}

// ---------------------------------------------------------------------------
// Per-topic accounting, isolation, and non-interference
// ---------------------------------------------------------------------------

#[test]
fn multi_topic_request_increments_once_per_topic() {
    let snap = run_observed(false, 3, |own, peer, handler| {
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
        outcome_series(&snap),
        series(&[
            ("subscribe", "processed", 1),
            ("subscribe", "rejected_or_error", 2),
        ]),
        "the outcome counter is per-topic, not per-message"
    );
    assert_eq!(
        plain_count(&snap, SUBSCRIBES_RECEIVED),
        1,
        "arrival is per-message and must NOT be inflated to per-topic by this change"
    );
}

#[test]
fn subscribe_ack_and_unrelated_payloads_emit_no_outcome_series() {
    let snap = run_observed(false, 0, |own, peer, handler| {
        handler(NetworkMessage::subscribe_ack(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
        handler(NetworkMessage::ping(peer.clone(), own.clone()));
    });

    assert_eq!(
        outcome_series(&snap),
        Vec::new(),
        "SubscribeAck and Ping must emit no subscription-control outcome series"
    );
}

#[test]
fn arrival_counters_are_not_double_counted() {
    let snap = run_observed(false, 2, |own, peer, handler| {
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

    // Exactly the two expected series and nothing else.
    assert_eq!(
        outcome_series(&snap),
        series(&[
            ("subscribe", "processed", 1),
            ("unsubscribe", "processed", 1),
        ])
    );
    // Each arrival counter fires exactly once per received message, unchanged by #2482.
    assert_eq!(plain_count(&snap, SUBSCRIBES_RECEIVED), 1);
    assert_eq!(plain_count(&snap, UNSUBSCRIBES_RECEIVED), 1);
}
