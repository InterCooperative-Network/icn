# Federation Interoperability Contract

**Version:** 1.0  
**Status:** NORMATIVE  
**Last Updated:** 2026-02-02

## Abstract

This document defines the normative protocol-enforced contract for ICN federations. It specifies the kernel invariants, governance proof structure, verification algorithm, and interoperability requirements that enable adversarial interoperability across ICN implementations.

This specification transforms the ICN federation layer from "interesting architecture" into "protocol law" — a cross-implementation compliance contract that implementations MUST follow to participate in the federation network.

## Keywords

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

---

## 1. Overview

### 1.1 Purpose

The ICN Federation Interoperability Contract defines:
- **Kernel invariants**: Non-overridable protocol rules enforced by all implementations
- **Governance proofs**: Cryptographic attestations of cooperative governance decisions
- **Verification algorithm**: Steps to validate governance proofs across implementations
- **Interoperability requirements**: Compliance checklist for cross-implementation compatibility

### 1.2 Design Principles

1. **Separation of Concerns**: Kernel enforces invariants; governance (Policy Oracle) determines policies
2. **Adversarial Interoperability**: Implementations verify proofs without trusting senders
3. **Deterministic Execution**: Identical inputs produce identical outputs
4. **Cryptographic Accountability**: All actions are cryptographically attributable
5. **Anti-Equivocation**: Double-signing is automatically detected and penalized

### 1.3 Terminology

- **Federation**: Group of ≥2 cooperatives coordinating via a shared governance protocol
- **Cell**: Distributed gossip topic where federation members synchronize state
- **Governance Proof**: Cryptographic attestation of a governance decision
- **Action**: Atomic state transition (e.g., settlement, member admission)
- **State Root**: Merkle root of governance state at a specific sequence number
- **Sequence**: Monotonically increasing counter preventing replay attacks
- **Constitution**: Immutable governance rules locked at proposal submission

---

## 2. Kernel Invariants

These are **non-overridable** protocol rules enforced by the ICN kernel. Governance MAY NOT override these invariants.

> **Architecture Note (Meaning Firewall):** Per ICN's kernel/app separation, the kernel enforces
> constraints WITHOUT understanding their semantic origin. The invariants below are generic
> safety properties that the kernel can enforce blindly. Domain-specific policies (membership
> rules, governance thresholds, etc.) belong in PolicyOracles and are covered in Section 2.5.

### 2.1 Settlement Conservation

**Invariant 2.1.1:** Cross-cooperative settlements MUST sum to zero per currency.

**Invariant 2.3.1:** Cross-cooperative settlements MUST sum to zero per currency.

**Rationale:** Prevents value creation/destruction exploits.

**Enforcement:**
```rust
fn validate_settlement_conservation(settlements: &[Settlement]) -> Result<()> {
    let mut balances: BTreeMap<CurrencyId, i64> = BTreeMap::new();
    
    for s in settlements {
        let from_balance = balances.entry(s.currency.clone()).or_insert(0);
        *from_balance = from_balance.checked_sub(s.amount)
            .ok_or(Error::IntegerOverflow)?;
        
        let to_balance = balances.entry(s.currency.clone()).or_insert(0);
        *to_balance = to_balance.checked_add(s.amount)
            .ok_or(Error::IntegerOverflow)?;
    }
    
    // Verify sum-to-zero per currency
    for (currency, balance) in balances {
        if balance != 0 {
            return Err(Error::ConservationViolation {
                currency,
                imbalance: balance,
            });
        }
    }
    
    Ok(())
}
```

**Invariant 2.1.2:** Settlement amounts MUST be positive integers.

**Rationale:** Prevents negative-amount exploits and ambiguous direction.

### 2.2 Anti-Equivocation

**Invariant 2.2.1:** A cooperative signing two different actions at the same sequence number is AUTOMATICALLY penalized.

**Rationale:** Byzantine fault tolerance requires equivocation detection.

**Penalty Mechanism:**
- Equivocating cooperative's governance weight reduced to 0
- Existing credit balances frozen
- No new settlements accepted from equivocator
- Removal proposal automatically opened

**Detection:**
```rust
fn detect_equivocation(
    proof_a: &GovernanceProof,
    proof_b: &GovernanceProof,
) -> Option<Did> {
    if proof_a.sequence != proof_b.sequence {
        return None;  // Different sequences, no equivocation
    }
    
    if proof_a.action_hash == proof_b.action_hash {
        return None;  // Same action, no equivocation
    }
    
    // Find common signer
    for did in &proof_a.confirmations {
        if proof_b.confirmations.contains(did) {
            return Some(did.clone());  // Equivocating signer found
        }
    }
    
    None
}
```

### 2.3 Arithmetic Safety

**Invariant 2.3.1:** All arithmetic operations MUST use checked arithmetic (no overflow/underflow).

**Rationale:** Prevents integer overflow exploits.

**Enforcement:**
- Use `checked_add`, `checked_sub`, `checked_mul`
- Return `Error::IntegerOverflow` on overflow
- Use i64 for balances (signed to represent debt/credit)

### 2.4 Default Federation Policies

> **Note:** These policies are enforced by the **FederationPolicyOracle** (app layer), not the kernel.
> Federations MAY customize these through their constitution, but the defaults below represent
> recommended practices for cooperative networks.

**Policy 2.4.1 (Membership Minimum):** Federations SHOULD have ≥2 member organizations.

**Rationale:** Prevents single-entity "federations" that violate cooperative principles.

**Policy 2.4.2 (Organization-Level Participation):** Federation actions SHOULD involve organization-level
entities (cooperatives or sub-federations), not individual nodes.

**Rationale:** Federations coordinate between organizations. Individual participation belongs in
cooperative-level governance.

**Policy 2.4.3 (Cell Operation):** Federations SHOULD operate a Cell (distributed gossip topic) for
state synchronization.

**Recommended Cell Properties:**
- **Topic ID**: Derived from federation constitution hash
- **Access Control**: TrustGated (only members can write)
- **Persistence**: Immutable append-only log
- **Replication**: ≥3 nodes per member cooperative

---

## 3. GovernanceProof Structure

### 3.1 Field Specification

```rust
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GovernanceProof {
    /// Federation identifier (normalized)
    pub federation_id: String,
    
    /// Action type (derived from action, not trusted)
    pub action_type: ActionType,
    
    /// Hash of the action (BLAKE3)
    pub action_hash: [u8; 32],
    
    /// Current governance state root (Merkle root)
    pub state_root: [u8; 32],
    
    /// Previous state root (for chain verification)
    pub prev_state_root: [u8; 32],
    
    /// Sequence number (monotonically increasing)
    pub sequence: u64,
    
    /// Unix timestamp (seconds since epoch)
    pub timestamp: u64,
    
    /// Decision records (proposal IDs -> decision hashes)
    pub decision_records: BTreeMap<String, Vec<u8>>,
    
    /// Ed25519 signatures from confirming cooperatives (DID -> signature)
    /// Each confirmer signs the canonical payload independently.
    /// Keys MUST be sorted (BTreeMap ensures this).
    pub signatures: BTreeMap<String, [u8; 64]>,
}
```

### 3.2 Field Constraints

| Field | Type | Constraints |
|-------|------|-------------|
| `federation_id` | String | Normalized federation DID or Cell ID |
| `action_type` | ActionType | Derived from action, see §5 |
| `action_hash` | [u8; 32] | BLAKE3 hash of canonical action |
| `state_root` | [u8; 32] | Merkle root of governance state |
| `prev_state_root` | [u8; 32] | Previous state root (0x00...00 for genesis) |
| `sequence` | u64 | sequence > prev_sequence |
| `timestamp` | u64 | Unix seconds, must be ≤ current_time + 300s |
| `decision_records` | BTreeMap | Optional, sorted keys, bounded size ≤ 10MB |
| `signatures` | BTreeMap<String, [u8; 64]> | Multi-sig: DID → Ed25519 signature, ≥ threshold signers |

### 3.3 Required Fields

All fields are REQUIRED except:
- `decision_records` MAY be empty if no proposals are recorded in this proof

---

## 4. Canonical Signature Payload

### 4.1 Payload Construction

The signature payload MUST be the canonical CBOR encoding of:

```rust
#[derive(Serialize)]
struct SignaturePayload {
    federation_id: String,
    action_hash: [u8; 32],
    state_root: [u8; 32],
    prev_state_root: [u8; 32],
    sequence: u64,
    timestamp: u64,
}
```

**Note:** `decision_records` and `signatures` are EXCLUDED from the signature payload to avoid circular dependencies (signatures can't sign themselves).

### 4.2 Domain Separation

The signature MUST use typed hashing with domain separation:

**Domain:** `icn-federation:governance-proof:v1`

**Signature Computation:**
```rust
fn compute_signature_payload(proof: &GovernanceProof) -> Vec<u8> {
    let payload = SignaturePayload {
        federation_id: proof.federation_id.clone(),
        action_hash: proof.action_hash,
        state_root: proof.state_root,
        prev_state_root: proof.prev_state_root,
        sequence: proof.sequence,
        timestamp: proof.timestamp,
    };
    
    // Canonical CBOR encoding
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&payload, &mut buf).unwrap();
    
    // Typed hash
    typed_hash("icn-federation:governance-proof:v1", &buf)
}

fn sign_proof(proof: &GovernanceProof, signing_key: &SigningKey) -> [u8; 64] {
    let payload_hash = compute_signature_payload(proof);
    signing_key.sign(&payload_hash).to_bytes()
}
```

### 4.3 Multi-Signature Aggregation

For multi-signature proofs:
1. Each cooperative signs the SAME payload hash
2. Signatures are collected in `confirmations` list (sorted by DID)
3. Verification checks ALL signatures against the payload

**Note:** ICN does not use signature aggregation (e.g., BLS). Each signature is verified independently.

---

## 5. Consensus-Visible Sequence

### 5.1 Sequence Commitment

The sequence number MUST be:
- **Monotonically increasing**: `sequence > prev_sequence`
- **Globally visible**: Included in signature payload
- **Partition-safe**: Used for anti-replay across network partitions

### 5.2 Sequence Gaps

Sequence gaps are PERMITTED if:
- Previous state root is valid (matches known state at prev_sequence)
- Gap does not violate constitution's max_sequence_gap parameter

```rust
fn validate_sequence_gap(
    sequence: u64,
    prev_sequence: u64,
    max_gap: u64,
) -> Result<()> {
    if sequence <= prev_sequence {
        return Err(Error::NonMonotonicSequence);
    }
    
    let gap = sequence - prev_sequence - 1;
    if gap > max_gap {
        return Err(Error::ExcessiveSequenceGap { gap, max_gap });
    }
    
    Ok(())
}
```

### 5.3 Sequence Reset

Sequence numbers MUST NOT reset except:
- Genesis of a new federation (sequence = 1)
- Hard fork with new constitution (constitution_version increments)

---

## 6. Governance State Chain

### 6.1 State Root Computation

The state root is a Merkle root over:
1. **Member registry**: `{coop_did -> MemberInfo}`
2. **Credit balances**: `{(coop_did, currency) -> balance}`
3. **Constitution hash**: Current constitution hash
4. **Proposal states**: `{proposal_id -> ProposalState}`
5. **Sequence metadata**: Current sequence, timestamp

```rust
/// Domain-separated BLAKE3 state-root hash.
/// 
/// NOTE: The canonical computation is defined normatively in
/// `GOVERNANCE_STATE_MACHINE.md` §4.1. This is a summary; implementations
/// MUST follow the full specification for interoperability.
fn compute_state_root(state: &GovernanceState) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();

    // Domain separation prefix
    hasher.update(b"icn-federation:state-root:v1");
    hasher.update(&[0x00]);

    // Hash federation ID
    hasher.update(state.federation_id.as_bytes());
    hasher.update(&[0x00]);

    // Hash members (sorted by DID, via BTreeMap iteration order)
    for (did, member_info) in &state.members {
        hasher.update(did.as_bytes());
        hasher.update(&[0x00]);
        hasher.update(&member_info.public_key);
        hasher.update(&member_info.governance_weight.to_le_bytes());
        hasher.update(&[member_info.status as u8]);
    }
    hasher.update(&[0x00]);

    // Hash balances (sorted by (DID, currency), via BTreeMap iteration order)
    for ((did, currency), balance) in &state.balances {
        hasher.update(did.as_bytes());
        hasher.update(&[0x00]);
        hasher.update(currency.as_bytes());
        hasher.update(&[0x00]);
        hasher.update(&balance.to_le_bytes());
    }
    hasher.update(&[0x00]);

    // Hash constitution
    hasher.update(&state.constitution_hash);

    // Hash sequence metadata
    hasher.update(&state.sequence.to_le_bytes());
    hasher.update(&state.timestamp.to_le_bytes());

    *hasher.finalize().as_bytes()
}
```

> **Normative Reference:** See [GOVERNANCE_STATE_MACHINE.md §4.1](./GOVERNANCE_STATE_MACHINE.md#41-state-root-computation) for the complete specification including proposal and decision hashing.

### 6.2 State Root Chain

State roots form an append-only chain:

```
Genesis:  state_root_0 (prev = 0x00...00)
            ↓
Proof 1:  state_root_1 (prev = state_root_0)
            ↓
Proof 2:  state_root_2 (prev = state_root_1)
            ↓
          ...
```

**Chain Validation:**
```rust
fn validate_state_chain(
    proof: &GovernanceProof,
    known_state_root: [u8; 32],
    known_sequence: u64,
) -> Result<()> {
    if proof.sequence == known_sequence + 1 {
        // Sequential proof
        if proof.prev_state_root != known_state_root {
            return Err(Error::StateRootMismatch);
        }
    } else if proof.sequence > known_sequence + 1 {
        // Gap in sequence (acceptable if constitution allows)
        // Requires fetching intermediate proofs or trusted state snapshot
    } else {
        return Err(Error::NonMonotonicSequence);
    }
    
    Ok(())
}
```

### 6.3 Fork Detection

If two proofs have:
- Same `sequence`
- Same `prev_state_root`
- Different `state_root` or `action_hash`

Then a **fork** has occurred. The federation MUST:
1. Identify the equivocating signer(s)
2. Apply automatic penalty (§2.4)
3. Halt new actions until resolution

---

## 7. Constitution Binding

### 7.1 Constitution Lock

The constitution hash MUST be locked at proposal submission time and included in the governance proof.

**Rationale:** Prevents retroactive rule changes after votes are cast.

**Enforcement:**
```rust
struct ProposalContext {
    pub proposal_id: String,
    pub constitution_hash: [u8; 32],  // Locked at submission
    pub submission_timestamp: u64,
}

fn validate_constitution_binding(
    proof: &GovernanceProof,
    proposal_ctx: &ProposalContext,
) -> Result<()> {
    // Derive expected constitution hash from state
    let expected_constitution = derive_constitution_at_sequence(
        proof.sequence,
        proposal_ctx.submission_timestamp,
    )?;
    
    if proposal_ctx.constitution_hash != expected_constitution {
        return Err(Error::ConstitutionMismatch);
    }
    
    Ok(())
}
```

### 7.2 Constitution Versioning

Constitutions MUST include:
- **Version number**: Monotonically increasing
- **Effective timestamp**: When new constitution takes effect
- **Transition rules**: How to migrate from previous version

---

## 8. Decision Record Handling

### 8.1 Structure

Decision records map proposal IDs to decision hashes:

```rust
pub decision_records: BTreeMap<String, Vec<u8>>
```

**Decision Hash:** BLAKE3 of canonical decision structure:

```rust
#[derive(Serialize)]
struct Decision {
    pub proposal_id: String,
    pub outcome: DecisionOutcome,
    pub vote_tally: VoteTally,
    pub timestamp: u64,
}
```

### 8.2 Size Limits

Decision records MUST be bounded:
- Maximum total size: 10 MB per proof
- Maximum decisions per proof: 100
- Maximum decision hash size: 32 bytes (BLAKE3)

**Rationale:** Prevents DoS attacks via oversized proofs.

### 8.3 Verification

Implementations MUST verify:
1. Decision record keys (proposal IDs) are valid UUIDs or federation-local IDs
2. Decision hashes are 32 bytes (BLAKE3)
3. Total size ≤ 10 MB
4. Decision outcomes are consistent with vote tallies

---

## 9. Action Type Derivation

### 9.1 Trust Model

The `action_type` field in GovernanceProof is **informational only** and MUST NOT be trusted.

**Rationale:** Prevents action type spoofing attacks.

### 9.2 Derivation Algorithm

Verifiers MUST derive the action type from the action structure:

```rust
impl FederationAction {
    pub fn action_type(&self) -> ActionType {
        match self {
            FederationAction::SettleCrossCoop { .. } => ActionType::SettleCrossCoop,
            FederationAction::AdmitMember { .. } => ActionType::AdmitMember,
            FederationAction::ExpelMember { .. } => ActionType::ExpelMember,
            FederationAction::UpdateConstitution { .. } => ActionType::UpdateConstitution,
            FederationAction::AllocateResources { .. } => ActionType::AllocateResources,
            FederationAction::RecordExternalTrade { .. } => ActionType::RecordExternalTrade,
            FederationAction::UpdateCreditLimits { .. } => ActionType::UpdateCreditLimits,
            FederationAction::PauseMember { .. } => ActionType::PauseMember,
            FederationAction::ResumeMember { .. } => ActionType::ResumeMember,
            FederationAction::RecordDecision { .. } => ActionType::RecordDecision,
        }
    }
}
```

### 9.3 Verification

```rust
fn verify_action_type(proof: &GovernanceProof, action: &FederationAction) -> Result<()> {
    let derived_type = action.action_type();
    
    // Compare derived type to claimed type (informational check)
    if derived_type != proof.action_type {
        // Log warning but don't fail verification
        warn!("Action type mismatch: claimed {:?}, derived {:?}", 
              proof.action_type, derived_type);
    }
    
    // Use derived type for policy enforcement
    Ok(())
}
```

---

## 10. Verification Modes

### 10.1 Mode Taxonomy

Three verification modes are supported:

1. **Replay Verification**: Full state replay from genesis
2. **Attested Verification**: Trust bootstrap from signed checkpoint
3. **ZkProof Verification**: Zero-knowledge proof of valid state transition (future)

### 10.2 Replay Verification

**Requirements:**
- Access to full history from genesis
- Deterministic state transition function
- Computational resources to replay all proofs

**Algorithm:**
```rust
fn verify_by_replay(proofs: &[GovernanceProof]) -> Result<GovernanceState> {
    let mut state = GovernanceState::genesis();
    
    for proof in proofs {
        // Verify proof is valid for current state
        verify_proof(proof, &state)?;
        
        // Apply action to state
        let action = fetch_action(proof.action_hash)?;
        state = apply_action(state, &action)?;
        
        // Verify new state root matches proof
        if compute_state_root(&state) != proof.state_root {
            return Err(Error::StateRootMismatch);
        }
    }
    
    Ok(state)
}
```

### 10.3 Attested Verification

**Requirements:**
- Signed checkpoint from trusted quorum
- Proof sequence from checkpoint to current state

**Algorithm:**
```rust
fn verify_by_attestation(
    checkpoint: &SignedCheckpoint,
    proofs: &[GovernanceProof],
) -> Result<GovernanceState> {
    // Verify checkpoint signatures (≥2/3 of governance weight)
    verify_checkpoint_signatures(checkpoint)?;
    
    let mut state = checkpoint.state.clone();
    let mut sequence = checkpoint.sequence;
    
    for proof in proofs {
        if proof.sequence <= sequence {
            continue;  // Already included in checkpoint
        }
        
        // Verify proof chain from checkpoint
        verify_proof(proof, &state)?;
        
        let action = fetch_action(proof.action_hash)?;
        state = apply_action(state, &action)?;
        
        if compute_state_root(&state) != proof.state_root {
            return Err(Error::StateRootMismatch);
        }
        
        sequence = proof.sequence;
    }
    
    Ok(state)
}
```

### 10.4 ZkProof Verification (Future)

**Requirements:**
- Zero-knowledge proof system (e.g., PLONK, Halo2)
- Circuit for state transition verification
- Trusted setup or transparent proof system

**Status:** Planned for future versions. Not required for initial interoperability.

---

## 11. Complete Verifier Algorithm

### 11.1 Entry Point

```rust
pub fn verify_governance_proof(
    proof: &GovernanceProof,
    action: &FederationAction,
    current_state: &GovernanceState,
) -> Result<()> {
    // Step 1: Validate proof structure
    validate_proof_structure(proof)?;
    
    // Step 2: Verify action hash
    verify_action_hash(proof, action)?;
    
    // Step 3: Verify action type
    verify_action_type(proof, action)?;
    
    // Step 4: Verify state chain
    validate_state_chain(proof, current_state)?;
    
    // Step 5: Verify sequence
    validate_sequence(proof, current_state)?;
    
    // Step 6: Verify timestamp
    validate_timestamp(proof)?;
    
    // Step 7: Verify signature
    verify_signature(proof, current_state)?;
    
    // Step 8: Verify action-specific constraints
    verify_action_constraints(action, current_state)?;
    
    // Step 9: Verify decision records
    verify_decision_records(proof, current_state)?;
    
    // Step 10: Verify governance threshold
    verify_governance_threshold(proof, current_state)?;
    
    Ok(())
}
```

### 11.2 Step-by-Step Breakdown

#### Step 1: Validate Proof Structure

```rust
fn validate_proof_structure(proof: &GovernanceProof) -> Result<()> {
    // Check required fields are present
    if proof.federation_id.is_empty() {
        return Err(Error::MissingFederationId);
    }
    
    if proof.sequence == 0 {
        return Err(Error::InvalidSequence);
    }
    
    if proof.timestamp == 0 {
        return Err(Error::InvalidTimestamp);
    }
    
    if proof.confirmations.is_empty() {
        return Err(Error::NoConfirmations);
    }
    
    // Verify confirmations are sorted and unique
    for i in 1..proof.confirmations.len() {
        if proof.confirmations[i] <= proof.confirmations[i - 1] {
            return Err(Error::UnsortedConfirmations);
        }
    }
    
    // Verify decision records size
    let decision_records_size: usize = proof.decision_records.iter()
        .map(|(k, v)| k.len() + v.len())
        .sum();
    
    if decision_records_size > 10 * 1024 * 1024 {
        return Err(Error::DecisionRecordsTooLarge);
    }
    
    Ok(())
}
```

#### Step 2: Verify Action Hash

```rust
fn verify_action_hash(
    proof: &GovernanceProof,
    action: &FederationAction,
) -> Result<()> {
    let mut action_copy = action.clone();
    let computed_hash = action_copy.compute_hash();
    
    if computed_hash != proof.action_hash {
        return Err(Error::ActionHashMismatch {
            expected: proof.action_hash,
            actual: computed_hash,
        });
    }
    
    Ok(())
}
```

#### Step 3: Verify Action Type

```rust
fn verify_action_type(
    proof: &GovernanceProof,
    action: &FederationAction,
) -> Result<()> {
    let derived_type = action.action_type();
    
    // Informational check only
    if derived_type != proof.action_type {
        warn!("Action type mismatch: claimed {:?}, derived {:?}",
              proof.action_type, derived_type);
    }
    
    Ok(())
}
```

#### Step 4: Verify State Chain

```rust
fn validate_state_chain(
    proof: &GovernanceProof,
    current_state: &GovernanceState,
) -> Result<()> {
    let expected_prev_root = compute_state_root(current_state);
    
    if proof.prev_state_root != expected_prev_root {
        return Err(Error::StateRootMismatch {
            expected: expected_prev_root,
            actual: proof.prev_state_root,
        });
    }
    
    Ok(())
}
```

#### Step 5: Verify Sequence

```rust
fn validate_sequence(
    proof: &GovernanceProof,
    current_state: &GovernanceState,
) -> Result<()> {
    if proof.sequence <= current_state.sequence {
        return Err(Error::NonMonotonicSequence {
            current: current_state.sequence,
            proof: proof.sequence,
        });
    }
    
    let gap = proof.sequence - current_state.sequence - 1;
    let max_gap = current_state.constitution.max_sequence_gap;
    
    if gap > max_gap {
        return Err(Error::ExcessiveSequenceGap { gap, max_gap });
    }
    
    Ok(())
}
```

#### Step 6: Verify Timestamp

```rust
fn validate_timestamp(proof: &GovernanceProof) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Allow 5 minutes clock skew
    if proof.timestamp > now + 300 {
        return Err(Error::FutureTimestamp {
            proof: proof.timestamp,
            now,
        });
    }
    
    // Reject ancient proofs (older than 1 year)
    if now > proof.timestamp + 31536000 {
        return Err(Error::ExpiredProof {
            proof: proof.timestamp,
            now,
        });
    }
    
    Ok(())
}
```

#### Step 7: Verify Signatures (Multi-Sig)

```rust
fn verify_signatures(
    proof: &GovernanceProof,
    current_state: &GovernanceState,
) -> Result<()> {
    let payload_hash = compute_signature_payload(proof);
    
    // Calculate governance weight of signers
    let mut total_weight = 0u64;
    let required_threshold = current_state.constitution.approval_threshold;
    
    for (signer_did, signature_bytes) in &proof.signatures {
        // Lookup public key from state
        let member_info = current_state.members.get(signer_did)
            .ok_or(Error::UnknownMember(signer_did.clone()))?;
        
        // Verify member is active
        if member_info.status != MemberStatus::Active {
            return Err(Error::InactiveSigner(signer_did.clone()));
        }
        
        let verifying_key = VerifyingKey::from_bytes(&member_info.public_key)?;
        let signature = Signature::from_bytes(signature_bytes)?;
        
        // Verify this signer's signature (Ed25519)
        verifying_key.verify(&payload_hash, &signature)?;
        
        // Accumulate governance weight
        total_weight = total_weight.checked_add(member_info.governance_weight)
            .ok_or(Error::IntegerOverflow)?;
    }
    
    // Verify quorum is met
    if total_weight < required_threshold {
        return Err(Error::InsufficientQuorum {
            required: required_threshold,
            actual: total_weight,
        });
    }
    
    Ok(())
}
```

**Note:** Each confirming cooperative signs the canonical payload independently. The `signatures` field maps signer DID to their Ed25519 signature. Verification ensures:
1. Each signature is valid for the signer's public key
2. All signers are active members
3. Total governance weight meets the threshold

#### Step 8: Verify Action-Specific Constraints

```rust
fn verify_action_constraints(
    action: &FederationAction,
    current_state: &GovernanceState,
) -> Result<()> {
    match action {
        FederationAction::SettleCrossCoop { settlements } => {
            validate_settlement_conservation(settlements)?;
            
            for s in settlements {
                // Verify members exist
                if !current_state.members.contains_key(&s.from_coop) {
                    return Err(Error::UnknownMember(s.from_coop.clone()));
                }
                if !current_state.members.contains_key(&s.to_coop) {
                    return Err(Error::UnknownMember(s.to_coop.clone()));
                }
                
                // Verify no self-settlement
                if s.from_coop == s.to_coop {
                    return Err(Error::SelfSettlement);
                }
                
                // Verify credit limits
                let from_balance = current_state.get_balance(&s.from_coop, &s.currency);
                let new_balance = from_balance.checked_sub(s.amount)
                    .ok_or(Error::IntegerOverflow)?;
                
                let credit_limit = current_state.get_credit_limit(&s.from_coop, &s.currency);
                if new_balance < -credit_limit {
                    return Err(Error::CreditLimitExceeded {
                        coop: s.from_coop.clone(),
                        balance: new_balance,
                        limit: credit_limit,
                    });
                }
            }
        }
        
        FederationAction::AdmitMember { coop_did, .. } => {
            // Verify not already a member
            if current_state.members.contains_key(coop_did) {
                return Err(Error::AlreadyMember(coop_did.clone()));
            }
        }
        
        // ... other action types
        _ => {}
    }
    
    Ok(())
}
```

#### Step 9: Verify Decision Records

```rust
fn verify_decision_records(
    proof: &GovernanceProof,
    current_state: &GovernanceState,
) -> Result<()> {
    for (proposal_id, decision_hash) in &proof.decision_records {
        // Verify proposal exists
        let proposal = current_state.proposals.get(proposal_id)
            .ok_or(Error::UnknownProposal(proposal_id.clone()))?;
        
        // Verify proposal is in votable state
        if !proposal.is_votable() {
            return Err(Error::ProposalNotVotable(proposal_id.clone()));
        }
        
        // Verify decision hash (fetch and hash decision object)
        // Implementation depends on where decisions are stored
    }
    
    Ok(())
}
```

#### Step 10: Verify Governance Threshold

```rust
fn verify_governance_threshold(
    proof: &GovernanceProof,
    current_state: &GovernanceState,
) -> Result<()> {
    let action_type = derive_action_type_from_proof(proof);
    let required_threshold = current_state.constitution.get_threshold(action_type);
    
    // Compute governance weight of confirmations
    let confirmed_weight: u64 = proof.confirmations.iter()
        .filter_map(|did| {
            current_state.members.get(did).map(|m| m.governance_weight)
        })
        .sum();
    
    let total_weight: u64 = current_state.members.values()
        .map(|m| m.governance_weight)
        .sum();
    
    // Verify threshold (e.g., 2/3 supermajority)
    if confirmed_weight * required_threshold.denominator 
       < total_weight * required_threshold.numerator {
        return Err(Error::InsufficientGovernanceWeight {
            confirmed: confirmed_weight,
            total: total_weight,
            threshold: required_threshold,
        });
    }
    
    Ok(())
}
```

---

## 12. Interop Compliance Checklist

Implementations MUST satisfy the following requirements to be interoperable:

### 12.1 Data Structures

- [ ] Uses `BTreeMap<K, V>` for all map fields in signed/hashed objects
- [ ] Uses `BTreeSet<T>` for all set fields in signed/hashed objects
- [ ] Sorts all set-like `Vec<T>` before canonical encoding
- [ ] Implements `Canonicalize` trait for all governance objects
- [ ] Implements `CanonicalEncode` trait

### 12.2 Cryptography

- [ ] Uses BLAKE3 for content hashing
- [ ] Uses Ed25519 for signatures
- [ ] Uses typed hashing with domain separation
- [ ] Verifies signatures against canonical payloads
- [ ] Supports ML-DSA (Dilithium) for post-quantum (optional)

### 12.3 Encoding

- [ ] Uses CBOR for canonical encoding
- [ ] Uses definite-length CBOR encoding
- [ ] Encodes integers in shortest form
- [ ] Sorts CBOR map keys bytewise lexicographically

### 12.4 Identifiers

- [ ] Normalizes DIDs before encoding
- [ ] Normalizes CurrencyIds before encoding
- [ ] Validates identifier formats

### 12.5 Arithmetic

- [ ] Uses checked arithmetic for all consensus-critical operations
- [ ] Validates numeric ranges before encoding
- [ ] Returns errors on overflow/underflow

### 12.6 Protocol Rules

- [ ] Enforces ≥2 member federations
- [ ] Enforces settlement conservation
- [ ] Detects and penalizes equivocation
- [ ] Validates state root chain
- [ ] Validates monotonic sequence
- [ ] Validates timestamp bounds
- [ ] Derives action type from structure
- [ ] Validates governance thresholds

### 12.7 Verification

- [ ] Implements complete verifier algorithm (§11)
- [ ] Supports replay verification mode
- [ ] Supports attested verification mode
- [ ] Handles sequence gaps correctly
- [ ] Detects and reports forks

### 12.8 Test Coverage

- [ ] Includes test vectors for all action types
- [ ] Tests cross-implementation compatibility
- [ ] Tests edge cases (empty collections, max values)
- [ ] Tests attack scenarios (conservation violation, equivocation, etc.)

---

## 13. Security Considerations

### 13.1 Threat Model

**Assumptions:**
- Adversary controls ≤1/3 of governance weight
- Network may partition temporarily
- Adversary may attempt double-spending, equivocation, replay attacks
- Cryptographic primitives (Ed25519, BLAKE3) are secure

### 13.2 Conservation Violation

**Threat:** Attacker creates settlement that doesn't sum to zero.

**Mitigation:**
- Kernel enforces conservation check (§2.3)
- All verifiers independently validate
- Invalid proofs are rejected

### 13.3 Equivocation Attack

**Threat:** Attacker signs two conflicting actions at same sequence.

**Mitigation:**
- Automatic detection (§2.4)
- Automatic penalty (governance weight → 0)
- Provable evidence for dispute resolution

### 13.4 Replay Attack

**Threat:** Attacker reuses old governance proof in new context.

**Mitigation:**
- Sequence numbers prevent replay within same federation
- State root chain prevents cross-sequence replay
- Timestamp bounds prevent ancient replays

### 13.5 Fork Attack

**Threat:** Network partition leads to conflicting state branches.

**Mitigation:**
- State root chain enables fork detection
- Conflicting proofs trigger reconciliation protocol
- Manual resolution if automatic reconciliation fails

### 13.6 Timestamp Manipulation

**Threat:** Attacker claims future or ancient timestamps.

**Mitigation:**
- 5-minute future tolerance for clock skew
- 1-year expiry for ancient proofs
- Reject proofs outside bounds

### 13.7 Signature Malleability

**Threat:** Attacker modifies signature without invalidating.

**Mitigation:**
- Ed25519 signatures are non-malleable
- Canonical encoding prevents payload manipulation

### 13.8 Integer Overflow

**Threat:** Arithmetic overflow corrupts balances.

**Mitigation:**
- Checked arithmetic (§2.5)
- i64 balances support large values (±9 quintillion)
- Explicit overflow errors

---

## 14. References

- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) - Key words for use in RFCs
- [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949) - CBOR
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032) - Ed25519
- [BLAKE3 Specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf)
- [CANONICAL_ENCODING.md](./CANONICAL_ENCODING.md) - Encoding rules
- [FEDERATION_ACTIONS.md](./FEDERATION_ACTIONS.md) - Action specification
- [GOVERNANCE_STATE_MACHINE.md](./GOVERNANCE_STATE_MACHINE.md) - State machine

---

**Document Status:** NORMATIVE - Implementations MUST comply with this specification.

**Change History:**
- 2026-02-02: Initial version 1.0
