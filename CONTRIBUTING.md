# Contributing to ICN

## Architectural Guardrails

### 1. Interfaces First

Before extracting domain logic:

1.  Introduce trait boundaries.
2.  Land a minimal abstraction PR.
3.  *Then* move the implementation.

**Guideline**: No extraction PR should introduce new interfaces and move 40 files at once.

### 2. No Drive-By Refactors

Extraction PRs must:
*   Move code
*   Adjust wiring
*   Fix compilation
*   Add minimal tests

They do **NOT**:
*   Rename unrelated types
*   Redesign logic
*   Improve style

**Principle**: Stability > Elegance.

### 3. The Meaning Firewall

*   **Kernel enforces**: Identity primitives, trust mechanisms, state primitives, auditability, resource accounting.
*   **Apps define**: Governance, membership semantics, economics, federation agreements.

**Goal**: No domain language in the kernel.

---

## Regulatory Architecture Invariants — PR Checklist

ICN is communications infrastructure, not a financial intermediary. Every PR that touches gateway
APIs, SDK types, ledger logic, or UI text must be checked against these seven invariants before
merge. Answer each question. If any answer is **Yes**, the PR needs architectural review before
landing.

**Full rationale**: [`docs/design/regulatory-safe-verifiable-state.md`](docs/design/regulatory-safe-verifiable-state.md)
**Terminology rules**: [`docs/dev/language-guide.md`](docs/dev/language-guide.md)

---

### Invariant 1 — User-Signed Transitions Only

> Does this PR add any endpoint or code path that **initiates a state transition on a user's
> behalf** without a user-provided signature over the specific transition?

- [ ] No new gateway endpoint originates obligation changes without the user's signing key
- [ ] No scheduler or background job creates ledger entries without an authorizing signature
- [ ] Recurring settlement requests contain the original user authorization reference

---

### Invariant 2 — No Hosted Balances

> Does this PR introduce any concept of a **balance stored under operator control**, where the
> operator could move that balance without the account owner's signature?

- [ ] No new "hosted account" type where the node holds signing authority
- [ ] No endpoint `POST /move?from=A&to=B&amount=X` authorized by operator credentials alone
- [ ] Position/balance fields are clearly derived views, not authoritative stored values

---

### Invariant 3 — No Operator Routing of Value

> Does this PR add any code where the **gateway routes a transfer** (rather than broadcasting a
> user-signed entry)?

- [ ] Gateway endpoints are read/write proxies for user-signed entries — not transfer executors
- [ ] No new "send funds from A to B" semantics initiated by gateway logic
- [ ] The gateway does not hold signing keys for member accounts

---

### Invariant 4 — Derived Views Are Not Protocol Primitives

> Does this PR treat a **position/balance view as a writable protocol primitive** rather than a
> derived read-only interpretation of the signed-entry graph?

- [ ] Balance/position endpoints accept reads only; writes go through the signed entry path
- [ ] No new `set_balance()` or `adjust_position()` semantics that bypass the journal entry path
- [ ] `JournalEntry` (and its future `Obligation` successor) remains the write primitive

---

### Invariant 5 — No Embedded Convertibility

> Does this PR add any **`exchange_rate`, `fiat_equivalent`, `redeem`, or convertibility** field,
> endpoint, or semantic?

- [ ] No `exchange_rate` field in any gateway API, SDK type, or protocol message
- [ ] No `redeem()` endpoint or "redeem for cash/fiat" UX flow
- [ ] No unit defined as "pegged to" or "backed by" an external asset
- [ ] FX oracle types (`CurrencyPair`, `ExchangeRate`) stay internal — not exposed in gateway or SDK

---

### Invariant 6 — Matching and Market Features Are Opt-In, Scoped, and Governance-Authorized

> Does this PR add any **operator-side matching, clearing, or marketplace** feature that runs
> outside member governance?

- [ ] Service discovery endpoints return data; they do not execute matches
- [ ] Any "allocation" or "offer matching" feature is scoped to a specific org/federation
- [ ] New marketplace-like features require an authorizing governance proposal, not operator config
- [ ] No global order book or clearing function operated by the ICN node itself

---

### Invariant 7 — Execution Receipts Close the Loop

> Does this PR add governance actions or resource allocations that **do not produce a linked
> `ExecutionReceipt`**?

- [ ] Accepted proposals that involve resource allocation generate a linked `ExecutionReceipt`
- [ ] New ledger entries resulting from governance decisions reference `decision_receipt_id`
- [ ] No "allocation approved but unverifiable" state — the receipt is the proof

---

### Language Check

> Does this PR introduce any forbidden terms in public API, SDK types, or UI text?

- [ ] No `payment` in endpoint paths, request types, or event names (use `settlement`)
- [ ] No `currency` in request/response fields (use `unit`)
- [ ] No `balance` in response fields or endpoint paths (use `position`)
- [ ] No `wallet` in user-facing concepts (use `account`)
- [ ] No `send funds` / `redeem` / `exchange rate` in UI or API

See [`docs/dev/language-guide.md`](docs/dev/language-guide.md) for the full list.

---

## Issue Labels

Every issue gets **exactly one `epic:*` + exactly one `type:*`**. Trust issues also get one `tier:*`. No exceptions.

See **[.github/ISSUE_POLICY.md](.github/ISSUE_POLICY.md)** for the full label system, triage rules, and agent behavior contract.
