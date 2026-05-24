---
Status: draft
Funder: Sovereign Tech Fund / Sovereign Tech Agency
Amount target: €100,000–€200,000
Deadline: Rolling
Last Reviewed: 2026-05-19
---

# Application Draft — Sovereign Tech Fund

**Apply at:** [sovereign.tech/programs/fund](https://www.sovereign.tech/programs/fund)
**Status:** Draft — rolling, can submit any time

## Critical constraint

**STF cannot stack with other public funding on the same activity.** This means we apply for a slice that is NOT what we ask NLnet to fund. If NLnet funds Option A (federation runtime), STF funds a different slice.

## Fit check

| STF criterion | ICN match |
|---|---|
| Open digital base technology | ✓ Libraries, protocols, dev tools, infrastructure |
| OSI-approved license | ✓ AGPL-3.0 |
| Foundational, not application-layer | ✓ Kernel + identity + trust + ledger; apps build on top |
| Long-term importance | ✓ Cooperative substrate for institutions that need to own their infrastructure |
| Not already publicly funded for same work | ✓ Provided we carve correctly |

## Recommended slice

**Slice: Kernel hardening, identity primitives, and substrate cryptography (€150K, 9 months)**

This is the foundational layer ICN runs on — the layer most aligned with STF's mandate, and the layer NLnet is less likely to fund (NLnet tends toward narrower application-layer commons; STF specifically funds substrate).

Deliverables:
1. **Post-quantum hybrid cryptography** — `icn-crypto-pq` matures from interim implementation to production hardening, including ML-KEM / ML-DSA hybrid handshakes, key rotation primitives, and the X25519 + Kyber768 hybrid scheme.
2. **DID-TLS binding hardening** — second-pass review of the TLS-bound DID scheme; threat model documented; resistance to known classes of cross-protocol attacks formally analyzed.
3. **Replay protection ratchet** — current Bloom-filter saturation mitigation extended; sequence-window rotation under load; long-running production node test.
4. **Misbehavior + reputation runtime** — current 7-violation-type detector extended; quarantine + auto-ban path audited end-to-end; rate-limiting traffic class boundary tested under adversarial load.
5. **Security audit funding** — external review by a recognized firm (Trail of Bits, NCC, or Cure53 equivalent).

## Application sections (STF's structure)

### 1. Project name
InterCooperative Network (ICN) — kernel and identity substrate hardening

### 2. Project description
ICN is a peer-to-peer coordination substrate for democratic organizations — cooperatives, federations, community land trusts, mutual aid networks. The project implements a kernel/app separation: the kernel enforces generic constraints (rate limits, capability tokens, signed envelopes); apps translate domain semantics (governance, mutual credit, membership) into those constraints. The substrate is built in Rust, deployed live on a K3s cluster since December 2025.

This funding request covers the **kernel and identity substrate hardening slice** — the foundational layer every ICN-running cooperative depends on, regardless of the institution-specific apps they install on top.

### 3. Why open-source
ICN is institutional infrastructure for organizations that own themselves democratically. The licensing requirement is intrinsic: you cannot ask a cooperative to depend on infrastructure they don't control. AGPL-3.0 is the licensing choice precisely because it ensures any deployment improves the commons (modifications to the network-deployed code must be shared back).

### 4. Why now
Phase 1 substrate is shipped and live; the substrate spec ladder landed May 14–15 (thirteen design-level architecture-spec documents). What's missing for production readiness is the hardening pass — the substrate works, but key cryptographic primitives are interim, the security audit hasn't happened, and resilience under adversarial load is unproven. This slice closes those gaps before the first cooperative pilot deployment.

### 5. Why us
- 451K lines of Rust across 37 crates, 5,933 passing tests
- 4-node K3s cluster running federated demo flows since 2025-12-03
- Substrate Phase 1 complete: kernel, identity, trust, gossip, ledger, governance, gateway
- Active institutional partner: NYCN (NY Cooperative Network) preparing first pilot
- 70+ REST endpoints, full receipt-chain verification in CI

### 6. Deliverables and timeline (9 months)
- **Months 1–2:** PQ-hybrid cryptography production hardening
- **Months 2–3:** DID-TLS binding threat model + remediation
- **Months 3–4:** Replay protection ratchet hardening under load
- **Months 4–6:** Misbehavior + reputation runtime adversarial-load test
- **Months 6–9:** External security audit + remediation + report publication

### 7. Budget breakdown
- Lead developer time, 9 months @ 25 hrs/wk: €60,000
- External security audit (Trail of Bits class): €60,000
- Fiscal sponsor fees: €10,000
- Test hardware + adversarial-load infrastructure: €15,000
- Publication + dissemination: €5,000

**Total: €150,000**

### 8. Risks and mitigation
- **Risk:** External audit identifies critical vulnerabilities late in cycle
  **Mitigation:** Audit scoped to first 6 months; final 3 months reserved for remediation
- **Risk:** PQ hybrid scheme picks the wrong combination (algorithms still consolidating)
  **Mitigation:** Implement hybrid envelope that's algorithm-agile; expose primitive selection as configuration
- **Risk:** SSDI income constraint on lead developer
  **Mitigation:** SSDI-compatible payment structure; surplus to coop entity; accountant engaged

## Open questions before submission
- [ ] Confirm slice (kernel hardening, or pivot to a different STF-aligned slice)
- [ ] Confirm €150K total ask is appropriate (their minimum is €50K, max €1M)
- [ ] Identify external audit firm + get preliminary scoping quote
- [ ] Confirm SSDI-compatible payment plan with accountant
- [ ] Confirm fiscal sponsor agreement covers EU funder

## Submission checklist
- [ ] Letter of inquiry (STF takes LOI before full proposal)
- [ ] Full proposal
- [ ] Repo + license confirmation linked
- [ ] Audit firm preliminary quote attached
- [ ] Submitted via sovereign.tech application portal
- [ ] Logged here with submission ID
