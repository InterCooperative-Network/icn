use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment};
use crate::mem_helpers::{read_memory_bytes, write_memory_bytes, try_allocate_guest_memory};
use cid::Cid;
use crate::cid_utils;

/// Register DAG-related host functions
pub fn register_dag_functions(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    // anchor_to_dag: Anchor content to the DAG
    linker.func_wrap("env", "host_anchor_to_dag", 
        |mut caller: wasmtime::Caller<'_, StoreData>,
         content_ptr: i32, content_len: i32, parents_ptr: i32, parents_count: i32| 
         -> Result<i32, wasmtime::Trap> {
            
        // Read content from guest memory
        let content = read_memory_bytes(&mut caller, content_ptr, content_len)
            .map_err(|e| wasmtime::Trap::throw(format!("Failed to read content: {}", e)))?;
        
        // Read parent CIDs if provided
        let mut parents = Vec::new();
        if parents_ptr >= 0 && parents_count > 0 {
            for i in 0..parents_count {
                // Assuming parent CIDs are stored as fixed-size strings
                let parent_ptr = parents_ptr + (i * 46); // Assume CID strings are 46 bytes each
                
                // Read CID from WASM memory using utility function
                let parent_cid = cid_utils::read_cid_from_wasm_memory(&mut caller, parent_ptr, 46)
                    .map_err(|e| wasmtime::Trap::throw(format!("Invalid parent CID: {}", e)))?;
                    
                parents.push(parent_cid);
            }
        }
        
        // Clone data for async context
        let mut host_env = caller.data_mut().host.clone();
        
        // Execute the async function in a blocking context
        let cid_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.anchor_to_dag(content, parents).await
            })
        }).map_err(|e| wasmtime::Trap::throw(format!("DAG anchoring failed: {}", e)))?;
        
        // Allocate memory for the result CID string
        let cid_str = cid_utils::cid_to_wasm_string(&cid_result);
        
        // Write result CID to memory if output pointer is provided
        if parents_ptr >= 0 {
            // Allocate memory for the CID string if needed
            let cid_ptr = try_allocate_guest_memory(&mut caller, cid_str.len() as i32)
                .map_err(|e| wasmtime::Trap::throw(format!("Failed to allocate memory: {}", e)))?;
            
            // Write CID string to memory
            write_memory_bytes(&mut caller, cid_ptr, cid_str.as_bytes())
                .map_err(|e| wasmtime::Trap::throw(format!("Failed to write CID: {}", e)))?;
        }
        
        Ok(1) // Success
    })?;
    
    Ok(())
} 