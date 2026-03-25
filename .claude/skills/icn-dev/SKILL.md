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

### Current Sprint: Sprint 19 — Pilot Genesis

**Cluster:** 3/3 nodes Ready, K3s v1.34.4+k3s1, ~21-day uptime (as of Mar 21)

**Demo Flows:**
- Flow 1A (Governance): OPERATIONAL
- Flow 1B (Governance + Provenance): BLOCKED (signing key not configured)
- Flow 2 (Patronage): OPERATIONAL
- Flow 3 (Federation): STRUCTURALLY BLOCKED (P2P address bug, PR #1381 needed)
- Flow 4 (Reporting): BLOCKED (depends on Flow 3)

**Phase 1 Compliance Sprint (urgent before grant applications):**
- Rename: `payment` -> `settlement`, `currency` -> `unit`, `balance` -> `position`
- Make `JournalEntry.provenance` required
- Add `Obligation` type with lifecycle states
- Extract commons credit formula to CCL
- CI linter for regulatory terminology

**Critical blockers:**
- PR #1381 (service discovery persistence) — needs review/merge
- ExecutionReceiptGate signing key — not configured in K3s secrets
- ops/mcp: 4 modified + 10 untracked files uncommitted on icn-dev
- Villain CI broken since Mar 8 (separate project, same Zentith infra)

**Target:** v1.0.0 tag when vertical slice integration test passes (~Apr 15)

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

Load these when needed:
- Crate detail: `~/.claude_launchpad/projects/icn/icn-crate-reference.md`
- Current state: `~/.claude_launchpad/projects/icn/icn-state-2026-03-21.md`
- Forward plan: `~/.claude_launchpad/projects/icn/icn-forward-plan.md`
- Ecosystem map: `~/.claude_launchpad/projects/icn/icn-ecosystem-map.md`
- Demo flows: `~/.claude_launchpad/projects/icn/SPRINT-DEMO-READY.md`

## Commands

- `/icn-trace` — map a feature across crates, routes, tests, docs
- `/icn-audit` — security, trust, and regulatory audit of a code path
- `/icn-demo-check` — full demo readiness check with live cluster queries
- `/icn-state-sync` — sync state docs and flag stale documentation

## Operating Rules

1. **State before action.** Read the current state doc before making architectural recommendations. The project is at Sprint 19, not at the beginning.
2. **Regulatory framing is not optional.** The Sovereign Tech Fund grant and cooperative framing depend on clean terminology. Flag violations in code review even when they seem cosmetic.
3. **The economists and architects don't talk to each other by default.** When a change spans economics AND protocol shape (e.g. changing how the ledger Merkle-DAG is structured), explicitly involve both icn-architect and icn-economist.
4. **ops/mcp dirty state blocks session continuity.** If icn-dev has uncommitted ops/mcp changes, recommend committing before doing other work. The event bus and decision log aren't trustworthy with dirty state.
5. **Four demo flows are the acceptance criteria.** "Done" means the relevant demo flows pass. Code that compiles but breaks a demo flow is not done.
6. **icn-dev disk is a recurring hazard.** Rust build artifacts fill `/var/lib/rancher`. Check before long build sessions. `cargo clean` is safe.
