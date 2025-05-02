use anyhow;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment};
use crate::mem_helpers::{read_memory_string, read_memory_bytes, write_memory_bytes};
use icn_identity::IdentityScope;
use futures::executor::block_on;

/// Register identity-related host functions
pub fn register_identity_functions(linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    // get_caller_did: Get the DID of the caller
    linker.func_wrap("env", "host_get_caller_did", |mut caller: wasmtime::Caller<'_, StoreData>,
                     out_ptr: i32, out_len: i32| -> Result<i32, anyhow::Error> {
        // Call the host function
        let did = caller.data().host.get_caller_did()
            .map_err(|e| anyhow::anyhow!("Failed to get caller DID: {}", e))?;
        
        // Write the DID to guest memory
        if out_ptr >= 0 {
            write_memory_bytes(&mut caller, out_ptr, did.as_bytes())?;
        }
        
        // Return the length of the DID string
        Ok(did.len() as i32)
    })?;
    
    // get_caller_scope: Get the scope of the caller
    linker.func_wrap("env", "host_get_caller_scope", |mut caller: wasmtime::Caller<'_, StoreData>| -> Result<i32, anyhow::Error> {
        // Call the host function - need mutable caller for context access
        let scope = caller.data().host.get_caller_scope()
            .map_err(|e| anyhow::anyhow!("Failed to get caller scope: {}", e))?;
        
        // Convert scope to integer representation
        let scope_int = match scope {
            IdentityScope::Individual => 0,
            IdentityScope::Cooperative => 1,
            IdentityScope::Community => 2,
            IdentityScope::Federation => 3,
            IdentityScope::Node => 4,
            IdentityScope::Guardian => 5,
        };
        
        Ok(scope_int)
    })?;
    
    // verify_signature: Verify a signature
    linker.func_wrap("env", "host_verify_signature", |mut caller: wasmtime::Caller<'_, StoreData>,
                     did_ptr: i32, did_len: i32, msg_ptr: i32, msg_len: i32, sig_ptr: i32, sig_len: i32| -> Result<i32, anyhow::Error> {
        // Read parameters from guest memory
        let did_str = read_memory_string(&mut caller, did_ptr, did_len)?;
        let message = read_memory_bytes(&mut caller, msg_ptr, msg_len)?;
        let signature = read_memory_bytes(&mut caller, sig_ptr, sig_len)?;
        
        // Call the host function
        let verify_result = {
            // Execute the async function in a blocking context
            block_on(async {
                // Clone needed parts to avoid borrowing issues
                let did_str = did_str.clone();
                let message = message.clone();
                let signature = signature.clone();
                let host_env = caller.data().host.clone();
                
                host_env.verify_signature(&did_str, &message, &signature).await
            }).map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?
        };
        
        // Return 1 for valid, 0 for invalid
        Ok(if verify_result { 1 } else { 0 })
    })?;
    
    Ok(())
} 