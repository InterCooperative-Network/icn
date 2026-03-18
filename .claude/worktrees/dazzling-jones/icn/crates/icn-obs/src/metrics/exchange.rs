//! Exchange (listings) metrics
//!
//! Prometheus metrics for internal exchange feature tracking.
//! Tracks listing creation, interest expression, and status transitions.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize exchange metric descriptions
pub fn init_descriptions() {
    // Counter metrics
    describe_counter!(
        "icn_exchange_listings_created_total",
        "Total number of listings created by type"
    );
    describe_counter!(
        "icn_exchange_listings_completed_total",
        "Total number of listings completed (exchanged)"
    );
    describe_counter!(
        "icn_exchange_listings_cancelled_total",
        "Total number of listings cancelled"
    );
    describe_counter!(
        "icn_exchange_listings_expired_total",
        "Total number of listings expired"
    );
    describe_counter!(
        "icn_exchange_interests_expressed_total",
        "Total number of interests expressed on listings"
    );
    describe_counter!(
        "icn_exchange_duplicate_interests_blocked_total",
        "Total number of duplicate interest attempts blocked"
    );

    // Gauge metrics
    describe_gauge!(
        "icn_exchange_listings_active",
        "Current number of active listings"
    );
    describe_gauge!(
        "icn_exchange_listings_matched",
        "Current number of matched listings (pending completion)"
    );

    // Histogram metrics
    describe_histogram!(
        "icn_exchange_listing_interests_count",
        "Distribution of interests per listing"
    );
    describe_histogram!(
        "icn_exchange_listing_time_to_match_seconds",
        "Time from listing creation to first match in seconds"
    );
    describe_histogram!(
        "icn_exchange_listing_time_to_complete_seconds",
        "Time from listing creation to completion in seconds"
    );
}

// Counter metrics

/// Increment listings created counter
pub fn listings_created_inc(listing_type: &str, category: &str) {
    counter!(
        "icn_exchange_listings_created_total",
        "listing_type" => listing_type.to_string(),
        "category" => category.to_string()
    )
    .increment(1);
}

/// Increment listings completed counter
pub fn listings_completed_inc() {
    counter!("icn_exchange_listings_completed_total").increment(1);
}

/// Increment listings cancelled counter
pub fn listings_cancelled_inc() {
    counter!("icn_exchange_listings_cancelled_total").increment(1);
}

/// Increment listings expired counter
pub fn listings_expired_inc() {
    counter!("icn_exchange_listings_expired_total").increment(1);
}

/// Increment interests expressed counter
pub fn interests_expressed_inc(listing_type: &str) {
    counter!(
        "icn_exchange_interests_expressed_total",
        "listing_type" => listing_type.to_string()
    )
    .increment(1);
}

/// Increment duplicate interests blocked counter
pub fn duplicate_interests_blocked_inc() {
    counter!("icn_exchange_duplicate_interests_blocked_total").increment(1);
}

// Gauge metrics

/// Set current number of active listings
pub fn listings_active_set(count: usize) {
    gauge!("icn_exchange_listings_active").set(count as f64);
}

/// Set current number of matched listings
pub fn listings_matched_set(count: usize) {
    gauge!("icn_exchange_listings_matched").set(count as f64);
}

// Histogram metrics

/// Record the number of interests on a completed/matched listing
pub fn listing_interests_count_observe(count: usize) {
    histogram!("icn_exchange_listing_interests_count").record(count as f64);
}

/// Record time from listing creation to first match
pub fn listing_time_to_match_observe(duration_secs: f64) {
    histogram!("icn_exchange_listing_time_to_match_seconds").record(duration_secs);
}

/// Record time from listing creation to completion
pub fn listing_time_to_complete_observe(duration_secs: f64) {
    histogram!("icn_exchange_listing_time_to_complete_seconds").record(duration_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_metrics() {
        // Test listing type counters
        listings_created_inc("Offer", "Equipment");
        listings_created_inc("Offer", "Services");
        listings_created_inc("Want", "Materials");
        listings_created_inc("Want", "Space");

        // Test completion counters
        listings_completed_inc();
        listings_cancelled_inc();
        listings_expired_inc();

        // Test interest counters
        interests_expressed_inc("Offer");
        interests_expressed_inc("Want");
        duplicate_interests_blocked_inc();
    }

    #[test]
    fn test_gauge_metrics() {
        listings_active_set(10);
        listings_matched_set(3);

        // Test zero counts
        listings_active_set(0);
        listings_matched_set(0);
    }

    #[test]
    fn test_histogram_metrics() {
        // Test interests distribution
        listing_interests_count_observe(0);
        listing_interests_count_observe(1);
        listing_interests_count_observe(5);
        listing_interests_count_observe(10);

        // Test time to match
        listing_time_to_match_observe(3600.0); // 1 hour
        listing_time_to_match_observe(86400.0); // 1 day

        // Test time to complete
        listing_time_to_complete_observe(7200.0); // 2 hours
        listing_time_to_complete_observe(172800.0); // 2 days
    }
}
