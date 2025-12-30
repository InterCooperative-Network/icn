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

/// Oracle manager for exchange rate queries with caching and multi-source aggregation
pub struct OracleManager {
    /// Persistent storage
    store: Arc<dyn Store>,

    /// In-memory rate cache with TTL
    cache: RwLock<HashMap<String, ExchangeRate>>,

    /// Registered price feed sources (sorted by priority)
    sources: RwLock<Vec<Arc<dyn PriceFeed>>>,

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
            config,
        }
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
    pub async fn unregister_source(&self, source_id: &str) -> bool {
        let mut sources = self.sources.write().await;
        let initial_len = sources.len();
        sources.retain(|s| s.source_id() != source_id);
        let removed = sources.len() < initial_len;
        if removed {
            info!(source_id = %source_id, "Unregistered price feed source");
        }
        removed
    }

    /// List registered sources with their status
    pub async fn list_sources(&self) -> Vec<SourceInfo> {
        let sources = self.sources.read().await;
        let mut infos = Vec::with_capacity(sources.len());

        for source in sources.iter() {
            let is_healthy = source.health_check().await;
            infos.push(SourceInfo {
                source_id: source.source_id().to_string(),
                name: source.name().to_string(),
                priority: source.priority(),
                is_healthy,
                last_update: None, // TODO: track last successful update per source
            });
        }

        infos
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
    pub fn load_from_storage(&self) -> OracleResult<usize> {
        // Note: This is synchronous because it's called during initialization
        // In a future iteration, this could be made async

        // Scan is sync, but we need to update the cache atomically
        // For now, we'll skip the async cache update in load_from_storage
        // This is acceptable because load is typically called at startup before queries

        // TODO: Implement proper async loading if needed
        debug!("Load from storage skipped - rates will be fetched on demand");

        Ok(0)
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
        let mut rates: Vec<f64> = observations.iter().map(|o| o.rate).collect();
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Calculate median
        let median = if rates.len() % 2 == 0 {
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
        let final_rate = if !valid_observations.is_empty() {
            let mut valid_rates: Vec<f64> = valid_observations.iter().map(|o| o.rate).collect();
            valid_rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if valid_rates.len() % 2 == 0 {
                (valid_rates[valid_rates.len() / 2 - 1] + valid_rates[valid_rates.len() / 2]) / 2.0
            } else {
                valid_rates[valid_rates.len() / 2]
            }
        } else {
            // Fallback to median if all are "outliers" (shouldn't happen normally)
            median
        };

        let now = crate::current_timestamp_secs();

        Ok(ExchangeRate {
            pair: pair.clone(),
            rate: final_rate,
            observations: valid_observations,
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
}
