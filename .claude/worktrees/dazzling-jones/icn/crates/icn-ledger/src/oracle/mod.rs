//! Exchange Rate Oracle
//!
//! Provides multi-source exchange rate aggregation with caching, consensus,
//! and staleness detection for multi-currency support in ICN.
//!
//! # Overview
//!
//! The oracle module enables currency conversion by:
//! - Aggregating rates from multiple sources (manual, federation, external)
//! - Using median consensus with outlier detection
//! - Caching rates with configurable TTL
//! - Detecting stale rates and providing fallbacks
//!
//! # Example
//!
//! ```rust,no_run
//! use icn_ledger::oracle::{OracleManager, ManualRateSource, CurrencyPair};
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create oracle with storage
//! let store = Arc::new(SledStore::temporary()?);
//! let oracle = OracleManager::new(store.clone());
//!
//! // Register manual rate source
//! let manual = ManualRateSource::new(store.clone());
//! manual.set_rate(&CurrencyPair::new("hours", "USD"), 25.0, "did:icn:admin", None)?;
//! oracle.register_source(Arc::new(manual)).await;
//!
//! // Query rate
//! let pair = CurrencyPair::new("hours", "USD");
//! let rate = oracle.get_rate(&pair).await?;
//! println!("1 hour = {} USD", rate.rate);
//!
//! // Convert amount
//! let usd_amount = oracle.convert_amount(10, "hours", "USD").await?;
//! println!("10 hours = {} USD", usd_amount);
//! # Ok(())
//! # }
//! ```
//!
//! # Rate Sources
//!
//! The oracle supports multiple rate sources with priority ordering:
//!
//! | Source | Priority | Description |
//! |--------|----------|-------------|
//! | Manual | 10 | Cooperative-defined rates via governance |
//! | Federation | 20 | Rates from bilateral clearing agreements |
//! | External | 50+ | External API sources (future) |
//!
//! Lower priority numbers are preferred. When multiple sources provide rates,
//! the oracle calculates the median and filters outliers.
//!
//! # Consensus Algorithm
//!
//! 1. Fetch rates from all sources that support the currency pair
//! 2. Calculate median of all rates
//! 3. Filter outliers beyond threshold (default: 15% from median)
//! 4. Return median of remaining valid rates
//!
//! # Staleness Detection
//!
//! Rates are marked stale when:
//! - No source has updated within the staleness threshold (default: 24h)
//! - All sources fail but a cached rate exists
//!
//! Stale rates are still returned but flagged, allowing applications to
//! decide how to handle them.

pub mod error;
pub mod manager;
pub mod price_feed;
pub mod sources;
pub mod types;

// Re-export main types
pub use error::{OracleError, OracleResult};
pub use manager::{CacheStats, OracleManager};
pub use price_feed::PriceFeed;
pub use sources::{FederationRateSource, ManualRateSource};
pub use types::{
    CurrencyPair, ExchangeRate, ManualRateRecord, OracleConfig, RateAuditEntry, RateObservation,
    SourceInfo, DEFAULT_SUSPICIOUS_RATE_THRESHOLD,
};

// Re-export AgreementRates for federation integration
pub use sources::federation::AgreementRates;
