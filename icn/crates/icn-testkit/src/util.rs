//! Utility functions for test setup

use anyhow::Result;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Once;

/// Type alias for test results
pub type TestResult<T = ()> = Result<T>;

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
            "Should get at least 5 unique ports, got {}", ports.len()
        );
    }

    #[test]
    fn test_crypto_provider_is_idempotent() {
        // Should not panic when called multiple times
        install_crypto_provider();
        install_crypto_provider();
        install_crypto_provider();
    }
}
