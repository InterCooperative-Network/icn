use anyhow::Result;
use chrono::{Duration, Utc};
use cid::Cid;
use mesh_net::{MeshExecutionEngine, MeshNetwork, WasmtimeTaskRunner};
use mesh_types::{TaskIntent, TaskRunner};
use std::{path::PathBuf, str::FromStr};
use tokio::sync::mpsc;

// Helper function to create a test WASM file
async fn create_test_wasm_file(wasm_dir: &PathBuf) -> Result<Cid> {
    use tokio::fs;
    
    // Create the directory if it doesn't exist
    fs::create_dir_all(wasm_dir).await?;
    
    // Simple WAT module that adds two numbers
    let wat = r#"
    (module
      ;; Import memory and host functions
      (import "env" "memory" (memory 1))
      (import "env" "input_load" (func $input_load (param i32 i32) (result i32)))
      (import "env" "output_store" (func $output_store (param i32 i32) (result i32)))
      (import "env" "log" (func $log (param i32 i32 i32)))
      
      ;; Input/output buffers
      (data (i32.const 0) "Input buffer")
      (data (i32.const 100) "Output buffer")
      
      ;; Message for logging
      (data (i32.const 200) "Hello from WASM task!")
      
      ;; Main function
      (func (export "_start")
        ;; Log a message
        i32.const 1          ;; Log level (INFO)
        i32.const 200        ;; Message pointer
        i32.const 22         ;; Message length
        call $log
        
        ;; Load input data into memory at position 0
        i32.const 0         ;; Buffer position
        i32.const 1024      ;; Max length
        call $input_load    ;; Returns actual input length
        drop                ;; Ignore the length for this simple test
        
        ;; Store a simple output ("Hello, Mesh!")
        (i32.store (i32.const 100) (i32.const 0x6c6c6548))  ;; "Hell"
        (i32.store (i32.const 104) (i32.const 0x654d206f))  ;; "o Me"
        (i32.store (i32.const 108) (i32.const 0x21687373))  ;; "sh!"
        (i32.store (i32.const 112) (i32.const 0x00000000))  ;; Null terminator
        
        ;; Store the output (12 bytes)
        i32.const 100        ;; Output buffer 
        i32.const 12         ;; Output length
        call $output_store
        drop                 ;; Ignore result
      )
    )
    "#;
    
    // Convert WAT to WASM
    let wasm = wat::parse_str(wat)?;
    
    // Generate a CID for the WASM
    let cid = icn_common::utils::cid_utils::bytes_to_cid(&wasm)?;
    
    // Save the WASM file
    let wasm_path = wasm_dir.join(format!("{}.wasm", cid.to_string()));
    fs::write(wasm_path, wasm).await?;
    
    Ok(cid)
}

// Helper function to create test input data
async fn create_test_input(input_dir: &PathBuf) -> Result<Cid> {
    use tokio::fs;
    
    // Create the directory if it doesn't exist
    fs::create_dir_all(input_dir).await?;
    
    // Simple input data (JSON in this case)
    let input = r#"{"value1": 123, "value2": 456}"#.as_bytes();
    
    // Generate a CID for the input
    let cid = icn_common::utils::cid_utils::bytes_to_cid(input)?;
    
    // Save the input file
    let input_path = input_dir.join(format!("{}.bin", cid.to_string()));
    fs::write(input_path, input).await?;
    
    Ok(cid)
}

#[tokio::test]
async fn test_mesh_compute_task_execution() -> Result<()> {
    // Set up test directories
    let test_dir = PathBuf::from("/tmp/icn-mesh-test");
    let wasm_dir = test_dir.join("wasm");
    let input_dir = test_dir.join("input");
    let output_dir = test_dir.join("output");
    
    // Clean up previous test data
    let _ = tokio::fs::remove_dir_all(&test_dir).await;
    
    // Create test WASM and input files
    let wasm_cid = create_test_wasm_file(&wasm_dir).await?;
    let input_cid = create_test_input(&input_dir).await?;
    
    // Create a task intent
    let task = TaskIntent {
        publisher_did: "did:icn:test:publisher".to_string(),
        wasm_cid,
        input_cid,
        fee: 100,
        verifiers: 3,
        expiry: Utc::now() + Duration::minutes(60),
        metadata: None,
    };
    
    // Create a task runner
    let runner = WasmtimeTaskRunner::new(
        wasm_dir.clone(),
        input_dir.clone(),
        output_dir.clone(),
        "did:icn:test:worker",
    )?;
    
    // Execute the task
    let result = runner.execute_task(&task).await?;
    
    // Verify execution was successful
    assert_eq!(result.exit_code, 0, "Task execution failed");
    assert!(result.metrics.fuel_consumed > 0, "No fuel consumed");
    assert!(result.metrics.execution_time_ms > 0, "No execution time recorded");
    
    // Generate a receipt
    let receipt = runner.generate_receipt(&task, &result, "did:icn:test:worker")?;
    
    // Verify the receipt
    let verification = runner.verify_receipt(&receipt).await?;
    assert!(verification, "Receipt verification failed");
    
    // Clean up test data
    let _ = tokio::fs::remove_dir_all(&test_dir).await;
    
    Ok(())
}

#[tokio::test]
async fn test_mesh_compute_network() -> Result<()> {
    // Create a mesh network
    let (mut network, event_rx) = MeshNetwork::new().await?;
    
    // Set up test directories
    let test_dir = PathBuf::from("/tmp/icn-mesh-net-test");
    let wasm_dir = test_dir.join("wasm");
    let input_dir = test_dir.join("input");
    let output_dir = test_dir.join("output");
    
    // Clean up previous test data
    let _ = tokio::fs::remove_dir_all(&test_dir).await;
    
    // Create the execution engine
    let event_sender = network.event_sender.clone();
    let execution_engine = MeshExecutionEngine::new(event_sender, wasm_dir.clone(), output_dir.clone());
    
    // Create test WASM and input files
    let wasm_cid = create_test_wasm_file(&wasm_dir).await?;
    let input_cid = create_test_input(&input_dir).await?;
    
    // Create a task intent
    let task = TaskIntent {
        publisher_did: "did:icn:test:publisher".to_string(),
        wasm_cid,
        input_cid,
        fee: 100,
        verifiers: 3,
        expiry: Utc::now() + Duration::minutes(60),
        metadata: None,
    };
    
    // Process the task
    execution_engine.process_task(task.clone()).await?;
    
    // Check task status
    let task_cid = mesh_net::execution::task_to_cid(&task)?;
    let status = execution_engine.get_task_status(&task_cid);
    
    assert!(status.is_some(), "Task status not found");
    
    // Clean up test data
    let _ = tokio::fs::remove_dir_all(&test_dir).await;
    
    Ok(())
} 