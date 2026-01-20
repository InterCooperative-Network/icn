//! Storage metrics
//!
//! Metrics for monitoring storage backend health and performance.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Storage challenge result types
pub enum ChallengeResult {
    /// Proof was generated and sent successfully
    ProofSent,
    /// Content was not found, not-found response sent
    ContentNotFound,
    /// Challenge was expired
    Expired,
    /// Challenge was for a different target
    WrongTarget,
    /// Invalid protocol version
    InvalidVersion,
    /// Signature verification failed
    SignatureFailed,
    /// No keypair available to sign proof
    NoKeypair,
    /// Error generating Merkle proof
    ProofError,
}

impl ChallengeResult {
    fn as_str(&self) -> &'static str {
        match self {
            ChallengeResult::ProofSent => "proof_sent",
            ChallengeResult::ContentNotFound => "content_not_found",
            ChallengeResult::Expired => "expired",
            ChallengeResult::WrongTarget => "wrong_target",
            ChallengeResult::InvalidVersion => "invalid_version",
            ChallengeResult::SignatureFailed => "signature_failed",
            ChallengeResult::NoKeypair => "no_keypair",
            ChallengeResult::ProofError => "proof_error",
        }
    }
}

/// Storage proof verification result types
pub enum ProofVerifyResult {
    /// Verification successful
    Valid,
    /// Signature verification failed
    InvalidSignature,
    /// Content-not-found response received
    NotFound,
    /// Content-not-found response too old (replay attack)
    NotFoundExpired,
    /// Content-not-found response unsigned
    NotFoundUnsigned,
    /// Scheduler not configured
    NoScheduler,
}

impl ProofVerifyResult {
    fn as_str(&self) -> &'static str {
        match self {
            ProofVerifyResult::Valid => "valid",
            ProofVerifyResult::InvalidSignature => "invalid_signature",
            ProofVerifyResult::NotFound => "not_found",
            ProofVerifyResult::NotFoundExpired => "not_found_expired",
            ProofVerifyResult::NotFoundUnsigned => "not_found_unsigned",
            ProofVerifyResult::NoScheduler => "no_scheduler",
        }
    }
}

/// Initialize storage metric descriptions
pub fn init_descriptions() {
    describe_gauge!(
        "icn_storage_size_bytes",
        "Current size of the storage backend on disk in bytes"
    );
    describe_gauge!(
        "icn_storage_space_amplification",
        "Space amplification factor (actual size / logical size)"
    );
    describe_counter!(
        "icn_storage_flush_total",
        "Total number of storage flush operations"
    );
    describe_counter!(
        "icn_storage_flush_bytes_total",
        "Total bytes flushed to disk"
    );
    describe_histogram!(
        "icn_storage_flush_duration_seconds",
        "Duration of flush operations in seconds"
    );
    describe_counter!(
        "icn_storage_operations_total",
        "Total storage operations by type"
    );
    // Maintenance metrics
    describe_counter!(
        "icn_storage_maintenance_runs_total",
        "Total number of maintenance runs"
    );
    describe_histogram!(
        "icn_storage_maintenance_duration_seconds",
        "Duration of maintenance operations in seconds"
    );
    describe_counter!(
        "icn_storage_maintenance_space_reclaimed_bytes",
        "Total bytes reclaimed by maintenance"
    );
    describe_counter!(
        "icn_storage_maintenance_errors_total",
        "Total number of maintenance errors"
    );

    // Storage challenge metrics
    describe_counter!(
        "icn_storage_challenges_received_total",
        "Total number of storage challenges received"
    );
    describe_counter!(
        "icn_storage_challenge_results_total",
        "Total challenge handling results by type (proof_sent, content_not_found, expired, etc.)"
    );
    describe_counter!(
        "icn_storage_proofs_received_total",
        "Total number of storage proofs received"
    );
    describe_counter!(
        "icn_storage_proof_verification_total",
        "Total proof verification results by type (valid, invalid_signature, not_found, etc.)"
    );
}

/// Set the current storage size in bytes
pub fn size_bytes_set(bytes: u64) {
    gauge!("icn_storage_size_bytes").set(bytes as f64);
}

/// Set the current space amplification factor
pub fn space_amplification_set(factor: f64) {
    gauge!("icn_storage_space_amplification").set(factor);
}

/// Increment flush operations counter
pub fn flush_total_inc() {
    counter!("icn_storage_flush_total").increment(1);
}

/// Add to total bytes flushed
pub fn flush_bytes_add(bytes: u64) {
    counter!("icn_storage_flush_bytes_total").increment(bytes);
}

/// Record a flush duration observation
pub fn flush_duration_record(seconds: f64) {
    histogram!("icn_storage_flush_duration_seconds").record(seconds);
}

/// Increment storage operations counter by type
pub fn operations_inc(operation: &str) {
    counter!(
        "icn_storage_operations_total",
        "operation" => operation.to_string()
    )
    .increment(1);
}

/// Record a get operation
pub fn get_inc() {
    operations_inc("get");
}

/// Record a put operation
pub fn put_inc() {
    operations_inc("put");
}

/// Record a delete operation
pub fn delete_inc() {
    operations_inc("delete");
}

/// Record a scan operation
pub fn scan_inc() {
    operations_inc("scan");
}

// Maintenance metrics

/// Increment maintenance runs counter
pub fn maintenance_runs_inc() {
    counter!("icn_storage_maintenance_runs_total").increment(1);
}

/// Record maintenance duration
pub fn maintenance_duration_record(seconds: f64) {
    histogram!("icn_storage_maintenance_duration_seconds").record(seconds);
}

/// Add to space reclaimed counter
pub fn maintenance_space_reclaimed_add(bytes: u64) {
    counter!("icn_storage_maintenance_space_reclaimed_bytes").increment(bytes);
}

/// Increment maintenance errors counter
pub fn maintenance_errors_inc() {
    counter!("icn_storage_maintenance_errors_total").increment(1);
}

// Storage challenge metrics

/// Increment storage challenges received counter
pub fn challenges_received_inc() {
    counter!("icn_storage_challenges_received_total").increment(1);
}

/// Record a challenge handling result
pub fn challenge_result_inc(result: ChallengeResult) {
    counter!(
        "icn_storage_challenge_results_total",
        "result" => result.as_str().to_string()
    )
    .increment(1);
}

/// Increment storage proofs received counter
pub fn proofs_received_inc() {
    counter!("icn_storage_proofs_received_total").increment(1);
}

/// Record a proof verification result
pub fn proof_verification_inc(result: ProofVerifyResult) {
    counter!(
        "icn_storage_proof_verification_total",
        "result" => result.as_str().to_string()
    )
    .increment(1);
}
