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
use crate::mem_helpers::{read_memory_string, write_memory_string};
use wasmtime::{Caller, Memory, Trap, WasmBacktrace};
use tracing::*;
use anyhow::Error;

/// Maximum allowed length for a key or value in bytes
const MAX_STRING_LENGTH: usize = 1024 * 1024; // 1 MB

/// Maximum allowed size for a memory allocation
const MAX_ALLOCATION_SIZE: u32 = 1024 * 1024 * 10; // 10 MB

/// Safely read a string from WebAssembly memory with bounds checking
fn safe_read_string(
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
    ptr: i32,
    len: i32,
) -> Result<String, Error> {
    // Validate pointer and length
    if ptr < 0 {
        return Err(Error::msg(format!("Negative pointer: {}", ptr)));
    }
    
    if len < 0 {
        return Err(Error::msg(format!("Negative length: {}", len)));
    }
    
    if len as usize > MAX_STRING_LENGTH {
        return Err(Error::msg(format!(
            "String too long: {} (max {})",
            len,
            MAX_STRING_LENGTH
        )));
    }
    
    // Get memory from caller
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| Error::msg("Failed to find memory export"))?;
    
    // Check if the range is within bounds
    let memory_size = memory.data_size(&mut *caller);
    if (ptr as u64 + len as u64) > memory_size as u64 {
        return Err(Error::msg(format!(
            "Memory access out of bounds: ptr={}, len={}, memory_size={}",
            ptr, len, memory_size
        )));
    }
    
    // Read the string safely
    read_memory_string(caller, ptr, len)
}

/// Safely write a string to WebAssembly memory with bounds checking
fn safe_write_string(
    caller: &mut Caller<'_, ConcreteHostEnvironment>,
    value: &str,
    ptr: i32,
    max_len: i32,
) -> Result<i32, Error> {
    // Validate pointer and length
    if ptr < 0 {
        return Err(Error::msg(format!("Negative pointer: {}", ptr)));
    }
    
    if max_len < 0 {
        return Err(Error::msg(format!("Negative max length: {}", max_len)));
    }
    
    if max_len as usize > MAX_STRING_LENGTH {
        return Err(Error::msg(format!(
            "Max length too large: {} (max {})",
            max_len,
            MAX_STRING_LENGTH
        )));
    }
    
    // Get memory from caller
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| Error::msg("Failed to find memory export"))?;
    
    // Check if the range is within bounds
    let memory_size = memory.data_size(&mut *caller);
    if (ptr as u64 + max_len as u64) > memory_size as u64 {
        return Err(Error::msg(format!(
            "Memory access out of bounds: ptr={}, max_len={}, memory_size={}",
            ptr, max_len, memory_size
        )));
    }
    
    // Write the string safely
    let written = write_memory_string(caller, memory, value, ptr as u32, max_len as u32)?;
    Ok(written as i32)
}

/// Register all host functions with a wasmtime::Linker
pub fn register_host_functions(
    _store: &mut wasmtime::Store<ConcreteHostEnvironment>,
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
        "log", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, message_ptr: i32, message_len: i32| -> Result<i32, Error> {
            // Read message from memory
            let message = safe_read_string(&mut caller, message_ptr, message_len)?;
            
            // Get a reference to the environment
            let env = caller.data_mut();
            
            // Measure operation cost
            let operation_cost = std::cmp::max(1, message_len / 500) as u64;
            env.record_compute_usage(operation_cost)
                .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
            
            // Log the message
            match env.log(&message) {
                Ok(_) => Ok(message_len), // Return message length on success
                Err(e) => {
                    warn!("host_log failed: {}", e);
                    Ok(0) // Failure
                }
            }
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register log: {}", e)))?;
    
    // Define host_get_caller_did function
    linker.func_wrap(
        "env", 
        "get_caller_did", 
        |mut caller: Caller<'_, ConcreteHostEnvironment>, ptr: i32, max_len: i32| -> Result<i32, Error> {
            // Get a reference to the environment
            let env = caller.data_mut();
            
            // Get the caller DID
            let did = env.caller_did().to_string();
            
            // Measure minimal operation cost
            env.record_compute_usage(1)
                .map_err(|e| Error::msg(format!("Failed to record compute usage: {}", e)))?;
            
            // Drop env before writing to memory
            std::mem::drop(env);
            
            // Write to memory
            safe_write_string(&mut caller, &did, ptr, max_len)
        }
    ).map_err(|e| VmError::InitializationError(format!("Failed to register get_caller_did: {}", e)))?;
    
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
    
    Ok(())
} 