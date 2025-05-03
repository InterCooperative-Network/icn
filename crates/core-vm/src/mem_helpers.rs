use anyhow;
use wasmtime::{Memory, AsContextMut, Caller};
use crate::ConcreteHostEnvironment;

/// Get the memory export from a WASM module
pub fn get_memory(caller: &mut Caller<'_, ConcreteHostEnvironment>) -> Result<Memory, anyhow::Error> {
    // In newer wasmtime versions, get_export is now available directly on Caller
    // instead of through as_context_mut()
    caller.get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| anyhow::anyhow!("Failed to find memory export"))
}

/// Read a string from WASM memory
pub fn read_memory_string<'a>(caller: &mut Caller<'a, ConcreteHostEnvironment>, ptr: i32, len: i32) -> Result<String, anyhow::Error> {
    if ptr < 0 || len < 0 {
        return Err(anyhow::anyhow!("Invalid memory parameters"));
    }
    
    let memory = get_memory(caller)?;
    let data = memory.data(caller.as_context_mut());
    
    let start = ptr as usize;
    let end = start + len as usize;
    
    if end > data.len() {
        return Err(anyhow::anyhow!(
            "Memory access out of bounds: offset={}, size={}, mem_size={}",
            start, len, data.len()
        ));
    }
    
    let bytes = &data[start..end];
    String::from_utf8(bytes.to_vec())
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 string: {}", e))
}

/// Read raw bytes from WASM memory
pub fn read_memory_bytes<'a>(caller: &mut Caller<'a, ConcreteHostEnvironment>, ptr: i32, len: i32) -> Result<Vec<u8>, anyhow::Error> {
    if ptr < 0 || len < 0 {
        return Err(anyhow::anyhow!("Invalid memory parameters"));
    }
    
    let memory = get_memory(caller)?;
    let data = memory.data(caller.as_context_mut());
    
    let start = ptr as usize;
    let end = start + len as usize;
    
    if end > data.len() {
        return Err(anyhow::anyhow!(
            "Memory access out of bounds: offset={}, size={}, mem_size={}",
            start, len, data.len()
        ));
    }
    
    Ok(data[start..end].to_vec())
}

/// Write bytes to WASM memory
pub fn write_memory_bytes<'a>(caller: &mut Caller<'a, ConcreteHostEnvironment>, ptr: i32, bytes: &[u8]) -> Result<(), anyhow::Error> {
    if ptr < 0 {
        return Err(anyhow::anyhow!("Invalid memory parameters"));
    }
    
    let memory = get_memory(caller)?;
    let start = ptr as usize;
    
    let mem_size = memory.data_size(caller.as_context_mut());
    if start + bytes.len() > mem_size {
        return Err(anyhow::anyhow!(
            "Memory write out of bounds: offset={}, size={}, mem_size={}",
            start, bytes.len(), mem_size
        ));
    }
    
    memory.write(caller.as_context_mut(), start, bytes)
        .map_err(|e| anyhow::anyhow!("Memory write failed: {}", e))
}

/// Write a u32 value to WASM memory
pub fn write_memory_u32<'a>(caller: &mut Caller<'a, ConcreteHostEnvironment>, ptr: i32, value: u32) -> Result<(), anyhow::Error> {
    if ptr < 0 {
        return Err(anyhow::anyhow!("Invalid memory parameters"));
    }
    
    let bytes = value.to_le_bytes();
    write_memory_bytes(caller, ptr, &bytes)
}

/// Write a u64 value to the guest memory
pub fn write_memory_u64(caller: &mut wasmtime::Caller<'_, ConcreteHostEnvironment>, ptr: i32, value: u64) -> Result<(), anyhow::Error> {
    if ptr < 0 {
        return Err(anyhow::anyhow!("Invalid memory pointer"));
    }
    
    let memory = get_memory(caller)?;
    
    // Write the u64 value as 8 bytes in little-endian order
    memory.write(
        caller, 
        ptr as usize, 
        &value.to_le_bytes(),
    ).map_err(|_| anyhow::anyhow!("Failed to write to guest memory"))?;
    
    Ok(())
}

/// Try to allocate memory in the WASM guest
pub fn try_allocate_guest_memory<'a>(caller: &mut Caller<'a, ConcreteHostEnvironment>, size: i32) -> Result<i32, anyhow::Error> {
    if size < 0 {
        return Err(anyhow::anyhow!("Cannot allocate negative memory size"));
    }
    
    if let Some(alloc) = caller.get_export("alloc") {
        if let Some(alloc_func) = alloc.into_func() {
            if let Ok(alloc_typed) = alloc_func.typed::<i32, i32>(caller.as_context_mut()) {
                return alloc_typed.call(caller.as_context_mut(), size)
                    .map_err(|e| anyhow::anyhow!("Alloc function call failed: {}", e));
            }
        }
    }
    
    Ok(1024)
}

/// Write a string to WASM memory
pub fn write_memory_string<'a>(
    caller: &mut Caller<'a, ConcreteHostEnvironment>,
    memory: Memory,
    value: &str,
    ptr: u32,
    max_len: u32,
) -> Result<u32, anyhow::Error> {
    let bytes = value.as_bytes();
    let write_len = std::cmp::min(bytes.len(), max_len as usize) as u32;
    
    // Write the string data
    memory.write(
        caller.as_context_mut(),
        ptr as usize,
        &bytes[0..write_len as usize],
    )?;
    
    Ok(write_len)
} 