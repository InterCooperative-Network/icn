# ICN Economic System Specification

## Introduction

This document specifies the economic layer of the Intercooperative Network (ICN), defining the token system that enables resource metering, scoped incentives, and verifiable execution across federations. The ICN economic system is designed to facilitate cooperative economic activity while preventing speculation, ensuring resource availability, and maintaining cryptographic verifiability.

> **Related Documentation:**
> - [ARCHITECTURE.md](ARCHITECTURE.md) - Overall system architecture
> - [DAG_STRUCTURE.md](DAG_STRUCTURE.md) - DAG implementation details
> - [GOVERNANCE_SYSTEM.md](GOVERNANCE_SYSTEM.md) - Governance mechanisms
> - [TRUST_MODEL.md](TRUST_MODEL.md) - Trust model and federation relationships

## Economic Model Overview

The ICN economic model is built on the principle of *scoped resource tokens* that represent rights to consume specific network resources within defined contexts. Unlike speculative cryptocurrencies, ICN tokens are:

1. **Purpose-bound** - Tokens represent rights to specific resources
2. **Scope-limited** - Tokens are valid only within defined jurisdictions
3. **Governance-controlled** - Token economics are subject to federation governance
4. **Non-speculative** - Designed to facilitate resource allocation, not financial speculation

The economic system serves several key purposes:

```
┌─────────────────────────────────────────────────────────┐
│                Economic System Purposes                 │
├─────────────────────────────────────────────────────────┤
│ • Resource metering and accounting                      │
│ • Preventing resource abuse                             │
│ • Incentivizing contribution                            │
│ • Enabling fair resource allocation                     │
│ • Supporting federation sustainability                  │
│ • Providing cryptographic proof of economic activity    │
└─────────────────────────────────────────────────────────┘
```

### Principles

The ICN economic system follows these core principles:

1. **Resource Relevance**: Tokens directly correspond to measurable resources
2. **Scoped Jurisdiction**: Economic activity is contained within governance boundaries
3. **Governance Enforcement**: Token rules are set and enforced by governance
4. **Contribution Basis**: Token issuance is tied to verifiable contributions
5. **Transparency**: Economic activity is auditable and verifiable
6. **Sustainability**: Economic models must support long-term cooperative viability

## Core Economic Structures

### ResourceType

Resources in the ICN are categorized by type, with each type having specific metering, valuation, and accounting rules:

```rust
pub enum ResourceType {
    // Computational resources
    Compute {
        // CPU time in milliseconds
        cpu_time_ms: u64,
        // Memory allocation in bytes
        memory_bytes: u64,
    },
    
    // Storage resources
    Storage {
        // Storage space in bytes
        space_bytes: u64,
        // Duration of storage in days
        duration_days: u32,
        // Redundancy factor
        redundancy: u8,
    },
    
    // Network bandwidth
    Bandwidth {
        // Data transfer in bytes
        transfer_bytes: u64,
        // Quality of service level
        qos_level: QosLevel,
    },
    
    // Governance participation rights
    Governance {
        // Type of governance action
        action_type: GovernanceActionType,
        // Scope of governance
        scope: GovernanceScope,
    },
    
    // Verification services
    Verification {
        // Type of verification
        verification_type: VerificationType,
        // Complexity metric
        complexity: u32,
    },
    
    // Identity services
    Identity {
        // Type of identity service
        identity_service_type: IdentityServiceType,
        // Number of operations
        operation_count: u32,
    },
    
    // Custom resource type
    Custom {
        // Resource identifier
        resource_id: String,
        // Resource parameters
        parameters: HashMap<String, Value>,
        // Metering WASM module
        metering_wasm: Vec<u8>,
    },
}
```

Each resource type has specific metering functions to quantify usage:

```rust
pub fn meter_resource_usage(
    resource_type: &ResourceType,
    usage_context: &UsageContext,
) -> Result<ResourceQuantity, MeteringError> {
    match resource_type {
        ResourceType::Compute { .. } => {
            meter_compute_usage(usage_context)
        },
        ResourceType::Storage { .. } => {
            meter_storage_usage(usage_context)
        },
        ResourceType::Bandwidth { .. } => {
            meter_bandwidth_usage(usage_context)
        },
        ResourceType::Governance { .. } => {
            meter_governance_usage(usage_context)
        },
        ResourceType::Verification { .. } => {
            meter_verification_usage(usage_context)
        },
        ResourceType::Identity { .. } => {
            meter_identity_usage(usage_context)
        },
        ResourceType::Custom { metering_wasm, .. } => {
            execute_custom_metering(metering_wasm, usage_context)
        },
    }
}
```

### ScopedResourceToken (SRT)

The fundamental unit of the ICN economic system is the Scoped Resource Token (SRT):

```rust
pub struct ScopedResourceToken {
    // Token identifier
    pub id: TokenId,
    
    // Resource this token represents
    pub resource_type: ResourceType,
    
    // Quantity of the resource
    pub quantity: ResourceQuantity,
    
    // Scope in which this token is valid
    pub scope: TokenScope,
    
    // Token metadata
    pub metadata: TokenMetadata,
    
    // Issuance information
    pub issuance: TokenIssuance,
    
    // Constraints on usage
    pub constraints: Vec<TokenConstraint>,
    
    // Expiration policy
    pub expiration: ExpirationPolicy,
    
    // Current status
    pub status: TokenStatus,
    
    // Cryptographic proof
    pub proof: TokenProof,
}
```

Token metadata includes additional information about the token:

```rust
pub struct TokenMetadata {
    // Token name
    pub name: String,
    
    // Token description
    pub description: String,
    
    // Creation timestamp
    pub created_at: DateTime<Utc>,
    
    // Last updated timestamp
    pub updated_at: DateTime<Utc>,
    
    // Tags for categorization
    pub tags: Vec<String>,
    
    // Custom metadata
    pub custom: HashMap<String, Value>,
}
```

### ResourceAuthorization

Resource authorization defines how tokens can be spent:

```rust
pub struct ResourceAuthorization {
    // Authorization identifier
    pub id: AuthorizationId,
    
    // Entity being authorized
    pub authorized_entity: Did,
    
    // Resource being authorized
    pub resource_type: ResourceType,
    
    // Maximum authorized quantity
    pub max_quantity: ResourceQuantity,
    
    // Rate limits
    pub rate_limits: Option<RateLimits>,
    
    // Valid time range
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    
    // Authorization constraints
    pub constraints: Vec<AuthorizationConstraint>,
    
    // Source tokens funding this authorization
    pub source_tokens: Vec<TokenReference>,
    
    // Approval information
    pub approval: AuthorizationApproval,
    
    // Current status
    pub status: AuthorizationStatus,
}
```

### EconomicPolicy

Economic policies govern token behavior:

```rust
pub struct EconomicPolicy {
    // Policy identifier
    pub id: PolicyId,
    
    // Policy name
    pub name: String,
    
    // Policy description
    pub description: String,
    
    // Scope this policy applies to
    pub scope: TokenScope,
    
    // Resource types covered
    pub resource_types: Vec<ResourceType>,
    
    // Minting rules
    pub minting_rules: MintingRules,
    
    // Transfer rules
    pub transfer_rules: TransferRules,
    
    // Consumption rules
    pub consumption_rules: ConsumptionRules,
    
    // Expiration rules
    pub expiration_rules: ExpirationRules,
    
    // Policy enforcement
    pub enforcement: PolicyEnforcement,
    
    // Governance parameters
    pub governance_parameters: GovernanceParameters,
    
    // Versioning information
    pub version: SemanticVersion,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

## Token Lifecycle

### Minting Process

Tokens are created through a governance-approved minting process:

```rust
pub fn mint_tokens(
    minting_authority: &KeyPair,
    resource_type: ResourceType,
    quantity: ResourceQuantity,
    recipient: Did,
    scope: TokenScope,
    policy_id: PolicyId,
    constraints: Vec<TokenConstraint>,
) -> Result<ScopedResourceToken, MintingError> {
    // 1. Verify minting authority
    verify_minting_authority(
        &minting_authority.public_key(),
        &resource_type,
        &scope,
        &policy_id,
    )?;
    
    // 2. Check policy constraints
    let policy = get_economic_policy(&policy_id)?;
    check_minting_constraints(
        &policy,
        &resource_type,
        &quantity,
        &recipient,
    )?;
    
    // 3. Verify within policy limits
    verify_minting_limits(
        &policy,
        &resource_type,
        &quantity,
        &scope,
    )?;
    
    // 4. Create token
    let token = ScopedResourceToken {
        id: generate_token_id(),
        resource_type,
        quantity,
        scope,
        metadata: TokenMetadata {
            name: format!("{} Token", resource_type_to_string(&resource_type)),
            description: format!("Token for {} resources in scope {}", 
                               resource_type_to_string(&resource_type),
                               scope_to_string(&scope)),
            created_at: DateTime::now_utc(),
            updated_at: DateTime::now_utc(),
            tags: vec![],
            custom: HashMap::new(),
        },
        issuance: TokenIssuance {
            issuer: did_from_keypair(minting_authority),
            issuance_time: DateTime::now_utc(),
            issuance_policy: policy_id,
            issuance_authority: IssuanceAuthority::Federation,
            issuance_reason: IssuanceReason::PolicyBasedAllocation,
        },
        constraints,
        expiration: policy.expiration_rules.default_expiration.clone(),
        status: TokenStatus::Active,
        proof: TokenProof::None, // Will be filled below
    };
    
    // 5. Generate cryptographic proof
    let token_with_proof = generate_token_proof(
        token,
        minting_authority,
    )?;
    
    // 6. Create DAG node for token
    let token_node = create_dag_node(
        minting_authority,
        &token_with_proof,
        NodeType::Token,
    )?;
    
    // 7. Submit to network
    submit_dag_node(token_node)?;
    
    // 8. Update token registry
    update_token_registry(&token_with_proof, TokenRegistryAction::Mint)?;
    
    // 9. Notify recipient
    notify_token_recipient(&token_with_proof, &recipient)?;
    
    Ok(token_with_proof)
}
```

### Allocation Process

Tokens can be allocated through various mechanisms:

```rust
pub enum AllocationMechanism {
    // Direct admin allocation
    AdminAllocation {
        admin_did: Did,
        authorization: AdminAuthorizationProof,
    },
    
    // Governance-approved allocation
    GovernanceApproved {
        proposal_id: ProposalId,
        vote_result: VoteResult,
    },
    
    // Contribution-based allocation
    ContributionBased {
        contribution_proof: ContributionProof,
        verification_result: VerificationResult,
    },
    
    // Algorithmic allocation
    Algorithmic {
        algorithm_id: AlgorithmId,
        input_parameters: HashMap<String, Value>,
        execution_proof: AlgorithmExecutionProof,
    },
    
    // Resource exchange allocation
    Exchange {
        exchange_id: ExchangeId,
        source_tokens: Vec<TokenReference>,
        exchange_rate: ExchangeRate,
        exchange_timestamp: DateTime<Utc>,
    },
}
```

### Usage Flow

Token usage follows a multi-step verification process:

```rust
pub fn perform_metered_action(
    user_keypair: &KeyPair,
    resource_type: &ResourceType,
    action_context: &ActionContext,
) -> Result<ActionReceipt, ActionError> {
    // 1. Estimate resource requirements
    let estimated_usage = estimate_resource_usage(
        resource_type,
        action_context,
    )?;
    
    // 2. Check resource authorization
    let authorization = get_resource_authorization(
        &did_from_keypair(user_keypair),
        resource_type,
        &estimated_usage,
    )?;
    
    // 3. Verify authorization is valid
    verify_authorization_validity(&authorization)?;
    
    // 4. Pre-allocate resources
    preallocate_resources(
        &authorization,
        &estimated_usage,
    )?;
    
    // 5. Perform the action
    let action_result = execute_action(
        action_context,
        &authorization,
    )?;
    
    // 6. Measure actual resource usage
    let actual_usage = measure_actual_usage(
        resource_type,
        &action_result,
    )?;
    
    // 7. Consume tokens
    consume_tokens(
        &authorization,
        &actual_usage,
    )?;
    
    // 8. Generate action receipt
    let receipt = ActionReceipt {
        id: generate_receipt_id(),
        user: did_from_keypair(user_keypair),
        resource_type: resource_type.clone(),
        estimated_usage,
        actual_usage,
        action_context: action_context.clone(),
        action_result: action_result.clone(),
        timestamp: DateTime::now_utc(),
        status: ActionStatus::Completed,
    };
    
    // 9. Create DAG node for receipt
    let receipt_node = create_dag_node(
        user_keypair,
        &receipt,
        NodeType::ActionReceipt,
    )?;
    
    // 10. Submit to network
    submit_dag_node(receipt_node)?;
    
    // 11. Update usage records
    update_usage_records(&receipt)?;
    
    Ok(receipt)
}
```

### Burn and Expiry

Tokens can be burned or expire according to policy:

```rust
pub enum ExpirationPolicy {
    // Token never expires
    NoExpiration,
    
    // Token expires at specific time
    TimeBasedExpiration {
        expiration_time: DateTime<Utc>,
    },
    
    // Token expires after inactivity
    InactivityExpiration {
        max_inactivity: Duration,
        last_activity: DateTime<Utc>,
    },
    
    // Token expires after usage
    UsageBasedExpiration {
        remaining_uses: u32,
    },
    
    // Token with decreasing value over time
    GradualExpiration {
        decay_start: DateTime<Utc>,
        decay_rate: f64,
        minimum_value: Option<f64>,
    },
    
    // Custom expiration logic
    CustomExpiration {
        policy_id: String,
        parameters: HashMap<String, Value>,
        expiration_wasm: Vec<u8>,
    },
}
```

Token burning process:

```rust
pub fn burn_tokens(
    owner_keypair: &KeyPair,
    token_id: &TokenId,
    quantity: Option<ResourceQuantity>,
    burn_reason: BurnReason,
) -> Result<BurnReceipt, BurnError> {
    // 1. Get the token
    let mut token = get_token(token_id)?;
    
    // 2. Verify ownership
    verify_token_ownership(
        &did_from_keypair(owner_keypair),
        &token,
    )?;
    
    // 3. Verify token is active
    if token.status != TokenStatus::Active {
        return Err(BurnError::TokenNotActive);
    }
    
    // 4. Determine burn quantity
    let burn_quantity = quantity.unwrap_or(token.quantity.clone());
    
    // 5. Check if partial burn
    let is_partial_burn = burn_quantity < token.quantity;
    
    // 6. Update token if partial burn
    if is_partial_burn {
        // Subtract burned quantity
        token.quantity = token.quantity - burn_quantity.clone();
        token.updated_at = DateTime::now_utc();
        
        // Create updated token node
        let updated_token_node = create_dag_node(
            owner_keypair,
            &token,
            NodeType::TokenUpdate,
        )?;
        
        // Submit update
        submit_dag_node(updated_token_node)?;
    } else {
        // Mark as burned if complete burn
        token.status = TokenStatus::Burned;
        token.updated_at = DateTime::now_utc();
        
        // Create token burn node
        let burn_token_node = create_dag_node(
            owner_keypair,
            &token,
            NodeType::TokenBurn,
        )?;
        
        // Submit burn
        submit_dag_node(burn_token_node)?;
    }
    
    // 7. Create burn receipt
    let receipt = BurnReceipt {
        id: generate_receipt_id(),
        token_id: token_id.clone(),
        burner: did_from_keypair(owner_keypair),
        quantity: burn_quantity,
        burn_time: DateTime::now_utc(),
        reason: burn_reason,
        is_partial: is_partial_burn,
    };
    
    // 8. Create DAG node for burn receipt
    let receipt_node = create_dag_node(
        owner_keypair,
        &receipt,
        NodeType::BurnReceipt,
    )?;
    
    // 9. Submit to network
    submit_dag_node(receipt_node)?;
    
    // 10. Update token registry
    update_token_registry(
        &token,
        if is_partial_burn {
            TokenRegistryAction::Update
        } else {
            TokenRegistryAction::Burn
        },
    )?;
    
    Ok(receipt)
}
```

## Scoped Economies

### Token Scopes

Tokens are scoped to specific jurisdictions:

```rust
pub enum TokenScope {
    // Global scope (across all federations)
    Global,
    
    // Federation scope
    Federation(FederationId),
    
    // Cooperative scope
    Cooperative {
        federation_id: FederationId,
        cooperative_id: CooperativeId,
    },
    
    // Working group scope
    WorkingGroup {
        federation_id: FederationId,
        cooperative_id: Option<CooperativeId>,
        group_id: GroupId,
    },
    
    // Project scope
    Project {
        federation_id: FederationId,
        project_id: ProjectId,
    },
    
    // Individual scope
    Individual(Did),
    
    // Custom scope
    Custom {
        scope_id: String,
        parent_scope: Box<TokenScope>,
        scope_definition: ScopeDefinition,
    },
}
```

### Scope Isolation

Scopes define the boundaries of economic activity:

```rust
pub struct ScopeIsolationPolicy {
    // Scope this policy applies to
    pub scope: TokenScope,
    
    // Whether tokens can be transferred across scope boundaries
    pub allows_external_transfers: bool,
    
    // Scope boundary crossing rules
    pub boundary_rules: BoundaryRules,
    
    // Exchange rates for cross-scope transfers (if allowed)
    pub exchange_rates: Option<ExchangeRatePolicy>,
    
    // Regulatory requirements for cross-scope transfers
    pub regulatory_requirements: Vec<RegulatoryRequirement>,
}
```

### Economic Anchoring

Economic state is anchored in the DAG through periodic economic anchors:

```rust
pub struct EconomicAnchor {
    // Base DAG node
    pub base_node: DagNode,
    
    // Federation this anchor belongs to
    pub federation_id: FederationId,
    
    // Anchor time
    pub anchor_time: DateTime<Utc>,
    
    // Economic state root
    pub economic_state_root: Hash,
    
    // Merkle tree of token operations
    pub token_operations_root: Hash,
    
    // Aggregated economic metrics
    pub economic_metrics: EconomicMetrics,
    
    // Range of nodes covered
    pub node_range: NodeRange,
    
    // Quorum signatures
    pub signatures: Vec<QuorumSignature>,
}
```

## Metered Execution and Runtime Enforcement

### WASM Resource Metering

Resource usage is metered through a combination of static and dynamic analysis:

```rust
pub struct ResourceMeteringConfig {
    // Resource type being metered
    pub resource_type: ResourceType,
    
    // Static analysis configuration
    pub static_analysis: StaticAnalysisConfig,
    
    // Dynamic metering configuration
    pub dynamic_metering: DynamicMeteringConfig,
    
    // Resource limits
    pub resource_limits: ResourceLimits,
    
    // Metering precision
    pub metering_precision: MeteringPrecision,
    
    // Reporting frequency
    pub reporting_frequency: ReportingFrequency,
}
```

### Host ABI for Resource Checking

WASM modules interact with the runtime through host functions:

```rust
pub enum HostFunction {
    // Check if operation is authorized
    HostCheckResourceAuthorization {
        resource_type: ResourceType,
        quantity: ResourceQuantity,
        context: Vec<u8>,
    },
    
    // Record resource usage
    HostRecordResourceUsage {
        resource_type: ResourceType,
        quantity: ResourceQuantity,
        context: Vec<u8>,
    },
    
    // Get available resources
    HostGetAvailableResources {
        resource_type: ResourceType,
    },
    
    // Record metering event
    HostRecordMeteringEvent {
        event_type: MeteringEventType,
        resource_type: ResourceType,
        quantity: ResourceQuantity,
        context: Vec<u8>,
    },
}
```

Example of authorization check:

```rust
// Host function implementation
pub fn host_check_resource_authorization(
    ctx: &mut RuntimeContext,
    resource_type: ResourceType,
    quantity: ResourceQuantity,
    context_data: Vec<u8>,
) -> Result<bool, HostError> {
    // 1. Get the module caller
    let caller = ctx.get_caller()?;
    
    // 2. Deserialize context
    let context = deserialize_context(&context_data)?;
    
    // 3. Get applicable authorizations
    let authorizations = get_applicable_authorizations(
        &caller,
        &resource_type,
        &context,
    )?;
    
    // 4. Check against authorizations
    for auth in authorizations {
        if check_authorization_covers(
            &auth,
            &resource_type,
            &quantity,
            &context,
        )? {
            return Ok(true);
        }
    }
    
    Ok(false)
}
```

### Fuel Metering vs. Scoped Tokens

The ICN distinguishes between fuel metering and scoped tokens:

```rust
pub enum MeteringMethod {
    // Low-level runtime fuel metering
    FuelMetering {
        // Fuel cost per operation type
        operation_costs: HashMap<WasmOperationType, u64>,
        // Fuel limit
        fuel_limit: u64,
        // Refund policy
        refund_policy: RefundPolicy,
    },
    
    // Higher-level token-based metering
    TokenMetering {
        // Token consumption rules
        token_consumption: TokenConsumptionRules,
        // Resource mapping
        resource_mapping: HashMap<ResourceType, ResourceMapping>,
        // Verification method
        verification_method: VerificationMethod,
    },
    
    // Hybrid metering approach
    HybridMetering {
        fuel_metering: Box<MeteringMethod>,
        token_metering: Box<MeteringMethod>,
        correlation_rules: Vec<CorrelationRule>,
    },
}
```

## Economic Credential Issuance

### Contribution Credentials

```rust
pub struct ContributionCredential {
    // Basic credential fields
    pub id: CredentialId,
    pub issuer: Did,
    pub subject: Did,
    pub issuance_date: DateTime<Utc>,
    pub expiration_date: Option<DateTime<Utc>>,
    
    // Contribution details
    pub contribution_type: ContributionType,
    pub resource_type: ResourceType,
    pub quantity: ResourceQuantity,
    pub period: Period,
    
    // Verification method
    pub verification_method: VerificationMethod,
    
    // Proof of contribution
    pub contribution_proof: ContributionProof,
    
    // Credential status
    pub status: CredentialStatus,
    
    // Cryptographic proof
    pub proof: CredentialProof,
}
```

### Resource Allocation Credentials

```rust
pub struct ResourceAllocationCredential {
    // Basic credential fields
    pub id: CredentialId,
    pub issuer: Did,
    pub subject: Did,
    pub issuance_date: DateTime<Utc>,
    pub expiration_date: Option<DateTime<Utc>>,
    
    // Allocation details
    pub resource_type: ResourceType,
    pub quantity: ResourceQuantity,
    pub allocation_policy: PolicyId,
    pub allocation_reason: AllocationReason,
    
    // Usage constraints
    pub usage_constraints: Vec<UsageConstraint>,
    
    // Associated tokens
    pub token_references: Vec<TokenReference>,
    
    // Credential status
    pub status: CredentialStatus,
    
    // Cryptographic proof
    pub proof: CredentialProof,
}
```

### Selective Disclosure of Resource Usage

```rust
pub struct ResourceUsageCredential {
    // Basic credential fields
    pub id: CredentialId,
    pub issuer: Did,
    pub subject: Did,
    pub issuance_date: DateTime<Utc>,
    pub expiration_date: Option<DateTime<Utc>>,
    
    // Usage details
    pub resource_type: ResourceType,
    pub usage_metrics: UsageMetrics,
    pub period: Period,
    
    // Usage context
    pub usage_context: UsageContext,
    
    // Disclosure control
    pub disclosure_policy: DisclosurePolicy,
    
    // Credential status
    pub status: CredentialStatus,
    
    // Cryptographic proof
    pub proof: CredentialProof,
}
```

Example of selective disclosure:

```rust
pub fn create_selective_disclosure_proof(
    credential: &ResourceUsageCredential,
    disclosure_attributes: &[String],
    nonce: &str,
) -> Result<SelectiveDisclosureProof, CredentialError> {
    // 1. Verify credential is valid
    verify_credential(credential)?;
    
    // 2. Check disclosure policy
    check_disclosure_policy_allows(
        &credential.disclosure_policy,
        disclosure_attributes,
    )?;
    
    // 3. Generate blinded commitment
    let blinded_commitment = generate_blinded_commitment(
        credential,
        disclosure_attributes,
        nonce,
    )?;
    
    // 4. Create disclosure proof
    let proof = SelectiveDisclosureProof {
        credential_id: credential.id.clone(),
        issuer: credential.issuer.clone(),
        subject: credential.subject.clone(),
        disclosed_attributes: disclosure_attributes.to_vec(),
        blinded_commitment,
        nonce: nonce.to_string(),
        created_at: DateTime::now_utc(),
    };
    
    // 5. Generate cryptographic proof
    let proof_with_signature = sign_disclosure_proof(proof)?;
    
    Ok(proof_with_signature)
}
```

## Governance Integration

### Economic Policy Proposals

Economic policies are configured through governance proposals:

```rust
pub struct EconomicPolicyProposal {
    // Base proposal fields
    pub base_proposal: Proposal,
    
    // Economic policy being proposed
    pub policy: EconomicPolicy,
    
    // Current policy (if updating)
    pub current_policy: Option<EconomicPolicy>,
    
    // Impact analysis
    pub impact_analysis: EconomicImpactAnalysis,
    
    // Implementation timeline
    pub implementation_timeline: Timeline,
    
    // Transition plan (if updating)
    pub transition_plan: Option<TransitionPlan>,
}
```

### Federation Quorum for Economic Policy

Economic policy changes require federation quorum:

```rust
pub struct EconomicQuorumRules {
    // Base quorum rules
    pub base_rules: QuorumRules,
    
    // Economic impact thresholds
    pub impact_thresholds: HashMap<ImpactLevel, f64>,
    
    // Required economic expertise
    pub required_expertise: Vec<ExpertiseRequirement>,
    
    // Additional signers for high-impact changes
    pub high_impact_additional_signers: Vec<SignerRole>,
    
    // Economic guardian requirements
    pub economic_guardian_requirements: Option<GuardianRequirements>,
}
```

### Credential + Token Double Check

Sensitive operations require both credential and token verification:

```rust
pub fn verify_economic_operation(
    operation: &EconomicOperation,
    executor: &Did,
) -> Result<VerificationResult, VerificationError> {
    // 1. Check operation sensitivity
    let sensitivity = determine_operation_sensitivity(operation)?;
    
    // 2. If not sensitive, do basic verification
    if sensitivity == OperationSensitivity::Low {
        return verify_basic_authorization(operation, executor);
    }
    
    // 3. For sensitive operations, verify credentials
    let credential_verification = verify_executor_credentials(
        executor,
        operation,
    )?;
    
    // 4. Verify token authorization
    let token_verification = verify_token_authorization(
        executor,
        operation,
    )?;
    
    // 5. Both must pass for sensitive operations
    if credential_verification.is_valid && token_verification.is_valid {
        Ok(VerificationResult::Valid)
    } else {
        let errors = Vec::new();
        if !credential_verification.is_valid {
            errors.extend(credential_verification.errors);
        }
        if !token_verification.is_valid {
            errors.extend(token_verification.errors);
        }
        
        Ok(VerificationResult::Invalid(errors))
    }
}
```

## Security Considerations

### Double Spend Prevention

The ICN prevents double spending through a combination of techniques:

```rust
pub fn prevent_double_spend(
    token_id: &TokenId,
    operation: &TokenOperation,
) -> Result<(), DoubleSpendError> {
    // 1. Check token status
    let token = get_token(token_id)?;
    if token.status != TokenStatus::Active {
        return Err(DoubleSpendError::TokenNotActive);
    }
    
    // 2. Acquire token lock
    let _lock = acquire_token_lock(token_id)?;
    
    // 3. Verify token wasn't spent elsewhere (in DAG)
    verify_token_not_spent_in_dag(token_id)?;
    
    // 4. Check for conflicting operations
    check_conflicting_operations(token_id, operation)?;
    
    // 5. Record spending operation
    record_token_operation(token_id, operation)?;
    
    // 6. Release lock on success (implicitly via RAII)
    
    Ok(())
}
```

### Sandboxed Execution

```rust
pub struct SandboxConfig {
    // Resource limits
    pub resource_limits: ResourceLimits,
    
    // Host functions available to sandbox
    pub allowed_host_functions: Vec<HostFunction>,
    
    // Network access policy
    pub network_policy: NetworkPolicy,
    
    // Storage access policy
    pub storage_policy: StoragePolicy,
    
    // Execution timeout
    pub execution_timeout: Duration,
    
    // Isolation level
    pub isolation_level: IsolationLevel,
}
```

### Token Inflation Prevention

```rust
pub struct InflationPreventionPolicy {
    // Resource type this applies to
    pub resource_type: ResourceType,
    
    // Maximum total supply
    pub max_total_supply: Option<ResourceQuantity>,
    
    // Maximum mint rate
    pub max_mint_rate: TokenRate,
    
    // Required approvals for minting
    pub minting_approvals: Vec<ApprovalRequirement>,
    
    // Monitoring rules
    pub monitoring_rules: Vec<MonitoringRule>,
    
    // Automatic controls
    pub automatic_controls: Vec<AutomaticControl>,
    
    // Audit requirements
    pub audit_requirements: AuditRequirements,
}
```

## Future Extensions

### Mutual Credit Systems

```rust
pub struct MutualCreditSystem {
    // System identifier
    pub id: SystemId,
    
    // Federation this system belongs to
    pub federation_id: FederationId,
    
    // Credit parameters
    pub credit_parameters: CreditParameters,
    
    // Account management
    pub account_management: AccountManagement,
    
    // Trust metrics
    pub trust_metrics: TrustMetrics,
    
    // Balance limits
    pub balance_limits: BalanceLimits,
    
    // Clearing mechanism
    pub clearing_mechanism: ClearingMechanism,
    
    // Dispute resolution
    pub dispute_resolution: DisputeResolution,
}
```

### Liquid Pledging

```rust
pub struct LiquidPledging {
    // Pledge system identifier
    pub id: PledgeSystemId,
    
    // Federation this system belongs to
    pub federation_id: FederationId,
    
    // Pledge parameters
    pub pledge_parameters: PledgeParameters,
    
    // Delegation rules
    pub delegation_rules: DelegationRules,
    
    // Project vetting
    pub project_vetting: ProjectVetting,
    
    // Cancellation rules
    pub cancellation_rules: CancellationRules,
    
    // Transparency rules
    pub transparency_rules: TransparencyRules,
}
```

### Cooperative Dividend Logic

```rust
pub struct CooperativeDividend {
    // Dividend system identifier
    pub id: DividendSystemId,
    
    // Cooperative this system belongs to
    pub cooperative_id: CooperativeId,
    
    // Surplus calculation
    pub surplus_calculation: SurplusCalculation,
    
    // Distribution formula
    pub distribution_formula: DistributionFormula,
    
    // Member participation metrics
    pub participation_metrics: ParticipationMetrics,
    
    // Payout mechanism
    pub payout_mechanism: PayoutMechanism,
    
    // Retention rules
    pub retention_rules: RetentionRules,
}
```

### Cross-Federation Clearing

```rust
pub struct CrossFederationClearing {
    // Clearing system identifier
    pub id: ClearingSystemId,
    
    // Participating federations
    pub federations: Vec<FederationId>,
    
    // Clearing rules
    pub clearing_rules: ClearingRules,
    
    // Exchange rates
    pub exchange_rates: ExchangeRatePolicy,
    
    // Settlement mechanism
    pub settlement_mechanism: SettlementMechanism,
    
    // Dispute resolution
    pub dispute_resolution: DisputeResolution,
    
    // Regulatory compliance
    pub regulatory_compliance: RegulatoryCompliance,
}
```

## Glossary

| Term | Definition |
|------|------------|
| **Allocation** | The process of distributing tokens to specific entities based on governance decisions or contribution metrics. |
| **Burn** | The process of permanently removing tokens from circulation, often used for consumed resources or expired tokens. |
| **Economic Anchor** | A periodic cryptographic commitment to the economic state, signed by federation quorum. |
| **Economic Policy** | A set of rules governing token behavior, including minting, transfer, consumption, and expiration. |
| **Federation-Scoped Economy** | An economic domain limited to activities within a specific federation. |
| **Fuel Metering** | Low-level accounting of computational resources used during WASM execution. |
| **Metering** | The process of measuring resource usage during system operations. |
| **Mint** | The process of creating new tokens, typically governed by economic policy. |
| **Resource Authorization** | Permission to use a specific quantity of a resource, backed by tokens. |
| **Resource Grant** | An allocation of resource usage rights to a specific entity. |
| **Resource Type** | A category of system resource that can be metered and tokenized. |
| **Scope** | The jurisdictional boundary defining where tokens are valid. |
| **Scoped Resource Token (SRT)** | A token representing the right to use a specific resource within a defined scope. |
| **Selective Disclosure** | The ability to reveal only specific aspects of resource usage while keeping others private. |
| **Token Constraint** | A limitation on how a token can be used, transferred, or consumed. |
| **Token Lifecycle** | The sequence of states a token passes through from minting to expiration or burning. | 