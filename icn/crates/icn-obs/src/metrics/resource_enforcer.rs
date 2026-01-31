//! Resource access enforcer metrics
//!
//! Metrics for monitoring the resource access enforcement actor,
//! including enforcement checks, revocations, and gossip publication.

use metrics::{counter, describe_counter};

/// Initialize resource enforcer metric descriptions
pub fn init_descriptions() {
    describe_counter!(
        "icn_resource_enforcer_checks_total",
        "Total number of enforcement check cycles performed"
    );
    describe_counter!(
        "icn_resource_enforcer_resources_checked_total",
        "Total number of resources evaluated during enforcement checks"
    );
    describe_counter!(
        "icn_resource_enforcer_revocations_total",
        "Total number of enforcement cycles that produced revocations"
    );
    describe_counter!(
        "icn_resource_access_revoked_total",
        "Total number of individual resource access entries revoked"
    );
    describe_counter!(
        "icn_resource_revocation_gossip_published_total",
        "Total number of revocation events successfully published to gossip"
    );
    // Labels: reason={serialization, publish}
    describe_counter!(
        "icn_resource_revocation_gossip_failures_total",
        "Total number of failed attempts to publish revocation events to gossip (by reason label)"
    );
}

/// Increment the enforcement checks counter
pub fn checks_total_inc() {
    counter!("icn_resource_enforcer_checks_total").increment(1);
}

/// Increment resources checked counter
pub fn resources_checked_inc(count: u64) {
    counter!("icn_resource_enforcer_resources_checked_total").increment(count);
}

/// Increment revocations counter
pub fn revocations_total_inc(count: u64) {
    counter!("icn_resource_enforcer_revocations_total").increment(count);
}

/// Increment individual access revoked counter
pub fn access_revoked_inc() {
    counter!("icn_resource_access_revoked_total").increment(1);
}

/// Increment gossip published counter
pub fn gossip_published_inc() {
    counter!("icn_resource_revocation_gossip_published_total").increment(1);
}

/// Increment gossip failure counter with reason label
pub fn gossip_failure_inc(reason: &str) {
    counter!("icn_resource_revocation_gossip_failures_total", "reason" => reason.to_string())
        .increment(1);
}
