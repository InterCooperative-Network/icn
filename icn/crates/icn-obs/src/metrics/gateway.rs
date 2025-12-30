//! Gateway metrics
//!
//! Metrics for WebSocket connections, event broadcasting, and backpressure.

use metrics::{counter, describe_counter, describe_gauge, gauge};

/// Initialize gateway metric descriptions
pub fn init_descriptions() {
    describe_gauge!(
        "icn_gateway_websocket_connections_active",
        "Current number of active WebSocket connections"
    );
    describe_counter!(
        "icn_gateway_websocket_connections_total",
        "Total number of WebSocket connections established"
    );
    describe_counter!(
        "icn_gateway_websocket_slow_client_disconnects_total",
        "Total number of slow clients disconnected due to full channels"
    );
    describe_gauge!(
        "icn_gateway_websocket_subscribers_per_coop",
        "Number of subscribers per cooperative"
    );
    describe_counter!(
        "icn_gateway_websocket_events_broadcast_total",
        "Total number of events broadcast"
    );
    describe_counter!(
        "icn_gateway_websocket_backfill_requests_total",
        "Total number of backfill requests"
    );
    describe_counter!(
        "icn_gateway_websocket_subscriber_limit_rejections_total",
        "Total number of subscription rejections due to subscriber limit"
    );
    describe_counter!(
        "icn_gateway_websocket_disconnections_total",
        "Total number of WebSocket disconnections"
    );
    describe_counter!(
        "icn_gateway_websocket_messages_sent_total",
        "Total number of WebSocket messages sent to clients"
    );
}

// Connection metrics

/// Set active WebSocket connections gauge
pub fn websocket_connections_active_set(count: u64) {
    gauge!("icn_gateway_websocket_connections_active").set(count as f64);
}

/// Increment active WebSocket connections
pub fn websocket_connections_active_inc() {
    let g = gauge!("icn_gateway_websocket_connections_active");
    g.increment(1.0);
}

/// Decrement active WebSocket connections
pub fn websocket_connections_active_dec() {
    let g = gauge!("icn_gateway_websocket_connections_active");
    g.decrement(1.0);
}

/// Increment total WebSocket connections (alias for consistency)
pub fn websocket_connections_inc() {
    counter!("icn_gateway_websocket_connections_total").increment(1);
}

/// Increment total WebSocket connections
pub fn websocket_connections_total_inc() {
    counter!("icn_gateway_websocket_connections_total").increment(1);
}

/// Increment WebSocket disconnections counter
pub fn websocket_disconnections_inc() {
    counter!("icn_gateway_websocket_disconnections_total").increment(1);
}

/// Increment messages sent counter
pub fn websocket_messages_sent_inc() {
    counter!("icn_gateway_websocket_messages_sent_total").increment(1);
}

// Backpressure metrics

/// Increment slow client disconnect counter
pub fn websocket_slow_client_disconnects_inc() {
    counter!("icn_gateway_websocket_slow_client_disconnects_total").increment(1);
}

/// Set subscribers count for a cooperative
pub fn websocket_subscribers_per_coop_set(coop_id: &str, count: u64) {
    gauge!(
        "icn_gateway_websocket_subscribers_per_coop",
        "coop_id" => coop_id.to_string()
    )
    .set(count as f64);
}

// Event metrics

/// Increment events broadcast counter
pub fn websocket_events_broadcast_inc(coop_id: &str) {
    counter!(
        "icn_gateway_websocket_events_broadcast_total",
        "coop_id" => coop_id.to_string()
    )
    .increment(1);
}

/// Increment backfill requests counter
pub fn websocket_backfill_requests_inc(coop_id: &str) {
    counter!(
        "icn_gateway_websocket_backfill_requests_total",
        "coop_id" => coop_id.to_string()
    )
    .increment(1);
}

/// Increment subscriber limit rejection counter
pub fn websocket_subscriber_limit_rejections_inc(coop_id: &str) {
    counter!(
        "icn_gateway_websocket_subscriber_limit_rejections_total",
        "coop_id" => coop_id.to_string()
    )
    .increment(1);
}

// Entity audit metrics

/// Increment entity audit pruned counter
pub fn entity_audit_pruned_inc(count: usize) {
    counter!("icn_gateway_entity_audit_pruned_total").increment(count as u64);
}

/// Record entity audit prune duration
pub fn entity_audit_prune_duration(duration_secs: f64) {
    gauge!("icn_gateway_entity_audit_prune_duration_seconds").set(duration_secs);
}

/// Increment entity audit prune failure counter
pub fn entity_audit_prune_failed_inc() {
    counter!("icn_gateway_entity_audit_prune_failures_total").increment(1);
}

// Rate limiting metrics

/// Increment rate limit exceeded counter
pub fn rate_limit_exceeded_inc(identifier: &str) {
    counter!(
        "icn_gateway_rate_limit_exceeded_total",
        "identifier" => identifier.to_string()
    )
    .increment(1);
}

/// Increment IP rate limit exceeded counter
pub fn ip_rate_limit_exceeded_inc() {
    counter!("icn_gateway_ip_rate_limit_exceeded_total").increment(1);
}

/// Increment velocity limit exceeded counter
pub fn velocity_limit_exceeded_inc() {
    counter!("icn_gateway_velocity_limit_exceeded_total").increment(1);
}

// Auth metrics

/// Increment auth challenges counter
pub fn auth_challenges_inc() {
    counter!("icn_gateway_auth_challenges_total").increment(1);
}

/// Increment auth verifications counter
pub fn auth_verifications_inc() {
    counter!("icn_gateway_auth_verifications_total").increment(1);
}

/// Increment auth failures counter
pub fn auth_failures_inc(reason: &str) {
    counter!(
        "icn_gateway_auth_failures_total",
        "reason" => reason.to_string()
    )
    .increment(1);
}

/// Increment auth successes counter
pub fn auth_successes_inc() {
    counter!("icn_gateway_auth_successes_total").increment(1);
}

/// Increment authorization failures counter
pub fn authorization_failures_inc(required_scope: &str) {
    counter!(
        "icn_gateway_authorization_failures_total",
        "required_scope" => required_scope.to_string()
    )
    .increment(1);
}

// Coop metrics

/// Increment coops created counter
pub fn coops_created_inc() {
    counter!("icn_gateway_coops_created_total").increment(1);
}

/// Increment coops deleted counter
pub fn coops_deleted_inc() {
    counter!("icn_gateway_coops_deleted_total").increment(1);
}

/// Increment members added counter
pub fn members_added_inc() {
    counter!("icn_gateway_members_added_total").increment(1);
}

/// Increment members removed counter
pub fn members_removed_inc() {
    counter!("icn_gateway_members_removed_total").increment(1);
}

/// Increment stats accessed counter
pub fn stats_accessed_inc() {
    counter!("icn_gateway_stats_accessed_total").increment(1);
}

// Ledger metrics

/// Increment payments created counter
pub fn payments_created_inc() {
    counter!("icn_gateway_payments_created_total").increment(1);
}

/// Record payment amount
pub fn payment_amount_record(currency: &str, amount: i64) {
    counter!(
        "icn_gateway_payment_amount_total",
        "currency" => currency.to_string()
    )
    .increment(amount.unsigned_abs());
}

/// Increment balance queries counter
pub fn balance_queries_inc() {
    counter!("icn_gateway_balance_queries_total").increment(1);
}

/// Increment history queries counter
pub fn history_queries_inc() {
    counter!("icn_gateway_history_queries_total").increment(1);
}

// Governance metrics

/// Increment governance domains created counter
pub fn governance_domains_created_inc() {
    counter!("icn_gateway_governance_domains_created_total").increment(1);
}

/// Increment governance proposals created counter
pub fn governance_proposals_created_inc() {
    counter!("icn_gateway_governance_proposals_created_total").increment(1);
}

/// Increment governance proposals opened counter
pub fn governance_proposals_opened_inc() {
    counter!("icn_gateway_governance_proposals_opened_total").increment(1);
}

/// Increment governance proposals closed counter
pub fn governance_proposals_closed_inc() {
    counter!("icn_gateway_governance_proposals_closed_total").increment(1);
}

/// Increment governance votes cast counter
pub fn governance_votes_cast_inc() {
    counter!("icn_gateway_governance_votes_cast_total").increment(1);
}

// Invite metrics

/// Increment invites created counter
pub fn invites_created(coop_id: &str) {
    counter!(
        "icn_gateway_invites_created_total",
        "coop_id" => coop_id.to_string()
    )
    .increment(1);
}

/// Increment invites used counter
pub fn invites_used(coop_id: &str) {
    counter!(
        "icn_gateway_invites_used_total",
        "coop_id" => coop_id.to_string()
    )
    .increment(1);
}

// Request metrics

/// Increment requests total counter
pub fn requests_total_inc(endpoint: &str, method: &str) {
    counter!(
        "icn_gateway_requests_total",
        "endpoint" => endpoint.to_string(),
        "method" => method.to_string()
    )
    .increment(1);
}

/// Record request duration
pub fn request_duration_record(endpoint: &str, status: u16, duration_secs: f64) {
    gauge!(
        "icn_gateway_request_duration_seconds",
        "endpoint" => endpoint.to_string(),
        "status" => status.to_string()
    )
    .set(duration_secs);
}

// Entity audit metrics (additional)

/// Increment entity audit record counter
pub fn entity_audit_record_inc(operation: &str) {
    counter!(
        "icn_gateway_entity_audit_records_total",
        "operation" => operation.to_string()
    )
    .increment(1);
}

/// Record entity audit query duration
pub fn entity_audit_query_duration(duration_secs: f64) {
    gauge!("icn_gateway_entity_audit_query_duration_seconds").set(duration_secs);
}

/// Increment entity audit rollback failure counter
pub fn entity_audit_rollback_failure_inc(endpoint: &str) {
    counter!(
        "icn_gateway_entity_audit_rollback_failures_total",
        "endpoint" => endpoint.to_string()
    )
    .increment(1);
}

/// Increment entity audit corruption counter
pub fn entity_audit_corruption_inc() {
    counter!("icn_gateway_entity_audit_corruption_total").increment(1);
}

// Oracle metrics

/// Increment oracle rate queries counter
pub fn oracle_rate_queries_inc(pair: &str) {
    counter!(
        "icn_gateway_oracle_rate_queries_total",
        "pair" => pair.to_string()
    )
    .increment(1);
}

/// Increment oracle cache hits counter
pub fn oracle_cache_hits_inc() {
    counter!("icn_gateway_oracle_cache_hits_total").increment(1);
}

/// Increment oracle cache misses counter
pub fn oracle_cache_misses_inc() {
    counter!("icn_gateway_oracle_cache_misses_total").increment(1);
}

/// Increment oracle source errors counter
pub fn oracle_source_errors_inc(source: &str) {
    counter!(
        "icn_gateway_oracle_source_errors_total",
        "source" => source.to_string()
    )
    .increment(1);
}

/// Increment oracle rate updates counter
pub fn oracle_rate_updates_inc(pair: &str, source: &str) {
    counter!(
        "icn_gateway_oracle_rate_updates_total",
        "pair" => pair.to_string(),
        "source" => source.to_string()
    )
    .increment(1);
}

/// Increment oracle conversions counter
pub fn oracle_conversions_inc(from: &str, to: &str) {
    counter!(
        "icn_gateway_oracle_conversions_total",
        "from" => from.to_string(),
        "to" => to.to_string()
    )
    .increment(1);
}

/// Increment oracle stale rate returns counter
pub fn oracle_stale_rates_inc(pair: &str) {
    counter!(
        "icn_gateway_oracle_stale_rates_total",
        "pair" => pair.to_string()
    )
    .increment(1);
}

/// Increment oracle outliers filtered counter
pub fn oracle_outliers_filtered_inc(pair: &str, count: usize) {
    counter!(
        "icn_gateway_oracle_outliers_filtered_total",
        "pair" => pair.to_string()
    )
    .increment(count as u64);
}

/// Set oracle cached pairs gauge
pub fn oracle_cached_pairs_set(count: usize) {
    gauge!("icn_gateway_oracle_cached_pairs").set(count as f64);
}
