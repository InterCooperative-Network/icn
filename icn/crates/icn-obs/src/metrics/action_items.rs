//! Action Items metrics
//!
//! Prometheus metrics for governance action item tracking.
//! Tracks action item creation, status changes, and completion patterns.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

/// Initialize action item metric descriptions
pub fn init_descriptions() {
    // Counter metrics
    describe_counter!(
        "icn_action_items_created_total",
        "Total number of action items created by priority"
    );
    describe_counter!(
        "icn_action_items_completed_total",
        "Total number of action items completed"
    );
    describe_counter!(
        "icn_action_items_deferred_total",
        "Total number of action items deferred"
    );
    describe_counter!(
        "icn_action_items_deleted_total",
        "Total number of action items deleted"
    );
    describe_counter!(
        "icn_action_items_status_changes_total",
        "Total number of status transitions"
    );

    // Gauge metrics
    describe_gauge!(
        "icn_action_items_pending",
        "Current number of pending action items"
    );
    describe_gauge!(
        "icn_action_items_in_progress",
        "Current number of in-progress action items"
    );
    describe_gauge!(
        "icn_action_items_overdue",
        "Current number of overdue action items"
    );

    // Histogram metrics
    describe_histogram!(
        "icn_action_items_time_to_complete_seconds",
        "Time from action item creation to completion in seconds"
    );
    describe_histogram!(
        "icn_action_items_days_overdue",
        "Distribution of days overdue when items are completed"
    );
}

// Counter metrics

/// Increment action items created counter
pub fn action_items_created_inc(priority: &str) {
    counter!(
        "icn_action_items_created_total",
        "priority" => priority.to_string()
    )
    .increment(1);
}

/// Increment action items completed counter
pub fn action_items_completed_inc() {
    counter!("icn_action_items_completed_total").increment(1);
}

/// Increment action items deferred counter
pub fn action_items_deferred_inc() {
    counter!("icn_action_items_deferred_total").increment(1);
}

/// Increment action items deleted counter
pub fn action_items_deleted_inc() {
    counter!("icn_action_items_deleted_total").increment(1);
}

/// Increment status transitions counter
pub fn action_items_status_change_inc(from_status: &str, to_status: &str) {
    counter!(
        "icn_action_items_status_changes_total",
        "from_status" => from_status.to_string(),
        "to_status" => to_status.to_string()
    )
    .increment(1);
}

// Gauge metrics

/// Set current number of pending action items
pub fn action_items_pending_set(count: usize) {
    gauge!("icn_action_items_pending").set(count as f64);
}

/// Set current number of in-progress action items
pub fn action_items_in_progress_set(count: usize) {
    gauge!("icn_action_items_in_progress").set(count as f64);
}

/// Set current number of overdue action items
pub fn action_items_overdue_set(count: usize) {
    gauge!("icn_action_items_overdue").set(count as f64);
}

// Histogram metrics

/// Record time from action item creation to completion
pub fn action_items_time_to_complete_observe(duration_secs: f64) {
    histogram!("icn_action_items_time_to_complete_seconds").record(duration_secs);
}

/// Record how many days overdue an item was when completed
pub fn action_items_days_overdue_observe(days_overdue: f64) {
    histogram!("icn_action_items_days_overdue").record(days_overdue);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_metrics() {
        // Test creation by priority
        action_items_created_inc("High");
        action_items_created_inc("Medium");
        action_items_created_inc("Low");

        // Test status counters
        action_items_completed_inc();
        action_items_deferred_inc();
        action_items_deleted_inc();

        // Test status transitions
        action_items_status_change_inc("Pending", "InProgress");
        action_items_status_change_inc("InProgress", "Completed");
        action_items_status_change_inc("Pending", "Deferred");
    }

    #[test]
    fn test_gauge_metrics() {
        action_items_pending_set(10);
        action_items_in_progress_set(3);
        action_items_overdue_set(2);

        // Test zero counts
        action_items_pending_set(0);
        action_items_in_progress_set(0);
        action_items_overdue_set(0);
    }

    #[test]
    fn test_histogram_metrics() {
        // Test time to complete
        action_items_time_to_complete_observe(3600.0); // 1 hour
        action_items_time_to_complete_observe(86400.0); // 1 day
        action_items_time_to_complete_observe(604800.0); // 1 week

        // Test days overdue
        action_items_days_overdue_observe(0.0); // Completed on time
        action_items_days_overdue_observe(1.5); // 1.5 days overdue
        action_items_days_overdue_observe(7.0); // 1 week overdue
    }
}
