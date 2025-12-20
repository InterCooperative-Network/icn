//! ICN Time - Clock Synchronization (Phase 19 Week 3-4)
//!
//! Provides cooperative-wide clock sync using Rough Time Protocol (RFC 8915) for:
//! - Distributed timestamp validation
//! - Protection against replay attacks
//! - Clock skew detection and correction
//!
//! ## Example
//!
//! ```rust,no_run
//! use icn_time::{ClockSync, start_clock_sync_task};
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create clock sync with default servers
//! let mut sync = ClockSync::new();
//!
//! // Sync with time servers
//! sync.sync().await?;
//!
//! // Validate a timestamp
//! let timestamp_millis = 1700000000000;
//! sync.validate_timestamp(timestamp_millis)?;
//!
//! // Start background sync task (every 10 minutes)
//! tokio::spawn(start_clock_sync_task(sync.clone(), Duration::from_secs(600)));
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod sync;
mod timestamp;

pub use error::{Result, TimeError};
pub use sync::{
    start_clock_sync_task, ClockSync, RoughTimeServer, MAX_CLOCK_SKEW, MIN_SERVERS, QUERY_TIMEOUT,
};
pub use timestamp::{current_timestamp_millis, current_timestamp_secs};
