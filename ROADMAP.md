# ICN Roadmap

**Status**: Phase 14 Gateway ✅, Phase 12 ✅, Track B1 ✅, Track B3 ✅ - All 268 Tests Passing
**Next**: Track C1 (Pilot Selection) → MVC Track (13 weeks focused work)

## Executive Summary

**Substrate Status**: Production-ready. Security hardened, multi-device identity, economic safety rails, operational tooling, economic validation, and Gateway API complete.

**Gap Analysis**: ICN is a *substrate daemon*, not a deployable product for cooperatives. Two complementary gap assessments:
- [Strategic Gap Analysis](docs/strategic-gap-analysis.md): 15 structural gaps (substrate→system transition)
- [Implementation Gap Analysis](docs/gap-analysis.md): Documentation vs reality audit with status markers (✅🚧📋❌)
- [Minimal Viable Coop Track](docs/MINIMAL-VIABLE-COOP.md): Focused 13-week roadmap to pilot deployment

**Critical Path**: MVC Track (Minimal Viable Coop) - ruthlessly focused on one end-to-end use case rather than polishing individual components. Select pilot community, close only gaps blocking that pilot, deploy and learn.

---

## Roadmap Structure

ICN's development follows three parallel tracks:

- **Track A: Substrate Evolution** - Core protocol and security features (sequential)
- **Track B: Operational & Legal Backbone** - Production readiness (parallel)
- **Track C: Pilot Community** - Real-world deployment and learning (convergent)

**Guiding Principle**: Track C (pilot deployment) drives priorities in Tracks A and B. We build what real communities need, not what the architecture diagram suggests.

## Gap Analysis Summary

**15 Structural Gaps** identified across 4 tiers (see [docs/strategic-gap-analysis.md](docs/strategic-gap-analysis.md)):

**Status Breakdown**:
- ✅ **Closed** (3): Multi-device identity, protective ledger (partial), economic simulation
- 🚧 **Partial** (4): Ledger mechanics, security posture, storage sync, observability
- ⏸️ **Deferred** (2): NAT traversal, federation (intentional - wait for pilot need)
- 🔴 **Critical Path** (6): Client SDK, governance, templates, onboarding, observability UX, cooperation UX

**Key Insight**: The substrate is ready. The missing pieces are *social layer*, *usability*, and *real-world workflows* - all discovered through pilot deployment.

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

### Phase 12: Economic Safety Rails ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Blocker For**: Pilot deployment in mutual credit scenarios - NOW UNBLOCKED

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

### Phase 14: Gateway API ✅ COMPLETE (Core), SDK & Reference App (Pending)
**Status**: Gateway Complete (2025-01-15), SDK/App Deferred until Pilot Selected
**Blocker For**: Client applications - Partially Unblocked

**Strategic Shift**: ICN as Cooperative Backend Platform

**Vision**: Co-ops build apps that use ICN under the hood. Members never see `icnd` or `icnctl`.

**What This Is**:
- Developer-facing API layer (REST + WebSocket)
- TypeScript SDK for easy integration
- Reference app (Shopper's Club) as starting point
- **NOT** an app runtime (that's Phase 16+ conditional)

**What This Enables**:
- Co-ops can build custom apps (or fork reference app)
- Early pilots: we host the reference app multi-tenant (Phase 15)
- Later: co-ops can self-host or customize

**Completed Components** ✅:
1. **icn-gateway** - REST + WebSocket API server (Actix-web)
   - 13 REST endpoints (health, auth, coops, ledger, governance, proposals)
   - JWT authentication with challenge-response flow
   - Bearer token middleware protecting sensitive endpoints
   - WebSocket real-time event streaming with post-connection auth
   - Cooperative namespace isolation
2. **Runtime Integration** - Gateway integrated into icnd
   - GatewayConfig in configuration system
   - CLI arguments: `--gateway-enable`, `--gateway-bind`, `--gateway-jwt-secret`
   - Environment variable support: `ICN_GATEWAY_JWT_SECRET`
   - Dedicated thread spawn for Actix-web compatibility

**Still Needed** (Deferred until pilot selection):
3. **TypeScript SDK** - `@icn/client` npm package
4. **Reference App** - Timebank or other pilot-specific application
5. **API Improvements**:
   - API versioning (`/v1/` namespacing)
   - Per-DID rate limiting
   - WebSocket reconnection and event backfill
   - Scope-based authorization enforcement in handlers

**Success Criteria**: ✅ Gateway Complete
- ✅ Gateway API operational and integrated into icnd
- ✅ JWT authentication working (challenge → token → protected endpoints)
- ✅ WebSocket events streaming to connected clients
- ✅ Cooperative namespace isolation functional
- 🔲 TypeScript SDK (deferred)
- 🔲 Reference application (deferred)

**Deliverables**: ✅ Gateway Complete
- ✅ `icn-gateway` crate with 30 passing tests
- ✅ Integration into icnd supervisor
- ✅ Configuration system and CLI support
- ✅ Documentation: dev journals, CHANGELOG.md, example configs
- 🔲 `@icn/client` SDK (deferred)
- 🔲 Reference app (deferred)

**Next Steps**:
- Select pilot community (Track C1)
- Build SDK + reference app for pilot's specific workflows
- Don't build generic SDK speculatively - build what pilots need

---

### Phase 15: Hosted Pilot Deployment (1-2 months)
**Status**: Not Started - Blocked on Phase 14
**Purpose**: Get real signal from actual co-ops

**Approach**:
- Deploy reference app multi-tenant (one instance, namespace-scoped)
- Run on ICN Foundation infrastructure (or similar)
- Onboard 1-2 pilot co-ops: "Here's your app, here's your login"
- **Watch what hurts** - weekly feedback sessions

**Learning Questions**:
- Where do users get stuck? (onboarding, UX, concepts)
- What do they want to customize?
- Do they care about self-hosting?
- Is the ledger model intuitive?
- What governance patterns emerge?

**Success Criteria**:
- 10+ active users logging transactions weekly
- 3+ governance decisions (or attempts at governance)
- Community feedback informs Phase 16+ priorities
- Clear signal on what to build next

**Deliverables**:
- Hosted multi-tenant deployment
- Pilot onboarding guide
- Weekly learnings documented in `docs/pilots/learnings/`

**Design Doc**: [docs/pilots/hosted-approach.md](docs/pilots/hosted-approach.md)

---

### Phase 16: TBD - Driven by Pilot Learnings
**Status**: Not Planned Yet
**Approach**: Let Phase 15 pilots reveal what's actually needed

**Possible Directions** (conditional on pilot feedback):
- **App Runtime** (if co-ops need custom backend logic but can't run servers)
- **Governance Templates** (if governance patterns are clear and repeatable)
- **Better Self-Hosting Tools** (if co-ops want to self-host but struggle with devops)
- **Federation** (if multiple co-ops want to interconnect)
- **Mobile Apps** (if web-on-phone isn't sufficient)

**Decision Gate**:
Build these ONLY if pilots reveal patterns like:
- "We need custom event-driven logic but can't run servers"
- "We want to deploy small scripts that react to ledger changes"
- "Self-hosting is the blocker, not the platform"

**Philosophy**: Don't speculate. Build what pilots prove is necessary.

---

### Phase 13: Governance Primitives v1 (6-8 weeks)
**Status**: Foundation Started, Full Implementation Deferred
**Driven By**: Platform layer enables governance features in apps

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

### Intentional Deferments (Pending Pilot Feedback)

These features are **NOT on the roadmap** until pilot communities demonstrate need. Based on gap assessment (2025-01-14):

**Federation/Interoperability** (Deferred):
- **Status**: ICN is pure P2P with mDNS local discovery
- **Gap**: No ActivityPub, OIDC, SAML, or other federation protocols
- **Interim**: Manual peer connection works over internet (`icnctl network add-peer`)
- **Decision**: Wait for 2+ successful pilots wanting to interconnect before building cross-network discovery
- **Rationale**: Single 50-member pilot doesn't need federation; premature complexity

**Integrated Messaging** (Deferred):
- **Status**: Gossip provides pub/sub bulletin board, not real-time chat
- **Gap**: No Signal Protocol, OMEMO, or private messaging
- **Interim**: Use external tools (Signal, email) for chat; gossip for announcements
- **Decision**: Pilot first, add messaging in Phase 14+ if bulletin board insufficient
- **Rationale**: Tight scope enables pilot success; messaging is scope creep

**Advanced Privacy** (Deferred):
- **Status**: QUIC/TLS transport + X25519 end-to-end encryption for payloads
- **Gap**: No zero-knowledge proofs, selective disclosure, anonymous credentials
- **Decision**: Trust-first communities don't need advanced privacy tech
- **Rationale**: Cooperatives share resources among known members; ZK is solution looking for problem

**Cross-Network Standards** (Deferred):
- **Status**: QUIC/TLS works over internet, only discovery is LAN-only (mDNS)
- **Gap**: No standardized discovery protocol for ICN-to-ICN across regions
- **Interim**: Manual peer connection (`icnctl network add-peer <addr> <did>`)
- **Decision**: Add lightweight discovery (DNS TXT records?) in Phase 14+ if pilots demand it
- **Rationale**: Manual peering validates need before building full discovery infrastructure

**Explicitly Out of Scope**:

**Formal Verification** (Never):
- **Status**: CCL has fuel metering, type checking, comprehensive tests (268 passing)
- **Gap**: No formal proofs of contract correctness
- **Decision**: Too expensive for 1-2 developer team; tests + code review sufficient for cooperative-scale (10-1000 members)
- **Rationale**: Formal verification targets financial infrastructure at nation-scale; ICN serves community-scale mutual credit

**Philosophy**: Build what communities need, not what the architecture diagram suggests. Pilot feedback drives roadmap.

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

### B1: Operational Hardening ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Blocker For**: Production deployment - NOW UNBLOCKED

**Backup & Restore**: ✅
- [x] Document all ICN data locations (`~/.icn/*`)
- [x] Implement `icnctl backup <path>` (encrypted Age bundle with SHA256 checksum)
- [x] Implement `icnctl restore <path>` (with validation and force-restore)
- [x] Best practices doc: daily snapshots, off-site storage, encryption
- [x] State snapshot integration (backup includes `state.snapshot`)

**Monitoring Dashboard**: ✅
- [x] Real-time web UI at `:8080/` with Prometheus metrics
- [x] Key metrics: connections, gossip topics, subscriptions, message rates, snapshot operations
- [x] Health check endpoint (`/health`) for external monitoring (JSON format)
- [x] 11 snapshot-specific metrics for operational visibility

**Upgrade Mechanism**: ✅
- [x] Versioned network protocol with automatic validation
- [x] **Graceful restart semantics** (preserve vector clocks, subscriptions, X25519 keys)
- [x] State snapshot persistence (gossip + network state)
- [x] Signal handling (SIGTERM, SIGINT) for clean shutdown
- [x] Sub-millisecond snapshot save/load performance

**Incident Response Playbook**: ✅
- [x] Document: "Node is compromised - what do?" (7 procedures)
- [x] Document: "Ledger corruption detected - how to recover?"
- [x] Document: "Key suspected stolen - rotation ceremony"
- [x] Document: Network partition, gossip divergence, disk full, protocol mismatch
- [x] Comprehensive troubleshooting guides

**Deliverables**: ✅
- [x] `docs/operations-guide.md` (comprehensive, 800+ lines)
- [x] `docs/incident-response.md` (7 major incident procedures)
- [x] Backup/restore commands in `icnctl` (with test coverage)
- [x] Real-time monitoring dashboard (static HTML + Prometheus integration)
- [x] Graceful restart implementation (snapshot-based state persistence)

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

### B3: Economic Modeling ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Purpose**: Validate economic assumptions before they blow up in production

**Implementation**: Agent-based simulation using Mesa 3.3.1
- **Agents**: 100 per scenario with 5 behavioral types
- **Duration**: 12 months (360 days) per simulation
- **Scenarios**: 5 configurations testing different policy parameters
- **Results**: ~13,000 transactions per scenario, comprehensive metrics

**Key Findings**:
1. ✅ **Dynamic credit limits work**: -33% defaults, -16% velocity (stability vs growth tradeoff)
2. ✅ **Demurrage highly effective**: -22% inequality (Gini) without harming velocity
3. ✅ **System tolerates free-riders**: Up to 20% before serious stress (4.1% defaults)
4. ⚠️ **Sparse trust networks increase hoarding**: 2x hoarding at 30% density vs 60% (counterintuitive)

**Validated Defaults** (now implemented in Phase 12):
- Credit limits: -20 initial → -500 max, +10 per 50 cleared, 2x trust multiplier
- Demurrage: -2% monthly on balances >50
- New member protection: 3-month ramp, 10 credit contribution requirement

**Deliverables**: ✅
- [x] `sims/mutual-credit/` - Complete simulation framework (agents, economy, trust, model)
- [x] 5 JSON scenario configurations (baseline, dynamic limits, demurrage, free riders, low trust)
- [x] `sims/mutual-credit/RESULTS_SUMMARY.md` - Comprehensive analysis
- [x] `docs/econ-modeling.md` - Updated with simulation results
- [x] Analysis notebooks for visualization

**Next**: Calibrate against pilot data (Track C3) to validate real-world applicability

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

**Completed Prerequisites for Pilot Deployment:**
1. ✅ Phase 10: Security hardening, encryption (COMPLETE)
2. ✅ Phase 11: Multi-Device Identity (COMPLETE - 2025-01-14)
3. ✅ Phase 12: Economic Safety Rails (COMPLETE - 2025-01-14)
4. ✅ Track B1: Operational Hardening (COMPLETE - 2025-01-14)
5. ✅ Track B3: Economic Modeling (COMPLETE - 2025-01-14)
6. ✅ Phase 14: Gateway API Core (COMPLETE - 2025-01-15)

**What Must Happen Before Pilot Deployment:**
1. ⏳ C1: Select pilot community (2-4 weeks, can start NOW) - **CRITICAL PATH**
2. ⏳ C2: Build MVP for that community's workflows (4-6 weeks)
   - TypeScript SDK for pilot needs
   - Simple web UI for pilot workflows
   - Pilot-specific integrations (email, notifications)

**Total time to pilot: ~6-10 weeks** (1.5-2.5 months) from pilot selection

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

---

## Strategic Assessment (2025-01-15)

### What We've Built

**Substrate Infrastructure** (Phases 1-12):
- Three-layer security architecture (transport, message, application)
- Multi-device identity with key rotation and gossip sync
- Dynamic credit limits with new member protection
- Dispute resolution and write-off mechanisms
- Backup/restore, graceful restart, monitoring
- Economic validation via agent-based simulation

**All 268 tests passing** ✅

### What We're Missing

**Social Layer**:
- Invitation flows, role management, consent mechanisms
- Onboarding workflows for non-technical users
- Social protocols (how humans coordinate, not just protocols)

**Usability**:
- Web/mobile clients (currently CLI only)
- Intuitive UI for cooperative workflows
- Visualization tools (trust graph, ledger browser, topology)

**Real-World Integration**:
- Guided cooperative setup (group creation → governance → ledger)
- Email/SMS notifications
- Export formats for accountants/treasurers

See [Strategic Gap Analysis](docs/strategic-gap-analysis.md) for complete 15-gap assessment.

### The Path Forward

**Build with communities, not for them.**

ICN is ready for pilot deployment. The next phase isn't more infrastructure - it's learning from a real cooperative what actually matters.

**Success Criteria (3-month pilot)**:
- 10+ active users logging hours/transactions weekly
- 3+ governance decisions made using ICN primitives
- Community prefers ICN over their previous system
- 2-3 other communities express interest

**Next Actions**:
1. Select pilot community (timebank recommended)
2. Build minimal MVP for their workflows
3. Run weekly learning loop
4. Let their needs drive Phase 13+ scope

**Philosophy**: We're building infrastructure for civilizational transition. The substrate is ready. Now we listen to communities and build what they need.

---

**Last Updated**: 2025-01-15 (Phase 14 Gateway Complete, Gap Analyses Complete, MVC Track Defined)
**Next Review**: After Track C1 (community selection) completes
