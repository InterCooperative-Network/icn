---
Status: descriptive
Canonical: no
Last Reviewed: 2026-08-19
---

# ICN Ecosystem Atlas

> **Ecosystem index for humans and agents.** This file routes questions to owners. It does not own project state, protocol semantics, repo-local operating detail, deployment liveness, active partnerships, sprint state, or readiness claims.

Machine-readable fact ownership lives in [`ops/state/truth/sources.json`](../ops/state/truth/sources.json). Repository/worktree coordination metadata lives in [`ops/state/config/repo-map.json`](../ops/state/config/repo-map.json). Cross-repo machine orientation lives in [`ops/state/ecosystem.json`](../ops/state/ecosystem.json).

If this Atlas disagrees with a registered owner, **the owner wins**.

## 1. What this repository is

ICN is the generic substrate/runtime for democratic institutions and cooperative coordination. Its architectural boundary is the **Meaning Firewall**: generic substrate enforces structure, cryptographic evidence, scope, constraints, and protocol state; institution-specific meaning belongs in governed applications/packages/policy.

Use [`AGENTS.md`](../AGENTS.md) for the five provider-neutral invariants and [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) for system architecture. Do not derive current implementation/readiness from this paragraph.

Institution packages such as NYCN consume ICN primitives while retaining institution-local meaning and private operating detail outside generic core. Resolve the current package/repository boundary from the registered sources and relevant boundary documents rather than from a copied status summary here.

## 2. Start by classifying the question

| You need to know… | Start here |
|---|---|
| Which source owns this kind of fact? | [`ops/state/truth/sources.json`](../ops/state/truth/sources.json) |
| What is real/current for a specific claim? | [`current-truth-map.md`](reference/project-index/current-truth-map.md), then the claim's owner/live evidence |
| How should conflicting sources be interpreted? | [`source-of-truth-map.md`](reference/project-index/source-of-truth-map.md) |
| What repository/worktree/repo-family metadata exists? | [`ops/state/config/repo-map.json`](../ops/state/config/repo-map.json) + [`ops/state/ecosystem.json`](../ops/state/ecosystem.json) |
| What does this checkout implement? | Current source/tests/schemas; use project-index maps for navigation |
| What semantics are allowed for a domain? | Resolve that domain's owner in `sources.json` |
| What decisions were accepted? | [`docs/adr/`](adr/) / RFC status plus the affected domain owner |
| What is the current PR/CI/issue state? | Live GitHub; never this Atlas |
| What merge policy applies? | [`ops/state/truth/policy.json`](../ops/state/truth/policy.json) plus live branch protection |
| Which specialist/skill should an agent use? | [`ops/state/truth/agents.json`](../ops/state/truth/agents.json) / [`ops/state/truth/skills.json`](../ops/state/truth/skills.json) |
| Where is a source file/crate/runtime surface? | [`docs/reference/project-index/`](reference/project-index/README.md) |
| What can safely be claimed publicly? | [`show-readiness-map.md`](reference/project-index/show-readiness-map.md) + current evidence/registered owners |

## 3. The map family

The project-index directory contains **navigation projections**, not higher-order truth.

Useful maps include:

- [`README.md`](reference/project-index/README.md) — role/task navigation;
- [`current-truth-map.md`](reference/project-index/current-truth-map.md) — current-claim routing;
- [`source-of-truth-map.md`](reference/project-index/source-of-truth-map.md) — conflict/claim classification;
- [`source-tree-map.md`](reference/project-index/source-tree-map.md) — source-tree navigation;
- [`rust-workspace-map.md`](reference/project-index/rust-workspace-map.md) — Rust workspace navigation;
- [`runtime-surface-map.md`](reference/project-index/runtime-surface-map.md) — runtime-surface inventory/orientation;
- [`show-readiness-map.md`](reference/project-index/show-readiness-map.md) — public-claim guardrails;
- generated records under `docs/reference/project-index/generated/` — mechanical snapshots/projections.

When a map cites a code path, owner, or live state, inspect that source before making a consequential claim.

## 4. Repository and privacy boundaries

Do not duplicate a mutable cross-repo inventory in this file. Resolve repo relationships from `ops/state/config/repo-map.json` and `ops/state/ecosystem.json`.

The durable boundary rules are:

- **ICN core:** generic primitives/runtime/public substrate truth.
- **Institution packages:** institution-local meaning, governed configuration, fixtures/templates appropriate to that package.
- **Private operations/provider sources:** concrete topology, credentials/secrets, real partner/member/attendee data, and provider-specific operating detail.
- **Learning/outreach/integration repos:** consumers/translators of canonical ICN truth, not automatic owners of it.

Never move private operating detail or real personal/partner data into this public repository just to make an agent self-contained. An agent that lacks access must report the boundary rather than hallucinate or copy sensitive context into public docs.

## 5. Claim boundaries

A few distinctions matter across the ecosystem regardless of subsystem:

- normative contract ≠ implementation;
- implementation ≠ runtime integration;
- integration ≠ deployment;
- deployment ≠ production readiness/adoption;
- fixture/demo/rehearsal ≠ live institutional use;
- cryptographic authorship ≠ authority/legitimacy;
- service dependency/custody ≠ sovereignty;
- generated projection ≠ source truth;
- handoff/session memory ≠ current state.

For a concrete claim, resolve the relevant owner/evidence rather than treating this list as proof.

## 6. Agent / LLM entrypoint

A fresh agent should **not** read the entire Atlas before doing anything. Use it only when cross-repo/ecosystem orientation is relevant.

Normal non-trivial startup is:

1. [`AGENTS.md`](../AGENTS.md) — provider-neutral operating contract;
2. [`ops/state/truth/sources.json`](../ops/state/truth/sources.json) — resolve the relevant fact domain(s);
3. read only the registered owner(s) needed for the task;
4. query live Git/GitHub if the question is volatile;
5. inspect current code/tests if the claim concerns implementation;
6. use the Agent Context Spine path brief/project-index maps for navigation;
7. consult this Atlas for ecosystem/private-boundary/cross-repo routing;
8. consult handoffs/history only when resume/rationale context is actually needed.

Provider adapters (`CLAUDE.md`, Codex workflow, plugins) may add tool mechanics but do not change this ownership model.

## 7. NYCN and other institution packages

Do not encode a mutable statement here about which package is "the first partner," whether a pilot is formal, which summit/committee is active, or which repo is currently operationally primary.

For institution-package work:

1. resolve the package/boundary owner through `sources.json`, repo-map, and current boundary docs;
2. inspect the package's own source when access is available;
3. use live issues/PRs for current coordination state;
4. keep real PII/private overlays/private operating detail out of public ICN docs;
5. route generic primitive pressure upstream as ICN work rather than copying ICN implementation into the package.

If package ownership itself is disputed, treat that as a boundary/ownership finding and resolve it explicitly. Do not let this index decide it by prose.

## 8. Readiness and status material

`docs/status.toml`, status reports, phase narratives, and historical state docs can be valuable descriptive evidence. They are not universal current-state roots.

Before using a readiness/status assertion:

- inspect its `last_verified`/evidence metadata;
- inspect the cited implementation/runtime evidence when material;
- distinguish code evidence from operational liveness;
- requery live systems when the claim is volatile;
- do not let an old high-level score override a newer domain contract or source finding.

## 9. Keeping the Atlas healthy

Update this file only when **routing/boundary structure** changes:

- a registered owner/map moves;
- a repository/boundary role changes materially;
- a new project-index navigation surface becomes the preferred route;
- the agent bootstrap/ownership architecture changes.

Do **not** update it for:

- sprint numbers;
- active PR/issue lists;
- current main SHA;
- phase completion;
- subsystem maturity values;
- partner/pilot status;
- deployment liveness;
- machine addresses/ports.

Those belong to their owners/live sources. Linking instead of copying is the mechanism that keeps the Atlas useful.
