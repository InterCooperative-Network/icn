---
id: "0015"
title: "Semantic Architecture Invariants — ICN Is Not a Blockchain Clone"
status: "accepted"
date: "2026-04-07"
context: "semantic-integrity-pass / bond-slash-correction aftermath"
deciders: ["Matt Faherty"]
tags: ["architecture", "governance", "economics", "identity", "semantics", "guardrails"]
---

# ADR 0015: Semantic Architecture Invariants

## Status

**Accepted (2026-04-07)**

## Context

Following the ADR-0014 bond/slash semantic correction, a broader semantic audit confirmed
that the codebase is largely clean of blockchain/DeFi contamination. This ADR records the
positive semantic invariants that must hold across all future development so that the
original error class cannot re-enter through adjacent work.

## The Core Framing

**ICN is a governable institutional operating system, not a blockchain clone.**

It coordinates real cooperatives and federations via governed, auditable, trust-based
mechanisms. It is NOT:
- A smart contract platform
- A token issuance system
- A staking or validation network
- A permissionless decentralized ledger
- A DeFi protocol

When importing mental models from other systems (distributed systems, crypto protocols,
cooperative law, institutional economics), always ask: **does this concept belong to
ICN's institutional model, or to a different domain?**

---

## Invariant A: Stewardship Is an Institutional Office

**Rule**: Steward authority derives from governance ratification, not financial stake.

**What this means**:
- Stewards are elected or permissioned via governance proposals
- Steward sanctions are status/authority changes (`Warning`, `Censure`, `Suspension`,
  `TierDemotion`, `Removal`, `Probation`) — never financial deductions
- `StewardRecord` has no bond or collateral field (enforced since ADR-0014)
- `EnrollmentToken` is a cryptographic proof of verification, not an economic token
- Trust score informs steward selection (via `TrustPolicyOracle`) but is not a substitute
  for governance ratification

**Canonical source**: `icn-governance/src/sdis.rs`, `icn-commons/src/steward.rs`, ADR-0014

**Red flags for future drift**:
- `bond_amount` field on a steward type
- `slash_steward` method in any storage layer
- Steward appointment requiring a fee, deposit, or collateral
- Sanction variants with financial fields
- Recovery participant authority weighted by held balance

---

## Invariant B: Identity Recovery Is Trust-Governed

**Rule**: Authority in SDIS flows derives from trust, institutional mandate, and governance
approval — never from capital stake.

**What this means**:
- Recovery ceremony participants are selected by trust score + governance mandate
- Vouch/attestation authority is granted via `SdisProposal::ApproveAuthority` — a
  governance proposal, not an economic deposit
- `EnrollmentToken.issuing_steward` is governance-appointed; the token carries no
  transferable economic value
- Dispute arbiters in SDIS are high-trust nodes (`TrustPolicyOracle`-gated), not
  stakers or bonded validators

**Red flags**:
- Recovery quorum weighted by balance
- `VotingWeight` proportional to stake in any SDIS path
- "Bonded verifier" pattern in enrollment or recovery

---

## Invariant C: Governance Must Be Executable, Not Just Ceremonial

**Rule**: Accepted governance proposals must produce real, auditable institutional effects.

**What this means**:
- Every accepted proposal must produce a `KernelEffect` (or `KernelEffect::NoOp { reason }`
  with an explicit justification)
- Silent failures and unhandled proposals are NOT acceptable — use `TranslationError::unsupported`
  with an explicit message so the gap is visible
- Record-only outcomes (`Warning`, `Censure`, `TextResolution`) use `KernelEffect::NoOp`
  intentionally — the governance receipt IS the institutional act
- Deferred effects must be tracked explicitly; "deferred" ≠ "ignored"
- Where governance gaps exist, they must be named (not papered over)

**Current status (2026-04-07)**:
- ~23 proposal types fully wired to real kernel effects
- SDIS: `SanctionSteward` wired (all penalty types)
- SDIS: `ModifyThreshold`, `ApproveAuthority`, `RevokeAuthority`, `RevocationAppeal`,
  `UpdateJurisdictionTier`, `ForceKeyRotation` — explicitly deferred, fail-fast error

**Red flags**:
- `Ok(vec![])` for a non-trivially record-only proposal (empty means nothing happened)
- Swallowed `TranslationError` that produces no receipt
- Test that asserts proposal "succeeds" without checking what state changed

---

## Invariant D: Economics Must Be Governed Accounting

**Rule**: All economic operations must be authorized by governance, expressed as
governed accounting entries, and traceable to audit receipts.

**What this means**:
- Treasury operations (`Spend`, `CreateBudget`, `IssueBond`, `DistributeSurplus`) require
  an accepted governance proposal as the authorizing act
- `BondIssuance` in governance is a cooperative capital-raising mechanism —
  governance-approved, not blockchain-minted
- Commons credit settlement is mutual-credit bookkeeping under cooperative governance —
  not a payment rail or settlement network
- Federation clearing uses bilateral institutional agreements (`BilateralClearingAgreement`)
  with governance-approved terms — not automated liquidity pools

**What does NOT belong**:
- Token minting or burning unilaterally by any party
- "Yield", "liquidity", "swap", "slippage", "arbitrage" language in core economic types
- Supply-side incentive mechanisms (block rewards, mining, staking yields)
- Balance transfers that bypass governance authorization

**Legitimate economic mechanisms that SHOULD remain**:
- `CooperativeBond` (governance-approved capital raise, maturity/interest tracked)
- `EscrowHold` (governance-linked idempotent hold for compute/settlement)
- `ClearingPosition` (bilateral imbalance tracking with governance-approved terms)
- `BudgetAllocation` (governance-authorized spending envelope)
- `SurplusDistribution` (governance-approved patronage allocation)

---

## Invariant E: Kernel/Policy Boundary Must Hold

**Rule**: The kernel enforces structure and integrity; apps/governance authorize
institutional changes; CCL/policy defines higher-order meaning.

**What this means**:
- Kernel crates (`icn-core`, `icn-gateway`, `icn-gossip`, `icn-net`) NEVER import
  domain crates (`icn-trust`, `icn-governance`, `icn-ccl`)
- Domain semantics (trust scores, governance rules, membership criteria) must not
  appear in `KernelEffect` variants as named domain concepts
- `KernelEffect` carries only generic primitives: IDs, hashes, amounts, timestamps
- Policy/CCL may define downstream consequences for governance decisions (e.g., a
  censure may reduce trust score per CCL policy) — these consequences are NOT in kernel

**Enforced by**:
- `icn/.github/workflows/ci.yml` — `Meaning Firewall Check` gate (required, blocking)
- `icn/.claude/rules/kernel-boundary.md` — agent coding constraint
- `icn/crates/icn-core/src/meaning_firewall.rs` — runtime boundary enforcement

---

## Invariant F: Federation Is Scoped Institutional Coordination

**Rule**: Federation is governed inter-cooperative coordination with explicit agreements,
not permissionless decentralization.

**What this means**:
- Cooperatives join federations via `JoinFederation` governance proposals
- Federation terms (`BilateralClearingAgreement`, vouching thresholds) are explicit
  governance documents, not emergent from protocol incentives
- "Decentralized" language is acceptable when it means "no central server required"
  but misleading if it implies permissionless global consensus
- Preferred language: "federated", "cooperative coordination", "scoped governance",
  "bilateral agreement", "trust-routed"

---

## Overloaded Terms Reference

| Term | ICN Meaning | NOT this |
|------|-------------|----------|
| `bond` | Governance-approved cooperative capital instrument (icn-ledger) | Validator stake deposit |
| `consensus_threshold` | Fraction of executors agreeing on result | BFT/blockchain finality |
| `settlement` | Mutual-credit reconciliation / task payment completion | Blockchain finality |
| `token` | Cryptographic proof of verification (EnrollmentToken) | Cryptocurrency token |
| `treasury` | Governed cooperative financial pool | Protocol-owned liquidity |
| `escrow` | Governance-linked idempotent hold | Multi-sig smart contract escrow |
| `governance` | Real executable institutional authorization | Off-chain signaling |
| `trust score` | Social trust computed from participation, NOT capital weighted | Credit score |
| `commons` | Shared institutional resource pool (CommonsHandle three-layer model) | Public goods vague |
| `federation` | Scoped inter-cooperative coordination via bilateral agreements | Blockchain federation |

---

## How to Apply in Code Review

When reviewing any PR that introduces new types, ask:

1. **Does this type carry authority that should be trust/governance-derived?**
   If yes: does it require a governance proposal? Is appointment tracked?

2. **Does this type have financial fields attached to an institutional role?**
   If yes: is this a legitimate economic instrument (bond, escrow) or contamination
   of an institutional office (steward, member, trustee)?

3. **Does this proposal type produce a real effect on accepted?**
   If no clear effect: should it be `NoOp { reason }` with an explicit justification?

4. **Does the naming carry blockchain/DeFi baggage?**
   If yes: is it disambiguated in doc comments? Would a cooperative accountant or
   a cooperative lawyer understand the term correctly?

5. **Is the kernel boundary intact?**
   Run `grep -rn 'use icn_trust::' crates/icn-core/` — must produce no output.

---

## See Also

- ADR-0014: Stewardship Semantics Boundary (bond/slash removal, specific steward invariants)
- `icn/.claude/rules/kernel-boundary.md` — Agent coding constraint for kernel boundary
- `icn-governance/src/sdis.rs` — `StewardPenalty` enum (canonical reference for sanction semantics)
- `icn-kernel-api/src/effects.rs` — `KernelEffect` enum (canonical reference for what kernel can do)
