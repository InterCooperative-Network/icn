use anyhow;
use wasmtime::Linker;
use crate::{StoreData, LogLevel, HostEnvironment};
use crate::mem_helpers::read_memory_string;

/// Register logging-related host functions
pub fn register_logging_functions(linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    // log_message: Log a message from the WASM module
    linker.func_wrap("env", "host_log_message", |mut caller: wasmtime::Caller<'_, StoreData>,
                     level: i32, msg_ptr: i32, msg_len: i32| -> Result<(), anyhow::Error> {
        // Convert level integer to LogLevel
        let log_level = match level {
            0 => LogLevel::Debug,
            1 => LogLevel::Info,
            2 => LogLevel::Warn,
            3 => LogLevel::Error,
            _ => LogLevel::Info,
        };
        
        // Read message from guest memory
        let message = read_memory_string(&mut caller, msg_ptr, msg_len)?;
        
        // Create a clone of the message to avoid lifetime issues with the original string
        let message_owned = message.to_string();
        
        // Get mutable access to the store data and call log_message
        let mut store_data = caller.data_mut();
        
        // Call the host function with the cloned message
        match store_data.host.log_message(log_level, &message_owned) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Logging failed: {}", e))
        }
    })?;
    
    Ok(())
} 