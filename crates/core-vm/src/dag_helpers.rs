use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment};
use crate::mem_helpers::{read_memory_string, read_memory_bytes, write_memory_bytes, try_allocate_guest_memory};
use cid::Cid;
use futures::executor::block_on;
use crate::cid_utils;

/// Register DAG-related host functions
pub fn register_dag_functions(linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    // anchor_to_dag: Anchor content to the DAG
    linker.func_wrap("env", "host_anchor_to_dag", |mut caller: wasmtime::Caller<'_, StoreData>,
                     content_ptr: i32, content_len: i32, parents_ptr: i32, parents_count: i32| -> Result<i32, anyhow::Error> {
        // Read content from guest memory
        let content = read_memory_bytes(&mut caller, content_ptr, content_len)?;
        
        // Read parent CIDs if provided
        let mut parents = Vec::new();
        if parents_ptr >= 0 && parents_count > 0 {
            for i in 0..parents_count {
                // Assuming parent CIDs are stored as fixed-size strings
                let parent_ptr = parents_ptr + (i * 46); // Assume CID strings are 46 bytes each
                
                // Read CID from WASM memory using utility function
                let parent_cid = cid_utils::read_cid_from_wasm_memory(&mut caller, parent_ptr, 46)
                    .map_err(|e| anyhow::anyhow!("Invalid parent CID: {}", e))?;
                    
                parents.push(parent_cid);
            }
        }
        
        // Call the host function
        let cid_result = {
            let content = content.clone();
            let parents = parents.clone();
            let mut host_env = caller.data_mut().host.clone();
            
            // Execute the async function without using block_on
            // Instead, we'll rely on the host's implementation to handle this properly
            let cid_future = host_env.anchor_to_dag(content, parents);
            
            // We need to block_on here since we're in a sync context
            block_on(cid_future)
                .map_err(|e| anyhow::anyhow!("DAG anchoring failed: {}", e))?
        };
        
        // Allocate memory for the result CID string
        let cid_str = cid_utils::cid_to_wasm_string(&cid_result);
        let allocated_ptr = try_allocate_guest_memory(&mut caller, cid_str.len() as i32)?;
        
        // Write the CID string to the allocated memory
        write_memory_bytes(&mut caller, allocated_ptr, cid_str.as_bytes())?;
        
        // Return a pointer to the CID string
        Ok(allocated_ptr)
    })?;
    
    Ok(())
} 