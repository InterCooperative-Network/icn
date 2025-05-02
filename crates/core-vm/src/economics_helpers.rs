use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment, LogLevel};
use crate::mem_helpers::{read_memory_string, read_memory_bytes, write_memory_u64};
use icn_economics::ResourceType;
use std::collections::HashMap;
use uuid::Uuid;

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
        let mut host_env = caller.data_mut().host.clone();
        host_env.record_resource_usage(res_type, amount as u64)
            .map_err(|e| anyhow::anyhow!("Resource usage recording failed: {}", e))?;
        
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
        
        // Clone data for async context
        let mut host_env = caller.data_mut().host.clone();
        
        // Execute the async function in a blocking context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.budget_allocate(&budget_id, amount as u64, res_type).await
            })
        }).map_err(|e| anyhow::anyhow!("Budget allocation failed: {}", e))?;
        
        Ok(1) // Success
    })?;
    
    // budget_query_balance: Get the available balance for a resource in a budget
    linker.func_wrap("env", "host_budget_query_balance", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32, resource_type: i32| -> Result<i64, anyhow::Error> {
        // Read budget ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        
        // Convert resource_type integer to ResourceType
        let res_type = match resource_type {
            0 => ResourceType::Compute,
            1 => ResourceType::Storage,
            2 => ResourceType::NetworkBandwidth,
            _ => return Err(anyhow::anyhow!("Invalid resource type: {}", resource_type)),
        };
        
        // Clone the host for async context
        let host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        let balance = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.query_budget_balance(&budget_id, res_type).await
            })
        }).map_err(|e| anyhow::anyhow!("Budget query failed: {}", e))?;
        
        // Return the balance as i64 (safe since we know it's a u64 that will fit in i64 for most balances)
        Ok(balance as i64)
    })?;
    
    // budget_vote: Vote on a budget proposal
    linker.func_wrap("env", "host_budget_vote", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32, proposal_id_ptr: i32, proposal_id_len: i32, 
                     vote_type: i32, vote_weight: i32| -> Result<i32, anyhow::Error> {
        // Read budget ID and proposal ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let proposal_id_str = read_memory_string(&mut caller, proposal_id_ptr, proposal_id_len)?;
        
        // Parse proposal ID as UUID
        let proposal_id = Uuid::parse_str(&proposal_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal ID: {}", e))?;
        
        // Convert vote_type to VoteChoice
        let vote_choice = match vote_type {
            0 => icn_economics::VoteChoice::Approve,
            1 => icn_economics::VoteChoice::Reject,
            2 => icn_economics::VoteChoice::Abstain,
            3 => icn_economics::VoteChoice::Quadratic(vote_weight as u32), // Quadratic voting with weight, convert i32 to u32
            _ => return Err(anyhow::anyhow!("Invalid vote type: {}", vote_type)),
        };
        
        // Get the caller DID to use as voter
        let voter_did = caller.data().ctx.caller_did.clone();
        
        // Log the vote for debugging
        let mut host = caller.data().host.clone();
        let _ = host.log_message(LogLevel::Info, &format!("Recording vote from {}: {:?}", voter_did, vote_choice));
        
        // Clone the host for async context
        let mut host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.record_budget_vote(&budget_id, proposal_id, vote_choice).await
            })
        }).map_err(|e| anyhow::anyhow!("Budget vote failed: {}", e))?;
        
        Ok(1) // Success
    })?;
    
    // budget_tally_votes: Tally votes on a budget proposal
    linker.func_wrap("env", "host_budget_tally_votes", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32, proposal_id_ptr: i32, proposal_id_len: i32| -> Result<i32, anyhow::Error> {
        // Read budget ID and proposal ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let proposal_id_str = read_memory_string(&mut caller, proposal_id_ptr, proposal_id_len)?;
        
        // Parse proposal ID as UUID
        let proposal_id = Uuid::parse_str(&proposal_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal ID: {}", e))?;
        
        // Clone the host for async context
        let host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        let status = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.tally_budget_votes(&budget_id, proposal_id).await
            })
        }).map_err(|e| anyhow::anyhow!("Budget tally failed: {}", e))?;
        
        // Convert status to integer result
        let status_code = match status {
            icn_economics::ProposalStatus::Proposed => 0,
            icn_economics::ProposalStatus::VotingOpen => 1,
            icn_economics::ProposalStatus::VotingClosed => 2,
            icn_economics::ProposalStatus::Approved => 3,
            icn_economics::ProposalStatus::Rejected => 4,
            icn_economics::ProposalStatus::Executed => 5,
            icn_economics::ProposalStatus::Failed => 6,
            icn_economics::ProposalStatus::Cancelled => 7,
        };
        
        Ok(status_code)
    })?;
    
    // budget_finalize_proposal: Finalize a budget proposal
    linker.func_wrap("env", "host_budget_finalize_proposal", |mut caller: wasmtime::Caller<'_, StoreData>,
                     budget_id_ptr: i32, budget_id_len: i32, proposal_id_ptr: i32, proposal_id_len: i32| -> Result<i32, anyhow::Error> {
        // Read budget ID and proposal ID from guest memory
        let budget_id = read_memory_string(&mut caller, budget_id_ptr, budget_id_len)?;
        let proposal_id_str = read_memory_string(&mut caller, proposal_id_ptr, proposal_id_len)?;
        
        // Parse proposal ID as UUID
        let proposal_id = Uuid::parse_str(&proposal_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal ID: {}", e))?;
        
        // Clone the host for async context
        let mut host_env = caller.data().host.clone();
        
        // Execute the async function in a blocking context
        let status = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.finalize_budget_proposal(&budget_id, proposal_id).await
            })
        }).map_err(|e| anyhow::anyhow!("Budget finalization failed: {}", e))?;
        
        // Convert status to integer result
        let status_code = match status {
            icn_economics::ProposalStatus::Proposed => 0,
            icn_economics::ProposalStatus::VotingOpen => 1,
            icn_economics::ProposalStatus::VotingClosed => 2,
            icn_economics::ProposalStatus::Approved => 3,
            icn_economics::ProposalStatus::Rejected => 4,
            icn_economics::ProposalStatus::Executed => 5,
            icn_economics::ProposalStatus::Failed => 6,
            icn_economics::ProposalStatus::Cancelled => 7,
        };
        
        Ok(status_code)
    })?;
    
    Ok(())
} 