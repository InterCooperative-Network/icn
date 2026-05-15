# Handoff — Compute Placement Policy Spec (#1801)

**Date:** 2026-05-14 (session day; 2026-05-15 in filename per chronological sequence after the entity-scope vocabulary boundary handoff)
**Branch:** `spec/compute-placement-policy`
**Primary issue advanced:** [#1801](https://github.com/InterCooperative/icn-core/issues/1801) (compute placement policy)
**Refs (not closed):** `#1801`, `#1794`, `#1795`, `#1796`, `#1797`, `#1798`, `#1799`, `#1815`, `#1816`, `#1817`, `#1818`, `#1792`, `#1634`
**Closes:** none

---

## Session Goal
<!-- TRUTH TYPE: Intent — what we set out to do -->

Define ICN compute placement policy as the missing-middle policy decision between ADR-0030 (workload manifest and authority boundary) and ADR-0031 (commons admission and settlement policy). Consume the entity-scope vocabulary doctrine landed in `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 (merged via PR #1825) by using `LocalDomain` / `LocalDomainBound` / "local-domain-scoped" / "processed inside your institution" instead of the `Coop`-prefixed forms still in `#1801`'s body.

Fix two additional vocabulary boundaries that the placement layer touches:

- **Execution budget vs capacity vs fuel.** `execution budget` is the policy-facing term for bounded execution. `fuel_limit` / `FuelLimit` is the implementation field on `ComputeTask` and is preserved verbatim. `capacity` is reserved for executor / node resource availability — not an alias for fuel. `resource envelope` is the workload's declared CPU / memory / storage / network shape. `allocation` is a governed permission to consume capacity. `settlement` is the receipted accounting outcome after execution.
- **Settlement vs payment.** ICN-native compute surfaces use settlement / unit / position / obligation / allocation / receipt framing. Legacy `payment_rate` / `payment_currency` fields on `ComputeTask` are preserved without endorsement and called out for a separate reconciliation follow-up.

Do not rename Rust code. Do not edit `#1801`'s body. Do not start adjacent work.

---

## Decisive Test
<!-- TRUTH TYPE: Falsifiable — what would prove this session failed -->

This PR fails if any of the following holds:

1. The new spec at `docs/spec/compute-placement-policy.md` uses generic `Coop` / `Cooperative` framing in placement-class names, member-shell strings, dashboard copy, or scope vocabulary outside explicit naming notes pointing at legacy code.
2. The new spec renames or aliases `FuelLimit`, `fuel_limit`, `payment_rate`, `payment_currency`, `DataLocality::CoopReplicated`, the `icn-coop` crate, or `coop_core` module paths.
3. The new spec uses unsafe vocabulary (payment, wallet, balance, currency, token, timebank) for ICN-native compute surfaces outside negation context.
4. The new spec edits `#1801`'s GitHub issue body.
5. `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict` fails on a warning introduced by this PR's added doc.
6. `python3 docs/scripts/lint-arch.py docs/spec/compute-placement-policy.md --cargo icn/Cargo.toml` fails.
7. `python3 docs/scripts/freshness-check.py --repo . --registry docs/registry.toml` introduces a new failure rooted in this PR's added doc.
8. `python3 .github/scripts/compliance_linter.py` flags new violations rooted in this PR's added doc.
9. The PR's commit closes `#1801` instead of using `Refs:`.
10. The PR touches Rust, SDK, deploy scripts, K3s manifests, the website, the gateway, the scheduler, or the runtime.

The decisive-test items above define the failure mode. See §"Verification Commands" for the exact commands used to check them.

---

## Final State Verified
<!-- TRUTH TYPE: Execution truth — actual state of the repo at handoff time -->

**Branch:** `spec/compute-placement-policy`.

**Initial commit SHA:** `caa3258c7` (spec, registry, INDEX, regenerated DOCUMENT_REGISTRY, this handoff). The branch head moves with each push to address review feedback; the latest head SHA is reflected in `gh pr view 1826 --json headRefOid`.

**PR:** [#1826](https://github.com/InterCooperative-Network/icn/pull/1826).

**Branch base:** `main` at `3af0d7ffd` (post-#1824, post-#1825 merge).

The PR is in **open** state. Per session pattern, the agent does not merge without explicit instruction. As of the latest push, all required CI checks have passed and AI reviewer (Copilot + Codex) feedback has been received and addressed.

Files modified or created:

1. `docs/spec/compute-placement-policy.md` — new file.
2. `docs/registry.toml` — new entry for the spec inserted before `[docs."docs/spec/governed-service-binding.md"]`.
3. `docs/INDEX.md` — Specifications section: new line after `artifact-registry-and-scoped-vault.md`.
4. `docs/DOCUMENT_REGISTRY.md` — regenerated via `--write-document-registry`. Corpus moves from 809 to 810 markdown files under `docs/`.
5. `docs/dev/handoff-2026-05-15-compute-placement-policy.md` — this file.

No other files touched. Rust workspace untouched. SDK untouched. Website untouched. Deploy scripts untouched.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. `docs/spec/compute-placement-policy.md` (new doc)

Length: roughly 400 lines. Authority class: normative (defines forward-direction primitives that `DomainPolicy` adopts and the compute substrate consumes). Sections:

- **Purpose** — names the missing-middle policy decision between ADR-0030 and ADR-0031.
- **What this spec is not** — ten non-claim bullets covering scheduler implementation, schema redefinition, infrastructure mutation, production rollout, kernel surface, Rust rename, new receipt classes, NYCN framing, and closure of `#1801`.
- **Vocabulary boundaries** — three boundaries (scope per `INSTITUTION_PACKAGE_BOUNDARY.md` §C3, execution vs capacity, settlement vs payment) with a six-row term table and the verbatim compatibility note the user specified.
- **Placement classes** — seven closed classes: `LocalOnly`, `DomainLocalPreferred`, `LocalDomainBound`, `FederationBound`, `CommonsEligible`, `ExternalCustodianRequired`, `RejectedByPolicy`.
- **Placement hierarchy** — local-first default with explicit override path.
- **Decision contract** — eighteen candidate inputs (domain id, submitting actor, authority basis, workload kind, privacy class, determinism class, data locality, execution budget / `fuel_limit`, resource envelope, deadline / priority, executor capabilities, executor capacity, trust / admission class, federation agreement refs, commons pool policy refs, allocation policy refs, settlement policy refs, required review state). Two-layer outputs: Layer 1 policy-oracle return value is exactly one of `PlacementDecision` or `PlacementRejected`, with an optional attached `PlacementFallbackReceipt` and a surfaced `ReviewRequiredActionCard`. Layer 2 post-placement artifact is `ExecutorAdmissionDecision` emitted by the selected executor. `ComputeReceipt`, `OutputArtifactReceipt`, and `SettlementReceipt` are referenced from the placement decision but emitted by the post-execution chain.
- **Boundary rules** — nine load-bearing rules.
- **Fallback behavior** — five rules covering capacity-bound fallback, authority-bound fallback, receipt emission, fallback-is-not-retry, and no-silent-fallback-to-commons.
- **Example policies to specify** — four illustrative examples: public advisory workload, private care / accessibility workload, federation report workload, deterministic governance-grade workload.
- **Operator / steward dashboard** — twelve rendered fields using `execution budget`, `capacity fit`, `allocation decision`, `settlement receipt`. No `fuel`, no `payment`, no `Coop` framing in generic strings.
- **Member shell** — seven plain-language strings using "processed inside your institution" (per `#1825`'s §C3 corrected form).
- **Receipts** — explicit mapping to existing receipt classes from ADR-0026, ADR-0030, ADR-0031, and `docs/spec/artifact-registry-and-scoped-vault.md`. No new receipt classes introduced.
- **Failure and safety table** — sixteen rows.
- **Known drift (preserved without endorsement)** — six items: `FuelLimit` / `fuel_limit`, `payment_rate` / `payment_currency`, `DataLocality::CoopReplicated`, ADR-0030 `Coop(coop_id)` and ADR-0031 `Coop-scoped` (both with existing naming notes), `icn-coop` crate / `coop_core` paths, `coop-scoped` comments in `icn-rpc`.
- **First safe proof-loop / dogfood slice** — names two safe slices satisfying acceptance criterion 6: (a) read-only placement-decision rehearsal; (b) dry-run fallback exercise.
- **Cross-links** — explicit links to `INSTITUTION_PACKAGE_BOUNDARY.md` §C3, the seven sibling specs, eight ADRs, and the related GitHub issues.
- **Non-claims** — repeated grep-friendly block covering production readiness, partner federation, NYCN pilot, Rust rename, new receipts, scheduler implementation, retry semantics, kernel surface, schema change, unsafe vocabulary.

### 2. `docs/registry.toml` (one new entry)

`[docs."docs/spec/compute-placement-policy.md"]`:
- `category = "architecture"`
- `status = "draft"`
- `description` summarizes the placement-class taxonomy, decision contract, three fixed vocabulary boundaries, boundary rules, failure/safety table, first safe proof-loop, and the legacy identifiers preserved without endorsement.
- `domain_tags = ["compute", "placement", "scheduler", "policy-oracle", "kernel-app-boundary", "commons", "federation", "settlement", "spec"]`.
- `depends_on` lists the integrating spine doc, the meaning-firewall doc, the boundary doc, the seven sibling specs, and eight ADRs.

### 3. `docs/INDEX.md` (Specifications section)

Added one line after the `artifact-registry-and-scoped-vault.md` entry pointing at the new spec.

### 4. `docs/DOCUMENT_REGISTRY.md` (regenerated)

Regenerated via `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. Corpus moves from 809 to 810 markdown files under `docs/`. 54 pre-existing yaml-mismatch warnings persist (unrelated to this PR).

### 5. `docs/dev/handoff-2026-05-15-compute-placement-policy.md` (this file)

Records intent, decisive test, final state, what changed, what's open, unsafe assumptions, next move, architectural decisions, verification commands, truth-plane notes, and follow-up issue drafts.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Branch created (`spec/compute-placement-policy`).
- [x] Spec written (`docs/spec/compute-placement-policy.md`).
- [x] Registry entry added.
- [x] INDEX entry added.
- [x] `DOCUMENT_REGISTRY.md` regenerated (810 markdown files).
- [x] Handoff written (this file).
- [x] Validation suite run before commit (doc_control_check `--strict` pass; lint-arch 0 errors / 9 warnings, all in explicit negation context; compliance_linter clean; cross-link smoke check OK; targeted vocabulary check OK).
- [x] Commit + push (`caa3258c7`).
- [x] PR opened ([#1826](https://github.com/InterCooperative-Network/icn/pull/1826)).
- [x] Initial CI watched: 23 pass / 5 expected docs-only skips / 0 fail; `mergeStateStatus: CLEAN`.
- [x] AI-reviewer feedback (Copilot 6 threads + 2 low-confidence comments; Codex 1 P2 thread) received and addressed in a follow-up commit on the same branch.
- [ ] Per the established session pattern: **"Apply valid feedback only. Stop and summarize unless explicitly told to merge."** Wait for explicit merge instruction.
- [ ] Decide whether to file the follow-up issue drafts listed in §"Follow-up issue drafts" (deferred to user).

### Carried forward (not addressed in this PR)

**`docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`** — `AGENTS.md` lines 281–313 still say `docs/dev-journal/`; `docs/dev/HANDOFF_TEMPLATE.md` and 18+ merged PRs use `docs/dev/`. Surfaced in #1820 review and carried through every subsequent PR. **Not addressed in this PR** to keep scope tight.

**Legacy `payment_rate` / `payment_currency` on `ComputeTask`.** Not edited in this PR per the user's explicit instruction. Reconciliation draft included as Follow-up #2 below.

**Issue `#1801` body.** Still contains `CoopBound`, `coop-scoped execution policy`, "cooperative-bound executor," and "sent for cooperative processing." Not edited in this PR per the user's explicit instruction. The new spec carries the corrected vocabulary; reconciling the issue body to the spec belongs to a separate human-driven action.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **Privacy class taxonomy.** `ComputeTask`'s current `PrivacyClass` enum has variants `Public`, `Member`, `NeedToKnow` (verified by grep against `icn-compute/src/types.rs`). The forward-direction richer taxonomies in `#1792` are referenced as forward-direction; the spec does not pre-decide which taxonomy will land first. If `#1792` lands a different shape before placement-decision code is written, the spec's privacy-class inputs need a re-read.
- **`DataLocality` variants.** Four variants: `CellLocal = 0`, `CoopReplicated = 1`, `FederationMirrored = 2`, `CommonsPublic = 3` (verified by grep against `icn-kernel-api/src/storage.rs`). The spec preserves `DataLocality::CoopReplicated` as the Rust identifier and uses `LocalDomain`-replicated framing only in spec prose.
- **`FuelLimit`** is the bounded-execution guard in `ComputeTask` (verified by grep). If a parallel in-flight PR adds an alias or a new bounded-execution field with a different name, the spec's compatibility note needs an addendum.
- **No `LocalDomainBound` identifier exists in code.** Verified by `rg "LocalDomainBound" icn/ sdk/ docs/` returning only this spec and the boundary-doc tables in `INSTITUTION_PACKAGE_BOUNDARY.md` §C3. If a parallel in-flight PR adds the identifier with a different shape, this spec's placement-class definition needs reconciliation.
- **Receipt-class mapping.** The placement decision references `PlacementDecision`, `PlacementRejected`, `ExecutorAdmissionDecision`, `PlacementFallbackReceipt`, `ReviewRequiredActionCard` as forward-direction objects. `ComputeReceipt`, `OutputArtifactReceipt`, `SettlementReceipt`, `RatificationReceipt` are named per existing ADRs and specs. The spec asserts no new receipt classes are introduced; if a reviewer surfaces that `PlacementDecision` as named here would map to a new ADR-0026 receipt class with its own retention policy, the spec's "no new receipt classes" claim needs revision.
- **`#1801` acceptance criteria.** The spec satisfies the first five and the seventh acceptance criterion (placement policy spec; placement classes and decision IO; privacy/locality/determinism/trust/resource/settlement/receipts mapped; fallback/rejection behavior; dashboard and member-shell language; cross-link list). It satisfies criterion 6 ("Spec identifies first safe proof-loop or dogfood slice without starting implementation") via the §"First safe proof-loop / dogfood slice" section. **Closure of `#1801` is left to the user; the PR uses `Refs: #1801`, not `Closes: #1801`.**

---

## Next Move
<!-- What the next agent or human action should be -->

1. Run validation suite (commands below).
2. Commit (single commit; `docs(spec): define compute placement policy`).
3. Push branch.
4. Open PR with `Refs: #1801` and the cross-refs listed at top of this handoff.
5. Watch the initial CI run; address any failure rooted in this PR's added doc.
6. Watch the AI reviewer outputs (Copilot, Codex). Apply only valid feedback (verify each claim against the live repo with grep before acting). Reply to review threads with commit SHA and exact fix per the established session pattern.
7. **Stop and summarize after applying valid feedback.** Do not merge unless explicitly told to merge.

---

## Architectural Decisions
<!-- TRUTH TYPE: Decision truth — choices made and why -->

### 1. Placement classes are an app-side enum, not a kernel-side one.

The seven placement classes (`LocalOnly` / `DomainLocalPreferred` / `LocalDomainBound` / `FederationBound` / `CommonsEligible` / `ExternalCustodianRequired` / `RejectedByPolicy`) are domain-semantic. The kernel never imports them. The policy oracle returns a `ConstraintSet` derived from the placement class plus the manifest; the kernel enforces that constraint set blindly. This preserves the meaning-firewall boundary from `docs/architecture/KERNEL_APP_SEPARATION.md`.

### 2. No new receipt classes.

Every receipt this spec touches maps onto existing classes from ADR-0026 (receipt envelope), ADR-0030 (compute outputs), ADR-0031 (settlement), and `docs/spec/artifact-registry-and-scoped-vault.md` (output artifacts). Placement evidence lands as Stage 5 evidence artifacts per `docs/spec/effect-dispatch-contract.md` §"Stage 5." Adding new receipt classes would require an ADR amendment, which this PR does not have authority to do.

### 3. Vocabulary boundaries are documented in spec text, not enforced in code.

The execution-vs-capacity vocabulary boundary is documented as a six-row term table plus a verbatim compatibility note. Code identifiers (`FuelLimit`, `fuel_limit`, `payment_rate`, `payment_currency`, `DataLocality::CoopReplicated`) are preserved as-is. The spec's compatibility note explicitly names the divergence: spec-facing term is `execution budget`; runtime field is `fuel_limit`; both are correct in their own layer. This pattern matches the entity-scope vocabulary boundary's naming-notes approach in ADR-0030 and ADR-0031.

### 4. The first safe proof-loop is a read-only rehearsal, not a scheduler change.

Per acceptance criterion 6, the spec names two safe slices — (a) read-only placement-decision rehearsal and (b) dry-run fallback exercise. Both exercise the decision contract without invoking the executor, the admission engine, or any settlement code. The user's previous session pattern ("identify the slice without starting implementation") is preserved.

### 5. The spec uses `Refs:`, not `Closes:`.

Per the standing session guardrail and the `feedback_pr_refs_not_closes_unless_fully_satisfied` memory: closure of `#1801` requires explicit user approval and verification that all seven acceptance criteria are fully satisfied by this single PR. The spec satisfies the first five plus the seventh on its face and offers a credible interpretation of criterion 6 (the §"First safe proof-loop / dogfood slice" section). Closure is a user-driven decision.

---

## Verification Commands
<!-- TRUTH TYPE: Verification — exact commands that prove the claims above -->

```bash
cd /home/matt/projects/icn

# Doc control plane (strict)
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md
# Expected: passes; corpus 810 markdown files; 54 pre-existing yaml-mismatch warnings (unrelated).

# Architecture vocabulary lint on the new doc
python3 docs/scripts/lint-arch.py docs/spec/compute-placement-policy.md --cargo icn/Cargo.toml
# Expected: passes.

# Freshness (informational; new doc should pass)
python3 docs/scripts/freshness-check.py --repo . --registry docs/registry.toml || true

# Regulatory vocabulary linter
python3 .github/scripts/compliance_linter.py || true

# Targeted vocabulary checks against forbidden core terms in the new doc
rg -n "\bpayment\b|\bwallet\b|\bbalance\b|\bcurrency\b|\bMana\b|\bCoVM\b|\btimebank\b" docs/spec/compute-placement-policy.md
# Expected: only matches inside negation context (Boundary rule 7, Known drift, Non-claims).

# Scope vocabulary check
rg -n "CoopBound|cooperative-bound|coop-scoped" docs/spec/compute-placement-policy.md
# Expected: matches only inside "Known drift" preserving legacy code names with naming notes.

# Cross-link smoke check
rg -o "docs/[a-zA-Z0-9_/\\-]+\\.md" docs/spec/compute-placement-policy.md | sort -u | while read p; do test -f "$p" && echo "OK $p" || echo "MISSING $p"; done
# Expected: all OK.

# Diff scope check (this PR should touch only docs/)
git diff --stat main..HEAD
# Expected: only docs/spec/compute-placement-policy.md, docs/registry.toml, docs/INDEX.md, docs/DOCUMENT_REGISTRY.md, docs/dev/handoff-2026-05-15-compute-placement-policy.md.
```

---

## Truth-Plane Notes
<!-- TRUTH TYPE: Provenance — where the assertions in this handoff come from -->

- **Issue body content for `#1801`** verified via `gh issue view 1801 --json body --jq '.body'` against the repo at session start. The body still uses `CoopBound`, `coop-scoped execution policy`, "cooperative-bound executor," and "sent for cooperative processing." Not edited by this PR.
- **§C3 doctrine in `INSTITUTION_PACKAGE_BOUNDARY.md`** verified by `grep` against the merged `main` post-#1825. The "Correct vs incorrect naming" table in §C3 lines 232–257 (approx) explicitly names `CoopBound` → `LocalDomainBound` and "sent for cooperative processing" → "processed inside your institution."
- **ADR-0030 naming note** verified at line 53 of `docs/adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md`.
- **ADR-0031 naming notes** verified at lines 51 and 59 of `docs/adr/ADR-0031-commons-compute-admission-and-settlement-policy.md`.
- **`DataLocality::CoopReplicated`** verified in `icn/crates/icn-kernel-api/src/storage.rs`.
- **`FuelLimit` field on `ComputeTask`** verified via grep against `icn-compute/src/types.rs`.
- **No `LocalDomainBound` identifier in code** verified by `rg "LocalDomainBound" icn/ sdk/ docs/` returning only `INSTITUTION_PACKAGE_BOUNDARY.md` and (post-edit) this spec.
- **No new receipt classes claim** verified by reading `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md` summary and the receipt-class summary in `docs/spec/effect-dispatch-contract.md`.
- **Sibling spec patterns** verified by reading the first 60 lines of `docs/spec/governed-service-binding.md` and the `[docs."docs/spec/storage-durability-policies.md"]` and `[docs."docs/spec/governed-service-binding.md"]` registry entries.
- **Codex P2 handoff-path drift** (carried from #1820/#1822/#1823/#1824/#1825) is **not** addressed by this PR. Tracked separately.

---

## Follow-up issue drafts (not filed)

Seven titles drafted per the user's instruction. Paste-ready for separate review.

### 1. `refactor(compute): rename FuelLimit / fuel_limit to align with execution-budget vocabulary`

**Body draft:**

```
## Purpose

Align the runtime bounded-execution guard's identifier with the policy-facing vocabulary fixed in `docs/spec/compute-placement-policy.md` §"Vocabulary boundaries." The current `FuelLimit` / `fuel_limit` identifiers are correct in their layer (low-level runtime), but the policy-facing surfaces have started using `execution budget`, which makes the divergence visible to future readers. This issue evaluates whether a code-side rename is worth the migration cost.

## Options

- **Option A — Preserve `FuelLimit`.** No code rename. Spec carries the compatibility note in perpetuity. Lowest cost, highest divergence between spec and code.
- **Option B — Rename to `ExecutionBudget`.** Identifier rename with `#[serde(alias = "fuel_limit")]` for backward compatibility. Update all call sites in `icn-compute`, `icn-core`, tests. Approximately N call sites per grep (need to enumerate before deciding).
- **Option C — Add `ExecutionBudget` as a type alias.** `pub type ExecutionBudget = FuelLimit;` so both names work. Doc-only rename in spec; code stays. Mid-cost.

## Non-goals

- Not a wire-format change (the underlying integer value stays).
- Not a rename of `payment_rate` / `payment_currency` fields (see follow-up #2).

## Decision-blocking inputs

- Cost of Option B (touch count, test surface).
- Whether any external SDK consumers depend on the `fuel_limit` serialized name.
- Whether the rename should land before or after the scheduler-integration PR that consumes the placement-policy spec.

## Related

- `docs/spec/compute-placement-policy.md` §"Vocabulary boundaries."
- `docs/adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md`.
```

### 2. `spec(compute): reconcile fuel/payment legacy vocabulary on ComputeTask`

**Body draft (verbatim from the user's request, expanded):**

```
## Purpose

Define vocabulary boundaries for compute execution and accounting terms before any code migration.

## Known drift

- `FuelLimit` / `fuel_limit` is a low-level runtime term; policy / member-facing surfaces should prefer `execution budget`.
- `capacity` should remain reserved for executor / node resource availability.
- `ComputeTask` still contains legacy `payment_rate` / `payment_currency` fields. Current ICN vocabulary should use settlement / unit / position / obligation / allocation / receipt framing instead.

## Scope of this spec

- Document the vocabulary boundaries above (already partially landed in `docs/spec/compute-placement-policy.md` §"Vocabulary boundaries"; this issue extends the boundary doc to the accounting-side fields specifically).
- Propose a target shape for the accounting fields on `ComputeTask`: a structured `SettlementEstimate` (or similar) replacing `payment_rate` / `payment_currency` once a settlement policy reference and an obligation policy reference are available.
- Specify the migration path: spec first; alias / structured replacement second; deprecation of the original fields third.

## Non-goals

- No Rust rename in the issue itself.
- No schema migration until a compatibility plan exists.
- No settlement implementation change.

## Related

- `docs/spec/compute-placement-policy.md` §"Vocabulary boundaries" and §"Known drift."
- `docs/adr/ADR-0031-commons-compute-admission-and-settlement-policy.md`.
- `#1634` — obligation / allocation / settlement primitives.
```

### 3. `refactor(storage): rename CoopReplicated locality to LocalDomainReplicated`

(Carried verbatim from the entity-scope vocabulary boundary handoff #1825. Re-listed here for completeness; not re-drafted.)

### 4. `spec(compute): wire-stable PlacementDecision and ExecutorAdmissionDecision schema`

**Body draft:**

```
## Purpose

Land wire-stable schemas for the placement-decision artifacts named in `docs/spec/compute-placement-policy.md` §"Decision contract": `PlacementDecision`, `PlacementRejected`, `ExecutorAdmissionDecision`, `PlacementFallbackReceipt`, `ReviewRequiredActionCard`. The placement-policy spec defines the contract at design granularity; this issue produces the wire-stable record shapes that an implementation can encode and a verifier can deserialize.

## Scope

- Record shape per artifact (fields, types, nullability, serialization, canonicalization).
- ADR amendment to ADR-0026 if any of these artifacts need a new entry in the receipt-class summary.
- Round-trip tests covering each artifact.
- Cross-link to `ContentHash`, `SemanticVersion`, `policy_version_id` per existing canonicalization conventions.

## Non-goals

- Not a scheduler change.
- Not an executor change.
- Not a settlement engine change.

## Related

- `docs/spec/compute-placement-policy.md` §"Decision contract" and §"Receipts."
- `docs/spec/effect-dispatch-contract.md` §"Stage 5 — Application and evidence."
- `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md`.
```

### 5. `feat(compute): policy oracle for placement decisions (read-only proof-loop)`

**Body draft:**

```
## Purpose

Implement the first safe slice named in `docs/spec/compute-placement-policy.md` §"First safe proof-loop / dogfood slice": a read-only placement-policy oracle that, given a `ComputeTask` and a `DomainPolicy` reference, returns a `PlacementDecision` artifact (or `PlacementRejected` artifact) without invoking any executor. Demonstrates the policy oracle is wired correctly and the placement-class taxonomy covers the realistic workload mix.

## Scope

- A policy oracle module under `apps/compute-placement` (forward-direction; structure per kernel/app separation).
- Read-only API: given inputs from the spec's §"Candidate inputs," returns one of the §"Candidate outputs."
- Tests covering each of the seven placement classes and the rejection paths in the failure-and-safety table.
- No executor admission code change.
- No scheduler change.

## Non-goals

- No executor admission.
- No settlement engine change.
- No PlacementFallbackReceipt emission (covered by follow-up #6).

## Related

- `docs/spec/compute-placement-policy.md` §"First safe proof-loop / dogfood slice."
```

### 6. `feat(compute): dry-run fallback exercise + PlacementFallbackReceipt emission`

**Body draft:**

```
## Purpose

Implement the second safe slice named in `docs/spec/compute-placement-policy.md` §"First safe proof-loop / dogfood slice": a dry-run fallback exercise that submits a workload preferring `DomainLocalPreferred`, synthetically constrains local capacity to zero, and verifies the resulting `PlacementDecision` records `LocalDomainBound` plus a `PlacementFallbackReceipt`.

## Scope

- Synthetic capacity-shim for the dry-run test.
- `PlacementFallbackReceipt` emission path through the placement-policy oracle.
- Tests covering capacity-bound fallback (Boundary rule §"Fallback behavior" rule 1) and authority-bound fallback (rule 2).

## Non-goals

- No real executor admission.
- No production capacity reporting.

## Related

- `docs/spec/compute-placement-policy.md` §"Fallback behavior" and §"First safe proof-loop / dogfood slice."
```

### 7. `docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`

(Carried from #1820/#1822/#1823/#1824/#1825. The agent docs claim handoffs go to `docs/dev-journal/`; the template and 18+ merged PRs use `docs/dev/`. One-line fix to `AGENTS.md`.)
