# ICN project state snapshot (2026-02-09)

## Purpose

This document captures the current project state based on direct repository inspection on **2026-02-09**.

It is intended to be a high-confidence map of what is implemented, how the system is wired, where safety boundaries exist, and what risks or inconsistencies remain.

## Snapshot metadata

- Date: 2026-02-09
- Branch: `feat/treasury-spend-proof-gate`
- HEAD: `d2d46bf8733e764c947c1168446a27156ea8e405`
- Working tree at capture: clean (`git status --short` had no output)

## Method

The snapshot is derived from source and build metadata, not only from historical docs.

Primary evidence sources:

- Workspace and dependency graph: `icn/Cargo.toml`, `cargo metadata`
- Runtime entrypoints: `icn/bins/icnd/src/main.rs`, `icn/bins/icnctl/src/main.rs`, `icn/bins/icn-console/src/main.rs`
- Supervisor wiring: `icn/crates/icn-core/src/supervisor/mod.rs`, `icn/crates/icn-core/src/supervisor/lifecycle.rs`
- Gateway boundary/auth surfaces: `icn/crates/icn-gateway/src/server.rs`, `icn/crates/icn-gateway/src/api/`, `icn/crates/icn-gateway/src/middleware.rs`
- Protocol invariants in core domains:
  - Networking envelope: `icn/crates/icn-net/src/envelope.rs`
  - Gossip protocol types: `icn/crates/icn-gossip/src/types.rs`
  - Ledger types/invariants: `icn/crates/icn-ledger/src/types.rs`
  - Governance state machine: `icn/crates/icn-governance/src/proposal.rs`
- CI and quality gates: `.github/workflows/ci.yml`, `.github/workflows/api-types.yml`, `.github/workflows/security-audit.yml`
- SDK/Web/Deploy surfaces:
  - `sdk/typescript/package.json`
  - `sdk/react-native/package.json`
  - `web/pilot-ui/package.json`
  - `web/dashboard/package.json`
  - `deploy/` tree and docs

## Executive state

ICN is an actively developed multi-surface system with a large Rust core plus SDK/web/deploy layers, and clear evidence of ongoing kernel/app separation work.

Current structure is substantial and coherent:

- Rust workspace root correctly located at `icn/`
- 36 workspace members currently registered via `cargo metadata`
- 31 crate directories under `icn/crates/`, 2 workspace app crates under `icn/apps/`, 3 binaries under `icn/bins/`
- Gateway API surface is broad (`295` route macro declarations in `icn-gateway/src/api`)
- CI includes formatting, linting, unit/integration test split, OpenAPI/type drift checks, security audit, and deployment checks

## Architecture and boundaries

### Runtime and entrypoints

- `icnd` (`icn/bins/icnd/src/main.rs`) is the daemon entrypoint and explicitly denies unwrap/expect panics in this binary.
- `icnctl` and `icn-console` are additional operational/user entrypoints.
- Supervisor orchestration is centralized in `icn-core`:
  - `icn-core/src/supervisor/mod.rs`
  - `icn-core/src/supervisor/lifecycle.rs`
- The supervisor initializes actors and binds:
  - network <-> gossip bridging
  - ledger/governance/compute/cooperative/community/entity/federation/steward handles
  - optional gateway startup

### Kernel/app separation direction

There is clear implementation of kernel/app separation mechanics (service registry, policy oracle routing), but separation is still transitional in places.

Observed indicators:

- Daemon-side service construction in `icnd` uses app crates and injects services into kernel runtime.
- `OracleRegistry` and policy phases are wired in supervisor lifecycle.
- CI includes Meaning Firewall checks and forbidden dependency checks.
- However, `icn-core/Cargo.toml` still includes domain dependencies (`icn-ledger`, `icn-governance`, and dev dependency `icn-trust`), signaling ongoing migration rather than a fully completed separation.

### Dual app locations (important structural note)

There are two app locations in the repo:

- Top-level `apps/` with `echo`, `governance`, `ledger`, `trust`
- Workspace `icn/apps/` with `governance`, `membership`

`icnd` wiring references top-level app crates (for example trust/governance service creation), while workspace members include `icn/apps/...` crates.

This is workable but introduces discoverability and maintenance risk unless explicitly documented as intentional architecture.

## Security and trust posture (code-evidenced)

### Network and message integrity

- `icn-net/src/envelope.rs` implements signed envelopes with replay-related fields and optional hybrid signatures (`post-quantum` feature path).
- Envelope verification paths support classical and hybrid transition behavior.

### Gossip hardening

- `icn-gossip/src/types.rs` shows defensive limits and structures:
  - compression thresholding
  - bounded decompression max size
  - sync cursor expiry
  - scoped propagation

### Ledger determinism and safety

- `icn-ledger/src/types.rs` defines Merkle-addressed journal entries and account deltas with checked arithmetic.
- Overflow-safe paths and explicit invariant-related error types are present.

### Governance semantics

- `icn-governance/src/proposal.rs` has an explicit proposal lifecycle/state machine with terminal-state semantics documented in code.

### Gateway auth and boundary handling

- Gateway is Actix-based (`icn-gateway/src/server.rs`) with auth middleware and JWT-driven request identity propagation across many API modules.
- Route surface includes core domain APIs plus SDIS, constitutional, commons, treasury, federation, etc.

## Test and CI posture

### CI coverage (workflow-level)

`ci.yml` includes:

- formatting check (`cargo fmt --all --check`)
- clippy with warnings denied
- unit tests + serial integration tests
- dedicated `icn-gateway --features sled-storage` test run
- backup validation workflow
- TypeScript SDK and Web UI test jobs
- accessibility tests

Additional workflows:

- `api-types.yml`: OpenAPI generation and TypeScript type drift checks
- `security-audit.yml`: scheduled security audit pipeline

### Non-blocking checks (important)

There are several `continue-on-error: true` jobs in CI, including:

- Meaning firewall check
- Firewall contract enforcement
- coverage job notes
- SDK/web-related jobs

Implication: some quality/security assertions are currently observational rather than hard merge gates.

## SDK and web surfaces

### TypeScript SDK (`sdk/typescript`)

- Package: `@icn/client` `0.1.0`
- Build pipeline includes generated types from `docs/api/openapi.generated.yaml`
- Lint/test/build scripts present

### React Native SDK (`sdk/react-native`)

- Package: `@icn/react-native` `0.1.0`
- Depends on `@icn/client`
- Test/build scripts present

### Pilot UI (`web/pilot-ui`)

- Jest + Playwright + explicit accessibility test scripts
- Appears integrated into CI

### Dashboard (`web/dashboard`)

- Static server scripts only (`python3 -m http.server 8080`)
- No meaningful automated tests yet (`"No tests yet"` script)

## Deployment and operations state

The repo contains multiple deployment pathways:

- Docker Compose (`deploy/docker-compose.yml`, root compose variants)
- Kubernetes manifests (`deploy/kubernetes/` and `deploy/k8s/`)
- Helm chart (`deploy/helm/icn`)
- K3s-focused scripts and runbooks (`deploy/k8s/...`)

Operational docs indicate active K3s/homelab usage, but some status docs are point-in-time and should not be treated as live state without runtime verification.

## Known risks and inconsistencies

1. Documentation freshness is uneven.
- Some status docs are explicitly historical snapshots.
- Legacy summaries can conflict (for example roadmap/phase framing changed over time).

2. CI has non-blocking gates in key architectural checks.
- Meaning-firewall and related checks are not fully strict yet.

3. Architecture migration still in transition.
- Kernel/app separation is real and advanced, but not fully complete in dependency graph terms.

4. App location split (`apps/` vs `icn/apps/`) can confuse contributors.
- This should be documented as intentional or consolidated over time.

5. Large files indicate maintainability hotspots.
- `icn/bins/icnctl/src/main.rs` (9737 lines)
- `icn/crates/icn-gateway/src/api/governance.rs` (4869 lines)
- `icn/crates/icn-ledger/src/ledger.rs` (4628 lines)

## Open TODO hotspots (sample, code-derived)

Selected unresolved TODOs in core paths:

- `icn/crates/icn-core/src/apps/dispatcher.rs` (state snapshot copy-on-write TODO)
- `icn/crates/icn-core/src/supervisor/init_rpc.rs` (PolicyOracle-based rate limiting TODO)
- `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs` (threshold PRF/rate-limiting TODOs)
- `icn/crates/icn-ledger/src/commons_credits.rs` (governance-configurable constants TODO)
- `icn/crates/icn-governance/src/proposal_cleanup.rs` (archive/index follow-up TODO)

Repository-wide TODO/FIXME/XXX markers under `icn/crates`, `icn/bins`, `icn/apps`: `60` matches from grep scan.

## Definition of done for this snapshot

This snapshot is complete for its purpose because it now provides:

- code-backed architecture map and boundary inventory
- current workspace/member counts and entrypoint map
- CI/testing/deployment posture with explicit caveats
- security and invariant-relevant mechanism pointers
- concrete risk list and migration-status observations
- reproducible evidence paths for all major claims

## Recommended next documentation actions

1. Adopt this file as the canonical operational snapshot for Q1 2026 and date-stamp updates.
2. Add an explicit note in architecture docs clarifying the two app roots and intended ownership.
3. Promote selected `continue-on-error` checks to blocking as migration phases close.
4. Create a small \"hotspots\" refactor plan for oversized files in gateway/ledger/icnctl.

## Boundary hardening follow-up

- CI ratchet plan: `docs/ci/GATE_RATCHET_PLAN.md`
- App topology ADR: `docs/adr/ADR-0010-app-topology.md`
