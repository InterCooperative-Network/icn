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
    mem_helpers::{read_memory_string, write_memory_string}
};
use wasmtime::{Caller, Memory};
use std::convert::TryFrom;
use tracing::*;

/// Maximum allowed length for a key or value in bytes
const MAX_STRING_LENGTH: usize = 1024 * 1024; // 1 MB

/// Maximum allowed size for a memory allocation
const MAX_ALLOCATION_SIZE: u32 = 1024 * 1024 * 10; // 10 MB

/// Safely read a string from WebAssembly memory with bounds checking
fn safe_read_string(
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
    ptr: i32,
    len: i32,
) -> Result<String, VmError> {
    // Validate pointer and length
    if ptr < 0 {
        return Err(VmError::MemoryError(format!("Negative pointer: {}", ptr)));
    }
    
    if len < 0 {
        return Err(VmError::MemoryError(format!("Negative length: {}", len)));
    }
    
    if len as usize > MAX_STRING_LENGTH {
        return Err(VmError::MemoryError(format!(
            "String too long: {} (max {})",
            len,
            MAX_STRING_LENGTH
        )));
    }
    
    // Get memory from caller
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| VmError::MemoryError("Failed to find memory export".to_string()))?;
    
    // Check if the range is within bounds
    let memory_size = memory.data_size(caller);
    if (ptr as u64 + len as u64) > memory_size as u64 {
        return Err(VmError::MemoryError(format!(
            "Memory access out of bounds: ptr={}, len={}, memory_size={}",
            ptr, len, memory_size
        )));
    }
    
    // Read the string safely
    match read_memory_string(caller, memory, ptr as u32, len as u32) {
        Ok(s) => Ok(s),
        Err(e) => Err(VmError::MemoryError(format!("Failed to read string: {}", e))),
    }
}

/// Safely write a string to WebAssembly memory with bounds checking
fn safe_write_string(
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
    value: &str,
    ptr: i32,
    max_len: i32,
) -> Result<i32, VmError> {
    // Validate pointer and length
    if ptr < 0 {
        return Err(VmError::MemoryError(format!("Negative pointer: {}", ptr)));
    }
    
    if max_len < 0 {
        return Err(VmError::MemoryError(format!("Negative max length: {}", max_len)));
    }
    
    if max_len as usize > MAX_STRING_LENGTH {
        return Err(VmError::MemoryError(format!(
            "Max length too large: {} (max {})",
            max_len,
            MAX_STRING_LENGTH
        )));
    }
    
    // Get memory from caller
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| VmError::MemoryError("Failed to find memory export".to_string()))?;
    
    // Check if the range is within bounds
    let memory_size = memory.data_size(caller);
    if (ptr as u64 + max_len as u64) > memory_size as u64 {
        return Err(VmError::MemoryError(format!(
            "Memory access out of bounds: ptr={}, max_len={}, memory_size={}",
            ptr, max_len, memory_size
        )));
    }
    
    // Write the string safely
    match write_memory_string(caller, memory, value, ptr as u32, max_len as u32) {
        Ok(written) => Ok(written as i32),
        Err(e) => Err(VmError::MemoryError(format!("Failed to write string: {}", e))),
    }
}

/// Host function to get a value from host storage
pub fn host_get_value(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    key_ptr: i32,
    key_len: i32,
    value_ptr: i32,
    value_max_len: i32,
) -> Result<i32, VmError> {
    // Access the host environment
    let env = caller.data_mut();
    
    // Measure operation cost based on key length - we charge for reading the key
    let key_cost = std::cmp::max(1, key_len / 100) as u64;
    env.record_compute_usage(key_cost)?;
    
    // Safely read the key
    let key = safe_read_string(&mut caller, key_ptr, key_len)?;
    debug!("host_get_value: key={}", key);
    
    // Try to get the value
    if let Some(value) = env.get_value(&key) {
        // Measure operation cost based on value length - we charge for reading and returning the value
        let value_cost = std::cmp::max(1, value.len() as i32 / 100) as u64;
        env.record_compute_usage(value_cost)?;
        
        // Convert to string for writing
        let value_str = String::from_utf8_lossy(&value);
        
        // Write to memory
        safe_write_string(&mut caller, &value_str, value_ptr, value_max_len)
    } else {
        // Not found
        Ok(-1)
    }
}

/// Host function to set a value in host storage
pub fn host_set_value(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    key_ptr: i32,
    key_len: i32,
    value_ptr: i32,
    value_len: i32,
) -> Result<i32, VmError> {
    // Access the host environment
    let env = caller.data_mut();
    
    // Safely read the key
    let key = safe_read_string(&mut caller, key_ptr, key_len)?;
    
    // Safely read the value
    let value = safe_read_string(&mut caller, value_ptr, value_len)?;
    debug!("host_set_value: key={}, value_len={}", key, value.len());
    
    // Measure operation cost based on key+value length
    let operation_cost = std::cmp::max(1, (key_len + value_len) / 50) as u64;
    env.record_compute_usage(operation_cost)?;
    
    // Record storage usage based on total size
    let storage_cost = (key.len() + value.len()) as u64;
    env.record_storage_usage(storage_cost)?;
    
    // Set the value
    match env.set_value(&key, value.into_bytes()) {
        Ok(_) => Ok(1), // Success
        Err(e) => {
            warn!("host_set_value failed: {}", e);
            Ok(0) // Failure
        }
    }
}

/// Host function to delete a value from host storage
pub fn host_delete_value(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    key_ptr: i32,
    key_len: i32,
) -> Result<i32, VmError> {
    // Access the host environment
    let env = caller.data_mut();
    
    // Safely read the key
    let key = safe_read_string(&mut caller, key_ptr, key_len)?;
    debug!("host_delete_value: key={}", key);
    
    // Measure operation cost
    let operation_cost = std::cmp::max(1, key_len / 100) as u64;
    env.record_compute_usage(operation_cost)?;
    
    // Delete the value
    match env.delete_value(&key) {
        Ok(_) => Ok(1), // Success
        Err(e) => {
            warn!("host_delete_value failed: {}", e);
            Ok(0) // Failure
        }
    }
}

/// Host function to log a message
pub fn host_log(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    message_ptr: i32,
    message_len: i32,
) -> Result<i32, VmError> {
    // Access the host environment
    let env = caller.data_mut();
    
    // Safely read the message
    let message = safe_read_string(&mut caller, message_ptr, message_len)?;
    
    // Measure operation cost
    let operation_cost = std::cmp::max(1, message_len / 500) as u64;
    env.record_compute_usage(operation_cost)?;
    
    // Log the message
    match env.log(&message) {
        Ok(_) => Ok(message_len), // Return message length on success
        Err(e) => {
            warn!("host_log failed: {}", e);
            Ok(0) // Failure
        }
    }
}

/// Host function to get the caller's DID
pub fn host_get_caller_did(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    ptr: i32,
    max_len: i32,
) -> Result<i32, VmError> {
    // Access the host environment
    let env = caller.data_mut();
    
    // Get the caller DID
    let did = env.caller_did();
    
    // Minimal compute usage for this operation
    env.record_compute_usage(1)?;
    
    // Write to memory
    safe_write_string(&mut caller, did, ptr, max_len)
}

/// Host function to verify a signature
pub fn host_verify_signature(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    did_ptr: i32,
    did_len: i32,
    message_ptr: i32,
    message_len: i32,
    signature_ptr: i32,
    signature_len: i32,
) -> Result<i32, VmError> {
    // Access the host environment
    let env = caller.data_mut();
    
    // Safely read inputs
    let did = safe_read_string(&mut caller, did_ptr, did_len)?;
    let message = safe_read_string(&mut caller, message_ptr, message_len)?;
    let signature = safe_read_string(&mut caller, signature_ptr, signature_len)?;
    
    // Measure significant operation cost - signature verification is expensive
    let operation_cost = 1000_u64; // Base cost
    env.record_compute_usage(operation_cost)?;
    
    // In real implementation, this would verify using proper crypto libraries
    // For testing, we'll just make a simple check
    if did.starts_with("did:icn:") && !signature.is_empty() {
        Ok(1) // Valid signature
    } else {
        Ok(0) // Invalid signature
    }
}

/// Register all host functions with a wasmtime::Store
pub fn register_host_functions(
    store: &mut wasmtime::Store<ConcreteHostEnvironment>,
    linker: &mut wasmtime::Linker<ConcreteHostEnvironment>,
) -> Result<(), VmError> {
    // Define host functions
    linker.func_wrap("env", "get_value", host_get_value)?;
    linker.func_wrap("env", "set_value", host_set_value)?;
    linker.func_wrap("env", "delete_value", host_delete_value)?;
    linker.func_wrap("env", "log", host_log)?;
    linker.func_wrap("env", "get_caller_did", host_get_caller_did)?;
    linker.func_wrap("env", "verify_signature", host_verify_signature)?;
    
    Ok(())
} 