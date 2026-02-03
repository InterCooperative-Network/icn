# Federation Actions Specification

**Version:** 1.0  
**Status:** NORMATIVE  
**Last Updated:** 2026-02-02

## Abstract

This document specifies the FederationAction enum and its canonical encoding rules. Federation actions represent state transitions in the ICN federation protocol, and their deterministic serialization enables cross-implementation verification.

## Keywords

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

---

## 1. Overview

A **FederationAction** represents an atomic state transition in a federation. Actions are:
- **Deterministic**: Same inputs produce same outputs
- **Verifiable**: Governance proofs attest to action validity
- **Typed**: Each action has a unique `ActionType`
- **Canonical**: Byte-identical encoding for identical logical actions

---

## 2. FederationAction Enum

### 2.1 Structure

```rust
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FederationAction {
    /// Cross-cooperative credit settlement
    SettleCrossCoop {
        settlements: Vec<Settlement>,
    },
    
    /// Admit a new member cooperative
    AdmitMember {
        coop_did: String,
        coop_name: String,
        constitution_hash: [u8; 32],
        initial_credit_limit: i64,
        currency: String,
        governance_weight: u64,
        confirmations: Vec<String>,  // Sorted sponsor DIDs
    },
    
    /// Expel a member cooperative
    ExpelMember {
        coop_did: String,
        reason: String,
        final_settlement: Option<Settlement>,
        confirmations: Vec<String>,  // Sorted approver DIDs
    },
    
    /// Update federation constitution
    UpdateConstitution {
        new_constitution_hash: [u8; 32],
        rationale: String,
        effective_timestamp: u64,
        confirmations: Vec<String>,  // Sorted ratifier DIDs
    },
    
    /// Allocate shared resources
    AllocateResources {
        allocations: Vec<ResourceAllocation>,
        rationale: String,
    },
    
    /// Record external trade
    RecordExternalTrade {
        counterparty: String,
        trade_hash: [u8; 32],
        settlements: Vec<Settlement>,
        metadata: BTreeMap<String, String>,
    },
    
    /// Update member credit limits
    UpdateCreditLimits {
        updates: Vec<CreditLimitUpdate>,
        rationale: String,
        confirmations: Vec<String>,  // Sorted approver DIDs
    },
    
    /// Pause member operations (emergency)
    PauseMember {
        coop_did: String,
        reason: String,
        duration_seconds: Option<u64>,
        confirmations: Vec<String>,  // Sorted emergency signatories
    },
    
    /// Resume member operations
    ResumeMember {
        coop_did: String,
        confirmations: Vec<String>,  // Sorted approver DIDs
    },
    
    /// Record governance decision
    RecordDecision {
        proposal_id: String,
        outcome: DecisionOutcome,
        vote_tally: VoteTally,
        decision_hash: [u8; 32],
    },
}
```

### 2.2 Supporting Types

```rust
/// Single credit settlement between two parties
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settlement {
    pub from_coop: String,       // DID (normalized)
    pub to_coop: String,         // DID (normalized)
    pub amount: i64,             // Positive value (from pays to)
    pub currency: String,        // Normalized CurrencyId
}

/// Resource allocation to a cooperative
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResourceAllocation {
    pub recipient: String,       // DID (normalized)
    pub resource_type: String,
    pub quantity: u64,
    pub duration_seconds: Option<u64>,
}

/// Credit limit update for a cooperative
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreditLimitUpdate {
    pub coop_did: String,        // DID (normalized)
    pub currency: String,        // Normalized CurrencyId
    pub new_limit: i64,
    pub effective_timestamp: u64,
}

/// Governance decision outcome
/// 
/// NOTE: This enum MUST match `DecisionOutcome` from GOVERNANCE_STATE_MACHINE.md
/// exactly (same variants, same encoding). This document repeats it for federation
/// action context. Implementations MUST support all four variants.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Approved,
    Rejected,
    NoQuorum,
    Vetoed,  // Federation-specific: veto by privileged actor
}

/// Vote tally for a decision
///
/// NOTE: This struct MUST match `VoteTally` from GOVERNANCE_STATE_MACHINE.md
/// exactly. The `signatories` field MUST be sorted lexicographically for canonical encoding.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VoteTally {
    pub votes_for: u64,
    pub votes_against: u64,
    pub votes_abstain: u64,
    pub eligible_voters: u64,
    pub signatories: Vec<String>,  // Sorted DIDs of voters
}
```

---

## 3. ActionType Enumeration

### 3.1 Definition

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    SettleCrossCoop,
    AdmitMember,
    ExpelMember,
    UpdateConstitution,
    AllocateResources,
    RecordExternalTrade,
    UpdateCreditLimits,
    PauseMember,
    ResumeMember,
    RecordDecision,
}
```

### 3.2 Derivation

The `ActionType` MUST be derived from the action structure, NOT trusted from external input:

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

**Rationale:** Preventing action type spoofing attacks.

---

## 4. Canonicalization Rules

### 4.1 Canonicalize Trait Implementation

```rust
impl Canonicalize for FederationAction {
    fn canonicalize(&mut self) {
        match self {
            FederationAction::SettleCrossCoop { settlements } => {
                // Sort settlements by (from_coop, to_coop, currency)
                settlements.sort_by(|a, b| {
                    (&a.from_coop, &a.to_coop, &a.currency)
                        .cmp(&(&b.from_coop, &b.to_coop, &b.currency))
                });
                
                // Normalize identifiers
                for s in settlements.iter_mut() {
                    s.from_coop = normalize_did(&s.from_coop);
                    s.to_coop = normalize_did(&s.to_coop);
                    s.currency = normalize_currency(&s.currency);
                }
            }
            
            FederationAction::AdmitMember {
                coop_did,
                currency,
                confirmations,
                ..
            } => {
                *coop_did = normalize_did(coop_did);
                *currency = normalize_currency(currency);
                confirmations.sort();
                confirmations.dedup();
            }
            
            FederationAction::ExpelMember {
                coop_did,
                final_settlement,
                confirmations,
                ..
            } => {
                *coop_did = normalize_did(coop_did);
                if let Some(settlement) = final_settlement {
                    settlement.from_coop = normalize_did(&settlement.from_coop);
                    settlement.to_coop = normalize_did(&settlement.to_coop);
                    settlement.currency = normalize_currency(&settlement.currency);
                }
                confirmations.sort();
                confirmations.dedup();
            }
            
            FederationAction::UpdateConstitution {
                confirmations,
                ..
            } => {
                confirmations.sort();
                confirmations.dedup();
            }
            
            FederationAction::AllocateResources { allocations, .. } => {
                // Sort allocations by (recipient, resource_type)
                allocations.sort_by(|a, b| {
                    (&a.recipient, &a.resource_type)
                        .cmp(&(&b.recipient, &b.resource_type))
                });
                
                for alloc in allocations.iter_mut() {
                    alloc.recipient = normalize_did(&alloc.recipient);
                }
            }
            
            FederationAction::RecordExternalTrade {
                settlements,
                metadata,
                ..
            } => {
                settlements.sort_by(|a, b| {
                    (&a.from_coop, &a.to_coop, &a.currency)
                        .cmp(&(&b.from_coop, &b.to_coop, &b.currency))
                });
                
                for s in settlements.iter_mut() {
                    s.from_coop = normalize_did(&s.from_coop);
                    s.to_coop = normalize_did(&s.to_coop);
                    s.currency = normalize_currency(&s.currency);
                }
                
                // BTreeMap is already ordered
            }
            
            FederationAction::UpdateCreditLimits {
                updates,
                confirmations,
                ..
            } => {
                // Sort updates by (coop_did, currency)
                updates.sort_by(|a, b| {
                    (&a.coop_did, &a.currency).cmp(&(&b.coop_did, &b.currency))
                });
                
                for update in updates.iter_mut() {
                    update.coop_did = normalize_did(&update.coop_did);
                    update.currency = normalize_currency(&update.currency);
                }
                
                confirmations.sort();
                confirmations.dedup();
            }
            
            FederationAction::PauseMember {
                coop_did,
                confirmations,
                ..
            } | FederationAction::ResumeMember {
                coop_did,
                confirmations,
            } => {
                *coop_did = normalize_did(coop_did);
                confirmations.sort();
                confirmations.dedup();
            }
            
            FederationAction::RecordDecision { vote_tally, .. } => {
                vote_tally.signatories.sort();
                vote_tally.signatories.dedup();
            }
        }
    }
    
    fn is_canonical(&self) -> bool {
        // NOTE: This is pseudocode for the specification.
        //
        // Production implementations MUST verify:
        // - All DID fields are normalized (lowercase prefix, valid base58btc)
        // - All currency codes are normalized (scope lowercase, symbol uppercase)
        // - All Vec fields representing sets are sorted and deduplicated
        // - All BTreeMap fields have keys in ascending order
        //
        // Returns true only if the action is already in canonical form.
        // Normalization rules are defined in CANONICAL_ENCODING.md §5 (Identifier Normalization)
        // and §3.2 (Set-Like Vectors). These helper methods verify compliance:
        self.verify_dids_normalized() 
            && self.verify_currencies_normalized()
            && self.verify_collections_sorted()
    }
}

impl CanonicalEncode for FederationAction {}
```

### 4.2 Normalization Functions

```rust
fn normalize_did(did: &str) -> String {
    // Lowercase prefix, preserve Base58btc case
    if let Some(pos) = did.rfind(':') {
        let prefix = did[..pos].to_lowercase();
        let identifier = &did[pos + 1..];
        format!("{}:{}", prefix, identifier)
    } else {
        did.to_string()
    }
}

fn normalize_currency(currency: &str) -> String {
    // Format: <scope>:<symbol>
    if let Some(pos) = currency.find(':') {
        let scope = currency[..pos].to_lowercase();
        let symbol = currency[pos + 1..].to_uppercase();
        format!("{}:{}", scope, symbol)
    } else {
        currency.to_uppercase()
    }
}
```

### 4.3 Validation Helper Functions

The following helper functions verify that data is already in canonical form.
These are used by the `is_canonical()` method in §4.1.

```rust
// Note: These are conceptual helpers. Actual implementations should validate
// according to the rules in CANONICAL_ENCODING.md §5 and §3.2.

fn verify_dids_normalized(action: &FederationAction) -> bool {
    // Check that all DID fields use lowercase "did:icn:" prefix
    // while preserving Base58btc identifier case.
    // See CANONICAL_ENCODING.md §5.1 for complete DID normalization rules.
    //
    // Implementation: iterate through all DID fields in the action and verify:
    // 1. Prefix is lowercase: "did:icn:"
    // 2. Optional coop-name component is lowercase: "[a-z0-9-]+"
    // 3. Identifier is valid Base58btc (case-preserved)
    true  // Placeholder
}

fn verify_currencies_normalized(action: &FederationAction) -> bool {
    // Check that all currency identifiers follow normalized format:
    // - Scope is lowercase (e.g., "fed", "global")
    // - Symbol is uppercase (e.g., "USD", "EUR")
    // Format: "scope:SYMBOL" or just "SYMBOL" for scopeless currencies
    // See CANONICAL_ENCODING.md §5.2 for currency normalization rules.
    true  // Placeholder
}

fn verify_collections_sorted(action: &FederationAction) -> bool {
    // Check that all Vec<String> fields representing sets are:
    // 1. Sorted lexicographically
    // 2. Deduplicated (no duplicate entries)
    // See CANONICAL_ENCODING.md §3.2 for set-like vector rules.
    //
    // Also verify that all BTreeMap fields maintain sorted key order
    // (BTreeMap guarantees this by construction, so this is always true).
    true  // Placeholder
}
```

---

## 5. ActionHash Computation

### 5.1 Typed Hashing

```rust
impl FederationAction {
    pub fn compute_hash(&mut self) -> [u8; 32] {
        self.hash_canonical("icn-federation:action:v1")
            .expect("Canonical encoding failed")
    }
}
```

### 5.2 Hash Domain

**Domain:** `icn-federation:action:v1`

**Format:** `BLAKE3(domain || 0x00 || canonical_cbor)`

---

## 6. Action-Specific Constraints

### 6.1 SettleCrossCoop

**Constraints:**
- Sum of settlements MUST be zero per currency (conservation law)
- All DIDs MUST be federation members
- All amounts MUST be positive
- No self-settlements (from_coop ≠ to_coop)

```rust
impl FederationAction {
    pub fn validate_settle_cross_coop(&self, members: &HashSet<String>) -> Result<()> {
        if let FederationAction::SettleCrossCoop { settlements } = self {
            // Check conservation per currency
            let mut balances: BTreeMap<String, i64> = BTreeMap::new();
            
            for s in settlements {
                // Verify membership
                if !members.contains(&s.from_coop) || !members.contains(&s.to_coop) {
                    return Err(Error::NonMemberParticipant);
                }
                
                // Verify no self-settlement
                if s.from_coop == s.to_coop {
                    return Err(Error::SelfSettlement);
                }
                
                // Verify positive amount
                if s.amount <= 0 {
                    return Err(Error::NonPositiveAmount);
                }
                
                // Accumulate balance changes (checked arithmetic)
                let from_balance = balances.entry(s.currency.clone()).or_insert(0);
                *from_balance = from_balance.checked_sub(s.amount)
                    .ok_or(Error::IntegerOverflow)?;
                
                let to_balance = balances.entry(s.currency.clone()).or_insert(0);
                *to_balance = to_balance.checked_add(s.amount)
                    .ok_or(Error::IntegerOverflow)?;
            }
            
            // Verify conservation (sum to zero per currency)
            for (currency, balance) in balances {
                if balance != 0 {
                    return Err(Error::ConservationViolation { currency, imbalance: balance });
                }
            }
            
            Ok(())
        } else {
            Err(Error::WrongActionType)
        }
    }
}
```

### 6.2 AdmitMember

**Constraints:**
- Cooperative DID MUST NOT already be a member
- Constitution hash MUST be valid (32 bytes)
- Initial credit limit MUST be non-negative
- Governance weight MUST be positive
- Confirmations MUST include ≥2/3 of existing members

```rust
impl FederationAction {
    pub fn validate_admit_member(
        &self,
        members: &HashSet<String>,
        total_governance_weight: u64,
    ) -> Result<()> {
        if let FederationAction::AdmitMember {
            coop_did,
            initial_credit_limit,
            governance_weight,
            confirmations,
            ..
        } = self {
            // Verify not already a member
            if members.contains(coop_did) {
                return Err(Error::AlreadyMember);
            }
            
            // Verify non-negative credit limit
            if *initial_credit_limit < 0 {
                return Err(Error::NegativeCreditLimit);
            }
            
            // Verify positive governance weight
            if *governance_weight == 0 {
                return Err(Error::ZeroGovernanceWeight);
            }
            
            // Verify confirmations (2/3 supermajority)
            let confirmed_weight: u64 = confirmations.iter()
                .filter_map(|did| /* lookup weight */ Some(1))
                .sum();
            
            if confirmed_weight * 3 < total_governance_weight * 2 {
                return Err(Error::InsufficientConfirmations);
            }
            
            Ok(())
        } else {
            Err(Error::WrongActionType)
        }
    }
}
```

### 6.3 UpdateConstitution

**Constraints:**
- New constitution hash MUST differ from current constitution
- Effective timestamp MUST be in the future
- Confirmations MUST include ≥3/4 of members (supermajority)

### 6.4 PauseMember

**Constraints:**
- Member MUST be active (not already paused)
- Confirmations MUST include emergency signatories (per constitution)
- Duration MUST be reasonable (≤90 days) or None for indefinite

---

## 7. Canonical Encoding Examples

### 7.1 SettleCrossCoop

**Logical Action:**
```rust
FederationAction::SettleCrossCoop {
    settlements: vec![
        Settlement {
            from_coop: "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
            to_coop: "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd".to_string(),
            amount: 1000,
            currency: "food-coop:HOURS".to_string(),
        },
        Settlement {
            from_coop: "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd".to_string(),
            to_coop: "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
            amount: 1000,
            currency: "tool-coop:CREDITS".to_string(),
        },
    ],
}
```

**After Canonicalization (sorted by from_coop, to_coop, currency):**
```rust
FederationAction::SettleCrossCoop {
    settlements: vec![
        Settlement {
            from_coop: "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
            to_coop: "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd".to_string(),
            amount: 1000,
            currency: "food-coop:HOURS".to_string(),
        },
        Settlement {
            from_coop: "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd".to_string(),
            to_coop: "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
            amount: 1000,
            currency: "tool-coop:CREDITS".to_string(),
        },
    ],
}
```

**Canonical JSON (for human readability):**
```json
{
  "type": "settle_cross_coop",
  "settlements": [
    {
      "from_coop": "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
      "to_coop": "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd",
      "amount": 1000,
      "currency": "food-coop:HOURS"
    },
    {
      "from_coop": "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd",
      "to_coop": "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
      "amount": 1000,
      "currency": "tool-coop:CREDITS"
    }
  ]
}
```

**ActionHash (BLAKE3 of canonical CBOR):**
```
0x7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b
```

### 7.2 AdmitMember

**Logical Action:**
```rust
FederationAction::AdmitMember {
    coop_did: "did:icn:bike-coop:z6Mkp8uK3J7N5cMqD8fG9hL2jR4tV6wX8yA0bC1dE2fG3hI".to_string(),
    coop_name: "Bike Repair Cooperative".to_string(),
    constitution_hash: [0x12; 32],
    initial_credit_limit: 5000,
    currency: "bike-coop:SPOKES".to_string(),
    governance_weight: 100,
    confirmations: vec![
        "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd".to_string(),
        "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
    ],
}
```

**After Canonicalization (confirmations sorted):**
```rust
FederationAction::AdmitMember {
    coop_did: "did:icn:bike-coop:z6Mkp8uK3J7N5cMqD8fG9hL2jR4tV6wX8yA0bC1dE2fG3hI".to_string(),
    coop_name: "Bike Repair Cooperative".to_string(),
    constitution_hash: [0x12; 32],
    initial_credit_limit: 5000,
    currency: "bike-coop:SPOKES".to_string(),
    governance_weight: 100,
    confirmations: vec![
        "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
        "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd".to_string(),
    ],
}
```

**Canonical JSON:**
```json
{
  "type": "admit_member",
  "coop_did": "did:icn:bike-coop:z6Mkp8uK3J7N5cMqD8fG9hL2jR4tV6wX8yA0bC1dE2fG3hI",
  "coop_name": "Bike Repair Cooperative",
  "constitution_hash": "0x1212121212121212121212121212121212121212121212121212121212121212",
  "initial_credit_limit": 5000,
  "currency": "bike-coop:SPOKES",
  "governance_weight": 100,
  "confirmations": [
    "did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
    "did:icn:tool-coop:z6MkfNzT9bU9Ua5fHKwBpWJVN8XEfBD6e7o4kEwV9RxYnRpd"
  ]
}
```

**ActionHash:**
```
0x4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d
```

---

## 8. Compliance Checklist

Implementations MUST satisfy:

### 8.1 Action Structure

- [ ] Uses `BTreeMap<K, V>` for all map fields
- [ ] Uses `Vec<T>` for ordered lists
- [ ] Includes all required fields per action type
- [ ] Uses normalized identifier types (Did, CurrencyId, CellId)

### 8.2 Canonicalization

- [ ] Implements `Canonicalize` trait
- [ ] Sorts all set-like Vec fields
- [ ] Normalizes all identifiers
- [ ] Deduplicates sorted lists
- [ ] Preserves field order as declared in struct

### 8.3 Action Validation

- [ ] Validates SettleCrossCoop conservation
- [ ] Validates AdmitMember supermajority
- [ ] Validates UpdateConstitution supermajority
- [ ] Validates no self-settlements
- [ ] Validates member membership status

### 8.4 Hashing

- [ ] Uses typed hash with domain separation
- [ ] Computes hash over canonical CBOR
- [ ] ActionType derived from structure, not trusted

### 8.5 Test Coverage

- [ ] Test vectors for each action type
- [ ] Cross-implementation hash compatibility tests
- [ ] Edge cases (empty lists, max values, special characters)

---

## 9. Security Considerations

### 9.1 Conservation Violation

**Threat:** Settlements that don't sum to zero create or destroy value.

**Mitigation:**
- Verify conservation per currency in `validate_settle_cross_coop`
- Use checked arithmetic to prevent overflow

### 9.2 Action Type Spoofing

**Threat:** Attacker submits action with mismatched type field.

**Mitigation:**
- Derive `ActionType` from structure using `action_type()` method
- Never trust `ActionType` from external input

### 9.3 Confirmation Replay

**Threat:** Attacker reuses confirmations from previous actions.

**Mitigation:**
- Include action hash in governance proof
- Verify signatures against specific action

### 9.4 Identifier Confusion

**Threat:** Similar DIDs confused in multi-coop settlements.

**Mitigation:**
- Normalize identifiers before comparison
- Validate identifier formats
- Reject non-normalized identifiers

---

## 10. References

- [CANONICAL_ENCODING.md](./CANONICAL_ENCODING.md) - Encoding rules
- [FEDERATION_INTEROP_CONTRACT.md](./FEDERATION_INTEROP_CONTRACT.md) - Protocol specification
- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) - Key words for RFCs

---

**Document Status:** NORMATIVE - Implementations MUST comply with this specification.

**Change History:**
- 2026-02-02: Initial version 1.0
