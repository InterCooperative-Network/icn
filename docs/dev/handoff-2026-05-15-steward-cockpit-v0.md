# Session Handoff — 2026-05-15

**Topic:** Steward Cockpit v0 (#1795)
**Branch:** `spec/steward-cockpit-v0`
**Primary issue advanced:** [#1795](https://github.com/InterCooperative-Network/icn/issues/1795) (steward cockpit v0 for node and domain operators)
**Refs (not closed):** `#1795`, `#1830`, `#1613`, `#1012`, `#1792`, `#1610`, `#1740`, `#1767`, `#1799`, `#1801`, `#1798`, `#1816`, `#1815`, `#1817`, `#1794`, `#1797`, `#1793`
**Closes:** none

---

## Session Goal

Define `docs/spec/steward-cockpit-v0.md` as the design-level rendering contract for the ICN steward cockpit at v0: the operator/steward complement of `docs/spec/member-shell-v0.md` (merged #1830). The spec consumes the 9-field cockpit surface from merged #1829, the 14-field operator/steward dashboard from merged #1826, the storage durability policy objects from merged #1823, the artifact / vault posture from merged #1824, the governed-service-binding "Observe" lifecycle state from merged #1822, the CCL policy registry adoption surface from merged #1821, the institutional-domain policy hooks from merged #1820, and the Stage 5 effect-dispatch evidence from merged #1819 — all verbatim, without redefinition.

The spec advances `#1795`'s named acceptance criteria. Closure is left for human review.

---

## Decisive Test

The PR fails if any of these holds:

1. The spec invents new endpoints, new schemas, new receipt classes, new metric names, or new wire formats.
2. The spec specifies a frontend technology (React, Tauri, native, Electron, web framework), a platform choice, or a deployment target for the cockpit UI.
3. The spec describes the cockpit as a generic admin panel, surveillance console, private-data viewer, wallet, fintech dashboard, or account-management product.
4. The spec uses positive ICN-native payment / wallet / balance / currency / token / crypto / blockchain / timebank framing outside explicit negation context or verbatim legacy-identifier quotation.
5. The spec lets the cockpit render bodies of `PrivateEvidence` artifacts or any other private vault content.
6. The spec lets the cockpit show "healthy" while a Boundary rule in a merged sibling spec is failing.
7. The spec lets the cockpit show "healthy" while the member shell is showing degraded (the v0 violation in the failure / safety table).
8. The spec preempts the node operator civic-role surface (`#1613`); that is a separate operator surface with its own contract.
9. The spec touches Rust code, the SDK, the website, the existing `web/dashboard/` legacy directory, deploy scripts, K3s manifests, DNS, Forgejo, or the gateway.
10. The PR commit closes `#1795` instead of using `Refs:`.
11. The spec claims production readiness, live federation, NYCN as a formal pilot, or any partner operating under this contract today.
12. `python3 docs/scripts/doc_control_check.py --strict` introduces a new warning rooted in this PR.
13. `python3 docs/scripts/lint-arch.py docs/spec/steward-cockpit-v0.md --cargo icn/Cargo.toml` returns errors (warnings in explicit negation context are acceptable).

See §"Verification Commands" below for the exact commands.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD before branching

`1a2b4607d` — `docs(spec): define member shell v0 (#1830)`

### Branches

- `spec/steward-cockpit-v0` — historical; merged via PR #1831 at SHA `eab37348402bba16dfaa8651392c0eb33c7c4170`.

### Open PRs

| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| _none open from this session_ — see "Late reviewer feedback" below | — | — | — | — |

### Late reviewer feedback (post-merge)

Codex (12:47:26Z) and Copilot (12:49:41Z) reviews landed **after** PR #1831 was merged at 12:48:54Z. Nine review threads, all valid: (1) anti-entropy cockpit field count is 9, not 8, across spec / registry / INDEX / handoff; (2) ADR-0027 ActionCard schema can't represent operator scenarios — claim must be reframed as forward-direction; (3) `RepairReceipt` is an evidence-artifact identifier per #1829, not a top-level ADR-0026 receipt class; (4) the Warnings / Incidents / Repair surface is listed in the IA but has no per-surface rendering section; (5) this handoff's "Open PRs" table was stale (now corrected). A follow-up PR (`fix(spec): correct steward cockpit review drift`) addresses all five drifts. **The sections of this handoff that pre-date the merge remain historical; they are not updated except where the merge superseded them.**

### Recent merged PRs (including #1831)

`#1814`, `#1819`, `#1820`, `#1821`, `#1822`, `#1823`, `#1824`, `#1825`, `#1826`, `#1827`, `#1829`, `#1830`, `#1831` — `cee87b936515d80326286e9d62bedf3fdcbdb020` (`docs(spec): define steward cockpit v0`).

### Sibling issues verified OPEN at session start

`#1795` (primary; this PR advances), `#1613`, `#1012`, `#1792`, `#1610`, `#1740`.

### Existing surfaces inspected

| Surface | Where | Status |
|---|---|---|
| 9-field anti-entropy cockpit surface | `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface" (merged #1829) | Authoritative; this spec consumes verbatim. |
| 14-field operator/steward placement dashboard | `docs/spec/compute-placement-policy.md` §"Operator / steward dashboard" (merged #1826) | Authoritative; this spec consumes verbatim. |
| Storage durability policy objects | `docs/spec/storage-durability-policies.md` (merged #1823) | Authoritative; this spec surfaces posture. |
| ArtifactRegistry / ScopedVault | `docs/spec/artifact-registry-and-scoped-vault.md` (merged #1824) | Authoritative; this spec surfaces posture, never content. |
| 10-state binding lifecycle (incl. "Observe") | `docs/spec/governed-service-binding.md` (merged #1822) | Authoritative; "Observe" is where cockpit-side degradation surfaces. |
| CCL policy registry / `policy_version_id` | `docs/spec/ccl-policy-registry.md` (merged #1821) | Authoritative; this spec surfaces adopted versions. |
| DomainPolicy hooks | `docs/spec/institutional-domain.md` (merged #1820) | Authoritative; supplies receipt retention, accessibility, language, recovery policy. |
| Stage 5 effect dispatch evidence | `docs/spec/effect-dispatch-contract.md` (merged #1819) | Authoritative; the envelope this spec's cockpit artifacts ride inside. |
| Member shell v0 (closed sync + execution-scope string sets) | `docs/spec/member-shell-v0.md` (merged #1830) | Authoritative; the cockpit's member-impact summary mapping uses these verbatim. |
| `icn-obs` metrics modules | `icn/crates/icn-obs/src/metrics/` (16 files: action_items, agreement, apps, exchange, gateway, governance, ledger, macros, mod, nat, network, resource_enforcer, rpc, service_discovery, storage, trust) | Implemented; cockpit consumes, does not redefine. |
| Governance proof / receipt backend | `icn/crates/icn-governance/src/proof.rs`; `icn/crates/icn-gateway/src/receipt_store.rs` | Implemented. |
| ActionCard schema | `docs/contracts/institution-package/action-card.schema.json`; ADR-0027 | Implemented (14 fields). |
| Legacy `web/dashboard/` | `web/dashboard/{app.js,style.css,index.html,...}` | Pre-existing legacy; this spec is NOT a replacement implementation, only the design contract. |
| ORGANIZER_MEMBER_ACCESSIBILITY_GATE | `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`; ADR-0028 | Authoritative; the 12-category gate applies to cockpit surfaces too. |

### Files Changed

1. `docs/spec/steward-cockpit-v0.md` — new file (~550 lines).
2. `docs/registry.toml` — new entry inserted before `[docs."docs/spec/member-shell-v0.md"]`.
3. `docs/INDEX.md` — Specifications section: one new line after the `member-shell-v0.md` entry.
4. `docs/DOCUMENT_REGISTRY.md` — regenerated. Corpus 815 → 816 markdown files; will reach 817 after this handoff lands.
5. `docs/dev/handoff-2026-05-15-steward-cockpit-v0.md` — this file.

No Rust code touched. No SDK changes. No website changes. No frontend changes. No deploy scripts.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. `docs/spec/steward-cockpit-v0.md` (new doc)

Authority class: `normative`. Length: roughly 550 lines. Sections:

- **Purpose** — ten plain-detail questions the cockpit must answer ("Is this node / domain healthy?" through "What evidence proves each claim?"). Names the consumer relationships with merged #1829 (9 fields), #1826 (14 fields), #1823, #1824, #1822, #1821, #1820, #1819.
- **Scope and non-goals** — twelve non-claim bullets covering frontend technology, endpoint definitions, role redefinition, `#1613` overlap, surveillance / admin / fintech framing, K3s / DNS / Forgejo mutation, NYCN-specific labels, production claims, closure of `#1795`.
- **Boundary lines** — six hard boundaries: vs member shell, vs node operator civic-role surface (`#1613`), vs public website, vs institution-package skin, vs backend / runtime, vs surveillance / admin-control panel.
- **Design principles** — ten v0 principles led by stewardship-not-domination. Others: proof-before-confidence, degraded-is-visible, privacy-posture-not-private-content, receipts-explain-state, required-actions-explicit, authority-basis-visible, scope-visible, member-impact-summary-always-present (the load-bearing honesty rule), no-financial-framing.
- **Cockpit information architecture** — twelve surfaces: Overview/Required Actions, Node Status, Domain Status, Network/Federation, Receipt Store, Storage/Artifacts/ScopedVault, Governance/Process, Compute/Commons, Participation Access, Privacy Posture, Backup/Export/Recovery, Warnings/Incidents/Repair.
- **Required Actions / Steward Action Cards** — fourteen-row table naming operator scenarios with source class, authority pattern, expected outcome. Covers failed receipt write, stale peer, degraded sync, missing replica, backup overdue, restore drill due, compute output awaiting review, accessibility gate failed, missing translation, private overlay missing, overbroad access grant, export receipt awaiting review, key rotation needed, policy conflict / challenge window open.
- **Node Status surface** — eight rendered fields (daemon health, version, uptime, config status, key status, local storage health, current mode, evidence timestamp).
- **Domain Status surface** — eight rendered fields (InstitutionalDomain id, domain policy version, standing read-model health, active proposals/decisions, adopted CCL policy versions, service bindings, action-card generation posture, member-facing readiness summary).
- **Network / Federation surface** — consumes the 9-field cockpit surface from #1829 verbatim plus QuorumSyncCheck / FederationSyncWindow posture.
- **Receipt Store surface** — eight rendered fields including failed-writes, evidence-envelope refs, opaque receipt-store posture.
- **Storage / Artifacts / ScopedVault surface** — eight rendered fields consuming #1823 + #1824, including the hard "no content preview" rule.
- **Governance / Process surface** — seven rendered fields consuming #1819 / #1820 / #1821.
- **Compute / Commons surface** — consumes the 14-field operator/steward dashboard from #1826 verbatim, including the execution-budget vs fuel_limit vs capacity compatibility note.
- **Participation Access surface** — six rendered fields tying back to ADR-0028 / ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md.
- **Privacy Posture surface** — seven rendered fields + the hard "no raw private content" rule.
- **Backup / Export / Recovery surface** — six rendered fields consuming #1823.
- **Status vocabulary** — closed v0 set of twelve operator states + a load-bearing member-impact summary mapping table into the merged member-shell vocabulary from #1829.
- **Failure and safety table** — twenty rows including the load-bearing "Dashboard says healthy while member shell says degraded" v0 violation.
- **First safe proof-loop / dogfood slice** — three fixture-first slices (Slice A read-only receipt-store + anti-entropy degraded/repair, Slice B storage / backup / restore-test, Slice C compute placement review-required). All fixture-only; no real operators; no live network; no real federation.
- **Relationship to sibling work** — comprehensive cross-link table.
- **Non-claims (repeat block for grep clarity)** — fourteen grep-friendly negation lines.

### 2. `docs/registry.toml` (one new entry)

`[docs."docs/spec/steward-cockpit-v0.md"]`:
- `category = "architecture"`
- `status = "draft"`
- `domain_tags = ["steward-cockpit", "operator", "dashboard", "ux", "anti-entropy", "compute", "storage", "governance", "privacy", "accessibility", "kernel-app-boundary", "spec"]`
- `audiences` adds `"operators"` to the standard architects / developers / contributors set.
- `depends_on` enumerates the integrating spine doc, meaning-firewall, boundary doc, eight merged sibling specs, the federation-settlement-finality spec, the accessibility gate, and ten ADRs.

### 3. `docs/INDEX.md` (Specifications section)

One new line after the `member-shell-v0.md` entry, mirroring the recent-specs pattern.

### 4. `docs/DOCUMENT_REGISTRY.md` (regenerated)

Via `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. Corpus 815 → 816 after spec lands; will reach 817 after this handoff lands.

### 5. `docs/dev/handoff-2026-05-15-steward-cockpit-v0.md` (this file)

Filename uses the descriptive topic suffix convention canonicalized by #1827. Structure matches `docs/dev/HANDOFF_TEMPLATE.md`.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work + preserved boundaries -->

### Incomplete work this session deliberately did not start

- [ ] Live cockpit implementation (frontend technology choice + UI build). Tracked as follow-up #1 below.
- [ ] The three fixture-first dogfood slices (Slice A receipt-store + anti-entropy rehearsal, Slice B storage rehearsal, Slice C compute placement rehearsal). Tracked as follow-ups #2 / #3 / #4.
- [ ] Per-surface implementation specs for the twelve information-architecture surfaces. Each can become its own follow-up if implementation parallelizes.
- [ ] Integration with the legacy `web/dashboard/` directory; either replacement or boundary documentation. Tracked as follow-up #5.
- [ ] The cross-link audit against future `icn-obs` metric module renames or additions.
- [ ] Per the established session pattern: **"Apply valid feedback only. Stop and summarize unless explicitly told to merge."** Wait for explicit merge instruction.
- [ ] Decision on filing the follow-up issue drafts in §"Follow-up Issue Drafts" (deferred to user).
- [ ] Closure of `#1795`. The PR uses `Refs: #1795`; closure is a user-driven decision against the issue's acceptance criteria.

### Preserved scope boundaries (explicitly NOT changed)

- No Rust code in `icn/crates/` or `icn/apps/`.
- No SDK changes (`sdk/typescript/`, `sdk/react-native/`).
- No website changes.
- No edits to the existing `web/dashboard/` directory.
- No new endpoints. `icn-obs` metrics, governance proof / receipt surfaces, `/me/standing`, `/me/action-cards`, and the merged sibling specs' surfaces are consumed verbatim.
- No new receipt classes. The cockpit surfaces existing classes (per ADR-0026 / ADR-0025 / merged sibling specs) in operator-facing detail.
- No new ADRs. The spec cross-links ADR-0014, -0019, -0020, -0025, -0026, -0027, -0028, -0030, -0031 without amending them.
- No edits to merged sibling specs.
- No edits to `#1795`'s GitHub issue body.
- No preemption of `#1613` (node operator civic-role surface). Boundary documented.
- No platform / frontend technology decision. Forward-direction.
- No K3s, DNS, Forgejo, gateway, storage-backend, identity-bridge, or deploy-script changes.
- No `DataLocality::CoopReplicated` migration. No `FuelLimit` / `fuel_limit` rename. No `payment_rate` / `payment_currency` reconciliation. No `PrivacyClass` taxonomy migration. All carried forward as separate follow-ups in prior handoffs.
- No `signal_rule` / `obligation_lifecycle` ActionCard source path enablement.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **The 9-field cockpit surface in merged #1829 is canonical and complete.** Verified by reading `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface" on the merged copy in `main`.
- **The 14-field operator/steward dashboard in merged #1826 is canonical and complete.** Verified by reading `docs/spec/compute-placement-policy.md` §"Operator / steward dashboard" on the merged copy.
- **The 12-category accessibility gate applies to operators too.** ADR-0028 names the gate as foundational for member-facing surfaces; this spec extends the gate's applicability to operator-facing surfaces. If a future ADR creates an operator-specific accessibility baseline, this section needs a re-read.
- **The legacy `web/dashboard/` directory is not the cockpit.** Verified by `ls web/dashboard/` — contents are `app.js`, `style.css`, `index.html`, `deploy.sh`, `package.json`, `README.md`. The spec treats this directory as legacy that the cockpit spec is not retrofitting; follow-up #5 names the explicit-boundary work.
- **`icn-obs` metrics modules are the canonical telemetry surface.** Verified by `ls icn/crates/icn-obs/src/metrics/`. Sixteen files: action_items, agreement, apps, exchange, gateway, governance, ledger, macros, mod, nat, network, resource_enforcer, rpc, service_discovery, storage, trust. The cockpit will consume; if a metric module is renamed or removed, the spec's anchoring needs re-checking.
- **`#1795` acceptance criteria.** The issue body's seven acceptance criteria are: (1) Dashboard v0 spec or design doc merged; (2) Spec defines steward/operator Action Cards; (3) Spec separates member shell concerns from steward/operator concerns; (4) Spec includes storage, receipt, network, compute, accessibility, translation, privacy, and backup/export posture; (5) Spec removes stale fintech/timebank/credit dashboard vocabulary for ICN-native surfaces; (6) Spec identifies which fields can be powered by existing endpoints vs future runtime work; (7) Follow-up implementation issues opened only after the spec is accepted. The spec satisfies (1)–(6) on its face; (7) is honored — drafts in handoff, none filed. **Closure of `#1795` is left to the user.**
- **The eighteen-class divergence taxonomy from #1829 is closed and stable.** This spec surfaces it on the Network / Federation surface; if a follow-up amends the taxonomy, the surface's row vocabulary needs an update.
- **`web/pilot-ui/src/` does not exist at session start.** Verified by `ls`. If a parallel branch creates this directory before merge, follow-up #5 may need expansion.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Run validation suite (commands below).
2. Commit (single commit; `docs(spec): define steward cockpit v0`).
3. Push branch.
4. Open PR with `Refs: #1795` and the cross-refs listed at top.
5. Watch initial CI; address any failure rooted in this PR.
6. Watch AI reviewer outputs (Copilot, Codex). Apply only valid feedback. Reply with commit SHA and exact fix.
7. **Stop and summarize after applying valid feedback.** Do not merge unless explicitly told.
8. After merge, the natural next architecture-spec PR is one of: **`#1613`** (node operator civic-role surface — the other operator-facing boundary this spec names), **`#1767`** (encrypted private-overlay storage — completes the privacy stack the cockpit's Privacy Posture surface consumes), or **`#1792`** (private data disclosure boundary — informs the PrivacyClass taxonomy reconciliation forward work).

---

## Architectural Decisions
<!-- TRUTH TYPE: Decision truth — choices made and why -->

### 1. The cockpit is the operator/steward complement of the member shell.

Per `docs/spec/member-shell-v0.md` §"Boundary lines" → "Member shell vs steward cockpit," the member shell shows plain participation status and the cockpit shows technical detail. This spec is the opposite-side boundary doc. The same divergence event surfaces in both, in different vocabularies; the cockpit always carries a member-impact summary using the merged member-shell vocabulary so the operator sees what the member is seeing simultaneously.

### 2. Member-impact summary is a load-bearing honesty rule.

Design principle 9 ("Member-impact summary is always present") plus the failure-table row "Dashboard says healthy while member shell says degraded" make this concrete: if the cockpit shows healthy on a state class that's degraded in the member shell, that's a v0 violation. The cockpit cannot silently disagree with the member shell about what state the institution is in. This prevents a class of operator-side lying that would otherwise be tempting under load.

### 3. The 9-field anti-entropy cockpit surface and the 14-field placement dashboard are consumed verbatim.

This spec deliberately re-states the field sets from merged #1829 and #1826 rather than inventing new ones. The rendering contract is "render what the merged spec already names"; the cockpit's job is to surface those fields with appropriate technical detail plus member-impact summaries. Consuming verbatim prevents drift between the merged spec and the cockpit rendering.

### 4. No frontend technology decision.

The spec is platform-agnostic. Whether the cockpit lives in a desktop app, a web SPA, a Tauri/Electron shell, or a native TUI is a separate engineering decision (follow-up #1). The rendering contract holds for any platform that can satisfy the twelve cockpit surfaces and the accessibility gate.

### 5. No new endpoints, no new schemas, no new receipt classes.

The cockpit consumes existing surfaces. `icn-obs` metrics modules supply telemetry. ADR-0026 / ADR-0025 / merged sibling specs supply receipts. ADR-0027 supplies the action-card schema. ADR-0020 supplies standing. The cockpit's added value is the rendering contract — the choice of which fields to show together, in what order, under what authority, with what member-impact summary — not the addition of new data.

### 6. Privacy is posture, not content.

Per `docs/spec/artifact-registry-and-scoped-vault.md`: the cockpit surfaces that private overlays exist, that vaults are loaded, that access grants are within policy, that export receipts are landing. The cockpit does **not** show body bytes of `PrivateEvidence` artifacts. This is enforced both in the rendering contract (Privacy Posture surface) and in the failure / safety table ("ScopedVault content exposed in cockpit" is a v0 violation).

### 7. The PR uses `Refs:`, not `Closes:`.

Per the standing session pattern. The spec satisfies six of `#1795`'s seven acceptance criteria on its face; criterion (7) is honored by drafting follow-ups in the handoff without filing them. Closure is a user-driven decision.

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn

# Branch state
git checkout spec/steward-cockpit-v0
git status --short
gh pr view <PR-number> --json mergeStateStatus,state,headRefOid,statusCheckRollup

# Validation suite
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md
python3 docs/scripts/lint-arch.py docs/spec/steward-cockpit-v0.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Targeted vocabulary check
rg -n "payment|currency|balance|wallet|token|blockchain|crypto|timebank|NYCN|Summit|live federation|production-ready|production readiness" \
   docs/spec/steward-cockpit-v0.md docs/dev/handoff-2026-05-15-steward-cockpit-v0.md docs/INDEX.md docs/registry.toml

# Cross-link smoke check
rg -o "docs/[a-zA-Z0-9_/\-]+\.(md|json)" docs/spec/steward-cockpit-v0.md | sort -u | while read p; do test -f "$p" && echo "OK $p" || echo "MISSING $p"; done

# See the spec contents
sed -n '1,80p' docs/spec/steward-cockpit-v0.md
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/PHASE_HISTORY.md` and `docs/STATE.md`. No conflict; this PR is doc-only design work.
- **Implementation truth:** verified against the merged sibling specs in `main` (#1819, #1820, #1821, #1822, #1823, #1824, #1826, #1829, #1830); the `icn-obs` metrics directory listing; the ADR-0027 schema JSON; the existence of `web/dashboard/` as legacy. The cockpit content is design-level; no implementation truth changes.
- **Execution truth:** verified branch and PR state via `gh`. Initial commit SHA, PR number, head SHA recorded after commit / push.
- **Narrative truth:** loaded from prior session handoffs and the twelve merged architecture-spec PRs.
- **Known conflicts between layers:** the spec's design principle 9 / member-impact summary mapping creates a new honesty rule between the cockpit and the member shell ("Dashboard says healthy while member shell says degraded" is a v0 violation). This is new but consistent with merged #1830 Boundary lines — the boundary is now load-bearing on both sides.

---

## Follow-up Issue Drafts (Not Filed)

Six follow-ups drafted (the original five plus one added after the post-merge review on PR #1831 surfaced the ADR-0027 schema gap). Paste-ready for separate review.

### 0. `spec(contracts): define steward required-action card contract` (added post-merge)

```
## Purpose

The steward cockpit v0 spec at `docs/spec/steward-cockpit-v0.md` names fourteen operator scenarios (failed receipt write, stale peer, degraded sync, missing replica, backup overdue, restore drill due, compute output awaiting review, accessibility gate failed, missing translation, private overlay missing, overbroad access grant, export receipt awaiting review, key rotation needed, policy conflict / challenge window open) that the cockpit's Required Actions surface must render. The existing ADR-0027 `ActionCard` schema cannot represent these scenarios: its closed enums for `source_kind` (`proposal` / `meeting` / `action_item` + two RFC-gated reserved) and `action_kind` (`vote` / `attend` / `complete`) were defined for member participation cards, not operator required actions.

## Scope

- Decide between (a) amending ADR-0027 with an operator-required-action superset, or (b) defining a separate `StewardRequiredActionCard` primitive alongside the member-facing `ActionCard`.
- Specify the closed source-class / action-class enums for the fourteen named operator scenarios.
- Specify the wire-stable record shape.
- Cross-link the cockpit spec's §"Required Actions / Steward Action Cards" so the forward-direction status is reconciled.

## Non-goals

- No cockpit implementation in this issue.
- No new ADR-0026 receipt classes.

## Related

- `docs/spec/steward-cockpit-v0.md` §"Required Actions / Steward Action Cards."
- ADR-0027 (`docs/adr/ADR-0027-action-card-contract.md`).
- `docs/contracts/institution-package/action-card.schema.json`.
- `#1795` parent.
```

### 1. `spec(web): pick the steward cockpit platform target`

```
## Purpose

Decide the implementation platform for the v0 steward cockpit. The spec at `docs/spec/steward-cockpit-v0.md` is deliberately platform-agnostic. This issue is the separate decision-making PR.

## Scope

- Trade-off analysis: desktop app (Tauri / Electron), web SPA, native TUI.
- Constraints: operator-facing but accessible per ADR-0028; consumes `icn-obs` metrics; renders 12 surfaces; must pass the 12-category accessibility gate; not a surveillance console.

## Non-goals

- Not the rendering contract (that's `docs/spec/steward-cockpit-v0.md`).
- Not the legacy `web/dashboard/` retrofit (separate concern).

## Related

- `docs/spec/steward-cockpit-v0.md` §"Boundary lines" and §"Design principles."
- `#1795` parent.
```

### 2. `test(devnet): cockpit divergence-render fixture (Slice A)`

```
## Purpose

Implement Slice A from `docs/spec/steward-cockpit-v0.md` §"First safe proof-loop / dogfood slice": fixture-only read-only cockpit rendering of an open `DivergenceEvidence` of class "missing receipt," a `RepairPlan`, and a `RepairReceipt` across three fixture peers.

## Scope

- Same fixture stack as `docs/spec/network-anti-entropy-proof-loops.md` Slice A.
- All 9 cockpit fields rendered.
- Member-impact summary attached: "Members see: Sync delayed → Receipt available."
- The 12-category accessibility gate passed on the fixture rendering.

## Non-goals

- No live cockpit implementation.
- No real network.
- No real federation.

## Related

- `docs/spec/steward-cockpit-v0.md` §"First safe proof-loop / dogfood slice" Slice A.
- `docs/spec/network-anti-entropy-proof-loops.md` Slice A.
```

### 3. `test(devnet): cockpit storage / backup / restore-test fixture (Slice B)`

```
## Purpose

Implement Slice B: fixture artifacts below ReplicationPolicy.target_replicas; backup-overdue; restore-test receipt fixtures.

## Scope

- Same fixture stack as Slice A.
- ReplicationPolicy.target_replicas = 3 with only two peers holding the artifact.
- DivergenceEvidence class "replica missing"; RepairPlan of action "re-replicate within scope"; RepairReceipt.
- Backup-overdue Required Action card surfaces.
- Restore-test receipt fixture: one successful, one failed.

## Non-goals

- No real backup engine.
- No real restore implementation.

## Related

- `docs/spec/steward-cockpit-v0.md` §"First safe proof-loop / dogfood slice" Slice B.
- `docs/spec/storage-durability-policies.md`.
```

### 4. `test(devnet): cockpit compute placement review-required fixture (Slice C)`

```
## Purpose

Implement Slice C: fixture PlacementDecision with ReviewRequiredActionCard for an advisory workload. All 14 operator/steward dashboard fields rendered.

## Scope

- Same fixture stack.
- Fixture advisory workload generating PlacementDecision + ReviewRequiredActionCard.
- All 14 fields rendered including fallback path if applicable.
- ExecutorAdmissionDecision and optional PlacementFallbackReceipt exercised.
- Member-impact summary: "Members see: Review required."

## Non-goals

- No real compute admission engine.
- No real ratification flow.

## Related

- `docs/spec/steward-cockpit-v0.md` §"First safe proof-loop / dogfood slice" Slice C.
- `docs/spec/compute-placement-policy.md` §"Operator / steward dashboard."
```

### 5. `spec(web): retire or document the legacy web/dashboard/ directory`

```
## Purpose

The legacy `web/dashboard/` directory pre-dates the cockpit spec. This issue resolves the boundary: either retire the legacy directory, retrofit it against the cockpit spec, or document it as a separate historical artifact.

## Scope

- Inventory of `web/dashboard/` contents.
- Decision: retire / retrofit / preserve-as-historical.
- Cross-link to `docs/spec/steward-cockpit-v0.md` §"Boundary lines" if preserved.

## Non-goals

- No new cockpit implementation in this issue.

## Related

- `docs/spec/steward-cockpit-v0.md` §"Boundary lines."
- `#1795` parent.
```

---

## Process Note

The AGENTS.md handoff-path drift was resolved by **PR #1827** (merged 2026-05-15). The active convention is `docs/dev/handoff-YYYY-MM-DD-<topic>.md`, which is what this handoff uses. **Do not carry the old AGENTS.md drift forward** in future handoffs unless an AI reviewer surfaces it again, in which case respond with a verified rebuttal that points at #1827.
