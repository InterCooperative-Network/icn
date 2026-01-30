//! Service discovery metrics
//!
//! Metrics for service endpoint announcements, withdrawals, discovery queries,
//! and registry health.

use metrics::{counter, describe_counter, describe_gauge, gauge};

/// Initialize service discovery metric descriptions
pub fn init_descriptions() {
    describe_counter!(
        "icn_service_discovery_announcements_total",
        "Total number of service endpoint announcements"
    );
    describe_counter!(
        "icn_service_discovery_withdrawals_total",
        "Total number of service endpoint withdrawals"
    );
    describe_counter!(
        "icn_service_discovery_expired_removed_total",
        "Total number of expired endpoints removed by background cleanup"
    );
    describe_gauge!(
        "icn_service_discovery_registry_size",
        "Current number of service endpoints in the registry"
    );
    describe_counter!(
        "icn_service_discovery_queries_total",
        "Total number of service discovery queries"
    );
    describe_counter!(
        "icn_service_discovery_registry_full_rejections_total",
        "Total number of announcements rejected because the registry is full"
    );
}

/// Increment service announcements counter
pub fn announcements_inc() {
    counter!("icn_service_discovery_announcements_total").increment(1);
}

/// Increment service withdrawals counter
pub fn withdrawals_inc() {
    counter!("icn_service_discovery_withdrawals_total").increment(1);
}

/// Add to expired-removed counter
pub fn expired_removed_add(count: u64) {
    counter!("icn_service_discovery_expired_removed_total").increment(count);
}

/// Set current registry size gauge
pub fn registry_size_set(count: u64) {
    gauge!("icn_service_discovery_registry_size").set(count as f64);
}

/// Increment discovery queries counter
pub fn queries_inc() {
    counter!("icn_service_discovery_queries_total").increment(1);
}

/// Increment registry-full rejections counter
pub fn registry_full_rejections_inc() {
    counter!("icn_service_discovery_registry_full_rejections_total").increment(1);
}
