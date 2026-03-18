//! Federation rate source for inter-cooperative exchange rates
//!
//! Reads exchange rates from bilateral clearing agreements between cooperatives.
//! These rates are negotiated between cooperatives and stored in the federation module.

use crate::oracle::error::{OracleError, OracleResult};
use crate::oracle::price_feed::PriceFeed;
use crate::oracle::types::{CurrencyPair, RateObservation};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// A price feed source for federation bilateral agreement rates
///
/// This source reads exchange rates from `BilateralClearingAgreement` structures
/// that are negotiated between cooperatives. It has high priority (20) since
/// federation-negotiated rates should take precedence over external sources
/// but may be overridden by cooperative-specific manual rates.
pub struct FederationRateSource {
    /// Cached rates from clearing agreements
    /// Key format: "from:to" -> (rate, agreement_id, timestamp)
    rates: Arc<RwLock<HashMap<String, FederationRate>>>,
}

/// A rate from a federation agreement
#[derive(Debug, Clone)]
struct FederationRate {
    rate: f64,
    agreement_id: String,
    timestamp: u64,
}

impl FederationRateSource {
    /// Create a new empty federation rate source
    pub fn new() -> Self {
        Self {
            rates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update rates from a list of bilateral clearing agreements
    ///
    /// This should be called when agreements are created or updated.
    /// The format expected is compatible with `icn_federation::BilateralClearingAgreement`.
    pub async fn update_from_agreements(&self, agreements: Vec<AgreementRates>) {
        let mut rates = self.rates.write().await;
        rates.clear();

        for agreement in agreements {
            for (pair_key, rate) in agreement.exchange_rates {
                rates.insert(
                    pair_key,
                    FederationRate {
                        rate,
                        agreement_id: agreement.agreement_id.clone(),
                        timestamp: agreement.created_at,
                    },
                );
            }
        }

        debug!(
            rate_count = rates.len(),
            "Updated federation rates from agreements"
        );
    }

    /// Add or update a single rate
    ///
    /// Returns an error if the rate is invalid (NaN, infinity, zero, or negative).
    pub async fn set_rate(
        &self,
        from: &str,
        to: &str,
        rate: f64,
        agreement_id: &str,
    ) -> OracleResult<()> {
        // Validate rate
        if rate <= 0.0 || !rate.is_finite() {
            return Err(OracleError::InvalidRate(format!(
                "Rate must be a positive finite number, got: {rate}"
            )));
        }

        let mut rates = self.rates.write().await;
        let key = format!("{from}:{to}");
        rates.insert(
            key,
            FederationRate {
                rate,
                agreement_id: agreement_id.to_string(),
                timestamp: crate::current_timestamp_secs(),
            },
        );
        Ok(())
    }

    /// Remove a rate
    pub async fn remove_rate(&self, from: &str, to: &str) -> bool {
        let mut rates = self.rates.write().await;
        let key = format!("{from}:{to}");
        rates.remove(&key).is_some()
    }

    /// Get all available rates
    pub async fn list_rates(&self) -> Vec<(String, f64, String)> {
        let rates = self.rates.read().await;
        rates
            .iter()
            .map(|(key, rate)| (key.clone(), rate.rate, rate.agreement_id.clone()))
            .collect()
    }
}

impl Default for FederationRateSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Simplified representation of agreement rates for updating the source
#[derive(Debug, Clone)]
pub struct AgreementRates {
    /// Agreement identifier
    pub agreement_id: String,
    /// Exchange rates: "from:to" -> rate
    pub exchange_rates: HashMap<String, f64>,
    /// When the agreement was created
    pub created_at: u64,
}

#[async_trait]
impl PriceFeed for FederationRateSource {
    fn source_id(&self) -> &str {
        "federation"
    }

    fn name(&self) -> &str {
        "Federation Bilateral Agreements"
    }

    fn priority(&self) -> u8 {
        20 // High priority for inter-coop rates, but below manual rates
    }

    async fn get_rate(&self, pair: &CurrencyPair) -> OracleResult<RateObservation> {
        debug!(pair = %pair, "Fetching federation rate");

        let rates = self.rates.read().await;

        if let Some(fed_rate) = rates.get(&pair.key()) {
            return Ok(RateObservation {
                rate: fed_rate.rate,
                source: format!("federation:{}", fed_rate.agreement_id),
                observed_at: fed_rate.timestamp,
                confidence: Some(0.95), // High confidence for bilateral rates
            });
        }

        Err(OracleError::RateNotFound(pair.clone()))
    }

    fn supported_pairs(&self) -> Option<Vec<CurrencyPair>> {
        // Return list of pairs we have rates for
        // This is blocking but acceptable since it's just reading a hashmap
        // In production, this could be improved with a cached list
        None // Dynamic - check via supports_pair
    }

    fn supports_pair(&self, pair: &CurrencyPair) -> bool {
        // Use try_read to avoid blocking; return false if lock is contested
        self.rates
            .try_read()
            .map(|rates| rates.contains_key(&pair.key()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_rate() {
        let source = FederationRateSource::new();

        // Set a rate
        source
            .set_rate("hours", "USD", 25.0, "agreement-1")
            .await
            .expect("should set rate");

        // Get it back
        let pair = CurrencyPair::new("hours", "USD");
        let obs = source.get_rate(&pair).await.expect("should get rate");

        assert!((obs.rate - 25.0).abs() < 0.001);
        assert_eq!(obs.source, "federation:agreement-1");
        assert_eq!(obs.confidence, Some(0.95));
    }

    #[tokio::test]
    async fn test_update_from_agreements() {
        let source = FederationRateSource::new();

        let agreements = vec![
            AgreementRates {
                agreement_id: "food-tech".to_string(),
                exchange_rates: {
                    let mut m = HashMap::new();
                    m.insert("hours:USD".to_string(), 25.0);
                    m.insert("hours:EUR".to_string(), 22.0);
                    m
                },
                created_at: 1000,
            },
            AgreementRates {
                agreement_id: "energy-food".to_string(),
                exchange_rates: {
                    let mut m = HashMap::new();
                    m.insert("kWh:USD".to_string(), 0.15);
                    m
                },
                created_at: 2000,
            },
        ];

        source.update_from_agreements(agreements).await;

        // Check rates
        let rates = source.list_rates().await;
        assert_eq!(rates.len(), 3);

        let pair = CurrencyPair::new("hours", "USD");
        let obs = source.get_rate(&pair).await.expect("should get rate");
        assert!((obs.rate - 25.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_remove_rate() {
        let source = FederationRateSource::new();

        // Set and remove
        source
            .set_rate("hours", "USD", 25.0, "agreement-1")
            .await
            .expect("should set rate");
        assert!(source.supports_pair(&CurrencyPair::new("hours", "USD")));

        source.remove_rate("hours", "USD").await;
        assert!(!source.supports_pair(&CurrencyPair::new("hours", "USD")));
    }

    #[tokio::test]
    async fn test_rate_not_found() {
        let source = FederationRateSource::new();
        let pair = CurrencyPair::new("hours", "USD");

        let result = source.get_rate(&pair).await;
        assert!(matches!(result, Err(OracleError::RateNotFound(_))));
    }

    #[tokio::test]
    async fn test_priority() {
        let source = FederationRateSource::new();
        assert_eq!(source.priority(), 20);
        assert_eq!(source.source_id(), "federation");
    }

    #[tokio::test]
    async fn test_invalid_rate() {
        let source = FederationRateSource::new();

        // Negative rate
        let result = source.set_rate("hours", "USD", -25.0, "agreement-1").await;
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));

        // Zero rate
        let result = source.set_rate("hours", "USD", 0.0, "agreement-1").await;
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));

        // NaN rate
        let result = source
            .set_rate("hours", "USD", f64::NAN, "agreement-1")
            .await;
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));

        // Infinity rate
        let result = source
            .set_rate("hours", "USD", f64::INFINITY, "agreement-1")
            .await;
        assert!(matches!(result, Err(OracleError::InvalidRate(_))));
    }
}
