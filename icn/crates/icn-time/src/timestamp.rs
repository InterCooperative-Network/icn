//! Safe timestamp utilities
//!
//! Provides helper functions for getting current timestamps without panicking.
//! These functions handle the edge case where system time is before UNIX epoch
//! (which should never happen on a properly configured system, but we handle
//! it gracefully anyway).
//!
//! ## Error Handling
//!
//! Two variants are provided for each timestamp function:
//! - `current_timestamp_*()` - Returns 0 on error (backward compatible)
//! - `try_current_timestamp_*()` - Returns `Result<_, TimeError>` for explicit error handling

use crate::TimeError;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Get current timestamp in seconds since UNIX epoch.
///
/// Returns 0 if system time is somehow before UNIX epoch (should never happen).
/// A warning is logged when this fallback is used, as it may break replay detection.
///
/// For explicit error handling, use [`try_current_timestamp_secs`] instead.
///
/// # Example
///
/// ```
/// use icn_time::current_timestamp_secs;
///
/// let now = current_timestamp_secs();
/// assert!(now > 1700000000); // After Nov 2023
/// ```
#[inline]
pub fn current_timestamp_secs() -> u64 {
    match try_current_timestamp_secs() {
        Ok(ts) => ts,
        Err(e) => {
            warn!("System clock error, returning 0: {}", e);
            0
        }
    }
}

/// Get current timestamp in seconds since UNIX epoch (fallible).
///
/// Returns an error if system time is before UNIX epoch.
///
/// # Example
///
/// ```
/// use icn_time::try_current_timestamp_secs;
///
/// let now = try_current_timestamp_secs().expect("system clock valid");
/// assert!(now > 1700000000); // After Nov 2023
/// ```
#[inline]
pub fn try_current_timestamp_secs() -> Result<u64, TimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| TimeError::SystemClockError)
}

/// Get current timestamp in milliseconds since UNIX epoch.
///
/// Returns 0 if system time is somehow before UNIX epoch (should never happen).
/// A warning is logged when this fallback is used, as it may break replay detection.
///
/// For explicit error handling, use [`try_current_timestamp_millis`] instead.
///
/// # Example
///
/// ```
/// use icn_time::current_timestamp_millis;
///
/// let now = current_timestamp_millis();
/// assert!(now > 1700000000000); // After Nov 2023
/// ```
#[inline]
pub fn current_timestamp_millis() -> u64 {
    match try_current_timestamp_millis() {
        Ok(ts) => ts,
        Err(e) => {
            warn!("System clock error, returning 0: {}", e);
            0
        }
    }
}

/// Get current timestamp in milliseconds since UNIX epoch (fallible).
///
/// Returns an error if system time is before UNIX epoch.
///
/// # Example
///
/// ```
/// use icn_time::try_current_timestamp_millis;
///
/// let now = try_current_timestamp_millis().expect("system clock valid");
/// assert!(now > 1700000000000); // After Nov 2023
/// ```
#[inline]
pub fn try_current_timestamp_millis() -> Result<u64, TimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|_| TimeError::SystemClockError)
}

/// Get current timestamp in nanoseconds since UNIX epoch.
///
/// Returns 0 if system time is somehow before UNIX epoch (should never happen).
/// A warning is logged when this fallback is used, as it may break replay detection.
///
/// For explicit error handling, use [`try_current_timestamp_nanos`] instead.
///
/// # Example
///
/// ```
/// use icn_time::current_timestamp_nanos;
///
/// let now = current_timestamp_nanos();
/// assert!(now > 1700000000000000000); // After Nov 2023
/// ```
#[inline]
pub fn current_timestamp_nanos() -> u128 {
    match try_current_timestamp_nanos() {
        Ok(ts) => ts,
        Err(e) => {
            warn!("System clock error, returning 0: {}", e);
            0
        }
    }
}

/// Get current timestamp in nanoseconds since UNIX epoch (fallible).
///
/// Returns an error if system time is before UNIX epoch.
///
/// # Example
///
/// ```
/// use icn_time::try_current_timestamp_nanos;
///
/// let now = try_current_timestamp_nanos().expect("system clock valid");
/// assert!(now > 1700000000000000000); // After Nov 2023
/// ```
#[inline]
pub fn try_current_timestamp_nanos() -> Result<u128, TimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .map_err(|_| TimeError::SystemClockError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp_secs() {
        let now = current_timestamp_secs();
        // Should be after 2023-11-14
        assert!(now > 1700000000);
        // Should be before year 3000
        assert!(now < 32503680000);
    }

    #[test]
    fn test_current_timestamp_millis() {
        let now = current_timestamp_millis();
        // Should be after 2023-11-14
        assert!(now > 1700000000000);
        // Should be before year 3000
        assert!(now < 32503680000000);
    }

    #[test]
    fn test_millis_greater_than_secs() {
        let secs = current_timestamp_secs();
        let millis = current_timestamp_millis();
        // Millis should be approximately 1000x secs
        assert!(millis >= secs * 1000);
        assert!(millis < (secs + 1) * 1000);
    }
}
