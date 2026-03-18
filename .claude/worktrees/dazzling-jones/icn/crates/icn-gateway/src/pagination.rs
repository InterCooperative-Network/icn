//! Pagination re-exported from `icn-http-kit`.
//!
//! All types have moved to the shared `icn-http-kit` crate so that other
//! API app crates (governance, ledger, federation) can use the same
//! pagination primitives without depending on gateway internals.

pub use icn_http_kit::pagination::*;
