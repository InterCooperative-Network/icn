# Topic Subscriptions API

This document provides a comprehensive guide to using the topic subscription system in ICN.

## Overview

Topic subscriptions allow peers to express interest in specific gossip topics and manage which peers receive updates. The subscription system provides:

- **Access control**: Topics can enforce trust requirements
- **Explicit subscription**: Peers must subscribe before receiving updates
- **Query capabilities**: Inspect subscription state for debugging and monitoring
- **Metrics**: Track subscription activity via Prometheus

## Table of Contents

1. [GossipActor API](#gossipactor-api)
2. [Network Protocol](#network-protocol)
3. [Usage Examples](#usage-examples)
4. [Access Control](#access-control)
5. [Metrics](#metrics)
6. [Testing](#testing)

---

## GossipActor API

### Subscribe to a Topic

```rust
pub fn subscribe(&mut self, topic: &str, subscriber: Did) -> Result<Subscription>
```

Subscribe a DID to a topic. Performs ACL checks based on the topic's `AccessControl` policy.

**Parameters:**
- `topic`: Topic name (e.g., "global:identity", "contract:abc123")
- `subscriber`: DID of the subscribing peer

**Returns:**
- `Ok(Subscription)`: Subscription successful
- `Err`: Topic not found or ACL check failed

**Example:**
```rust
let subscription = gossip.subscribe("global:identity", peer_did.clone())?;
info!("Subscribed {} to {}", subscription.subscriber, subscription.topic);
```

**Notes:**
- Duplicate subscriptions are ignored (idempotent)
- Increments `icn_gossip_subscriptions_total` metric
- Logs subscription at INFO level

---

### Unsubscribe from a Topic

```rust
pub fn unsubscribe(&mut self, topic: &str, subscriber: &Did) -> Result<()>
```

Remove a DID's subscription to a topic.

**Parameters:**
- `topic`: Topic name
- `subscriber`: DID to unsubscribe

**Returns:**
- `Ok(())`: Unsubscription successful or DID was not subscribed
- `Err`: Topic not found

**Example:**
```rust
gossip.unsubscribe("global:identity", &peer_did)?;
info!("Unsubscribed {} from global:identity", peer_did);
```

**Notes:**
- No-op if DID is not subscribed (safe to call multiple times)
- Decrements `icn_gossip_subscriptions_total` metric
- Logs unsubscription at INFO level

---

### Get Subscribers for a Topic

```rust
pub fn get_subscribers(&self, topic: &str) -> Vec<Did>
```

Query all DIDs subscribed to a topic.

**Parameters:**
- `topic`: Topic name

**Returns:**
- Vector of subscriber DIDs (empty if topic has no subscribers)

**Example:**
```rust
let subscribers = gossip.get_subscribers("global:identity");
info!("Topic has {} subscribers", subscribers.len());
for subscriber in subscribers {
    info!("  - {}", subscriber);
}
```

---

### Get Subscriptions for a DID

```rust
pub fn get_subscriptions(&self, did: &Did) -> Vec<String>
```

Query all topics a DID is subscribed to.

**Parameters:**
- `did`: DID to query

**Returns:**
- Vector of topic names (empty if DID has no subscriptions)

**Example:**
```rust
let subscriptions = gossip.get_subscriptions(&peer_did);
info!("{} is subscribed to {} topics", peer_did, subscriptions.len());
for topic in subscriptions {
    info!("  - {}", topic);
}
```

---

### Check Subscription Status

```rust
pub fn is_subscribed(&self, topic: &str, did: &Did) -> bool
```

Check if a DID is subscribed to a topic.

**Parameters:**
- `topic`: Topic name
- `did`: DID to check

**Returns:**
- `true` if subscribed, `false` otherwise

**Example:**
```rust
if gossip.is_subscribed("global:identity", &peer_did) {
    info!("{} is subscribed to global:identity", peer_did);
} else {
    warn!("{} is NOT subscribed", peer_did);
}
```

---

## Network Protocol

### Subscribe Message

Send a `Subscribe` message to request subscription to one or more topics on a peer.

```rust
let subscribe_msg = NetworkMessage::subscribe(
    own_did,
    peer_did.clone(),
    vec!["global:identity".to_string(), "global:rendezvous".to_string()],
);

network_handle.send_message(peer_did, subscribe_msg).await?;
```

**Flow:**
1. Node A sends `Subscribe {topics: [...]}` to Node B
2. Node B's supervisor receives message
3. For each topic:
   - Call `gossip.subscribe(topic, sender_did)`
   - ACL check performed
   - Add to subscribers if authorized
4. Send `SubscribeAck {topics: [...]}` back with successful subscriptions

---

### Unsubscribe Message

Send an `Unsubscribe` message to cancel subscriptions.

```rust
let unsubscribe_msg = NetworkMessage::unsubscribe(
    own_did,
    peer_did.clone(),
    vec!["global:identity".to_string()],
);

network_handle.send_message(peer_did, unsubscribe_msg).await?;
```

**Flow:**
1. Node A sends `Unsubscribe {topics: [...]}` to Node B
2. Node B's supervisor receives message
3. For each topic:
   - Call `gossip.unsubscribe(topic, &sender_did)`
   - Remove from subscribers
4. No acknowledgment sent for unsubscribe

---

### SubscribeAck Message

Acknowledgment sent by the peer after successful subscription.

```rust
// Sent automatically by supervisor - application receives it via incoming handler
MessagePayload::SubscribeAck { topics } => {
    info!("Successfully subscribed to: {:?}", topics);
}
```

---

## Usage Examples

### Example 1: Subscribe to Multiple Topics

```rust
use icn_net::NetworkMessage;

// Send subscription request
let subscribe_msg = NetworkMessage::subscribe(
    my_did.clone(),
    peer_did.clone(),
    vec![
        "global:identity".to_string(),
        "global:rendezvous".to_string(),
        "ledger:hours".to_string(),
    ],
);

network_handle.send_message(peer_did, subscribe_msg).await?;
```

### Example 2: Handle Incoming Subscription Requests

```rust
use icn_net::{MessagePayload, NetworkMessage};

let incoming_handler = Arc::new(move |net_msg| {
    match net_msg.payload {
        MessagePayload::Subscribe { topics } => {
            let sender = net_msg.from.clone();
            let gossip = gossip_handle.clone();
            let net = network_handle.clone();

            // Keep callback non-blocking; do async state mutation in a spawned task.
            tokio::spawn(async move {
                let mut gossip = gossip.write().await;
                let mut acked_topics = Vec::new();

                for topic in &topics {
                    match gossip.subscribe(topic, sender.clone()).await {
                        Ok(_) => acked_topics.push(topic.clone()),
                        Err(e) => warn!("Subscription denied for {}: {}", topic, e),
                    }
                }

                if !acked_topics.is_empty() {
                    let ack = NetworkMessage::subscribe_ack(own_did, sender.clone(), acked_topics);
                    // Send ack via network_handle...
                    let _ = net.send_message(sender, ack).await;
                }
            });
        }
        _ => {}
    }
});
```

### Example 3: Query Subscription State

```rust
// Get all subscribers for a topic
let subscribers = gossip.get_subscribers("global:identity");
println!("global:identity has {} subscribers:", subscribers.len());
for did in subscribers {
    println!("  {}", did);
}

// Get all topics a peer is subscribed to
let subscriptions = gossip.get_subscriptions(&peer_did);
println!("{} is subscribed to:", peer_did);
for topic in subscriptions {
    println!("  {}", topic);
}

// Check specific subscription
if gossip.is_subscribed("ledger:hours", &peer_did) {
    println!("{} is subscribed to ledger:hours", peer_did);
}
```

---

## Access Control

Topics enforce access control policies that are checked during subscription:

### Public Topics

Anyone can subscribe:

```rust
let topic = Topic::new("global:identity".to_string(), AccessControl::Public);
gossip.create_topic(topic);

// Any DID can subscribe
gossip.subscribe("global:identity", any_did)?; // Always succeeds
```

### TrustClass-Gated Topics

Only peers with minimum trust level can subscribe:

```rust
let topic = Topic::new(
    "partner:ledger".to_string(),
    AccessControl::TrustClass(TrustClass::Partner),
);
gossip.create_topic(topic);

// Requires trust_lookup(did) >= TrustClass::Partner
gossip.subscribe("partner:ledger", trusted_did)?; // Succeeds
gossip.subscribe("partner:ledger", untrusted_did)?; // Fails with ACL error
```

### Participants-Only Topics

Only whitelisted DIDs can subscribe:

```rust
let topic = Topic::new(
    "contract:abc123".to_string(),
    AccessControl::Participants(vec![alice_did.clone(), bob_did.clone()]),
);
gossip.create_topic(topic);

// Only alice and bob can subscribe
gossip.subscribe("contract:abc123", alice_did)?; // Succeeds
gossip.subscribe("contract:abc123", charlie_did)?; // Fails - not in whitelist
```

---

## Metrics

The subscription system exports Prometheus metrics:

### Gauge Metrics

**`icn_gossip_subscriptions_total`**
- Type: Gauge
- Description: Total number of active subscriptions across all topics
- Updates: Automatically updated on subscribe/unsubscribe

### Counter Metrics

**`icn_gossip_subscribes_received_total`**
- Type: Counter
- Description: Total Subscribe messages received
- Increments: When supervisor processes Subscribe message

**`icn_gossip_unsubscribes_received_total`**
- Type: Counter
- Description: Total Unsubscribe messages received
- Increments: When supervisor processes Unsubscribe message

**`icn_gossip_subscribe_acks_sent_total`**
- Type: Counter
- Description: Total SubscribeAck messages sent
- Increments: When supervisor sends SubscribeAck

### Querying Metrics

```bash
# View all subscription metrics
curl http://localhost:9090/metrics | grep icn_gossip_subscribe

# Example output:
# icn_gossip_subscriptions_total 15
# icn_gossip_subscribes_received_total 42
# icn_gossip_unsubscribes_received_total 7
# icn_gossip_subscribe_acks_sent_total 38
```

---

## Testing

### Unit Tests

Test subscription functionality without network:

```rust
#[test]
fn test_subscribe_and_query() {
    let mut gossip = GossipActor::new(did.clone(), trust_lookup);

    // Subscribe
    gossip.subscribe("global:identity", peer_did.clone()).unwrap();

    // Query
    assert!(gossip.is_subscribed("global:identity", &peer_did));
    assert_eq!(gossip.get_subscribers("global:identity").len(), 1);
    assert_eq!(gossip.get_subscriptions(&peer_did).len(), 1);

    // Unsubscribe
    gossip.unsubscribe("global:identity", &peer_did).unwrap();
    assert!(!gossip.is_subscribed("global:identity", &peer_did));
}
```

### Integration Tests

Test end-to-end subscription flow over network:

```bash
# Run integration tests (requires network interfaces)
cargo test -p icn-core --test subscription_integration -- --ignored
```

See `icn/crates/icn-core/tests/subscription_integration.rs` for examples.

---

## Limitations (v1)

The current implementation has the following limitations:

1. **No persistence**: Subscriptions are in-memory only and lost on restart
2. **No automatic resubscription**: Peers must manually resubscribe after reconnection
3. **No selective routing**: Broadcasts still go to all peers regardless of subscriptions
4. **No metadata**: No timestamps, preferences, or filters on subscriptions

These limitations will be addressed in future releases.

---

## See Also

- [ARCHITECTURE.md](../../ARCHITECTURE.md#66-topic-subscriptions) - Architecture documentation
- [CLAUDE.md](../../../CLAUDE.md) - Development guide
- Integration test examples: `icn/crates/icn-core/tests/subscription_integration.rs`
