//! RPC metrics
//!
//! Metrics for RPC authentication and token revocation.

use metrics::{describe_counter, describe_gauge, describe_histogram};

/// Initialize RPC metric descriptions
pub fn init_descriptions() {
    // Token revocation metrics
    describe_counter!(
        "icn_rpc_revocation_checks_total",
        "Total number of token revocation checks performed"
    );
    describe_counter!(
        "icn_rpc_revocation_hits_total",
        "Total number of revocation checks that found a revoked token (cache hits)"
    );
    describe_gauge!(
        "icn_rpc_revoked_tokens_total",
        "Current number of revoked tokens in the cache"
    );
    describe_histogram!(
        "icn_rpc_revocation_load_duration_seconds",
        "Time taken to load token revocation list from storage on startup"
    );
    describe_histogram!(
        "icn_rpc_revocation_cleanup_duration_seconds",
        "Time taken to clean up expired revocations"
    );
    describe_counter!(
        "icn_rpc_revocation_cleanup_total",
        "Total number of expired revocation entries cleaned up"
    );
    describe_gauge!(
        "icn_rpc_revocation_scan_entries",
        "Number of entries scanned during the last cleanup operation"
    );
    describe_histogram!(
        "icn_rpc_revocation_add_duration_seconds",
        "Time taken to add a token to the revocation list"
    );
}
