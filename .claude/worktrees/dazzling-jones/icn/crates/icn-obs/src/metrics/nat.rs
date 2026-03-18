//! NAT traversal metrics
//!
//! Metrics for STUN/TURN discovery, NAT dial attempts, success rates, and connection types.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize NAT metric descriptions
pub fn init_descriptions() {
    // Dial strategy metrics
    describe_counter!(
        "icn_nat_dial_attempts_total",
        "Total number of NAT dial attempts by address type"
    );
    describe_counter!(
        "icn_nat_dial_success_total",
        "Total number of successful NAT dial attempts by address type"
    );
    describe_counter!(
        "icn_nat_dial_failure_total",
        "Total number of failed NAT dial attempts (all address types exhausted)"
    );
    describe_gauge!(
        "icn_nat_dial_latency_seconds",
        "Last dial latency by address type"
    );

    // STUN discovery metrics
    describe_counter!(
        "icn_stun_discovery_total",
        "Total number of STUN discovery attempts by result"
    );
    describe_histogram!(
        "icn_stun_discovery_duration_seconds",
        "Time taken for STUN server discovery"
    );

    // TURN allocation metrics
    describe_counter!(
        "icn_turn_allocations_total",
        "Total number of TURN allocations requested"
    );
    describe_counter!(
        "icn_turn_allocation_failures_total",
        "Total number of TURN allocation failures by reason"
    );
    describe_counter!(
        "icn_turn_permission_refreshes_total",
        "Total number of TURN permission refresh operations"
    );
    describe_counter!(
        "icn_turn_relay_send_total",
        "Total number of TURN relay data send operations"
    );
    describe_counter!(
        "icn_turn_relay_recv_total",
        "Total number of TURN relay data receive operations"
    );
}

// Dial strategy metrics

/// Increment dial attempt counter for address type
///
/// Types: "local", "public", "relay", "total"
pub fn dial_attempt_inc(addr_type: &str) {
    counter!(
        "icn_nat_dial_attempts_total",
        "type" => addr_type.to_string()
    )
    .increment(1);
}

/// Increment dial success counter for address type
///
/// Types: "local", "public", "relay"
pub fn dial_success_inc(addr_type: &str) {
    counter!(
        "icn_nat_dial_success_total",
        "type" => addr_type.to_string()
    )
    .increment(1);
}

/// Increment dial failure counter (all address types exhausted)
pub fn dial_failure_inc() {
    counter!("icn_nat_dial_failure_total").increment(1);
}

/// Set dial latency gauge for address type
///
/// Types: "local", "public", "relay"
pub fn dial_latency_set(addr_type: &str, seconds: f64) {
    gauge!(
        "icn_nat_dial_latency_seconds",
        "type" => addr_type.to_string()
    )
    .set(seconds);
}

// STUN discovery metrics

/// Increment STUN discovery counter with result
///
/// Results: "success", "timeout", "error"
pub fn stun_discovery_inc(result: &str) {
    counter!(
        "icn_stun_discovery_total",
        "result" => result.to_string()
    )
    .increment(1);
}

/// Record STUN discovery duration
pub fn stun_discovery_duration_record(duration_secs: f64) {
    histogram!("icn_stun_discovery_duration_seconds").record(duration_secs);
}

// TURN allocation metrics

/// Increment TURN allocation counter
pub fn turn_allocation_inc() {
    counter!("icn_turn_allocations_total").increment(1);
}

/// Increment TURN allocation failure counter with reason
///
/// Reasons: "auth_failed", "timeout", "no_server", "protocol_error"
pub fn turn_allocation_failure_inc(reason: &str) {
    counter!(
        "icn_turn_allocation_failures_total",
        "reason" => reason.to_string()
    )
    .increment(1);
}

/// Increment TURN permission refresh counter
pub fn turn_permission_refresh_inc() {
    counter!("icn_turn_permission_refreshes_total").increment(1);
}

/// Increment TURN relay send counter
pub fn turn_relay_send_inc() {
    counter!("icn_turn_relay_send_total").increment(1);
}

/// Increment TURN relay receive counter
pub fn turn_relay_recv_inc() {
    counter!("icn_turn_relay_recv_total").increment(1);
}
