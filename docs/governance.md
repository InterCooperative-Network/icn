# Governance Primitives

**Status**: Phase 13 – Governance MVC
**Audience**: Cooperative members, facilitators, and ICN developers

## 1. Overview

ICN governance is **not** "one true DAO."
It is a set of **governance runtime primitives** that communities can assemble into their own decision systems.

Design goals:

- **Democratic by default**
  Ship with a boring, cooperative baseline that "just works" without anyone having to hand-roll a constitution.

- **Configurable by communities**
  Each community (governance domain) defines its own membership rules and decision thresholds.

- **Extensible over time**
  New governance profiles can be added, and communities can migrate to them using proposals.

- **Mechanisms, not ideology**
  ICN provides the mechanics: proposals, votes, outcomes. How those are used is a political choice of the community.

At a high level, the system is structured as:

1. **Membership** – who is allowed to vote
2. **Tally** – how votes are counted
3. **Outcome** – how tallies are turned into decisions
4. **Execution** – what the system does when a proposal is accepted

This separation keeps the core small and lets future governance models plug in without rewriting everything.

---

## 2. Core Types

### 2.1 Governance Domains

A **governance domain** represents a space where decisions are made. This usually maps to a single cooperative, working group, or project.

```rust
pub struct GovernanceDomainId(pub String);

pub struct GovernanceDomain {
    pub id: GovernanceDomainId,
    pub name: String,
    pub config: GovernanceConfig,
}
```

Examples:

* `coop:tiny-food`
* `timebank:city-center`
* `did:icn:org123…` (org DID as domain id)

Each domain has its own membership configuration and decision thresholds.

---

### 2.2 Membership

Membership answers the question: **"Who is allowed to vote in this domain?"**

```rust
pub enum MembershipSource {
    /// Explicit list of member DIDs
    StaticList(Vec<Did>),

    /// Trust-based membership: any DID whose trust score is above a threshold
    TrustThreshold(f64),

    // Future: roles, contract-defined groups, etc.
}

pub struct MembershipConfig {
    pub source: MembershipSource,
}
```

Phase 13 focuses primarily on `StaticList`, with `TrustThreshold` scaffolded for future use (e.g., "everyone trusted at least X by the org DID").

---

### 2.3 Governance Configuration & Profiles

A **governance profile** is a rule set that decides how tallies become outcomes.

```rust
pub struct GovernanceProfileId(pub String);
// e.g. "cooperative_default"

pub struct GovernanceParams {
    /// Quorum as a percentage of eligible voters (0–100)
    pub quorum_percentage: u8,
    /// Required percentage of For vs Against for approval (0–100)
    pub approval_threshold_percentage: u8,
    /// Voting period in seconds (used by higher layers / scheduling)
    pub voting_period_seconds: u64,
}
```

These parameters are wrapped in a higher-level `GovernanceConfig`:

```rust
pub struct GovernanceConfig {
    pub profile: GovernanceProfileId,
    pub membership: MembershipConfig,
    pub params: GovernanceParams,
}
```

This is the object communities can change over time (via governance proposals).

---

### 2.4 Proposals

A **proposal** is a decision being made within a governance domain.

```rust
pub struct ProposalId(pub String);

pub enum ProposalState {
    Draft,
    Open { opened_at: Timestamp, closes_at: Timestamp },
    Accepted { closed_at: Timestamp },
    Rejected { closed_at: Timestamp },
    NoQuorum { closed_at: Timestamp },
    Cancelled { cancelled_at: Timestamp },
}
```

Proposal payloads describe the *type* of decision:

```rust
pub enum ProposalPayload {
    /// Informational / advisory decisions
    Text {
        body: String,
    },

    /// Budget or resource allocation decisions
    Budget {
        amount: i64,
        currency: String,
        recipient: Did,
        purpose: String,
    },

    /// Membership changes (add/remove members)
    Membership {
        action: MembershipAction,
        member: Did,
    },

    /// Changes to governance config (thresholds, profile, membership rules)
    ConfigChange {
        /// Serialized representation of the new configuration
        new_config: String,
    },
}
```

Combined into:

```rust
pub struct Proposal {
    pub id: ProposalId,
    pub domain_id: GovernanceDomainId,
    pub proposer: Did,

    pub title: String,
    pub description: String,
    pub payload: ProposalPayload,

    pub created_at: Timestamp,
    pub updated_at: Timestamp,

    pub state: ProposalState,
}
```

Phase 13 provides structured payloads for common decision types and a generic `ConfigChange` for evolving governance itself.

---

### 2.5 Votes and Tallies

A **vote** is a member's position on a proposal.

```rust
pub enum VoteChoice {
    For,
    Against,
    Abstain,
}

pub struct Vote {
    pub proposal_id: ProposalId,
    pub voter: Did,
    /// Weight of this vote (Phase 13: typically 1 for 1-member-1-vote)
    pub weight: u64,
    pub choice: VoteChoice,
    pub timestamp: Timestamp,
    pub comment: Option<String>,
}
```

Votes are aggregated into a **tally**:

```rust
pub struct VoteTally {
    pub for_votes: usize,
    pub against_votes: usize,
    pub abstain_votes: usize,
}

impl VoteTally {
    pub fn total_votes(&self) -> usize {
        self.for_votes + self.against_votes + self.abstain_votes
    }

    pub fn deciding_votes(&self) -> usize {
        self.for_votes + self.against_votes
    }

    pub fn approval_percentage(&self) -> u8 {
        // Returns percentage of For among deciding votes (0-100)
    }
}
```

**Note:**

* **Eligible voter count** is not stored on `VoteTally` – it is provided separately when we evaluate outcomes (see below).
* Phase 13 assumes 1-member-1-vote semantics, but uses `weight: u64` to keep room for future weighted models.
* Vote weights are summed during tally computation, allowing future extensions.

---

### 2.6 Decision Outcomes & Rules

The result of a closed proposal is represented as:

```rust
pub enum DecisionOutcome {
    Accepted,
    Rejected,
    NoQuorum,
}
```

Decision logic is provided by **governance profiles** via a trait:

```rust
pub trait GovernanceRule: Send + Sync {
    fn evaluate(
        &self,
        tally: &VoteTally,
        params: &GovernanceParams,
        eligible_voter_count: usize,
    ) -> Result<DecisionOutcome>;

    fn profile_id(&self) -> &GovernanceProfileId;
    fn description(&self) -> &str;
}
```

* `tally` holds the For/Against/Abstain counts.
* `params` is the configured quorum and approval thresholds.
* `eligible_voter_count` is the number of members allowed to vote in this domain.

Different profiles implement `GovernanceRule`. The cooperative default profile is just one such implementation.

---

## 3. The `cooperative_default` Profile

The **`cooperative_default`** profile is the boring, democratic baseline:

* Members are defined by a `MembershipConfig` (usually a static list of DIDs).
* Each member's vote is counted once (1-member-1-vote).
* Quorum is enforced as a percentage of eligible voters.
* Approval is enforced as a percentage of For vs Against among participating voters.

### 3.1 Default Parameters

The built-in defaults are:

```text
quorum_percentage              = 50   # at least 50% of eligible members must vote
approval_threshold_percentage  = 50   # simple majority of For vs Against
voting_period_seconds          = 604800  # 7 days
```

### 3.2 Evaluation Logic

The `cooperative_default` profile implements `GovernanceRule::evaluate` as follows:

1. **Quorum check**

   * Compute `quorum_required = (eligible_voter_count * quorum_percentage) / 100`.
   * If `tally.total_votes() < quorum_required` → `DecisionOutcome::NoQuorum`.

2. **Approval check**

   * Compute `approval_required = (total_votes * approval_threshold_percentage) / 100`.
   * If `tally.for_votes > approval_required` → `DecisionOutcome::Accepted`.
   * Otherwise → `DecisionOutcome::Rejected`.

**Key behaviors:**

* **Abstain votes** count towards **quorum** but not towards the approval calculation.
* The approval threshold is computed as `>` (strictly greater than), ensuring a true majority at 50%.
* Edge case: If all votes are abstentions, quorum is met but approval fails → `Rejected`.

The exact implementation includes comprehensive unit tests covering boundary conditions:
- Exact threshold matches (49%, 50%, 51%)
- All-abstain scenarios
- Zero participation
- Multi-currency weighted votes (future)

---

## 4. Proposal Lifecycle

The governance system enforces a clear state machine:

```text
Draft → Open → {Accepted, Rejected, NoQuorum}
          ↘ Cancelled
```

* **Draft**
  Proposal exists but is not yet open for voting. Can be edited by the proposer.

* **Open { opened_at, closes_at }**
  Voting is active. Eligible members can cast **For / Against / Abstain**.
  The `closes_at` timestamp indicates when voting ends.

* **Accepted { closed_at }**
  Proposal passed both quorum and approval thresholds.

* **Rejected { closed_at }**
  Proposal met quorum but failed approval threshold.

* **NoQuorum { closed_at }**
  Insufficient participation.

* **Cancelled { cancelled_at }**
  Proposal was intentionally cancelled (e.g. withdrawn by proposer, superseded, or invalid).

**State transitions:**

```rust
impl Proposal {
    pub fn open(&mut self, voting_period_seconds: u64) -> Result<()>;
    pub fn close(&mut self, final_state: ProposalState) -> Result<()>;
    pub fn cancel(&mut self) -> Result<()>;
}
```

Transitions are validated:
- Can only `open()` from `Draft`
- Can only `close()` from `Open`
- Can only `cancel()` from non-terminal states

---

## 5. Gossip, Storage, and Integration

Phase 13 introduces three integration layers:

### 5.1 Gossip Protocol

A **`GovernanceMessage`** enum is broadcast on the `governance:proposal` topic:

```rust
pub enum GovernanceMessage {
    DomainCreated { domain: GovernanceDomain },
    DomainUpdated { domain: GovernanceDomain },
    ProposalCreated { proposal: Proposal },
    ProposalOpened { id: ProposalId, opened_at: u64, closes_at: u64 },
    VoteCast { vote: Vote, signature: Option<Vec<u8>> },
    ProposalClosed { id: ProposalId, outcome: ProposalOutcome, closed_at: u64, tally: TallySnapshot },
    ProposalCancelled { id: ProposalId, cancelled_by: Did, cancelled_at: u64 },
}
```

**TallySnapshot** provides an immutable record of the final vote count:

```rust
pub struct TallySnapshot {
    pub for_votes: usize,
    pub against_votes: usize,
    pub abstain_votes: usize,
    pub eligible_voters: usize,
}
```

This enables independent outcome verification by any node.

### 5.2 Storage Layer

A **`GovernanceStore`** trait abstracts persistence:

```rust
pub trait GovernanceStore: Send + Sync {
    fn store_domain(&self, domain: &GovernanceDomain) -> Result<()>;
    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    fn store_proposal(&self, proposal: &Proposal) -> Result<()>;
    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>>;
    fn list_proposals(&self, domain_id: &GovernanceDomainId) -> Result<Vec<Proposal>>;

    fn store_vote(&self, vote: &Vote) -> Result<()>;
    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>>;
    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>>;

    fn compute_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally>;
}
```

Phase 13 includes:
- `InMemoryGovernanceStore` for testing
- `SledGovernanceStore` scaffold for production (not yet complete)

**Vote replacement:** The store allows voters to change their votes - calling `store_vote()` with the same `(proposal_id, voter)` replaces the previous vote. This reflects the democratic principle that voters can change their minds before voting closes.

### 5.3 Membership Resolution

A **`MembershipResolver`** trait determines who can vote:

```rust
pub trait MembershipResolver: Send + Sync {
    fn resolve_members(&self, domain: &GovernanceDomain) -> Result<Vec<Did>>;
    fn is_member(&self, domain: &GovernanceDomain, did: &Did) -> Result<bool>;
    fn member_count(&self, domain: &GovernanceDomain) -> Result<usize>;
}
```

Phase 13 implementations:
- **StaticMembershipResolver**: Handles `MembershipSource::StaticList`
- **TrustMembershipResolver**: Scaffold for trust graph integration (not yet wired)
- **CompositeMembershipResolver**: Tries multiple strategies in sequence

---

## 6. How Communities Evolve Their Governance

Because **governance configuration is itself governed**, communities can:

1. **Start simple**: Use `cooperative_default` with a static member list and 50/50 thresholds.

2. **Propose changes** via `ProposalPayload::ConfigChange`:
   * Increase quorum to 67%
   * Require supermajority (when that profile exists)
   * Switch to trust-based membership
   * Migrate to a different profile entirely

3. **Vote using current rules**: The config change proposal is evaluated under the *existing* governance rules.

4. **Apply if accepted**: The new configuration takes effect for future proposals.

Every governance evolution is transparent, auditable, and requires community consent.

---

## 7. CLI Design (Planned)

Phase 13 will provide `icnctl gov` commands for governance workflows:

### Domain Management

```bash
# Create a new governance domain
icnctl gov domain create \
  --domain-id "coop:tiny-food" \
  --name "Tiny Food Cooperative" \
  --members did:icn:alice,did:icn:bob,did:icn:carol \
  --profile cooperative_default \
  --quorum 50 \
  --approval 50

# Show domain configuration
icnctl gov domain show --domain-id "coop:tiny-food"

# List all domains
icnctl gov domain list
```

### Proposal Workflow

```bash
# Create a text proposal
icnctl gov proposal create \
  --domain-id "coop:tiny-food" \
  --title "Approve new supplier" \
  --description "Add Local Veggie Farm as approved supplier" \
  --payload text

# Create a budget proposal
icnctl gov proposal create \
  --domain-id "coop:tiny-food" \
  --title "Q1 Marketing Budget" \
  --payload budget \
  --amount 5000 \
  --currency USD \
  --recipient did:icn:marketing \
  --purpose "Social media campaign"

# Open proposal for voting (7 days from now)
icnctl gov proposal open <proposal-id> --duration 7d

# List proposals
icnctl gov proposal list --domain-id "coop:tiny-food"
icnctl gov proposal list --domain-id "coop:tiny-food" --state open

# Show proposal details
icnctl gov proposal show <proposal-id>
```

### Voting

```bash
# Cast a vote
icnctl gov vote cast <proposal-id> for
icnctl gov vote cast <proposal-id> against
icnctl gov vote cast <proposal-id> abstain

# Add a comment to your vote
icnctl gov vote cast <proposal-id> for --comment "Strong support for this initiative"

# Show your vote on a proposal
icnctl gov vote show <proposal-id>
```

### Closing and Outcomes

```bash
# Close proposal and compute outcome (manual close)
icnctl gov proposal close <proposal-id>

# Show proposal status with tally
icnctl gov proposal status <proposal-id>
```

### Governance Changes

```bash
# Propose governance config change
icnctl gov proposal create \
  --domain-id "coop:tiny-food" \
  --title "Increase quorum to 67%" \
  --payload config-change \
  --quorum 67

# If accepted, new threshold applies to future proposals
```

---

## 8. Future Extensions (Not Yet Implemented)

Phase 13 focuses on a robust but minimal base. Several extensions are explicitly left for later:

### Additional GovernanceParams Fields

* `supermajority_percentage: Option<u8>` for built-in supermajority profiles
* `allow_abstain: bool` to control whether abstain is offered as a choice
* `veto_roles: Vec<String>` for role-based veto power

### Additional Decision Outcomes

* `Inconclusive` for ambiguous cases (all abstentions, tied votes in even-number electorates)

### Weighted Voting

* True `f64` weights derived from:
  - Trust scores
  - Role-based authority
  - Stake or contribution metrics
  - Time-weighted participation

### Additional Governance Profiles

* **Consent/Sociocratic**: Proposal accepted unless there's a reasoned objection
* **Supermajority**: Built-in 2/3, 3/4, 4/5 profiles
* **Council/Role-Based**: Different chambers or roles with different voting powers
* **Liquid Democracy**: Delegation and proxy voting

### Contract-Based Profiles

* `profile = "contract:<did>"` delegates evaluation to a CCL contract
* Enables arbitrarily complex governance logic without core changes

### Automatic Side Effects

* Direct integration with ledger for budget execution
* Membership changes applied automatically when proposals pass
* Contract invocation for ConfigChange payloads

### Time-Based Features

* Automatic proposal opening/closing based on timestamps
* Proposal scheduling and queuing
* Grace periods and execution delays

The current implementation is intentionally conservative, but the architecture supports all of these extensions without breaking existing governance domains.

---

## 9. Test Coverage

Phase 13 includes comprehensive unit tests:

* **39 total tests** covering all core types and logic
* **Governance profile edge cases**:
  - Quorum boundary testing (49%, 50%, 51%)
  - Approval threshold edge cases
  - All-abstain scenarios
  - Zero participation handling
* **Proposal lifecycle validation**:
  - State transition enforcement
  - Invalid transition rejection
  - Cancellation from various states
* **Vote tallying**:
  - Weighted vote aggregation
  - Percentage calculations
  - From-iterator construction
* **Store operations**:
  - CRUD for domains, proposals, votes
  - Vote replacement (changing your vote)
  - Tally computation
* **Membership resolution**:
  - Static list lookups
  - Trust threshold validation
  - Composite resolution strategies
* **Message serialization**:
  - Round-trip JSON encoding
  - All message variants
  - TallySnapshot calculations

Integration tests (planned):
- Multi-node governance via gossip
- Full proposal lifecycle across network
- Outcome convergence verification
- Byzantine voter behavior resistance

---

## 10. Architecture Philosophy

The governance primitives follow ICN's core design principles:

**Separation of Concerns:**
- Membership ≠ Tallying ≠ Evaluation ≠ Execution
- Each layer is independently testable and replaceable

**Extensibility via Composition:**
- New profiles implement `GovernanceRule` trait
- New membership sources extend `MembershipSource` enum
- New payload types extend `ProposalPayload` enum

**Defaults, Not Dogma:**
- `cooperative_default` is boring and democratic
- Communities can switch profiles via proposals
- No hardcoded political assumptions

**Gossip-Based Coordination:**
- Same pattern as social recovery (Phase 11)
- Eventually consistent decision-making
- Independent outcome verification via TallySnapshot

**Data Sovereignty:**
- Each governance domain is independent
- No global registry or permissions
- Communities control their own rules

**Future-Proof:**
- Weight field supports future weighted voting
- Profile system supports arbitrary complexity
- Contract integration planned but not required

This governance layer is not the "ICN way to make decisions" - it's a **substrate for communities to build their own democratic processes**.

---

## 11. References

**Code:**
- `icn/crates/icn-governance/` - All governance primitives
- `icn-governance/src/profile.rs` - GovernanceRule trait and cooperative_default
- `icn-governance/src/config.rs` - GovernanceParams and defaults
- `icn-governance/src/message.rs` - Gossip protocol messages

**Related Documentation:**
- [ARCHITECTURE.md](ARCHITECTURE.md) - Overall ICN architecture
- [social-recovery.md](social-recovery.md) - Similar gossip-based coordination pattern

**External Context:**
- [ROADMAP.md](../ROADMAP.md) - Phase 13 objectives and next steps
- [econ-modeling.md](econ-modeling.md) - Economic safety mechanisms (Phase 12)
