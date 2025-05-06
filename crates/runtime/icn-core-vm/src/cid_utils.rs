use cid::Cid;
use crate::VmError;
use std::convert::TryFrom;

/// Convert a CID to bytes for WASM ABI
pub fn cid_to_wasm_bytes(cid: &Cid) -> Vec<u8> {
    cid.to_bytes()
}

/// Convert bytes from WASM ABI to CID
pub fn cid_from_wasm_bytes(bytes: &[u8]) -> Result<Cid, VmError> {
    Cid::try_from(bytes)
        .map_err(|e| VmError::MemoryError(format!("Invalid CID bytes: {}", e)))
}

/// Convert a CID to a string for WASM ABI
pub fn cid_to_wasm_string(cid: &Cid) -> String {
    cid.to_string()
}

/// Convert a string from WASM ABI to CID
pub fn cid_from_wasm_string(s: &str) -> Result<Cid, VmError> {
    Cid::try_from(s)
        .map_err(|e| VmError::MemoryError(format!("Invalid CID string: {}", e)))
}

/// Helper to read a CID from WASM memory
pub fn read_cid_from_wasm_memory(
    caller: &mut wasmtime::Caller<'_, crate::ConcreteHostEnvironment>,
    cid_ptr: i32,
    cid_len: i32,
) -> Result<Cid, VmError> {
    if cid_ptr < 0 || cid_len <= 0 {
        return Err(VmError::MemoryError("Invalid CID pointer or length".to_string()));
    }
    
    // Read CID string from memory
    let cid_str = crate::mem_helpers::read_memory_string(caller, cid_ptr, cid_len)
        .map_err(|e| VmError::MemoryError(format!("Failed to read CID string: {}", e)))?;
    
    // Convert to CID
    cid_from_wasm_string(&cid_str)
}

/// Helper to write a CID to WASM memory
pub fn write_cid_to_wasm_memory(
    caller: &mut wasmtime::Caller<'_, crate::ConcreteHostEnvironment>,
    cid: &Cid,
    out_ptr: i32,
    out_len_ptr: i32,
) -> Result<(), VmError> {
    if out_ptr < 0 {
        return Err(VmError::MemoryError("Invalid output pointer".to_string()));
    }
    
    // Convert CID to string
    let cid_str = cid_to_wasm_string(cid);
    let cid_bytes = cid_str.as_bytes();
    
    // Write to memory
    if out_ptr >= 0 {
        crate::mem_helpers::write_memory_bytes(caller, out_ptr, cid_bytes)
            .map_err(|e| VmError::MemoryError(format!("Failed to write CID to memory: {}", e)))?;
    }
    
    // Write length if needed
    if out_len_ptr >= 0 {
        crate::mem_helpers::write_memory_u32(caller, out_len_ptr, cid_bytes.len() as u32)
            .map_err(|e| VmError::MemoryError(format!("Failed to write CID length: {}", e)))?;
    }
    
    Ok(())
}

/// Convert between core-vm CID and storage CID format
pub fn convert_to_storage_cid(cid: &Cid) -> Result<Cid, String> {
    // For consistency, ensure we have a standard conversion
    Ok(cid.clone())
}

/// Convert from storage CID to core-vm CID
pub fn convert_from_storage_cid(storage_cid: &Cid) -> Result<Cid, String> {
    // For consistency, ensure we have a standard conversion
    Ok(storage_cid.clone())
} 