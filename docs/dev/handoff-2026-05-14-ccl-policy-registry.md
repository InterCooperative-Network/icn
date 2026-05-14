# Session Handoff — 2026-05-14 (CCL policy registry and hook contract)

## Session Goal

Define the CCL policy registry, versioning convention, adoption contract, and evaluator-selection contract — the design-level bridge between `DomainPolicy` (adopted CCL policy references, per the merged institutional-domain spec) and the Stage 2 / Stage 3 CCL hook points (per the merged effect dispatch spec).

## Decisive Test

Can a downstream implementer — looking only at this spec plus the canon it harmonizes with — name (1) what a policy registry entry vs a policy version is, (2) how a domain adopts a specific version through governance, (3) how the runtime resolves a proposal kind to exactly one evaluator (and what fail-closed states exist), (4) what shape an evaluator returns and how that flows into Stage 2/3 of the dispatch chain? If yes, the spec has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`100ecdbf7` — `docs(spec): define institutional domain and policy primitive (#1820)` (merged earlier today).

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| #1821 (this session's, expected number after `gh pr create`) | `spec/ccl-policy-registry-hooks` | OPEN after push | About to be opened; CI runs on PR creation | Awaiting initial CI |
| #1790 | dependabot (pilot-ui dev deps) | OPEN | n/a | Out of session scope |
| #1791 | dependabot (ts-sdk dev deps) | OPEN | n/a | Out of session scope |

(The PR number above is **provisional** until `gh pr create` returns. After creation, the row reflects truth.)

### Branches
- `spec/ccl-policy-registry-hooks` — local; about to be pushed. Initial head is the single spec-commit on this branch; check `git rev-parse HEAD` after push for the SHA.
- Local `main` synced to `100ecdbf7` (unchanged this session beyond branching).

### Issues
- `#1817` — OPEN. This session writes the candidate spec doc. PR will use `Refs:`, not `Closes:`.
- `#1794`, `#1815`, `#1816`, `#1818` — sibling spec issues, all OPEN.
- `#1797` — closed (merged via #1819).
- `#1793` — closed (merged via #1814).

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Spec doc landed locally (`docs/spec/ccl-policy-registry.md`, ~290 lines)

The doc:

- **Names the bridge problem.** ADR-0021 froze CCL safety properties, ADR-0022 froze the schema-bridge doctrine, the merged dispatch and domain specs say *where* CCL evaluators participate. What was missing — and what `#1817` exists to define — is the bridge between `DomainPolicy` (adopted policy refs) and the Stage 2/3 hook points: a registry, an adoption contract, and an evaluator-selection contract.
- **Establishes seven boundary lines.** CCL document vs adopted CCL policy version; draft policy vs adopted policy; policy registry vs `DomainPolicy`; CCL evaluator vs runtime dispatcher; CCL output vs authoritative mutation; model/agent-drafted text vs governance-adopted policy; institution-package vocabulary vs ICN core grammar.
- **Specifies the policy registry at object granularity.** Identity (policy id + stable address), descriptive metadata (title / summary / class), lifecycle status (draft / proposed / adopted / superseded / deprecated), versioning (version list + per-version metadata), review and audit fields, bindings (adopting domains, linked proposal kinds, evaluator binding references), output expectations (receipt classes, audit/export visibility).
- **Specifies the policy version model.** Stable version id combining existing `ContentHash` and `SemanticVersion` types from `icn-ccl/src/registry.rs`; authorship/review provenance; adoption proposal reference; effective date / activation condition; supersession/deprecation relation; compatibility with `DomainPolicy`; rollback/reversal/challenge relation; receipt/provenance references.
- **Specifies an eight-step adoption contract** that maps onto the existing dispatch chain (proposal created → review path → vote/decision → `GovernanceDecisionReceipt` → `DomainPolicy` update → adoption receipt → member/operator visibility → challenge/reversal path). Every step references an existing stage in `effect-dispatch-contract.md`.
- **Specifies a deterministic evaluator-selection contract.** Selection input: `(domain_id, proposal_kind, current DomainPolicy snapshot)`. Selection output: a single `EvaluatorBinding`. Eight-step procedure with explicit fail-closed semantics for zero bindings, multiple bindings, deprecated versions, and superseded-without-hold cases. **Kernel never consults the registry.**
- **Specifies evaluator output → effect plan shape.** Structured envelope: decision suggestion (advisory), reasons / rule references, structured effect plan candidate, disclosure policy, receipt expectations, challengeability metadata, required authority basis, unresolved questions / review gates. Names how each piece flows into Stage 2 (mandate composition) and Stage 3 (effect plan) hook points; reaffirms that CCL runs in neither Stage 4 nor Stage 5.
- **Specifies review and audit surfaces.** Registry surfaces drafts, adopted, superseded, amendment history; members see plain-language rules; operators see evaluator bindings and failures; receipts show which policy version was applied; exports carry `(policy_id, version_id, evaluator_id, decision_hash)` provenance; accessibility and translation are legitimacy requirements.
- **Specifies a ten-row failure/safety table.** Every failure mode named in the user's brief (unadopted policy, missing version, conflicting policies, unknown proposal kind, evaluator unavailable, nondeterministic output, model-generated unadopted text, package nouns leaking, private data in evaluator I/O, stale references) has a deterministic response. The cross-cutting rule: **fail closed, surface honestly, never silently fall back.**
- **Cross-links every sibling future-work issue and ADR.** No duplication.
- **Names nine open questions** with their forward homes.

### 2. Registered the doc (`docs/registry.toml`, +13 lines before the `institutional-domain.md` entry)

Same registry pattern as PR #1819 / PR #1820: `category = "architecture"`, `status = "draft"`, no `truth_class` / `role` overrides (lets the `docs/spec/` prefix default supply `truth_class = "normative"`, `role = "spec"`). Carries `description`, `last_updated`, `last_reviewed`, `owner`, `audiences`, `domain_tags`, `depends_on`, `recommended_action`.

### 3. Followed the #1819 / #1820 review lessons proactively

- YAML frontmatter `Status: normative` (matches prefix default).
- Doc opening labeled "spec, work-in-progress" — honest about which clauses are forward-direction.
- MUST claims are limited to what existing types + behaviors can support today. Where a stronger contract requires schema fields that do not exist (e.g., `EvaluatorBinding` as a wire-stable record, `Adopting domains` index on registry entries), the spec marks the gap as forward-direction.
- Handoff written in `docs/dev/HANDOFF_TEMPLATE.md` structure with truthful Final State (no "PR not yet open" or "head pending commit" rows that age the moment the PR opens).
- Used canon vocabulary verbatim (`ContentHash`, `SemanticVersion`, `CclDocument`, `SchemaVersion`, `Capability`, `ConstraintSet`, `GovernanceDecisionReceipt`, `GovernanceProof`, `GovernanceDecisionAttestation`, `Mandate`, `AuthorityGrant`, `AuthorityClass`, `TypedScope`, `EffectManifest`, `KernelEffect`, `EffectOutcome`, `InstitutionalEffectRecord`, `EffectDispatchEvidence`, `DomainPolicy`).

### 4. Drafted follow-up issues (not filed)

Per the user's instruction. See "Next Move" §5.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Run validation suite. *(All four scripts: doc_control_check pass with 53 pre-existing unrelated warnings; lint-arch CLEAN; compliance_linter clean; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness.)*
- [x] Branch + initial commit.
- [ ] Push branch and open PR.
- [ ] Watch CI on the initial head.
- [ ] Reply to any AI-reviewer threads (Copilot / Codex) using the verified-rebuttal pattern from #1819 / #1820.
- [ ] Recheck `mergeStateStatus` and merge if clean. *(The user's prompt for #1817 says "After opening PR ... Stop and summarize." Merging is not directed in the same prompt.)*
- [ ] Decide whether to file the follow-up issue drafts listed in §"Next Move §5" (deferred to user).
- [ ] After `#1817` lands, the next safe spec is `#1815` (governed service binding / workload manifest / runtime provider). The user explicitly asked not to start it this session.

### Carried forward (not addressed in this PR)

**AGENTS.md vs `docs/dev/HANDOFF_TEMPLATE.md` drift.** Surfaced by Codex on PR #1820 (#1820's earlier handoff discussion). AGENTS.md lines 281–313 say handoffs go to `docs/dev-journal/session-YYYY-MM-DD.md`; HANDOFF_TEMPLATE.md lines 10 / 109–111 say `docs/dev/handoff-YYYY-MM-DD.md`. The `docs/dev-journal/` directory does not exist. The de-facto canon (this template + 10+ merged PRs) is `docs/dev/handoff-*.md`. Suggested separate scoped fix: `docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`. **Not addressed in this PR** to keep scope tight; flagged here so the user can decide whether to file the cleanup PR.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`icn-ccl/src/registry.rs` carries `ContentHash` (blake3, 32 bytes) and `SemanticVersion`.** Confirmed by Explore agent reading the file headers. I did not exhaustively read both types' implementations; if either has changed shape since the agent's read this session, the spec's "reuse the existing types" framing is wrong.
- **No `CCLPolicy` / `PolicyVersion` / `PolicyRegistry` / `EvaluatorBinding` types exist in code today.** Verified by Explore agent grep of `icn/`. If a parallel in-flight PR is adding any of these, the spec's "forward-direction" framing for those names is wrong.
- **The current `Capability` enum in `icn-ccl/src/types.rs` covers what evaluators need.** Spec references `Capability` as a fact in code; I did not exhaustively check whether every capability the evaluator-selection contract names is enumerated.
- **The merged `effect-dispatch-contract.md` and `institutional-domain.md` are unchanged on `main` as of session start.** I read them post-merge but did not diff against the most recent main HEAD. If an in-flight follow-up amended either, the spec's cross-links may point at moved sections.
- **ADR-0021 / ADR-0022 / ADR-0023 are still `accepted` and not amended.** The Explore agent reported all three as accepted-with-implementation; I did not check for in-flight amendment PRs.
- **The 29 cross-link targets (12 docs + 9 ADRs + 8 source files) all exist at session start.** Verified by file-existence sweep before committing. If a downstream PR renames or removes one of them before `#1817` lands, the spec's link will rot.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Push the branch and open PR. Body must explicitly state `Refs: #1817` and "does not start runtime/schema/grammar implementation."
2. Watch CI on the initial head. Expected: all required checks pass; non-required jobs skip (docs-only).
3. Reply to any AI reviewer threads using the verified-rebuttal pattern from #1819 / #1820. For each comment: confirm or disconfirm against the live repo before applying.
4. Per the user's `#1817` prompt: **"Stop and summarize"** after the post-CI review pass. The user did not direct merge in the same prompt. Wait for merge instruction.
5. **Follow-up issue drafts (deferred; not filed in this PR):**
   - `schema(ccl): define CCLPolicyRegistry and PolicyVersion records` — the wire-stable schema this spec deliberately omits.
   - `spec(ccl): define evaluator binding and deterministic execution envelope` — fuel budget per evaluator class, capability set per evaluator class, in-process evaluator lifecycle.
   - `spec(governance): define policy adoption / amendment proposal lifecycle` — the closed taxonomy of proposal classes that adopt or amend CCL policy versions.
   - `ux(member): surface adopted policy versions and plain-language rule explanations` — possibly within `#1818` scope; flagged for triage.
6. **Separate process cleanup recommendation (carried from #1820 review):** `docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md` — one-line edit to `AGENTS.md` § "Agent handoff protocol." Not in this PR's scope.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **No new primitives introduced by this spec.** Every type the spec references already exists in `icn-ccl` or `icn-governance`. New names (`CCLPolicyRegistry`, `PolicyVersion`, `EvaluatorBinding`) are explicitly marked as forward-direction.
2. **Selection fails closed.** No silent default evaluator, no silent "latest version" fallback, no silent execution of deprecated policy. Each failure mode has a named, deterministic response.
3. **CCL runs in Stages 2 and 3 only.** Stage 1 (decision recording), Stage 4 (kernel dispatch), and Stage 5 (application + evidence) do not invoke CCL evaluators. The meaning firewall stays intact.
4. **Adoption is a governance act with full receipt provenance.** Adoption never happens silently; the registry's `Adoption history` is the institutional record.
5. **Model output is never authoritative.** Models or agents may draft CCL text; adoption is a governance act; the registry treats drafts and adopted versions as fundamentally distinct.
6. **Determinism is non-negotiable.** Per ADR-0021, non-deterministic evaluator output is a bug, not a tunable. Quarantine + deprecate is the response.

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn

# Branch + PR state
git branch --show-current
gh pr view <pr-number> --json state,mergeable,mergeStateStatus

# Validation suite
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 docs/scripts/lint-arch.py docs/spec/ccl-policy-registry.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Review thread state
gh api repos/InterCooperative-Network/icn/pulls/<pr-number>/comments \
  --jq '.[] | {id, user: .user.login, line, in_reply_to_id, created_at}'
```

Expected at session start before push: doc_control_check pass with 53 pre-existing unrelated warnings; lint-arch CLEAN (already verified); compliance_linter no violations; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness (recorded honestly, not in this PR's scope).

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/STATE.md` and `docs/PHASE_PROGRESS.md`. No conflict surfaced.
- **Implementation truth:** verified by Explore agent reading `icn-ccl/src/registry.rs`, `schema/mod.rs`, `schema/version.rs`, `schema/bridge.rs`, `types.rs`; spot-confirmed by the session-start file-existence sweep. No `CCLPolicy` / `PolicyVersion` / `PolicyRegistry` / `EvaluatorBinding` types exist in code today.
- **Execution truth:** confirmed `main` HEAD `100ecdbf7`, two dependabot PRs open, no other open PRs. `#1817` confirmed OPEN with the body that matches the user's prompt.
- **Known truth-plane conflict:** `freshness-check.py` reports `docs/ARCHITECTURE.md` as carrying stale sections (06-identity-cryptography, 10-state-ledger, 11-contract-execution, 12-governance, 14-federation). CI workflow treats this as informational and reports SUCCESS; the script itself exits 1. This PR does not broaden scope to address it.
- **Known process drift (carried, not in this PR):** AGENTS.md vs `docs/dev/HANDOFF_TEMPLATE.md` disagree on handoff path. De-facto canon is `docs/dev/handoff-*.md`. Recommend separate scoped fix.
