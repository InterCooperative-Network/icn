//! STARK proof generation and verification using winterfell
//!
//! This module provides the actual cryptographic implementation for ZK proofs
//! using the STARK proving system via the winterfell library.
//!
//! # Architecture
//!
//! Each proof type has:
//! - An AIR (Algebraic Intermediate Representation) defining constraints
//! - A Prover that generates proofs
//! - Integration with the high-level Circuit trait
//!
//! # Security
//!
//! These are real cryptographic proofs with ~96-bit security level.

#[cfg(feature = "stark")]
pub mod age;

#[cfg(feature = "stark")]
pub mod citizenship;

#[cfg(feature = "stark")]
pub mod membership;

#[cfg(feature = "stark")]
pub mod non_revocation;

#[cfg(feature = "stark")]
pub mod common;

#[cfg(feature = "stark")]
pub use common::*;
