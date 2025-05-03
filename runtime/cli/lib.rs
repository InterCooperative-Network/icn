/*!
# ICN Runtime CLI Library

This library exports functions from the ICN Runtime CLI for testing purposes.
*/

/// Re-export CLI functions for testing
mod covm;

pub use covm::{
    handle_execute_command, 
    sign_node_data, 
    create_identity_context, 
    derive_core_vm_authorizations,
};

/// Export version info
pub const VERSION: &str = env!("CARGO_PKG_VERSION"); 