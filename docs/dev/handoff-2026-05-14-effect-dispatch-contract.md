# Session Handoff — 2026-05-14 (effect dispatch contract)

## Session Goal

Produce the written behavior contract that satisfies #1797's first four acceptance criteria by harmonizing the existing governance / dispatch types and ADRs into one document, while preserving every deferred decision named in ADR-0014 / ADR-0019 / ADR-0025.

## Decisive Test

Can a downstream implementer — looking only at the spec — name (1) which existing type each of the five dispatch stages emits, (2) what is enforceable today versus what is deferred, and (3) the first safe runtime slice to exercise? If yes, the spec has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`d49c83b12` — `docs(architecture): add integrated cooperative operating model spine (#1814)` (merged earlier this session).

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| #1819 | `spec/governance-effect-dispatch-contract` | OPEN | All required pass; non-required skipping | Awaiting review |
| #1790 | dependabot (ts-sdk dev deps) | OPEN | n/a | Out of session scope |
| #1791 | dependabot (pilot-ui dev deps) | OPEN | n/a | Out of session scope |

### Branches
- `spec/governance-effect-dispatch-contract` — head `801acd022` (original spec commit); review fixes applied locally pending push.
- Local `main` synced to `d49c83b12` (post-#1814 merge).

### Issues
- #1797 — OPEN. PR #1819 uses `Refs:`, not `Closes:`. Closure left for human review against the issue's six acceptance criteria.
- #1793 — closed earlier this session (#1814 merge).
- #1815, #1816, #1817, #1818 — filed earlier this session; all OPEN; labeled `type:spec`.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Spec doc landed (`docs/spec/effect-dispatch-contract.md`, new ~390 lines)

Defines five stages from acceptance to receipt, each using existing types verbatim:

- Stage 1 — Decision recording → `GovernanceDecisionReceipt` (anchors `decision_hash`); authenticity carried by companion `GovernanceProof` and `GovernanceDecisionAttestation`. (ADR-0026 Layer 1.)
- Stage 2 — Mandate minting → `Mandate` + zero-or-more `AuthorityGrant`s via the `grant_minting.rs` seam. (ADR-0014, ADR-0019.)
- Stage 3 — Effect plan → `EffectManifest` (deterministic, hashable). (Intrinsic.)
- Stage 4 — Effect dispatch → one `KernelEffect` per manifest entry, per target subsystem. (ADR-0014 meaning-firewall contract.)
- Stage 5 — Application + evidence → `EffectOutcome` + opaque `receipt_ref` → `InstitutionalEffectRecord` + `EffectDispatchEvidence`. (Forward: `EffectRecord` per ADR-0025.)

Adds the behavior rules that no ADR yet writes down (dry-run/preview, idempotency, partial failure, challenge/reversal, CCL hook points, action-card trigger, privacy/redaction, package boundary, first dogfood slice = #1748).

### 2. Registered the doc (`docs/registry.toml`, +14 lines after the post-review revision)

Inserted after the `[docs."docs/spec/KERNEL_CONTRACTS.md"]` block. Carries `category`, `status = "draft"`, `title`, `description`, `last_updated`, `last_reviewed`, `owner`, `audiences`, `domain_tags`, `depends_on`, `recommended_action`. Lets the `docs/spec/` prefix default supply `truth_class = "normative"`, `role = "spec"`, `canonical = "no"`, `freshness = "unknown"`. (Per Copilot review feedback, the original `truth_class = "descriptive"` and `role = "architecture"` overrides were dropped.)

### 3. Review feedback applied (commit pending push)

Five comments received on PR #1819 head `801acd022`:

| # | Reviewer | Where | Severity | Action |
|---|----------|-------|----------|--------|
| 1 | Codex | line 76 (spec doc, Stage 1) | P2 | Reworded Stage 1: `GovernanceDecisionReceipt` records/anchors the decision; companion `GovernanceProof` and `GovernanceDecisionAttestation` carry signatures. |
| 2 | Codex | line 182 (spec doc, Idempotency) | P2 | Split Idempotency into "Current contract" (uses existing fields) and "Future schema work" (the stable `(decision_hash, manifest_hash, effect_index)` key and retry-policy field are deferred to follow-up implementation issues). |
| 3 | Copilot | line 10 (YAML frontmatter) | normal | YAML `Status: descriptive` → `Status: normative`; body opening reframed to "spec, work-in-progress." |
| 4 | Copilot | registry entry | normal | Dropped `truth_class = "descriptive"`, `role = "architecture"`, `canonical = "no"`, `freshness = "current"` overrides on the `docs/spec/` entry; prefix defaults now apply. |
| 5 | Copilot | handoff format | normal | This handoff rewritten to follow `docs/dev/HANDOFF_TEMPLATE.md` structure. |

### 4. Future-direction issues remain unfiled this PR

Per #1797's sixth acceptance criterion ("Follow-up implementation issues opened only after spec acceptance"), no new issues were filed in this PR. The deferred-schema work flagged by Codex comment #2 (stable idempotency key + retry-policy fields) is named in the spec's §"Future schema work (deferred)" and in §"Open questions and deferred decisions"; filing will happen after #1797 reaches a closure decision.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [ ] Push the post-review fix commit and confirm CI green on the new head.
- [ ] Reply to all five PR #1819 review threads with explanation + commit SHA.
- [ ] Confirm `mergeStateStatus` returns to `CLEAN` after the push (was `UNSTABLE` before fixes — caused by the github-actions freshness comment, not a check failure).
- [ ] Squash-merge #1819 if clean.
- [ ] Decide whether #1797 should be closed based on the merged doc satisfying its six acceptance criteria. (Left for human review per the session's PR body.)
- [ ] If #1797 is closed, file the follow-up implementation issues for the deferred-schema items named in the spec's §"Future schema work (deferred)" and §"Open questions and deferred decisions."

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`EffectDispatchEvidence` schema is stable.** The post-review idempotency section says today's `subsystem`/`receipt_ref`/`success`/`EffectOutcome` fields are what the contract observes against. I confirmed these fields in `dispatch_evidence.rs` lines ~30–70, but did not exhaustively confirm that no in-flight PR adds an idempotency-intent field. If one is in flight, the spec's "Future schema work" section may be redundant rather than forward-direction.
- **`InstitutionalEffectRecord.decision_hash` is reliably populated.** The current-contract idempotency check assumes `(proposal_id, effect_kind)` plus a populated `decision_hash` is the existing duplicate-detection surface. The struct doc says `decision_hash` is `Some` "when the governance decision receipt was available at record-persistence time" and `None` only in edge cases. I did not measure how often that edge case fires in practice.
- **`GovernanceDecisionReceipt` is the institutional-memory record and `GovernanceProof` is the signature carrier.** Codex flagged this distinction; I confirmed both type names exist in `icn-governance/src/proof.rs` (lines 54 and 224 respectively), but did not read every field of each struct. The reworded Stage 1 prose treats them as institutional-record vs signature-carrier; if a later read of `proof.rs` reveals that `GovernanceDecisionReceipt` itself carries a signature field, the reword would need a second pass.
- **ADR-0025's `EffectRecord` taxonomy is still in `proposed` state.** I cited it as forward-direction. I did not check whether a subsequent commit on `main` has promoted it.
- **The `docs/spec/` prefix default's `truth_class = "normative"` is what the registry resolves to at validation time.** I dropped the overrides on the assumption that this is the correct effective classification. If the validator surfaces a YAML/registry truth_class mismatch on the next strict run, the assumption needs revisiting.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Run validation:
   ```bash
   python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
   python3 docs/scripts/lint-arch.py docs/spec/effect-dispatch-contract.md --cargo icn/Cargo.toml
   python3 .github/scripts/compliance_linter.py
   python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
   ```
2. Commit with a scoped message (`docs(spec): address effect dispatch review feedback`).
3. Push the fix commit. CI should green on the same checks as before (all required pass; non-required skipping for docs-only).
4. Reply to each of the five review threads with concise explanation + the fix commit SHA.
5. Re-check `mergeStateStatus` (expected `CLEAN` once the review comments are resolved).
6. Squash-merge if clean.
7. Update local main; force-delete the local branch (squash-merge invalidates ancestor for `-d`).
8. Decide on #1797 closure separately. If closed, file the follow-up implementation issues named in the spec's deferred-schema sections.

After #1819 merges, the next safe spec PR is **#1794** (`InstitutionalDomain` and `DomainPolicy` primitive). The user explicitly asked that #1794 not start this session.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **No new primitives.** The spec deliberately uses every name from existing code. Naming new types would have invited duplication with ADR-0014/0019/0025; the project's gap is documentation, not data shape.
2. **Idempotency is split current vs future.** The behavioral doctrine (no double-mutation) holds today; the stable `(decision_hash, manifest_hash, effect_index)` key and retry-policy field are explicitly named as future schema work to avoid a `MUST` the current schema cannot honor. This was a direct response to Codex review.
3. **`GovernanceDecisionReceipt` is institutional memory; `GovernanceProof` and `GovernanceDecisionAttestation` are signature carriers.** The spec separates anchoring from authentication. This was a direct response to Codex review.
4. **`docs/spec/` prefix defaults apply.** The registry entry no longer overrides `truth_class` and `role`. The doc is classified as a normative spec; the YAML `Status: normative` matches.
5. **First runtime dogfood slice = #1748 process-transition receipts.** Chosen because it exercises the full chain end-to-end without requiring kernel-side mandate enforcement (which remains deferred per ADR-0019).

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn

# Branch + PR state
git branch --show-current
gh pr view 1819 --json state,mergeable,mergeStateStatus

# Validation suite
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 docs/scripts/lint-arch.py docs/spec/effect-dispatch-contract.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Review thread state
gh api repos/InterCooperative-Network/icn/pulls/1819/comments \
  --jq '.[] | {id, user: .user.login, line, in_reply_to_id, created_at}'
```

Expected: doc_control passes (with the 53 pre-existing unrelated warnings, none on the new doc); lint-arch CLEAN; compliance_linter no violations; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness (recorded honestly, not in this PR's scope to fix).

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/STATE.md` (last reviewed 2026-05-07, current as of the spine merge earlier today) and from `docs/PHASE_PROGRESS.md` (last updated 2026-05-07). No conflict surfaced during this session.
- **Implementation truth:** verified by reading `icn-governance/src/proof.rs`, `mandate.rs`, `authority.rs`, `effect_manifest.rs`; `icn-kernel-api/src/effects.rs`, `governance.rs`; `apps/governance/src/grant_minting.rs`, `institutional_effect.rs`, `dispatch_evidence.rs`. Confirmed type names, field shapes, and module docstrings.
- **Execution truth:** confirmed branch `spec/governance-effect-dispatch-contract`, PR #1819 OPEN, all required CI green, two Codex P2 review threads and three Copilot review threads open before the fix commit.
- **Known truth-plane conflict:** `freshness-check.py` reports `docs/ARCHITECTURE.md` as carrying stale sections (06-identity-cryptography, 10-state-ledger, 11-contract-execution, 12-governance, 14-federation). The CI workflow treats this as informational and reports SUCCESS; the script itself exits 1. Both are correct in their own frame. This PR does not broaden scope to address it. (Same pattern recorded in the previous handoff for #1814.)
