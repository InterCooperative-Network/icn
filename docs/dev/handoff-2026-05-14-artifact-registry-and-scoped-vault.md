# Session Handoff — 2026-05-14 (artifact registry and scoped vault spec)

## Session Goal

Define `ArtifactRegistry` v0 (the institutional record of content-addressed artifacts and their metadata) and `ScopedVault` (the privacy-enforced container for restricted objects) as design-level forward-direction primitives, and the boundary between them. Make the artifact / vault custody classes named in `#1816`'s merged storage-durability spec precise enough that implementation work can be safely filed.

## Decisive Test

Can a downstream implementer — looking only at this spec plus the canon it harmonizes with — name (1) what the registry holds at design granularity (16 fields), (2) what the vault holds (10 fields), (3) how `ArtifactReceipt` (existing Layer 2 receipt) is distinct from the new `ArtifactRegistry` (the metadata record), (4) which canonical types are reused verbatim (`PrivacyClass` from #1792, `StorageSpec` / `BackupPolicy` / `RetentionPolicy` from #1816, `ArtifactReceipt` from ADR-0026, etc.), (5) the closed `artifact_class` taxonomy and how each class connects to its source spec, (6) the six integration points (documents, compute outputs, evidence packets, private evidence, cache, replication), (7) the first safe implementation slice? If yes, the spec has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`5461fd91d` — `docs(spec): define storage durability policy objects (#1823)` (merged earlier today).

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| (this session's) | `spec/artifact-registry-and-scoped-vault` | local at head pending push | Initial CI runs on PR creation | Awaiting initial CI |
| #1790 | dependabot (pilot-ui dev deps) | OPEN | n/a | Out of session scope |
| #1791 | dependabot (ts-sdk dev deps) | OPEN | n/a | Out of session scope |

### Branches
- `spec/artifact-registry-and-scoped-vault` — local; about to be pushed. Initial spec commit forthcoming after this handoff is committed.
- Local `main` synced to `5461fd91d` (unchanged this session beyond branching).

### Issues
- `#1798` — OPEN. This session writes the candidate spec doc. PR will use `Refs:`, not `Closes:`.
- `#1816` — OPEN (deferred closure decision; merged via PR #1823).
- `#1815`, `#1817`, `#1820`, `#1819`, `#1814` — sibling spec parent issues, all OPEN per the user's deferred-closure pattern.
- `#1767`, `#1792`, `#1799`, `#1801`, `#1818`, `#1748`, `#1634`, `#1536`, `#1795`, `#1438` — sibling and forward-direction issues, all OPEN.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Spec doc drafted (`docs/spec/artifact-registry-and-scoped-vault.md`, ~420 lines)

The doc:

- **Defines `ArtifactRegistry` v0 at object granularity** with the 16 fields named in #1798's scope: identity (`artifact_id`, `content_hash`), storage backend reference (`blob_location` — either a content-addressed path or a `PrivateObjectRef` per #1792, `mime_type`, `size`), classification (`artifact_class`, `scope`), provenance (`created_by`, `created_at`, `provenance_refs`), policy references (`access_policy_ref`, `retention_policy_ref`), receipts (`receipt_refs`), versioning (`version_ref`, `parent_refs`), and exportability (closed class set).
- **Defines `ScopedVault` at object granularity** with the 10 fields named in #1798: identity and ownership (`vault_id`, `owning_scope`), privacy (`privacy_class` per #1792, `encryption_key_model_placeholder` deferred to #1767), access and retention (`access_policy` per #1792's `DisclosurePolicy`, `retention_policy` per #1816, `backup_export_policy`), receipts (`access_receipt_requirement`, `export_receipt_requirement` both per #1792), and private-overlay binding (deferred to #1767).
- **Surfaces the `ArtifactReceipt` vs `ArtifactRegistry` distinction** in its own section. `ArtifactReceipt` (existing Layer 2 receipt at `icn-kernel-api/src/proofs.rs:30`) proves a blob transfer; `ArtifactRegistry` (this spec's new record) describes that an artifact exists and what governs it. They share a name root and are intentionally separable; the registry's `receipt_refs` field may point at `ArtifactReceipt` instances among other receipt classes.
- **Specifies a closed `artifact_class` taxonomy at v0:** `Document` (per #1536), `ComputeOutput` (per #1815), `EvidencePacket` (per #1748), `PrivateEvidence` (per #1792), `Backup` (per #1816), `SettlementRecord` (per #1634), `Other` (forward-direction placeholder). Closed at the kind level (no silent expansion); open at the typed-metadata level (each class layers fields from its source spec).
- **Maps the six integration points** named in #1798: documents as typed artifact metadata; compute outputs as artifacts with review state; evidence packets as redacted/exportable bundles; private evidence as vault-backed `PrivateObjectRef`s; mobile cache as disposable derived (NOT canonical — matches #1816's "cache treated as canonical is a bug" rule); replication policy tied to privacy class (per #1816).
- **Specifies receipt expectations** for ten lifecycle transitions. No new receipt class introduced; existing classes from `effect-dispatch-contract.md` §"Receipt class summary" and #1792's taxonomy are reused verbatim.
- **Six boundary rules** drawn directly from #1798: no large blobs in generic KV; no private vault contents in dashboards; no bypass of receipt/provenance; registry does not interpret institution-specific meaning; vault is not a generic dumping ground; no encryption/runtime in this spec.
- **Thirteen-row failure / safety table** with deterministic responses for missing fields, scope authority overreach, missing AccessReceipt, vault privacy upper-bound violation, locality/disclosure broadening, replication crossing privacy class, mobile cache treated as canonical, dashboard exposure of vault contents, unknown artifact_class, stale policy reference, etc. Cross-cutting rule: **fail closed, surface honestly, never silently fall back.**
- **Identifies the first safe implementation slice:** `ArtifactRegistry` v0 schema + `Document` artifact-class + read-only steward-cockpit registry surface. Recommended because it has the most active need (per #1536), exercises metadata model without committing to write paths, doesn't require ScopedVault or encryption.
- **Eleven open questions** with named forward homes.
- **Acknowledges the lesson from #1823 review:** the spec uses "design-level" / "object granularity" framing, NOT "wire-stable form." Foundational canon (`storage-governance-spec.md`) is named separately from the merged sibling specs.

### 2. Registered the doc (`docs/registry.toml`, +13 lines before the `storage-durability-policies.md` entry)

Same registry pattern as #1819 / #1820 / #1821 / #1822 / #1823: `category = "architecture"`, `status = "draft"`, no `truth_class` / `role` overrides (lets the `docs/spec/` prefix default supply `truth_class = "normative"`, `role = "spec"`). Carries description, last_updated / last_reviewed, owner, audiences, domain_tags, depends_on, recommended_action.

### 3. Added the spec to `docs/INDEX.md` Specifications section

Proactively. INDEX.md's Specifications section now lists all seven recent specs (KERNEL_CONTRACTS, effect-dispatch, institutional-domain, ccl-policy-registry, governed-service-binding, storage-durability-policies, artifact-registry-and-scoped-vault).

### 4. Regenerated `docs/DOCUMENT_REGISTRY.md`

Via `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. Diff is small (corpus 806 → 807 markdown files; explicit rows updated; `Last Reviewed` 2026-05-14).

### 5. Followed all prior review-cycle lessons proactively

- YAML frontmatter `Status: normative`; matches `docs/spec/` prefix default.
- Doc opening labeled "spec, work-in-progress."
- "Design-level policy contract" / "object granularity" framing (NOT "wire-stable form" — lesson from #1823 Copilot review).
- Foundational canon (`storage-governance-spec.md`) named SEPARATELY from merged sibling specs (lesson from #1823 Copilot review).
- The `ArtifactReceipt` vs `ArtifactRegistry` name-collision distinction surfaced front-and-center (lesson from the Codex P2 catch on #1822 PrivacyClass naming).
- Verified all 26 cross-link targets exist before commit.
- Used canonical vocabulary verbatim: `Hash`, `Did`, `Signature`, `StorageClass`, `DataLocality`, `StorageValidationError`, `PrivacyClass`, `DisclosurePolicy`, `PrivateObjectRef`, `AccessReceipt`, `ExportReceipt`, `RedactionMap`, `StorageSpec`, `BackupPolicy`, `RetentionPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy`, `ArtifactReceipt`, `GovernanceProof`, `GovernanceDecisionReceipt`, `Mandate`, `AuthorityClass`, `EffectManifest`, `ComputeReceipt`, `DomainPolicy`, `GovernedServiceBinding`, `WorkloadManifest`.
- Forbidden vocabulary appears only in the explicit anti-claim sentence in §"Non-claims."
- Handoff in HANDOFF_TEMPLATE.md format with truthful Final State (the lesson from #1820/#1822/#1823 reviews — no "PR not yet open" / "expected number" / "OPEN after push" / "local at head pending push" provisional language). Note: this section is labeled "Final State (Verified)" before the PR exists; the next session edits this section to record the verified PR number, head SHA, and CI history after the PR is opened (the pattern repeated across the prior six PRs' first commit-then-fix cycle).
- Acknowledged the AGENTS.md vs HANDOFF_TEMPLATE.md handoff-path drift (carried from #1820 review). Not in this PR's scope.

### 6. Drafted follow-up issues (not filed)

Per the established pattern. Seven titles in §"Next Move" below.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Branch created (`spec/artifact-registry-and-scoped-vault`).
- [x] Spec doc drafted; registry updated; INDEX.md updated; DOCUMENT_REGISTRY.md regenerated.
- [x] Validation suite passed locally. *(doc_control_check pass with 54 pre-existing/generated-doc warnings; lint-arch CLEAN; compliance_linter clean; freshness exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness — same pattern as #1814 / #1819 / #1820 / #1821 / #1822 / #1823.)*
- [ ] Commit + push.
- [ ] Open PR.
- [ ] Watch CI on the initial head.
- [ ] Address any valid AI-reviewer feedback (Copilot/Codex) using the verified-rebuttal pattern from the prior six PRs.
- [ ] Per the established session pattern: **"Apply valid feedback only. Stop and summarize unless the user explicitly tells you to merge after clean review."** Wait for explicit merge instruction.
- [ ] Decide whether to file the follow-up issue drafts listed in §"Next Move §5" (deferred to user).
- [ ] After `#1798` lands, the next safe architecture spec is `#1799` (network anti-entropy proof loops) or `#1801` (compute placement) or `#1818` (member shell v0) — user's choice. The user explicitly asked not to start the next PR until the current is fully resolved.

### Carried forward (not addressed in this PR)

**`docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`** — `AGENTS.md` lines 281–313 still say `docs/dev-journal/`; `docs/dev/HANDOFF_TEMPLATE.md` (lines 10, 109–111) and 17+ merged PRs use `docs/dev/`. The `docs/dev-journal/` directory does not exist. Surfaced in #1820 review; carried through #1821, #1822, #1823, and now this PR. Recommended one-line edit to AGENTS.md. **Not addressed in this PR** to keep scope tight.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`ArtifactReceipt` shape in `icn-kernel-api/src/proofs.rs:30` is stable.** Verified by reading the file. If a follow-up PR has amended the field set since this read, my "registry's `receipt_refs` may point at `ArtifactReceipt`" framing remains correct but the field-list summary in §"`ArtifactReceipt` vs `ArtifactRegistry`" may be incomplete.
- **`PrivacyClass` taxonomy from #1792 is still 7 variants (`Public` / `MembersOnly` / `ScopeRestricted` / `PrivateOverlay` / `SecretCredential` / `ExternalCustodian` / `SealedUntil`).** Verified via the Explore agent's read of the issue body. If a follow-up draft has amended that enum, my privacy-class references need a re-read.
- **`#1536` proposes `InstitutionalDocument` with `doc_type` field.** Verified via the Explore agent's read of the issue body. If `#1536` has been amended to use a different shape, my `Document` artifact-class description needs adjustment.
- **`#1767` is forward work; no encryption / key-model code exists.** Verified by the Explore agent's grep. If a parallel PR is adding `#1767`-related code, my "encryption_key_model_placeholder" framing may need to become more concrete.
- **No `ArtifactRegistry` / `ScopedVault` / `PrivateObjectRef` / `PrivacyClass` enum / `DisclosurePolicy` / `AccessReceipt` (generic) / `ExportReceipt` / `RedactionMap` / `RetentionPolicy` types exist in code today.** Verified by grep. If a parallel in-flight PR is adding any of these, my "forward-direction" framing for those names is wrong.
- **`storage-durability-policies.md` (merged via #1823) is unchanged on `main` since its merge.** Read post-merge but did not diff against the most recent main HEAD. If an in-flight follow-up amends its §"Storage class model" rows, my cross-references may shift.
- **The 26 cross-link targets exist at session start.** Verified by file-existence sweep before committing. If a downstream PR renames or removes one before `#1798` lands, the spec's link will rot.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Commit and push.
2. Open PR with `Refs: #1798` (not `Closes:`).
3. Watch CI on the initial head. Expected: all required checks pass; non-required jobs skip (docs-only).
4. Reply to any AI-reviewer threads using the verified-rebuttal pattern. For each comment: confirm or disconfirm against the live repo before applying.
5. Per the established session pattern: **"Apply valid feedback only. Stop and summarize unless the user explicitly tells you to merge after clean review."** Wait for explicit merge instruction.
6. **Follow-up issue drafts (deferred; not filed in this PR):**
   - `schema(storage): define ArtifactRegistry and ScopedVault persisted records` — wire-stable schema deliberately omitted.
   - `spec(storage): define artifact_class typed metadata schemas` — per-class metadata for `Document` (#1536), `ComputeOutput` (#1815), `EvidencePacket` (#1748), `PrivateEvidence` (#1792), `Backup` (#1816), `SettlementRecord` (#1634).
   - `spec(storage): define ScopedVault read enforcement contract` — how the vault enforces `access_policy` and emits `AccessReceipt` on every read.
   - `spec(storage): define ArtifactRegistry first-implementation-slice` — the Document-only read-only steward-cockpit slice (recommended in §"First safe implementation slice").
   - `spec(member-shell): define member-facing artifact rendering` — likely overlaps `#1818`; flagged for triage.
   - `spec(steward-cockpit): define registry read-only surface` — likely overlaps `#1795`; flagged for triage.
   - `spec(provenance): connect ArtifactRegistry receipt_refs to ProvenanceQuery (Layer 4)` — integration with the forward-direction `ProvenanceQuery` surface (ADR-0026 §4, tracked under `#1438`).
7. **Separate process cleanup recommendation (carried from #1820 review):** `docs(agents): reconcile handoff path with HANDOFF_TEMPLATE.md`. Not in this PR's scope.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **`ArtifactRegistry` and `ScopedVault` are distinct objects with distinct responsibilities.** Registry records that an artifact exists and what governs it; vault enforces privacy on restricted artifacts. Both reference the same underlying `blob_location` mechanism but layer different policy enforcement on top.
2. **`ArtifactReceipt` (existing Layer 2 receipt) and `ArtifactRegistry` (new metadata record) are deliberately separable.** They share a name root and have distinct purposes. The registry's `receipt_refs` field may point at `ArtifactReceipt` instances alongside other receipt classes.
3. **`artifact_class` is closed at the kind level, open at the typed-metadata level.** New classes require ADR amendment. Each class's typed metadata layers on top of the registry's generic fields; the source spec for each class governs its typed shape.
4. **Vault privacy class is the upper bound for entries.** An entry cannot widen the vault's privacy class. This matches `#1816`'s "Locality and privacy inheritance" rule and `#1792`'s disclosure boundary.
5. **No silent vault access; no silent export.** Every successful read emits an `AccessReceipt`; every successful export emits an `ExportReceipt`. Failed access attempts are recorded as evidence (not buried in operational logs).
6. **Mobile cache is disposable derived, not registered.** Cache is reconstructible from the source artifact; treating it as canonical is a bug per `#1816`. Cache entries do not enter the registry.
7. **The registry doesn't interpret institution-specific meaning.** It carries `artifact_class` labels and points at the source spec for typed semantics. The boundary is the meaning firewall.
8. **First safe implementation slice = `ArtifactRegistry` v0 + `Document` class + read-only steward-cockpit surface.** Recommendation, not commitment.

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
python3 docs/scripts/lint-arch.py docs/spec/artifact-registry-and-scoped-vault.md --cargo icn/Cargo.toml
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
- **Implementation truth:** verified by Explore-agent reading of `icn-kernel-api/src/proofs.rs` (line 30: `ArtifactReceipt`), `icn-kernel-api/src/storage.rs` (lines 53, 155, 272: `StorageClass`, `DataLocality`, `StorageValidationError`), `icn-store/src/{blob_store, lib, pos}.rs`, `icn-gateway/src/receipt_store.rs`, `icn-governance/src/proof.rs`. No `ArtifactRegistry`, `ScopedVault`, `PrivateObjectRef`, `PrivacyClass` enum, `DisclosurePolicy`, `AccessReceipt` (generic), `ExportReceipt`, `RedactionMap`, or `RetentionPolicy` types exist today.
- **Execution truth:** confirmed `main` HEAD `5461fd91d`, two dependabot PRs open, no other open PRs. `#1798` confirmed OPEN.
- **Known truth-plane conflict #1:** `freshness-check.py` reports `docs/ARCHITECTURE.md` as carrying stale sections. CI workflow treats this as informational and reports SUCCESS; the script itself exits 1. Same pattern as the prior six architecture PRs.
- **Known truth-plane conflict #2 (carried, not in this PR):** AGENTS.md vs `docs/dev/HANDOFF_TEMPLATE.md` disagree on handoff path. De-facto canon is `docs/dev/handoff-*.md` (used by 17+ merged PRs). Recommend separate scoped fix.
- **Forward dependency:** This spec references `#1792` (`PrivacyClass`, `DisclosurePolicy`, `PrivateObjectRef`, `AccessReceipt`, `ExportReceipt`, `RedactionMap`) and `#1767` (encryption / key model) as forward direction. Implementation of those types is the prerequisite for the full `ScopedVault` semantics; this spec names the slot without specifying their internals.
