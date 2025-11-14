# Governance Primitives

**Status**: Design Phase (Phase 13 not started)
**Last Updated**: 2025-01-13

## Purpose

This document specifies the minimal primitives ICN needs to support diverse governance models without prescribing "the" governance system.

**Key Insight**: Different cooperatives use radically different decision-making processes (consensus, sociocracy, delegation, etc.). ICN cannot pick one. Instead, we provide composable primitives in CCL that communities can use to encode their own governance patterns.

---

## Design Principles

1. **Pluggable, not prescriptive**: Governance is a library of patterns, not a single engine
2. **Explicit state machines**: Proposals have clear lifecycle stages
3. **Auditable**: All governance actions are recorded in the ledger/store
4. **Trust-integrated**: Governance can reference trust scores, reputation, history
5. **Evolvable**: Communities can upgrade their governance contracts without forking

---

## CCL Primitives (Proposed)

These are **capabilities** that CCL contracts can invoke:

### Proposal Management
```rust
// Create a new proposal
proposal_create(
    subject: String,        // Human-readable title
    payload_ref: Hash,      // Link to detailed proposal content
    category: String        // "membership", "budget", "policy", etc.
) -> ProposalID

// Cast a vote
proposal_vote(
    id: ProposalID,
    vote: Vote              // Consent | Block(reason) | Abstain
) -> Result

// Query proposal state
proposal_state(id: ProposalID) -> ProposalState {
    id: ProposalID,
    author: DID,
    created_at: u64,
    subject: String,
    votes: Vec<(DID, Vote)>,
    status: ProposalStatus  // Open | Passed | Blocked | Expired
}
```

### Quorum & Threshold Calculations
```rust
// Check if quorum is met (enough participation)
quorum_met(
    voters: Vec<DID>,
    total_members: u64,
    quorum_threshold: f64   // e.g., 0.5 for 50% participation
) -> bool

// Check if vote threshold is met
threshold_met(
    yes: u64,
    no: u64,
    abstain: u64,
    threshold: f64          // e.g., 0.66 for 2/3 majority
) -> bool

// Consent-based: passes unless explicit block
consent_met(
    blocks: Vec<(DID, String)>,  // Blocks with reasons
    allow_blocks: u64              // e.g., 0 for pure consent
) -> bool
```

### Membership & Role Checks
```rust
// Check if a DID has a specific role
has_role(member: DID, role: String) -> bool

// Get all members with a role
members_with_role(role: String) -> Vec<DID>

// Total member count
member_count() -> u64

// Check if DID is an active member
is_member(did: DID) -> bool
```

### Time-Bounded Phases
```rust
// Check if a time period has elapsed
time_elapsed(start: u64, duration: u64) -> bool

// Automatically transition proposal state based on time
// (This might be a runtime behavior, not a CCL primitive)
```

---

## Governance Templates

These are **reference implementations** shipped as `.ccl` files that communities can use or adapt.

### 1. Consensus with Fallback Majority
**Use Case**: Default for small cooperatives (< 30 people)

**Flow**:
1. Proposal opens with 7-day consensus period
2. If no blocks, proposal passes
3. If blocked, trigger deliberation (mandatory discussion)
4. After deliberation, fall back to 2/3 majority vote

**CCL Pseudocode**:
```
contract ConsensusWithFallback {
    rule propose(subject: String, payload: Hash) {
        let id = proposal_create(subject, payload, "general");
        // 7-day timeout, then check consensus
    }

    rule check_consensus(id: ProposalID) {
        require time_elapsed(proposal.created_at, 7 days);

        let blocks = proposal_state(id).votes.filter(v => v is Block);
        if blocks.is_empty() {
            execute_proposal(id);
        } else {
            trigger_deliberation(id);
        }
    }

    rule vote_after_deliberation(id: ProposalID, vote: Vote) {
        require has_role(sender, "member");
        proposal_vote(id, vote);
    }

    rule finalize_majority(id: ProposalID) {
        let votes = proposal_state(id).votes;
        let yes = votes.count(v => v is Consent);
        let no = votes.count(v => v is Block);

        if threshold_met(yes, no, 0, 0.66) {
            execute_proposal(id);
        } else {
            reject_proposal(id);
        }
    }
}
```

---

### 2. Sociocracy-Style Consent
**Use Case**: Organizations prioritizing "good enough for now, safe enough to try"

**Flow**:
1. Proposal opens
2. Members can object with *reasoned objections*
3. If no objections, proposal passes
4. If objections, facilitator works with objector to integrate concerns
5. Re-propose with amendments

**Key Difference from Consensus**: Objections must be *reasoned* (not just "I don't like it"). Aim is to address concerns, not achieve unanimity.

**CCL Pseudocode**:
```
contract SociocratiConsent {
    rule propose(subject: String, payload: Hash) {
        let id = proposal_create(subject, payload, "sociocratic");
        // 5-day objection period
    }

    rule object(id: ProposalID, reason: String) {
        require has_role(sender, "member");
        require reason.len() > 20;  // Must provide reasoning
        proposal_vote(id, Block(reason));
    }

    rule finalize(id: ProposalID) {
        require time_elapsed(proposal.created_at, 5 days);

        let objections = proposal_state(id).votes.filter(v => v is Block);
        if objections.is_empty() {
            execute_proposal(id);
        } else {
            // Trigger facilitated integration process
            require_integration(id, objections);
        }
    }
}
```

---

### 3. Council Delegation
**Use Case**: Larger cooperatives (50+ people) where day-to-day decisions need speed

**Flow**:
1. Members elect a council (5-9 people) via ranked-choice vote
2. Council makes day-to-day operational decisions
3. Major decisions (budget, membership, governance changes) go to full membership
4. Members can recall council with supermajority (2/3)

**CCL Pseudocode**:
```
contract CouncilDelegation {
    rule elect_council(candidates: Vec<DID>) {
        // Ranked-choice voting (needs vote aggregation primitive)
        let winners = ranked_choice_vote(candidates, 7);  // Elect 7 council members
        set_role_batch(winners, "council");
    }

    rule council_decide(subject: String, payload: Hash) {
        require has_role(sender, "council");
        let id = proposal_create(subject, payload, "council");
        // Council proposals pass with simple majority
    }

    rule major_decision(subject: String, payload: Hash) {
        require has_role(sender, "member");
        let id = proposal_create(subject, payload, "major");
        // Full membership vote, 2/3 threshold
    }

    rule recall_council(member: DID, reason: String) {
        let id = proposal_create("Recall council member", hash(member, reason), "recall");
        // 2/3 supermajority required
    }
}
```

---

### 4. Emergency Lock
**Use Case**: Immediate action required (security incident, legal threat, etc.)

**Flow**:
1. Designated responders (2-3 trusted members) can invoke emergency action
2. Action takes effect immediately
3. **Must be ratified by full membership within 48 hours**
4. If not ratified, action is reversed

**CCL Pseudocode**:
```
contract EmergencyLock {
    rule emergency_action(action: String, payload: Hash) {
        require has_role(sender, "emergency_responder");

        let id = proposal_create("EMERGENCY: " + action, payload, "emergency");
        execute_immediately(id);  // Action happens NOW
        set_timeout(id, 48 hours);  // Ratification deadline
    }

    rule ratify_emergency(id: ProposalID) {
        require proposal_state(id).category == "emergency";
        proposal_vote(id, Consent);
    }

    rule check_ratification(id: ProposalID) {
        require time_elapsed(proposal.created_at, 48 hours);

        let votes = proposal_state(id).votes;
        if !threshold_met(votes.yes, votes.no, 0, 0.5) {
            reverse_action(id);  // Undo the emergency action
        }
    }
}
```

---

## Implementation Notes

### Storage

Proposals need persistent storage:
```rust
// icn-store addition
pub struct Proposal {
    id: ProposalID,
    author: DID,
    created_at: u64,
    subject: String,
    payload_ref: Hash,
    category: String,
    votes: Vec<(DID, Vote, u64)>,  // (voter, vote, timestamp)
    status: ProposalStatus,
}

pub enum ProposalStatus {
    Open,
    Passed { executed_at: u64 },
    Blocked { reasons: Vec<String> },
    Expired { timeout_at: u64 },
}
```

### CCL Capability System

Governance primitives require new capabilities:
```rust
// icn-ccl/src/capability.rs
pub enum Capability {
    // Existing
    ReadLedger,
    WriteLedger,
    ReadTrust,

    // New for governance
    CreateProposal,
    VoteProposal,
    ReadProposal,
    ExecuteProposal,
    ManageRoles,
}
```

### Integration with Trust Graph

Governance can weight votes by trust:
```rust
// Trust-weighted voting
rule weighted_vote(id: ProposalID, vote: Vote) {
    let trust_score = trust_graph.compute_trust(sender);
    proposal_vote_weighted(id, vote, trust_score);
}

// Or restrict voting to trusted members
rule trusted_only(id: ProposalID, vote: Vote) {
    require trust_graph.compute_trust(sender) > 0.5;
    proposal_vote(id, vote);
}
```

---

## Open Questions

1. **Ranked-choice voting**: Does CCL need a built-in primitive, or can it be implemented in contract logic?
2. **Proposal execution**: Who triggers `execute_proposal()`? Automated runtime? Manual `icnctl` command?
3. **Vote privacy**: Should votes be public (auditable) or private (sealed until deadline)? Trade-off between transparency and coercion resistance.
4. **Delegation chains**: Can votes be delegated? (Alice delegates to Bob, Bob delegates to Carol)
5. **Retroactive governance**: Can a proposal modify past decisions? (Dangerous but sometimes necessary for corrections)

---

## Next Steps

**Do NOT implement this until:**
1. Phase C1 (pilot community selection) completes
2. We understand what governance model they actually use
3. We can validate these primitives against real workflows

**When ready:**
1. Extend CCL with governance capabilities
2. Implement 1-2 templates that match pilot community needs
3. Test in production with real decisions
4. Iterate based on what breaks

**Success Criteria**:
- Pilot community can encode their existing governance in ICN
- Decisions made via ICN contracts are considered legitimate by members
- Governance overhead is *lower* than pre-ICN (not higher)

---

**Contributors**: This is a living design doc. Add use cases, critique primitives, propose alternatives.
