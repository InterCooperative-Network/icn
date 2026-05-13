//! Stupid WASM gate validator: bounded facts only, no WASI, no imports.
//! `#![no_std]` + `alloc` on `wasm32-unknown-unknown` (this crate is excluded from
//! the root workspace so serde/postcard defaults are not unified with the main tree).
#![no_std]
#![no_main]
#![allow(static_mut_refs)]
#![allow(clippy::missing_safety_doc)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use core::slice;

use icn_boundary::{
    CanonicalFacts, ExecutionInputEnvelopeV1, ExecutionOutputEnvelopeV1, ResultKind,
};

const ARENA_SIZE: usize = 96 * 1024;
static mut ARENA: [u8; ARENA_SIZE] = [0u8; ARENA_SIZE];
static mut BUMP: usize = 0;

struct BumpAlloc;

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match bump_raw(layout.size(), layout.align()) {
            Some(p) => p.as_ptr(),
            None => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Leak-only bump arena; guest is short-lived under wasmtime fuel.
    }
}

#[global_allocator]
static GLOBAL: BumpAlloc = BumpAlloc;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

unsafe fn bump_raw(size: usize, align: usize) -> Option<NonNull<u8>> {
    if size == 0 || align == 0 || !align.is_power_of_two() {
        return None;
    }
    let base = ARENA.as_ptr() as usize;
    let off = (BUMP + align - 1) & !(align - 1);
    let end = off.checked_add(size)?;
    if end > ARENA_SIZE {
        return None;
    }
    BUMP = end;
    NonNull::new((base + off) as *mut u8)
}

/// # Safety
/// Single-threaded guest; host must not invoke reentrantly across threads.
#[no_mangle]
pub unsafe extern "C" fn alloc(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    match bump_raw(n as usize, 16) {
        Some(p) => p.as_ptr() as usize as i32,
        None => 0,
    }
}

/// # Safety
/// `input_ptr`/`input_len` must address valid bytes in guest linear memory.
#[no_mangle]
pub unsafe extern "C" fn evaluate(input_ptr: i32, input_len: i32) -> i64 {
    if input_ptr <= 0 || input_len <= 0 {
        return 0;
    }
    let input = slice::from_raw_parts(input_ptr as *const u8, input_len as usize);
    let env: ExecutionInputEnvelopeV1 = match postcard::from_bytes(input) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let passed = validate_facts(&env.canonical_facts);
    let mut diagnostics = Vec::new();
    if !passed {
        diagnostics.push(String::from("canonical_facts_failed"));
    }
    let out = ExecutionOutputEnvelopeV1 {
        abi_version: env.abi_version,
        schema_id: env.expected_output_schema.clone(),
        workload_id: env.workload_id.clone(),
        process_id: env.process_id.clone(),
        module_hash: env.module_hash,
        result_kind: ResultKind::GateValidation,
        passed,
        output_artifact_refs: Vec::new(),
        diagnostics,
        proposed_transition_ref: None,
        consumed_fuel: 0,
    };

    let bytes = match postcard::to_allocvec(&out) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    let len = bytes.len() as i32;
    let ptr = alloc(len);
    if ptr == 0 {
        return 0;
    }
    let dst = slice::from_raw_parts_mut(ptr as *mut u8, len as usize);
    dst.copy_from_slice(&bytes);
    pack_ptr_len(ptr as u32, len as u32)
}

fn validate_facts(f: &CanonicalFacts) -> bool {
    f.approvals >= f.required_approvals
        && f.notice_delivered
        && f.allocation_requested <= f.allocation_limit
        && f.reservation_not_consumed
}

fn pack_ptr_len(ptr: u32, len: u32) -> i64 {
    ((ptr as u64) << 32 | (len as u64)) as i64
}
