//! Utility functions for test setup

use anyhow::Result;
use std::future::Future;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};

/// Type alias for test results
pub type TestResult<T = ()> = Result<T>;

/// Configuration for exponential backoff polling
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Initial delay between polls
    pub initial_delay: Duration,
    /// Maximum delay between polls
    pub max_delay: Duration,
    /// Multiplier for delay increase (default: 2.0)
    pub multiplier: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(200),
            multiplier: 2.0,
        }
    }
}

impl BackoffConfig {
    /// Create a new backoff config with custom initial and max delays
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial_delay: initial,
            max_delay: max,
            multiplier: 2.0,
        }
    }

    /// Set the multiplier for delay increase
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }
}

/// Poll a condition with exponential backoff until it returns true or timeout
///
/// This is useful for waiting on asynchronous conditions (connections, gossip
/// convergence, etc.) without using fixed delays that may be too short under
/// load or too long in fast environments.
///
/// # Arguments
///
/// * `check` - Async function returning `true` when the condition is met
/// * `config` - Backoff configuration (initial delay, max delay, multiplier)
/// * `timeout` - Maximum time to wait before returning an error
///
/// # Returns
///
/// `Ok(())` if the condition became true, `Err` if timeout was reached
///
/// # Example
///
/// ```rust,ignore
/// use icn_testkit::util::{poll_with_backoff, BackoffConfig};
///
/// poll_with_backoff(
///     || async { node.is_connected_to(&peer).await },
///     BackoffConfig::default(),
///     Duration::from_secs(5),
/// ).await?;
/// ```
pub async fn poll_with_backoff<F, Fut>(
    mut check: F,
    config: BackoffConfig,
    timeout: Duration,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = Instant::now();
    let mut delay = config.initial_delay;

    loop {
        if check().await {
            return Ok(());
        }

        if start.elapsed() >= timeout {
            anyhow::bail!(
                "Timeout after {:?} (polled with backoff from {:?} to {:?})",
                timeout,
                config.initial_delay,
                delay
            );
        }

        // Don't sleep longer than remaining time
        let remaining = timeout.saturating_sub(start.elapsed());
        let sleep_time = delay.min(remaining);

        tokio::time::sleep(sleep_time).await;

        // Increase delay with exponential backoff, capped at max
        delay = Duration::from_secs_f64(delay.as_secs_f64() * config.multiplier)
            .min(config.max_delay);
    }
}

static CRYPTO_INIT: Once = Once::new();

/// Atomic counter for fallback port allocation to avoid collisions
static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);

/// Install the rustls crypto provider (required for TLS/QUIC)
///
/// This is idempotent and safe to call multiple times.
/// Call this at the start of any test that uses networking.
pub fn install_crypto_provider() {
    CRYPTO_INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// Pick an available port for testing
///
/// Uses portpicker to find an unused port on the local machine.
/// Each call returns a different port.
pub fn pick_port() -> u16 {
    portpicker::pick_unused_port().unwrap_or_else(|| {
        // Fallback using atomic counter to avoid port collisions in parallel tests
        let counter = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
        30000 + (counter % 30000)
    })
}

/// Initialize tracing for tests
///
/// Sets up tracing-subscriber with a test writer.
/// Safe to call multiple times (subsequent calls are no-ops).
///
/// Note: Requires tracing-subscriber as a dev-dependency.
#[cfg(test)]
#[allow(dead_code)]
pub fn init_test_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_pick_port_returns_valid_ports() {
        // Pick several ports and verify they're in valid range
        let mut ports = HashSet::new();
        for _ in 0..10 {
            let port = pick_port();
            assert!(port > 1024, "Port {port} should be > 1024");
            ports.insert(port);
        }
        // Should have gotten multiple unique ports (may not be all 10 due to race conditions)
        assert!(
            ports.len() >= 5,
            "Should get at least 5 unique ports, got {}",
            ports.len()
        );
    }

    #[test]
    fn test_crypto_provider_is_idempotent() {
        // Should not panic when called multiple times
        install_crypto_provider();
        install_crypto_provider();
        install_crypto_provider();
    }

    #[test]
    fn test_backoff_config_defaults() {
        let config = BackoffConfig::default();
        assert_eq!(config.initial_delay, Duration::from_millis(10));
        assert_eq!(config.max_delay, Duration::from_millis(200));
        assert!((config.multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_backoff_config_builder() {
        let config = BackoffConfig::new(Duration::from_millis(5), Duration::from_millis(50))
            .with_multiplier(1.5);
        assert_eq!(config.initial_delay, Duration::from_millis(5));
        assert_eq!(config.max_delay, Duration::from_millis(50));
        assert!((config.multiplier - 1.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_poll_with_backoff_succeeds_immediately() {
        // Condition is immediately true
        let result = poll_with_backoff(
            || async { true },
            BackoffConfig::default(),
            Duration::from_secs(1),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_poll_with_backoff_succeeds_after_retries() {
        // Condition becomes true after a few polls
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let result = poll_with_backoff(
            move || {
                let c = counter_clone.clone();
                async move {
                    let count = c.fetch_add(1, Ordering::SeqCst);
                    count >= 3 // True on 4th poll (indices 0, 1, 2, 3)
                }
            },
            BackoffConfig::new(Duration::from_millis(1), Duration::from_millis(10)),
            Duration::from_secs(1),
        )
        .await;

        assert!(result.is_ok());
        assert!(counter.load(Ordering::SeqCst) >= 4);
    }

    #[tokio::test]
    async fn test_poll_with_backoff_times_out() {
        // Condition never becomes true
        let result = poll_with_backoff(
            || async { false },
            BackoffConfig::new(Duration::from_millis(5), Duration::from_millis(10)),
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Timeout"), "Expected timeout error: {err_msg}");
    }
}
