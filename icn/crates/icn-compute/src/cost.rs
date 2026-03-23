//! Compute cost calculation and credit enforcement.
//!
//! This module provides the cost gate for task submission, calculating the
//! minimum credits required to execute a task based on fuel limits and payment rates.

use crate::types::ComputeTask;

/// Default fuel cost divisor (1_000 fuel units = 1 credit cost).
///
/// This divisor matches CommonsPoolPolicy's default and determines the base
/// cost per fuel unit. The governance-configurable divisor is a planned
/// follow-on enhancement (Phase TBD).
const DEFAULT_FUEL_COST_DIVISOR: i64 = 1_000;

/// Compute the minimum credits required for task submission.
///
/// This is the **V1 scheduling-time cost gate** formula. It determines whether
/// a submitter has sufficient credit balance to submit a task, preventing
/// resource exhaustion and enforcing economic discipline.
///
/// # Formula
///
/// - If `payment_rate` is set: `base = (fuel_limit * payment_rate) / DEFAULT_FUEL_COST_DIVISOR`
/// - If `payment_rate` is None: `base = fuel_limit / DEFAULT_FUEL_COST_DIVISOR`
/// - Final cost: `base.max(1)` — ensures even tiny tasks cost at least 1 credit
///
/// # Implementation Notes
///
/// - The `DEFAULT_FUEL_COST_DIVISOR` (1_000) is hardcoded to match CommonsPoolPolicy's default.
///   A governance-configurable divisor is a planned follow-on.
/// - The `.max(1)` floor covers task overhead (scheduling, gossip, verification) for very cheap tasks.
///   Without it, free-tier tasks could exhaust resources via volume.
///
/// # Examples
///
/// ```rust,no_run
/// use icn_compute::types::{ComputeTask, FuelLimit};
/// use icn_compute::cost::compute_credits_required;
///
/// let task = ComputeTask {
///     fuel_limit: FuelLimit(10_000),
///     payment_rate: None,
///     ..Default::default()
/// };
/// assert_eq!(compute_credits_required(&task), 10);
///
/// let task_with_rate = ComputeTask {
///     fuel_limit: FuelLimit(10_000),
///     payment_rate: Some(2),
///     ..Default::default()
/// };
/// assert_eq!(compute_credits_required(&task_with_rate), 20);
///
/// let tiny_task = ComputeTask {
///     fuel_limit: FuelLimit(500),
///     payment_rate: None,
///     ..Default::default()
/// };
/// assert_eq!(compute_credits_required(&tiny_task), 1);  // floor applied
/// ```
pub fn compute_credits_required(task: &ComputeTask) -> i64 {
    let base = match task.payment_rate {
        Some(rate) => (task.fuel_limit.0 as i64 * rate as i64) / DEFAULT_FUEL_COST_DIVISOR,
        None => task.fuel_limit.0 as i64 / DEFAULT_FUEL_COST_DIVISOR,
    };

    base.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComputeTask, FuelLimit, TaskId, TaskCode, TaskPriority};

    /// Helper to construct a minimal test task.
    fn test_task(fuel_limit: u64, payment_rate: Option<u64>) -> ComputeTask {
        ComputeTask {
            id: TaskId::from("test_task"),
            submitter: "did:icn:test".to_string(),
            coop_id: None,
            code: TaskCode::Ccl(vec![]),
            inputs: vec![],
            fuel_limit: FuelLimit(fuel_limit),
            required_capabilities: vec![],
            priority: TaskPriority::default(),
            created_at: 0,
            deadline: None,
            payment_rate,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
        }
    }

    #[test]
    fn test_cost_no_rate_normal_fuel() {
        // fuel_limit=10_000, no payment_rate → 10_000 / 1_000 = 10
        let task = test_task(10_000, None);
        assert_eq!(compute_credits_required(&task), 10);
    }

    #[test]
    fn test_cost_no_rate_tiny_fuel() {
        // fuel_limit=500, no payment_rate → 500 / 1_000 = 0, floored to 1
        let task = test_task(500, None);
        assert_eq!(compute_credits_required(&task), 1);
    }

    #[test]
    fn test_cost_with_rate() {
        // fuel_limit=10_000, payment_rate=2 → (10_000 * 2) / 1_000 = 20
        let task = test_task(10_000, Some(2));
        assert_eq!(compute_credits_required(&task), 20);
    }
}
