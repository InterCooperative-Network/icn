use anyhow::{anyhow, Result};
use async_trait::async_trait;
use blake3::Hasher;
use chrono::Utc;
use cid::Cid;
use icn_common::utils::cid_utils::{bytes_to_cid, encode_cid};
use mesh_types::{
    Did, ExecutionReceipt, TaskExecutionResult, TaskIntent, TaskMetrics, TaskRunner, TaskRunnerConfig,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::fs;
use tracing::{debug, error, info, warn};
use wasmtime::{
    Config, Engine, Func, Instance, Module, Store, ExternRef,
    AsContextMut, AsContextMut as _, Caller, Linker, Memory, WasmBacktraceDetails,
};

/// Environment for the WASM task execution
pub struct TaskEnvironment {
    /// The worker's DID (used for signing receipts)
    worker_did: Did,
    
    /// Start time of the current execution
    start_time: Option<Instant>,
    
    /// Metrics being collected during execution
    metrics: TaskMetrics,
    
    /// Path to store output files
    output_dir: PathBuf,
    
    /// Content ID of the output (filled when execution completes)
    output_cid: Option<Cid>,
    
    /// Logs from the WASM execution
    logs: Vec<String>,
    
    /// Output data for hashing
    output_data: Option<Vec<u8>>,
}

impl TaskEnvironment {
    /// Create a new task environment
    fn new(worker_did: &str, output_dir: PathBuf) -> Self {
        Self {
            worker_did: worker_did.to_string(),
            start_time: None,
            metrics: TaskMetrics {
                execution_time_ms: 0,
                peak_memory_bytes: 0,
                fuel_consumed: 0,
                io_operations: 0,
                custom_metrics: HashMap::new(),
            },
            output_dir,
            output_cid: None,
            logs: Vec::new(),
            output_data: None,
        }
    }
    
    /// Start execution timing
    fn start_execution(&mut self) {
        self.start_time = Some(Instant::now());
    }
    
    /// End execution timing and update metrics
    fn end_execution(&mut self) {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed();
            self.metrics.execution_time_ms = elapsed.as_millis() as u64;
            self.start_time = None;
        }
    }
    
    /// Record a log message
    fn log(&mut self, level: u32, message: &str) {
        let level_str = match level {
            0 => "DEBUG",
            1 => "INFO",
            2 => "WARN",
            3 => "ERROR",
            _ => "UNKNOWN",
        };
        
        let log_entry = format!("[{}] {}", level_str, message);
        self.logs.push(log_entry.clone());
        
        // Also log to the host system
        match level {
            0 => debug!("{}", message),
            1 => info!("{}", message),
            2 => warn!("{}", message),
            3 => error!("{}", message),
            _ => info!("{}", message),
        }
    }
    
    /// Store output data and get its CID
    async fn store_output(&mut self, data: &[u8], task_id: &str) -> Result<Cid> {
        // Create output directory if it doesn't exist
        tokio::fs::create_dir_all(&self.output_dir).await?;
        
        // Generate a filename based on task ID
        let filename = format!("output_{}.bin", task_id);
        let output_path = self.output_dir.join(filename);
        
        // Write the data to a file
        tokio::fs::write(&output_path, data).await?;
        
        // Generate CID for the data
        let cid = bytes_to_cid(data)?;
        self.output_cid = Some(cid.clone());
        self.metrics.io_operations += 1;
        
        // Save the output data for hashing later
        self.output_data = Some(data.to_vec());
        
        Ok(cid)
    }
}

/// Implementation of the Wasmtime Task Runner
pub struct WasmtimeTaskRunner {
    /// Directory to store WASM modules
    wasm_dir: PathBuf,
    
    /// Directory to store input data
    input_dir: PathBuf,
    
    /// Directory to store output data
    output_dir: PathBuf,
    
    /// Worker DID used for execution receipts
    worker_did: Did,
    
    /// Wasmtime engine
    engine: Engine,
}

impl WasmtimeTaskRunner {
    /// Create a new wasmtime task runner
    pub fn new(
        wasm_dir: PathBuf,
        input_dir: PathBuf,
        output_dir: PathBuf,
        worker_did: &str,
    ) -> Result<Self> {
        // Create a wasmtime config
        let mut config = Config::new();
        config.wasm_bulk_memory(true);
        config.wasm_reference_types(true);
        config.async_support(true);
        config.consume_fuel(true);
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
        
        let engine = Engine::new(&config)?;
        
        Ok(Self {
            wasm_dir,
            input_dir,
            output_dir,
            worker_did: worker_did.to_string(),
            engine,
        })
    }
    
    /// Load a WASM module
    async fn load_wasm_module(&self, cid: &Cid) -> Result<Vec<u8>> {
        // Generate filename from CID
        let filename = format!("{}.wasm", cid.to_string());
        let wasm_path = self.wasm_dir.join(filename);
        
        // Check if the file exists
        if !tokio::fs::try_exists(&wasm_path).await? {
            return Err(anyhow!("WASM module not found: {}", cid));
        }
        
        // Load the WASM bytes
        let wasm_bytes = tokio::fs::read(&wasm_path).await?;
        
        Ok(wasm_bytes)
    }
    
    /// Load input data
    async fn load_input_data(&self, cid: &Cid) -> Result<Vec<u8>> {
        // Generate filename from CID
        let filename = format!("{}.bin", cid.to_string());
        let input_path = self.input_dir.join(filename);
        
        // Check if the file exists
        if !tokio::fs::try_exists(&input_path).await? {
            return Err(anyhow!("Input data not found: {}", cid));
        }
        
        // Load the input bytes
        let input_bytes = tokio::fs::read(&input_path).await?;
        
        Ok(input_bytes)
    }
    
    /// Create a linker with host functions for the WASM module
    fn create_host_imports<T: AsContextMut>(
        &self,
        mut store: impl AsContextMut<Data = TaskEnvironment>,
        input_data: Vec<u8>,
    ) -> Result<Linker<TaskEnvironment>> {
        let mut linker = Linker::new(&self.engine);
        
        // Define the WASI imports
        // wasmtime_wasi::add_to_linker(&mut linker, |ctx| ctx)?;
        
        // Add input_load - Gets the input data for the task
        linker.func_wrap("env", "input_load", move |mut caller: Caller<'_, TaskEnvironment>,
                          ptr: i32, max_len: i32| -> Result<i32, wasmtime::Trap> {
            let memory = match caller.get_export("memory") {
                Some(export) => match export.into_memory() {
                    Some(memory) => memory,
                    None => return Err(wasmtime::Trap::new("memory export is not a memory")),
                },
                None => return Err(wasmtime::Trap::new("no memory export")),
            };
            
            // Check if the input will fit
            if input_data.len() > max_len as usize {
                return Err(wasmtime::Trap::new(format!(
                    "input too large: {} > {}", input_data.len(), max_len
                )));
            }
            
            // Write the input data to memory
            let mem_slice = memory.data_mut(&mut caller);
            let start = ptr as usize;
            let end = start + input_data.len();
            
            if end > mem_slice.len() {
                return Err(wasmtime::Trap::new("out of bounds memory access"));
            }
            
            mem_slice[start..end].copy_from_slice(&input_data);
            
            // Increment I/O operations count
            caller.data_mut().metrics.io_operations += 1;
            
            // Return the length of the input data
            Ok(input_data.len() as i32)
        })?;
        
        // Add output_store - Store output data
        linker.func_wrap("env", "output_store", move |mut caller: Caller<'_, TaskEnvironment>,
                          ptr: i32, len: i32| -> Result<i32, wasmtime::Trap> {
            // Get the memory export
            let memory = match caller.get_export("memory") {
                Some(export) => match export.into_memory() {
                    Some(memory) => memory,
                    None => return Err(wasmtime::Trap::new("memory export is not a memory")),
                },
                None => return Err(wasmtime::Trap::new("no memory export")),
            };
            
            // Read the output data from memory
            let mem_slice = memory.data(&caller);
            let start = ptr as usize;
            let end = start + len as usize;
            
            if end > mem_slice.len() {
                return Err(wasmtime::Trap::new("out of bounds memory access"));
            }
            
            let output_data = mem_slice[start..end].to_vec();
            
            // The 'output_store' host function is meant to be used in WASM to store output.
            // Since we're running in a synchronous context, we'll just store a reference to the output
            // data in our environment and process it after the WASM execution completes.
            
            // Store output data length for now, we'll process it after execution
            caller.data_mut().metrics.custom_metrics.insert("output_size".to_string(), output_data.len() as u64);
            
            // Save the output data for hashing later
            caller.data_mut().output_data = Some(output_data.clone());
            
            // Increment I/O operations count
            caller.data_mut().metrics.io_operations += 1;
            
            // Clone the output data for use in the future (if needed)
            // In a real implementation, we might use a more efficient approach
            caller.data_mut().logs.push(format!("Output data size: {} bytes", output_data.len()));
            
            // Generate a temporary CID for now, for accurate return value
            match bytes_to_cid(&output_data) {
                Ok(cid) => {
                    caller.data_mut().output_cid = Some(cid);
                    Ok(1) // Success
                },
                Err(e) => {
                    caller.data_mut().logs.push(format!("Error generating output CID: {}", e));
                    Err(wasmtime::Trap::new(format!("Failed to generate CID: {}", e)))
                }
            }
        })?;
        
        // Add log function - Log messages from the WASM module
        linker.func_wrap("env", "log", |mut caller: Caller<'_, TaskEnvironment>,
                         level: i32, ptr: i32, len: i32| -> Result<(), wasmtime::Trap> {
            // Get the memory export
            let memory = match caller.get_export("memory") {
                Some(export) => match export.into_memory() {
                    Some(memory) => memory,
                    None => return Err(wasmtime::Trap::new("memory export is not a memory")),
                },
                None => return Err(wasmtime::Trap::new("no memory export")),
            };
            
            // Read the message from memory
            let mem_slice = memory.data(&caller);
            let start = ptr as usize;
            let end = start + len as usize;
            
            if end > mem_slice.len() {
                return Err(wasmtime::Trap::new("out of bounds memory access"));
            }
            
            let message_bytes = &mem_slice[start..end];
            let message = match std::str::from_utf8(message_bytes) {
                Ok(s) => s,
                Err(e) => return Err(wasmtime::Trap::new(format!("Invalid UTF-8: {}", e))),
            };
            
            // Log the message
            caller.data_mut().log(level as u32, message);
            
            Ok(())
        })?;
        
        Ok(linker)
    }
}

#[async_trait]
impl TaskRunner for WasmtimeTaskRunner {
    async fn execute_task(&self, task: &TaskIntent) -> Result<TaskExecutionResult> {
        // Default configuration
        let config = TaskRunnerConfig::default();
        
        // Execute with default configuration
        self.execute_task_with_config(task, config).await
    }
    
    async fn execute_task_with_config(
        &self,
        task: &TaskIntent,
        config: TaskRunnerConfig,
    ) -> Result<TaskExecutionResult> {
        // Load the WASM module
        let wasm_bytes = self.load_wasm_module(&task.wasm_cid).await?;
        
        // Load the input data
        let input_data = self.load_input_data(&task.input_cid).await?;
        
        // Compile the module
        let module = Module::new(&self.engine, &wasm_bytes)?;
        
        // Create the task environment
        let mut task_env = TaskEnvironment::new(&self.worker_did, self.output_dir.clone());
        
        // Create a store with the task environment
        let mut store = Store::new(&self.engine, task_env);
        
        // Add fuel to the store
        store.add_fuel(config.fuel_limit)?;
        
        // Create the imports
        let linker = self.create_host_imports(store.as_context_mut(), input_data)?;
        
        // Instantiate the module
        let instance = linker.instantiate(&mut store, &module)?;
        
        // Get the start function
        let start = instance.get_func(&mut store, "_start")
            .ok_or_else(|| anyhow!("WASM module missing _start function"))?;
        
        // Start execution timing
        store.data_mut().start_execution();
        
        // Execute the start function
        let result = start.call(&mut store, &[], &mut []);
        
        // End execution timing
        store.data_mut().end_execution();
        
        // Get consumed fuel
        let consumed_fuel = config.fuel_limit - store.fuel_consumed().unwrap_or(0);
        store.data_mut().metrics.fuel_consumed = consumed_fuel;
        
        // Extract the exit code and handle errors
        let exit_code = match result {
            Ok(_) => 0, // Success
            Err(e) => {
                store.data_mut().log(3, &format!("Execution error: {}", e));
                -1 // Error
            }
        };
        
        // Process output data after execution
        let (output_cid, output_data) = if let Some(cid) = store.data().output_cid.clone() {
            // Use the output data saved during execution
            (cid, store.data().output_data.clone())
        } else if let Some(output_size) = store.data().metrics.custom_metrics.get("output_size") {
            // No output CID set yet, generate an empty one as fallback
            (bytes_to_cid(&[])?, Some(vec![]))
        } else {
            // No output data stored, return an empty CID
            (bytes_to_cid(&[])?, None)
        };
        
        // Create the result
        let execution_result = TaskExecutionResult {
            exit_code,
            metrics: store.data().metrics.clone(),
            output_cid,
            output_data,
            deterministic: true, // Assume deterministic for now
            execution_trace_hash: None, // No trace for now
        };
        
        Ok(execution_result)
    }
    
    fn generate_receipt(
        &self,
        task: &TaskIntent,
        result: &TaskExecutionResult,
        worker_did: &str,
    ) -> Result<ExecutionReceipt> {
        // Get the output data from the execution result
        let output_data = result.output_data.as_ref().unwrap_or(&vec![]);
        
        // Calculate the hash of the output data using blake3
        let mut hasher = Hasher::new();
        hasher.update(output_data);
        let output_hash = hasher.finalize();
        
        // Create the receipt
        let receipt = ExecutionReceipt {
            worker_did: worker_did.to_string(),
            task_cid: task.wasm_cid.clone(), // Usually we'd hash the whole task
            output_cid: result.output_cid.clone(),
            output_hash: output_hash.as_bytes().to_vec(),
            fuel_consumed: result.metrics.fuel_consumed,
            timestamp: Utc::now(),
            signature: vec![], // Would be signed in a real implementation
            metadata: None,
        };
        
        Ok(receipt)
    }
    
    async fn verify_receipt(&self, receipt: &ExecutionReceipt) -> Result<bool> {
        // In a real implementation, we would:
        // 1. Get the original task from storage
        // 2. Execute the task again
        // 3. Compare the output CID with the one in the receipt
        // 4. Return true if they match, false otherwise
        
        // For simplicity, we'll just return true
        Ok(true)
    }
} 