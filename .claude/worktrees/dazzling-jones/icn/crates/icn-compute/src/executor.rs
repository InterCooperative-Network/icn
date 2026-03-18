//! Task execution engine.

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::error::ComputeError;
use crate::types::{ComputeResult, ComputeTask, ExecutionOutcome, ExecutorCapability, TaskCode};

/// Callback type for resolving contract references from the registry
/// Takes a contract hash and returns the contract if found
pub type ContractResolverCallback =
    Arc<dyn Fn([u8; 32]) -> Option<icn_ccl::Contract> + Send + Sync>;

/// Execution context provided to the executor
pub struct ExecutionContext {
    /// DID of the executor
    pub executor_did: String,
    /// Fuel remaining
    pub fuel_remaining: u64,
}

impl ExecutionContext {
    /// Consume fuel, returns error if exhausted
    pub fn consume_fuel(&mut self, amount: u64) -> Result<(), ComputeError> {
        if amount > self.fuel_remaining {
            return Err(ComputeError::OutOfFuel {
                used: self.fuel_remaining,
                limit: self.fuel_remaining,
            });
        }
        self.fuel_remaining -= amount;
        Ok(())
    }
}

/// Trait for task executors
pub trait Executor: Send + Sync {
    /// Execute a task and return the result
    fn execute(&self, task: &ComputeTask, ctx: &mut ExecutionContext) -> ExecutionOutcome;

    /// Capabilities this executor provides
    fn capabilities(&self) -> Vec<ExecutorCapability>;

    /// Check if executor can handle the task
    fn can_execute(&self, task: &ComputeTask) -> bool {
        let caps = self.capabilities();
        task.required_capabilities
            .iter()
            .all(|req| caps.contains(req))
    }
}

/// Local CCL executor (placeholder - will integrate with icn-ccl)
pub struct LocalExecutor {
    capabilities: Vec<ExecutorCapability>,
    /// Optional contract resolver for CclRef resolution
    contract_resolver: Option<ContractResolverCallback>,
    /// Optional WASM executor for WasmRef/WasmInline tasks
    #[cfg(feature = "wasm")]
    wasm_executor: Option<Arc<crate::wasm_executor::WasmExecutor>>,
}

impl LocalExecutor {
    /// Create a new local executor
    pub fn new() -> Self {
        Self {
            capabilities: vec![ExecutorCapability::Ccl],
            contract_resolver: None,
            #[cfg(feature = "wasm")]
            wasm_executor: None,
        }
    }

    /// Set the contract resolver callback for CclRef resolution
    pub fn set_contract_resolver(&mut self, resolver: ContractResolverCallback) {
        self.contract_resolver = Some(resolver);
    }

    /// Set the WASM executor for WasmRef/WasmInline tasks
    #[cfg(feature = "wasm")]
    pub fn set_wasm_executor(&mut self, executor: Arc<crate::wasm_executor::WasmExecutor>) {
        // Add Wasm capability when executor is set
        if !self.capabilities.contains(&ExecutorCapability::Wasm) {
            self.capabilities.push(ExecutorCapability::Wasm);
        }
        self.wasm_executor = Some(executor);
    }

    /// Execute a CCL contract with a specific rule
    ///
    /// This helper handles the common CCL execution logic for both inline contracts
    /// and contract references resolved from the registry.
    fn execute_ccl_inline(
        &self,
        contract: &icn_ccl::Contract,
        rule_name: &str,
        task: &ComputeTask,
        ctx: &mut ExecutionContext,
    ) -> ExecutionOutcome {
        // Validate contract structure
        if let Err(e) = contract.validate() {
            return ExecutionOutcome::Failed(format!("Contract validation failed: {e}"));
        }

        // Use the specified rule name
        let rule = rule_name.to_string();

        // Check that the rule exists
        if contract.get_rule(&rule).is_none() {
            return ExecutionOutcome::Failed(format!("Rule '{rule}' not found in contract"));
        }

        // Parse caller DID
        let caller_did: icn_identity::Did =
            match serde_json::from_value(serde_json::Value::String(ctx.executor_did.clone())) {
                Ok(d) => d,
                Err(e) => return ExecutionOutcome::Failed(format!("Invalid caller DID: {e}")),
            };

        // Create execution context for CCL
        let timestamp = icn_time::current_timestamp_secs();

        // Propagate determinism class to CCL context (E3 enforcement)
        let ccl_determinism = match task.determinism_class {
            crate::types::DeterminismClass::Canonical => {
                icn_kernel_api::compute::DeterminismClass::Canonical
            }
            crate::types::DeterminismClass::Advisory => {
                icn_kernel_api::compute::DeterminismClass::Advisory
            }
        };

        let ccl_context = icn_ccl::ExecutionContext {
            caller: caller_did.clone(),
            timestamp,
            fuel: ctx.fuel_remaining,
            fuel_limit: ctx.fuel_remaining,
            capabilities: vec![],
            participants: vec![caller_did],
            determinism_class: ccl_determinism,
        };

        // Create contract state
        let state = icn_ccl::ContractState::default();

        // Parse inputs as arguments
        let args: std::collections::HashMap<String, icn_ccl::Value> = if task.inputs.is_empty() {
            std::collections::HashMap::new()
        } else {
            serde_json::from_slice(&task.inputs).unwrap_or_default()
        };

        // Execute via interpreter
        let interpreter = icn_ccl::Interpreter::new(contract.clone(), state, ccl_context);
        match interpreter.execute_rule(&rule, args) {
            Ok(result) => {
                // Update fuel consumed
                ctx.fuel_remaining = ctx.fuel_remaining.saturating_sub(result.fuel_consumed);

                // Serialize result
                let output = serde_json::to_vec(&result.value).unwrap_or_else(|_| b"null".to_vec());
                ExecutionOutcome::Success(output)
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("fuel") || err_str.contains("Fuel") {
                    ExecutionOutcome::OutOfFuel
                } else {
                    ExecutionOutcome::Failed(err_str)
                }
            }
        }
    }

    /// Execute a task and produce a signed result
    pub fn execute_task(
        &self,
        task: &ComputeTask,
        executor_did: &str,
        signing_key: &[u8],
    ) -> Result<ComputeResult, ComputeError> {
        // Check deadline
        if let Some(deadline) = task.deadline {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(std::time::Duration::ZERO)
                .as_millis() as u64;
            if now > deadline {
                return Err(ComputeError::DeadlineExceeded);
            }
        }

        // Check capabilities
        if !self.can_execute(task) {
            return Err(ComputeError::MissingCapability(format!(
                "{:?}",
                task.required_capabilities
            )));
        }

        let start = Instant::now();
        let mut ctx = ExecutionContext {
            executor_did: executor_did.to_string(),
            fuel_remaining: task.fuel_limit.0,
        };

        let outcome = self.execute(task, &mut ctx);
        let duration_ms = start.elapsed().as_millis() as u64;
        let fuel_used = task.fuel_limit.0 - ctx.fuel_remaining;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_millis() as u64;

        // Parse signing key and sign the result
        let signing_key_bytes: [u8; 32] = signing_key
            .try_into()
            .map_err(|_| ComputeError::InvalidSignature("Invalid signing key length".into()))?;
        // SAFETY: SigningKey::from_bytes is infallible for any 32-byte array.
        // Length is already validated by try_into() above.
        let ed25519_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_bytes);

        let mut result = ComputeResult {
            task_hash: task.hash(),
            task_id: task.id.clone(),
            executor: executor_did.to_string(),
            outcome,
            fuel_used,
            duration_ms,
            completed_at: now,
            signature: vec![],
        };

        // Sign the result
        result.sign(&ed25519_key);

        Ok(result)
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor for LocalExecutor {
    fn execute(&self, task: &ComputeTask, ctx: &mut ExecutionContext) -> ExecutionOutcome {
        match &task.code {
            TaskCode::Ccl(source) => {
                // Parse CCL contract from JSON
                let contract: icn_ccl::Contract = match serde_json::from_str(source) {
                    Ok(c) => c,
                    Err(e) => return ExecutionOutcome::Failed(format!("Invalid CCL JSON: {e}")),
                };

                // Validate contract structure
                if let Err(e) = contract.validate() {
                    return ExecutionOutcome::Failed(format!("Contract validation failed: {e}"));
                }

                // Find the default rule to execute (first rule or "main")
                let rule_name = contract
                    .rules
                    .first()
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| "main".to_string());

                // Parse caller DID
                let caller_did: icn_identity::Did = match serde_json::from_value(
                    serde_json::Value::String(ctx.executor_did.clone()),
                ) {
                    Ok(d) => d,
                    Err(e) => return ExecutionOutcome::Failed(format!("Invalid caller DID: {e}")),
                };

                // Create execution context for CCL
                let timestamp = icn_time::current_timestamp_secs();

                // Propagate determinism class to CCL context (E3 enforcement)
                let ccl_determinism = match task.determinism_class {
                    crate::types::DeterminismClass::Canonical => {
                        icn_kernel_api::compute::DeterminismClass::Canonical
                    }
                    crate::types::DeterminismClass::Advisory => {
                        icn_kernel_api::compute::DeterminismClass::Advisory
                    }
                };

                let ccl_context = icn_ccl::ExecutionContext {
                    caller: caller_did.clone(),
                    timestamp,
                    fuel: ctx.fuel_remaining,
                    fuel_limit: ctx.fuel_remaining,
                    capabilities: vec![],
                    participants: vec![caller_did],
                    determinism_class: ccl_determinism,
                };

                // Create contract state
                let state = icn_ccl::ContractState::default();

                // Parse inputs as arguments
                let args: std::collections::HashMap<String, icn_ccl::Value> =
                    if task.inputs.is_empty() {
                        std::collections::HashMap::new()
                    } else {
                        serde_json::from_slice(&task.inputs).unwrap_or_default()
                    };

                // Execute via interpreter
                let interpreter = icn_ccl::Interpreter::new(contract, state, ccl_context);
                match interpreter.execute_rule(&rule_name, args) {
                    Ok(result) => {
                        // Update fuel consumed
                        ctx.fuel_remaining =
                            ctx.fuel_remaining.saturating_sub(result.fuel_consumed);

                        // Serialize result
                        let output =
                            serde_json::to_vec(&result.value).unwrap_or_else(|_| b"null".to_vec());
                        ExecutionOutcome::Success(output)
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("fuel") || err_str.contains("Fuel") {
                            ExecutionOutcome::OutOfFuel
                        } else {
                            ExecutionOutcome::Failed(err_str)
                        }
                    }
                }
            }
            TaskCode::CclRef { hash, rule } => {
                // CclRef requires a contract registry to resolve the hash
                let hash_hex = hex::encode(hash);

                match &self.contract_resolver {
                    Some(resolver) => {
                        // Resolve the contract from the registry
                        match resolver(*hash) {
                            Some(contract) => {
                                // Execute the resolved contract
                                tracing::debug!(
                                    hash = %hash_hex,
                                    rule = %rule,
                                    "Resolved CclRef, executing contract"
                                );
                                self.execute_ccl_inline(&contract, rule, task, ctx)
                            }
                            None => {
                                tracing::warn!(
                                    hash = %hash_hex,
                                    rule = %rule,
                                    "Contract not found in registry"
                                );
                                ExecutionOutcome::Failed(format!(
                                    "Contract {hash_hex} not found in registry"
                                ))
                            }
                        }
                    }
                    None => {
                        tracing::warn!(
                            hash = %hash_hex,
                            rule = %rule,
                            "CclRef requires contract registry (not configured)"
                        );
                        ExecutionOutcome::Failed(format!(
                            "Contract registry not configured. Cannot resolve {hash_hex}."
                        ))
                    }
                }
            }
            TaskCode::WasmRef(_) | TaskCode::WasmInline(_) => {
                #[cfg(feature = "wasm")]
                {
                    if let Some(ref wasm_exec) = self.wasm_executor {
                        return wasm_exec.execute(task, ctx);
                    }
                    ExecutionOutcome::Failed(
                        "WASM executor not configured. Registry may not be initialized.".into(),
                    )
                }
                #[cfg(not(feature = "wasm"))]
                {
                    ExecutionOutcome::Failed(
                        "WASM support not enabled. Rebuild with --features wasm".into(),
                    )
                }
            }
        }
    }

    fn capabilities(&self) -> Vec<ExecutorCapability> {
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeterminismClass, FuelLimit, PrivacyClass};

    /// Valid test DID (proper multibase-encoded Ed25519 public key)
    /// Generated from deterministic seed [1u8; 32]
    const TEST_ALICE_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";

    /// Simple CCL contract that returns a constant
    fn simple_contract() -> String {
        format!(
            r#"{{
            "name": "SimpleReturn",
            "participants": ["{TEST_ALICE_DID}"],
            "currency": null,
            "state_vars": [],
            "rules": [{{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{{ "Return": {{ "value": {{ "Literal": {{ "Int": 42 }} }} }} }}]
            }}],
            "triggers": []
        }}"#
        )
    }

    /// CCL contract with addition
    fn calculator_contract() -> String {
        format!(
            r#"{{
            "name": "Calculator",
            "participants": ["{TEST_ALICE_DID}"],
            "currency": null,
            "state_vars": [],
            "rules": [{{
                "name": "add",
                "params": [{{"name": "a"}}, {{"name": "b"}}],
                "requires": [],
                "body": [{{
                    "Return": {{
                        "value": {{
                            "BinOp": {{
                                "op": "Add",
                                "left": {{ "Var": "a" }},
                                "right": {{ "Var": "b" }}
                            }}
                        }}
                    }}
                }}]
            }}],
            "triggers": []
        }}"#
        )
    }

    fn make_task(code: &str, fuel: u64) -> ComputeTask {
        ComputeTask {
            id: "test".into(),
            submitter: TEST_ALICE_DID.into(),
            coop_id: None,
            code: TaskCode::Ccl(code.into()),
            inputs: vec![],
            fuel_limit: FuelLimit(fuel),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            // E1: Workload manifest fields
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
            scope: Default::default(),
        }
    }

    fn test_signing_key() -> [u8; 32] {
        // Deterministic test key
        [1u8; 32]
    }

    #[test]
    fn test_local_executor_success() {
        let executor = LocalExecutor::new();
        let task = make_task(&simple_contract(), 10_000);

        let result = executor
            .execute_task(&task, TEST_ALICE_DID, &test_signing_key())
            .unwrap();

        // Verify execution succeeded
        assert!(
            matches!(&result.outcome, ExecutionOutcome::Success(_)),
            "Expected successful execution, got: {:?}",
            result.outcome
        );

        // Verify output is non-empty
        if let ExecutionOutcome::Success(output) = &result.outcome {
            assert!(!output.is_empty(), "Expected non-empty output");
        }
    }

    #[test]
    fn test_local_executor_with_args() {
        let executor = LocalExecutor::new();
        let mut task = make_task(&calculator_contract(), 10_000);
        // Pass arguments using CCL Value format
        task.inputs = r#"{"a": {"Int": 5}, "b": {"Int": 3}}"#.as_bytes().to_vec();

        let result = executor
            .execute_task(&task, TEST_ALICE_DID, &test_signing_key())
            .unwrap();

        // Verify execution succeeded with arguments
        assert!(
            matches!(&result.outcome, ExecutionOutcome::Success(_)),
            "Expected successful execution with args, got: {:?}",
            result.outcome
        );
    }

    #[test]
    fn test_local_executor_invalid_ccl() {
        let executor = LocalExecutor::new();
        let task = make_task("not valid json", 10_000);

        let result = executor
            .execute_task(&task, "did:icn:bob", &test_signing_key())
            .unwrap();

        assert!(matches!(result.outcome, ExecutionOutcome::Failed(_)));
    }

    #[test]
    fn test_capabilities_check() {
        let executor = LocalExecutor::new();
        let task = make_task(&simple_contract(), 1000);

        assert!(executor.can_execute(&task));

        let wasm_task = ComputeTask {
            id: "wasm-test".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::WasmRef([0u8; 32]),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Wasm],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
            scope: Default::default(),
        };

        assert!(!executor.can_execute(&wasm_task));
    }

    #[test]
    fn test_deadline_exceeded() {
        let executor = LocalExecutor::new();
        let task = ComputeTask {
            id: "expired".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::Ccl(simple_contract()),
            inputs: vec![],
            fuel_limit: FuelLimit::default(),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: Some(1), // Already expired
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
            scope: Default::default(),
        };

        let result = executor.execute_task(&task, "did:icn:bob", &test_signing_key());
        assert!(matches!(result, Err(ComputeError::DeadlineExceeded)));
    }
}
