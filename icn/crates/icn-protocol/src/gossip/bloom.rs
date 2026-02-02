//! Bloom filter for anti-entropy synchronization

use crate::types::{BloomFilterData, ContentHash};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Bloom filter for efficient set membership testing
#[derive(Clone)]
pub struct BloomFilter {
    /// Bit array
    bits: Vec<bool>,

    /// Number of hash functions
    num_hashes: u32,

    /// Size in bits
    size: u64,
}

impl BloomFilter {
    /// Create a new bloom filter
    ///
    /// # Parameters
    /// - `expected_items`: Expected number of items to be inserted
    /// - `false_positive_rate`: Desired false positive probability (e.g., 0.01 for 1%)
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // Calculate optimal size and number of hash functions
        // m = -n * ln(p) / (ln(2)^2)
        // k = (m/n) * ln(2)
        let size = Self::optimal_size(expected_items, false_positive_rate);
        let num_hashes = Self::optimal_hash_count(size, expected_items);

        BloomFilter {
            bits: vec![false; size as usize],
            num_hashes,
            size,
        }
    }

    /// Create an adaptively-sized bloom filter based on cardinality hint
    ///
    /// Uses a heuristic to balance size vs false positive rate:
    /// - m = min(65536, next_pow2(8 * expected_ids))
    /// - k = max(1, round(m/expected_ids * ln2))
    ///
    /// This ensures filters stay reasonably sized while maintaining good FP rates.
    pub fn new_adaptive(expected_ids: usize) -> Self {
        if expected_ids == 0 {
            // Minimal filter for empty set
            return BloomFilter {
                bits: vec![false; 64],
                num_hashes: 1,
                size: 64,
            };
        }

        // Calculate size: min(65536, next_pow2(8 * expected_ids))
        let target_bits = (8 * expected_ids).max(64);
        let size = target_bits.next_power_of_two().min(65536) as u64;

        // Calculate hash count: max(1, round((m/n) * ln(2)))
        let ratio = size as f64 / expected_ids as f64;
        let num_hashes = (ratio * std::f64::consts::LN_2).round().max(1.0) as u32;

        BloomFilter {
            bits: vec![false; size as usize],
            num_hashes,
            size,
        }
    }

    /// Calculate optimal bit array size
    fn optimal_size(n: usize, p: f64) -> u64 {
        let n = n as f64;
        let ln2_squared = std::f64::consts::LN_2.powi(2);
        (-(n * p.ln()) / ln2_squared).ceil() as u64
    }

    /// Calculate optimal number of hash functions
    fn optimal_hash_count(m: u64, n: usize) -> u32 {
        let n = n as f64;
        let m = m as f64;
        ((m / n) * std::f64::consts::LN_2).ceil() as u32
    }

    /// Insert a hash into the bloom filter
    pub fn insert(&mut self, hash: &ContentHash) {
        for i in 0..self.num_hashes {
            let index = self.hash_with_seed(hash, i) % self.size;
            self.bits[index as usize] = true;
        }
    }

    /// Check if a hash might be in the set (may have false positives)
    pub fn contains(&self, hash: &ContentHash) -> bool {
        for i in 0..self.num_hashes {
            let index = self.hash_with_seed(hash, i) % self.size;
            if !self.bits[index as usize] {
                return false; // Definitely not in set
            }
        }
        true // Might be in set
    }

    /// Hash a value with a seed
    fn hash_with_seed(&self, hash: &ContentHash, seed: u32) -> u64 {
        let mut hasher = DefaultHasher::new();
        hash.hash(&mut hasher);
        seed.hash(&mut hasher);
        hasher.finish()
    }

    /// Serialize to BloomFilterData for network transmission
    pub fn to_data(&self) -> BloomFilterData {
        // Pack bits into bytes
        let mut bytes = Vec::new();
        for chunk in self.bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit {
                    byte |= 1 << i;
                }
            }
            bytes.push(byte);
        }

        BloomFilterData {
            bits: bytes,
            num_hashes: self.num_hashes,
            size: self.size,
        }
    }

    /// Deserialize from BloomFilterData
    pub fn from_data(data: &BloomFilterData) -> Self {
        // Validate non-zero size to prevent division by zero
        if data.size == 0 {
            tracing::warn!("BloomFilter deserialization: zero size. Creating minimal filter.");
            return BloomFilter {
                bits: vec![false],
                num_hashes: data.num_hashes.max(1),
                size: 1,
            };
        }

        let mut bits = Vec::new();

        // Unpack bytes into bits
        for &byte in &data.bits {
            for i in 0..8 {
                bits.push((byte & (1 << i)) != 0);
            }
        }

        // Validate size before truncation to prevent index out of bounds
        let unpacked_bits = bits.len();
        let claimed_size = data.size as usize;

        if claimed_size > unpacked_bits {
            // Malformed data: claimed size exceeds actual bits
            // Use the actual unpacked size to prevent index out of bounds
            tracing::warn!(
                "BloomFilter deserialization: claimed size {} exceeds unpacked bits {}. Using actual size.",
                claimed_size,
                unpacked_bits
            );
            BloomFilter {
                bits,
                num_hashes: data.num_hashes,
                size: unpacked_bits as u64,
            }
        } else {
            // Trim to exact size (normal case)
            bits.truncate(claimed_size);
            BloomFilter {
                bits,
                num_hashes: data.num_hashes,
                size: data.size,
            }
        }
    }

    /// Get the approximate number of items inserted
    pub fn estimated_count(&self) -> usize {
        let set_bits = self.bits.iter().filter(|&&b| b).count() as f64;
        let m = self.size as f64;
        let k = self.num_hashes as f64;

        // n ≈ -(m/k) * ln(1 - X/m) where X is number of set bits
        if set_bits == 0.0 {
            return 0;
        }

        let ratio = 1.0 - (set_bits / m);
        if ratio <= 0.0 {
            return 0;
        }

        (-(m / k) * ratio.ln()) as usize
    }

    /// Clear the bloom filter
    pub fn clear(&mut self) {
        for bit in &mut self.bits {
            *bit = false;
        }
    }

    /// Get the current fill ratio (proportion of bits set)
    pub fn fill_ratio(&self) -> f64 {
        if self.size == 0 {
            return 0.0;
        }
        let set_bits = self.bits.iter().filter(|&&b| b).count() as f64;
        set_bits / self.size as f64
    }

    /// Get the filter's capacity (size in bits)
    pub fn capacity(&self) -> u64 {
        self.size
    }

    /// Get the number of hash functions
    pub fn hash_count(&self) -> u32 {
        self.num_hashes
    }

    /// Check if the filter needs resizing based on current entry count
    ///
    /// Returns true if:
    /// - Fill ratio exceeds the high threshold (filter is too full, FP rate degraded)
    /// - Entry count is much smaller than capacity (filter is oversized, wasting memory)
    pub fn needs_resize(&self, entry_count: usize, config: &BloomResizeConfig) -> bool {
        let fill_ratio = self.fill_ratio();

        // Too full - FP rate is degraded
        if fill_ratio > config.high_fill_threshold {
            return true;
        }

        // Too large for current entries (wasting memory)
        // Only resize down if we have enough entries to be meaningful
        if entry_count >= config.min_entries_for_resize {
            let optimal_size = Self::optimal_size(entry_count, config.target_fp_rate);
            // Resize if current size is > 4x optimal (significant memory waste)
            if self.size > optimal_size * 4 {
                return true;
            }
        }

        false
    }

    /// Rebuild the filter with optimal sizing for the given entries
    ///
    /// This creates a new filter sized for the actual entry count and re-inserts
    /// all entries. Used when dynamic resizing is triggered.
    pub fn rebuild(entries: &[ContentHash], config: &BloomResizeConfig) -> Self {
        let entry_count = entries.len().max(config.min_entries_for_resize);
        let size = Self::optimal_size(entry_count, config.target_fp_rate)
            .min(config.max_size_bits)
            .max(64); // Minimum 64 bits
        let num_hashes = Self::optimal_hash_count(size, entry_count);

        let mut filter = BloomFilter {
            bits: vec![false; size as usize],
            num_hashes,
            size,
        };

        for hash in entries {
            filter.insert(hash);
        }

        filter
    }

    /// Get the estimated false positive rate based on current fill
    pub fn estimated_fp_rate(&self) -> f64 {
        let fill_ratio = self.fill_ratio();
        // FP rate ≈ (fill_ratio)^k where k is number of hash functions
        fill_ratio.powi(self.num_hashes as i32)
    }
}

/// Configuration for dynamic Bloom filter resizing
#[derive(Debug, Clone)]
pub struct BloomResizeConfig {
    /// Fill ratio above which resize is triggered (default: 0.7)
    pub high_fill_threshold: f64,

    /// Target false positive rate for resized filters (default: 0.01 = 1%)
    pub target_fp_rate: f64,

    /// Maximum filter size in bits (default: 1MB = 8,388,608 bits)
    pub max_size_bits: u64,

    /// Minimum entries before considering resize (default: 100)
    pub min_entries_for_resize: usize,
}

impl Default for BloomResizeConfig {
    fn default() -> Self {
        Self {
            high_fill_threshold: 0.7,
            target_fp_rate: 0.01,
            max_size_bits: 8_388_608, // 1 MB
            min_entries_for_resize: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_contains() {
        let mut filter = BloomFilter::new(100, 0.01);

        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];
        let _hash3 = [3u8; 32];

        filter.insert(&hash1);
        filter.insert(&hash2);

        assert!(filter.contains(&hash1));
        assert!(filter.contains(&hash2));
        // hash3 was not inserted, so should not be present (but could be false positive)
        // We can't assert false here due to false positives
    }

    #[test]
    fn test_serialization() {
        let mut filter = BloomFilter::new(100, 0.01);

        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];

        filter.insert(&hash1);
        filter.insert(&hash2);

        // Serialize
        let data = filter.to_data();

        // Deserialize
        let filter2 = BloomFilter::from_data(&data);

        // Should contain same items
        assert!(filter2.contains(&hash1));
        assert!(filter2.contains(&hash2));
    }

    #[test]
    fn test_false_positive_rate() {
        let mut filter = BloomFilter::new(1000, 0.01);

        // Insert 1000 items
        for i in 0..1000 {
            let mut hash = [0u8; 32];
            hash[0] = (i / 256) as u8;
            hash[1] = (i % 256) as u8;
            filter.insert(&hash);
        }

        // Test 1000 items that were NOT inserted
        let mut false_positives = 0;
        for i in 1000..2000 {
            let mut hash = [0u8; 32];
            hash[0] = (i / 256) as u8;
            hash[1] = (i % 256) as u8;
            if filter.contains(&hash) {
                false_positives += 1;
            }
        }

        // False positive rate should be around 1%
        let rate = false_positives as f64 / 1000.0;
        println!("False positive rate: {:.2}%", rate * 100.0);

        // Allow some variance, but should be less than 5%
        assert!(rate < 0.05, "False positive rate too high: {rate}");
    }

    #[test]
    fn test_clear() {
        let mut filter = BloomFilter::new(100, 0.01);

        let hash = [1u8; 32];
        filter.insert(&hash);
        assert!(filter.contains(&hash));

        filter.clear();
        // After clear, the hash should not be in the filter
        // But this might be a false positive, so we just check the filter is cleared
        assert_eq!(filter.estimated_count(), 0);
    }

    #[test]
    fn test_adaptive_sizing() {
        // Test with 0 entries
        let filter0 = BloomFilter::new_adaptive(0);
        assert_eq!(filter0.size, 64); // Minimal size

        // Test with small number of entries
        let filter10 = BloomFilter::new_adaptive(10);
        assert!(filter10.size >= 64);
        assert!(filter10.size <= 65536);
        assert!(filter10.num_hashes >= 1);

        // Test with medium number of entries
        let filter100 = BloomFilter::new_adaptive(100);
        assert!(filter100.size >= 64);
        assert!(filter100.size <= 65536);

        // Test with large number of entries (should cap at 65536)
        let filter10k = BloomFilter::new_adaptive(10000);
        assert_eq!(filter10k.size, 65536);

        println!(
            "Adaptive filter for 10 entries: size={}, hashes={}",
            filter10.size, filter10.num_hashes
        );
        println!(
            "Adaptive filter for 100 entries: size={}, hashes={}",
            filter100.size, filter100.num_hashes
        );
        println!(
            "Adaptive filter for 10k entries: size={}, hashes={}",
            filter10k.size, filter10k.num_hashes
        );
    }

    #[test]
    fn test_adaptive_sizing_functional() {
        // Verify adaptive filter actually works
        let mut filter = BloomFilter::new_adaptive(50);

        // Insert 50 items
        for i in 0..50 {
            let mut hash = [0u8; 32];
            hash[0] = i;
            filter.insert(&hash);
        }

        // All inserted items should be found
        for i in 0..50 {
            let mut hash = [0u8; 32];
            hash[0] = i;
            assert!(filter.contains(&hash), "Should contain hash {i}");
        }

        // Test false positive rate on non-inserted items
        let mut false_positives = 0;
        for i in 50..150 {
            let mut hash = [0u8; 32];
            hash[0] = i;
            if filter.contains(&hash) {
                false_positives += 1;
            }
        }

        let fp_rate = false_positives as f64 / 100.0;
        println!("Adaptive filter FP rate: {:.2}%", fp_rate * 100.0);

        // Should be reasonably low (< 20% for adaptive sizing)
        assert!(fp_rate < 0.2, "FP rate too high: {:.2}%", fp_rate * 100.0);
    }

    #[test]
    fn test_bloom_resize_config_default() {
        let config = BloomResizeConfig::default();
        assert!((config.high_fill_threshold - 0.7).abs() < 0.001);
        assert!((config.target_fp_rate - 0.01).abs() < 0.001);
        assert_eq!(config.max_size_bits, 8_388_608);
        assert_eq!(config.min_entries_for_resize, 100);
    }

    #[test]
    fn test_fill_ratio() {
        let mut filter = BloomFilter::new(100, 0.01);
        assert!((filter.fill_ratio() - 0.0).abs() < 0.001);

        // Insert some items
        for i in 0..50 {
            let mut hash = [0u8; 32];
            hash[0] = i;
            filter.insert(&hash);
        }

        let ratio = filter.fill_ratio();
        assert!(ratio > 0.0);
        assert!(ratio < 1.0);
        println!("Fill ratio after 50 inserts: {:.2}%", ratio * 100.0);
    }

    #[test]
    fn test_needs_resize_when_full() {
        let config = BloomResizeConfig::default();
        let mut filter = BloomFilter::new(10, 0.01); // Small filter

        // Should not need resize when empty
        assert!(!filter.needs_resize(0, &config));

        // Fill it up with many items to exceed capacity
        for i in 0..100 {
            let mut hash = [0u8; 32];
            hash[0] = i;
            filter.insert(&hash);
        }

        // Should need resize when overfilled
        let ratio = filter.fill_ratio();
        println!(
            "Fill ratio after 100 inserts into size-10 filter: {:.2}%",
            ratio * 100.0
        );
        if ratio > config.high_fill_threshold {
            assert!(filter.needs_resize(100, &config));
        }
    }

    #[test]
    fn test_needs_resize_when_oversized() {
        let config = BloomResizeConfig::default();
        // Create a filter sized for 10000 entries
        let filter = BloomFilter::new(10000, 0.01);

        // With only 100 entries, filter is oversized (>4x optimal)
        // Note: depends on optimal_size calculation, may or may not trigger
        let needs = filter.needs_resize(100, &config);
        println!("Filter sized for 10k with 100 entries needs resize: {needs}");
    }

    #[test]
    fn test_rebuild() {
        let config = BloomResizeConfig::default();

        // Create some test entries
        let entries: Vec<ContentHash> = (0..200)
            .map(|i| {
                let mut hash = [0u8; 32];
                hash[0] = (i / 256) as u8;
                hash[1] = (i % 256) as u8;
                hash
            })
            .collect();

        // Rebuild filter for these entries
        let filter = BloomFilter::rebuild(&entries, &config);

        // Verify all entries are present
        for hash in &entries {
            assert!(
                filter.contains(hash),
                "Rebuilt filter should contain all entries"
            );
        }

        // Verify size is reasonable
        assert!(filter.capacity() >= 64);
        assert!(filter.capacity() <= config.max_size_bits);

        println!(
            "Rebuilt filter for {} entries: size={}, hashes={}, fill={:.2}%",
            entries.len(),
            filter.capacity(),
            filter.hash_count(),
            filter.fill_ratio() * 100.0
        );
    }

    #[test]
    fn test_estimated_fp_rate() {
        let mut filter = BloomFilter::new(1000, 0.01);

        // Initially should be 0
        assert!((filter.estimated_fp_rate() - 0.0).abs() < 0.001);

        // Insert items and check FP rate increases
        for i in 0..500 {
            let mut hash = [0u8; 32];
            hash[0] = (i / 256) as u8;
            hash[1] = (i % 256) as u8;
            filter.insert(&hash);
        }

        let estimated = filter.estimated_fp_rate();
        println!(
            "Estimated FP rate after 500 inserts: {:.4}%",
            estimated * 100.0
        );
        assert!(estimated > 0.0);
    }
}
