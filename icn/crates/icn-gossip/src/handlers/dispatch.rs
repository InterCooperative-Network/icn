//! Message dispatch logic
//!
//! This module contains the shared message dispatch logic used by both
//! the main protocol handler and batch handlers. It maps GossipMessage
//! variants to their appropriate handler methods while avoiding async
//! recursion issues.
//!
//! # Design
//!
//! The dispatch method is non-async and returns an enum indicating whether
//! the handler is synchronous (already executed) or asynchronous (returns
//! a future to be awaited by the caller). This design allows:
//!
//! - Single source of truth for message routing
//! - Use in both single-message and batch contexts
//! - Avoidance of async recursion in batch handlers
//! - Clear separation of dispatch logic from handling logic

use crate::gossip::GossipActor;
use crate::types::{GossipEntry, GossipMessage};
use anyhow::Result;
use icn_identity::Did;

/// Result of dispatching a message - either sync (already handled) or async (needs awaiting)
pub enum DispatchResult {
    /// Handler executed synchronously, no further action needed
    Sync(Result<()>),
    /// Handler needs async execution - contains the action type
    AsyncResponse(Did, GossipEntry),
    /// Handler needs async execution - contains the pull response data
    AsyncPullResponse {
        sender: Did,
        topic: String,
        entries: Vec<GossipEntry>,
        truncated: bool,
        nonce: u64,
        next_cursor: Option<crate::types::SyncCursor>,
    },
}

impl GossipActor {
    /// Dispatch a gossip message to its handler
    ///
    /// This is the single source of truth for message routing. It maps
    /// GossipMessage variants to their handler methods without introducing
    /// async recursion.
    ///
    /// # Returns
    ///
    /// - `DispatchResult::Sync(result)` - Synchronous handler executed, result is final
    /// - `DispatchResult::AsyncResponse(...)` - Needs `handle_response()` to be called
    /// - `DispatchResult::AsyncPullResponse{...}` - Needs `handle_pull_response()` to be called
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// match self.dispatch_message(sender, message) {
    ///     DispatchResult::Sync(result) => result?,
    ///     DispatchResult::AsyncResponse(sender, entry) => {
    ///         self.handle_response(&sender, entry).await?
    ///     }
    ///     DispatchResult::AsyncPullResponse { sender, topic, entries, truncated, nonce, next_cursor } => {
    ///         self.handle_pull_response(&sender, topic, entries, truncated, nonce, next_cursor).await?
    ///     }
    /// }
    /// ```
    pub fn dispatch_message(&mut self, sender: &Did, message: GossipMessage) -> DispatchResult {
        match message {
            GossipMessage::Announce {
                hash,
                author,
                clock: _,
                topic,
            } => DispatchResult::Sync(self.handle_announce(sender, hash, author, topic)),

            GossipMessage::Request { hash } => {
                DispatchResult::Sync(self.handle_request(sender, hash))
            }

            GossipMessage::Response { entry } => {
                // Async handler - return data for caller to await
                DispatchResult::AsyncResponse(sender.clone(), entry)
            }

            GossipMessage::RequestBloomFilter { topic } => {
                DispatchResult::Sync(self.handle_request_bloom_filter(sender, topic))
            }

            GossipMessage::SendBloomFilter { topic, filter } => {
                DispatchResult::Sync(self.handle_send_bloom_filter(sender, topic, filter))
            }

            GossipMessage::RequestMissing { hashes } => {
                DispatchResult::Sync(self.handle_request_missing(sender, hashes))
            }

            GossipMessage::Digest {
                topic,
                vector,
                bloom,
                hint_count,
                nonce,
            } => DispatchResult::Sync(self.handle_digest(
                sender, topic, vector, bloom, hint_count, nonce,
            )),

            GossipMessage::PullRequest {
                topic,
                want_ids,
                max_bytes,
                nonce,
                cursor,
            } => DispatchResult::Sync(self.handle_pull_request(
                sender, topic, want_ids, max_bytes, nonce, cursor,
            )),

            GossipMessage::PullResponse {
                topic,
                entries,
                truncated,
                nonce,
                next_cursor,
            } => {
                // Async handler - return data for caller to await
                DispatchResult::AsyncPullResponse {
                    sender: sender.clone(),
                    topic,
                    entries,
                    truncated,
                    nonce,
                    next_cursor,
                }
            }

            GossipMessage::BlobAnnounce {
                blob_hash,
                peer_did,
                size_bytes,
            } => DispatchResult::Sync(self.handle_blob_announce(
                sender, blob_hash, peer_did, size_bytes,
            )),

            GossipMessage::ReplicaRequest {
                content_hash,
                requesting_peer,
            } => DispatchResult::Sync(self.handle_replica_request(
                sender,
                content_hash,
                requesting_peer,
            )),

            GossipMessage::ReplicaOffer {
                content_hash,
                offering_peer,
                health,
            } => DispatchResult::Sync(self.handle_replica_offer(
                sender,
                content_hash,
                offering_peer,
                health,
            )),

            GossipMessage::ReplicaStatus {
                content_hash,
                replicas,
            } => DispatchResult::Sync(self.handle_replica_status(sender, content_hash, replicas)),

            GossipMessage::PartitionHealRequest {
                requesting_peer,
                vector_clock,
                last_contact_ms,
            } => DispatchResult::Sync(self.handle_partition_heal_request(
                sender,
                requesting_peer,
                vector_clock,
                last_contact_ms,
            )),

            GossipMessage::PartitionHealResponse {
                responding_peer,
                vector_clock,
                diverged_topics,
                entries_behind,
            } => DispatchResult::Sync(self.handle_partition_heal_response(
                sender,
                responding_peer,
                vector_clock,
                diverged_topics,
                entries_behind,
            )),

            GossipMessage::StorageChallengeMsg { challenge } => {
                DispatchResult::Sync(self.handle_storage_challenge(sender, challenge))
            }

            GossipMessage::StorageProofMsg { proof } => {
                DispatchResult::Sync(self.handle_storage_proof(sender, proof))
            }

            GossipMessage::StorageContentNotFoundMsg { response } => {
                DispatchResult::Sync(self.handle_storage_content_not_found(sender, response))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GossipEntry;
    use crate::GossipActor;
    use icn_identity::KeyPair;
    use icn_trust::TrustClass;
    use std::sync::Arc;

    fn mock_trust_lookup(_did: &Did) -> Option<TrustClass> {
        Some(TrustClass::Partner)
    }

    fn create_test_actor() -> GossipActor {
        let keypair = KeyPair::generate().unwrap();
        GossipActor::new(keypair.did().clone(), Arc::new(mock_trust_lookup))
    }

    fn create_test_entry(topic: &str) -> GossipEntry {
        let keypair = KeyPair::generate().unwrap();
        let data = vec![1, 2, 3];
        let hash = blake3::hash(&data).into();
        
        GossipEntry {
            hash,
            author: keypair.did().clone(),
            clock: Default::default(),
            topic: topic.to_string(),
            data,
            compressed: false,
            timestamp: 0,
            replica_offered: None,
        }
    }

    #[test]
    fn test_dispatch_announce_is_sync() {
        let mut actor = create_test_actor();
        let keypair = KeyPair::generate().unwrap();
        let sender = keypair.did().clone();
        let hash = [0u8; 32];

        let message = GossipMessage::Announce {
            hash,
            author: sender.clone(),
            clock: Default::default(),
            topic: "test".to_string(),
        };

        match actor.dispatch_message(&sender, message) {
            DispatchResult::Sync(_) => {
                // Expected - Announce is synchronous
            }
            _ => panic!("Announce should be handled synchronously"),
        }
    }

    #[test]
    fn test_dispatch_response_is_async() {
        let mut actor = create_test_actor();
        let keypair = KeyPair::generate().unwrap();
        let sender = keypair.did().clone();
        let entry = create_test_entry("test");

        let message = GossipMessage::Response {
            entry: entry.clone(),
        };

        match actor.dispatch_message(&sender, message) {
            DispatchResult::AsyncResponse(returned_sender, returned_entry) => {
                assert_eq!(returned_sender, sender);
                assert_eq!(returned_entry.topic, entry.topic);
            }
            _ => panic!("Response should be handled asynchronously"),
        }
    }

    #[test]
    fn test_dispatch_pull_response_is_async() {
        let mut actor = create_test_actor();
        let keypair = KeyPair::generate().unwrap();
        let sender = keypair.did().clone();
        let entries = vec![create_test_entry("test")];

        let message = GossipMessage::PullResponse {
            topic: "test".to_string(),
            entries: entries.clone(),
            truncated: false,
            nonce: 12345,
            next_cursor: None,
        };

        match actor.dispatch_message(&sender, message) {
            DispatchResult::AsyncPullResponse {
                sender: returned_sender,
                topic,
                entries: returned_entries,
                truncated,
                nonce,
                next_cursor,
            } => {
                assert_eq!(returned_sender, sender);
                assert_eq!(topic, "test");
                assert_eq!(returned_entries.len(), entries.len());
                assert!(!truncated);
                assert_eq!(nonce, 12345);
                assert!(next_cursor.is_none());
            }
            _ => panic!("PullResponse should be handled asynchronously"),
        }
    }

    #[test]
    fn test_dispatch_all_sync_handlers() {
        let mut actor = create_test_actor();
        let keypair = KeyPair::generate().unwrap();
        let sender = keypair.did().clone();

        // Create a mock BloomFilterData
        let bloom = crate::types::BloomFilterData {
            bits: vec![],
            num_hashes: 3,
            size: 1024,
        };

        // Test a few sync message types
        let sync_messages = vec![
            GossipMessage::Request { hash: [0u8; 32] },
            GossipMessage::RequestBloomFilter {
                topic: "test".to_string(),
            },
            GossipMessage::RequestMissing {
                hashes: vec![[0u8; 32]],
            },
            GossipMessage::Digest {
                topic: "test".to_string(),
                vector: Default::default(),
                bloom,
                hint_count: 0,
                nonce: 0,
            },
            GossipMessage::BlobAnnounce {
                blob_hash: [0u8; 32],
                peer_did: sender.clone(),
                size_bytes: 1024,
            },
        ];

        for message in sync_messages {
            match actor.dispatch_message(&sender, message) {
                DispatchResult::Sync(_) => {
                    // Expected - these should all be sync
                }
                _ => panic!("Message should be handled synchronously"),
            }
        }
    }
}
