# ICN Roadmap: Closing the Gaps

**March 17, 2026 — Grounded in what's actually proven, not what the docs aspire to.**

---

## Status Overview

| Category | Count |
|----------|-------|
| **Proven end-to-end** | 1 flow (governance: proposal → vote → decision → cryptographic proof) |
| **Endpoints exist, not demo'd** | 3 flows (patronage, federation, reporting — merged from demo branch, unverified) |
| **Designed, not integrated** | 2 receipt chain links (Allocation → Execution) |
| **Claimed but doesn't exist** | 1 CLI command (`icnctl audit verify`) |
| **Compliance epic** | 3/10 done (#1307, #1309, #1310) |
| **Meaning Firewall** | 9/29 crates clean, 11 infected, 9 needs-review |

---

## The Roadmap

### NOW: Audit & Prove What We Have (Mar 17–23)

This week answers one question: **what actually works when you run it?**

| Item | Description | Status | Dependencies | Owner |
|------|-------------|--------|--------------|-------|
| **N1: Verify 3 unproven demo flows** | Run patronage, federation, reporting flows from merged demo branch end-to-end on K3s. Record what passes, what fails, what panics. | **Not Started** | K3s cluster running (confirmed) | Matt |
| **N2: Runtime-verify governance flow** | Run the 14-call governance demo (vertical slice assessment §6) against live `icnd` on K3s. Confirm proof endpoint returns valid, verifiable receipt. | **Not Started** | icnd running with `--gateway-enable` | Matt |
| **N3: Classify each flow honestly** | For each of the 4 flows, assign: PROVEN (runs clean), FRAGILE (runs with workarounds), BROKEN (fails), MISSING (doesn't exist). Update all docs to match. | **Not Started** | N1, N2 complete | Matt |
| **N4: Write 1-page demo script** | Pick the strongest scenario from N3. Write what the audience sees and what fires underneath. This is the demo for everything downstream. | **Not Started** | N3 | Matt |

**Exit criteria:** Honest classification of all 4 demo flows. One demo script for the strongest path.

**Hard deadline:** March 19 summit planning call — need to know what you can credibly promise for the May workshop proposal.

---

### NEXT: Close Receipt Chain + Compliance (Mar 24 – Apr 13)

Two parallel tracks. Both must complete before any grant application or summit demo.

#### Track 1: Complete the 6-Link Receipt Chain

The whitepaper and scenarios claim: Proposal → Discussion → Votes → Decision → Allocation → Execution. Reality: the first four links work. The last two are designed but not wired.

| Item | Description | Status | Est. Effort | Dependencies | Priority |
|------|-------------|--------|-------------|--------------|----------|
| **X1: AllocationProposal** (#1311) | Governance proposal type that produces AllocationReceipt when accepted. Closes the "decision authorizes allocation" link. | **Not Started** | 2-3 days | None | P0 |
| **X2: Allocation→Execution wiring** | ExecutionReceiptGate (#1310, merged) requires an AllocationReceipt to produce an ExecutionReceipt. Wire these together so `icnctl audit verify` has a complete chain to walk. | **Not Started** | 2 days | X1 | P0 |
| **X3: `icnctl audit verify`** | CLI command that takes a receipt ID, walks the chain (Decision → Allocation → Execution), verifies every signature, and returns pass/fail. This is the demo's punchline. | **Not Started** | 2-3 days | X2 | P0 |
| **X4: Vertical slice integration test** (D1) | `cargo test --test vertical_slice_integration` — exercises the complete 6-link chain in CI. When this passes, it's v1.0. | **Not Started** | 2 days | X3 | P0 |

**Critical path:** X1 → X2 → X3 → X4. Sequential. ~8-10 days.

#### Track 2: Compliance Sprint (Parallel)

Required before Sovereign Tech Fund application. Closes Epic #1302.

| Item | Description | Status | Est. Effort | Priority |
|------|-------------|--------|-------------|----------|
| **C1: Terminology rename** (#1303) | payment→settlement, currency→unit, balance→position. Grep-and-replace + test updates. | **Not Started** | 1 day | P0 |
| **C2: UX language guide** (#1304) | Seven Invariants PR checklist in CONTRIBUTING.md. What words ICN uses and why. | **Not Started** | 0.5 days | P0 |
| **C3: JournalEntry.provenance** (#1305) | Make ProvenanceRef required on every ledger entry. No provenance = compile error. | **Not Started** | 1 day | P1 |
| **C4: Obligation lifecycle** (#1306) | Issued→Accepted→Settled→Defaulted→Disputed. Uses AssetType::Claim. | **Not Started** | 1-2 days | P1 |
| **C5: CCL formula extraction** (#1308) | Move commons credit formula from kernel to CCL PolicyOracle parameter. Meaning Firewall hygiene. | **Not Started** | 1 day | P1 |
| **C6: CI compliance linter** (#1312) | GitHub Actions check that fails if `payment`, `currency`, `balance` appear as API-exposed terms. Self-enforcing terminology discipline. | **Not Started** | 0.5 days | P0 |

**Already done:** #1307 (PatronageTracker), #1309 (DelegationManager), #1310 (ExecutionReceiptGate).

**Effort:** ~5-6 days total, parallelizable with Track 1.

---

### NEXT: Demo-Ready Package (Apr 7–17)

Only starts after Track 1 (receipt chain) is complete. The demo must show real receipts, not simulated ones.

| Item | Description | Status | Dependencies |
|------|-------------|--------|--------------|
| **D1: Update demo scripts to real calls** | The four flows in `demo/scripts/` call real `icnctl` commands instead of simulated responses. `present.sh` becomes a live demo. | **Not Started** | X3 (audit verify exists) |
| **D2: Record walkthrough** | Screen recording with narration, 5-10 minutes. The demo scenario from N4 running live. | **Not Started** | D1 |
| **D3: Draft summit workshop proposal** | "Live Demo: Spin Up a Cooperative in 5 Minutes." Title, description, learning outcomes, format. Ready for May submission. | **Not Started** | N4 (demo script) |
| **D4: Update the four new docs** | Add honest status annotations to ICN-Scenarios, ICN-Pitch, ICN-Whitepaper, ICN-Roadmap-Strategy. Change "produces" to "produces" only where proven; mark others "designed, integration pending." | **Not Started** | N3 (classification) |

**Exit criteria:** Recorded demo. Workshop proposal draft. All docs match reality.

---

### LATER: Architecture Cleanup + Scale (Apr–Jun)

| Item | Description | Est. Effort | Dependencies |
|------|-------------|-------------|--------------|
| **L1: Meaning Firewall cleanup** | Extract semantic business logic from 11 infected kernel crates. Goal: 29/29 clean. The single most important technical task. | 2-3 weeks | Compliance sprint done |
| **L2: Website overhaul** | Stats sync automation, 4 blog posts, Matrix room, mailing list, community page. | 1-2 weeks (parallel) | Demo recording exists |
| **L3: Federation settlement finality** | Formal spec for how cross-org obligations reach finality. Currently unspecified gap. | 1 week | L1 |
| **L4: Mobile SDK stabilization** | React Native key management, push notification flow, receipt verification on device. | 2 weeks | X4 (receipt chain) |
| **L5: Sovereign Tech Fund application** | €50K+ grant for open digital infrastructure. Requires: clean terminology, working demo, architecture doc. | 1 week to write | C1-C6 done, D2 done |
| **L6: Pilot partner recruitment** | 3-touch outreach to upstate NY co-ops. First pilot commitment. Gate B passage. | Ongoing | D2, D3 |
| **L7: NAT traversal** (C3) | K3s pods federate for real. Transforms demo from "four pods on one cluster" to "four nodes actually discovering each other." | 2-3 days | Track 1 done |

---

## Dependencies Map

```
N1,N2 (verify what works)
  → N3 (classify honestly)
    → N4 (demo script)
      → D3 (workshop proposal)   ← Mar 19 call needs at least a verbal pitch
      → D1 (real demo scripts)
        → D2 (recorded walkthrough)
          → L2 (website)
          → L5 (STF application)
          → L6 (pilot recruitment)

X1 (AllocationProposal)
  → X2 (Allocation→Execution wiring)
    → X3 (icnctl audit verify)
      → X4 (vertical slice test = v1.0.0 tag)
        → D1 (demo scripts use real commands)
        → L4 (mobile SDK)

C1-C6 (compliance sprint) ← parallel, no dependencies on Track 1
  → L1 (Meaning Firewall cleanup)
  → L5 (STF application)
```

---

## Risks

| Risk | Impact | Prob. | Mitigation |
|------|--------|-------|------------|
| **N1 reveals all 3 unverified flows are broken** | "Four demo flows" claim collapses to one | Medium | Simplify: demo the governance flow only. It's the strongest. Federation is scenery, not the punchline. |
| **Receipt chain wiring (X1-X2) reveals architectural issues** | Track 1 timeline blows out | Medium | ExecutionReceiptGate is already merged and tested. AllocationProposal is well-scoped (#1311). The risk is integration, not design. |
| **Compliance rename (C1) breaks tests across 38 crates** | Day turns into a week | Low | It's grep-and-replace on 3 terms. The Forward Plan explicitly says "nothing behavioral changes." |
| **Solo developer capacity** | Everything takes 2x | High | Ruthless scope control. Only touch what the demo and the grant application need. Phase 2 plan already says this. |
| **March 19 call arrives before N1-N3 are done** | Can't credibly pitch a workshop | High | You know the governance flow works. Pitch that. "Live demo of provable cooperative governance." Don't promise federation demo yet. |
| **STF application needs clean codebase** | Can't apply until compliance sprint is done | Medium | C1 (terminology rename) and C6 (CI linter) are the P0s. Do those first. C3-C5 can follow. |

---

## Hard Deadlines

| Date | Event | What Must Be True |
|------|-------|-------------------|
| **Mar 19** | Summit planning call | Know what you can credibly demo. RSVP confirmed to Joe. |
| **May 2026** | Call for presenters | Workshop proposal submitted. Demo script finalized. |
| **Apr–May** | STF application | Compliance sprint complete. Demo recorded. Architecture doc ready. |
| **Oct 2026** | NY Coop Summit | Live demo. Pilot partner recruited. Workshop delivered. |

---

## What Changed From Previous Roadmap

The previous roadmap (ICN-Roadmap-Strategy.md) was written before the gap analysis. This version corrects it:

| Before | After | Why |
|--------|-------|-----|
| "Four working demo flows" treated as fact | Reclassified: 1 proven, 3 unverified | Gap analysis found only governance flow is confirmed end-to-end |
| Receipt chain described as complete | Split into 4 proven links + 2 pending | AllocationProposal (#1311) and Allocation→Execution wiring not yet merged |
| `icnctl audit verify` in success criteria | Explicitly marked as "doesn't exist yet" | It's a Phase 3 deliverable, not current state |
| Phase 2 "Vertical Slice" and Phase 2 "Demo-Ready" conflated | Separated: NOW (audit), NEXT (build + comply), NEXT (package) | Can't package what you haven't verified |
| Compliance sprint and receipt chain interleaved | Parallelized explicitly | No dependencies between them; both block STF application |
| Track E (Compute Substrate) in Phase 2 | Moved to LATER | Mana is descoped. Compute substrate is post-v1.0. Don't touch it now. |

---

## The Honest Summary

**1 thing is proven:** The governance pipeline produces real cryptographic proofs.

**3 things need verification this week:** Patronage, federation, and reporting demo flows.

**2 things need building in the next 2 weeks:** AllocationProposal and `icnctl audit verify` to complete the receipt chain.

**7 things need cleanup before a grant application:** The compliance epic (#1302), with 3 done and 7 remaining.

**1 thing determines v1.0:** The vertical slice integration test passing in CI.

**1 thing determines adoption:** A cooperative organizer watching a demo and saying "I want that."

Everything else is sequencing.
