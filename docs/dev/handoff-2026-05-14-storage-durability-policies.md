# Session Handoff — 2026-05-14 (storage durability policies spec)

## Session Goal

Define `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, and `IntegrityPolicy` as design-level forward-direction policy objects so the storage-durability half of the operating model is precise enough for later schema, provider, and proof-loop work to be filed safely.

## Decisive Test

Can a downstream implementer — looking only at this spec plus the canon it harmonizes with — name (1) what each of the six policy objects carries at design granularity, (2) how `StorageSpec` ties to the merged `GovernedServiceBinding` and `DomainPolicy`, (3) what the locality / privacy inheritance rule is and why it cannot be silently broadened, (4) what a restore-test receipt MUST record, (5) which authority class covers a restore of authoritative state? If yes, the spec has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`09331622d` — `docs(spec): define governed service binding, workload manifest, and runtime provider (#1822)` (merged earlier today).

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| #1823 | `spec/storage-durability-policies` | OPEN | All required checks passed on commit `e375687b3` (initial spec push). Two Copilot threads landed after the initial no-feedback polling window; a fix commit follows this handoff edit and must re-run CI before merge. | Awaiting CI re-run on the Copilot fix commit, then mergeability re-check |
| #1790 | dependabot (pilot-ui dev deps) | OPEN | n/a | Out of session scope |
| #1791 | dependabot (ts-sdk dev deps) | OPEN | n/a | Out of session scope |

### Branches
- `spec/storage-durability-policies` — pushed to origin. Last verified head before this fix: `e375687b3`. The head moves with each push; check `git rev-parse HEAD` on the branch after the next push for the current SHA.
- Local `main` synced to `09331622d` (unchanged this session).

### Issues
- `#1816` — OPEN. This session writes the candidate spec doc. PR will use `Refs:`, not `Closes:`.
- `#1815` — OPEN (your manual closure decision deferred; PR #1822 used `Refs:` not `Closes:`).
- `#1767`, `#1792`, `#1798`, `#1799`, `#1801`, `#1818` — sibling storage / privacy / network / compute / mobile issues, all OPEN.
- `#1797`, `#1793`, `#1817` — closed previously.
- `#1794` — OPEN (deferred closure decision).

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Spec doc drafted (`docs/spec/storage-durability-policies.md`, ~490 lines)

The doc:

- **Defines six policy objects at design granularity.** `StorageSpec` (binding identifier, custody class, kernel `StorageClass`, `DataLocality`, privacy class, capacity, six policy references, export requirement, deletion expectation, receipt expectations). `BackupPolicy` (cadence class, target locality + custody, privacy inheritance, encryption / key custody, retention window, integrity check, restore-test cadence, authority, receipt classes). `ReplicationPolicy` (replica count class, placement constraints, anti-entropy expectation, divergence detection, repair authority, replica lifecycle authority, receipt classes). `RecoveryPolicy` (restore objective class — NOT SLA, restore authority, drill cadence, restore-target constraints, full / partial / point-in-time, challenge / reversal / audit, receipt classes). `ArchivePolicy` (long-term retention class, immutability expectation, access path, redaction, institutional hold, export, verification cadence, expiration authority, receipt classes). `IntegrityPolicy` (verification cadence, proof shape, repair path, relationships to replication and recovery and anti-entropy, receipt classes).
- **Maps the seven-class custody taxonomy** (canonical store / service state / artifact-blob / volume-block / scoped vault / secret-key material / cache-derived) to the implemented kernel-level `StorageClass` (3 variants) verbatim. Per-class design expectations cover custody, locality, privacy, backup, replication, archive, restore authority, export, receipts.
- **Surfaces the drift honestly.** The 7-class taxonomy (spine doc) and the 3-variant kernel enum (`icn/crates/icn-kernel-api/src/storage.rs` line 53) overlap but do not align 1:1. Reconciliation is deferred to a named follow-up; both stand today.
- **Restore-test receipts in their own section.** What a drill is, what triggers, what constraints (no silent mutation of authoritative state), what the receipt records (source, target, authority, verification outcome, convergence time), what failure means (governance-relevant event surfaced in the steward cockpit), how it surfaces in the member shell.
- **The single load-bearing inheritance rule** in §"Locality and privacy inheritance", stated three ways: backups, replicas, archives, exports, and restore targets inherit the source's `DataLocality` and privacy / disclosure constraints; policy MAY narrow disclosure; policy MAY NOT broaden locality or disclosure without an explicit governance act.
- **Backup / export / restore authority** mapped to ADR-0014 (`AuthorityClass`, `Mandate`) and the effect dispatch chain. Routine backup execution is operational; restore of authoritative state is governed; emergency authority is bounded, reviewable, receipt-backed.
- **Cross-links every sibling issue.** `#1767` (encrypted private overlay), `#1792` (disclosure boundary), `#1798` (ArtifactRegistry / ScopedVault), `#1799` (anti-entropy proof loops), `#1801` (compute placement), `#1818` (member shell), `#1795` (steward cockpit). No duplication.
- **Sixteen-row failure / safety table.** Each failure mode has a deterministic response. Cross-cutting rule: fail closed, surface honestly, never silently fall back.
- **Eleven open questions** with named forward homes.

### 2. Registered the doc (`docs/registry.toml`, +13 lines before the `governed-service-binding.md` entry)

Same registry pattern as #1819 / #1820 / #1821 / #1822: `category = "architecture"`, `status = "draft"`, no `truth_class` / `role` overrides (lets the `docs/spec/` prefix default supply `truth_class = "normative"`, `role = "spec"`). Carries description, last_updated / last_reviewed, owner, audiences, domain_tags, depends_on, recommended_action.

### 3. Added the spec to `docs/INDEX.md` Specifications section

Proactively added before any reviewer surfaces it. INDEX.md's Specifications section now lists all five recent specs (KERNEL_CONTRACTS, effect-dispatch, institutional-domain, ccl-policy-registry, governed-service-binding, storage-durability-policies).

### 4. Regenerated `docs/DOCUMENT_REGISTRY.md`

Ran `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. Diff is small (corpus 804 → 805 markdown files; explicit rows updated; `Last Reviewed` 2026-05-14).

### 5. Followed prior review-cycle lessons proactively

- YAML frontmatter `Status: normative`; matches `docs/spec/` prefix default.
- Doc opening labeled "spec, work-in-progress" — honest about which clauses are forward-direction.
- MUST claims gated on existing types. Where the abstract model adds new names, those are explicitly forward-direction.
- Verified all 23 cross-link targets exist before commit.
- Used canonical vocabulary verbatim: `StorageClass` (with `Canonical` / `ServiceState` / `Blobs`), `DataLocality` (with `CellLocal` / `CoopReplicated` / `FederationMirrored` / `CommonsPublic`), `GovernanceProof` / `GovernanceDecisionReceipt` / `ArtifactReceipt` / `FederationProvenance`, `Mandate` / `AuthorityGrant` / `AuthorityClass`, `EffectManifest` / `KernelEffect`, `DomainPolicy`, `GovernedServiceBinding`, `WorkloadManifest`. Forbidden vocabulary only in the explicit anti-claim sentence.
- Acknowledged the 7-class vs 3-variant `StorageClass` taxonomy drift explicitly (lesson from the Codex P2 catch on #1822 about ADR-0030's privacy-class names vs the implemented `PrivacyClass` enum).

### 6. Drafted follow-up issues (not filed)

Per the established pattern. Six titles named in §"Next Move" below.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Branch created (`spec/storage-durability-policies`).
- [x] Spec doc drafted; registry updated; INDEX.md updated; DOCUMENT_REGISTRY.md regenerated.
- [x] Validation suite passed locally. *(doc_control_check pass with 54 pre-existing/generated-doc warnings; lint-arch CLEAN; compliance_linter clean; freshness exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness — same pattern as #1814 / #1819 / #1820 / #1821 / #1822.)*
- [x] Initial commit (`e375687b3`) and push.
- [x] PR #1823 opened.
- [x] Initial CI passed on `e375687b3` (all required checks green; non-required skip).
- [x] Two Copilot review threads landed after the initial no-feedback polling window: (1) opening-paragraph internal inconsistency about "wire-stable form" and sibling-spec list; (2) handoff Final State stale recording the branch as local/pending push.
- [x] Copilot review-feedback fix applied in this session's pending commit.
- [ ] Push the Copilot fix commit and re-run CI.
- [ ] Recheck `mergeStateStatus` after CI returns green.
- [ ] Reply to / resolve the two Copilot threads with the fix commit SHA.
- [ ] Per the user's current prompt: "If checks are green, mergeability is clean, and no valid unresolved feedback remains, squash-merge PR #1823." Squash-merge if clean.
- [ ] Sync local `main` and prune the branch (squash-merge requires `git branch -D`).
- [ ] Leave `#1816` closure to human review (PR uses `Refs:`, not `Closes:`).
- [ ] Decide whether to file the follow-up issue drafts listed in §"Next Move §5" (deferred to user).
- [ ] After `#1816` is closed by the user (if they decide closure is warranted), the recommended next PR is `#1798` (`ArtifactRegistry` v0 and `ScopedVault` boundary). The user explicitly asked not to start `#1798` / `#1799` / `#1801` / `#1818` / implementation in this session.

### Carried forward (not addressed in this PR)

**`docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`** — `AGENTS.md` lines 281–313 still say `docs/dev-journal/`; `docs/dev/HANDOFF_TEMPLATE.md` (lines 10, 109–111) and 15+ merged PRs use `docs/dev/`. The `docs/dev-journal/` directory does not exist. Surfaced in #1820 review; carried through #1821, #1822, and now this PR. Recommended one-line edit. **Not addressed in this PR** to keep scope tight.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`StorageClass` and `DataLocality` definitions in `icn/crates/icn-kernel-api/src/storage.rs` are stable.** Verified by reading the file (line 53 + line 155). If a follow-up PR has amended the variant set since the Explore agent's read this session, my "reuse verbatim" framing may need adjustment.
- **No `StorageSpec` / `BackupPolicy` / `ReplicationPolicy` / `RecoveryPolicy` / `ArchivePolicy` / `IntegrityPolicy` types exist in code.** Verified by Explore-agent grep (no matches). If a parallel in-flight PR is adding any of these, my "forward-direction" framing for those names is wrong.
- **`storage-governance-spec.md` is the foundational storage doc.** I treated it as the canonical source for `StorageClass` and `DataLocality`. If a follow-up doc has been written or it has been deprecated, my harmonization framing may need revisiting.
- **The 7-class custody taxonomy from the spine doc is current canon.** Mapped to the kernel-level `StorageClass` enum with an explicit drift note. If the spine has been amended to align the taxonomies, the drift note is unnecessary; otherwise it stays.
- **`#1767`'s `PrivateOverlay` + `SecretCredential` privacy classes are still the forward-direction names.** I referenced them via `#1792`'s `PrivacyClass` taxonomy. If those names have changed in an in-flight draft of `#1767` or `#1792`, my privacy-inheritance section needs a re-read.
- **The 23 cross-link targets exist at session start.** Verified by file-existence sweep before committing. If a downstream PR renames or removes one before `#1816` lands, the spec's link will rot.
- **The Codex P2 lesson from #1822 (ADR text vs implementation drift) was correctly internalized.** The spec acknowledges the 7-class vs 3-variant `StorageClass` drift explicitly and names reconciliation as forward work. If a reviewer disagrees with the framing, the reword pattern from #1822 applies.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Push the Copilot fix commit prepared in this session. Watch CI to settle.
2. Reply to / resolve each open Copilot thread on PR #1823 with the new fix commit SHA. The two open Copilot comments are: (a) opening-paragraph "wire-stable form" / sibling-spec list conflation; (b) handoff Final State staleness.
3. Re-check `mergeStateStatus` after CI returns green.
4. Per the user's current prompt: squash-merge PR #1823 if clean. Sync local `main`; force-delete the branch (`git branch -D spec/storage-durability-policies`) since squash-merge invalidates the ancestor check for `-d`.
6. **Follow-up issue drafts (deferred; not filed in this PR):**
   - `schema(storage): define StorageSpec and durability policy records` — the wire-stable schema deliberately omitted.
   - `spec(storage): define restore-test receipt envelope` — the Rust type, persistence, and audit-query surface.
   - `spec(storage): define locality/privacy inheritance checks` — the `validate_storage_access()`-style enforcement function named as TBD in `storage-governance-spec.md`.
   - `spec(storage): define archive verification and export contract` — per-archive proof shape, hand-off protocol, custodian agreement.
   - `spec(storage): define backup provider / runtime provider interface` — how a `BackupPolicy` is executed by a `RuntimeProvider` per `#1815`.
   - `spec(anti-entropy): connect IntegrityPolicy receipts to network proof loops` — the integration with `#1799`.
   - Optional: `spec(storage): reconcile 7-class custody taxonomy with kernel StorageClass enum` — if the user wants the drift formally resolved.
7. **Separate process cleanup recommendation (carried from #1820 review):** `docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`. Not in this PR's scope.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **Six policy objects at design granularity.** `StorageSpec` is the binding; the other five are referenced policies. None introduces a Rust type; all are forward-direction.
2. **Reuse kernel `StorageClass` and `DataLocality` verbatim.** The 3-variant kernel enum and the 4-level locality enum from `icn/crates/icn-kernel-api/src/storage.rs` are canonical. The spec does not redefine them.
3. **Acknowledge the 7-class custody taxonomy as a richer design-level grouping.** Reconciliation with the 3-variant kernel enum is forward work, not a hidden assumption.
4. **Locality and privacy inheritance is mandatory.** Backups, replicas, archives, exports, and restore targets MUST inherit source constraints. Policy MAY narrow disclosure; policy MAY NOT broaden locality or disclosure without explicit governance.
5. **Restore of authoritative state is governed.** The restore-execution receipt is the institutional record; emergency restore authority is bounded, reviewable, receipt-backed.
6. **Restore drills produce real receipts.** A failed restore test is a governance-relevant event, not a hidden warning. The steward cockpit surfaces it; member shell summarizes the recovery posture (not the data).
7. **Replication is not backup.** Both share a verification surface (`#1799`'s anti-entropy proof loops) but answer different institutional questions.
8. **Archive is not quiet deletion.** Archive expiration is a governance act with a named authority and an expiration receipt.
9. **Integrity verification reuses existing hash / proof vocabulary** (`blake3` content hashes per `icn-ccl/src/registry.rs`, Merkle proofs per `icn-store/src/lib.rs`). The spec does not introduce new hash algorithms or proof formats.

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
python3 docs/scripts/lint-arch.py docs/spec/storage-durability-policies.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Review thread state
gh api repos/InterCooperative-Network/icn/pulls/<pr-number>/comments \
  --jq '.[] | {id, user: .user.login, line, in_reply_to_id, created_at}'
```

Expected at session start before push: doc_control_check pass with 54 enforcement warnings (none on the new doc); lint-arch CLEAN (already verified); compliance_linter no violations; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness.

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/STATE.md` and `docs/PHASE_PROGRESS.md`. No conflict surfaced.
- **Implementation truth:** verified by reading `icn-kernel-api/src/storage.rs` (lines 53, 155, 272) for the canonical `StorageClass`, `DataLocality`, `StorageValidationError` enums. Verified that none of the six policy objects exist as Rust types today. Verified that `RestoreReceipt` / `BackupReceipt` / `IntegrityReceipt` do not exist; spec defers their wire-stable form to follow-up.
- **Execution truth:** confirmed `main` HEAD `09331622d`, two dependabot PRs open, no other open PRs. `#1816` confirmed OPEN with the body matching the user's prompt.
- **Known truth-plane conflict #1:** `freshness-check.py` reports `docs/ARCHITECTURE.md` as carrying stale sections (06-identity-cryptography, 10-state-ledger, 11-contract-execution, 12-governance, 14-federation). CI workflow treats this as informational and reports SUCCESS; the script itself exits 1. Same pattern as #1814, #1819, #1820, #1821, #1822.
- **Known truth-plane conflict #2 (the spec surfaces honestly):** The 7-class custody taxonomy in the spine doc vs the 3-variant `StorageClass` kernel enum. Both stand today; reconciliation is named as forward work.
- **Known process drift (carried, not in this PR):** AGENTS.md vs `docs/dev/HANDOFF_TEMPLATE.md` disagree on handoff path. De-facto canon is `docs/dev/handoff-*.md`. Recommend separate scoped fix.
