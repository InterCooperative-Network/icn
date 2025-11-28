//! WASM executor for compute tasks.
//!
//! This module provides WebAssembly execution capabilities using the Wasmtime runtime.
//! Enable with the `wasm` feature flag.

#[cfg(feature = "wasm")]
use wasmtime::{Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use crate::error::ComputeError;
use crate::executor::{ExecutionContext, Executor};
use crate::types::{ComputeTask, ExecutionOutcome, ExecutorCapability, TaskCode};

/// WASM execution state for memory limits
#[cfg(feature = "wasm")]
struct WasmState {
    limits: StoreLimits,
}

#[cfg(feature = "wasm")]
impl WasmState {
    fn new(max_memory_bytes: usize) -> Self {
        Self {
            limits: StoreLimitsBuilder::new()
                .memory_size(max_memory_bytes)
                .table_elements(10_000)
                .instances(10)
                .tables(10)
                .memories(10)
                .build(),
        }
    }
}

/// WASM executor using Wasmtime
pub struct WasmExecutor {
    capabilities: Vec<ExecutorCapability>,
    #[cfg(feature = "wasm")]
    engine: Engine,
    /// Maximum memory per WASM instance (bytes)
    max_memory: usize,
}

impl WasmExecutor {
    /// Create a new WASM executor
    #[cfg(feature = "wasm")]
    pub fn new() -> Result<Self, ComputeError> {
        // Use default config - fuel metering can be enabled later
        let engine = Engine::default();

        Ok(Self {
            capabilities: vec![ExecutorCapability::Wasm, ExecutorCapability::Ccl],
            engine,
            max_memory: 64 * 1024 * 1024, // 64MB default
        })
    }

    /// Create a new WASM executor (stub when feature disabled)
    #[cfg(not(feature = "wasm"))]
    pub fn new() -> Result<Self, ComputeError> {
        Ok(Self {
            capabilities: vec![ExecutorCapability::Ccl],
            max_memory: 64 * 1024 * 1024,
        })
    }

    /// Set maximum memory per WASM instance
    pub fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory = bytes;
        self
    }

    /// Execute a WASM module
    #[cfg(feature = "wasm")]
    fn execute_wasm(
        &self,
        wasm_bytes: &[u8],
        inputs: &[u8],
        ctx: &mut ExecutionContext,
    ) -> ExecutionOutcome {
        // Create store with memory limits
        let state = WasmState::new(self.max_memory);
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);

        // Compile module
        let module = match Module::new(&self.engine, wasm_bytes) {
            Ok(m) => m,
            Err(e) => return ExecutionOutcome::Failed(format!("WASM compilation failed: {e}")),
        };

        // Create linker and add WASI-like imports
        let mut linker: Linker<WasmState> = Linker::new(&self.engine);

        // Add ICN host functions
        if let Err(e) = self.add_host_functions(&mut linker) {
            return ExecutionOutcome::Failed(format!("Failed to add host functions: {e}"));
        }

        // Instantiate module
        let instance = match linker.instantiate(&mut store, &module) {
            Ok(i) => i,
            Err(e) => return ExecutionOutcome::Failed(format!("WASM instantiation failed: {e}")),
        };

        // Get the main/run function
        let run_fn = match instance.get_typed_func::<(i32, i32), i32>(&mut store, "run") {
            Ok(f) => f,
            Err(_) => {
                // Try alternative signatures
                match instance.get_typed_func::<(), i32>(&mut store, "run") {
                    Ok(f) => {
                        // No-arg version
                        match f.call(&mut store, ()) {
                            Ok(result) => {
                                // Simple fuel estimation: 100 fuel per execution
                                ctx.fuel_remaining = ctx.fuel_remaining.saturating_sub(100);
                                return ExecutionOutcome::Success(result.to_le_bytes().to_vec());
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                if err_str.contains("fuel") || err_str.contains("out of fuel") {
                                    return ExecutionOutcome::OutOfFuel;
                                }
                                return ExecutionOutcome::Failed(format!("Execution failed: {err_str}"));
                            }
                        }
                    }
                    Err(_) => {
                        return ExecutionOutcome::Failed(
                            "No 'run' function found with supported signature".into(),
                        )
                    }
                }
            }
        };

        // Write inputs to WASM memory if provided
        let (input_ptr, input_len) = if !inputs.is_empty() {
            // Get memory export
            let memory = match instance.get_memory(&mut store, "memory") {
                Some(m) => m,
                None => return ExecutionOutcome::Failed("No memory export found".into()),
            };

            // Allocate space for inputs (simplified - assumes module has an allocator)
            // For now, write to a fixed offset
            let offset = 1024u32; // Start at 1KB offset
            let data = memory.data_mut(&mut store);
            if offset as usize + inputs.len() > data.len() {
                return ExecutionOutcome::Failed("Input too large for WASM memory".into());
            }
            data[offset as usize..offset as usize + inputs.len()].copy_from_slice(inputs);
            (offset as i32, inputs.len() as i32)
        } else {
            (0, 0)
        };

        // Call the function
        match run_fn.call(&mut store, (input_ptr, input_len)) {
            Ok(result) => {
                // Simple fuel estimation: 100 fuel per execution
                ctx.fuel_remaining = ctx.fuel_remaining.saturating_sub(100);
                // Return result as bytes
                ExecutionOutcome::Success(result.to_le_bytes().to_vec())
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("fuel") || err_str.contains("out of fuel") {
                    ExecutionOutcome::OutOfFuel
                } else {
                    ExecutionOutcome::Failed(format!("Execution failed: {err_str}"))
                }
            }
        }
    }

    /// Add ICN host functions to the linker
    #[cfg(feature = "wasm")]
    fn add_host_functions(&self, linker: &mut Linker<WasmState>) -> Result<(), ComputeError> {
        // Add logging function
        linker
            .func_wrap("icn", "log", |_caller: wasmtime::Caller<'_, WasmState>, ptr: i32, len: i32| {
                tracing::debug!(ptr, len, "WASM log call");
            })
            .map_err(|e| ComputeError::ExecutionFailed(format!("Failed to add log function: {e}")))?;

        // Add timestamp function
        linker
            .func_wrap("icn", "timestamp", |_caller: wasmtime::Caller<'_, WasmState>| -> i64 {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64
            })
            .map_err(|e| {
                ComputeError::ExecutionFailed(format!("Failed to add timestamp function: {e}"))
            })?;

        Ok(())
    }

    /// Execute WASM (stub when feature disabled)
    #[cfg(not(feature = "wasm"))]
    fn execute_wasm(
        &self,
        _wasm_bytes: &[u8],
        _inputs: &[u8],
        _ctx: &mut ExecutionContext,
    ) -> ExecutionOutcome {
        ExecutionOutcome::Failed(
            "WASM support not enabled. Rebuild with --features wasm".into(),
        )
    }
}

#[cfg(feature = "wasm")]
impl Default for WasmExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default WasmExecutor")
    }
}

#[cfg(not(feature = "wasm"))]
impl Default for WasmExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default WasmExecutor")
    }
}

impl Executor for WasmExecutor {
    fn execute(&self, task: &ComputeTask, ctx: &mut ExecutionContext) -> ExecutionOutcome {
        match &task.code {
            TaskCode::Ccl(source) => {
                // Delegate CCL execution to the CCL interpreter (same as LocalExecutor)
                let contract: icn_ccl::Contract = match serde_json::from_str(source) {
                    Ok(c) => c,
                    Err(e) => return ExecutionOutcome::Failed(format!("Invalid CCL JSON: {e}")),
                };

                if let Err(e) = contract.validate() {
                    return ExecutionOutcome::Failed(format!("Contract validation failed: {e}"));
                }

                let rule_name = contract
                    .rules
                    .first()
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| "main".to_string());

                let caller_did: icn_identity::Did = match serde_json::from_value(
                    serde_json::Value::String(ctx.executor_did.clone()),
                ) {
                    Ok(d) => d,
                    Err(e) => return ExecutionOutcome::Failed(format!("Invalid caller DID: {e}")),
                };

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let ccl_context = icn_ccl::ExecutionContext {
                    caller: caller_did.clone(),
                    timestamp,
                    fuel: ctx.fuel_remaining,
                    capabilities: vec![],
                    participants: vec![caller_did],
                };

                let state = icn_ccl::ContractState::default();
                let args: std::collections::HashMap<String, icn_ccl::Value> =
                    if task.inputs.is_empty() {
                        std::collections::HashMap::new()
                    } else {
                        serde_json::from_slice(&task.inputs).unwrap_or_default()
                    };

                let interpreter = icn_ccl::Interpreter::new(contract, state, ccl_context);
                match interpreter.execute_rule(&rule_name, args) {
                    Ok(result) => {
                        ctx.fuel_remaining =
                            ctx.fuel_remaining.saturating_sub(result.fuel_consumed);
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
            TaskCode::WasmInline(bytes) => self.execute_wasm(bytes, &task.inputs, ctx),
            TaskCode::WasmRef(_hash) => {
                // TODO: Fetch WASM from blob storage using hash
                ExecutionOutcome::Failed("WASM reference execution not yet implemented".into())
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
    use crate::types::FuelLimit;

    #[test]
    fn test_wasm_executor_creation() {
        let executor = WasmExecutor::new().unwrap();
        let caps = executor.capabilities();

        #[cfg(feature = "wasm")]
        assert!(caps.contains(&ExecutorCapability::Wasm));

        assert!(caps.contains(&ExecutorCapability::Ccl));
    }

    #[test]
    fn test_wasm_executor_ccl_fallback() {
        let executor = WasmExecutor::new().unwrap();

        let contract = r#"{
            "name": "SimpleReturn",
            "participants": ["did:icn:alice"],
            "currency": null,
            "state_vars": [],
            "rules": [{
                "name": "run",
                "params": [],
                "requires": [],
                "body": [{ "Return": { "value": { "Literal": { "Int": 42 } } } }]
            }],
            "triggers": []
        }"#;

        let task = ComputeTask {
            id: "test".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::Ccl(contract.into()),
            inputs: vec![],
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
        };

        let mut ctx = ExecutionContext {
            executor_did: "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".into(),
            fuel_remaining: 10_000,
        };

        let result = executor.execute(&task, &mut ctx);
        assert!(
            matches!(result, ExecutionOutcome::Success(_)),
            "Expected success, got: {result:?}"
        );
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_wasm_inline_execution() {
        // Simple WASM module that returns 42
        let wat_source = r#"
            (module
                (func $run (export "run") (result i32)
                    i32.const 42
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_source).expect("Failed to parse WAT");

        let executor = WasmExecutor::new().unwrap();
        let task = ComputeTask {
            id: "wasm-test".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::WasmInline(wasm_bytes),
            inputs: vec![],
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Wasm],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
        };

        let mut ctx = ExecutionContext {
            executor_did: "did:icn:executor".into(),
            fuel_remaining: 10_000,
        };

        let result = executor.execute(&task, &mut ctx);
        match result {
            ExecutionOutcome::Success(output) => {
                // Should return 42 as little-endian i32
                assert_eq!(output, vec![42, 0, 0, 0]);
            }
            other => panic!("Expected success, got: {:?}", other),
        }
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_wasm_with_host_functions() {
        // WASM module that calls host functions (log, timestamp)
        let wat_source = r#"
            (module
                ;; Import ICN host functions
                (import "icn" "log" (func $log (param i32 i32)))
                (import "icn" "timestamp" (func $timestamp (result i64)))

                ;; Memory for log message
                (memory (export "memory") 1)

                ;; Store "Hi" at offset 0
                (data (i32.const 0) "Hi")

                (func $run (export "run") (result i32)
                    ;; Call log("Hi", 2)
                    (call $log (i32.const 0) (i32.const 2))
                    ;; Call timestamp (result ignored)
                    (drop (call $timestamp))
                    ;; Return 99
                    (i32.const 99)
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_source).expect("Failed to parse WAT");

        let executor = WasmExecutor::new().unwrap();
        let task = ComputeTask {
            id: "host-fn-test".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::WasmInline(wasm_bytes),
            inputs: vec![],
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Wasm],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
        };

        let mut ctx = ExecutionContext {
            executor_did: "did:icn:executor".into(),
            fuel_remaining: 10_000,
        };

        let result = executor.execute(&task, &mut ctx);
        match result {
            ExecutionOutcome::Success(output) => {
                assert_eq!(output, vec![99, 0, 0, 0]);
            }
            other => panic!("Expected success, got: {:?}", other),
        }
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn test_wasm_with_inputs() {
        // WASM module that reads inputs and sums bytes
        let wat_source = r#"
            (module
                (memory (export "memory") 1)

                ;; run(ptr, len) -> i32: sum bytes from ptr..ptr+len
                (func $run (export "run") (param $ptr i32) (param $len i32) (result i32)
                    (local $sum i32)
                    (local $end i32)
                    (local.set $end (i32.add (local.get $ptr) (local.get $len)))
                    (block $done
                        (loop $loop
                            (br_if $done (i32.ge_u (local.get $ptr) (local.get $end)))
                            (local.set $sum
                                (i32.add (local.get $sum)
                                    (i32.load8_u (local.get $ptr))))
                            (local.set $ptr (i32.add (local.get $ptr) (i32.const 1)))
                            (br $loop)
                        )
                    )
                    (local.get $sum)
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_source).expect("Failed to parse WAT");

        let executor = WasmExecutor::new().unwrap();
        let task = ComputeTask {
            id: "input-test".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::WasmInline(wasm_bytes),
            inputs: vec![1, 2, 3, 4, 5], // Sum = 15
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Wasm],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
        };

        let mut ctx = ExecutionContext {
            executor_did: "did:icn:executor".into(),
            fuel_remaining: 10_000,
        };

        let result = executor.execute(&task, &mut ctx);
        match result {
            ExecutionOutcome::Success(output) => {
                // Sum of [1,2,3,4,5] = 15
                assert_eq!(output, vec![15, 0, 0, 0]);
            }
            other => panic!("Expected success, got: {:?}", other),
        }
    }

    #[cfg(not(feature = "wasm"))]
    #[test]
    fn test_wasm_disabled_message() {
        let executor = WasmExecutor::new().unwrap();
        let task = ComputeTask {
            id: "wasm-test".into(),
            submitter: "did:icn:alice".into(),
            coop_id: None,
            code: TaskCode::WasmInline(vec![0x00, 0x61, 0x73, 0x6d]),
            inputs: vec![],
            fuel_limit: FuelLimit(10_000),
            required_capabilities: vec![ExecutorCapability::Wasm],
            priority: crate::types::TaskPriority::Normal,
            created_at: 1000,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
        };

        let mut ctx = ExecutionContext {
            executor_did: "did:icn:executor".into(),
            fuel_remaining: 10_000,
        };

        let result = executor.execute(&task, &mut ctx);
        match result {
            ExecutionOutcome::Failed(msg) => {
                assert!(msg.contains("WASM support not enabled"));
            }
            other => panic!("Expected failure message, got: {other:?}"),
        }
    }
}
