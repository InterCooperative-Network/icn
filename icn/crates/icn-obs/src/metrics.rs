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

    // Peer Exchange metrics (Federation)
    describe_counter!(
        "icn_peer_exchange_requests_sent_total",
        "Total number of peer exchange requests sent"
    );
    describe_counter!(
        "icn_peer_exchange_requests_received_total",
        "Total number of peer exchange requests received"
    );
    describe_counter!(
        "icn_peer_exchange_responses_sent_total",
        "Total number of peer exchange responses sent"
    );
    describe_counter!(
        "icn_peer_exchange_responses_received_total",
        "Total number of peer exchange responses received"
    );
    describe_counter!(
        "icn_peer_exchange_announces_sent_total",
        "Total number of peer announce messages sent"
    );
    describe_counter!(
        "icn_peer_exchange_announces_received_total",
        "Total number of peer announce messages received"
    );
    describe_counter!(
        "icn_peer_exchange_unannounces_sent_total",
        "Total number of peer unannounce messages sent"
    );
    describe_counter!(
        "icn_peer_exchange_unannounces_received_total",
        "Total number of peer unannounce messages received"
    );
    describe_counter!(
        "icn_peer_exchange_peers_discovered_total",
        "Total number of peers discovered via peer exchange"
    );
    describe_counter!(
        "icn_peer_exchange_peers_dialed_total",
        "Total number of peers auto-dialed from peer exchange"
    );
    describe_counter!(
        "icn_peer_exchange_dial_failures_total",
        "Total number of peer exchange dial failures"
    );

    // NAT Traversal metrics (STUN)
    describe_counter!(
        "icn_stun_queries_total",
        "Total number of STUN queries by server and result"
    );
    describe_histogram!(
        "icn_stun_discovery_duration_seconds",
        "Duration of STUN discovery operations in seconds"
    );
    describe_counter!(
        "icn_stun_consensus_votes_total",
        "Distribution of majority vote outcomes by endpoint"
    );
    describe_counter!(
        "icn_stun_server_failures_total",
        "Total number of STUN server query failures by server"
    );

    // NAT Traversal metrics (Candidates)
    describe_counter!(
        "icn_candidates_received_total",
        "Total number of connection candidates received via gossip"
    );
    describe_gauge!(
        "icn_candidates_cached_total",
        "Current number of candidates in cache"
    );
    describe_counter!(
        "icn_candidates_expired_total",
        "Total number of expired candidates removed from cache"
    );
    describe_counter!(
        "icn_candidates_stale_rejected_total",
        "Total number of stale candidates rejected on arrival"
    );
    describe_counter!(
        "icn_candidates_published_total",
        "Total number of connection candidates published to gossip"
    );

    // NAT Traversal metrics (Connection Attempts)
    describe_counter!(
        "icn_nat_connection_attempts_total",
        "Total number of NAT traversal connection attempts by method"
    );
    describe_counter!(
        "icn_nat_connection_success_total",
        "Total number of successful NAT traversal connections by method"
    );
    describe_histogram!(
        "icn_nat_connection_duration_seconds",
        "Duration of NAT traversal connection attempts in seconds by method"
    );
    describe_counter!(
        "icn_nat_hole_punch_attempts_total",
        "Total number of NAT hole punching attempts"
    );
    describe_counter!(
        "icn_nat_hole_punch_success_total",
        "Total number of successful NAT hole punches"
    );

    // Topology metrics
    describe_gauge!(
        "icn_topology_neighbors_by_set",
        "Number of neighbors in each neighbor set (local_cluster, regional, backbone, trusted)"
    );
    describe_histogram!(
        "icn_topology_gossip_fanout",
        "Gossip fanout count by scope (local_cluster, regional, global)"
    );
    describe_histogram!(
        "icn_topology_rtt_milliseconds",
        "Round-trip time measurements for peers in milliseconds"
    );
    describe_histogram!(
        "icn_topology_bandwidth_bytes_per_second",
        "Bandwidth measurements for peers in bytes per second"
    );

    // Gossip metrics
    describe_gauge!("icn_gossip_topics_total", "Total number of gossip topics");
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

    // Scalability metrics (Phase 19)
    describe_counter!(
        "icn_scalability_vector_clocks_compressed_total",
        "Total number of vector clocks compressed"
    );
    describe_counter!(
        "icn_scalability_vector_clocks_decompressed_total",
        "Total number of vector clocks decompressed"
    );
    describe_histogram!(
        "icn_scalability_compression_ratio",
        "Vector clock compression ratio (uncompressed/compressed)"
    );
    describe_gauge!(
        "icn_scalability_compressed_size_bytes",
        "Current size of compressed vector clock in bytes"
    );
    describe_gauge!(
        "icn_scalability_delta_count",
        "Number of non-zero deltas in compressed vector clock"
    );
    describe_histogram!(
        "icn_scalability_compression_duration_seconds",
        "Time to compress a vector clock"
    );
    describe_counter!(
        "icn_scalability_trust_cache_hits_total",
        "Total number of trust cache hits"
    );
    describe_counter!(
        "icn_scalability_trust_cache_misses_total",
        "Total number of trust cache misses"
    );
    describe_counter!(
        "icn_scalability_trust_cache_expired_total",
        "Total number of expired trust cache entries"
    );
    describe_counter!(
        "icn_scalability_trust_cache_invalidations_total",
        "Total number of trust cache invalidations"
    );
    describe_gauge!(
        "icn_scalability_trust_cache_size",
        "Current number of entries in trust cache"
    );
    describe_counter!(
        "icn_scalability_batch_verify_success_total",
        "Total number of successful batch verifications"
    );
    describe_counter!(
        "icn_scalability_batch_verify_failed_total",
        "Total number of failed batch verifications"
    );
    describe_counter!(
        "icn_scalability_batch_verify_invalid_signatures_total",
        "Total number of invalid signatures found during batch verification"
    );
    describe_histogram!(
        "icn_scalability_batch_verify_duration_seconds",
        "Duration of batch signature verification operations"
    );
    describe_histogram!(
        "icn_scalability_batch_verify_size",
        "Number of signatures verified in each batch"
    );
    describe_counter!(
        "icn_scalability_topic_sharding_enabled_total",
        "Total number of topics that enabled sharding (exceeded 1000 entries)"
    );
    describe_gauge!(
        "icn_scalability_sharded_topic_size",
        "Current number of entries in sharded topics by topic name"
    );

    // Clock sync metrics
    describe_counter!(
        "icn_scalability_clock_sync_success_total",
        "Total number of successful clock synchronizations"
    );
    describe_counter!(
        "icn_scalability_clock_sync_failed_total",
        "Total number of failed clock synchronization attempts"
    );
    describe_histogram!(
        "icn_scalability_clock_sync_duration_seconds",
        "Duration of clock sync operations in seconds"
    );
    describe_histogram!(
        "icn_scalability_clock_sync_offset_seconds",
        "Clock offset from network median in seconds"
    );
    describe_counter!(
        "icn_scalability_timestamp_validation_accepted_total",
        "Total number of timestamps accepted as valid"
    );
    describe_counter!(
        "icn_scalability_timestamp_validation_rejected_total",
        "Total number of timestamps rejected (out of range)"
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

    // Governance metrics
    describe_counter!(
        "icn_governance_proposals_executed_total",
        "Total number of governance proposals executed by payload type"
    );
    describe_counter!(
        "icn_governance_execution_failures_total",
        "Total number of proposal execution failures by reason"
    );
    describe_histogram!(
        "icn_governance_execution_duration_seconds",
        "Duration of proposal execution in seconds"
    );
    describe_counter!(
        "icn_governance_audit_failures_total",
        "Total number of audit trail write failures"
    );
    describe_counter!(
        "icn_governance_idempotent_skips_total",
        "Total number of duplicate executions prevented by idempotency check"
    );

    // Trust graph metrics
    describe_gauge!(
        "icn_trust_edges_total",
        "Total number of trust edges in the graph"
    );
    describe_gauge!("icn_trust_peers_by_class", "Number of peers by trust class");
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
    describe_gauge!("icn_system_uptime_seconds", "System uptime in seconds");
    describe_gauge!("icn_system_actors_active", "Number of active actors");

    // Snapshot metrics (graceful restart)
    describe_histogram!(
        "icn_snapshot_save_duration_seconds",
        "Duration of snapshot save operations in seconds"
    );
    describe_histogram!(
        "icn_snapshot_load_duration_seconds",
        "Duration of snapshot load operations in seconds"
    );
    describe_counter!("icn_snapshot_save_total", "Total number of snapshots saved");
    describe_counter!(
        "icn_snapshot_load_total",
        "Total number of snapshots loaded"
    );
    describe_counter!(
        "icn_snapshot_save_errors_total",
        "Total number of snapshot save errors"
    );
    describe_counter!(
        "icn_snapshot_load_errors_total",
        "Total number of snapshot load errors"
    );
    describe_gauge!(
        "icn_snapshot_size_bytes",
        "Size of the last saved snapshot in bytes"
    );
    describe_gauge!(
        "icn_snapshot_gossip_vector_clock_entries",
        "Number of vector clock entries in last snapshot"
    );
    describe_gauge!(
        "icn_snapshot_gossip_subscriptions",
        "Number of subscriptions in last snapshot"
    );
    describe_gauge!(
        "icn_snapshot_gossip_topics",
        "Number of topics in last snapshot"
    );
    describe_gauge!(
        "icn_snapshot_network_x25519_keys",
        "Number of peer X25519 keys in last snapshot"
    );

    // Gateway API metrics
    describe_counter!(
        "icn_gateway_requests_total",
        "Total number of HTTP requests by endpoint and method"
    );
    describe_histogram!(
        "icn_gateway_request_duration_seconds",
        "Request duration in seconds by endpoint and status"
    );
    describe_counter!(
        "icn_gateway_auth_challenges_total",
        "Total number of authentication challenges issued"
    );
    describe_counter!(
        "icn_gateway_auth_verifications_total",
        "Total number of authentication verification attempts"
    );
    describe_counter!(
        "icn_gateway_auth_failures_total",
        "Total number of authentication failures by reason"
    );
    describe_counter!(
        "icn_gateway_auth_successes_total",
        "Total number of successful authentications"
    );
    describe_counter!(
        "icn_gateway_rate_limit_exceeded_total",
        "Total number of requests rejected due to rate limiting"
    );
    describe_counter!(
        "icn_gateway_authorization_failures_total",
        "Total number of authorization failures by required scope"
    );
    describe_gauge!(
        "icn_gateway_websocket_connections_active",
        "Current number of active WebSocket connections"
    );
    describe_counter!(
        "icn_gateway_websocket_connections_total",
        "Total number of WebSocket connections established"
    );
    describe_counter!(
        "icn_gateway_websocket_disconnections_total",
        "Total number of WebSocket disconnections"
    );
    describe_counter!(
        "icn_gateway_websocket_messages_sent_total",
        "Total number of WebSocket messages sent"
    );
    describe_counter!(
        "icn_gateway_coops_created_total",
        "Total number of cooperatives created"
    );
    describe_counter!(
        "icn_gateway_coops_deleted_total",
        "Total number of cooperatives deleted"
    );
    describe_counter!(
        "icn_gateway_members_added_total",
        "Total number of members added to cooperatives"
    );
    describe_counter!(
        "icn_gateway_members_removed_total",
        "Total number of members removed from cooperatives"
    );
    describe_counter!(
        "icn_gateway_payments_created_total",
        "Total number of payments created"
    );
    describe_histogram!(
        "icn_gateway_payment_amount",
        "Distribution of payment amounts by currency"
    );
    describe_counter!(
        "icn_gateway_balance_queries_total",
        "Total number of balance queries"
    );
    describe_counter!(
        "icn_gateway_history_queries_total",
        "Total number of transaction history queries"
    );
    describe_counter!(
        "icn_gateway_governance_domains_created_total",
        "Total number of governance domains created"
    );
    describe_counter!(
        "icn_gateway_governance_proposals_created_total",
        "Total number of governance proposals created"
    );
    describe_counter!(
        "icn_gateway_governance_proposals_opened_total",
        "Total number of governance proposals opened for voting"
    );
    describe_counter!(
        "icn_gateway_governance_proposals_closed_total",
        "Total number of governance proposals closed"
    );
    describe_counter!(
        "icn_gateway_governance_votes_cast_total",
        "Total number of governance votes cast"
    );

    // Compute metrics
    describe_counter!(
        "icn_compute_tasks_submitted_total",
        "Total number of compute tasks submitted"
    );
    describe_counter!(
        "icn_compute_tasks_claimed_total",
        "Total number of compute tasks claimed by executors"
    );
    describe_counter!(
        "icn_compute_tasks_completed_total",
        "Total number of compute tasks completed"
    );
    describe_counter!(
        "icn_compute_tasks_failed_total",
        "Total number of compute tasks that failed"
    );
    describe_gauge!(
        "icn_compute_tasks_pending",
        "Current number of pending compute tasks"
    );
    describe_gauge!(
        "icn_compute_tasks_executing",
        "Current number of tasks being executed"
    );
    describe_histogram!(
        "icn_compute_task_duration_seconds",
        "Duration of compute task execution in seconds"
    );
    describe_histogram!(
        "icn_compute_fuel_used",
        "Distribution of fuel consumed by compute tasks"
    );
    describe_counter!(
        "icn_compute_fuel_total",
        "Total fuel consumed across all tasks"
    );
    describe_counter!(
        "icn_compute_payments_settled_total",
        "Total number of compute payments settled"
    );
    describe_counter!(
        "icn_compute_payment_amount_total",
        "Total payment amount for compute tasks"
    );
    describe_counter!(
        "icn_compute_tasks_rejected_trust_total",
        "Total number of tasks rejected due to insufficient trust"
    );
    describe_counter!(
        "icn_compute_tasks_timeout_total",
        "Total number of tasks that timed out"
    );
    describe_counter!(
        "icn_compute_tasks_out_of_fuel_total",
        "Total number of tasks that ran out of fuel"
    );
    describe_counter!(
        "icn_compute_signatures_verified_total",
        "Total number of compute result signatures verified"
    );
    describe_counter!(
        "icn_compute_signatures_invalid_total",
        "Total number of invalid compute result signatures detected"
    );
    describe_gauge!(
        "icn_compute_executors_available",
        "Current number of available executors in the registry"
    );
    describe_counter!(
        "icn_compute_placement_requests_received_total",
        "Total number of placement requests received by this executor"
    );
    describe_counter!(
        "icn_compute_placement_offers_sent_total",
        "Total number of placement offers sent by this executor"
    );
    describe_counter!(
        "icn_compute_placement_offers_received_total",
        "Total number of placement offers received by submitters"
    );
    describe_counter!(
        "icn_compute_placement_wins_total",
        "Total number of tasks this executor won via placement negotiation"
    );
    describe_counter!(
        "icn_compute_placement_losses_total",
        "Total number of tasks this executor lost (had offer but didn't win)"
    );
    describe_histogram!(
        "icn_compute_placement_score",
        "Distribution of placement scores computed by this executor"
    );
    describe_histogram!(
        "icn_compute_placement_duration_seconds",
        "Time from PlacementRequest to TaskClaimed in seconds"
    );

    // Policy metrics (Phase 16E)
    describe_counter!(
        "icn_compute_policy_violations_total",
        "Total number of policy violations detected"
    );
    describe_counter!(
        "icn_compute_quota_exceeded_total",
        "Total number of quota exceeded events"
    );
    describe_counter!(
        "icn_compute_priority_adjustments_total",
        "Total number of task priority adjustments by policy"
    );
    describe_gauge!(
        "icn_compute_member_cpu_hours",
        "CPU hours used by member in current month"
    );
    describe_gauge!(
        "icn_compute_member_concurrent_tasks",
        "Current concurrent tasks for member"
    );
    describe_gauge!(
        "icn_compute_member_credits_spent",
        "Credits spent by member in current month"
    );
    describe_counter!(
        "icn_compute_placement_constraints_enforced_total",
        "Total number of placement constraint enforcements"
    );

    // Misbehavior detection metrics (Phase 18)
    describe_counter!(
        "icn_misbehavior_violations_total",
        "Total number of misbehavior violations detected"
    );
    describe_gauge!(
        "icn_misbehavior_quarantined_peers",
        "Number of peers currently quarantined"
    );
    describe_gauge!(
        "icn_misbehavior_banned_peers",
        "Number of peers permanently banned"
    );
    describe_counter!(
        "icn_misbehavior_auto_bans_total",
        "Total number of automatic bans issued"
    );
    describe_counter!(
        "icn_misbehavior_reputation_penalties_total",
        "Total number of reputation penalties applied"
    );

    // Partition detection and healing metrics (Phase 18 Week 3)
    describe_gauge!(
        "icn_partition_peers_detected",
        "Number of peers detected as partitioned"
    );
    describe_counter!(
        "icn_partition_heals_total",
        "Total number of partition healing operations"
    );
    describe_counter!(
        "icn_partition_conflicts_detected_total",
        "Total number of conflicts detected during partition healing"
    );
    describe_counter!(
        "icn_partition_conflicts_resolved_total",
        "Total number of conflicts successfully resolved"
    );
    describe_counter!(
        "icn_partition_conflicts_manual_total",
        "Total number of conflicts requiring manual resolution"
    );
    describe_histogram!(
        "icn_partition_heal_duration_seconds",
        "Duration of partition healing operations in seconds"
    );
    describe_counter!(
        "icn_partition_vector_clock_merges_total",
        "Total number of vector clock merges performed"
    );

    // Contract execution dispute metrics (Phase 18 Week 4)
    describe_counter!("icn_disputes_filed_total", "Total number of disputes filed");
    describe_gauge!(
        "icn_disputes_pending",
        "Number of disputes currently pending"
    );
    describe_gauge!(
        "icn_disputes_investigating",
        "Number of disputes currently being investigated"
    );
    describe_counter!(
        "icn_disputes_resolved_total",
        "Total number of disputes resolved"
    );
    describe_counter!(
        "icn_disputes_under_mediation_total",
        "Total number of disputes assigned to mediators"
    );
    describe_histogram!(
        "icn_disputes_investigation_duration_seconds",
        "Duration of dispute investigation in seconds"
    );
    describe_counter!(
        "icn_disputes_outcome_total",
        "Total number of disputes by outcome type"
    );
    describe_gauge!(
        "icn_disputes_mediator_pool_size",
        "Number of mediators in the pool"
    );

    // Ledger fork resolution metrics (Phase 18 Week 5)
    describe_counter!(
        "icn_ledger_forks_detected_total",
        "Total number of ledger forks detected"
    );
    describe_counter!(
        "icn_ledger_forks_resolved_total",
        "Total number of ledger forks resolved"
    );
    describe_histogram!(
        "icn_ledger_forks_resolution_duration_seconds",
        "Duration of fork resolution in seconds"
    );
    describe_counter!(
        "icn_ledger_forks_manual_required_total",
        "Total number of forks requiring manual resolution"
    );
    describe_counter!(
        "icn_ledger_forks_timestamp_tiebreaker_total",
        "Total number of forks resolved by timestamp tiebreaker"
    );
    describe_counter!(
        "icn_ledger_forks_trust_resolution_total",
        "Total number of forks resolved by trust score"
    );

    // Storage quota metrics (Phase 18 Week 6)
    describe_counter!(
        "icn_storage_quota_exceeded_total",
        "Total number of quota exceeded errors"
    );
    describe_counter!(
        "icn_storage_evictions_total",
        "Total number of storage evictions by priority"
    );
    describe_gauge!(
        "icn_storage_global_usage_bytes",
        "Current global storage usage in bytes"
    );
    describe_gauge!(
        "icn_storage_global_limit_bytes",
        "Global storage limit in bytes"
    );
    describe_gauge!(
        "icn_storage_global_usage_percentage",
        "Global storage usage as percentage (0.0 to 1.0)"
    );
    describe_gauge!(
        "icn_storage_did_quota_usage_bytes",
        "Per-DID storage usage in bytes"
    );
    describe_gauge!(
        "icn_storage_did_quota_percentage",
        "Per-DID storage usage as percentage (0.0 to 1.0)"
    );
    describe_gauge!(
        "icn_storage_total_quotas",
        "Total number of configured storage quotas"
    );
    describe_gauge!(
        "icn_storage_exceeded_quotas",
        "Number of quotas currently exceeded"
    );

    // Privacy metrics (Phase 20)
    describe_counter!(
        "icn_privacy_topics_encrypted_total",
        "Total number of topics encrypted for privacy"
    );
    describe_counter!(
        "icn_privacy_topics_decrypted_total",
        "Total number of encrypted topics decrypted"
    );
    describe_counter!(
        "icn_privacy_bloom_filter_hits_total",
        "Total number of Bloom filter matches"
    );
    describe_counter!(
        "icn_privacy_bloom_filter_misses_total",
        "Total number of Bloom filter misses"
    );
    describe_counter!(
        "icn_privacy_onion_routes_created_total",
        "Total number of onion routes created"
    );
    describe_counter!(
        "icn_privacy_onion_hops_forwarded_total",
        "Total number of onion routing hops forwarded"
    );
    describe_counter!(
        "icn_privacy_cover_traffic_sent_total",
        "Total number of cover traffic messages sent"
    );
    describe_counter!(
        "icn_privacy_messages_padded_total",
        "Total number of messages padded for size obfuscation"
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
        gauge!("icn_network_active_peers_by_class", "class" => trust_class.to_string())
            .set(count as f64);
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

    pub fn protocol_version_mismatch_inc() {
        counter!("icn_network_protocol_version_mismatch_total").increment(1);
    }

    pub fn protocol_version_too_old_inc() {
        counter!("icn_network_protocol_version_too_old_total").increment(1);
    }

    pub fn protocol_version_too_new_inc() {
        counter!("icn_network_protocol_version_too_new_total").increment(1);
    }

    pub fn peer_version_set(version: u32, count: u64) {
        gauge!("icn_network_peer_versions", "version" => version.to_string()).set(count as f64);
    }

    pub fn peer_capability_set(capability: &str, count: u64) {
        gauge!("icn_network_peer_capabilities", "capability" => capability.to_string())
            .set(count as f64);
    }

    pub fn version_negotiation_failure_inc(reason: &str) {
        counter!("icn_network_version_negotiation_failures_total", "reason" => reason.to_string())
            .increment(1);
    }

    pub fn version_negotiation_success_inc(negotiated_version: u32) {
        counter!("icn_network_version_negotiation_success_total", "negotiated_version" => negotiated_version.to_string()).increment(1);
    }
}

/// Peer exchange metrics (Federation)
pub mod peer_exchange {
    use metrics::counter;

    pub fn requests_sent_inc() {
        counter!("icn_peer_exchange_requests_sent_total").increment(1);
    }

    pub fn requests_received_inc() {
        counter!("icn_peer_exchange_requests_received_total").increment(1);
    }

    pub fn responses_sent_inc() {
        counter!("icn_peer_exchange_responses_sent_total").increment(1);
    }

    pub fn responses_received_inc() {
        counter!("icn_peer_exchange_responses_received_total").increment(1);
    }

    pub fn announces_sent_inc() {
        counter!("icn_peer_exchange_announces_sent_total").increment(1);
    }

    pub fn announces_received_inc() {
        counter!("icn_peer_exchange_announces_received_total").increment(1);
    }

    pub fn unannounces_sent_inc() {
        counter!("icn_peer_exchange_unannounces_sent_total").increment(1);
    }

    pub fn unannounces_received_inc() {
        counter!("icn_peer_exchange_unannounces_received_total").increment(1);
    }

    pub fn peers_discovered_add(count: u64) {
        counter!("icn_peer_exchange_peers_discovered_total").increment(count);
    }

    pub fn peers_dialed_inc() {
        counter!("icn_peer_exchange_peers_dialed_total").increment(1);
    }

    pub fn dial_failures_inc() {
        counter!("icn_peer_exchange_dial_failures_total").increment(1);
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
        )
        .increment(1);
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
        gauge!("icn_gossip_peer_deficit_bytes", "peer" => peer.to_string()).set(deficit as f64);
    }

    pub fn bloom_fp_rate_record(topic: &str, rate: f64) {
        histogram!("icn_gossip_bloom_fp_rate", "topic" => topic.to_string()).record(rate);
    }

    pub fn pull_truncated_inc() {
        counter!("icn_gossip_pull_truncated_total").increment(1);
    }

    // Phase 18 Week 3: Partition detection and healing
    pub fn partition_detected_inc() {
        counter!("icn_partition_detections_total").increment(1);
    }

    pub fn partition_healed_inc() {
        counter!("icn_partition_heals_total").increment(1);
    }

    pub fn partition_peers_detected_set(count: usize) {
        gauge!("icn_partition_peers_detected").set(count as f64);
    }

    pub fn partition_conflicts_detected_inc() {
        counter!("icn_partition_conflicts_detected_total").increment(1);
    }

    pub fn partition_conflicts_resolved_inc() {
        counter!("icn_partition_conflicts_resolved_total").increment(1);
    }

    pub fn partition_conflicts_manual_inc() {
        counter!("icn_partition_conflicts_manual_total").increment(1);
    }

    pub fn partition_heal_duration_record(duration_secs: f64) {
        histogram!("icn_partition_heal_duration_seconds").record(duration_secs);
    }

    pub fn partition_vector_clock_merges_inc() {
        counter!("icn_partition_vector_clock_merges_total").increment(1);
    }
}

/// Scalability metrics (Phase 19)
pub mod scalability {
    use metrics::{counter, gauge, histogram};

    pub fn vector_clocks_compressed_inc() {
        counter!("icn_scalability_vector_clocks_compressed_total").increment(1);
    }

    pub fn vector_clocks_decompressed_inc() {
        counter!("icn_scalability_vector_clocks_decompressed_total").increment(1);
    }

    pub fn compression_ratio_record(ratio: f64) {
        histogram!("icn_scalability_compression_ratio").record(ratio);
    }

    pub fn compressed_size_bytes_set(bytes: usize) {
        gauge!("icn_scalability_compressed_size_bytes").set(bytes as f64);
    }

    pub fn delta_count_set(count: usize) {
        gauge!("icn_scalability_delta_count").set(count as f64);
    }

    pub fn compression_duration_record(duration_secs: f64) {
        histogram!("icn_scalability_compression_duration_seconds").record(duration_secs);
    }

    // Trust cache metrics
    pub fn trust_cache_hits_inc() {
        counter!("icn_scalability_trust_cache_hits_total").increment(1);
    }

    pub fn trust_cache_misses_inc() {
        counter!("icn_scalability_trust_cache_misses_total").increment(1);
    }

    pub fn trust_cache_expired_inc() {
        counter!("icn_scalability_trust_cache_expired_total").increment(1);
    }

    pub fn trust_cache_invalidations_inc() {
        counter!("icn_scalability_trust_cache_invalidations_total").increment(1);
    }

    pub fn trust_cache_size_set(size: usize) {
        gauge!("icn_scalability_trust_cache_size").set(size as f64);
    }

    // Batch verification metrics
    pub fn batch_verify_success_inc() {
        counter!("icn_scalability_batch_verify_success_total").increment(1);
    }

    pub fn batch_verify_failed_inc() {
        counter!("icn_scalability_batch_verify_failed_total").increment(1);
    }

    pub fn batch_verify_invalid_inc() {
        counter!("icn_scalability_batch_verify_invalid_signatures_total").increment(1);
    }

    pub fn batch_verify_duration_record(duration_secs: f64) {
        histogram!("icn_scalability_batch_verify_duration_seconds").record(duration_secs);
    }

    pub fn batch_verify_size_record(count: usize) {
        histogram!("icn_scalability_batch_verify_size").record(count as f64);
    }

    // Topic sharding metrics
    pub fn topic_sharding_enabled_inc() {
        counter!("icn_scalability_topic_sharding_enabled_total").increment(1);
    }

    pub fn sharded_topic_size_set(topic: &str, size: usize) {
        gauge!("icn_scalability_sharded_topic_size", "topic" => topic.to_string()).set(size as f64);
    }

    // Clock sync metrics
    pub fn clock_sync_success_inc() {
        counter!("icn_scalability_clock_sync_success_total").increment(1);
    }

    pub fn clock_sync_failed_inc() {
        counter!("icn_scalability_clock_sync_failed_total").increment(1);
    }

    pub fn clock_sync_duration_record(duration_secs: f64) {
        histogram!("icn_scalability_clock_sync_duration_seconds").record(duration_secs);
    }

    pub fn clock_sync_offset_record(offset_secs: f64) {
        histogram!("icn_scalability_clock_sync_offset_seconds").record(offset_secs);
    }

    pub fn timestamp_validation_accepted_inc() {
        counter!("icn_scalability_timestamp_validation_accepted_total").increment(1);
    }

    pub fn timestamp_validation_rejected_inc() {
        counter!("icn_scalability_timestamp_validation_rejected_total").increment(1);
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

/// Governance execution metrics
pub mod governance {
    use metrics::{counter, histogram};

    pub fn proposals_executed_inc(payload_type: &str) {
        counter!("icn_governance_proposals_executed_total", "payload_type" => payload_type.to_string()).increment(1);
    }

    pub fn execution_failures_inc(reason: &str) {
        counter!("icn_governance_execution_failures_total", "reason" => reason.to_string())
            .increment(1);
    }

    pub fn execution_duration_record(payload_type: &str, duration: f64) {
        histogram!("icn_governance_execution_duration_seconds", "payload_type" => payload_type.to_string()).record(duration);
    }

    pub fn audit_failures_inc() {
        counter!("icn_governance_audit_failures_total").increment(1);
    }

    pub fn idempotent_skips_inc() {
        counter!("icn_governance_idempotent_skips_total").increment(1);
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
        counter!("icn_contract_deployments_rejected_total", "reason" => reason.to_string())
            .increment(1);
    }

    pub fn deployments_rejected_trust_inc(deployer: &str, trust_score: f64) {
        counter!(
            "icn_contract_deployments_rejected_trust_total",
            "deployer" => deployer.to_string(),
            "trust_score" => format!("{:.2}", trust_score)
        )
        .increment(1);
    }

    pub fn deployments_rejected_signature_inc(signer: &str) {
        counter!(
            "icn_contract_deployments_rejected_signature_total",
            "signer" => signer.to_string()
        )
        .increment(1);
    }

    pub fn executions_inc(contract_name: &str, rule_name: &str) {
        counter!(
            "icn_contract_executions_total",
            "contract" => contract_name.to_string(),
            "rule" => rule_name.to_string()
        )
        .increment(1);
    }

    pub fn executions_failed_inc(contract_name: &str, rule_name: &str, error: &str) {
        counter!(
            "icn_contract_executions_failed_total",
            "contract" => contract_name.to_string(),
            "rule" => rule_name.to_string(),
            "error" => error.to_string()
        )
        .increment(1);
    }

    pub fn executions_rejected_unauthorized_inc(caller: &str) {
        counter!(
            "icn_contract_executions_rejected_unauthorized_total",
            "caller" => caller.to_string()
        )
        .increment(1);
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

/// Topology metrics
pub mod topology {
    use metrics::{gauge, histogram};

    /// Update neighbor count gauges for all sets
    pub fn neighbors_by_set_update(
        local_cluster: usize,
        regional: usize,
        backbone: usize,
        trusted: usize,
    ) {
        gauge!("icn_topology_neighbors_by_set", "set" => "local_cluster").set(local_cluster as f64);
        gauge!("icn_topology_neighbors_by_set", "set" => "regional").set(regional as f64);
        gauge!("icn_topology_neighbors_by_set", "set" => "backbone").set(backbone as f64);
        gauge!("icn_topology_neighbors_by_set", "set" => "trusted").set(trusted as f64);
    }

    /// Record gossip fanout for a specific scope
    pub fn gossip_fanout_record(scope: &str, count: usize) {
        histogram!("icn_topology_gossip_fanout", "scope" => scope.to_string()).record(count as f64);
    }

    /// Record RTT measurement for a peer
    pub fn rtt_observe(rtt_ms: f64) {
        histogram!("icn_topology_rtt_milliseconds").record(rtt_ms);
    }

    /// Record bandwidth measurement for a peer
    pub fn bandwidth_observe(bandwidth_bps: f64) {
        histogram!("icn_topology_bandwidth_bytes_per_second").record(bandwidth_bps);
    }
}

/// Snapshot metrics (graceful restart)
pub mod snapshot {
    use metrics::{counter, gauge, histogram};

    pub fn save_duration_record(duration_secs: f64) {
        histogram!("icn_snapshot_save_duration_seconds").record(duration_secs);
    }

    pub fn load_duration_record(duration_secs: f64) {
        histogram!("icn_snapshot_load_duration_seconds").record(duration_secs);
    }

    pub fn save_total_inc() {
        counter!("icn_snapshot_save_total").increment(1);
    }

    pub fn load_total_inc() {
        counter!("icn_snapshot_load_total").increment(1);
    }

    pub fn save_errors_inc() {
        counter!("icn_snapshot_save_errors_total").increment(1);
    }

    pub fn load_errors_inc() {
        counter!("icn_snapshot_load_errors_total").increment(1);
    }

    pub fn size_bytes_set(size: u64) {
        gauge!("icn_snapshot_size_bytes").set(size as f64);
    }

    pub fn gossip_vector_clock_entries_set(count: usize) {
        gauge!("icn_snapshot_gossip_vector_clock_entries").set(count as f64);
    }

    pub fn gossip_subscriptions_set(count: usize) {
        gauge!("icn_snapshot_gossip_subscriptions").set(count as f64);
    }

    pub fn gossip_topics_set(count: usize) {
        gauge!("icn_snapshot_gossip_topics").set(count as f64);
    }

    pub fn network_x25519_keys_set(count: usize) {
        gauge!("icn_snapshot_network_x25519_keys").set(count as f64);
    }
}

/// Gateway API metrics
pub mod gateway {
    use metrics::{counter, gauge, histogram};

    pub fn requests_total_inc(endpoint: &str, method: &str) {
        counter!("icn_gateway_requests_total",
                 "endpoint" => endpoint.to_string(),
                 "method" => method.to_string())
        .increment(1);
    }

    pub fn request_duration_record(endpoint: &str, status: u16, duration_secs: f64) {
        histogram!("icn_gateway_request_duration_seconds",
                   "endpoint" => endpoint.to_string(),
                   "status" => status.to_string())
        .record(duration_secs);
    }

    pub fn auth_challenges_inc() {
        counter!("icn_gateway_auth_challenges_total").increment(1);
    }

    pub fn auth_verifications_inc() {
        counter!("icn_gateway_auth_verifications_total").increment(1);
    }

    pub fn auth_failures_inc(reason: &str) {
        counter!("icn_gateway_auth_failures_total",
                 "reason" => reason.to_string())
        .increment(1);
    }

    pub fn auth_successes_inc() {
        counter!("icn_gateway_auth_successes_total").increment(1);
    }

    pub fn rate_limit_exceeded_inc(did: &str) {
        counter!("icn_gateway_rate_limit_exceeded_total",
                 "did" => did.to_string())
        .increment(1);
    }

    pub fn authorization_failures_inc(required_scope: &str) {
        counter!("icn_gateway_authorization_failures_total",
                 "required_scope" => required_scope.to_string())
        .increment(1);
    }

    pub fn websocket_connections_active_set(count: u64) {
        gauge!("icn_gateway_websocket_connections_active").set(count as f64);
    }

    pub fn websocket_connections_inc() {
        counter!("icn_gateway_websocket_connections_total").increment(1);
    }

    pub fn websocket_disconnections_inc() {
        counter!("icn_gateway_websocket_disconnections_total").increment(1);
    }

    pub fn websocket_messages_sent_inc() {
        counter!("icn_gateway_websocket_messages_sent_total").increment(1);
    }

    pub fn coops_created_inc() {
        counter!("icn_gateway_coops_created_total").increment(1);
    }

    pub fn coops_deleted_inc() {
        counter!("icn_gateway_coops_deleted_total").increment(1);
    }

    pub fn members_added_inc() {
        counter!("icn_gateway_members_added_total").increment(1);
    }

    pub fn members_removed_inc() {
        counter!("icn_gateway_members_removed_total").increment(1);
    }

    pub fn payments_created_inc() {
        counter!("icn_gateway_payments_created_total").increment(1);
    }

    pub fn payment_amount_record(currency: &str, amount: i64) {
        histogram!("icn_gateway_payment_amount",
                   "currency" => currency.to_string())
        .record(amount.abs() as f64);
    }

    pub fn balance_queries_inc() {
        counter!("icn_gateway_balance_queries_total").increment(1);
    }

    pub fn history_queries_inc() {
        counter!("icn_gateway_history_queries_total").increment(1);
    }

    pub fn governance_domains_created_inc() {
        counter!("icn_gateway_governance_domains_created_total").increment(1);
    }

    pub fn governance_proposals_created_inc() {
        counter!("icn_gateway_governance_proposals_created_total").increment(1);
    }

    pub fn governance_proposals_opened_inc() {
        counter!("icn_gateway_governance_proposals_opened_total").increment(1);
    }

    pub fn governance_proposals_closed_inc() {
        counter!("icn_gateway_governance_proposals_closed_total").increment(1);
    }

    pub fn governance_votes_cast_inc() {
        counter!("icn_gateway_governance_votes_cast_total").increment(1);
    }
}

/// NAT traversal metrics
pub mod nat_traversal {
    use metrics::{counter, gauge, histogram};

    // STUN metrics
    pub fn stun_query_inc(server: &str, result: &str) {
        counter!("icn_stun_queries_total",
                 "server" => server.to_string(),
                 "result" => result.to_string())
        .increment(1);
    }

    pub fn stun_discovery_duration_record(duration_secs: f64) {
        histogram!("icn_stun_discovery_duration_seconds").record(duration_secs);
    }

    pub fn stun_consensus_vote_inc(endpoint: &str, votes: usize, total_servers: usize) {
        counter!("icn_stun_consensus_votes_total",
                 "endpoint" => endpoint.to_string(),
                 "votes" => votes.to_string(),
                 "total_servers" => total_servers.to_string())
        .increment(1);
    }

    pub fn stun_server_failure_inc(server: &str, reason: &str) {
        counter!("icn_stun_server_failures_total",
                 "server" => server.to_string(),
                 "reason" => reason.to_string())
        .increment(1);
    }

    // Candidate metrics
    pub fn candidates_received_inc() {
        counter!("icn_candidates_received_total").increment(1);
    }

    pub fn candidates_cached_set(count: usize) {
        gauge!("icn_candidates_cached_total").set(count as f64);
    }

    pub fn candidates_expired_inc() {
        counter!("icn_candidates_expired_total").increment(1);
    }

    pub fn candidates_stale_rejected_inc() {
        counter!("icn_candidates_stale_rejected_total").increment(1);
    }

    pub fn candidates_published_inc() {
        counter!("icn_candidates_published_total").increment(1);
    }

    // Connection attempt metrics
    pub fn connection_attempt_inc(method: &str) {
        counter!("icn_nat_connection_attempts_total",
                 "method" => method.to_string())
        .increment(1);
    }

    pub fn connection_success_inc(method: &str) {
        counter!("icn_nat_connection_success_total",
                 "method" => method.to_string())
        .increment(1);
    }

    pub fn connection_duration_record(method: &str, duration_secs: f64) {
        histogram!("icn_nat_connection_duration_seconds",
                   "method" => method.to_string())
        .record(duration_secs);
    }

    pub fn hole_punch_attempt_inc() {
        counter!("icn_nat_hole_punch_attempts_total").increment(1);
    }

    pub fn hole_punch_success_inc() {
        counter!("icn_nat_hole_punch_success_total").increment(1);
    }
}

/// Compute metrics
pub mod compute {
    use metrics::{counter, gauge, histogram};

    pub fn tasks_submitted_inc() {
        counter!("icn_compute_tasks_submitted_total").increment(1);
    }

    pub fn tasks_claimed_inc() {
        counter!("icn_compute_tasks_claimed_total").increment(1);
    }

    pub fn tasks_completed_inc(outcome: &str) {
        counter!("icn_compute_tasks_completed_total", "outcome" => outcome.to_string())
            .increment(1);
    }

    pub fn tasks_failed_inc(reason: &str) {
        counter!("icn_compute_tasks_failed_total", "reason" => reason.to_string()).increment(1);
    }

    pub fn tasks_cancelled_inc() {
        counter!("icn_compute_tasks_cancelled_total").increment(1);
    }

    pub fn tasks_pending_set(count: u64) {
        gauge!("icn_compute_tasks_pending").set(count as f64);
    }

    pub fn tasks_executing_set(count: u64) {
        gauge!("icn_compute_tasks_executing").set(count as f64);
    }

    pub fn task_duration_record(duration_secs: f64) {
        histogram!("icn_compute_task_duration_seconds").record(duration_secs);
    }

    pub fn fuel_used_record(fuel: u64) {
        histogram!("icn_compute_fuel_used").record(fuel as f64);
    }

    pub fn fuel_total_add(fuel: u64) {
        counter!("icn_compute_fuel_total").increment(fuel);
    }

    pub fn payments_settled_inc() {
        counter!("icn_compute_payments_settled_total").increment(1);
    }

    pub fn payment_amount_add(amount: u64) {
        counter!("icn_compute_payment_amount_total").increment(amount);
    }

    pub fn tasks_rejected_trust_inc(submitter: &str, trust_score: f64) {
        counter!(
            "icn_compute_tasks_rejected_trust_total",
            "submitter" => submitter.to_string(),
            "trust_score" => format!("{:.2}", trust_score)
        )
        .increment(1);
    }

    pub fn tasks_timeout_inc() {
        counter!("icn_compute_tasks_timeout_total").increment(1);
    }

    pub fn tasks_out_of_fuel_inc() {
        counter!("icn_compute_tasks_out_of_fuel_total").increment(1);
    }

    pub fn signatures_verified_inc() {
        counter!("icn_compute_signatures_verified_total").increment(1);
    }

    pub fn signatures_invalid_inc(reason: &str) {
        counter!("icn_compute_signatures_invalid_total", "reason" => reason.to_string())
            .increment(1);
    }

    pub fn executors_available_set(count: f64) {
        gauge!("icn_compute_executors_available").set(count);
    }

    pub fn executor_load_set(executor_did: &str, load: f64) {
        gauge!("icn_compute_executor_load", "executor" => executor_did.to_string()).set(load);
    }

    pub fn tasks_rejected_capacity_inc() {
        counter!("icn_compute_tasks_rejected_capacity_total").increment(1);
    }

    // Placement negotiation metrics (Phase 16B)
    pub fn placement_requests_received_inc() {
        counter!("icn_compute_placement_requests_received_total").increment(1);
    }

    pub fn placement_offers_sent_inc() {
        counter!("icn_compute_placement_offers_sent_total").increment(1);
    }

    pub fn placement_offers_received_inc() {
        counter!("icn_compute_placement_offers_received_total").increment(1);
    }

    pub fn placement_wins_inc() {
        counter!("icn_compute_placement_wins_total").increment(1);
    }

    pub fn placement_losses_inc() {
        counter!("icn_compute_placement_losses_total").increment(1);
    }

    pub fn placement_score_observe(score: f64) {
        histogram!("icn_compute_placement_score").record(score);
    }

    pub fn placement_duration_observe(duration_secs: f64) {
        histogram!("icn_compute_placement_duration_seconds").record(duration_secs);
    }

    // Policy metrics (Phase 16E)
    pub fn policy_violations_inc(coop_id: &str, reason: &str) {
        counter!(
            "icn_compute_policy_violations_total",
            "coop_id" => coop_id.to_string(),
            "reason" => reason.to_string()
        )
        .increment(1);
    }

    pub fn quota_exceeded_inc(coop_id: &str, member: &str, quota_type: &str) {
        counter!(
            "icn_compute_quota_exceeded_total",
            "coop_id" => coop_id.to_string(),
            "member" => member.to_string(),
            "quota_type" => quota_type.to_string()
        )
        .increment(1);
    }

    pub fn priority_adjustments_inc(coop_id: &str, from_priority: &str, to_priority: &str) {
        counter!(
            "icn_compute_priority_adjustments_total",
            "coop_id" => coop_id.to_string(),
            "from" => from_priority.to_string(),
            "to" => to_priority.to_string()
        )
        .increment(1);
    }

    pub fn member_cpu_hours_set(coop_id: &str, member: &str, hours: f64) {
        gauge!(
            "icn_compute_member_cpu_hours",
            "coop_id" => coop_id.to_string(),
            "member" => member.to_string()
        )
        .set(hours);
    }

    pub fn member_concurrent_tasks_set(coop_id: &str, member: &str, count: u32) {
        gauge!(
            "icn_compute_member_concurrent_tasks",
            "coop_id" => coop_id.to_string(),
            "member" => member.to_string()
        )
        .set(count as f64);
    }

    pub fn member_credits_spent_set(coop_id: &str, member: &str, credits: u64) {
        gauge!(
            "icn_compute_member_credits_spent",
            "coop_id" => coop_id.to_string(),
            "member" => member.to_string()
        )
        .set(credits as f64);
    }

    pub fn placement_constraints_enforced_inc(constraint_type: &str) {
        counter!(
            "icn_compute_placement_constraints_enforced_total",
            "constraint_type" => constraint_type.to_string()
        )
        .increment(1);
    }

    // Dispute resolution metrics (Phase 18 Week 4)
    pub fn disputes_filed_inc() {
        counter!("icn_compute_disputes_filed_total").increment(1);
    }

    pub fn disputes_resolved_inc(outcome: &str) {
        counter!("icn_compute_disputes_resolved_total", "outcome" => outcome.to_string())
            .increment(1);
    }

    pub fn disputes_pending_set(count: u64) {
        gauge!("icn_compute_disputes_pending").set(count as f64);
    }

    pub fn disputes_investigating_set(count: u64) {
        gauge!("icn_compute_disputes_investigating").set(count as f64);
    }
}

/// Misbehavior detection metrics (Phase 18)
pub mod misbehavior {
    use metrics::{counter, gauge};

    pub fn violations_inc(did: &str, violation_type: &str) {
        counter!(
            "icn_misbehavior_violations_total",
            "did" => did.to_string(),
            "violation_type" => violation_type.to_string()
        )
        .increment(1);
    }

    pub fn quarantined_inc() {
        gauge!("icn_misbehavior_quarantined_peers").increment(1.0);
    }

    pub fn quarantined_dec() {
        gauge!("icn_misbehavior_quarantined_peers").decrement(1.0);
    }

    pub fn quarantined_set(count: u64) {
        gauge!("icn_misbehavior_quarantined_peers").set(count as f64);
    }

    pub fn banned_inc() {
        gauge!("icn_misbehavior_banned_peers").increment(1.0);
    }

    pub fn banned_set(count: u64) {
        gauge!("icn_misbehavior_banned_peers").set(count as f64);
    }

    pub fn auto_bans_inc() {
        counter!("icn_misbehavior_auto_bans_total").increment(1);
    }

    pub fn reputation_penalties_inc(did: &str, severity: u32) {
        counter!(
            "icn_misbehavior_reputation_penalties_total",
            "did" => did.to_string(),
            "severity" => severity.to_string()
        )
        .increment(1);
    }
}

/// Partition detection and healing metrics (Phase 18 Week 3)
pub mod partition {
    use metrics::{counter, gauge, histogram};

    pub fn peers_detected_set(count: u64) {
        gauge!("icn_partition_peers_detected").set(count as f64);
    }

    pub fn heals_inc() {
        counter!("icn_partition_heals_total").increment(1);
    }

    pub fn conflicts_detected_inc(data_type: &str) {
        counter!(
            "icn_partition_conflicts_detected_total",
            "data_type" => data_type.to_string()
        )
        .increment(1);
    }

    pub fn conflicts_resolved_inc(data_type: &str, strategy: &str) {
        counter!(
            "icn_partition_conflicts_resolved_total",
            "data_type" => data_type.to_string(),
            "strategy" => strategy.to_string()
        )
        .increment(1);
    }

    pub fn conflicts_manual_inc(data_type: &str) {
        counter!(
            "icn_partition_conflicts_manual_total",
            "data_type" => data_type.to_string()
        )
        .increment(1);
    }

    pub fn heal_duration_record(duration_secs: f64) {
        histogram!("icn_partition_heal_duration_seconds").record(duration_secs);
    }

    pub fn vector_clock_merges_inc() {
        counter!("icn_partition_vector_clock_merges_total").increment(1);
    }
}

/// Contract execution dispute metrics (Phase 18 Week 4)
pub mod disputes {
    use metrics::{counter, gauge, histogram};

    pub fn filed_inc(executor: &str, challenger: &str) {
        counter!(
            "icn_disputes_filed_total",
            "executor" => executor.to_string(),
            "challenger" => challenger.to_string()
        )
        .increment(1);
    }

    pub fn pending_set(count: u64) {
        gauge!("icn_disputes_pending").set(count as f64);
    }

    pub fn investigating_set(count: u64) {
        gauge!("icn_disputes_investigating").set(count as f64);
    }

    pub fn resolved_inc(outcome_type: &str) {
        counter!(
            "icn_disputes_resolved_total",
            "outcome" => outcome_type.to_string()
        )
        .increment(1);
    }

    pub fn under_mediation_inc(mediator: &str) {
        counter!(
            "icn_disputes_under_mediation_total",
            "mediator" => mediator.to_string()
        )
        .increment(1);
    }

    pub fn investigation_duration_record(duration_secs: f64) {
        histogram!("icn_disputes_investigation_duration_seconds").record(duration_secs);
    }

    pub fn outcome_inc(outcome_type: &str, executor: &str) {
        counter!(
            "icn_disputes_outcome_total",
            "outcome" => outcome_type.to_string(),
            "executor" => executor.to_string()
        )
        .increment(1);
    }

    pub fn mediator_pool_size_set(count: usize) {
        gauge!("icn_disputes_mediator_pool_size").set(count as f64);
    }
}

/// Ledger fork resolution metrics (Phase 18 Week 5)
pub mod ledger_forks {
    use metrics::{counter, histogram};

    pub fn detected_inc() {
        counter!("icn_ledger_forks_detected_total").increment(1);
    }

    pub fn resolved_inc(strategy: &str) {
        counter!(
            "icn_ledger_forks_resolved_total",
            "strategy" => strategy.to_string()
        )
        .increment(1);
    }

    pub fn resolution_duration_record(duration_secs: f64) {
        histogram!("icn_ledger_forks_resolution_duration_seconds").record(duration_secs);
    }

    pub fn manual_resolution_required_inc(reason: &str) {
        counter!(
            "icn_ledger_forks_manual_required_total",
            "reason" => reason.to_string()
        )
        .increment(1);
    }

    pub fn timestamp_tiebreaker_inc() {
        counter!("icn_ledger_forks_timestamp_tiebreaker_total").increment(1);
    }

    pub fn trust_resolution_inc(winner: &str) {
        counter!(
            "icn_ledger_forks_trust_resolution_total",
            "winner" => winner.to_string()
        )
        .increment(1);
    }
}

/// Storage quota metrics (Phase 18 Week 6)
pub mod storage_quotas {
    use metrics::{counter, gauge};

    pub fn quota_exceeded_inc(did: &str) {
        counter!(
            "icn_storage_quota_exceeded_total",
            "did" => did.to_string()
        )
        .increment(1);
    }

    pub fn evictions_inc(priority: &str) {
        counter!(
            "icn_storage_evictions_total",
            "priority" => priority.to_string()
        )
        .increment(1);
    }

    pub fn global_usage_set(bytes: u64) {
        gauge!("icn_storage_global_usage_bytes").set(bytes as f64);
    }

    pub fn global_limit_set(bytes: u64) {
        gauge!("icn_storage_global_limit_bytes").set(bytes as f64);
    }

    pub fn global_usage_percentage_set(percentage: f64) {
        gauge!("icn_storage_global_usage_percentage").set(percentage);
    }

    pub fn did_quota_usage_set(did: &str, bytes: u64) {
        gauge!(
            "icn_storage_did_quota_usage_bytes",
            "did" => did.to_string()
        )
        .set(bytes as f64);
    }

    pub fn did_quota_percentage_set(did: &str, percentage: f64) {
        gauge!(
            "icn_storage_did_quota_percentage",
            "did" => did.to_string()
        )
        .set(percentage);
    }

    pub fn total_quotas_set(count: usize) {
        gauge!("icn_storage_total_quotas").set(count as f64);
    }

    pub fn exceeded_quotas_set(count: usize) {
        gauge!("icn_storage_exceeded_quotas").set(count as f64);
    }

    /// Simple counter for quota exceeded events (without DID label)
    pub fn exceeded_inc() {
        counter!("icn_storage_quota_exceeded_total").increment(1);
    }

    /// Counter for total evicted entries
    pub fn evicted_inc(count: u64) {
        counter!("icn_storage_evicted_total").increment(count);
    }
}

pub mod privacy {
    use metrics::counter;

    pub fn topics_encrypted_inc() {
        counter!("icn_privacy_topics_encrypted_total").increment(1);
    }

    pub fn topics_decrypted_inc() {
        counter!("icn_privacy_topics_decrypted_total").increment(1);
    }

    pub fn bloom_filter_hits_inc() {
        counter!("icn_privacy_bloom_filter_hits_total").increment(1);
    }

    pub fn bloom_filter_misses_inc() {
        counter!("icn_privacy_bloom_filter_misses_total").increment(1);
    }

    pub fn onion_routes_created_inc() {
        counter!("icn_privacy_onion_routes_created_total").increment(1);
    }

    pub fn onion_hops_forwarded_inc() {
        counter!("icn_privacy_onion_hops_forwarded_total").increment(1);
    }

    pub fn cover_traffic_sent_inc() {
        counter!("icn_privacy_cover_traffic_sent_total").increment(1);
    }

    pub fn messages_padded_inc() {
        counter!("icn_privacy_messages_padded_total").increment(1);
    }
}
