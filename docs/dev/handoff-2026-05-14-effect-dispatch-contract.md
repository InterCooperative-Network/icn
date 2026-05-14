# Session Handoff — 2026-05-14 (effect dispatch contract)

**Trigger:** PR #1814 (`ICN_INTEGRATED_SYSTEM_MODEL.md`) merged today and named #1797 as the next safe step in the spec ladder. #1797 ("spec(governance): define accepted-proposal effect dispatch contract") has six concrete acceptance criteria. This session writes the spec doc that satisfies the first four — the remaining two ("first safe runtime dogfood slice identified" and "follow-up implementation issues opened only after spec acceptance") are addressed by the spec body and by deferring filing to a post-merge decision.

**Session goal:** Produce one written contract that harmonizes the substantial existing types and ADRs (EffectManifest, Mandate, KernelEffect, InstitutionalEffectRecord, EffectDispatchEvidence; ADR-0014/0019/0025/0026/0027/0029/0030/0031) into a single end-to-end chain. Name what is missing in spec form: dry-run, idempotency, partial failure, challenge/reversal, CCL hook points, package boundary, first dogfood slice.

## Final state (verified)

- **Repo HEAD before session:** `d49c83b12` (the squashed #1814 spine merge on main).
- **Branch:** `spec/governance-effect-dispatch-contract`.
- **Working tree before session:** clean.
- **Open PRs at session start:** dependabot #1790, #1791 only.

## Existing infrastructure mapped (Phase 1)

The exploration confirmed the gap #1797 names is real but **narrower than naive reading suggests**. ICN already implements most of the dispatch chain in code; what's missing is a written behavior contract that names how the pieces compose.

**Already in code (canonical types this spec uses verbatim):**

| Type | Crate / File |
|---|---|
| `GovernanceDecisionReceipt`, `GovernanceProof` | `icn/crates/icn-governance/src/proof.rs` |
| `Mandate` (with `new_pending_grants` truthful fall-through) | `icn/crates/icn-governance/src/mandate.rs` |
| `AuthorityClass`, `AuthorityGrant`, `TypedScope` | `icn/crates/icn-governance/src/authority.rs` |
| `EffectManifest` (versioned, hashable, deterministic; capability/economic/membership/protocol effects, timelock-relevant blocks) | `icn/crates/icn-governance/src/effect_manifest.rs` |
| `KernelEffect` (closed aggregate: Treasury/Membership/Protocol/Control/Federation/Dispute/Resource/SDIS/NoOp) | `icn/crates/icn-kernel-api/src/effects.rs` |
| `EffectOutcome` (`Applied`/`NoOp`/`Partial`/`Failed`), `ProposalExecutor` trait | `icn/crates/icn-kernel-api/src/governance.rs` |
| `grant_minting.rs` seam (ADR-0019 minting + persistence) | `icn/apps/governance/src/grant_minting.rs` |
| `InstitutionalEffectRecord` | `icn/apps/governance/src/institutional_effect.rs` |
| `EffectDispatchEvidence` (append-only log, sync reporting) | `icn/apps/governance/src/dispatch_evidence.rs` |

**Already decided in ADRs:**

- ADR-0014 — constitutional object model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) — types-first, partially implemented.
- ADR-0019 — accepted-decision seam: mints grants conservatively, persists exactly one `Mandate`, no kernel-dispatch enforcement.
- ADR-0025 (proposed) — `EffectRecord` closed-taxonomy schema (RoleAssigned, AllocationMade, EffectReversed, etc.).
- ADR-0026 — receipt and provenance proof envelope (Layers 1–4).
- ADR-0027 — action card contract (cards are derived, not stored).
- ADR-0029 — conflict resolution object model.
- ADR-0030 — compute workload manifest and authority boundary (compute executes, does not decide).
- ADR-0031 — commons compute admission and settlement policy.

**Genuine gaps the spec fills:**

1. The end-to-end **behavior contract** — how the existing types compose into a chain. ADRs cover pieces; no document covers the full sequence.
2. **Dry-run / preview** — referenced in #1797's scope; not specified anywhere.
3. **Idempotency rule** — the manifest is hashable but the idempotency *contract* (`(decision_hash, manifest_hash, effect_index)`) is not written down.
4. **Partial failure semantics** — `EffectOutcome::Partial` exists; the institutional handling rule does not.
5. **Challenge / reversal / counter-receipt** — ADR-0025 names `EffectReversed` as an effect kind; the protocol that produces one is not specified.
6. **CCL hook points** — the spine doc names CCL inside governance; this spec names where CCL participates in the dispatch chain.
7. **Action-card trigger rules** — ADR-0027 says cards are derived; the spec names which effect kinds drive which cards.
8. **Package boundary** — the rule that package vocabulary translates to generic effects *before* dispatch is named here explicitly.
9. **First safe runtime dogfood slice** — #1748 process-transition receipts identified as the entry that exercises the chain without requiring kernel-side enforcement.

## Files changed

| Path | Change | Lines |
|---|---|---|
| `docs/spec/effect-dispatch-contract.md` | new | ~360 |
| `docs/registry.toml` | new entry inserted after `KERNEL_CONTRACTS.md` block | +18 |
| `docs/dev/handoff-2026-05-14-effect-dispatch-contract.md` | new (this file) | ~150 |

No Rust changes. No SDK changes. No website changes. No CI changes.

## How the spec maps to #1797's acceptance criteria

| Criterion | Where addressed in the spec |
|---|---|
| Effect dispatch spec / design doc merged | This doc, once the PR lands. |
| Distinguishes decision / mandate / authority / mutation plan / applied effect / receipt | §"The dispatch chain" (5 stages) + §"Canonical objects" table. |
| Defines dry-run / preview, idempotency, partial failure, challenge / reversal | §"Dry-run / preview", §"Idempotency", §"Partial failure semantics", §"Challenge / reversal / counter-receipt". |
| Preserves core / package boundary | §"Package boundary". Cross-link to `INSTITUTION_PACKAGE_BOUNDARY.md`. |
| Identifies first safe runtime dogfood slice (preferably tied to #1748) | §"First safe runtime dogfood slice" — #1748 process-transition receipts. |
| Follow-up implementation issues opened only after spec acceptance | Deferred. No new issues filed in this PR. The §"Open questions and deferred decisions" table enumerates what needs follow-up; filing happens after #1797 review. |

The PR uses `Refs: #1797` rather than `Closes`, leaving the closure decision with the user post-review.

## Issue hygiene findings

The recent spec ladder (#1815, #1816, #1817, #1818 filed earlier today) cross-references this spec correctly. #1817 (CCL policy registry) is the closest natural follow-up because it specifies the CCL hook points named in §"CCL hook points." #1815 (governed service binding) follows once dispatch routing for service-binding effects is exercised. #1816 (backup / replication / recovery / archive) intersects with the privacy / redaction rules and with restore-test receipts named in §"Privacy and redaction" and §"Receipt class summary."

ADR-0025 is still in `proposed` state. Implementing it (the `EffectRecord` closed-taxonomy type and crate placement) is named as forward work in §"Stage 5" and in §"Open questions." Whether ADR-0025 should be promoted to `accepted` based on this spec is a separate decision for the user.

## Remaining risks and known gaps

The spec does **not**:

- Authorize kernel-side mandate enforcement. Deferred per ADR-0019 "Non-decisions."
- Implement ADR-0025's `EffectRecord` closed taxonomy. Forward direction.
- Specify the CCL policy registry / versioning / evaluator-selection contract. That is #1817's job.
- Define backup, replication, or recovery policy objects (incl. restore-test receipts). That is #1816's job.
- Define `GovernedServiceBinding` / `WorkloadManifest` / `RuntimeProvider`. That is #1815's job.
- Specify the member shell rendering of effect previews and challenge flows. That is #1818's job.
- Specify federation-side mandate recognition. Deferred.

Each gap is named explicitly in the spec's §"Open questions and deferred decisions" table.

## Recommended next PR

**#1817 — CCL policy registry, versioning, and governance-effect hook contract.** That spec operationalizes the §"CCL hook points" in this contract: it names how a CCL evaluator is selected for a proposal kind, how its output produces an `EffectManifest`, and how policy versions are bound to a domain. With #1797 (this spec) and #1817 in place, the chain has an executable rule layer and a written contract.

After #1817, the natural sequence: #1815 (workload binding) → ADR-0025 implementation (`EffectRecord` type) → #1816 (backup policies) → #1799 (network anti-entropy proof loops) → #1795 (steward cockpit) → #1818 (member shell).

## Validation run

Commands executed (results filled in after running):

```bash
cd /home/matt/projects/icn

# Doc control plane
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict

# Architecture lint (forbidden-vocab + crate-name check)
python3 docs/scripts/lint-arch.py docs/spec/effect-dispatch-contract.md --cargo icn/Cargo.toml

# Regulatory compliance linter
python3 .github/scripts/compliance_linter.py

# Freshness check (CI-equivalent invocation per Codex feedback on PR #1814)
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
```

### Validation results

- **`doc_control_check.py --strict`** → exit 0. *Recording pre-existing unrelated warnings honestly; not broadening scope.*
- **`lint-arch.py`** → exit 0, `CLEAN: No violations found` after rewording one cell to drop the word "token" (kept the `Capability` type name; cross-references ADR-0014's deferred tranche).
- **`compliance_linter.py`** → exit 0, no violations.
- **Cross-link existence sweep** → 19/19 referenced paths present (12 docs + 7 source files).
- **Forbidden-vocabulary grep** → zero positive uses; all soft-forbidden vocabulary appears only in negation context.

*(Detailed validation outputs are captured in the PR body; left out of the handoff to avoid duplication.)*

## Non-claims preserved

This handoff and the PR it documents:

- Do not claim implementation. The contract names invariants; the code already mostly implements them, but enforcement at the kernel boundary remains deferred per ADR-0019.
- Do not authorize the kernel dispatcher to consult mandates as a gate.
- Do not mint kernel `Capability` records from `AuthorityGrant`s. ADR-0014 explicitly defers that.
- Do not expand the supported proposal-class set for grant minting beyond ADR-0019's seam.
- Do not specify federation mandate recognition.
- Do not introduce new ADR-0025 `EffectRecord` kinds.
- Do not introduce new schema or wire formats.
- Do not by themselves close #1797. The PR uses `Refs:`, not `Closes:`.
- Do not use payment, wallet, balance, currency, or blockchain as ICN-native framing. (The one residual "token" mention in the original draft was reworded after the lint warning surfaced it; kept ADR-0014's `Capability` type name without the qualifier.)
