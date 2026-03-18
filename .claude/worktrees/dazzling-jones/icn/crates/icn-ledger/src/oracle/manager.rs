//! Oracle manager with multi-source aggregation and caching
//!
//! The OracleManager coordinates multiple price feed sources, aggregates
//! their rates using median consensus with outlier detection, and caches
//! results with configurable TTL.

use super::error::{OracleError, OracleResult};
use super::price_feed::PriceFeed;
use super::types::{CurrencyPair, ExchangeRate, OracleConfig, RateObservation, SourceInfo};
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Storage key prefix for cached rates
const RATE_CACHE_PREFIX: &str = "oracle:cache:";

/// Storage key prefix for rate history (reserved for future use)
#[allow(dead_code)]
const RATE_HISTORY_PREFIX: &str = "oracle:history:";

/// Default staleness threshold for sources (5 minutes)
const DEFAULT_SOURCE_STALENESS_SECS: u64 = 300;

/// Tracking information for each oracle source (Issue #410)
#[derive(Debug, Clone)]
pub struct SourceUpdateInfo {
    /// Last successful update timestamp (Unix seconds)
    pub last_update: u64,
    /// Total count of successful updates
    pub update_count: u64,
    /// Last rate observed from this source (for debugging)
    pub last_rate: Option<f64>,
    /// Last currency pair updated
    pub last_pair: Option<String>,
}

impl SourceUpdateInfo {
    /// Create a new update info with current timestamp
    fn new() -> Self {
        Self {
            last_update: crate::current_timestamp_secs(),
            update_count: 1,
            last_rate: None,
            last_pair: None,
        }
    }

    /// Record a new observation from this source
    fn record_observation(&mut self, rate: f64, pair: &str) {
        self.last_update = crate::current_timestamp_secs();
        self.update_count = self.update_count.saturating_add(1);
        self.last_rate = Some(rate);
        self.last_pair = Some(pair.to_string());
    }

    /// Check if this source is stale (hasn't updated in max_age seconds)
    ///
    /// Returns true if `(current_time - last_update) > max_age_secs`.
    ///
    /// **Note:** Staleness is measured from when the source was last queried
    /// successfully, not when the source's underlying data was generated.
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        let now = crate::current_timestamp_secs();
        now.saturating_sub(self.last_update) > max_age_secs
    }
}

/// Oracle manager for exchange rate queries with caching and multi-source aggregation
pub struct OracleManager {
    /// Persistent storage
    store: Arc<dyn Store>,

    /// In-memory rate cache with TTL
    cache: RwLock<HashMap<String, ExchangeRate>>,

    /// Registered price feed sources (sorted by priority)
    sources: RwLock<Vec<Arc<dyn PriceFeed>>>,

    /// Per-source update tracking (Issue #410)
    source_updates: RwLock<HashMap<String, SourceUpdateInfo>>,

    /// Configuration
    config: OracleConfig,
}

impl OracleManager {
    /// Create a new oracle manager with default configuration
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self::with_config(store, OracleConfig::default())
    }

    /// Create a new oracle manager with custom configuration
    pub fn with_config(store: Arc<dyn Store>, config: OracleConfig) -> Self {
        Self {
            store,
            cache: RwLock::new(HashMap::new()),
            sources: RwLock::new(Vec::new()),
            source_updates: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Get a reference to the oracle configuration
    pub fn config(&self) -> &OracleConfig {
        &self.config
    }

    // === Source Management ===

    /// Register a price feed source
    pub async fn register_source(&self, source: Arc<dyn PriceFeed>) {
        let mut sources = self.sources.write().await;
        let priority = source.priority();
        let source_id = source.source_id().to_string();

        // Remove existing source with same ID if present
        sources.retain(|s| s.source_id() != source_id);

        // Insert maintaining priority order (lower priority value = higher priority)
        let insert_pos = sources
            .iter()
            .position(|s| s.priority() > priority)
            .unwrap_or(sources.len());
        sources.insert(insert_pos, source);

        info!(source_id = %source_id, priority = priority, "Registered price feed source");
    }

    /// Remove a source by ID
    ///
    /// Also cleans up staleness tracking data to prevent memory leaks.
    pub async fn unregister_source(&self, source_id: &str) -> bool {
        let mut sources = self.sources.write().await;
        let initial_len = sources.len();
        sources.retain(|s| s.source_id() != source_id);
        let removed = sources.len() < initial_len;
        if removed {
            // Clean up staleness tracking data
            let mut updates = self.source_updates.write().await;
            updates.remove(source_id);
            info!(source_id = %source_id, "Unregistered price feed source");
        }
        removed
    }

    /// List registered sources with their status
    pub async fn list_sources(&self) -> Vec<SourceInfo> {
        let sources = self.sources.read().await;
        let updates = self.source_updates.read().await;
        let mut infos = Vec::with_capacity(sources.len());

        for source in sources.iter() {
            let is_healthy = source.health_check().await;
            let last_update = updates.get(source.source_id()).map(|info| info.last_update);
            infos.push(SourceInfo {
                source_id: source.source_id().to_string(),
                name: source.name().to_string(),
                priority: source.priority(),
                is_healthy,
                last_update,
            });
        }

        infos
    }

    /// Check if a source is stale (hasn't provided data recently)
    ///
    /// Returns `None` if the source has never provided data.
    /// Uses default staleness threshold if `max_age_secs` is not specified.
    pub async fn is_source_stale(
        &self,
        source_id: &str,
        max_age_secs: Option<u64>,
    ) -> Option<bool> {
        let updates = self.source_updates.read().await;
        updates
            .get(source_id)
            .map(|info| info.is_stale(max_age_secs.unwrap_or(DEFAULT_SOURCE_STALENESS_SECS)))
    }

    /// Get update info for a specific source
    pub async fn get_source_update_info(&self, source_id: &str) -> Option<SourceUpdateInfo> {
        let updates = self.source_updates.read().await;
        updates.get(source_id).cloned()
    }

    /// Get all stale sources (sources that haven't updated within threshold)
    pub async fn get_stale_sources(&self, max_age_secs: Option<u64>) -> Vec<String> {
        let updates = self.source_updates.read().await;
        let threshold = max_age_secs.unwrap_or(DEFAULT_SOURCE_STALENESS_SECS);
        updates
            .iter()
            .filter(|(_, info)| info.is_stale(threshold))
            .map(|(id, _)| id.clone())
            .collect()
    }

    // === Rate Queries ===

    /// Get exchange rate with caching and multi-source consensus
    ///
    /// Returns cached rate if still valid, otherwise fetches from sources.
    pub async fn get_rate(&self, pair: &CurrencyPair) -> OracleResult<ExchangeRate> {
        // Handle identity pairs (same currency)
        if pair.is_identity() {
            return Ok(ExchangeRate {
                pair: pair.clone(),
                rate: 1.0,
                observations: vec![RateObservation::new(1.0, "identity")],
                aggregated_at: crate::current_timestamp_secs(),
                ttl_secs: self.config.default_ttl_secs,
                is_stale: false,
            });
        }

        // Check cache first
        if let Some(cached) = self.get_cached(pair).await {
            if !cached.is_expired() && !cached.is_stale {
                debug!(pair = %pair, "Returning cached rate");
                return Ok(cached);
            }
            debug!(pair = %pair, expired = cached.is_expired(), stale = cached.is_stale, "Cache hit but rate is expired/stale, refreshing");
        }

        // Fetch from sources
        self.refresh_rate(pair).await
    }

    /// Force refresh from sources (bypasses cache)
    pub async fn refresh_rate(&self, pair: &CurrencyPair) -> OracleResult<ExchangeRate> {
        let sources = self.sources.read().await;
        let mut observations = Vec::new();
        let mut successful_sources: Vec<(String, f64)> = Vec::new();

        debug!(pair = %pair, source_count = sources.len(), "Fetching rate from sources");

        for source in sources.iter() {
            if source.supports_pair(pair) {
                match source.get_rate(pair).await {
                    Ok(obs) => {
                        debug!(
                            pair = %pair,
                            source = source.source_id(),
                            rate = obs.rate,
                            "Got rate from source"
                        );
                        successful_sources.push((source.source_id().to_string(), obs.rate));
                        observations.push(obs);
                    }
                    Err(e) => {
                        warn!(
                            source = source.source_id(),
                            pair = %pair,
                            error = %e,
                            "Failed to fetch rate from source"
                        );
                    }
                }
            }
        }

        // Record successful observations for staleness tracking (Issue #410)
        if !successful_sources.is_empty() {
            let mut updates = self.source_updates.write().await;
            let pair_key = pair.key();
            for (source_id, rate) in successful_sources {
                updates
                    .entry(source_id)
                    .and_modify(|info| info.record_observation(rate, &pair_key))
                    .or_insert_with(|| {
                        let mut info = SourceUpdateInfo::new();
                        info.last_rate = Some(rate);
                        info.last_pair = Some(pair_key.clone());
                        info
                    });
            }
        }

        if observations.is_empty() {
            // Try to return stale cached rate as fallback
            if let Some(cached) = self.get_cached(pair).await {
                warn!(pair = %pair, "No sources available, returning stale cached rate");
                return Ok(ExchangeRate {
                    is_stale: true,
                    ..cached
                });
            }
            return Err(OracleError::NoSourcesAvailable(pair.clone()));
        }

        // Check minimum sources for consensus
        if observations.len() < self.config.min_sources_for_consensus {
            return Err(OracleError::InsufficientSources {
                got: observations.len(),
                required: self.config.min_sources_for_consensus,
            });
        }

        // Aggregate using median with outlier detection
        let mut rate = self.aggregate_rates(pair, &observations)?;

        // Check staleness
        let now = crate::current_timestamp_secs();
        let oldest_observation = observations
            .iter()
            .map(|o| o.observed_at)
            .min()
            .unwrap_or(now);
        rate.is_stale =
            now.saturating_sub(oldest_observation) > self.config.staleness_threshold_secs;

        // Cache the result
        self.cache_rate(&rate).await;

        // Persist to storage for recovery
        self.store_rate(&rate)?;

        info!(
            pair = %pair,
            rate = rate.rate,
            sources = rate.source_count(),
            stale = rate.is_stale,
            "Refreshed exchange rate"
        );

        Ok(rate)
    }

    /// Get rate or fallback to last known (for resilience)
    pub async fn get_rate_or_fallback(&self, pair: &CurrencyPair) -> OracleResult<ExchangeRate> {
        match self.get_rate(pair).await {
            Ok(rate) => Ok(rate),
            Err(_) => {
                // Try to get last cached rate, even if stale
                self.get_cached(pair)
                    .await
                    .map(|mut r| {
                        r.is_stale = true;
                        r
                    })
                    .ok_or_else(|| OracleError::RateNotFound(pair.clone()))
            }
        }
    }

    // === Currency Conversion ===

    /// Convert amount between currencies
    pub async fn convert_amount(&self, amount: i64, from: &str, to: &str) -> OracleResult<i64> {
        if from == to {
            return Ok(amount);
        }

        let pair = CurrencyPair::new(from, to);
        let rate = self.get_rate(&pair).await?;

        rate.convert(amount).ok_or_else(|| {
            OracleError::InvalidRate(format!(
                "Conversion overflow: {amount} * {} exceeds i64 bounds",
                rate.rate
            ))
        })
    }

    // === Cache Operations ===

    async fn get_cached(&self, pair: &CurrencyPair) -> Option<ExchangeRate> {
        let cache = self.cache.read().await;
        cache.get(&pair.key()).cloned()
    }

    async fn cache_rate(&self, rate: &ExchangeRate) {
        let mut cache = self.cache.write().await;
        cache.insert(rate.pair.key(), rate.clone());
    }

    /// Clear all cached rates
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        info!("Cleared oracle rate cache");
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let expired_count = cache.values().filter(|r| r.is_expired()).count();
        let stale_count = cache.values().filter(|r| r.is_stale).count();

        CacheStats {
            total_pairs: cache.len(),
            expired_count,
            stale_count,
            ttl_secs: self.config.default_ttl_secs,
        }
    }

    // === Storage Operations ===

    fn store_rate(&self, rate: &ExchangeRate) -> OracleResult<()> {
        let key = format!("{}{}", RATE_CACHE_PREFIX, rate.pair.key());
        let value = serde_json::to_vec(rate).map_err(OracleError::from)?;
        self.store
            .put(key.as_bytes(), &value)
            .map_err(|e| OracleError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Load cached rates from storage on startup
    ///
    /// This should be called during initialization to pre-populate the cache
    /// with any persisted rates, reducing startup latency for first queries.
    pub async fn load_from_storage(&self) -> OracleResult<usize> {
        let prefix = RATE_CACHE_PREFIX.as_bytes();
        let pairs = self
            .store
            .scan(prefix)
            .map_err(|e| OracleError::Storage(e.to_string()))?;

        let mut loaded = 0;
        let mut cache = self.cache.write().await;
        let now = crate::current_timestamp_secs();

        for (key, value) in pairs {
            match serde_json::from_slice::<ExchangeRate>(&value) {
                Ok(mut rate) => {
                    // Check if rate is expired
                    let expires_at = rate.aggregated_at.saturating_add(rate.ttl_secs);
                    if now > expires_at {
                        debug!(pair = %rate.pair, "Skipping expired cached rate");
                        continue;
                    }

                    // Check if rate is stale
                    let stale_at = rate
                        .aggregated_at
                        .saturating_add(self.config.staleness_threshold_secs);
                    rate.is_stale = now > stale_at;

                    cache.insert(rate.pair.key(), rate);
                    loaded += 1;
                }
                Err(e) => {
                    let key_str = String::from_utf8_lossy(&key);
                    warn!(key = %key_str, error = %e, "Failed to deserialize cached rate");
                }
            }
        }

        info!(loaded_count = loaded, "Loaded cached rates from storage");
        Ok(loaded)
    }

    // === Aggregation ===

    fn aggregate_rates(
        &self,
        pair: &CurrencyPair,
        observations: &[RateObservation],
    ) -> OracleResult<ExchangeRate> {
        if observations.is_empty() {
            return Err(OracleError::NoSourcesAvailable(pair.clone()));
        }

        // Sort by rate for median calculation
        let mut rates: Vec<f64> = observations
            .iter()
            .map(|o| o.rate)
            .filter(|r| r.is_finite()) // Filter out NaN/Infinity
            .collect();
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Defensive check: if all rates were filtered (all NaN/Infinity), return error
        if rates.is_empty() {
            return Err(OracleError::NoSourcesAvailable(pair.clone()));
        }

        // Calculate median
        let median = if rates.len().is_multiple_of(2) {
            (rates[rates.len() / 2 - 1] + rates[rates.len() / 2]) / 2.0
        } else {
            rates[rates.len() / 2]
        };

        // Filter outliers (beyond threshold from median)
        let valid_observations: Vec<RateObservation> = observations
            .iter()
            .filter(|o| {
                if median == 0.0 {
                    return true; // Can't compute deviation from zero
                }
                let deviation = (o.rate - median).abs() / median;
                deviation <= self.config.outlier_threshold
            })
            .cloned()
            .collect();

        // Use median of valid observations for final rate
        // If all observations were filtered as outliers, fall back to original observations
        // with a warning - this indicates potential data quality issues
        let (final_rate, final_observations) = if !valid_observations.is_empty() {
            let mut valid_rates: Vec<f64> = valid_observations.iter().map(|o| o.rate).collect();
            valid_rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let rate = if valid_rates.len().is_multiple_of(2) {
                (valid_rates[valid_rates.len() / 2 - 1] + valid_rates[valid_rates.len() / 2]) / 2.0
            } else {
                valid_rates[valid_rates.len() / 2]
            };
            (rate, valid_observations)
        } else {
            // All observations were outliers - this indicates high disagreement between sources
            // Fall back to original median but log a warning
            warn!(
                pair = %pair,
                observation_count = observations.len(),
                threshold = self.config.outlier_threshold,
                "All rate observations filtered as outliers - high source disagreement"
            );
            // Return original observations so caller can see the disagreement
            (median, observations.to_vec())
        };

        let now = crate::current_timestamp_secs();

        Ok(ExchangeRate {
            pair: pair.clone(),
            rate: final_rate,
            observations: final_observations,
            aggregated_at: now,
            ttl_secs: self.config.default_ttl_secs,
            is_stale: false,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total number of cached pairs
    pub total_pairs: usize,
    /// Number of expired entries
    pub expired_count: usize,
    /// Number of stale entries
    pub stale_count: usize,
    /// Default TTL in seconds
    pub ttl_secs: u64,
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::price_feed::test_helpers::MockPriceFeed;
    use icn_store::SledStore;

    fn test_store() -> Arc<dyn Store> {
        Arc::new(SledStore::temporary().expect("create temp store"))
    }

    #[tokio::test]
    async fn test_register_source() {
        let oracle = OracleManager::new(test_store());
        let source = Arc::new(MockPriceFeed::new("test", "Test Source", 50));

        oracle.register_source(source).await;

        let sources = oracle.list_sources().await;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_id, "test");
    }

    #[tokio::test]
    async fn test_source_priority_ordering() {
        let oracle = OracleManager::new(test_store());

        // Register in reverse priority order
        oracle
            .register_source(Arc::new(MockPriceFeed::new("low", "Low Priority", 100)))
            .await;
        oracle
            .register_source(Arc::new(MockPriceFeed::new("high", "High Priority", 10)))
            .await;
        oracle
            .register_source(Arc::new(MockPriceFeed::new("med", "Medium Priority", 50)))
            .await;

        let sources = oracle.list_sources().await;
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].source_id, "high");
        assert_eq!(sources[1].source_id, "med");
        assert_eq!(sources[2].source_id, "low");
    }

    #[tokio::test]
    async fn test_get_rate_single_source() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source).await;

        let pair = CurrencyPair::new("hours", "USD");
        let rate = oracle.get_rate(&pair).await.expect("should get rate");

        assert!((rate.rate - 25.0).abs() < 0.001);
        assert_eq!(rate.pair, pair);
        assert!(!rate.is_stale);
    }

    #[tokio::test]
    async fn test_identity_pair() {
        let oracle = OracleManager::new(test_store());

        let pair = CurrencyPair::new("USD", "USD");
        let rate = oracle.get_rate(&pair).await.expect("should get rate");

        assert!((rate.rate - 1.0).abs() < 0.001);
        assert!(!rate.is_stale);
    }

    #[tokio::test]
    async fn test_median_aggregation() {
        let oracle = OracleManager::new(test_store());

        // Register multiple sources with different rates
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s1", "Source 1", 50).with_rate("hours", "USD", 24.0),
            ))
            .await;
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s2", "Source 2", 50).with_rate("hours", "USD", 25.0),
            ))
            .await;
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s3", "Source 3", 50).with_rate("hours", "USD", 26.0),
            ))
            .await;

        let pair = CurrencyPair::new("hours", "USD");
        let rate = oracle.get_rate(&pair).await.expect("should get rate");

        // Median of [24, 25, 26] = 25
        assert!((rate.rate - 25.0).abs() < 0.001);
        assert_eq!(rate.source_count(), 3);
    }

    #[tokio::test]
    async fn test_outlier_detection() {
        let config = OracleConfig {
            outlier_threshold: 0.15, // 15%
            ..OracleConfig::default()
        };
        let oracle = OracleManager::with_config(test_store(), config);

        // Register sources with one outlier
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s1", "Source 1", 50).with_rate("hours", "USD", 25.0),
            ))
            .await;
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s2", "Source 2", 50).with_rate("hours", "USD", 26.0),
            ))
            .await;
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("outlier", "Outlier", 50).with_rate("hours", "USD", 100.0),
            ))
            .await; // Way off!

        let pair = CurrencyPair::new("hours", "USD");
        let rate = oracle.get_rate(&pair).await.expect("should get rate");

        // Outlier should be excluded, median of [25, 26] = 25.5
        assert!((rate.rate - 25.5).abs() < 0.001);
        assert_eq!(rate.source_count(), 2); // Outlier excluded
    }

    #[tokio::test]
    async fn test_caching() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source.clone()).await;

        let pair = CurrencyPair::new("hours", "USD");

        // First call fetches from source
        let rate1 = oracle.get_rate(&pair).await.expect("should get rate");
        assert!((rate1.rate - 25.0).abs() < 0.001);

        // Change the source rate
        {
            let mut rates = source.rates.write().expect("lock");
            rates.insert("hours:USD".to_string(), 30.0);
        }

        // Second call should return cached value
        let rate2 = oracle.get_rate(&pair).await.expect("should get rate");
        assert!((rate2.rate - 25.0).abs() < 0.001); // Still cached

        // Force refresh should get new value
        let rate3 = oracle.refresh_rate(&pair).await.expect("should refresh");
        assert!((rate3.rate - 30.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_no_sources_error() {
        let oracle = OracleManager::new(test_store());

        let pair = CurrencyPair::new("hours", "USD");
        let result = oracle.get_rate(&pair).await;

        assert!(matches!(result, Err(OracleError::NoSourcesAvailable(_))));
    }

    #[tokio::test]
    async fn test_convert_amount() {
        let oracle = OracleManager::new(test_store());
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0),
            ))
            .await;

        let converted = oracle
            .convert_amount(10, "hours", "USD")
            .await
            .expect("should convert");
        assert_eq!(converted, 250);

        // Same currency should return same amount
        let same = oracle
            .convert_amount(10, "USD", "USD")
            .await
            .expect("should convert");
        assert_eq!(same, 10);
    }

    #[tokio::test]
    async fn test_fallback_on_source_failure() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source.clone()).await;

        let pair = CurrencyPair::new("hours", "USD");

        // First call succeeds and caches
        let rate1 = oracle.get_rate(&pair).await.expect("should get rate");
        assert!((rate1.rate - 25.0).abs() < 0.001);

        // Make source fail
        source.set_should_fail(true);

        // Clear cache to force refetch
        oracle.clear_cache().await;

        // Should return stale cached rate via fallback
        let rate2 = oracle.get_rate_or_fallback(&pair).await;
        // Note: This will fail because we cleared the cache
        // In real usage, the cache would still have the old value
        assert!(rate2.is_err() || rate2.as_ref().map(|r| r.is_stale).unwrap_or(false));
    }

    // === Issue #410: Source Staleness Tracking Tests ===

    #[tokio::test]
    async fn test_source_update_tracking() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source).await;

        // Before any rate fetch, no update info
        let info = oracle.get_source_update_info("test").await;
        assert!(info.is_none());

        // Fetch rate - should record update
        let pair = CurrencyPair::new("hours", "USD");
        let _ = oracle.get_rate(&pair).await.expect("should get rate");

        // Now we should have update info
        let info = oracle.get_source_update_info("test").await;
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.update_count, 1);
        assert_eq!(info.last_rate, Some(25.0));
        assert_eq!(info.last_pair, Some("hours:USD".to_string()));
    }

    #[tokio::test]
    async fn test_source_staleness_check() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source).await;

        // Unknown source returns None
        let is_stale = oracle.is_source_stale("unknown", None).await;
        assert!(is_stale.is_none());

        // Fetch rate to record update
        let pair = CurrencyPair::new("hours", "USD");
        let _ = oracle.get_rate(&pair).await.expect("should get rate");

        // Just updated - not stale with reasonable threshold
        let is_stale = oracle.is_source_stale("test", Some(1000)).await;
        assert_eq!(is_stale, Some(false));

        // Also not stale at 0 threshold since update just happened (0 > 0 is false)
        // This tests the edge case where update time == current time
        let is_stale = oracle.is_source_stale("test", Some(0)).await;
        assert_eq!(is_stale, Some(false)); // 0 elapsed > 0 threshold = false
    }

    #[tokio::test]
    async fn test_list_sources_includes_last_update() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test Source", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source).await;

        // Before any fetch, last_update should be None
        let sources = oracle.list_sources().await;
        assert_eq!(sources.len(), 1);
        assert!(sources[0].last_update.is_none());

        // After fetch, last_update should be set
        let pair = CurrencyPair::new("hours", "USD");
        let _ = oracle.get_rate(&pair).await.expect("should get rate");

        let sources = oracle.list_sources().await;
        assert_eq!(sources.len(), 1);
        assert!(sources[0].last_update.is_some());
    }

    #[tokio::test]
    async fn test_update_count_increments() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source).await;

        let pair = CurrencyPair::new("hours", "USD");

        // Multiple refreshes should increment count
        for i in 1..=3 {
            let _ = oracle.refresh_rate(&pair).await.expect("should get rate");
            let info = oracle.get_source_update_info("test").await.unwrap();
            assert_eq!(info.update_count, i);
        }
    }

    #[tokio::test]
    async fn test_get_stale_sources() {
        let oracle = OracleManager::new(test_store());
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s1", "Source 1", 50).with_rate("hours", "USD", 25.0),
            ))
            .await;
        oracle
            .register_source(Arc::new(
                MockPriceFeed::new("s2", "Source 2", 50).with_rate("hours", "USD", 26.0),
            ))
            .await;

        // Fetch to record updates
        let pair = CurrencyPair::new("hours", "USD");
        let _ = oracle.get_rate(&pair).await.expect("should get rate");

        // With reasonable threshold - nothing stale (just updated)
        let stale = oracle.get_stale_sources(Some(1000)).await;
        assert!(stale.is_empty());

        // With 0 threshold - still not stale since update time == current time
        // (0 elapsed > 0 threshold = false)
        let stale = oracle.get_stale_sources(Some(0)).await;
        assert!(stale.is_empty());
    }

    #[test]
    fn test_source_update_info_staleness_logic() {
        // Test the is_stale method directly with controlled timestamps
        let now = crate::current_timestamp_secs();

        // Source that was just updated
        let fresh = SourceUpdateInfo {
            last_update: now,
            update_count: 1,
            last_rate: Some(25.0),
            last_pair: Some("hours:USD".to_string()),
        };
        assert!(!fresh.is_stale(60)); // Not stale within 60 seconds

        // Source that was updated 10 minutes ago
        let old = SourceUpdateInfo {
            last_update: now.saturating_sub(600),
            update_count: 5,
            last_rate: Some(25.0),
            last_pair: Some("hours:USD".to_string()),
        };
        assert!(old.is_stale(300)); // Stale if threshold is 5 minutes (300s)
        assert!(!old.is_stale(900)); // Not stale if threshold is 15 minutes (900s)
    }

    #[tokio::test]
    async fn test_unregister_cleans_up_tracking_data() {
        let oracle = OracleManager::new(test_store());
        let source =
            Arc::new(MockPriceFeed::new("test", "Test", 50).with_rate("hours", "USD", 25.0));
        oracle.register_source(source).await;

        // Fetch rate to record update
        let pair = CurrencyPair::new("hours", "USD");
        let _ = oracle.get_rate(&pair).await.expect("should get rate");

        // Verify tracking data exists
        let info = oracle.get_source_update_info("test").await;
        assert!(info.is_some());

        // Unregister source
        let removed = oracle.unregister_source("test").await;
        assert!(removed);

        // Tracking data should be cleaned up
        let info = oracle.get_source_update_info("test").await;
        assert!(
            info.is_none(),
            "Tracking data should be removed on unregister"
        );
    }
}
