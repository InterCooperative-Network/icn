# Regulatory-Safe Verifiable State Architecture

**Status**: Accepted — Phases 22-24
**Priority**: Tier 1 — Foundational positioning
**Companion**: `nat-traversal-design.md`, `ipv6-endpoint-sets-design.md`

---

## Core Thesis

This document is already implicit in the codebase. From `icn-kernel-api/src/economics.rs`:

> *"ICN's thesis: money is not value, it's a claim on value."*

The `AssetType::Claim` variant, `ExecutionReceipt`, and `GovernanceDecisionReceipt` already exist.
`receipt` and `attestation` appear 800+ times across the codebase. The settlement engine is already
a neutral kernel-level enforcer that understands nothing about what balanced entries "mean."

**The problem is not the architecture. The problem is that the surface language still says
"currency", "payment", and "balance"** — creating regulatory exposure where none is architecturally
necessary, and obscuring what ICN actually is: communications infrastructure for verifiable cooperative
coordination.

---

## Problem Statement

Any decentralized coordination network that tracks the exchange of labor and resources faces three
regulatory classification risks if its surface behavior resembles financial infrastructure:

1. **FinCEN MSB classification** — triggered by *intermediation + custody*. If any ICN-operated
   component "accepts value from A and transmits it to B," or maintains hosted accounts with unilateral
   operator control, it qualifies as a Money Services Business with full BSA/AML compliance obligations.

2. **IRS barter exchange classification** — triggered by operating as a *brokered commercial marketplace*.
   If ICN acts as an exchange operator (centralized matching, scrip issuance, Form 1099-B obligations),
   it assumes heavy reporting requirements.

3. **Regulatory drift** — even if the initial architecture is clean, future features can accidentally
   introduce custody, operator-mediated settlement, or market-making semantics. Without explicit invariants
   enforced at the PR level, the system will drift toward the regulated pattern.

### The Trigger Is Intermediation, Not Ledgers

A ledger is not inherently regulated. Cryptography is not inherently regulated. The trigger is:

- Does the network operator **accept** value from one party?
- Does the operator **transmit** that value to another?
- Does the operator maintain **custody** of keys or accounts and control balances unilaterally?

If the answer to all three is *no*, and state transitions are peer-signed and non-custodial, the system
is a communications and records platform — not a financial intermediary.

---

## Threat Model: What Triggers Regulatory Attention

| Behavior | Risk level | Why |
|----------|-----------|-----|
| Operator routes "transfers" between accounts | HIGH — MSB | Intermediation + custody |
| Hosted accounts where operator can move balances | HIGH — MSB | Unilateral operator control |
| Protocol embeds exchange rates, USD pegs, "redeem for cash" | HIGH — MSB + securities | Convertibility claims |
| Centralized service-matching / order book | HIGH — barter exchange | Operator-run marketplace |
| Issuing "scrip" or "trade dollars" that behave like currency | MEDIUM — barter exchange | Commercial scrip |
| Operator-controlled clearing of net obligations | MEDIUM — MSB | Clearing = intermediation |
| User-signed state transitions, peer-to-peer | LOW — software provider | No custody, no intermediation |
| Non-custodial keys, local computation, broadcast of signed state | LOW — software provider | Unhosted wallet pattern |
| Derived balance views computed from attestations | LOW | Interpretation, not primitive |
| Membership-scoped, governance-authorized allocations | LOW | Internal capital accounts |

---

## The Seven Architecture Invariants

These are the "don't accidentally become a bank" invariants. They must be enforced in code review,
CI, and architectural decisions.

### Invariant 1: User-Signed Transitions Only

Every state transition that could be interpreted as "value movement" must be signed by the
participant(s) whose obligations change. ICN infrastructure may *broadcast* these signed transitions;
it must never *originate* them on behalf of a user.

**Code implication**: No gateway endpoint that initiates an obligation change without a user-provided
signature over the specific transition.

### Invariant 2: No Hosted Balances

ICN nodes do not maintain custodial accounts where the operator can move balances unilaterally.
The ledger state is a Merkle-DAG of content-addressed, peer-signed entries. The "balance" of a member
is a derived view computed from those entries — not a value stored under operator control.

**Code implication**: No endpoint that accepts "move X from account A to account B" with operator-side
authorization. Transfers require the signing key of the originating account.

### Invariant 3: No Operator Routing of Value

Gateways route *messages*, not *transfers*. If a message carries what looks like "send," it is
broadcasting a user-signed state transition — not the gateway executing a transfer on a user's behalf.

**Code implication**: The gateway's ledger endpoints are read/write proxies to the peer's own signed
entries. The gateway does not hold the signing keys.

### Invariant 4: Derived Views Are Not Protocol Primitives

"Balance," "credit," "net position" are computed from the attestation/obligation graph. They are
*interpretations*, not the protocol's source of truth. The source of truth is the set of signed
receipts and accepted obligations.

**Code implication**: `JournalEntry` (and its future `Obligation` successor) is the primitive.
`AccountState`/`position` is a view derived from it. APIs that return balances must not accept
balance-modifying writes — those go through the signed entry path.

### Invariant 5: No Embedded Convertibility

The protocol must not embed "redeem for cash," "pegged to USD," "exchange rate for external currency,"
or any claim of convertibility to fiat or external assets. If communities establish social conventions
around unit values, that is outside protocol scope and outside ICN's operational surface.

**Code implication**: No `exchange_rate` field, no `fiat_equivalent`, no `redemption` semantics in
any kernel or API type. If CCL governance wants to define custom exchange rates between internal units,
that is a governance policy — not a protocol primitive.

### Invariant 6: Matching and Market Features Are Opt-In, Scoped, and Governance-Authorized

Service discovery, offer matching, resource scheduling, and allocation proposals must be:
- Explicitly scoped to a federation or organization (not global)
- Authorized by member governance (not operator decision)
- Fully auditable
- Never operated as a clearinghouse by the ICN node itself

**Code implication**: Service discovery endpoints return data; they do not execute matches. Any
"allocation" is a governance proposal result, not an operator function.

### Invariant 7: Execution Receipts Close the Loop

Governance decisions must be coupled to cryptographic proof of execution. "We voted to allocate
resources to Project X" must produce a verifiable receipt proving the allocation actually occurred.
This is Digital Public Infrastructure behavior, not fintech behavior.

**Code implication**: The `decision_receipt_id` field on `JournalEntry` must become *required*, not
optional. Accepted proposals that involve resource allocation must generate a linked `ExecutionReceipt`.

---

## Protocol Primitives

The base layer of ICN economics models these — not payments:

### Receipt Attestation
*"X delivered service Y at time T under scope S"*

Signed by the recipient of the service. Unforgeable because it requires the recipient's key.
Authorizes a corresponding obligation acknowledgement. Already exists as `ExecutionReceipt`.

### Obligation Acceptance
*"I accept obligation O under terms T, in exchange for Receipt R"*

Signed by the party accepting the obligation. Tracks a future claim on cooperative capacity.
Currently implicit in `JournalEntry` — needs a first-class `Obligation` type.

### Governance Action
*"Proposal → Deliberation → Vote → Execution Receipt"*

Full lifecycle with cryptographic proof at each step. `GovernanceDecisionReceipt` already exists.
Missing: enforcement that accepted proposals produce linked `ExecutionReceipt`s.

### Allocation Decision
*"Members voted to allocate N units of capacity to purpose P"*

Participatory budgeting as resource scheduling, not payments. Not yet implemented. Phase B.

### Dispute / Appeal
*"I contest attestation A; here is counter-evidence E"*

Mechanism for contesting receipts before they generate obligations.
`DisputeResolution` proposal type exists; needs linking to the receipt/obligation lifecycle.

---

## Anti-Features: Explicitly Forbidden in Protocol Design

These behaviors must not appear in ICN protocol definitions, gateway APIs, or SDK types:

| Anti-feature | Why forbidden |
|---|---|
| `send_funds(from, to, amount)` semantics | Operator-mediated transfer |
| `redeem()` or `exchange_for_fiat()` | Convertibility claim |
| `operator_adjust_balance()` | Custodial control |
| Currency symbols that match or imply fiat equivalence | Regulatory confusion |
| Global service marketplace with operator-side matching | Barter exchange operation |
| `CreatePaymentRequest` or `PaymentOrder` types | Money-like naming in public API |
| Any type named `Wallet` that the operator controls | Hosted wallet classification |

---

## Semantic Mapping: Current → Target

| Current | Target | Scope of change |
|---------|--------|-----------------|
| `currency` field on `AccountDelta` | `unit` | Moderate — touches core types |
| `Currency` struct | `UnitOfAccount` | Moderate |
| `balance` in gateway API | `position` | Low — API surface only |
| `CreatePaymentRequest` | `RecordTransferRequest` | Low — gateway models |
| `payment` endpoint | `settlement` or `record` | Low — gateway routes |
| `CreditLimit` | `ObligationCeiling` | Low-moderate |
| `commons-credits` currency string | `commons-capacity` or `participation-units` | Low — constant |
| `AccountBalances` | `AccountState` | Moderate |
| `TransactionHistoryEntry` | `LedgerRecord` | Low |
| `mutual_credit()` constructor | `reciprocal_account()` | Low |
| `debit`/`credit` in `AccountDelta` | **Keep** — standard bookkeeping mechanics | None |
| `JournalEntry` | **Keep** — standard bookkeeping term | None |
| `SettlementEngine` | **Keep** — already good | None |
| `receipt`, `attestation`, `obligation` terms | **Keep and expand** — already aligned | Expand |

---

## Structural Gaps to Address

Beyond terminology, these structural gaps must be closed for the architecture to match the thesis:

### Gap 1: Optional Provenance (Phase A)
`decision_receipt_id` and `decision_hash` on `JournalEntry` are currently optional fields.
For verifiable state, every ledger change must be traceable to an authorization.
**Fix**: Make provenance required on `JournalEntry`. Add a `ProvenanceRef` enum:
`{ Governance(receipt_id), DirectMember(signature), SystemGenerated(reason) }`.

### Gap 2: No First-Class Obligation Type (Phase A)
The implicit model is: journal entries encode net position changes, obligations are inferred.
The `AssetType::Claim` variant in `icn-kernel-api` already defines the shape.
**Fix**: Implement `Obligation { id, creditor, debtor, unit, amount, authorizing_receipt, due_by, state: ObligationState }` using `AssetType::Claim` as the kernel primitive.

### Gap 3: No PatronageTracker (Phase A)
NY Corp Law §93 requires capital accounts based on patronage (contribution), not capital.
The CCL schema has `patronage_refund` as an allocation target but nothing accumulates patronage.
**Fix**: `PatronageTracker` that accumulates per-member contribution epochs, queried by capital account calculations.

### Gap 4: Commons Formula in Kernel (Phase A)
`commons_credits.rs` has the credit formula (`cpu_millis + memory_mb_millis/1000 + ...`) hardcoded
in the kernel layer. The existing `// TODO(governance)` comment says this should be a CCL parameter.
**Fix**: Extract to a `CommonsResourcePolicy` CCL parameter, evaluated by a PolicyOracle.

### Gap 5: Delegation State Is In-Memory (Phase B)
`DelegationManager` is not persisted and not gossip-synced. Liquid democracy breaks on node restart.
**Fix**: Persist to `icn-store`, add gossip sync for delegation events, add query API.

### Gap 6: No Execution Receipt Enforcement (Phase B)
Accepted governance proposals don't automatically link to the economic events they authorize.
**Fix**: Add `ExecutionReceiptGate` middleware — allocations blocked until linked to a valid
`GovernanceDecisionReceipt`. Track "open" vs "closed" governance decisions.

### Gap 7: No Participatory Budgeting (Phase B)
`ProposalPayload::Budget` is a single-item allocation. No allocation-across-competing-proposals.
**Fix**: `AllocationProposal` type with `BudgetPool` and `AllocationOptions`, processed through
the existing tally engine with ranked-choice or conviction semantics.

---

## Implementation Phases

### Phase 0 — Terminology and Surface Positioning (1-2 weeks)
*Minimum needed for pilot/funding applications.*
- Rename `CreatePaymentRequest` → `RecordTransferRequest` in gateway models
- Rename `payment` API route to `settlement`
- Rename `balance` response fields to `position` in gateway API
- Change `commons-credits` constant to `commons-capacity`
- Add UX language rules to `docs/dev/language-guide.md`
- Extract `docs/design/regulatory-safe-verifiable-state.md` (this document)
- Add code review checklist to `CONTRIBUTING.md`

### Phase A — Structural Attestation Model (4-6 weeks)
- Make `JournalEntry` provenance required (`ProvenanceRef`)
- Implement `Obligation` type using `AssetType::Claim`
- Add `PatronageTracker` for NY-CC §93
- Extract commons credit formula to CCL PolicyOracle parameter
- Wire `Obligation` into settlement paths (alongside existing journal entries)

### Phase B — Governance Completeness (2-3 months)
- Persist `DelegationManager` with gossip sync
- Add delegation query API (`/v1/governance/{id}/delegations`)
- Implement `ExecutionReceiptGate` (enforce governance → execution linkage)
- Add `AllocationProposal` (participatory budgeting)
- Add `DisputeResolution` → receipt/obligation lifecycle linkage

### Phase C — CI Guardrails (alongside Phase A/B)
- Add `cargo-deny` or custom clippy lint forbidding custodial patterns in public API types
- Add forbidden-terminology check to CI for gateway API surface
- Add `docs/dev/review-checklists.md` with the seven invariants as a PR checklist item

---

## Compliance Posture Language

Use this framing in all external-facing documentation and grant applications:

**Use**: "reduces risk by avoiding trigger behaviors"
**Not**: "is legally compliant" or "is legally bulletproof"

Compliance posture is a *behavioral invariant*, not a label. The architecture enforces non-intermediating
behaviors; legal interpretation of those behaviors is context-dependent and must be verified by counsel.

---

## Connection to the IPv6 Work

The same separation-of-concerns principle drives both major architectural threads:

| Change | Separation |
|--------|-----------|
| IPv6 endpoint sets | Separate *identity* (DID) from *transport* (address) |
| Verifiable state | Separate *coordination* (obligations/receipts) from *money* (payment semantics) |

Both changes make ICN more honest about what it actually is: a substrate for cryptographically
verifiable peer coordination — not a financial network, and not a single-transport system.

---

## Files Requiring Changes

| File | Change | Phase |
|------|--------|-------|
| `icn-gateway/src/models.rs` | Rename `CreatePaymentRequest`, `BalanceResponse` | 0 |
| `icn-gateway/src/api/ledger.rs` | Rename `payment` route to `settlement`/`record` | 0 |
| `icn-ledger/src/commons_credits.rs` | Extract formula to CCL parameter | A |
| `icn-ledger/src/types.rs` | Make provenance required; add `ProvenanceRef` | A |
| `icn-kernel-api/src/economics.rs` | Implement `Obligation` using `AssetType::Claim` | A |
| `icn-ledger/src/settlement.rs` | Wire `Obligation` into settlement paths | A |
| `icn-coop/src/membership.rs` | Add `PatronageTracker` | A |
| `icn-governance/src/delegation.rs` | Add persistence + gossip sync | B |
| `icn-governance/src/amendment.rs` | Add `ExecutionReceiptGate` | B |
| `icn-governance/src/proposal.rs` | Add `AllocationProposal` type | B |
| `CONTRIBUTING.md` | Add regulatory invariants checklist | C |
| `docs/dev/language-guide.md` | UX language rules (new file) | 0 |
| `docs/dev/review-checklists.md` | PR review checklist (new file) | C |
