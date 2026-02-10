---
name: icn-orchestrator
description: Route and decompose ICN work across subsystems with explicit invariants, task boundaries, verification commands, and merge order.
metadata:
  short-description: ICN task router
---

# ICN Orchestrator Skill (Codex)

Use this skill when a request may touch more than one ICN subsystem.

## Purpose

- Classify scope.
- Decompose work into parallelizable tasks.
- Preserve ICN invariants and change-routing discipline.

## Inputs to Read First

1. `AGENTS.md`
2. `CLAUDE.md`
3. `.github/copilot-instructions.md`
4. `.github/agents/README.md`
5. Relevant `.github/instructions/*.md`

## Output Contract

Always produce:

1. Classification:
   - Subsystems touched.
   - Single-scope or multi-scope.
2. Invariants at risk:
   - Explicit list tied to the requested change.
3. Task breakdown:
   - Task ID, goal, files, verification commands, dependencies.
4. Merge order:
   - Deterministic and conflict-minimizing.

## ICN Subsystem Labels

- `rust-core`
- `trust-identity`
- `gossip-net`
- `gateway-api`
- `ledger-econ`
- `governance-ccl`
- `sdk-ts`
- `sdk-react-native`
- `web-pilot-ui`
- `deploy-k8s`
- `docs`

## Verification Reference

Always reference `AGENTS.md` "Change routing" for the correct commands per subsystem.
