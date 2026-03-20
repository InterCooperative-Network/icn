# ICN Milestones

**Project timeline from current state through pilot deployment.**

---

## Completed Milestones

| Milestone | Date | Evidence |
|-----------|------|----------|
| Phase 0: Foundation | Dec 2025 | 34-crate Rust workspace, actor-based runtime, all core subsystems |
| Phase 1: Federation Demo | Mar 13, 2026 | 4 demo flows merged, K3s deployment, TypeScript SDK |
| K3s Cluster Deployment | Dec 3, 2025 | 4-node federated demo running on dedicated hardware |
| Epic #1302: Regulatory-Safe Architecture | Mar 19, 2026 | 10/10 sub-issues closed, 8 success criteria verified (PR #1348, #1349, #1351) |
| 6-Link Receipt Chain | Mar 19, 2026 | Vertical slice integration test (PR #1351) proves complete chain in CI |
| Sprint 15: Receipt Chain & Demo Truth | Mar 20, 2026 | 10/10 tasks done. Flows 1+3 PROVEN on K3s. Compliance linter in CI. |

## Current Phase: Grant Prep & Stabilization (Mar 20 - Mar 31, 2026)

| Task | Status | Target |
|------|--------|--------|
| Fix demo scripts for renamed API routes | Done (PR #1355) | Mar 20 |
| Fix K3s deploy pipeline | In progress | Mar 22 |
| Redeploy to K3s + verify all 4 flows | Blocked on pipeline | Mar 23 |
| Package grant artifacts | In progress | Mar 25 |
| Submit Outta Excuses application | Pending | Mar 31 |
| Submit Verizon Digital Ready application | Pending | Mar 31 |

## Near-Term Milestones (Apr - Jun 2026)

| Milestone | Target | Deliverable | Dependencies |
|-----------|--------|-------------|--------------|
| Sovereign Tech Fund application | Apr 2026 | Full application + recorded demo | K3s flows verified |
| First pilot partner identified | Apr 2026 | Named cooperative + needs assessment | NY coop network contacts |
| Mobile member UX v1 | May 2026 | Phone-based voting + receipt verification | React Native SDK |
| Pilot onboarding (1 cooperative) | Jun 2026 | External org running ICN node | Pilot partner, mobile UX |

## Medium-Term Milestones (Jul - Dec 2026)

| Milestone | Target | Deliverable |
|-----------|--------|-------------|
| NY Cooperative Summit workshop | Oct 2026 | Live demo + hands-on session |
| Federation pilot (3-5 coops) | Q4 2026 | Multiple organizations federated on ICN |
| Phase 2 completion (~90%) | Dec 2026 | Production-ready for small-scale deployment |

## Long-Term Vision (2027+)

| Milestone | Target | Deliverable |
|-----------|--------|-------------|
| Article 5-A Worker Cooperative formation | Q1 2027 | ICN development governed cooperatively |
| First non-NY deployment | Q2 2027 | Geographic expansion beyond Finger Lakes |
| v1.0 release | Q3 2027 | Stable API, migration guarantees, documentation |

---

## Key Decision Points

1. **Pilot partner selection (Apr 2026):** Which cooperative or community organization becomes the first external user? Criteria: governance complexity, geographic proximity, willingness to provide feedback.

2. **Mobile-first vs. desktop-first (May 2026):** The mobile UX spec exists (docs/mobile/icn-mobile-ux-spec-v1.md). Decision: invest in React Native native app, or PWA-first with native later?

3. **Legal structure timing (Q1 2027):** When does ICN transition from sole proprietorship to worker cooperative? Depends on: funding secured, pilot partners engaged, at least one additional developer.
