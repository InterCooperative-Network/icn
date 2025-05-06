use anyhow::Error as AnyhowError;
use wasmtime::Linker;
use crate::ConcreteHostEnvironment;
use crate::mem_helpers::{read_memory_string};
use crate::host_abi::create_trap;
use crate::resources::ResourceType;
use std::collections::HashMap;
use uuid::Uuid;

/// Log level enum for VM logging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Write a string to guest memory
pub fn write_memory_string(caller: &mut wasmtime::Caller<'_, ConcreteHostEnvironment>, ptr: i32, value: &str) -> Result<(), AnyhowError> {
    crate::mem_helpers::write_memory_bytes(caller, ptr, value.as_bytes())
}

/// Register economics-related host functions
pub fn register_economics_functions(linker: &mut Linker<ConcreteHostEnvironment>) -> Result<(), wasmtime::Error> {
    // check_resource_authorization: Check if a resource usage is authorized
    linker.func_wrap("env", "economics_check_resource_authorization", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         resource_type: i32, amount: i32| 
         -> Result<i32, wasmtime::Trap> {
             
        if amount < 0 {
            return Err(create_trap("Amount cannot be negative"));
        }
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::Network,
            3 => ResourceType::Token,
            _ => return Err(create_trap(format!("Invalid resource type: {}", resource_type))),
        };
        
        // Check if the amount is below the authorized limit
        let host_env = caller.data();
        let auth_limits = host_env.vm_context.resource_authorizations();
        
        let authorized = auth_limits.iter()
            .find(|auth| auth.resource_type == res_type)
            .map(|auth| auth.limit >= amount as u64)
            .unwrap_or(false);
        
        // Return 1 for authorized, 0 for not authorized
        Ok(if authorized { 1 } else { 0 })
    })?;
    
    // Record resource usage (simplified)
    linker.func_wrap("env", "economics_record_resource_usage", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         resource_type: i32, amount: i32| 
         -> Result<(), wasmtime::Trap> {
             
        if amount < 0 {
            return Err(create_trap("Amount cannot be negative"));
        }
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::Network,
            3 => ResourceType::Token,
            _ => return Err(create_trap(format!("Invalid resource type: {}", resource_type))),
        };
        
        // Get host environment and record usage
        let env = caller.data();
        
        // Use the proper method for recording resource consumption
        env.record_resource_consumption(res_type, amount as u64)
            .map_err(|e| create_trap(format!("Resource usage recording failed: {}", e)))?;
        
        Ok(())
    })?;
    
    // budget_allocate: Allocate budget for a resource
    linker.func_wrap("env", "economics_budget_allocate", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         budget_id_ptr: i32, budget_id_len: i32, 
         amount: i32, resource_type: i32| -> Result<i32, AnyhowError> {
        
        if amount < 0 {
            return Err(anyhow::anyhow!("Amount cannot be negative"));
        }
        
        // Read budget ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::Network,
            3 => ResourceType::Token,
            _ => return Err(anyhow::anyhow!("Invalid resource type: {}", resource_type)),
        };
        
        // Get host environment
        let env = caller.data();
        
        // Execute the async function in a blocking context
        let handle = tokio::runtime::Handle::current();
        handle.block_on(async {
            // This is stubbed out since the actual implementation depends on the economics subsystem
            Ok(1) // Pretend it succeeded
        })
    })?;
    
    // budget_query_balance: Get the available balance for a resource in a budget
    linker.func_wrap("env", "economics_budget_query_balance", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         budget_id_ptr: i32, budget_id_len: i32, 
         resource_type: i32| -> Result<i64, AnyhowError> {
        
        // Read budget ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::Network,
            3 => ResourceType::Token,
            _ => return Err(anyhow::anyhow!("Invalid resource type: {}", resource_type)),
        };
        
        // Get host environment
        let env = caller.data();
        
        // Execute the async function in a blocking context
        let handle = tokio::runtime::Handle::current();
        let balance = handle.block_on(async {
            // This is stubbed out since the actual implementation depends on the economics subsystem
            Ok(1000i64) // Pretend it returns a balance of 1000
        })?;
        
        Ok(balance)
    })?;
    
    // budget_vote: Vote on a budget proposal
    linker.func_wrap("env", "economics_budget_vote", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         budget_id_ptr: i32, budget_id_len: i32, 
         proposal_id_ptr: i32, proposal_id_len: i32, 
         vote_type: i32, vote_weight: i32| -> Result<i32, AnyhowError> {
        
        // Read budget ID and proposal ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let proposal_id_str = read_memory_string(&mut caller, proposal_id_ptr, proposal_id_len)?;
        
        // Parse proposal ID as UUID
        let proposal_id = Uuid::parse_str(&proposal_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal ID: {}", e))?;
        
        // Convert vote_type to VoteChoice (using a placeholder implementation)
        enum VoteChoice {
            Approve,
            Reject,
            Abstain,
            Quadratic(u32),
        }
        
        let vote_choice = match vote_type {
            0 => VoteChoice::Approve,
            1 => VoteChoice::Reject,
            2 => VoteChoice::Abstain,
            3 => VoteChoice::Quadratic(vote_weight as u32), // Quadratic voting with weight, convert i32 to u32
            _ => return Err(anyhow::anyhow!("Invalid vote type: {}", vote_type)),
        };
        
        // Get the caller DID to use as voter
        let voter_did = caller.data().caller_did().to_string();
        
        // Get host environment
        let env = caller.data();
        
        // Execute the async function in a blocking context
        let handle = tokio::runtime::Handle::current();
        handle.block_on(async {
            // This is stubbed out since the actual implementation depends on the economics subsystem
            Ok(1) // Pretend it succeeded
        })
    })?;
    
    // budget_tally_votes: Tally votes on a budget proposal
    linker.func_wrap("env", "economics_budget_tally_votes", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         budget_id_ptr: i32, budget_id_len: i32, 
         proposal_id_ptr: i32, proposal_id_len: i32| -> Result<i32, AnyhowError> {
        
        // Read budget ID and proposal ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let proposal_id_str = read_memory_string(&mut caller, proposal_id_ptr, proposal_id_len)?;
        
        // Parse proposal ID as UUID
        let proposal_id = Uuid::parse_str(&proposal_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal ID: {}", e))?;
        
        // Get host environment
        let env = caller.data();
        
        // Execute the async function in a blocking context
        let handle = tokio::runtime::Handle::current();
        let status_code = handle.block_on(async {
            // This is stubbed out since the actual implementation depends on the economics subsystem
            // Return status code 3 (Approved)
            Ok(3)
        })?;
        
        Ok(status_code)
    })?;
    
    // budget_finalize_proposal: Finalize a budget proposal
    linker.func_wrap("env", "economics_budget_finalize_proposal", 
        move |mut caller: wasmtime::Caller<'_, ConcreteHostEnvironment>,
         budget_id_ptr: i32, budget_id_len: i32, 
         proposal_id_ptr: i32, proposal_id_len: i32| -> Result<i32, AnyhowError> {
        
        // Read budget ID and proposal ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let proposal_id_str = read_memory_string(&mut caller, proposal_id_ptr, proposal_id_len)?;
        
        // Parse proposal ID as UUID
        let proposal_id = Uuid::parse_str(&proposal_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal ID: {}", e))?;
        
        // Get host environment
        let env = caller.data();
        
        // Execute the async function in a blocking context
        let handle = tokio::runtime::Handle::current();
        let status_code = handle.block_on(async {
            // This is stubbed out since the actual implementation depends on the economics subsystem
            // Return status code 5 (Executed)
            Ok(5)
        })?;
        
        Ok(status_code)
    })?;
    
    Ok(())
} 