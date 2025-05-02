use anyhow::Error as AnyhowError;
use wasmtime::Linker;
use crate::{StoreData, HostEnvironment, LogLevel};
use crate::cid_utils;
use crate::mem_helpers::{read_memory_string, read_memory_bytes, write_memory_bytes, write_memory_u32};
use icn_identity::IdentityScope;

/// Register identity-related host functions
pub fn register_identity_functions(linker: &mut Linker<StoreData>) -> Result<(), AnyhowError> {
    // get_caller_did: Returns the DID of the caller
    linker.func_wrap("env", "host_get_caller_did",
                     |mut caller: wasmtime::Caller<'_, StoreData>,
                      out_ptr: i32, _out_len: i32| -> Result<i32, AnyhowError> {
        let store_data = caller.data();
        // Get caller DID from the context
        let caller_did = store_data.ctx.caller_did.clone();
        
        // Write result to guest memory
        if out_ptr >= 0 {
            write_memory_bytes(&mut caller, out_ptr, caller_did.as_bytes())?;
        }
        
        // Return the length of the string
        Ok(caller_did.len() as i32)
    })?;
    
    // get_caller_scope: Returns the scope of the caller
    linker.func_wrap("env", "host_get_caller_scope", |caller: wasmtime::Caller<'_, StoreData>| -> Result<i32, AnyhowError> {
        let store_data = caller.data();
        
        // Get caller scope from the context, convert to i32
        let scope_i32 = match store_data.ctx.caller_scope {
            icn_identity::IdentityScope::Individual => 0,
            icn_identity::IdentityScope::Cooperative => 1,
            icn_identity::IdentityScope::Community => 2,
            icn_identity::IdentityScope::Federation => 3,
            icn_identity::IdentityScope::Node => 4,
            icn_identity::IdentityScope::Guardian => 5,
        };
        
        Ok(scope_i32)
    })?;
    
    // verify_signature: Verify a signature against a message and DID
    linker.func_wrap("env", "host_verify_signature",
                     |mut caller: wasmtime::Caller<'_, StoreData>,
                      did_ptr: i32, did_len: i32,
                      msg_ptr: i32, msg_len: i32,
                      sig_ptr: i32, sig_len: i32| -> Result<i32, AnyhowError> {
        // Read did, message, and signature from guest memory
        let did = read_memory_string(&mut caller, did_ptr, did_len)?;
        let message = read_memory_bytes(&mut caller, msg_ptr, msg_len)?;
        let signature = read_memory_bytes(&mut caller, sig_ptr, sig_len)?;
        
        // Clone necessary data before releasing the borrow
        let mut host_env = caller.data_mut().host.clone();
        
        // Call the host function to verify the signature
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                host_env.verify_signature(&did, &message, &signature).await
            })
        });
        
        match result {
            Ok(is_valid) => Ok(if is_valid { 1 } else { 0 }),
            Err(e) => {
                // This requires a mutable reference, so we need to re-borrow caller.data_mut()
                // and clone the host to avoid borrowing issues
                let mut host = caller.data_mut().host.clone();
                
                // Log the error
                let error_message = format!("Signature verification failed: {}", e);
                let _ = host.log_message(LogLevel::Error, &error_message);
                
                // Return error code
                Ok(0)
            }
        }
    })?;
    
    Ok(())
} 