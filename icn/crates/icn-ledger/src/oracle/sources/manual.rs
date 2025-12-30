//! Manual rate source for cooperative-defined exchange rates
//!
//! Allows cooperatives to set and manage their own exchange rates,
//! typically through governance proposals or admin actions.

use crate::oracle::error::{OracleError, OracleResult};
use crate::oracle::price_feed::PriceFeed;
use crate::oracle::types::{CurrencyPair, ManualRateRecord, RateObservation};
use async_trait::async_trait;
use icn_store::Store;
use std::sync::Arc;
use tracing::{debug, info};

/// Storage key prefix for manual rates
const MANUAL_RATE_PREFIX: &str = "oracle:manual:";

/// A price feed source for cooperative-defined (manual) exchange rates
///
/// Manual rates are set by governance or administrators and stored persistently.
/// They have the highest priority (lowest priority number) since cooperative-defined
/// rates should take precedence over external sources.
pub struct ManualRateSource {
    store: Arc<dyn Store>,
}

impl ManualRateSource {
    /// Create a new manual rate source
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Set a manual exchange rate
    ///
    /// # Arguments
    /// * `pair` - The currency pair
    /// * `rate` - The exchange rate (how many `to` per 1 `from`)
    /// * `set_by` - DID of who set this rate
    /// * `note` - Optional note/reason for the rate
    pub fn set_rate(
        &self,
        pair: &CurrencyPair,
        rate: f64,
        set_by: &str,
        note: Option<String>,
    ) -> OracleResult<ManualRateRecord> {
        // Validate rate
        if rate <= 0.0 || rate.is_nan() || rate.is_infinite() {
            return Err(OracleError::InvalidRate(format!(
                "Rate must be a positive finite number, got: {rate}"
            )));
        }

        let record = ManualRateRecord {
            rate,
            set_by: set_by.to_string(),
            set_at: crate::current_timestamp_secs(),
            note,
        };

        let key = format!("{}{}", MANUAL_RATE_PREFIX, pair.key());
        let value = serde_json::to_vec(&record).map_err(OracleError::from)?;

        self.store
            .put(key.as_bytes(), &value)
            .map_err(|e| OracleError::Storage(e.to_string()))?;

        info!(
            pair = %pair,
            rate = rate,
            set_by = set_by,
            "Set manual exchange rate"
        );

        Ok(record)
    }

    /// Remove a manual exchange rate
    pub fn remove_rate(&self, pair: &CurrencyPair) -> OracleResult<bool> {
        let key = format!("{}{}", MANUAL_RATE_PREFIX, pair.key());

        // Check if it exists first
        let exists = self
            .store
            .get(key.as_bytes())
            .map_err(|e| OracleError::Storage(e.to_string()))?
            .is_some();

        if exists {
            self.store
                .delete(key.as_bytes())
                .map_err(|e| OracleError::Storage(e.to_string()))?;

            info!(pair = %pair, "Removed manual exchange rate");
        }

        Ok(exists)
    }

    /// Get a manual rate record (includes metadata)
    pub fn get_rate_record(&self, pair: &CurrencyPair) -> OracleResult<Option<ManualRateRecord>> {
        let key = format!("{}{}", MANUAL_RATE_PREFIX, pair.key());

        match self.store.get(key.as_bytes()) {
            Ok(Some(bytes)) => {
                let record: ManualRateRecord =
                    serde_json::from_slice(&bytes).map_err(OracleError::from)?;
                Ok(Some(record))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(OracleError::Storage(e.to_string())),
        }
    }

    /// List all manually set rates
    pub fn list_rates(&self) -> OracleResult<Vec<(CurrencyPair, ManualRateRecord)>> {
        let prefix = MANUAL_RATE_PREFIX.as_bytes();
        let mut rates = Vec::new();

        let pairs = self
            .store
            .scan(prefix)
            .map_err(|e| OracleError::Storage(e.to_string()))?;

        for (key, value) in pairs {
            // Extract pair from key: "oracle:manual:from:to"
            let key_str = String::from_utf8_lossy(&key);
            if let Some(pair_str) = key_str.strip_prefix(MANUAL_RATE_PREFIX) {
                if let Some((from, to)) = pair_str.split_once(':') {
                    let pair = CurrencyPair::new(from, to);
                    if let Ok(record) = serde_json::from_slice::<ManualRateRecord>(&value) {
                        rates.push((pair, record));
                    }
                }
            }
        }

        Ok(rates)
    }
}

#[async_trait]
impl PriceFeed for ManualRateSource {
    fn source_id(&self) -> &str {
        "manual"
    }

    fn name(&self) -> &str {
        "Manual Cooperative Rates"
    }

    fn priority(&self) -> u8 {
        10 // High priority - cooperative rates preferred over external sources
    }

    async fn get_rate(&self, pair: &CurrencyPair) -> OracleResult<RateObservation> {
        debug!(pair = %pair, "Fetching manual rate");

        match self.get_rate_record(pair)? {
            Some(record) => Ok(RateObservation {
                rate: record.rate,
                source: self.source_id().to_string(),
                observed_at: record.set_at,
                confidence: Some(1.0), // Manual rates are authoritative
            }),
            None => Err(OracleError::RateNotFound(pair.clone())),
        }
    }

    fn supported_pairs(&self) -> Option<Vec<CurrencyPair>> {
        // Return list of pairs we have rates for
        match self.list_rates() {
            Ok(rates) => Some(rates.into_iter().map(|(pair, _)| pair).collect()),
            Err(_) => None,
        }
    }

    fn supports_pair(&self, pair: &CurrencyPair) -> bool {
        // Check if we have a rate for this specific pair
        matches!(self.get_rate_record(pair), Ok(Some(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_store::SledStore;

    fn test_store() -> Arc<dyn Store> {
        Arc::new(SledStore::temporary().expect("create temp store"))
    }

    #[test]
    fn test_set_and_get_rate() {
        let source = ManualRateSource::new(test_store());
        let pair = CurrencyPair::new("hours", "USD");

        // Set rate
        let record = source
            .set_rate(
                &pair,
                25.0,
                "did:icn:alice",
                Some("initial rate".to_string()),
            )
            .expect("should set rate");

        assert!((record.rate - 25.0).abs() < 0.001);
        assert_eq!(record.set_by, "did:icn:alice");
        assert_eq!(record.note, Some("initial rate".to_string()));

        // Get rate
        let retrieved = source.get_rate_record(&pair).expect("should get rate");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!((retrieved.rate - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_update_rate() {
        let source = ManualRateSource::new(test_store());
        let pair = CurrencyPair::new("hours", "USD");

        // Set initial rate
        source
            .set_rate(&pair, 25.0, "did:icn:alice", None)
            .expect("should set rate");

        // Update rate
        source
            .set_rate(
                &pair,
                30.0,
                "did:icn:bob",
                Some("rate adjustment".to_string()),
            )
            .expect("should update rate");

        // Check new rate
        let retrieved = source
            .get_rate_record(&pair)
            .expect("should get rate")
            .expect("rate should exist");
        assert!((retrieved.rate - 30.0).abs() < 0.001);
        assert_eq!(retrieved.set_by, "did:icn:bob");
    }

    #[test]
    fn test_remove_rate() {
        let source = ManualRateSource::new(test_store());
        let pair = CurrencyPair::new("hours", "USD");

        // Set rate
        source
            .set_rate(&pair, 25.0, "did:icn:alice", None)
            .expect("should set rate");

        // Remove rate
        let removed = source.remove_rate(&pair).expect("should remove");
        assert!(removed);

        // Verify gone
        let retrieved = source.get_rate_record(&pair).expect("should query");
        assert!(retrieved.is_none());

        // Remove again should return false
        let removed_again = source.remove_rate(&pair).expect("should not error");
        assert!(!removed_again);
    }

    #[test]
    fn test_invalid_rate() {
        let source = ManualRateSource::new(test_store());
        let pair = CurrencyPair::new("hours", "USD");

        // Negative rate
        let result = source.set_rate(&pair, -25.0, "did:icn:alice", None);
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));

        // Zero rate
        let result = source.set_rate(&pair, 0.0, "did:icn:alice", None);
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));

        // NaN rate
        let result = source.set_rate(&pair, f64::NAN, "did:icn:alice", None);
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));
    }

    #[test]
    fn test_list_rates() {
        let source = ManualRateSource::new(test_store());

        // Set multiple rates
        source
            .set_rate(
                &CurrencyPair::new("hours", "USD"),
                25.0,
                "did:icn:alice",
                None,
            )
            .expect("should set");
        source
            .set_rate(
                &CurrencyPair::new("hours", "EUR"),
                22.0,
                "did:icn:alice",
                None,
            )
            .expect("should set");
        source
            .set_rate(
                &CurrencyPair::new("kWh", "USD"),
                0.15,
                "did:icn:alice",
                None,
            )
            .expect("should set");

        // List all
        let rates = source.list_rates().expect("should list");
        assert_eq!(rates.len(), 3);
    }

    #[tokio::test]
    async fn test_price_feed_implementation() {
        let source = ManualRateSource::new(test_store());
        let pair = CurrencyPair::new("hours", "USD");

        // Set rate
        source
            .set_rate(&pair, 25.0, "did:icn:alice", None)
            .expect("should set rate");

        // Use PriceFeed interface
        assert_eq!(source.source_id(), "manual");
        assert_eq!(source.priority(), 10);
        assert!(source.supports_pair(&pair));

        let obs = source.get_rate(&pair).await.expect("should get rate");
        assert!((obs.rate - 25.0).abs() < 0.001);
        assert_eq!(obs.source, "manual");
        assert_eq!(obs.confidence, Some(1.0));
    }

    #[tokio::test]
    async fn test_rate_not_found() {
        let source = ManualRateSource::new(test_store());
        let pair = CurrencyPair::new("hours", "USD");

        let result = source.get_rate(&pair).await;
        assert!(matches!(result, Err(OracleError::RateNotFound(_))));
    }
}
