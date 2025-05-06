# ICN Developer Guide

This comprehensive guide is designed to help new developers get started with the ICN (Internet Cooperation Network) system quickly. It covers setup, architecture, key concepts, and provides practical examples for common development tasks.

## Quick Start

### Prerequisites

- **Rust:** 1.70+ with `rustc` and `cargo`
- **Node.js:** 18+ (for CLI tools)
- **Docker:** 20.10+ (for development environment)
- **Git:** 2.20+

### Setup Instructions

1. **Clone the Repository**

```bash
git clone https://github.com/intercoop-network/icn.git
cd icn
```

2. **Install Development Tools**

```bash
# Install Rust components
rustup target add wasm32-unknown-unknown
rustup component add clippy rustfmt

# Install cargo tools
cargo install cargo-audit cargo-watch
```

3. **Start Development Environment**

```bash
# Start the devnet environment
./start_icn_devnet.sh

# In a separate terminal, run the integration tests to verify setup
./icn-runtime-root/run_integration_tests.sh
```

4. **Set Up Your Wallet**

```bash
# Build and set up a development wallet
cd icn-wallet-root
cargo build
./setup_dev_wallet.sh
```

## System Architecture

The ICN system consists of four primary components:

### Runtime System (CoVM v3)

The Runtime is the execution engine for governance operations:

- **WebAssembly (WASM) Execution** - Sandboxed environment for executing governance code
- **DAG State Management** - Directed Acyclic Graph for representing state changes
- **Federation Logic** - Rules for federation operation and governance
- **CCL Compiler** - Transforms Cooperative Coordination Language to WASM

### Wallet System

The Wallet manages user identity and federation participation:

- **Identity Management** - Creation and control of DIDs
- **Credential Storage** - Secure storage of verifiable credentials
- **DAG Synchronization** - Keeping local state in sync with federation
- **Governance Participation** - Creating and voting on proposals

### AgoraNet

AgoraNet provides the deliberation layer and external API:

- **REST API** - External interface for applications
- **Deliberation Threads** - Discussion and decision-making forums
- **Federation Communication** - Protocol for inter-federation messaging
- **User Interface** - Web dashboard for governance participation

### Mesh Compute

Mesh provides distributed computation resources:

- **Task Distribution** - Publishing computation tasks to the network
- **Worker Selection** - Choosing appropriate nodes for execution
- **Verified Execution** - Ensuring correctness of computation
- **Reward Distribution** - Compensating workers for resources

## Core Concepts

### 1. Federation Model

Federation is the fundamental organizational unit in ICN:

```
┌───────────────────────────────────────────────────────┐
│                     Federation                        │
│                                                       │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐           │
│  │ Member  │    │ Member  │    │ Member  │    ...    │
│  └─────────┘    └─────────┘    └─────────┘           │
│                                                       │
│  ┌───────────────────────────────────────────────┐   │
│  │                                               │   │
│  │              Governance Rules                 │   │
│  │                                               │   │
│  └───────────────────────────────────────────────┘   │
│                                                       │
└───────────────────────────────────────────────────────┘
```

- **Federation Bootstrap** - Process of initializing a new federation
- **Membership** - Rules for joining and leaving federations
- **Federation Lifecycle** - Merging, splitting, and evolving federations
- **Cross-Federation Communication** - Interaction between distinct federations

### 2. DAG (Directed Acyclic Graph)

The DAG is the foundation for state representation and trust:

```
                            ┌─────────────┐
                            │  TrustBundle │
                            │    Node     │
                            └──────┬──────┘
                                   │
                                   ▼
┌─────────────┐            ┌─────────────┐            ┌─────────────┐
│   Proposal  │            │   Anchor    │            │  Federation  │
│    Node     │◄───────────┤    Node     │────────────►   Config    │
└──────┬──────┘            └──────┬──────┘            └─────────────┘
       │                          │
       │                          │
┌──────▼──────┐            ┌──────▼──────┐
│    Vote     │            │    Vote     │
│    Node     │            │    Node     │
└──────┬──────┘            └──────┬──────┘
       │                          │
       │                          │
┌──────▼──────┐            ┌──────▼──────┐
│   Receipt   │            │   Appeal    │
│    Node     │            │    Node     │
└─────────────┘            └─────────────┘
```

- **Node Types** - Different operations in the DAG
- **Causality** - Tracking "happens-before" relationships
- **Anchoring** - Creating checkpoints for verification
- **Conflict Resolution** - Handling concurrent operations

### 3. Identity and Trust

Identity is managed through DIDs and Verifiable Credentials:

- **DIDs (Decentralized Identifiers)** - Self-sovereign identities
- **VCs (Verifiable Credentials)** - Cryptographically verifiable claims
- **Trust Bundles** - Collections of trusted credential issuers
- **Authorization** - Permission model based on credentials

### 4. CCL (Capability Control Language)

CCL is a domain-specific language for governance rules:

```ccl
schema ProposalSchema {
    title: String,
    description: String,
    action: String
}

rule AddMemberRule {
    description: "Rule for adding new members to the federation"
    when:
        proposal oftype ProposalSchema
        with proposal.action == "add_member"
    then:
        authorize(invoker, "federation:add_member")
}
```

- **Schemas** - Data structure definitions
- **Rules** - Conditional logic for permissions
- **Actions** - Operations that can be performed
- **Compilation** - Transformation to executable WASM

## Development Workflows

### Creating a Federation

```rust
// 1. Initialize a new federation
let federation_config = FederationConfig {
    name: "ExampleFederation".to_string(),
    description: "A test federation for development".to_string(),
    founding_members: vec![
        did1.to_string(),
        did2.to_string(),
        did3.to_string(),
    ],
    voting_threshold: 67, // 2/3 majority
    // ...other configuration
};

// 2. Create bootstrap proposal
let proposal = BootstrapProposal::new(federation_config);
let proposal_node = create_dag_node(proposal);

// 3. Collect signatures from founding members
let signatures = collect_signatures(&proposal_node, &founding_members);

// 4. Submit bootstrap transaction
let bootstrap_result = runtime.bootstrap_federation(
    proposal_node,
    signatures,
).await?;

// 5. Initialize wallet with federation
wallet.join_federation(bootstrap_result.federation_did).await?;
```

### Creating and Voting on a Proposal

```rust
// 1. Create proposal
let proposal = ProposalData {
    title: "Add New Member".to_string(),
    description: "Add Alice to our federation".to_string(),
    action: "add_member".to_string(),
    parameters: json!({
        "member_did": "did:icn:alice",
    }),
};

// 2. Submit proposal
let proposal_id = wallet.create_proposal(
    federation_id,
    proposal,
).await?;

// 3. Other members vote
let vote = Vote {
    proposal_id: proposal_id.clone(),
    choice: VoteChoice::Approve,
    justification: Some("Alice will be a valuable member".to_string()),
};

wallet.submit_vote(vote).await?;

// 4. Check proposal status
let status = wallet.get_proposal_status(proposal_id).await?;
if status.is_approved() {
    println!("Proposal approved!");
}
```

### Developing a CCL Contract

1. **Write CCL Code**

```ccl
// In file: member_management.ccl
schema MemberRequest {
    member_did: String,
    reason: String,
    vouchers: Array<String>
}

rule RequireVouchers {
    description: "Require at least two existing members to vouch for a new member"
    when:
        request oftype MemberRequest
    then:
        require(request.vouchers.length >= 2)
}

contract MemberManagement {
    action add_member(MemberRequest request) {
        // Run the voucher rule
        apply_rule(RequireVouchers, request);
        
        // If successful, authorize the member addition
        authorize(federation, "member:add", request.member_did);
    }
    
    action remove_member(String member_did) {
        // Only federation admin can remove members
        require_credential(invoker, "federation:admin");
        
        // Authorize the removal
        authorize(federation, "member:remove", member_did);
    }
}
```

2. **Compile to WASM**

```bash
# Compile the CCL contract to WASM
icn-ccl-compiler member_management.ccl -o member_management.wasm
```

3. **Upload and Register**

```rust
// Upload the compiled WASM to the federation
let wasm_bytes = fs::read("member_management.wasm")?;
let wasm_cid = wallet.upload_wasm(wasm_bytes).await?;

// Register the contract with the federation
let register_proposal = ProposalData {
    title: "Register Member Management Contract".to_string(),
    description: "Contract for managing federation membership".to_string(),
    action: "register_contract".to_string(),
    parameters: json!({
        "contract_cid": wasm_cid.to_string(),
        "contract_name": "member_management",
    }),
};

wallet.create_proposal(federation_id, register_proposal).await?;
```

### Setting Up Mesh Compute

1. **Define a Mesh Policy**

```rust
let mesh_policy = MeshPolicy {
    // Reputation parameters
    alpha: 0.6, // Execution performance weight
    beta: 0.3,  // Verification accuracy weight
    gamma: 0.1, // Penalty factor
    lambda: 0.05, // Reputation decay rate
    
    // Economic settings
    reward_settings: RewardSettings {
        worker_percentage: 70,
        verifier_percentage: 20,
        platform_fee_percentage: 10,
        use_reputation_weighting: true,
        platform_fee_address: federation_did.to_string(),
    },
    
    // Resource requirements
    base_capability_scope: CapabilityScope {
        mem_mb: 128,
        cpu_cycles: 1_000_000,
        gpu_flops: 0,
        io_mb: 50,
    },
    
    // Verification settings
    verification_quorum: VerificationQuorum {
        required_percentage: 66,
        minimum_verifiers: 3,
        maximum_verifiers: 7,
        verification_timeout_minutes: 15,
    },
    
    // Other settings...
};

// Create a proposal to activate the mesh policy
let policy_proposal = ProposalData {
    title: "Activate Mesh Policy".to_string(),
    description: "Initial mesh compute policy for our federation".to_string(),
    action: "activate_mesh_policy".to_string(),
    parameters: json!(mesh_policy),
};

wallet.create_proposal(federation_id, policy_proposal).await?;
```

2. **Start a Mesh Node**

```bash
# Start a mesh node
cargo run --bin meshctl -- start --node-id "did:icn:mesh:node1" --listen "/ip4/0.0.0.0/tcp/9000"
```

3. **Submit a Computation Task**

```rust
// Create a task
let task = TaskIntent {
    publisher_did: wallet.did().to_string(),
    wasm_cid: wasm_module_cid,
    input_cid: input_data_cid,
    fee: 100,
    verifiers: 3,
    capability_scope: CapabilityScope {
        mem_mb: 256,
        cpu_cycles: 2_000_000,
        gpu_flops: 0,
        io_mb: 100,
    },
    expiry: Utc::now() + Duration::hours(24),
};

// Publish task
let task_cid = wallet.publish_mesh_task(task).await?;
```

## API Reference

### Runtime API

```rust
// Federation operations
fn bootstrap_federation(config: FederationConfig) -> Result<FederationId>
fn join_federation(federation_id: FederationId, member_did: Did) -> Result<()>
fn leave_federation(federation_id: FederationId, member_did: Did) -> Result<()>

// Governance operations
fn submit_proposal(proposal: ProposalData) -> Result<ProposalId>
fn submit_vote(vote: Vote) -> Result<()>
fn submit_appeal(appeal: Appeal) -> Result<AppealId>

// DAG operations
fn submit_dag_node(node: DagNode) -> Result<NodeSubmissionResponse>
fn get_dag_node(cid: Cid) -> Result<Option<DagNode>>
fn get_dag_tips() -> Result<Vec<Cid>>

// Identity operations
fn resolve_did(did: &str) -> Result<DidDocument>
fn verify_credential(credential: &VerifiableCredential) -> Result<bool>
```

### Wallet API

```rust
// Identity operations
fn create_identity() -> Result<Did>
fn sign(data: &[u8]) -> Result<Signature>
fn verify(did: &str, data: &[u8], signature: &Signature) -> Result<bool>

// Federation operations
fn list_federations() -> Result<Vec<Federation>>
fn join_federation(invitation: FederationInvitation) -> Result<()>
fn leave_federation(federation_id: &str) -> Result<()>

// Governance operations
fn create_proposal(federation_id: &str, proposal: ProposalData) -> Result<ProposalId>
fn vote_on_proposal(proposal_id: &str, choice: VoteChoice) -> Result<()>
fn get_proposal_status(proposal_id: &str) -> Result<ProposalStatus>

// Sync operations
fn sync_federation(federation_id: &str) -> Result<SyncStats>
fn get_federation_updates(federation_id: &str) -> Result<Vec<Update>>
```

### AgoraNet API

AgoraNet exposes a RESTful API:

```
GET    /api/v1/federations              # List accessible federations
GET    /api/v1/federations/{id}         # Get federation details
GET    /api/v1/federations/{id}/threads # List deliberation threads
POST   /api/v1/federations/{id}/threads # Create a new thread
GET    /api/v1/threads/{id}             # Get thread details
POST   /api/v1/threads/{id}/messages    # Post a message to a thread
GET    /api/v1/proposals                # List governance proposals
GET    /api/v1/proposals/{id}           # Get proposal details
POST   /api/v1/proposals/{id}/votes     # Vote on a proposal
```

### Mesh API

Mesh provides a set of command-line and programmatic interfaces:

```
mesh policy view                        # View active mesh policy
mesh policy propose [options]           # Propose policy update
mesh task publish [options]             # Publish a computation task
mesh task list [options]                # List active tasks
mesh worker register [options]          # Register as a compute worker
mesh worker list [options]              # List active workers
```

## Common Developer Tasks

### 1. Custom Credential Issuance

```rust
// Create a credential issuer
let issuer = CredentialIssuer::new(wallet_did, wallet_key_pair);

// Define credential data
let credential_data = json!({
    "type": "FederationMembership",
    "federationId": federation_id,
    "joinDate": Utc::now().to_rfc3339(),
    "role": "member"
});

// Issue the credential
let credential = issuer.issue_credential(
    recipient_did,
    "FederationMembership",
    credential_data,
    Some(expiry_date),
).await?;

// Store in recipient's wallet
recipient_wallet.store_credential(credential).await?;
```

### 2. Creating a Custom CCL Rule

```ccl
// Define a custom authorization rule
rule ResourceQuotaRule {
    description: "Enforce resource quota limits for federation members"
    
    // Define variables
    let current_usage = get_resource_usage(invoker);
    let quota_limit = get_member_quota(invoker);
    
    when:
        resource_request oftype ResourceRequest
        with resource_request.amount + current_usage <= quota_limit
    then:
        authorize(invoker, "resource:allocate")
}
```

### 3. Setting Up Local Development

```bash
# 1. Start the development network
./start_icn_devnet.sh

# 2. Create a test federation
cargo run --bin icn-cli -- federation create \
  --name "Test Federation" \
  --description "For development purposes" \
  --members did:icn:dev1,did:icn:dev2,did:icn:dev3

# 3. Create a test wallet
cargo run --bin icn-wallet -- create \
  --name "Dev Wallet" \
  --federation did:icn:federation:test

# 4. Connect to the dashboard
cargo run --bin agoranet-dashboard

# 5. Watch logs for debugging
tail -f logs/devnet.log
```

### 4. Writing Integration Tests

```rust
#[tokio::test]
async fn test_proposal_lifecycle() -> Result<()> {
    // Set up test environment
    let runtime = setup_test_runtime().await?;
    let wallet1 = setup_test_wallet("member1").await?;
    let wallet2 = setup_test_wallet("member2").await?;
    
    // Create test federation
    let federation_id = setup_test_federation(
        &runtime, 
        &[wallet1.did(), wallet2.did()]
    ).await?;
    
    // Create a test proposal
    let proposal_data = ProposalData {
        title: "Test Proposal".to_string(),
        description: "For testing purposes".to_string(),
        action: "test_action".to_string(),
        parameters: json!({"test_param": "test_value"}),
    };
    
    let proposal_id = wallet1.create_proposal(
        &federation_id,
        proposal_data,
    ).await?;
    
    // Submit votes
    wallet1.vote_on_proposal(&proposal_id, VoteChoice::Approve).await?;
    wallet2.vote_on_proposal(&proposal_id, VoteChoice::Approve).await?;
    
    // Check proposal status
    let status = wallet1.get_proposal_status(&proposal_id).await?;
    assert_eq!(status, ProposalStatus::Approved);
    
    // Verify execution receipt
    let receipt = wallet1.get_execution_receipt(&proposal_id).await?;
    assert!(receipt.is_some());
    assert!(receipt.unwrap().successful);
    
    Ok(())
}
```

## Troubleshooting

### Common Issues and Solutions

1. **Build Errors**

```
error[E0308]: mismatched types
```

Check for version mismatches in dependencies. The project uses specific versions that work together.

2. **Runtime Errors**

```
Error: NodeValidationFailed("Signature verification failed")
```

Ensure you're using the correct identity key for signing operations.

3. **Network Connection Issues**

```
Error: Connection refused (os error 111)
```

Make sure the devnet is running and check the port configuration.

4. **DAG Synchronization Failures**

```
Error: AnchorNotFound("bafy...")
```

Run `wallet sync --force` to resynchronize from the latest anchor.

### Debugging Tips

1. **Enable Detailed Logging**

```bash
RUST_LOG=debug cargo run -- <command>
```

2. **Inspect DAG State**

```bash
cargo run --bin icn-dag-explorer
```

3. **Monitor Network Traffic**

```bash
cargo run --bin icn-net-monitor
```

4. **Use the Test Harness**

```bash
cargo test --features "test-utilities"
```

## Additional Resources

- [Architecture Reference](./CONSOLIDATED_ARCHITECTURE.md)
- [Integration Guide](./CONSOLIDATED_SYSTEM_INTEGRATION.md)
- [CCL Language Specification](https://specs.intercoop.network/ccl/)
- [API Documentation](https://docs.intercoop.network/api/)
- [Example Projects](https://github.com/intercoop-network/examples)

## Community

- **Discord**: [discord.gg/intercoop](https://discord.gg/intercoop)
- **Forums**: [forum.intercoop.network](https://forum.intercoop.network)
- **GitHub**: [github.com/intercoop-network](https://github.com/intercoop-network)
- **Documentation**: [docs.intercoop.network](https://docs.intercoop.network) 