---
Status: normative
Canonical: yes
Last Reviewed: 2026-05-17
---

# ICN Design Principles

This is the canonical index of ICN's design principles. The principles already exist — they are
encoded in code (`icn-kernel-api/src/invariants.rs`), enforced in CI
(`.github/scripts/firewall_denylist.py`), surfaced at session start
(`adversarial-by-default | determinism | canonical-encodings | no-panics | kernel/app-boundaries`),
and explained at length in adjacent documents. This document consolidates them into a single
three-tier hierarchy and points to the authoritative source for each.

> **The frame:** ICN is a P2P **constraint engine**. Apps translate meaning into constraints; the
> kernel enforces constraints without understanding meaning. Every principle below either
> protects that frame, makes it verifiable, or makes it survivable.

---

## The three tiers

| Tier | Concern | Count | Authority |
|------|---------|-------|-----------|
| **1. Operational invariants** | Kernel quality — what the daemon must always do | 5 | [Section 1](#1-operational-invariants-tier-1) |
| **2. Firewall contract** | Kernel/app boundary — what the kernel must never know | 6 | [`docs/architecture/KERNEL_APP_SEPARATION.md`](architecture/KERNEL_APP_SEPARATION.md) §Firewall Contract |
| **3. Frozen-core invariants** | Sovereignty / anti-capture / economic safety | 10 | [`icn/crates/icn-kernel-api/src/invariants.rs`](../icn/crates/icn-kernel-api/src/invariants.rs) |

Tier 1 governs *how* the kernel runs. Tier 2 governs *what* the kernel is allowed to know. Tier 3
governs *what governance and ledger apps are allowed to do to people*. Tier 1 violations are bugs.
Tier 2 violations are architectural drift. Tier 3 violations are constitutional failures.

---

## 1. Operational invariants (tier 1)

These are the five invariants printed at every session start. They apply to all protocol-path code.

### 1.1 Adversarial-by-default

**Definition.** Every peer is untrusted until trust is established through verifiable participation.
No code path may assume good faith from a remote DID.

**Forbids.** Accepting unsigned messages on protocol paths. Skipping replay-guard checks. Granting
capabilities by network position. Trusting transport-layer identity without DID-TLS binding. Using
default-trust shortcuts in non-test code.

**Enforced by.** `SignedEnvelope` + `ReplayGuard` in `icn-net/src/envelope.rs`. DID-TLS binding in
`icn-net` session setup. Trust-gated rate limiting (Tier 2 → `PolicyOracle` → `RateLimit`).
Three-layer security model (Transport / Message / Application — see CLAUDE.md §Security).

**Violation pattern.**
```rust
// ❌ Accepting payload before envelope verification
async fn handle(msg: NetworkMessage) {
    self.process(msg.payload).await;        // skipped SignedEnvelope check
}
```

---

### 1.2 Determinism

**Definition.** Same inputs produce same outputs. Two nodes processing the same log arrive at the
same state. No floating point in consensus or hash inputs. RNG is seeded. Serialization is
canonical.

**Forbids.** `HashMap` iteration order in any hashed/signed output. `SystemTime::now()` inside
deterministic code paths (use logical clocks). `f64` arithmetic in ledger or consensus paths.
Unseeded `rand::thread_rng()` in protocol code. Non-deterministic CBOR/JSON encoders.

**Enforced by.** `icn-encoding` canonical serialization. `BTreeMap` over `HashMap` in serialized
types. Frozen `report_version` on `InvariantReport`. CCL fuel metering + sandbox.

**Violation pattern.**
```rust
// ❌ HashMap iteration order leaks into hashed output
let map: HashMap<Did, u64> = ...;
let bytes = serde_json::to_vec(&map)?;
let hash = blake3::hash(&bytes);          // hash depends on insertion order
```

---

### 1.3 Canonical encodings

**Definition.** One valid byte representation per value. No ambiguity in wire format, proof
format, or storage format.

**Forbids.** Indefinite-length CBOR. Unsorted map keys. Multiple equivalent integer encodings.
Floating-point in canonical contexts. Per-implementation field ordering for signed objects.

**Enforced by.** [`docs/architecture/CANONICAL_ENCODING.md`](architecture/CANONICAL_ENCODING.md)
(normative spec). CBOR RFC 8949 with definite-length encoding, shortest-form integers, bytewise-sorted
map keys. `icn-encoding` provides the canonical codecs.

**Violation pattern.**
```rust
// ❌ HashMap → CBOR — keys not deterministically ordered
let h: HashMap<&str, i64> = [("b", 1), ("a", 2)].into();
serde_cbor::to_vec(&h)?;                  // key order undefined → hash drifts
```

---

### 1.4 No panics in protocol paths

**Definition.** The daemon does not crash on bad input. Protocol/network/actor/deserialization
paths return `Result<T, E>`. Failures are handled, logged, metricized, and degraded against —
never propagated as a panic.

**Forbids.** `.unwrap()` / `.expect()` outside `#[cfg(test)]`. `panic!()` in message handlers.
`array[i]` indexing without bounds check on attacker-controlled input. `assert!()` on remote data.
`todo!()` / `unimplemented!()` in shipped code.

**Enforced by.** `.claude/rules/rust-core.md` §Error Handling. `thiserror` for crate-local errors,
`anyhow` at binary boundaries, `ErrCode` from `icn-kernel-api` for protocol-level rejections.
Clippy lints (`-D warnings` in CI).

**Violation pattern.**
```rust
// ❌ Attacker-controlled deserialization can panic the actor
let parsed: GossipMessage = serde_cbor::from_slice(&bytes).unwrap();
```

---

### 1.5 Kernel/app boundary is inviolable

**Definition.** The dependency graph enforces it. Kernel crates may not import domain crates.
The Meaning Firewall is structural, not aspirational.

**Forbids.** Importing `icn-trust`, `icn-governance`, `icn-ccl`, `icn-coop`, `icn-community`,
`icn-ledger` (internals), or `apps/*` from any kernel crate. Reconstructing domain semantics inside
the kernel from `ConstraintSet.custom` keys. Bridge/adapter layers that quietly carry domain
meaning across the firewall.

**Enforced by.** [`docs/architecture/KERNEL_APP_SEPARATION.md`](architecture/KERNEL_APP_SEPARATION.md)
(full contract). `.github/scripts/firewall_denylist.py` (CI enforcement, currently advisory during
waves 2–6 migration). `.claude/rules/kernel-boundary.md` (agent guardrails).

**This invariant is the gateway to tier 2.** Tier 2 is the same rule, decomposed into six concrete
mechanical sub-rules a reviewer can check on a PR.

---

## 2. Firewall contract (tier 2)

The six invariants that operationalize 1.5. Defined and exhaustively exampled in
[`docs/architecture/KERNEL_APP_SEPARATION.md`](architecture/KERNEL_APP_SEPARATION.md)
§Firewall Contract.

| # | Invariant | One-line rule |
|---|-----------|---------------|
| 1 | **No kernel→domain imports** | Kernel crates' Cargo deps must not include domain crates |
| 2 | **Pure data only at kernel entrypoints** | Kernel APIs accept `ConstraintSet` / `Did` / `Hash`, not `&TrustGraph` |
| 3 | **Semantic lookup only in `PolicyOracle`** | Trust/role/membership decisions happen in app oracles, not kernel code |
| 4 | **No pattern-matching on `ConstraintSet.custom`** | Kernel reads typed fields; `custom` is an app-to-app channel |
| 5 | **Kernel owns mechanism, app owns value** | Kernel ships the rate limiter; app decides 200/sec vs 20/sec |
| 6 | **Opaque receipt storage** | Kernel persists typed receipts as `(class, record_hash) → bytes` and never parses the body |

**Status.** Wave 1 (contract + CI script) complete as of 2026-02-01. Waves 2–6 migrate 10 known
kernel→domain dependencies. CI gate is currently `continue-on-error: true` and flips to a hard
gate once waves complete. See KERNEL_APP_SEPARATION.md §6.0 for the wave schedule.

---

## 3. Frozen-core invariants (tier 3)

Ten invariants that protect the sovereignty of identities and the conservation of value, regardless
of which governance app is running. They live in code as `InvariantId` in
[`icn/crates/icn-kernel-api/src/invariants.rs`](../icn/crates/icn-kernel-api/src/invariants.rs) and
are evaluated by app-layer code; the kernel verifies attestation presence + hash linkage but never
re-evaluates the predicates (consistent with tier-2 invariant #6).

### 3.1 SovereigntyCore (#1–#3)

| # | Name | What it forbids |
|---|------|-----------------|
| 1 | `RightOfExit` | A rule that prevents an identity from leaving an entity |
| 2 | `NoRetroactiveObligations` | Rules applying to actions before their activation block |
| 3 | `ExplicitCryptographicEntry` | Adding an identity to an entity without their signature |

### 3.2 AntiCaptureCore (#4–#7)

| # | Name | What it forbids |
|---|------|-----------------|
| 4 | `NoSilentPowerChanges` | Authz changes that don't include a computable capability delta |
| 5 | `BoundedAuthority` | Wildcard capabilities across all resources |
| 6 | `MandatoryExecutionDelay` | Skipping the timelock on economic / capability changes |
| 7 | `CapabilityConservation` | Delegating capabilities you do not possess |

### 3.3 EconomicCore (#8–#10)

| # | Name | What it forbids |
|---|------|-----------------|
| 8 | `MutualCreditConservation` | A credit loop that does not net to zero |
| 9 | `SovereignSignatureRequirement` | Moving funds without owner signature or pre-consented dispute hook |
| 10 | `UniversalReceiptGeneration` | A state transition that emits no immutable receipt |

**Operational note.** Invariant #10 is the universal-receipt rule. It is enforced today through
the opaque receipt cascade described in tier-2 #6 (PRs #1755–#1759). It is currently classed as a
governance invariant, but every successful state transition in the system already depends on it —
see the *Audit findings* §A2 below.

---

## 4. The eight primitives

The kernel provides exactly eight mechanical primitives. Adding a ninth requires an ADR amending
[`docs/strategy/ADR-001-What-ICN-Is.md`](strategy/ADR-001-What-ICN-Is.md). Everything else is an
app.

| Primitive | Crates | What it does |
|-----------|--------|--------------|
| **Identity** | `icn-crypto`, `icn-crypto-pq` | Create, sign, verify, resolve DIDs |
| **Authorization** | `icn-authz` | Issue/check unforgeable capability tokens |
| **State** | `icn-store`, `icn-encoding`, `icn-snapshot` | Append-only logs, blobs, KV — content-addressed |
| **Compute** | (CCL runtime in `icn-ccl` interpreter layer) | Deterministic sandboxed execution with fuel metering |
| **Communication** | `icn-net`, `icn-gossip`, `icn-rpc`, `icn-gateway`, `icn-protocol`, `icn-privacy` | Pub/sub, request/response, streaming over QUIC/TLS |
| **Time** | `icn-time` | Logical clocks, scheduling, leases |
| **Coordination** | (in `icn-core` supervisor + actor patterns) | Consensus groups, CRDT merge, conflict resolution |
| **Naming** | (trait in `icn-kernel-api`; impl in `icn-naming` straddles firewall) | Hierarchical name resolution |

"Apps that ship with `icnd`" today: governance, ledger, membership, trust, federation (see
ADR-001 for the full descope table). Apps are not ICN — they are the first things built on it.

---

## 5. Meta-principles (the *why* behind the rules)

These are not invariants. They are the dispositions a contributor should adopt before writing
code that touches the kernel.

1. **Principle of Least Kernel.** When in doubt, push it into an app. The kernel acquires
   features by adding semantic knowledge, and semantic knowledge is the thing the Meaning
   Firewall exists to prevent.

2. **Mechanism in the kernel, value in the app.** "Token bucket algorithm" is a mechanism — it
   lives in `icn-net`. "200 messages per second for Federated trust" is a value — it lives in
   `apps/trust`. Mistaking value for mechanism is the most common firewall violation
   ([KERNEL_APP_SEPARATION.md §Invariant 5](architecture/KERNEL_APP_SEPARATION.md#invariant-5-kernel-contains-enforcement-mechanisms-not-policy-decisions)).

3. **Constraints, not configuration.** The kernel does not read config to decide policy. Apps
   evaluate policy and emit a `ConstraintSet`. The kernel enforces the `ConstraintSet`. If you
   reach for a config flag inside the kernel to express domain meaning, you are about to violate
   tier-2 invariant #5.

4. **Pipes, not policies.** The kernel provides pipes. Apps provide policies. This is the
   one-sentence form of the Meaning Firewall.

5. **Receipts are the unit of truth.** If a state transition matters, it emits a receipt. If it
   doesn't matter enough to emit a receipt, it doesn't belong in shared state. This is the
   operational form of tier-3 invariant #10.

---

## 6. Where each principle lives in the codebase

| Principle | Code | Doc | CI |
|-----------|------|-----|----|
| 1.1 Adversarial-by-default | `icn-net/src/envelope.rs` | KERNEL_APP_SEPARATION.md §Invariant 5 | `cargo test -p icn-net` |
| 1.2 Determinism | `icn-encoding`, frozen `report_version` | CANONICAL_ENCODING.md | `cargo test --workspace` |
| 1.3 Canonical encodings | `icn-encoding` | CANONICAL_ENCODING.md (normative) | Drift checks via `Check API Types Drift` |
| 1.4 No panics | `.claude/rules/rust-core.md` | (rule file is normative) | `cargo clippy -- -D warnings` |
| 1.5 Kernel/app boundary | `icn-kernel-api`, `kernel_surface.toml` | KERNEL_APP_SEPARATION.md | `.github/scripts/firewall_denylist.py` |
| Tier 2 (×6) | (per-invariant) | KERNEL_APP_SEPARATION.md §Firewall Contract | `firewall_denylist.py` (advisory) |
| Tier 3 (×10) | `icn-kernel-api/src/invariants.rs` | this doc + invariants.rs | `cargo test -p icn-kernel-api invariants` |
| 8 primitives | (cross-crate — see §4) | ADR-001-What-ICN-Is.md | (no automated check) |

---

## 7. Audit findings (2026-05-17)

This document was produced by auditing the corpus of existing principles. The audit surfaced
the following gaps. None of them block any current work; they are noted here so future
contributors can address them deliberately.

### A1. Quick-ref drift

[`docs/architecture/ARCHITECTURE_QUICK_REF.md`](architecture/ARCHITECTURE_QUICK_REF.md)
(snapshot dated December 2025) lists 25 crates. The workspace currently has 34 (per CLAUDE.md
§Workspace Structure). It also lists trust class ranges (`Unknown / Known / Colleague / Close /
Intimate`) that differ from the canonical thresholds in `icn-kernel-api/src/services.rs`
(`Isolated / Known / Partner / Federated`). The doc is correctly marked as a historical snapshot,
but downstream readers may quote it. **Action:** add a header pointing readers to this file plus
`KERNEL_APP_SEPARATION.md` §3.2 for current trust thresholds, or regenerate the snapshot.

### A2. Receipt universality is structurally a tier-1 invariant

Tier-3 #10 (`UniversalReceiptGeneration`) is currently classified as a governance invariant,
but every successful state transition in the system already depends on it — the entire opaque
receipt cascade (tier-2 #6, PRs #1755–#1759) exists to make it cheap. **Recommendation (not yet
applied):** promote it to a sixth tier-1 invariant ("Receipts are the unit of truth") and keep
its frozen-core slot as the governance-app-facing form. Requires consensus before edit — see
also Meta-principle §5.5.

### A3. No machine-readable principles registry

Principles are documented in prose. The `kernel_surface.toml` file referenced in ADR-001 is the
closest thing to a machine-readable principles registry, but it audits crates against the
firewall, not principles against code. **Recommendation:** if future enforcement wants to assert
*"every kernel crate has a doc-comment header citing the tier-1 invariants it depends on,"* that
will need a registry. Not needed yet.

### A4. "Adversarial-by-default" lacks a single-page operational definition

The principle is named everywhere but never decomposed into a checklist a reviewer could use on
a PR. Tier-1 §1.1 above is the first such decomposition; it should be exercised on a few real PR
reviews before being treated as load-bearing.

### A5. The CCL boundary question is answered but not surfaced in tier hierarchy

ADR-001 cleanly resolves where CCL lives (interpreter = kernel via Compute primitive; contracts
= apps). Neither tier 1 nor tier 2 has an invariant that names this boundary, so a future
contributor adding CCL features could violate it without tripping any documented rule.
**Recommendation:** add a tier-2 invariant #7, *"CCL contracts may not be re-exported as Rust
types from kernel crates,"* once CCL contract authoring becomes a regular activity.

---

## 8. UI/UX surface counterpart

The design system that renders this kernel architecture to humans lives in
[`docs/design/ICN_DESIGN_SYSTEM.md`](design/ICN_DESIGN_SYSTEM.md) (entry point) and is built from:

- [`design-language/brief-v0.md`](design-language/brief-v0.md) — the canonical visual design language
- [`design-language/concept-map.md`](design-language/concept-map.md) — canonical → public → local term mapping
- [`design-language/accessibility.md`](design-language/accessibility.md) — component-level WCAG 2.2 AA rules
- [`design/ICN_VISUAL_SYSTEM.md`](design/ICN_VISUAL_SYSTEM.md) — stable visual doctrine across surfaces
- [`design/ACCESSIBILITY_BASELINE.md`](design/ACCESSIBILITY_BASELINE.md) — per-surface accessibility floor
- [`design/CONTENT_STYLE_GUIDE.md`](design/CONTENT_STYLE_GUIDE.md) — voice, vocabulary, dangerous-action copy
- [`design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — 12-category PR-time gate
- [`mobile/icn-mobile-ux-spec-v1.md`](mobile/icn-mobile-ux-spec-v1.md) — mobile member UX spec (5-tab navigation)

ICN_DESIGN_SYSTEM.md §"Kernel architecture binding" carries the load-bearing mapping between this
document's invariants and that document's design principles + product surfaces. A change to the
firewall contract or the frozen-core invariants implies a review of that section.

## 9. Maintenance

This document is normative for the meta-principles (§5) and audit findings (§7). For the actual
invariants, the linked authoritative sources win on conflict:

| If tier 1 disagrees with… | …trust this | Reason |
|---------------------------|-------------|--------|
| Session-start preamble | session preamble | The preamble is the live runtime contract |
| KERNEL_APP_SEPARATION.md | KERNEL_APP_SEPARATION.md | It is the wave-1 firewall contract, gated by CI |
| `invariants.rs` | `invariants.rs` | Code wins for tier-3 (it is checked by tests) |
| ADR-001 | ADR-001 for scoping, this doc for kernel-runtime invariants | ADR sets descope, doesn't enumerate runtime rules |

**Last Reviewed:** 2026-05-17. Re-review on any of: change to `invariants.rs`, merge of a Wave-2+
PR, or amendment to ADR-001.
