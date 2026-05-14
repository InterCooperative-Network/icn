# Session Handoff — 2026-05-14 (governed service binding spec)

## Session Goal

Define `GovernedServiceBinding`, `WorkloadManifest`, and `RuntimeProvider` as design-level forward-direction primitives that integrate hosted services, installable tools, compute jobs, CCL evaluators, and future container or microVM workloads under one envelope.

## Decisive Test

Can a downstream implementer — looking only at this spec plus the canon it harmonizes with — name (1) what a binding is and how it relates to `ComputeTask` (ADR-0030) and `ToolBinding` (RFC-0017), (2) which fields each of the three primitives carries at design granularity, (3) which seven runtime classes are closed and what determinism / authority each implies, (4) what authority basis and receipt class each of the ten lifecycle states requires? If yes, the spec has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`ca4bc683f` — `docs(spec): define CCL policy registry and hook contract (#1821)` (merged earlier today).

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| #1822 | `spec/governed-service-binding` | OPEN | All required checks passed on commits `2e0512b35` (initial) and `03da6fbea` (Codex P2 privacy-class fix); a Copilot review-feedback fix commit follows this handoff edit and must re-run CI before merge | Awaiting CI re-run on the Copilot fix commit, then mergeability re-check |
| #1790 | dependabot (pilot-ui dev deps) | OPEN | n/a | Out of session scope |
| #1791 | dependabot (ts-sdk dev deps) | OPEN | n/a | Out of session scope |

### Branches
- `spec/governed-service-binding` — pushed to origin. Last verified head before this fix: `03da6fbea`. The head moves with each push; check `git rev-parse HEAD` on the branch after the next push for the current SHA.
- Local `main` synced to `ca4bc683f` (unchanged this session).

### Issues
- `#1815` — OPEN. This session writes the candidate spec doc. PR will use `Refs:`, not `Closes:`.
- `#1816`, `#1818` — sibling spec issues, OPEN.
- `#1817` — closed (merged via PR #1821).
- `#1794` — OPEN (your manual closure decision deferred).
- `#1797`, `#1793` — closed previously.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Spec doc drafted (`docs/spec/governed-service-binding.md`, ~340 lines)

The doc:

- **Names what the spec is and is not.** Generalization, not redefinition. ADR-0030's `ComputeTask`, RFC-0017's `ToolManifest`/`ToolBinding`, and `icn-compute`'s `Executor` trait remain authoritative for their respective scopes; this spec lifts them into abstract `WorkloadManifest`, `GovernedServiceBinding`, and `RuntimeProvider`.
- **Defines the three integrating primitives at design granularity.** Identity, inputs/outputs, runtime class, custody and privacy and determinism class, fuel limit, capability requirements, disclosure policy — for `WorkloadManifest`. Domain reference, manifest reference, authority context (mandate, decision-receipt, authority classes granted), runtime class pin, custody class, dispatch policy, capability scope, resource allocation, backup-policy reference, exit policy, operator scope, lifecycle state — for `GovernedServiceBinding`. Capability discovery, workload instantiation, constraint enforcement, lifecycle hooks, receipt emission — for `RuntimeProvider`.
- **Names seven closed runtime classes** with determinism and authority expectations: deterministic legitimacy compute, utility computation, container, microVM, accelerator, local device, external bridge. New classes require ADR amendment.
- **Names a ten-state lifecycle** mapped to authority basis and receipt class per transition: declare → authorize → allocate → bind → run → observe → upgrade → suspend → remove → export. Reversal is a separate transition; chain is append-only; exit is constitutional.
- **Maps the spine doc's five-stage maturity progression** (unmodeled, hosted, governed, adapted, native) onto binding-state requirements rather than a separate lifecycle.
- **Specifies the relationship table** to every existing surface: `ComputeTask`, `ToolManifest`/`ToolBinding`, `Executor`, `EvaluatorBinding`, `ServiceIdentity`, `Mandate`/`AuthorityGrant`/`TypedScope`, `EffectManifest`/`KernelEffect`/`EffectOutcome`. Each is named as either a specialized projection of the abstract model or a primitive the binding references.
- **Eight boundary rules** including the meaning-firewall rule (kernel does not consult `GovernedServiceBinding`), the manifest-pin rule (version is fixed at adoption; symmetric with `ccl-policy-registry.md` step 8), the package-vocabulary boundary rule, and the binding-never-widens rule (capability scope, custody class, runtime class — all tightened by the binding, never expanded).
- **Eleven-row failure/safety table** with deterministic responses for manifest version drift, capability scope overrun, runtime class mismatch, missing mandate, custody class violation, provider unavailable, nondeterministic legitimacy compute, dispatch policy violation, disclosure policy violation, indefinite stalled `Authorize`, and operator-removed resources without `Remove`.
- **Cross-links every sibling future-work issue and ADR.** No duplication.

### 2. Registered the doc (`docs/registry.toml`, +13 lines before the `ccl-policy-registry.md` entry)

Same registry pattern as #1819 / #1820 / #1821: `category = "architecture"`, `status = "draft"`, no `truth_class` / `role` overrides (lets the `docs/spec/` prefix default supply `truth_class = "normative"`, `role = "spec"`). Carries `description`, `last_updated`, `last_reviewed`, `owner`, `audiences`, `domain_tags`, `depends_on`, `recommended_action`.

### 3. Added the spec to `docs/INDEX.md` Specifications section

Proactively added before any reviewer surfaces it. INDEX.md was previously updated in #1821 to include the three earlier specs; this PR adds the fourth.

### 4. Regenerated `docs/DOCUMENT_REGISTRY.md`

Ran `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. Generated diff is small: corpus counts updated (markdown files 802 → 803; explicit rows 285 → 286).

### 5. Followed the #1819 / #1820 / #1821 review lessons proactively

- YAML frontmatter `Status: normative`.
- Doc opening labeled "spec, work-in-progress" — honest about which clauses depend on forward-direction primitives.
- MUST claims supported by existing types and behaviors today. Where the abstract model adds new names (`WorkloadManifest`, `GovernedServiceBinding`, `RuntimeProvider`), those are explicitly forward-direction.
- Handoff in HANDOFF_TEMPLATE format with truthful Final State (no provisional language like "PR not yet open" or "expected number").
- Canonical vocabulary used verbatim: `ComputeTask`, `FuelLimit`, `Executor`, `ComputeReceipt`, `ToolManifest`, `ToolBinding`, `Capability`, `Mandate`, `AuthorityClass`, `AuthorityGrant`, `TypedScope`, `EffectManifest`, `KernelEffect`, `EffectOutcome`, `InstitutionalEffectRecord`, `EffectDispatchEvidence`, `DomainPolicy`, `ServiceIdentity`, `ContentHash`, `SemanticVersion`.
- Forbidden vocabulary appears only in the explicit anti-claim sentence in §"Non-claims."

### 6. Drafted follow-up issues (not filed)

Per the established pattern. See §"Next Move" below.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Branch created (`spec/governed-service-binding`).
- [x] Spec doc drafted; registry updated; INDEX.md updated; DOCUMENT_REGISTRY.md regenerated.
- [x] Validation suite passed locally. *(doc_control_check pass with 54 pre-existing/generated-doc warnings; lint-arch CLEAN; compliance_linter clean; freshness exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness.)*
- [x] Initial commit (`2e0512b35`) + push.
- [x] PR #1822 opened.
- [x] Initial CI passed on `2e0512b35`.
- [x] Codex P2 privacy-class fix applied in `03da6fbea` and CI re-passed.
- [x] Copilot review-feedback fix (four items: ComputeTask source path, handoff Final State staleness, runtime-class contradiction, handoff checklist staleness) applied in this session's pending commit.
- [ ] Push the Copilot fix commit and re-run CI.
- [ ] Recheck `mergeStateStatus` after CI returns green.
- [ ] Reply to / resolve the open Copilot threads with the fix commit SHA.
- [ ] Squash-merge if clean.
- [ ] Sync local `main` and prune the branch (squash-merge requires `git branch -D`).
- [ ] Leave `#1815` closure to human review (PR uses `Refs:`, not `Closes:`).
- [ ] Decide whether to file the follow-up issue drafts (six titles named in §"Next Move" below).
- [ ] After `#1815` lands, the natural next architecture spec is `#1816` (backup, replication, recovery, archive policies) per the spine doc's recommended ladder.

### Carried forward (not addressed in this PR)

**`docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`** — AGENTS.md lines 281–313 still say `docs/dev-journal/`; HANDOFF_TEMPLATE.md (line 10, 109–111) and 14+ merged PRs use `docs/dev/`. Surfaced in #1820 review, carried in #1821 handoff. Not in this PR's scope; recommended one-line edit to AGENTS.md.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **ADR-0030's `ComputeTask` definition is what the Explore agent reported.** I treated it as the canonical compute-workload manifest. If a follow-up ADR or PR has amended the field set, my "ComputeTask is the compute-specific projection of WorkloadManifest" framing may need adjustment.
- **RFC-0017's `ToolManifest`/`ToolBinding` are stable.** I treated them as the canonical tool-install primitives. If RFC-0017 has been further amended (it was still under review per `#1709`), the projection framing may shift.
- **`icn-compute`'s `Executor` trait covers what providers need.** The spec frames `RuntimeProvider` as the cross-class generalization of `Executor`. If `Executor` has changed shape recently (e.g., added or removed methods), my generalization may not be accurate.
- **No `WorkloadManifest` / `GovernedServiceBinding` / `RuntimeProvider` types exist in code.** Verified by Explore agent grep of `icn/`. If a parallel in-flight PR is adding any of them, the spec's "forward-direction" framing for those names is wrong.
- **The seven runtime classes from `ICN_INTEGRATED_SYSTEM_MODEL.md` are still the closed set.** I treated them as authoritative. If the spine doc has been amended to add an eighth, the spec needs adjustment.
- **The five-stage maturity progression from `ICN_INTEGRATED_SYSTEM_MODEL.md` is current canon.** I mapped it to binding-state requirements. If `SERVICE_HOSTING_MODEL.md`'s three-stage version is preferred by a reviewer, the mapping table may need to be reframed.
- **The 30 cross-link targets all exist at session start.** Verified by file-existence sweep before committing. If a downstream PR renames or removes one before `#1815` lands, the spec's link will rot.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Push the Copilot fix commit currently being prepared. Watch CI to settle.
2. Reply to / resolve each open Copilot thread on PR #1822 with the new fix commit SHA. The four open Copilot comments are: ComputeTask source path, handoff Final State staleness, runtime-class contradiction, handoff checklist staleness.
3. Re-check `mergeStateStatus` after CI returns green.
4. Squash-merge if clean. Sync local `main`; force-delete the branch (`git branch -D spec/governed-service-binding`) since squash-merge invalidates the ancestor check for `-d`.
5. **Follow-up issue drafts (deferred; not filed in this PR):**
   - `schema(runtime): define GovernedServiceBinding, WorkloadManifest, and RuntimeProvider persisted records` — the wire-stable schema deliberately omitted.
   - `spec(runtime): define generic RuntimeProvider trait` — the cross-class generalization of `Executor` as a Rust trait.
   - `spec(runtime-container): define container workload provider envelope` — container-class provider specification.
   - `spec(runtime-microvm): define stronger-isolation provider envelope` — microVM-class provider specification.
   - `spec(governance): define service-binding stage promotion contract` — the hosted→governed→adapted→native acceptance gates.
   - `spec(privacy): reconcile PrivacyClass naming between ADR-0030 and icn-compute implementation` — added in response to the Codex P2 review on this PR.
6. After `#1815` is closed by the user (if they decide closure is warranted), the next safe architecture spec is `#1816` (backup, replication, recovery, archive policies). Do NOT start `#1816` until the user directs.
7. **Separate process cleanup recommendation (carried from #1820 review):** `docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`. AGENTS.md still says `docs/dev-journal/`; HANDOFF_TEMPLATE.md and 15+ merged PRs use `docs/dev/`. One-line edit. Not in this PR's scope.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **`WorkloadManifest` generalizes; does not replace `ComputeTask` or `ToolManifest`.** Both existing types are specialized projections. Neither needs a code change to align with the abstract model.
2. **`GovernedServiceBinding` generalizes; does not replace `ToolBinding` or `EvaluatorBinding`.** Both are specialized cases. `EvaluatorBinding` (from `ccl-policy-registry.md`) is the CCL-evaluator projection; `ToolBinding` (RFC-0017) is the tool-install projection.
3. **`RuntimeProvider` generalizes `Executor`.** The `icn-compute` `Executor` trait continues to be the compute-class provider implementation. Other classes (container, microVM, accelerator, local device, external bridge) get analogous traits at the same level.
4. **The seven runtime classes are closed.** New classes require an ADR amendment, matching how new effect kinds are gated (ADR-0025) and new authority classes are gated (ADR-0014).
5. **Manifest version is pinned at adoption.** Upgrade is a governance act — symmetric with `ccl-policy-registry.md` step 8 ("registry-level supersession is informational; only governance adoption changes the version a binding uses").
6. **Maturity stages are not a separate lifecycle.** Hosted, governed, adapted, native are constraints on what the binding has bound, applied to the same ten-state lifecycle.
7. **A binding never widens what the manifest declared.** Capability scope, custody class, runtime class — the manifest is the upper bound; the binding is the actual grant.

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
python3 docs/scripts/lint-arch.py docs/spec/governed-service-binding.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Review thread state
gh api repos/InterCooperative-Network/icn/pulls/<pr-number>/comments \
  --jq '.[] | {id, user: .user.login, line, in_reply_to_id, created_at}'
```

Expected: doc_control_check pass with 54 enforcement warnings (none on the new doc); lint-arch CLEAN; compliance_linter no violations; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness.

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/STATE.md` and `docs/PHASE_PROGRESS.md` (last reviewed 2026-05-07). No conflict surfaced.
- **Implementation truth:** verified by Explore agent reading ADR-0030 (`ComputeTask` shape), RFC-0017 (`ToolManifest`/`ToolBinding` shape), `icn-compute` module surface (no `WorkloadManifest`/`GovernedServiceBinding`/`RuntimeProvider` types), and the three merged sibling specs. Cross-link existence sweep verified 30/30 paths present.
- **Execution truth:** confirmed `main` HEAD `ca4bc683f`, two dependabot PRs open, no other open PRs. `#1815` confirmed OPEN with the body matching the user's prompt.
- **Known truth-plane conflict:** `freshness-check.py` reports `docs/ARCHITECTURE.md` as carrying stale sections. CI workflow treats this as informational and reports SUCCESS; the script itself exits 1. Same pattern as #1814, #1819, #1820, #1821.
- **Known process drift (carried, not in this PR):** AGENTS.md vs `docs/dev/HANDOFF_TEMPLATE.md` disagree on handoff path. De-facto canon is `docs/dev/handoff-*.md`. Recommend separate scoped fix.
