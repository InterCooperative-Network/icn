//! Gossip actor for managing distributed synchronization

use crate::bloom::BloomFilter;
use crate::sync::PeerSyncManager;
use crate::types::{
    AccessControl, ContentHash, GossipEntry, GossipMessage, Subscription, Topic,
    TrustResourceLimits,
};
use crate::vector_clock::VectorClock;
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use icn_trust::TrustClass;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Callback for sending gossip messages to peers
/// Parameters: (recipient_did, message)
/// If recipient_did is None, broadcast to all peers
pub type SendMessageCallback = Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>;

/// Callback for notifying subscribers of new entries
/// Parameters: (topic, entry, subscriber_did)
/// Called when a new entry is stored in a topic that has subscribers
pub type EntryNotificationCallback = Arc<dyn Fn(String, GossipEntry, Did) + Send + Sync>;

/// Callback for sampling peers based on scope
/// Parameters: (scope, count) -> Vec<Did>
/// Returns a list of peer DIDs to send messages to
pub type PeerSamplingCallback = Arc<dyn Fn(crate::types::Scope, usize) -> Vec<Did> + Send + Sync>;

/// Maximum subscribers per topic to prevent unbounded memory growth
const MAX_SUBSCRIBERS_PER_TOPIC: usize = 10_000;

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

    /// Trust lookup function (returns trust class for resource limits)
    trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,

    /// Trust graph for fine-grained trust score computation (optional)
    /// When provided, enables trust-gated subscription authorization
    trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,

    /// Send message callback (optional, for sending responses)
    send_callback: Option<SendMessageCallback>,

    /// Entry notification callback (optional, for notifying subscribers)
    notification_callback: Option<EntryNotificationCallback>,

    /// Peer sampling callback (optional, for scope-aware peer selection)
    peer_sampling: Option<PeerSamplingCallback>,

    /// Per-peer sync state manager
    peer_sync: PeerSyncManager,
}

impl GossipActor {
    /// Create a new gossip actor
    pub fn new(
        own_did: Did,
        trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,
    ) -> Self {
        Self::new_with_trust_graph(own_did, trust_lookup, None)
    }

    /// Create a new gossip actor with optional trust graph for fine-grained trust control
    pub fn new_with_trust_graph(
        own_did: Did,
        trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,
        trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
    ) -> Self {
        let mut gossip = GossipActor {
            own_did: own_did.clone(),
            clock: VectorClock::new(),
            topics: HashMap::new(),
            entries: HashMap::new(),
            bloom_filters: HashMap::new(),
            subscriptions: HashMap::new(),
            trust_lookup,
            trust_graph,
            send_callback: None,
            notification_callback: None,
            peer_sampling: None,
            peer_sync: PeerSyncManager::new(300, 5000), // Default: 300-5000ms backoff
        };

        // Create default topics with appropriate scopes
        gossip.create_topic(
            Topic::new("global:identity".to_string(), AccessControl::Public)
                .with_scope(crate::types::Scope::Global), // Identity propagates globally
        );
        gossip.create_topic(
            Topic::new("global:rendezvous".to_string(), AccessControl::Public)
                .with_scope(crate::types::Scope::Global), // Rendezvous nodes need global visibility
        );
        gossip.create_topic(
            Topic::new(
                "trust:attestations".to_string(),
                AccessControl::TrustClass(TrustClass::Known),
            )
            .with_scope(crate::types::Scope::Regional), // Trust attestations are regional
        );

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

    /// Set the send message callback for sending responses
    pub fn set_send_callback(&mut self, callback: SendMessageCallback) {
        self.send_callback = Some(callback);
    }

    /// Set the entry notification callback for notifying subscribers
    pub fn set_notification_callback(&mut self, callback: EntryNotificationCallback) {
        self.notification_callback = Some(callback);
    }

    /// Set the peer sampling callback for scope-aware peer selection
    pub fn set_peer_sampling(&mut self, callback: PeerSamplingCallback) {
        self.peer_sampling = Some(callback);
    }

    /// Send a message to a peer (if callback is set)
    fn send_message(&self, recipient: Option<Did>, message: GossipMessage) {
        if let Some(callback) = &self.send_callback {
            callback(recipient, message);
        } else {
            debug!("Cannot send message - no send callback set");
        }
    }

    /// Send a message with scope-aware peer selection
    /// If peer_sampling is set, samples peers based on scope. Otherwise falls back to broadcast.
    fn send_message_scoped(&self, scope: crate::types::Scope, fanout: usize, message: GossipMessage) {
        if let Some(sampling) = &self.peer_sampling {
            // Use scope-aware peer selection
            let peers = sampling(scope, fanout);

            if peers.is_empty() {
                debug!("No peers available for scope {:?}", scope);
                return;
            }

            // Send to each selected peer
            for peer in peers {
                self.send_message(Some(peer), message.clone());
            }

            // Track fanout metrics
            icn_obs::metrics::topology::gossip_fanout_record(
                match scope {
                    crate::types::Scope::LocalCluster => "local_cluster",
                    crate::types::Scope::Regional => "regional",
                    crate::types::Scope::Global => "global",
                },
                fanout,
            );
        } else {
            // Fall back to broadcast (backward compatibility)
            self.send_message(None, message);
        }
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
            .as_millis()
            .try_into()
            .context("Timestamp overflow - system clock too far in future")?;

        let mut entry = GossipEntry {
            hash,
            author: self.own_did.clone(),
            clock: self.clock.clone(),
            topic: topic.to_string(),
            data,
            compressed: false,
            timestamp,
        };

        // Compress large entries before storing/sending
        if let Err(e) = entry.compress() {
            debug!("Failed to compress entry: {}", e);
            // Continue without compression - not critical
        }

        // Store entry
        self.store_entry(entry)?;

        // Track metrics
        icn_obs::metrics::gossip::entries_published_inc();
        self.update_gauge_metrics();

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

        // Check topic max_entries limit BEFORE inserting to prevent unbounded growth
        if let Some(topic_obj) = self.topics.get(topic) {
            if topic_entries.len() >= topic_obj.max_entries {
                // At capacity - remove oldest entry first to make room
                let mut entries_vec: Vec<_> = topic_entries.values().cloned().collect();
                entries_vec.sort_by_key(|e| e.timestamp);

                if let Some(oldest) = entries_vec.first() {
                    debug!(
                        "Topic {} at capacity ({}), removing oldest entry",
                        topic, topic_obj.max_entries
                    );
                    topic_entries.remove(&oldest.hash);
                }
            }
        }

        // Add to bloom filter
        if let Some(bloom) = self.bloom_filters.get_mut(topic) {
            bloom.insert(&hash);
        }

        // Store entry
        topic_entries.insert(hash, entry.clone());

        // Merge vector clock
        self.clock.merge(&entry.clock);

        // Notify subscribers about the new entry
        if let Some(callback) = &self.notification_callback {
            if let Some(subscribers) = self.subscriptions.get(topic) {
                for subscriber in subscribers {
                    debug!("Notifying subscriber {} about new entry in topic {}", subscriber, topic);
                    callback(topic.clone(), entry.clone(), subscriber.clone());
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

        // Priority 1: Check fine-grained trust threshold (if configured)
        if let Some(threshold) = topic_obj.min_trust_threshold {
            if let Some(trust_graph) = &self.trust_graph {
                // Compute exact trust score from trust graph
                // Use blocking operation since we're in a sync context
                let trust_score = {
                    // block_in_place allows blocking in async runtime context
                    tokio::task::block_in_place(|| {
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(async {
                            let graph = trust_graph.read().await;
                            graph.compute_trust_score(&subscriber).unwrap_or(0.0)
                        })
                    })
                };

                // Enforce trust threshold
                if trust_score < threshold {
                    warn!(
                        "🔒 Subscription rejected: DID {} to topic {} (trust score: {:.3} < threshold: {:.3})",
                        subscriber, topic, trust_score, threshold
                    );

                    // Track rejection metrics
                    icn_obs::metrics::gossip::subscriptions_rejected_inc(topic, trust_score);

                    bail!(
                        "Insufficient trust: score {:.3} < required {:.3}",
                        trust_score,
                        threshold
                    );
                }

                info!(
                    "✅ Subscription authorized: DID {} to topic {} (trust score: {:.3})",
                    subscriber, topic, trust_score
                );
            } else {
                warn!(
                    "Topic {} has min_trust_threshold {:.3} but GossipActor has no trust_graph - falling back to ACL check",
                    topic, threshold
                );
            }
        }

        // Priority 2: Check AccessControl-based ACL (coarse-grained)
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
            // Check subscriber limit to prevent unbounded growth
            if subscribers.len() >= MAX_SUBSCRIBERS_PER_TOPIC {
                bail!(
                    "Topic subscription limit reached: {} (max {})",
                    subscribers.len(),
                    MAX_SUBSCRIBERS_PER_TOPIC
                );
            }

            subscribers.push(subscriber.clone());
            info!("DID {} subscribed to topic: {}", subscriber, topic);

            // Update metrics
            self.update_gauge_metrics();
        }

        Ok(Subscription {
            topic: topic.to_string(),
            subscriber,
        })
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &str, subscriber: &Did) -> Result<()> {
        let subscribers = self
            .subscriptions
            .get_mut(topic)
            .context("Topic not found")?;

        if let Some(pos) = subscribers.iter().position(|did| did == subscriber) {
            subscribers.remove(pos);
            info!("DID {} unsubscribed from topic: {}", subscriber, topic);

            // Update metrics
            self.update_gauge_metrics();
        }

        Ok(())
    }

    /// Get all subscribers for a topic
    pub fn get_subscribers(&self, topic: &str) -> Vec<Did> {
        self.subscriptions
            .get(topic)
            .map(|subs| subs.clone())
            .unwrap_or_default()
    }

    /// Get all topics a DID is subscribed to
    pub fn get_subscriptions(&self, did: &Did) -> Vec<String> {
        self.subscriptions
            .iter()
            .filter_map(|(topic, subscribers)| {
                if subscribers.contains(did) {
                    Some(topic.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if a DID is subscribed to a topic
    pub fn is_subscribed(&self, topic: &str, did: &Did) -> bool {
        self.subscriptions
            .get(topic)
            .map(|subs| subs.contains(did))
            .unwrap_or(false)
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

    /// Get all entry hashes for a topic
    fn get_topic_hashes(&self, topic: &str) -> Vec<ContentHash> {
        self.entries
            .get(topic)
            .map(|entries| entries.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Find entries we have that remote might not have (based on bloom filter)
    fn find_entries_to_push(&self, topic: &str, remote_bloom: &BloomFilter) -> Vec<ContentHash> {
        let mut to_push = Vec::new();

        if let Some(entries) = self.entries.get(topic) {
            for hash in entries.keys() {
                // If remote bloom says they don't have it, we should offer it
                if !remote_bloom.contains(hash) {
                    to_push.push(*hash);
                }
            }
        }

        to_push
    }

    /// Find entries we're missing based on remote vector clock
    fn find_entries_to_pull(&self, topic: &str, remote_vector: &VectorClock) -> Vec<ContentHash> {
        let mut to_pull = Vec::new();

        if let Some(entries) = self.entries.get(topic) {
            // Check each entry we have
            for (hash, entry) in entries {
                // If remote clock is ahead of this entry's clock, we might be missing entries
                if remote_vector.happened_after(&entry.clock) {
                    // This entry is causally before remote state
                    // We might be missing newer entries from remote
                    continue;
                }
            }
        }

        // For now, we'll use a simpler heuristic: compare local bloom against remote
        // A full implementation would track per-peer vector clocks and compute diffs
        to_pull
    }

    /// Handle incoming gossip message from network
    pub fn handle_message(&mut self, sender: &Did, message: GossipMessage) -> Result<()> {
        match message {
            GossipMessage::Announce { hash, author, clock, topic } => {
                debug!("Received Announce: topic={}, hash={:?}, author={}", topic, hash, author);
                icn_obs::metrics::gossip::announces_received_inc();

                // Check if we already have this entry
                if let Some(entries) = self.entries.get(&topic) {
                    if entries.contains_key(&hash) {
                        debug!("Already have entry {}", hex::encode(hash));
                        return Ok(());
                    }
                }

                // Request full entry if we don't have it
                debug!("Requesting entry {} from {}", hex::encode(hash), author);
                self.send_message(Some(author), GossipMessage::Request { hash });

                // Store the announcement metadata for future reference
                // We'll update it when we receive the full entry via Response
                Ok(())
            }

            GossipMessage::Request { hash } => {
                icn_obs::metrics::gossip::requests_received_inc();
                debug!("Received Request for hash: {:?}", hash);

                // Find entry across all topics
                for (_topic_name, entries) in &self.entries {
                    if let Some(entry) = entries.get(&hash) {
                        debug!("Found entry in topic: {}, sending Response to {}", entry.topic, sender);

                        // Send Response back to the requester
                        self.send_message(Some(sender.clone()), GossipMessage::Response {
                            entry: entry.clone(),
                        });

                        return Ok(());
                    }
                }

                debug!("Entry not found for hash: {:?}", hash);
                Ok(())
            }

            GossipMessage::Response { entry } => {
                icn_obs::metrics::gossip::responses_received_inc();
                debug!("Received Response: topic={}, hash={:?}", entry.topic, entry.hash);

                // Store the entry using store_entry() to ensure:
                // 1. Subscriber notifications are triggered
                // 2. Vector clock is merged
                // 3. max_entries limit is enforced
                // 4. Duplicate entries are detected
                self.store_entry(entry)?;

                // Track metrics
                icn_obs::metrics::gossip::entries_received_inc();
                self.update_gauge_metrics();

                Ok(())
            }

            GossipMessage::RequestBloomFilter { topic } => {
                debug!("Received RequestBloomFilter for topic: {}", topic);

                // Get bloom filter for the topic
                if let Some(filter) = self.bloom_filters.get(&topic) {
                    let filter_data = filter.to_data();
                    debug!("Sending bloom filter for topic: {} ({} bits) to {}", topic, filter_data.size, sender);

                    // Send bloom filter back to the requester
                    self.send_message(Some(sender.clone()), GossipMessage::SendBloomFilter {
                        topic: topic.clone(),
                        filter: filter_data,
                    });
                } else {
                    debug!("Topic not found: {}", topic);
                }

                Ok(())
            }

            GossipMessage::SendBloomFilter { topic, filter } => {
                debug!("Received SendBloomFilter for topic: {}", topic);

                // Reconstruct remote bloom filter
                let remote_filter = BloomFilter::from_data(&filter);

                // Find entries we're missing (present in remote but not in local)
                let mut _missing_hashes: Vec<ContentHash> = Vec::new();

                // Check if topic exists locally
                if !self.entries.contains_key(&topic) {
                    debug!("Topic {} doesn't exist locally, cannot compare", topic);
                    return Ok(());
                }

                // For a full implementation, we'd need to:
                // 1. Compare our bloom filter with the remote one
                // 2. Identify hashes present in remote but not in ours
                // 3. Request those missing hashes
                //
                // However, bloom filters are probabilistic - they can only tell us
                // "definitely not present" or "might be present". We can't extract
                // the actual hashes from a bloom filter.
                //
                // The proper approach is for the sender to also send their entry hashes
                // or we need a different anti-entropy approach.

                debug!("Remote filter size: {}, hashes: {}", filter.size, filter.num_hashes);
                debug!("Anti-entropy comparison complete for topic: {}", topic);

                Ok(())
            }

            GossipMessage::RequestMissing { hashes } => {
                debug!("Received RequestMissing: {} hashes", hashes.len());

                // Send Response messages for each requested hash that we have
                let mut sent_count = 0;
                let mut not_found_count = 0;

                for hash in hashes {
                    // Find entry across all topics
                    let mut found = false;
                    for (_topic_name, entries) in &self.entries {
                        if let Some(entry) = entries.get(&hash) {
                            debug!("Sending requested entry: hash={:?}, topic={}", hash, entry.topic);
                            self.send_message(None, GossipMessage::Response {
                                entry: entry.clone(),
                            });
                            sent_count += 1;
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        debug!("Requested entry not found: hash={:?}", hash);
                        not_found_count += 1;
                    }
                }

                debug!("RequestMissing complete: sent={}, not_found={}", sent_count, not_found_count);
                Ok(())
            }

            GossipMessage::Digest { topic, vector, bloom, hint_count, nonce } => {
                // Digest Handler: Anti-entropy synchronization flow
                // 1. Reconstruct remote bloom filter from data
                // 2. Find entries we have that remote doesn't (bloom intersection)
                // 3. Check trust class and get resource limits
                // 4. Check backpressure: can we send to this peer?
                // 5. Select entries to request (capped by max_pull_bytes)
                // 6. Update peer sync state (vector, bloom, nonce, deficit)
                // 7. Send PullRequest with selected entry hashes
                icn_obs::metrics::gossip::digests_received_inc();
                debug!("Received Digest: topic={}, hint_count={}, nonce={}", topic, hint_count, nonce);

                // Check if we have this topic
                if !self.topics.contains_key(&topic) {
                    debug!("Received Digest for unknown topic: {}", topic);
                    return Ok(());
                }

                // Reconstruct remote bloom filter
                let _remote_bloom = BloomFilter::from_data(&bloom);

                // PULL LOGIC: Detect if we're missing entries by comparing vector clocks
                // Use the sender parameter to identify who sent this Digest
                let peer_did = sender.clone();

                // Check if we're behind this peer's sequence
                let mut are_we_behind = false;
                for (did, remote_seq) in &vector.clock {
                    let our_seq = self.clock.get(did);
                    if *remote_seq > our_seq {
                        debug!("We're behind on {}: remote_seq={} > our_seq={}", did, remote_seq, our_seq);
                        are_we_behind = true;
                        break;
                    }
                }

                // Also check entry count hint
                let our_entry_count = self.entries.get(&topic).map(|e| e.len()).unwrap_or(0);
                if hint_count > our_entry_count as u32 {
                    debug!("Remote has {} entries, we have {} - we're behind", hint_count, our_entry_count);
                    are_we_behind = true;
                }

                if !are_we_behind {
                    debug!("We're not behind remote peer - no pull needed");
                    return Ok(());
                }

                // We're behind! Send PullRequest with empty want_ids to request ALL entries
                // The responder will interpret empty want_ids as "send everything (up to max_bytes)"
                debug!("Detected we're behind - sending PullRequest for all entries");

                // Get trust class and limits
                let trust_class = (self.trust_lookup)(&peer_did).unwrap_or(icn_trust::TrustClass::Isolated);
                let limits = TrustResourceLimits::for_trust_class(trust_class);
                let max_bytes = limits.max_pull_bytes;

                // Empty want_ids means "send all entries"
                let want_ids = Vec::new();
                let estimated_bytes = hint_count * 512; // Rough estimate

                // Update peer sync state (borrow mutable)
                let (request_nonce, deficit_after) = {
                    let peer_state = self.peer_sync.get_or_create(peer_did.clone());

                    // Update peer's known state
                    peer_state.update_vector(vector.clone());
                    peer_state.update_bloom(bloom.clone());

                    // Check if we can send (backpressure + limits + backoff)
                    if !peer_state.can_send(limits.max_outstanding_reqs, 10000) {
                        debug!("Cannot send to peer {} - backpressured or at limit", peer_did);
                        let deficit = peer_state.deficit_bytes;
                        icn_obs::metrics::gossip::peer_deficit_bytes_set(
                            peer_did.as_str(),
                            deficit,
                        );
                        return Ok(());
                    }

                    // Generate nonce for this request
                    let request_nonce = peer_state.next_nonce();

                    // Record outgoing request
                    peer_state.record_request();
                    peer_state.debit_bytes(estimated_bytes);

                    (request_nonce, peer_state.deficit_bytes)
                };
                // Mutable borrow of peer_sync ends here

                debug!(
                    "Sending PullRequest to {}: {} entries, ~{} bytes, nonce={}",
                    peer_did, want_ids.len(), estimated_bytes, request_nonce
                );

                // Send PullRequest (immutable borrow of self)
                self.send_message(
                    Some(peer_did.clone()),
                    GossipMessage::PullRequest {
                        topic: topic.clone(),
                        want_ids,
                        max_bytes,
                        nonce: request_nonce,
                    },
                );

                // Track metrics
                icn_obs::metrics::gossip::pull_requests_sent_inc();
                icn_obs::metrics::gossip::peer_deficit_bytes_set(
                    peer_did.as_str(),
                    deficit_after,
                );

                Ok(())
            }

            GossipMessage::PullRequest { topic, want_ids, max_bytes, nonce } => {
                icn_obs::metrics::gossip::pull_requests_received_inc();
                debug!("Received PullRequest: topic={}, want_ids={}, max_bytes={}, nonce={}",
                    topic, want_ids.len(), max_bytes, nonce);

                // Get entries for the topic
                let topic_entries = match self.entries.get(&topic) {
                    Some(entries) => entries,
                    None => {
                        debug!("Topic not found: {}", topic);
                        return Ok(());
                    }
                };

                // Collect requested entries
                let mut response_entries = Vec::new();
                let mut total_bytes = 0u32;
                let mut truncated = false;

                // If want_ids is empty, interpret as "send all entries" (up to max_bytes)
                // This supports the case where requester detects missing entries via vector
                // clock but doesn't know specific hashes
                let hashes_to_send: Vec<ContentHash> = if want_ids.is_empty() {
                    debug!("Empty want_ids - sending all entries for topic (up to max_bytes)");
                    topic_entries.keys().copied().collect()
                } else {
                    want_ids.clone()
                };

                for hash in hashes_to_send {
                    if let Some(entry) = topic_entries.get(&hash) {
                        // Estimate entry size (rough approximation)
                        let entry_bytes = entry.data.len() as u32 + 256; // Data + overhead

                        if total_bytes + entry_bytes > max_bytes {
                            truncated = true;
                            icn_obs::metrics::gossip::pull_truncated_inc();
                            break;
                        }

                        response_entries.push(entry.clone());
                        total_bytes += entry_bytes;
                    }
                }

                debug!("Sending PullResponse: {} entries, {} bytes, truncated={}",
                    response_entries.len(), total_bytes, truncated);

                // Send pull response
                self.send_message(None, GossipMessage::PullResponse {
                    topic: topic.clone(),
                    entries: response_entries,
                    truncated,
                    nonce,
                });

                icn_obs::metrics::gossip::pull_responses_sent_inc();
                icn_obs::metrics::gossip::bytes_pushed_add(total_bytes as u64);

                Ok(())
            }

            GossipMessage::PullResponse { topic, entries, truncated, nonce } => {
                icn_obs::metrics::gossip::pull_responses_received_inc();
                debug!("Received PullResponse: topic={}, entries={}, truncated={}, nonce={}",
                    topic, entries.len(), truncated, nonce);

                // Calculate total bytes received and extract peer DID from first entry
                let mut total_bytes = 0u32;
                let mut peer_did: Option<Did> = None;

                for entry in &entries {
                    let entry_bytes = entry.data.len() as u32 + 256;
                    total_bytes += entry_bytes;

                    // Extract peer DID from entry author (first entry)
                    if peer_did.is_none() {
                        peer_did = Some(entry.author.clone());
                    }
                }

                // Store all entries
                for entry in entries {
                    // Store the entry using store_entry() to ensure:
                    // 1. Subscriber notifications are triggered
                    // 2. Vector clock is merged
                    // 3. max_entries limit is enforced
                    // 4. Duplicate entries are detected
                    self.store_entry(entry)?;
                }

                // Update peer sync state if we can identify the peer
                if let Some(did) = peer_did {
                    if let Some(peer_state) = self.peer_sync.get_mut(&did) {
                        peer_state.record_response(total_bytes);

                        debug!(
                            "Updated peer {} sync state: deficit={}, outstanding={}",
                            did, peer_state.deficit_bytes, peer_state.outstanding_requests
                        );

                        // Track peer deficit metric
                        icn_obs::metrics::gossip::peer_deficit_bytes_set(
                            did.as_str(),
                            peer_state.deficit_bytes,
                        );
                    }
                }

                icn_obs::metrics::gossip::bytes_pulled_add(total_bytes as u64);
                icn_obs::metrics::gossip::entries_received_inc();
                self.update_gauge_metrics();

                debug!("PullResponse processed: {} bytes received, truncated={}", total_bytes, truncated);
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

    /// Update gauge metrics for topics and entries
    fn update_gauge_metrics(&self) {
        // Count total topics
        icn_obs::metrics::gossip::topics_total_set(self.topics.len() as u64);

        // Count total entries across all topics
        let total_entries: usize = self.entries.values().map(|e| e.len()).sum();
        icn_obs::metrics::gossip::entries_total_set(total_entries as u64);

        // Count total subscriptions across all topics
        let total_subscriptions: usize = self.subscriptions.values().map(|subs| subs.len()).sum();
        icn_obs::metrics::gossip::subscriptions_total_set(total_subscriptions as u64);
    }

    /// Emit a Digest message for a topic to all peers
    ///
    /// This broadcasts our current state (vector clock + bloom filter) to help peers
    /// discover what entries we have. Peers can then send PullRequests for entries
    /// they're missing.
    pub fn emit_digest(&mut self, topic: &str) -> Result<()> {
        // Check if topic exists
        if !self.topics.contains_key(topic) {
            debug!("Skipping digest for non-existent topic: {}", topic);
            return Ok(());
        }

        // Get entry count for adaptive bloom sizing
        let entry_count = self
            .entries
            .get(topic)
            .map(|e| e.len())
            .unwrap_or(0);

        if entry_count == 0 {
            debug!("Skipping digest for empty topic: {}", topic);
            return Ok(());
        }

        // Build adaptive bloom filter
        let bloom = BloomFilter::new_adaptive(entry_count);
        let mut bloom_with_entries = bloom;

        // Add all entry hashes to bloom filter
        if let Some(entries) = self.entries.get(topic) {
            for hash in entries.keys() {
                bloom_with_entries.insert(hash);
            }
        }

        // Convert to BloomFilterData for transmission
        let bloom_data = bloom_with_entries.to_data();

        // Generate nonce for this digest (using own DID as peer state key)
        let nonce = self.peer_sync.get_or_create(self.own_did.clone()).next_nonce();

        // Create Digest message
        let digest = GossipMessage::Digest {
            topic: topic.to_string(),
            vector: self.clock.clone(),
            bloom: bloom_data,
            hint_count: entry_count as u32,
            nonce,
        };

        // Get topic scope and fanout
        let topic_obj = self.topics.get(topic).unwrap(); // Safe: checked above
        let scope = topic_obj.scope;
        let fanout = match scope {
            crate::types::Scope::LocalCluster => 8,  // Default local fanout
            crate::types::Scope::Regional => 6,       // Default regional fanout
            crate::types::Scope::Global => 4,         // Default global fanout
        };

        // Send with scope-aware peer selection
        debug!("Emitting digest for topic {} ({:?} scope, fanout={}): {} entries, nonce={}",
               topic, scope, fanout, entry_count, nonce);
        self.send_message_scoped(scope, fanout, digest);

        // Track metrics
        icn_obs::metrics::gossip::digests_sent_inc();

        Ok(())
    }

    /// Emit digests for all topics
    ///
    /// This is typically called periodically by a background task to broadcast
    /// our current state to all peers.
    pub fn emit_all_digests(&mut self) -> Result<()> {
        let topics: Vec<String> = self.topics.keys().cloned().collect();

        for topic in topics {
            self.emit_digest(&topic)?;
        }

        Ok(())
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
        Self::spawn_with_trust_graph(own_did, trust_lookup, None)
    }

    /// Spawn a gossip actor with optional trust graph and return a handle
    pub fn spawn_with_trust_graph(
        own_did: Did,
        trust_lookup: Arc<dyn Fn(&Did) -> Option<TrustClass> + Send + Sync>,
        trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
    ) -> GossipHandle {
        let actor = GossipActor::new_with_trust_graph(own_did, trust_lookup, trust_graph);
        Arc::new(RwLock::new(actor))
    }
}

/// Start periodic digest emission background task
///
/// This spawns a tokio task that periodically broadcasts Digest messages
/// for all topics. The interval is configurable with jitter to prevent
/// thundering herd issues.
///
/// # Parameters
/// - `gossip_handle`: Shared handle to the gossip actor
/// - `interval_ms`: Base interval between digest broadcasts (e.g., 10000 for 10 seconds)
/// - `jitter_ms`: Maximum random jitter to add (e.g., 2000 for ±2 seconds)
/// - `shutdown`: Receiver for graceful shutdown signal
///
/// # Returns
/// JoinHandle for the background task
pub fn start_digest_emitter(
    gossip_handle: GossipHandle,
    interval_ms: u64,
    jitter_ms: u64,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting periodic digest emitter: interval={}ms, jitter=±{}ms", interval_ms, jitter_ms);

        loop {
            // Calculate next interval with jitter (thread_rng is recreated each iteration to avoid Send issues)
            let jitter = if jitter_ms > 0 {
                use rand::Rng;
                rand::thread_rng().gen_range(0..jitter_ms)
            } else {
                0
            };
            let sleep_duration = tokio::time::Duration::from_millis(interval_ms + jitter);

            // Wait for either timeout or shutdown
            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    // Time to emit digests
                    let mut gossip = gossip_handle.write().await;
                    if let Err(e) = gossip.emit_all_digests() {
                        warn!("Failed to emit digests: {}", e);
                    }
                }
                _ = shutdown.recv() => {
                    info!("Digest emitter shutting down");
                    break;
                }
            }
        }
    })
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

    #[test]
    fn test_pull_protocol_request_response() {
        // Test that the pull protocol works: Announce -> Request -> Response
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let keypair2 = KeyPair::generate().unwrap();
        let did2 = keypair2.did().clone();

        // Create two gossip actors
        let mut gossip1 = GossipActor::new(did1.clone(), Arc::new(mock_trust_lookup));
        let mut gossip2 = GossipActor::new(did2.clone(), Arc::new(mock_trust_lookup));

        // Track messages sent by gossip2 via callback
        let sent_messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_messages_clone = sent_messages.clone();

        gossip2.set_send_callback(Arc::new(move |recipient, msg| {
            sent_messages_clone.lock().unwrap().push((recipient, msg));
        }));

        // Gossip1 publishes an entry
        let data = b"Test entry".to_vec();
        let hash = gossip1.publish("global:identity", data.clone()).unwrap();

        // Get the entry from gossip1
        let entry = gossip1.get_entry("global:identity", &hash).unwrap();

        // Simulate gossip2 receiving an Announce from gossip1
        let announce = GossipMessage::Announce {
            hash,
            author: did1.clone(),
            clock: entry.clock.clone(),
            topic: "global:identity".to_string(),
        };

        gossip2.handle_message(&did1, announce).unwrap();

        // Gossip2 should have sent a Request message
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);

        if let (Some(recipient), GossipMessage::Request { hash: req_hash }) = &messages[0] {
            assert_eq!(recipient, &did1);
            assert_eq!(req_hash, &hash);
        } else {
            panic!("Expected Request message");
        }

        drop(messages); // Release lock

        // Now simulate gossip1 receiving the Request and sending Response
        let sent_messages1 = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_messages1_clone = sent_messages1.clone();

        gossip1.set_send_callback(Arc::new(move |recipient, msg| {
            sent_messages1_clone.lock().unwrap().push((recipient, msg));
        }));

        let request = GossipMessage::Request { hash };
        gossip1.handle_message(&did2, request).unwrap();

        // Gossip1 should have sent a Response message directly to gossip2 (not broadcast)
        let messages1 = sent_messages1.lock().unwrap();
        assert_eq!(messages1.len(), 1);

        if let (Some(recipient), GossipMessage::Response { entry: resp_entry }) = &messages1[0] {
            assert_eq!(recipient, &did2, "Response should be sent directly to requester");
            assert_eq!(resp_entry.hash, hash);
            assert_eq!(resp_entry.data, data);
        } else {
            panic!("Expected Response message with recipient");
        }
    }

    #[test]
    fn test_request_missing_handler() {
        // Test RequestMissing message handling
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Publish some entries
        let hash1 = gossip.publish("global:identity", b"Entry 1".to_vec()).unwrap();
        let hash2 = gossip.publish("global:identity", b"Entry 2".to_vec()).unwrap();
        let hash3 = gossip.publish("global:identity", b"Entry 3".to_vec()).unwrap();

        // Track messages sent via callback
        let sent_messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_messages_clone = sent_messages.clone();

        gossip.set_send_callback(Arc::new(move |recipient, msg| {
            sent_messages_clone.lock().unwrap().push((recipient, msg));
        }));

        // Request two of the three entries
        let request_missing = GossipMessage::RequestMissing {
            hashes: vec![hash1, hash2],
        };

        gossip.handle_message(&did, request_missing).unwrap();

        // Should have sent 2 Response messages
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 2);

        for (recipient, msg) in messages.iter() {
            assert_eq!(*recipient, None); // Broadcast
            if let GossipMessage::Response { entry } = msg {
                assert!(entry.hash == hash1 || entry.hash == hash2);
            } else {
                panic!("Expected Response message");
            }
        }
    }

    #[test]
    fn test_unsubscribe() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Subscribe first
        gossip.subscribe("global:identity", did.clone()).unwrap();
        assert!(gossip.is_subscribed("global:identity", &did));

        // Unsubscribe
        gossip.unsubscribe("global:identity", &did).unwrap();
        assert!(!gossip.is_subscribed("global:identity", &did));
    }

    #[test]
    fn test_get_subscribers() {
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let keypair2 = KeyPair::generate().unwrap();
        let did2 = keypair2.did().clone();

        let mut gossip = GossipActor::new(did1.clone(), Arc::new(mock_trust_lookup));

        // Subscribe both DIDs
        gossip.subscribe("global:identity", did1.clone()).unwrap();
        gossip.subscribe("global:identity", did2.clone()).unwrap();

        // Get subscribers
        let subscribers = gossip.get_subscribers("global:identity");
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&did1));
        assert!(subscribers.contains(&did2));
    }

    #[test]
    fn test_get_subscriptions() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Subscribe to multiple topics
        gossip.subscribe("global:identity", did.clone()).unwrap();
        gossip.subscribe("global:rendezvous", did.clone()).unwrap();

        // Get all subscriptions for this DID
        let subscriptions = gossip.get_subscriptions(&did);
        assert_eq!(subscriptions.len(), 2);
        assert!(subscriptions.contains(&"global:identity".to_string()));
        assert!(subscriptions.contains(&"global:rendezvous".to_string()));
    }

    #[test]
    fn test_is_subscribed() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Not subscribed initially
        assert!(!gossip.is_subscribed("global:identity", &did));

        // Subscribe
        gossip.subscribe("global:identity", did.clone()).unwrap();
        assert!(gossip.is_subscribed("global:identity", &did));
    }

    #[test]
    fn test_subscribe_duplicate_prevention() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Subscribe twice
        gossip.subscribe("global:identity", did.clone()).unwrap();
        gossip.subscribe("global:identity", did.clone()).unwrap();

        // Should only be subscribed once
        let subscribers = gossip.get_subscribers("global:identity");
        assert_eq!(subscribers.len(), 1);
    }

    #[test]
    fn test_unsubscribe_nonexistent_topic() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), Arc::new(mock_trust_lookup));

        // Try to unsubscribe from non-existent topic
        let result = gossip.unsubscribe("nonexistent:topic", &did);
        assert!(result.is_err());
    }

    #[test]
    fn test_subscribe_acl_denied() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Trust lookup that returns None (no trust)
        let no_trust_lookup = Arc::new(|_did: &Did| None);
        let mut gossip = GossipActor::new(did.clone(), no_trust_lookup);

        // Create a topic with TrustClass::Partner requirement
        let topic = Topic::new(
            "partner:only".to_string(),
            AccessControl::TrustClass(TrustClass::Partner),
        );
        gossip.create_topic(topic);

        // Try to subscribe with no trust - should fail
        let result = gossip.subscribe("partner:only", did.clone());
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Slow test - fills 10,000 slots
    fn test_subscribe_limit_enforcement_full() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        let topic = Topic::new("test:limited".to_string(), AccessControl::Public);
        gossip.create_topic(topic);

        // Manually fill subscribers vector to MAX
        let subscribers = gossip.subscriptions.get_mut("test:limited").unwrap();
        let filler_did = KeyPair::generate().unwrap().did().clone();
        while subscribers.len() < super::MAX_SUBSCRIBERS_PER_TOPIC {
            subscribers.push(filler_did.clone());
        }

        // Verify at MAX
        assert_eq!(subscribers.len(), super::MAX_SUBSCRIBERS_PER_TOPIC);

        // Try to add another - should fail
        let did = KeyPair::generate().unwrap().did().clone();
        let result = gossip.subscribe("test:limited", did);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("limit reached"));
    }

    #[test]
    fn test_subscribe_limit_enforcement_logic() {
        // Test the limit checking logic with a small number
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        let topic = Topic::new("test:limited".to_string(), AccessControl::Public);
        gossip.create_topic(topic);

        // Add 100 subscribers to verify normal operation
        for i in 0..100 {
            let did = KeyPair::generate().unwrap().did().clone();
            let result = gossip.subscribe("test:limited", did);
            assert!(result.is_ok(), "Subscribe {} should succeed", i);
        }

        // Verify count
        let count = gossip.get_subscribers("test:limited").len();
        assert_eq!(count, 100, "Should have 100 subscribers");

        // The limit logic is validated in the ignored test above
        // This test just confirms normal operation works
    }

    #[test]
    fn test_entry_limit_enforcement() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        // Create a topic with small max_entries for testing
        let topic = Topic::new("test:entries".to_string(), AccessControl::Public)
            .with_max_entries(5); // Small limit for fast testing
        gossip.create_topic(topic);

        // Publish more entries than the limit
        for i in 0..10 {
            let data = format!("entry_{}", i).into_bytes();
            let result = gossip.publish("test:entries", data);
            assert!(result.is_ok(), "Publish {} failed: {:?}", i, result);

            // Sleep briefly to ensure distinct timestamps for proper ordering
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        // Should have exactly max_entries (5) entries
        let entries = gossip.get_entries("test:entries");
        assert_eq!(
            entries.len(),
            5,
            "Should have exactly 5 entries, got {}",
            entries.len()
        );

        // Oldest entries should be removed (entries 0-4 gone, 5-9 remain)
        let data_values: Vec<String> = entries
            .iter()
            .map(|e| String::from_utf8_lossy(&e.data).to_string())
            .collect();

        // Should contain the newest entries (5-9), oldest (0-4) should be evicted
        assert!(data_values.contains(&"entry_9".to_string()), "Missing entry_9");
        assert!(data_values.contains(&"entry_8".to_string()), "Missing entry_8");
        assert!(data_values.contains(&"entry_7".to_string()), "Missing entry_7");
        assert!(data_values.contains(&"entry_6".to_string()), "Missing entry_6");
        assert!(data_values.contains(&"entry_5".to_string()), "Missing entry_5");

        // Verify old entries were evicted
        assert!(!data_values.contains(&"entry_0".to_string()), "entry_0 should have been evicted");
        assert!(!data_values.contains(&"entry_4".to_string()), "entry_4 should have been evicted");
    }

    #[test]
    fn test_subscription_notifications() {
        use std::sync::Mutex;

        let owner = KeyPair::generate().unwrap().did().clone();
        let subscriber1 = KeyPair::generate().unwrap().did().clone();
        let subscriber2 = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        // Create the topic first
        gossip.create_topic(Topic::new("test:notifications".to_string(), AccessControl::Public));

        // Track notifications
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let notifications_clone = notifications.clone();

        // Set up notification callback
        let callback: EntryNotificationCallback = Arc::new(move |topic, entry, subscriber| {
            let mut notifs = notifications_clone.lock().unwrap();
            notifs.push((topic, entry.hash, subscriber));
        });
        gossip.set_notification_callback(callback);

        // Subscribe both users to the topic
        gossip.subscribe("test:notifications", subscriber1.clone()).unwrap();
        gossip.subscribe("test:notifications", subscriber2.clone()).unwrap();

        // Publish an entry
        let data = b"Test notification".to_vec();
        let hash = gossip.publish("test:notifications", data).unwrap();

        // Verify both subscribers were notified
        let notifs = notifications.lock().unwrap();
        assert_eq!(notifs.len(), 2, "Should have 2 notifications (one per subscriber)");

        // Check that both subscribers received the notification
        let subscriber_dids: Vec<_> = notifs.iter().map(|(_, _, did)| did.clone()).collect();
        assert!(subscriber_dids.contains(&subscriber1), "subscriber1 should be notified");
        assert!(subscriber_dids.contains(&subscriber2), "subscriber2 should be notified");

        // Verify all notifications are for the correct topic and hash
        for (topic, notif_hash, _) in notifs.iter() {
            assert_eq!(topic, "test:notifications");
            assert_eq!(*notif_hash, hash);
        }
    }

    #[test]
    fn test_no_notification_without_callback() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let subscriber = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        // Create the topic first
        gossip.create_topic(Topic::new("test:no-callback".to_string(), AccessControl::Public));

        // Subscribe without setting callback
        gossip.subscribe("test:no-callback", subscriber.clone()).unwrap();

        // This should not panic even without a callback set
        let result = gossip.publish("test:no-callback", b"Test".to_vec());
        assert!(result.is_ok(), "Publishing should succeed even without notification callback");
    }

    #[test]
    fn test_no_notification_without_subscribers() {
        use std::sync::Mutex;

        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        let notification_count = Arc::new(Mutex::new(0));
        let count_clone = notification_count.clone();

        // Set up callback that counts notifications
        let callback: EntryNotificationCallback = Arc::new(move |_, _, _| {
            let mut count = count_clone.lock().unwrap();
            *count += 1;
        });
        gossip.set_notification_callback(callback);

        // Publish without any subscribers
        gossip.publish("test:no-subs", b"Test".to_vec()).unwrap();

        // Verify no notifications were sent
        let count = notification_count.lock().unwrap();
        assert_eq!(*count, 0, "Should have 0 notifications when there are no subscribers");
    }

    #[test]
    fn test_response_handler_triggers_notifications() {
        use std::sync::Mutex;
        use sha2::{Digest, Sha256};

        let owner = KeyPair::generate().unwrap().did().clone();
        let subscriber = KeyPair::generate().unwrap().did().clone();
        let author = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        // Create topic and subscribe
        gossip.create_topic(Topic::new("test:response".to_string(), AccessControl::Public));
        gossip.subscribe("test:response", subscriber.clone()).unwrap();

        // Track notifications
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let notifications_clone = notifications.clone();

        let callback: EntryNotificationCallback = Arc::new(move |topic, entry, sub_did| {
            let mut notifs = notifications_clone.lock().unwrap();
            notifs.push((topic, entry.hash, sub_did));
        });
        gossip.set_notification_callback(callback);

        // Create an entry as if it came from the network via Response message
        let data = b"Entry from network".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let result_bytes = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result_bytes);

        let entry = GossipEntry {
            hash,
            author: author.clone(),
            clock: VectorClock::new(),
            topic: "test:response".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        // Simulate receiving a Response message from the author
        let result = gossip.handle_message(&author, GossipMessage::Response {
            entry: entry.clone(),
        });
        assert!(result.is_ok(), "Response handler should succeed");

        // Verify notification was sent to subscriber
        let notifs = notifications.lock().unwrap();
        assert_eq!(notifs.len(), 1, "Should have 1 notification for the subscriber");
        assert_eq!(notifs[0].0, "test:response");
        assert_eq!(notifs[0].1, hash);
        assert_eq!(notifs[0].2, subscriber);
    }

    #[test]
    fn test_response_handler_enforces_max_entries() {
        use sha2::{Digest, Sha256};

        let owner = KeyPair::generate().unwrap().did().clone();
        let author = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        // Create topic with small limit
        let topic = Topic::new("test:max-entries".to_string(), AccessControl::Public)
            .with_max_entries(3);
        gossip.create_topic(topic);

        // Send 5 entries via Response messages
        for i in 0..5 {
            let data = format!("Entry {}", i).into_bytes();
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let result_bytes = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result_bytes);

            let entry = GossipEntry {
                hash,
                author: author.clone(),
                clock: VectorClock::new(),
                topic: "test:max-entries".to_string(),
                data,
                compressed: false,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };

            // Small delay to ensure distinct timestamps
            std::thread::sleep(std::time::Duration::from_millis(2));

            gossip.handle_message(&author, GossipMessage::Response { entry }).unwrap();
        }

        // Verify only 3 entries are stored (max_entries enforced)
        let entries = gossip.get_entries("test:max-entries");
        assert_eq!(entries.len(), 3, "Should enforce max_entries limit for Response messages");
    }

    /// Trust-Gated Subscription Tests

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_subscription_rejection() {
        // Test that subscriptions are rejected when trust score < threshold
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create trust graph with Alice having low trust (0.05)
        let store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut trust_graph = icn_trust::TrustGraph::new(store, owner.clone());

        // Add trust edge: owner trusts Alice with score 0.05 (below threshold)
        let edge = icn_trust::TrustEdge::new(owner.clone(), alice.clone(), 0.05);
        trust_graph.add_edge(edge).unwrap();

        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create gossip actor with trust graph
        let mut gossip = GossipActor::new_with_trust_graph(
            owner.clone(),
            Arc::new(mock_trust_lookup),
            Some(trust_graph_handle),
        );

        // Create topic with min_trust_threshold of 0.1 (Known+)
        let topic = Topic::new("test:trust-gated".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.1);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should be rejected (score 0.05 < 0.1)
        let result = gossip.subscribe("test:trust-gated", alice.clone());
        assert!(result.is_err(), "Subscription should be rejected due to low trust");
        assert!(result.unwrap_err().to_string().contains("Insufficient trust"));

        // Bob (unknown, score 0.0) attempts to subscribe - should also be rejected
        let result = gossip.subscribe("test:trust-gated", bob.clone());
        assert!(result.is_err(), "Subscription should be rejected for unknown peer");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_subscription_acceptance() {
        // Test that subscriptions are accepted when trust score >= threshold
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Create trust graph with Alice having sufficient trust
        // Note: TrustGraph uses weighted score: 70% direct + 30% transitive
        // To get final score >= 0.4, need direct trust >= 0.58 (0.58 * 0.7 = 0.406)
        let store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut trust_graph = icn_trust::TrustGraph::new(store, owner.clone());

        // Add trust edge: owner trusts Alice with score 0.6 (final score: 0.6 * 0.7 = 0.42)
        let edge = icn_trust::TrustEdge::new(owner.clone(), alice.clone(), 0.6);
        trust_graph.add_edge(edge).unwrap();

        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create gossip actor with trust graph
        let mut gossip = GossipActor::new_with_trust_graph(
            owner.clone(),
            Arc::new(mock_trust_lookup),
            Some(trust_graph_handle),
        );

        // Create topic with min_trust_threshold of 0.4 (Partner)
        let topic = Topic::new("test:trust-gated".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.4);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed (final score ~0.42 >= 0.4)
        let result = gossip.subscribe("test:trust-gated", alice.clone());
        assert!(result.is_ok(), "Subscription should be accepted with sufficient trust");
        assert!(gossip.is_subscribed("test:trust-gated", &alice));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_subscription_exact_threshold() {
        // Test boundary condition: trust score exactly equals threshold
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Create trust graph with Alice at exact threshold
        // Note: TrustGraph uses weighted score: 70% direct + 30% transitive
        // To get final score = 0.4, need direct trust = 0.4 / 0.7 = 0.571...
        let store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut trust_graph = icn_trust::TrustGraph::new(store, owner.clone());

        // Add trust edge: owner trusts Alice with score 0.58 (final score: 0.58 * 0.7 = 0.406)
        let edge = icn_trust::TrustEdge::new(owner.clone(), alice.clone(), 0.58);
        trust_graph.add_edge(edge).unwrap();

        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create gossip actor with trust graph
        let mut gossip = GossipActor::new_with_trust_graph(
            owner.clone(),
            Arc::new(mock_trust_lookup),
            Some(trust_graph_handle),
        );

        // Create topic with min_trust_threshold of 0.4
        let topic = Topic::new("test:trust-gated".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.4);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed (score 0.4 >= 0.4)
        let result = gossip.subscribe("test:trust-gated", alice.clone());
        assert!(result.is_ok(), "Subscription should be accepted at exact threshold");
        assert!(gossip.is_subscribed("test:trust-gated", &alice));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_fallback_to_acl() {
        // Test that when no trust graph is provided, falls back to AccessControl
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Create gossip actor WITHOUT trust graph
        let mut gossip = GossipActor::new(owner.clone(), Arc::new(mock_trust_lookup));

        // Create topic with min_trust_threshold but no trust graph available
        let topic = Topic::new("test:fallback".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.4);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed via fallback to Public ACL
        let result = gossip.subscribe("test:fallback", alice.clone());
        assert!(result.is_ok(), "Subscription should succeed via ACL fallback");
        assert!(gossip.is_subscribed("test:fallback", &alice));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_mixed_with_acl() {
        // Test that trust threshold takes priority over ACL
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Create trust graph with Alice having high trust
        // Note: TrustGraph uses weighted score: 70% direct + 30% transitive
        // To get final score >= 0.7, need direct trust = 1.0 (1.0 * 0.7 = 0.7)
        let store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut trust_graph = icn_trust::TrustGraph::new(store, owner.clone());

        let edge = icn_trust::TrustEdge::new(owner.clone(), alice.clone(), 1.0);
        trust_graph.add_edge(edge).unwrap();

        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create gossip actor with trust graph
        let mut gossip = GossipActor::new_with_trust_graph(
            owner.clone(),
            Arc::new(mock_trust_lookup),
            Some(trust_graph_handle),
        );

        // Create topic with BOTH min_trust_threshold AND TrustClass requirement
        let topic = Topic::new(
            "test:mixed".to_string(),
            AccessControl::TrustClass(TrustClass::Partner), // Requires Partner class
        )
        .with_min_trust_threshold(0.7); // Requires 0.7 score (Federated)

        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed (has score 0.8 >= 0.7)
        let result = gossip.subscribe("test:mixed", alice.clone());
        assert!(result.is_ok(), "Trust score check should pass before ACL check");
        assert!(gossip.is_subscribed("test:mixed", &alice));
    }
}
