# ICN Roadmap

**Status**: Phase 11 Complete (Multi-Device Identity & Sync) - All 265 Tests Passing
**Next**: Track B1 (Operational Hardening) or Phase 12 (Economic Safety Rails)

## Roadmap Structure

ICN's development follows three parallel tracks:

- **Track A: Substrate Evolution** - Core protocol and security features (sequential)
- **Track B: Operational & Legal Backbone** - Production readiness (parallel)
- **Track C: Pilot Community** - Real-world deployment and learning (convergent)

**Guiding Principle**: Track C (pilot deployment) drives priorities in Tracks A and B. We build what real communities need, not what the architecture diagram suggests.

---

## Track A: Substrate Evolution

### Phase 11: Multi-Device Identity & Sync ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Blocker For**: All pilot deployments - NOW UNBLOCKED

**Motivation**: Current identity model (1 keypair = 1 person) fails when devices break, get stolen, or users need multiple access points. A real cooperative substrate must survive hardware failure without losing economic history.

**Scope**:
- DID Document v2 with multiple verification methods
- Device management (add/revoke devices)
- Key rotation with event chains
- Recovery mechanisms (social recovery or backup seeds)

**Technical Design**:
```
DID Document v2:
{
  "id": "did:icn:abc123",
  "verificationMethod": [
    { "id": "device-1", "publicKey": "...", "capabilities": ["sign", "rotate"] },
    { "id": "device-2", "publicKey": "...", "capabilities": ["sign"] }
  ],
  "authentication": ["device-1", "device-2"],
  "recovery": { "method": "social", "threshold": 3, "trustees": [...] }
}

Key Rotation Chain (stored in icn-store):
RotationEvent {
  old_key: PublicKey,
  new_key: PublicKey,
  proof: Signature,
  timestamp: u64,
  reason: RotationReason  // Scheduled, Compromise, DeviceChange
}
```

**Success Criteria**:
- User can add a second device without losing identity
- User can revoke a compromised device
- User can recover identity if all devices are lost (via social recovery)
- All ledger/trust/contract operations work across device rotations
- Tests: Multi-device signing, rotation chain validation, recovery flows

**Deliverables**:
- `icn-identity` crate updates for DID Document v2
- `icnctl device add`, `icnctl device revoke`, `icnctl recover` commands
- Migration path from v2.1 keystore to multi-device format
- Documentation: `docs/multi-device-identity.md`

---

### Phase 12: Economic Safety Rails (4 weeks)
**Status**: Not Started
**Blocker For**: Pilot deployment in mutual credit scenarios

**Motivation**: Mutual credit systems fail predictably: free riders, credit limit gaming, defaults without recourse. Without guard rails, the first scammer destroys community trust in the entire system.

**Scope**:
- Dynamic credit limits based on trust + history
- New member protective throttling
- Dispute resolution primitives
- Default handling protocols

**Technical Design**:
```rust
// icn-ledger/src/credit_policy.rs
pub struct CreditPolicy {
    baseline: i64,           // Base credit for all members
    trust_multiplier: f64,   // Scale by trust score
    history_bonus: i64,      // Bonus for cleared obligations
}

impl CreditPolicy {
    pub fn calculate_limit(&self, member: &DID, ledger: &Ledger, trust: &TrustGraph) -> i64 {
        let trust_score = trust.compute_trust(member)?;
        let cleared_volume = ledger.total_cleared_by(member)?;

        self.baseline
            + (self.baseline as f64 * trust_score * self.trust_multiplier) as i64
            + (cleared_volume / 10)  // 10% of historical cleared volume
    }
}

// New member throttle
pub struct NewMemberPolicy {
    initial_limit: i64,      // Very low (e.g., 10 hours)
    ramp_period: Duration,   // Time to reach full limit
    contribution_required: u64, // Min contributions before ramp starts
}

// Dispute resolution
pub enum EntryStatus {
    Normal,
    Contested { filed_by: DID, reason: String, filed_at: u64 },
    Resolved { mediator: DID, outcome: Resolution },
}
```

**Success Criteria**:
- New members cannot immediately max out credit and disappear
- Credit limits adapt to demonstrated trustworthiness
- Community can flag disputed entries
- Default handling is explicit and visible (not silent failure)
- Economic simulation shows reduced vulnerability to common attacks

**Deliverables**:
- `icn-ledger` credit policy system
- CCL primitives: `dispute_entry()`, `resolve_dispute()`, `write_off_debt()`
- Economic simulation (see Track B3)
- Documentation: `docs/economic-safety.md`

---

### Phase 13: Governance Primitives v1 (6-8 weeks)
**Status**: Not Started
**Driven By**: Pilot community needs (Phase C2)

**Motivation**: Cooperatives need to make collective decisions: membership, resource allocation, conflict resolution, rule changes. ICN currently has contracts but no governance patterns. We don't need "the" governance system - we need pluggable primitives that communities can compose.

**Scope**:
- CCL primitives for proposals, quorum, thresholds
- 3-4 governance template contracts
- Role/membership management
- State machine hooks for governance flows

**CCL Extensions Needed**:
```
// Governance primitives (built-in capabilities)
proposal_create(subject: String, payload_ref: Hash) -> ProposalID
proposal_vote(id: ProposalID, vote: Vote) -> Result
proposal_state(id: ProposalID) -> ProposalState
quorum_met(members: Vec<DID>) -> bool
threshold_met(yes: u64, no: u64, abstain: u64, threshold: f64) -> bool
has_role(member: DID, role: String) -> bool
member_count() -> u64

// State machine hooks
on_proposal_open(callback)
on_proposal_consent(callback)
on_proposal_block(callback)
on_proposal_timeout(callback)
on_proposal_execute(callback)
```

**Governance Templates** (shipped as `.ccl` files):
1. **Consensus with Fallback Majority**
   - Try for full consensus (7-day period)
   - Fall back to 2/3 majority if no consensus
2. **Sociocracy-style Consent**
   - Passes unless explicit objection with reason
   - Objection triggers mandatory deliberation
3. **Council Delegation**
   - Elected council makes day-to-day decisions
   - Membership can recall with supermajority
4. **Emergency Lock**
   - Immediate action by designated responders
   - Requires ratification within 48 hours

**Success Criteria**:
- Pilot community can encode their existing governance model in CCL
- Proposals have clear lifecycle (open → deliberation → decision → execution)
- System supports at least 3 distinct governance patterns
- Documentation shows how to create custom governance contracts

**Deliverables**:
- CCL governance primitives (`icn-ccl/src/governance.rs`)
- 4 governance template contracts (`templates/governance/*.ccl`)
- `icnctl governance` subcommands
- Documentation: `docs/governance-primitives.md`

**IMPORTANT**: Do not build this until Phase C2 (pilot community engagement) reveals what they actually need. This scope is a *hypothesis* to be validated by real use.

---

### Future Phases (Driven by Pilot Learnings)

**Phase 14+: Cooperation Layer**
- Proposals, decisions, signaling, scheduling
- Group identities and working groups
- Role-based permissions
- *Scope TBD based on pilot community workflows*

**Phase 15+: Reputation Layer**
- Structured contribution records
- Signed evidence (not scores)
- Time-based decay
- *Driven by what communities actually track*

**Phase 16+: Federation Protocols**
- Cross-cooperative boundaries
- Inter-coop credit settlement
- Governance bridging
- *Scope TBD based on multi-community adoption*

---

## Track B: Operational & Legal Backbone

### B1: Operational Hardening (2 weeks, can start immediately)
**Status**: Not Started
**Blocker For**: Any production deployment

**Backup & Restore**:
- [ ] Document all ICN data locations (`~/.icn/*`)
- [ ] Implement `icnctl backup <path>` (encrypted bundle of keystore + store + config)
- [ ] Implement `icnctl restore <path>` (with validation)
- [ ] Best practices doc: daily snapshots, off-site storage, encryption

**Monitoring Dashboard**:
- [ ] Simple web UI hitting Prometheus at `:9090/metrics`
- [ ] Key metrics: peer count, gossip health, ledger error rates, disk usage
- [ ] Health check endpoint for external monitoring

**Upgrade Mechanism**:
- [ ] Versioned network protocol (currently implicit)
- [ ] Graceful restart semantics (preserve state across daemon restarts)
- [ ] `icnctl migrate` for schema changes
- [ ] Rolling upgrade strategy for multi-node communities

**Incident Response Playbook**:
- [ ] Document: "Node is compromised - what do?"
- [ ] Document: "Ledger corruption detected - how to recover?"
- [ ] Document: "Key suspected stolen - rotation ceremony"
- [ ] Even if v1 responses are crude, having the playbook matters

**Deliverables**:
- `docs/operations-guide.md`
- `docs/incident-response.md`
- Backup/restore commands in `icnctl`
- Simple monitoring dashboard (could be static HTML + JS fetching Prometheus)

---

### B2: Legal & Regulatory Radar (ongoing, lightweight)
**Status**: Not Started
**Priority**: Medium (document early, don't block on it)

**Goal**: Not "solve all legal problems" but "know what questions communities will face."

**Deliverables**:
- [ ] `docs/legal-considerations.md`:
  - Are mutual credits "money" under US/EU/UK law? (spoiler: probably not, but varies)
  - What records do cooperatives need for tax reporting?
  - How to export ledger history for accountants (CSV format)
  - Data protection stance (GDPR, CCPA)
  - Liability model if ICN loses economic history
- [ ] Privacy/data minimization guidelines:
  - ICN nodes are self-hosted by communities
  - How to handle delete/portability requests
  - Don't log more PII than necessary
- [ ] Treat this as a living document, updated as real communities raise concerns

**Non-Goals**:
- We are NOT building a compliance framework
- We are NOT seeking legal opinions yet
- We ARE documenting known questions so communities can consult their own lawyers

---

### B3: Economic Modeling (parallel research track)
**Status**: Not Started
**Purpose**: Validate economic assumptions before they blow up in production

**Known Failure Modes of Mutual Credit**:
1. **Tragedy of the credits**: Users hoard positive balances → deflation
2. **Free-rider problem**: Extract value, never contribute
3. **Credit limit gaming**: Max out borrowing, ghost the network
4. **Velocity collapse**: Low trust → no one spends → credits stagnate

**Approach**:
- Build agent-based simulation (Python or Rust)
- Model agents with different behaviors: hoarders, reciprocators, free riders, etc.
- Experiment with:
  - Demurrage (negative interest on positive balances)
  - Credit limits (fixed vs. dynamic)
  - Default handling (write-offs, reputation impact)
  - Trust-based risk adjustment
- Output: Recommended default parameters for Phase 12 credit policies

**Deliverables**:
- `sims/mutual-credit/` directory with simulation code
- `docs/econ-modeling.md` with:
  - Failure mode catalog
  - Simulation results
  - Recommended defaults (starting credit, max exposure, demurrage rate)
- Validation against pilot community data (Phase C3)

**Timeline**: Start during Phase 11, inform Phase 12 design

---

## Track C: Pilot Community & Bootstrap

### C1: Community Selection (2-4 weeks, can start immediately)
**Status**: Not Started
**Critical Path**: This drives everything else

**Selection Criteria**:
1. **Existing trust web** (ICN is not solving "everyone hates each other")
2. **Real, recurring coordination problems** (not hypothetical use case)
3. **Openness to experiment** (willing to tolerate rough edges)
4. **Some digital fluency** (can handle CLI tools initially, want better UX)

**Candidate Archetypes** (ranked by simplicity):
1. **Timebank** (RECOMMENDED FIRST)
   - Already mutual-credit-shaped (hours = currency)
   - Simple economic model (1 hour = 1 hour, no pricing complexity)
   - Clear value: "replace our spreadsheet with something that doesn't break"
   - Lower stakes than housing/money
2. **Housing Cooperative**
   - Rich governance needs (maintenance, conflict resolution, membership)
   - Real stakes (people's homes)
   - More complex, but if you have a warm relationship, could work
3. **Community Land Trust**
   - Very high stakes, slower decision cycles
   - Better as second-wave pilot after timebank proves the model

**Action Items**:
- [ ] List 2-3 real organizations you have connections to
- [ ] Draft one-page pilot proposal:
  - "Here's what ICN does"
  - "Here's what we'd pilot (replace X painful workflow)"
  - "Here's what we need from you (weekly feedback, 3-5 active users)"
  - "Here's the timeline (3-month initial pilot)"
- [ ] Start one real conversation this week

---

### C2: Minimum Viable Product for Pilot (scoped after C1 completes)
**Status**: Blocked on C1 (community selection)
**Philosophy**: We are not shipping "ICN the substrate." We are solving 3-5 specific painful workflows for one community.

**Example: Timebank Pilot MVP**

**Jobs to Be Done** (validate with actual community):
1. Log hours worked/received
2. Browse offers and requests
3. See my balance and history
4. Resolve disputes about logged hours
5. View community health (total hours exchanged, active members)

**Technical Scope**:
- **Simple web UI** (not mobile app, not fancy)
  - Login with DID (QR code or key file upload)
  - Dashboard: your balance, recent transactions
  - Log hours form: "I gave 2 hours to Alice for gardening help"
  - Browse: list of open offers/requests (stored as CCL contracts)
  - Dispute: flag an entry as contested
- **Backend**: `icn-rpc` gRPC API (already exists, may need extensions)
- **Interoperability v0**:
  - Email notifications: "You received 2 hours from Alice" (via simple SMTP)
  - Public web page: read-only stats (total hours, active members) as HTML
  - CSV export: for treasurer to hand to accountant

**Non-Goals for MVP**:
- ❌ Mobile app (use web on phone)
- ❌ Real-time collaboration (async is fine)
- ❌ Complex governance (Phase 13)
- ❌ Federation (one community only)

**Deliverables**:
- `icn-web/` - Simple web UI (could be static HTML + JS, or basic Rust/Actix server)
- `docs/pilot-deployment.md` - How to run the pilot stack
- Instrumentation for learning (see C3)

---

### C3: Learning Loop (ongoing during pilot)
**Status**: Not Started
**Purpose**: "This single deployment will teach you more than 6 months of architecture work."

**Weekly Debrief Structure**:
- Meet with 2-3 core pilot community members
- Questions:
  - What worked this week?
  - What broke or confused you?
  - What did you try to do but couldn't?
  - What would you change?
- Document in `docs/pilot-learnings/YYYY-MM-DD.md`

**Instrumentation** (add to pilot MVP):
- Failed transactions: what errors do users hit?
- Abandoned flows: where do people give up?
- Support requests: what questions come up repeatedly?
- Feature requests: what do they ask for that doesn't exist?

**Decision Protocol**:
- **Do NOT over-fit the substrate to one community's quirks**
- Look for patterns across 3+ similar requests
- Validate: "Is this a general cooperative need or specific to this group?"
- Prioritize: Does this unblock adoption or just polish the happy path?

**Success Criteria** (3-month pilot):
- 10+ active users logging hours weekly
- At least 3 governance decisions made using ICN primitives
- Community says: "We'd rather fix this than go back to spreadsheets"
- 2-3 other communities express interest based on pilot results

**Deliverables**:
- `docs/pilot-learnings/` directory with weekly notes
- Quarterly retrospective: what changed in the roadmap based on reality?
- Public case study (with community permission)

---

## Critical Path Summary

**What Must Happen Before Pilot Deployment:**
1. ✅ Phase 10 complete (security hardened, encryption working)
2. ✅ Phase 11: Multi-Device Identity (COMPLETE - 2025-01-14)
3. ⏳ B1: Operational Hardening (backup/restore, monitoring) (2 weeks) - **NEXT**
4. ⏳ Phase 12: Economic Safety Rails (4 weeks)
5. ⏳ C1: Select pilot community (2-4 weeks, can start NOW)
6. ⏳ C2: Build MVP for that community's workflows (4-6 weeks)

**Total time to pilot: ~8-12 weeks** (2-3 months) - reduced with Phase 11 complete

**Parallelization**:
- C1 (community selection) can start immediately
- B3 (economic modeling) can run during Phase 11
- B2 (legal docs) can be written anytime

**What Happens After Pilot:**
- Phase 13 scope is driven by pilot learnings
- Track A future phases prioritized by what pilot communities actually need
- Federation (Phase 16+) only makes sense when 2+ communities are running ICN

---

## Open Questions

**Technical:**
- Multi-device identity: social recovery vs. backup seeds? (Both? User choice?)
- Economic modeling: what demurrage rate prevents hoarding without punishing savers?
- Governance: should templates be CCL contracts or Rust-level primitives?

**Strategic:**
- Should ICN target existing cooperatives or help form new ones?
- How much interoperability with legacy systems (email, banking) is necessary?
- What's the business model for ongoing ICN development? (Grant-funded? Cooperative membership dues? Service contracts?)

**Operational:**
- Who runs the pilot infrastructure? (Us? Community? Shared?)
- What's the handoff plan when pilot becomes production?
- How do we avoid becoming a single point of failure for the community?

---

## How to Use This Roadmap

**For contributors:**
- Pick a phase, read the scope, build it
- Update status as work progresses
- Add learnings to dev journal (`docs/dev-journal/`)

**For potential pilot communities:**
- Read Track C to understand what we're looking for
- Reach out if your community fits the criteria
- Expect a collaborative design process, not a finished product

**For the broader cooperative movement:**
- This is a living document, not a fixed plan
- Priorities will shift based on pilot learnings
- We're building infrastructure for a civilizational transition, not a product roadmap

**Last Updated**: 2025-01-14 (Phase 11 Complete)
**Next Review**: After Track B1 completion or pilot community selection (whichever comes first)
