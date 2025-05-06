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
    InternalHostError
};
use crate::mem_helpers::{read_memory_string, write_memory_string, read_memory_bytes, safe_check_bounds};
use wasmtime::{Caller, Linker, Memory, Trap, WasmBacktrace};
use tracing::*;
use anyhow::{anyhow, Error};
use icn_models::storage::StorageError;
use icn_identity::IdentityScope;
use cid::Cid;
use std::convert::TryInto;
use std::io::Cursor;
use icn_models::{dag_storage_codec, DagNode, DagNodeMetadata, DagNodeBuilder, IcnIdentityId};
use libipld::codec::{Decode, Encode};
use libipld::Ipld;

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
        InternalHostError::VmError(_) => -7,
        InternalHostError::Other(_) => -99, // Generic internal error
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
fn check_compute(caller: &mut Caller<'_, ConcreteHostEnvironment>, cost: u64) -> Result<(), i32> {
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
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
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
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
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
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
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

/// Modified Trap constructor to handle different versions of wasmtime
pub fn create_trap<S: ToString>(message: S) -> Trap {
    // Always use Trap::new regardless of feature flags
    // This is the modern API method
    Trap::new(message.to_string())
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
    
    // Read parent_did from memory
    let parent_did = match safe_read_string(&mut caller, parent_did_ptr, parent_did_len) {
        Ok(s) => s,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Read genesis_payload from memory
    let genesis_payload = match safe_read_bytes(&mut caller, genesis_payload_ptr, genesis_payload_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Read entity_type from memory
    let entity_type = match safe_read_string(&mut caller, entity_type_ptr, entity_type_len) {
        Ok(s) => s,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Resource check
    let estimated_compute_cost = 7000_u64;
    let estimated_storage_cost = genesis_payload.len() as u64 + 512;
    if let Err(code) = check_compute(&mut caller, estimated_compute_cost) { return Ok(code); }
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
        let entity_did = safe_read_string(&mut caller, entity_did_ptr, entity_did_len)?;
        cost += entity_did_len as u64 * 2;
        let payload_bytes = safe_read_bytes(&mut caller, payload_ptr, payload_len)?;
        cost += payload_len as u64;
        let signature_bytes = safe_read_bytes(&mut caller, signature_ptr, signature_len)?;
        cost += signature_len as u64;
        let metadata_bytes = safe_read_bytes(&mut caller, metadata_ptr, metadata_len)?;
        cost += metadata_len as u64;

        let (parent_cids_bytes, read_cost) = read_vec_of_bytes(
            &mut caller,
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
    if let Err(code) = check_compute(&mut caller, cost) { return Ok(code); }

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.store_dag_node(
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
        let entity_did = safe_read_string(&mut caller, entity_did_ptr, entity_did_len)?;
        cost += entity_did_len as u64;
        let cid_bytes = safe_read_bytes(&mut caller, cid_ptr, cid_len)?;
        cost += cid_len as u64;
        Ok((entity_did, cid_bytes))
    }();

     let (entity_did, cid_bytes) = match result {
        Ok(data) => data,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Preliminary resource check
    if let Err(code) = check_compute(&mut caller, cost) { return Ok(code); }

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.get_dag_node(&entity_did, cid_bytes));

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
        let entity_did = safe_read_string(&mut caller, entity_did_ptr, entity_did_len)?;
        cost += entity_did_len as u64;
        let cid_bytes = safe_read_bytes(&mut caller, cid_ptr, cid_len)?;
        cost += cid_len as u64;
        Ok((entity_did, cid_bytes))
    }();

     let (entity_did, cid_bytes) = match result {
        Ok(data) => data,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };

    // Preliminary resource check
    if let Err(code) = check_compute(&mut caller, cost) { return Ok(code); }

    // Call async logic
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    let result = handle.block_on(env.contains_dag_node(&entity_did, cid_bytes));

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
    
    // Get host environment
    let env = caller.data();
    
    // Check if the caller has authorization for this resource usage
    let authorized = env.vm_context.resource_authorizations().iter()
        .find(|auth| auth.resource_type == res_type)
        .map(|auth| auth.limit >= amount as u64)
        .unwrap_or(false);
    
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
    let env = caller.data_mut();
    
    // Record the usage
    if let Err(e) = env.record_resource_consumption(res_type, amount as u64) {
        return Err(create_trap(format!("Resource usage recording failed: {}", e)));
    }
    
    // Also anchor the usage to DAG for governance tracking
    // Get current timestamp
    let timestamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(e) => return Err(create_trap(format!("Failed to get timestamp: {}", e))),
    };
    
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
    let anchor_bytes = match safe_read_bytes(&mut caller, ptr as u32, len as u32) {
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
        return Err(create_trap("Amount must be positive"));
    }
    
    // Convert resource_type integer to ResourceType
    let res_type = match resource_type {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        3 => ResourceType::Token,
        _ => return Err(create_trap(format!("Invalid resource type: {}", resource_type))),
    };
    
    // Read recipient DID from memory
    let recipient_did = match read_memory_string(&mut caller, recipient_ptr, recipient_len) {
        Ok(did) => did,
        Err(e) => return Err(create_trap(format!("Failed to read recipient DID: {}", e))),
    };
    
    // Get host environment
    let env = caller.data_mut();
    
    // Verify Administrator role - only Administrators can mint tokens
    let caller_scope = env.caller_scope();
    if caller_scope != IdentityScope::Administrator {
        error!("Minting attempted by non-Administrator identity scope: {:?}", caller_scope);
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
                "resource_type": format!("{:?}", res_type),
                "recipient": recipient_did,
                "amount": amount,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| Trap::new(format!("Failed to get timestamp: {}", e)))?
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
                .map_err(|e| Trap::new(format!("Failed to get timestamp: {}", e)))?
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
        return Err(create_trap("Amount must be positive"));
    }
    
    // Convert resource_type integer to ResourceType
    let res_type = match resource_type {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        3 => ResourceType::Token,
        _ => return Err(create_trap(format!("Invalid resource type: {}", resource_type))),
    };
    
    // Read from/to DIDs from memory
    let from_did = match read_memory_string(&mut caller, from_ptr, from_len) {
        Ok(did) => did,
        Err(e) => return Err(create_trap(format!("Failed to read from DID: {}", e))),
    };
    
    let to_did = match read_memory_string(&mut caller, to_ptr, to_len) {
        Ok(did) => did,
        Err(e) => return Err(create_trap(format!("Failed to read to DID: {}", e))),
    };
    
    // Get host environment
    let env = caller.data_mut();
    
    // Additional check for Administrators, who can transfer on behalf of others
    if env.caller_scope() != IdentityScope::Administrator {
        error!("Transfer attempted by unauthorized identity: {} for account {}", 
            env.caller_did(), from_did);
        return Ok(-2); // Not authorized
    }
    
    // Call into the economic system to transfer tokens
    // This is a simplified version - in a real implementation we'd have a proper token transfer system
    let handle = tokio::runtime::Handle::current();
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
                    .map_err(|e| Trap::new(format!("Failed to get timestamp: {}", e)))?
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
                .map_err(|e| Trap::new(format!("Failed to get timestamp: {}", e)))?
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

/// Wrapper for host_store_dag_node_wrapper
fn host_store_dag_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>, 
    ptr: u32, 
    len: u32
) -> Result<i32, Trap> {
    debug!("host_store_dag_node called with ptr: {}, len: {}", ptr, len);
    
    // Read the serialized DagNode from WASM memory
    let node_bytes = match safe_read_bytes(&mut caller, ptr, len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Record compute cost for this operation
    if let Err(code) = check_compute(&mut caller, 1000 + (node_bytes.len() as u64) / 10) {
        return Ok(code);
    }
    
    // Deserialize the node
    let codec = dag_storage_codec();
    let node: DagNode = match codec.decode(&node_bytes) {
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

/// Wrapper for host_get_dag_node_wrapper
fn host_get_dag_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>, 
    cid_ptr: u32, 
    cid_len: u32, 
    result_ptr: u32
) -> Result<i32, Trap> {
    debug!("host_get_dag_node called with cid_ptr: {}, cid_len: {}", cid_ptr, cid_len);
    
    // Read the CID bytes from WASM memory
    let cid_bytes = match safe_read_bytes(&mut caller, cid_ptr, cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Record compute cost for this operation
    if let Err(code) = check_compute(&mut caller, 500) {
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
            let codec = dag_storage_codec();
            let node_bytes = match codec.encode(&node) {
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

/// Wrapper for host_contains_dag_node_wrapper
fn host_contains_dag_node_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>, 
    cid_ptr: u32, 
    cid_len: u32
) -> Result<i32, Trap> {
    debug!("host_contains_dag_node called with cid_ptr: {}, cid_len: {}", cid_ptr, cid_len);
    
    // Read the CID bytes from WASM memory
    let cid_bytes = match safe_read_bytes(&mut caller, cid_ptr, cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Record compute cost for this operation
    if let Err(code) = check_compute(&mut caller, 200) {
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
    let scope = match safe_read_string(&mut caller, scope_ptr as u32, scope_len as u32) {
        Ok(s) => s,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Check if we have a timestamp filter
    let timestamp_opt = if timestamp_ptr != 0 {
        match safe_read_bytes(&mut caller, timestamp_ptr as u32, 8) {
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

/// Wrapper for host_lock_tokens
fn host_lock_tokens_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    escrow_cid_ptr: u32, escrow_cid_len: u32,
    amount: u64,
) -> Result<i32, Trap> {
    debug!(escrow_cid_ptr, amount, "host_lock_tokens called");
    
    // Read the escrow CID from memory
    let cid_bytes = match safe_read_bytes(&mut caller, escrow_cid_ptr, escrow_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the CID
    let escrow_cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 500) {
        return Ok(code);
    }
    
    // Get the host environment and call the lock function
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.lock_tokens(&escrow_cid, amount)) {
        Ok(_) => Ok(0), // Success
        Err(e) => {
            error!("Failed to lock tokens: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_release_tokens
fn host_release_tokens_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    escrow_cid_ptr: u32, escrow_cid_len: u32,
    worker_did_ptr: u32, worker_did_len: u32,
    amount: u64,
) -> Result<i32, Trap> {
    debug!(escrow_cid_ptr, worker_did_ptr, amount, "host_release_tokens called");
    
    // Read the escrow CID from memory
    let cid_bytes = match safe_read_bytes(&mut caller, escrow_cid_ptr, escrow_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the CID
    let escrow_cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Read the worker DID from memory
    let worker_did = match safe_read_string(&mut caller, worker_did_ptr, worker_did_len) {
        Ok(did) => did,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 700) {
        return Ok(code);
    }
    
    // Get the host environment and call the release function
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.release_tokens(&escrow_cid, &worker_did, amount)) {
        Ok(_) => Ok(0), // Success
        Err(e) => {
            error!("Failed to release tokens: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_refund_tokens
fn host_refund_tokens_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    escrow_cid_ptr: u32, escrow_cid_len: u32,
) -> Result<i32, Trap> {
    debug!(escrow_cid_ptr, "host_refund_tokens called");
    
    // Read the escrow CID from memory
    let cid_bytes = match safe_read_bytes(&mut caller, escrow_cid_ptr, escrow_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the CID
    let escrow_cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 500) {
        return Ok(code);
    }
    
    // Get the host environment and call the refund function
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.refund_tokens(&escrow_cid)) {
        Ok(_) => Ok(0), // Success
        Err(e) => {
            error!("Failed to refund tokens: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_get_active_mesh_policy_cid
fn host_get_active_mesh_policy_cid_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    federation_did_ptr: u32, federation_did_len: u32,
    cid_out_ptr: u32, cid_out_max_len: u32,
) -> Result<i32, Trap> {
    debug!("host_get_active_mesh_policy_cid called");
    
    // Read the federation DID from memory
    let federation_did = match safe_read_string(&mut caller, federation_did_ptr, federation_did_len) {
        Ok(did) => did,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 500) {
        return Ok(code);
    }
    
    // Get the host environment and call the function
    let env = caller.data();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.get_active_mesh_policy_cid(&federation_did)) {
        Ok(Some(cid)) => {
            let cid_bytes = cid.to_bytes();
            match safe_write_bytes(&mut caller, &cid_bytes, cid_out_ptr, cid_out_max_len) {
                Ok(len) => Ok(len as i32),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        },
        Ok(None) => Ok(0), // No active policy CID found
        Err(e) => {
            error!("Failed to get active mesh policy CID: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_load_mesh_policy
fn host_load_mesh_policy_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    policy_cid_ptr: u32, policy_cid_len: u32,
    policy_out_ptr: u32, policy_out_max_len: u32,
) -> Result<i32, Trap> {
    debug!("host_load_mesh_policy called");
    
    // Read the policy CID from memory
    let cid_bytes = match safe_read_bytes(&mut caller, policy_cid_ptr, policy_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the CID
    let policy_cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 800) {
        return Ok(code);
    }
    
    // Get the host environment and call the function
    let env = caller.data();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.load_mesh_policy(&policy_cid)) {
        Ok(Some(policy_json)) => {
            match safe_write_string(&mut caller, &policy_json, policy_out_ptr, policy_out_max_len) {
                Ok(len) => Ok(len),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        },
        Ok(None) => Ok(0), // Policy not found
        Err(e) => {
            error!("Failed to load mesh policy: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_update_mesh_policy
fn host_update_mesh_policy_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    previous_cid_ptr: u32, previous_cid_len: u32,
    fragment_ptr: u32, fragment_len: u32,
    federation_did_ptr: u32, federation_did_len: u32,
    cid_out_ptr: u32, cid_out_max_len: u32,
) -> Result<i32, Trap> {
    debug!("host_update_mesh_policy called");
    
    // Read inputs from memory
    let previous_cid_bytes = match safe_read_bytes(&mut caller, previous_cid_ptr, previous_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    let fragment_json = match safe_read_string(&mut caller, fragment_ptr, fragment_len) {
        Ok(json) => json,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    let federation_did = match safe_read_string(&mut caller, federation_did_ptr, federation_did_len) {
        Ok(did) => did,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the previous CID
    let previous_cid = match Cid::read_bytes(Cursor::new(previous_cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse previous CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 1500) {
        return Ok(code);
    }
    
    // Get the host environment and call the function
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.update_mesh_policy(&previous_cid, &fragment_json, &federation_did)) {
        Ok(new_cid) => {
            let cid_bytes = new_cid.to_bytes();
            match safe_write_bytes(&mut caller, &cid_bytes, cid_out_ptr, cid_out_max_len) {
                Ok(len) => Ok(len as i32),
                Err(e) => Ok(map_abi_error_to_wasm(e)),
            }
        },
        Err(e) => {
            error!("Failed to update mesh policy: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_activate_mesh_policy
fn host_activate_mesh_policy_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    policy_cid_ptr: u32, policy_cid_len: u32,
) -> Result<i32, Trap> {
    debug!("host_activate_mesh_policy called");
    
    // Read the policy CID from memory
    let cid_bytes = match safe_read_bytes(&mut caller, policy_cid_ptr, policy_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the CID
    let policy_cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 1000) {
        return Ok(code);
    }
    
    // Get the host environment and call the function
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.activate_mesh_policy(&policy_cid)) {
        Ok(_) => Ok(1), // Success
        Err(e) => {
            error!("Failed to activate mesh policy: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Wrapper for host_record_policy_vote
fn host_record_policy_vote_wrapper(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    voter_did_ptr: u32, voter_did_len: u32,
    policy_cid_ptr: u32, policy_cid_len: u32,
    approve: i32,
) -> Result<i32, Trap> {
    debug!("host_record_policy_vote called");
    
    // Read inputs from memory
    let voter_did = match safe_read_string(&mut caller, voter_did_ptr, voter_did_len) {
        Ok(did) => did,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    let cid_bytes = match safe_read_bytes(&mut caller, policy_cid_ptr, policy_cid_len) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(map_abi_error_to_wasm(e)),
    };
    
    // Parse the CID
    let policy_cid = match Cid::read_bytes(Cursor::new(cid_bytes)) {
        Ok(cid) => cid,
        Err(e) => {
            error!("Failed to parse CID: {}", e);
            return Ok(-5); // Invalid input
        }
    };
    
    // Convert approval integer to boolean
    let approved = approve != 0;
    
    // Resource usage check
    if let Err(code) = check_compute(&mut caller, 700) {
        return Ok(code);
    }
    
    // Get the host environment and call the function
    let env = caller.data_mut();
    let handle = tokio::runtime::Handle::current();
    
    match handle.block_on(env.record_policy_vote(&voter_did, &policy_cid, approved)) {
        Ok(_) => Ok(1), // Success
        Err(e) => {
            error!("Failed to record policy vote: {}", e);
            Ok(map_internal_error_to_wasm(e))
        }
    }
}

/// Creates a Linker with all the registered host functions for the ConcreteHostEnvironment
pub fn create_import_object(store: &mut wasmtime::Store<ConcreteHostEnvironment>) -> wasmtime::Linker<ConcreteHostEnvironment> {
    let mut linker = wasmtime::Linker::new(store.engine());
    
    // Register all host functions
    register_host_functions(&mut linker).expect("Failed to register host functions");
    
    linker
}

/// Register all host functions
pub fn register_host_functions(
    linker: &mut wasmtime::Linker<ConcreteHostEnvironment>,
) -> Result<(), VmError> {
    // Define host_get_value function
    linker.func_wrap(
        "env", 
        "get_value", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, key_ptr: i32, key_len: i32, value_ptr: i32, value_max_len: i32| -> Result<i32, Trap> {
            // Read the key from memory
            let key = match safe_read_string(&mut caller, key_ptr as u32, key_len as u32) {
                Ok(k) => k,
                Err(e) => return Ok(map_abi_error_to_wasm(e)),
            };
            debug!("host_get_value: key={}", key);
            
            // Get a reference to the environment
            let env = caller.data();
            
            // Look up the value in the environment
            match env.get_value(&key) {
                Some(value) => {
                    // Write the value back to WASM memory
                    match safe_write_bytes(&mut caller, &value, value_ptr as u32, value_max_len as u32) {
                        Ok(bytes_written) => Ok(bytes_written as i32),
                        Err(e) => Ok(map_abi_error_to_wasm(e)),
                    }
                }
                None => {
                    Ok(0) // Key not found, return 0 bytes written
                }
            }
        }
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register get_value: {}", e)))?;
    
    // Define host_set_value function
    linker.func_wrap(
        "env", 
        "set_value", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32| -> Result<i32, Trap> {
            // Read the key and value from memory
            let key = match safe_read_string(&mut caller, key_ptr as u32, key_len as u32) {
                Ok(k) => k,
                Err(e) => return Ok(map_abi_error_to_wasm(e)),
            };
            
            let value = match safe_read_bytes(&mut caller, value_ptr as u32, value_len as u32) {
                Ok(v) => v,
                Err(e) => return Ok(map_abi_error_to_wasm(e)),
            };
            
            debug!("host_set_value: key={}, value_len={}", key, value.len());
            
            // Check compute resource allowance
            let estimated_cost = (key.len() + value.len()) as u64;
            if let Err(code) = check_compute(&mut caller, estimated_cost) {
                return Ok(code);
            }
            
            // Get a mutable reference to the environment
            let env = caller.data_mut();
            
            // Set the value in the environment
            match env.set_value(&key, value) {
                Ok(_) => Ok(1), // Success, return 1
                Err(internal_err) => Ok(map_internal_error_to_wasm(internal_err)),
            }
        }
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register set_value: {}", e)))?;
    
    // Register other host functions similarly
    // host_delete_value
    linker.func_wrap(
        "env", 
        "delete_value", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, key_ptr: i32, key_len: i32| -> Result<i32, Trap> {
            let key = match safe_read_string(&mut caller, key_ptr as u32, key_len as u32) {
                Ok(k) => k,
                Err(e) => return Ok(map_abi_error_to_wasm(e)),
            };
            
            debug!("host_delete_value: key={}", key);
            
            // Check compute resource allowance
            let estimated_cost = key.len() as u64;
            if let Err(code) = check_compute(&mut caller, estimated_cost) {
                return Ok(code);
            }
            
            let env = caller.data_mut();
            
            match env.delete_value(&key) {
                Ok(_) => Ok(1), // Success, return 1
                Err(internal_err) => Ok(map_internal_error_to_wasm(internal_err)),
            }
        }
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register delete_value: {}", e)))?;

    // DAG operations
    linker.func_wrap(
        "env", 
        "create_sub_dag", 
        host_create_sub_dag_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register create_sub_dag: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "store_dag_node", 
        host_store_node_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register store_dag_node: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "get_dag_node", 
        host_get_node_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register get_dag_node: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "contains_dag_node", 
        host_contains_node_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register contains_dag_node: {}", e)))?;
    
    // Resource management
    linker.func_wrap(
        "env", 
        "check_resource_authorization", 
        host_check_resource_authorization_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register check_resource_authorization: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "record_resource_usage", 
        host_record_resource_usage_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register record_resource_usage: {}", e)))?;
    
    // Anchoring
    linker.func_wrap(
        "env", 
        "anchor_to_dag", 
        host_anchor_to_dag_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register anchor_to_dag: {}", e)))?;
    
    // Token management
    linker.func_wrap(
        "env", 
        "mint_token", 
        host_mint_token_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register mint_token: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "transfer_resource", 
        host_transfer_resource_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register transfer_resource: {}", e)))?;
    
    // Mesh escrow functions
    linker.func_wrap(
        "env", 
        "host_lock_tokens", 
        host_lock_tokens_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_lock_tokens: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "host_release_tokens", 
        host_release_tokens_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_release_tokens: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "host_refund_tokens", 
        host_refund_tokens_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_refund_tokens: {}", e)))?;
    
    // Mesh policy governance functions
    linker.func_wrap(
        "env", 
        "host_get_active_mesh_policy_cid", 
        host_get_active_mesh_policy_cid_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_get_active_mesh_policy_cid: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "host_load_mesh_policy", 
        host_load_mesh_policy_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_load_mesh_policy: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "host_update_mesh_policy", 
        host_update_mesh_policy_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_update_mesh_policy: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "host_activate_mesh_policy", 
        host_activate_mesh_policy_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_activate_mesh_policy: {}", e)))?;
    
    linker.func_wrap(
        "env", 
        "host_record_policy_vote", 
        host_record_policy_vote_wrapper
    ).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_record_policy_vote: {}", e)))?;
    
    // Add economics helpers
    crate::economics_helpers::register_economics_functions(linker)
        .map_err(|e| VmError::EngineCreationFailed(format!("Failed to register economics functions: {}", e)))?;
    
    Ok(())
} 