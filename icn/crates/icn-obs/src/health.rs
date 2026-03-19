//! Health check and monitoring endpoints
//!
//! ## Lock Poisoning Handling
//!
//! This module uses graceful degradation for lock poisoning. Health monitoring
//! is critical infrastructure - a failed health check could cause cascading
//! issues with load balancers and orchestrators. Better to report stale health
//! data than to fail the health endpoint entirely.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::error;

/// Health status of the ICN node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health status
    pub status: HealthState,
    /// Node uptime in seconds
    pub uptime_seconds: u64,
    /// Number of active network connections
    pub active_connections: u64,
    /// Number of gossip topics
    pub gossip_topics: u64,
    /// Number of ledger quarantine entries
    pub ledger_quarantine_size: u64,
    /// Timestamp of last health check
    pub timestamp: u64,
}

/// Current health state of the node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    /// All systems operational
    Healthy,
    /// Non-critical issues detected
    Degraded,
    /// Critical issues detected
    Unhealthy,
}

/// Shared health state
#[derive(Clone)]
pub struct HealthService {
    inner: Arc<RwLock<HealthStatus>>,
    start_time: SystemTime,
}

impl HealthService {
    /// Create a new health service
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HealthStatus {
                status: HealthState::Healthy,
                uptime_seconds: 0,
                active_connections: 0,
                gossip_topics: 0,
                ledger_quarantine_size: 0,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0))
                    .as_secs(),
            })),
            start_time: SystemTime::now(),
        }
    }

    /// Update health metrics
    pub fn update(&self, active_connections: u64, gossip_topics: u64, ledger_quarantine_size: u64) {
        // Use graceful degradation - skip update if lock is poisoned
        // Health monitoring must remain available for load balancers
        let mut status = match self.inner.write() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Health status lock poisoned in update - skipping update: {e}");
                return;
            }
        };

        status.uptime_seconds = self
            .start_time
            .elapsed()
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        status.active_connections = active_connections;
        status.gossip_topics = gossip_topics;
        status.ledger_quarantine_size = ledger_quarantine_size;
        status.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        // Determine health state based on metrics
        // Check most restrictive condition first
        status.status = if ledger_quarantine_size > 1000 {
            HealthState::Unhealthy
        } else if ledger_quarantine_size > 100 {
            HealthState::Degraded
        } else {
            HealthState::Healthy
        };
    }

    /// Get current health status
    pub fn get_status(&self) -> HealthStatus {
        // Use graceful degradation - return degraded status if lock is poisoned
        // Health monitoring must remain available for load balancers
        match self.inner.read() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                error!(
                    "Health status lock poisoned in get_status - returning degraded status: {e}"
                );
                // Return a degraded status to indicate something is wrong
                HealthStatus {
                    status: HealthState::Degraded,
                    uptime_seconds: self
                        .start_time
                        .elapsed()
                        .unwrap_or(Duration::from_secs(0))
                        .as_secs(),
                    active_connections: 0,
                    gossip_topics: 0,
                    ledger_quarantine_size: 0,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::from_secs(0))
                        .as_secs(),
                }
            }
        }
    }
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new()
    }
}

/// Health check handler
async fn health_check(State(health): State<HealthService>) -> impl IntoResponse {
    let status = health.get_status();

    let status_code = match status.status {
        HealthState::Healthy => StatusCode::OK,
        HealthState::Degraded => StatusCode::OK, // Still return 200 for degraded
        HealthState::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(status))
}

/// Dashboard HTML page
async fn dashboard() -> Html<&'static str> {
    Html(include_str!("../static/dashboard.html"))
}

/// Create monitoring router
pub fn monitoring_router(health: HealthService) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/", get(dashboard))
        .with_state(health)
}

/// Start monitoring server
pub async fn start_monitoring_server(port: u16, health: HealthService) -> anyhow::Result<()> {
    let app = monitoring_router(health);

    let addr = format!("[::]:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Monitoring server listening on http://{}", addr);
    tracing::info!("  - Health check: http://{}/health", addr);
    tracing::info!("  - Dashboard: http://{}/", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_state_healthy() {
        let health = HealthService::new();

        // Quarantine size <= 100 should be Healthy
        health.update(5, 10, 50);

        let status = health.get_status();
        assert_eq!(status.status, HealthState::Healthy);
        assert_eq!(status.ledger_quarantine_size, 50);
    }

    #[test]
    fn test_health_state_degraded() {
        let health = HealthService::new();

        // Quarantine size > 100 but <= 1000 should be Degraded
        health.update(5, 10, 500);

        let status = health.get_status();
        assert_eq!(status.status, HealthState::Degraded);
        assert_eq!(status.ledger_quarantine_size, 500);
    }

    #[test]
    fn test_health_state_unhealthy() {
        let health = HealthService::new();

        // Quarantine size > 1000 should be Unhealthy
        health.update(5, 10, 1500);

        let status = health.get_status();
        assert_eq!(status.status, HealthState::Unhealthy);
        assert_eq!(status.ledger_quarantine_size, 1500);
    }

    #[test]
    fn test_health_state_boundary_conditions() {
        let health = HealthService::new();

        // Test boundary: exactly 100 should be Healthy
        health.update(5, 10, 100);
        let status = health.get_status();
        assert_eq!(status.status, HealthState::Healthy);

        // Test boundary: 101 should be Degraded
        health.update(5, 10, 101);
        let status = health.get_status();
        assert_eq!(status.status, HealthState::Degraded);

        // Test boundary: exactly 1000 should be Degraded
        health.update(5, 10, 1000);
        let status = health.get_status();
        assert_eq!(status.status, HealthState::Degraded);

        // Test boundary: 1001 should be Unhealthy
        health.update(5, 10, 1001);
        let status = health.get_status();
        assert_eq!(status.status, HealthState::Unhealthy);
    }

    // ========== Lock Poisoning Tests ==========

    #[test]
    fn test_lock_poison_graceful_degradation() {
        // Test that poisoned locks result in graceful degradation, not errors
        let health = HealthService::new();

        // Set initial state
        health.update(5, 10, 50);
        let status = health.get_status();
        assert_eq!(status.status, HealthState::Healthy);

        // Poison the lock by panicking while holding it
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = health.inner.write().unwrap();
            panic!("intentional panic to poison lock");
        }));

        // Verify the lock is actually poisoned
        assert!(
            health.inner.write().is_err(),
            "inner lock should be poisoned after panic"
        );

        // get_status should still work and return degraded status
        let status = health.get_status();
        assert_eq!(
            status.status,
            HealthState::Degraded,
            "should return degraded status when lock is poisoned"
        );

        // update should not panic - it should gracefully skip
        health.update(10, 20, 100); // Should not panic

        // get_status should still work
        let status = health.get_status();
        assert_eq!(
            status.status,
            HealthState::Degraded,
            "should still return degraded status"
        );
    }
}
