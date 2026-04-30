---
title: "ICN Operating Model — How It Actually Works"
status: "current"
date: "2026-04-07"
audience: "contributors, reviewers, grant readers"
---

# ICN Operating Model

> This document explains how ICN actually works — not what it aspires to be,
> but what the code currently does, how the layers compose, and where the
> seams are.

---

## The Core Model in One Paragraph

ICN is a **governed institutional operating system**. Cooperatives and federations
use it to make binding institutional decisions (governance), track and move
resources under those decisions (economics), verify and recover member identities
(SDIS), and coordinate across organizational boundaries (federation). The
system has a kernel/app separation: the kernel enforces constraints without
understanding their meaning; apps translate domain semantics into constraints
the kernel can enforce blindly. This architecture — the "meaning firewall" — is
the central invariant.

---

## Layer 1: Governance (how decisions are made)

### The Flow

```
Member submits proposal
       ↓
Governance app validates + stores the proposal
       ↓
Members vote (on-chain ballots, tracked in icn-governance)
       ↓
Tally reaches quorum + approval threshold
       ↓
Proposal accepted → produce a DecisionReceipt
       ↓
translate_payload_to_effects() → Vec<KernelEffect>
       ↓
KernelEffect executed by DefaultEffectExecutor
       ↓
EffectResult stored → audit chain
```

### What "accepted" means

An accepted proposal ALWAYS produces at least one observable output:
- A real durable state change (most cases), OR
- A `KernelEffect::NoOp { reason }` with an explicit justification (for
  record-only proposals like `Warning`, `Censure`, or `Text` resolutions)

There is no path where an accepted proposal silently disappears. The audit
chain always receives an `EffectResult`. See ADR-0015 Invariant C.

### Proposal categories

| Category | Examples | Typical Effect |
|----------|---------|----------------|
| Treasury | Spend, Budget, Allocation, IssueBond | Ledger journal entry |
| Membership | Add, Remove, Freeze, Unfreeze | Membership record change |
| Steward/SDIS | Appoint, Suspend, Remove | Steward record change |
| Federation | JoinFederation, EstablishClearing | Federation registry change |
| Protocol | ConfigChange, SchedulingPolicy | Config hash stored |
| Control | VetoProposal, ForceClose, Text | Record-only or veto action |
| Charter | Charter ratification | Policy config deployed |
| Resource | GrantAccess, RevokeAccess | Resource access record |

For the full execution map, see:
`docs/architecture/governance-execution-inventory.md`

### What is NOT governance

Governance in ICN is **executable institutional authorization**, not
off-chain signaling. A `Text` resolution's value is the governance receipt
in the audit trail. An `AppointSteward` vote's value is the resulting
`StewardRecord` in the commons store. Proposals that produce nothing are
bugs, not features.

---

## Layer 2: Economics (how resources move)

### What the economic layer is

ICN economics is **governed accounting**: all resource movements require
prior governance authorization, produce auditable ledger entries, and are
traceable to the accepting vote.

The ledger (`icn-ledger`) is a Merkle-DAG double-entry journal. Every
economic event is a `JournalEntry` with:
- Debit and credit sides (double-entry)
- A provenance hash linking it to the authorizing governance decision
- A content hash for immutability

The ledger is NOT:
- A token/coin system
- A payment rail
- A blockchain with fees

It's cooperative bookkeeping with cryptographic integrity.

### Economic instruments that exist

| Instrument | What it is | How it's authorized |
|-----------|------------|-------------------|
| `Treasury` | Governed cooperative financial pool | Treasury proposals |
| `Budget` | Spending envelope within a treasury | `Budget` / `Allocation` proposals |
| `CooperativeBond` | Governance-approved capital raise | `BondIssuance` proposal |
| `EscrowHold` | Governance-linked idempotent compute hold | Compute task escrow |
| `ClearingPosition` | Bilateral inter-coop imbalance tracking | `EstablishClearing` proposal |
| `SurplusDistribution` | Governance-approved patronage payout | `SurplusAllocation` proposal |

### The settlement flow

```
Governance vote (e.g., Spend) accepted
       ↓
TreasuryEffect::Spend → treasury executor
       ↓
Ledger debit (treasury) + credit (recipient) journal entries
       ↓
EffectResult with ledger_entry_id
       ↓
Audit chain: decision_hash → journal_entry → state_hash
```

### What "settlement" means

In ICN, "settlement" means mutual-credit reconciliation — completing a
cooperative accounting transaction. It does NOT mean "blockchain finality"
or "payment network settlement." Federation clearing settlement nets bilateral
imbalance positions and produces a single ledger entry per settlement cycle.

---

## Layer 3: Identity / SDIS (how members are verified and recovered)

### What SDIS is

SDIS (Sovereign Digital Identity System) is ICN's privacy-preserving identity
layer. It lets cooperatives verify that members are unique humans without
revealing their real identity to the cooperative itself.

The key mechanisms:
- **POP (Proof of Personhood)** ceremonies: steward-mediated verification
- **Blind signatures**: steward signs without seeing what they're signing
- **EnrollmentToken**: cryptographic proof of verification, not economic token
- **Recovery ceremonies**: M-of-N trust-based account recovery

### How authority flows in SDIS

```
Governance ratification → Steward appointment
                ↓
         Steward mandate
                ↓
    Steward signs blinded enrollment
                ↓
    User unblinds → EnrollmentToken
                ↓
     Token proves membership without linkability
```

Authority derives from **governance ratification**, not from:
- Capital stake
- Collateral posted
- Economic weight

A steward's signing authority is valid because the cooperative voted to
appoint them. The `EnrollmentToken` is a cryptographic proof of that
governance-backed verification. See ADR-0014 and ADR-0015 Invariant A.

### Recovery

Recovery is M-of-N threshold-based: a user can recover their identity if M
out of N high-trust stewards attest to their identity. Recovery participant
selection is trust-gated (via `TrustPolicyOracle`) — it is never
capital-weighted.

---

## Layer 4: Federation (how cooperatives coordinate)

### What federation is

Federation is **scoped inter-cooperative coordination** via explicit governance
agreements. Two cooperatives can:
- Join a common federation (shared governance namespace)
- Vouch for each other (trust propagation)
- Establish bilateral clearing agreements (inter-coop accounting)

Federation is NOT:
- Permissionless decentralization
- Blockchain-style global consensus
- Automated liquidity routing

Every federation relationship is authorized by governance votes in both
participating cooperatives.

### The federation data model

```
FederationRegistry: coop_did → FederationRecord {
    federation_id, decision_receipt_id, gateway_endpoints
}

BilateralClearingAgreement: agreement_id → {
    coop_a_did, coop_b_did, settlement_interval, max_imbalance
}

VouchRecord: voucher_did × vouchee_did → trust_score
```

### The clearing model (ADR-0011 through ADR-0013)

```
EstablishClearing (governance) → BilateralClearingAgreement
        ↓
Inter-coop transfers accumulate as ClearingPosition
        ↓
SettleClearing (governance or periodic) → net single ledger entry
```

---

## The Kernel/App Boundary

### The meaning firewall

```
┌─────────────────────────────────────────────────────────────┐
│  DOMAIN LAYER (apps)                                         │
│                                                             │
│  trust scores, governance rules, membership tiers,          │
│  steward mandates, CCL contracts...                         │
│                                                             │
│  TrustPolicyOracle → translate → ConstraintSet              │
│  GovernanceApp → translate → Vec<KernelEffect>              │
├─────────────────────────────────────────────────────────────┤
│  MEANING FIREWALL                                           │
│  (kernel never imports domain crates)                       │
├─────────────────────────────────────────────────────────────┤
│  KERNEL LAYER (icn-core, icn-gateway, icn-net, icn-gossip)  │
│                                                             │
│  rate limits, credit amounts, DIDs, hashes, timestamps...   │
│  enforced blindly; no domain semantics visible here         │
└─────────────────────────────────────────────────────────────┘
```

### Why this matters

The meaning firewall ensures:
1. The kernel is predictable and auditable regardless of policy changes
2. Apps can evolve governance rules without touching the kernel
3. The kernel cannot be corrupted by domain-level disputes (e.g., "what does
   'trust' mean?" is an app concern, not a kernel concern)

Enforced by: `ci.yml` Meaning Firewall Check gate (required, blocking).

---

## How the Layers Compose

```
Member action (e.g., "I want to spend from treasury")
       ↓
Membership check (governance app: am I a member?)
       ↓
Submit SpendProposal
       ↓
Governance vote
       ↓
Treasury::Spend kernel effect
       ↓
Ledger journal entry (debit treasury, credit recipient)
       ↓
Audit receipt (linked to vote → journal → state hash)
       ↓
Recipient's balance updated in cooperative accounting
```

The same audit chain applies to every institutional action — steward
appointment, member removal, federation clearing, charter ratification.
Every institutional act is traceable from the governance vote to the
state change.

---

## Known Gaps (as of 2026-04-07)

These are explicitly named, not silently missing:

| Gap | Impact | Resolution path |
|-----|--------|-----------------|
| `UpdateJurisdictionTier` executor deferred | Tier changes don't persist | Add `CommonsHandle::update_jurisdiction_tier` |
| `TerminateClearing` executor deferred | Clearing termination not durable | Add `FederationService::terminate_clearing` |
| `RevokeVouch` executor deferred | Vouch revocation not durable | Add `FederationService::revoke_vouch` |
| `ModifyThreshold` SDIS has no kernel path | Threshold changes not executable | Design threshold registry + ADR |
| `ApproveAuthority` / `RevokeAuthority` | Authority registry doesn't exist | Design authority registry |
| Timed auto-reinstatement | Suspended stewards need manual reinstate | Add CommonsHandle scheduler or cron |
| Dispute outcome completeness | Only `Partial` outcome wires compensation | Handle `Uphold`/`Reject`/`VoidTransaction` |

See `docs/architecture/governance-execution-inventory.md` for the full map.
