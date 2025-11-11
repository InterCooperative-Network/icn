//! Gossip actor for managing distributed synchronization

use crate::bloom::BloomFilter;
use crate::types::{AccessControl, ContentHash, GossipEntry, GossipMessage, Subscription, Topic};
use crate::vector_clock::VectorClock;
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use icn_trust::TrustClass;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Gossip actor manages topics and entry synchronization
pub struct GossipActor {
    /// This node's DID
    own_did: Did,

    /// Vector clock for this node
    clock: VectorClock,

    /// Topics (topic name -> Topic)
    topics: HashMap<String, Topic>,

    /// Entries (topic -> hash -> entry)
    entries: HashMap<String, HashMap<ContentHash, GossipEntry>>,

    /// Bloom filters (topic -> filter)
    bloom_filters: HashMap<String, BloomFilter>,

    /// Subscriptions (topic -> subscribers)
    subscriptions: HashMap<String, Vec<Did>>,

    /// Trust lookup function
    trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,
}

impl GossipActor {
    /// Create a new gossip actor
    pub fn new(
        own_did: Did,
        trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,
    ) -> Self {
        let mut gossip = GossipActor {
            own_did: own_did.clone(),
            clock: VectorClock::new(),
            topics: HashMap::new(),
            entries: HashMap::new(),
            bloom_filters: HashMap::new(),
            subscriptions: HashMap::new(),
            trust_lookup,
        };

        // Create default topics
        gossip.create_topic(Topic::new(
            "global:identity".to_string(),
            AccessControl::Public,
        ));
        gossip.create_topic(Topic::new(
            "global:rendezvous".to_string(),
            AccessControl::Public,
        ));

        gossip
    }

    /// Create a new topic
    pub fn create_topic(&mut self, topic: Topic) {
        info!("Creating topic: {}", topic.name);
        self.bloom_filters.insert(
            topic.name.clone(),
            BloomFilter::new(topic.max_entries, 0.01),
        );
        self.entries.insert(topic.name.clone(), HashMap::new());
        self.subscriptions.insert(topic.name.clone(), Vec::new());
        self.topics.insert(topic.name.clone(), topic);
    }

    /// Publish an entry to a topic
    pub fn publish(&mut self, topic: &str, data: Vec<u8>) -> Result<ContentHash> {
        // Auto-create topic if it doesn't exist (as public topic)
        if !self.topics.contains_key(topic) {
            debug!("Auto-creating public topic: {}", topic);
            self.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
        }

        let topic_obj = self
            .topics
            .get(topic)
            .context("Topic not found")?;

        // Check ACL
        let trust_class = (self.trust_lookup)(&self.own_did);
        if !topic_obj.can_publish(&self.own_did, trust_class) {
            bail!("Not authorized to publish to topic: {}", topic);
        }

        // Increment vector clock
        self.clock.increment(&self.own_did);

        // Create entry
        let hash = Self::hash_data(&data);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;

        let entry = GossipEntry {
            hash,
            author: self.own_did.clone(),
            clock: self.clock.clone(),
            topic: topic.to_string(),
            data,
            timestamp,
        };

        // Store entry
        self.store_entry(entry)?;

        debug!("Published entry {} to topic {}", hex::encode(hash), topic);

        Ok(hash)
    }

    /// Store an entry (from publish or receive)
    fn store_entry(&mut self, entry: GossipEntry) -> Result<()> {
        let topic = &entry.topic;
        let hash = entry.hash;

        // Get or create topic entries
        let topic_entries = self.entries.entry(topic.clone()).or_insert_with(HashMap::new);

        // Check if already have this entry
        if topic_entries.contains_key(&hash) {
            return Ok(()); // Already have it
        }

        // Add to bloom filter
        if let Some(bloom) = self.bloom_filters.get_mut(topic) {
            bloom.insert(&hash);
        }

        // Store entry
        topic_entries.insert(hash, entry.clone());

        // Merge vector clock
        self.clock.merge(&entry.clock);

        // Apply retention policy (remove old entries if over limit)
        if let Some(topic_obj) = self.topics.get(topic) {
            if topic_entries.len() > topic_obj.max_entries {
                // Remove oldest entries
                let mut entries_vec: Vec<_> = topic_entries.values().collect();
                entries_vec.sort_by_key(|e| e.timestamp);

                let to_remove = topic_entries.len() - topic_obj.max_entries;
                let hashes_to_remove: Vec<ContentHash> = entries_vec
                    .iter()
                    .take(to_remove)
                    .map(|e| e.hash)
                    .collect();

                for hash in hashes_to_remove {
                    topic_entries.remove(&hash);
                }
            }
        }

        Ok(())
    }

    /// Get entries for a topic
    pub fn get_entries(&self, topic: &str) -> Vec<GossipEntry> {
        self.entries
            .get(topic)
            .map(|entries| entries.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get a specific entry by hash
    pub fn get_entry(&self, topic: &str, hash: &ContentHash) -> Option<GossipEntry> {
        self.entries
            .get(topic)
            .and_then(|entries| entries.get(hash).cloned())
    }

    /// Subscribe to a topic
    pub fn subscribe(&mut self, topic: &str, subscriber: Did) -> Result<Subscription> {
        let topic_obj = self.topics.get(topic).context("Topic not found")?;

        // Check ACL
        let trust_class = (self.trust_lookup)(&subscriber);
        if !topic_obj.can_subscribe(&subscriber, trust_class) {
            bail!("Not authorized to subscribe to topic: {}", topic);
        }

        // Add subscriber
        let subscribers = self
            .subscriptions
            .entry(topic.to_string())
            .or_insert_with(Vec::new);

        if !subscribers.contains(&subscriber) {
            subscribers.push(subscriber.clone());
        }

        Ok(Subscription {
            topic: topic.to_string(),
            subscriber,
        })
    }

    /// Get bloom filter for a topic
    pub fn get_bloom_filter(&self, topic: &str) -> Option<BloomFilter> {
        self.bloom_filters.get(topic).cloned()
    }

    /// Find missing entries based on remote bloom filter
    pub fn find_missing(&self, topic: &str, remote_filter: &BloomFilter) -> Vec<ContentHash> {
        let mut missing = Vec::new();

        if let Some(entries) = self.entries.get(topic) {
            for hash in entries.keys() {
                if !remote_filter.contains(hash) {
                    missing.push(*hash);
                }
            }
        }

        missing
    }

    /// Handle incoming gossip message from network
    pub fn handle_message(&mut self, message: GossipMessage) -> Result<()> {
        match message {
            GossipMessage::Announce { hash, author, clock, topic } => {
                debug!("Received Announce: topic={}, hash={:?}, author={}", topic, hash, author);

                // Check if we already have this entry
                if let Some(entries) = self.entries.get(&topic) {
                    if entries.contains_key(&hash) {
                        debug!("Already have entry {}", hex::encode(hash));
                        return Ok(());
                    }
                }

                // TODO: Request full entry if we don't have it
                // This would require access to NetworkHandle to send Request message
                debug!("Would request entry {} from {}", hex::encode(hash), author);
                Ok(())
            }

            GossipMessage::Request { hash } => {
                debug!("Received Request for hash: {:?}", hash);

                // Find entry across all topics
                for (topic_name, entries) in &self.entries {
                    if let Some(entry) = entries.get(&hash) {
                        debug!("Found entry in topic: {}", topic_name);
                        // TODO: Send Response with entry
                        // This would require access to NetworkHandle
                        return Ok(());
                    }
                }

                debug!("Entry not found for hash: {:?}", hash);
                Ok(())
            }

            GossipMessage::Response { entry } => {
                debug!("Received Response: topic={}, hash={:?}", entry.topic, entry.hash);

                // Store the entry
                self.entries
                    .entry(entry.topic.clone())
                    .or_insert_with(HashMap::new)
                    .insert(entry.hash, entry.clone());

                // Update bloom filter
                if let Some(filter) = self.bloom_filters.get_mut(&entry.topic) {
                    filter.insert(&entry.hash);
                }

                debug!("Stored entry in topic: {}", entry.topic);
                Ok(())
            }

            GossipMessage::RequestBloomFilter { topic } => {
                debug!("Received RequestBloomFilter for topic: {}", topic);
                // TODO: Send bloom filter
                Ok(())
            }

            GossipMessage::SendBloomFilter { topic, filter: _ } => {
                debug!("Received SendBloomFilter for topic: {}", topic);
                // TODO: Compare with local entries and request missing
                Ok(())
            }

            GossipMessage::RequestMissing { hashes } => {
                debug!("Received RequestMissing: {} hashes", hashes.len());
                // TODO: Send Response messages for each requested hash
                Ok(())
            }
        }
    }

    /// Get all topic names
    pub fn get_topics(&self) -> Vec<String> {
        self.topics.keys().cloned().collect()
    }

    /// Perform anti-entropy for a specific topic
    ///
    /// Returns the bloom filter for the topic and a list of missing hashes
    /// to request from the remote peer.
    pub fn anti_entropy_check(
        &self,
        topic: &str,
        remote_filter_data: &crate::types::BloomFilterData,
    ) -> Result<(crate::types::BloomFilterData, Vec<ContentHash>)> {
        // Get our bloom filter
        let local_filter = self
            .get_bloom_filter(topic)
            .context("Topic not found")?;

        // Reconstruct remote bloom filter
        let remote_filter = BloomFilter::from_data(remote_filter_data);

        // Find entries we have that remote doesn't
        let missing_on_remote = self.find_missing(topic, &remote_filter);

        // Serialize our bloom filter for sending
        let local_filter_data = local_filter.to_data();

        Ok((local_filter_data, missing_on_remote))
    }

    /// Hash data to create content hash
    fn hash_data(data: &[u8]) -> ContentHash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

/// Shared gossip actor handle
pub type GossipHandle = Arc<RwLock<GossipActor>>;

impl GossipActor {
    /// Spawn a gossip actor and return a handle
    pub fn spawn(
        own_did: Did,
        trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,
    ) -> GossipHandle {
        let actor = GossipActor::new(own_did, trust_lookup);
        Arc::new(RwLock::new(actor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn mock_trust_lookup(_did: &Did) -> Option<TrustClass> {
        Some(TrustClass::Partner)
    }

    #[test]
    fn test_create_and_publish() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Publish to default topic
        let data = b"Hello, world!".to_vec();
        let hash = gossip.publish("global:identity", data.clone()).unwrap();

        // Retrieve entry
        let entry = gossip.get_entry("global:identity", &hash).unwrap();
        assert_eq!(entry.data, data);
        assert_eq!(entry.author, did);
    }

    #[test]
    fn test_subscribe() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        let subscription = gossip.subscribe("global:identity", did.clone()).unwrap();
        assert_eq!(subscription.topic, "global:identity");
    }

    #[test]
    fn test_bloom_filter_sync() {
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let keypair2 = KeyPair::generate().unwrap();
        let did2 = keypair2.did().clone();

        let mut gossip1 = GossipActor::new(did1.clone(), Arc::new(mock_trust_lookup));
        let mut gossip2 = GossipActor::new(did2.clone(), Arc::new(mock_trust_lookup));

        // Node 1 publishes entries
        gossip1
            .publish("global:identity", b"Entry 1".to_vec())
            .unwrap();
        gossip1
            .publish("global:identity", b"Entry 2".to_vec())
            .unwrap();

        // Node 2 publishes different entry
        gossip2
            .publish("global:identity", b"Entry 3".to_vec())
            .unwrap();

        // Get bloom filter from node 2
        let bloom2 = gossip2.get_bloom_filter("global:identity").unwrap();

        // Find what node 1 has that node 2 doesn't
        let missing = gossip1.find_missing("global:identity", &bloom2);

        // Should have 2 missing entries (Entry 1 and Entry 2)
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn test_vector_clock_merge() {
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let mut gossip = GossipActor::new(did1.clone(), Arc::new(mock_trust_lookup));

        // Initial clock
        let initial_count = gossip.clock.get(&did1);

        // Publish entry (increments clock)
        gossip
            .publish("global:identity", b"Test".to_vec())
            .unwrap();

        // Clock should have incremented
        assert_eq!(gossip.clock.get(&did1), initial_count + 1);
    }
}
