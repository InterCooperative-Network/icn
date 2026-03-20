# ICN Compliance Architecture

**How ICN stays regulatory-safe by design, not by disclaimer.**

---

## The Regulatory Context

Any system that handles economic coordination between organizations triggers regulatory scrutiny. Payment systems, money transmission, securities, lending. The cooperative economy needs economic coordination. ICN provides it without becoming a regulated financial technology.

This is not a legal argument. It is an architectural fact.

## The Meaning Firewall

ICN's core architectural principle is a strict separation between domain semantics and constraint enforcement:

```
┌─────────────────────────────────────────────────┐
│                  APPLICATIONS                     │
│  Governance  │  Mutual Credit  │  Membership      │
│  "votes"     │  "obligations"  │  "members"       │
│  "proposals" │  "settlements"  │  "cooperatives"  │
├─────────────── MEANING FIREWALL ─────────────────┤
│                    KERNEL                          │
│  "state transitions"  │  "constraints"             │
│  "signed envelopes"   │  "capability tokens"       │
│  "hash chains"        │  "rate limits"             │
└─────────────────────────────────────────────────┘
```

**Above the firewall:** Applications understand what a "vote" means, what an "obligation" is, what a "cooperative" does. They translate these concepts into generic constraints.

**Below the firewall:** The kernel enforces constraints mechanically. It processes signed state transitions with quorum thresholds, position constraints, and capability checks. It does not know what any of it means.

## Why This Matters for Regulation

1. **No payment processing.** The kernel does not process payments. It enforces position constraints on state transitions. Applications interpret position changes as economic activity (mutual credit, patronage distribution, resource sharing). The kernel sees numbers.

2. **No currency issuance.** ICN does not create, issue, or manage currency. Cooperatives track mutual obligations using unit-denominated positions. The units are defined by governance policy, not by the system. "Hours," "patronage credits," and "commons capacity" are application-layer concepts.

3. **No money transmission.** ICN does not move money between parties. It records signed state transitions that applications interpret as settlements of mutual obligations. The actual movement of value happens through existing cooperative financial channels (credit unions, cooperative banks).

4. **No securities.** Cooperative membership shares are not securities under most US state cooperative statutes (see NY Cooperative Corporations Law Article 5-A, Section 81). ICN tracks membership and governance participation, not investment returns.

## Terminology Discipline

ICN enforces regulatory-safe terminology through automated CI checks:

| Forbidden Term | Required Alternative | Why |
|---------------|---------------------|-----|
| payment | settlement | Settlements of mutual obligations, not payments |
| currency | unit | Units of account, not currencies |
| balance | position | Net position in obligation graph, not account balance |
| transaction | state transition | Generic state change, not financial transaction |
| ledger | state change journal | Journal of state transitions, not financial ledger |
| wallet | keystore | Cryptographic key storage, not money storage |
| token (monetary) | capability | Authorization tokens, not value tokens |

A **regulatory compliance linter** runs in CI (PR #1349) and flags any new introduction of forbidden terms in the API surface.

## Enforcement Mechanisms

| Mechanism | What It Checks | Status |
|-----------|---------------|--------|
| Meaning Firewall CI gate | No domain imports in kernel crates | Active, blocking |
| Kernel Forbidden Dependencies | No `icn-trust`, `icn-governance` in kernel | Active, blocking |
| Firewall Contract Enforcement | No semantic types cross the boundary | Active, blocking |
| Regulatory Compliance Linter | Forbidden financial terms in API surface | Active, warning (ratcheting to blocking) |
| Seven Invariants checklist | PR review checklist in CONTRIBUTING.md | Active, manual |

## Evidence

- **PR #1349:** Compliance linter CI job — catalogs 33 pre-existing violations, prevents new ones
- **PR #1348:** CommonsResourcePolicy — extracted formula from kernel to governance-configurable policy
- **PR #1351:** Vertical slice test — proves the complete receipt chain from governance to audit
- **PR #1354:** `icnctl audit verify` — CLI command for machine-verifiable receipt chain integrity
- **Epic #1302:** All 10 sub-issues closed with verified evidence (8 success criteria met)

## What This Means for Funders

ICN is **digital public infrastructure** for democratic coordination. It is architecturally equivalent to TCP/IP, not to Venmo. A grant funding ICN is funding infrastructure development, not financial technology development.

The regulatory-safe architecture is not a surface-level framing exercise. It is enforced by automated CI gates that reject code violating the boundary. Every PR to ICN is checked against these constraints before it can merge.
