/*!
# Host ABI Security Layer

This module provides a security-hardened ABI surface for the Core VM. It handles:
1. Input validation for all host functions
2. Bounds checking for memory access
3. Resource usage tracking and limits
4. Secure error handling
*/

use crate::{
    ConcreteHostEnvironment, VmError, ResourceType,
    HostEnvironment
};
use crate::mem_helpers::{read_memory_string, write_memory_string, read_memory_bytes, safe_check_bounds};
use wasmtime::{Caller, Linker, Memory, Trap, WasmBacktrace};
use tracing::*;
use anyhow::{anyhow, Error};
use crate::InternalHostError;
use icn_storage::StorageError;
use cid::Cid;
use std::convert::TryInto;
use std::io::Cursor;
use icn_models::dag_storage_codec;
use icn_models::DagNode;

/// Maximum allowed length for a key or value in bytes
const MAX_STRING_LENGTH: usize = 1024 * 1024; // 1 MB

/// Maximum allowed size for a memory allocation
const MAX_ALLOCATION_SIZE: u32 = 1024 * 1024 * 10; // 10 MB

/// Trait result type alias
type HostAbiResult<T> = Result<T, Error>;

/// Maps InternalHostError to a negative i32 code for WASM return.
fn map_internal_error_to_wasm(err: InternalHostError) -> i32 {
    error!(error = %err, "Internal host error during ABI call");
    match err {
        InternalHostError::IdentityError(_) => -1,
        InternalHostError::StorageError(_) => -2,
        InternalHostError::DagError(_) => -3,
        InternalHostError::CodecError(_) => -4,
        InternalHostError::InvalidInput(_) => -5,
        InternalHostError::ConfigurationError(_) => -6,
        InternalHostError::Other(_) => -99, // Generic internal error
        // Map VmError::ResourceLimitExceeded specifically if needed
        // Or handle resource limits before calling the internal logic
    }
}

/// Function to map anyhow::Error from helpers to i32 WASM error code
/// Using a distinct code for memory/ABI argument errors
fn map_abi_error_to_wasm(err: Error) -> i32 {
    error!(error = %err, "Host ABI argument/memory error");
    -101 // Example: Generic ABI error code
}

/// Function to map VmError (e.g., resource limit) to i32 WASM error code
fn map_vm_error_to_wasm(err: VmError) -> i32 {
    error!(error = %err, "Host resource/VM error");
    match err {
        VmError::ResourceLimitExceeded(_) => -102,
        _ => -100, // Other VM errors
    }
}

/// Checks compute resource limits before proceeding.
fn check_compute(caller: &Caller<'_, ConcreteHostEnvironment>, cost: u64) -> Result<(), i32> {
     let env = caller.data();
     let current = env.get_compute_consumed();
     let limit = env.vm_context.resource_authorizations().iter()
         .find(|auth| auth.resource_type == ResourceType::Compute)
         .map_or(0, |auth| auth.limit);

     if current.saturating_add(cost) > limit {
         error!(current, cost, limit, "Compute resource limit exceeded");
         Err(map_vm_error_to_wasm(VmError::ResourceLimitExceeded("Compute limit hit".into())))
     } else {
         Ok(())
     }
}

/// Safely read bytes, returning HostAbiResult
fn safe_read_bytes(
    caller: &Caller<'_, ConcreteHostEnvironment>,
    ptr: u32,
    len: u32,
) -> HostAbiResult<Vec<u8>> {
    let memory = caller.get_export("memory")
        .and_then(|exp| exp.into_memory())
        .ok_or_else(|| anyhow!("Memory export not found"))?;
    safe_check_bounds(&memory, caller, ptr, len)?;
    let data = memory.data(caller);
    Ok(data[ptr as usize..(ptr + len) as usize].to_vec())
}

/// Safely read string, returning HostAbiResult
fn safe_read_string(
    caller: &Caller<'_, ConcreteHostEnvironment>,
    ptr: u32,
    len: u32,
) -> HostAbiResult<String> {
    let bytes = safe_read_bytes(caller, ptr, len)?;
    String::from_utf8(bytes).map_err(|e| anyhow!("Invalid UTF-8 sequence: {}", e))
}

/// Safely write bytes to WASM memory.
fn safe_write_bytes(
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
    bytes: &[u8],
    out_ptr: u32,
    max_len: u32,
) -> HostAbiResult<usize> { // Returns bytes written
    let bytes_len = bytes.len();
    if bytes_len > max_len as usize {
        return Err(anyhow!("Output buffer too small: required {}, max {}", bytes_len, max_len));
    }

    let memory = caller.get_export("memory")
        .and_then(|exp| exp.into_memory())
        .ok_or_else(|| anyhow!("Memory export not found"))?;

    // Check bounds *before* writing
    safe_check_bounds(&memory, caller, out_ptr, bytes_len as u32)?;

    memory.write(caller, out_ptr as usize, bytes)
        .map_err(|e| anyhow!("Memory write failed: {}", e))?;

    Ok(bytes_len)
}

/// Safely write string to WASM memory.
fn safe_write_string(
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
    value: &str,
    ptr: u32,
    max_len: u32,
) -> HostAbiResult<i32> {
    safe_write_bytes(caller, value.as_bytes(), ptr, max_len).map(|len| len as i32)
}

/// Helper for reading Vec<Vec<u8>> from WASM memory.
/// Reads an array of pointers and an array of lengths.
fn read_vec_of_bytes(
    caller: &Caller<'_, ConcreteHostEnvironment>,
    ptr_ptr: u32,
    count: u32,
    lens_ptr: u32,
) -> HostAbiResult<(Vec<Vec<u8>>, u64)> { // Return cost as well
    let mut vecs = Vec::with_capacity(count as usize);
    let mut cost = 0u64;
    let memory = caller.get_export("memory")
        .and_then(|exp| exp.into_memory())
        .ok_or_else(|| anyhow!("Memory export not found"))?;

    // Check bounds for reading the pointer array and length array
    safe_check_bounds(&memory, caller, ptr_ptr, count * 4)?;
    safe_check_bounds(&memory, caller, lens_ptr, count * 4)?;

    let data = memory.data(caller);

    for i in 0..count {
        // Read the pointer to the i-th byte vector
        let current_ptr_offset = (ptr_ptr + i * 4) as usize;
        let current_ptr_bytes: [u8; 4] = data[current_ptr_offset..current_ptr_offset + 4]
            .try_into()
            .map_err(|_| anyhow!("Failed to read pointer bytes"))?;
        let current_ptr = u32::from_le_bytes(current_ptr_bytes);

        // Read the length of the i-th byte vector
        let current_len_offset = (lens_ptr + i * 4) as usize;
        let current_len_bytes: [u8; 4] = data[current_len_offset..current_len_offset + 4]
            .try_into()
            .map_err(|_| anyhow!("Failed to read length bytes"))?;
        let current_len = u32::from_le_bytes(current_len_bytes);

        // Read the actual bytes using safe_read_bytes (which does its own bounds check)
        let bytes = safe_read_bytes(caller, current_ptr, current_len)?;
        cost += current_len as u64;
        vecs.push(bytes);
    }

    Ok((vecs, cost))
}

/// Wrapper for host_create_sub_dag
fn host_create_sub_dag_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    parent_did_ptr: u32,
    parent_did_len: u32,
    genesis_payload_ptr: u32,
    genesis_payload_len: u32,
    entity_type_ptr: u32,
    entity_type_len: u32,
    did_out_ptr: u32,
    did_out_max_len: u32
) -> Result<i32, Trap> { // Trap on fatal error
    debug!(parent_did_ptr, genesis_payload_ptr, entity_type_ptr, "host_create_sub_dag called");
    let result = || -> HostAbiResult<String> {
        let parent_did = safe_read_string(&caller, parent_did_ptr, parent_did_len)?;
        let genesis_payload = safe_read_bytes(&caller, genesis_payload_ptr, genesis_payload_len)?;
        let entity_type = safe_read_string(&caller, entity_type_ptr, entity_type_len)?;
        Ok((parent_did, genesis_payload, entity_type))
    }();

    let (parent_did, genesis_payload, entity_type) = match result {
        Ok(data) => data,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Resource check
    let estimated_compute_cost = 7000_u64;
    let estimated_storage_cost = genesis_payload.len() as u64 + 512;
    if let Err(code) = check_compute(&caller, estimated_compute_cost) { return Ok(code); }
    // TODO: Check storage limit if necessary

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.create_sub_entity_dag(&parent_did, genesis_payload, &entity_type));

    match result {
        Ok(new_did) => {
            debug!(new_did=%new_did, "Sub-entity creation successful");
            // Write DID back
            match safe_write_string(&mut caller, &new_did, did_out_ptr, did_out_max_len) {
                Ok(len) => Ok(len),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        }
        Err(internal_err) => Ok(map_internal_error_to_wasm(internal_err)),
    }
}

/// Wrapper for host_store_node
fn host_store_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    entity_did_ptr: u32, entity_did_len: u32,
    payload_ptr: u32, payload_len: u32,
    parents_cids_ptr_ptr: u32, parents_cids_count: u32, parent_cid_lens_ptr: u32,
    signature_ptr: u32, signature_len: u32,
    metadata_ptr: u32, metadata_len: u32,
    cid_out_ptr: u32, cid_out_max_len: u32,
) -> Result<i32, Trap> {
    debug!("Host ABI: host_store_node called");
    let mut cost = 1000; // Base cost

    // Use closure for fallible reading
    let result = || -> HostAbiResult<_> {
        let entity_did = safe_read_string(&caller, entity_did_ptr, entity_did_len)?;
        cost += entity_did_len as u64 * 2;
        let payload_bytes = safe_read_bytes(&caller, payload_ptr, payload_len)?;
        cost += payload_len as u64;
        let signature_bytes = safe_read_bytes(&caller, signature_ptr, signature_len)?;
        cost += signature_len as u64;
        let metadata_bytes = safe_read_bytes(&caller, metadata_ptr, metadata_len)?;
        cost += metadata_len as u64;

        let (parent_cids_bytes, read_cost) = read_vec_of_bytes(
            &caller,
            parents_cids_ptr_ptr,
            parents_cids_count,
            parent_cid_lens_ptr,
        )?;
        cost += read_cost;
        Ok((entity_did, payload_bytes, parent_cids_bytes, signature_bytes, metadata_bytes))
    }();

    let (entity_did, payload_bytes, parent_cids_bytes, signature_bytes, metadata_bytes) = match result {
        Ok(data) => data,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Preliminary resource check
    if let Err(code) = check_compute(&caller, cost) { return Ok(code); }

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.store_node(
        &entity_did,
        payload_bytes,
        parent_cids_bytes,
        signature_bytes,
        metadata_bytes,
    ));

    match result {
        Ok(cid) => {
            let cid_bytes = cid.to_bytes();
            match safe_write_bytes(&mut caller, &cid_bytes, cid_out_ptr, cid_out_max_len) {
                Ok(len) => Ok(len as i32),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        }
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_get_node
fn host_get_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    entity_did_ptr: u32, entity_did_len: u32,
    cid_ptr: u32, cid_len: u32,
    node_out_ptr: u32, node_out_max_len: u32,
) -> Result<i32, Trap> {
    debug!("Host ABI: host_get_node called");
    let mut cost = 200; // Base cost

    let result = || -> HostAbiResult<_> {
        let entity_did = safe_read_string(&caller, entity_did_ptr, entity_did_len)?;
        cost += entity_did_len as u64;
        let cid_bytes = safe_read_bytes(&caller, cid_ptr, cid_len)?;
        cost += cid_len as u64;
        Ok((entity_did, cid_bytes))
    }();

     let (entity_did, cid_bytes) = match result {
        Ok(data) => data,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Preliminary resource check
    if let Err(code) = check_compute(&caller, cost) { return Ok(code); }

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.get_node(&entity_did, cid_bytes));

    match result {
        Ok(Some(node_bytes)) => {
            match safe_write_bytes(&mut caller, &node_bytes, node_out_ptr, node_out_max_len) {
                Ok(len) => Ok(len as i32),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        }
        Ok(None) => Ok(0), // Indicate node not found
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_contains_node
fn host_contains_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    entity_did_ptr: u32, entity_did_len: u32,
    cid_ptr: u32, cid_len: u32,
) -> Result<i32, Trap> {
    debug!("Host ABI: host_contains_node called");
    let mut cost = 100; // Base cost

    let result = || -> HostAbiResult<_> {
        let entity_did = safe_read_string(&caller, entity_did_ptr, entity_did_len)?;
        cost += entity_did_len as u64;
        let cid_bytes = safe_read_bytes(&caller, cid_ptr, cid_len)?;
        cost += cid_len as u64;
        Ok((entity_did, cid_bytes))
    }();

     let (entity_did, cid_bytes) = match result {
        Ok(data) => data,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Preliminary resource check
    if let Err(code) = check_compute(&caller, cost) { return Ok(code); }

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.contains_node(&entity_did, cid_bytes));

    match result {
        Ok(true) => Ok(1),
        Ok(false) => Ok(0),
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_check_resource_authorization
fn host_check_resource_authorization_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    resource_type: i32,
    amount: i32,
) -> Result<i32, Trap> {
    debug!(resource_type, amount, "host_check_resource_authorization called");
    
    if amount < 0 {
        return Err(Trap::throw("Amount cannot be negative"));
    }
    
    // Convert resource_type integer to ResourceType
    let res_type = match resource_type {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        3 => ResourceType::Token,
        _ => return Err(Trap::throw(format!("Invalid resource type: {}", resource_type))),
    };
    
    // Get host environment
    let env = caller.data();
    
    // Check if the caller has authorization for this resource usage
    let authorized = env.check_resource_authorization(res_type, amount as u64)
        .map_err(|e| Trap::throw(format!("Resource authorization check failed: {}", e)))?;
    
    // Return 1 for authorized, 0 for not authorized
    Ok(if authorized { 1 } else { 0 })
}

/// Wrapper for host_record_resource_usage
fn host_record_resource_usage_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    resource_type: i32,
    amount: i32,
) -> Result<(), Trap> {
    debug!(resource_type, amount, "host_record_resource_usage called");
    
    if amount < 0 {
        return Err(Trap::throw("Amount cannot be negative"));
    }
    
    // Convert resource_type integer to ResourceType
    let res_type = match resource_type {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        3 => ResourceType::Token,
        _ => return Err(Trap::throw(format!("Invalid resource type: {}", resource_type))),
    };
    
    // Get host environment and record usage
    let env = caller.data_mut();
    
    // Record the usage
    env.record_resource_usage(res_type, amount as u64)
        .map_err(|e| Trap::throw(format!("Resource usage recording failed: {}", e)))?;
    
    // Also anchor the usage to DAG for governance tracking
    // Get current timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Trap::throw(format!("Failed to get timestamp: {}", e)))?
        .as_secs();
    
    // Anchor the usage record to DAG (asynchronously, don't block)
    let usage_record = serde_json::json!({
        "resource_type": format!("{}", res_type),
        "amount": amount,
        "timestamp": timestamp,
        "caller_did": env.caller_did().to_string(),
        "execution_context": env.vm_context.execution_id()
    });
    
    // Serialize the usage record
    let usage_bytes = match serde_json::to_vec(&usage_record) {
        Ok(bytes) => bytes,
        Err(e) => {
            // Log the error but don't fail the operation
            error!("Failed to serialize usage record: {}", e);
            return Ok(());
        }
    };
    
    // Spawn a task to anchor the data to DAG
    let env_clone = env.clone();
    tokio::spawn(async move {
        if let Err(e) = env_clone.anchor_to_dag(&format!("resource_usage:{}", timestamp), usage_bytes).await {
            error!("Failed to anchor resource usage to DAG: {}", e);
        }
    });
    
    Ok(())
}

/// Wrapper for host_anchor_to_dag
fn host_anchor_to_dag_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    ptr: i32,
    len: i32,
) -> Result<i32, Trap> {
    debug!(ptr, len, "host_anchor_to_dag called");
    
    // Read the anchor payload from memory
    let anchor_bytes = match safe_read_bytes(&caller, ptr as u32, len as u32) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Convert bytes to string (assuming JSON or similar text format)
    let anchor_str = match String::from_utf8(anchor_bytes) {
        Ok(s) => s,
        Err(e) => {
            error!("Invalid UTF-8 in anchor payload: {}", e);
            return Ok(map_abi_error_to_wasm(anyhow::anyhow!("Invalid UTF-8 in anchor payload")));
        }
    };
    
    // Record compute cost for this operation
    let env = caller.data();
    if let Err(e) = env.record_compute_usage(100 + (len as u64) / 10) {
        return Ok(map_vm_error_to_wasm(e));
    }
    
    // Call the host environment to anchor the data
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.anchor_metadata_to_dag(&anchor_str)) {
        Ok(_) => Ok(0), // Success
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_mint_token
fn host_mint_token_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    resource_type: i32,
    recipient_ptr: i32,
    recipient_len: i32,
    amount: i32,
) -> Result<i32, Trap> {
    debug!(resource_type, amount, "host_mint_token called");
    
    if amount <= 0 {
        return Err(Trap::throw("Amount must be positive"));
    }
    
    // Convert resource_type integer to ResourceType
    let res_type = match resource_type {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        3 => ResourceType::Token,
        _ => return Err(Trap::throw(format!("Invalid resource type: {}", resource_type))),
    };
    
    // Read recipient DID from memory
    let recipient_did = match read_memory_string(&mut caller, recipient_ptr, recipient_len) {
        Ok(did) => did,
        Err(e) => return Err(Trap::throw(format!("Failed to read recipient DID: {}", e))),
    };
    
    // Get host environment
    let env = caller.data_mut();
    
    // Verify Guardian role - only Guardians can mint tokens
    let caller_scope = env.caller_scope();
    if caller_scope != IdentityScope::Guardian {
        error!("Minting attempted by non-Guardian identity scope: {:?}", caller_scope);
        return Ok(-2); // Not authorized
    }
    
    // Get tokio runtime handle
    let handle = tokio::runtime::Handle::current();
    
    // Call into the economic system to mint tokens
    // This is a simplified version - in a real implementation we'd have a proper token minting system
    let token_result = handle.block_on(env.mint_tokens(res_type, &recipient_did, amount as u64));
    
    match token_result {
        Ok(_) => {
            // Anchor the mint operation to DAG for governance tracking
            let mint_record = serde_json::json!({
                "operation": "mint",
                "resource_type": format!("{}", res_type),
                "recipient": recipient_did,
                "amount": amount,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| Trap::throw(format!("Failed to get timestamp: {}", e)))?
                    .as_secs(),
                "issuer": env.caller_did().to_string()
            });
            
            // Serialize the mint record
            let mint_bytes = match serde_json::to_vec(&mint_record) {
                Ok(bytes) => bytes,
                Err(e) => {
                    // Log the error but consider the mint operation successful
                    error!("Failed to serialize mint record: {}", e);
                    return Ok(1);
                }
            };
            
            // Attempt to anchor the mint operation to DAG
            let dag_key = format!("token_mint:{}:{}", recipient_did, std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| Trap::throw(format!("Failed to get timestamp: {}", e)))?
                .as_secs());
                
            if let Err(e) = handle.block_on(env.anchor_to_dag(&dag_key, mint_bytes)) {
                error!("Failed to anchor mint operation to DAG: {}", e);
                // Still consider the mint operation successful
            }
            
            Ok(1) // Success
        }
        Err(e) => {
            error!("Failed to mint tokens: {}", e);
            Ok(-1) // Error
        }
    }
}

/// Wrapper for host_transfer_resource
fn host_transfer_resource_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    from_ptr: i32,
    from_len: i32,
    to_ptr: i32,
    to_len: i32,
    resource_type: i32,
    amount: i32,
) -> Result<i32, Trap> {
    debug!(resource_type, amount, "host_transfer_resource called");
    
    if amount <= 0 {
        return Err(Trap::throw("Amount must be positive"));
    }
    
    // Convert resource_type integer to ResourceType
    let res_type = match resource_type {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        3 => ResourceType::Token,
        _ => return Err(Trap::throw(format!("Invalid resource type: {}", resource_type))),
    };
    
    // Read from/to DIDs from memory
    let from_did = match read_memory_string(&mut caller, from_ptr, from_len) {
        Ok(did) => did,
        Err(e) => return Err(Trap::throw(format!("Failed to read from DID: {}", e))),
    };
    
    let to_did = match read_memory_string(&mut caller, to_ptr, to_len) {
        Ok(did) => did,
        Err(e) => return Err(Trap::throw(format!("Failed to read to DID: {}", e))),
    };
    
    // Get host environment
    let env = caller.data_mut();
    
    // Get tokio runtime handle
    let handle = tokio::runtime::Handle::current();
    
    // Check if the caller has authority over the 'from' account
    // This is a simplified check - in a real system, we'd have signature verification
    if env.caller_did() != from_did {
        // Additional check for Guardians, who can transfer on behalf of others
        if env.caller_scope() != IdentityScope::Guardian {
            error!("Transfer attempted by unauthorized identity: {} for account {}", 
                env.caller_did(), from_did);
            return Ok(-2); // Not authorized
        }
    }
    
    // Call into the economic system to transfer tokens
    // This is a simplified version - in a real implementation we'd have a proper token transfer system
    let transfer_result = handle.block_on(env.transfer_resources(res_type, &from_did, &to_did, amount as u64));
    
    match transfer_result {
        Ok(_) => {
            // Anchor the transfer operation to DAG for governance tracking
            let transfer_record = serde_json::json!({
                "operation": "transfer",
                "resource_type": format!("{}", res_type),
                "from": from_did,
                "to": to_did,
                "amount": amount,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| Trap::throw(format!("Failed to get timestamp: {}", e)))?
                    .as_secs(),
                "authorized_by": env.caller_did().to_string()
            });
            
            // Serialize the transfer record
            let transfer_bytes = match serde_json::to_vec(&transfer_record) {
                Ok(bytes) => bytes,
                Err(e) => {
                    // Log the error but consider the transfer operation successful
                    error!("Failed to serialize transfer record: {}", e);
                    return Ok(1);
                }
            };
            
            // Attempt to anchor the transfer operation to DAG
            let dag_key = format!("resource_transfer:{}:{}", from_did, std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| Trap::throw(format!("Failed to get timestamp: {}", e)))?
                .as_secs());
                
            if let Err(e) = handle.block_on(env.anchor_to_dag(&dag_key, transfer_bytes)) {
                error!("Failed to anchor transfer operation to DAG: {}", e);
                // Still consider the transfer operation successful
            }
            
            Ok(1) // Success
        }
        Err(e) => {
            error!("Failed to transfer resources: {}", e);
            Ok(-1) // Error
        }
    }
}

/// Wrapper for host_store_node ABI function
fn host_store_dag_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>, 
    ptr: u32, 
    len: u32
) -> Result<i32, Trap> {
    debug!("host_store_dag_node called with ptr: {}, len: {}", ptr, len);
    
    // Read the serialized DagNode from WASM memory
    let node_bytes = match safe_read_bytes(&caller, ptr, len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Record compute cost for this operation
    if let Err(code) = check_compute(&caller, 1000 + (node_bytes.len() as u64) / 10) {
        return Ok(code);
    }
    
    // Deserialize the node
    let node: DagNode = match dag_storage_codec().decode(&node_bytes) {
        Ok(node) => node,
        Err(e) => {
            error!("Failed to deserialize DagNode: {}", e);
            return Ok(-4); // Codec error
        }
    };
    
    // Call the host environment to store the node
    let env = caller.data();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.store_node(node)) {
        Ok(_) => Ok(0), // Success
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_get_node ABI function
fn host_get_dag_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>, 
    cid_ptr: u32, 
    cid_len: u32, 
    result_ptr: u32
) -> Result<i32, Trap> {
    debug!("host_get_dag_node called with cid_ptr: {}, cid_len: {}", cid_ptr, cid_len);
    
    // Read the CID bytes from WASM memory
    let cid_bytes = match safe_read_bytes(&caller, cid_ptr, cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Record compute cost for this operation
    if let Err(code) = check_compute(&caller, 500) {
        return Ok(code);
    }
    
    // Parse the CID
    let cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Call the host environment to get the node
    let env = caller.data();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.get_node(&cid)) {
        Ok(Some(node)) => {
            // Serialize the node
            let node_bytes = match dag_storage_codec().encode(&node) {
                Ok(bytes) => bytes,
                Err(e) => {
                    error!("Failed to serialize DagNode: {}", e);
                    return Ok(-4); // Codec error
                }
            };
            
            // Write the serialized node to WASM memory
            match safe_write_bytes(&mut caller, &node_bytes, result_ptr, node_bytes.len() as u32) {
                Ok(_) => Ok(node_bytes.len() as i32), // Return the number of bytes written
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        },
        Ok(None) => Ok(0), // Node not found
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_contains_node ABI function
fn host_contains_dag_node_wrapper(
    caller: Caller<'_, ConcreteHostEnvironment>, 
    cid_ptr: u32, 
    cid_len: u32
) -> Result<i32, Trap> {
    debug!("host_contains_dag_node called with cid_ptr: {}, cid_len: {}", cid_ptr, cid_len);
    
    // Read the CID bytes from WASM memory
    let cid_bytes = match safe_read_bytes(&caller, cid_ptr, cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Record compute cost for this operation
    if let Err(code) = check_compute(&caller, 200) {
        return Ok(code);
    }
    
    // Parse the CID
    let cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Call the host environment to check if the node exists
    let env = caller.data();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.contains_node(&cid)) {
        Ok(true) => Ok(1), // Node exists
        Ok(false) => Ok(0), // Node doesn't exist
        Err(e) => Ok(map_internal_error_to_wasm(e)),
    }
}

/// Wrapper for host_get_execution_receipts ABI function
fn host_get_execution_receipts_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>, 
    scope_ptr: i32, 
    scope_len: i32,
    timestamp_ptr: i32,
    result_ptr: i32,
    result_max_len: i32
) -> Result<i32, Trap> {
    debug!("host_get_execution_receipts called with scope_ptr: {}, scope_len: {}", scope_ptr, scope_len);
    
    // Read the scope string from WASM memory
    let scope = match safe_read_string(&caller, scope_ptr as u32, scope_len as u32) {
        Ok(s) => s,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Check if we have a timestamp filter
    let timestamp_opt = if timestamp_ptr != 0 {
        match safe_read_bytes(&caller, timestamp_ptr as u32, 8) {
            Ok(bytes) => {
                if bytes.len() == 8 {
                    let timestamp_bytes: [u8; 8] = match bytes.try_into() {
                        Ok(arr) => arr,
                        Err(_) => return Ok(map_abi_error_to_wasm(anyhow!("Failed to convert timestamp bytes"))),
                    };
                    Some(i64::from_le_bytes(timestamp_bytes))
                } else {
                    return Ok(map_abi_error_to_wasm(anyhow!("Invalid timestamp length")));
                }
            },
            Err(e) => return Ok(map_abi_error_to_wasm(e)),
        }
    } else {
        None
    };
    
    // Record compute cost for this operation
    let env = caller.data();
    if let Err(e) = env.record_compute_usage(200 + (scope_len as u64) / 10) {
        return Ok(map_vm_error_to_wasm(e));
    }
    
    // Get the tokio runtime handle
    let handle = tokio::runtime::Handle::current();
    
    // Call the method to get simplified execution receipts
    let result = handle.block_on(
        crate::credentials::get_simplified_execution_receipts(env, &scope, timestamp_opt)
    );
    
    match result {
        Ok(json_string) => {
            // Write the result to WASM memory
            match safe_write_string(&mut caller, &json_string, result_ptr as u32, result_max_len as u32) {
                Ok(len) => Ok(len),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        },
        Err(e) => {
            error!("Failed to get execution receipts: {}", e);
            Ok(-1) // Error code for credential retrieval failure
        }
    }
}

/// Register all host functions
pub fn register_host_functions(
    linker: &mut wasmtime::Linker<ConcreteHostEnvironment>,
) -> Result<(), VmError> {
    // Define host_get_value function
    linker.func_wrap(
        "env", 
        "get_value", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, key_ptr: i32, key_len: i32, value_ptr: i32, value_max_len: i32| -> Result<i32, Error> {
            // Read the key from memory
            let key = safe_read_string(&mut caller, key_ptr, key_len)?;
            debug!("host_get_value: key={}", key);
            
            // Get a reference to the environment
            let env = caller.data_mut();
            
            // Measure operation cost based on key length
            let key_cost = std::cmp::max(1, key_len / 100) as u64;
            env.record_compute_usage(key_cost)
                .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
            
            // Try to get the value
            if let Some(value) = env.get_value(&key) {
                // Measure operation cost for reading/returning value
                let value_cost = std::cmp::max(1, value.len() as i32 / 100) as u64;
                env.record_compute_usage(value_cost)
                    .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
                
                // Convert to string for writing
                let value_str = String::from_utf8_lossy(&value);
                
                // Drop env before writing to memory
                std::mem::drop(env);
                
                // Write to memory
                safe_write_string(&mut caller, &value_str, value_ptr, value_max_len)
            } else {
                // Not found
                Ok(-1)
            }
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register get_value: {}", e)))?;
    
    // Define host_set_value function
    linker.func_wrap(
        "env", 
        "set_value", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32| -> Result<i32, Error> {
            // Read key and value from memory
            let key = safe_read_string(&mut caller, key_ptr, key_len)?;
            let value = safe_read_string(&mut caller, value_ptr, value_len)?;
            debug!("host_set_value: key={}, value_len={}", key, value.len());
            
            // Get a reference to the environment
            let env = caller.data_mut();
            
            // Measure operation cost
            let operation_cost = std::cmp::max(1, (key_len + value_len) / 50) as u64;
            env.record_compute_usage(operation_cost)
                .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
            
            // Record storage usage
            let storage_cost = (key.len() + value.len()) as u64;
            env.record_storage_usage(storage_cost)
                .map_err(|e| Error::msg(format!("Failed to record storage usage: {}", e)))?;
            
            // Set the value
            match env.set_value(&key, value.into_bytes()) {
                Ok(_) => Ok(1), // Success
                Err(e) => {
                    warn!("host_set_value failed: {}", e);
                    Ok(0) // Failure
                }
            }
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register set_value: {}", e)))?;
    
    // Define host_delete_value function
    linker.func_wrap(
        "env", 
        "delete_value", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, key_ptr: i32, key_len: i32| -> Result<i32, Error> {
            // Read key from memory
            let key = safe_read_string(&mut caller, key_ptr, key_len)?;
            debug!("host_delete_value: key={}", key);
            
            // Get a reference to the environment
            let env = caller.data_mut();
            
            // Measure operation cost
            let operation_cost = std::cmp::max(1, key_len / 100) as u64;
            env.record_compute_usage(operation_cost)
                .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
            
            // Delete the value
            match env.delete_value(&key) {
                Ok(_) => Ok(1), // Success
                Err(e) => {
                    warn!("host_delete_value failed: {}", e);
                    Ok(0) // Failure
                }
            }
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register delete_value: {}", e)))?;
    
    // Define host_log function
    linker.func_wrap(
        "env", 
        "host_log", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, message_ptr: i32, message_len: i32| -> Result<i32, Error> {
            let message = safe_read_string(&mut caller, message_ptr, message_len)?;
            let env = caller.data_mut();
            let cost = std::cmp::max(1, message_len / 500) as u64;
            env.record_compute_usage(cost)?;
            debug!("[VM] {}", message);
            Ok(message_len)
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_log: {}", e)))?;
    
    // Define host_get_caller_did function
    linker.func_wrap(
        "env", 
        "host_get_caller_did", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, ptr: i32, max_len: i32| -> Result<i32, Error> {
            let env = caller.data_mut();
            let did = env.caller_did().to_string();
            env.record_compute_usage(10)?;
            drop(env);
            safe_write_string(&mut caller, &did, ptr, max_len)
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_get_caller_did: {}", e)))?;
    
    // Define host_verify_signature function
    linker.func_wrap(
        "env", 
        "verify_signature", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, did_ptr: i32, did_len: i32, 
         message_ptr: i32, message_len: i32, signature_ptr: i32, signature_len: i32| -> Result<i32, Error> {
            // Read inputs from memory
            let did = safe_read_string(&mut caller, did_ptr, did_len)?;
            let message = safe_read_string(&mut caller, message_ptr, message_len)?;
            let signature = safe_read_string(&mut caller, signature_ptr, signature_len)?;
            
            // Get a reference to the environment
            let env = caller.data_mut();
            
            // Measure significant operation cost
            let operation_cost = 1000_u64; // Base cost for signature verification
            env.record_compute_usage(operation_cost)
                .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
            
            // Drop env
            std::mem::drop(env);
            
            // Simple verification (placeholder)
            if did.starts_with("did:icn:") && !signature.is_empty() {
                Ok(1) // Valid signature
            } else {
                Ok(0) // Invalid signature
            }
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register verify_signature: {}", e)))?;
    
    // Define host_create_sub_dag function
    linker.func_wrap(
        "env",
        "host_create_sub_dag",
        host_create_sub_dag_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_create_sub_dag: {}", e)))?;

    // DAG Node Operations
    linker.func_wrap(
        "env",
        "host_store_node",
        host_store_dag_node_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_store_node: {}", e)))?;

    linker.func_wrap(
        "env",
        "host_get_node",
        host_get_dag_node_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_get_node: {}", e)))?;

    linker.func_wrap(
        "env",
        "host_contains_node",
        host_contains_dag_node_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_contains_node: {}", e)))?;

    // Register the enhanced economic/resource functions
    linker.func_wrap(
        "env",
        "host_check_resource_authorization",
        host_check_resource_authorization_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_check_resource_authorization: {}", e)))?;
    
    linker.func_wrap(
        "env",
        "host_record_resource_usage",
        host_record_resource_usage_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_record_resource_usage: {}", e)))?;
    
    // Register the DAG anchoring function
    linker.func_wrap(
        "env",
        "host_anchor_to_dag",
        host_anchor_to_dag_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_anchor_to_dag: {}", e)))?;
    
    // Register token minting function (Guardian-only)
    linker.func_wrap(
        "env",
        "host_mint_token",
        host_mint_token_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_mint_token: {}", e)))?;
    
    // Register resource transfer function
    linker.func_wrap(
        "env",
        "host_transfer_resource",
        host_transfer_resource_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_transfer_resource: {}", e)))?;

    // Register execution receipts function
    linker.func_wrap(
        "env",
        "host_get_execution_receipts",
        host_get_execution_receipts_wrapper,
    ).map_err(|e| VmError::InitializationError(format!("Failed to register host_get_execution_receipts: {}", e)))?;

    Ok(())
} 