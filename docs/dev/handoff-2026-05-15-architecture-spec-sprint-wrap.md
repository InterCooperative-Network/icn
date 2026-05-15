# Session Handoff — 2026-05-15 — Architecture-Spec Sprint Wrap

**Topic:** Post-sprint truth-sync / closure review after PR #1832 merged
**Branch:** `docs/architecture-spec-sprint-wrap`
**Closes:** none directly (this is a review, not a closure PR)
**Refs:** `#1794`, `#1795`, `#1797`, `#1798`, `#1799`, `#1801`, `#1815`, `#1816`, `#1817`, `#1818`, `#1829`, `#1830`, `#1831`, `#1832`

This handoff is the **post-sprint review**. It does not advance a new spec; it audits what landed against the issues the sprint advanced, drafts closure comments for human posting, deduplicates follow-up issue drafts collected across the per-PR handoffs, and lists truth-sync targets. **It does not close issues, file follow-ups, post comments, or update STATE / PHASE_PROGRESS / public docs.** Those steps are deferred to human decision.

---

## Session Goal

Audit the thirteen merged architecture-spec PRs against the open sibling-issue acceptance criteria. For each issue: classify as **close now**, **keep open with named gap**, **close with follow-up**, or **needs human decision**. Draft closure comments (not posted). Deduplicate the follow-up issue drafts that prior per-PR handoffs accumulated. Identify which truth-sync surfaces (STATE, PHASE_PROGRESS, INDEX, source-of-truth-map, website, labels/milestones) need updates after closures land. Preserve the cross-sprint non-claims.

---

## Decisive Test

This wrap-up fails if any of the following holds:

1. It closes an issue. It must only **draft** closure comments.
2. It files new follow-up issues. It must only **deduplicate** the drafts already in the per-PR handoffs.
3. It claims any of the merged specs implements anything beyond a design-level contract.
4. It updates `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, public-website surfaces, or repository labels / milestones. Those are explicitly deferred.
5. It opens or amends an ADR.
6. It misclassifies a sibling issue (e.g., calling something a close-candidate when its acceptance criteria still have unsatisfied items).
7. It carries the AGENTS.md handoff-path drift forward — that was resolved by #1827; future handoffs do not need to re-litigate.
8. It uses payment / wallet / balance / currency / token / crypto / blockchain framing outside negation context.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands -->

### `main` HEAD before branching

`eee935aa9e837cb85723fab8e341a80efb431888` — `fix(spec): correct steward cockpit review drift (#1832)`

### Branches

- `docs/architecture-spec-sprint-wrap` — branch for this review handoff. Head SHA recorded after commit.

### Open PRs at session start

| PR | Branch | Purpose |
|----|--------|---------|
| #1790 | `dependabot/npm_and_yarn/web/pilot-ui/dev-dependencies-19401a181c` | Dev-dep bumps (unrelated). |
| #1791 | `dependabot/npm_and_yarn/sdk/typescript/dev-dependencies-6a32bfed2f` | Dev-dep bumps (unrelated). |

No sprint-related PRs open.

### Sprint PR roster (chronological, all merged)

| PR | Merged at (UTC) | Title | Squash SHA |
|---|---|---|---|
| #1814 | 2026-05-14 09:22:05Z | docs(architecture): add integrated cooperative operating model spine | `d49c83b12` |
| #1819 | 2026-05-14 12:59:00Z | docs(spec): add accepted-proposal effect dispatch contract | `494dce9fa` |
| #1820 | 2026-05-14 13:36:53Z | docs(spec): define institutional domain and policy primitive | `100ecdbf7` |
| #1821 | 2026-05-14 14:26:48Z | docs(spec): define CCL policy registry and hook contract | `ca4bc683f` |
| #1822 | 2026-05-14 14:59:48Z | docs(spec): define governed service binding, workload manifest, and runtime provider | `09331622d` |
| #1823 | 2026-05-14 15:54:24Z | docs(spec): define storage durability policy objects | `5461fd91d` |
| #1825 | 2026-05-15 01:28:37Z | docs(architecture): define entity-scope vocabulary boundary | `1842e9839` |
| #1824 | 2026-05-15 01:48:46Z | docs(spec): define ArtifactRegistry v0 and ScopedVault boundary | `3af0d7ffd` |
| #1826 | 2026-05-15 02:44:10Z | docs(spec): define compute placement policy | `3a78052f6` |
| #1827 | 2026-05-15 03:06:39Z | docs(agents): reconcile handoff path with template | `2a5b75c58` |
| #1829 | 2026-05-15 08:53:22Z | docs(spec): define network anti-entropy proof loops | `0b3009478` |
| #1830 | 2026-05-15 12:22:20Z | docs(spec): define member shell v0 | `1a2b4607d` |
| #1831 | 2026-05-15 12:48:54Z | docs(spec): define steward cockpit v0 | `cee87b936` |
| #1832 | 2026-05-15 14:24:31Z | fix(spec): correct steward cockpit review drift | `eee935aa9` |

Fourteen PRs total: twelve substantive design-level spec PRs (#1814, #1819–#1826, #1829–#1831), one process-doc PR (#1827 reconciled the AGENTS.md handoff-path drift), and one post-merge drift-fix PR (#1832 corrected late-review feedback that landed on #1831).

### Issue states at session start

| Issue | State | Title |
|---|---|---|
| #1794 | OPEN | `docs(spec): define InstitutionalDomain and DomainPolicy primitive` |
| #1795 | OPEN | `ux(dashboard): define steward cockpit v0 for node and domain operators` |
| #1797 | **CLOSED** | `spec(governance): define accepted-proposal effect dispatch contract` |
| #1798 | OPEN | `spec(storage): define ArtifactRegistry v0 and ScopedVault boundary` |
| #1799 | OPEN | `test(devnet): define network routing, redundancy, and anti-entropy proof loops` |
| #1801 | OPEN | `spec(compute): define placement policy across local, cooperative, federation, and commons pools` |
| #1815 | OPEN | `spec(runtime): define governed service binding, workload manifest, and runtime provider model` |
| #1816 | OPEN | `spec(storage): define backup, replication, recovery, and archive policies` |
| #1817 | OPEN | `spec(ccl): define policy registry, versioning, and governance-effect hook contract` |
| #1818 | OPEN | `ux(mobile): define member shell v0 as primary participation surface` |

#1797 is the only sprint-related issue already closed; the other nine remain open pending human closure decision.

---

## What Changed in This Wrap-Up

### 1. `docs/dev/handoff-2026-05-15-architecture-spec-sprint-wrap.md` (this file)

A review-only handoff. No spec changes, no runtime changes, no closures. Contains:

- Sprint PR roster + chronology.
- Per-issue closure analysis evaluating each open sprint issue against its acceptance criteria, classified as close-now / keep-open / close-with-followup / needs-human-decision.
- Draft closure comments (one per close-now candidate) prepared for human posting.
- Deduplicated follow-up issue drafts grouped by concern area.
- Truth-sync target list with do-now / defer recommendations.
- Cross-sprint non-claims block.
- Recommended next decision sequence.

### 2. `docs/DOCUMENT_REGISTRY.md` (regenerated)

Corpus moves from 817 to 818 markdown files. No registry entry for this handoff (per `docs/dev/handoff-*.md` precedent — handoffs are not registered).

No spec, INDEX, or `docs/registry.toml` changes.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — explicitly preserved boundaries -->

### Deferred by this wrap-up

- [ ] Posting any of the draft closure comments below. Closure is a human decision.
- [ ] Filing any of the deduplicated follow-up issue drafts. Filing is a human decision (the per-PR handoffs already named the drafts; this wrap-up deduplicates and groups them).
- [ ] Updating `docs/STATE.md` to record the spec ladder.
- [ ] Updating `docs/PHASE_PROGRESS.md` to reflect the architecture-spec sprint completion.
- [ ] Updating `docs/INDEX.md` for closure-state changes (the per-spec INDEX entries already landed during the sprint).
- [ ] Updating `docs/reference/project-index/source-of-truth-map.md` for any of the merged specs.
- [ ] Updating public-website / pilot-ui copy to reflect the spec ladder.
- [ ] Adjusting issue labels / milestones to reflect closure batch.
- [ ] Deciding the next implementation or fixture PR (see §"Recommended next decision" below).

### Preserved boundaries

- No issue closure. Drafts only.
- No new follow-up issues filed.
- No spec changes. No ADR amendments.
- No runtime / code changes.
- No website / pilot-ui changes.
- No K3s, DNS, Forgejo, gateway, storage-backend, identity-bridge, or deploy-script changes.
- No edits to `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, `docs/registry.toml`, or any other state-of-the-project surface.

---

## Closure Analysis

Per-issue evaluation against acceptance criteria. Classification key: **CN** = close now; **KO** = keep open with named gap; **CF** = close with follow-up(s); **HD** = needs human decision.

### #1794 — `docs(spec): define InstitutionalDomain and DomainPolicy primitive`

**Verdict: CN — close now.** All six acceptance criteria satisfied by `docs/spec/institutional-domain.md` merged via #1820. Criterion-by-criterion: (1) spec merged; (2) domain authority separated from DNS/public routing in §"Boundary"; (3) domain separated from node and federation in §"DomainPolicy" → entity-class enumeration; (4) DomainPolicy maps to storage, compute, tools, routing, privacy, receipts, accessibility, translation, export (all sections present); (5) ICN core vs institution package boundary preserved (consumed by #1825 §C3); (6) follow-up schema/runtime issues identified in the #1820 handoff without starting implementation.

### #1795 — `ux(dashboard): define steward cockpit v0 for node and domain operators`

**Verdict: CN — close now.** All seven acceptance criteria satisfied by `docs/spec/steward-cockpit-v0.md` merged via #1831 and corrected via #1832. Criterion-by-criterion: (1) v0 spec merged; (2) steward/operator action cards defined as 14 named scenarios with the schema explicitly forward-direction (per the #1832 fix that reframed the ADR-0027 claim as a rendering-analog set with a follow-up `spec(contracts): define steward required-action card contract`); (3) member-shell concerns separated via six boundary lines, member-impact summary mapping, and the failure-table v0 violation when cockpit and shell disagree; (4) twelve cockpit surfaces cover storage, receipt, network, compute, accessibility, translation, privacy, backup/export posture; (5) Design principle 10 + failure-table row remove fintech / timebank / credit dashboard vocabulary; (6) §"Existing surfaces inspected" names what's powered by existing endpoints vs forward-direction; (7) five follow-up implementation drafts in the handoff (plus one added in #1832), none filed.

### #1797 — `spec(governance): define accepted-proposal effect dispatch contract`

**Already CLOSED.** Closed by #1819 merge (the issue was closed in that PR rather than left open with `Refs:`). Verification: `gh issue view 1797 --json state` returns `CLOSED`. No action.

### #1798 — `spec(storage): define ArtifactRegistry v0 and ScopedVault boundary`

**Verdict: CN — close now.** All six acceptance criteria satisfied by `docs/spec/artifact-registry-and-scoped-vault.md` merged via #1824. Criterion-by-criterion: (1) ArtifactRegistry v0 design merged; (2) ScopedVault boundary merged in the same doc; (3) artifact-class taxonomy explains how documents (`Document` artifact_class), compute outputs (`ComputeOutput`), evidence packets (`EvidencePacket`), and private evidence (`PrivateEvidence`) fit; (4) integration points enumerate the relationship to receipts, access policy, redaction/export, and replication; (5) first safe implementation slice named without starting it; (6) #1536 cross-linked at lines 54, 174, 190 and #1767 cross-linked at lines 17, 25, 34, 130, 145, 204, 244 of the spec.

### #1799 — `test(devnet): define network routing, redundancy, and anti-entropy proof loops`

**Verdict: CN — close now with note.** All five acceptance criteria satisfied by `docs/spec/network-anti-entropy-proof-loops.md` merged via #1829. Criterion-by-criterion: (1) design doc / test plan merged; (2) positive proof scenarios classified by proof level — partial; the criterion's "if available" clause for #1796 taxonomy was honored, classification using #1796 awaits that issue; (3) each scenario identifies required source paths/endpoints/tests and current gaps via the §"Existing code surface (anchors only)" table and §"First safe proof-loop / dogfood slice" section; (4) dashboard / member status mapping defined (steward cockpit 9 fields + member shell 7 strings); (5) eight follow-up implementation/test drafts in the handoff, none filed. The "if available" clause on criterion 2 makes #1796 dependency explicit; close-now is appropriate.

### #1801 — `spec(compute): define placement policy across local, cooperative, federation, and commons pools`

**Verdict: CN — close now.** All seven acceptance criteria satisfied by `docs/spec/compute-placement-policy.md` merged via #1826. Criterion-by-criterion: (1) placement policy spec merged; (2) defines seven placement classes (`LocalOnly`, `DomainLocalPreferred`, `LocalDomainBound`, `FederationBound`, `CommonsEligible`, `ExternalCustodianRequired`, `RejectedByPolicy`) with eighteen decision inputs and five candidate outputs; (3) Boundary rules + §"Decision contract" map placement to privacy, data locality, determinism, trust/admission, resource profile, settlement, receipts; (4) §"Fallback behavior" defines structured fallback; (5) §"Operator / steward dashboard" + §"Member shell" define both status languages; (6) §"First safe proof-loop / dogfood slice" names read-only placement rehearsal + dry-run fallback exercise; (7) cross-links ADR-0030, ADR-0031, #1794, #1795, #1798, #1799 in §"Cross-links."

### #1815 — `spec(runtime): define governed service binding, workload manifest, and runtime provider model`

**Verdict: CN — close now.** No formal numbered acceptance criteria; evaluating scope items. `docs/spec/governed-service-binding.md` merged via #1822 covers: `GovernedServiceBinding`, `WorkloadManifest`, `RuntimeProvider` (the three integrating primitives), ten-state lifecycle (declare → authorize → allocate → bind → run → observe → upgrade → suspend → remove → export), relationship to ToolBinding / hosted services / compute jobs / contract execution, receipt policy per lifecycle transition, seven closed runtime classes (deterministic legitimacy compute, utility computation, container, microVM, accelerator, local device, external bridge), eight boundary rules. Non-goals preserved: no runtime implementation, no schema/wire format, no production rollout, no service-specific bindings.

### #1816 — `spec(storage): define backup, replication, recovery, and archive policies`

**Verdict: CN — close now.** No formal numbered acceptance criteria; evaluating scope items. `docs/spec/storage-durability-policies.md` merged via #1823 covers: `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy`, restore-test receipts, locality and privacy inheritance, backup/export/restore authority. Doctrine encoded ("Redundancy keeps the service alive. Backups keep the institution recoverable. Archives keep the institution accountable. Disaster recovery proves the promises are not decorative."). Non-goals preserved: no storage backend implementation, no vendor integrations, no replication algorithm choice, no SLA commitments.

### #1817 — `spec(ccl): define policy registry, versioning, and governance-effect hook contract`

**Verdict: CN — close now.** No formal numbered acceptance criteria; evaluating scope items. `docs/spec/ccl-policy-registry.md` merged via #1821 covers: CCL policy registry, versioning (`policy_version_id` provenance), adoption (eight-step adoption contract binding CCL policy version to `DomainPolicy`), proposal kind → evaluator selection (deterministic, fail-closed on missing/conflicting/deprecated bindings), evaluator output → effect plan, review and audit surfaces. Rules encoded: CCL makes governance executable; unadopted documents are inert; CCL produces effect plans not unilateral mutations; models may draft text but governance adopts. Non-goals preserved: no CCL language grammar changes, no runtime implementation, no model-tooling integration, no partner-institution workflow.

### #1818 — `ux(mobile): define member shell v0 as primary participation surface`

**Verdict: CN — close now.** No formal numbered acceptance criteria; evaluating scope rules. `docs/spec/member-shell-v0.md` merged via #1830 covers every named scope rule: mobile-first + accessibility-first + offline-tolerant (Design principles 1, 2, 3), ActionCard derivation from standing, standing surface, receipts (three-tier rendering), offline mode (cache as derived), safe signing confirmations (ten-step flow), accessibility bound to `ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` (twelve-category gate inherited), multilingual / inclusive-access (Design principle 9, follow-up draft #5). Encoded rules: no financial-product framing (Design principle 10), not account management (boundary lines), member sees decisions / standing / open actions (information architecture). Non-goals preserved: no native-app implementation, no platform-specific choice, no partner skinning, no backend endpoint definitions.

---

## Draft Closure Comments — Not Posted

These are paste-ready for the issue's "Close issue" comment box. **Do not post until human review.**

### For #1794

```
Closing as the merged spec at docs/spec/institutional-domain.md (#1820) satisfies all six acceptance criteria:

1. ✓ Spec/design doc merged defining InstitutionalDomain, DomainPolicy, and adjacent concepts.
2. ✓ Doc clearly separates domain authority from DNS/public routing.
3. ✓ Doc clearly separates domain from node and federation.
4. ✓ Doc maps DomainPolicy to storage, compute, tools, routing, privacy, receipts, accessibility, translation, and export.
5. ✓ Doc preserves the ICN core vs institution package boundary (consumed by #1825 §C3).
6. ✓ Follow-up schema/runtime issues identified in the #1820 handoff without starting implementation.

Out of scope (preserved):
- No runtime implementation.
- No schema or wire format.
- No partner-institution policy.

This is a docs/spec-level closure, not a runtime-implementation closure. Implementation work continues under the named follow-ups; see docs/dev/handoff-2026-05-14-institutional-domain.md.
```

### For #1795

```
Closing as the merged specs at docs/spec/steward-cockpit-v0.md (#1831, corrected by #1832) satisfy all seven acceptance criteria:

1. ✓ Dashboard v0 spec merged.
2. ✓ Steward/operator Action Cards defined as 14 named scenarios. The wire-stable record shape is explicitly forward-direction; ADR-0027's member ActionCard schema does not extend to operator scenarios. A follow-up `spec(contracts): define steward required-action card contract` is named in the handoff.
3. ✓ Member-shell concerns separated via six boundary lines, member-impact summary mapping, and the failure-table v0 violation when cockpit and shell disagree.
4. ✓ Spec includes storage, receipt, network, compute, accessibility, translation, privacy, and backup/export posture across the twelve cockpit surfaces.
5. ✓ Stale fintech/timebank/credit dashboard vocabulary removed (Design principle 10 + failure-table row + non-claims block).
6. ✓ §"Existing surfaces inspected" identifies which fields can be powered by existing endpoints (icn-obs metrics, governance proof/receipt backend, ADR-0020 standing, ADR-0027 ActionCard) vs forward-direction work.
7. ✓ Five follow-up implementation drafts (plus one added in #1832) in the handoff; none filed.

Out of scope (preserved):
- No frontend technology decision (forward-direction; see follow-up `spec(web): pick the steward cockpit platform target`).
- No surveillance console; no private-data preview; no production-dashboard claim.
- ADR-0027 ActionCard schema is not reused for operator-required-action scenarios.

This is a docs/spec-level closure, not a cockpit-implementation closure.
```

### For #1798

```
Closing as the merged spec at docs/spec/artifact-registry-and-scoped-vault.md (#1824) satisfies all six acceptance criteria:

1. ✓ ArtifactRegistry v0 design/spec merged.
2. ✓ ScopedVault boundary design/spec merged (same doc).
3. ✓ Spec explains how documents (artifact_class = Document), compute outputs (ComputeOutput), evidence packets (EvidencePacket), and private evidence (PrivateEvidence) fit.
4. ✓ Spec defines relationship to receipts, access policy, redaction/export, and replication via six integration points.
5. ✓ First safe implementation slice named without starting it.
6. ✓ #1536 cross-linked (lines 54, 174, 190) and #1767 cross-linked (lines 17, 25, 34, 130, 145, 204, 244).

Out of scope (preserved):
- No encryption / key model implementation (deferred to #1767).
- No runtime implementation.
- No production claim.

This is a docs/spec-level closure, not a runtime-implementation closure.
```

### For #1799

```
Closing as the merged spec at docs/spec/network-anti-entropy-proof-loops.md (#1829) satisfies all five acceptance criteria:

1. ✓ Network proof-loop design doc merged.
2. ✓ "If available" clause honored — #1796 proof-level taxonomy is itself forward-direction; classification using it awaits #1796.
3. ✓ Each scenario identifies required source paths/endpoints/tests and current gaps via the §"Existing code surface (anchors only)" table (icn-gossip anti_entropy module, BloomFilter, VectorClock, PartitionDetector; icn-core spawn_anti_entropy_task; icn-federation crate; icn-obs metrics modules) and §"First safe proof-loop / dogfood slice" section.
4. ✓ Dashboard/member status mapping defined (steward cockpit 9 fields + member shell 7-string sync vocabulary).
5. ✓ Eight follow-up implementation/test drafts in the handoff; none filed.

Out of scope (preserved):
- No runtime/network implementation.
- No live federation claim.
- No private data movement.

This is a docs/spec-level closure, not a runtime-implementation closure. The #1796 taxonomy dependency is explicit and is the only un-bound piece of criterion 2.
```

### For #1801

```
Closing as the merged spec at docs/spec/compute-placement-policy.md (#1826) satisfies all seven acceptance criteria:

1. ✓ Placement policy spec merged.
2. ✓ Seven placement classes defined (LocalOnly, DomainLocalPreferred, LocalDomainBound, FederationBound, CommonsEligible, ExternalCustodianRequired, RejectedByPolicy) with eighteen decision inputs and five candidate outputs.
3. ✓ Placement mapped to privacy, data locality, determinism, trust/admission, resource profile, settlement, receipts via Boundary rules + §"Decision contract."
4. ✓ Structured fallback behavior defined.
5. ✓ Operator/steward dashboard (14 fields) and member-shell status language (7 strings) both defined.
6. ✓ §"First safe proof-loop / dogfood slice" names read-only placement rehearsal and dry-run fallback exercise.
7. ✓ Cross-links ADR-0030, ADR-0031, #1794, #1795, #1798, #1799 in §"Cross-links."

Out of scope (preserved):
- No scheduler / executor / admission engine implementation.
- No `DataLocality::CoopReplicated` migration (deferred to a named follow-up).
- No fuel/payment legacy reconciliation on `ComputeTask` (deferred to a named follow-up).

This is a docs/spec-level closure, not a scheduler-implementation closure.
```

### For #1815

```
Closing as the merged spec at docs/spec/governed-service-binding.md (#1822) addresses every scope item:

- ✓ GovernedServiceBinding, WorkloadManifest, RuntimeProvider — three integrating primitives defined.
- ✓ Lifecycle: declare → authorize → allocate → bind → run → observe → upgrade → suspend → remove → export — ten states defined.
- ✓ Relationship to ToolBinding (RFC-0017), hosted services (SERVICE_HOSTING_MODEL.md), compute jobs (ADR-0030 ComputeTask as the compute-specific projection of WorkloadManifest), contract execution.
- ✓ Receipt policy per lifecycle transition.
- ✓ Seven closed runtime classes: deterministic legitimacy compute, utility computation, container, microVM, accelerator, local device, external bridge.
- ✓ Eight boundary rules.

Non-goals preserved:
- No runtime implementation.
- No schema or wire format.
- No production rollout.
- No service-specific bindings.

This is a docs/spec-level closure, not a runtime-implementation closure. Wire-stable schema, generic RuntimeProvider trait, per-class provider specs, and federation-side binding recognition are tracked as named follow-ups in docs/dev/handoff-2026-05-14-governed-service-binding.md.
```

### For #1816

```
Closing as the merged spec at docs/spec/storage-durability-policies.md (#1823) addresses every scope item:

- ✓ StorageSpec, BackupPolicy, ReplicationPolicy, RecoveryPolicy, ArchivePolicy, IntegrityPolicy — six policy objects defined.
- ✓ Restore-test receipts defined.
- ✓ Locality and privacy inheritance rule: backups inherit source locality and disclosure; policy may narrow, may not broaden.
- ✓ Backup / export / restore authority rules.
- ✓ Doctrine encoded: "Redundancy keeps the service alive. Backups keep the institution recoverable. Archives keep the institution accountable. Disaster recovery proves the promises are not decorative."
- ✓ Sixteen-row failure / safety table.

Non-goals preserved:
- No storage backend implementation.
- No vendor-specific integrations.
- No replication algorithm choice.
- No SLA commitments.

This is a docs/spec-level closure, not a storage-backend-implementation closure. Wire-stable schema, restore-test receipt envelope, backup-provider interface, and anti-entropy integration are tracked as named follow-ups in docs/dev/handoff-2026-05-14-storage-durability-policies.md.
```

### For #1817

```
Closing as the merged spec at docs/spec/ccl-policy-registry.md (#1821) addresses every scope item:

- ✓ CCL policy registry — registration, addressing, discovery.
- ✓ Versioning — policy_version_id provenance + authorship + amendment path.
- ✓ Adoption — eight-step adoption contract binding CCL policy version to DomainPolicy.
- ✓ Proposal kind → evaluator selection — deterministic, fail-closed on missing/conflicting/deprecated bindings.
- ✓ Evaluator output → effect plan contract.
- ✓ Review and audit surfaces — registry shows drafts and adopted versions; receipts carry policy_version_id provenance.

Rules encoded: CCL makes governance executable; unadopted documents are inert; CCL produces effect plans not unilateral mutations; models may draft text, governance adopts.

Non-goals preserved:
- No CCL language grammar changes.
- No runtime implementation.
- No model-tooling integration.
- No partner-institution workflow.

This is a docs/spec-level closure, not a CCL-runtime-implementation closure.
```

### For #1818

```
Closing as the merged spec at docs/spec/member-shell-v0.md (#1830) addresses every scope rule:

- ✓ Mobile-first, accessibility-first, offline-tolerant (Design principles 1, 2, 3).
- ✓ ActionCard derivation from standing (rendering contract over ADR-0027's 14-field schema).
- ✓ Standing surface (six rendering elements).
- ✓ Receipts (three-tier rendering: plain summary → explanation → formal record).
- ✓ Offline mode (cache as derived; six rules; integration with #1829's seven sync-state strings).
- ✓ Safe signing confirmations (ten-step pre-confirm flow with reversibility, privacy, sync warnings).
- ✓ Accessibility bound to docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md (twelve-category gate inherited).
- ✓ Multilingual / inclusive-access (Design principle 9; follow-up draft for #1610 + #1740 integration).

Encoded rules:
- ✓ No financial-product framing (Design principle 10 + failure-safety row + non-claims).
- ✓ Not account management (boundary lines).
- ✓ Member sees decisions / standing / open actions (information architecture).

Non-goals preserved:
- No native-app implementation.
- No iOS / Android / PWA platform decision (deferred to follow-up).
- No partner-institution skinning.
- No backend endpoint definitions beyond reference to existing /me/standing and /me/action-cards.

This is a docs/spec-level closure, not a member-shell-implementation closure. Platform target, fixture slices, and Layer 4 ProvenanceQuery consumption are tracked as named follow-ups in docs/dev/handoff-2026-05-15-member-shell-v0.md.
```

---

## Deduplicated Follow-up Issue Drafts (Not Filed)

Aggregated from per-PR handoffs in the sprint. Deduplicated against the per-handoff lists; cross-cutting items consolidated. **Not filed** — this is the candidate list for a future batch-filing decision.

### Contracts / schema (8)

1. **`schema(network): define AntiEntropyProbe and StateDigest records`** — wire-stable record shapes for the probe and digest forms from #1829.
2. **`schema(network): define DivergenceEvidence and RepairPlan records`** — wire-stable record shapes for classification and plan artifacts from #1829.
3. **`schema(compute): wire-stable PlacementDecision and ExecutorAdmissionDecision schemas`** — record shapes for the placement decision contract from #1826.
4. **`spec(contracts): define steward required-action card contract`** — either amend ADR-0027 with an operator-required-action superset, or define a separate `StewardRequiredActionCard` primitive. From #1831 + #1832. (See also: ADR-0027 today only covers member ActionCards; the cockpit's 14 operator scenarios cannot be represented by ADR-0027's closed enums.)
5. **`schema(storage): wire-stable BackupPolicy / ReplicationPolicy / RecoveryPolicy / ArchivePolicy / IntegrityPolicy records`** — from #1823.
6. **`schema(ccl): evaluator execution envelope and adoption proposal lifecycle`** — wire-stable records for the CCL evaluator hook from #1821.
7. **`schema(storage): ArtifactRegistry v0 and ScopedVault wire-stable schemas`** — from #1824.
8. **`spec(compute): reconcile fuel/payment legacy vocabulary on ComputeTask`** — from #1826; addresses the legacy `payment_rate` / `payment_currency` fields preserved without endorsement.

### Fixture / devnet proof (9)

9. **`test(devnet): receipt-index anti-entropy fixture (Slice A)`** — from #1829.
10. **`test(devnet): replica-count RedundancyProof simulation (Slice B)`** — from #1829.
11. **`test(devnet): QuorumSyncCheck federation-fixture rehearsal (Slice C)`** — from #1829.
12. **`test(devnet): member shell read-only Slice A`** — standing + ActionCard + receipt + sync-delayed fixture; from #1830.
13. **`test(devnet): member shell signing flow rehearsal (Slice B)`** — from #1830.
14. **`test(devnet): member shell offline / degraded rehearsal (Slice C)`** — from #1830.
15. **`test(devnet): cockpit divergence-render fixture (Slice A)`** — from #1831.
16. **`test(devnet): cockpit storage / backup / restore-test fixture (Slice B)`** — from #1831.
17. **`test(devnet): cockpit compute placement review-required fixture (Slice C)`** — from #1831.

### UX / platform (4)

18. **`spec(web): pick the member shell v0 platform target (PWA vs native vs hybrid)`** — from #1830.
19. **`spec(web): pick the steward cockpit platform target`** — from #1831.
20. **`spec(web): retire or document the legacy web/dashboard/ directory`** — from #1831; resolve boundary of pre-existing legacy dashboard.
21. **`spec(member-shell): integrate with #1610 glossary + #1740 multilingual access`** — from #1830; translation tagging for closed status-string sets.

### Privacy / storage (4)

22. **`spec(privacy): define private-object digest proof without content disclosure`** — formal contract for divergence class 16 in #1829; from #1829.
23. **`refactor(storage): rename CoopReplicated locality to LocalDomainReplicated`** — code-level migration with serde alias; from #1825.
24. **`spec(privacy): PrivacyClass taxonomy reconciliation`** — ADR-0030 names Public/Encrypted/Sealed; implementation has Public/Member/NeedToKnow; #1792 forward-tracks a richer 7-variant taxonomy; cross-sprint follow-up.
25. **`spec(storage): connect replication repair receipts to StorageSpec / RecoveryPolicy`** — close cross-link between #1823 policy objects and #1829's RepairReceipt / RedundancyProof; from #1829.

### Federation / compute (5)

26. **`spec(federation): define quorum sync window for federation-bound placement`** — cross-link between #1829's `QuorumSyncCheck` / `FederationSyncWindow` and #1826's federation-fail-closed gates; from #1829.
27. **`feat(compute): policy oracle for placement decisions (read-only proof-loop)`** — first implementation slice from #1826.
28. **`feat(compute): dry-run fallback exercise + PlacementFallbackReceipt emission`** — second implementation slice from #1826.
29. **`spec(compute): federation agreement adoption surface`** — from #1826; how a domain adopts a `ComputeAgreement`.
30. **`spec(compute): external custodian policy surface`** — from #1826; how a domain adopts an external bridge/custodian policy.

### Docs / state sync (4)

31. **`docs(state): record architecture-spec sprint completion in STATE.md / PHASE_PROGRESS.md`** — see §"Truth-sync targets" below.
32. **`spec(member-shell): consume Layer 4 ProvenanceQuery when #1438 lands`** — from #1830.
33. **`docs(guides): audit cooperative-specific examples vs generic domain examples`** — from #1825; post-#1825 doctrine application across guides corpus.
34. **`refactor(rpc): assess coop-scoped comments in icn-rpc against #1825 vocabulary doctrine`** — preserved-as-code per #1825; question is whether the comments should be renamed in a future pass.

**Total: 34 deduplicated drafts.** None filed.

---

## Truth-Sync Targets

Per-target recommendation. Defer unless the wrap-up review clearly says do-now.

| Target | Status | Recommendation | Rationale |
|---|---|---|---|
| `docs/STATE.md` | 998 lines | **Defer** | A real STATE update after the closure batch (and after the user decides which follow-ups to file) is the right shape. Avoid drift between closures-pending and STATE-says-closed. |
| `docs/PHASE_PROGRESS.md` | 418 lines | **Defer** | Same reason. The sprint is meaningful for phase progress; record once after closures land. |
| `docs/INDEX.md` | per-spec entries already landed | **Do-now: no change required** | Each merged sprint PR landed its own INDEX entry; the architecture spec ladder is already visible. No additional INDEX work needed for this wrap-up. |
| `docs/DOCUMENT_REGISTRY.md` | regenerated 817 → 818 | **Do-now: this PR** | Standard regen for any new doc. |
| `docs/reference/project-index/source-of-truth-map.md` | exists | **Defer** | Should reflect the spec ladder; pair with the closure batch update. |
| `docs/registry.toml` | per-spec entries already landed | **Do-now: no change required** | Handoffs aren't registered (precedent across the sprint). |
| Public website surfaces (`web/`, root site) | unchanged this sprint | **Defer (out of scope here)** | Public copy should not be updated until product framing for the spec ladder is reviewed separately. |
| Issue labels / milestones | unchanged | **Defer** | Tie label updates to the closure batch human decision. |
| AGENTS.md | reconciled in #1827 | **Do nothing** | The handoff-path drift is resolved. Do not re-litigate. |

**Net for this PR:** only `docs/DOCUMENT_REGISTRY.md` regen happens; everything else is recorded as a deferred target for human decision after the closure batch.

---

## Non-Claims Preserved (Cross-Sprint)

This wrap-up reaffirms the non-claims that every sprint PR carried:

- No production readiness for any ICN-native surface.
- No live federation operating under any of the merged specs today.
- No formal NYCN pilot operating under any of the merged specs today.
- No runtime implementation from the merged docs-only specs. Code-level work continues under named follow-ups.
- No private data movement. Body bytes of private vault artifacts never reach any rendering layer.
- No new ADR-0026 receipt classes from any of the sprint specs. Proof / evidence artifacts (PlacementDecision, RepairReceipt, DivergenceEvidence, etc.) travel inside existing Stage 5 `EffectDispatchEvidence` or Layer 2 `ArtifactReceipt` envelopes.
- No ADR-0027 support for steward / operator required-action cards. ADR-0027 covers member ActionCards; steward required-action surfaces are forward-direction and require either an ADR-0027 amendment or a separate `StewardRequiredActionCard` primitive (named follow-up).
- No wallet / payment / currency / balance / token / crypto / blockchain / timebank framing for ICN-native surfaces. All such terms in the sprint docs appear only in explicit negation context (Boundary rules, Non-claims, Vocabulary discipline sections) or as verbatim quotation of existing legacy code identifiers preserved without endorsement (the `bonds:payments` gossip topic; the `payment_rate` / `payment_currency` legacy fields on `ComputeTask`; the `DataLocality::CoopReplicated` kernel enum variant). Each legacy identifier is tracked under a named reconciliation follow-up.
- No K3s, DNS, Forgejo, gateway, storage-backend, identity-bridge, deploy-script, or any deployed-infrastructure changes from the spec ladder.
- No closure of any sibling issue by this wrap-up. Closure is a human decision against the draft comments above.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **All thirteen merged sprint PRs are in `main`.** Verified via `git log --oneline -20` and per-PR `gh pr view`.
- **#1797 is the only sprint-related issue already closed.** Verified via `gh issue view 1797 --json state` returning `CLOSED`. All nine other sibling issues (#1794, #1795, #1798, #1799, #1801, #1815, #1816, #1817, #1818) are OPEN at session start.
- **Each merged spec satisfies its issue's named acceptance criteria as enumerated in the analysis above.** Verified by reading the spec sections cited in the analysis. If a parallel in-flight PR or an amendment changes any cited section before closure happens, the draft closure comment for that issue needs a re-read.
- **The per-PR handoffs accurately enumerated their follow-up drafts.** Verified by reading each handoff. If a handoff was later edited (e.g., #1832 added one follow-up), the latest version is reflected. The deduplicated list above is the union of all per-handoff drafts.
- **#1827 fully reconciled the AGENTS.md handoff-path drift.** Verified via `git log` and the merged content. Future handoffs do not need to re-litigate.
- **#1832 fully reconciled the steward-cockpit drift identified post-#1831-merge.** Verified by inspection of the four rounds of fixes in #1832: round 1 (initial five drifts), round 2 (remaining 8-field sites + ADR-0027 14-field requirement), round 3 (PlacementFallbackReceipt attribution + handoff timing), round 4 (IA-row no longer routes through ADR-0027). All seven Codex/Copilot review threads have replies with fix SHAs.
- **No new ADR has been added during this sprint.** Verified by listing `docs/adr/` for any new file dated 2026-05; only the merged specs landed, not new ADRs. ADR amendments are deferred to the named follow-ups.

---

## Next Move

This is the recommended decision sequence after this wrap-up merges:

1. **Human closure review.** Post the eight draft closure comments above (one per close-now candidate: #1794, #1795, #1798, #1799, #1801, #1815, #1816, #1817, #1818 — note #1797 is already closed). The drafts are paste-ready. Close each issue after posting.
2. **Batch-file the deduplicated follow-up issues** (or a subset). The 34 drafts above are grouped by concern area. The user can decide which subset to file in the first batch; common-sense first batch would be: the four schema follow-ups (1–4), the three first-slice fixture follow-ups (9, 12, 15 — one per merged spec where the cockpit Slice A complements the member-shell Slice A and the anti-entropy Slice A), and the steward required-action card contract (#4 in the list, the one #1832 explicitly named as the post-merge follow-up). Defer the rest until that batch lands.
3. **STATE.md / PHASE_PROGRESS.md update PR.** After the closure batch is posted, write a single update PR that records the sprint completion: the spec ladder, the closed issues, the open follow-up batch, and the recommended next implementation step. This is the right time to update the source-of-truth-map.
4. **Choose the next implementation or fixture PR.** Three credible options:
   - **First implementation slice** — `feat(compute): policy oracle for placement decisions (read-only proof-loop)` (drafted in #1826's handoff; produces working code without crossing live network). Closes the longest spec-vs-implementation gap in the sprint.
   - **First fixture rehearsal** — pick one of the nine Slice A fixtures (anti-entropy / member-shell / cockpit). Exercises the rendering contracts without runtime risk.
   - **Next spec-ladder doc** — `spec(contracts): define steward required-action card contract` (the largest gap surfaced by #1831/#1832). Closes the ADR-0027-vs-operator-scenarios contract gap before any cockpit implementation work tries to fill it.

The cleanest sequence is **1 → 2 → 3 → 4**, doing all three of those in order before committing to a fourth implementation/fixture PR.

---

## Architectural Decisions Recorded in This Sprint
<!-- TRUTH TYPE: Decision truth — choices made and ratified across the ladder -->

The sprint ratified the following cross-spec decisions that future work should preserve:

1. **The architecture-spec ladder is doc-only.** No spec PR in this sprint introduced a new ADR-0026 receipt class, a new endpoint, a new wire format, or a new piece of runtime code. The ladder defines contracts; implementation is the next phase.
2. **The kernel never imports app-side rendering.** Member shell, steward cockpit, and policy oracle outputs are all app-side. The meaning firewall from `docs/architecture/KERNEL_APP_SEPARATION.md` is preserved on every spec.
3. **Generic scope vocabulary is `LocalDomain`, `InstitutionalDomain`, `Domain`, `DomainPolicy`.** Not `Coop` / `Cooperative` as a generic stand-in for the local institutional scope (per #1825 §C3). Existing serialized `Coop`-prefixed identifiers (`DataLocality::CoopReplicated`, `Coop(coop_id)` in ADR-0030, etc.) are preserved with naming notes pending the code-rename follow-up.
4. **Execution budget is the policy-facing term; `fuel_limit` is the runtime field; `capacity` is reserved for executor / node resource availability.** Per #1826 §"Vocabulary boundaries."
5. **Settlement / position / obligation / allocation / receipt / provenance — never payment / wallet / currency / balance / token / crypto / blockchain / timebank** — for ICN-native compute / settlement / federation surfaces. Per `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" and reaffirmed across every sprint PR.
6. **Member shell shows plain participation status; steward cockpit shows technical detail.** The same divergence event surfaces in both, in different vocabularies. The cockpit must always carry a member-impact summary using the merged member-shell vocabulary so the operator sees what members are seeing (#1831 Design principle 9; #1832 reconciliation).
7. **Member shell uses ADR-0027 ActionCard schema; steward cockpit does NOT.** ADR-0027's closed enums cover member participation; operator-required-action cards are forward-direction (per #1832).
8. **Anti-entropy is an institutional evidence loop, not eventual-consistency vibes.** Per #1829 §"Anti-entropy loop model": eight phases producing evidence artifacts that travel inside existing receipt envelopes.
9. **Stewardship, not domination.** Per #1831 Design principle 1: every steward action that mutates institutional state runs through the same mandate / authority / receipt envelope as every member action.
10. **Privacy is posture, not content.** Per #1824 + #1829 + #1830 + #1831: surfaces render that private artifacts exist, that access grants are within policy, that export receipts are landing — without surfacing the contents.

---

## Verification Commands

```bash
cd /home/matt/projects/icn

# Confirm branch state
git checkout docs/architecture-spec-sprint-wrap
git status --short
gh pr view <PR-number> --json mergeStateStatus,state,headRefOid,statusCheckRollup

# Run validation suite
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md
python3 docs/scripts/lint-arch.py docs/dev/handoff-2026-05-15-architecture-spec-sprint-wrap.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Targeted vocabulary check
rg -n "payment|currency|balance|wallet|token|blockchain|crypto|timebank|NYCN|Summit|live federation|production-ready|production readiness" \
   docs/dev/handoff-2026-05-15-architecture-spec-sprint-wrap.md

# Cross-link smoke check
rg -o "docs/[a-zA-Z0-9_/\-]+\.md" docs/dev/handoff-2026-05-15-architecture-spec-sprint-wrap.md | sort -u | while read p; do test -f "$p" && echo "OK $p" || echo "MISSING $p"; done

# Sprint PR roster verification
for n in 1814 1819 1820 1821 1822 1823 1824 1825 1826 1827 1829 1830 1831 1832; do
  gh pr view $n --json state,mergedAt,mergeCommit --jq "\"#$n | \(.state) | \(.mergedAt)\""
done

# Sibling issue state verification (the candidates for closure)
for n in 1794 1795 1798 1799 1801 1815 1816 1817 1818; do
  gh issue view $n --json state --jq "\"#$n | \(.state)\""
done
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/STATE.md` and `docs/PHASE_PROGRESS.md` (998 + 418 lines). This wrap-up does not update them; the recommended sequence in §"Next Move" updates them after the closure batch lands.
- **Implementation truth:** verified against the merged sprint PRs and the cited spec sections. No new code lands; the analysis is doc-vs-acceptance-criteria.
- **Execution truth:** verified branch state, PR roster, issue states via `gh` and `git`.
- **Narrative truth:** loaded from the per-PR handoffs. The deduplicated follow-up list is the union of every handoff's drafts.
- **Known conflicts between layers:** none introduced by this wrap-up. The pre-existing cross-sprint drift items (`DataLocality::CoopReplicated`, `FuelLimit`/`fuel_limit`, `payment_rate`/`payment_currency`, `PrivacyClass` taxonomy) are explicitly enumerated in the follow-up list and the non-claims block; this wrap-up does not address them.

---

## Process Note

The AGENTS.md handoff-path drift was resolved by **PR #1827** (merged 2026-05-15). The active convention is `docs/dev/handoff-YYYY-MM-DD-<topic>.md`, which this handoff uses. **Do not carry the old AGENTS.md drift forward** in future handoffs unless an AI reviewer surfaces it again; respond with a verified rebuttal pointing at #1827.

The post-merge late-review pattern observed on #1831 (Codex landed pre-merge but unaddressed; Copilot landed post-merge; nine valid drift items resolved in #1832 across four rounds) is **not** a new doctrine. The right response to that pattern is the one #1832 already used: a small, surgical, docs-only follow-up PR addressing each valid drift item explicitly. Future merges in narrow review windows should either (a) wait for both AI reviewers before merging, or (b) accept that a small follow-up PR may be required.
