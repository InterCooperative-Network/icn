//! Ledger metrics
//!
//! Metrics for monitoring ledger operations, witness signatures, and transaction processing.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize ledger metric descriptions
pub fn init_descriptions() {
    // Witness signature metrics (Issue #676)
    describe_counter!(
        "icn_ledger_witnessed_entries_accepted_total",
        "Total number of witnessed entries accepted"
    );
    describe_counter!(
        "icn_ledger_witnessed_entries_rejected_total",
        "Total number of witnessed entries rejected"
    );
    describe_histogram!(
        "icn_ledger_witness_signature_count",
        "Number of signatures per witnessed entry"
    );
    // Note: Witness quarantine events are tracked via witnessed_entries_rejected_*
    // counters with specific reasons (invalid_signature, insufficient_signatures)

    // Trust-based entry rejection
    describe_counter!(
        "icn_ledger_entries_rejected_low_trust_total",
        "Total number of entries rejected due to low trust"
    );

    // Parent/orphan handling
    describe_counter!(
        "icn_ledger_missing_parents_total",
        "Total number of missing parent references encountered"
    );
    describe_counter!(
        "icn_ledger_entries_rejected_missing_parents_total",
        "Total number of entries rejected due to missing parents"
    );
    describe_counter!(
        "icn_ledger_orphan_entries_total",
        "Total number of orphan entries queued"
    );

    // Membership
    describe_counter!(
        "icn_ledger_new_members_registered_total",
        "Total number of new members registered"
    );
    describe_counter!(
        "icn_ledger_membership_storage_errors_total",
        "Total number of membership storage errors"
    );

    // Sync
    describe_gauge!(
        "icn_ledger_sync_lag_seconds",
        "Time lag in seconds between local and remote ledger state"
    );

    // Recompute/rollback
    describe_counter!(
        "icn_ledger_recompute_aborted_version_mismatch_total",
        "Total number of recomputes aborted due to version mismatch"
    );
    describe_counter!(
        "icn_ledger_rollback_performed_total",
        "Total number of ledger rollbacks performed"
    );

    // Credit limits
    describe_counter!(
        "icn_ledger_entries_rejected_credit_limit_total",
        "Total number of entries rejected due to credit limit"
    );
    describe_counter!(
        "icn_ledger_credit_limit_bypass_via_contribution_total",
        "Total number of credit limit bypasses via contribution"
    );

    // Progressive limits
    describe_counter!(
        "icn_ledger_progressive_balance_limit_exceeded_total",
        "Total number of progressive balance limit violations"
    );
    describe_counter!(
        "icn_ledger_progressive_velocity_limit_exceeded_total",
        "Total number of progressive velocity limit violations"
    );
    describe_counter!(
        "icn_ledger_progressive_pop_level_defaulted_total",
        "Total number of times PoP level defaulted"
    );

    // Dynamic limits
    describe_counter!(
        "icn_ledger_dynamic_limit_recovery_total",
        "Total number of dynamic limit recoveries"
    );
    describe_counter!(
        "icn_ledger_dynamic_limit_decay_applied_total",
        "Total number of dynamic limit decay applications"
    );
    describe_counter!(
        "icn_ledger_dynamic_limit_recalculations_total",
        "Total number of dynamic limit recalculations"
    );

    // Fork resolution
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
        "Total number of entries discarded"
    );
    describe_gauge!(
        "icn_ledger_quarantine_size",
        "Current number of entries in quarantine"
    );
}

// Witness signature metrics

/// Increment the count of witnessed entries accepted
pub fn witnessed_entries_accepted_inc() {
    counter!("icn_ledger_witnessed_entries_accepted_total").increment(1);
}

/// Increment witness entries rejected due to invalid signature
pub fn witnessed_entries_rejected_invalid_signature_inc() {
    counter!(
        "icn_ledger_witnessed_entries_rejected_total",
        "reason" => "invalid_signature"
    )
    .increment(1);
}

/// Increment witness entries rejected due to insufficient signatures
pub fn witnessed_entries_rejected_insufficient_inc() {
    counter!(
        "icn_ledger_witnessed_entries_rejected_total",
        "reason" => "insufficient_signatures"
    )
    .increment(1);
}

/// Record the number of signatures on a witnessed entry
pub fn witness_signature_count_record(count: u64) {
    histogram!("icn_ledger_witness_signature_count").record(count as f64);
}

// Trust-based rejection metrics

/// Increment entries rejected due to low trust
pub fn entries_rejected_low_trust_inc() {
    counter!("icn_ledger_entries_rejected_low_trust_total").increment(1);
}

// Parent/orphan handling metrics

/// Add to missing parents counter
pub fn missing_parents_add(count: u64) {
    counter!("icn_ledger_missing_parents_total").increment(count);
}

/// Increment entries rejected due to missing parents
pub fn entries_rejected_missing_parents_inc() {
    counter!("icn_ledger_entries_rejected_missing_parents_total").increment(1);
}

/// Increment orphan entries counter
pub fn orphan_entries_inc() {
    counter!("icn_ledger_orphan_entries_total").increment(1);
}

// Membership metrics

/// Increment new members registered counter
pub fn new_members_registered_inc() {
    counter!("icn_ledger_new_members_registered_total").increment(1);
}

/// Increment membership storage errors counter
pub fn membership_storage_errors_inc() {
    counter!("icn_ledger_membership_storage_errors_total").increment(1);
}

// Sync metrics

/// Set sync lag in seconds
pub fn sync_lag_seconds_set(lag: f64) {
    gauge!("icn_ledger_sync_lag_seconds").set(lag);
}

// Recompute/rollback metrics

/// Increment recompute aborted due to version mismatch
pub fn recompute_aborted_version_mismatch_inc() {
    counter!("icn_ledger_recompute_aborted_version_mismatch_total").increment(1);
}

/// Increment rollback performed counter
pub fn rollback_performed_inc() {
    counter!("icn_ledger_rollback_performed_total").increment(1);
}

// Credit limit metrics

/// Increment entries rejected due to credit limit
pub fn entries_rejected_credit_limit_inc() {
    counter!("icn_ledger_entries_rejected_credit_limit_total").increment(1);
}

/// Increment credit limit bypass via contribution counter
pub fn credit_limit_bypass_via_contribution_inc() {
    counter!("icn_ledger_credit_limit_bypass_via_contribution_total").increment(1);
}

// Progressive limit metrics

/// Increment progressive balance limit exceeded counter
pub fn progressive_balance_limit_exceeded_inc() {
    counter!("icn_ledger_progressive_balance_limit_exceeded_total").increment(1);
}

/// Increment progressive velocity limit exceeded counter
pub fn progressive_velocity_limit_exceeded_inc() {
    counter!("icn_ledger_progressive_velocity_limit_exceeded_total").increment(1);
}

/// Increment PoP level defaulted counter
pub fn progressive_pop_level_defaulted_inc() {
    counter!("icn_ledger_progressive_pop_level_defaulted_total").increment(1);
}

// Dynamic limit metrics

/// Increment dynamic limit recovery counter
pub fn dynamic_limit_recovery_inc() {
    counter!("icn_ledger_dynamic_limit_recovery_total").increment(1);
}

/// Increment dynamic limit decay applied counter
pub fn dynamic_limit_decay_applied_inc() {
    counter!("icn_ledger_dynamic_limit_decay_applied_total").increment(1);
}

/// Increment dynamic limit recalculations counter
pub fn dynamic_limit_recalculations_inc() {
    counter!("icn_ledger_dynamic_limit_recalculations_total").increment(1);
}

// Fork resolution metrics

/// Increment merge conflicts counter
pub fn merge_conflicts_inc() {
    counter!("icn_ledger_merge_conflicts_total").increment(1);
}

/// Increment entries quarantined counter
pub fn entries_quarantined_inc() {
    counter!("icn_ledger_entries_quarantined_total").increment(1);
}

/// Increment entries discarded counter
pub fn entries_discarded_inc() {
    counter!("icn_ledger_entries_discarded_total").increment(1);
}

/// Set quarantine size gauge
pub fn quarantine_size_set(count: u64) {
    gauge!("icn_ledger_quarantine_size").set(count as f64);
}
