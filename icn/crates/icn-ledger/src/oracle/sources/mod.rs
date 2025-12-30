//! Price feed source implementations
//!
//! This module contains implementations of the `PriceFeed` trait for different
//! rate sources.

pub mod federation;
mod manual;

pub use federation::FederationRateSource;
pub use manual::ManualRateSource;
