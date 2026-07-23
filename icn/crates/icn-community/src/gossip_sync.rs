//! Gossip synchronization for community state (last-write-wins merge).
//!
//! This logic previously lived in `icn-core`'s notification dispatcher
//! (`init_notifications.rs::handle_community_update`, wired via a per-domain
//! field on the shared `NotificationDeps` struct). Moved here as part of the
//! kernel/domain boundary inversion (B0): construction, configuration, and
//! merge semantics for the community domain now live entirely with the
//! community crate + the daemon composition root, never in `icn-core`.
//! `icn-core` only transports an opaque handle and never imports this crate.

use crate::store::CommunityStore;
use crate::types::Community;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Apply a gossip-received community entry to the local store using a
/// last-write-wins merge on `updated_at`.
///
/// Behavior moved verbatim from the former `icn-core` dispatcher arm: decode
/// failures, missing-local-record inserts, and merge decisions all log
/// identically to preserve observable behavior across the move.
pub async fn apply_gossip_update(entry_data: Vec<u8>, community_store: Arc<CommunityStore>) {
    match icn_encoding::decode::<Community>(&entry_data) {
        Ok(remote_community) => {
            // Check for existing community and use last-write-wins merge
            match community_store.get(&remote_community.id) {
                Ok(Some(local_community)) => {
                    // Only update if remote is newer
                    if remote_community.updated_at > local_community.updated_at {
                        if let Err(e) = community_store.put(&remote_community) {
                            warn!(
                                community_id = %remote_community.id,
                                error = %e,
                                "Failed to merge community from gossip"
                            );
                        } else {
                            debug!(
                                community_id = %remote_community.id,
                                community_name = %remote_community.name,
                                "Merged newer community state from gossip"
                            );
                        }
                    } else {
                        debug!(
                            community_id = %remote_community.id,
                            "Ignoring older community state from gossip"
                        );
                    }
                }
                Ok(None) => {
                    // New community, save it
                    if let Err(e) = community_store.put(&remote_community) {
                        warn!(
                            community_id = %remote_community.id,
                            error = %e,
                            "Failed to save new community from gossip"
                        );
                    } else {
                        info!(
                            community_id = %remote_community.id,
                            community_name = %remote_community.name,
                            "Synced new community from gossip"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        community_id = %remote_community.id,
                        error = %e,
                        "Failed to check existing community for merge"
                    );
                }
            }
        }
        Err(e) => {
            warn!(
                error = %e,
                "Failed to deserialize community from gossip"
            );
        }
    }
}

/// Build an `EntryNotificationCallback` that filters for `COMMUNITY_TOPIC` and
/// applies gossip updates to the given store.
///
/// Registered via `GossipActor::add_notification_callback` — the same
/// multi-subscriber fan-out seam governance (`apps/governance/src/actor.rs`)
/// and steward (`icn-core/src/supervisor/init_steward.rs`) already use
/// independently. Non-matching topics are ignored; each matching entry is
/// decoded and merged on a spawned task (parity with the former dispatcher's
/// `tokio::spawn` per entry).
pub fn gossip_update_callback(store: Arc<CommunityStore>) -> icn_gossip::EntryNotificationCallback {
    Arc::new(move |topic, entry, _subscriber_did| {
        if topic != crate::actor::COMMUNITY_TOPIC {
            return;
        }
        let store = store.clone();
        let data = match entry.get_data() {
            Ok(data) => data,
            Err(e) => {
                warn!(topic = %topic, error = %e, "Failed to get entry data for community topic");
                return;
            }
        };
        tokio::spawn(async move {
            apply_gossip_update(data, store).await;
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CommunityType;
    use std::path::Path;

    fn open_store(dir: &Path) -> Arc<CommunityStore> {
        let sled_store: Arc<dyn icn_store::Store> =
            Arc::new(icn_store::SledStore::open(dir).expect("open test sled store"));
        Arc::new(CommunityStore::new(sled_store))
    }

    fn make_community(id: &str, name: &str) -> Community {
        Community::new(
            id.to_string(),
            name.to_string(),
            CommunityType::Geographic,
            "test-domain".to_string(),
        )
    }

    #[tokio::test]
    async fn apply_gossip_update_inserts_new_community() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let store = open_store(tempdir.path());
        let community = make_community("c-1", "First");
        let encoded = icn_encoding::encode(&community).expect("encode");

        apply_gossip_update(encoded, store.clone()).await;

        let fetched = store
            .get(&"c-1".to_string())
            .expect("get")
            .expect("present");
        assert_eq!(fetched.name, "First");
    }

    #[tokio::test]
    async fn apply_gossip_update_newer_wins() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let store = open_store(tempdir.path());

        let older = make_community("c-2", "Original");
        store.put(&older).expect("seed");

        let mut newer = older.clone();
        newer.name = "Renamed".to_string();
        newer.updated_at = older.updated_at + chrono::Duration::seconds(60);
        let encoded = icn_encoding::encode(&newer).expect("encode");

        apply_gossip_update(encoded, store.clone()).await;

        let fetched = store
            .get(&"c-2".to_string())
            .expect("get")
            .expect("present");
        assert_eq!(fetched.name, "Renamed", "newer remote update must win");
    }

    #[tokio::test]
    async fn apply_gossip_update_older_is_ignored() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let store = open_store(tempdir.path());

        let current = make_community("c-3", "Current");
        store.put(&current).expect("seed");

        let mut stale = current.clone();
        stale.name = "Stale".to_string();
        stale.updated_at = current.updated_at - chrono::Duration::seconds(60);
        let encoded = icn_encoding::encode(&stale).expect("encode");

        apply_gossip_update(encoded, store.clone()).await;

        let fetched = store
            .get(&"c-3".to_string())
            .expect("get")
            .expect("present");
        assert_eq!(
            fetched.name, "Current",
            "older remote update must be ignored"
        );
    }

    #[tokio::test]
    async fn apply_gossip_update_decode_failure_does_not_panic() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let store = open_store(tempdir.path());

        // Garbage bytes: decode fails, function must warn and return, not panic.
        apply_gossip_update(vec![0xFF, 0x00, 0xDE, 0xAD], store).await;
    }

    fn make_entry(topic: &str, data: Vec<u8>) -> icn_gossip::GossipEntry {
        icn_gossip::GossipEntry {
            hash: [0u8; 32],
            author: icn_identity::KeyPair::generate()
                .expect("keypair")
                .did()
                .clone(),
            clock: icn_gossip::VectorClock::new(),
            topic: topic.to_string(),
            data,
            compressed: false,
            timestamp: 0,
            replica_offered: None,
        }
    }

    #[tokio::test]
    async fn callback_ignores_foreign_topics() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let store = open_store(tempdir.path());
        let callback = gossip_update_callback(store.clone());

        let subscriber = icn_identity::KeyPair::generate()
            .expect("keypair")
            .did()
            .clone();
        let entry = make_entry("some:other:topic", b"irrelevant".to_vec());
        callback("some:other:topic".to_string(), entry, subscriber);

        // No spawned task should have written anything for this ID; since the
        // callback returns before spawning on a topic mismatch, there is
        // nothing to await — a short yield is enough to prove no phantom
        // write races in under it.
        tokio::task::yield_now().await;
        assert!(store.get(&"c-1".to_string()).expect("get").is_none());
    }

    #[tokio::test]
    async fn callback_processes_matching_topic() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let store = open_store(tempdir.path());
        let callback = gossip_update_callback(store.clone());

        let community = make_community("c-4", "Via Callback");
        let encoded = icn_encoding::encode(&community).expect("encode");
        let subscriber = icn_identity::KeyPair::generate()
            .expect("keypair")
            .did()
            .clone();
        let entry = make_entry(crate::actor::COMMUNITY_TOPIC, encoded);
        callback(crate::actor::COMMUNITY_TOPIC.to_string(), entry, subscriber);

        // The callback spawns the merge asynchronously; poll with a bounded
        // deadline instead of a fixed sleep so a loaded CI host can't flake this.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let fetched = loop {
            if let Some(community) = store.get(&"c-4".to_string()).expect("get") {
                break community;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "spawned merge did not complete within the 5s deadline"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        assert_eq!(fetched.name, "Via Callback");
    }
}
