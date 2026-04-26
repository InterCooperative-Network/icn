---
name: icn-dev
description: >
  ICN development companion for the InterCooperative Network Rust monorepo. Use when
  working on ICN code, docs, deployment, or protocol design. Provides crate-aware routing
  to specialist agents (icn-architect, icn-economist, icn-ops), enforces project conventions,
  and understands the current sprint state, cluster topology, and demo flow status.
  Triggers on: any ICN crate names, "cooperative contract", "mutual credit", "governance",
  "gossip", "K3s", "icn-dev", "ops/mcp", "Sprint", "demo flow", "CCL", "federation",
  "DID", "ledger", "trust graph", "icnd", "icnctl".
version: 0.1.0
truth_contract:
  canonical_sources:
    - ops/state/sprint/current.json     # sprint number, tasks, goals (ALWAYS live-read)
    - ops/state/truth/agents.json       # agent routing table
    - ops/state/config/repo-map.json    # cluster topology, workspace structure
  live_load_required:
    - "cat \"$(git rev-parse --show-toplevel)/ops/state/sprint/current.json\""
    - "git branch --show-current"
  examples_only: []
  never_hardcode:
    - sprint number (always read from current.json)
    - cluster IPs (read from repo-map.json)
    - PR numbers or branch names
    - demo flow status (live-read from sprint/current.json or kubectl)
---

# ICN Dev Skill — Routing and Context Layer

This skill is the routing and context layer for ICN development sessions. It loads project state, routes to specialist agents, enforces conventions, and tracks the current sprint.

## Core Principle

Every ICN request passes through three questions:
1. **Which domain?** Code architecture / economics / deployment / docs / planning
2. **Which agent?** Route to the specialist with the right mental model
3. **What's the current state?** Load sprint context before acting

## Specialist Routing

| Domain | Route to | When |
|--------|----------|------|
| Crate boundaries, kernel/app separation, public API, refactors, ADRs, protocol shape, concurrency | **icn-architect** | Anything touching crate structure or protocol design |
| Ledger, CCL, mutual credit, treasury, mana, settlement flows, regulatory terminology, economic invariants | **icn-economist** | Anything touching economics or the compliance sprint |
| K3s cluster, pods, demo flows, CI, release readiness, ops/mcp, disk space | **icn-ops** | Anything touching deployment or demo validation |
| Cross-cutting: feature trace, audit, state sync | **/icn-trace, /icn-audit, /icn-state-sync** | Use the commands directly |

**When in doubt:** Use **icn-architect** as the default. It has the broadest view and can re-route.

## Project State (load on every ICN session)

**Always read live state — never trust hardcoded snapshots in this file.**

```bash
# Sprint state
cat "$(git rev-parse --show-toplevel)/ops/state/sprint/current.json"

# Open PRs
gh pr list --repo InterCooperative-Network/icn --json number,title,headRefName,mergeable

# Recent CI
gh run list --repo InterCooperative-Network/icn --branch main --limit 3 --json status,conclusion,name
```

**Cluster endpoints** (post Feb-2026 VLAN 30 migration):
- Control: `10.8.30.40`, Workers: `10.8.30.41`, `10.8.30.42`, Dev VM: `10.8.30.45`
- Gateway: `10.8.30.40:30080`, Pilot UI: `10.8.30.40:30030`, Metrics: `10.8.30.40:30090`

## Convention Enforcement

When working in this skill context, enforce:

1. **Branch discipline** — always verify `git branch --show-current` before editing. ICN uses feature branches.
2. **Regulatory terminology** — immediately flag and suggest replacement for: payment, currency, wallet, token, blockchain, transaction fee.
3. **Kernel/app boundary** — flag changes to kernel crates (icn-identity, icn-trust, icn-net, icn-gossip, icn-core, icn-store, icn-encoding, icn-obs, icn-time, icn-snapshot, icn-security, icn-rpc, icn-crypto-pq, icn-zkp, icn-steward) that need ADR review.
4. **Test coverage** — new logic in any crate must have tests. Flag if missing.
5. **Provenance required** — any JournalEntry creation must have provenance set. Flag if empty/None.

## Crate Quick Reference

**Kernel (ADR required for public API changes):**
icn-identity, icn-trust, icn-net, icn-gossip, icn-privacy, icn-core, icn-store, icn-encoding, icn-obs, icn-rpc, icn-time, icn-snapshot, icn-security, icn-crypto-pq, icn-zkp, icn-steward

**Application (normal change process):**
icn-ledger, icn-ccl, icn-compute, icn-gateway, icn-governance, icn-federation, icn-community, icn-entity, icn-coop

**Binaries:** icnd, icnctl, icn-console

## Reference Files

**In-repo sources (authoritative):**
- Sprint state: `icn/ops/state/sprint/current.json`
- ADRs: `icn/docs/adr/` (canonical; `ops/state/decisions/` is a redirect)
- Architecture: `icn/docs/ARCHITECTURE.md`
- Current state: `icn/docs/STATE.md`
- Planning: `icn/docs/planning/` and `icn/docs/strategy/`

**Launchpad docs** (on Zentith / Launchpad HQ — not on this dev VM):
- `~/.claude_launchpad/projects/icn/icn-crate-reference.md`
- `~/.claude_launchpad/projects/icn/icn-forward-plan.md`
- Only accessible if SSH'd to Zentith (10.8.10.100) or running locally there.

## Commands

- `/icn-trace` — map a feature across crates, routes, tests, docs
- `/icn-audit` — security, trust, and regulatory audit of a code path
- `/icn-demo-check` — full demo readiness check with live cluster queries
- `/icn-state-sync` — sync state docs and flag stale documentation

## Operating Rules

1. **State before action.** Read `ops/state/sprint/current.json` before making architectural recommendations. The project is deep into Sprint 26+ — do not reason from first principles about what's been built.
2. **Regulatory framing is not optional.** The Sovereign Tech Fund grant and cooperative framing depend on clean terminology. Flag violations in code review even when they seem cosmetic.
3. **The economists and architects don't talk to each other by default.** When a change spans economics AND protocol shape (e.g. changing how the ledger Merkle-DAG is structured), explicitly involve both icn-architect and icn-economist.
4. **ops/mcp dirty state blocks session continuity.** If icn-dev has uncommitted ops/mcp changes, recommend committing before doing other work. The event bus and decision log aren't trustworthy with dirty state.
5. **Four demo flows are the acceptance criteria.** "Done" means the relevant demo flows pass. Code that compiles but breaks a demo flow is not done.
6. **icn-dev disk is a recurring hazard.** Rust build artifacts fill `/var/lib/rancher`. Check before long build sessions. `cargo clean` is safe.
