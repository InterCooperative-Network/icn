# ICN Roadmap & Strategy: From Solo Developer to Cooperative Infrastructure

**March 17, 2026**

This document maps the path from where ICN is today to its first external deployment. It ties technical milestones to adoption milestones to funding milestones. Every section addresses the question: what must be true before the next gate can open?

---

## Where We Are

**Codebase:** 38 Rust crates, ~414K LOC, 2,287 tests, 70+ real API endpoints. Three binaries (icnd, icnctl, icn-console). TypeScript and React Native SDKs. K3s cluster running four federated cooperative pods. Four demo flows with presenter mode. Phase 0+1 complete. Roughly 75% to first external deployment.

**Funding:** Zero external funding. Solo developer on SSDI with a $1,210/month income ceiling. ACCES-VR ($11K) stalled since November 2025. Sovereign Tech Fund application in preparation. NLnet NGI Zero deadline passed (April 1).

**Organization:** No legal entity. No fiscal sponsor yet. Hack Club Bank identified as the target Model A fiscal sponsor. BCL Corporation formation blocked until fiscal sponsor is in place. Article 5-A worker cooperative conversion requires 5 members (currently 1).

**Network:** The NY cooperative movement knows ICN exists. Joe Marraffino (CFNE) is the primary ally. Frank Cetera (Institute.coop) is the summit co-organizer. The Rochester Cooperative Collective meets biweekly. The 3rd NY Coop Summit is planned for October 2026.

---

## The Three Gates

ICN's path from solo project to funded cooperative follows three sequential gates from the Foundation Launch Kit. No skipping.

### Gate A: Foundation Viability (Survival & Ops)

**Passage criteria:**
1. Model A fiscal sponsor secured (Hack Club Bank), W-2 payroll active at $1,200/month
2. Maintainer pay capped below SSA thresholds ($1,210 TWP, $1,690 SGA)
3. Tech stack locked to non-transferable data objects only (no FinCEN/IRS triggers)

**Fatal failures:** 1099 contractor payment. Fiscal sponsor refuses W-2. Token architecture allows peer-to-peer asset transfer.

**Current status:** Pre-Gate A. Fiscal sponsor not yet onboarded. ACCES-VR medical release unsigned since November. Business plan drafted but not submitted.

**Blockers:**
- Sign and return ACCES-VR medical release to Brenda Harner (immediate)
- Complete Hack Club Bank onboarding
- Set W-2 payroll to $1,200/month
- Strip remaining "payment/currency/balance" language from codebase (Compliance Epic #1302)

### Gate B: Pilot-Ready Adoption (Product & Partner)

**Passage criteria:**
1. Local anchor (cooperative or municipality) commits to 8-10 month pilot
2. Metrics defined (governance auditability, reduced coordination overhead)
3. Integration architecture scoped

**Fatal failures:** Anchor demands "crypto" functionality. Scope creep introduces financial clearing before legal review.

**Requires:** Working vertical slice demo. Narrative documentation for non-technical audiences. Summit workshop demonstrating ICN to cooperative organizers.

### Gate C: Institutional Scale (Independence)

**Passage criteria:**
1. Core team reaches 5+ members
2. BCL Corporation formed
3. BCL §82 election filed (Article 5-A Worker Cooperative)
4. Multi-year grant secured

**Fatal failures:** Cannot recruit 5 members (blocks NY cooperative incorporation). Funding relies on venture capital.

---

## Technical Roadmap

### Phase 2: Demo-Ready (March 17 - April 17, 2026)

**Objective:** A recorded walkthrough demonstrating ICN's federation capabilities to cooperative organizers, funders, and technologists. Doubles as a grant application artifact and summit workshop proposal.

**Week 1 (Mar 17-23): Assess & Scope**
- Walk through merged federation demo end-to-end
- Catalog what works, what's fragile, what's missing for a convincing story
- Define demo scenario: two cooperatives federating to coordinate a shared decision
- Write 1-page demo script + blocker list with severity ratings

**Week 2 (Mar 24-30): Stabilize & Build**
- Fix demo-path bugs from blocker list (critical path only)
- Fill narrow gaps (finish 80% features, simplify around 0% features)
- Stand up clean demo environment on K3s

**Week 3 (Mar 31 - Apr 6): Document & Narrate**
- Plain-language overview for cooperative organizers (ICN-Pitch.md is the starting point)
- One-page architecture diagram for technical reviewers
- Draft summit workshop proposal: "Live Demo: Spin Up a Cooperative in 5 Minutes"

**Week 4 (Apr 7-17): Record & Package**
- Screen recording with narration (5-10 minutes)
- README refresh + GitHub landing section
- Phase 2 retro

**Deliverables:** Recorded demo. Workshop proposal ready for May submission. Enough documentation to support Sovereign Tech Fund application.

### Phase 2.5: Compliance Completion (Parallel)

Closes Epic #1302. Required before any grant application.

| Issue | Work | Priority |
|-------|------|----------|
| #1303 | Rename payment→settlement, currency→unit, balance→position | P0 |
| #1304 | UX language guide + Seven Invariants PR checklist | P0 |
| #1305 | JournalEntry.provenance required (ProvenanceRef) | P1 |
| #1306 | Obligation type with lifecycle (Issued→Accepted→Settled→Defaulted) | P1 |
| #1308 | Extract commons credit formula to CCL PolicyOracle | P1 |
| #1311 | AllocationProposal for participatory budgeting | P1 |
| #1312 | CI compliance linter (blocks payment-rail patterns) | P0 |

Already done: #1307 (PatronageTracker), #1309 (DelegationManager), #1310 (ExecutionReceiptGate).

### Phase 3: Vertical Slice (April - May 2026)

The proof that ICN works end-to-end. Success criteria:

```
icnctl coop create --name "Harbor Homes" --charter ./charters/default.toml
  → DID created, treasury DID linked
icnctl member apply --coop did:icn:harbor-homes --identity did:icn:alice
  → application recorded
icnctl vote --proposal governance/001 --choice yes
  → vote counted, Decision Receipt produced
icnctl audit verify --receipt did:icn:receipt/decision/001
  → chain verified: Decision → Allocation → Execution ✓
cargo test --test vertical_slice_integration -- --nocapture
  → PASS in CI
```

**Track execution:**

| Track | Duration | Work |
|-------|----------|------|
| B: Treasury & Economics | ~5 days | Treasury-coop integration, asset type hierarchy, allocation receipt chain |
| A: Kernel/App Separation | ~4 days (parallel with B) | State generalization, governance extraction, membership consolidation, kernel cleanup |
| C: Ops & Pilot Prep | ~3 days | Backup validation, pre-deployment health check, NAT traversal |
| D: Integration | ~2 days | Vertical slice test + demo scripts calling real icnctl |

**After Phase 3: tag v1.0.0.** The vertical slice test passing is the definition of 1.0.

### Phase 4: Meaning Firewall Cleanup (May - June 2026)

The single most important technical task. 11 crates currently contain semantic business logic that belongs in the application layer, not the kernel.

`kernel_surface.toml` tracks the audit. The goal: 29 crates, all clean. Zero semantic leakage across the boundary. CI enforces it permanently.

This is what makes ICN defensibly infrastructure rather than a product. Without a clean Meaning Firewall, the regulatory argument and the architectural purity both collapse.

### Phase 5: Website Overhaul (Parallel)

The current website (intercooperative.network) is stale — stats last synced February 18, one blog post, no community links beyond GitHub. It needs:

- Live stats sync (PRs, contributors, deployment status)
- Narrative content for non-developers (adapted from ICN-Pitch.md)
- Architecture overview for technical reviewers (adapted from whitepaper)
- Community links (Matrix/Discord/mailing list — need to set up)
- Blog with regular posts (summit preparation, development updates, cooperative movement)
- Automated CI deploy from `icn-website` repo

### Phases 6-8: Hardening (June - September 2026)

| Phase | Focus | Key Items |
|-------|-------|-----------|
| 6 | Network & Security | IPv6 dual-stack (#1295), post-quantum crypto integration, penetration testing |
| 7 | SDK & Mobile | React Native SDK stabilization, mobile key management, push notification flow |
| 8 | Federation & Scale | Federation settlement finality spec, multi-federation routing, 50+ node testing |

---

## Funding Strategy

### Immediate Targets

| Opportunity | Amount | Deadline | Fit | Status |
|-------------|--------|----------|-----|--------|
| **Sovereign Tech Fund** | €50K+ | Rolling (target: April) | 8/10 — open digital infrastructure | Preparing application |
| ACCES-VR | Up to $11K | Stalled | Primary — self-employment startup costs | Blocked: medical release unsigned |
| SSA PASS | Variable | Not yet applied | Work incentive — business investment plan | Requires Form SSA-545-BK |
| Outta Excuses Grant | $500-$3K | March 31 | Low barrier, quick cash | Not started |
| Verizon Digital Ready | $10K | March 31 | Requires 1 free course completion | Not started |

### Medium-Term

| Opportunity | Amount | Timeline | Notes |
|-------------|--------|----------|-------|
| Capital Impact Partners | $10K-$50K | Annual (watch 2026) | Cooperative innovation |
| USDA RCDG | Up to $200K | Annual | Rural cooperative development — research eligibility |
| NYS CFA | Variable | July cycle | High admin burden; partner with municipality as lead |
| NLnet NGI Zero | €20-25K | Next cycle | Rated 10/10 fit in Foundation Launch Kit |

### Non-Extractive Loans (No Credit Score Required)

| Lender | Terms | Connection |
|--------|-------|------------|
| CFNE | Never uses credit scores | Joe Marraffino works here |
| The Working World | No profits = no payback | Worker co-op focus |
| Shared Capital Cooperative | Mission-driven, flexible | Worker Ownership Loan Fund |
| KIVA | Up to $15K, 0% interest | Community endorsement model |

### Fiscal Sponsor Pathway

| Step | Entity | Timeline |
|------|--------|----------|
| 1 | Hack Club Bank (Model A fiscal sponsor) | Months 1-10: W-2 payroll at $1,200/mo, 7% fee |
| 2 | BCL Bridge Corporation | Month 10+: when contract-signing needed, before 5 members |
| 3 | Article 5-A Worker Cooperative | When 5+ members: BCL §82 election, patronage-based capital accounts |

---

## Adoption Strategy

### The Entry Point

A single organization runs `icnd` and uses it for proposals and votes. Every decision gets a cryptographic receipt. That's the minimum viable adoption. No federation required. No mutual credit required. Just provable governance.

**Target first adopters:** Upstate NY cooperatives and community organizations reachable through the summit network and Joe Marraffino's connections. Housing co-ops, community land trusts, worker cooperatives. Organizations that already value democratic governance and would benefit from being able to prove it.

### Pilot Recruitment

**3-Touch Outreach:**
1. Email: "Your co-op makes decisions democratically. What if you could prove it — cryptographically — to a grant funder? 5-minute demo?"
2. Demo: Show the governance flow. Proposal → vote → receipt. Frame as "decision infrastructure," not technology.
3. Ask: "Free pilot partner for 8 months. We build what you need; we get a case study."

**Pilot Onboarding:**
- Day 30: Map their governance rules to ICN ConstraintSets
- Day 60: Issue non-transferable member credentials to test group
- Day 90: Execute one real-world decision through ICN
- Day 180: Federation with one other pilot organization
- Day 240: Case study documenting before/after

### Summit as Launchpad

The 3rd NY Cooperative Summit (October 2026, Albany/Schenectady area) is the primary adoption event. Matt is a core organizer for the second year running.

**Timeline:**
- March: Location selection
- April: Organizing team formalized
- May: Call for presenters opens — submit ICN workshop proposal
- June: Sponsor outreach ($24K budget, $27K income target)
- October: Event — live demo, pilot partner recruitment

The workshop proposal: "Live Demo: Spin Up a Cooperative in 5 Minutes." Audience is cooperative organizers, not developers. The pitch is provable governance — something they care about deeply and cannot get from any existing tool.

### Developer Attraction

The longer-term play. Once ICN has pilot deployments and a clean Meaning Firewall, the developer story becomes compelling:

- 38-crate Rust workspace with real architecture (not a toy)
- Eight well-defined kernel primitives that apps compose
- PolicyOracle interface as the extension point
- Regulatory-safe by architecture, not by promises
- Cooperative ownership means contributors can become worker-owners

**Funnel:** GitHub → Contributing guide → Good first issues → Mentorship → Worker-owner track

---

## Timeline Summary

| Period | Technical | Funding | Adoption |
|--------|-----------|---------|----------|
| **Mar 17 - Apr 17** | Phase 2: Demo-ready + compliance completion | Sovereign Tech Fund application prep | Summit planning call Mar 19 |
| **Apr - May** | Phase 3: Vertical slice → v1.0.0 tag | STF application submitted. ACCES-VR unblocked. | Workshop proposal submitted |
| **May - Jun** | Phase 4: Meaning Firewall cleanup | STF decision timeline (~6 months from application) | First pilot partner identified |
| **Jun - Sep** | Phases 5-8: Website, hardening, mobile, federation | Gate A passage (fiscal sponsor + W-2) | Pilot onboarding begins |
| **Oct 2026** | Live demo at summit | STF funding arrives (if approved) | Summit workshop + pilot partner recruitment |
| **Q1 2027** | v2.0 with clean Meaning Firewall + mobile | Gate B passage (pilot commitment) | 3-5 organizations running ICN |
| **2027** | Scale, harden, recruit | Gate C (5+ members → Article 5-A co-op) | Federation of pilot organizations |

---

## Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| ACCES-VR remains stalled | No startup funding, delays Gate A | High | Sign medical release immediately; escalate through Brenda Harner |
| Sovereign Tech Fund rejects application | Primary grant pathway lost | Medium | Diversify: NLnet next cycle, Capital Impact Partners, USDA RCDG |
| No pilot partner found | Gate B blocked | Medium | Leverage summit network; Joe Marraffino has warm intros to 50+ co-ops |
| Solo developer burnout | Everything stops | High | Gate A payroll ($1,200/mo) provides sustainable base; summit community provides motivation |
| Meaning Firewall cleanup reveals deeper architectural issues | Phase 4 extends significantly | Medium | kernel_surface.toml already identifies the 11 infected crates; scope is known |
| hyperion remains offline | Reduced cluster capacity | Low | K3s runs fine on VLAN 30 without hyperion |
| Income exceeds SSDI thresholds | Benefits suspended | Critical | W-2 payroll at $1,200/mo (below $1,210 TWP). No 1099. No LLC. |

---

## The Honest Assessment

ICN is architecturally real. 38 crates, 70+ endpoints, cryptographic receipts, federation protocol, working demo flows. This isn't a whiteboard drawing.

ICN is not yet deployable. The Meaning Firewall has 11 infected crates. The vertical slice integration test doesn't pass yet. The compliance epic is 60% done. The website is stale. There is no legal entity, no fiscal sponsor, no external funding, and no pilot partner.

The path from here to first deployment is narrow but clear. Phase 2 gets the demo. Phase 3 gets the proof. Phase 4 cleans the architecture. The summit gets the audience. The Sovereign Tech Fund gets the funding. The pilot gets the validation.

Everything depends on execution. One person, constrained income, chronic pain, and a conviction that democratic organizations deserve infrastructure they own.

That's where it stands.
