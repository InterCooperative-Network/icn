//! Trust service metrics
//!
//! Histogram metrics for monitoring trust query latency and performance characteristics.

use metrics::{counter, describe_counter, describe_histogram, histogram};
use std::time::Duration;

/// Initialize trust metric descriptions
pub fn init_descriptions() {
    // Fast path query latency (authorization decisions)
    describe_histogram!(
        "icn_trust_score_duration_seconds",
        "Trust score query latency (fast path for authorization decisions)"
    );

    // Enriched path query latency (cache validation, debugging)
    describe_histogram!(
        "icn_trust_score_detailed_duration_seconds",
        "Trust score detailed query latency (enriched path with provenance metadata)"
    );

    // Legacy metrics (used by icn-trust crate)
    describe_counter!("icn_trust_lookups_total", "Total trust score lookups");
    describe_counter!("icn_trust_cache_hits_total", "Total trust cache hits");
    describe_counter!("icn_trust_cache_misses_total", "Total trust cache misses");
    describe_histogram!(
        "icn_trust_score_distribution",
        "Distribution of trust scores"
    );
    describe_counter!(
        "icn_trust_computation_errors_total",
        "Total trust computation errors"
    );

    // Attestation metrics (used by trust app)
    describe_counter!(
        "icn_trust_attestations_ingested_total",
        "Total trust attestations ingested"
    );
    describe_counter!(
        "icn_trust_attestations_signed_total",
        "Total trust attestations signed"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_expired_total",
        "Total trust attestations rejected due to expiration"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_invalid_signature_total",
        "Total trust attestations rejected due to invalid signature"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_malformed_total",
        "Total trust attestations rejected due to malformed data"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_outdated_total",
        "Total trust attestations rejected due to outdated timestamp"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_size_limit_total",
        "Total trust attestations rejected due to size limit"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_rate_limited_total",
        "Total trust attestations rejected due to rate limiting"
    );
}

/// Record trust_score() query duration
///
/// This is the fast path used for authorization decisions.
///
/// Expected values:
/// - P50: ~5µs
/// - P95: ~10µs
/// - P99: ~50µs
pub fn trust_score_duration_record(duration: Duration) {
    histogram!("icn_trust_score_duration_seconds").record(duration.as_secs_f64());
}

/// Record trust_score_detailed() query duration
///
/// This is the enriched path used for cache validation and debugging.
///
/// Expected values (1000-node network):
/// - P50: ~120ms (current implementation)
/// - P50: ~6ms (after reverse edge index optimization)
pub fn trust_score_detailed_duration_record(duration: Duration) {
    histogram!("icn_trust_score_detailed_duration_seconds").record(duration.as_secs_f64());
}

// Legacy metrics (used by icn-trust crate)

/// Increment trust lookups counter
pub fn lookups_inc() {
    counter!("icn_trust_lookups_total").increment(1);
}

/// Increment trust cache hits counter
pub fn cache_hits_inc() {
    counter!("icn_trust_cache_hits_total").increment(1);
}

/// Increment trust cache misses counter
pub fn cache_misses_inc() {
    counter!("icn_trust_cache_misses_total").increment(1);
}

/// Record trust score distribution
pub fn score_distribution_record(score: f64) {
    histogram!("icn_trust_score_distribution").record(score);
}

/// Increment trust computation errors counter
pub fn computation_errors_inc() {
    counter!("icn_trust_computation_errors_total").increment(1);
}

// Attestation metrics (used by trust app)

/// Increment trust attestations ingested counter
pub fn attestations_ingested_inc() {
    counter!("icn_trust_attestations_ingested_total").increment(1);
}

/// Increment trust attestations signed counter
pub fn attestations_signed_inc() {
    counter!("icn_trust_attestations_signed_total").increment(1);
}

/// Increment attestations rejected due to expiration
pub fn attestations_rejected_expired_inc() {
    counter!("icn_trust_attestations_rejected_expired_total").increment(1);
}

/// Increment attestations rejected due to invalid signature
pub fn attestations_rejected_invalid_signature_inc() {
    counter!("icn_trust_attestations_rejected_invalid_signature_total").increment(1);
}

/// Increment attestations rejected due to malformed data
pub fn attestations_rejected_malformed_inc() {
    counter!("icn_trust_attestations_rejected_malformed_total").increment(1);
}

/// Increment attestations rejected due to outdated timestamp
pub fn attestations_rejected_outdated_inc() {
    counter!("icn_trust_attestations_rejected_outdated_total").increment(1);
}

/// Increment attestations rejected due to size limit
pub fn attestations_rejected_size_limit_inc() {
    counter!("icn_trust_attestations_rejected_size_limit_total").increment(1);
}

/// Increment attestations rejected due to rate limiting
pub fn attestations_rejected_rate_limited_inc() {
    counter!("icn_trust_attestations_rejected_rate_limited_total").increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_descriptions() {
        // Should not panic
        init_descriptions();
    }

    #[test]
    fn test_trust_score_duration_record() {
        init_descriptions();
        trust_score_duration_record(Duration::from_micros(5));
    }

    #[test]
    fn test_trust_score_detailed_duration_record() {
        init_descriptions();
        trust_score_detailed_duration_record(Duration::from_millis(120));
    }

    #[test]
    fn test_legacy_metrics() {
        init_descriptions();
        lookups_inc();
        cache_hits_inc();
        cache_misses_inc();
        score_distribution_record(0.75);
        computation_errors_inc();
    }

    #[test]
    fn test_attestation_metrics() {
        init_descriptions();
        attestations_ingested_inc();
        attestations_signed_inc();
        attestations_rejected_expired_inc();
        attestations_rejected_invalid_signature_inc();
        attestations_rejected_malformed_inc();
        attestations_rejected_outdated_inc();
        attestations_rejected_size_limit_inc();
        attestations_rejected_rate_limited_inc();
    }
}
