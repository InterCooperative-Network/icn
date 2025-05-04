# ICN Trust Model

## Introduction

The Intercooperative Network (ICN) Trust Model defines how entities establish, maintain, and verify trust relationships in a federated context. The model is designed to support cooperative governance while balancing the need for federation autonomy with cross-federation verification and coordination.

> **Related Documentation:**
> - [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Overall system architecture
> - [DAG_STRUCTURE.md](docs/DAG_STRUCTURE.md) - DAG implementation details
> - [FEDERATION_BOOTSTRAP.md](docs/FEDERATION_BOOTSTRAP.md) - Federation initialization

## Core Principles

The ICN Trust Model is built on several foundational principles:

1. **Federation Autonomy**: Each federation maintains sovereign control over its trust policies
2. **Verifiable Delegation**: Trust delegation follows clear, cryptographically verifiable paths
3. **Selective Disclosure**: Participants control what information they share while maintaining verifiability
4. **Graceful Evolution**: Trust relationships can evolve without breaking historical verification
5. **Cryptographic Roots**: All trust relationships are anchored in cryptographic primitives
6. **Cooperative Governance**: Trust policies themselves are governed through transparent processes

## Trust Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Federation                        │
├─────────────────────────────────────────────────────┤
│                                                     │
│  ┌─────────────┐     ┌─────────────┐                │
│  │  TrustBundle │────►   Guardian  │                │
│  │             │     │  Committee  │                │
│  └──────┬──────┘     └─────────────┘                │
│         │                                           │
│         │                                           │
│  ┌──────▼──────┐     ┌─────────────┐                │
│  │   Trusted   │────►  Credential  │                │
│  │   Issuers   │     │   Registry  │                │
│  └──────┬──────┘     └─────────────┘                │
│         │                                           │
│         │                                           │
│  ┌──────▼──────┐     ┌─────────────┐                │
│  │ Verification│────► ZK Circuit   │                │
│  │    Keys     │     │  Registry   │                │
│  └─────────────┘     └─────────────┘                │
│                                                     │
└─────────────────────────────────────────────────────┘
```

## TrustBundles

TrustBundles are the primary mechanism for expressing, managing, and versioning trust in the ICN system.

### Definition and Structure

```rust
pub struct TrustBundle {
    // Bundle identifier (includes version)
    pub id: String,
    
    // Federation that issued this bundle
    pub issuer_federation: FederationId,
    
    // Valid time range
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    
    // Trusted issuer DIDs and their roles
    pub trusted_issuers: Vec<TrustedIssuer>,
    
    // Verification keys for signatures
    pub verification_keys: Vec<VerificationKey>,
    
    // Cross-federation trust relationships
    pub trusted_federations: Vec<FederationTrust>,
    
    // ZK circuit registry
    pub zk_circuits: Vec<ZkCircuitRegistry>,
    
    // Trust policy rules
    pub policy_rules: Vec<PolicyRule>,
    
    // Federation signature (quorum-based)
    pub signature: FederationSignature,
    
    // Previous bundle reference (for chaining)
    pub previous_bundle: Option<String>,
}
```

### Trusted Issuer Definition

```rust
pub struct TrustedIssuer {
    // DID of the trusted entity
    pub did: String,
    
    // Verification methods (keys)
    pub verification_methods: Vec<VerificationMethod>,
    
    // Authorized roles
    pub roles: Vec<Role>,
    
    // Delegation constraints
    pub delegation_constraints: Option<DelegationConstraints>,
    
    // Valid time period
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    
    // Revocation status
    pub revocation_status: RevocationStatus,
}
```

### Federation Trust Definition

```rust
pub struct FederationTrust {
    // Federation identifier
    pub federation_id: FederationId,
    
    // Trust level (full, partial, restricted)
    pub trust_level: TrustLevel,
    
    // Trusted operations from this federation
    pub trusted_operations: Vec<OperationType>,
    
    // Valid time period
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    
    // Verification requirements
    pub verification_requirements: VerificationRequirements,
}
```

## Trust Bundle Lifecycle

### 1. Creation and Bootstrap

TrustBundles are initially created during federation bootstrap:

```rust
pub fn create_initial_trust_bundle(
    federation_id: &FederationId,
    founding_members: &[Member],
    governance_policy: &GovernancePolicy,
) -> Result<TrustBundle> {
    // Generate a bundle ID
    let bundle_id = format!("{}:bundle:v1", federation_id);
    
    // Collect trusted issuers from founding members
    let trusted_issuers = founding_members
        .iter()
        .map(|member| TrustedIssuer {
            did: member.did.clone(),
            verification_methods: member.verification_methods.clone(),
            roles: member.roles.clone(),
            delegation_constraints: None,
            valid_from: DateTime::now_utc(),
            valid_until: None,
            revocation_status: RevocationStatus::Active,
        })
        .collect();
    
    // Create the initial bundle
    let bundle = TrustBundle {
        id: bundle_id,
        issuer_federation: federation_id.clone(),
        valid_from: DateTime::now_utc(),
        valid_until: None,
        trusted_issuers,
        verification_keys: collect_verification_keys(founding_members),
        trusted_federations: Vec::new(), // No initial cross-federation trust
        zk_circuits: governance_policy.initial_zk_circuits.clone(),
        policy_rules: governance_policy.initial_policy_rules.clone(),
        signature: FederationSignature::empty(), // Will be signed below
        previous_bundle: None,
    };
    
    // Collect signatures from founding members
    let signatures = collect_founding_signatures(&bundle, founding_members)?;
    
    // Add quorum signature
    let mut signed_bundle = bundle;
    signed_bundle.signature = FederationSignature::new(
        signatures,
        governance_policy.quorum_threshold,
    );
    
    Ok(signed_bundle)
}
```

### 2. Version Updates

TrustBundles evolve through federation governance:

```rust
pub fn create_updated_trust_bundle(
    previous_bundle: &TrustBundle,
    changes: &BundleChanges,
    signers: &[Member],
    governance_policy: &GovernancePolicy,
) -> Result<TrustBundle> {
    // Generate a new bundle ID with incremented version
    let new_version = extract_version(&previous_bundle.id) + 1;
    let bundle_id = format!("{}:bundle:v{}", previous_bundle.issuer_federation, new_version);
    
    // Apply changes to the previous bundle
    let mut trusted_issuers = previous_bundle.trusted_issuers.clone();
    apply_issuer_changes(&mut trusted_issuers, &changes.issuer_changes)?;
    
    let mut verification_keys = previous_bundle.verification_keys.clone();
    apply_key_changes(&mut verification_keys, &changes.key_changes)?;
    
    let mut trusted_federations = previous_bundle.trusted_federations.clone();
    apply_federation_changes(&mut trusted_federations, &changes.federation_changes)?;
    
    let mut zk_circuits = previous_bundle.zk_circuits.clone();
    apply_circuit_changes(&mut zk_circuits, &changes.circuit_changes)?;
    
    let mut policy_rules = previous_bundle.policy_rules.clone();
    apply_policy_changes(&mut policy_rules, &changes.policy_changes)?;
    
    // Create the updated bundle
    let bundle = TrustBundle {
        id: bundle_id,
        issuer_federation: previous_bundle.issuer_federation.clone(),
        valid_from: DateTime::now_utc(),
        valid_until: None,
        trusted_issuers,
        verification_keys,
        trusted_federations,
        zk_circuits,
        policy_rules,
        signature: FederationSignature::empty(), // Will be signed below
        previous_bundle: Some(previous_bundle.id.clone()),
    };
    
    // Collect signatures from authorized signers
    let signatures = collect_signatures(&bundle, signers)?;
    
    // Add quorum signature
    let mut signed_bundle = bundle;
    signed_bundle.signature = FederationSignature::new(
        signatures,
        governance_policy.quorum_threshold,
    );
    
    Ok(signed_bundle)
}
```

### 3. Publication and Distribution

```rust
pub fn publish_trust_bundle(bundle: &TrustBundle) -> Result<()> {
    // Add bundle to local DAG
    dag_manager.add_bundle(bundle)?;
    
    // Create anchor referencing the bundle
    let anchor = create_anchor_with_bundle_reference(bundle)?;
    dag_manager.add_anchor(&anchor)?;
    
    // Publish to IPFS for wider availability
    let bundle_cid = ipfs_client.put(bundle)?;
    
    // Notify connected federation nodes
    broadcast_bundle_update(bundle, &bundle_cid)?;
    
    // Store in federation registry
    registry.update_active_bundle(bundle)?;
    
    Ok(())
}
```

### 4. Verification and Usage

```rust
pub fn verify_with_trust_bundle(
    operation: &Operation,
    bundle: &TrustBundle,
) -> Result<VerificationResult> {
    // Verify bundle signature
    verify_bundle_signature(bundle)?;
    
    // Verify bundle is not expired
    if is_bundle_expired(bundle) {
        return Err(VerificationError::ExpiredBundle);
    }
    
    // Verify operation issuer is trusted
    let issuer = operation.issuer();
    let trusted_issuer = find_trusted_issuer(issuer, bundle)?;
    
    // Verify operation signature using issuer keys
    verify_operation_signature(operation, trusted_issuer)?;
    
    // Verify issuer has required roles for operation
    verify_issuer_roles(operation, trusted_issuer)?;
    
    // Verify against policy rules
    verify_policy_compliance(operation, bundle.policy_rules.as_slice())?;
    
    // If operation contains ZK proofs, verify them
    if let Some(zk_proofs) = operation.zk_proofs() {
        verify_zk_proofs(zk_proofs, bundle)?;
    }
    
    Ok(VerificationResult::Valid)
}
```

## Federation-Specific Delegation

### Delegation Model

The ICN implements a constrained delegation model that allows entities to delegate specific authorities while maintaining federation governance control.

```rust
pub struct DelegationCredential {
    // Issuer DID (the delegator)
    pub issuer: String,
    
    // Subject DID (the delegate)
    pub subject: String,
    
    // Delegated capabilities
    pub capabilities: Vec<Capability>,
    
    // Constraints on delegation usage
    pub constraints: DelegationConstraints,
    
    // Credential metadata
    pub issuance_date: DateTime<Utc>,
    pub expiration_date: Option<DateTime<Utc>>,
    pub credential_id: String,
    
    // Proof of issuance
    pub proof: Proof,
}
```

### Delegation Constraints

```rust
pub struct DelegationConstraints {
    // Time-based constraints
    pub time_constraints: Option<TimeConstraints>,
    
    // Operation count constraints
    pub count_constraints: Option<CountConstraints>,
    
    // Network/location constraints
    pub network_constraints: Option<NetworkConstraints>,
    
    // Sub-delegation permissions
    pub sub_delegation: SubDelegationPolicy,
    
    // Required attestations
    pub required_attestations: Vec<RequiredAttestation>,
}
```

### Delegation Verification

```rust
pub fn verify_delegation_chain(
    operation: &Operation,
    delegation_chain: &[DelegationCredential],
    trust_bundle: &TrustBundle,
) -> Result<(), VerificationError> {
    // Verify chain is not empty
    if delegation_chain.is_empty() {
        return Err(VerificationError::EmptyDelegationChain);
    }
    
    // Verify the root delegator is trusted in the bundle
    let root_delegator = &delegation_chain[0].issuer;
    let root_trusted_issuer = find_trusted_issuer(root_delegator, trust_bundle)?;
    
    // Verify each link in the delegation chain
    let mut current_delegator = root_delegator;
    for credential in delegation_chain {
        // Verify credential signature
        verify_credential_signature(credential)?;
        
        // Verify correct delegator
        if credential.issuer != *current_delegator {
            return Err(VerificationError::InvalidDelegationChain);
        }
        
        // Verify credential is not expired
        if is_credential_expired(credential) {
            return Err(VerificationError::ExpiredDelegation);
        }
        
        // Verify delegation constraints
        verify_delegation_constraints(credential, operation)?;
        
        // Verify sub-delegation is allowed
        if credential.constraints.sub_delegation == SubDelegationPolicy::NotAllowed 
           && credential != delegation_chain.last().unwrap() {
            return Err(VerificationError::SubDelegationNotAllowed);
        }
        
        // Update current delegator for next iteration
        current_delegator = &credential.subject;
    }
    
    // Verify the operation issuer matches the final delegate
    let final_delegate = &delegation_chain.last().unwrap().subject;
    if operation.issuer() != *final_delegate {
        return Err(VerificationError::DelegationSubjectMismatch);
    }
    
    // Verify the delegated capabilities include the operation type
    let final_credential = delegation_chain.last().unwrap();
    if !has_capability_for_operation(&final_credential.capabilities, operation) {
        return Err(VerificationError::InsufficientDelegatedCapability);
    }
    
    Ok(())
}
```

## Guardian System

The Guardian system provides an optional governance safety mechanism for federations that choose to implement it.

### Guardian Roles and Responsibilities

Guardians are designated entities with special oversight responsibilities:

1. **Policy Oversight**: Review and approve major policy changes
2. **Emergency Response**: Respond to security incidents and governance crises
3. **Cross-Federation Coordination**: Facilitate trusted interactions with other federations
4. **Dispute Resolution**: Provide final arbitration for unresolved disputes
5. **Key Recovery**: Participate in key recovery ceremonies

### Guardian Committee Structure

```rust
pub struct GuardianCommittee {
    // Committee identifier
    pub id: String,
    
    // Guardian members
    pub members: Vec<Guardian>,
    
    // Committee configuration
    pub config: GuardianConfig,
    
    // Active mandates
    pub active_mandates: Vec<Mandate>,
    
    // Committee signature
    pub signature: CommitteeSignature,
}
```

### Guardian Definition

```rust
pub struct Guardian {
    // Guardian DID
    pub did: String,
    
    // Guardian metadata
    pub name: Option<String>,
    pub description: Option<String>,
    
    // Verification methods
    pub verification_methods: Vec<VerificationMethod>,
    
    // Guardian roles (can be a subset of all guardian roles)
    pub roles: Vec<GuardianRole>,
    
    // Appointment credentials
    pub appointment_credential: Credential,
}
```

### Mandate Types

Guardians operate through formally defined mandates:

1. **Oversight Mandate**: Authority to review and approve changes to trust policies
2. **Emergency Mandate**: Authority to take immediate action in response to critical security issues
3. **Recovery Mandate**: Authority to participate in key recovery or state recovery processes
4. **Dispute Mandate**: Authority to resolve disputes through formal arbitration

```rust
pub struct Mandate {
    // Mandate identifier
    pub id: String,
    
    // Mandate type
    pub mandate_type: MandateType,
    
    // Scope of authority
    pub scope: MandateScope,
    
    // Valid time period
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    
    // Authorized actions
    pub authorized_actions: Vec<AuthorizedAction>,
    
    // Required quorum
    pub required_quorum: u32,
    
    // Issuance proof (federation approval)
    pub issuance_proof: FederationSignature,
}
```

### Mandate Execution

```rust
pub fn execute_guardian_action(
    action: &GuardianAction,
    committee: &GuardianCommittee,
    mandate: &Mandate,
    signatures: &[GuardianSignature],
) -> Result<ActionReceipt> {
    // Verify mandate is valid
    verify_mandate_validity(mandate)?;
    
    // Verify action is authorized under mandate
    verify_action_authorization(action, mandate)?;
    
    // Verify guardian signatures
    let signing_guardians = verify_guardian_signatures(
        action,
        signatures,
        committee
    )?;
    
    // Verify quorum is met
    verify_guardian_quorum(
        signing_guardians,
        mandate.required_quorum,
        committee
    )?;
    
    // Execute the action based on type
    let result = match action.action_type {
        GuardianActionType::ApproveTrustUpdate => {
            execute_trust_update_approval(action, committee)?
        },
        GuardianActionType::EmergencyAction => {
            execute_emergency_action(action, committee)?
        },
        GuardianActionType::DisputeResolution => {
            execute_dispute_resolution(action, committee)?
        },
        GuardianActionType::RecoveryAction => {
            execute_recovery_action(action, committee)?
        },
    };
    
    // Create and return receipt
    let receipt = ActionReceipt {
        action_id: action.id.clone(),
        execution_time: DateTime::now_utc(),
        result,
        signatures: collect_execution_signatures(result, signing_guardians)?,
    };
    
    Ok(receipt)
}
```

## Trust Versioning, Revocation, and Inheritance

### Trust Bundle Versioning

Trust Bundles are versioned to allow for evolution while maintaining verification of historical operations:

1. **Sequenced Versions**: Each bundle includes its version and a reference to the previous bundle
2. **Non-Repudiation**: Once published, a bundle version cannot be modified (only superseded)
3. **Temporal Validity**: Each bundle includes an explicit validity period
4. **Transitional Overlaps**: New versions can have an overlap period with previous versions
5. **Version Discovery**: Clients can discover the latest version through federation anchors

### Revocation Mechanisms

The ICN supports multiple revocation mechanisms:

1. **Bundle Invalidation**: Complete invalidation of a trust bundle (rare, for compromise scenarios)
2. **Issuer Revocation**: Removal of a trusted issuer from the bundle
3. **Key Revocation**: Revocation of specific verification keys
4. **Credential Revocation**: Revocation of specific credentials
5. **Status List**: Efficient revocation checking through status lists

```rust
pub enum RevocationMethod {
    // Immediate removal from trust bundle
    ImmediateRemoval,
    
    // Addition to a revocation list
    RevocationList {
        list_id: String,
        entry_index: u64,
    },
    
    // Status list credential
    StatusList2021 {
        status_list_credential: String,
        status_index: u64,
    },
    
    // On-chain revocation registry
    BlockchainRegistry {
        registry_address: String,
        revocation_id: String,
    },
}
```

### Inheritance Models

Trust can be inherited through several patterns:

1. **Explicit Delegation**: Direct delegation of authority from one entity to another
2. **Role-Based Inheritance**: Trust in role-holders rather than specific entities
3. **Federation Recognition**: Recognition of another federation's trust decisions
4. **Transitive Trust**: Trust in entities trusted by already-trusted entities

```rust
pub enum TrustInheritanceModel {
    // Direct trust in specific DIDs
    DirectTrust,
    
    // Trust based on role credentials
    RoleBased {
        accepted_roles: Vec<Role>,
        accepted_issuers: Vec<String>,
    },
    
    // Trust in another federation's decisions
    FederationBased {
        trusted_federation: FederationId,
        required_endorsements: u32,
    },
    
    // Trust in entities trusted by entities we trust
    TransitiveTrust {
        max_depth: u32,
        required_path_length: u32,
    },
}
```

## Integration with DIDs, VCs, and ZK Proofs

### DID Integration

DIDs (Decentralized Identifiers) serve as the foundation for entity identification in the ICN:

1. **Method Agnostic**: ICN supports multiple DID methods (did:key, did:web, did:peer, etc.)
2. **Resolution Pipeline**: Standardized resolution process for all supported methods
3. **Verification Method Extraction**: Consistent extraction of verification methods
4. **Key Management**: Secure management of DID control keys
5. **Service Endpoints**: Discovery of entity services through DID documents

```rust
pub fn resolve_and_verify_did(
    did: &str,
    trust_bundle: &TrustBundle,
) -> Result<VerifiedDid> {
    // Resolve DID to a DID Document
    let (did_doc, metadata) = did_resolver.resolve(did)?;
    
    // Verify DID Document
    verify_did_document(&did_doc, &metadata)?;
    
    // Extract verification methods
    let verification_methods = extract_verification_methods(&did_doc);
    
    // Check if the DID is trusted in the bundle
    let is_trusted = is_did_trusted(did, trust_bundle);
    
    // Extract services
    let services = extract_services(&did_doc);
    
    Ok(VerifiedDid {
        did: did.to_string(),
        document: did_doc,
        verification_methods,
        services,
        is_trusted,
        trust_path: derive_trust_path(did, trust_bundle),
    })
}
```

### Verifiable Credentials Integration

Verifiable Credentials (VCs) provide claims about entities with cryptographic verifiability:

1. **Credential Registry**: Registry of recognized credential types and schemas
2. **Issuance Constraints**: Rules governing who can issue specific credential types
3. **Verification**: Standards-compliant credential verification process
4. **Selective Disclosure**: Support for partial credential disclosure
5. **Credential Formats**: Support for JWT, JSON-LD, and SummonVC formats

```rust
pub fn verify_credential(
    credential: &Credential,
    trust_bundle: &TrustBundle,
    verification_options: &VerificationOptions,
) -> Result<VerificationResult> {
    // Verify credential structure
    verify_credential_structure(credential)?;
    
    // Resolve issuer DID
    let issuer = resolve_and_verify_did(&credential.issuer, trust_bundle)?;
    
    // Check if issuer is trusted in bundle
    if verification_options.require_trusted_issuer && !issuer.is_trusted {
        return Err(VerificationError::UntrustedIssuer);
    }
    
    // Verify credential signature
    verify_credential_signature(credential, &issuer)?;
    
    // Check credential status (revocation)
    if verification_options.check_revocation {
        check_credential_status(credential)?;
    }
    
    // Verify issuance policy compliance
    if verification_options.check_issuance_policy {
        verify_issuance_policy_compliance(credential, trust_bundle)?;
    }
    
    // Verify credential schema compliance
    verify_credential_schema(credential)?;
    
    Ok(VerificationResult::Valid)
}
```

### Zero-Knowledge Proof Integration

ZK Proofs enable privacy-preserving verification:

1. **Circuit Registry**: Registry of approved ZK circuits and verifier keys
2. **Proof Generation**: Standard API for generating ZK proofs
3. **Verification**: Efficient proof verification process
4. **Circuit Governance**: Governance process for adding or updating circuits
5. **Multi-Proof Composition**: Support for composing multiple proofs

```rust
pub fn verify_zk_proof(
    proof: &ZkProof,
    public_inputs: &[String],
    trust_bundle: &TrustBundle,
) -> Result<bool> {
    // Find the circuit information in the trust bundle
    let circuit = find_circuit_in_bundle(&proof.circuit_id, trust_bundle)?;
    
    // Get the verification key
    let verification_key = &circuit.verification_key;
    
    // Verify the proof using the verification key
    let is_valid = zk_verifier.verify(
        &proof.proof,
        verification_key,
        public_inputs
    )?;
    
    // Verify the proof was issued by an authorized issuer
    if is_valid && !circuit.authorized_issuers.is_empty() {
        let is_authorized = circuit.authorized_issuers.contains(&proof.issuer);
        if !is_authorized {
            return Err(VerificationError::UnauthorizedProofIssuer);
        }
    }
    
    Ok(is_valid)
}
```

## Federation Merging, Splitting, and Trust Continuity

### Federation Merging

Federations can merge through a formal process that preserves trust relationships:

```rust
pub fn initiate_federation_merge(
    federation_a: &Federation,
    federation_b: &Federation,
    merge_proposal: &MergeProposal,
) -> Result<MergeProcess> {
    // Verify both federations have approved the merge
    verify_federation_approval(federation_a, &merge_proposal.approval_a)?;
    verify_federation_approval(federation_b, &merge_proposal.approval_b)?;
    
    // Create trust mapping between the federations
    let trust_mapping = create_trust_mapping(
        &federation_a.active_trust_bundle,
        &federation_b.active_trust_bundle,
        &merge_proposal.trust_mapping_rules
    )?;
    
    // Create merged governance policy
    let merged_policy = create_merged_governance_policy(
        &federation_a.governance_policy,
        &federation_b.governance_policy,
        &merge_proposal.governance_merger_rules
    )?;
    
    // Create the initial merged trust bundle
    let merged_bundle = create_merged_trust_bundle(
        &federation_a.active_trust_bundle,
        &federation_b.active_trust_bundle,
        &merge_proposal.new_federation_id,
        &trust_mapping,
        &merged_policy
    )?;
    
    // Create merge process
    let merge_process = MergeProcess {
        id: generate_uuid(),
        federation_a_id: federation_a.id.clone(),
        federation_b_id: federation_b.id.clone(),
        new_federation_id: merge_proposal.new_federation_id.clone(),
        merge_proposal: merge_proposal.clone(),
        trust_mapping,
        merged_policy,
        merged_bundle,
        status: MergeStatus::Initiated,
        start_time: DateTime::now_utc(),
        completion_time: None,
    };
    
    Ok(merge_process)
}
```

### Federation Splitting

Federations can split while maintaining trust in both resulting federations:

```rust
pub fn initiate_federation_split(
    original_federation: &Federation,
    split_proposal: &SplitProposal,
) -> Result<SplitProcess> {
    // Verify federation has approved the split
    verify_federation_approval(original_federation, &split_proposal.approval)?;
    
    // Create trust mappings for the two new federations
    let trust_mapping_a = create_split_trust_mapping(
        &original_federation.active_trust_bundle,
        &split_proposal.federation_a_members,
        &split_proposal.trust_mapping_rules_a
    )?;
    
    let trust_mapping_b = create_split_trust_mapping(
        &original_federation.active_trust_bundle,
        &split_proposal.federation_b_members,
        &split_proposal.trust_mapping_rules_b
    )?;
    
    // Create governance policies for both new federations
    let policy_a = create_split_governance_policy(
        &original_federation.governance_policy,
        &split_proposal.governance_rules_a
    )?;
    
    let policy_b = create_split_governance_policy(
        &original_federation.governance_policy,
        &split_proposal.governance_rules_b
    )?;
    
    // Create initial trust bundles for both new federations
    let bundle_a = create_split_trust_bundle(
        &original_federation.active_trust_bundle,
        &split_proposal.federation_a_id,
        &trust_mapping_a,
        &policy_a
    )?;
    
    let bundle_b = create_split_trust_bundle(
        &original_federation.active_trust_bundle,
        &split_proposal.federation_b_id,
        &trust_mapping_b,
        &policy_b
    )?;
    
    // Create split process
    let split_process = SplitProcess {
        id: generate_uuid(),
        original_federation_id: original_federation.id.clone(),
        federation_a_id: split_proposal.federation_a_id.clone(),
        federation_b_id: split_proposal.federation_b_id.clone(),
        split_proposal: split_proposal.clone(),
        trust_mapping_a,
        trust_mapping_b,
        policy_a,
        policy_b,
        bundle_a,
        bundle_b,
        status: SplitStatus::Initiated,
        start_time: DateTime::now_utc(),
        completion_time: None,
    };
    
    Ok(split_process)
}
```

### Trust Continuity

The ICN ensures trust continuity across federation lifecycle events:

1. **Anchored Trust History**: Historical trust relationships are preserved in the DAG
2. **Cross-Federation Verification**: Operations from previous federations remain verifiable
3. **Credential Continuity**: Credentials issued by previous federations remain valid
4. **Trust Bridge Bundles**: Special bundles that map between old and new trust structures
5. **Historical Bundle Verification**: Ability to verify using historical trust bundles

```rust
pub fn verify_historical_operation(
    operation: &Operation,
    current_trust_bundle: &TrustBundle,
    timestamp: DateTime<Utc>,
) -> Result<VerificationResult> {
    // Find the trust bundle active at the given timestamp
    let historical_bundle = trust_archive.find_bundle_at_time(
        &current_trust_bundle.issuer_federation,
        timestamp
    )?;
    
    // If federation has changed (merge/split), find the relevant bridge bundle
    let effective_bundle = if historical_bundle.issuer_federation != current_trust_bundle.issuer_federation {
        let bridge_bundle = trust_archive.find_bridge_bundle(
            &historical_bundle.issuer_federation,
            &current_trust_bundle.issuer_federation
        )?;
        
        // Apply bridge mappings to the historical bundle
        apply_bridge_mappings(&historical_bundle, &bridge_bundle)?
    } else {
        historical_bundle
    };
    
    // Verify the operation using the effective bundle
    verify_with_trust_bundle(operation, &effective_bundle)
}
```

## Security Considerations

### Compromise Recovery

The ICN includes mechanisms for recovering from key compromises:

1. **Key Rotation**: Standard procedure for routine key rotation
2. **Emergency Revocation**: Fast-path revocation for compromised keys
3. **Recovery Keys**: Backup keys stored in secure escrow
4. **Social Recovery**: Threshold-based recovery using guardian keys
5. **State Rollback**: Ability to roll back to pre-compromise state

### Trust Anchor Security

Trust anchors are protected through multiple security measures:

1. **Quorum Signatures**: Multiple signatures required for trust changes
2. **Key Separation**: Separation of operational and trust management keys
3. **Physical Security**: Critical keys stored in secure hardware
4. **Ceremony Documentation**: Formal ceremony protocols for trust operations
5. **Transparency Logs**: Public logs of all trust anchor changes

### Denial of Service Mitigation

The trust system includes protections against denial of service:

1. **Caching**: Efficient caching of trust verification results
2. **Local Verification**: Most verification can occur locally without network calls
3. **Compact Proofs**: Efficient proof formats to minimize bandwidth
4. **Rate Limiting**: Protection against verification request floods
5. **Alternative Paths**: Multiple paths for trust verification

## References

- [ICN Architecture](docs/ARCHITECTURE.md) - Overall system architecture
- [DAG Structure](docs/DAG_STRUCTURE.md) - DAG implementation details
- [W3C DID Specification](https://www.w3.org/TR/did-core/) - DID standard
- [W3C VC Data Model](https://www.w3.org/TR/vc-data-model/) - Verifiable Credentials standard
- [ZKProof Standards](https://zkproof.org/papers/) - ZK Proof standards

## Glossary

| Term | Definition |
|------|------------|
| **Capability** | A specific permission granted through delegation |
| **Delegation** | The process of granting authority from one entity to another |
| **Federation** | A collection of entities operating under shared governance rules |
| **Guardian** | A special role with oversight responsibilities in a federation |
| **Mandate** | A formal authorization for guardians to act in specific ways |
| **Quorum** | The minimum number of participants required to make a valid decision |
| **Revocation** | The process of invalidating previously granted trust |
| **Trust Bundle** | A signed collection of trusted issuers, verification keys, and policies |
| **Trust Path** | The chain of trust relationships from a trust root to an entity |
| **TrustRoot** | The foundational entities that are inherently trusted in a system |
| **Verification Key** | A cryptographic key used to verify signatures or proofs |
| **ZK Circuit** | A program that defines the computation verified by a zero-knowledge proof |

---

*TRUST_MODEL.md v0.1 – May 2025 – ICN Protocol Team* 