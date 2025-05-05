# ICN Mesh Compute Implementation Guide

This document serves as a bridge between the philosophical vision outlined in [ICN_COMPUTE_COMMONS.md](./ICN_COMPUTE_COMMONS.md) and its concrete implementation in code.

## Core Components

### 1. From Fuel to Purpose-Bound Execution

The system has moved from a "fuel" metaphor to a purpose-bound execution model, implemented through:

```rust
// In mesh-types/src/lib.rs
/// Capability scope defines the resources required for task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityScope {
    /// Required memory in MB
    pub mem_mb: u32,
    
    /// Required CPU cycles (relative measure)
    pub cpu_cycles: u32,
    
    /// Required GPU operations (in FLOPS)
    pub gpu_flops: u32,
    
    /// Required I/O bandwidth (in MB)
    pub io_mb: u32,
}

impl CapabilityScope {
    /// Check if these capabilities fit within the provided hardware capabilities
    pub fn fits(&self, hw_caps: &HwCaps) -> bool {
        self.mem_mb <= hw_caps.mem_mb &&
        self.cpu_cycles <= hw_caps.cpu_cycles &&
        self.gpu_flops <= hw_caps.gpu_flops &&
        self.io_mb <= hw_caps.io_mb
    }
}
```

This allows tasks to explicitly declare their resource needs rather than simply consuming an abstract "fuel" measure.

### 2. MeshPolicy Governance

Federation-based democratic control of mesh parameters is implemented through:

```rust
// In mesh-types/src/lib.rs
/// Apply a policy fragment update to this policy
pub fn apply_update(&mut self, fragment: &MeshPolicyFragment) -> Result<(), String> {
    // Increment policy version
    self.policy_version += 1;
    
    // Apply reputation parameters if present
    if let Some(params) = &fragment.reputation_params {
        if let Some(alpha) = params.alpha {
            self.alpha = alpha;
        }
        // ... more parameter updates
    }
    
    // ... apply other policy sections (reward settings, bonding, etc.)
    
    // Update timestamp
    self.activation_timestamp = Utc::now();
    
    Ok(())
}
```

The governance process is defined in the `mesh-policy-update.ccl` contract:

```
// In policies/mesh-policy-update.ccl
contract mesh_policy_update(cid previous_policy_cid, MeshPolicy updated_policy_fragment, federation_did) {
    // Verify the proposal is from a valid federation member
    verify federation_membership(federation_did, context.caller_did);
    
    // Verify the previous policy CID matches the currently active policy
    verify previous_policy_matches_active(previous_policy_cid, federation_did);
    
    // Apply the update to create a new policy version
    action apply_update {
        // Host call merges the fragment with the current policy and activates it
        host_update_mesh_policy(previous_policy_cid, updated_policy_fragment, federation_did)
    }
    
    // Allow members to vote on the policy update
    action vote(bool approve) {
        // Record the vote in the governance system
        host_record_policy_vote(context.caller_did, approve)
    }
    
    // Execute the policy update if quorum is reached
    action execute_if_approved {
        // Check if approval quorum is reached
        verify policy_update_approved();
        
        // Activate the new policy
        host_activate_mesh_policy(previous_policy_cid, updated_policy_fragment)
    }
    
    // Cancel the proposal if it's rejected or expired
    action cancel {
        // Verify the proposal is eligible for cancellation
        verify can_cancel_proposal();
        
        // Cancel the proposal
        host_cancel_policy_update()
    }
}
```

### 3. Host ABI Functions for Policy and Escrow

The integration with the ICN runtime is facilitated through host ABI functions:

```rust
// In icn-core-vm/src/host_abi.rs

// Mesh escrow functions
linker.func_wrap(
    "env", 
    "host_lock_tokens", 
    host_lock_tokens_wrapper
).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_lock_tokens: {}", e)))?;

linker.func_wrap(
    "env", 
    "host_release_tokens", 
    host_release_tokens_wrapper
).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_release_tokens: {}", e)))?;

linker.func_wrap(
    "env", 
    "host_refund_tokens", 
    host_refund_tokens_wrapper
).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_refund_tokens: {}", e)))?;

// Mesh policy governance functions
linker.func_wrap(
    "env", 
    "host_get_active_mesh_policy_cid", 
    host_get_active_mesh_policy_cid_wrapper
).map_err(|e| VmError::EngineCreationFailed(format!("Failed to register host_get_active_mesh_policy_cid: {}", e)))?;

// ...additional policy management functions
```

### 4. DAG Event System

Events in the mesh system are anchored to the DAG through defined event types:

```rust
// In icn-dag/src/events.rs
/// Mesh escrow event
MeshEscrowEvent {
    /// The content ID of the escrow contract
    escrow_cid: Cid,
    /// Event type (created, claimed, refunded)
    event_type: String,
    /// Worker DID (if applicable)
    worker_did: Option<String>,
    /// Amount of tokens involved
    amount: Option<u64>,
    /// Timestamp
    timestamp: DateTime<Utc>,
},

/// Mesh policy activated event
MeshPolicyActivated {
    /// Content ID of the activated policy
    policy_cid: Cid,
    /// Federation DID
    federation_did: String,
    /// Policy version
    policy_version: u32,
    /// Proposer DID
    proposer_did: String,
    /// Previous policy CID (if any)
    previous_policy_cid: Option<Cid>,
    /// Timestamp
    timestamp: DateTime<Utc>,
},

/// Mesh policy vote event
MeshPolicyVote {
    /// Content ID of the policy proposal
    policy_cid: Cid,
    /// Federation DID
    federation_did: String,
    /// Voter DID
    voter_did: String,
    /// Approval status
    approved: bool,
    /// Timestamp
    timestamp: DateTime<Utc>,
},
```

### 5. CLI Interface

The wallet CLI provides a user-friendly interface for interacting with the mesh system:

```rust
// In icn-wallet-agent/src/commands/mesh.rs
/// Policy subcommands
#[derive(Debug, Subcommand)]
pub enum PolicySubcommand {
    /// View the current active policy for a federation
    View {
        /// Federation DID (defaults to the wallet's federation)
        #[clap(long)]
        federation_did: Option<String>,
    },
    
    /// Propose a policy update
    Propose {
        /// Federation DID (defaults to the wallet's federation)
        #[clap(long)]
        federation_did: Option<String>,
        
        /// JSON file containing the policy update fragment
        #[clap(long)]
        update_file: PathBuf,
        
        /// Description of the update
        #[clap(long)]
        description: String,
    },
    
    /// List policy proposals
    List {
        /// Federation DID (defaults to the wallet's federation)
        #[clap(long)]
        federation_did: Option<String>,
        
        /// Show all proposals, including inactive ones
        #[clap(long)]
        all: bool,
    },
    
    /// Vote on a policy proposal
    Vote {
        /// Policy CID to vote on
        #[clap(long)]
        policy_cid: String,
        
        /// Vote approval (yes/no)
        #[clap(long)]
        approve: bool,
    },
}
```

## Implementation Testing

The mesh policy governance flow is tested in integration tests:

```rust
// In icn-runtime-root/tests/mesh_policy_governance.rs
#[tokio::test]
async fn test_mesh_policy_governance() -> Result<()> {
    // ... test setup

    // Create a fragment for policy update (increase worker rewards)
    let fragment = mesh_types::MeshPolicyFragment {
        reputation_params: None,
        stake_weight: None,
        min_fee: None,
        base_capability_scope: None,
        reward_settings: Some(mesh_types::RewardSettingsFragment {
            worker_percentage: Some(80),
            verifier_percentage: Some(15),
            platform_fee_percentage: Some(5),
            use_reputation_weighting: None,
            platform_fee_address: None,
        }),
        bonding_requirements: None,
        scheduling_params: None,
        verification_quorum: None,
        description: "Increase worker rewards".to_string(),
        proposer_did: "did:icn:member:1".to_string(),
    };
    
    // Create proposal
    let fragment_json = serde_json::to_string(&fragment)?;
    let proposal_cid = governance.create_proposal(
        federation_did,
        &fragment.proposer_did,
        initial_policy_cid,
        &fragment_json,
    )?;
    
    // Submit votes
    governance.vote_on_proposal(&proposal_cid, "did:icn:member:1", true)?;
    governance.vote_on_proposal(&proposal_cid, "did:icn:member:2", true)?;
    governance.vote_on_proposal(&proposal_cid, "did:icn:member:3", true)?;
    governance.vote_on_proposal(&proposal_cid, "did:icn:member:4", false)?;
    
    // Verify proposal approved and applied
    // ... rest of test
}
```

## Future Development Tasks

1. **Inter-Federation Mesh**
   - Implement cross-federation task dispatch
   - Create token exchange contracts between federation DAGs
   - Develop reputation attestation mechanism

2. **Hardware Capability Expansion**
   - Add support for specialized hardware (ML accelerators, FPGA, etc.)
   - Implement energy-usage tracking for green weighting

3. **Enhanced Verification**
   - Implement ZK-proof verification paths
   - Add support for trusted execution environments (TEE)

4. **Economic Refinement**
   - Develop more sophisticated reward models based on quality metrics
   - Implement reputation-weighted distribution

5. **User Experience**
   - Graphical interfaces for policy management
   - Task monitoring dashboards
   - Federation performance analytics

## Get Involved

Join us in building the ICN Compute Commons:

1. Explore the codebase at [github.com/intercoop-network/icn](https://github.com/intercoop-network/icn)
2. Join our discussion forum at [forum.intercoop.network](https://forum.intercoop.network)
3. Set up a local federation for testing using our quickstart guide
4. Contribute to our open issues or propose new features 