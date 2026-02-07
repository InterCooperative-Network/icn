//! Provider registry for blob transfer protocol (Issue #1071)
//!
//! Tracks which peers announced availability for which blob hashes.
//! Provider selection is deterministic: given the same local registry state,
//! the same provider is always selected (lowest DID lexicographic for tie-break).
//!
//! # Design Principles
//!
//! - **Local knowledge only**: The registry reflects what THIS node has seen.
//!   Different nodes may have different views. No global consensus.
//! - **Deterministic selection**: Given identical local state, selection is reproducible.
//! - **Announcements are hints**: The registry records announcements but the requester
//!   always validates the actual blob after transfer.

use icn_identity::Did;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Content hash for blob addressing (blake3)
type ContentHash = [u8; 32];

/// How long a provider announcement stays valid before expiry.
const DEFAULT_ANNOUNCEMENT_TTL: Duration = Duration::from_secs(15 * 60); // 15 minutes

/// Maximum number of providers tracked per blob hash.
const MAX_PROVIDERS_PER_BLOB: usize = 64;

/// Maximum number of blob hashes tracked in the registry.
const MAX_TRACKED_BLOBS: usize = 4096;

/// A recorded provider announcement.
#[derive(Clone, Debug)]
struct ProviderEntry {
    /// The announcing peer's DID
    did: Did,
    /// Blob size from announcement
    size_bytes: u64,
    /// When the announcement was received
    received_at: Instant,
}

impl ProviderEntry {
    #[allow(dead_code)] // Used by select_provider/list_providers (PR2d #1071)
    fn is_expired(&self, ttl: Duration) -> bool {
        self.received_at.elapsed() > ttl
    }
}

/// Registry of blob providers populated from BlobAnnounce messages.
///
/// Thread-safe: designed to be wrapped in `Arc<RwLock<_>>` by the caller.
pub struct ProviderRegistry {
    /// blob_hash → set of providers (sorted by DID for deterministic iteration)
    providers: HashMap<ContentHash, Vec<ProviderEntry>>,
    /// Announcement TTL
    #[allow(dead_code)] // Used by select_provider/list_providers (PR2d #1071)
    ttl: Duration,
}

impl ProviderRegistry {
    /// Create a new empty registry with default TTL.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            ttl: DEFAULT_ANNOUNCEMENT_TTL,
        }
    }

    /// Create a registry with a custom TTL (for testing).
    #[cfg(test)]
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            providers: HashMap::new(),
            ttl,
        }
    }

    /// Record a provider announcement.
    ///
    /// Called when a BlobAnnounce message is received from the gossip layer.
    /// Duplicate announcements from the same DID for the same blob update
    /// the timestamp (refreshing the TTL).
    pub fn record_announcement(
        &mut self,
        blob_hash: ContentHash,
        provider_did: Did,
        size_bytes: u64,
    ) {
        // Enforce global blob limit with LRU-style eviction
        if !self.providers.contains_key(&blob_hash) && self.providers.len() >= MAX_TRACKED_BLOBS {
            // Evict the blob with the oldest most-recent announcement
            let oldest = self
                .providers
                .iter()
                .map(|(hash, entries)| {
                    let newest = entries
                        .iter()
                        .map(|e| e.received_at)
                        .max()
                        .unwrap_or(Instant::now());
                    (*hash, newest)
                })
                .min_by_key(|(_, t)| *t)
                .map(|(h, _)| h);
            if let Some(to_evict) = oldest {
                self.providers.remove(&to_evict);
            }
        }

        let entries = self.providers.entry(blob_hash).or_default();

        // Update existing entry or insert new one
        if let Some(existing) = entries.iter_mut().find(|e| e.did == provider_did) {
            existing.received_at = Instant::now();
            existing.size_bytes = size_bytes;
            debug!(
                provider = %provider_did,
                blob = %hex::encode(blob_hash),
                "Provider announcement refreshed"
            );
        } else {
            // Enforce per-blob provider limit
            if entries.len() >= MAX_PROVIDERS_PER_BLOB {
                warn!(
                    blob = %hex::encode(blob_hash),
                    "Max providers per blob reached, ignoring new announcement"
                );
                return;
            }
            entries.push(ProviderEntry {
                did: provider_did.clone(),
                size_bytes,
                received_at: Instant::now(),
            });
            debug!(
                provider = %provider_did,
                blob = %hex::encode(blob_hash),
                size_bytes,
                "New provider recorded"
            );
        }
    }

    /// Select the best provider for a blob hash.
    ///
    /// Selection is deterministic: among non-expired providers, returns the one
    /// with the lexicographically lowest DID. If `exclude` is provided, those
    /// DIDs are skipped (e.g., already-failed providers).
    ///
    /// Returns `None` if no valid providers are available.
    #[allow(dead_code)] // API surface for PR2d #1071
    pub fn select_provider(
        &mut self,
        blob_hash: &ContentHash,
        exclude: &HashSet<Did>,
    ) -> Option<ProviderInfo> {
        let entries = self.providers.get_mut(blob_hash)?;

        // Remove expired entries
        entries.retain(|e| !e.is_expired(self.ttl));

        if entries.is_empty() {
            self.providers.remove(blob_hash);
            return None;
        }

        // Collect eligible providers (not excluded, not expired)
        let mut candidates: Vec<&ProviderEntry> = entries
            .iter()
            .filter(|e| !exclude.contains(&e.did))
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Deterministic selection: sort by DID string (lexicographic), pick lowest
        candidates.sort_by(|a, b| a.did.as_str().cmp(b.did.as_str()));

        candidates.first().map(|e| ProviderInfo {
            did: e.did.clone(),
            size_bytes: e.size_bytes,
        })
    }

    /// List all known providers for a blob hash (non-expired).
    #[allow(dead_code)] // API surface for PR2d #1071
    pub fn list_providers(&mut self, blob_hash: &ContentHash) -> Vec<ProviderInfo> {
        let Some(entries) = self.providers.get_mut(blob_hash) else {
            return Vec::new();
        };

        // Remove expired
        entries.retain(|e| !e.is_expired(self.ttl));

        let mut result: Vec<ProviderInfo> = entries
            .iter()
            .map(|e| ProviderInfo {
                did: e.did.clone(),
                size_bytes: e.size_bytes,
            })
            .collect();

        // Sort by DID string for deterministic ordering
        result.sort_by(|a, b| a.did.as_str().cmp(b.did.as_str()));
        result
    }

    /// Remove a specific provider for a blob (e.g., after transfer failure).
    #[allow(dead_code)] // API surface for PR2d #1071
    pub fn remove_provider(&mut self, blob_hash: &ContentHash, provider_did: &Did) {
        if let Some(entries) = self.providers.get_mut(blob_hash) {
            entries.retain(|e| e.did != *provider_did);
            if entries.is_empty() {
                self.providers.remove(blob_hash);
            }
        }
    }

    /// Number of blobs with at least one provider.
    #[allow(dead_code)] // API surface for PR2d #1071
    pub fn blob_count(&self) -> usize {
        self.providers.len()
    }

    /// Total number of provider entries across all blobs.
    #[allow(dead_code)] // API surface for PR2d #1071
    pub fn total_entries(&self) -> usize {
        self.providers.values().map(|v| v.len()).sum()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a selected provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfo {
    /// Provider's DID
    pub did: Did,
    /// Announced blob size in bytes
    pub size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    /// Generate a unique test DID from a fresh keypair.
    fn gen_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// Generate N DIDs and return them sorted lexicographically by string.
    fn sorted_dids(n: usize) -> Vec<Did> {
        let mut dids: Vec<Did> = (0..n).map(|_| gen_did()).collect();
        dids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        dids
    }

    #[test]
    fn record_and_select_single_provider() {
        let mut reg = ProviderRegistry::new();
        let hash = [0xAA; 32];
        let alice = gen_did();

        reg.record_announcement(hash, alice.clone(), 1024);

        let selected = reg.select_provider(&hash, &HashSet::new());
        assert_eq!(
            selected,
            Some(ProviderInfo {
                did: alice,
                size_bytes: 1024,
            })
        );
    }

    #[test]
    fn deterministic_lowest_did_tiebreak() {
        let mut reg = ProviderRegistry::new();
        let hash = [0xBB; 32];
        let dids = sorted_dids(3);

        // Insert in reverse order (highest first)
        reg.record_announcement(hash, dids[2].clone(), 1024);
        reg.record_announcement(hash, dids[0].clone(), 1024);
        reg.record_announcement(hash, dids[1].clone(), 1024);

        let selected = reg.select_provider(&hash, &HashSet::new()).unwrap();
        assert_eq!(selected.did, dids[0], "must pick lowest DID lex");
    }

    #[test]
    fn exclude_providers() {
        let mut reg = ProviderRegistry::new();
        let hash = [0xCC; 32];
        let dids = sorted_dids(3);

        reg.record_announcement(hash, dids[0].clone(), 1024);
        reg.record_announcement(hash, dids[1].clone(), 1024);
        reg.record_announcement(hash, dids[2].clone(), 1024);

        let mut exclude = HashSet::new();
        exclude.insert(dids[0].clone());

        let selected = reg.select_provider(&hash, &exclude).unwrap();
        assert_eq!(
            selected.did, dids[1],
            "lowest excluded, next lowest is selected"
        );
    }

    #[test]
    fn all_excluded_returns_none() {
        let mut reg = ProviderRegistry::new();
        let hash = [0xDD; 32];
        let alice = gen_did();

        reg.record_announcement(hash, alice.clone(), 1024);

        let mut exclude = HashSet::new();
        exclude.insert(alice);

        assert!(reg.select_provider(&hash, &exclude).is_none());
    }

    #[test]
    fn unknown_blob_returns_none() {
        let mut reg = ProviderRegistry::new();
        assert!(reg.select_provider(&[0xFF; 32], &HashSet::new()).is_none());
    }

    #[test]
    fn expired_providers_evicted() {
        let mut reg = ProviderRegistry::with_ttl(Duration::from_millis(1));
        let hash = [0xEE; 32];
        let alice = gen_did();

        reg.record_announcement(hash, alice, 1024);

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(5));

        assert!(reg.select_provider(&hash, &HashSet::new()).is_none());
    }

    #[test]
    fn refresh_announcement_extends_ttl() {
        let mut reg = ProviderRegistry::with_ttl(Duration::from_millis(50));
        let hash = [0x11; 32];
        let alice = gen_did();

        reg.record_announcement(hash, alice.clone(), 1024);
        std::thread::sleep(Duration::from_millis(30));

        // Refresh before expiry
        reg.record_announcement(hash, alice, 2048);
        std::thread::sleep(Duration::from_millis(30));

        // Should still be valid (refreshed 30ms ago, TTL is 50ms)
        let selected = reg.select_provider(&hash, &HashSet::new());
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().size_bytes, 2048, "size should be updated");
    }

    #[test]
    fn remove_provider() {
        let mut reg = ProviderRegistry::new();
        let hash = [0x22; 32];
        let dids = sorted_dids(2);

        reg.record_announcement(hash, dids[0].clone(), 1024);
        reg.record_announcement(hash, dids[1].clone(), 1024);

        reg.remove_provider(&hash, &dids[0]);

        let selected = reg.select_provider(&hash, &HashSet::new()).unwrap();
        assert_eq!(selected.did, dids[1]);
    }

    #[test]
    fn list_providers_sorted() {
        let mut reg = ProviderRegistry::new();
        let hash = [0x33; 32];
        let dids = sorted_dids(3);

        // Insert in reverse order
        reg.record_announcement(hash, dids[2].clone(), 3000);
        reg.record_announcement(hash, dids[0].clone(), 1000);
        reg.record_announcement(hash, dids[1].clone(), 2000);

        let providers = reg.list_providers(&hash);
        assert_eq!(providers.len(), 3);
        assert_eq!(providers[0].did, dids[0]);
        assert_eq!(providers[1].did, dids[1]);
        assert_eq!(providers[2].did, dids[2]);
    }

    #[test]
    fn max_providers_per_blob_enforced() {
        let mut reg = ProviderRegistry::new();
        let hash = [0x44; 32];

        for _ in 0..MAX_PROVIDERS_PER_BLOB {
            reg.record_announcement(hash, gen_did(), 1024);
        }

        assert_eq!(reg.list_providers(&hash).len(), MAX_PROVIDERS_PER_BLOB);

        // One more should be rejected
        reg.record_announcement(hash, gen_did(), 1024);
        assert_eq!(reg.list_providers(&hash).len(), MAX_PROVIDERS_PER_BLOB);
    }

    #[test]
    fn stats() {
        let mut reg = ProviderRegistry::new();
        let alice = gen_did();
        let bob = gen_did();

        reg.record_announcement([0x11; 32], alice.clone(), 1024);
        reg.record_announcement([0x11; 32], bob, 1024);
        reg.record_announcement([0x22; 32], alice, 2048);

        assert_eq!(reg.blob_count(), 2);
        assert_eq!(reg.total_entries(), 3);
    }
}
