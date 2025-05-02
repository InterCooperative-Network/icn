use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment};
use crate::mem_helpers::{read_memory_string, read_memory_bytes, write_memory_u64};
use icn_economics::ResourceType;
use futures::executor::block_on;
use std::collections::HashMap;

/// Write a string to guest memory
pub fn write_memory_string(caller: &mut wasmtime::Caller<'_, StoreData>, ptr: i32, value: &str) -> Result<(), anyhow::Error> {
    crate::mem_helpers::write_memory_bytes(caller, ptr, value.as_bytes())
}

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
    
    // propose_budget_spend: Create a proposal to spend from a budget
    linker.func_wrap("env", "host_propose_budget_spend", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32,
                     title_ptr: i32, title_len: i32,
                     desc_ptr: i32, desc_len: i32,
                     resources_ptr: i32, resources_len: i32,
                     category_ptr: i32, category_len: i32,
                     proposal_id_ptr: i32| -> Result<i32, anyhow::Error> {
        
        // Read parameters from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let title = read_memory_string(&mut caller, title_ptr, title_len)?;
        let description = read_memory_string(&mut caller, desc_ptr, desc_len)?;
        
        // Read resources JSON string and parse it
        let resources_json = read_memory_string(&mut caller, resources_ptr, resources_len)?;
        let resources_value: serde_json::Value = serde_json::from_str(&resources_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse resources JSON: {}", e))?;
        
        // Convert JSON to HashMap<ResourceType, u64>
        let mut requested_resources = HashMap::new();
        if let serde_json::Value::Object(map) = resources_value {
            for (key, value) in map {
                let resource_type = match key.as_str() {
                    "Compute" => ResourceType::Compute,
                    "Storage" => ResourceType::Storage,
                    "NetworkBandwidth" => ResourceType::NetworkBandwidth,
                    _ => {
                        // Skip unknown resource types
                        continue;
                    }
                };
                
                if let serde_json::Value::Number(num) = value {
                    if let Some(amount) = num.as_u64() {
                        requested_resources.insert(resource_type, amount);
                    }
                }
            }
        }
        
        // Read optional category
        let category = if category_ptr >= 0 && category_len > 0 {
            let cat = read_memory_string(&mut caller, category_ptr, category_len)?;
            if cat.is_empty() { None } else { Some(cat) }
        } else {
            None
        };
        
        // Call the host function
        let proposal_id = {
            let budget_id = budget_id.clone();
            let title = title.clone();
            let description = description.clone();
            let requested_resources = requested_resources.clone();
            let category = category.clone();
            let mut host_env = caller.data_mut().host.clone();
            
            // Execute the async function in a blocking context
            block_on(async {
                host_env.propose_budget_spend(
                    &budget_id, &title, &description, requested_resources, category).await
            }).map_err(|e| anyhow::anyhow!("Budget proposal creation failed: {}", e))?
        };
        
        // Write the proposal ID back to guest memory
        if proposal_id_ptr >= 0 {
            write_memory_string(&mut caller, proposal_id_ptr, &proposal_id.to_string())?;
        }
        
        Ok(1) // Success
    })?;
    
    // query_budget_balance: Get the available balance for a resource type in a budget
    linker.func_wrap("env", "host_query_budget_balance", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32,
                     resource_type: i32,
                     balance_ptr: i32| -> Result<i32, anyhow::Error> {
        
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
        let balance = {
            let budget_id = budget_id.clone();
            let res_type = res_type.clone();
            let host_env = caller.data().host.clone();
            
            // Execute the async function in a blocking context
            block_on(async {
                host_env.query_budget_balance(&budget_id, res_type).await
            }).map_err(|e| anyhow::anyhow!("Budget balance query failed: {}", e))?
        };
        
        // Write the balance back to guest memory
        if balance_ptr >= 0 {
            write_memory_u64(&mut caller, balance_ptr, balance)?;
        }
        
        Ok(1) // Success
    })?;
    
    Ok(())
} 