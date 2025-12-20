//! Safe timestamp utilities
//!
//! Provides helper functions for getting current timestamps without panicking.
//! These functions handle the edge case where system time is before UNIX epoch
//! (which should never happen on a properly configured system, but we handle
//! it gracefully anyway).

use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp in seconds since UNIX epoch.
///
/// Returns 0 if system time is somehow before UNIX epoch (should never happen).
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Get current timestamp in milliseconds since UNIX epoch.
///
/// Returns 0 if system time is somehow before UNIX epoch (should never happen).
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Get current timestamp in nanoseconds since UNIX epoch.
///
/// Returns 0 if system time is somehow before UNIX epoch (should never happen).
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
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
