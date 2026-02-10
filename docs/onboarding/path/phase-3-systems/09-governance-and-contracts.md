# 09: Governance and Contracts — Democratic Rule Changes

> **Phase**: 3 | **Tier**: Builder  
> **Patterns introduced**: [State Machine](#state-machine), [Capability-Based Security](#capability-based-security)  
> **Prerequisite**: 08-network-and-gossip.md

## Why This Matters

Cooperatives need to change rules without breaking trust. ICN's governance system enables democratic proposals, voting, and protocol parameter changes — all enforced through the Meaning Firewall.

→ See `manual.md` § "Governance Without Forks" for design rationale.

## What You'll Read

### 1. Proposal Lifecycle: `icn/crates/icn-governance/src/proposal.rs`

```rust
pub enum ProposalState {
    Draft,
    Proposed { voting_ends: Timestamp },
    Active,
    Terminated { reason: String },
}
```

**Transitions**:
- Draft → Proposed (submit for vote)
- Proposed → Active (vote passes)
- Proposed → Terminated (vote fails or expires)
- Active → Terminated (revoked by governance)

**File**: `icn/crates/icn-governance/src/proposal.rs`

### 2. Governance Proof: `icn/crates/icn-governance/src/proof.rs`

Votes and outcomes are cryptographically verifiable:
```rust
pub struct GovernanceProof {
    pub proposal_id: ProposalId,
    pub votes: Vec<SignedVote>,
    pub tally: VoteTally,
    pub outcome: Outcome,
}
```

### 3. CCL Execution with Fuel Limits: `icn/crates/icn-ccl/src/interpreter.rs`

Contracts have bounded execution:
```rust
pub struct Interpreter {
    fuel: u64,
}

impl Interpreter {
    pub fn execute(&mut self, stmt: &Stmt) -> Result<Value, CclError> {
        self.consume_fuel(1)?;
        match stmt {
            Stmt::Expr(expr) => self.eval_expr(expr),
            // ...
        }
    }
    
    fn consume_fuel(&mut self, amount: u64) -> Result<(), CclError> {
        if self.fuel < amount {
            return Err(CclError::OutOfFuel);
        }
        self.fuel -= amount;
        Ok(())
    }
}
```

**Why fuel**: Prevents infinite loops in contract execution.

### 4. Capability-Based Permissions

**File**: `icn/crates/icn-ccl/src/types.rs`

```rust
pub enum Capability {
    ReadLedger,
    WriteLedger,
    ReadTrust,
    ExecuteContract { contract_id: String },
}
```

Contracts declare required capabilities; runtime checks before execution.

### 5. ProtocolParameterStore: `icn/crates/icn-governance/src/protocol_store/state.rs`

Parameters changed via governance:
```rust
pub struct ProtocolParameterStore {
    params: HashMap<String, Parameter>,
}

pub struct Parameter {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub proposal_id: Option<ProposalId>,  // Which proposal changed it
}
```

**Example parameters**: `max_proposal_size`, `min_voting_period`, `trust_decay_rate`

## What You'll Build

→ **Lab**: `labs/lab-08-governance-flow/`

Implement proposal → vote → parameter change → constraint update flow:
- Create proposal to change parameter
- Cast votes
- Tally votes, activate proposal
- Oracle reads new parameter, updates constraints

**Done when**: Governance decision changes oracle behavior, which changes kernel enforcement.

## Checkpoint

You've completed this layer when you can:

1. **Trace governance flow**: Diagram proposal → vote → activation → oracle update
2. **Implement state machine**: Add proposal lifecycle to your lab
3. **Explain capabilities**: Describe why CCL uses capabilities instead of full access
4. **Debug contract execution**: Given "contract out of fuel", explain how to fix

**Artifact**: Lab-08 complete with end-to-end governance flow.

## Deep Reference

→ `reference/module-14-governance-ccl-deep-dive.md` — Full CCL syntax, semantics  
→ `icn/crates/icn-governance/src/` — Source code for proposals, voting  
→ `docs/architecture/GOVERNANCE_STATE_MACHINE.md` — State machine diagrams
