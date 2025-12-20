//! Consensus helpers for compute result verification.

use crate::types::{ComputeResult, ExecutionOutcome};

/// Convert an ExecutionOutcome to a CCL Value for dispute evidence
pub(crate) fn outcome_to_value(outcome: &ExecutionOutcome) -> icn_ccl::Value {
    match outcome {
        ExecutionOutcome::Success(data) => {
            // Convert bytes to hex string for dispute evidence
            icn_ccl::Value::String(format!("success:{}", hex::encode(data)))
        }
        ExecutionOutcome::Failed(error) => {
            icn_ccl::Value::String(format!("failed:{error}"))
        }
        ExecutionOutcome::OutOfFuel => {
            icn_ccl::Value::String("out_of_fuel".to_string())
        }
        ExecutionOutcome::Timeout => icn_ccl::Value::String("timeout".to_string()),
    }
}

/// Compare two compute results to check if they conflict
/// Returns true if the results match, false if they conflict
pub(crate) fn results_match(r1: &ComputeResult, r2: &ComputeResult) -> bool {
    match (&r1.outcome, &r2.outcome) {
        // Both success - compare output bytes
        (ExecutionOutcome::Success(data1), ExecutionOutcome::Success(data2)) => data1 == data2,
        // Both failed - compare error messages
        (ExecutionOutcome::Failed(err1), ExecutionOutcome::Failed(err2)) => err1 == err2,
        // Both out of fuel
        (ExecutionOutcome::OutOfFuel, ExecutionOutcome::OutOfFuel) => true,
        // Both timeout
        (ExecutionOutcome::Timeout, ExecutionOutcome::Timeout) => true,
        // Different outcome types = conflict
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskHash;

    fn make_result(outcome: ExecutionOutcome) -> ComputeResult {
        ComputeResult {
            task_hash: TaskHash::default(),
            task_id: "test-task".to_string(),
            executor: "test".to_string(),
            outcome,
            fuel_used: 100,
            duration_ms: 50,
            completed_at: 0,
            signature: vec![],
        }
    }

    #[test]
    fn test_results_match_both_success_same_data() {
        let data = vec![1, 2, 3, 4];
        let r1 = make_result(ExecutionOutcome::Success(data.clone()));
        let r2 = make_result(ExecutionOutcome::Success(data));
        assert!(results_match(&r1, &r2));
    }

    #[test]
    fn test_results_match_both_success_different_data() {
        let r1 = make_result(ExecutionOutcome::Success(vec![1, 2, 3]));
        let r2 = make_result(ExecutionOutcome::Success(vec![4, 5, 6]));
        assert!(!results_match(&r1, &r2));
    }

    #[test]
    fn test_results_match_success_vs_failed() {
        let r1 = make_result(ExecutionOutcome::Success(vec![1, 2, 3]));
        let r2 = make_result(ExecutionOutcome::Failed("error".to_string()));
        assert!(!results_match(&r1, &r2));
    }

    #[test]
    fn test_results_match_both_out_of_fuel() {
        let r1 = make_result(ExecutionOutcome::OutOfFuel);
        let r2 = make_result(ExecutionOutcome::OutOfFuel);
        assert!(results_match(&r1, &r2));
    }

    #[test]
    fn test_results_match_both_timeout() {
        let r1 = make_result(ExecutionOutcome::Timeout);
        let r2 = make_result(ExecutionOutcome::Timeout);
        assert!(results_match(&r1, &r2));
    }

    #[test]
    fn test_outcome_to_value_success() {
        let outcome = ExecutionOutcome::Success(vec![0xde, 0xad, 0xbe, 0xef]);
        let value = outcome_to_value(&outcome);
        assert_eq!(value, icn_ccl::Value::String("success:deadbeef".to_string()));
    }

    #[test]
    fn test_outcome_to_value_failed() {
        let outcome = ExecutionOutcome::Failed("test error".to_string());
        let value = outcome_to_value(&outcome);
        assert_eq!(
            value,
            icn_ccl::Value::String("failed:test error".to_string())
        );
    }

    #[test]
    fn test_outcome_to_value_out_of_fuel() {
        let outcome = ExecutionOutcome::OutOfFuel;
        let value = outcome_to_value(&outcome);
        assert_eq!(value, icn_ccl::Value::String("out_of_fuel".to_string()));
    }

    #[test]
    fn test_outcome_to_value_timeout() {
        let outcome = ExecutionOutcome::Timeout;
        let value = outcome_to_value(&outcome);
        assert_eq!(value, icn_ccl::Value::String("timeout".to_string()));
    }
}
