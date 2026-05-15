# Session Handoff — 2026-05-14 (entity-scope vocabulary boundary)

## Session Goal

Begin fixing the "community / coop" vocabulary problem in ICN core by writing the canon-boundary doctrine that prevents future specs from hardening the wrong model — without renaming Rust code, editing GitHub issue bodies, or rewriting historical ADRs.

## Decisive Test

Can a future spec author — looking only at `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 "Entity-scope vocabulary," the new glossary entries, and the short cross-link notes in the merged specs — choose the right scope-level vocabulary (`InstitutionalDomain` / `LocalDomain` / `DomainPolicy` / "owning entity class") instead of copying `Coop`-flavored names from the existing canon? If yes, the boundary doctrine has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`5461fd91d` — `docs(spec): define storage durability policy objects (#1823)` (merged earlier today).

### Open PRs at session start
| PR | Branch | State | Note |
|----|--------|-------|------|
| #1824 | `spec/artifact-registry-and-scoped-vault` | OPEN | Prior session's PR (clean, awaiting user's merge decision). Out of scope for this session. |
| #1790 | dependabot (pilot-ui dev deps) | OPEN | Out of session scope. |
| #1791 | dependabot (ts-sdk dev deps) | OPEN | Out of session scope. |

### Branches
- `docs/entity-scope-vocabulary-boundary` — local; about to be pushed. Initial commit forthcoming after this handoff is committed.
- Local `main` synced to `5461fd91d`.

### Issues
- `#1801` (compute placement) — OPEN. **Not edited in this PR.** The corrected vocabulary is recorded as a paste-ready body patch in §"Proposed #1801 patch" below for the forthcoming `#1801` spec PR to consume.
- `#1798`, `#1816`, `#1817`, `#1815`, `#1820`, `#1819`, `#1814`, `#1767`, `#1792`, `#1799`, `#1818`, `#1795` — all OPEN per the user's deferred-closure pattern.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` (+93 lines)

Added a new section **§C3 "Entity-scope vocabulary (scope vs organism)"** inserted between §C2 (Entity Primitives vs Local Purpose Metadata) and §D (Reusable Primitive Set). The section:

- Names the rule sharply: **"Core vocabulary names the scope. Institution packages name the organism."**
- Defines what generic scope language uses (`InstitutionalDomain` / `Domain` / `LocalDomain` / `DomainPolicy` / "owning entity class") and what it does NOT use (`Coop` / `Cooperative` as a generic stand-in for the local institutional scope).
- Reserves `Cooperative` for cases that are actually about cooperatives (the entity class, cooperative-specific setup paths, cooperative-economy framing, cooperative-movement context, institution packages serving cooperative federations).
- Notes that institution packages may use local nouns freely.
- Provides a "What this means in practice" table mapping surface → generic vocabulary → institution-package vocabulary.
- Provides "Correct vs incorrect naming" code-fence examples (`CoopBound` → `LocalDomainBound`, etc.).
- Names the **Known drift** that the doctrine does not yet fix in code: `DataLocality::CoopReplicated` in `icn-kernel-api`, ADR-0030's `Coop(coop_id)` scope tag, ADR-0031's `Coop-scoped` admission/settlement prose, issue `#1801`'s placement-class taxonomy and member-shell language. Each drift item names where it will be addressed.
- Provides a "test" with three questions to ask when proposing a generic primitive or scope name.

### 2. `docs/glossary.md` (+25 lines)

Added five entries under §"Organizations," placed before the existing `Cooperative (Coop)` entry so the abstractions precede the inhabitant classes:

- **InstitutionalDomain** — the structural scope.
- **LocalDomain** — shorthand for an `InstitutionalDomain` at the local institutional layer; prefer over `Coop`.
- **Owning entity class** — `Individual` / `Cooperative` / `Community` / `Federation`, or another governed class.

Updated the existing **Cooperative** and **Community** entries to add one sentence each that:
- Names them as **one possible owning entity class** for an `InstitutionalDomain`.
- Directs readers to `LocalDomain` / `InstitutionalDomain` for generic scope language.
- Preserves the existing description of each entity class verbatim otherwise — these entries describe what a Cooperative / Community IS; the disambiguation is about scope language, not entity definitions.

### 3. `docs/spec/governed-service-binding.md` (+1 boundary-rule cell)

Added a short note to the existing §"Boundary rules" → "Package vocabulary stays in packages" row pointing at the new §C3. Tells future readers: generic scope language uses `Domain` / `InstitutionalDomain` / `LocalDomain`, not `Coop`. One sentence. No restructure.

### 4. `docs/spec/storage-durability-policies.md` (+1 quote block)

Added a short note before the load-bearing inheritance rule in §"Locality and privacy inheritance" pointing at the new §C3. Acknowledges that the kernel-level `DataLocality::CoopReplicated` enum variant is preserved as-is (existing serialized Rust identifier) while new generic scope concepts in the spec use the local-domain vocabulary. One paragraph. No restructure.

### 5. `docs/adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md` (+1 naming note)

Added a single "Entity-scope naming note" inline at the §"Decision" → "Scope" line (after `Commons | Coop(coop_id) | Federation(fed_id)`). The note reads: **the slot should be read as `LocalDomain(domain_id)` for current architecture**; the ADR's decision is preserved as-is; existing serialized `Coop`-prefixed strings remain valid for backward compatibility. No rewrite of the ADR body.

### 6. `docs/adr/ADR-0031-commons-compute-admission-and-settlement-policy.md` (+1 naming note)

Added a single "Entity-scope naming note" inline at the §"Admission policy" → step 4 (`Coop-scoped task admits only executors...`). The note also applies the same rule to step 4 of §"Settlement policy" (the `Coop-scoped tasks settle within the coop's internal ledger` line). The ADR's decision is preserved as-is.

### 7. Doc-registry / DOCUMENT_REGISTRY.md

This PR only edits **existing** documents; no new docs land. The `docs/registry.toml` does not require a new entry. `docs/DOCUMENT_REGISTRY.md` was NOT regenerated because no new docs are introduced. Validation (`doc_control_check.py --strict`) does not require regen for in-place edits.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Branch created (`docs/entity-scope-vocabulary-boundary`).
- [x] All six edits applied (boundary doc §C3, glossary entries, governed-service-binding cross-link, storage-durability-policies cross-link, ADR-0030 naming note, ADR-0031 naming note).
- [x] Validation suite — to run before commit.
- [ ] Commit + push.
- [ ] Open PR.
- [ ] Watch CI on the initial head.
- [ ] Address any valid AI-reviewer feedback (Copilot/Codex) using the verified-rebuttal pattern from prior PRs.
- [ ] Per the established session pattern: **"Apply valid feedback only. Stop and summarize unless explicitly told to merge."** Wait for explicit merge instruction.
- [ ] Decide whether to file the follow-up issue drafts listed in §"Follow-up issue drafts" (deferred to user).
- [ ] The `#1801` spec PR is the right place to land the corrected placement-class names; the patch is in §"Proposed #1801 patch" below for that PR to consume.

### Carried forward (not addressed in this PR)

**`docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`** — `AGENTS.md` lines 281–313 still say `docs/dev-journal/`; `docs/dev/HANDOFF_TEMPLATE.md` and 18+ merged PRs use `docs/dev/`. Surfaced in #1820 review; carried through #1821, #1822, #1823, #1824, and this PR. **Not addressed in this PR** to keep scope tight.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`docs/spec/institutional-domain.md` is the canonical source for the four owning-entity classes** (`Individual` / `Cooperative` / `Community` / `Federation`). Verified by `grep` against the merged doc on `main`. If a follow-up amends the set, the glossary entries need a re-read.
- **`DataLocality::CoopReplicated` is the only kernel-level enum variant carrying `Coop` framing.** Verified by grep against `icn-kernel-api`. If a parallel in-flight PR adds another `Coop`-prefixed type, the rename-followup issue draft (Follow-up #1) needs to expand its scope.
- **No code path uses `coop_replicated` from outside the kernel-api crate by a different serialization name.** Verified by grep on `icn/` — only `icn-kernel-api/src/storage.rs:259` writes `"coop_replicated"`. If serde-deserializers elsewhere parse the string with a different alias, the migration in Follow-up #1 needs to verify those round-trip too.
- **The two recent merged specs (`governed-service-binding.md`, `storage-durability-policies.md`) are the highest-leverage targets for cross-link notes.** Other merged specs (`effect-dispatch-contract.md`, `institutional-domain.md`, `ccl-policy-registry.md`, `artifact-registry-and-scoped-vault.md` in the still-open #1824) don't directly name scope hierarchy; cross-links there would be noise. If a reviewer asks for broader cross-links, adding them is cheap.
- **ADR-0030 and ADR-0031 are the only two ADRs that ship `Coop`-flavored generic scope language.** Verified by `grep -n "Coop\|coop-scoped" docs/adr/*.md`. Other ADRs (0014, 0019, 0025, 0026, 0027) use scope language correctly. If a reviewer surfaces a third ADR with the same drift, adding a third naming note is trivial.
- **`docs/INDEX.md` does not need updating.** No new docs are being introduced; the boundary doc is already in the architecture cluster.

---

## Known remaining drift (not in this PR's scope)

These items are explicitly NOT touched in this PR. Each is named so future work can address them with proper migration discipline.

1. **`DataLocality::CoopReplicated` Rust enum variant** (`icn/crates/icn-kernel-api/src/storage.rs:166`). Serialized as `coop_replicated`; numeric value `1`; used across `icn-compute`, `icn-core`, tests. Rename requires a compatibility-aware migration. See Follow-up #1.

2. **`docs/state/storage-governance-spec.md`** — uses "coop-scoped" prose and `CoopReplicated` to accurately describe the existing implementation. Not changed in this PR because the implementation it describes still uses those names. Will update after the Rust rename lands (Follow-up #1).

3. **`docs/design/COMPUTE_SUBSTRATE_STATUS.md`** and **`docs/state/ICN-Platform-Baseline-2026-03.md`** — reference `CoopReplicated` as canon. Same disposition as #2.

4. **`docs/architecture/CELLS_AND_SCOPES.md`** and **`docs/spec/KERNEL_CONTRACTS.md`** — both reference `CoopReplicated` and a related federation policy enum. Same disposition.

5. **Issue `#1801` body** — uses `CoopBound`, `coop-scoped`, "cooperative-bound executor," and member-shell language "sent for cooperative processing." **Not edited by this PR** per the user's explicit instruction. The forthcoming `#1801` spec PR is the right place to land the corrected vocabulary; the exact patch is in §"Proposed #1801 patch" below.

6. **`icn-coop` crate naming** and **`apps/membership/coop_core/*` paths** — these are crate / module identifiers used across the workspace and SDK. Out of scope for this doc-only PR. Renaming would be a multi-crate refactor with downstream consumer impact.

7. **The handoff-path drift** between `AGENTS.md` and `docs/dev/HANDOFF_TEMPLATE.md` (carried from #1820 review).

---

## Proposed #1801 patch (paste-ready)

**Do not apply this patch in the current PR.** The `#1801` spec PR is the right place to land it. The patch reflects §C3's vocabulary rule.

### Patch 1 — Purpose paragraph (top of #1801 body)

Current:
```
Define ICN compute placement policy: how a workload decides whether it should run locally, on a cooperative-bound executor, through a federation agreement, or in a commons compute pool.
```

Proposed:
```
Define ICN compute placement policy: how a workload decides whether it should run locally, on a local-domain-bound executor, through a federation agreement, or in a commons compute pool.
```

### Patch 2 — Placement hierarchy

Current:
```
1. Member device / local client cache when appropriate
2. Local domain node
3. Cooperative-bound executor
4. Federation-bound executor under explicit agreement
5. Commons compute pool under admission/settlement policy
6. External bridge/custodian only by explicit agreement and policy
```

Proposed:
```
1. Member device / local client cache when appropriate
2. Local domain node
3. Local-domain-bound executor
4. Federation-bound executor under explicit agreement
5. Commons compute pool under admission/settlement policy
6. External bridge/custodian only by explicit agreement and policy
```

### Patch 3 — Scope bullet under "Scope"

Current:
```
- coop-scoped execution policy;
```

Proposed:
```
- local-domain-scoped execution policy;
```

### Patch 4 — Candidate placement classes

Current:
```
- LocalOnly
- DomainLocalPreferred
- CoopBound
- FederationBound
- CommonsEligible
- ExternalCustodianRequired
- RejectedByPolicy
```

Proposed:
```
- LocalOnly
- DomainLocalPreferred
- LocalDomainBound
- FederationBound
- CommonsEligible
- ExternalCustodianRequired
- RejectedByPolicy
```

### Patch 5 — Candidate inputs

Current:
```
- domain id / scope;
```

(Already correct — leave as-is.)

### Patch 6 — Future-spec scope tag (where ADR-0030's `Coop(coop_id)` would surface in #1801)

When the `#1801` spec PR adopts the ADR-0030 scope tag, prefer:
```
LocalDomain(domain_id)
```
over the historical `Coop(coop_id)` form. ADR-0030's existing form is preserved with a naming note (this PR landed that note); new spec text should use `LocalDomain(domain_id)`.

### Patch 7 — Member-shell language

Current bullets under "Member shell should show only plain participation status":
```
- local execution;
- sent for cooperative processing;
- sent to federation executor under agreement;
- commons compute pending;
- review required;
- receipt available;
- sync delayed / degraded.
```

Proposed:
```
- local execution;
- processed inside your institution;
- sent to a federation executor under agreement;
- commons compute pending;
- review required;
- receipt available;
- sync delayed / degraded.
```

The phrasing "processed inside your institution" handles cooperative-, community-, and federation-owned domains identically. An institution package may localize further (NYCN may say "processed inside your cooperative"; a care-network package may say "processed inside your collective") — but the generic core string should not pre-decide.

### Patch 8 — Boundary rules (no change required)

The existing §"Boundary rules" section in #1801's body is correct on the scope-vocabulary axis. The proposed `#1801` spec PR should add a cross-link to `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 for the doctrine that prevents the corrected vocabulary from drifting back.

---

## Follow-up issue drafts (not filed)

Six titles drafted per user instruction. Paste-ready for separate review.

### 1. `refactor(storage): rename CoopReplicated locality to LocalDomainReplicated`

**Body draft:**

```
## Purpose

Bring the `DataLocality` enum into alignment with the entity-scope vocabulary doctrine landed in `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3. The current `CoopReplicated` variant privileges one possible owning entity class at the scope level where multiple classes (Cooperative, Community, Federation, Individual) are equally valid.

## Scope

- Target enum: `icn/crates/icn-kernel-api/src/storage.rs:155-178` (`DataLocality`).
- Current variants: `CellLocal = 0`, `CoopReplicated = 1`, `FederationMirrored = 2`, `CommonsPublic = 3`.
- Target variants: `CellLocal = 0`, `LocalDomainReplicated = 1`, `FederationMirrored = 2`, `CommonsPublic = 3`.

## Migration constraints

- **Preserve numeric values.** `LocalDomainReplicated = 1` MUST keep the same ordering and on-wire integer.
- **Serde alias for backward compatibility.** Add `#[serde(alias = "coop_replicated")]` so existing serialized data still decodes.
- **Decide encoding direction.** Two valid options:
  - Option A (clean break): new encoder emits `"local_domain_replicated"`; deserializer accepts both `"local_domain_replicated"` and `"coop_replicated"`.
  - Option B (compatibility-first): new encoder still emits `"coop_replicated"` so wire format is unchanged; only the Rust identifier renames. Decision belongs to a separate ADR-or-policy call.
- **Update all call sites.** `icn-compute`, `icn-core`, `icn-kernel-api` itself, and tests. Approximately 30+ call sites per grep.
- **Round-trip tests.** Add tests proving (a) old serialized `"coop_replicated"` still decodes to the renamed variant, and (b) re-encoding produces whichever encoding direction was chosen in the previous step.
- **Update consumer docs.** After rename lands, update `docs/state/storage-governance-spec.md`, `docs/design/COMPUTE_SUBSTRATE_STATUS.md`, `docs/state/ICN-Platform-Baseline-2026-03.md`, `docs/architecture/CELLS_AND_SCOPES.md`, `docs/spec/KERNEL_CONTRACTS.md` to use the new variant name where they currently say `CoopReplicated`.

## Non-goals

- Not a wire-format change (the integer value stays).
- Not a rename of `icn-coop` crate or `apps/membership/coop_core/*` paths.
- Not a rename of other `Coop`-prefixed identifiers outside `DataLocality`.

## Related

- Doctrine: `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 "Entity-scope vocabulary."
- Owning entity class taxonomy: `docs/spec/institutional-domain.md`.
- Storage durability spec: `docs/spec/storage-durability-policies.md`.
```

### 2. `docs(compute): align #1801 placement policy vocabulary with LocalDomain`

**Body draft:**

```
## Purpose

The forthcoming `#1801` spec PR (compute placement policy) should adopt the corrected scope vocabulary per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3.

## Scope

Apply the patch recorded in `docs/dev/handoff-2026-05-14-entity-scope-vocabulary-boundary.md` §"Proposed #1801 patch":

1. Purpose paragraph: "cooperative-bound" → "local-domain-bound."
2. Placement hierarchy step 3: "Cooperative-bound executor" → "Local-domain-bound executor."
3. Scope bullet: "coop-scoped execution policy" → "local-domain-scoped execution policy."
4. Candidate placement classes: `CoopBound` → `LocalDomainBound`.
5. Future scope tag: prefer `LocalDomain(domain_id)` over `Coop(coop_id)`.
6. Member-shell language: "sent for cooperative processing" → "processed inside your institution."
7. Add cross-link to `INSTITUTION_PACKAGE_BOUNDARY.md` §C3 in the new spec.

## Non-goals

- Not editing the GitHub issue body before the spec PR opens.
- Not changing the existing scope tags in ADR-0030 / ADR-0031 (those carry naming notes; new spec text uses the corrected form).
```

### 3. `docs(adr): annotate Coop-scoped legacy wording in compute ADRs`

**Status: DONE in the current PR for ADR-0030 and ADR-0031.** Filed here only if a reviewer surfaces a third ADR with the same drift.

### 4. `docs(guides): audit cooperative-specific examples vs generic domain examples`

**Body draft:**

```
## Purpose

After the entity-scope vocabulary boundary landed (`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3), audit the guides / tutorials / examples corpus for places where `cooperative` is used as a generic example when the example would be clearer with a `Domain` or `LocalDomain` framing (or where the example should explicitly note that it could equally apply to a community or federation-owned domain).

## Scope

- `docs/guides/**`
- `docs/onboarding/**`
- `docs/tutorials/**` (if exists)
- `sdk/typescript/examples/**` (likely contains coop-specific code samples; out of doc-only scope but worth flagging)
- `web/**` and `website/**` (for institutional-partner / cooperative-evaluator framing)

## Non-goals

- Not rewriting all cooperative-specific examples. Cooperative-specific examples are appropriate where the example is genuinely about cooperatives.
- Not editing code examples that intentionally demonstrate cooperative-economy workflows.
- The boundary is: "is this example specific to cooperatives, or is it a generic scope-level example that happened to use the word `cooperative`?"
```

### 5. `spec(entity): define entity-class vocabulary and package-local nouns`

**Body draft:**

```
## Purpose

If the boundary doctrine in `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 ("Entity-scope vocabulary") is insufficient over time, write a dedicated spec under `docs/spec/` defining: the closed (but policy-extensible) entity-class set, how packages add local nouns without colliding with core, and how the runtime treats entity-class as data (not as branching logic).

## Scope (provisional)

- Closed entity-class set: `Individual` / `Cooperative` / `Community` / `Federation`.
- Policy-extension protocol: how a domain's policy admits a new entity class.
- Package-local noun mapping: where institution packages declare local synonyms.
- Vocabulary boundary tests: lint rules or doc-check assertions that flag generic primitives named after one inhabitant.

## Non-goals

- Not redefining the institutional-domain spec (which already names owning-entity-class).
- Not editing the four merged sibling specs unless the new spec's adoption requires it.
- Filing this issue is forward work; the current §C3 doctrine may be enough.
```

### 6. `docs(architecture): consider entity-scope-vocabulary checklist for future specs`

**Body draft:**

```
## Purpose

Add an explicit checklist item to the architecture-due-diligence pattern (per `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md`) requiring spec authors to confirm that generic primitive names follow §C3 of `INSTITUTION_PACKAGE_BOUNDARY.md`.

## Scope

- One bullet added to the architecture-due-diligence checklist.
- Optional: a similar bullet in the spec-PR template.

## Non-goals

- Not a doc-control-system enforcement (lint rule). That belongs to the Follow-up #5 spec if it lands.
```

---

## Validation Commands

```bash
cd /home/matt/projects/icn

# Doc control plane
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict

# Architecture lint on every touched file
python3 docs/scripts/lint-arch.py docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md --cargo icn/Cargo.toml
python3 docs/scripts/lint-arch.py docs/spec/governed-service-binding.md --cargo icn/Cargo.toml
python3 docs/scripts/lint-arch.py docs/spec/storage-durability-policies.md --cargo icn/Cargo.toml
python3 docs/scripts/lint-arch.py docs/adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md --cargo icn/Cargo.toml
python3 docs/scripts/lint-arch.py docs/adr/ADR-0031-commons-compute-admission-and-settlement-policy.md --cargo icn/Cargo.toml

# Compliance + freshness
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
```

Expected: all pass; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness (informational; CI workflow treats as SUCCESS).

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **ICN core names scopes, not inhabitants.** The doctrine is now load-bearing in `INSTITUTION_PACKAGE_BOUNDARY.md` §C3. Future specs that violate it are reviewable on that basis.
2. **`Cooperative` is reserved** for the actual cooperative entity class, cooperative-economy framing, and cooperative-specific institution packages. The generic local-institutional scope is `LocalDomain` / `InstitutionalDomain`.
3. **Existing serialized `Coop`-prefixed identifiers** (`DataLocality::CoopReplicated`, ADR-0030's `Coop(coop_id)` scope tag, ADR-0031's `Coop-scoped` admission/settlement prose) are preserved as-is. Rename is forward work with proper compatibility migration.
4. **Issue `#1801`'s body is NOT edited by this PR.** The corrected vocabulary lands when the `#1801` spec PR opens, consuming the patch in §"Proposed #1801 patch."
5. **No Rust enum rename, no schema migration, no wire-format change** in this PR. Doc-only.

---

## Verification Commands (for the next session)

```bash
cd /home/matt/projects/icn

git branch --show-current
gh pr view <pr-number> --json state,mergeable,mergeStateStatus

# Re-run validation if anything is in question:
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 docs/scripts/lint-arch.py docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md --cargo icn/Cargo.toml

# Find any remaining drift not in the "Known drift" list:
rg -nE "CoopBound|cooperative-bound|coop-scoped|sent for cooperative processing" docs icn/crates/icn-kernel-api icn/crates/icn-compute
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/spec/institutional-domain.md` (merged via PR #1820) for the canonical owning-entity-class set.
- **Implementation truth:** verified by grep against `icn/crates/icn-kernel-api/src/storage.rs` (`DataLocality::CoopReplicated` exists at line 166); `icn/crates/icn-compute/src/types.rs` (uses `CoopReplicated` at line 1188). No new Rust types invented; no existing types touched.
- **Execution truth:** confirmed `main` HEAD `5461fd91d`; PR #1824 is still open (separate session's work); no other open PRs.
- **Known truth-plane conflict:** AGENTS.md vs `docs/dev/HANDOFF_TEMPLATE.md` disagree on handoff path. Carried.
- **No partner-package vocabulary** introduced. NYCN is mentioned in §C3's "What this means in practice" table as an example of an institution package, consistent with the existing §C2 worked example of NYCN. No NYCN-specific institutional nouns are imported into core text.
