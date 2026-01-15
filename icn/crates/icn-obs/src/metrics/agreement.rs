//! Agreement metrics
//!
//! Prometheus metrics for inter-cooperative agreement lifecycle events.
//! Tracks agreement creation, activation, termination, signatures, and amendments.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize agreement metric descriptions
pub fn init_descriptions() {
    // Counter metrics
    describe_counter!(
        "icn_agreement_created_total",
        "Total number of agreements created by type"
    );
    describe_counter!(
        "icn_agreement_activated_total",
        "Total number of agreements activated"
    );
    describe_counter!(
        "icn_agreement_terminated_total",
        "Total number of agreements terminated by reason"
    );
    describe_counter!(
        "icn_agreement_signatures_total",
        "Total number of signatures added to agreements"
    );
    describe_counter!(
        "icn_agreement_amendments_proposed_total",
        "Total number of amendments proposed"
    );
    describe_counter!(
        "icn_agreement_amendments_ratified_total",
        "Total number of amendments ratified"
    );
    describe_counter!(
        "icn_agreement_suspended_total",
        "Total number of agreements suspended"
    );
    describe_counter!(
        "icn_agreement_resumed_total",
        "Total number of agreements resumed from suspension"
    );

    // Gauge metrics
    describe_gauge!(
        "icn_agreement_active",
        "Current number of active agreements"
    );
    describe_gauge!(
        "icn_agreement_pending",
        "Current number of pending agreements awaiting signatures"
    );
    describe_gauge!(
        "icn_agreement_suspended",
        "Current number of suspended agreements"
    );
    describe_gauge!("icn_agreement_draft", "Current number of draft agreements");

    // Histogram metrics
    describe_histogram!(
        "icn_agreement_activation_duration_seconds",
        "Time from proposal to activation in seconds"
    );
    describe_histogram!(
        "icn_agreement_amendment_ratification_duration_seconds",
        "Time from amendment proposal to ratification in seconds"
    );
}

// Counter metrics

/// Increment agreements created counter
pub fn agreements_created_inc(agreement_type: &str) {
    counter!(
        "icn_agreement_created_total",
        "agreement_type" => agreement_type.to_string()
    )
    .increment(1);
}

/// Increment agreements activated counter
pub fn agreements_activated_inc() {
    counter!("icn_agreement_activated_total").increment(1);
}

/// Increment agreements terminated counter
pub fn agreements_terminated_inc(reason: &str) {
    counter!(
        "icn_agreement_terminated_total",
        "reason" => reason.to_string()
    )
    .increment(1);
}

/// Increment agreement signatures counter
pub fn agreement_signatures_inc() {
    counter!("icn_agreement_signatures_total").increment(1);
}

/// Increment amendments proposed counter
pub fn amendments_proposed_inc() {
    counter!("icn_agreement_amendments_proposed_total").increment(1);
}

/// Increment amendments ratified counter
pub fn amendments_ratified_inc() {
    counter!("icn_agreement_amendments_ratified_total").increment(1);
}

/// Increment agreements suspended counter
pub fn agreements_suspended_inc() {
    counter!("icn_agreement_suspended_total").increment(1);
}

/// Increment agreements resumed counter
pub fn agreements_resumed_inc() {
    counter!("icn_agreement_resumed_total").increment(1);
}

// Gauge metrics

/// Set current number of active agreements
pub fn agreements_active_set(count: usize) {
    gauge!("icn_agreement_active").set(count as f64);
}

/// Set current number of pending agreements
pub fn agreements_pending_set(count: usize) {
    gauge!("icn_agreement_pending").set(count as f64);
}

/// Set current number of suspended agreements
pub fn agreements_suspended_set(count: usize) {
    gauge!("icn_agreement_suspended").set(count as f64);
}

/// Set current number of draft agreements
pub fn agreements_draft_set(count: usize) {
    gauge!("icn_agreement_draft").set(count as f64);
}

// Histogram metrics

/// Record time from proposal to activation
pub fn agreement_activation_duration_observe(duration_secs: f64) {
    histogram!("icn_agreement_activation_duration_seconds").record(duration_secs);
}

/// Record time from amendment proposal to ratification
pub fn amendment_ratification_duration_observe(duration_secs: f64) {
    histogram!("icn_agreement_amendment_ratification_duration_seconds").record(duration_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_metrics() {
        // Test agreement type counters
        agreements_created_inc("Trade");
        agreements_created_inc("Credit");
        agreements_created_inc("ResourceSharing");
        agreements_created_inc("FederationMembership");

        // Test activation counter
        agreements_activated_inc();

        // Test termination counters
        agreements_terminated_inc("Expiration");
        agreements_terminated_inc("MutualConsent");
        agreements_terminated_inc("Breach");

        // Test signature counter
        agreement_signatures_inc();

        // Test amendment counters
        amendments_proposed_inc();
        amendments_ratified_inc();

        // Test suspend/resume counters
        agreements_suspended_inc();
        agreements_resumed_inc();
    }

    #[test]
    fn test_gauge_metrics() {
        agreements_active_set(10);
        agreements_pending_set(5);
        agreements_suspended_set(2);
        agreements_draft_set(3);

        // Test zero counts
        agreements_active_set(0);
        agreements_pending_set(0);
    }

    #[test]
    fn test_histogram_metrics() {
        // Test activation duration
        agreement_activation_duration_observe(3600.0); // 1 hour
        agreement_activation_duration_observe(86400.0); // 1 day
        agreement_activation_duration_observe(0.5); // 500ms

        // Test amendment ratification duration
        amendment_ratification_duration_observe(7200.0); // 2 hours
        amendment_ratification_duration_observe(172800.0); // 2 days
    }
}
