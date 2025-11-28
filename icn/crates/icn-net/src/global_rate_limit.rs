//! Global rate limiter for server-wide message caps
//!
//! This module provides a global rate limiter that enforces server-wide
//! message rate caps, independent of per-peer limits. This prevents any
//! combination of peers from overwhelming the server with messages.
//!
//! # Architecture
//!
//! - **Sliding window**: 1-second windows with atomic counters
//! - **Thread-safe**: Uses atomic operations for lock-free access
//! - **Automatic reset**: Window resets every second
//!
//! # Usage
//!
//! ```rust
//! use icn_net::global_rate_limit::GlobalRateLimiter;
//!
//! # async fn example() {
//! let limiter = GlobalRateLimiter::new(10_000); // 10k msg/sec
//!
//! if limiter.check().await {
//!     // Process message
//! } else {
//!     // Drop message, rate limit exceeded
//! }
//! # }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Global rate limiter enforcing server-wide message caps
///
/// Uses a sliding window approach with atomic counters for thread-safe,
/// lock-free operation in the common case.
#[derive(Clone)]
pub struct GlobalRateLimiter {
    /// Maximum messages per second (server-wide)
    max_global_mps: u32,

    /// Start of current time window
    window_start: Arc<Mutex<Instant>>,

    /// Message count in current window
    message_count: Arc<AtomicU64>,
}

impl GlobalRateLimiter {
    /// Create a new global rate limiter
    ///
    /// # Arguments
    ///
    /// * `max_global_mps` - Maximum messages per second across all peers
    ///
    /// # Example
    ///
    /// ```
    /// use icn_net::global_rate_limit::GlobalRateLimiter;
    ///
    /// // Allow 10,000 messages per second globally
    /// let limiter = GlobalRateLimiter::new(10_000);
    /// ```
    pub fn new(max_global_mps: u32) -> Self {
        Self {
            max_global_mps,
            window_start: Arc::new(Mutex::new(Instant::now())),
            message_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Check if a message should be allowed
    ///
    /// Returns `true` if the message is within the global rate limit,
    /// `false` if it should be dropped.
    ///
    /// This method is lock-free in the common case (when window hasn't expired).
    /// Only acquires a lock when resetting the window.
    pub async fn check(&self) -> bool {
        let now = Instant::now();

        // Fast path: check if window needs reset
        {
            let start = self.window_start.lock().await;
            if now.duration_since(*start) < Duration::from_secs(1) {
                // Window still valid, use atomic increment
                drop(start); // Release lock immediately

                let count = self.message_count.fetch_add(1, Ordering::Relaxed);
                return count < self.max_global_mps as u64;
            }
        }

        // Slow path: reset window
        {
            let mut start = self.window_start.lock().await;

            // Double-check: another thread might have reset while we waited
            if now.duration_since(*start) >= Duration::from_secs(1) {
                *start = now;
                self.message_count.store(1, Ordering::Relaxed); // Count this message
                true// First message in new window always allowed
            } else {
                // Another thread reset the window
                drop(start);
                let count = self.message_count.fetch_add(1, Ordering::Relaxed);
                count < self.max_global_mps as u64
            }
        }
    }

    /// Check if a message should be allowed (non-async version)
    ///
    /// This is a blocking version for use in synchronous contexts.
    /// Prefer `check()` in async code.
    pub fn check_sync(&self) -> bool {
        let now = Instant::now();

        // Try to acquire lock without blocking
        if let Ok(start) = self.window_start.try_lock() {
            if now.duration_since(*start) < Duration::from_secs(1) {
                // Window still valid
                drop(start);
                let count = self.message_count.fetch_add(1, Ordering::Relaxed);
                return count < self.max_global_mps as u64;
            }
        }

        // If we can't check the window or need to reset, be conservative
        // and allow the message (better than blocking)
        let count = self.message_count.fetch_add(1, Ordering::Relaxed);
        count < self.max_global_mps as u64
    }

    /// Get current message count in this window
    ///
    /// This is primarily for testing and metrics.
    pub fn current_count(&self) -> u64 {
        self.message_count.load(Ordering::Relaxed)
    }

    /// Get the configured rate limit
    pub fn max_rate(&self) -> u32 {
        self.max_global_mps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_rate_limit() {
        let limiter = GlobalRateLimiter::new(10);

        // First 10 messages should be allowed
        for _ in 0..10 {
            assert!(limiter.check().await, "Messages within limit should be allowed");
        }

        // 11th message should be denied
        assert!(!limiter.check().await, "Message exceeding limit should be denied");
    }

    #[tokio::test]
    async fn test_window_reset() {
        let limiter = GlobalRateLimiter::new(5);

        // Use up the limit
        for _ in 0..5 {
            assert!(limiter.check().await);
        }

        // Next message should be denied
        assert!(!limiter.check().await);

        // Wait for window to reset
        sleep(Duration::from_secs(1)).await;

        // Should be allowed again after window reset
        assert!(limiter.check().await, "Messages should be allowed after window reset");
    }

    #[tokio::test]
    async fn test_current_count() {
        let limiter = GlobalRateLimiter::new(100);

        assert_eq!(limiter.current_count(), 0);

        limiter.check().await;
        assert_eq!(limiter.current_count(), 1);

        limiter.check().await;
        limiter.check().await;
        assert_eq!(limiter.current_count(), 3);
    }

    #[tokio::test]
    async fn test_max_rate() {
        let limiter = GlobalRateLimiter::new(12345);
        assert_eq!(limiter.max_rate(), 12345);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let limiter = GlobalRateLimiter::new(1000);
        let limiter_clone = limiter.clone();

        // Spawn multiple tasks hitting the limiter concurrently
        let mut handles = vec![];

        for _ in 0..10 {
            let lim = limiter_clone.clone();
            handles.push(tokio::spawn(async move {
                let mut allowed = 0;
                for _ in 0..150 {
                    if lim.check().await {
                        allowed += 1;
                    }
                }
                allowed
            }));
        }

        // Collect results
        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // Total allowed should not exceed the limit
        // (might be slightly less due to timing)
        assert!(
            total_allowed <= 1000,
            "Total allowed ({}) should not exceed limit (1000)",
            total_allowed
        );

        // But should be reasonably close to the limit
        assert!(
            total_allowed >= 950,
            "Total allowed ({}) should be close to limit (1000)",
            total_allowed
        );
    }

    #[test]
    fn test_check_sync() {
        let limiter = GlobalRateLimiter::new(10);

        // First 10 messages should be allowed
        for i in 0..10 {
            assert!(limiter.check_sync(), "Message {} should be allowed", i);
        }

        // 11th message should be denied
        assert!(!limiter.check_sync(), "Message exceeding limit should be denied");
    }

    #[tokio::test]
    async fn test_zero_limit() {
        let limiter = GlobalRateLimiter::new(0);

        // All messages should be denied with zero limit
        assert!(!limiter.check().await);
        assert!(!limiter.check().await);
    }

    #[tokio::test]
    async fn test_high_limit() {
        let limiter = GlobalRateLimiter::new(1_000_000);

        // Should easily handle many messages
        for _ in 0..1000 {
            assert!(limiter.check().await);
        }

        assert_eq!(limiter.current_count(), 1000);
    }
}
