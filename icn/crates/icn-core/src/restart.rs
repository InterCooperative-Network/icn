//! Actor restart with exponential backoff.
//!
//! This module provides utilities for restarting failed actor tasks with
//! configurable exponential backoff to prevent resource exhaustion from
//! rapid restart loops.
//!
//! # Example
//!
//! ```ignore
//! let policy = RestartPolicy::from_config(&config.supervisor.restart_policy);
//! let mut tracker = RestartTracker::new(policy.clone());
//!
//! // In a spawn loop
//! loop {
//!     let result = actor_task().await;
//!     if result.is_err() {
//!         if let Some(delay) = tracker.should_restart("rpc_server") {
//!             tokio::time::sleep(delay).await;
//!             // Restart actor
//!         } else {
//!             // Max attempts exceeded, give up
//!             break;
//!         }
//!     }
//! }
//! ```

use crate::config::RestartPolicyConfig;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Restart policy with exponential backoff parameters.
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    /// Initial delay before first restart attempt
    pub initial_delay: Duration,
    /// Maximum delay between restart attempts
    pub max_delay: Duration,
    /// Backoff multiplier applied after each failure
    pub backoff_multiplier: f64,
    /// Maximum restart attempts within the restart window
    pub max_attempts: u32,
    /// Time window for counting restart attempts
    pub restart_window: Duration,
}

impl RestartPolicy {
    /// Create a restart policy from configuration.
    pub fn from_config(config: &RestartPolicyConfig) -> Self {
        Self {
            initial_delay: Duration::from_millis(config.initial_delay_ms),
            max_delay: Duration::from_millis(config.max_delay_ms),
            backoff_multiplier: config.backoff_multiplier,
            max_attempts: config.max_attempts,
            restart_window: Duration::from_secs(config.restart_window_secs),
        }
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::from_config(&RestartPolicyConfig::default())
    }
}

/// State for a single actor's restart tracking.
#[derive(Debug)]
struct ActorRestartState {
    /// Timestamps of recent restart attempts
    restart_times: Vec<Instant>,
    /// Current backoff delay
    current_delay: Duration,
    /// Number of consecutive failures
    consecutive_failures: u32,
}

impl ActorRestartState {
    fn new(initial_delay: Duration) -> Self {
        Self {
            restart_times: Vec::new(),
            current_delay: initial_delay,
            consecutive_failures: 0,
        }
    }

    /// Clean up restart times outside the window.
    fn cleanup_old_restarts(&mut self, window: Duration) {
        let cutoff = Instant::now() - window;
        self.restart_times.retain(|t| *t > cutoff);
    }

    /// Record a restart attempt.
    fn record_restart(&mut self) {
        self.restart_times.push(Instant::now());
        self.consecutive_failures += 1;
    }

    /// Reset state after successful recovery.
    pub fn reset(&mut self, initial_delay: Duration) {
        self.restart_times.clear();
        self.current_delay = initial_delay;
        self.consecutive_failures = 0;
    }
}

/// Tracks restart state for multiple actors.
pub struct RestartTracker {
    policy: RestartPolicy,
    actors: HashMap<String, ActorRestartState>,
}

impl RestartTracker {
    /// Create a new restart tracker with the given policy.
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            policy,
            actors: HashMap::new(),
        }
    }

    /// Check if an actor should be restarted and return the delay.
    ///
    /// Returns `Some(delay)` if the actor should be restarted after the delay.
    /// Returns `None` if max attempts exceeded within the restart window.
    pub fn should_restart(&mut self, actor_name: &str) -> Option<Duration> {
        let state = self
            .actors
            .entry(actor_name.to_string())
            .or_insert_with(|| ActorRestartState::new(self.policy.initial_delay));

        // Clean up old restart times
        state.cleanup_old_restarts(self.policy.restart_window);

        // Check if we've exceeded max attempts within the window
        if state.restart_times.len() >= self.policy.max_attempts as usize {
            warn!(
                actor = actor_name,
                attempts = state.restart_times.len(),
                window_secs = self.policy.restart_window.as_secs(),
                "Actor exceeded max restart attempts within window, not restarting"
            );
            icn_obs::metrics::supervisor::restart_limit_exceeded_inc(actor_name);
            return None;
        }

        // Record this restart attempt
        state.record_restart();

        // Calculate delay with exponential backoff
        let delay = state.current_delay;

        // Update delay for next attempt (with cap)
        let next_delay = Duration::from_secs_f64(
            (state.current_delay.as_secs_f64() * self.policy.backoff_multiplier)
                .min(self.policy.max_delay.as_secs_f64()),
        );
        state.current_delay = next_delay;

        info!(
            actor = actor_name,
            delay_ms = delay.as_millis(),
            attempt = state.consecutive_failures,
            max_attempts = self.policy.max_attempts,
            "Scheduling actor restart with backoff"
        );

        // Emit metrics
        icn_obs::metrics::supervisor::actor_restarted_inc(actor_name);
        icn_obs::metrics::supervisor::restart_backoff_set(actor_name, delay.as_secs_f64());

        Some(delay)
    }

    /// Mark an actor as successfully recovered (resets backoff).
    pub fn mark_recovered(&mut self, actor_name: &str) {
        if let Some(state) = self.actors.get_mut(actor_name) {
            state.reset(self.policy.initial_delay);
            info!(actor = actor_name, "Actor recovered, reset backoff state");
        }
    }

    /// Get the current consecutive failure count for an actor.
    pub fn consecutive_failures(&self, actor_name: &str) -> u32 {
        self.actors
            .get(actor_name)
            .map(|s| s.consecutive_failures)
            .unwrap_or(0)
    }

    /// Get the current backoff delay for an actor.
    pub fn current_delay(&self, actor_name: &str) -> Duration {
        self.actors
            .get(actor_name)
            .map(|s| s.current_delay)
            .unwrap_or(self.policy.initial_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy() -> RestartPolicy {
        RestartPolicy {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            max_attempts: 3,
            restart_window: Duration::from_secs(60),
        }
    }

    #[test]
    fn test_should_restart_with_backoff() {
        let mut tracker = RestartTracker::new(test_policy());

        // First restart should have initial delay
        let delay1 = tracker.should_restart("test_actor").unwrap();
        assert_eq!(delay1.as_millis(), 100);

        // Second restart should have doubled delay
        let delay2 = tracker.should_restart("test_actor").unwrap();
        assert_eq!(delay2.as_millis(), 200);

        // Third restart should have quadrupled delay
        let delay3 = tracker.should_restart("test_actor").unwrap();
        assert_eq!(delay3.as_millis(), 400);

        // Fourth restart should be denied (max_attempts=3)
        let delay4 = tracker.should_restart("test_actor");
        assert!(delay4.is_none());
    }

    #[test]
    fn test_backoff_caps_at_max() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            initial_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 3.0,
            max_attempts: 10,
            restart_window: Duration::from_secs(300),
        });

        // First: 5s
        let delay1 = tracker.should_restart("test_actor").unwrap();
        assert_eq!(delay1.as_secs(), 5);

        // Second: min(15, 10) = 10s
        let delay2 = tracker.should_restart("test_actor").unwrap();
        assert_eq!(delay2.as_secs(), 10);

        // Third: still capped at 10s
        let delay3 = tracker.should_restart("test_actor").unwrap();
        assert_eq!(delay3.as_secs(), 10);
    }

    #[test]
    fn test_mark_recovered_resets_state() {
        let mut tracker = RestartTracker::new(test_policy());

        // Cause some failures
        tracker.should_restart("test_actor");
        tracker.should_restart("test_actor");
        assert_eq!(tracker.consecutive_failures("test_actor"), 2);

        // Mark as recovered
        tracker.mark_recovered("test_actor");
        assert_eq!(tracker.consecutive_failures("test_actor"), 0);
        assert_eq!(tracker.current_delay("test_actor").as_millis(), 100);
    }

    #[test]
    fn test_independent_actors() {
        let mut tracker = RestartTracker::new(test_policy());

        // Actor A has failures
        tracker.should_restart("actor_a");
        tracker.should_restart("actor_a");

        // Actor B is fresh
        let delay_b = tracker.should_restart("actor_b").unwrap();
        assert_eq!(delay_b.as_millis(), 100); // Initial delay, not affected by A
    }

    #[test]
    fn test_from_config() {
        let config = RestartPolicyConfig {
            initial_delay_ms: 500,
            max_delay_ms: 60_000,
            backoff_multiplier: 1.5,
            max_attempts: 10,
            restart_window_secs: 120,
        };

        let policy = RestartPolicy::from_config(&config);
        assert_eq!(policy.initial_delay.as_millis(), 500);
        assert_eq!(policy.max_delay.as_secs(), 60);
        assert_eq!(policy.backoff_multiplier, 1.5);
        assert_eq!(policy.max_attempts, 10);
        assert_eq!(policy.restart_window.as_secs(), 120);
    }
}
