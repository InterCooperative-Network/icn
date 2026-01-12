//! Utility functions for test setup

use anyhow::Result;
use std::sync::Once;

/// Type alias for test results
pub type TestResult<T = ()> = Result<T>;

static CRYPTO_INIT: Once = Once::new();

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
        // Fallback to a random high port if portpicker fails
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u16)
            .unwrap_or(12345);
        30000 + (seed % 30000)
    })
}

/// Initialize tracing for tests
///
/// Sets up tracing-subscriber with a test writer.
/// Safe to call multiple times (subsequent calls are no-ops).
///
/// Note: Requires tracing-subscriber as a dev-dependency.
#[cfg(test)]
pub fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap_or_else(|_| "info".parse().unwrap())),
        )
        .with_test_writer()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_port_returns_unique_ports() {
        let port1 = pick_port();
        let port2 = pick_port();
        let port3 = pick_port();

        // Ports should all be different
        assert_ne!(port1, port2);
        assert_ne!(port2, port3);
        assert_ne!(port1, port3);

        // Ports should be in valid range
        assert!(port1 > 1024);
        assert!(port2 > 1024);
        assert!(port3 > 1024);
    }

    #[test]
    fn test_crypto_provider_is_idempotent() {
        // Should not panic when called multiple times
        install_crypto_provider();
        install_crypto_provider();
        install_crypto_provider();
    }
}
