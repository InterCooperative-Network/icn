# ICN Copilot Agents

This repo defines custom Copilot agents under `.github/agents/`.

## How selection works

- `infer: true` agents may be auto-selected by Copilot based on the prompt.
- Most ICN specialists are **infer: false** (manual) to avoid "wrong agent drift."
- The **icn-orchestrator** is `infer: true` and is the default router.

## Agent Categories

### Meta Agents (cross-cutting)

| Agent | Purpose |
|-------|---------|
| `@icn-orchestrator` | Router + decomposer for multi-subsystem work (auto-selects) |
| `@icn-planner` | Strategic planning, task breakdown, dependency analysis |
| `@icn-architect` | System design, crate boundaries, API design review |
| `@icn-code-reviewer` | PR review with ICN invariants lens, high signal-to-noise |
| `@icn-invariants-guardian` | Blocks safety regressions, enforces protocol semantics |
| `@icn-pr-writer` | PR descriptions, commit messages, changelog entries |
| `@icn-security-auditor` | Security review, threat modeling, attack surface analysis |
| `@icn-refactoring` | Safe refactoring with invariant preservation |

### Infrastructure & Operations

| Agent | Purpose |
|-------|---------|
| `@icn-homelab-infra` | K3s cluster ops, NFS storage, networking, runner management |
| `@icn-devnet-deploy` | Docker/K8s manifests, config templates, monitoring setup |
| `@icn-ci-reliability` | CI/test reliability, flake hunting, workflow fixes |
| `@icn-monitoring` | Prometheus rules, Grafana dashboards, alerting, observability |

### Domain Specialists

| Agent | Purpose |
|-------|---------|
| `@icn-rust-core` | Rust workspace implementer |
| `@icn-gateway-api` | Gateway API + OpenAPI + TS types drift |
| `@icn-trust-identity` | DIDs, credentials, trust graphs, SDIS |
| `@icn-gossip-net` | Gossip, discovery, network safety |
| `@icn-ledger-econ` | Mutual credit, economic invariants |
| `@icn-governance-ccl` | Governance proofs, CCL, policy oracles |
| `@icn-docs-spec` | Docs/spec consistency enforcer |
| `@icn-docs-synchronizer` | Documentation drift prevention (existing) |
| `@icn-sdk-web` | TypeScript SDK, React Native SDK, Pilot UI |

## Parallel work (recommended)

For multi-area changes, run **multiple Copilot tasks/PRs in parallel**:

1. Create Task A (Rust/core) → use `@icn-rust-core`
2. Create Task B (Gateway/OpenAPI/SDK drift) → use `@icn-gateway-api`
3. Create Task C (Docs/spec alignment) → use `@icn-docs-spec`
4. Create Task D (CI/test reliability) → use `@icn-ci-reliability`
5. Create Task E (Invariants/security review) → use `@icn-invariants-guardian`

Then merge in this order:
- Invariants review first (or in parallel but reviewed first)
- Core changes
- API/spec drift
- Docs/spec sync
- CI hardening

## Orchestrator Workflow

```
User Request
     │
     ▼
┌─────────────────┐
│ icn-orchestrator│  ← auto-selected (infer: true)
│                 │
│ 1. Classify     │
│ 2. Decompose    │
│ 3. Route        │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
Single-scope  Multi-scope
    │              │
    ▼              ▼
Invoke via     Output parallel plan:
`agent` tool   - Task A → @icn-rust-core
               - Task B → @icn-gateway-api
               - Task C → @icn-docs-spec
               - Merge order + dependencies
```

## Non-negotiables

All agents must follow `AGENTS.md`, plus:
- Rust workspace is `icn/` (repo root is not a Cargo workspace).
- No weakening trust gates, signature checks, or canonical encodings to "fix tests."
- If semantics change, docs/spec must change in the same PR (or a blocking issue is created).

## Expert Knowledge

All agents are imbued with ICN-relevant expertise:
- **Distributed Systems**: CAP theorem, vector clocks, CRDTs, Byzantine fault tolerance
- **Cryptography**: Ed25519, X25519, post-quantum (ML-DSA, ML-KEM), ZKPs, threshold crypto
- **Systems Engineering**: Actor model, async/await, memory safety, backpressure
- **Protocol Design**: Canonical encoding, schema evolution, deterministic serialization
- **Security Mindset**: Adversarial thinking, defense in depth, fail-secure defaults
