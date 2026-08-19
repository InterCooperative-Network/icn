# AGENTS.md

Provider-neutral operating contract for coding agents working in this repository.

This file is intentionally small and stable. It owns **agent operating invariants**, not current project state. Do not put sprint numbers, PR numbers, deployment claims, crate counts, current blockers, or other volatile facts here.

For the architecture of the agent context system, read [`docs/ai/WORKFLOW_ARCHITECTURE.md`](docs/ai/WORKFLOW_ARCHITECTURE.md).

## 1. Start from the question, then resolve its owner

ICN does not have one giant document that wins every disagreement. Different questions have different owners.

Start with [`ops/state/truth/sources.json`](ops/state/truth/sources.json). It maps important fact domains to their canonical owners. [`docs/ATLAS.md`](docs/ATLAS.md) is the human-readable ecosystem index, not a competing truth root.

Use this rule:

| Question | Source to trust |
|---|---|
| What does the current checkout implement? | Current code, tests, schemas, and generated behavior in this checkout |
| What semantics are allowed or intended for a domain? | The domain owner named by `ops/state/truth/sources.json`, plus applicable accepted ADRs |
| What is the repository/worktree topology? | `ops/state/config/repo-map.json` |
| What is being worked on right now? | The live issue/PR control surface via `gh`. Never a sprint file, a handoff, or a cached prompt |
| Is a sprint cadence running, and what is the sprint board lineage? | `ops/state/sprint/current.json` |
| What are the merge requirements? | `ops/state/truth/policy.json` plus live branch protection |
| Which agent or skill owns a workflow? | `ops/state/truth/agents.json` and `ops/state/truth/skills.json` |
| What is the current branch, PR, review, or CI state? | Live `git` and GitHub data. Never a handoff or cached prompt |
| What happened previously? | Git history, issues/PRs, and handoffs as historical evidence |

A normative document does not make code implemented. Existing code does not silently override a normative contract. If implementation and the domain owner conflict, report a **truth conflict** and determine whether the defect is in code, documentation, or the ownership map. Do not blend them into a convenient answer.

## 2. Bootstrap before non-trivial work

For anything beyond a tiny typo or mechanical one-line edit:

1. **Verify the checkout.**
   - `git rev-parse --show-toplevel`
   - `git branch --show-current`
   - `git status --short`
   - `git fetch origin --prune` when network access is available and freshness matters.
2. **Bound the task.** State the requested outcome and what is explicitly out of scope.
3. **Resolve truth owners.** Read `ops/state/truth/sources.json`, then only the owners relevant to this task.
4. **Refresh volatile state live.** Query the relevant issue/PR/reviews/checks. Do not reconstruct live state from session memory.
5. **Load path context.** Prefer `python3 scripts/generate-agent-context-spine.py --brief <paths>` when it can narrow the relevant crates, docs, checks, and boundaries.
6. **Verify the claimed gap.** Run or inspect the smallest evidence that proves the problem still exists.
7. **Plan before mutation.** For architectural, security-sensitive, migration, or cross-cutting work, brief the maintainer on the live state and bounded action before editing.

The session frame in `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md` is a compact way to record these decisions. It is grounding, not ceremony.

## 3. ICN invariants

These five invariants are owned here and must be preserved across every change.

### Adversarial by default
Treat peers, inputs, replicated state, credentials, and external claims as untrusted until the relevant protocol establishes otherwise. Do not create convenience paths that silently bypass authentication, authorization, validation, or provenance.

### Determinism
Protocol state transitions, canonical derivations, proofs, hashes, and conflict handling must not depend on arrival order, local clock, map iteration order, process-local randomness, or another hidden selector unless the protocol explicitly authorizes that source of variation.

### Canonical encodings
Wire formats, signed bytes, hash domains, proof formats, event identities, and persisted key encodings are compatibility surfaces. Do not change them accidentally. If a change is intentional, name the migration/compatibility consequence and pin it with tests.

### No panics in protocol paths
Network, protocol, actor-runtime, storage-decoding, and untrusted-input paths return explicit errors. Do not use panic as validation or recovery behavior.

### Meaning Firewall and kernel/app boundary
The generic substrate enforces structure, cryptographic evidence, scope, constraints, and protocol state. Institution-specific meaning belongs in apps, governance packages, policy oracles, and charters. Do not smuggle domain semantics into the kernel or make dependency direction imply institutional sovereignty.

The source-linked invariant catalog is `docs/reference/project-index/invariants-catalog.md`. It indexes evidence; it does not create a second set of invariants.

## 4. Scope and mutation discipline

- The maintainer's requested scope is a hard boundary.
- Prefer one bounded semantic change per PR.
- Do not turn a failing check into permission for unrelated cleanup.
- Do not weaken validation, security, branch protection, tests, or invariant checks to make a branch green.
- Do not silently upgrade toolchains or dependencies.
- Do not perform deployment, release, migration/cutover, branch-protection changes, destructive infrastructure actions, or merges without the authorization appropriate to that action.
- When a selected implementation slice appears to require changing a settled contract, stop and report the contract conflict instead of widening the implementation quietly.

## 5. Verification is path- and claim-specific

Do not carry a giant static command list in memory. Determine verification from:

1. the changed paths;
2. the relevant domain owner and scoped instructions;
3. the Agent Context Spine path brief;
4. the repository's CI/merge policy.

General examples:

- Rust work starts from the repository root and runs Cargo from `$(git rev-parse --show-toplevel)/icn`.
- Documentation changes use the doc-control machinery under `docs/scripts/` and the checks named by the affected truth/source files.
- Website changes use `just website-verify`.
- API changes must preserve generated OpenAPI/type consistency.

The universal documentation-control entrypoints are intentionally recorded **once here**, not duplicated across provider adapters:

```bash
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
python3 .github/scripts/compliance_linter.py --repo-root .
python3 .github/scripts/readiness_overclaim_linter.py --repo-root .
```

These are navigation rules, not frozen CI policy. `ops/state/truth/policy.json` and live CI own merge requirements.

## 6. Memory is not authority

Session handoffs, chat summaries, local notes, and model memory are **ephemeral memory surfaces**.

They may contain useful evidence, rationale, command output, and resume pointers. They may not establish current truth for a future session.

A future agent must reverify every volatile claim, including:

- branch/head state;
- PR status and mergeability;
- reviews and unresolved threads;
- CI/check state;
- issue status;
- deployment/runtime state;
- selected next work when the control surface may have changed.

When a session discovers something that must survive, promote it to the correct durable surface:

- protocol/architecture decision -> domain owner or ADR;
- current operational/task state -> its machine-readable owner or live issue/control surface;
- new defect/follow-up -> issue;
- reproducible behavior -> test;
- navigation metadata -> generated index/registry source.

If it exists only in a handoff, it is not durable project truth.

## 7. Generated artifacts are projections

Do not hand-edit generated project indexes, live overlays, website projections, or other files that name their source and generator. Change the owner/source, regenerate, and review the resulting diff.

Generated artifacts can help answer **where to look**. They do not outrank their inputs.

## 8. Specialist agents and provider adapters are thin overlays

Specialist prompts should define:

- scope and routing triggers;
- which truth domains to load;
- domain-specific review questions;
- verification hints that are stable enough to be useful.

They should not duplicate identity models, current architecture state, live issue lists, merge-check sets, deployment topology, or other facts already owned elsewhere.

`CLAUDE.md`, `docs/ai/CODEX_WORKFLOW.md`, `.claude/**`, and similar provider surfaces adapt this contract to a tool. They do not override it.

## 9. Completion report

Before claiming completion, state:

- the final base/head or relevant live state;
- what changed and what deliberately did not;
- the evidence run or inspected;
- any unresolved assumptions or external failures;
- whether the work is implementation, test/library-only, integration, migration, deployment, or documentation-only;
- the exact next blocker if the task did not reach its acceptance condition.

Do not turn "implemented" into "integrated," "deployed," "production-ready," or "adopted" without evidence for those separate claims.
