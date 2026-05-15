# Session Handoff — 2026-05-15

**Topic:** Member Shell v0 (#1818)
**Branch:** `spec/member-shell-v0`
**Primary issue advanced:** [#1818](https://github.com/InterCooperative-Network/icn/issues/1818) (member shell v0 as primary participation surface)
**Refs (not closed):** `#1818`, `#1795`, `#1613`, `#1608`, `#1646`, `#1740`, `#1610`, `#1742`, `#1631`, `#1634`, `#1767`, `#1792`, `#1799`, `#1801`, `#1798`, `#1816`, `#1815`, `#1817`, `#1794`, `#1797`, `#1793`
**Closes:** none

---

## Session Goal

Define `docs/spec/member-shell-v0.md` as the design-level rendering contract for the ICN member shell at v0: mobile-first, offline-tolerant, accessibility-first, plain-language-first. The spec consumes ADR-0020 `/me/standing`, ADR-0027 ActionCard schema, ADR-0026 receipt envelope, ADR-0028 accessibility baseline, the closed seven-string sync vocabulary from merged `#1829` (network-anti-entropy-proof-loops.md), and the closed seven-string placement vocabulary from merged `#1826` (compute-placement-policy.md). It does not redefine any of those contracts. It does not specify a native app, a PWA, an iOS/Android decision, a new endpoint, a partner skin, or any runtime change.

The spec advances `#1818`'s purpose and scope; closure is left for human review against the issue's acceptance criteria.

---

## Decisive Test

The PR fails if any of these holds:

1. The spec invents new ActionCard schema fields, new receipt classes, or new endpoint surfaces.
2. The spec uses positive ICN-native payment / wallet / balance / currency / token / crypto / blockchain framing outside explicit negation context.
3. The spec picks a native-app / iOS / Android / PWA / web technology, or otherwise locks the implementation surface to a platform choice.
4. The spec invents member-facing status strings outside the closed sets already merged in `#1826` and `#1829` plus the small action-lifecycle and privacy / disclosure additions named in §"Member-facing status vocabulary."
5. The spec describes the member shell as account management, a wallet, a fintech product, a trading interface, an admin panel, or the steward cockpit.
6. The spec treats the local cache as the source of truth for any read or signed write path.
7. The spec lets degraded sync render as healthy on any member-facing surface.
8. The spec enables the `signal_rule` or `obligation_lifecycle` ActionCard source paths in the generic v0 shell (those are RFC-gated against `#1631` and `#1634` and must remain inert at the rendering layer until those land).
9. The spec edits the `#1818` GitHub issue body.
10. The spec touches Rust code, the SDK, the website, deploy scripts, K3s manifests, DNS, Forgejo, or the gateway.
11. The PR commit closes `#1818` instead of using `Refs:`.
12. The spec claims production readiness, live federation, NYCN as a formal pilot, or any partner operating under this contract today.
13. `python3 docs/scripts/doc_control_check.py --strict` introduces a new warning rooted in this PR.
14. `python3 docs/scripts/lint-arch.py docs/spec/member-shell-v0.md --cargo icn/Cargo.toml` returns errors (warnings in explicit negation context are acceptable).

See §"Verification Commands" below for the exact commands.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD before branching

`0b3009478` — `docs(spec): define network anti-entropy proof loops (#1829)`

### Branches

- `spec/member-shell-v0` — head SHA at first commit recorded after `git commit`. Branch head moves with each push to address review feedback.

### Open PRs

| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| (to be opened) | `spec/member-shell-v0` | pending PR | pending initial CI | None |

### Recent merged PRs

`#1814`, `#1819`, `#1820`, `#1821`, `#1822`, `#1823`, `#1824`, `#1825`, `#1826`, `#1827`, `#1829`.

### Sibling issues verified OPEN at session start

`#1818` (primary; this PR advances), `#1795`, `#1613`, `#1608`, `#1646`, `#1740`, `#1610`, `#1742`, `#1711`, `#1712`. The `signal_rule` (`#1631`) and `obligation_lifecycle` (`#1634`) source-path issues are referenced but the issue numbers are forward-direction per ADR-0027's `x-icn-emitted-source-kinds` reserved-set; their state was not queried this session.

### Existing surfaces inspected

| Surface | Where | Status |
|---|---|---|
| `/me/standing` runtime contract | ADR-0020 (`docs/adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md`); `icn/apps/governance/src/http/handlers.rs` | Implemented (per ADR-0020 implementation_status). |
| `/me/action-cards` endpoint | ADR-0027 + `#1608` + `#1646`; `icn/apps/governance/src/http/handlers.rs::get_my_action_cards` | Implemented as a vertical slice. |
| ActionCard schema (14 fields) | `docs/contracts/institution-package/action-card.schema.json` (x-icn-status: rfc); ADR-0027 | Implemented at the schema level; rendering rules are this spec's job. |
| ActionCard source paths | ADR-0027 closed taxonomy | `proposal/vote`, `action_item/complete`, `meeting/attend` implemented end-to-end with receipts. `signal_rule` reserved against `#1631`; `obligation_lifecycle` reserved against `#1634`. |
| Receipt classes | ADR-0026 (envelope), ADR-0025 (EffectRecord), `icn/crates/icn-governance/src/proof.rs` | `GovernanceDecisionReceipt`, `ActionItemCompletionReceipt`, `MeetingAttendanceReceipt` implemented; forward classes (`RatificationReceipt`, `RepairReceipt`) named in merged specs. |
| Accessibility gate | `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`; ADR-0028 | Live PR-time checklist with twelve categories. |
| Anti-entropy member-shell status strings | merged `#1829`, §"Member shell surface" | Closed set of seven strings. |
| Placement member-shell strings | merged `#1826`, §"Member shell" | Closed set of seven strings. |
| ScopedVault opaque-reference framing | merged `#1824`, §"ScopedVault" | Existence + scope + access path only; no body. |
| Scope vocabulary | merged `#1825` (`INSTITUTION_PACKAGE_BOUNDARY.md` §C3) | `LocalDomain` / `InstitutionalDomain` / `DomainPolicy`; not `Coop`. |
| Vocabulary discipline | merged `#1814` (`ICN_INTEGRATED_SYSTEM_MODEL.md`) | Settlement / position / obligation / allocation / receipt / provenance — not payment / wallet / currency. |

### Files Changed

1. `docs/spec/member-shell-v0.md` — new file (~500 lines).
2. `docs/registry.toml` — new entry inserted before `[docs."docs/spec/network-anti-entropy-proof-loops.md"]`.
3. `docs/INDEX.md` — Specifications section: one new line after the `network-anti-entropy-proof-loops.md` entry.
4. `docs/DOCUMENT_REGISTRY.md` — regenerated via `--write-document-registry`. Corpus moves from 813 to 814 markdown files under `docs/` (the new spec; this handoff brings it to 815 once committed).
5. `docs/dev/handoff-2026-05-15-member-shell-v0.md` — this file.

No Rust code touched. SDK untouched. Website untouched. Deploy scripts untouched. Existing specs not modified.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. `docs/spec/member-shell-v0.md` (new doc)

Authority class: `normative`. Length: roughly 500 lines. Sections:

- **Purpose** — names the nine questions the shell must answer in plain language ("Who am I in this institution?" through "How do I ask for help, challenge, review, revoke, or exit?").
- **Scope and non-goals** — twelve non-claim bullets covering platform choice, runtime change, schema redefinition, partner skinning, account-management framing, NYCN-specific labels, production claims, closure of `#1818`, the `#1631` / `#1634` ActionCard source paths.
- **Boundary lines** — five hard boundaries: vs steward cockpit (`#1795`), vs node operator civic-role surface (`#1613`), vs public website, vs institution-package skin, vs backend / runtime.
- **Design principles** — ten v0 principles: mobile-first, accessibility-first, offline-tolerant, plain language before formal records, receipt visible for every state change, scope-aware action, safe signing confirmations, local cache as derived state never source of truth, multilingual / inclusive-access path, no financial-product framing.
- **Member shell information architecture** — ten surfaces: Home/Today, My Standing, Current Scope, Action Cards, Decisions/Governance, Receipts, Records/Artifacts, Privacy/Access, Sync/Offline status, Help/Challenge/Review/Exit.
- **ActionCard rendering contract** — fourteen-field rendering table (one row per ADR-0027 schema field) plus eight card states the shell must distinguish (Open, Open but paused, Closed: insufficient authority, Closed: deadline passed, Closed: superseded, Closed: confirmed, Closed: declined, Forward-direction). Explicit "reserved source paths" subsection naming `signal_rule` / `obligation_lifecycle` as inert in v0.
- **Standing surface** — six rendering elements (identity, memberships, roles, mandates and authority, current scope, what is open / unavailable and why).
- **Signing / confirmation flow** — ten-step pre-confirm flow ending in post-action receipt status. Draft intent vs signed-but-pending vs confirmed distinguished visibly.
- **Receipt and provenance rendering** — three-tier rendering (plain summary → explanation → formal record under details) plus the always-visible challenge / review path where the receipt class supports one.
- **Offline / low-bandwidth behavior** — six rules including cache-as-derived, stale markers, blocked-actions-when-sync-required (per `#1829` Boundary rule 6), draft intent vs signed action distinction, queued-but-not-accepted vs confirmed distinction, sync degradation strings used verbatim.
- **Privacy and ScopedVault member affordances** — six rules including existence-only surfacing, who can access, why, access/export receipts, request/review/challenge path, and the hard rule "no dashboard preview of private vault content."
- **Accessibility baseline** — twelve-category gate inherited from `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` and ADR-0028.
- **Member-facing status vocabulary** — closed v0 set: seven sync-state strings (from `#1829`), seven execution-scope strings (from `#1826`), nine action-lifecycle strings (defined here), five privacy / disclosure strings (defined here), eight receipt-class plain-language labels (mapping to existing receipt classes per ADR-0026 / ADR-0025 / merged sibling specs).
- **Failure and safety table** — eighteen rows.
- **First safe proof-loop / dogfood slice** — three fixture-first slices: Slice A (read-only standing + ActionCard + receipt + sync-delayed rehearsal, preferred), Slice B (signing flow rehearsal), Slice C (offline / degraded sync rehearsal). All fixture-only; no real members; no live network; no runtime mutation; no partner data; no production claim.
- **Relationship to sibling work** — comprehensive cross-link table including every merged spec, every relevant open issue, and every relevant ADR.
- **Non-claims (repeat block for grep clarity)** — thirteen grep-friendly negation lines.

### 2. `docs/registry.toml` (one new entry)

`[docs."docs/spec/member-shell-v0.md"]`:
- `category = "architecture"`
- `status = "draft"`
- `domain_tags = ["member-shell", "mobile", "accessibility", "ux", "action-cards", "standing", "receipts", "offline", "privacy", "kernel-app-boundary", "spec"]`
- `depends_on` enumerates the integrating spine doc, meaning-firewall doc, boundary doc, seven merged sibling specs, the accessibility gate, nine ADRs, and the ActionCard schema file.

### 3. `docs/INDEX.md` (Specifications section)

One new line after the `network-anti-entropy-proof-loops.md` entry, mirroring the section pattern of recent specs.

### 4. `docs/DOCUMENT_REGISTRY.md` (regenerated)

Via `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. Corpus moves from 813 to 814 markdown files under `docs/` after spec lands; will move to 815 after this handoff lands.

### 5. `docs/dev/handoff-2026-05-15-member-shell-v0.md` (this file)

Filename uses the descriptive topic suffix convention canonicalized by PR #1827. Structure aligns with `docs/dev/HANDOFF_TEMPLATE.md`.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work + preserved boundaries -->

### Incomplete work this session deliberately did not start

- [ ] Live UI implementation of the v0 shell (native app, PWA, web, or otherwise). The platform decision is itself a separate concern.
- [ ] The fixture-first dogfood slices named in §"First safe proof-loop / dogfood slice" (Slice A read-only rehearsal, Slice B signing-flow rehearsal, Slice C offline / degraded rehearsal). Tracked as follow-ups below.
- [ ] Integration with `#1610` glossary endpoint and `#1740` multilingual access for translation tagging of the closed status-string set.
- [ ] Layer 4 ProvenanceQuery (`#1438`) consumption when that surface lands; the shell currently consumes Layers 1–3 only.
- [ ] Steward cockpit spec (`#1795`) — the operator-side complement of this PR.
- [ ] Node operator civic-role surface spec (`#1613`).
- [ ] Per the established session pattern: **"Apply valid feedback only. Stop and summarize unless explicitly told to merge."** Wait for explicit merge instruction.
- [ ] Decision on whether to file the follow-up issue drafts listed in §"Follow-up Issue Drafts" (deferred to user).
- [ ] Closure of `#1818`. The PR uses `Refs: #1818`; closure is left to the user against the issue's acceptance criteria.

### Preserved scope boundaries (explicitly NOT changed)

- No Rust code in `icn/crates/` or `icn/apps/`.
- No SDK changes (`sdk/typescript/`, `sdk/react-native/`).
- No website changes (`web/`).
- No new endpoints. `/me/standing` (per ADR-0020) and `/me/action-cards` (per `#1608` / `#1646`) are consumed verbatim.
- No new receipt classes. The shell maps existing classes to plain-language labels at the rendering layer only.
- No new ActionCard schema fields. The fourteen fields in `docs/contracts/institution-package/action-card.schema.json` are consumed verbatim.
- No new ADRs. The spec cross-links ADR-0014, -0019, -0020, -0025, -0026, -0027, -0028, -0030, -0031 without amending them.
- No edits to merged sibling specs.
- No edits to `#1818`'s GitHub issue body.
- No platform decision (iOS, Android, PWA, web). Forward-direction.
- No `signal_rule` / `obligation_lifecycle` enablement; both remain RFC-gated and inert in v0.
- No K3s, DNS, Forgejo, gateway, storage-backend, or deploy-script changes.
- No `DataLocality::CoopReplicated` migration. No `FuelLimit` / `fuel_limit` rename. No `payment_rate` / `payment_currency` reconciliation. No `PrivacyClass` taxonomy migration. All carried forward as separate follow-ups in prior handoffs.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **ADR-0020 `/me/standing` is the canonical standing read model.** Verified by reading the ADR header and the implementation_status line. If a parallel in-flight PR amends the ADR, the spec's standing surface mapping needs a re-read.
- **ADR-0027's 14 ActionCard schema fields are the canonical set.** Verified by reading the schema JSON properties directly (`id`, `source_kind`, `action_kind`, `scope`, `title`, `summary`, `authority_basis`, `required_authority_scope`, `deadline`, `risk_level`, `accessibility_hint`, `receipt_expected`, `source_id`, `domain_id`). If a parallel in-flight PR adds a field, the rendering table needs an addendum.
- **ADR-0028's twelve accessibility categories are the canonical gate.** Verified by reading the ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md file structure. The category set may be reordered or renamed in future ADR work; this spec inherits the structure as it stands today.
- **The closed seven-string sync vocabulary from merged `#1829` is canonical and complete.** Verified by reading `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface" on the merged copy in `main`.
- **The closed seven-string execution-scope vocabulary from merged `#1826` is canonical and complete.** Verified by reading `docs/spec/compute-placement-policy.md` §"Member shell" on the merged copy in `main`.
- **`signal_rule` and `obligation_lifecycle` source paths remain RFC-gated.** Verified per ADR-0027 implementation_status. If `#1631` or `#1634` lands implementation while this spec is in review, the "reserved source paths" subsection needs an update.
- **`#1818` acceptance criteria.** The issue body lists scope rules (mobile-first, accessibility-first, etc.) rather than enumerated acceptance criteria. The spec advances every named scope rule. Closure is a user-driven decision; the PR uses `Refs:`.
- **`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` exists in main.** Verified by `ls`. If the doc is renamed or restructured before merge, the cross-link needs an update.
- **ADR-0028 exists and references `#1610` glossary endpoint.** Verified by reading the ADR header. If `#1610` is restructured, the translation-readiness mapping in the accessibility section may need an update.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Run validation suite (commands below).
2. Commit (single commit; `docs(spec): define member shell v0`).
3. Push branch.
4. Open PR with `Refs: #1818` and the cross-refs in §"Refs."
5. Watch initial CI; address any failure rooted in this PR's added doc.
6. Watch the AI reviewer outputs (Copilot, Codex). Apply only valid feedback (verify each claim against the live repo with grep before acting). Reply to review threads with commit SHA and exact fix.
7. **Stop and summarize after applying valid feedback.** Do not merge unless explicitly told to merge.
8. The natural next architecture-spec PR after `#1828`-or-equivalent lands is `#1795` (steward cockpit) — the operator-side complement of this PR. Alternative: `#1767` (encrypted distributed private-overlay storage) if the user prefers private-storage work; the member-shell spec's privacy / ScopedVault affordances forward-reference it.

---

## Architectural Decisions
<!-- TRUTH TYPE: Decision truth — choices made and why -->

### 1. The member shell is app-side rendering only.

Per `docs/architecture/KERNEL_APP_SEPARATION.md`. The kernel never imports member-shell strings, layouts, status enums, or rendering rules. This keeps the meaning firewall intact: the kernel enforces constraints; the policy oracle decides constraint values; the shell renders the result. Matches the pattern from #1826 (placement classes are app-side) and #1829 (proof-loop artifacts are app-side).

### 2. Closed status-string vocabulary, inherited not invented.

The spec uses the merged seven-string sync set from `#1829` and the merged seven-string placement set from `#1826` **verbatim**. It adds a small action-lifecycle set (nine strings), a privacy / disclosure set (five strings), and the receipt-class plain-language label set (eight strings). All sets are closed; v0 violations include inventing new strings outside these sets. This prevents string drift between member-facing surfaces in different parts of the institution.

### 3. ActionCard rendering contract, not ActionCard schema redefinition.

ADR-0027 defines the schema (14 fields). This spec defines the **rendering rules** for each field at the member-shell layer — which fields are mandatory in the list-row vs the pre-confirm summary vs the details affordance, how raw enum values translate to plain-language labels, and how reserved source paths (`signal_rule`, `obligation_lifecycle`) are kept inert at v0. The schema is unchanged.

### 4. Local cache is derived state, not source of truth.

A v0 hard rule. The shell may cache freely for offline performance, but it never treats the cache as authoritative. Every cached surface shows "last refreshed at <time>"; stale cache renders stale; pending actions that depend on stale state render `Action paused until records sync` rather than letting the member act on stale evidence. This matches the storage-durability-policies.md rule that backups are not source of truth, and the anti-entropy spec's Boundary rule 8 (no member-facing lie).

### 5. Reversibility and challenge windows are surfaced before confirm.

The ten-step signing flow makes reversibility legible **before** any confirm gesture. Irreversible actions require a distinct second confirmation; single-tap confirm is never allowed for irreversible actions. This matches ADR-0028 category 7 ("Cognitive load and step complexity — consequences explained pre-action, reversibility legible") and category 12 ("Governance and action access — ActionCards expose authority basis, status, reversibility, decision, and consequence in plain language before confirm").

### 6. Private vault contents never reach the rendering layer.

A v0 hard rule. The shell receives ScopedVault references as opaque digests; the body bytes never reach the rendering surface. If the rendering layer receives body bytes for a `PrivateEvidence` artifact, the surface drops them and logs a safety failure. This matches `docs/spec/artifact-registry-and-scoped-vault.md` §"ScopedVault" and the privacy / custody rules in `docs/spec/network-anti-entropy-proof-loops.md` §"Privacy and custody rules."

### 7. The PR uses `Refs:`, not `Closes:`.

Per the standing session pattern and the `feedback_pr_refs_not_closes_unless_fully_satisfied` memory: closure of `#1818` requires verification that all named scope rules and the implied "v0 design specification merged" criterion are fully satisfied. The spec advances every scope rule named in `#1818`'s body (mobile-first, offline-tolerant, accessibility-first, ActionCard rendering, standing surface, receipt rendering, safe signing, multilingual integration, anti-financial-product framing). **Closure is a user-driven decision.**

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn

# Confirm branch state
git checkout spec/member-shell-v0
git status --short
gh pr view <PR-number> --json mergeStateStatus,state,headRefOid,statusCheckRollup

# Re-run the validation suite
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md
python3 docs/scripts/lint-arch.py docs/spec/member-shell-v0.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Targeted vocabulary check
rg -n "payment|currency|balance|wallet|token|blockchain|crypto|NYCN|Summit|live federation|production-ready|production readiness" \
   docs/spec/member-shell-v0.md docs/dev/handoff-2026-05-15-member-shell-v0.md docs/INDEX.md docs/registry.toml

# Cross-link smoke check
rg -o "docs/[a-zA-Z0-9_/\-]+\.md" docs/spec/member-shell-v0.md | sort -u | while read p; do test -f "$p" && echo "OK $p" || echo "MISSING $p"; done

# To see the spec contents
sed -n '1,80p' docs/spec/member-shell-v0.md
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/PHASE_HISTORY.md` (last completed phase per main) and `docs/STATE.md`. No conflict with this PR; the spec is doc-only design work.
- **Implementation truth:** verified against ADR-0020, ADR-0027, ADR-0028, the action-card schema JSON, and the merged sibling spec member-shell sections. The implementation status of `/me/standing` and `/me/action-cards` was read from the ADR-0020 header, not from running the gateway against a live system.
- **Execution truth:** verified branch + PR state via `gh` commands. Initial commit SHA, PR number, head SHA recorded after commit / push.
- **Narrative truth:** loaded from prior session handoffs and the eleven merged architecture-spec PRs.
- **Known conflicts between layers:** the eight-string action-lifecycle set added by this spec is **new** to the spec corpus; it does not exist in any prior merged sibling spec. It coexists with the seven-string sync set (`#1829`) and seven-string placement set (`#1826`) without overlap.

---

## Follow-up Issue Drafts (Not Filed)

Six follow-ups drafted per the user's instruction. Paste-ready for separate review.

### 1. `test(devnet): fixture rendering rehearsal for member shell v0 (Slice A)`

```
## Purpose

Implement Slice A from `docs/spec/member-shell-v0.md` §"First safe proof-loop / dogfood slice": a fixture-only read-only rendering rehearsal that produces a standing surface, an ActionCard list, a receipt summary, a sync-delayed action, and an opaque private-record reference. Accessibility checklist applied to fixture rendering. No real members, no live network, no runtime mutation.

## Scope

- Fixture member DID, signing key, display name, device.
- Fixture LocalDomain with the four owning-entity classes.
- Fixture ActionCards: one `proposal/vote`, one `meeting/attend` (completed with receipt), one paused due to sync delay, one closed for insufficient authority.
- Fixture `PrivateEvidence` artifact rendered as existence + scope + access path only.
- Twelve-category accessibility checklist passed on the fixture rendering.

## Non-goals

- No live shell implementation.
- No platform choice (iOS, Android, PWA, web).
- No partner skin.

## Related

- `docs/spec/member-shell-v0.md` §"First safe proof-loop / dogfood slice" Slice A.
```

### 2. `test(devnet): signing flow rehearsal for member shell v0 (Slice B)`

```
## Purpose

Implement Slice B: the ten-step pre-confirm flow walked by a fixture member through a fixture ActionCard. Verifies the flow's accessibility, plain-language consistency, reversibility / privacy / sync warning surfaces.

## Scope

- Same fixture stack as Slice A.
- Fixture member walks the ten-step pre-confirm summary.
- Confirm gesture is exercised; fixture receipt arrives.
- Failure surfaces from the eighteen-row failure / safety table exercised in negative-path fixtures.

## Non-goals

- No live signing key handling.
- No real authority validation.
- No platform decision.

## Related

- `docs/spec/member-shell-v0.md` §"Signing / confirmation flow" and §"First safe proof-loop / dogfood slice" Slice B.
```

### 3. `test(devnet): offline / degraded sync rehearsal for member shell v0 (Slice C)`

```
## Purpose

Implement Slice C: a fixture-first rehearsal of the offline / degraded path, transitioning through the seven anti-entropy sync states with plain-language warnings, queued draft intent, signed-but-pending state, and final receipt arrival.

## Scope

- Same fixture stack as Slices A and B.
- Fixture member starts an action while shell is in `Sync delayed`.
- Action queued as draft intent.
- Shell transitions through `Some records are being verified` and `Sync delayed / degraded`.
- Member sees plain-language warnings at each transition and is offered the help / review path.

## Non-goals

- No real anti-entropy probe traffic.
- No live federation peer.

## Related

- `docs/spec/member-shell-v0.md` §"Offline / low-bandwidth behavior" and §"First safe proof-loop / dogfood slice" Slice C.
- `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface."
```

### 4. `spec(web): pick the v0 platform target (PWA vs native vs hybrid)`

```
## Purpose

The member-shell v0 spec is deliberately platform-agnostic. This issue is the separate decision-making PR that picks the platform target — likely PWA-first given the mobile-first / accessibility-first / offline-tolerant constraints. Tracks the trade-off analysis and the implementation roadmap.

## Scope

- Trade-off analysis: PWA vs native iOS/Android vs hybrid.
- Constraints: mobile-first, offline-tolerant, accessibility-first, plain-language-first, must satisfy the twelve-category gate.
- Implementation roadmap (which surfaces land first).

## Non-goals

- Not the v0 rendering contract (that's `docs/spec/member-shell-v0.md`).
- Not a partner-pilot decision.
- Not a native-app commitment in this issue.

## Related

- `docs/spec/member-shell-v0.md` §"Boundary lines" and §"Design principles."
- `#1818` parent issue.
```

### 5. `spec(member-shell): integrate with #1610 glossary + #1740 multilingual access`

```
## Purpose

Define the integration contract between the member shell's closed status-string vocabulary and the glossary endpoint (`#1610`) plus the multilingual access plan (`#1740`). Names which strings are translation-tagged, how fallbacks render, and how the institution-package skin layer overlays local terms.

## Scope

- Translation tagging for the closed status-string set (sync + execution-scope + action-lifecycle + privacy / disclosure + receipt-class labels).
- Fallback strategy when current-language translation is missing.
- Institution-package localization seam.

## Non-goals

- No new glossary endpoint behavior.
- No new web-i18n framework choice.

## Related

- `docs/spec/member-shell-v0.md` §"Design principles" (multilingual / inclusive-access path) and §"Member-facing status vocabulary."
- `#1610`, `#1740`.
```

### 6. `spec(member-shell): consume Layer 4 ProvenanceQuery when it lands (#1438)`

```
## Purpose

When the generic Layer 4 `ProvenanceQuery` receipt-index surface lands (`#1438`), update the member-shell receipt rendering contract to consume it for the Receipts surface. Until then, the shell consumes Layers 1–3 (per ADR-0026) via the existing receipt-class endpoints.

## Scope

- Mapping from `ProvenanceQuery` results to the member-shell plain-language receipt summaries.
- Pagination, freshness, and stale-marker handling for the Receipts surface.
- Cross-link to `RoutingProof` / `RedundancyProof` / `RepairReceipt` (per merged `#1829`) when those land in the receipt index.

## Non-goals

- No `ProvenanceQuery` implementation (that's `#1438`).

## Related

- `docs/spec/member-shell-v0.md` §"Receipt and provenance rendering."
- ADR-0026 §4.
- `#1438`.
```

---

## Process Note

The AGENTS.md handoff-path drift was resolved by **PR #1827** (merged 2026-05-15). The active convention is `docs/dev/handoff-YYYY-MM-DD-<topic>.md`, which is what this handoff uses. **Do not carry the old AGENTS.md drift forward** in future handoffs unless an AI reviewer surfaces it again, in which case respond with a verified rebuttal that points at #1827.
