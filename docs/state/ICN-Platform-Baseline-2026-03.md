# ICN Platform Baseline — March 2026

**Date**: 2026-03-22
**Sprint**: 23 (Baseline Lock + Narrative Surface)
**Status**: Operational

---

## What ICN Is

ICN (InterCooperative Network) is a peer-to-peer coordination substrate for cooperatives,
communities, and federations. It provides the infrastructure layer that lets groups of
people govern shared resources, compute tasks, and coordinate decisions without trusting a
central authority. The target audience is cooperative organizations: worker co-ops, housing
co-ops, federations of co-ops, and independent contributors to shared commons infrastructure.

The architecture rests on a constraint enforcement model called the **meaning firewall**.
Apps (called PolicyOracles) translate domain semantics — trust scores, governance rules,
membership tiers, credit ceilings — into generic kernel constraints. The kernel enforces
those constraints without ever understanding what they mean. A CharterPriority preemption
rule looks the same to the kernel as any other resource constraint; the ComputePolicyConfig
that encodes it is app territory. This separation is enforced structurally: the kernel-api
crate defines trait interfaces only, and a CI gate (`forbidden-deps`) blocks any attempt to
import domain logic into kernel crates.

The runtime is actor-based (Tokio). Peers communicate via message passing; no shared
mutable state crosses actor boundaries. The security model is adversarial-by-default:
a peer is untrusted until trust is established through explicit mechanisms. State transitions
are deterministic — same inputs produce same outputs, always. The Rust workspace contains
34 library crates and 4 app crates. ICN is not a blockchain and not a payment system.
The correct framing is **Digital Public Infrastructure** or **Coordination OS**.

---

## What Is Complete

### Meaning Firewall (Sprints 19-22)

The meaning firewall hardening campaign ran across four sprints and is fully complete as of
Sprint 22. All hardcoded policy constants have been extracted to typed configuration structs:

- **icn-security** (Sprint 22, PR #1389): `max_violations_per_hour` and
  `violation_retention_secs` extracted to `MisbehaviorThresholdsConfig`.
- **icn-compute** (Sprint 22, PR #1391): `CharterPriority` preemption routing weights and
  credit ceiling validation extracted to `ComputePolicyConfig`.
- **icn-ledger** (Sprint 22, PR #1392): `CreditPolicy` and `NewMemberPolicy` factory values
  extracted to `CreditPolicyConfig`. Highest regulatory priority — these values affect credit
  issuance semantics.
- **icn-obs** (Sprint 22, PR #1390): Five attestation threshold constants extracted to
  `AttestationConfig` / `ContributionAttestationConfig`.

The audit document at `docs/architecture/meaning-firewall-audit.md` reflects Sprint 22
completion. No remaining hardcoded policy violations are known across the four target crates.

### Pilot Completion Epic (#1099)

The Pilot Completion epic closed all but three issues before Sprint 23. As of the Sprint 23
board:

- **#1095** (CRDT OrSet + LwwRegister implementation) — deferred to Sprint 24 with rationale
  (see "What Is Deferred" below).
- **#1096** (ContainerRuntime trait interface) — complete; `ContainerRuntime` trait defined in
  `icn-kernel-api/src/container.rs`, object-safe, async, with `ContainerSpec`,
  `ResourceLimits`, `ContainerResult`, `ResourceUsage`, and `ContainerError` types.
- **#1131** (Storage Specification) — complete; spec written at
  `docs/state/storage-governance-spec.md`; `StorageClass` and `DataLocality` types
  implemented in `icn-kernel-api/src/storage.rs` (see Kernel API Surface below).

### Vertical Slice Epic (#1147)

The Vertical Slice epic is closed. All tracks are done. The vertical slice demonstrated
end-to-end path from identity through governance through compute settlement, with all
components wired through the constraint enforcement architecture.

### Sprint 23 Track A — Operational Closure

All four Track A tasks are done. The repo now tells the truth:

- **s23-t1**: Test Coverage CI failure classified as non-blocking observational gate.
  Rationale: `cargo-tarpaulin` v0.35.2 requires a full clean instrumented recompile of all
  34+ crates; on GitHub-hosted runners this exceeds runner lifetime (~28 min) before tests
  can run. Gate is `GATE_RATCHET_PHASE_COVERAGE: observational`. Does not block merges.
  Documented in `ops/state/ci-exceptions.md`.
- **s23-t2**: Dirty file from the #1394 skills rewrite merge committed to `main` (commit
  `627857a2`). `git status` is clean.
- **s23-t3**: Stale `1310-execution-receipt-gate` worktree removed. `icn-wt/` is clean.
- **s23-t4**: Sprint 22 formally closed. `ops/state/sprint/sprint-22-closed.json` committed.
  All five s22 tasks confirmed done. No "in-review" state points at already-merged PRs.

### Sprint 23 Track B — P0 Residue

- **s23-t6**: `ContainerRuntime` trait implemented (`icn-kernel-api/src/container.rs`),
  closing #1096. See Kernel API Surface below.
- **s23-t7**: Storage governance spec written (`docs/state/storage-governance-spec.md`),
  closing #1131.

### Kernel API Surface (icn-kernel-api)

The `icn-kernel-api` crate (`icn/crates/icn-kernel-api/`) defines trait interfaces for all
kernel primitives. Implementations live elsewhere; this crate provides only contracts.
Key types now present and tested:

| Module | Key types |
|--------|-----------|
| `storage` | `StorageClass` (Canonical, ServiceState, Blobs), `DataLocality` (CellLocal, CoopReplicated, FederationMirrored, CommonsPublic), `StorageValidationError` |
| `container` | `ContainerRuntime` (trait), `ContainerSpec`, `ResourceLimits`, `ContainerResult`, `ResourceUsage`, `ContainerError` |
| `coord` | `CrdtType` enum variants (GCounter, PNCounter, OrSet, LwwRegister, MvRegister — stubs, no implementations yet) |
| `authz` | `PolicyOracle` (trait), `ConstraintSet`, `PolicyDecision`, `CapabilityEngine`, `AllowAllOracle`, `DenyAllOracle` |

The kernel is eight primitives: Identity, Authorization, State, Compute, Communication,
Time, Coordination, and Naming. All eight have module stubs in `icn-kernel-api`. Maturity
varies — `authz` and `storage` are the most developed; `naming` and `coordination` are
skeletal.

### Demo Flows

Three governance demo flows are implemented in `demo/scripts/`:

- `flow-1-governance.sh` — WASM compute task with governance constraint enforcement
- `flow-2-patronage.sh` — Patronage discovery and member coordination
- `flow-3-federation.sh` — Cross-cooperative federation treaty and resource sharing

A fourth flow (`flow-4-reporting.sh`) exists. The canonical demo entry point is
`present-governance.sh`. Run on Zentith with `bash demo/scripts/present-governance.sh --port 9080`
(port 8080 is occupied by the Docker devnet).

---

## What Is In Progress / Deferred

### CRDT Implementations (#1095) — Deferred to Sprint 24

`CrdtType` in `icn-kernel-api/src/coord.rs` defines the enum variants: `GCounter`,
`PNCounter`, `OrSet`, `LwwRegister`, `MvRegister`. These are type-level stubs only.
Zero concrete implementations exist. The `Coordination` primitive trait references these
types but no crate implements the merge functions.

Deferred rationale: Sprint 23 is a stabilization sprint. New feature implementations land
on a clean, locked baseline. CRDT implementations are Sprint 24 work.

### ContainerAttestation — Not Yet Defined

`ContainerAttestation` — the type that proves a container ran what it claimed — is not yet
defined anywhere in the codebase. It is a Sprint 24 item alongside CRDT implementations.
The `ContainerRuntime` trait as implemented returns execution results (`ContainerResult`),
not attested receipts.

### CI: Test Coverage Gate — Permanently Observational

`cargo-tarpaulin` v0.35.2 consistently times out on GitHub-hosted runners due to
instrumented build time. The gate is classified `observational` (does not block merges).
Resolution options include scoping tarpaulin to a subset of crates or switching to the
self-hosted `ci-runner` at `10.8.30.46`. No resolution is required before Sprint 24 begins.
See `ops/state/ci-exceptions.md` for full technical detail.

### Commons Compute Layer — Not Yet Started

The commons compute architecture (resource pooling, unaffiliated node participation,
stale participant expiry) is designed via issues #925, #947, #964 but has no Rust
implementation. This is the primary Sprint 24 spine.

---

## What Is Next

### Sprint 24 Candidates (Commons Compute Hardening)

**#925 — Commons Resource Pool and Contribution Accounting**
Scope: Implement the `CommonsPool` aggregate capacity tracker and credit accounting for
nodes contributing to and consuming from the public `Commons` scope.
Rationale: The commons layer is the mechanism for unaffiliated and independent node
participation. Without it, ICN serves only organizationally-affiliated peers.

**#947 — Unaffiliated Node Participation Protocol**
Scope: Enable nodes without cell or org membership to announce capacity (`commons_share =
1.0`), claim `Commons`-scoped tasks, and earn credits via execution receipt settlement.
Rationale: Cooperative infrastructure must be accessible to individuals, not just to
incorporated cooperatives. Unaffiliated participation is the on-ramp.

**#964 — Stale Commons Pool Participant Expiry**
Scope: Add configurable expiry for `CommonsParticipant` entries that have not announced
within a threshold (default: 5 minutes), with metrics (`commons_pool_expired_total`) and
info-level logging.
Rationale: The `CommonsPool` as designed accumulates stale entries indefinitely. Expiry is
required for pool health before commons compute can be trusted in production.

Full Sprint 24 planning is out of scope for this document.

---

## Quick Reference

| Thing | Where |
|-------|-------|
| Rust workspace | `icn/icn/` (Cargo root, 34 crates + 4 apps) |
| Sprint board | `ops/state/sprint/current.json` |
| Sprint 22 closed | `ops/state/sprint/sprint-22-closed.json` |
| CI exceptions | `ops/state/ci-exceptions.md` |
| Architecture | `docs/ARCHITECTURE.md` |
| Meaning firewall audit | `docs/architecture/meaning-firewall-audit.md` |
| Kernel API | `icn/crates/icn-kernel-api/src/` |
| Storage spec | `docs/state/storage-governance-spec.md` |
| Gateway port | 8080 (local), 30080 (K3s NodePort) |
| K3s cluster | control `10.8.30.40`, workers `.41` `.42` (VLAN 30) |
| Demo flows | `demo/scripts/flow-1-governance.sh`, `flow-2-patronage.sh`, `flow-3-federation.sh` |
| Demo entry point | `demo/scripts/present-governance.sh --port 9080` |
