//! Prometheus metrics for federation layer
//!
//! All federation metrics use the `icn_federation_` prefix.

use ::metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize all federation metric descriptions
pub fn init_descriptions() {
    // Registry metrics
    describe_gauge!(
        "icn_federation_coops_known",
        "Number of known federated cooperatives"
    );
    describe_counter!(
        "icn_federation_announcements_sent_total",
        "Total number of cooperative announcements sent"
    );
    describe_counter!(
        "icn_federation_announcements_received_total",
        "Total number of cooperative announcements received"
    );
    describe_counter!(
        "icn_federation_coops_registered_total",
        "Total number of cooperatives registered"
    );
    describe_counter!(
        "icn_federation_coops_removed_total",
        "Total number of cooperatives removed from registry"
    );
    describe_counter!(
        "icn_federation_vouches_sent_total",
        "Total number of vouches sent for other cooperatives"
    );
    describe_counter!(
        "icn_federation_vouches_received_total",
        "Total number of vouches received from other cooperatives"
    );
    describe_counter!(
        "icn_federation_policy_violations_total",
        "Total number of federation policy violations"
    );

    // Attestation metrics
    describe_counter!(
        "icn_federation_attestations_stored_total",
        "Total number of trust attestations stored"
    );
    describe_counter!(
        "icn_federation_attestations_expired_total",
        "Total number of trust attestations that expired"
    );
    describe_counter!(
        "icn_federation_attestations_published_total",
        "Total number of trust attestations published"
    );
    describe_counter!(
        "icn_federation_attestations_received_total",
        "Total number of trust attestations received"
    );
    describe_counter!(
        "icn_federation_attestations_rejected_total",
        "Total number of trust attestations rejected"
    );
    describe_histogram!(
        "icn_federation_attestation_trust_score",
        "Distribution of trust scores in attestations"
    );

    // Clearing/settlement metrics
    describe_gauge!(
        "icn_federation_clearing_agreements_active",
        "Number of active bilateral clearing agreements"
    );
    describe_counter!(
        "icn_federation_clearing_agreements_created_total",
        "Total number of clearing agreements created"
    );
    describe_gauge!(
        "icn_federation_transfers_pending",
        "Number of pending cross-cooperative transfers"
    );
    describe_counter!(
        "icn_federation_transfers_confirmed_total",
        "Total number of cross-cooperative transfers confirmed"
    );
    describe_counter!(
        "icn_federation_transfers_settled_total",
        "Total number of cross-cooperative transfers settled"
    );
    describe_counter!(
        "icn_federation_transfers_disputed_total",
        "Total number of cross-cooperative transfers disputed"
    );
    describe_histogram!(
        "icn_federation_transfer_amount",
        "Distribution of cross-cooperative transfer amounts"
    );
    describe_counter!(
        "icn_federation_settlements_completed_total",
        "Total number of settlement cycles completed"
    );
    describe_histogram!(
        "icn_federation_settlement_net_amount",
        "Distribution of net settlement amounts"
    );

    // Channel metrics
    describe_gauge!(
        "icn_federation_channels_active",
        "Number of active federation channels"
    );
    describe_counter!(
        "icn_federation_channels_opened_total",
        "Total number of federation channels opened"
    );
    describe_counter!(
        "icn_federation_channels_closed_total",
        "Total number of federation channels closed"
    );
    describe_counter!(
        "icn_federation_channel_messages_sent_total",
        "Total number of messages sent via federation channels"
    );
    describe_counter!(
        "icn_federation_channel_messages_received_total",
        "Total number of messages received via federation channels"
    );
    describe_histogram!(
        "icn_federation_channel_latency_seconds",
        "Latency of federation channel messages in seconds"
    );

    // DID resolution metrics
    describe_counter!(
        "icn_federation_did_resolutions_total",
        "Total number of federated DID resolutions"
    );
    describe_counter!(
        "icn_federation_did_resolution_cache_hits",
        "Total number of DID resolution cache hits"
    );
    describe_counter!(
        "icn_federation_did_resolution_cache_misses",
        "Total number of DID resolution cache misses"
    );
    describe_histogram!(
        "icn_federation_did_resolution_latency_seconds",
        "Latency of DID resolution in seconds"
    );
    describe_counter!(
        "icn_federation_did_resolution_failures_total",
        "Total number of DID resolution failures"
    );

    // Gossip routing metrics
    describe_counter!(
        "icn_federation_gossip_forwarded_total",
        "Total number of gossip messages forwarded to federation"
    );
    describe_counter!(
        "icn_federation_gossip_received_total",
        "Total number of gossip messages received from federation"
    );
    describe_counter!(
        "icn_federation_gossip_dropped_scope_total",
        "Total number of gossip messages dropped due to scope"
    );
}

/// Registry metrics
pub mod registry {
    use super::*;

    pub fn coops_known_set(count: usize) {
        gauge!("icn_federation_coops_known").set(count as f64);
    }

    pub fn announcements_sent_inc() {
        counter!("icn_federation_announcements_sent_total").increment(1);
    }

    pub fn announcements_received_inc() {
        counter!("icn_federation_announcements_received_total").increment(1);
    }

    pub fn coops_registered_inc() {
        counter!("icn_federation_coops_registered_total").increment(1);
    }

    pub fn coops_removed_inc() {
        counter!("icn_federation_coops_removed_total").increment(1);
    }

    pub fn vouches_sent_inc(target_coop: &str) {
        counter!(
            "icn_federation_vouches_sent_total",
            "target_coop" => target_coop.to_string()
        )
        .increment(1);
    }

    pub fn vouches_received_inc(voucher_coop: &str) {
        counter!(
            "icn_federation_vouches_received_total",
            "voucher_coop" => voucher_coop.to_string()
        )
        .increment(1);
    }

    pub fn policy_violations_inc(policy: &str, reason: &str) {
        counter!(
            "icn_federation_policy_violations_total",
            "policy" => policy.to_string(),
            "reason" => reason.to_string()
        )
        .increment(1);
    }
}

/// Attestation metrics
pub mod attestation {
    use super::*;

    pub fn stored_inc(source_coop: &str, context: &str) {
        counter!(
            "icn_federation_attestations_stored_total",
            "source_coop" => source_coop.to_string(),
            "context" => context.to_string()
        )
        .increment(1);
    }

    pub fn expired_inc() {
        counter!("icn_federation_attestations_expired_total").increment(1);
    }

    pub fn published_inc(context: &str) {
        counter!(
            "icn_federation_attestations_published_total",
            "context" => context.to_string()
        )
        .increment(1);
    }

    pub fn received_inc(source_coop: &str) {
        counter!(
            "icn_federation_attestations_received_total",
            "source_coop" => source_coop.to_string()
        )
        .increment(1);
    }

    pub fn rejected_inc(reason: &str) {
        counter!(
            "icn_federation_attestations_rejected_total",
            "reason" => reason.to_string()
        )
        .increment(1);
    }

    pub fn trust_score_record(score: f64) {
        histogram!("icn_federation_attestation_trust_score").record(score);
    }
}

/// Clearing and settlement metrics
pub mod clearing {
    use super::*;

    pub fn agreements_active_set(count: usize) {
        gauge!("icn_federation_clearing_agreements_active").set(count as f64);
    }

    pub fn agreements_created_inc(coop_a: &str, coop_b: &str) {
        counter!(
            "icn_federation_clearing_agreements_created_total",
            "coop_a" => coop_a.to_string(),
            "coop_b" => coop_b.to_string()
        )
        .increment(1);
    }

    pub fn transfers_pending_set(count: usize) {
        gauge!("icn_federation_transfers_pending").set(count as f64);
    }

    pub fn transfers_confirmed_inc() {
        counter!("icn_federation_transfers_confirmed_total").increment(1);
    }

    pub fn transfers_settled_inc() {
        counter!("icn_federation_transfers_settled_total").increment(1);
    }

    pub fn transfers_disputed_inc() {
        counter!("icn_federation_transfers_disputed_total").increment(1);
    }

    pub fn transfer_amount_record(amount: i64, currency: &str) {
        histogram!(
            "icn_federation_transfer_amount",
            "currency" => currency.to_string()
        )
        .record(amount.abs() as f64);
    }

    pub fn settlements_completed_inc(agreement_id: &str) {
        counter!(
            "icn_federation_settlements_completed_total",
            "agreement_id" => agreement_id.to_string()
        )
        .increment(1);
    }

    pub fn settlement_net_amount_record(amount: i64, currency: &str) {
        histogram!(
            "icn_federation_settlement_net_amount",
            "currency" => currency.to_string()
        )
        .record(amount.abs() as f64);
    }
}

/// Federation channel metrics
pub mod channel {
    use super::*;

    pub fn active_set(count: usize) {
        gauge!("icn_federation_channels_active").set(count as f64);
    }

    pub fn opened_inc(remote_coop: &str) {
        counter!(
            "icn_federation_channels_opened_total",
            "remote_coop" => remote_coop.to_string()
        )
        .increment(1);
    }

    pub fn closed_inc(remote_coop: &str, reason: &str) {
        counter!(
            "icn_federation_channels_closed_total",
            "remote_coop" => remote_coop.to_string(),
            "reason" => reason.to_string()
        )
        .increment(1);
    }

    pub fn messages_sent_inc(remote_coop: &str) {
        counter!(
            "icn_federation_channel_messages_sent_total",
            "remote_coop" => remote_coop.to_string()
        )
        .increment(1);
    }

    pub fn messages_received_inc(remote_coop: &str) {
        counter!(
            "icn_federation_channel_messages_received_total",
            "remote_coop" => remote_coop.to_string()
        )
        .increment(1);
    }

    pub fn latency_record(remote_coop: &str, latency_secs: f64) {
        histogram!(
            "icn_federation_channel_latency_seconds",
            "remote_coop" => remote_coop.to_string()
        )
        .record(latency_secs);
    }
}

/// DID resolution metrics
pub mod did_resolution {
    use super::*;

    pub fn total_inc(coop_hint: Option<&str>) {
        if let Some(coop) = coop_hint {
            counter!(
                "icn_federation_did_resolutions_total",
                "coop_hint" => coop.to_string()
            )
            .increment(1);
        } else {
            counter!("icn_federation_did_resolutions_total").increment(1);
        }
    }

    pub fn cache_hits_inc() {
        counter!("icn_federation_did_resolution_cache_hits").increment(1);
    }

    pub fn cache_misses_inc() {
        counter!("icn_federation_did_resolution_cache_misses").increment(1);
    }

    pub fn latency_record(latency_secs: f64) {
        histogram!("icn_federation_did_resolution_latency_seconds").record(latency_secs);
    }

    pub fn failures_inc(reason: &str) {
        counter!(
            "icn_federation_did_resolution_failures_total",
            "reason" => reason.to_string()
        )
        .increment(1);
    }
}

/// Gossip routing metrics
pub mod gossip_routing {
    use super::*;

    pub fn forwarded_inc(scope: &str) {
        counter!(
            "icn_federation_gossip_forwarded_total",
            "scope" => scope.to_string()
        )
        .increment(1);
    }

    pub fn received_inc(from_coop: &str) {
        counter!(
            "icn_federation_gossip_received_total",
            "from_coop" => from_coop.to_string()
        )
        .increment(1);
    }

    pub fn dropped_scope_inc(scope: &str, reason: &str) {
        counter!(
            "icn_federation_gossip_dropped_scope_total",
            "scope" => scope.to_string(),
            "reason" => reason.to_string()
        )
        .increment(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_descriptions() {
        // Should not panic
        init_descriptions();
    }
}
