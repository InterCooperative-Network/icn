//! Metrics definitions for ICN components

use metrics::{describe_counter, describe_gauge, describe_histogram};

/// Initialize all metric descriptions
pub fn init_descriptions() {
    // Network metrics
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

    // Gossip metrics
    describe_gauge!(
        "icn_gossip_topics_total",
        "Total number of gossip topics"
    );
    describe_gauge!(
        "icn_gossip_entries_total",
        "Total number of gossip entries across all topics"
    );
    describe_counter!(
        "icn_gossip_entries_published_total",
        "Total number of entries published"
    );
    describe_counter!(
        "icn_gossip_entries_received_total",
        "Total number of entries received from peers"
    );
    describe_counter!(
        "icn_gossip_announces_sent_total",
        "Total number of Announce messages sent"
    );
    describe_counter!(
        "icn_gossip_requests_sent_total",
        "Total number of Request messages sent"
    );
    describe_counter!(
        "icn_gossip_responses_sent_total",
        "Total number of Response messages sent"
    );
    describe_counter!(
        "icn_gossip_announces_received_total",
        "Total number of Announce messages received"
    );
    describe_counter!(
        "icn_gossip_requests_received_total",
        "Total number of Request messages received"
    );
    describe_counter!(
        "icn_gossip_responses_received_total",
        "Total number of Response messages received"
    );
    describe_gauge!(
        "icn_gossip_subscriptions_total",
        "Total number of active subscriptions across all topics"
    );
    describe_counter!(
        "icn_gossip_subscriptions_rejected_total",
        "Total number of subscriptions rejected due to insufficient trust"
    );
    describe_counter!(
        "icn_gossip_subscribes_received_total",
        "Total number of Subscribe messages received"
    );
    describe_counter!(
        "icn_gossip_unsubscribes_received_total",
        "Total number of Unsubscribe messages received"
    );
    describe_counter!(
        "icn_gossip_subscribe_acks_sent_total",
        "Total number of SubscribeAck messages sent"
    );
    describe_counter!(
        "icn_gossip_digests_sent_total",
        "Total number of Digest messages sent"
    );
    describe_counter!(
        "icn_gossip_digests_received_total",
        "Total number of Digest messages received"
    );
    describe_counter!(
        "icn_gossip_pull_requests_sent_total",
        "Total number of PullRequest messages sent"
    );
    describe_counter!(
        "icn_gossip_pull_requests_received_total",
        "Total number of PullRequest messages received"
    );
    describe_counter!(
        "icn_gossip_pull_responses_sent_total",
        "Total number of PullResponse messages sent"
    );
    describe_counter!(
        "icn_gossip_pull_responses_received_total",
        "Total number of PullResponse messages received"
    );
    describe_counter!(
        "icn_gossip_bytes_pulled_total",
        "Total bytes received via pull protocol"
    );
    describe_counter!(
        "icn_gossip_bytes_pushed_total",
        "Total bytes sent via push protocol"
    );
    describe_gauge!(
        "icn_gossip_peer_deficit_bytes",
        "Current deficit bytes per peer (negative means backpressure debt)"
    );
    describe_histogram!(
        "icn_gossip_bloom_fp_rate",
        "Bloom filter false positive rate by topic"
    );
    describe_counter!(
        "icn_gossip_pull_truncated_total",
        "Total number of pull responses truncated due to size limits"
    );

    // Ledger metrics
    describe_gauge!(
        "icn_ledger_accounts_total",
        "Total number of accounts in ledger"
    );
    describe_gauge!(
        "icn_ledger_currencies_total",
        "Total number of currencies in ledger"
    );
    describe_counter!(
        "icn_ledger_transactions_total",
        "Total number of transactions"
    );
    describe_histogram!(
        "icn_ledger_transaction_amount",
        "Distribution of transaction amounts"
    );
    describe_counter!(
        "icn_ledger_merge_conflicts_total",
        "Total number of merge conflicts detected"
    );
    describe_counter!(
        "icn_ledger_entries_quarantined_total",
        "Total number of entries quarantined"
    );
    describe_counter!(
        "icn_ledger_entries_discarded_total",
        "Total number of entries discarded during merge"
    );
    describe_gauge!(
        "icn_ledger_quarantine_size",
        "Current number of entries in quarantine"
    );

    // Trust graph metrics
    describe_gauge!(
        "icn_trust_edges_total",
        "Total number of trust edges in the graph"
    );
    describe_gauge!(
        "icn_trust_peers_by_class",
        "Number of peers by trust class"
    );
    describe_counter!(
        "icn_trust_lookups_total",
        "Total number of trust score lookups"
    );
    describe_counter!(
        "icn_trust_cache_hits_total",
        "Total number of trust cache hits"
    );
    describe_counter!(
        "icn_trust_cache_misses_total",
        "Total number of trust cache misses"
    );
    describe_histogram!(
        "icn_trust_score_distribution",
        "Distribution of trust scores"
    );
    describe_counter!(
        "icn_trust_attestations_received_total",
        "Total number of trust attestations received from network"
    );
    describe_counter!(
        "icn_trust_attestations_broadcasted_total",
        "Total number of trust attestations broadcasted to network"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_expired_total",
        "Total number of attestations rejected due to expiration"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_invalid_signature_total",
        "Total number of attestations rejected due to invalid signature"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_outdated_total",
        "Total number of attestations rejected as outdated (older than existing)"
    );
    describe_counter!(
        "icn_trust_attestations_rejected_rate_limited_total",
        "Total number of attestations rejected due to rate limiting"
    );
    describe_counter!(
        "icn_trust_attestations_new_total",
        "Total number of new trust edges created from attestations"
    );
    describe_counter!(
        "icn_trust_attestations_updated_total",
        "Total number of existing trust edges updated from attestations"
    );

    // Contract metrics
    describe_gauge!(
        "icn_contract_installed_total",
        "Total number of installed contracts"
    );
    describe_counter!(
        "icn_contract_deployments_total",
        "Total number of contract deployments initiated"
    );
    describe_counter!(
        "icn_contract_deployments_received_total",
        "Total number of contract deployments received from network"
    );
    describe_counter!(
        "icn_contract_deployments_rejected_total",
        "Total number of contract deployments rejected"
    );
    describe_counter!(
        "icn_contract_deployments_rejected_trust_total",
        "Total number of contract deployments rejected due to insufficient trust"
    );
    describe_counter!(
        "icn_contract_deployments_rejected_signature_total",
        "Total number of contract deployments rejected due to invalid signatures"
    );
    describe_counter!(
        "icn_contract_executions_total",
        "Total number of contract rule executions"
    );
    describe_counter!(
        "icn_contract_executions_failed_total",
        "Total number of failed contract executions"
    );
    describe_counter!(
        "icn_contract_executions_rejected_unauthorized_total",
        "Total number of contract executions rejected due to unauthorized caller"
    );
    describe_histogram!(
        "icn_contract_execution_fuel_used",
        "Distribution of fuel consumed during contract execution"
    );
    describe_histogram!(
        "icn_contract_execution_duration_seconds",
        "Duration of contract rule executions in seconds"
    );
    describe_counter!(
        "icn_contract_ledger_operations_total",
        "Total number of ledger operations from contract execution"
    );

    // System metrics
    describe_gauge!(
        "icn_system_uptime_seconds",
        "System uptime in seconds"
    );
    describe_gauge!(
        "icn_system_actors_active",
        "Number of active actors"
    );
}

/// Network metrics
pub mod network {
    use metrics::{counter, gauge};

    pub fn connections_total_inc() {
        counter!("icn_network_connections_total").increment(1);
    }

    pub fn connections_active_set(value: u64) {
        gauge!("icn_network_connections_active").set(value as f64);
    }

    pub fn messages_sent_inc() {
        counter!("icn_network_messages_sent_total").increment(1);
    }

    pub fn messages_received_inc() {
        counter!("icn_network_messages_received_total").increment(1);
    }

    pub fn bytes_sent_add(bytes: u64) {
        counter!("icn_network_bytes_sent_total").increment(bytes);
    }

    pub fn bytes_received_add(bytes: u64) {
        counter!("icn_network_bytes_received_total").increment(bytes);
    }

    pub fn peers_discovered_set(value: u64) {
        gauge!("icn_network_peers_discovered").set(value as f64);
    }

    pub fn messages_rate_limited_inc() {
        counter!("icn_network_messages_rate_limited_total").increment(1);
    }

    pub fn messages_rate_limited_by_class_inc(trust_class: &str) {
        counter!("icn_network_messages_rate_limited_by_class_total", "class" => trust_class.to_string()).increment(1);
    }

    pub fn active_peers_by_class_set(trust_class: &str, count: u64) {
        gauge!("icn_network_active_peers_by_class", "class" => trust_class.to_string()).set(count as f64);
    }

    pub fn trust_class_changes_inc() {
        counter!("icn_network_trust_class_changes_total").increment(1);
    }

    pub fn connections_rejected_untrusted_inc(peer_did: &str, trust_score: f64) {
        counter!("icn_network_connections_rejected_untrusted_total",
                 "peer_did" => peer_did.to_string(),
                 "trust_score" => format!("{:.3}", trust_score))
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

        counter!("icn_network_connections_rejected_by_class_total", "class" => trust_class.to_string())
            .increment(1);
    }
}

/// Gossip metrics
pub mod gossip {
    use metrics::{counter, gauge, histogram};

    pub fn topics_total_set(value: u64) {
        gauge!("icn_gossip_topics_total").set(value as f64);
    }

    pub fn entries_total_set(value: u64) {
        gauge!("icn_gossip_entries_total").set(value as f64);
    }

    pub fn entries_published_inc() {
        counter!("icn_gossip_entries_published_total").increment(1);
    }

    pub fn entries_received_inc() {
        counter!("icn_gossip_entries_received_total").increment(1);
    }

    pub fn announces_sent_inc() {
        counter!("icn_gossip_announces_sent_total").increment(1);
    }

    pub fn requests_sent_inc() {
        counter!("icn_gossip_requests_sent_total").increment(1);
    }

    pub fn responses_sent_inc() {
        counter!("icn_gossip_responses_sent_total").increment(1);
    }

    pub fn announces_received_inc() {
        counter!("icn_gossip_announces_received_total").increment(1);
    }

    pub fn requests_received_inc() {
        counter!("icn_gossip_requests_received_total").increment(1);
    }

    pub fn responses_received_inc() {
        counter!("icn_gossip_responses_received_total").increment(1);
    }

    pub fn subscriptions_total_set(value: u64) {
        gauge!("icn_gossip_subscriptions_total").set(value as f64);
    }

    pub fn subscriptions_rejected_inc(topic: &str, trust_score: f64) {
        counter!(
            "icn_gossip_subscriptions_rejected_total",
            "topic" => topic.to_string(),
            "trust_score" => format!("{:.2}", trust_score)
        ).increment(1);
    }

    pub fn subscribes_received_inc() {
        counter!("icn_gossip_subscribes_received_total").increment(1);
    }

    pub fn unsubscribes_received_inc() {
        counter!("icn_gossip_unsubscribes_received_total").increment(1);
    }

    pub fn subscribe_acks_sent_inc() {
        counter!("icn_gossip_subscribe_acks_sent_total").increment(1);
    }

    pub fn digests_sent_inc() {
        counter!("icn_gossip_digests_sent_total").increment(1);
    }

    pub fn digests_received_inc() {
        counter!("icn_gossip_digests_received_total").increment(1);
    }

    pub fn pull_requests_sent_inc() {
        counter!("icn_gossip_pull_requests_sent_total").increment(1);
    }

    pub fn pull_requests_received_inc() {
        counter!("icn_gossip_pull_requests_received_total").increment(1);
    }

    pub fn pull_responses_sent_inc() {
        counter!("icn_gossip_pull_responses_sent_total").increment(1);
    }

    pub fn pull_responses_received_inc() {
        counter!("icn_gossip_pull_responses_received_total").increment(1);
    }

    pub fn bytes_pulled_add(bytes: u64) {
        counter!("icn_gossip_bytes_pulled_total").increment(bytes);
    }

    pub fn bytes_pushed_add(bytes: u64) {
        counter!("icn_gossip_bytes_pushed_total").increment(bytes);
    }

    pub fn peer_deficit_bytes_set(peer: &str, deficit: i64) {
        gauge!("icn_gossip_peer_deficit_bytes", "peer" => peer.to_string())
            .set(deficit as f64);
    }

    pub fn bloom_fp_rate_record(topic: &str, rate: f64) {
        histogram!("icn_gossip_bloom_fp_rate", "topic" => topic.to_string()).record(rate);
    }

    pub fn pull_truncated_inc() {
        counter!("icn_gossip_pull_truncated_total").increment(1);
    }
}

/// Ledger metrics
pub mod ledger {
    use metrics::{counter, gauge, histogram};

    pub fn accounts_total_set(value: u64) {
        gauge!("icn_ledger_accounts_total").set(value as f64);
    }

    pub fn currencies_total_set(value: u64) {
        gauge!("icn_ledger_currencies_total").set(value as f64);
    }

    pub fn transactions_total_inc() {
        counter!("icn_ledger_transactions_total").increment(1);
    }

    pub fn transaction_amount_record(amount: i64) {
        histogram!("icn_ledger_transaction_amount").record(amount.abs() as f64);
    }

    pub fn merge_conflicts_inc() {
        counter!("icn_ledger_merge_conflicts_total").increment(1);
    }

    pub fn entries_quarantined_inc() {
        counter!("icn_ledger_entries_quarantined_total").increment(1);
    }

    pub fn entries_discarded_inc() {
        counter!("icn_ledger_entries_discarded_total").increment(1);
    }

    pub fn quarantine_size_set(size: u64) {
        gauge!("icn_ledger_quarantine_size").set(size as f64);
    }
}

/// Trust graph metrics
pub mod trust {
    use metrics::{counter, gauge, histogram};

    pub fn edges_total_set(value: u64) {
        gauge!("icn_trust_edges_total").set(value as f64);
    }

    pub fn peers_by_class_set(trust_class: &str, count: u64) {
        gauge!("icn_trust_peers_by_class", "class" => trust_class.to_string()).set(count as f64);
    }

    pub fn lookups_inc() {
        counter!("icn_trust_lookups_total").increment(1);
    }

    pub fn cache_hits_inc() {
        counter!("icn_trust_cache_hits_total").increment(1);
    }

    pub fn cache_misses_inc() {
        counter!("icn_trust_cache_misses_total").increment(1);
    }

    pub fn score_distribution_record(score: f64) {
        histogram!("icn_trust_score_distribution").record(score);
    }

    pub fn attestations_received_inc() {
        counter!("icn_trust_attestations_received_total").increment(1);
    }

    pub fn attestations_broadcasted_inc() {
        counter!("icn_trust_attestations_broadcasted_total").increment(1);
    }

    pub fn attestations_rejected_expired_inc() {
        counter!("icn_trust_attestations_rejected_expired_total").increment(1);
    }

    pub fn attestations_rejected_invalid_signature_inc() {
        counter!("icn_trust_attestations_rejected_invalid_signature_total").increment(1);
    }

    pub fn attestations_rejected_outdated_inc() {
        counter!("icn_trust_attestations_rejected_outdated_total").increment(1);
    }

    pub fn attestations_rejected_rate_limited_inc() {
        counter!("icn_trust_attestations_rejected_rate_limited_total").increment(1);
    }

    pub fn attestations_new_inc() {
        counter!("icn_trust_attestations_new_total").increment(1);
    }

    pub fn attestations_updated_inc() {
        counter!("icn_trust_attestations_updated_total").increment(1);
    }
}

/// Contract metrics
pub mod contract {
    use metrics::{counter, gauge, histogram};

    pub fn installed_total_set(value: u64) {
        gauge!("icn_contract_installed_total").set(value as f64);
    }

    pub fn deployments_inc() {
        counter!("icn_contract_deployments_total").increment(1);
    }

    pub fn deployments_received_inc() {
        counter!("icn_contract_deployments_received_total").increment(1);
    }

    pub fn deployments_rejected_inc(reason: &str) {
        counter!("icn_contract_deployments_rejected_total", "reason" => reason.to_string()).increment(1);
    }

    pub fn deployments_rejected_trust_inc(deployer: &str, trust_score: f64) {
        counter!(
            "icn_contract_deployments_rejected_trust_total",
            "deployer" => deployer.to_string(),
            "trust_score" => format!("{:.2}", trust_score)
        ).increment(1);
    }

    pub fn deployments_rejected_signature_inc(signer: &str) {
        counter!(
            "icn_contract_deployments_rejected_signature_total",
            "signer" => signer.to_string()
        ).increment(1);
    }

    pub fn executions_inc(contract_name: &str, rule_name: &str) {
        counter!(
            "icn_contract_executions_total",
            "contract" => contract_name.to_string(),
            "rule" => rule_name.to_string()
        ).increment(1);
    }

    pub fn executions_failed_inc(contract_name: &str, rule_name: &str, error: &str) {
        counter!(
            "icn_contract_executions_failed_total",
            "contract" => contract_name.to_string(),
            "rule" => rule_name.to_string(),
            "error" => error.to_string()
        ).increment(1);
    }

    pub fn executions_rejected_unauthorized_inc(caller: &str) {
        counter!(
            "icn_contract_executions_rejected_unauthorized_total",
            "caller" => caller.to_string()
        ).increment(1);
    }

    pub fn execution_fuel_used_record(fuel: u64) {
        histogram!("icn_contract_execution_fuel_used").record(fuel as f64);
    }

    pub fn execution_duration_record(duration_secs: f64) {
        histogram!("icn_contract_execution_duration_seconds").record(duration_secs);
    }

    pub fn ledger_operations_add(count: u64) {
        counter!("icn_contract_ledger_operations_total").increment(count);
    }
}

/// System metrics
pub mod system {
    use metrics::gauge;

    pub fn uptime_seconds_set(value: u64) {
        gauge!("icn_system_uptime_seconds").set(value as f64);
    }

    pub fn actors_active_set(value: u64) {
        gauge!("icn_system_actors_active").set(value as f64);
    }
}
