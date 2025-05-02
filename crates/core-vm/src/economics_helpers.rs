use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment};
use crate::mem_helpers::{read_memory_string};
use icn_economics::ResourceType;
use futures::executor::block_on;

/// Register economics-related host functions
pub fn register_economics_functions(linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    // check_resource_authorization: Check if a resource usage is authorized
    linker.func_wrap("env", "host_check_resource_authorization", |caller: wasmtime::Caller<'_, StoreData>,
                     resource_type: i32, amount: i32| -> Result<i32, anyhow::Error> {
        if amount < 0 {
            return Err(anyhow::anyhow!("Amount cannot be negative"));
        }
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::NetworkBandwidth,
            _ => return Err(anyhow::anyhow!("Invalid resource type: {}", resource_type)),
        };
        
        // Call the host function
        let authorized = caller.data().host.check_resource_authorization(res_type, amount as u64)
            .map_err(|e| anyhow::anyhow!("Resource authorization check failed: {}", e))?;
        
        // Return 1 for authorized, 0 for not authorized
        Ok(if authorized { 1 } else { 0 })
    })?;
    
    // record_resource_usage: Record resource consumption
    linker.func_wrap("env", "host_record_resource_usage", |mut caller: wasmtime::Caller<'_, StoreData>,
                     resource_type: i32, amount: i32| -> Result<(), anyhow::Error> {
        if amount < 0 {
            return Err(anyhow::anyhow!("Amount cannot be negative"));
        }
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::NetworkBandwidth,
            _ => return Err(anyhow::anyhow!("Invalid resource type: {}", resource_type)),
        };
        
        // Call the host function
        {
            let res_type = res_type.clone();
            let mut host_env = caller.data_mut().host.clone();
            
            host_env.record_resource_usage(res_type, amount as u64)
                .map_err(|e| anyhow::anyhow!("Resource usage recording failed: {}", e))?;
        }
        
        Ok(())
    })?;
    
    // budget_allocate: Allocate budget for a resource
    linker.func_wrap("env", "host_budget_allocate", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32, amount: i32, resource_type: i32| -> Result<i32, anyhow::Error> {
        if amount < 0 {
            return Err(anyhow::anyhow!("Amount cannot be negative"));
        }
        
        // Read budget ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::NetworkBandwidth,
            _ => return Err(anyhow::anyhow!("Invalid resource type: {}", resource_type)),
        };
        
        // Call the host function
        {
            let budget_id = budget_id.clone();
            let res_type = res_type.clone();
            let mut host_env = caller.data_mut().host.clone();
            
            // Execute the async function in a blocking context
            block_on(async {
                host_env.budget_allocate(&budget_id, amount as u64, res_type).await
            }).map_err(|e| anyhow::anyhow!("Budget allocation failed: {}", e))?;
        }
        
        Ok(1) // Success
    })?;
    
    Ok(())
} 