# Implementation Gap Analysis

**Version:** 1.0  
**Status:** INFORMATIONAL  
**Last Updated:** 2026-02-02

## Executive Summary

This document analyzes gaps between the ICN Federation Interoperability Contract specification and the current implementation in the `icn` Rust workspace. It identifies specific code locations requiring changes, prioritizes work items, and provides actionable recommendations for achieving full specification compliance.

---

## 1. Critical Gaps (Security/Correctness Issues)

These gaps represent security vulnerabilities or correctness issues that MUST be addressed before federation interoperability can be considered production-ready.

### 1.1 HashMap Usage in Signed/Hashed Structures

**Specification Requirement:**  
All data structures that are signed or hashed MUST use `BTreeMap<K, V>` instead of `HashMap<K, V>` to ensure deterministic serialization (CANONICAL_ENCODING.md §3.1).

**Current State:**  
Multiple crates use `HashMap` in structures that may be signed or hashed:

**Affected Files:**
1. **icn-governance** (8 files):
   - `src/tally.rs` - Vote tallies use HashMap
   - `src/discussion.rs` - Comment tracking uses HashMap
   - `src/delegation.rs` - Delegation tracking uses HashMap
   - `src/store.rs` - Proposal storage uses HashMap
   - `src/steward_store.rs` - Steward data uses HashMap
   - `src/action_item.rs` - Action item tracking uses HashMap
   - `src/charter_store.rs` - Charter storage uses HashMap
   - `src/protocol_store/state.rs` - Protocol state uses HashMap

2. **icn-ledger** (13 files):
   - `src/balance.rs` - Account balances use HashMap
   - `src/types.rs` - Journal entry types use HashMap
   - `src/ledger.rs` - Ledger state uses HashMap
   - `src/entry.rs` - Entry metadata uses HashMap
   - `src/progressive_limits.rs` - Limit tracking uses HashMap
   - `src/dispute.rs` - Dispute tracking uses HashMap
   - `src/treasury.rs` - Treasury state uses HashMap
   - `src/commons_credits.rs` - Commons credit tracking uses HashMap
   - `src/fork_resolution.rs` - Fork state uses HashMap
   - `src/ledger_impl/balances.rs` - Balance implementation uses HashMap
   - `src/oracle/price_feed.rs` - Price data uses HashMap
   - `src/oracle/types.rs` - Oracle types use HashMap
   - `src/freeze.rs` - Freeze state uses HashMap

3. **icn-federation** (6 files):
   - `src/clearing.rs` - Clearing state uses HashMap
   - `src/netting.rs` - Netting state uses HashMap
   - `src/clearing_manager.rs` - Manager state uses HashMap
   - `src/router.rs` - Routing tables use HashMap
   - `src/registry.rs` - Registry uses HashMap
   - `src/agreement/store.rs` - Agreement storage uses HashMap

**Required Changes:**

```rust
// BEFORE (non-deterministic)
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct VoteTally {
    pub votes: HashMap<Did, VoteChoice>,  // ❌ Non-deterministic
}

// AFTER (deterministic)
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
pub struct VoteTally {
    pub votes: BTreeMap<Did, VoteChoice>,  // ✅ Deterministic
}
```

**Priority:** CRITICAL  
**Effort:** Medium (requires careful refactoring and testing)  
**Risk:** High (breaking change to serialization format)

**Recommendation:**
1. Create migration path for existing data
2. Update all affected structs in one coordinated change
3. Add compile-time checks to prevent future HashMap usage in signed contexts
4. Add tests verifying deterministic serialization

---

### 1.2 Missing GovernanceProof Structure

**Specification Requirement:**  
Federations MUST use a `GovernanceProof` structure with specific fields (FEDERATION_INTEROP_CONTRACT.md §3).

**Current State:**  
No `GovernanceProof` struct exists in the codebase. The governance system uses `Proposal` and `Vote` structures but lacks the cryptographic attestation layer.

**Required Changes:**

Create new file: `icn/crates/icn-federation/src/governance_proof.rs`

```rust
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GovernanceProof {
    pub federation_id: String,
    pub action_type: ActionType,
    pub action_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub prev_state_root: [u8; 32],
    pub sequence: u64,
    pub timestamp: u64,
    pub confirmations: Vec<String>,  // Sorted DIDs
    pub decision_records: BTreeMap<String, Vec<u8>>,
    pub signature: [u8; 64],
}

impl GovernanceProof {
    pub fn verify(&self, action: &FederationAction, state: &GovernanceState) -> Result<()> {
        // Implement full verification algorithm (FEDERATION_INTEROP_CONTRACT.md §11)
        todo!()
    }
}
```

**Priority:** CRITICAL  
**Effort:** High (new component, requires integration with existing governance)  
**Risk:** High (fundamental architectural change)

**Recommendation:**
1. Implement `GovernanceProof` structure first
2. Integrate with existing `Proposal` system
3. Add verification logic
4. Update gossip protocol to support proof distribution

---

### 1.3 Missing Typed Hashing with Domain Separation

**Specification Requirement:**  
All hashes MUST use domain separation: `BLAKE3(domain || 0x00 || data)` (CANONICAL_ENCODING.md §4.1.2).

**Current State:**  
Only partial domain separation exists:
- `icn-ledger/src/settlement.rs` line 15-16: Uses `b"icn-ledger:settlement:v1:"` prefix for deduplication
- No systematic typed hashing infrastructure

**Required Changes:**

Create new file: `icn/crates/icn-encoding/src/typed_hash.rs`

```rust
use blake3;

/// Compute a typed hash with domain separation
pub fn typed_hash(domain: &str, data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0x00]);  // Null separator
    hasher.update(data);
    (*hasher.finalize().as_bytes()).into()
}

/// Domain strings for federation protocol
pub mod domains {
    pub const GOVERNANCE_PROOF: &str = "icn-federation:governance-proof:v1";
    pub const FEDERATION_ACTION: &str = "icn-federation:action:v1";
    pub const STATE_ROOT: &str = "icn-federation:state-root:v1";
    pub const CONSTITUTION: &str = "icn-federation:constitution:v1";
    pub const PROPOSAL: &str = "icn-governance:proposal:v1";
    pub const VOTE: &str = "icn-governance:vote:v1";
    pub const SETTLEMENT: &str = "icn-ledger:settlement:v1";
    pub const JOURNAL_ENTRY: &str = "icn-ledger:journal-entry:v1";
}
```

Update `icn-ledger/src/settlement.rs`:

```rust
// BEFORE
const DEDUP_PREFIX: &[u8] = b"icn-ledger:settlement:v1:";
let dedup_key = sha256(DEDUP_PREFIX || receipt_hash);

// AFTER
use icn_encoding::typed_hash;
use icn_encoding::typed_hash::domains;

let dedup_key = typed_hash(domains::SETTLEMENT, &receipt_hash);
```

**Priority:** CRITICAL  
**Effort:** Medium  
**Risk:** Medium (changes hash values, requires migration)

**Recommendation:**
1. Create `typed_hash` infrastructure in `icn-encoding`
2. Migrate existing ad-hoc domain separation
3. Add domain constants for all federation/governance operations
4. Update tests with new hash values

---

### 1.4 Missing Canonical CBOR Encoding

**Specification Requirement:**  
All signed/hashed objects MUST use canonical CBOR encoding with sorted keys (CANONICAL_ENCODING.md §2).

**Current State:**  
- `icn-encoding` crate provides Postcard encoding (not canonical)
- Some structs use JSON serialization (e.g., `CooperativeInfo` in `icn-federation/src/types.rs` line 88)
- No canonical CBOR infrastructure

**Required Changes:**

Update `icn/crates/icn-encoding/Cargo.toml`:

```toml
[dependencies]
ciborium = "0.2"  # Add CBOR support
```

Add to `icn/crates/icn-encoding/src/lib.rs`:

```rust
/// Trait for objects requiring canonical encoding
pub trait Canonicalize {
    fn canonicalize(&mut self);
    fn is_canonical(&self) -> bool;
}

/// Trait for canonical CBOR encoding
pub trait CanonicalEncode: Canonicalize + Serialize {
    fn encode_canonical(&mut self) -> Result<Vec<u8>> {
        self.canonicalize();
        let mut buf = Vec::new();
        ciborium::ser::into_writer(self, &mut buf)?;
        Ok(buf)
    }
    
    fn hash_canonical(&mut self, domain: &str) -> Result<[u8; 32]> {
        let encoded = self.encode_canonical()?;
        Ok(typed_hash(domain, &encoded))
    }
}
```

**Priority:** CRITICAL  
**Effort:** High (affects many structures)  
**Risk:** High (breaking change to serialization)

**Recommendation:**
1. Add CBOR dependency
2. Implement `Canonicalize` and `CanonicalEncode` traits
3. Migrate existing signed structures to CBOR
4. Maintain JSON for human-readable formats (configuration, logging)

---

### 1.5 Missing Settlement Conservation Validation

**Specification Requirement:**  
Cross-cooperative settlements MUST sum to zero per currency (FEDERATION_INTEROP_CONTRACT.md §2.3).

**Current State:**  
`icn-ledger/src/settlement.rs` does NOT validate conservation. It only checks:
- Positive amount (line 72)
- Executor verification (line 79)
- Deduplication (line 132)

**Required Changes:**

Update `icn/crates/icn-ledger/src/settlement.rs`:

```rust
/// Validate settlement conservation for cross-coop settlements
pub fn validate_conservation(settlements: &[SettlementRequest]) -> Result<()> {
    let mut balances: BTreeMap<String, i64> = BTreeMap::new();
    
    for settlement in settlements {
        // Debit from_coop
        let from_balance = balances.entry(settlement.currency.clone()).or_insert(0);
        *from_balance = from_balance.checked_sub(settlement.amount)
            .ok_or(LedgerError::IntegerOverflow)?;
        
        // Credit to_coop
        let to_balance = balances.entry(settlement.currency.clone()).or_insert(0);
        *to_balance = to_balance.checked_add(settlement.amount)
            .ok_or(LedgerError::IntegerOverflow)?;
    }
    
    // Verify sum-to-zero per currency
    for (currency, balance) in balances {
        if balance != 0 {
            return Err(LedgerError::ConservationViolation {
                currency,
                imbalance: balance,
            });
        }
    }
    
    Ok(())
}
```

**Priority:** CRITICAL  
**Effort:** Low  
**Risk:** Low (new validation, doesn't break existing code)

**Recommendation:**
1. Add conservation validation function
2. Call from `SettlementEngine::settle`
3. Add comprehensive tests (including overflow cases)

---

### 1.6 Missing Checked Arithmetic in Settlement

**Specification Requirement:**  
All arithmetic MUST use checked operations (CANONICAL_ENCODING.md §6.1).

**Current State:**  
`icn-ledger/src/settlement.rs` uses `i64` but doesn't explicitly use checked arithmetic for all operations.

**Required Changes:**

Audit all arithmetic in ledger crate:

```rust
// BEFORE (potential overflow)
balance = balance + amount;

// AFTER (checked arithmetic)
balance = balance.checked_add(amount)
    .ok_or(LedgerError::IntegerOverflow)?;
```

**Priority:** CRITICAL  
**Effort:** Medium (requires audit of all arithmetic operations)  
**Risk:** Medium (could expose existing overflow bugs)

**Recommendation:**
1. Enable Clippy lint: `arithmetic_overflow` (already in workspace config)
2. Audit all arithmetic in `icn-ledger`, `icn-governance`, `icn-federation`
3. Replace `+`, `-`, `*` with checked variants in consensus-critical code
4. Add overflow tests

---

## 2. High-Priority Gaps (Required for Interoperability)

These gaps prevent cross-implementation interoperability but don't represent immediate security risks.

### 2.1 Missing State Root Computation

**Specification Requirement:**  
Governance state MUST have a Merkle root (GOVERNANCE_STATE_MACHINE.md §4).

**Current State:**  
No state root computation exists. Governance uses `Proposal` and `Vote` structures without state commitment.

**Required Changes:**

Create new file: `icn/crates/icn-governance/src/state_root.rs`

```rust
use blake3;
use std::collections::BTreeMap;

pub fn compute_state_root(state: &GovernanceState) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"icn-federation:state-root:v1");
    hasher.update(&[0x00]);
    
    // Hash federation ID
    hasher.update(state.federation_id.as_bytes());
    
    // Hash members (sorted)
    for (did, member) in &state.members {
        hasher.update(did.as_bytes());
        hasher.update(&member.governance_weight.to_le_bytes());
        hasher.update(&member.constitution_hash);
    }
    
    // Hash balances (sorted)
    for ((did, currency), balance) in &state.balances {
        hasher.update(did.as_bytes());
        hasher.update(currency.as_bytes());
        hasher.update(&balance.to_le_bytes());
    }
    
    // Hash constitution
    hasher.update(&state.constitution.hash);
    
    // Hash proposals (sorted)
    for (id, proposal) in &state.proposals {
        hasher.update(id.as_bytes());
        hasher.update(&(proposal.state as u8).to_le_bytes());
    }
    
    // Hash sequence metadata
    hasher.update(&state.sequence.to_le_bytes());
    hasher.update(&state.timestamp.to_le_bytes());
    
    (*hasher.finalize().as_bytes()).into()
}
```

**Priority:** HIGH  
**Effort:** High (requires designing state representation)  
**Risk:** Medium

**Recommendation:**
1. Design `GovernanceState` structure
2. Implement state root computation
3. Integrate with `GovernanceProof`
4. Add state transition function

---

### 2.2 Missing Sequence Number System

**Specification Requirement:**  
Governance proofs MUST have monotonically increasing sequence numbers (FEDERATION_INTEROP_CONTRACT.md §5).

**Current State:**  
No sequence number system exists. Proposals have UUIDs but no global ordering.

**Required Changes:**

Add to `GovernanceState`:

```rust
pub struct GovernanceState {
    pub sequence: u64,
    pub prev_state_root: [u8; 32],
    // ... other fields
}

impl GovernanceState {
    pub fn next_sequence(&self) -> u64 {
        self.sequence + 1
    }
    
    pub fn validate_sequence(&self, proof: &GovernanceProof) -> Result<()> {
        if proof.sequence <= self.sequence {
            return Err(Error::NonMonotonicSequence {
                current: self.sequence,
                proof: proof.sequence,
            });
        }
        Ok(())
    }
}
```

**Priority:** HIGH  
**Effort:** Medium  
**Risk:** Low

**Recommendation:**
1. Add sequence field to governance state
2. Initialize to 1 at genesis
3. Increment on each state transition
4. Validate monotonicity in proof verification

---

### 2.3 Missing FederationAction Enum

**Specification Requirement:**  
Federation actions MUST use specific enum variants (FEDERATION_ACTIONS.md §2).

**Current State:**  
No `FederationAction` enum exists. Federation operations are ad-hoc.

**Required Changes:**

Create new file: `icn/crates/icn-federation/src/action.rs`

```rust
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FederationAction {
    SettleCrossCoop {
        settlements: Vec<Settlement>,
    },
    AdmitMember {
        coop_did: String,
        coop_name: String,
        constitution_hash: [u8; 32],
        initial_credit_limit: i64,
        currency: String,
        governance_weight: u64,
        confirmations: Vec<String>,
    },
    ExpelMember {
        coop_did: String,
        reason: String,
        final_settlement: Option<Settlement>,
        confirmations: Vec<String>,
    },
    // ... other variants (see FEDERATION_ACTIONS.md §2.1)
}

impl FederationAction {
    pub fn action_type(&self) -> ActionType {
        match self {
            FederationAction::SettleCrossCoop { .. } => ActionType::SettleCrossCoop,
            FederationAction::AdmitMember { .. } => ActionType::AdmitMember,
            // ... other variants
        }
    }
}
```

**Priority:** HIGH  
**Effort:** High  
**Risk:** Medium

**Recommendation:**
1. Define all action variants per spec
2. Implement action-specific validation
3. Integrate with governance proof system

---

### 2.4 Missing Constitution Binding

**Specification Requirement:**  
Proposals MUST lock constitution hash at submission (FEDERATION_INTEROP_CONTRACT.md §7).

**Current State:**  
`Proposal` structure in `icn-governance/src/proposal.rs` doesn't include constitution hash.

**Required Changes:**

Update `icn/crates/icn-governance/src/proposal.rs`:

```rust
pub struct Proposal {
    pub id: ProposalId,
    pub domain_id: GovernanceDomainId,
    pub proposer: Did,
    pub title: String,
    pub description: String,
    pub payload: ProposalPayload,
    pub state: ProposalState,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    
    // NEW: Constitution hash locked at submission
    pub constitution_hash: [u8; 32],
}

impl Proposal {
    pub fn validate_constitution(&self, current_constitution_hash: [u8; 32]) -> Result<()> {
        if self.constitution_hash != current_constitution_hash {
            return Err(Error::ConstitutionMismatch {
                locked: self.constitution_hash,
                current: current_constitution_hash,
            });
        }
        Ok(())
    }
}
```

**Priority:** HIGH  
**Effort:** Low  
**Risk:** Low

**Recommendation:**
1. Add `constitution_hash` field to `Proposal`
2. Lock at proposal creation
3. Validate before applying governance proof

---

## 3. Medium-Priority Gaps (Correctness/Robustness)

These gaps affect correctness but don't prevent basic functionality.

### 3.1 Missing Identifier Normalization

**Specification Requirement:**  
DIDs and currency IDs MUST be normalized (CANONICAL_ENCODING.md §5).

**Current State:**  
No normalization logic exists. DIDs are used as-is.

**Required Changes:**

Add to `icn/crates/icn-identity/src/did.rs`:

```rust
impl Did {
    pub fn normalize(&self) -> Result<Did> {
        let s = self.0.as_str();
        if !s.starts_with("did:icn:") {
            return Err(Error::InvalidDidPrefix);
        }
        
        let identifier = &s[8..];
        
        // Verify Base58btc encoding
        let decoded = bs58::decode(identifier).into_vec()?;
        if decoded.len() != 32 {
            return Err(Error::InvalidDidLength);
        }
        
        Ok(Did(s.to_string()))
    }
}
```

**Priority:** MEDIUM  
**Effort:** Low  
**Risk:** Low

**Recommendation:**
1. Implement normalization for Did, CurrencyId, CellId
2. Normalize on construction/parsing
3. Add validation tests

---

### 3.2 Missing Equivocation Detection

**Specification Requirement:**  
Automatic equivocation detection and penalty (FEDERATION_INTEROP_CONTRACT.md §2.4).

**Current State:**  
No equivocation detection exists.

**Required Changes:**

Create new file: `icn/crates/icn-federation/src/equivocation.rs`

```rust
pub fn detect_equivocation(
    proof_a: &GovernanceProof,
    proof_b: &GovernanceProof,
) -> Option<Did> {
    if proof_a.sequence != proof_b.sequence {
        return None;
    }
    
    if proof_a.action_hash == proof_b.action_hash {
        return None;
    }
    
    // Find common signer
    for did in &proof_a.confirmations {
        if proof_b.confirmations.contains(did) {
            return Some(did.clone());
        }
    }
    
    None
}

pub fn penalize_equivocator(state: &mut GovernanceState, equivocator: &Did) {
    if let Some(member) = state.members.get_mut(equivocator.as_str()) {
        member.governance_weight = 0;
        member.status = MemberStatus::Paused;
    }
}
```

**Priority:** MEDIUM  
**Effort:** Medium  
**Risk:** Low

**Recommendation:**
1. Implement equivocation detection
2. Track proof history for comparison
3. Add automatic penalty mechanism
4. Add manual override for false positives

---

### 3.3 Missing Decision Record Handling

**Specification Requirement:**  
Decision records with size limits (FEDERATION_INTEROP_CONTRACT.md §8).

**Current State:**  
`GovernanceProof` doesn't exist, so decision records aren't implemented.

**Required Changes:**

Add to `GovernanceProof`:

```rust
pub struct GovernanceProof {
    // ...
    pub decision_records: BTreeMap<String, Vec<u8>>,  // proposal_id -> decision_hash
}

impl GovernanceProof {
    pub fn validate_decision_records(&self) -> Result<()> {
        let total_size: usize = self.decision_records.iter()
            .map(|(k, v)| k.len() + v.len())
            .sum();
        
        if total_size > 10 * 1024 * 1024 {
            return Err(Error::DecisionRecordsTooLarge {
                size: total_size,
                max: 10 * 1024 * 1024,
            });
        }
        
        if self.decision_records.len() > 100 {
            return Err(Error::TooManyDecisionRecords {
                count: self.decision_records.len(),
                max: 100,
            });
        }
        
        Ok(())
    }
}
```

**Priority:** MEDIUM  
**Effort:** Low  
**Risk:** Low

**Recommendation:**
1. Implement decision record validation
2. Add size limits (10MB total, 100 decisions max)
3. Add tests for boundary conditions

---

## 4. Low-Priority Gaps (Optimizations/Future Work)

These gaps are non-essential for initial interoperability.

### 4.1 Missing Incremental State Root

**Specification Requirement:**  
Implementations MAY use incremental Merkle trees (GOVERNANCE_STATE_MACHINE.md §4.2).

**Current State:**  
Not applicable (state root doesn't exist yet).

**Priority:** LOW  
**Effort:** High  
**Risk:** Low

**Recommendation:**  
Implement after basic state root is working. Use standard Merkle tree library.

---

### 4.2 Missing ZkProof Verification Mode

**Specification Requirement:**  
Future support for zero-knowledge proofs (FEDERATION_INTEROP_CONTRACT.md §10.4).

**Current State:**  
Not applicable (future feature).

**Priority:** LOW  
**Effort:** Very High  
**Risk:** Medium

**Recommendation:**  
Defer until replay and attested verification modes are stable.

---

### 4.3 Missing State Snapshot Format

**Specification Requirement:**  
Signed state snapshots for bootstrapping (GOVERNANCE_STATE_MACHINE.md §7).

**Current State:**  
`icn-snapshot` crate exists but doesn't implement federation state snapshots.

**Priority:** LOW  
**Effort:** Medium  
**Risk:** Low

**Recommendation:**  
Implement after governance state machine is stable. Use existing snapshot infrastructure.

---

## 5. Summary of Required Work

### 5.1 By Priority

| Priority | Items | Total Effort |
|----------|-------|--------------|
| CRITICAL | 6 | ~25 weeks |
| HIGH | 4 | ~12 weeks |
| MEDIUM | 3 | ~6 weeks |
| LOW | 3 | ~10 weeks (future) |

### 5.2 By Crate

| Crate | Changes Required | Priority |
|-------|------------------|----------|
| `icn-federation` | GovernanceProof, FederationAction, equivocation | CRITICAL/HIGH |
| `icn-governance` | BTreeMap migration, state root, sequence | CRITICAL/HIGH |
| `icn-ledger` | BTreeMap migration, conservation, checked arithmetic | CRITICAL |
| `icn-encoding` | Canonical CBOR, typed hashing | CRITICAL |
| `icn-identity` | Normalization | MEDIUM |
| `icn-snapshot` | State snapshots | LOW |

### 5.3 Recommended Implementation Order

1. **Phase 1: Foundation (CRITICAL)**
   - Typed hashing infrastructure (`icn-encoding`)
   - Canonical CBOR encoding (`icn-encoding`)
   - BTreeMap migration (`icn-governance`, `icn-ledger`, `icn-federation`)
   - Checked arithmetic audit (`icn-ledger`)

2. **Phase 2: Core Protocol (CRITICAL/HIGH)**
   - `GovernanceProof` structure (`icn-federation`)
   - `FederationAction` enum (`icn-federation`)
   - State root computation (`icn-governance`)
   - Sequence number system (`icn-governance`)

3. **Phase 3: Validation (CRITICAL/HIGH)**
   - Settlement conservation (`icn-ledger`)
   - Constitution binding (`icn-governance`)
   - Proof verification algorithm (`icn-federation`)

4. **Phase 4: Robustness (MEDIUM)**
   - Identifier normalization (`icn-identity`)
   - Equivocation detection (`icn-federation`)
   - Decision record handling (`icn-federation`)

5. **Phase 5: Optimization (LOW, future)**
   - Incremental state roots
   - State snapshots
   - ZkProof support

---

## 6. Migration Strategy

### 6.1 Backward Compatibility

Many changes are breaking. Recommend:
1. Version federation protocol: `v1` (current) → `v2` (spec-compliant)
2. Support both versions during transition period
3. Deprecate `v1` after 6 months
4. Remove `v1` support after 12 months

### 6.2 Data Migration

For existing deployments:
1. Export current state to JSON
2. Transform to spec-compliant format:
   - HashMap → BTreeMap
   - Add missing fields (sequence, state_root, constitution_hash)
   - Normalize identifiers
3. Recompute hashes with typed hashing
4. Import transformed state

### 6.3 Testing Strategy

1. **Unit Tests:** Test each component in isolation
2. **Integration Tests:** Test full verification algorithm
3. **Compatibility Tests:** Test cross-implementation compatibility
4. **Property Tests:** Use QuickCheck/proptest for invariants
5. **Fuzz Tests:** Fuzz proof verification for edge cases

---

## 7. Compliance Tracking

### 7.1 Checklist

Use this checklist to track compliance progress:

**Encoding (CANONICAL_ENCODING.md):**
- [ ] CBOR encoding implemented
- [ ] BTreeMap used in all signed/hashed structs
- [ ] Typed hashing with domain separation
- [ ] Identifier normalization (Did, CurrencyId, CellId)
- [ ] Checked arithmetic everywhere

**Actions (FEDERATION_ACTIONS.md):**
- [ ] FederationAction enum defined
- [ ] All action variants implemented
- [ ] Canonicalize trait implemented
- [ ] Action-specific validation implemented
- [ ] Settlement conservation validated

**Protocol (FEDERATION_INTEROP_CONTRACT.md):**
- [ ] GovernanceProof structure defined
- [ ] Signature payload canonical
- [ ] Sequence number system implemented
- [ ] State root chain implemented
- [ ] Constitution binding implemented
- [ ] Decision records implemented
- [ ] Action type derivation implemented
- [ ] Complete verifier algorithm implemented
- [ ] Equivocation detection implemented

**State Machine (GOVERNANCE_STATE_MACHINE.md):**
- [ ] GovernanceState structure defined
- [ ] State root computation implemented
- [ ] State transition function implemented
- [ ] Genesis state validation implemented
- [ ] State queries implemented

### 7.2 Test Coverage

Target test coverage by component:
- Core structures (GovernanceProof, FederationAction): 100%
- Verification algorithm: 100%
- State transitions: 90%
- Utilities (normalization, hashing): 95%

---

## 8. Open Questions

1. **Migration Timeline:** What's the target date for spec compliance?
2. **Breaking Changes:** Can we accept breaking changes for existing deployments?
3. **Performance:** What's the performance impact of BTreeMap vs HashMap?
4. **Tooling:** Do we need migration tools for existing data?
5. **Coordination:** How do we coordinate changes across multiple crates?

---

## 9. References

- [FEDERATION_INTEROP_CONTRACT.md](./FEDERATION_INTEROP_CONTRACT.md) - Protocol specification
- [CANONICAL_ENCODING.md](./CANONICAL_ENCODING.md) - Encoding rules
- [FEDERATION_ACTIONS.md](./FEDERATION_ACTIONS.md) - Action specification
- [GOVERNANCE_STATE_MACHINE.md](./GOVERNANCE_STATE_MACHINE.md) - State machine specification
- [ICN Codebase Exploration](../development/sessions/) - Development session notes

---

**Document Status:** INFORMATIONAL - This document tracks implementation gaps and is not normative.

**Change History:**
- 2026-02-02: Initial version 1.0 (based on codebase exploration 2026-02-02)

---

## Appendix A: Quick Reference - Files Requiring Changes

**Critical Changes:**

```
icn/crates/icn-encoding/
  ├── src/typed_hash.rs          [NEW] Typed hashing infrastructure
  ├── src/canonical.rs            [NEW] Canonical CBOR traits
  └── Cargo.toml                  [UPDATE] Add ciborium dependency

icn/crates/icn-federation/
  ├── src/governance_proof.rs     [NEW] GovernanceProof structure
  ├── src/action.rs               [NEW] FederationAction enum
  ├── src/equivocation.rs         [NEW] Equivocation detection
  ├── src/clearing.rs             [UPDATE] HashMap → BTreeMap
  ├── src/netting.rs              [UPDATE] HashMap → BTreeMap
  ├── src/clearing_manager.rs     [UPDATE] HashMap → BTreeMap
  ├── src/router.rs               [UPDATE] HashMap → BTreeMap
  ├── src/registry.rs             [UPDATE] HashMap → BTreeMap
  └── src/agreement/store.rs      [UPDATE] HashMap → BTreeMap

icn/crates/icn-governance/
  ├── src/state_root.rs           [NEW] State root computation
  ├── src/state.rs                [NEW] GovernanceState structure
  ├── src/proposal.rs             [UPDATE] Add constitution_hash field
  ├── src/tally.rs                [UPDATE] HashMap → BTreeMap
  ├── src/discussion.rs           [UPDATE] HashMap → BTreeMap
  ├── src/delegation.rs           [UPDATE] HashMap → BTreeMap
  ├── src/store.rs                [UPDATE] HashMap → BTreeMap
  ├── src/steward_store.rs        [UPDATE] HashMap → BTreeMap
  ├── src/action_item.rs          [UPDATE] HashMap → BTreeMap
  ├── src/charter_store.rs        [UPDATE] HashMap → BTreeMap
  └── src/protocol_store/state.rs [UPDATE] HashMap → BTreeMap

icn/crates/icn-ledger/
  ├── src/settlement.rs           [UPDATE] Add conservation validation
  ├── src/balance.rs              [UPDATE] HashMap → BTreeMap, checked arithmetic
  ├── src/types.rs                [UPDATE] HashMap → BTreeMap
  ├── src/ledger.rs               [UPDATE] HashMap → BTreeMap, checked arithmetic
  ├── src/entry.rs                [UPDATE] HashMap → BTreeMap
  ├── src/progressive_limits.rs   [UPDATE] HashMap → BTreeMap
  ├── src/dispute.rs              [UPDATE] HashMap → BTreeMap
  ├── src/treasury.rs             [UPDATE] HashMap → BTreeMap
  ├── src/commons_credits.rs      [UPDATE] HashMap → BTreeMap
  ├── src/fork_resolution.rs      [UPDATE] HashMap → BTreeMap
  ├── src/ledger_impl/balances.rs [UPDATE] HashMap → BTreeMap
  ├── src/oracle/price_feed.rs    [UPDATE] HashMap → BTreeMap
  ├── src/oracle/types.rs         [UPDATE] HashMap → BTreeMap
  └── src/freeze.rs               [UPDATE] HashMap → BTreeMap

icn/crates/icn-identity/
  └── src/did.rs                  [UPDATE] Add normalization
```

**Total Files:** ~40 files requiring changes
