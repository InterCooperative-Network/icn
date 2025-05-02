use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment};
use crate::mem_helpers::{read_memory_bytes, write_memory_bytes, write_memory_u32};
use crate::cid_utils;

/// Register storage-related host functions
pub fn register_storage_functions(linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    // storage_get: Get a value from storage by CID
    linker.func_wrap("env", "host_storage_get", |mut caller: wasmtime::Caller<'_, StoreData>, 
                     cid_ptr: i32, cid_len: i32, out_ptr: i32, out_len_ptr: i32| -> Result<i32, anyhow::Error> {
        // Read CID from WASM memory using utility function
        let cid = cid_utils::read_cid_from_wasm_memory(&mut caller, cid_ptr, cid_len)
            .map_err(|e| anyhow::anyhow!("Invalid CID: {}", e))?;
        
        // Clone the host environment for use in async context
        let mut host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.storage_get(cid).await
            })
        }).map_err(|e| anyhow::anyhow!("Storage get failed: {}", e))?;
        
        // If value is found, write it to guest memory
        match result {
            Some(value) => {
                let value_len = value.len() as i32;
                
                // Check if the output buffer is large enough
                if out_len_ptr >= 0 {
                    write_memory_u32(&mut caller, out_len_ptr, value_len as u32)?;
                }
                
                // Write the value to guest memory if buffer is provided
                if out_ptr >= 0 && value_len > 0 {
                    write_memory_bytes(&mut caller, out_ptr, &value)?;
                }
                
                // Return 1 if value was found
                Ok(1)
            },
            None => {
                // Return 0 if value was not found
                Ok(0)
            }
        }
    })?;
    
    // storage_put: Store a key-value pair in storage
    linker.func_wrap("env", "host_storage_put", |mut caller: wasmtime::Caller<'_, StoreData>,
                     key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32| -> Result<i32, anyhow::Error> {
        // Read CID from WASM memory using utility function
        let cid = cid_utils::read_cid_from_wasm_memory(&mut caller, key_ptr, key_len)
            .map_err(|e| anyhow::anyhow!("Invalid CID: {}", e))?;
        
        // Read value from guest memory
        let value = read_memory_bytes(&mut caller, value_ptr, value_len)?;
        
        // Clone the host environment and value for use in async context
        let mut host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.storage_put(cid, value).await
            })
        }).map_err(|e| anyhow::anyhow!("Storage put failed: {}", e))?;
        
        Ok(1) // Success
    })?;
    
    // blob_put: Store a blob in IPFS
    linker.func_wrap("env", "host_blob_put", |mut caller: wasmtime::Caller<'_, StoreData>,
                     content_ptr: i32, content_len: i32, out_ptr: i32, out_len: i32| -> Result<i32, anyhow::Error> {
        // Read content from guest memory
        let content = read_memory_bytes(&mut caller, content_ptr, content_len)?;
        
        // Clone the host environment for use in async context
        let mut host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        let cid_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.blob_put(content).await
            })
        }).map_err(|e| anyhow::anyhow!("Blob put failed: {}", e))?;
        
        // Write the CID to guest memory using utility function
        cid_utils::write_cid_to_wasm_memory(&mut caller, &cid_result, out_ptr, out_len)
            .map_err(|e| anyhow::anyhow!("Failed to write CID to memory: {}", e))?;
        
        Ok(1) // Success
    })?;
    
    // blob_get: Retrieve a blob by CID
    linker.func_wrap("env", "host_blob_get", |mut caller: wasmtime::Caller<'_, StoreData>,
                     cid_ptr: i32, cid_len: i32, out_ptr: i32, out_len_ptr: i32| -> Result<i32, anyhow::Error> {
        // Read CID from WASM memory using utility function
        let cid = cid_utils::read_cid_from_wasm_memory(&mut caller, cid_ptr, cid_len)
            .map_err(|e| anyhow::anyhow!("Invalid CID: {}", e))?;
        
        // Clone the host environment for use in async context
        let mut host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.blob_get(cid).await
            })
        }).map_err(|e| anyhow::anyhow!("Blob get failed: {}", e))?;
        
        // If blob is found, write it to guest memory
        match result {
            Some(data) => {
                let data_len = data.len() as i32;
                
                // Write size to out_len_ptr if provided
                if out_len_ptr >= 0 {
                    write_memory_u32(&mut caller, out_len_ptr, data_len as u32)?;
                }
                
                // Write data to out_ptr if provided and data is not empty
                if out_ptr >= 0 && data_len > 0 {
                    write_memory_bytes(&mut caller, out_ptr, &data)?;
                }
                
                Ok(1) // Success with data
            },
            None => Ok(0) // Not found
        }
    })?;
    
    Ok(())
} 