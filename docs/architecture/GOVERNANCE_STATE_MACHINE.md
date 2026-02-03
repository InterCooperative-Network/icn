# Governance State Machine Specification

**Version:** 1.0  
**Status:** NORMATIVE  
**Last Updated:** 2026-02-02

## Abstract

This document specifies the governance state machine for ICN federations. It defines the state representation, state derivation rules, Merkle tree structure, state transition function, and genesis state requirements.

## Keywords

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

---

## 1. Overview

### 1.1 Purpose

The governance state machine is a deterministic finite state machine where:
- **State** = Current federation configuration (members, balances, proposals, constitution)
- **Transitions** = Governance proofs attesting to federation actions
- **Output** = New state with updated configuration

### 1.2 Properties

The state machine MUST be:
- **Deterministic**: Same inputs produce same outputs
- **Verifiable**: State roots enable cryptographic verification
- **Replayable**: Full history can be replayed from genesis
- **Partition-safe**: Conflicting states can be detected and reconciled

---

## 2. State Representation

### 2.1 GovernanceState Structure

```rust
use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GovernanceState {
    /// Federation identifier
    pub federation_id: String,
    
    /// Current sequence number
    pub sequence: u64,
    
    /// Current timestamp (Unix seconds)
    pub timestamp: u64,
    
    /// Current state root (computed)
    pub state_root: [u8; 32],
    
    /// Member cooperatives (DID -> MemberInfo)
    pub members: BTreeMap<String, MemberInfo>,
    
    /// Credit balances ((DID, currency) -> balance)
    pub balances: BTreeMap<(String, String), i64>,
    
    /// Constitution (current governance rules)
    pub constitution: Constitution,
    
    /// Active proposals (proposal_id -> Proposal)
    pub proposals: BTreeMap<String, Proposal>,
    
    /// Historical decisions (proposal_id -> Decision)
    pub decisions: BTreeMap<String, Decision>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemberInfo {
    pub did: String,
    pub name: String,
    pub constitution_hash: [u8; 32],
    pub joined_at: u64,
    pub governance_weight: u64,
    pub credit_limits: BTreeMap<String, i64>,  // currency -> limit
    pub public_key: [u8; 32],  // Ed25519 public key
    pub status: MemberStatus,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Active,
    Paused,
    Expelled,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Constitution {
    pub version: u64,
    pub hash: [u8; 32],
    pub thresholds: BTreeMap<ActionType, Threshold>,
    pub max_sequence_gap: u64,
    pub proposal_duration_seconds: u64,
    pub emergency_signatories: Vec<String>,  // Sorted DIDs
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Threshold {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: String,
    pub proposer: String,
    pub action: FederationAction,
    pub constitution_hash: [u8; 32],  // Locked at submission
    pub submitted_at: u64,
    pub voting_ends_at: u64,
    pub state: ProposalState,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Open,
    Approved,
    Rejected,
    NoQuorum,
    Expired,
    /// Emergency veto by privileged actor
    Vetoed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Decision {
    pub proposal_id: String,
    pub outcome: DecisionOutcome,
    pub vote_tally: VoteTally,
    pub decided_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Approved,
    Rejected,
    NoQuorum,
    /// Federation-specific: veto by privileged actor (e.g., emergency council)
    Vetoed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VoteTally {
    pub votes_for: u64,
    pub votes_against: u64,
    pub votes_abstain: u64,
    pub eligible_voters: u64,
}
```

---

## 3. State Derivation Rules

### 3.1 Initial State (Genesis)

Genesis state MUST be constructed from:
1. Federation configuration (member list, constitution)
2. Initial sequence number (1)
3. Initial timestamp (federation founding time)
4. Empty balances (all balances = 0)
5. No active proposals

```rust
impl GovernanceState {
    pub fn genesis(
        federation_id: String,
        founding_members: Vec<MemberInfo>,
        constitution: Constitution,
        genesis_timestamp: u64,
    ) -> Result<Self> {
        // Validate ≥2 members
        if founding_members.len() < 2 {
            return Err(Error::InsufficientMembers);
        }
        
        // Build member map (sorted)
        let mut members = BTreeMap::new();
        for member in founding_members {
            members.insert(member.did.clone(), member);
        }
        
        let state = GovernanceState {
            federation_id,
            sequence: 1,
            timestamp: genesis_timestamp,
            state_root: [0u8; 32],  // Computed below
            members,
            balances: BTreeMap::new(),
            constitution,
            proposals: BTreeMap::new(),
            decisions: BTreeMap::new(),
        };
        
        // Compute initial state root
        let state_root = compute_state_root(&state);
        Ok(GovernanceState { state_root, ..state })
    }
}
```

### 3.2 State Transition Function

Given current state `S_n` and governance proof `P_n+1`, derive new state `S_n+1`:

```rust
pub fn apply_proof(
    mut state: GovernanceState,
    proof: &GovernanceProof,
    action: &FederationAction,
) -> Result<GovernanceState> {
    // Verify proof is valid for current state
    verify_governance_proof(proof, action, &state)?;
    
    // Apply action to state
    state = match action {
        FederationAction::SettleCrossCoop { settlements } => {
            apply_settle_cross_coop(state, settlements)?
        }
        
        FederationAction::AdmitMember {
            coop_did,
            coop_name,
            constitution_hash,
            initial_credit_limit,
            currency,
            governance_weight,
            ..
        } => {
            apply_admit_member(
                state,
                coop_did,
                coop_name,
                *constitution_hash,
                *initial_credit_limit,
                currency,
                *governance_weight,
                proof.timestamp,
            )?
        }
        
        FederationAction::ExpelMember { coop_did, final_settlement, .. } => {
            apply_expel_member(state, coop_did, final_settlement.as_ref())?
        }
        
        FederationAction::UpdateConstitution { new_constitution_hash, effective_timestamp, .. } => {
            apply_update_constitution(state, *new_constitution_hash, *effective_timestamp)?
        }
        
        FederationAction::UpdateCreditLimits { updates, .. } => {
            apply_update_credit_limits(state, updates)?
        }
        
        FederationAction::PauseMember { coop_did, .. } => {
            apply_pause_member(state, coop_did)?
        }
        
        FederationAction::ResumeMember { coop_did, .. } => {
            apply_resume_member(state, coop_did)?
        }
        
        FederationAction::RecordDecision { proposal_id, outcome, vote_tally, .. } => {
            apply_record_decision(state, proposal_id, *outcome, vote_tally)?
        }
        
        _ => state,  // Other actions don't modify state
    };
    
    // Update sequence and timestamp
    state.sequence = proof.sequence;
    state.timestamp = proof.timestamp;
    
    // Recompute state root
    state.state_root = compute_state_root(&state);
    
    // Verify state root matches proof
    if state.state_root != proof.state_root {
        return Err(Error::StateRootMismatch {
            expected: proof.state_root,
            actual: state.state_root,
        });
    }
    
    Ok(state)
}
```

### 3.3 Action Application Functions

#### 3.3.1 SettleCrossCoop

```rust
fn apply_settle_cross_coop(
    mut state: GovernanceState,
    settlements: &[Settlement],
) -> Result<GovernanceState> {
    for s in settlements {
        // Update from_coop balance
        let from_key = (s.from_coop.clone(), s.currency.clone());
        let from_balance = state.balances.entry(from_key).or_insert(0);
        *from_balance = from_balance.checked_sub(s.amount)
            .ok_or(Error::IntegerOverflow)?;
        
        // Update to_coop balance
        let to_key = (s.to_coop.clone(), s.currency.clone());
        let to_balance = state.balances.entry(to_key).or_insert(0);
        *to_balance = to_balance.checked_add(s.amount)
            .ok_or(Error::IntegerOverflow)?;
    }
    
    Ok(state)
}
```

#### 3.3.2 Helper: DID Public Key Extraction

```rust
/// Extracts the 32-byte Ed25519 public key from an ICN DID.
/// 
/// DID format: `did:icn:<base58btc-encoded-pubkey>`
/// 
/// # Errors
/// - `InvalidDidFormat`: DID does not start with `did:icn:`
/// - `InvalidBase58`: Identifier is not valid Base58btc
/// - `InvalidKeyLength`: Decoded key is not exactly 32 bytes
fn extract_public_key_from_did(did: &str) -> Result<[u8; 32]> {
    // Validate DID prefix
    if !did.to_lowercase().starts_with("did:icn:") {
        return Err(Error::InvalidDidFormat);
    }
    
    // Extract the identifier portion (after "did:icn:")
    let identifier = &did[8..];
    
    // Base58btc decode
    let decoded = bs58::decode(identifier)
        .into_vec()
        .map_err(|_| Error::InvalidBase58)?;
    
    // Validate length (Ed25519 public key = 32 bytes)
    if decoded.len() != 32 {
        return Err(Error::InvalidKeyLength {
            expected: 32,
            actual: decoded.len(),
        });
    }
    
    // Convert to fixed-size array
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&decoded);
    Ok(pubkey)
}
```

#### 3.3.3 AdmitMember

```rust
fn apply_admit_member(
    mut state: GovernanceState,
    coop_did: &str,
    coop_name: &str,
    constitution_hash: [u8; 32],
    initial_credit_limit: i64,
    currency: &str,
    governance_weight: u64,
    joined_at: u64,
) -> Result<GovernanceState> {
    // Create member info
    let mut credit_limits = BTreeMap::new();
    credit_limits.insert(currency.to_string(), initial_credit_limit);
    
    let member_info = MemberInfo {
        did: coop_did.to_string(),
        name: coop_name.to_string(),
        constitution_hash,
        joined_at,
        governance_weight,
        credit_limits,
        // Extract public key from DID using helper function above
        public_key: extract_public_key_from_did(coop_did)?,
        status: MemberStatus::Active,
    };
    
    state.members.insert(coop_did.to_string(), member_info);
    
    // Initialize balance to 0
    let balance_key = (coop_did.to_string(), currency.to_string());
    state.balances.insert(balance_key, 0);
    
    Ok(state)
}
```

#### 3.3.4 ExpelMember

```rust
fn apply_expel_member(
    mut state: GovernanceState,
    coop_did: &str,
    final_settlement: Option<&Settlement>,
) -> Result<GovernanceState> {
    // Apply final settlement if present
    if let Some(settlement) = final_settlement {
        state = apply_settle_cross_coop(state, &[settlement.clone()])?;
    }
    
    // Update member status
    if let Some(member) = state.members.get_mut(coop_did) {
        member.status = MemberStatus::Expelled;
        member.governance_weight = 0;
    }
    
    Ok(state)
}
```

#### 3.3.5 UpdateConstitution

```rust
fn apply_update_constitution(
    mut state: GovernanceState,
    new_constitution_hash: [u8; 32],
    effective_timestamp: u64,
) -> Result<GovernanceState> {
    // Update constitution hash
    state.constitution.hash = new_constitution_hash;
    state.constitution.version += 1;
    
    // Note: Full constitution content is fetched separately by hash
    
    Ok(state)
}
```

#### 3.3.6 UpdateCreditLimits

```rust
fn apply_update_credit_limits(
    mut state: GovernanceState,
    updates: &[CreditLimitUpdate],
) -> Result<GovernanceState> {
    for update in updates {
        if let Some(member) = state.members.get_mut(&update.coop_did) {
            member.credit_limits.insert(
                update.currency.clone(),
                update.new_limit,
            );
        }
    }
    
    Ok(state)
}
```

#### 3.3.7 PauseMember / ResumeMember

```rust
fn apply_pause_member(
    mut state: GovernanceState,
    coop_did: &str,
) -> Result<GovernanceState> {
    if let Some(member) = state.members.get_mut(coop_did) {
        member.status = MemberStatus::Paused;
    }
    Ok(state)
}

fn apply_resume_member(
    mut state: GovernanceState,
    coop_did: &str,
) -> Result<GovernanceState> {
    if let Some(member) = state.members.get_mut(coop_did) {
        if member.status == MemberStatus::Paused {
            member.status = MemberStatus::Active;
        }
    }
    Ok(state)
}
```

#### 3.3.8 RecordDecision

```rust
fn apply_record_decision(
    mut state: GovernanceState,
    proposal_id: &str,
    outcome: DecisionOutcome,
    vote_tally: &VoteTally,
) -> Result<GovernanceState> {
    // Update proposal state
    if let Some(proposal) = state.proposals.get_mut(proposal_id) {
        proposal.state = match outcome {
            DecisionOutcome::Approved => ProposalState::Approved,
            DecisionOutcome::Rejected => ProposalState::Rejected,
            DecisionOutcome::NoQuorum => ProposalState::NoQuorum,
            DecisionOutcome::Vetoed => ProposalState::Vetoed,
        };
    }
    
    // Record decision
    let decision = Decision {
        proposal_id: proposal_id.to_string(),
        outcome,
        vote_tally: vote_tally.clone(),
        decided_at: state.timestamp,
    };
    
    state.decisions.insert(proposal_id.to_string(), decision);
    
    Ok(state)
}
```

---

## 4. Merkle Tree Structure

### 4.1 State Root Computation

The state root is a BLAKE3 hash over:
1. Member registry (sorted by DID)
2. Credit balances (sorted by (DID, currency))
3. Constitution hash
4. Proposals (sorted by ID)
5. Decisions (sorted by ID)
6. Sequence metadata

```rust
pub fn compute_state_root(state: &GovernanceState) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    
    // Domain separation
    hasher.update(b"icn-federation:state-root:v1");
    hasher.update(&[0x00]);
    
    // Hash federation ID
    hasher.update(state.federation_id.as_bytes());
    
    // Hash members (sorted by DID)
    let mut members: Vec<_> = state.members.iter().collect();
    members.sort_by_key(|(did, _)| *did);
    
    for (did, member_info) in members {
        hasher.update(did.as_bytes());
        hasher.update(&member_info.governance_weight.to_le_bytes());
        hasher.update(&member_info.constitution_hash);
        hasher.update(&(member_info.status as u8).to_le_bytes());
        
        // Hash credit limits (sorted by currency)
        let mut limits: Vec<_> = member_info.credit_limits.iter().collect();
        limits.sort_by_key(|(currency, _)| *currency);
        for (currency, limit) in limits {
            hasher.update(currency.as_bytes());
            hasher.update(&limit.to_le_bytes());
        }
    }
    
    // Hash balances (sorted by (DID, currency))
    let mut balances: Vec<_> = state.balances.iter().collect();
    balances.sort_by_key(|(key, _)| *key);
    
    for ((did, currency), balance) in balances {
        hasher.update(did.as_bytes());
        hasher.update(currency.as_bytes());
        hasher.update(&balance.to_le_bytes());
    }
    
    // Hash constitution
    hasher.update(&state.constitution.hash);
    hasher.update(&state.constitution.version.to_le_bytes());
    
    // Hash proposals (sorted by ID)
    let mut proposals: Vec<_> = state.proposals.iter().collect();
    proposals.sort_by_key(|(id, _)| *id);
    
    for (id, proposal) in proposals {
        hasher.update(id.as_bytes());
        hasher.update(&proposal.constitution_hash);
        hasher.update(&(proposal.state as u8).to_le_bytes());
    }
    
    // Hash decisions (sorted by ID)
    let mut decisions: Vec<_> = state.decisions.iter().collect();
    decisions.sort_by_key(|(id, _)| *id);
    
    for (id, decision) in decisions {
        hasher.update(id.as_bytes());
        hasher.update(&(decision.outcome as u8).to_le_bytes());
    }
    
    // Hash sequence metadata
    hasher.update(&state.sequence.to_le_bytes());
    hasher.update(&state.timestamp.to_le_bytes());
    
    (*hasher.finalize().as_bytes()).into()
}
```

### 4.2 Incremental Updates

For efficiency, implementations MAY maintain incremental Merkle trees:

```rust
pub struct IncrementalStateRoot {
    member_tree: MerkleTree,
    balance_tree: MerkleTree,
    proposal_tree: MerkleTree,
    decision_tree: MerkleTree,
}

impl IncrementalStateRoot {
    pub fn update_member(&mut self, did: &str, member_info: &MemberInfo) {
        let leaf_hash = hash_member(did, member_info);
        self.member_tree.update(did, leaf_hash);
    }
    
    pub fn root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"icn-federation:state-root:v1");
        hasher.update(&[0x00]);
        hasher.update(&self.member_tree.root());
        hasher.update(&self.balance_tree.root());
        hasher.update(&self.proposal_tree.root());
        hasher.update(&self.decision_tree.root());
        (*hasher.finalize().as_bytes()).into()
    }
}
```

**Note:** Incremental updates are an optimization. The canonical state root computation (§4.1) MUST be used for verification.

---

## 5. State Queries

### 5.1 Member Queries

```rust
impl GovernanceState {
    pub fn get_member(&self, did: &str) -> Option<&MemberInfo> {
        self.members.get(did)
    }
    
    pub fn is_member(&self, did: &str) -> bool {
        self.members.contains_key(did)
    }
    
    pub fn get_active_members(&self) -> Vec<&MemberInfo> {
        self.members.values()
            .filter(|m| m.status == MemberStatus::Active)
            .collect()
    }
    
    pub fn get_total_governance_weight(&self) -> u64 {
        self.members.values()
            .filter(|m| m.status == MemberStatus::Active)
            .map(|m| m.governance_weight)
            .sum()
    }
}
```

### 5.2 Balance Queries

```rust
impl GovernanceState {
    pub fn get_balance(&self, did: &str, currency: &str) -> i64 {
        let key = (did.to_string(), currency.to_string());
        *self.balances.get(&key).unwrap_or(&0)
    }
    
    pub fn get_credit_limit(&self, did: &str, currency: &str) -> i64 {
        self.members.get(did)
            .and_then(|m| m.credit_limits.get(currency))
            .copied()
            .unwrap_or(0)
    }
    
    pub fn get_all_balances(&self, did: &str) -> BTreeMap<String, i64> {
        self.balances.iter()
            .filter_map(|((d, currency), balance)| {
                if d == did {
                    Some((currency.clone(), *balance))
                } else {
                    None
                }
            })
            .collect()
    }
}
```

### 5.3 Proposal Queries

```rust
impl GovernanceState {
    pub fn get_proposal(&self, id: &str) -> Option<&Proposal> {
        self.proposals.get(id)
    }
    
    pub fn get_open_proposals(&self) -> Vec<&Proposal> {
        self.proposals.values()
            .filter(|p| p.state == ProposalState::Open)
            .collect()
    }
    
    pub fn get_decision(&self, proposal_id: &str) -> Option<&Decision> {
        self.decisions.get(proposal_id)
    }
}
```

---

## 6. Genesis State Requirements

### 6.1 Founding Parameters

Genesis state MUST include:

```rust
pub struct FederationGenesis {
    pub federation_id: String,
    pub founding_timestamp: u64,
    pub founding_members: Vec<MemberInfo>,
    pub constitution: Constitution,
    pub initial_balances: BTreeMap<(String, String), i64>,  // Optional
}
```

### 6.2 Validation

```rust
impl FederationGenesis {
    pub fn validate(&self) -> Result<()> {
        // Require ≥2 founding members
        if self.founding_members.len() < 2 {
            return Err(Error::InsufficientMembers {
                required: 2,
                actual: self.founding_members.len(),
            });
        }
        
        // Verify all members are distinct
        let mut dids = HashSet::new();
        for member in &self.founding_members {
            if !dids.insert(&member.did) {
                return Err(Error::DuplicateMember(member.did.clone()));
            }
        }
        
        // Verify all members have governance weight
        for member in &self.founding_members {
            if member.governance_weight == 0 {
                return Err(Error::ZeroGovernanceWeight);
            }
        }
        
        // Verify constitution is valid
        self.constitution.validate()?;
        
        // Verify initial balances sum to zero per currency
        let mut totals: BTreeMap<String, i64> = BTreeMap::new();
        for ((_, currency), balance) in &self.initial_balances {
            let total = totals.entry(currency.clone()).or_insert(0);
            *total = total.checked_add(*balance)
                .ok_or(Error::IntegerOverflow)?;
        }
        
        for (currency, total) in totals {
            if total != 0 {
                return Err(Error::GenesisBalanceViolation { currency, total });
            }
        }
        
        Ok(())
    }
}
```

### 6.3 Bootstrap Process

```rust
pub fn bootstrap_federation(genesis: FederationGenesis) -> Result<GovernanceState> {
    // Validate genesis parameters
    genesis.validate()?;
    
    // Create genesis state
    let mut state = GovernanceState::genesis(
        genesis.federation_id,
        genesis.founding_members,
        genesis.constitution,
        genesis.founding_timestamp,
    )?;
    
    // Apply initial balances
    for ((did, currency), balance) in genesis.initial_balances {
        state.balances.insert((did, currency), balance);
    }
    
    // Recompute state root
    state.state_root = compute_state_root(&state);
    
    Ok(state)
}
```

---

## 7. State Snapshot Format

### 7.1 Snapshot Structure

```rust
#[derive(Serialize, Deserialize)]
pub struct StateSnapshot {
    pub version: u32,
    pub federation_id: String,
    pub sequence: u64,
    pub timestamp: u64,
    pub state_root: [u8; 32],
    pub state: GovernanceState,
    pub signatures: Vec<(String, [u8; 64])>,  // (DID, signature)
}
```

### 7.2 Snapshot Verification

```rust
pub fn verify_snapshot(snapshot: &StateSnapshot) -> Result<()> {
    // Verify version
    if snapshot.version != 1 {
        return Err(Error::UnsupportedSnapshotVersion);
    }
    
    // Recompute state root
    let computed_root = compute_state_root(&snapshot.state);
    if computed_root != snapshot.state_root {
        return Err(Error::StateRootMismatch {
            expected: snapshot.state_root,
            actual: computed_root,
        });
    }
    
    // Verify signatures (≥2/3 of governance weight)
    let total_weight = snapshot.state.get_total_governance_weight();
    let mut signed_weight = 0u64;
    
    let payload = snapshot_signature_payload(snapshot);
    
    for (did, signature) in &snapshot.signatures {
        let member = snapshot.state.get_member(did)
            .ok_or(Error::UnknownMember(did.clone()))?;
        
        // Verify signature
        let verifying_key = VerifyingKey::from_bytes(&member.public_key)?;
        let sig = Signature::from_bytes(signature)?;
        verifying_key.verify(&payload, &sig)?;
        
        signed_weight += member.governance_weight;
    }
    
    // Require ≥2/3 governance weight
    if signed_weight * 3 < total_weight * 2 {
        return Err(Error::InsufficientSignatures {
            signed: signed_weight,
            total: total_weight,
        });
    }
    
    Ok(())
}

fn snapshot_signature_payload(snapshot: &StateSnapshot) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"icn-federation:snapshot:v1");
    hasher.update(&[0x00]);
    hasher.update(&snapshot.state_root);
    hasher.update(&snapshot.sequence.to_le_bytes());
    hasher.update(&snapshot.timestamp.to_le_bytes());
    hasher.finalize().as_bytes().to_vec()
}
```

---

## 8. State Persistence

### 8.1 Storage Requirements

Implementations MUST persist:
- Current state (full GovernanceState)
- All governance proofs (for replay verification)
- State snapshots (for efficient bootstrapping)

Implementations SHOULD persist:
- Indexed queries (members, balances, proposals)
- State history (for auditing and dispute resolution)

### 8.2 Storage Format

**Recommended:** Use Sled or RocksDB for persistent storage:

```rust
pub struct StateStore {
    db: sled::Db,
}

impl StateStore {
    pub fn save_state(&self, state: &GovernanceState) -> Result<()> {
        let key = format!("state:current:{}", state.federation_id);
        let value = bincode::serialize(state)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }
    
    pub fn load_state(&self, federation_id: &str) -> Result<GovernanceState> {
        let key = format!("state:current:{}", federation_id);
        let value = self.db.get(key.as_bytes())?
            .ok_or(Error::StateNotFound)?;
        let state = bincode::deserialize(&value)?;
        Ok(state)
    }
    
    pub fn save_proof(&self, proof: &GovernanceProof) -> Result<()> {
        let key = format!("proof:{}:{}", proof.federation_id, proof.sequence);
        let value = bincode::serialize(proof)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }
}
```

---

## 9. Compliance Checklist

Implementations MUST satisfy:

### 9.1 State Representation

- [ ] Uses `BTreeMap<K, V>` for all map fields
- [ ] Implements deterministic state root computation
- [ ] Maintains sequence and timestamp metadata
- [ ] Tracks member status (Active, Paused, Expelled)

### 9.2 State Transitions

- [ ] Implements deterministic state transition function
- [ ] Validates proofs before applying
- [ ] Uses checked arithmetic for balance updates
- [ ] Updates state root after each transition
- [ ] Verifies state root matches proof

### 9.3 Genesis State

- [ ] Validates ≥2 founding members
- [ ] Validates constitution
- [ ] Validates initial balances (sum to zero per currency)
- [ ] Computes genesis state root

### 9.4 Queries

- [ ] Provides member queries
- [ ] Provides balance queries
- [ ] Provides proposal queries
- [ ] Provides decision queries

### 9.5 Persistence

- [ ] Persists current state
- [ ] Persists all governance proofs
- [ ] Persists state snapshots
- [ ] Supports state replay from genesis

### 9.6 Verification

- [ ] Verifies state snapshots
- [ ] Verifies state root chain
- [ ] Detects and reports forks

---

## 10. Security Considerations

### 10.1 State Manipulation

**Threat:** Attacker modifies state outside governance protocol.

**Mitigation:**
- All state changes MUST go through verified governance proofs
- State root chain enables detection of unauthorized changes

### 10.2 Integer Overflow

**Threat:** Arithmetic overflow corrupts balances.

**Mitigation:**
- Use checked arithmetic for all balance updates
- Use i64 for balances (±9 quintillion range)

### 10.3 State Root Collision

**Threat:** Two different states produce same root.

**Mitigation:**
- BLAKE3 provides 128-bit collision resistance
- Deterministic state root computation prevents manipulation

### 10.4 Snapshot Forgery

**Threat:** Attacker creates fake state snapshot.

**Mitigation:**
- Snapshots require ≥2/3 governance weight signatures
- Signatures cover state root, sequence, timestamp

---

## 11. References

- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) - Key words for RFCs
- [FEDERATION_INTEROP_CONTRACT.md](./FEDERATION_INTEROP_CONTRACT.md) - Protocol specification
- [CANONICAL_ENCODING.md](./CANONICAL_ENCODING.md) - Encoding rules
- [FEDERATION_ACTIONS.md](./FEDERATION_ACTIONS.md) - Action specification

---

**Document Status:** NORMATIVE - Implementations MUST comply with this specification.

**Change History:**
- 2026-02-02: Initial version 1.0
