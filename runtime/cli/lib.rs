/*!
# ICN Runtime CLI Library

This crate provides the command-line interface for the ICN Runtime, as well as
reusable functionality for other crates in the workspace.
*/

// Re-export the derive_authorizations function for use in tests
pub use crate::covm::derive_authorizations;

// Include the modules
pub mod covm; 