//! Governance proposal translation boundary.
//!
//! The governance app owns domain payloads and translates them into
//! kernel-safe `KernelEffect`s. Execution happens in kernel via the effect path.

mod execution;

pub use execution::translate_payload_to_effects;
