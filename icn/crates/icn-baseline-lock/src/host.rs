//! Paranoid wasmtime host: zero imports, fuel, hostile output validation.

use icn_boundary::{ExecutionInputEnvelopeV1, ExecutionOutputEnvelopeV1, Hash, ResultKind};
use icn_encoding::{decode, encode};
use thiserror::Error;
use wasmtime::{Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const MAX_MEMORY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("module hash mismatch")]
    ModuleHashMismatch,
    #[error("wasm compile: {0}")]
    Compile(String),
    #[error("instantiate: {0}")]
    Instantiate(String),
    #[error("missing export {0}")]
    MissingExport(&'static str),
    #[error("guest alloc failed")]
    GuestAllocFailed,
    #[error("guest evaluate failed")]
    GuestEvaluateFailed,
    #[error("guest output length {0} exceeds max {1}")]
    OutputTooLarge(usize, usize),
    #[error("deserialize output: {0}")]
    BadOutput(String),
    #[error("hostile output: {0}")]
    Hostile(&'static str),
    #[error("wasm trap: {0}")]
    Trap(String),
}

struct HostState {
    limits: StoreLimits,
}

impl HostState {
    fn new() -> Self {
        Self {
            limits: StoreLimitsBuilder::new()
                .memory_size(MAX_MEMORY_BYTES)
                .instances(2)
                .memories(2)
                .build(),
        }
    }
}

/// Run guest `evaluate` with full hostile validation. Returns output and fuel consumed (measured).
pub fn evaluate_guest(
    wasm_bytes: &[u8],
    expected_module_hash: &Hash,
    input: &ExecutionInputEnvelopeV1,
    max_output_bytes: usize,
) -> Result<(ExecutionOutputEnvelopeV1, u64), HostError> {
    let digest = *blake3::hash(wasm_bytes).as_bytes();
    if digest != expected_module_hash.0 {
        return Err(HostError::ModuleHashMismatch);
    }

    let mut cfg = wasmtime::Config::new();
    cfg.consume_fuel(true);
    let engine = Engine::new(&cfg).map_err(|e| HostError::Compile(e.to_string()))?;
    let module =
        Module::from_binary(&engine, wasm_bytes).map_err(|e| HostError::Compile(e.to_string()))?;

    let linker: Linker<HostState> = Linker::new(&engine);
    let mut store = Store::new(&engine, HostState::new());
    store.limiter(|s| &mut s.limits);
    store
        .set_fuel(input.fuel_limit)
        .map_err(|e| HostError::Compile(e.to_string()))?;

    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| HostError::Instantiate(e.to_string()))?;

    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or(HostError::MissingExport("memory"))?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, "alloc")
        .map_err(|_| HostError::MissingExport("alloc"))?;
    let evaluate = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, "evaluate")
        .map_err(|_| HostError::MissingExport("evaluate"))?;

    let input_bytes = encode(input).map_err(|e| HostError::Compile(e.to_string()))?;
    let in_len = input_bytes.len() as i32;
    let in_ptr = alloc
        .call(&mut store, in_len)
        .map_err(|e| HostError::Trap(e.to_string()))?;
    if in_ptr == 0 {
        return Err(HostError::GuestAllocFailed);
    }
    memory
        .write(&mut store, in_ptr as usize, &input_bytes)
        .map_err(|e| HostError::Trap(e.to_string()))?;

    let packed = evaluate
        .call(&mut store, (in_ptr, in_len))
        .map_err(|e| HostError::Trap(e.to_string()))?;
    if packed == 0 {
        return Err(HostError::GuestEvaluateFailed);
    }

    let p = ((packed as u64) >> 32) as u32 as usize;
    let len = (packed as u64) as u32 as usize;
    if len > max_output_bytes {
        return Err(HostError::OutputTooLarge(len, max_output_bytes));
    }

    let mut out_bytes = vec![0u8; len];
    memory
        .read(&mut store, p, &mut out_bytes)
        .map_err(|e| HostError::Trap(e.to_string()))?;

    let mut out: ExecutionOutputEnvelopeV1 =
        decode(&out_bytes).map_err(|e| HostError::BadOutput(e.to_string()))?;

    let remaining = store
        .get_fuel()
        .map_err(|e| HostError::Compile(e.to_string()))?;
    let fuel_consumed = input.fuel_limit.saturating_sub(remaining);
    out.consumed_fuel = fuel_consumed;

    validate_hostile_output(input, &out, fuel_consumed, max_output_bytes, len)?;

    Ok((out, fuel_consumed))
}

/// Public for integration tests (injected bad outputs).
pub fn validate_hostile_output(
    input: &ExecutionInputEnvelopeV1,
    out: &ExecutionOutputEnvelopeV1,
    fuel_measured: u64,
    max_output_bytes: usize,
    output_byte_len: usize,
) -> Result<(), HostError> {
    if out.abi_version != input.abi_version {
        return Err(HostError::Hostile("abi_version"));
    }
    if out.schema_id != input.expected_output_schema {
        return Err(HostError::Hostile("schema_id"));
    }
    if out.workload_id != input.workload_id {
        return Err(HostError::Hostile("workload_id"));
    }
    if out.process_id != input.process_id {
        return Err(HostError::Hostile("process_id"));
    }
    if out.module_hash.0 != input.module_hash.0 {
        return Err(HostError::Hostile("module_hash"));
    }
    if out.result_kind != ResultKind::GateValidation {
        return Err(HostError::Hostile("result_kind"));
    }
    if out.proposed_transition_ref.is_some() {
        return Err(HostError::Hostile("proposed_transition_ref"));
    }
    if fuel_measured > input.fuel_limit {
        return Err(HostError::Hostile("fuel"));
    }
    if output_byte_len > max_output_bytes {
        return Err(HostError::Hostile("output_len"));
    }
    Ok(())
}
