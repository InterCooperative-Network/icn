#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use icn_core_vm::{
    ConcreteHostEnvironment, ResourceType, ResourceAuthorization, VMContext, IdentityContext
};
use icn_identity::{IdentityId, KeyPair};
use std::sync::Arc;
use rand::Rng;

// Implement a fuzzable input for resource authorization checks
#[derive(Arbitrary, Debug)]
struct ResourceAuthInput {
    // Resource type (0-3 maps to Compute, Storage, Network, Token)
    resource_type: u8,
    
    // Resource amount to check
    amount: u32,
    
    // Authorization limit (how much is authorized)
    auth_limit: u32,
    
    // Already consumed amount
    consumed: u32,
    
    // Whether this resource should be authorized at all
    should_authorize: bool,
}

// Setup a test environment for executing the host function
fn setup_test_env(input: &ResourceAuthInput) -> ConcreteHostEnvironment {
    // Create test identity
    let mut rng = rand::thread_rng();
    let private_key = (0..32).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();
    let public_key = (0..32).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();
    let keypair = KeyPair::new(private_key, public_key);
    let caller_id = IdentityId::new("did:icn:fuzz:caller");
    
    // Determine resource type from input
    let resource_type = match input.resource_type % 4 {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        _ => ResourceType::Token,
    };
    
    // Create authorizations
    let mut authorizations = Vec::new();
    if input.should_authorize {
        authorizations.push(ResourceAuthorization {
            resource_type,
            limit: input.auth_limit as u64,
        });
    }
    
    // Create VM context
    let identity_context = Arc::new(IdentityContext::new(
        keypair,
        caller_id.to_string(),
    ));
    
    let vm_context = VMContext::new(
        identity_context.clone(),
        authorizations,
    );
    
    // Create host environment with pre-consumed resources
    let mut env = ConcreteHostEnvironment::new(vm_context);
    
    // Set consumed amount
    if input.consumed > 0 {
        let _ = env.record_resource_usage(resource_type, input.consumed as u64);
    }
    
    env
}

fuzz_target!(|input: ResourceAuthInput| {
    // Make sure fuzzer doesn't generate enormous values that could make test slow
    if input.amount > 1_000_000_000 || input.auth_limit > 1_000_000_000 || input.consumed > 1_000_000_000 {
        return;
    }
    
    // Set up environment
    let env = setup_test_env(&input);
    
    // Determine expected resource type
    let resource_type = match input.resource_type % 4 {
        0 => ResourceType::Compute,
        1 => ResourceType::Storage,
        2 => ResourceType::Network,
        _ => ResourceType::Token,
    };
    
    // Call function being fuzzed
    let result = env.check_resource_authorization(resource_type, input.amount as u64);
    
    // Verify behavior - resource should be authorized if:
    // 1. It's in the authorization list AND
    // 2. consumed + amount <= limit
    let is_authorized = result.unwrap_or(false);
    
    // Expected authorization status
    let should_be_authorized = input.should_authorize && 
        (input.consumed as u64 + input.amount as u64 <= input.auth_limit as u64);
        
    // If there's a mismatch, this could indicate a bug
    assert_eq!(is_authorized, should_be_authorized, 
               "Authorization mismatch: got {}, expected {}. Input: {:?}", 
               is_authorized, should_be_authorized, input);
}); 