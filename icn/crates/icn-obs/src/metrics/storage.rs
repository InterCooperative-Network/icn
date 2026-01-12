//! Storage metrics
//!
//! Metrics for monitoring storage backend health and performance.

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};

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
