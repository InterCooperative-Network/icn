#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Gossip subscription-control containment (issue #2471).
//!
//! These tests drive the **production** incoming-message handler
//! (`icn_core::supervisor::init_network::create_incoming_handler`) against a real
//! `GossipActor`, because that is the code path a remote peer actually reaches:
//! `NetworkMessage` -> `connection.rs` catch-all arm -> supervisor handler ->
//! `GossipActor::{subscribe,unsubscribe}` -> `store_entry` -> notification callbacks.
//!
//! Threat model: `NetworkMessage.from` is self-declared. TLS is TOFU with
//! `client_auth_mandatory() = false`, and nothing rebinds per-message `from` to the
//! Hello identity. So every `from` below is simply what an attacker chose to write.
//!
//! These tests do **not** assert that gossip is authenticated. They assert two
//! containment properties that hold *despite* gossip being unauthenticated:
//!
//! 1. A received message cannot add or remove the receiving node's own subscription.
//! 2. Peer-controlled subscriber state cannot multiply or suppress local callback delivery.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use icn_core::supervisor::init_network::{create_incoming_handler, MessageHandlerDeps};
use icn_gossip::{
    AccessControl, EntryNotificationCallback, GossipActor, GossipEntry, GossipMessage, Topic,
    VectorClock,
};
use icn_identity::{Did, KeyPair};
use icn_net::{IncomingMessageHandler, NetworkMessage};
use tokio::sync::RwLock;

const TOPIC: &str = "coop:updates";

fn did() -> Did {
    KeyPair::generate().expect("keypair").did().clone()
}

/// Build a gossip actor with `TOPIC` created, plus the production incoming handler wired to it.
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

/// The production handler dispatches onto `tokio::spawn`. Poll until `cond` or give up.
///
/// Returns the final observed value either way, so callers assert on it rather than on
/// this helper — a timeout must never be reported as a pass.
async fn settle<F>(mut cond: F) -> bool
where
    F: FnMut() -> bool,
{
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    cond()
}

/// Counting callback plus its counter.
fn counting_callback() -> (EntryNotificationCallback, Arc<Mutex<u32>>) {
    let count = Arc::new(Mutex::new(0u32));
    let c = count.clone();
    let cb: EntryNotificationCallback = Arc::new(move |_topic, _entry, _recipient| {
        *c.lock().expect("counter") += 1;
    });
    (cb, count)
}

/// A well-formed entry that did not come from us. `author` is attacker-chosen and stays
/// unauthenticated: since #2469 slice 2 the receiver re-derives `hash` from the payload, but
/// a correct content hash identifies the *bytes*, never the peer that sent them.
///
/// `marker` varies the payload rather than the hash, so repeated injections remain distinct
/// entries now that the digest is a function of the content.
fn remote_entry(author: &Did, marker: u8) -> GossipEntry {
    let data = format!("remote entry {marker}").into_bytes();
    GossipEntry {
        hash: icn_gossip::content_hash(&data),
        author: author.clone(),
        clock: VectorClock::new(),
        topic: TOPIC.to_string(),
        data,
        compressed: false,
        timestamp: 1_700_000_000_000,
        replica_offered: None,
    }
}

// ---------------------------------------------------------------------------
// 1. Forged local unsubscribe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn forged_unsubscribe_claiming_local_did_cannot_remove_local_subscription() {
    let own = did();
    let attacker = did();
    let (gossip, handler) = harness(&own).await;

    // A real local subscription, created through the legitimate internal path.
    gossip
        .write()
        .await
        .subscribe(TOPIC, own.clone())
        .await
        .expect("local subscribe");
    assert!(
        gossip.read().await.is_subscribed(TOPIC, &own),
        "precondition: local node is subscribed"
    );

    // Attacker sends Unsubscribe with `from` spoofed to the victim's own DID.
    handler(NetworkMessage::unsubscribe(
        own.clone(),
        attacker.clone(),
        vec![TOPIC.to_string()],
    ));

    // Give the spawned task a chance to do damage before asserting it did not.
    let removed = settle(|| {
        let g = gossip.try_read();
        matches!(g, Ok(ref g) if !g.is_subscribed(TOPIC, &own))
    })
    .await;

    assert!(
        !removed,
        "a received Unsubscribe claiming the local DID must not remove the local subscription"
    );
    assert!(
        gossip.read().await.is_subscribed(TOPIC, &own),
        "local subscription must survive a forged Unsubscribe"
    );
}

// ---------------------------------------------------------------------------
// 2. Forged local subscribe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn forged_subscribe_claiming_local_did_cannot_create_local_subscription() {
    let own = did();
    let attacker = did();
    let (gossip, handler) = harness(&own).await;

    assert!(
        !gossip.read().await.is_subscribed(TOPIC, &own),
        "precondition: local node has not subscribed"
    );

    handler(NetworkMessage::subscribe(
        own.clone(),
        attacker.clone(),
        vec![TOPIC.to_string()],
    ));

    let created = settle(|| {
        let g = gossip.try_read();
        matches!(g, Ok(ref g) if g.is_subscribed(TOPIC, &own))
    })
    .await;

    assert!(
        !created,
        "a received Subscribe claiming the local DID must not create a local subscription"
    );
}

// ---------------------------------------------------------------------------
// 3. Delivery survives the attack
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_callback_delivery_survives_forged_unsubscribe() {
    let own = did();
    let attacker = did();
    let (gossip, handler) = harness(&own).await;

    let (cb_a, count_a) = counting_callback();
    let (cb_b, count_b) = counting_callback();
    {
        let mut g = gossip.write().await;
        g.add_notification_callback(cb_a);
        g.add_notification_callback(cb_b);
        g.subscribe(TOPIC, own.clone()).await.expect("subscribe");
    }

    // Attack first...
    handler(NetworkMessage::unsubscribe(
        own.clone(),
        attacker.clone(),
        vec![TOPIC.to_string()],
    ));
    settle(|| false).await; // let the spawned task run to completion

    // ...then a validly formed entry arrives from a peer.
    gossip
        .write()
        .await
        .handle_message(
            &attacker,
            GossipMessage::Response {
                entry: remote_entry(&attacker, 1),
            },
        )
        .await
        .expect("handle Response");

    assert_eq!(
        *count_a.lock().unwrap(),
        1,
        "first callback must receive the entry exactly once after a forged Unsubscribe"
    );
    assert_eq!(
        *count_b.lock().unwrap(),
        1,
        "second callback must receive the entry exactly once after a forged Unsubscribe"
    );
}

// ---------------------------------------------------------------------------
// 4. Amplification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn attacker_subscribers_cannot_multiply_local_callback_delivery() {
    let own = did();
    let (gossip, handler) = harness(&own).await;

    let (cb_a, count_a) = counting_callback();
    let (cb_b, count_b) = counting_callback();
    {
        let mut g = gossip.write().await;
        g.add_notification_callback(cb_a);
        g.add_notification_callback(cb_b);
        g.subscribe(TOPIC, own.clone()).await.expect("subscribe");
    }

    // 25 distinct attacker-controlled peers each subscribe themselves. Every one of these
    // is a *legitimate* remote subscribe under current protocol semantics — the point is
    // that none of them may influence how often local callbacks run.
    let attackers: Vec<Did> = (0..25).map(|_| did()).collect();
    for a in &attackers {
        handler(NetworkMessage::subscribe(
            a.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
    }
    let all_subscribed = settle(|| match gossip.try_read() {
        Ok(g) => attackers.iter().all(|a| g.is_subscribed(TOPIC, a)),
        Err(_) => false,
    })
    .await;
    assert!(
        all_subscribed,
        "attacker peers should have subscribed themselves"
    );

    // Exactly one entry is stored.
    gossip
        .write()
        .await
        .handle_message(
            &attackers[0],
            GossipMessage::Response {
                entry: remote_entry(&attackers[0], 2),
            },
        )
        .await
        .expect("handle Response");

    assert_eq!(
        *count_a.lock().unwrap(),
        1,
        "one stored entry must produce exactly one invocation of the first callback, \
         regardless of how many remote subscribers exist"
    );
    assert_eq!(
        *count_b.lock().unwrap(),
        1,
        "one stored entry must produce exactly one invocation of the second callback"
    );
}

// ---------------------------------------------------------------------------
// 5. Legitimate remote behavior is preserved
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remote_peer_can_still_subscribe_and_unsubscribe_itself() {
    let own = did();
    let peer = did();
    let (gossip, handler) = harness(&own).await;

    handler(NetworkMessage::subscribe(
        peer.clone(),
        own.clone(),
        vec![TOPIC.to_string()],
    ));
    let subscribed = settle(|| match gossip.try_read() {
        Ok(g) => g.is_subscribed(TOPIC, &peer),
        Err(_) => false,
    })
    .await;
    assert!(
        subscribed,
        "a remote peer must still be able to subscribe itself"
    );

    handler(NetworkMessage::unsubscribe(
        peer.clone(),
        own.clone(),
        vec![TOPIC.to_string()],
    ));
    let unsubscribed = settle(|| match gossip.try_read() {
        Ok(g) => !g.is_subscribed(TOPIC, &peer),
        Err(_) => false,
    })
    .await;
    assert!(
        unsubscribed,
        "a remote peer must still be able to unsubscribe itself"
    );
}

// ---------------------------------------------------------------------------
// 6. No-remote-subscriber behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_delivery_does_not_depend_on_remote_subscribers_existing() {
    let own = did();
    let sender = did();
    let (gossip, _handler) = harness(&own).await;

    let (cb, count) = counting_callback();
    {
        let mut g = gossip.write().await;
        g.add_notification_callback(cb);
        g.subscribe(TOPIC, own.clone()).await.expect("subscribe");
    }

    assert_eq!(
        gossip.read().await.get_subscribers(TOPIC),
        vec![own.clone()],
        "precondition: the only subscriber is the local node itself"
    );

    gossip
        .write()
        .await
        .handle_message(
            &sender,
            GossipMessage::Response {
                entry: remote_entry(&sender, 3),
            },
        )
        .await
        .expect("handle Response");

    assert_eq!(
        *count.lock().unwrap(),
        1,
        "local delivery must not disappear merely because no remote peer subscribed"
    );
}

/// A topic the local node never subscribed to must not deliver into local callbacks —
/// entry-driven dispatch must stay gated on the node's own, locally-owned subscription.
#[tokio::test]
async fn entries_on_never_subscribed_topics_do_not_reach_local_callbacks() {
    let own = did();
    let sender = did();
    let (gossip, _handler) = harness(&own).await;

    let (cb, count) = counting_callback();
    gossip.write().await.add_notification_callback(cb);
    // deliberately no local subscribe

    gossip
        .write()
        .await
        .handle_message(
            &sender,
            GossipMessage::Response {
                entry: remote_entry(&sender, 4),
            },
        )
        .await
        .expect("handle Response");

    assert_eq!(
        *count.lock().unwrap(),
        0,
        "a topic the node never subscribed to must not deliver into local callbacks"
    );
    assert_eq!(
        gossip.read().await.get_entries(TOPIC).len(),
        1,
        "the entry is still stored — storage is independent of local delivery"
    );
}

// ---------------------------------------------------------------------------
// 7. Generic gossip behavior is preserved
// ---------------------------------------------------------------------------

#[tokio::test]
async fn storage_and_vector_clock_merge_survive_subscription_attacks() {
    let own = did();
    let attacker = did();
    let (gossip, handler) = harness(&own).await;

    gossip
        .write()
        .await
        .subscribe(TOPIC, own.clone())
        .await
        .expect("subscribe");

    handler(NetworkMessage::unsubscribe(
        own.clone(),
        attacker.clone(),
        vec![TOPIC.to_string()],
    ));
    settle(|| false).await;

    let mut clock = VectorClock::new();
    clock.increment(&attacker);
    clock.increment(&attacker);
    let mut entry = remote_entry(&attacker, 5);
    entry.clock = clock;

    gossip
        .write()
        .await
        .handle_message(&attacker, GossipMessage::Response { entry })
        .await
        .expect("handle Response");

    let g = gossip.read().await;
    assert_eq!(g.get_entries(TOPIC).len(), 1, "entry must still be stored");
    assert_eq!(
        g.get_clock().get(&attacker),
        2,
        "the remote vector clock must still be merged"
    );
}

// ---------------------------------------------------------------------------
// 8. Malformed and adversarial input
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_and_repeated_subscription_control_stays_bounded() {
    let own = did();
    let peer = did();
    let (gossip, handler) = harness(&own).await;

    gossip
        .write()
        .await
        .subscribe(TOPIC, own.clone())
        .await
        .expect("subscribe");

    // Unknown, empty, oversized and control-character topics must not panic the handler.
    let junk = vec![
        String::new(),
        "no:such:topic".to_string(),
        "\u{0}\u{1}\u{7f}".to_string(),
        "x".repeat(64 * 1024),
    ];
    handler(NetworkMessage::subscribe(
        peer.clone(),
        own.clone(),
        junk.clone(),
    ));
    handler(NetworkMessage::unsubscribe(peer.clone(), own.clone(), junk));

    // Repeated identical requests must not grow the subscriber list without bound,
    // and repeated forged own-DID requests must never take effect.
    for _ in 0..50 {
        handler(NetworkMessage::subscribe(
            peer.clone(),
            own.clone(),
            vec![TOPIC.to_string()],
        ));
        handler(NetworkMessage::subscribe(
            own.clone(),
            peer.clone(),
            vec![TOPIC.to_string()],
        ));
        handler(NetworkMessage::unsubscribe(
            own.clone(),
            peer.clone(),
            vec![TOPIC.to_string()],
        ));
    }
    settle(|| false).await;

    let g = gossip.read().await;
    let subs = g.get_subscribers(TOPIC);
    assert!(
        subs.len() <= 2,
        "repeated subscribe requests must be deduplicated, got {} subscribers",
        subs.len()
    );
    assert!(
        g.is_subscribed(TOPIC, &own),
        "repeated forged own-DID Unsubscribe must never remove the local subscription"
    );
}
