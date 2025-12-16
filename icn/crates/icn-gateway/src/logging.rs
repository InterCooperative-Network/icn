//! Logging configuration for the ICN Gateway
//!
//! Provides structured logging with JSON output for production environments
//! and human-readable output for development.

use std::env;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging subsystem
///
/// Configures tracing-subscriber with either JSON or pretty-print formatting
/// based on the LOG_FORMAT environment variable.
///
/// # Environment Variables
///
/// - `LOG_FORMAT`: Set to "json" for JSON output, anything else for pretty-print (default: pretty)
/// - `RUST_LOG`: Standard log level filter (default: info)
///
/// # Examples
///
/// ```bash
/// # JSON output for production
/// LOG_FORMAT=json RUST_LOG=info cargo run
///
/// # Pretty output for development
/// RUST_LOG=debug cargo run
/// ```
pub fn init() {
    let log_format = env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

    // Default to info level if RUST_LOG not set
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if log_format == "json" {
        // JSON formatter for production/log aggregation
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_thread_names(true),
            )
            .init();
    } else {
        // Pretty formatter for development
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_thread_names(true),
            )
            .init();
    }
}
