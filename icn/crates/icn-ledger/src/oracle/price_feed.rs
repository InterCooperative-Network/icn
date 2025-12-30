//! Price feed trait for exchange rate data sources
//!
//! Defines the interface for different exchange rate sources (manual, federation, external APIs).

use super::error::OracleResult;
use super::types::{CurrencyPair, RateObservation};
use async_trait::async_trait;

/// Abstract interface for exchange rate data sources
///
/// Implementations can provide rates from various sources:
/// - Manual/cooperative-defined rates
/// - Federation bilateral agreements
/// - External APIs (CoinGecko, ExchangeRate-API, etc.)
#[async_trait]
pub trait PriceFeed: Send + Sync {
    /// Unique identifier for this source
    fn source_id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Priority (lower = higher priority, used for fallback ordering)
    /// Default: 100
    fn priority(&self) -> u8 {
        100
    }

    /// Supported currency pairs (None = supports all pairs dynamically)
    fn supported_pairs(&self) -> Option<Vec<CurrencyPair>> {
        None
    }

    /// Check if this source supports a specific pair
    fn supports_pair(&self, pair: &CurrencyPair) -> bool {
        self.supported_pairs()
            .map(|pairs| pairs.contains(pair))
            .unwrap_or(true)
    }

    /// Fetch the current exchange rate for a currency pair
    async fn get_rate(&self, pair: &CurrencyPair) -> OracleResult<RateObservation>;

    /// Bulk fetch rates (optional optimization)
    /// Default implementation calls get_rate for each pair
    async fn get_rates(&self, pairs: &[CurrencyPair]) -> Vec<OracleResult<RateObservation>> {
        let mut results = Vec::with_capacity(pairs.len());
        for pair in pairs {
            results.push(self.get_rate(pair).await);
        }
        results
    }

    /// Check if the source is healthy/reachable
    /// Default: always returns true
    async fn health_check(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// A mock price feed for testing
    pub struct MockPriceFeed {
        pub source_id: String,
        pub name: String,
        pub priority: u8,
        pub rates: RwLock<HashMap<String, f64>>,
        pub should_fail: RwLock<bool>,
    }

    impl MockPriceFeed {
        pub fn new(source_id: &str, name: &str, priority: u8) -> Self {
            Self {
                source_id: source_id.to_string(),
                name: name.to_string(),
                priority,
                rates: RwLock::new(HashMap::new()),
                should_fail: RwLock::new(false),
            }
        }

        pub fn with_rate(self, from: &str, to: &str, rate: f64) -> Self {
            {
                let mut rates = self.rates.write().expect("lock");
                rates.insert(format!("{from}:{to}"), rate);
            }
            self
        }

        pub fn set_should_fail(&self, fail: bool) {
            *self.should_fail.write().expect("lock") = fail;
        }
    }

    #[async_trait]
    impl PriceFeed for MockPriceFeed {
        fn source_id(&self) -> &str {
            &self.source_id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> u8 {
            self.priority
        }

        async fn get_rate(&self, pair: &CurrencyPair) -> OracleResult<RateObservation> {
            if *self.should_fail.read().expect("lock") {
                return Err(super::super::error::OracleError::SourceUnavailable(
                    self.source_id.clone(),
                    "mock failure".to_string(),
                ));
            }

            let rates = self.rates.read().expect("lock");
            rates
                .get(&pair.key())
                .map(|&rate| RateObservation::new(rate, &self.source_id).with_confidence(0.9))
                .ok_or_else(|| super::super::error::OracleError::RateNotFound(pair.clone()))
        }

        async fn health_check(&self) -> bool {
            !*self.should_fail.read().expect("lock")
        }
    }
}
