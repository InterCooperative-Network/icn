---
Status: normative
Canonical: yes
Last Reviewed: 2026-04-10
---

# Constitutional Genesis

This document defines the immutable substrate of the Intercooperative Network: what cannot be changed by governance, what can, and how the system bootstraps from nothing into a governed cooperative.

## Foundation: The Constraint Engine

ICN is a constraint engine. Applications translate meaning into constraints; the kernel enforces constraints without understanding meaning. This separation is the architectural foundation of everything that follows.

The kernel sees `ConstraintSet` values: rate limits, topic caps, message size bounds, and opaque custom fields. It never sees trust scores, membership tiers, or governance roles. That boundary — the **meaning firewall** — is itself an immutable invariant.

## Hard Invariants

Hard invariants are substrate mechanics. Governance cannot remove them. They define what ICN *is* rather than how it is configured.

### Frozen-Core Invariants

The kernel recognizes 10 frozen-core invariants, organized into three domains. These are defined in `icn-kernel-api/src/invariants.rs` as `InvariantId`. The kernel verifies attestation presence, format, and hash linkage for these invariants — it never evaluates the predicates themselves. Predicate evaluation lives in the governance app layer (`InvariantGate`).

#### Sovereignty Core (Consent and Exit)

| # | Invariant | Meaning |
|---|-----------|---------|
| 1 | **Right of Exit** | No rule can prevent an identity from leaving an entity |
| 2 | **No Retroactive Obligations** | Rules only apply to actions after their activation block |
| 3 | **Explicit Cryptographic Entry** | Adding an identity to an entity requires their cryptographic signature |

#### Anti-Capture Core (Power and Governance)

| # | Invariant | Meaning |
|---|-----------|---------|
| 4 | **No Silent Power Changes** | Authorization changes must include a computable capability delta |
| 5 | **Bounded Authority** | No wildcard capabilities across all resources |
| 6 | **Mandatory Execution Delay** | Economic and capability changes require a timelock delay |
| 7 | **Capability Conservation** | Cannot delegate capabilities you do not possess |

#### Economic Core

| # | Invariant | Meaning |
|---|-----------|---------|
| 8 | **Mutual Credit Conservation** | Credit loops must net to zero |
| 9 | **Sovereign Signature Requirement** | Funds move only via owner signature or pre-consented dispute hook |
| 10 | **Universal Receipt Generation** | Every state transition emits an immutable receipt |

### Enforcement Mechanisms (Hard Invariants)

Beyond the frozen-core governance invariants, the substrate enforces these structural mechanics. Governance can change the *values* fed into these mechanisms but cannot remove the mechanisms themselves.

| Mechanism | Enforced In | What It Does |
|-----------|------------|--------------|
| **Zero-sum ledger** | `icn-ledger/src/ledger.rs` (`validate_entry`) | Every credit creates a matching debit. Double-entry balance check per currency. |
| **Deterministic compute** | `icn-kernel-api/src/compute.rs` (`DeterminismClass::Canonical`) | Canonical execution mode guarantees same inputs produce same outputs. |
| **Kernel/app separation** | `icn-core/src/meaning_firewall.rs` + CI denylist | Kernel crates never import domain types. Validated by CI import regression tests. |
| **Members control keys** | `icn-identity/src/bundle.rs`, `keystore.rs` | Private keys live in age-encrypted keystore. No custodial escrow. Export requires passphrase. |
| **Transport authentication** | `icn-net/src/envelope.rs` (`SignedEnvelope::verify`) | Ed25519 signature verification on all network messages. Hybrid ML-DSA verification available via `verify_with_pq_key()` when PQ public key is present. |
| **Replay guard** | `icn-net/src/replay_guard.rs` + network handlers | Monotonic sequence numbers and timestamp age checks prevent message replay. Wired into the network actor via signed message handlers. |
| **Rate limiter** | `icn-net/src/rate_limit.rs` (token bucket) | Per-peer token bucket rate limiting. Mechanism exists; values come from PolicyOracle. |
| **Capability gate** | `icn-kernel-api/src/bootstrap.rs` (`Capability`, `CapabilitySet`) | Capabilities are bearer tokens with delegation rules checked via `CapabilitySet::can_delegate()`. |
| **Credit gate** | `icn-ledger/src/credit_policy.rs` | Dynamic credit limits checked against account balance, trust score, and cleared volume. |

### The Critical Distinction

| | Governance CAN | Governance CANNOT |
|---|---|---|
| Rate limiting | Change the rate (20/s to 100/s) | Remove rate limiting entirely |
| Credit gating | Change the ceiling (1000 to 5000) | Remove credit gating entirely |
| Capability system | Change who gets what grants | Remove capability checking entirely |
| Zero-sum ledger | (not applicable) | Violate double-entry balance |
| Deterministic compute | (not applicable) | Make canonical execution non-deterministic |
| Kernel/app separation | (not applicable) | Allow domain imports in kernel crates |

If governance could remove enforcement mechanisms, there would be no stable substrate.

## Meta-Invariants

Meta-invariants are constraint *values* that governance can change through the PolicyOracle pattern. Apps translate domain semantics into `ConstraintSet` fields; the kernel enforces those fields blindly.

| Meta-Invariant | Source Oracle | What It Controls |
|---------------|-------------|-----------------|
| **Rate limit values** | `TrustPolicyOracle` (`apps/trust/src/oracle.rs`) | Requests per second per trust class (10/20/100/200) |
| **Credit ceiling values** | `LedgerPolicyOracle` (`apps/ledger/src/oracle.rs`) | Maximum credit extended per account |
| **Capability grants** | `CapabilityGate` (`icn-kernel-api/src/bootstrap.rs`) | Which actors hold which capabilities |
| **Trust class thresholds** | `TrustPolicyOracle` | Score boundaries for Isolated/Known/Partner/Federated (0.0/0.1/0.4/0.7) |
| **Voting methods** | `GovernancePolicyOracle` (`apps/governance/src/oracle.rs`) | Simple majority vs supermajority, quorum requirements |
| **Treasury policy** | Governance + Treasury actor | Surplus distribution rules, demurrage schedules |

### The Meaning Firewall Boundary

Each PolicyOracle converts domain-specific knowledge into generic constraints:

```
TrustPolicyOracle:
  trust_score(0.85) → ConstraintSet { rate_limit: 200/s, custom: { credit_multiplier: 0.85 } }

GovernancePolicyOracle:
  protocol_params → ConstraintSet { rate_limit: N, custom: { resource_quota: M } }

LedgerPolicyOracle:
  account_state → ConstraintSet { custom: { credit_ceiling: K } }
```

The kernel receives `ConstraintSet`. It enforces rate limits, topic caps, and opaque custom fields. It never knows these values came from trust scores or governance parameters.

## Bootstrap Sequence

A new ICN node starts from nothing and transitions through three phases before reaching normal operation.

### Phase 1: Genesis

The daemon loads configuration, unlocks the keystore, and enters the Genesis bootstrap phase.

**What happens:**
1. Configuration loaded from `icn.toml`
2. Identity bundle loaded or generated from age-encrypted keystore
3. `AllowAllOracle` is active — all operations are permitted
4. Genesis capabilities can be issued (time-limited, phase-guarded)
5. Trust service initializes with empty trust graph
6. Ledger initializes with empty state
7. Governance service initializes with default protocol parameters

**What this means:** During Genesis, the node has no trust relationships, no credit history, and no governance structure. The `AllowAllOracle` permits bootstrap operations that would otherwise be denied.

### Phase 2: CoreApps

First-party applications load and register their PolicyOracles.

**What happens:**
1. Trust oracle registers with the PolicyOracle registry
2. Governance oracle registers
3. Ledger oracle registers
4. Genesis capabilities remain valid but will expire
5. Missing oracles still default to allow (bootstrap flexibility)

**What this means:** The system is transitioning from permissive to enforced. Apps are loading their domain-specific authorization logic.

### Phase 3: Running

Normal operation begins. The system transitions to deny-by-default for unknown domains.

**What happens:**
1. Genesis capabilities expire
2. `AllowAllOracle` is replaced by registered domain oracles
3. Unknown domains receive `PolicyDecision::Deny` (not allow)
4. All frozen-core invariants are actively enforced
5. Gossip synchronization begins
6. Network actors accept peer connections

**What this means:** The cooperative is live. Every operation is authorized by a PolicyOracle or denied. The substrate enforces constraints without understanding their semantic origin.

### Genesis Identity

The first identity is created during node initialization:

1. `icnctl id init` generates an Ed25519 keypair
2. DID is derived: `did:icn:<base58-pubkey>`
3. Private key is stored in age-encrypted keystore at `~/.icn/keystore.age`
4. X25519 key is derived for TLS binding
5. The identity can be exported at any time via `icnctl id export`

### Genesis Trust

Initial trust is seeded explicitly. There is no implicit trust:

1. First node starts with an empty trust graph
2. Trust edges are added via explicit peer connections
3. Trust scores are computed transitively from direct edges
4. The genesis trust configuration is auditable and replaceable

### Genesis Treasury

The initial credit pool is created through the ledger:

1. Ledger starts with no accounts and no balances
2. Genesis credit issuance follows double-entry rules (credit creates matching debit)
3. The zero-sum invariant holds from the first transaction
4. Treasury policy is a meta-invariant — governance can change distribution rules

## Exit and Fork Rights

ICN guarantees data sovereignty. Members can always leave and take their data.

### What Members Can Export

| Data | Mechanism | Command |
|------|-----------|---------|
| Identity (keypair + DID) | Age-encrypted bundle | `icnctl id export` |
| Full node state | Archive backup | `icnctl backup` |
| Ledger entries authored | Cryptographically signed DAG entries | Ledger export (entries are self-authenticating) |
| Trust edges issued | Signed trust attestations | Trust graph export |

### What Cannot Be Taken

- Trust edges *received from others* (those belong to the issuer)
- Governance votes cast by other members
- Credit balances owed by others (the obligation is bilateral)

### Fork Rights

Because identity is self-sovereign (DID derived from public key, private key in member's keystore), a member can:

1. Export their identity
2. Export their authored ledger entries
3. Start a new node with the same identity
4. Establish new trust relationships independently

There is no administrator who can revoke a DID or freeze an account. The Sovereign Signature Requirement (invariant #9) guarantees that funds move only via owner signature or pre-consented dispute hooks.

## Invariant Enforcement Table

| Invariant | Hard/Meta | Domain | Where Enforced | How Tested | How Observed |
|-----------|-----------|--------|----------------|------------|--------------|
| Right of Exit | Hard | Sovereignty | `icn-kernel-api/src/invariants.rs` (attestation format); governance app (predicate) | Governance invariant attestation | Invariant report in governance decisions |
| No Retroactive Obligations | Hard | Sovereignty | `icn-kernel-api/src/invariants.rs` (attestation format); governance app (predicate) | Block height checks | Invariant report |
| Explicit Cryptographic Entry | Hard | Sovereignty | `icn-kernel-api/src/invariants.rs` (attestation format); governance app (predicate) | Signature verification | Invariant report |
| No Silent Power Changes | Hard | Anti-Capture | `icn-kernel-api/src/invariants.rs` | Capability delta checks | Invariant report |
| Bounded Authority | Hard | Anti-Capture | `icn-kernel-api/src/invariants.rs` | Wildcard rejection | Invariant report |
| Mandatory Execution Delay | Hard | Anti-Capture | `icn-kernel-api/src/invariants.rs` | Timelock verification | Invariant report |
| Capability Conservation | Hard | Anti-Capture | `icn-kernel-api/src/bootstrap.rs` | `test_capability_set_delegation` | Delegation metrics |
| Mutual Credit Conservation | Hard | Economic | `icn-ledger/src/ledger.rs` | Balance validation tests | `icn_ledger_transactions_total`, `icn_ledger_entries_quarantined_total` |
| Sovereign Signature Requirement | Hard | Economic | `icn-ledger/src/ledger.rs` | Entry signature checks | Signature failure counters |
| Universal Receipt Generation | Hard | Economic | `icn-kernel-api/src/proofs.rs` | Receipt generation tests | Receipt store metrics |
| Zero-sum ledger | Hard | Mechanism | `icn-ledger/src/ledger.rs` | `ledger_persistence.rs` | Arithmetic overflow counters |
| Deterministic compute | Hard | Mechanism | `icn-kernel-api/src/compute.rs` | `compute_integration.rs` | Canonical execution metrics |
| Kernel/app separation | Hard | Mechanism | `icn-core/src/meaning_firewall.rs` | `meaning_firewall.rs` (CI) | Import count regression |
| Members control keys | Hard | Mechanism | `icn-identity/src/bundle.rs` | `backup_restore_test.rs` | Export requires passphrase |
| Transport authentication | Hard | Mechanism | `icn-net/src/envelope.rs` | Handler integration tests | Signature failure logs |
| Replay guard | Hard | Mechanism | `icn-net/src/replay_guard.rs` + handlers | Rate limiting integration tests | Replay rejection counters |
| Rate limiter (mechanism) | Hard | Mechanism | `icn-net/src/rate_limit.rs` | `trust_gated_rate_limiting_integration.rs` | Token bucket metrics |
| Capability gate (mechanism) | Hard | Mechanism | `icn-kernel-api/src/bootstrap.rs` | Capability delegation tests | Capability metrics |
| Credit gate (mechanism) | Hard | Mechanism | `icn-ledger/src/credit_policy.rs` | Credit limit tests | Credit violation counters |
| Rate limit values | Meta | Value | `apps/trust/src/oracle.rs` | Oracle evaluation tests | `rate_limit_current` |
| Credit ceiling values | Meta | Value | `apps/ledger/src/oracle.rs` | Credit policy tests | `credit_ceiling_current` |
| Trust class thresholds | Meta | Value | `apps/trust/src/oracle.rs` | Trust class tests | Trust class distribution |
| Capability grants | Meta | Value | Bootstrap `CapabilitySet` | Installation tests | Capability registry |
| Voting methods | Meta | Value | `apps/governance/src/oracle.rs` | Governance parameter tests | Protocol parameter store |
| Treasury policy | Meta | Value | Governance + Treasury | Treasury distribution tests | Treasury metrics |

## Governance Decision Flow

When governance changes a meta-invariant:

1. **Proposal created** — governance app receives a proposal to change parameter values
2. **Invariant check** — an `InvariantReport` is generated, attesting that all 10 frozen-core invariants are preserved by the proposed change
3. **Voting** — members vote according to the current voting method (itself a meta-invariant)
4. **Execution** — the governance decision flows through the effect dispatcher to the `ProtocolParameterStore`
5. **Observable change** — PolicyOracles re-evaluate with new parameter values; constraint values change dynamically

The kernel participates in steps 2 (verifying attestation format and hash linkage) and 5 (enforcing the new constraint values). It never evaluates the predicates themselves.

## Summary

ICN's constitutional foundation rests on a clear separation:

- **Hard invariants** define what the substrate *is*. They cannot be removed by governance. They are enforced by code, validated by tests, and observed by metrics.
- **Meta-invariants** define how the substrate is *configured*. They are set by PolicyOracles, changed by governance decisions, and enforced blindly by the kernel.
- **The meaning firewall** separates the two. Domain semantics stay in apps. The kernel sees only constraints.

A cooperative built on ICN inherits these guarantees: members can always exit, credit always balances, power changes are always visible, and the substrate cannot be captured by changing its configuration.
