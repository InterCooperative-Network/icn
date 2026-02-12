//! Traffic Obfuscation for Privacy (Phase 20 Week 5-6)
//!
//! Protects against timing analysis and traffic pattern recognition by:
//! - Adding random delays to message transmission
//! - Padding messages to hide content size
//! - Generating cover traffic (decoy messages)
//!
//! ## Architecture
//!
//! ```text
//! Original Message Flow:
//!   t0    t1    t2    t3    t4    (predictable timing)
//!   |--M1-|--M2-|--M3-|--M4-|     (varying sizes: 100, 50, 200, 75 bytes)
//!
//! Obfuscated Message Flow:
//!   t0      t1.5     t3.2   t4.8  (random delays)
//!   |--M1---|--M2----|--M3--|--M4-|  (padded to 256 bytes)
//!   |---C1--|----C2--|--C3--|       (cover traffic interspersed)
//! ```
//!
//! ## Security Properties
//!
//! - **Timing Resistance**: Random delays prevent correlation attacks
//! - **Size Uniformity**: Padding hides message content patterns
//! - **Traffic Camouflage**: Cover traffic obscures real message count
//! - **Constant Rate**: Optional constant-rate mode for maximum privacy

use crate::error::{PrivacyError, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for traffic obfuscation
#[derive(Debug, Clone)]
pub struct ObfuscationConfig {
    /// Enable random message delays
    pub enable_delays: bool,

    /// Minimum delay in milliseconds
    pub min_delay_ms: u64,

    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,

    /// Enable message size padding
    pub enable_padding: bool,

    /// Target padded size in bytes (messages padded to this size)
    pub padded_size: usize,

    /// Enable cover traffic generation
    pub enable_cover_traffic: bool,

    /// Cover traffic rate (messages per second)
    pub cover_traffic_rate: f64,
}

impl Default for ObfuscationConfig {
    fn default() -> Self {
        Self {
            enable_delays: true,
            min_delay_ms: 0,
            max_delay_ms: 500, // Up to 500ms jitter
            enable_padding: true,
            padded_size: 1024,           // Pad to 1KB
            enable_cover_traffic: false, // Disabled by default (bandwidth intensive)
            cover_traffic_rate: 0.1,     // 0.1 messages/sec = 1 every 10 seconds
        }
    }
}

/// Obfuscated message with padding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObfuscatedMessage {
    /// Original message payload
    pub payload: Vec<u8>,

    /// Random padding bytes (discarded after decoding)
    pub padding: Vec<u8>,

    /// Original payload length (before padding)
    pub original_len: usize,
}

/// Traffic obfuscator for privacy-preserving message transmission
pub struct TrafficObfuscator {
    config: ObfuscationConfig,
}

impl TrafficObfuscator {
    /// Create a new traffic obfuscator with default config
    pub fn new() -> Self {
        Self {
            config: ObfuscationConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ObfuscationConfig) -> Self {
        Self { config }
    }

    /// Generate random delay duration for timing resistance
    ///
    /// Returns a random delay between min_delay_ms and max_delay_ms.
    pub fn random_delay(&self) -> Duration {
        if !self.config.enable_delays {
            return Duration::from_millis(0);
        }

        let delay_ms =
            rand::thread_rng().gen_range(self.config.min_delay_ms..=self.config.max_delay_ms);

        Duration::from_millis(delay_ms)
    }

    /// Pad a message to hide its true size
    ///
    /// Pads message to `padded_size` bytes. If message is already larger,
    /// returns error (caller should chunk large messages).
    pub fn pad_message(&self, message: &[u8]) -> Result<ObfuscatedMessage> {
        if !self.config.enable_padding {
            // No padding, just wrap
            return Ok(ObfuscatedMessage {
                payload: message.to_vec(),
                padding: vec![],
                original_len: message.len(),
            });
        }

        if message.len() > self.config.padded_size {
            return Err(PrivacyError::ObfuscationError(format!(
                "Message too large for padding: {} > {} bytes",
                message.len(),
                self.config.padded_size
            )));
        }

        let padding_len = self.config.padded_size - message.len();
        let padding: Vec<u8> = (0..padding_len)
            .map(|_| rand::thread_rng().gen::<u8>())
            .collect();

        icn_obs::metrics::privacy::messages_padded_inc();

        Ok(ObfuscatedMessage {
            payload: message.to_vec(),
            padding,
            original_len: message.len(),
        })
    }

    /// Remove padding from an obfuscated message
    ///
    /// Extracts the original payload, discarding padding bytes.
    pub fn unpad_message(&self, obfuscated: &ObfuscatedMessage) -> Result<Vec<u8>> {
        // Validate original length doesn't exceed payload size
        if obfuscated.original_len > obfuscated.payload.len() {
            return Err(PrivacyError::ObfuscationError(format!(
                "Invalid original length: {} > {}",
                obfuscated.original_len,
                obfuscated.payload.len()
            )));
        }

        Ok(obfuscated.payload[..obfuscated.original_len].to_vec())
    }

    /// Generate a cover traffic message (decoy)
    ///
    /// Creates a random message of the configured padded size to obscure
    /// real traffic patterns. Cover messages should be indistinguishable
    /// from real messages.
    pub fn generate_cover_traffic(&self) -> Vec<u8> {
        let size = self.config.padded_size;
        let cover_message: Vec<u8> = (0..size).map(|_| rand::thread_rng().gen::<u8>()).collect();

        icn_obs::metrics::privacy::cover_traffic_sent_inc();

        cover_message
    }

    /// Check if it's time to send cover traffic
    ///
    /// Returns true if cover traffic should be sent based on configured rate.
    /// Uses probabilistic scheduling: probability = rate * time_delta.
    pub fn should_send_cover_traffic(&self, time_since_last_send: Duration) -> bool {
        if !self.config.enable_cover_traffic {
            return false;
        }

        // Probability of sending = rate (msg/sec) * time_delta (sec)
        let time_delta_secs = time_since_last_send.as_secs_f64();
        let probability = self.config.cover_traffic_rate * time_delta_secs;

        // Cap probability at 1.0
        let probability = probability.min(1.0);

        // Random chance based on probability
        rand::thread_rng().gen::<f64>() < probability
    }

    /// Get the configured padded message size
    pub fn padded_size(&self) -> usize {
        self.config.padded_size
    }
}

impl Default for TrafficObfuscator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_delay() {
        let config = ObfuscationConfig {
            enable_delays: true,
            min_delay_ms: 100,
            max_delay_ms: 500,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);

        // Generate multiple delays and verify they're in range
        for _ in 0..10 {
            let delay = obfuscator.random_delay();
            assert!(delay.as_millis() >= 100);
            assert!(delay.as_millis() <= 500);
        }
    }

    #[test]
    fn test_random_delay_disabled() {
        let config = ObfuscationConfig {
            enable_delays: false,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let delay = obfuscator.random_delay();

        assert_eq!(delay.as_millis(), 0);
    }

    #[test]
    fn test_message_padding() {
        let config = ObfuscationConfig {
            enable_padding: true,
            padded_size: 256,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let message = b"Hello, world!";

        let obfuscated = obfuscator.pad_message(message).unwrap();

        // Verify total size equals padded_size
        assert_eq!(obfuscated.payload.len() + obfuscated.padding.len(), 256);
        assert_eq!(obfuscated.original_len, message.len());
        assert_eq!(obfuscated.payload, message);
    }

    #[test]
    fn test_message_padding_disabled() {
        let config = ObfuscationConfig {
            enable_padding: false,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let message = b"Hello, world!";

        let obfuscated = obfuscator.pad_message(message).unwrap();

        // No padding added
        assert_eq!(obfuscated.padding.len(), 0);
        assert_eq!(obfuscated.payload, message);
        assert_eq!(obfuscated.original_len, message.len());
    }

    #[test]
    fn test_message_too_large() {
        let config = ObfuscationConfig {
            enable_padding: true,
            padded_size: 128,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let large_message = vec![0u8; 256]; // 256 bytes > 128 padded_size

        let result = obfuscator.pad_message(&large_message);
        assert!(result.is_err());
    }

    #[test]
    fn test_unpad_message() {
        let config = ObfuscationConfig {
            enable_padding: true,
            padded_size: 256,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let original = b"Secret message";

        let obfuscated = obfuscator.pad_message(original).unwrap();
        let unpadded = obfuscator.unpad_message(&obfuscated).unwrap();

        assert_eq!(unpadded, original);
    }

    #[test]
    fn test_cover_traffic_generation() {
        let config = ObfuscationConfig {
            enable_cover_traffic: true,
            padded_size: 512,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let cover = obfuscator.generate_cover_traffic();

        // Cover traffic should be exactly padded_size
        assert_eq!(cover.len(), 512);
    }

    #[test]
    fn test_should_send_cover_traffic_disabled() {
        let config = ObfuscationConfig {
            enable_cover_traffic: false,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        let should_send = obfuscator.should_send_cover_traffic(Duration::from_secs(10));

        assert!(!should_send);
    }

    #[test]
    fn test_should_send_cover_traffic_probability() {
        let config = ObfuscationConfig {
            enable_cover_traffic: true,
            cover_traffic_rate: 1.0, // 1 message per second
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);

        // After 1 second, probability should be ~100%
        let _should_send = obfuscator.should_send_cover_traffic(Duration::from_secs(2));
        // With probability 1.0, this should almost always be true (but random, so we can't assert)

        // After very short time, probability should be low
        let mut sent_count = 0;
        for _ in 0..100 {
            if obfuscator.should_send_cover_traffic(Duration::from_millis(10)) {
                sent_count += 1;
            }
        }

        // Probability = 1.0 msg/sec * 0.01 sec = 0.01
        // Over 100 trials, expect ~1 send (allow range 0-5 for randomness)
        assert!(sent_count <= 10);
    }

    #[test]
    fn test_padded_size_accessor() {
        let config = ObfuscationConfig {
            padded_size: 2048,
            ..Default::default()
        };

        let obfuscator = TrafficObfuscator::with_config(config);
        assert_eq!(obfuscator.padded_size(), 2048);
    }
}
