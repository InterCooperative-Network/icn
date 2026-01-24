# Witness Signature Best Practices

**Last Updated**: 2026-01-24  
**Status**: Developer Guide  
**Related**: PR #826 (Witness Signature Support), Issue #832 (Trust-Graph Integration)

---

## Overview

This guide provides best practices for using witness signatures in ICN's resource access system. Witness signatures enable cryptographic attestation of high-value transactions, stewardship transfers, and multi-party agreements, providing an immutable audit trail and enhanced security for critical cooperative operations.

### What Are Witness Signatures?

Witness signatures are Ed25519 cryptographic signatures from trusted community members who attest to the validity of a ledger entry or resource transfer. Unlike the entry author (who records the transaction), witnesses provide independent verification that the transaction occurred as described.

**Key Components:**
- **WitnessedEntry**: A journal entry with cryptographic signatures from witnesses
- **WitnessPolicy**: Configuration defining witness requirements (Counterparty, Quorum, AllParties)
- **WitnessConfig**: Ledger-wide witness configuration with value thresholds and timeouts
- **UsageEvent Witnesses**: Witness attestation for resource usage and handoff events

---

## When to Use Witness Signatures

### High-Value Resources

Use witness signatures for resources with significant economic or community value:

**Examples:**
- **Land and real estate**: Community land trusts, shared farmland, cooperative housing
- **Equipment and machinery**: Tractors, manufacturing equipment, vehicles over certain value thresholds
- **Infrastructure**: Solar installations, water systems, shared workshops
- **Financial transactions**: Large mutual credit transfers, bulk resource purchases

**Recommendation**: Configure a value threshold above which witnesses are required automatically.

```rust
// Example: Require witnesses for transactions over 1000 hours
let witness_config = WitnessConfig {
    default_policy: WitnessPolicy::Counterparty,
    threshold: Some(1000), // Apply policy only above this value
    collection_timeout_secs: 300, // 5 minutes to collect signatures
};
```

### Legal and Regulatory Requirements

When legal compliance or regulatory oversight is required:

**Examples:**
- Transfers requiring notarization equivalents
- Compliance with cooperative bylaws requiring multi-party sign-off
- Agricultural equipment transfers with safety certifications
- Medical or hazardous equipment requiring attestation

**Recommendation**: Use `WitnessPolicy::Quorum` with designated governance roles as witnesses.

### Multi-Party Stewardship Transfers

When responsibility for a resource changes hands between stewards:

**Examples:**
- Tool library equipment checkout and return
- Seasonal farmland stewardship rotation
- Shared vehicle handoffs between members
- Workshop space time-slot transitions

**Recommendation**: Use `HandoffProcedure` with witness attestation at completion.

```rust
// Example: Tractor handoff with witness verification
AccessModel::Stewardship {
    steward: new_steward_did.clone(),
    review_period_seconds: 2_592_000, // 30 days
    duties: vec![
        StewardshipDuty::HandoffProcedure {
            steps: vec![
                "Verify fuel level and document meter reading".to_string(),
                "Inspect for damage and note any issues".to_string(),
                "Confirm maintenance schedule with new steward".to_string(),
                "Physical key transfer witnessed".to_string(),
            ],
        },
    ],
}

// Record handoff completion with witnesses (step_index=3 for final step in 4-step procedure)
let handoff_event = UsageEvent::handoff_step(timestamp, "Tractor handoff completed - all steps verified", 3)
    .with_witness("did:icn:equipment_coordinator".to_string())
    .with_witness("did:icn:farm_steward".to_string());

access.record_usage_event(handoff_event.clone())?;
if !access.validate_duty_completion(&handoff_event, 2) { // Require 2 witnesses
    return Err(AccessError::InsufficientWitnesses);
}
```

### Trust-Critical Operations

Operations where trust and transparency are paramount:

**Examples:**
- Dissolution of cooperative assets
- Major policy changes affecting resources
- Dispute resolution outcomes
- Emergency resource allocation decisions

**Recommendation**: Use `WitnessPolicy::AllParties` to ensure all affected parties sign off.

---

## Witness Selection Guidelines

### How Many Witnesses to Require

The number of required witnesses should balance security with practical feasibility:

| **Resource Type** | **Recommended Minimum** | **Rationale** |
|-------------------|------------------------|---------------|
| Low-value tools (<$100) | 0-1 witnesses | Counterparty signature sufficient |
| Medium-value equipment ($100-$5000) | 2 witnesses | Prevents single-point trust failures |
| High-value assets (>$5000) | 3-5 witnesses | Multi-party verification, quorum resilience |
| Legal/regulatory compliance | 2-3 designated witnesses | Specific governance roles required |
| Emergency operations | 2 witnesses minimum | Balance urgency with accountability |

**Threshold Configuration Strategy:**

```rust
// Tiered witness policy based on transaction value
impl WitnessConfig {
    /// Create config for standard cooperative operations
    pub fn standard_cooperative() -> Self {
        WitnessConfig {
            default_policy: WitnessPolicy::Counterparty,
            threshold: Some(500), // Hours-denominated threshold
            collection_timeout_secs: 300,
        }
    }
    
    /// Create config for high-security operations
    pub fn high_security() -> Self {
        WitnessConfig {
            default_policy: WitnessPolicy::Quorum {
                required: 3,
                witnesses: vec![], // Set at runtime based on governance roles
            },
            threshold: Some(1000),
            collection_timeout_secs: 600, // 10 minutes for complex operations
        }
    }
}
```

### Who Makes Good Witnesses

Effective witnesses have three key qualities:

#### 1. Trust and Reputation
- Established cooperative members with positive trust graph scores
- No conflict of interest in the transaction
- History of reliable participation

**Implementation:**
```rust
// Future enhancement (Issue #832): Trust-graph integration
// Automatically validate witness trust scores
pub fn validate_witness_trust(ledger: &Ledger, witness_did: &Did) -> Result<bool> {
    let trust_score = ledger.trust_graph.get_trust(witness_did)?;
    Ok(trust_score >= 0.5) // Minimum trust threshold
}
```

#### 2. Relevant Expertise or Role
- Equipment coordinators for machinery transfers
- Land stewards for farmland handoffs
- Financial officers for large monetary transactions
- Domain experts who can verify technical conditions

**Example Witness Pool:**
```rust
// Define witness pools by resource type
pub struct WitnessPool {
    resource_type: String,
    eligible_witnesses: Vec<Did>,
    required_count: usize,
}

let tractor_pool = WitnessPool {
    resource_type: "agricultural_equipment".to_string(),
    eligible_witnesses: vec![
        farm_coordinator_did,
        equipment_manager_did,
        experienced_farmer_did,
    ],
    required_count: 2,
};
```

#### 3. Availability and Responsiveness
- Able to respond within the `collection_timeout_secs` window
- Geographically proximate for physical verification requirements
- Sufficient time availability for due diligence

**Best Practice**: Maintain backup witnesses in case primary witnesses are unavailable.

### Geographic and Organizational Diversity

**Why Diversity Matters:**
- Prevents collusion or coordinated fraud
- Ensures multiple perspectives on resource state
- Reduces risk of single-point failures (e.g., network outages affecting one location)

**Recommendations:**
- For high-value assets, require witnesses from different local nodes/coops
- Include at least one witness outside the immediate working group
- Balance local knowledge with external oversight

**Example:**
```rust
// Diverse witness selection for federated cooperative asset transfer
WitnessPolicy::Quorum {
    required: 3,
    witnesses: vec![
        "did:icn:coop_a:asset_manager".to_string(),  // Local cooperative witness
        "did:icn:coop_b:treasurer".to_string(),      // Peer cooperative witness
        "did:icn:federation:auditor".to_string(),    // Federation-level witness
    ],
}
```

---

## Configuration Examples

### Basic Counterparty Witness

The simplest witness policy: require the other party to the transaction to sign.

```rust
use icn_ledger::types::{WitnessConfig, WitnessPolicy};

// Configure ledger to require counterparty signatures above threshold
let config = WitnessConfig {
    default_policy: WitnessPolicy::Counterparty,
    threshold: Some(100), // Require witnesses for transactions > 100 hours
    collection_timeout_secs: 300, // 5 minutes
};

ledger.set_witness_config(config);

// When recording a payment, both parties must sign
// Alice pays Bob 150 hours for equipment rental
let entry = JournalEntryBuilder::new(alice_did.clone())
    .debit(alice_did.clone(), "hours".to_string(), 150)
    .credit(bob_did.clone(), "hours".to_string(), 150)
    .build()?;

// Alice submits the entry
let entry_hash = entry.id.clone().unwrap();

// Bob must sign as counterparty witness
let bob_signature = bob_keypair.sign(entry_hash.as_bytes());
let witnessed = WitnessedEntry {
    entry,
    witness_signatures: vec![
        WitnessSignature::new(
            bob_did.clone(),
            bob_signature.to_bytes().to_vec(),
            icn_time::current_timestamp_secs(),
        )
    ],
    policy_applied: WitnessPolicy::Counterparty,
};

ledger.append_witnessed_entry(witnessed).await?;
```

### Quorum-Based Witness Policy

Require a specified number of signatures from a designated witness pool.

```rust
// Define witness pool for equipment transfers
let equipment_witnesses = vec![
    "did:icn:equipment_coordinator".to_string(),
    "did:icn:safety_officer".to_string(),
    "did:icn:maintenance_lead".to_string(),
    "did:icn:coop_treasurer".to_string(),
];

// Require 2-of-4 signatures
let config = WitnessConfig {
    default_policy: WitnessPolicy::Quorum {
        required: 2,
        witnesses: equipment_witnesses.clone(),
    },
    threshold: Some(500), // Apply to high-value equipment
    collection_timeout_secs: 600, // 10 minutes for coordination
};

ledger.set_witness_config(config);

// Record equipment transfer with witness signatures
// Note: JournalEntryBuilder supports debit, credit, contract_ref, add_delta, add_parent
let entry = JournalEntryBuilder::new(coop_did.clone())
    .debit(coop_did.clone(), "equipment_hours".to_string(), 800)
    .credit(member_did.clone(), "equipment_hours".to_string(), 800)
    .build()?;

let entry_hash = entry.id.clone().unwrap();

// Collect signatures from 2 witnesses
let coordinator_sig = coordinator_kp.sign(entry_hash.as_bytes());
let treasurer_sig = treasurer_kp.sign(entry_hash.as_bytes());

let witnessed = WitnessedEntry {
    entry,
    witness_signatures: vec![
        WitnessSignature::new(
            equipment_witnesses[0].clone(),
            coordinator_sig.to_bytes().to_vec(),
            icn_time::current_timestamp_secs(),
        ),
        WitnessSignature::new(
            equipment_witnesses[3].clone(),
            treasurer_sig.to_bytes().to_vec(),
            icn_time::current_timestamp_secs(),
        ),
    ],
    policy_applied: WitnessPolicy::Quorum {
        required: 2,
        witnesses: equipment_witnesses,
    },
};

ledger.append_witnessed_entry(witnessed).await?;
```

### HandoffProcedure with Witness Verification

Use witness signatures to attest to completion of multi-step handoff procedures.

```rust
use icn_ledger::use_access::{
    AccessModel, StewardshipDuty, UsageEvent, ResourceAccess,
};

// Define stewardship access with handoff procedure
// ResourceAccess::new(resource_id, holder, model) - timestamp is set automatically
let access = ResourceAccess::new(
    resource_id.clone(),
    member_did.clone(),
    AccessModel::Stewardship {
        steward: member_did.clone(),
        review_period_seconds: 2_592_000, // 30 days
        duties: vec![
            StewardshipDuty::Maintenance {
                description: "Weekly inspection and maintenance".to_string(),
                frequency_seconds: 604_800, // 7 days
            },
            StewardshipDuty::HandoffProcedure {
                steps: vec![
                    "Document current state with photos".to_string(),
                    "Complete maintenance checklist".to_string(),
                    "Physical inspection with witness".to_string(),
                    "Transfer keys and access codes".to_string(),
                ],
            },
        ],
    },
);

// When relinquishing stewardship, complete handoff with witnesses
// step_index=3 for final step in a 4-step handoff procedure (0-indexed)
let handoff_event = UsageEvent::handoff_step(
    icn_time::current_timestamp_secs(),
    "Completed all handoff steps: state documented, maintenance up to date, physical inspection passed, keys transferred",
    3, // Final step index
)
.with_witness("did:icn:outgoing_steward".to_string())
.with_witness("did:icn:incoming_steward".to_string())
.with_witness("did:icn:equipment_coordinator".to_string());

// Validate handoff has sufficient witnesses (require 2-of-3)
if !access.validate_duty_completion(&handoff_event, 2) {
    return Err(AccessError::InsufficientWitnesses);
}

access.record_usage_event(handoff_event)?;
```

### All-Parties Witness Policy

Require all non-author counterparties to sign (strictest policy).

```rust
// For critical multi-party agreements
let config = WitnessConfig {
    default_policy: WitnessPolicy::AllParties,
    threshold: None, // Apply to all transactions regardless of value
    collection_timeout_secs: 900, // 15 minutes for all parties to coordinate
};

// Three-way resource exchange: all parties must sign
let entry = JournalEntryBuilder::new(alice_did.clone())
    .debit(alice_did.clone(), "hours".to_string(), 50)
    .credit(bob_did.clone(), "hours".to_string(), 50)
    .debit(bob_did.clone(), "tools".to_string(), 1)
    .credit(charlie_did.clone(), "tools".to_string(), 1)
    .debit(charlie_did.clone(), "materials".to_string(), 20)
    .credit(alice_did.clone(), "materials".to_string(), 20)
    .build()?;

// Require Bob and Charlie to witness (Alice is author)
let bob_sig = bob_kp.sign(entry_hash.as_bytes());
let charlie_sig = charlie_kp.sign(entry_hash.as_bytes());

let witnessed = WitnessedEntry {
    entry,
    witness_signatures: vec![
        WitnessSignature::new(bob_did, bob_sig.to_bytes().to_vec(), timestamp),
        WitnessSignature::new(charlie_did, charlie_sig.to_bytes().to_vec(), timestamp),
    ],
    policy_applied: WitnessPolicy::AllParties,
};
```

---

## Edge Cases and Error Handling

### Empty Witness List

**Scenario**: A `WitnessedEntry` is created with an empty `witness_signatures` vector.

**Behavior**:
- `WitnessPolicy::None`: Entry accepted (no witnesses required)
- `WitnessPolicy::Counterparty`: Entry rejected (counterparty signature missing)
- `WitnessPolicy::Quorum { required: 0, .. }`: Entry accepted (explicitly requires zero witnesses)
- `WitnessPolicy::Quorum { required: n, .. }` where n > 0: Entry rejected
- `WitnessPolicy::AllParties`: Entry rejected (all counterparties must sign)

**Validation**:
```rust
// Check before submitting
if witnessed.witness_signatures.is_empty() && 
   !matches!(witnessed.policy_applied, WitnessPolicy::None) {
    return Err(LedgerError::InsufficientWitnesses);
}

// Ledger validates with has_sufficient_signatures()
assert!(witnessed.has_sufficient_signatures());
```

**Best Practice**: Always validate witness count before submission to avoid rejected entries.

### Unavailable Witnesses

**Scenario**: Required witnesses are offline, unresponsive, or have lost their keys.

**Mitigation Strategies**:

1. **Timeout Gracefully**:
   ```rust
   // Configure realistic timeouts based on coordination needs
   WitnessConfig {
       collection_timeout_secs: 600, // 10 minutes for complex coordination
       ..Default::default()
   }
   ```

2. **Maintain Backup Witnesses**:
   ```rust
   // Quorum allows flexibility - require N of M witnesses
   WitnessPolicy::Quorum {
       required: 2,
       witnesses: vec![
           primary_witness_1,
           primary_witness_2,
           backup_witness_1,
           backup_witness_2,
       ], // 2-of-4: still succeeds if 2 are unavailable
   }
   ```

3. **Governance Override** (for emergencies):
   ```rust
   // Define emergency procedures in cooperative bylaws
   // Allow governance body to authorize transactions when witnesses unavailable
   // This should be logged and audited
   ```

4. **Witness Rotation**:
   ```rust
   // Periodically update witness pools to ensure availability
   pub fn rotate_witnesses(ledger: &mut Ledger, new_witnesses: Vec<Did>) -> Result<()> {
       let current_config = ledger.witness_config.clone().unwrap();
       let updated_config = WitnessConfig {
           default_policy: match current_config.default_policy {
               WitnessPolicy::Quorum { required, .. } => WitnessPolicy::Quorum {
                   required,
                   witnesses: new_witnesses,
               },
               other => other, // Preserve non-quorum policies
           },
           ..current_config
       };
       ledger.set_witness_config(updated_config);
       Ok(())
   }
   ```

### Witness Rotation During Long Handoff Processes

**Scenario**: A handoff procedure takes multiple days/weeks, and witnesses change roles during the process.

**Challenge**: 
- Initial witnesses may no longer be appropriate by completion time
- Witness pool updates mid-process could invalidate collected signatures

**Solution 1: Freeze Witness Requirements at Start**:
```rust
// Store the witness policy with the handoff initiation
pub struct HandoffInProgress {
    resource_id: String,
    initiated_at: u64,
    required_witnesses: Vec<Did>, // Frozen at start
    collected_signatures: Vec<WitnessSignature>,
}

// When witnesses rotate, in-progress handoffs keep original requirements
```

**Solution 2: Allow Witness Updates with Re-Attestation**:
```rust
// If witness pool changes during handoff, notify parties
pub fn update_handoff_witnesses(
    handoff: &mut HandoffInProgress,
    new_witnesses: Vec<Did>,
) -> Result<()> {
    // Clear old signatures that are no longer valid
    handoff.collected_signatures.clear();
    handoff.required_witnesses = new_witnesses;
    // Parties must re-collect signatures
    Ok(())
}
```

**Best Practice**: 
- Keep handoff procedures short (complete within `collection_timeout_secs`)
- For long-term stewardship transitions, break into discrete steps with separate witness attestations
- Document witness rotation policy in cooperative governance

### Signature Timestamp Validation

**Scenario**: A witness signature is timestamped in the future or is expired.

**Behavior**:
```rust
// Witness signatures are validated with strict timestamp checks
// Note: Actual signature includes ledger reference for config access
pub fn validate_witness_signatures(ledger: &Ledger, witnessed: &WitnessedEntry) -> Result<()> {
    let current_time = icn_time::try_current_timestamp_secs()?;
    const MAX_CLOCK_SKEW_SECS: u64 = 5; // Strict 5-second tolerance

    for sig in &witnessed.witness_signatures {
        // Reject future timestamps (even with small skew tolerance)
        if sig.signed_at > current_time + MAX_CLOCK_SKEW_SECS {
            return Err(LedgerError::FutureWitnessSignature);
        }

        // Reject expired signatures based on collection_timeout_secs
        if let Some(config) = &ledger.witness_config {
            let age = current_time.saturating_sub(sig.signed_at);
            if age > config.collection_timeout_secs {
                return Err(LedgerError::ExpiredWitnessSignature);
            }
        }
    }
    Ok(())
}
```

**Best Practice**:
- Ensure node clocks are synchronized (ICN uses Rough Time Protocol)
- Collect and submit witness signatures promptly after signing
- Configure `collection_timeout_secs` based on realistic coordination timelines

### Duplicate Witness Signatures

**Scenario**: The same witness DID signs multiple times (e.g., user error, retry logic).

**Behavior**:
```rust
// Witnesses are deduplicated by DID when counting
pub fn unique_witness_count(&self) -> usize {
    let unique: HashSet<&Did> = self.witness_signatures
        .iter()
        .map(|w| &w.witness)
        .collect();
    unique.len()
}
```

**Implication**: Multiple signatures from the same DID count as one witness.

**Best Practice**: Client code should deduplicate before submission to reduce payload size.

### Author as Witness

**Scenario**: The entry author also appears in the witness list.

**Behavior**:
```rust
// Author is never counted as a witness (prevented double-counting)
pub fn count_entry_signers(entry: &JournalEntry, witnesses: &[WitnessSignature]) -> usize {
    let unique_witnesses: HashSet<_> = witnesses
        .iter()
        .map(|w| &w.witness)
        .filter(|did| *did != &entry.author) // Filter out author
        .collect();
    unique_witnesses.len() + 1 // +1 for author
}
```

**Implication**: Author always counts as 1 signer, but not as a witness.

**Best Practice**: Don't include the author in the witness list (wastes space, provides no additional verification).

---

## Security Considerations

### Trust Requirements for Witnesses

**Principle**: Witness signatures are only meaningful if witnesses are trustworthy.

**Current Implementation**:
- No automatic trust validation (manual selection)
- Witnesses are identified by DID, trust is social

**Future Enhancement (Issue #832)**:
```rust
// Planned: Trust-graph integration
pub fn select_qualified_witnesses(
    trust_graph: &TrustGraph,
    candidate_pool: Vec<Did>,
    required: usize,
    min_trust: f64,
) -> Result<Vec<Did>> {
    let qualified: Vec<_> = candidate_pool
        .into_iter()
        .filter(|did| {
            trust_graph.get_trust(did).unwrap_or(0.0) >= min_trust
        })
        .take(required)
        .collect();
    
    if qualified.len() < required {
        return Err(AccessError::InsufficientQualifiedWitnesses);
    }
    
    Ok(qualified)
}
```

**Best Practices**:
- Manually vet witnesses before adding to witness pools
- Require witnesses to have established trust relationships
- Periodically review and rotate witness pools
- Monitor for compromised witness keys (key rotation procedures)

### Avoiding Single Points of Failure

**Risk**: If all witnesses are controlled by a single party or collude, attestation is meaningless.

**Mitigations**:

1. **Diverse Witness Pools**:
   - Geographic distribution
   - Organizational diversity (different coops, roles)
   - Prevent witnesses from same household/entity

2. **Quorum Requirements**:
   - Require M-of-N where M > N/2 (majority threshold)
   - Higher thresholds for critical operations

3. **Separation of Concerns**:
   ```rust
   // Example: Separate financial and operational witnesses
   let financial_witnesses = vec![treasurer_did, accountant_did];
   let operational_witnesses = vec![coordinator_did, steward_did];
   
   // For high-value asset transfer, require witnesses from both pools
   WitnessPolicy::Quorum {
       required: 3,
       witnesses: [financial_witnesses, operational_witnesses].concat(),
   }
   ```

4. **Time-Separation**:
   - For long-term stewardship, require periodic re-attestation
   - Prevents compromised witness from affecting multiple transactions

### Audit Trail and Dispute Resolution

**Principle**: Witness signatures create an immutable audit trail for dispute resolution.

**Key Features**:

1. **Cryptographic Proof**:
   - Ed25519 signatures cannot be forged
   - Entry hash binds signature to specific transaction
   - Timestamp proves when attestation occurred

2. **Persistent Storage**:
   ```rust
   // Witness signatures are stored alongside entries for fork resolution
   pub fn store_witness_signatures(
       ledger: &Ledger,
       entry_hash: &ContentHash,
       witnesses: &[WitnessSignature],
   ) -> Result<()> {
       let key = format!("witness:{}", entry_hash.to_hex());
       ledger.store.put(key.as_bytes(), &serde_json::to_vec(witnesses)?)?;
       Ok(())
   }
   ```

3. **Fork Resolution**:
   - In case of conflicting entries (e.g., double-spend attempts), the entry with more unique signers wins
   - Witness signatures increase signer count, making entries more resilient to forks

4. **Dispute Process**:
   ```rust
   // Retrieve witness evidence for dispute resolution
   pub fn get_transaction_witnesses(
       ledger: &Ledger,
       entry_hash: &ContentHash,
   ) -> Result<Vec<WitnessSignature>> {
       let witnesses = load_witness_signatures(ledger, entry_hash)?;
       Ok(witnesses)
   }
   
   // Present evidence to governance body
   // Witnesses can be contacted to provide additional context
   // Signature timestamps prove chronology
   ```

**Best Practice**:
- Document witness responsibilities in cooperative agreements
- Define dispute resolution procedures that leverage witness attestations
- Maintain long-term archive of witness signatures (backup/restore procedures)
- Implement witness accountability mechanisms (reputation penalties for false attestations)

### Witness Key Security

**Risk**: Compromised witness keys could enable fraudulent attestations.

**Mitigations**:

1. **Key Rotation**:
   ```rust
   // Periodically rotate witness keys
   pub fn rotate_witness_key(old_did: &Did, new_did: &Did) -> Result<()> {
       // Update witness pools
       // Notify cooperative of key rotation
       // Invalidate signatures from old key after cutoff date
       Ok(())
   }
   ```

2. **Hardware Security**:
   - Encourage witnesses to use hardware wallets or TPM-backed keys
   - See `docs/tpm-setup.md` for TPM integration

3. **Multi-Device Identity**:
   - ICN supports multi-device identity (see `docs/multi-device-identity-design.md`)
   - Witnesses can sign from secure backup devices

4. **Revocation Procedures**:
   ```rust
   // If witness key is compromised, cooperative should:
   // 1. Remove witness from all active pools
   // 2. Review recent signatures from compromised witness
   // 3. Re-attest critical transactions if needed
   ```

**Best Practice**:
- Provide security training for designated witnesses
- Implement key management policies for witness roles
- Monitor for suspicious witness behavior (unusual signing patterns)

---

## Integration with Trust Graph (Future)

**Status**: Planned enhancement (Issue #832)

### Automatic Trust Validation

Future versions will integrate witness selection with ICN's trust graph:

```rust
// Planned API
pub struct TrustWeightedWitnessPolicy {
    min_trust_score: f64,
    required_witnesses: usize,
    candidate_pool: Vec<Did>,
}

impl TrustWeightedWitnessPolicy {
    /// Select witnesses automatically based on trust scores
    pub fn select_witnesses(
        &self,
        trust_graph: &TrustGraph,
    ) -> Result<Vec<Did>> {
        // Filter candidates by trust threshold
        let qualified: Vec<_> = self.candidate_pool
            .iter()
            .filter(|did| {
                trust_graph.compute_trust(did).unwrap_or(0.0) >= self.min_trust_score
            })
            .collect();
        
        // Rank by trust and diversity metrics
        // Select top N with maximum diversity
        Ok(/* ... */)
    }
}
```

### Reputation-Based Witness Quality

```rust
// Planned: Witness reputation tracking
pub struct WitnessReputation {
    did: Did,
    attestations_signed: u64,
    disputed_attestations: u64,
    reputation_score: f64,
}

// Penalize witnesses whose attestations are frequently disputed
// Boost reputation for witnesses whose attestations stand up to scrutiny
```

### Dynamic Witness Pool Management

```rust
// Planned: Automatically maintain witness pools based on trust graph
pub fn update_witness_pool_from_trust(
    trust_graph: &TrustGraph,
    resource_type: &str,
    min_trust: f64,
) -> Result<Vec<Did>> {
    // Query trust graph for members with relevant expertise and sufficient trust
    let pool = trust_graph.query_members(&TrustQuery {
        min_trust_score: min_trust,
        has_role: Some(format!("{}_witness", resource_type)),
        max_results: 10,
    })?;
    Ok(pool)
}
```

---

## Related Documentation

- **[ECONOMIC_ARCHITECTURE.md](../ECONOMIC_ARCHITECTURE.md)**: Asset token witness bonding (Layer 2 economy)
- **[production-hardening.md](../production-hardening.md)**: Witness signature timestamp validation
- **[trust-multi-graph-migration.md](../trust-multi-graph-migration.md)**: Trust graph architecture for future witness integration
- **[governance-primitives.md](../governance-primitives.md)**: Governance-based witness pool management
- **Ledger Implementation**: `icn/crates/icn-ledger/src/ledger_impl/witness_ops.rs`
- **Resource Access**: `icn/crates/icn-ledger/src/use_access.rs`
- **Integration Tests**: `icn/crates/icn-core/tests/witness_integration.rs`

---

## Summary

Witness signatures provide cryptographic attestation for high-value transactions and resource stewardship transfers in ICN. Key takeaways:

1. **Use witnesses for**: High-value resources, legal compliance, multi-party transfers, trust-critical operations
2. **Select witnesses based on**: Trust/reputation, relevant expertise, availability, and geographic/organizational diversity
3. **Configure thresholds**: Balance security (more witnesses) with practicality (coordination overhead)
4. **Handle edge cases**: Plan for unavailable witnesses, timeout gracefully, implement witness rotation
5. **Ensure security**: Vet witnesses manually (until trust-graph integration), avoid single points of failure, maintain audit trails

Witness signatures are a foundational security feature for cooperative resource management. As ICN matures, integration with the trust graph will enable automatic witness selection and reputation tracking, further enhancing the security and usability of this critical feature.

---

## Feedback and Contributions

This document is maintained by the ICN community. To suggest improvements:

1. Open an issue: https://github.com/InterCooperative-Network/icn/issues
2. Submit a PR with documentation updates
3. Discuss in the ICN community forum

**Last Reviewed**: 2026-01-24
