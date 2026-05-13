//! Deterministic boundary types shared by the Rust host and WASM guest.
//!
//! Governance-grade payloads use postcard only (no JSON). No floats, no
//! unordered maps in these structs.
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;

mod types;
pub use types::*;
