# ICN Pilot Readiness Assessment

**What works, what's close, and what a pilot cooperative needs.**

---

## Current Demo Flow Status (as of Sprint 16, March 2026)

| Flow | Name | Status | Evidence |
|------|------|--------|----------|
| Flow 1 | Governance (proposal → vote → decision → proof) | **PROVEN** | Runs clean on K3s. 14-step demo verified 2xx on all calls. |
| Flow 2 | Patronage (surplus distribution with formula transparency) | **Updated** | Demo script fixed (PR #1355). Pending K3s redeploy verification. |
| Flow 3 | Federation (cross-coop agreement ratification) | **PROVEN** | Runs clean on K3s after fix (PR #1344). Steps 5/6/7 verified 2xx. |
| Flow 4 | Institutional Reporting (federation-wide audit view) | **Updated** | No route changes needed. Depends on Flows 1-3 data. |

## What a Pilot Cooperative Gets

### Day 1: Core Governance

A cooperative installs ICN and immediately has:

- **Democratic proposal system.** Create proposals with structured payloads (text, budget, allocation). Members vote. Quorum is enforced. Decisions are recorded.
- **Cryptographic receipts.** Every governance action produces a verifiable receipt. The receipt chain links proposals to votes to decisions to allocations.
- **Audit trail.** Any member can query the full governance history. Any authorized external party (funder, federation, auditor) can verify specific decisions.
- **CLI tools.** `icnctl` provides command-line management for all governance operations, including `icnctl audit verify` for receipt chain integrity checks.

### Month 1: Economic Coordination

With governance established, a cooperative can use:

- **Mutual credit.** Track obligations between members or between cooperatives. Double-entry accounting with Merkle-DAG integrity.
- **Patronage distribution.** Governance-approved formulas distribute surplus proportionally to contribution. Every step is transparent and auditable.
- **Settlement tracking.** Record and verify settlements of mutual obligations. Cross-unit settlements with configurable exchange policies.

### Month 3: Federation

Once comfortable with single-node operation:

- **Peer discovery.** Find other ICN nodes on the network via mDNS or manual connection.
- **Federation agreements.** Ratify cross-organization agreements with governance on both sides.
- **Cross-coop visibility.** Read-only access to partner governance records for accountability without control.
- **Resource sharing.** Equipment lending, service exchanges, and joint procurement tracked with provenance.

## What's Not Ready Yet

| Capability | Status | Timeline |
|-----------|--------|----------|
| Mobile member app | UX spec complete, implementation not started | May-Jun 2026 |
| Multi-member identity | Single node DID per coop in demo; multi-member requires key management | Apr 2026 |
| Automated governance execution | Gate wired (PR #1350), full automation in progress | Apr 2026 |
| Production hardening | Security audit, rate limiting, backup/restore tested | Q3 2026 |

## Pilot Requirements

### What the cooperative needs:

- A server, cloud instance, or even a Raspberry Pi (minimum: 2 CPU, 4GB RAM, 10GB storage)
- A technical contact willing to run `icnd` and use `icnctl` (or a managed deployment)
- At least 3 members willing to participate in governance actions during the pilot
- A governance decision they actually need to make (not a toy scenario)

### What ICN provides:

- Installation and configuration support
- Onboarding documentation and guided walkthrough
- Weekly check-ins during the pilot period
- Bug fixes prioritized for pilot-blocking issues

### Ideal pilot partner characteristics:

- **Active governance need.** A cooperative that makes regular collective decisions (budgets, policy, elections)
- **Geographic proximity.** Upstate New York preferred for in-person onboarding support
- **Federation interest.** A cooperative that coordinates with other cooperatives or community organizations
- **Feedback willingness.** Willing to report what works, what breaks, and what's confusing

## Candidate Ecosystem

The NY Cooperative Summit network provides the primary pilot recruitment channel:

| Organization Type | Examples in Network | Governance Complexity |
|-------------------|--------------------|-----------------------|
| Worker cooperatives | Rochester-area coops | High (labor hours, surplus distribution, elections) |
| Housing cooperatives | Regional housing co-ops | Medium (maintenance budgets, capital reserves) |
| Tool libraries | Community tool lending orgs | Low-medium (lending policies, membership) |
| Food cooperatives | Finger Lakes food co-ops | Medium (member dividends, supplier decisions) |
| Cooperative federations | Regional CDNs | High (cross-org coordination, shared services) |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Pilot cooperative lacks technical capacity | Medium | High | Provide managed deployment option |
| Multi-member identity complexity | Medium | Medium | Start with single-node governance, add members incrementally |
| Mobile UX not ready for non-technical members | High | Medium | Desktop/CLI pilot first; mobile follows |
| Pilot org loses interest | Low | High | Choose org with active governance need, not theoretical interest |

---

## Missing Inputs for Pilot Launch

- [ ] Named pilot partner (1-3 cooperatives from NY network)
- [ ] Specific governance use case for each partner
- [ ] Managed deployment option (hosted nodes for non-technical coops)
- [ ] Pilot timeline commitment (minimum 3 months)
