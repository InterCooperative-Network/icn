//! App Runtime metrics
//!
//! Metrics for app lifecycle, dispatcher, and event processing.

use metrics::{counter, describe_counter, describe_histogram, histogram};

/// Initialize app metrics descriptions
pub fn init_descriptions() {
    // Dispatcher metrics
    describe_counter!(
        "icn_apps_dispatch_events_total",
        "Total number of events dispatched to reducers"
    );
    describe_counter!(
        "icn_apps_dispatch_events_failed_total",
        "Total number of failed event dispatches (by error type)"
    );
    describe_counter!(
        "icn_apps_dispatch_requests_total",
        "Total number of requests handled by services"
    );
    describe_counter!(
        "icn_apps_dispatch_requests_failed_total",
        "Total number of failed request dispatches"
    );
    describe_histogram!(
        "icn_apps_dispatch_event_duration_seconds",
        "Time taken to dispatch and process an event"
    );
    describe_histogram!(
        "icn_apps_dispatch_request_duration_seconds",
        "Time taken to dispatch and handle a request"
    );

    // App lifecycle metrics
    describe_counter!("icn_apps_installed_total", "Total number of apps installed");
    describe_counter!(
        "icn_apps_uninstalled_total",
        "Total number of apps uninstalled"
    );
    describe_counter!("icn_apps_started_total", "Total number of app starts");
    describe_counter!("icn_apps_stopped_total", "Total number of app stops");
    describe_counter!(
        "icn_apps_shutdown_timeout_total",
        "Total number of app shutdown timeouts"
    );

    // State snapshot metrics
    describe_histogram!(
        "icn_apps_snapshot_duration_seconds",
        "Time taken to create a state snapshot for reducer invocation"
    );
    describe_histogram!(
        "icn_apps_snapshot_keys_count",
        "Number of keys included in a state snapshot"
    );
}

/// Increment dispatch events total counter.
pub fn dispatch_events_total_inc(app_id: &str, event_type: &str) {
    counter!("icn_apps_dispatch_events_total", "app_id" => app_id.to_owned(), "event_type" => event_type.to_owned()).increment(1);
}

/// Increment dispatch events failed counter with error type.
pub fn dispatch_events_failed_total_inc(app_id: &str, event_type: &str, error_type: &str) {
    counter!(
        "icn_apps_dispatch_events_failed_total",
        "app_id" => app_id.to_owned(),
        "event_type" => event_type.to_owned(),
        "error" => error_type.to_owned()
    )
    .increment(1);
}

/// Increment dispatch requests total counter.
pub fn dispatch_requests_total_inc(app_id: &str, request_type: &str) {
    counter!("icn_apps_dispatch_requests_total", "app_id" => app_id.to_owned(), "request_type" => request_type.to_owned()).increment(1);
}

/// Increment dispatch requests failed counter.
pub fn dispatch_requests_failed_total_inc(app_id: &str, request_type: &str, error_type: &str) {
    counter!(
        "icn_apps_dispatch_requests_failed_total",
        "app_id" => app_id.to_owned(),
        "request_type" => request_type.to_owned(),
        "error" => error_type.to_owned()
    )
    .increment(1);
}

/// Record event dispatch duration.
pub fn dispatch_event_duration_observe(app_id: &str, event_type: &str, duration_secs: f64) {
    histogram!(
        "icn_apps_dispatch_event_duration_seconds",
        "app_id" => app_id.to_owned(),
        "event_type" => event_type.to_owned()
    )
    .record(duration_secs);
}

/// Record request dispatch duration.
pub fn dispatch_request_duration_observe(app_id: &str, request_type: &str, duration_secs: f64) {
    histogram!(
        "icn_apps_dispatch_request_duration_seconds",
        "app_id" => app_id.to_owned(),
        "request_type" => request_type.to_owned()
    )
    .record(duration_secs);
}

/// Increment apps installed counter.
pub fn apps_installed_total_inc() {
    counter!("icn_apps_installed_total").increment(1);
}

/// Increment apps uninstalled counter.
pub fn apps_uninstalled_total_inc() {
    counter!("icn_apps_uninstalled_total").increment(1);
}

/// Increment apps started counter.
pub fn apps_started_total_inc() {
    counter!("icn_apps_started_total").increment(1);
}

/// Increment apps stopped counter.
pub fn apps_stopped_total_inc() {
    counter!("icn_apps_stopped_total").increment(1);
}

/// Increment shutdown timeout counter.
pub fn apps_shutdown_timeout_total_inc() {
    counter!("icn_apps_shutdown_timeout_total").increment(1);
}

/// Record state snapshot creation duration.
pub fn snapshot_duration_observe(app_id: &str, duration_secs: f64) {
    histogram!(
        "icn_apps_snapshot_duration_seconds",
        "app_id" => app_id.to_owned()
    )
    .record(duration_secs);
}

/// Record state snapshot key count.
pub fn snapshot_keys_count_observe(app_id: &str, key_count: u64) {
    histogram!(
        "icn_apps_snapshot_keys_count",
        "app_id" => app_id.to_owned()
    )
    .record(key_count as f64);
}
