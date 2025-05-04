# ICN Integration Guide

## 1. Overview

The Intercooperative Network (ICN) is a decentralized governance and economic coordination system built on a foundation of verifiable credentials, directed acyclic graphs (DAGs), and WebAssembly (WASM) execution. This guide serves as an entry point for developers and federation operators integrating with the ICN stack.

The ICN architecture consists of three primary components:

```
┌─────────────────────────────────────────────────────────┐
│                   ICN Core Components                   │
├─────────────────────────────────────────────────────────┤
│ • Runtime/CoVM: DAG-based execution environment         │
│ • Wallet Core: Credential and identity management       │
│ • AgoraNet: Deliberation and coordination layer         │
└─────────────────────────────────────────────────────────┘
```

### Integration Goals

ICN integration typically focuses on one or more of these objectives:

1. **Federation Operation**: Running nodes in a federation network
2. **Governance Participation**: Creating and voting on proposals
3. **Economic Integration**: Implementing resource metering and token flows
4. **Deliberation**: Participating in discussion and coordination
5. **Identity Management**: Issuing and verifying credentials

### Related Documentation

For deeper understanding of specific subsystems, refer to:
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Complete system architecture
- [GOVERNANCE_SYSTEM.md](docs/GOVERNANCE_SYSTEM.md) - Governance mechanisms
- [ECONOMICS.md](docs/ECONOMICS.md) - Economic system specification
- [SECURITY.md](docs/SECURITY.md) - Security model and threat mitigations
- [DAG_STRUCTURE.md](docs/DAG_STRUCTURE.md) - DAG implementation details

## 2. Prerequisites

### Development Environment

The ICN stack requires the following components:

```bash
# Rust toolchain (minimum version 1.70)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup target add wasm32-unknown-unknown

# WASM tools
cargo install wasm-pack wasm-tools

# PostgreSQL 14+
sudo apt install postgresql-14

# Docker & Docker Compose
curl -fsSL https://get.docker.com | sh
sudo apt install docker-compose-plugin

# ICN CLI tools
cargo install --git https://github.com/intercooperative/icn-cli.git
```

### Development Container

For a preconfigured development environment, use our Dev Container:

```bash
# Clone the repository
git clone https://github.com/intercooperative/icn.git
cd icn

# Start the development container
docker-compose -f docker/dev-environment.yml up -d
docker exec -it icn_dev bash

# Inside the container, build the stack
./scripts/build_all.sh
```

### Federation Operator Requirements

Federation operators need additional infrastructure:

- Dedicated servers (4+ cores, 16GB+ RAM, 100GB+ SSD)
- Static IP addresses with ports 9000-9010 accessible
- HSM or secure key management solution
- Monitoring infrastructure (Prometheus/Grafana recommended)
- Backup solution for anchor and credential storage

## 3. Core Concepts

### Decentralized Identifiers (DIDs)

DIDs are persistent identifiers that enable verifiable, decentralized digital identity. In ICN, DIDs serve as the foundation for identity:

```rust
// Example DID creation
pub fn create_federation_did(
    federation_name: &str,
    key_pair: &KeyPair,
) -> Result<Did, DidError> {
    let method = "icn";
    let id = generate_id_from_public_key(&key_pair.public_key);
    let did = format!("did:{}:{}:{}", method, federation_name, id);
    
    // Register DID in the local DID registry
    register_did(&did, key_pair)?;
    
    Ok(Did::from_string(did)?)
}
```

### Directed Acyclic Graphs (DAGs)

The ICN uses a DAG data structure to record all operations and state transitions:

```rust
pub struct DagNode {
    // Content identifier (hash of the node)
    pub cid: CID,
    
    // Parent nodes (previous operations)
    pub parents: Vec<CID>,
    
    // Node issuer
    pub issuer: Did,
    
    // Timestamp
    pub timestamp: DateTime<Utc>,
    
    // Payload (operation data)
    pub payload: Vec<u8>,
    
    // Metadata
    pub metadata: DagNodeMetadata,
    
    // Signature
    pub signature: Vec<u8>,
}
```

### Verifiable Credentials

Credentials are cryptographically verifiable claims about entities:

```rust
pub struct Credential {
    // Credential identifier
    pub id: CredentialId,
    
    // Issuer
    pub issuer: Did,
    
    // Subject
    pub subject: Did,
    
    // Credential type
    pub credential_type: CredentialType,
    
    // Issuance date
    pub issuance_date: DateTime<Utc>,
    
    // Expiration date (optional)
    pub expiration_date: Option<DateTime<Utc>>,
    
    // Claims
    pub claims: HashMap<String, Value>,
    
    // Proof
    pub proof: CredentialProof,
}
```

### DAG Anchors

Anchors provide cryptographic commitments to the state of the DAG:

```rust
pub struct Anchor {
    // Federation identifier
    pub federation_id: FederationId,
    
    // Timestamp
    pub timestamp: DateTime<Utc>,
    
    // Merkle root of operations
    pub operations_root: Hash,
    
    // Previous anchor
    pub previous_anchor: Option<Hash>,
    
    // Quorum signatures
    pub signatures: Vec<QuorumSignature>,
    
    // Merkle tree
    pub merkle_tree: MerkleTree,
    
    // Anchor metadata
    pub anchor_metadata: AnchorMetadata,
}
```

### TrustBundles

TrustBundles define the trust relationships between entities:

```rust
pub struct TrustBundle {
    // Bundle identifier
    pub id: BundleId,
    
    // Federation issuing this bundle
    pub federation_id: FederationId,
    
    // Trusted entities
    pub trusted_entities: Vec<TrustedEntity>,
    
    // Trust policies
    pub trust_policies: Vec<TrustPolicy>,
    
    // Revocation information
    pub revocation_info: RevocationInfo,
    
    // Bundle validity
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    
    // Bundle signature
    pub signature: BundleSignature,
}
```

### Execution Receipts

Receipts prove that operations were executed correctly:

```rust
pub struct ExecutionReceipt {
    // Receipt identifier
    pub id: ReceiptId,
    
    // Operation identifier
    pub operation_id: OperationId,
    
    // Executor
    pub executor: Did,
    
    // Execution time
    pub execution_time: DateTime<Utc>,
    
    // Execution result
    pub result: ExecutionResult,
    
    // State changes
    pub state_changes: Option<Vec<StateChange>>,
    
    // Receipt signatures
    pub signatures: Vec<ReceiptSignature>,
}
```

## 4. Runtime Integration

### Setting Up a Runtime Node

To run an ICN Runtime node:

```bash
# Clone the repository
git clone https://github.com/intercooperative/icn.git
cd icn

# Configure the node
cp config/runtime.example.toml config/runtime.toml
nano config/runtime.toml  # Edit configuration

# Initialize the database
cargo run --bin icn-runtime-init -- --config config/runtime.toml

# Start the node
cargo run --bin icn-runtime -- --config config/runtime.toml
```

### Submitting Proposals

Proposals are submitted as DAG nodes:

```rust
pub fn submit_governance_proposal(
    runtime_client: &RuntimeClient,
    proposal: Proposal,
    key_pair: &KeyPair,
) -> Result<ProposalId, RuntimeError> {
    // Create proposal payload
    let payload = serialize_proposal(&proposal)?;
    
    // Create DAG node
    let node = runtime_client.create_dag_node(
        &key_pair,
        payload,
        NodeType::Proposal,
        vec![], // Optional parent CIDs
    )?;
    
    // Submit node to the network
    let cid = runtime_client.submit_dag_node(node)?;
    
    // Return proposal ID (matches CID)
    Ok(ProposalId::from(cid))
}
```

### Compiling CCL (Cooperative Coordination Language)

CCL is the domain-specific language for defining governance rules:

```bash
# Install CCL compiler
cargo install --git https://github.com/intercooperative/ccl.git

# Compile CCL to WASM
ccl compile --input my_policy.ccl --output my_policy.wasm

# Validate the compiled module
ccl validate --input my_policy.wasm
```

Example CCL policy:

```
// resource_policy.ccl
policy ResourceAllocation {
    params {
        max_cpu_per_user: u64 = 1000;
        max_storage_per_user: u64 = 50000;
    }
    
    rule check_cpu_allocation {
        when {
            op: ResourceAllocation,
            op.resource_type == "compute.cpu",
            user: User
        }
        require {
            op.quantity <= max_cpu_per_user - user.current_cpu_usage;
        }
    }
    
    rule check_storage_allocation {
        when {
            op: ResourceAllocation,
            op.resource_type == "storage",
            user: User
        }
        require {
            op.quantity <= max_storage_per_user - user.current_storage_usage;
        }
    }
}
```

### Running WASM Governance Modules

WASM modules can be deployed and executed through the Runtime:

```rust
pub fn deploy_governance_module(
    runtime_client: &RuntimeClient,
    wasm_bytes: Vec<u8>,
    module_name: &str,
    key_pair: &KeyPair,
) -> Result<ModuleId, RuntimeError> {
    // Create module deployment payload
    let deployment = ModuleDeployment {
        name: module_name.to_string(),
        wasm_bytecode: wasm_bytes,
        interface_version: "1.0".to_string(),
        metadata: HashMap::new(),
    };
    
    // Serialize the deployment
    let payload = serialize_module_deployment(&deployment)?;
    
    // Create DAG node
    let node = runtime_client.create_dag_node(
        &key_pair,
        payload,
        NodeType::ModuleDeployment,
        vec![], // Optional parent CIDs
    )?;
    
    // Submit node to the network
    let cid = runtime_client.submit_dag_node(node)?;
    
    // Return module ID
    Ok(ModuleId::from(cid))
}
```

### Anchoring Operations

All federation nodes participate in anchoring:

```rust
pub fn participate_in_anchoring(
    runtime_client: &RuntimeClient,
    federation_id: &FederationId,
    key_pair: &KeyPair,
) -> Result<(), RuntimeError> {
    // Get pending operations since last anchor
    let operations = runtime_client.get_operations_since_last_anchor(federation_id)?;
    
    // Generate anchor signature
    let anchor_payload = runtime_client.generate_anchor_payload(federation_id, &operations)?;
    let signature = sign_anchor_payload(&anchor_payload, key_pair)?;
    
    // Submit signature to the network
    runtime_client.submit_anchor_signature(
        federation_id,
        signature,
    )?;
    
    Ok(())
}
```

### Economic Actions

Resource metering and token operations are handled through the Runtime:

```rust
pub fn perform_metered_action(
    runtime_client: &RuntimeClient,
    resource_type: ResourceType,
    quantity: ResourceQuantity,
    key_pair: &KeyPair,
) -> Result<ActionReceipt, RuntimeError> {
    // Create resource usage operation
    let operation = ResourceUsageOperation {
        resource_type,
        quantity,
        timestamp: DateTime::now_utc(),
        context: HashMap::new(),
    };
    
    // Serialize the operation
    let payload = serialize_resource_operation(&operation)?;
    
    // Create DAG node
    let node = runtime_client.create_dag_node(
        &key_pair,
        payload,
        NodeType::ResourceUsage,
        vec![], // Optional parent CIDs
    )?;
    
    // Submit node to the network
    let cid = runtime_client.submit_dag_node(node)?;
    
    // Wait for execution receipt
    let receipt = runtime_client.wait_for_execution_receipt(cid, Duration::from_secs(30))?;
    
    Ok(receipt)
}
```

## 5. Wallet Integration

### Using the Mobile Wallet FFI

For mobile applications, use the Wallet FFI:

```rust
// Kotlin example using the Wallet FFI
fun initializeWallet(storageDirectory: String): Boolean {
    return WalletFFI.initialize(storageDirectory)
}

fun createIdentity(name: String): String? {
    val result = WalletFFI.createIdentity(name)
    return if (result.isSuccess) result.did else null
}

fun importCredential(credentialJson: String): Boolean {
    return WalletFFI.importCredential(credentialJson)
}
```

### Using the Wallet CLI

For command-line interactions:

```bash
# Initialize wallet
icn-wallet init --storage-dir ~/.icn-wallet

# Create a new identity
icn-wallet identity create --name "Federation Operator"

# Import a credential
icn-wallet credential import --file my_credential.json

# Create a selective disclosure proof
icn-wallet credential disclose --id cred-123 --attributes "name,role" --output proof.json

# Sign a message
icn-wallet sign --did did:icn:123 --message "Hello World" --output signature.bin
```

### Credential Issuance

Federations can issue credentials to members:

```rust
pub fn issue_member_credential(
    wallet: &Wallet,
    issuer_did: &Did,
    subject_did: &Did,
    member_attributes: &MemberAttributes,
) -> Result<Credential, WalletError> {
    // Create the credential
    let credential = wallet.create_credential(
        issuer_did,
        subject_did,
        CredentialType::FederationMember,
        &[
            ("name", Value::String(member_attributes.name.clone())),
            ("role", Value::String(member_attributes.role.clone())),
            ("joined_at", Value::DateTime(DateTime::now_utc())),
            ("federation_id", Value::String(member_attributes.federation_id.clone())),
        ],
        Some(DateTime::now_utc() + Duration::days(365)),
    )?;
    
    // Sign the credential
    let signed_credential = wallet.sign_credential(credential, issuer_did)?;
    
    // Export for delivery to the subject
    let exported = wallet.export_credential(&signed_credential.id)?;
    
    // TODO: Deliver credential to subject via secure channel
    
    Ok(signed_credential)
}
```

### Selective Disclosure

Wallet users can create proofs that reveal only specific attributes:

```rust
pub fn create_selective_disclosure(
    wallet: &Wallet,
    credential_id: &CredentialId,
    attributes: &[&str],
    challenge: &str,
) -> Result<SelectiveDisclosureProof, WalletError> {
    // Get the credential
    let credential = wallet.get_credential(credential_id)?;
    
    // Create the disclosure proof
    let proof = wallet.create_selective_disclosure(
        credential_id,
        attributes,
        challenge,
    )?;
    
    // Export the proof for presentation
    let exported_proof = wallet.export_disclosure_proof(&proof.id)?;
    
    // TODO: Present the proof to the verifier
    
    Ok(proof)
}
```

### Federation Share Links

Federation operators can generate invitation links:

```rust
pub fn generate_federation_invitation(
    wallet: &Wallet,
    federation_id: &FederationId,
    invitee_name: &str,
    role: &str,
    expiration: Duration,
) -> Result<String, WalletError> {
    // Create invitation payload
    let invitation = FederationInvitation {
        federation_id: federation_id.clone(),
        invitee_name: invitee_name.to_string(),
        role: role.to_string(),
        expires_at: DateTime::now_utc() + expiration,
        invitation_id: generate_uuid(),
    };
    
    // Sign the invitation
    let federation_did = wallet.get_did_for_federation(federation_id)?;
    let signature = wallet.sign_data(&serialize_invitation(&invitation)?, &federation_did)?;
    
    // Create shareable link
    let link = format!(
        "icn://join?federation={}&invitation={}&signature={}",
        federation_id,
        base64_encode(&serialize_invitation(&invitation)?),
        base64_encode(&signature),
    );
    
    Ok(link)
}
```

## 6. AgoraNet Integration

### Thread/Message APIs

AgoraNet provides discussion capabilities:

```rust
// Create a new discussion thread
pub async fn create_thread(
    agoranet_client: &AgoraNetClient,
    title: &str,
    description: &str,
    category: &str,
    key_pair: &KeyPair,
) -> Result<ThreadId, AgoraNetError> {
    let thread = Thread {
        title: title.to_string(),
        description: description.to_string(),
        category: category.to_string(),
        creator: did_from_keypair(key_pair),
        created_at: DateTime::now_utc(),
        status: ThreadStatus::Open,
        tags: vec![],
    };
    
    let thread_id = agoranet_client.create_thread(thread, key_pair).await?;
    Ok(thread_id)
}

// Post a message to a thread
pub async fn post_message(
    agoranet_client: &AgoraNetClient,
    thread_id: &ThreadId,
    content: &str,
    key_pair: &KeyPair,
) -> Result<MessageId, AgoraNetError> {
    let message = Message {
        thread_id: thread_id.clone(),
        content: content.to_string(),
        author: did_from_keypair(key_pair),
        created_at: DateTime::now_utc(),
        attachments: vec![],
    };
    
    let message_id = agoranet_client.post_message(message, key_pair).await?;
    Ok(message_id)
}
```

### Authentication with Wallet Tokens

AgoraNet clients authenticate using wallet-generated tokens:

```rust
pub async fn authenticate_with_agoranet(
    agoranet_client: &AgoraNetClient,
    wallet: &Wallet,
    user_did: &Did,
) -> Result<AuthToken, AuthError> {
    // Get challenge from server
    let challenge = agoranet_client.request_auth_challenge(user_did).await?;
    
    // Sign challenge with wallet
    let signature = wallet.sign_data(&challenge.challenge_bytes, user_did)?;
    
    // Submit signature and get token
    let auth_token = agoranet_client.authenticate(
        user_did,
        &challenge.challenge_id,
        &signature,
    ).await?;
    
    Ok(auth_token)
}
```

### Linking Deliberation to Proposals

Governance proposals can be linked to AgoraNet discussions:

```rust
pub async fn link_proposal_to_thread(
    runtime_client: &RuntimeClient,
    agoranet_client: &AgoraNetClient,
    proposal_id: &ProposalId,
    thread_id: &ThreadId,
    key_pair: &KeyPair,
) -> Result<(), LinkError> {
    // Create link payload
    let link = ProposalThreadLink {
        proposal_id: proposal_id.clone(),
        thread_id: thread_id.clone(),
        linked_at: DateTime::now_utc(),
        linker: did_from_keypair(key_pair),
    };
    
    // Submit link to runtime
    let payload = serialize_proposal_thread_link(&link)?;
    let node = runtime_client.create_dag_node(
        key_pair,
        payload,
        NodeType::ProposalThreadLink,
        vec![], // Optional parent CIDs
    )?;
    
    runtime_client.submit_dag_node(node)?;
    
    // Update thread metadata in AgoraNet
    agoranet_client.update_thread_metadata(
        thread_id,
        &[("linked_proposal", proposal_id.to_string())],
        key_pair,
    ).await?;
    
    Ok(())
}
```

## 7. Federation Bootstrap

### Genesis Flow

Creating a new federation requires a genesis process:

```bash
# Generate federation genesis configuration
icn-federation-init generate-config --name "Example Federation" --output federation-config.json

# Edit the configuration file
nano federation-config.json

# Initialize the federation
icn-federation-init create --config federation-config.json --output genesis.json

# Start the first node with genesis
icn-runtime --config runtime.toml --genesis genesis.json
```

The genesis configuration includes:

```json
{
  "federation_name": "Example Federation",
  "federation_id": "fed-12345",
  "genesis_timestamp": "2023-07-01T00:00:00Z",
  "initial_guardians": [
    {
      "name": "Guardian 1",
      "did": "did:icn:example:guardian1",
      "public_key": "..."
    },
    {
      "name": "Guardian 2",
      "did": "did:icn:example:guardian2",
      "public_key": "..."
    }
  ],
  "quorum_rules": {
    "min_guardians": 2,
    "threshold_percentage": 67
  },
  "initial_policies": [
    {
      "name": "Resource Allocation Policy",
      "policy_type": "economic",
      "wasm_module": "..."
    }
  ]
}
```

### Guardian Keys

Guardian keys should be generated securely:

```rust
pub fn generate_guardian_keys(
    wallet: &Wallet,
    guardian_name: &str,
) -> Result<(Did, String), WalletError> {
    // Create a new identity for the guardian
    let did = wallet.create_identity(guardian_name)?;
    
    // Export the public key for inclusion in the genesis
    let public_key = wallet.export_public_key(&did)?;
    
    // Generate a backup of the key (store securely!)
    let backup = wallet.backup_identity(&did, "strong-password-here")?;
    
    Ok((did, public_key))
}
```

### Quorum Configuration

Federation quorums are defined in the genesis and can be updated:

```rust
pub fn update_quorum_configuration(
    runtime_client: &RuntimeClient,
    federation_id: &FederationId,
    new_config: QuorumConfiguration,
    key_pair: &KeyPair,
) -> Result<(), RuntimeError> {
    // Create quorum update proposal
    let proposal = Proposal {
        title: "Update Quorum Configuration".to_string(),
        description: "Updating quorum rules for improved security.".to_string(),
        proposal_type: ProposalType::PolicyUpdate {
            policy_id: "quorum-policy".into(),
            update_type: PolicyUpdateType::QuorumUpdate,
            new_policy_text: serde_json::to_string(&new_config)?,
            rationale: "Increasing security requirements".to_string(),
        },
        scope: GovernanceScope::Federation(federation_id.clone()),
        // other fields omitted for brevity
    };
    
    // Submit the proposal
    let proposal_id = submit_governance_proposal(runtime_client, proposal, key_pair)?;
    
    // Note: Proposal must go through voting and execution phases
    
    Ok(())
}
```

### Anchoring Initial TrustBundle

The initial TrustBundle must be anchored:

```rust
pub fn create_initial_trust_bundle(
    runtime_client: &RuntimeClient,
    federation_id: &FederationId,
    trusted_entities: Vec<TrustedEntity>,
    key_pair: &KeyPair,
) -> Result<BundleId, RuntimeError> {
    // Create TrustBundle
    let bundle = TrustBundle {
        id: generate_bundle_id(),
        federation_id: federation_id.clone(),
        trusted_entities,
        trust_policies: vec![],
        revocation_info: RevocationInfo::new(),
        valid_from: DateTime::now_utc(),
        valid_until: DateTime::now_utc() + Duration::days(90),
        signature: BundleSignature::None, // Will be filled later
    };
    
    // Sign the bundle
    let signed_bundle = sign_trust_bundle(bundle, key_pair)?;
    
    // Create DAG node
    let payload = serialize_trust_bundle(&signed_bundle)?;
    let node = runtime_client.create_dag_node(
        key_pair,
        payload,
        NodeType::TrustBundle,
        vec![], // No parents for initial bundle
    )?;
    
    // Submit node to the network
    let cid = runtime_client.submit_dag_node(node)?;
    
    // Return bundle ID
    Ok(signed_bundle.id)
}
```

## 8. Diagnostics & Observability

### Prometheus Metrics

Runtime nodes expose Prometheus metrics:

```bash
# Configure Prometheus endpoint in runtime.toml
metrics_endpoint = "0.0.0.0:9090"

# Example Prometheus configuration
cat > prometheus.yml << EOF
scrape_configs:
  - job_name: 'icn_runtime'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:9090']
EOF

# Start Prometheus
docker run -d -p 9091:9090 -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml prom/prometheus
```

Key metrics include:

- `icn_dag_nodes_total`: Total number of DAG nodes
- `icn_dag_operations_by_type`: Operations by type
- `icn_anchor_creation_duration_seconds`: Anchor creation time
- `icn_wasm_execution_duration_seconds`: WASM execution time
- `icn_token_operations_total`: Token operations by type

### DAG Diagnostic CLI

The diagnostic CLI provides tools for DAG inspection:

```bash
# Visualize DAG
icn-dag-tools visualize --node-cid QmYour123NodeCID --output dag.svg

# Verify DAG consistency
icn-dag-tools verify --federation-id fed-12345

# Analyze DAG structure
icn-dag-tools analyze --federation-id fed-12345 --metric causality

# Export DAG subset
icn-dag-tools export --start-cid QmStart123CID --end-cid QmEnd456CID --output dag-subset.json
```

### Audit CLI

The audit tools help verify system integrity:

```bash
# Check credential revocation status
icn-audit credential-status --credential-id cred-123

# Verify anchor integrity
icn-audit verify-anchor --anchor-cid QmAnchor123CID

# Audit governance actions
icn-audit governance-log --federation-id fed-12345 --start-date 2023-01-01 --end-date 2023-01-31

# Verify token balances
icn-audit token-balances --federation-id fed-12345
```

### DAG Replay Verifier

Replay verification allows validating historical state:

```rust
pub fn verify_dag_replay(
    replay_client: &ReplayClient,
    federation_id: &FederationId,
    start_anchor: &AnchorId,
    end_anchor: &AnchorId,
) -> Result<ReplayVerificationResult, ReplayError> {
    // Start replay process
    let replay_id = replay_client.start_replay(
        federation_id,
        start_anchor,
        end_anchor,
    )?;
    
    // Wait for replay to complete
    let result = replay_client.wait_for_replay_completion(replay_id, Duration::from_secs(600))?;
    
    // Get detailed verification report
    let report = replay_client.get_replay_report(replay_id)?;
    
    // Check if verification succeeded
    if result.is_verified {
        println!("Replay verification successful!");
    } else {
        println!("Replay verification failed. See report for details.");
    }
    
    Ok(result)
}
```

## 9. Security Integration

### Verification Hooks

Implement custom verification hooks:

```rust
pub fn register_custom_verification_hook(
    runtime_client: &RuntimeClient,
    hook_type: VerificationHookType,
    hook_handler: Box<dyn VerificationHandler>,
) -> Result<HookId, SecurityError> {
    // Register the hook with the runtime
    let hook_id = runtime_client.register_verification_hook(
        hook_type,
        hook_handler,
    )?;
    
    println!("Registered {} hook with ID {}", hook_type, hook_id);
    
    Ok(hook_id)
}

// Example verification handler
struct TokenOperationVerifier;

impl VerificationHandler for TokenOperationVerifier {
    fn verify(&self, context: &VerificationContext) -> VerificationResult {
        // Extract token operation
        if let Some(token_op) = context.get_token_operation() {
            // Perform custom verification logic
            if token_op.quantity > MAX_ALLOWED_QUANTITY {
                return VerificationResult::Reject(
                    "Token quantity exceeds maximum allowed".into()
                );
            }
            
            if is_blacklisted_entity(&token_op.recipient) {
                return VerificationResult::Reject(
                    "Recipient is blacklisted".into()
                );
            }
        }
        
        VerificationResult::Accept
    }
}
```

### Key Rotation

Implement a key rotation schedule:

```rust
pub fn schedule_key_rotation(
    key_manager: &KeyManager,
    federation_id: &FederationId,
    key_types: &[KeyType],
    rotation_interval: Duration,
) -> Result<(), SecurityError> {
    // Create rotation schedule
    let schedule = KeyRotationSchedule {
        federation_id: federation_id.clone(),
        key_types: key_types.to_vec(),
        interval: rotation_interval,
        next_rotation: DateTime::now_utc() + rotation_interval,
    };
    
    // Register the schedule
    key_manager.register_rotation_schedule(schedule)?;
    
    // Start the rotation service
    key_manager.start_rotation_service()?;
    
    Ok(())
}
```

### Security Monitoring

Set up security monitoring:

```rust
pub fn configure_security_monitoring(
    security_monitor: &SecurityMonitor,
    federation_id: &FederationId,
    alert_endpoints: &[AlertEndpoint],
) -> Result<(), SecurityError> {
    // Configure anomaly detection
    security_monitor.configure_anomaly_detection(
        federation_id,
        AnomalyDetectionConfig::default(),
    )?;
    
    // Set up alerts
    for endpoint in alert_endpoints {
        security_monitor.register_alert_endpoint(
            federation_id,
            endpoint.clone(),
        )?;
    }
    
    // Enable threat detection
    security_monitor.enable_threat_detection(
        federation_id,
        &[
            ThreatType::DoubleSpendAttempt,
            ThreatType::UnauthorizedAccess,
            ThreatType::AbnormalVotingPattern,
            ThreatType::ResourceExhaustion,
        ],
    )?;
    
    // Start monitoring service
    security_monitor.start_monitoring(federation_id)?;
    
    Ok(())
}
```

For more detailed security documentation, refer to [SECURITY.md](docs/SECURITY.md).

## 10. Sample Flows

### End-to-End Cooperative Onboarding

This flow demonstrates onboarding a new cooperative to a federation:

```rust
// Step 1: Create cooperative credentials
let cooperative_did = wallet.create_identity("New Cooperative")?;

// Step 2: Federation issues membership credential
let membership_credential = federation_wallet.create_credential(
    &federation_did,
    &cooperative_did,
    CredentialType::FederationMember,
    &[
        ("name", "New Cooperative"),
        ("type", "Producer Cooperative"),
        ("joined_at", DateTime::now_utc()),
    ],
    Some(DateTime::now_utc() + Duration::days(365)),
)?;

// Step 3: Create onboarding proposal
let proposal = Proposal {
    title: "Onboard New Cooperative".to_string(),
    description: "Add New Cooperative to the federation.".to_string(),
    proposal_type: ProposalType::CooperativeOnboarding {
        cooperative_did: cooperative_did.clone(),
        cooperative_details: coop_details,
    },
    scope: GovernanceScope::Federation(federation_id.clone()),
    // other fields omitted for brevity
};

let proposal_id = submit_governance_proposal(&runtime_client, proposal, &federation_keypair)?;

// Step 4: Voting on proposal
// ... (voting process)

// Step 5: Execute the proposal (post-approval)
let execution_receipt = runtime_client.execute_proposal(
    &proposal_id,
    &executor_keypair,
)?;

// Step 6: Set up cooperative node
// ... (node setup process)

// Step 7: Join federation network
let join_receipt = cooperative_runtime.join_federation(
    &federation_id,
    &membership_credential,
    &cooperative_keypair,
)?;

// Step 8: Verify cooperative inclusion in next anchor
let next_anchor = runtime_client.wait_for_next_anchor(
    &federation_id,
    Duration::from_secs(300),
)?;

assert!(next_anchor.includes_operation(&join_receipt.operation_id));
```

### Governance Proposal Lifecycle

This flow demonstrates a complete governance proposal lifecycle:

```rust
// Step 1: Create a proposal
let proposal = Proposal {
    title: "Update Resource Allocation Policy".to_string(),
    description: "Increase storage limits for members.".to_string(),
    proposal_type: ProposalType::PolicyUpdate {
        policy_id: "resource-allocation-policy".into(),
        update_type: PolicyUpdateType::ParameterUpdate,
        new_policy_text: r#"{"max_storage_per_member": 100000}"#.to_string(),
        rationale: "Growing storage needs of members".to_string(),
    },
    scope: GovernanceScope::Federation(federation_id.clone()),
    deliberation_period: Duration::days(7),
    voting_period: Duration::days(3),
    quorum_rules: QuorumRules::default(),
    // other fields omitted for brevity
};

let proposal_id = submit_governance_proposal(&runtime_client, proposal, &proposer_keypair)?;

// Step 2: Create deliberation thread in AgoraNet
let thread = Thread {
    title: "Deliberation: Update Resource Allocation Policy".to_string(),
    description: "Discuss the proposal to increase storage limits.".to_string(),
    category: "governance".to_string(),
    // other fields omitted for brevity
};

let thread_id = agoranet_client.create_thread(thread, &proposer_keypair).await?;

// Link the thread to the proposal
link_proposal_to_thread(
    &runtime_client,
    &agoranet_client,
    &proposal_id,
    &thread_id,
    &proposer_keypair,
).await?;

// Step 3: Deliberation (post messages to the thread)
agoranet_client.post_message(
    Message {
        thread_id: thread_id.clone(),
        content: "I support this proposal because...".to_string(),
        // other fields omitted for brevity
    },
    &participant_keypair,
).await?;

// Step 4: Vote on the proposal (after deliberation period)
let vote = Vote {
    proposal_id: proposal_id.clone(),
    choice: VoteChoice::Yes,
    rationale: Some("Needed for growing storage requirements".into()),
    // other fields omitted for brevity
};

runtime_client.cast_vote(vote, &voter_keypair)?;

// Step 5: Execute the proposal (after voting period)
let execution_receipt = runtime_client.execute_proposal(
    &proposal_id,
    &executor_keypair,
)?;

// Step 6: Verify the policy update
let updated_policy = runtime_client.get_policy(&"resource-allocation-policy".into())?;
assert_eq!(
    updated_policy.parameters.get("max_storage_per_member"),
    Some(&Value::Number(100000.into()))
);
```

### Token Issuance and Resource Metering Example

This flow demonstrates token issuance and resource metering:

```rust
// Step 1: Create token issuance proposal
let proposal = Proposal {
    title: "Issue Compute Tokens".to_string(),
    description: "Issue compute tokens to members for Q3 2023.".to_string(),
    proposal_type: ProposalType::TokenIssuance {
        token_type: TokenType::ResourceToken(ResourceType::Compute {
            cpu_time_ms: 0, // Template value
            memory_bytes: 0, // Template value
        }),
        quantity: 10000,
        recipients: member_list.iter().map(|m| TokenRecipient {
            did: m.did.clone(),
            quantity: 10000 / member_list.len() as u64,
        }).collect(),
        conditions: vec![],
        purpose: "Q3 2023 compute resource allocation".to_string(),
    },
    scope: GovernanceScope::Federation(federation_id.clone()),
    // other fields omitted for brevity
};

let proposal_id = submit_governance_proposal(&runtime_client, proposal, &proposer_keypair)?;

// ... (voting process)

// Step 2: Execute token issuance (after approval)
let execution_receipt = runtime_client.execute_proposal(
    &proposal_id,
    &executor_keypair,
)?;

// Step 3: Member uses tokens for computation
let compute_operation = ResourceUsageOperation {
    resource_type: ResourceType::Compute {
        cpu_time_ms: 100,
        memory_bytes: 1024 * 1024,
    },
    quantity: ResourceQuantity::new(100), // Using 100 units
    context: HashMap::new(),
    // other fields omitted for brevity
};

let usage_receipt = runtime_client.perform_resource_operation(
    compute_operation,
    &member_keypair,
)?;

// Step 4: Verify remaining balance
let token_balance = runtime_client.get_token_balance(
    &member_did,
    TokenType::ResourceToken(ResourceType::Compute {
        cpu_time_ms: 0,
        memory_bytes: 0,
    }),
)?;

println!("Remaining compute balance: {}", token_balance);

// Step 5: Resource metering report
let usage_report = runtime_client.get_resource_usage_report(
    &member_did,
    &ResourceType::Compute {
        cpu_time_ms: 0,
        memory_bytes: 0,
    },
    &TimeRange::last_30_days(),
)?;

println!("Total compute usage: {} units", usage_report.total_usage);
```

## Conclusion

This integration guide provides the foundational knowledge needed to work with the ICN stack. For more detailed information on specific components, refer to the related documentation linked throughout this guide.

Remember that the ICN system is designed for federation-based cooperative governance, so integration should align with principles of transparency, verifiability, and collaborative decision-making.

For additional support, contact the ICN development team or join the community discussion at https://community.intercooperative.org. 