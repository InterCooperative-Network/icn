# Session Handoff — 2026-05-14

**Trigger:** Architecture control / truth-sync pass. Recent product discussion expanded ICN's framing from "governance substrate" toward "cooperative commons cloud / governed institutional infrastructure substrate," but no single integrating document tied the existing pieces into one civic loop. Issue #1793 named that gap explicitly and proposed `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` as the candidate output.

**Session goal:** Close the coherence gap by producing the integrating spine doc, register it, draft future-work issues, and prepare the next safe PR ladder. No runtime implementation, no schema changes, no NYCN-specific nouns in ICN core.

## Final state (verified)

- **Repo HEAD before session:** `f3a221088` (web: add scoped website format check (#1812)).
- **Branch:** `docs/commons-cloud-governance-spine`.
- **Open PRs at session start:** #1790, #1791 (dependabot only — untouched).
- **Working tree before session:** clean.
- **Open architecture-spec backlog reviewed:** #1010, #1012, #1623, #1608, #1646, #1689, #1692, #1709, #1710, #1711, #1712, #1748, #1767, #1792, #1793, #1794, #1795, #1797, #1798, #1799, #1801. All exist, all open, all titles confirmed via `gh issue view`.

## What was reviewed

Documents read in full:

- `docs/STATE.md` — Phase 2 still ⏳, May 2026 work captured.
- `docs/PHASE_PROGRESS.md` — `idea-0019` and `idea-0020` partially landed; acceptance gates unmet.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — meaning firewall, policy-oracle pattern, five invariants.
- `docs/architecture/SERVICE_HOSTING_MODEL.md` — three service stages, custody, exit policy.
- `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` — forward direction: domains, sessions, devices, services, vaults, agreements.
- `docs/architecture/COOPERATIVE_TOOL_COMMONS.md` — installable/forkable tools, manifests, capability grants.
- `docs/architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md` — protocol doctrine, receipts as truth.
- `docs/state/storage-governance-spec.md` — storage class + locality as governance constraints.
- `docs/design/deterministic-core.md`, `docs/design/compute-classes.md`, `docs/design/institutional-structure-spec.md`, `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`.
- `docs/rfcs/RFC-0017-tool-install-infrastructure.md`, `RFC-0016-relationship-surface.md`, `RFC-0001-obligation-allocation-settlement-primitives.md`.
- `docs/registry.toml` (lines 770–960 architecture cluster).
- `docs/dev/handoff-2026-05-07-a.md`, `handoff-2026-05-07.md` (format reference).
- Five most recent merged docs PRs (#1739 ARCHITECTURE_DUE_DILIGENCE, plus four web/visual PRs).
- `docs/scripts/doc_control_check.py`, `docs/scripts/lint-arch.py`, `docs/scripts/freshness-check.py` (interfaces only).

Issues confirmed by `gh issue view`: #1010, #1012, #1608, #1623, #1646, #1689, #1692, #1709, #1710, #1711, #1712, #1748, #1767, #1792, #1793, #1794, #1795, #1797, #1798, #1799, #1801.

ADRs referenced by the spine doc: ADR-0026 (receipt and provenance proof envelope), ADR-0027 (action card contract), ADR-0030 (compute workload manifest and authority boundary), ADR-0031 (commons compute admission and settlement policy) — all already canon, no edits.

## Files changed

| Path | Change | Lines |
|---|---|---|
| `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` | new | ~340 |
| `docs/registry.toml` | new entry inserted after `ARCHITECTURE_DUE_DILIGENCE.md` block | +17 |
| `docs/dev/handoff-2026-05-14-commons-cloud-governance-spine.md` | new (this file) | ~280 |

No other files were modified. No Rust workspace changes. No SDK changes. No website changes. No CI changes.

## Issue hygiene findings

The current architecture-spec backlog is well-sequenced. No stark duplicates were found. Specifically:

- **#1792 → #1767 → #1798** form a coherent privacy/storage chain (boundary → encrypted overlay → artifact registry). Each is distinct. Keep all three.
- **#1793 → #1794 / #1797 / #1798 / #1799 / #1801 / #1795 / #1792 / #1767** is a deliberate parent/child hierarchy. #1793 is the integrator; the others are narrower specs that should reference the integrator doc once merged. No closures recommended.
- **ActionCard chain (#1608 / #1646 / #1711 / #1712 / #1742)** — intentionally distinct (core endpoint, standing-derived layer, source paths gated on future work, compatibility review gate). Keep all five.
- **#1709 / #1710** — RFC-0017 resolution and Forgejo/auth-bridge MVP gate. Distinct, both still open.
- **#1748** — process substrate. Sits between `idea-0019` work and #1797. Keep.

**Flagged for human decision (out of this PR's scope):**

- **#1012** — "Wave 6 Legibility Dashboards: UX Spec for Constraint Visibility." The user has previously flagged it as potentially carrying stale `credit/balance/currency` framing inherited from earlier waves. I did not modify the issue body in this PR. Recommendation: read the issue body in a separate triage pass; if the framing has drifted from current safe vocabulary, file an amendment PR scoped to the issue body only.
- **Does the spine doc satisfy all 8 acceptance criteria of #1793?** The doc covers: integrated model (✓), substrate/app/package/teaching separation (✓), civic loop with each subsystem mapped (✓), regulatory-safe vocabulary discipline (✓), Cooperative OS marked as packaging direction (✓), cross-links to siblings (✓), no overclaims (✓). Outstanding question: criterion calling for cross-links **from** #1689 — those edits live on the parent issue side, not in this doc. The PR `Refs: #1793` rather than `Closes #1793` to leave that judgment with the user.

**No issue closures recommended in this PR.**

## Future-work issue drafts (not filed)

The following four future-work surfaces were verified absent from the open issue backlog. Drafts below are paste-ready. **Filing is deferred** to a separate operator turn so titles, labels, and acceptance criteria can be reviewed first.

### Draft 1 — `spec(runtime): define governed service binding, workload manifest, and runtime provider model`

**Type:** `type:spec` (no epic label proposed; see triage note).

**Body:**

```
## Purpose

Define the integrating envelope for hosted services, installable tools, compute jobs, contract execution, and future container or microVM workloads. This unblocks the hosted→governed→native progression named in SERVICE_HOSTING_MODEL.md and the placement work tracked under #1801.

## Scope

- GovernedServiceBinding — the institutional record that binds a service or workload to a domain. Holds authority context, custody class, runtime class, dispatch policy, capability scope, backup policy, and exit policy.
- WorkloadManifest — the declared shape of what a workload is supposed to do: inputs, outputs, runtime class, custody class, expected receipts, side-effect bounds.
- RuntimeProvider — the substrate-side counterpart that knows how to run a manifest under the binding's constraints.
- Lifecycle: declare → authorize → allocate → run → observe → upgrade / suspend / remove → export.
- Relationship to ToolBinding (RFC-0017), hosted services (SERVICE_HOSTING_MODEL.md), compute jobs, contract execution, and future model/agent workloads.
- Receipt policy per lifecycle transition.

## Runtime classes to cover

- Deterministic legitimacy compute (CCL evaluation, authoritative transitions).
- Utility computation (plugins, transforms; evidence not authority).
- Container workloads.
- Stronger-isolation (microVM-class) workloads.
- Hardware-accelerated workloads.
- Local device / client workloads.
- External bridge workloads (only by explicit policy).

## Boundary rules

- Utility services produce evidence; legitimacy compute records authoritative transitions.
- A workload without a binding is not allowed to run.
- A binding without a custody class and exit policy is incomplete.

## Non-goals

- No runtime implementation in this issue.
- No schema or wire format definition.
- No production rollout.
- No service-specific bindings; institution packages define their own.

## Related

- #1793 — integrated system model (parent).
- #1797 — accepted-proposal effect dispatch.
- #1801 — compute placement policy.
- #1798 — ArtifactRegistry and ScopedVault boundary.
- ADR-0030 — compute workload manifest and authority boundary.
- docs/architecture/SERVICE_HOSTING_MODEL.md.
- docs/architecture/COOPERATIVE_TOOL_COMMONS.md.
- docs/rfcs/RFC-0017-tool-install-infrastructure.md.
```

### Draft 2 — `spec(storage): define backup, replication, recovery, and archive policies`

**Type:** `type:spec` (no epic label proposed).

**Body:**

```
## Purpose

Define the policy objects that bind storage classes (canonical, service state, artifact, volume, scoped vault, secret, cache) to durability, recoverability, and accountability commitments. This is the prerequisite for every legitimate exit-on-conflict story and for restore-test receipts.

## Scope

- StorageSpec — what storage is bound to a workload: class, locality, capacity.
- BackupPolicy — frequency, target, integrity, retention.
- ReplicationPolicy — replicas, placement, anti-entropy expectations.
- RecoveryPolicy — restore objectives, drill cadence, authority required to restore.
- ArchivePolicy — long-term retention, immutability requirements.
- IntegrityPolicy — verification cadence, integrity-receipt class.
- Restore-test receipts — drills produce real evidence, not decoration.
- Locality and privacy constraints — backups inherit the source's locality and disclosure rules.
- Backup/export/restore authority — restoring authoritative state is a governed event.

## Doctrine to encode

- Redundancy keeps the service alive.
- Backups keep the institution recoverable.
- Archives keep the institution accountable.
- Disaster recovery proves the promises are not decorative.

## Boundary

- A storage binding without a custody class and policy set is unsafe; the system should refuse it.
- Backup operations and restore drills produce receipts.
- A backup that crosses a locality boundary violates the same policy the source enforces.

## Non-goals

- No storage backend implementation.
- No vendor-specific integrations.
- No replication algorithm choice.
- No SLA commitments.

## Related

- #1793 — integrated system model (parent).
- #1798 — ArtifactRegistry and ScopedVault boundary (artifact-storage half).
- #1767 — encrypted distributed private-overlay storage.
- #1792 — private data disclosure boundary.
- #1799 — network anti-entropy proof loops (shares verification surface).
- docs/state/storage-governance-spec.md.
```

### Draft 3 — `spec(ccl): define policy registry, versioning, and governance-effect hook contract`

**Type:** `type:spec` (no epic label proposed).

**Body:**

```
## Purpose

CCL is the executable institutional rule layer inside governance. To make it usable across domains, we need an explicit policy registry, a versioning convention, and a contract for how a governance decision selects a CCL evaluator and consumes its output as one or more bounded effects.

## Scope

- CCL policy registry — how a policy document is registered, addressed, and discovered.
- Versioning — every policy has authorship, adoption history, an amendment path, and a stable version identifier.
- Adoption — how a domain (#1794) binds a CCL policy version to its DomainPolicy.
- Proposal kind → evaluator selection — how the runtime picks the right CCL evaluator for a given proposal kind.
- Evaluator output → effect plan — how a CCL evaluator produces structured effect plans the runtime can dispatch (#1797).
- Review and audit surfaces — every CCL document is reviewable; the registry surfaces drafts, adopted versions, and amendment history.

## Rules to encode

- CCL makes governance executable; it does not make code sovereign.
- An unadopted CCL document is inert; only adopted versions are bound to authority.
- CCL produces effect plans, not unilateral mutations.
- Models or agents may draft CCL text; governance adopts it.
- No institution-specific nouns leak into ICN core CCL grammar.

## Non-goals

- No CCL language grammar changes in this issue.
- No runtime implementation.
- No model-tooling integration.
- No specific adoption workflow for a partner institution.

## Related

- #1793 — integrated system model (parent).
- #1794 — InstitutionalDomain and DomainPolicy primitive.
- #1797 — accepted-proposal effect dispatch.
- #1748 — institutional process substrate.
- docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md (§5, §6).
```

### Draft 4 — `ux(mobile): define member shell v0 as primary participation surface`

**Type:** `type:spec` (no epic label proposed).

**Body:**

```
## Purpose

Define the member-facing participation surface as a UX specification at version zero. The member shell is the primary place participation happens; it is distinct from the steward cockpit (#1795) and from operator civic-role surfaces (#1613). It must work mobile-first, offline-tolerant, and accessibility-first.

## Scope

- Mobile-first layout. Large tap targets. Plain language.
- Action cards derived from the member's standing.
- Standing surface — who I am, what my standing is, what is open to me.
- Receipts — every visible state change is explained by a receipt.
- Offline and low-bandwidth mode. Local cache treated as derived state, never as source of truth.
- Safe signing confirmations before any authorized action.
- Accessibility — screen-reader compatibility, keyboard accessibility, color-independent affordances, captions where audio is present, low cognitive load. Bound to docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md.
- Multilingual / inclusive-access path — interoperate with #1740.

## Rules to encode

- No financial-product framing. The forbidden vocabulary list in docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md §11 applies in full.
- The shell does not present participation as account management.
- A member who cannot see what was decided, what their standing is, and what action is open to them has not been served.

## Non-goals

- No native-app implementation.
- No platform-specific choices (iOS / Android / PWA) in this issue.
- No partner-institution skinning.
- No backend endpoint definitions beyond cross-link to existing /me/standing and /me/action-cards work.

## Related

- #1793 — integrated system model (parent).
- #1795 — steward cockpit (operator counterpart).
- #1613 — node operator civic-role surface in Commons Shell.
- #1608 — /me/action-cards endpoint.
- #1646 — action-cards on top of /me/standing.
- #1740 — multilingual / inclusive-access path.
- docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md.
```

### Skipped: consolidated threat-model umbrella issue

The user's outline proposed a fifth issue consolidating runtime/service/storage threat models. Verification showed the threat surface is already split across #1010 (substrate adversarial harness), #1767 (encrypted private overlay), #1792 (disclosure boundary), #1799 (network anti-entropy), and #1801 (compute placement). Filing a separate umbrella issue would duplicate scope without adding leverage. The spine doc cross-links the five existing issues instead (§10). If a future review prefers a single umbrella, it can be filed at that point.

## Remaining risks and known gaps

The spine doc does **not** specify:

- The exact decision → effect dispatch message shape (#1797's job).
- The receipt class taxonomy / registry. Multiple receipt classes are referenced by name (`restore-test receipt`, `integrity receipt`, `disclosure receipt`) but no enumeration is fixed. This belongs to the effect-dispatch contract work.
- Hosted → Governed → Native staging acceptance gates. The doc names what each stage requires but does not specify the signoff process.
- The CCL evaluator interface (Draft 3's job).
- The backup policy data model (Draft 2's job).
- The member shell UX flows (Draft 4's job).

Each of these is named as future work and either has an existing tracking issue or one of the four drafts above.

## Validation run

Commands executed:

```bash
cd /home/matt/projects/icn

# Doc control plane
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict

# Architecture lint (forbidden-vocab check)
python3 docs/scripts/lint-arch.py docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md --cargo icn/Cargo.toml

# Cross-link existence
rg -o "docs/[a-zA-Z0-9_/\\-]+\\.md" docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md | sort -u | while read p; do test -f "$p" && echo "OK $p" || echo "MISSING $p"; done

# Manual grep for forbidden vocab
rg -n "payment|wallet|balance|currency|\\bMana\\b|CoVM|timebank" docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md
```

Results: see [Validation results] section below (filled in after running).

## Recommended next PR

**#1797 — accepted-proposal effect dispatch contract specification.**

This is the highest-leverage next step because:

- It unblocks Draft 3 (CCL policy registry) and Draft 1 (GovernedServiceBinding).
- It is already filed and labeled.
- Its surface is well-scoped (decision → mandate → effect dispatch → receipt).
- It does not require any of the four drafts to land first.

After #1797 lands, the natural ladder is: #1794 (InstitutionalDomain) → Draft 3 (CCL registry) → Draft 1 (GovernedServiceBinding) → #1798 + Draft 2 (artifact registry + backup policy).

## Non-claims preserved

This handoff and the PR it documents:

- Do not claim Phase 2 complete.
- Do not claim NYCN is a formal pilot. NYCN appears only as a source-of-truth boundary in the repository-boundary table.
- Do not claim production readiness.
- Do not claim live federation.
- Do not claim implemented service hosting.
- Do not introduce NYCN/Summit/private partner data in public ICN docs.
- Do not mutate K3s, DNS, identity-bridge, or any deployed infrastructure.
- Do not use payment, wallet, balance, currency, token, or blockchain as ICN-native framing.
- Do not introduce schema or contract changes.
- Do not by themselves close #1793; the PR uses `Refs:` not `Closes:`.

## Validation results

- **`doc_control_check.py --repo . --registry docs/registry.toml --strict`** → exit 0. `OK: doc control check passed (796 docs markdown files under docs/); 53 enforcement warnings`. The 53 warnings are all **pre-existing and unrelated** to this PR — they are `[yaml-mismatch]` entries on `docs/ai/*`, `docs/design/*`, `docs/manual/*` and `[explicit-registry]` warnings on `docs/plans/*` files dating to early 2026. None mention the new doc. Recording honestly, not broadening this PR to fix them.

- **`lint-arch.py docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md --cargo icn/Cargo.toml`** → exit 0. `Total: 0 errors, 1 warnings`. The one warning is `Unknown crate: 'icn-learn'` on line 49, which is non-fatal and matches the same warning that the existing `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` carries — `icn-learn` is the teaching-repo name, not a Cargo crate.

  *(Note for the record: an earlier draft of §11 enumerated the hard-forbidden terms by name in a discipline reminder; `lint-arch.py` correctly rejected that with three errors because hard-forbidden terms are never allowed in any context. The section was reshaped to capture the discipline through explicit anti-claim sentences instead, which is the convention used by the other architecture docs in this repository.)*

- **`python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .`** → exit 1. *(Corrected from an earlier incorrect record in this handoff. The script does not accept `--registry`; the original line captured a piped `grep`'s exit code rather than the script's, which is why the impossible "exit 0" was recorded. Codex caught this during PR #1814 review.)* The non-zero exit reflects **pre-existing staleness** in `docs/ARCHITECTURE.md` sections (06-identity-cryptography, 10-state-ledger, 11-contract-execution, 12-governance, 14-federation), not anything introduced by this PR's new file. The corresponding GitHub Actions "Check Documentation Freshness" job in this PR's CI reports SUCCESS, indicating the workflow treats staleness as informational. The pre-existing staleness is recorded here honestly; this PR does not broaden scope to address it.

- **`compliance_linter.py`** → exit 0. `Scanned 107 files. ✅ No compliance violations detected!`.

- **Cross-link existence sweep** — pre-write check across 18 referenced documents (KERNEL_APP_SEPARATION, INSTITUTION_PACKAGE_BOUNDARY, COOPERATIVE_DOMAIN_INFRASTRUCTURE, SERVICE_HOSTING_MODEL, COOPERATIVE_TOOL_COMMONS, DOMAIN_ROUTING_AND_DNS_BINDINGS, MODEL_WORKLOADS_AND_DELIBERATION, storage-governance-spec, deterministic-core, compute-classes, RFC-0017, THE_COMMONS, PROTOCOL_SELECTION_FOR_MEMBER_SERVICES, institutional-structure-spec, ORGANIZER_MEMBER_ACCESSIBILITY_GATE, RFC-0016, RFC-0001, ARCHITECTURE) — 18/18 present.

- **Forbidden-vocabulary grep on the new doc** — all hits are in explicit-negation context:
  - Line 327: `- ICN is not a payment system.`
  - Line 328: `- ICN is not a wallet or balance dashboard.`
  - Line 329: `- ICN does not issue currency.`
  - Line 330: `- ICN is not a blockchain. ...`
  - Line 436 (non-claims block): `It does not use payment, wallet, balance, currency, token, or blockchain as ICN-native framing; those words appear only inside the discipline list and the explicit anti-claim sentences in §11.`

  No positive ICN-native usage of any forbidden term.

- **Cargo / SDK / website / CI** — not touched by this PR. No validation needed.
