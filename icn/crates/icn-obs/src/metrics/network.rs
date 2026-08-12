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
    // Two-phase inbound rate limiting (Issue #2491)
    describe_counter!(
        "icn_network_messages_rate_limited_pre_auth_total",
        "Total number of messages denied by an anonymous pre-authentication budget before any peer identity was bound to the connection, by which budget refused (connection or source)"
    );
    describe_counter!(
        "icn_network_messages_rate_limited_by_rate_total",
        "Total number of messages rate limited, bucketed by rate limit (0-10, 11-50, 51-100, 100+)"
    );
    // Pre-authentication connection admission (Issue #2547)
    describe_counter!(
        "icn_network_preauth_admission_refused_total",
        "Total inbound connections refused a pre-authentication slot, by which bound was hit (global_limit or source_limit)"
    );
    describe_counter!(
        "icn_network_preauth_admission_released_total",
        "Total pre-authentication slots released because the connection authenticated"
    );
    describe_gauge!(
        "icn_network_preauth_connections_live",
        "Established inbound connections that have not yet authenticated a peer identity"
    );
    describe_gauge!(
        "icn_network_preauth_sources_tracked",
        "Distinct sources currently holding at least one pre-authentication slot (bounded by the global admission limit)"
    );
    // Pre-authentication deadline (Issue #2552)
    describe_counter!(
        "icn_network_preauth_authentication_timeout_total",
        "Total inbound connections closed for holding a pre-authentication slot without ever authenticating"
    );
    // Per-source anonymous work budget (Issue #2549)
    describe_gauge!(
        "icn_network_preauth_source_budget_tracked",
        "Sources the pre-authentication budget table currently holds an allowance for (reclaimed lazily, only when the table is full)"
    );
    describe_counter!(
        "icn_network_preauth_source_budget_degraded_total",
        "Total anonymous messages charged to the shared budget because the per-source table was full"
    );
    describe_counter!(
        "icn_network_rate_limit_config_changes_total",
        "Total number of peer rate limit configuration changes"
    );
    describe_counter!(
        "icn_network_trust_class_toctou_mismatches_total",
        "Total number of TOCTOU mismatches where trust class changed during rate limit check"
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
    describe_counter!(
        "icn_network_hello_binding_rejected_total",
        "Total number of Hello messages refused because the claimed DID could not be bound \
         to the current connection's certificate, by coarse reason (never labelled by DID)"
    );
    describe_gauge!(
        "icn_network_replay_guard_peers",
        "Number of peers tracked in replay guard"
    );
    describe_counter!(
        "icn_network_replay_guard_cleanups_total",
        "Total number of replay guard cleanup cycles executed"
    );
    describe_counter!(
        "icn_network_replay_guard_peers_cleaned_total",
        "Total number of stale peers removed during cleanup"
    );
    describe_counter!(
        "icn_network_replay_guard_bloom_rotations_total",
        "Total number of Bloom filter rotations to prevent saturation"
    );
    // E2E Encryption metrics (Issue #404)
    describe_counter!(
        "icn_network_encrypted_messages_sent_total",
        "Total number of E2E encrypted messages sent"
    );
    describe_counter!(
        "icn_network_encrypted_messages_received_total",
        "Total number of E2E encrypted messages successfully received and decrypted"
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
    describe_counter!(
        "icn_network_encryption_circuit_breaker_trips_total",
        "Total number of times the encryption cleanup circuit breaker has tripped"
    );
    // Hybrid PQ verification metrics (Issue #530)
    describe_counter!(
        "icn_network_hybrid_verification_cache_hit_total",
        "Total number of hybrid signature verifications using cached PQ key (full verification)"
    );
    describe_counter!(
        "icn_network_hybrid_verification_cache_miss_total",
        "Total number of hybrid signature verifications with no cached PQ key (deferred verification)"
    );
    describe_counter!(
        "icn_network_hybrid_verification_failed_total",
        "Total number of hybrid signature verifications that failed"
    );
    // Blob registry metrics (Issue #570)
    describe_counter!(
        "icn_network_blob_rejected_total",
        "Total number of blob announcements rejected"
    );
    // Sybil resistance metrics (Issue #675)
    describe_counter!(
        "icn_network_messages_rate_limited_by_anchor_total",
        "Total number of messages rate limited due to per-person (per-anchor) limit"
    );
    describe_counter!(
        "icn_sybil_detection_total",
        "Total number of Sybil attack mitigations by detection type"
    );
    describe_counter!(
        "icn_personhood_required_rejections_total",
        "Total number of messages rejected due to RequirePersonhood enforcement"
    );
    describe_gauge!(
        "icn_personhood_anchors_active",
        "Number of unique personhood anchors with active rate limit buckets"
    );
    describe_counter!(
        "icn_network_blob_evicted_total",
        "Total number of blob entries evicted due to capacity limits"
    );
    describe_gauge!(
        "icn_network_blob_registry_size_bytes",
        "Current total size of all blobs in the registry"
    );
    describe_gauge!(
        "icn_network_blob_registry_entries",
        "Current number of blob location entries in the registry"
    );
    describe_counter!(
        "icn_personhood_store_failures_total",
        "Total number of personhood store lookup failures (Sybil protection degraded)"
    );
    describe_counter!(
        "icn_network_bootstrap_did_expectation_mismatch_total",
        "Total number of connections where a bootstrap entry's configured DID differed from the DID that authenticated"
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

/// A message denied against a connection's pre-authentication budget (#2491).
///
/// A subset of `messages_rate_limited_total`, separated because the two mean opposite
/// things operationally. A throttled *authenticated* peer is a tier that may need raising;
/// this counter is anonymous traffic that never got as far as saying who it was, and a
/// sustained rise in it is a load signal rather than a configuration one.
///
/// `bound` says which budget refused, and is the **only** label here on purpose (#2558). An
/// inbound connection spends two: its own burst (#2491) and the one shared by everything from
/// its source (#2549). Unlabelled, this counter could not separate a single connection
/// exhausting its own allowance from an address whose whole allowance is gone — conditions that
/// look identical here and want different responses. The value comes from the closed two-value
/// set `PreAuthRefusal::as_str`, so nothing a remote peer sends can add a series.
///
/// The labels this still deliberately does not carry — the claimed DID, the remote address, the
/// source key — are unbounded and attacker-chosen, which is the same mistake at the metrics
/// layer that #2491 fixes at the policy layer. They belong in the log line, and are there.
///
/// A refusal counted under `bound="source"` does **not** mean a source token was spent: the
/// connection's token was, and the source's was not. See `PreAuthBudget::check`.
///
/// (Distinguishing the *other* rate-limit counter's causes — policy denial versus throttling on
/// `messages_rate_limited_total` — is #2499's subject, not this one's.)
pub fn messages_rate_limited_pre_auth_inc(bound: &'static str) {
    counter!("icn_network_messages_rate_limited_pre_auth_total", "bound" => bound).increment(1);
}

/// An inbound connection was refused a pre-authentication slot (#2547).
///
/// `bound` says which limit was hit, and is the **only** label here on purpose: it comes
/// from a closed two-value set in the code (`AdmissionRefusal::as_str`), so nothing a remote
/// peer sends can add a series. The labels this deliberately does not carry — remote
/// address, source key, connection id — are attacker-chosen and unbounded, which is the same
/// cardinality mistake at the metrics layer that #2491 fixed at the policy layer.
///
/// The refused address belongs in logs, where a bounded-cardinality store is not the
/// constraint, and it is logged there.
pub fn preauth_admission_refused_inc(bound: &'static str) {
    counter!("icn_network_preauth_admission_refused_total", "bound" => bound).increment(1);
}

/// A pre-authentication slot was released because its connection authenticated (#2547).
///
/// Rising alongside `preauth_connections_live` staying flat is the healthy shape: peers are
/// arriving and identifying themselves. Refusals rising while this stays flat is the attack
/// shape — connections arriving that never say who they are.
pub fn preauth_admission_released_inc() {
    counter!("icn_network_preauth_admission_released_total").increment(1);
}

/// Established inbound connections that have not yet authenticated (#2547).
pub fn preauth_connections_live_set(count: usize) {
    gauge!("icn_network_preauth_connections_live").set(count as f64);
}

/// Distinct sources holding at least one pre-authentication slot (#2547).
///
/// This is the admission table's cardinality. It is bounded by the global admission limit by
/// construction, so an operator seeing it climb toward that limit is seeing the bound work,
/// not a leak.
pub fn preauth_sources_tracked_set(count: usize) {
    gauge!("icn_network_preauth_sources_tracked").set(count as f64);
}

/// A connection was closed for never authenticating within its deadline (#2552).
///
/// Unlabelled on purpose. There is exactly one reason to reach here, so a `reason` dimension
/// would carry no information, and every dimension worth attributing this to — source address,
/// /64, claimed DID — is chosen by the party being counted. Attaching any of them would let an
/// unauthenticated peer mint metric series, which is the same unbounded-cardinality mistake
/// `preauth_admission_refused_inc` avoids for the same reason. The address is in the log line.
///
/// Read together with `preauth_connections_live`: this rising while that gauge stays flat means
/// the deadline is turning squatters over as fast as they arrive, which is the bound working.
/// This flat while the gauge sits at its ceiling would mean the deadline is not firing at all.
pub fn preauth_authentication_timeout_inc() {
    counter!("icn_network_preauth_authentication_timeout_total").increment(1);
}

/// How many sources the anonymous-work budget table currently holds an allowance for (#2549).
///
/// Not a count of connections or of peers, and **not** a count of sources currently throttled: an
/// entry becomes reclaimable once its allowance refills, but is only dropped by a sweep, and a
/// sweep runs only on the miss path of a full table. Below capacity nothing is reclaimed, so this
/// climbs with distinct sources seen and does not fall back to zero on a quiet network. Read it as
/// an upper bound on "sources below full", bounded by `MAX_PREAUTH_BUDGET_SOURCES`.
///
/// Reaching that ceiling is therefore not itself degradation — it is the point at which sweeping
/// begins. `preauth_source_budget_degraded_total` moving is the signal that a sweep freed nothing,
/// which is the condition that actually costs per-source fairness.
pub fn preauth_source_budget_tracked_set(sources: usize) {
    gauge!("icn_network_preauth_source_budget_tracked").set(sources as f64);
}

/// An anonymous message was charged to the shared budget instead of its own source's (#2549).
///
/// This is the degraded mode, and it is a statement about *this node's table*, not about the
/// source — which is why it carries no source label, the same unbounded-cardinality reasoning as
/// `preauth_admission_refused_inc`. Nonzero means the per-source table was full, so every
/// untracked source is sharing one source's worth of allowance between them. Aggregate work stays
/// bounded; per-source fairness does not.
pub fn preauth_source_budget_degraded_inc() {
    counter!("icn_network_preauth_source_budget_degraded_total").increment(1);
}

/// Increment rate limit configuration change counter
///
/// Called when rate limit parameters change for a peer (e.g., due to policy updates).
pub fn rate_limit_config_changes_inc() {
    counter!("icn_network_rate_limit_config_changes_total").increment(1);
}

/// Increment TOCTOU mismatch counter (Issue #426)
///
/// Called when trust class changes between initial lookup and bucket update
/// during rate limit check. This is a low-severity race condition that is
/// self-correcting on the next check.
pub fn trust_class_toctou_mismatches_inc() {
    counter!("icn_network_trust_class_toctou_mismatches_total").increment(1);
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
/// Increment rate-limited message counter with a rate bucket label.
/// Uses buckets to avoid high cardinality: "0-10", "11-50", "51-100", "100+"
pub fn messages_rate_limited_by_rate_inc(messages_per_second: u32) {
    let bucket = match messages_per_second {
        0..=10 => "0-10",
        11..=50 => "11-50",
        51..=100 => "51-100",
        _ => "100+",
    };
    counter!(
        "icn_network_messages_rate_limited_by_rate_total",
        "limit_bucket" => bucket
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

/// A Hello was refused because the claimed DID could not be bound to the certificate
/// presented by the current QUIC connection.
///
/// The label is a coarse failure class, never a DID: an unauthenticated party chooses
/// the DID it claims, so labelling by DID would let anyone forge series for a peer they
/// do not control.
pub fn hello_binding_rejected_inc(reason: &str) {
    counter!(
        "icn_network_hello_binding_rejected_total",
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

/// Increment replay guard cleanup cycles counter (#153)
pub fn replay_guard_cleanups_inc() {
    counter!("icn_network_replay_guard_cleanups_total").increment(1);
}

/// Add to the count of peers removed during cleanup (#153)
pub fn replay_guard_peers_cleaned_add(count: u64) {
    counter!("icn_network_replay_guard_peers_cleaned_total").increment(count);
}

/// Increment Bloom filter rotation counter (#154)
pub fn replay_guard_bloom_rotations_inc() {
    counter!("icn_network_replay_guard_bloom_rotations_total").increment(1);
}

// E2E Encryption metrics (Issue #404)

/// Increment encrypted messages sent counter
pub fn encrypted_messages_sent_inc() {
    counter!("icn_network_encrypted_messages_sent_total").increment(1);
}

/// Increment encrypted messages received and successfully decrypted counter
pub fn encrypted_messages_received_inc() {
    counter!("icn_network_encrypted_messages_received_total").increment(1);
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

/// Increment circuit breaker trip counter
///
/// This is a critical alert metric - any non-zero value indicates that
/// encryption cleanup has failed repeatedly (5+ times) and storage may
/// be degraded. Operators should investigate immediately.
pub fn encryption_circuit_breaker_trips_inc() {
    counter!("icn_network_encryption_circuit_breaker_trips_total").increment(1);
}

// Hybrid PQ verification metrics (Issue #530)

/// Increment hybrid verification cache hit counter
///
/// Called when a hybrid signature is verified using a cached PQ public key.
/// This indicates full hybrid verification (both Ed25519 + ML-DSA verified).
pub fn hybrid_verification_cache_hit_inc() {
    counter!("icn_network_hybrid_verification_cache_hit_total").increment(1);
}

/// Increment hybrid verification cache miss counter
///
/// Called when a hybrid signature cannot be fully verified because no
/// PQ public key is cached for the sender. Falls back to deferred verification.
pub fn hybrid_verification_cache_miss_inc() {
    counter!("icn_network_hybrid_verification_cache_miss_total").increment(1);
}

/// Failure reason for hybrid signature verification
#[derive(Debug, Clone, Copy)]
pub enum HybridVerificationFailure {
    /// Cached ML-DSA key is malformed/corrupted
    InvalidPqKey,
    /// ML-DSA signature didn't verify with cached key
    PqSignatureMismatch,
    /// Ed25519 (classical) signature verification failed
    ClassicalSignatureFailed,
}

impl HybridVerificationFailure {
    fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidPqKey => "invalid_pq_key",
            Self::PqSignatureMismatch => "pq_signature_mismatch",
            Self::ClassicalSignatureFailed => "classical_signature_failed",
        }
    }
}

/// Increment hybrid verification failed counter with reason
///
/// Called when hybrid signature verification fails.
pub fn hybrid_verification_failed_inc(reason: HybridVerificationFailure) {
    counter!(
        "icn_network_hybrid_verification_failed_total",
        "reason" => reason.as_str()
    )
    .increment(1);
}

/// Record a rejected connection due to insufficient trust.
///
/// Note: This function uses trust class as the label to avoid high-cardinality
/// metrics from per-DID labels. For debugging specific peers, use structured logs.
/// See Issue #494 for cardinality guidelines.
pub fn connections_rejected_untrusted_inc(_peer_did: &str, trust_score: f64) {
    // Increment simple counter for total rejections
    counter!("icn_network_connections_rejected_untrusted_total").increment(1);

    // Also increment by trust class for aggregated metrics (bounded cardinality)
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

// Blob registry metrics (Issue #570)

/// Reason for blob announcement rejection
#[derive(Debug, Clone, Copy)]
pub enum BlobRejectionReason {
    /// Blob size exceeds the maximum allowed size
    TooLarge,
    /// Peer has exceeded their quota
    PeerQuotaExceeded,
}

impl BlobRejectionReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::PeerQuotaExceeded => "peer_quota_exceeded",
        }
    }
}

/// Increment blob rejection counter with reason
///
/// Called when a blob announcement is rejected due to size limits or quotas.
pub fn blob_rejected_inc(reason: BlobRejectionReason) {
    counter!(
        "icn_network_blob_rejected_total",
        "reason" => reason.as_str()
    )
    .increment(1);
}

/// Increment blob eviction counter
///
/// Called when a blob entry is evicted to make room for new entries.
pub fn blob_evicted_inc() {
    counter!("icn_network_blob_evicted_total").increment(1);
}

/// Set the current total size of all blobs in the registry
pub fn blob_registry_size_set(bytes: u64) {
    gauge!("icn_network_blob_registry_size_bytes").set(bytes as f64);
}

/// Set the current number of blob location entries in the registry
pub fn blob_registry_entries_set(count: usize) {
    gauge!("icn_network_blob_registry_entries").set(count as f64);
}

// Sybil resistance metrics (Issue #675)

/// Type of Sybil detection event
#[derive(Debug, Clone, Copy)]
pub enum SybilDetectionType {
    /// Per-person (per-anchor) rate limit exceeded
    PerPersonLimit,
    /// DID has no personhood anchor and RequirePersonhood is enabled
    NoPersonhoodRequired,
    /// Multiple DIDs detected sharing same anchor (informational)
    MultipleDidsSameAnchor,
}

impl SybilDetectionType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PerPersonLimit => "per_person_limit",
            Self::NoPersonhoodRequired => "no_personhood_required",
            Self::MultipleDidsSameAnchor => "multiple_dids_same_anchor",
        }
    }
}

/// Increment per-anchor rate limited counter
///
/// Called when a message is rate limited due to per-person (per-anchor) limit.
/// This indicates Sybil resistance is working - multiple DIDs from the same
/// person are being aggregated.
pub fn messages_rate_limited_by_anchor_inc() {
    counter!("icn_network_messages_rate_limited_by_anchor_total").increment(1);
}

/// Increment Sybil detection counter with detection type
///
/// Called when a Sybil attack mitigation is triggered.
pub fn sybil_detection_inc(detection_type: SybilDetectionType) {
    counter!(
        "icn_sybil_detection_total",
        "type" => detection_type.as_str()
    )
    .increment(1);
}

/// Increment personhood required rejections counter
///
/// Called when a message is rejected because the sender has no personhood
/// anchor and RequirePersonhood enforcement mode is enabled.
pub fn personhood_required_rejections_inc() {
    counter!("icn_personhood_required_rejections_total").increment(1);
}

/// Set the number of unique personhood anchors with active rate limit buckets
pub fn personhood_anchors_active_set(count: usize) {
    gauge!("icn_personhood_anchors_active").set(count as f64);
}

/// Increment personhood store failure counter
///
/// Called when the personhood store lookup fails, causing rate limiting to
/// fall back to per-DID limiting only (Sybil protection degraded).
/// Operators should alert on this metric to detect storage issues.
pub fn personhood_store_failures_inc() {
    counter!("icn_personhood_store_failures_total").increment(1);
}

/// A bootstrap entry named a DID, and a different DID authenticated on that connection.
///
/// Counts *connections*, not Hellos: a peer may repeat its Hello, and the rate of a signal
/// about local configuration must not be settable by the remote end.
///
/// Deliberately unlabelled. The two values this is about — the configured DID and the
/// authenticated one — are unbounded, so as labels they would let bootstrap configuration
/// and whoever answers the address drive metric cardinality between them. They are recorded
/// in the accompanying `warn!`, which is a diagnostic record rather than a time series.
///
/// This is not a misbehaviour signal. A divergence means the operator's configuration and
/// the network disagree — a rotated key, a redeployed node, a copy-pasted address — and
/// says nothing bad about either DID.
pub fn bootstrap_did_expectation_mismatch_inc() {
    counter!("icn_network_bootstrap_did_expectation_mismatch_total").increment(1);
}
