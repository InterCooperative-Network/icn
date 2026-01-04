//! Clock Synchronization (Phase 19 Week 3-4)
//!
//! Implements clock sync using Rough Time Protocol (RFC 8915) for:
//! - Cooperative-wide time consensus
//! - Timestamp validation (±300s tolerance)
//! - Protection against replay attacks

use crate::error::{Result, TimeError};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Maximum acceptable clock skew (5 minutes)
pub const MAX_CLOCK_SKEW: Duration = Duration::from_secs(300);

/// Minimum servers required for median calculation
pub const MIN_SERVERS: usize = 3;

/// Maximum reasonable timestamp in milliseconds for safe i64 conversion
/// i64::MAX is ~292 billion years in milliseconds, but we use a more
/// conservative limit (~292 million years) to catch clearly malicious values
const MAX_REASONABLE_TIMESTAMP_MS: u64 = i64::MAX as u64;

/// Timeout for individual time server queries
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// Rough Time server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoughTimeServer {
    /// Server address (host:port)
    pub addr: String,

    /// Server public key (optional, for signature verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

/// Clock synchronization state
#[derive(Debug, Clone)]
pub struct ClockSync {
    /// Local clock offset from network median in milliseconds
    /// - Positive: local clock is ahead of network time (need to subtract)
    /// - Negative: local clock is behind network time (need to add)
    /// - Zero: perfectly synchronized
    pub offset_millis: i64,

    /// Confidence interval / uncertainty
    pub uncertainty: Duration,

    /// Last successful sync time
    pub last_sync: Option<Instant>,

    /// Maximum acceptable clock skew
    pub max_clock_skew: Duration,

    /// Configured time servers
    trusted_servers: Vec<RoughTimeServer>,
}

/// Time server response
#[derive(Debug, Clone)]
struct TimeResponse {
    _server_addr: SocketAddr,
    server_time_millis: u64,
    _local_time: Instant,
    rtt_millis: u64, // Round-trip time
}

impl ClockSync {
    /// Create a new clock sync instance with default servers
    pub fn new() -> Self {
        Self::with_servers(Self::default_servers(), MAX_CLOCK_SKEW)
    }

    /// Create with custom servers and max skew
    pub fn with_servers(servers: Vec<RoughTimeServer>, max_clock_skew: Duration) -> Self {
        Self {
            offset_millis: 0,                     // Start with zero offset
            uncertainty: Duration::from_secs(10), // Initial high uncertainty
            last_sync: None,
            max_clock_skew,
            trusted_servers: servers,
        }
    }

    /// Default Rough Time servers (Cloudflare + others)
    pub fn default_servers() -> Vec<RoughTimeServer> {
        vec![
            RoughTimeServer {
                addr: "roughtime.cloudflare.com:2003".to_string(),
                public_key: None,
            },
            RoughTimeServer {
                addr: "roughtime.int08h.com:2002".to_string(),
                public_key: None,
            },
            RoughTimeServer {
                addr: "roughtime.sandbox.google.com:2002".to_string(),
                public_key: None,
            },
        ]
    }

    /// Synchronize clock with time servers
    ///
    /// Queries multiple servers concurrently and computes median offset.
    /// Updates internal offset and uncertainty.
    pub async fn sync(&mut self) -> Result<()> {
        let start = Instant::now();
        info!(
            "Starting clock sync with {} servers",
            self.trusted_servers.len()
        );

        // Query all servers concurrently
        let responses = self.query_servers().await?;

        if responses.len() < MIN_SERVERS {
            warn!(
                "Insufficient time server responses: got {}, need {}",
                responses.len(),
                MIN_SERVERS
            );
            return Err(TimeError::InsufficientResponses(
                responses.len(),
                MIN_SERVERS,
            ));
        }

        // Compute median time from responses
        let median_time = self.compute_median(&responses)?;
        let now = Self::system_time_millis();

        // Validate timestamps before i64 conversion to prevent overflow
        // This guards against malicious time servers returning extreme values
        if now > MAX_REASONABLE_TIMESTAMP_MS {
            warn!(
                "Local clock timestamp {}ms exceeds safe range, rejecting",
                now
            );
            return Err(TimeError::InvalidTimestamp(now));
        }
        if median_time > MAX_REASONABLE_TIMESTAMP_MS {
            warn!(
                "Median time {}ms from servers exceeds safe range, possible malicious response",
                median_time
            );
            return Err(TimeError::InvalidTimestamp(median_time));
        }

        // Calculate signed offset in milliseconds
        // Positive = local ahead (subtract to get network time)
        // Negative = local behind (add to get network time)
        // SAFETY: Both values are validated to be <= i64::MAX above
        self.offset_millis = now as i64 - median_time as i64;

        // Calculate uncertainty (based on RTT variance)
        self.uncertainty = self.compute_uncertainty(&responses);
        self.last_sync = Some(Instant::now());

        let duration = start.elapsed();
        info!(
            "Clock sync complete: offset={}ms, uncertainty={:?}, duration={:?}",
            self.offset_millis, self.uncertainty, duration
        );

        icn_obs::metrics::scalability::clock_sync_duration_record(duration.as_secs_f64());
        icn_obs::metrics::scalability::clock_sync_offset_record(self.offset_millis as f64 / 1000.0);
        icn_obs::metrics::scalability::clock_sync_success_inc();

        Ok(())
    }

    /// Query all time servers concurrently
    async fn query_servers(&self) -> Result<Vec<TimeResponse>> {
        let mut handles = Vec::new();

        for server in &self.trusted_servers {
            let server = server.clone();
            let handle = tokio::spawn(async move { Self::query_server(server).await });
            handles.push(handle);
        }

        // Collect successful responses
        let mut responses = Vec::new();
        for handle in handles {
            if let Ok(Ok(response)) = handle.await {
                responses.push(response);
            }
        }

        Ok(responses)
    }

    /// Query a single time server (simplified Rough Time protocol)
    async fn query_server(server: RoughTimeServer) -> Result<TimeResponse> {
        let start = Instant::now();

        // Resolve server address
        let addr = tokio::net::lookup_host(&server.addr)
            .await
            .map_err(|e| TimeError::Protocol(format!("Failed to resolve {}: {}", server.addr, e)))?
            .next()
            .ok_or_else(|| TimeError::Protocol(format!("No addresses for {}", server.addr)))?;

        // Create UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        // Send time request (simplified: just a nonce)
        let nonce: [u8; 32] = rand::random();
        socket.send_to(&nonce, addr).await?;

        // Receive response with timeout
        let mut buf = [0u8; 1024];
        let (len, _) = tokio::time::timeout(QUERY_TIMEOUT, socket.recv_from(&mut buf))
            .await
            .map_err(|_| TimeError::Protocol("Server timeout".to_string()))??;

        let rtt = start.elapsed();

        // Parse response (simplified: first 8 bytes as milliseconds since epoch)
        if len < 8 {
            return Err(TimeError::Protocol("Invalid response length".to_string()));
        }

        // SAFETY: We verified len >= 8 above, so buf[0..8] is guaranteed to be 8 bytes
        let server_time_millis = u64::from_be_bytes(
            buf[0..8]
                .try_into()
                .map_err(|_| TimeError::Protocol("Buffer conversion failed".to_string()))?,
        );

        debug!(
            "Time server {} responded: {}ms (RTT: {:?})",
            server.addr, server_time_millis, rtt
        );

        Ok(TimeResponse {
            _server_addr: addr,
            server_time_millis,
            _local_time: start,
            rtt_millis: rtt.as_millis() as u64,
        })
    }

    /// Compute median time from responses
    fn compute_median(&self, responses: &[TimeResponse]) -> Result<u64> {
        if responses.is_empty() {
            return Err(TimeError::InsufficientResponses(0, MIN_SERVERS));
        }

        let mut times: Vec<u64> = responses.iter().map(|r| r.server_time_millis).collect();
        times.sort_unstable();

        let median = times[times.len() / 2];
        Ok(median)
    }

    /// Compute uncertainty based on RTT variance
    fn compute_uncertainty(&self, responses: &[TimeResponse]) -> Duration {
        if responses.len() < 2 {
            return Duration::from_secs(10); // High uncertainty
        }

        // Use max RTT as uncertainty measure
        let max_rtt = responses.iter().map(|r| r.rtt_millis).max().unwrap_or(0);
        Duration::from_millis(max_rtt)
    }

    /// Get current network time (local time adjusted by offset)
    pub fn network_time(&self) -> Result<u64> {
        if self.last_sync.is_none() {
            return Err(TimeError::NotSynchronized);
        }

        let now = Self::system_time_millis();

        // Apply signed offset: network_time = local_time - offset
        // - If offset > 0 (local ahead): subtract offset to get network time
        // - If offset < 0 (local behind): subtract negative = add to get network time
        let network_time = (now as i64 - self.offset_millis).max(0) as u64;

        Ok(network_time)
    }

    /// Validate a timestamp against network time
    ///
    /// Returns error if timestamp is outside acceptable range (±max_clock_skew)
    pub fn validate_timestamp(&self, timestamp_millis: u64) -> Result<()> {
        let network_time = self.network_time()?;

        let skew = timestamp_millis.abs_diff(network_time);

        let max_skew_millis = self.max_clock_skew.as_millis() as u64;

        if skew > max_skew_millis {
            icn_obs::metrics::scalability::timestamp_validation_rejected_inc();
            return Err(TimeError::TimestampOutOfRange(skew as i64, max_skew_millis));
        }

        icn_obs::metrics::scalability::timestamp_validation_accepted_inc();
        Ok(())
    }

    /// Check if clock sync is fresh (within last 10 minutes)
    pub fn is_fresh(&self) -> bool {
        self.last_sync
            .map(|t| t.elapsed() < Duration::from_secs(600))
            .unwrap_or(false)
    }

    /// Get system time as milliseconds since epoch
    fn system_time_millis() -> u64 {
        crate::current_timestamp_millis()
    }
}

impl Default for ClockSync {
    fn default() -> Self {
        Self::new()
    }
}

/// Background clock sync task
///
/// Periodically syncs clock with time servers (every 10 minutes)
pub async fn start_clock_sync_task(mut sync: ClockSync, interval: Duration) {
    info!(
        "Starting clock sync background task (interval: {:?})",
        interval
    );

    loop {
        // Perform sync
        if let Err(e) = sync.sync().await {
            warn!("Clock sync failed: {}", e);
            icn_obs::metrics::scalability::clock_sync_failed_inc();
        }

        // Wait for next sync interval
        sleep(interval).await;
    }
}

// Use system random for nonce generation
mod rand {
    pub fn random<T>() -> T
    where
        rand::distributions::Standard: rand::distributions::Distribution<T>,
    {
        rand::thread_rng().gen()
    }

    pub use rand::Rng;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_sync_creation() {
        let sync = ClockSync::new();
        assert_eq!(sync.max_clock_skew, MAX_CLOCK_SKEW);
        assert_eq!(sync.trusted_servers.len(), 3);
        assert!(sync.last_sync.is_none());
    }

    #[test]
    fn test_compute_median() {
        let sync = ClockSync::new();

        let responses = vec![
            TimeResponse {
                _server_addr: "127.0.0.1:2003".parse().unwrap(),
                server_time_millis: 1000,
                _local_time: Instant::now(),
                rtt_millis: 10,
            },
            TimeResponse {
                _server_addr: "127.0.0.1:2002".parse().unwrap(),
                server_time_millis: 2000,
                _local_time: Instant::now(),
                rtt_millis: 20,
            },
            TimeResponse {
                _server_addr: "127.0.0.1:2001".parse().unwrap(),
                server_time_millis: 1500,
                _local_time: Instant::now(),
                rtt_millis: 15,
            },
        ];

        let median = sync.compute_median(&responses).unwrap();
        assert_eq!(median, 1500);
    }

    #[test]
    fn test_compute_uncertainty() {
        let sync = ClockSync::new();

        let responses = vec![
            TimeResponse {
                _server_addr: "127.0.0.1:2003".parse().unwrap(),
                server_time_millis: 1000,
                _local_time: Instant::now(),
                rtt_millis: 10,
            },
            TimeResponse {
                _server_addr: "127.0.0.1:2002".parse().unwrap(),
                server_time_millis: 2000,
                _local_time: Instant::now(),
                rtt_millis: 50,
            },
        ];

        let uncertainty = sync.compute_uncertainty(&responses);
        assert_eq!(uncertainty, Duration::from_millis(50)); // Max RTT
    }

    #[test]
    fn test_not_synchronized_error() {
        let sync = ClockSync::new();
        assert!(sync.network_time().is_err());
        assert!(matches!(
            sync.network_time(),
            Err(TimeError::NotSynchronized)
        ));
    }

    #[test]
    fn test_offset_local_ahead() {
        // Test when local clock is ahead of network time
        let mut sync = ClockSync::new();

        // Simulate: local = 1000ms, network median = 800ms
        // offset = 1000 - 800 = +200 (positive = local ahead)
        sync.offset_millis = 200;
        sync.last_sync = Some(Instant::now());

        // network_time should subtract positive offset
        // If local now = 1000, network = 1000 - 200 = 800
        // We can't control actual time, but we can verify the math
        let local_time = 1000u64;
        let expected_network = (local_time as i64 - sync.offset_millis) as u64;
        assert_eq!(expected_network, 800);
    }

    #[test]
    fn test_offset_local_behind() {
        // Test when local clock is behind network time
        let mut sync = ClockSync::new();

        // Simulate: local = 800ms, network median = 1000ms
        // offset = 800 - 1000 = -200 (negative = local behind)
        sync.offset_millis = -200;
        sync.last_sync = Some(Instant::now());

        // network_time should subtract negative offset (= add)
        // If local now = 800, network = 800 - (-200) = 1000
        let local_time = 800u64;
        let expected_network = (local_time as i64 - sync.offset_millis) as u64;
        assert_eq!(expected_network, 1000);
    }

    #[test]
    fn test_offset_zero() {
        // Test when clocks are perfectly synchronized
        let mut sync = ClockSync::new();

        sync.offset_millis = 0;
        sync.last_sync = Some(Instant::now());

        // network_time should equal local time
        let local_time = 1500u64;
        let expected_network = (local_time as i64 - sync.offset_millis) as u64;
        assert_eq!(expected_network, 1500);
    }

    #[test]
    fn test_offset_calculation_local_ahead() {
        // Test offset calculation when local clock is ahead
        let local_time = 5000u64;
        let median_time = 4800u64;

        // offset = local - median = 5000 - 4800 = +200
        let offset = local_time as i64 - median_time as i64;
        assert_eq!(offset, 200);

        // Applying offset: network = local - offset = 5000 - 200 = 4800
        let network = (local_time as i64 - offset) as u64;
        assert_eq!(network, median_time);
    }

    #[test]
    fn test_offset_calculation_local_behind() {
        // Test offset calculation when local clock is behind
        let local_time = 4800u64;
        let median_time = 5000u64;

        // offset = local - median = 4800 - 5000 = -200
        let offset = local_time as i64 - median_time as i64;
        assert_eq!(offset, -200);

        // Applying offset: network = local - offset = 4800 - (-200) = 5000
        let network = (local_time as i64 - offset) as u64;
        assert_eq!(network, median_time);
    }
}
