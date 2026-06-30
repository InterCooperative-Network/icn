//! Wire-stable appliance image build manifest.
//!
//! `icn-appliance` owns the typed, serializable record describing what an ICN
//! appliance image build produced: the built binaries and their hashes, the
//! image hash, and the honest production-posture flags. It replaces the ad-hoc
//! JSON heredoc emitted by `deploy/appliance/build-image.sh` with a single
//! wire-stable contract that the build path emits and (in a later rung) a
//! `verify` path can re-hash against.
//!
//! This crate is deployment-layer and **kernel-agnostic**: it imports no domain
//! crates and carries no protocol semantics — it only describes a build.

pub mod error;
pub mod hash;
pub mod manifest;

pub use error::ApplianceError;
pub use hash::sha256_hex;
pub use manifest::{ApplianceManifest, BinaryRecord, MANIFEST_VERSION};
