//! Governance metrics
//!
//! Metrics for monitoring governance operations, proposals, voting, and cleanup.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize governance metric descriptions
pub fn init_descriptions() {
    // Proposal lifecycle metrics
    describe_counter!(
        "icn_governance_proposals_created_total",
        "Total number of proposals created"
    );
    describe_counter!(
        "icn_governance_proposals_opened_total",
        "Total number of proposals opened for voting"
    );
    describe_counter!(
        "icn_governance_proposals_closed_total",
        "Total number of proposals closed (by outcome label)"
    );
    describe_gauge!(
        "icn_governance_proposals_active",
        "Current number of active proposals"
    );

    // Voting metrics
    describe_counter!(
        "icn_governance_votes_cast_total",
        "Total number of votes cast (by vote_type label)"
    );
    describe_counter!(
        "icn_governance_delegated_votes_total",
        "Total number of votes cast via delegation"
    );

    // Cleanup metrics (proposal_cleanup.rs)
    describe_counter!(
        "icn_governance_proposals_archived_total",
        "Total number of proposals archived during cleanup"
    );
    describe_counter!(
        "icn_governance_archives_deleted_total",
        "Total number of proposal archives deleted during cleanup"
    );
    describe_counter!(
        "icn_governance_cleanup_errors_total",
        "Total number of errors during proposal cleanup"
    );
    describe_gauge!(
        "icn_governance_cleanup_scanned",
        "Number of proposals scanned in last cleanup run"
    );

    // Domain metrics
    describe_counter!(
        "icn_governance_domains_created_total",
        "Total number of governance domains created"
    );
    describe_gauge!(
        "icn_governance_domains_active",
        "Current number of active governance domains"
    );

    // Orphan cleanup metrics (orphan_cleanup.rs)
    describe_counter!(
        "icn_governance_orphan_cleanup_run_total",
        "Total number of orphan cleanup runs"
    );
    describe_counter!(
        "icn_governance_orphan_parameters_found_total",
        "Total number of orphan parameters found"
    );
    describe_counter!(
        "icn_governance_orphan_parameters_deleted_total",
        "Total number of orphan parameters deleted"
    );
    describe_counter!(
        "icn_governance_orphan_cleanup_dry_run_total",
        "Total number of orphan cleanup dry runs"
    );
    describe_counter!(
        "icn_governance_orphan_cleanup_failures_total",
        "Total number of orphan cleanup failures"
    );

    // Delegation metrics (delegation.rs)
    describe_histogram!(
        "icn_governance_delegation_reconciliation_duration_seconds",
        "Duration of delegation reconciliation in seconds"
    );
    describe_counter!(
        "icn_governance_delegation_cycle_detected_total",
        "Total number of delegation cycles detected"
    );
    describe_counter!(
        "icn_governance_delegation_cycles_found_total",
        "Total number of delegation cycles found"
    );

    // Execution metrics (governance_handlers.rs)
    describe_counter!(
        "icn_governance_execution_failures_total",
        "Total number of proposal execution failures"
    );
    describe_counter!(
        "icn_governance_proposals_executed_total",
        "Total number of proposals successfully executed"
    );
    describe_histogram!(
        "icn_governance_execution_duration_seconds",
        "Duration of proposal execution in seconds"
    );
    describe_counter!(
        "icn_governance_idempotent_skips_total",
        "Total number of idempotent execution skips"
    );
    describe_counter!(
        "icn_governance_audit_failures_total",
        "Total number of audit trail failures"
    );

    // Voting outcome metrics (actor.rs)
    describe_histogram!(
        "icn_governance_participation_percentage",
        "Participation percentage in proposals"
    );
    describe_histogram!(
        "icn_governance_quorum_margin",
        "Margin to quorum threshold"
    );
    describe_counter!(
        "icn_governance_quorum_not_met_total",
        "Total number of proposals failing quorum"
    );
    describe_counter!(
        "icn_governance_emergency_quorum_not_met_total",
        "Total number of emergency proposals failing quorum"
    );
    describe_counter!(
        "icn_governance_proposals_vetoed_total",
        "Total number of proposals vetoed"
    );
    describe_counter!(
        "icn_governance_proposals_force_closed_total",
        "Total number of proposals force-closed"
    );
    describe_counter!(
        "icn_governance_domain_config_updated_total",
        "Total number of domain configuration updates"
    );
    describe_counter!(
        "icn_governance_membership_updated_total",
        "Total number of membership updates"
    );
    describe_counter!(
        "icn_governance_scope_overlap_storage_errors_total",
        "Total number of scope overlap storage errors"
    );
    describe_counter!(
        "icn_governance_deserialization_failures_total",
        "Total number of deserialization failures"
    );
}

// Proposal lifecycle metrics

/// Increment proposals created counter
pub fn proposals_created_inc() {
    counter!("icn_governance_proposals_created_total").increment(1);
}

/// Increment proposals opened counter
pub fn proposals_opened_inc() {
    counter!("icn_governance_proposals_opened_total").increment(1);
}

/// Increment proposals closed counter with outcome label
pub fn proposals_closed_inc(outcome: &str) {
    counter!("icn_governance_proposals_closed_total", "outcome" => outcome.to_string()).increment(1);
}

/// Set active proposals gauge
pub fn proposals_active_set(count: u64) {
    gauge!("icn_governance_proposals_active").set(count as f64);
}

// Voting metrics

/// Increment votes cast counter with vote type label
pub fn votes_cast_inc(vote_type: &str) {
    counter!("icn_governance_votes_cast_total", "vote_type" => vote_type.to_string()).increment(1);
}

/// Increment delegated votes counter
pub fn delegated_votes_inc() {
    counter!("icn_governance_delegated_votes_total").increment(1);
}

// Cleanup metrics

/// Increment proposals archived counter by amount
pub fn proposals_archived_inc_by(count: u64) {
    counter!("icn_governance_proposals_archived_total").increment(count);
}

/// Increment archives deleted counter by amount
pub fn archives_deleted_inc_by(count: u64) {
    counter!("icn_governance_archives_deleted_total").increment(count);
}

/// Increment cleanup errors counter
pub fn cleanup_errors_inc() {
    counter!("icn_governance_cleanup_errors_total").increment(1);
}

/// Increment cleanup errors counter by amount
pub fn cleanup_errors_inc_by(count: u64) {
    counter!("icn_governance_cleanup_errors_total").increment(count);
}

/// Set cleanup scanned gauge
pub fn cleanup_scanned_set(count: u64) {
    gauge!("icn_governance_cleanup_scanned").set(count as f64);
}

// Domain metrics

/// Increment domains created counter
pub fn domains_created_inc() {
    counter!("icn_governance_domains_created_total").increment(1);
}

/// Set active domains gauge
pub fn domains_active_set(count: u64) {
    gauge!("icn_governance_domains_active").set(count as f64);
}

// Orphan cleanup metrics

/// Increment orphan cleanup run counter
pub fn orphan_cleanup_run_inc() {
    counter!("icn_governance_orphan_cleanup_run_total").increment(1);
}

/// Add to orphan parameters found counter
pub fn orphan_parameters_found_add(count: u64) {
    counter!("icn_governance_orphan_parameters_found_total").increment(count);
}

/// Add to orphan parameters deleted counter
pub fn orphan_parameters_deleted_add(count: u64) {
    counter!("icn_governance_orphan_parameters_deleted_total").increment(count);
}

/// Increment orphan cleanup dry run counter
pub fn orphan_cleanup_dry_run_inc() {
    counter!("icn_governance_orphan_cleanup_dry_run_total").increment(1);
}

/// Increment orphan cleanup failures counter
pub fn orphan_cleanup_failures_inc() {
    counter!("icn_governance_orphan_cleanup_failures_total").increment(1);
}

// Delegation metrics

/// Record delegation reconciliation duration
pub fn delegation_reconciliation_duration_observe(duration_seconds: f64) {
    histogram!("icn_governance_delegation_reconciliation_duration_seconds").record(duration_seconds);
}

/// Increment delegation cycle detected counter
pub fn delegation_cycle_detected_inc() {
    counter!("icn_governance_delegation_cycle_detected_total").increment(1);
}

/// Add to delegation cycles found counter
pub fn delegation_cycles_found_add(count: u64) {
    counter!("icn_governance_delegation_cycles_found_total").increment(count);
}

// Execution metrics

/// Increment execution failures counter with reason label
pub fn execution_failures_inc(reason: &str) {
    counter!("icn_governance_execution_failures_total", "reason" => reason.to_string()).increment(1);
}

/// Increment proposals executed counter with payload type label
pub fn proposals_executed_inc(payload_type: &str) {
    counter!("icn_governance_proposals_executed_total", "payload_type" => payload_type.to_string()).increment(1);
}

/// Record proposal execution duration with payload type label
pub fn execution_duration_record(payload_type: &str, duration_seconds: f64) {
    histogram!("icn_governance_execution_duration_seconds", "payload_type" => payload_type.to_string()).record(duration_seconds);
}

/// Increment idempotent skips counter
pub fn idempotent_skips_inc() {
    counter!("icn_governance_idempotent_skips_total").increment(1);
}

/// Increment audit failures counter
pub fn audit_failures_inc() {
    counter!("icn_governance_audit_failures_total").increment(1);
}

// Voting outcome metrics

/// Record participation percentage with proposal type label
pub fn participation_percentage_observe(proposal_type: &str, percentage: f64) {
    histogram!("icn_governance_participation_percentage", "proposal_type" => proposal_type.to_string()).record(percentage);
}

/// Record quorum margin with proposal type label
pub fn quorum_margin_observe(proposal_type: &str, margin: f64) {
    histogram!("icn_governance_quorum_margin", "proposal_type" => proposal_type.to_string()).record(margin);
}

/// Increment quorum not met counter with proposal type label
pub fn quorum_not_met_inc(proposal_type: &str) {
    counter!("icn_governance_quorum_not_met_total", "proposal_type" => proposal_type.to_string()).increment(1);
}

/// Increment emergency quorum not met counter with emergency type label
pub fn emergency_quorum_not_met_inc(emergency_type: &str) {
    counter!("icn_governance_emergency_quorum_not_met_total", "emergency_type" => emergency_type.to_string()).increment(1);
}

/// Increment proposals vetoed counter
pub fn proposals_vetoed_inc() {
    counter!("icn_governance_proposals_vetoed_total").increment(1);
}

/// Increment proposals force closed counter
pub fn proposals_force_closed_inc() {
    counter!("icn_governance_proposals_force_closed_total").increment(1);
}

/// Increment domain config updated counter
pub fn domain_config_updated_inc() {
    counter!("icn_governance_domain_config_updated_total").increment(1);
}

/// Increment membership updated counter
pub fn membership_updated_inc() {
    counter!("icn_governance_membership_updated_total").increment(1);
}

/// Increment scope overlap storage errors counter
pub fn scope_overlap_storage_errors_inc() {
    counter!("icn_governance_scope_overlap_storage_errors_total").increment(1);
}

/// Increment deserialization failures counter
pub fn deserialization_failures_inc() {
    counter!("icn_governance_deserialization_failures_total").increment(1);
}
