//! Network metrics
//!
//! Metrics for network connections, messages, and peer discovery.

use metrics::{counter, describe_counter, describe_gauge, gauge};

/// Initialize network metric descriptions
pub fn init_descriptions() {
    describe_counter!(
        "icn_network_connections_total",
        "Total number of network connections established"
    );
    describe_gauge!(
        "icn_network_connections_active",
        "Current number of active connections"
    );
    describe_counter!(
        "icn_network_messages_sent_total",
        "Total number of messages sent"
    );
    describe_counter!(
        "icn_network_messages_received_total",
        "Total number of messages received"
    );
    describe_counter!(
        "icn_network_bytes_sent_total",
        "Total bytes sent over network"
    );
    describe_counter!(
        "icn_network_bytes_received_total",
        "Total bytes received from network"
    );
    describe_gauge!(
        "icn_network_peers_discovered",
        "Number of peers discovered via mDNS"
    );
    describe_counter!(
        "icn_network_messages_rate_limited_total",
        "Total number of messages dropped due to rate limiting"
    );
    describe_counter!(
        "icn_network_messages_rate_limited_by_class_total",
        "Total number of messages rate limited by trust class"
    );
    describe_gauge!(
        "icn_network_active_peers_by_class",
        "Number of active peers by trust class"
    );
    describe_counter!(
        "icn_network_trust_class_changes_total",
        "Total number of peer trust class changes affecting rate limits"
    );
    describe_counter!(
        "icn_network_connections_rejected_untrusted_total",
        "Total number of connections rejected due to insufficient trust"
    );
    describe_counter!(
        "icn_network_connections_rejected_by_class_total",
        "Total number of connections rejected by trust class"
    );
    describe_counter!(
        "icn_network_protocol_version_mismatch_total",
        "Total number of messages rejected due to protocol version mismatch"
    );
    describe_counter!(
        "icn_network_protocol_version_too_old_total",
        "Total number of messages rejected because version is too old"
    );
    describe_counter!(
        "icn_network_protocol_version_too_new_total",
        "Total number of messages rejected because version is too new"
    );
    describe_gauge!(
        "icn_network_peer_versions",
        "Number of peers running each protocol version"
    );
    describe_gauge!(
        "icn_network_peer_capabilities",
        "Number of peers supporting each capability"
    );
    describe_counter!(
        "icn_network_version_negotiation_failures_total",
        "Total number of version negotiation failures by reason"
    );
    describe_counter!(
        "icn_network_version_negotiation_success_total",
        "Total number of successful version negotiations"
    );
    describe_gauge!(
        "icn_network_replay_guard_peers",
        "Number of peers tracked in replay guard"
    );
    // E2E Encryption metrics (Issue #404)
    describe_counter!(
        "icn_network_encrypted_messages_sent_total",
        "Total number of E2E encrypted messages sent"
    );
    describe_counter!(
        "icn_network_encryption_failed_total",
        "Total number of messages dropped due to encryption failure (fail-closed)"
    );
    describe_counter!(
        "icn_network_encryption_rejected_total",
        "Total number of encrypted messages rejected by reason"
    );
    describe_counter!(
        "icn_network_encryption_sequence_cleanup_failed_total",
        "Total number of encryption sequence cleanup failures"
    );
    describe_gauge!(
        "icn_network_encryption_sequence_pairs",
        "Current number of active (sender, recipient) encryption sequence pairs being tracked"
    );
}

// Simple counters
pub fn connections_total_inc() {
    counter!("icn_network_connections_total").increment(1);
}

pub fn messages_sent_inc() {
    counter!("icn_network_messages_sent_total").increment(1);
}

pub fn messages_received_inc() {
    counter!("icn_network_messages_received_total").increment(1);
}

pub fn messages_rate_limited_inc() {
    counter!("icn_network_messages_rate_limited_total").increment(1);
}

pub fn trust_class_changes_inc() {
    counter!("icn_network_trust_class_changes_total").increment(1);
}

pub fn protocol_version_mismatch_inc() {
    counter!("icn_network_protocol_version_mismatch_total").increment(1);
}

pub fn protocol_version_too_old_inc() {
    counter!("icn_network_protocol_version_too_old_total").increment(1);
}

pub fn protocol_version_too_new_inc() {
    counter!("icn_network_protocol_version_too_new_total").increment(1);
}

// Counters with value
pub fn bytes_sent_add(bytes: u64) {
    counter!("icn_network_bytes_sent_total").increment(bytes);
}

pub fn bytes_received_add(bytes: u64) {
    counter!("icn_network_bytes_received_total").increment(bytes);
}

// Simple gauges
pub fn connections_active_set(value: u64) {
    gauge!("icn_network_connections_active").set(value as f64);
}

pub fn peers_discovered_set(value: u64) {
    gauge!("icn_network_peers_discovered").set(value as f64);
}

// Labeled counters
pub fn messages_rate_limited_by_class_inc(trust_class: &str) {
    counter!(
        "icn_network_messages_rate_limited_by_class_total",
        "class" => trust_class.to_string()
    )
    .increment(1);
}

pub fn version_negotiation_failure_inc(reason: &str) {
    counter!(
        "icn_network_version_negotiation_failures_total",
        "reason" => reason.to_string()
    )
    .increment(1);
}

pub fn version_negotiation_success_inc(negotiated_version: u32) {
    counter!(
        "icn_network_version_negotiation_success_total",
        "negotiated_version" => negotiated_version.to_string()
    )
    .increment(1);
}

// Labeled gauges
pub fn active_peers_by_class_set(trust_class: &str, count: u64) {
    gauge!(
        "icn_network_active_peers_by_class",
        "class" => trust_class.to_string()
    )
    .set(count as f64);
}

pub fn peer_version_set(version: u32, count: u64) {
    gauge!(
        "icn_network_peer_versions",
        "version" => version.to_string()
    )
    .set(count as f64);
}

pub fn peer_capability_set(capability: &str, count: u64) {
    gauge!(
        "icn_network_peer_capabilities",
        "capability" => capability.to_string()
    )
    .set(count as f64);
}

/// Set the number of peers tracked in replay guard
pub fn replay_guard_peers_set(value: u64) {
    gauge!("icn_network_replay_guard_peers").set(value as f64);
}

// E2E Encryption metrics (Issue #404)

/// Increment encrypted messages sent counter
pub fn encrypted_messages_sent_inc() {
    counter!("icn_network_encrypted_messages_sent_total").increment(1);
}

/// Increment encryption failed counter with reason (fail-closed: message dropped)
///
/// Reasons: "encryption_error", "peer_key_missing", "serialization_failed"
pub fn encryption_failed_inc(reason: &str) {
    counter!(
        "icn_network_encryption_failed_total",
        "reason" => reason.to_string()
    )
    .increment(1);
}

/// Increment encrypted message rejection counter with reason
///
/// Reasons: "missing_peer_key", "wrong_recipient", "decryption_failed", "invalid_inner_signature"
pub fn encryption_rejected_inc(reason: &str) {
    counter!(
        "icn_network_encryption_rejected_total",
        "reason" => reason.to_string()
    )
    .increment(1);
}

/// Increment encryption sequence cleanup failure counter
///
/// This metric helps operators detect storage issues before they cause problems.
pub fn encryption_sequence_cleanup_failed_inc() {
    counter!("icn_network_encryption_sequence_cleanup_failed_total").increment(1);
}

/// Set the current number of encryption sequence pairs being tracked
///
/// This gauge helps operators monitor memory usage in the sequence tracker.
/// The value represents active (sender, recipient) pairs with sequence numbers.
pub fn encryption_sequence_pairs_set(count: u64) {
    gauge!("icn_network_encryption_sequence_pairs").set(count as f64);
}

// Complex function with custom logic
pub fn connections_rejected_untrusted_inc(peer_did: &str, trust_score: f64) {
    counter!(
        "icn_network_connections_rejected_untrusted_total",
        "peer_did" => peer_did.to_string(),
        "trust_score" => format!("{:.3}", trust_score)
    )
    .increment(1);

    // Also increment by trust class for aggregated metrics
    let trust_class = if trust_score < 0.1 {
        "isolated"
    } else if trust_score < 0.4 {
        "known"
    } else if trust_score < 0.7 {
        "partner"
    } else {
        "federated"
    };

    counter!(
        "icn_network_connections_rejected_by_class_total",
        "class" => trust_class.to_string()
    )
    .increment(1);
}
