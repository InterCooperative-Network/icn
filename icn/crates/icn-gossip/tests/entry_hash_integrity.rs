//! Content-integrity of remotely received gossip entries (#2469, slice 2).
//!
//! `GossipEntry.hash` is the SHA-256 of the entry's *logical* (uncompressed) payload:
//! `GossipActor::publish` hashes the caller's bytes and only then compresses, so the
//! contract a receiver must re-derive is `hash == sha256(entry.get_data()?)`.
//!
//! Every entry a peer supplies arrives with that hash already filled in, and the receive
//! path used it as a storage key without ever recomputing it. These tests drive the real
//! ingress paths (`GossipMessage::Response` and `GossipMessage::PullResponse`) through
//! `GossipActor::handle_message` and pin that a claimed hash is re-derived before it is
//! trusted.
//!
//! Scope note: matching hashes prove only that the payload corresponds to the claimed
//! digest. They authenticate neither `entry.author` nor any institutional authority.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use icn_gossip::{AccessControl, GossipActor, GossipEntry, GossipMessage, Topic, VectorClock};
use icn_identity::{Did, KeyPair};
use sha2::{Digest, Sha256};

const TOPIC: &str = "integrity:test";

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// An actor subscribed to `TOPIC` by its own DID, so notification callbacks actually fire
/// (`store_entry` gates local delivery on `local_topic_subscriptions`).
///
/// Returns the actor plus the hashes of every entry delivered to a callback.
async fn subscribed_actor() -> (GossipActor, Arc<Mutex<Vec<[u8; 32]>>>) {
    let own = KeyPair::generate().unwrap().did().clone();
    let mut gossip = GossipActor::new(own.clone(), None);
    gossip.create_topic(Topic::new(TOPIC.to_string(), AccessControl::Public));
    gossip.subscribe(TOPIC, own.clone()).await.unwrap();
    assert!(
        gossip.is_locally_subscribed(TOPIC),
        "precondition: the actor must be locally subscribed or callbacks never fire"
    );

    let delivered = Arc::new(Mutex::new(Vec::new()));
    let sink = delivered.clone();
    gossip.add_notification_callback(Arc::new(move |_topic, entry: GossipEntry, _from| {
        sink.lock().unwrap().push(entry.hash);
    }));

    (gossip, delivered)
}

fn entry_with(author: &Did, hash: [u8; 32], data: &[u8]) -> GossipEntry {
    GossipEntry {
        hash,
        author: author.clone(),
        clock: VectorClock::new(),
        topic: TOPIC.to_string(),
        data: data.to_vec(),
        compressed: false,
        timestamp: 1_700_000_000_000,
        replica_offered: None,
    }
}

/// RED-1: a `Response` entry whose claimed hash is the digest of *different* bytes must not
/// be stored, must not claim a dedup slot, and must not reach a notification callback.
#[tokio::test]
async fn response_entry_with_mismatched_hash_is_rejected_before_storage() {
    let (mut gossip, delivered) = subscribed_actor().await;
    let sender = KeyPair::generate().unwrap().did().clone();

    let honest_payload = b"the payload this digest actually commits to";
    let claimed_hash = sha256(honest_payload);
    let forged = entry_with(&sender, claimed_hash, b"attacker-substituted payload");

    let result = gossip
        .handle_message(&sender, GossipMessage::Response { entry: forged })
        .await;

    assert!(
        result.is_err(),
        "an entry whose hash does not match its content must be rejected at ingress"
    );
    assert!(
        gossip.get_entries(TOPIC).is_empty(),
        "a rejected entry must never be stored"
    );
    assert!(
        gossip.get_entry(TOPIC, &claimed_hash).is_none(),
        "a rejected entry must never occupy the dedup slot for the hash it claimed"
    );
    assert!(
        delivered.lock().unwrap().is_empty(),
        "a rejected entry must never reach a notification callback"
    );
}

/// RED-2: the same path must still accept a legitimate entry — the check rejects forgeries,
/// not reception.
#[tokio::test]
async fn response_entry_with_matching_hash_is_still_accepted() {
    let (mut gossip, delivered) = subscribed_actor().await;
    let sender = KeyPair::generate().unwrap().did().clone();

    let payload = b"a legitimately hashed payload";
    let hash = sha256(payload);
    let honest = entry_with(&sender, hash, payload);

    gossip
        .handle_message(&sender, GossipMessage::Response { entry: honest })
        .await
        .expect("a correctly hashed entry must still be accepted");

    assert_eq!(
        gossip.get_entries(TOPIC).len(),
        1,
        "a correctly hashed entry must still be stored"
    );
    assert_eq!(
        *delivered.lock().unwrap(),
        vec![hash],
        "a correctly hashed entry must still reach notification callbacks exactly once"
    );
}

/// RED-3: anti-entropy is a second insertion path into the same store. It must not be a way
/// around the check, and a forged entry must not take the dedup slot away from the real one.
#[tokio::test]
async fn pull_response_entry_with_mismatched_hash_cannot_bypass_the_check() {
    let (mut gossip, delivered) = subscribed_actor().await;
    let sender = KeyPair::generate().unwrap().did().clone();

    let honest_payload = b"the payload this digest actually commits to";
    let claimed_hash = sha256(honest_payload);
    let forged = entry_with(&sender, claimed_hash, b"attacker-substituted payload");

    // Echoes the nonce of the PullRequest this answers. Despite the field name it is a
    // request/response correlation id from `PeerSyncState::next_nonce`, not a cryptographic
    // nonce — nothing about the check under test depends on its value.
    let correlation_nonce = u64::from_le_bytes(claimed_hash[..8].try_into().unwrap());

    let result = gossip
        .handle_message(
            &sender,
            GossipMessage::PullResponse {
                topic: TOPIC.to_string(),
                entries: vec![forged],
                truncated: false,
                nonce: correlation_nonce,
                next_cursor: None,
            },
        )
        .await;

    assert!(
        result.is_err(),
        "anti-entropy must re-derive the hash exactly as the push path does"
    );
    assert!(
        gossip.get_entries(TOPIC).is_empty(),
        "a rejected anti-entropy entry must never be stored"
    );
    assert!(
        delivered.lock().unwrap().is_empty(),
        "a rejected anti-entropy entry must never reach a notification callback"
    );

    // The forged entry must not have poisoned the slot: the genuine entry that really does
    // hash to `claimed_hash` still has to be accepted afterwards.
    let honest = entry_with(&sender, claimed_hash, honest_payload);
    gossip
        .handle_message(&sender, GossipMessage::Response { entry: honest })
        .await
        .expect("the genuine entry for that hash must still be accepted");

    assert_eq!(
        gossip.get_entry(TOPIC, &claimed_hash).map(|e| e.data),
        Some(honest_payload.to_vec()),
        "the genuine payload must be what ends up stored under that hash"
    );
}

/// A compressed entry is the case a naive `sha256(entry.data)` check gets wrong: `publish`
/// hashes the payload *before* compressing, so the digest commits to the uncompressed bytes
/// while `data` carries the zstd frame. Reception of compressed entries must keep working.
#[tokio::test]
async fn compressed_entry_hashed_over_uncompressed_payload_is_accepted() {
    let (mut gossip, delivered) = subscribed_actor().await;
    let sender = KeyPair::generate().unwrap().did().clone();

    // Well over the 1 KiB compression threshold and highly compressible, so `compress()`
    // actually engages.
    let payload = vec![b'z'; 8192];
    let hash = sha256(&payload);
    let mut entry = entry_with(&sender, hash, &payload);
    entry.compress().unwrap();
    assert!(
        entry.compressed && entry.data != payload,
        "precondition: the entry must actually be compressed for this test to mean anything"
    );

    gossip
        .handle_message(&sender, GossipMessage::Response { entry })
        .await
        .expect("a compressed entry hashed over its uncompressed payload must be accepted");

    assert_eq!(
        gossip.get_entries(TOPIC).len(),
        1,
        "compressed entries must still be storable"
    );
    assert_eq!(
        *delivered.lock().unwrap(),
        vec![hash],
        "compressed entries must still reach notification callbacks"
    );
}

/// A compressed entry whose digest does not match its decompressed payload is still a
/// forgery, and compression must not be a way to smuggle one past the check.
#[tokio::test]
async fn compressed_entry_with_mismatched_hash_is_rejected() {
    let (mut gossip, _delivered) = subscribed_actor().await;
    let sender = KeyPair::generate().unwrap().did().clone();

    let payload = vec![b'z'; 8192];
    let mut entry = entry_with(&sender, sha256(b"something else entirely"), &payload);
    entry.compress().unwrap();
    assert!(entry.compressed, "precondition: entry must be compressed");

    let result = gossip
        .handle_message(&sender, GossipMessage::Response { entry })
        .await;

    assert!(
        result.is_err(),
        "compression must not let a mismatched hash through"
    );
    assert!(
        gossip.get_entries(TOPIC).is_empty(),
        "a rejected compressed entry must never be stored"
    );
}

/// Hostile input must fail closed, not panic: `compressed = true` over bytes that are not a
/// valid zstd frame cannot be re-derived, so it must be refused.
#[tokio::test]
async fn entry_claiming_compression_over_undecodable_bytes_is_rejected() {
    let (mut gossip, _delivered) = subscribed_actor().await;
    let sender = KeyPair::generate().unwrap().did().clone();

    let mut entry = entry_with(&sender, sha256(b"anything"), b"not a zstd frame at all");
    entry.compressed = true;

    let result = gossip
        .handle_message(&sender, GossipMessage::Response { entry })
        .await;

    assert!(
        result.is_err(),
        "an entry whose payload cannot be decoded must be refused, not accepted or panicked on"
    );
    assert!(
        gossip.get_entries(TOPIC).is_empty(),
        "an undecodable entry must never be stored"
    );
}
