//! Vector clocks for causal ordering
//!
//! Includes LRU eviction to prevent unbounded memory growth in high-churn networks.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Instant;

/// Maximum number of entries in a vector clock before LRU eviction kicks in.
/// This prevents OOM in networks with high node churn.
pub const MAX_ENTRIES: usize = 10_000;

/// Threshold for triggering cleanup (80% of max)
const CLEANUP_THRESHOLD: usize = 8_000;

/// Entry in the vector clock with activity tracking for LRU eviction
#[derive(Debug, Clone)]
struct ClockEntry {
    /// The sequence number for this node
    count: u64,
    /// Last time this entry was accessed (for LRU eviction)
    /// Note: Not serialized - reconstructed on deserialization
    last_seen: Instant,
}

impl ClockEntry {
    fn new(count: u64) -> Self {
        Self {
            count,
            last_seen: Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_seen = Instant::now();
    }
}

/// Vector clock for tracking causality in distributed systems
///
/// Includes LRU eviction to prevent unbounded memory growth:
/// - Maximum of [`MAX_ENTRIES`] entries (default: 10,000)
/// - Evicts least-recently-used entries when threshold is exceeded
/// - Tracks last access time per entry for accurate LRU ordering
#[derive(Debug, Clone)]
pub struct VectorClock {
    /// Clock map: Node DID -> (sequence number, last_seen)
    entries: HashMap<Did, ClockEntry>,
}

/// Serialization wrapper that only includes count (not last_seen)
#[derive(Serialize, Deserialize)]
struct SerializedClock {
    #[serde(deserialize_with = "deserialize_clock_merged_by_max")]
    clock: HashMap<Did, u64>,
}

/// Decode a clock map, merging entries that name the same principal by **max**.
///
/// I7 (#2627) makes `Did` equality principal equality, so two accepted multibase
/// spellings of one node are one key here. `HashMap`'s derived `Deserialize`
/// resolves that collapse by plain insertion — the last entry in the input wins
/// — and postcard preserves whatever order the sender wrote. A clock carrying
/// `{spelling_a: 9, spelling_b: 5}` would therefore decode to 5 and move a
/// counter *backwards*, which for a vector clock means re-requesting entries
/// already seen and, where a clock gates delivery, re-accepting them.
///
/// Vector clocks merge by maximum — that is what [`VectorClock::merge`] does —
/// and this applies the same rule at the decode boundary, which is the only
/// place a duplicate key can appear.
///
/// This is correct in both regimes and is not conditional on I7: before I7 no
/// two keys collapse, so the fold is an identity over the derived behaviour;
/// after I7 max is the only answer consistent with `merge`. It reads exactly the
/// map shape the derive read and serialization is untouched, so **no wire byte
/// changes**. `VectorClockProjection::from_entries` in `icn-kernel-api` already
/// picks max for the same reason, and the N2-A0 inventory names it as the
/// precedent (`docs/architecture/n2-a0-stored-key-inventory.md` §12.1, ordering
/// constraint (ii)).
fn deserialize_clock_merged_by_max<'de, D>(deserializer: D) -> Result<HashMap<Did, u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct MergeByMax;

    impl<'de> serde::de::Visitor<'de> for MergeByMax {
        type Value = HashMap<Did, u64>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map of node DID to sequence count")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            // Cap only the *pre-allocation*, so a declared length cannot size
            // an allocation on its own. This is not an entry-count limit: the
            // map still grows past `MAX_ENTRIES` if that many real entries
            // arrive, exactly as the derived `HashMap` decoder did. `MAX_ENTRIES`
            // is enforced by `maybe_evict` on increment/merge, not on decode —
            // rejecting oversized clocks here would change what the wire
            // accepts, which is a separate decision from I7.
            let mut clock =
                HashMap::with_capacity(access.size_hint().unwrap_or(0).min(MAX_ENTRIES));
            while let Some((node, count)) = access.next_entry::<Did, u64>()? {
                let entry = clock.entry(node).or_insert(0u64);
                *entry = (*entry).max(count);
            }
            Ok(clock)
        }
    }

    deserializer.deserialize_map(MergeByMax)
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        VectorClock {
            entries: HashMap::new(),
        }
    }

    /// Create a vector clock with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        VectorClock {
            entries: HashMap::with_capacity(capacity.min(MAX_ENTRIES)),
        }
    }

    /// Increment the clock for a given node
    pub fn increment(&mut self, node: &Did) {
        self.maybe_evict();
        let entry = self
            .entries
            .entry(node.clone())
            .or_insert_with(|| ClockEntry::new(0));
        entry.count += 1;
        entry.touch();
    }

    /// Get the count for a node
    pub fn get(&self, node: &Did) -> u64 {
        self.entries.get(node).map(|e| e.count).unwrap_or(0)
    }

    /// Get the count for a node, updating last_seen time
    pub fn get_mut(&mut self, node: &Did) -> u64 {
        if let Some(entry) = self.entries.get_mut(node) {
            entry.touch();
            entry.count
        } else {
            0
        }
    }

    /// Merge another vector clock into this one (take max of each entry)
    pub fn merge(&mut self, other: &VectorClock) {
        for (node, other_entry) in &other.entries {
            self.maybe_evict();
            let entry = self
                .entries
                .entry(node.clone())
                .or_insert_with(|| ClockEntry::new(0));
            entry.count = entry.count.max(other_entry.count);
            entry.touch();
        }
    }

    /// Check if this clock happened before another (this < other)
    ///
    /// Returns true if:
    /// - All entries in this clock are <= corresponding entries in other
    /// - At least one entry in this clock is < corresponding entry in other
    pub fn happened_before(&self, other: &VectorClock) -> bool {
        let mut at_least_one_less = false;

        // Check all entries in this clock
        // If any entry in this > other, return false immediately
        for (node, entry) in &self.entries {
            let other_count = other.get(node);
            if entry.count > other_count {
                return false; // Found an entry where this > other
            }
            if entry.count < other_count {
                at_least_one_less = true;
            }
        }

        // Check for entries in other but not in this (implicitly this=0 < other)
        for (node, other_entry) in &other.entries {
            if !self.entries.contains_key(node) && other_entry.count > 0 {
                at_least_one_less = true;
            }
        }

        // If we reach here, all entries are <= (no early return)
        // Return true only if at least one entry was strictly less
        at_least_one_less
    }

    /// Check if this clock happened after another (this > other)
    pub fn happened_after(&self, other: &VectorClock) -> bool {
        other.happened_before(self)
    }

    /// Check if two clocks are concurrent (neither happened before the other)
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.happened_before(other) && !other.happened_before(self) && self != other
    }

    /// Compare two vector clocks
    pub fn partial_cmp(&self, other: &VectorClock) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else if self.happened_before(other) {
            Some(Ordering::Less)
        } else if self.happened_after(other) {
            Some(Ordering::Greater)
        } else {
            None // Concurrent
        }
    }

    /// Get the number of entries in the clock
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the clock is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Evict least-recently-used entries if we're above the cleanup threshold.
    ///
    /// This prevents unbounded memory growth in networks with high node churn.
    fn maybe_evict(&mut self) {
        if self.entries.len() < CLEANUP_THRESHOLD {
            return;
        }

        // Collect entries sorted by last_seen (oldest first)
        let mut entries_by_age: Vec<_> = self
            .entries
            .iter()
            .map(|(did, entry)| (did.clone(), entry.last_seen))
            .collect();
        entries_by_age.sort_by_key(|(_, last_seen)| *last_seen);

        // Evict oldest entries until we're below threshold
        let to_remove = self.entries.len().saturating_sub(CLEANUP_THRESHOLD / 2);
        for (did, _) in entries_by_age.into_iter().take(to_remove) {
            self.entries.remove(&did);
        }
    }

    /// Force eviction of stale entries older than the given duration.
    ///
    /// Call this periodically (e.g., every minute) to clean up inactive nodes.
    pub fn prune_stale(&mut self, max_age: std::time::Duration) {
        let now = Instant::now();
        self.entries
            .retain(|_, entry| now.duration_since(entry.last_seen) < max_age);
    }

    /// Get an iterator over all DIDs in the clock (for backward compatibility)
    pub fn iter(&self) -> impl Iterator<Item = (&Did, u64)> {
        self.entries.iter().map(|(did, entry)| (did, entry.count))
    }

    /// Get an iterator over all DID keys in the clock
    pub fn keys(&self) -> impl Iterator<Item = &Did> {
        self.entries.keys()
    }

    /// Get an iterator over all count values in the clock
    pub fn values(&self) -> impl Iterator<Item = u64> + '_ {
        self.entries.values().map(|e| e.count)
    }

    /// Check if a DID is present in the clock
    pub fn contains_key(&self, node: &Did) -> bool {
        self.entries.contains_key(node)
    }

    /// Insert a count directly for a node (for deserialization/reconstruction)
    pub fn insert(&mut self, node: Did, count: u64) {
        self.maybe_evict();
        self.entries.insert(node, ClockEntry::new(count));
    }

    /// Record an observed count for a node, keeping whichever is greater.
    ///
    /// This is [`VectorClock::merge`]'s rule applied to a single entry, for
    /// callers that rebuild a clock from a spelling-keyed source. Since I7
    /// (#2627) two accepted spellings of one principal are one key here, so a
    /// rebuild that used [`VectorClock::insert`] would let the last spelling it
    /// happened to visit overwrite the first — and where the source is a
    /// `HashMap<String, _>` that order is unspecified, so the survivor could be
    /// the *lower* count and the clock would move backwards.
    ///
    /// Prefer this over `insert` whenever more than one source entry can name
    /// the same principal. `insert` keeps its overwrite semantics for callers
    /// that genuinely mean "set this node to exactly this count".
    pub fn observe(&mut self, node: Did, count: u64) {
        self.maybe_evict();
        let entry = self
            .entries
            .entry(node)
            .or_insert_with(|| ClockEntry::new(0));
        entry.count = entry.count.max(count);
        entry.touch();
    }

    /// Clear all entries from the clock
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for VectorClock {
    fn eq(&self, other: &Self) -> bool {
        // Compare only the counts, not the last_seen times
        if self.entries.len() != other.entries.len() {
            return false;
        }
        for (did, entry) in &self.entries {
            match other.entries.get(did) {
                Some(other_entry) if entry.count == other_entry.count => continue,
                _ => return false,
            }
        }
        true
    }
}

impl Eq for VectorClock {}

impl Serialize for VectorClock {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize only the counts, not the last_seen times
        let clock: HashMap<Did, u64> = self
            .entries
            .iter()
            .map(|(did, entry)| (did.clone(), entry.count))
            .collect();
        SerializedClock { clock }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VectorClock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let serialized = SerializedClock::deserialize(deserializer)?;
        let entries = serialized
            .clock
            .into_iter()
            .map(|(did, count)| (did, ClockEntry::new(count)))
            .collect();
        Ok(VectorClock { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    // -----------------------------------------------------------------------
    // I7 (#2627): a clock whose entries alias one principal must merge by max.
    // -----------------------------------------------------------------------

    /// A second accepted spelling of the same principal.
    ///
    /// `did:icn:` identifiers are multibase, so the same 32 bytes have a
    /// base58btc spelling and a base16 spelling (`f` + lowercase hex). Both
    /// parse; both decode to one identifier; under I7 both are one `Did` key.
    fn alternate_spelling(did: &Did) -> Did {
        let bytes = did
            .identifier_bytes()
            .expect("a validated DID decodes to 32 identifier bytes");
        let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes)))
            .expect("a base16 multibase spelling of 32 bytes is a valid did:icn:");
        assert_ne!(
            did.as_str(),
            alias.as_str(),
            "CONTROL: the two spellings must differ, or the test proves nothing"
        );
        assert_eq!(
            alias.identifier_bytes().expect("alias decodes"),
            bytes,
            "CONTROL: the alias must name the same principal"
        );
        alias
    }

    /// Serialize `(spelling, count)` pairs as a map, in exactly the given order.
    ///
    /// A `VectorClock` cannot produce an aliased encoding itself — post-I7 its
    /// own `HashMap` has already merged the spellings — so the adversarial input
    /// has to be built at the wire level. This writes the same postcard map
    /// shape `SerializedClock` reads, with the entry order under test control,
    /// which is the whole point: the surviving count must not depend on it.
    struct OrderedClock<'a>(&'a [(String, u64)]);

    impl serde::Serialize for OrderedClock<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(self.0.len()))?;
            for (spelling, count) in self.0 {
                map.serialize_entry(spelling, count)?;
            }
            map.end()
        }
    }

    #[derive(serde::Serialize)]
    struct WireClock<'a> {
        clock: OrderedClock<'a>,
    }

    fn decode_clock(entries: &[(String, u64)]) -> VectorClock {
        let bytes = icn_encoding::encode(&WireClock {
            clock: OrderedClock(entries),
        })
        .expect("encoding a clock map is infallible");
        icn_encoding::decode(&bytes).expect("a well-formed clock map decodes")
    }

    #[test]
    fn an_aliased_clock_merges_to_the_maximum_count_in_either_order() {
        let node = KeyPair::generate().unwrap().did().clone();
        let alias = alternate_spelling(&node);

        // The high count written first, so plain last-writer-wins insertion
        // would keep 5 and move the counter *backwards*.
        let high_first = decode_clock(&[
            (node.as_str().to_string(), 9),
            (alias.as_str().to_string(), 5),
        ]);
        assert_eq!(
            high_first.get(&node),
            9,
            "two spellings of one principal must merge to the maximum, not to \
             whichever entry happened to deserialize last"
        );

        // And the other order, where last-writer-wins would coincidentally be
        // right — so a green result here is not order luck.
        let low_first = decode_clock(&[
            (alias.as_str().to_string(), 5),
            (node.as_str().to_string(), 9),
        ]);
        assert_eq!(
            low_first.get(&node),
            9,
            "the merge must not depend on order"
        );

        assert_eq!(
            high_first, low_first,
            "the same aliased clock in two input orders is the same clock"
        );

        // One principal, so one entry — not two.
        assert_eq!(high_first.len(), 1, "aliases are one entry, not two");

        // Looking it up under the other spelling finds the same count.
        assert_eq!(
            high_first.get(&alias),
            9,
            "either spelling must read the principal's count"
        );
    }

    #[test]
    fn observe_keeps_the_high_water_mark_whatever_order_spellings_arrive_in() {
        // The snapshot restore path (`GossipActor::restore_state`) rebuilds this
        // clock from a `HashMap<String, u64>` whose iteration order is
        // unspecified. Since I7 two spellings of one principal are one key, so
        // the rebuild must not let arrival order decide the count.
        let node = KeyPair::generate().unwrap().did().clone();
        let alias = alternate_spelling(&node);

        // High count first: a plain `insert` would leave 5 behind.
        let mut high_first = VectorClock::new();
        high_first.observe(node.clone(), 9);
        high_first.observe(alias.clone(), 5);
        assert_eq!(
            high_first.get(&node),
            9,
            "a lower count arriving later must not lower the clock"
        );

        let mut low_first = VectorClock::new();
        low_first.observe(alias.clone(), 5);
        low_first.observe(node.clone(), 9);
        assert_eq!(
            low_first.get(&node),
            9,
            "and the result is order-independent"
        );

        assert_eq!(high_first, low_first);
        assert_eq!(high_first.len(), 1, "one principal is one entry");

        // Contrast with `insert`, which deliberately keeps overwrite semantics
        // for callers that mean "set this node to exactly this count".
        let mut overwritten = VectorClock::new();
        overwritten.insert(node.clone(), 9);
        overwritten.insert(alias, 5);
        assert_eq!(
            overwritten.get(&node),
            5,
            "control: `insert` still overwrites, which is why the restore path \
             must not use it"
        );
    }

    #[test]
    fn merging_by_max_does_not_disturb_a_clock_without_aliases() {
        // The fold must be an identity on ordinary input: distinct principals
        // keep their own counts, and nothing is merged that should not be.
        let a = KeyPair::generate().unwrap().did().clone();
        let b = KeyPair::generate().unwrap().did().clone();
        assert_ne!(
            a.identifier_bytes().unwrap(),
            b.identifier_bytes().unwrap(),
            "CONTROL: two generated keypairs must differ"
        );

        let decoded = decode_clock(&[(a.as_str().to_string(), 3), (b.as_str().to_string(), 7)]);

        assert_eq!(decoded.len(), 2, "two principals stay two entries");
        assert_eq!(decoded.get(&a), 3);
        assert_eq!(decoded.get(&b), 7);
    }

    #[test]
    fn a_round_tripped_clock_is_unchanged_and_the_wire_shape_is_a_plain_map() {
        // Serialization is untouched by the merge: a clock encodes as the same
        // map it always did, and decoding it returns the same clock. This is the
        // "no wire byte changes" half of I7 for this type.
        let a = KeyPair::generate().unwrap().did().clone();
        let b = KeyPair::generate().unwrap().did().clone();

        let mut clock = VectorClock::new();
        clock.increment(&a);
        clock.increment(&a);
        clock.increment(&b);

        let bytes = icn_encoding::encode(&clock).expect("encode");
        let back: VectorClock = icn_encoding::decode(&bytes).expect("decode");
        assert_eq!(back, clock, "a round trip must not change the clock");

        // The encoding is byte-identical to the equivalent plain map, so the
        // custom decoder reads exactly what the derive wrote.
        let equivalent = icn_encoding::encode(&WireClock {
            clock: OrderedClock(&[(a.as_str().to_string(), 2), (b.as_str().to_string(), 1)]),
        })
        .expect("encode");
        let from_map: VectorClock = icn_encoding::decode(&equivalent).expect("decode");
        assert_eq!(
            from_map, clock,
            "the wire shape is a plain DID -> count map, unchanged by I7"
        );
    }

    #[test]
    fn test_new_clock() {
        let clock = VectorClock::new();
        assert!(clock.is_empty());
    }

    #[test]
    fn test_increment() {
        let mut clock = VectorClock::new();
        let node = KeyPair::generate().unwrap().did().clone();

        clock.increment(&node);
        assert_eq!(clock.get(&node), 1);

        clock.increment(&node);
        assert_eq!(clock.get(&node), 2);
    }

    #[test]
    fn test_merge() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);
        clock1.increment(&node_a);
        clock1.increment(&node_b);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_a);
        clock2.increment(&node_b);
        clock2.increment(&node_b);

        clock1.merge(&clock2);

        assert_eq!(clock1.get(&node_a), 2); // max(2, 1)
        assert_eq!(clock1.get(&node_b), 2); // max(1, 2)
    }

    #[test]
    fn test_happened_before() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_a);
        clock2.increment(&node_b);

        assert!(clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
    }

    #[test]
    fn test_concurrent() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_b);

        assert!(clock1.is_concurrent(&clock2));
        assert!(clock2.is_concurrent(&clock1));
    }

    #[test]
    fn test_equal_clocks() {
        let node = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node);

        assert!(!clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
        assert!(!clock1.is_concurrent(&clock2));
        assert_eq!(clock1, clock2);
    }

    #[test]
    fn test_partial_cmp() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_a);
        clock2.increment(&node_b);

        assert_eq!(clock1.partial_cmp(&clock2), Some(Ordering::Less));
        assert_eq!(clock2.partial_cmp(&clock1), Some(Ordering::Greater));
        assert_eq!(clock1.partial_cmp(&clock1), Some(Ordering::Equal));

        let mut clock3 = VectorClock::new();
        clock3.increment(&node_b);

        assert_eq!(clock1.partial_cmp(&clock3), None); // Concurrent
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut clock = VectorClock::new();
        assert!(clock.is_empty());
        assert_eq!(clock.len(), 0);

        let node = KeyPair::generate().unwrap().did().clone();
        clock.increment(&node);

        assert!(!clock.is_empty());
        assert_eq!(clock.len(), 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock = VectorClock::new();
        clock.increment(&node_a);
        clock.increment(&node_a);
        clock.increment(&node_b);

        // Serialize
        let bytes = icn_encoding::encode(&clock).unwrap();

        // Deserialize back
        let restored: VectorClock = icn_encoding::decode(&bytes).unwrap();

        // Verify equality (compares counts, not last_seen)
        assert_eq!(clock, restored);
        assert_eq!(restored.get(&node_a), 2);
        assert_eq!(restored.get(&node_b), 1);
    }

    #[test]
    fn test_prune_stale() {
        let mut clock = VectorClock::new();
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        clock.increment(&node_a);
        clock.increment(&node_b);
        assert_eq!(clock.len(), 2);

        // Prune with very short duration - all should be kept (just created)
        clock.prune_stale(std::time::Duration::from_secs(1));
        assert_eq!(clock.len(), 2);

        // Wait a tiny bit and prune with zero duration - all should be removed
        std::thread::sleep(std::time::Duration::from_millis(1));
        clock.prune_stale(std::time::Duration::ZERO);
        assert_eq!(clock.len(), 0);
    }

    #[test]
    fn test_lru_eviction_triggered_at_threshold() {
        let mut clock = VectorClock::new();

        // Add entries up to just below the cleanup threshold
        for _ in 0..CLEANUP_THRESHOLD {
            let node = KeyPair::generate().unwrap().did().clone();
            clock.increment(&node);

            // Should not have evicted yet
            assert!(clock.len() <= CLEANUP_THRESHOLD);
        }

        // Verify we have the expected number of entries
        assert_eq!(clock.len(), CLEANUP_THRESHOLD);

        // Adding one more should trigger eviction
        let extra_node = KeyPair::generate().unwrap().did().clone();
        clock.increment(&extra_node);

        // After eviction, we should be below threshold
        assert!(clock.len() < CLEANUP_THRESHOLD);
        // The new node should still be present (it was just added)
        assert!(clock.get(&extra_node) > 0);
    }

    #[test]
    fn test_iter() {
        let mut clock = VectorClock::new();
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        clock.increment(&node_a);
        clock.increment(&node_a);
        clock.increment(&node_b);

        let entries: HashMap<Did, u64> = clock
            .iter()
            .map(|(did, count)| (did.clone(), count))
            .collect();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries.get(&node_a), Some(&2));
        assert_eq!(entries.get(&node_b), Some(&1));
    }
}
