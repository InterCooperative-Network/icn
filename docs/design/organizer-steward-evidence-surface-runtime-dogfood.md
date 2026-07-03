# Organizer-steward evidence surface runtime dogfood — Design Contract

**Status**: draft — design / implementation-planning contract (not runtime implementation)
**Truth class**: descriptive / implementation-planning
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-03
**Source basis**: read against `main` @ `df9d8a50` (the #2288 merge commit and current `origin/main` tip). Code anchors and struct fields were verified at that commit — re-verify exact line numbers before relying on them.
**Related**: issue #2289 (the scoped tracker this contract serves) · issue #1748 (Institutional Process Substrate milestone) · issue #2141 (vertical spine control) · issue #2041 (member-shell v0 human/AT accessibility pass) · [docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md](ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) · [docs/contracts/rehearsal-evidence-export.schema.json](../contracts/rehearsal-evidence-export.schema.json) · [docs/pilots/no-cli-organizer-member-rehearsal-workflow.md](../pilots/no-cli-organizer-member-rehearsal-workflow.md) · [docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)

> Narrow implementation-planning document for #2289 only. It turns the scoped
> issue into an implementation-ready contract for the
> `receipt → surface → evidence/export` tail of the #1748 / #2141 vertical
> spine. It changes nothing at runtime: no Rust, no member-shell UI behavior,
> no schema, no OpenAPI/SDK, and no new receipt class. The surface itself is a
> future implementation PR under #2289.

## 1. Status / truth class

- **Status**: draft design contract.
- **Truth class**: descriptive / implementation-planning — this document describes
  verified current artifacts and pins a target slice contract; it does **not**
  claim any of the target surface behavior exists yet.
- **Canonical** current-implementation truth remains [docs/STATE.md](../STATE.md),
  [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md), and live code. This doc is
  non-canonical for runtime truth.
- Related: #2289, #1748, #2141, #2041.

## 2. Purpose

Turn #2289 into an implementation-ready contract for the
`receipt → surface → evidence/export` tail of the vertical spine
(`package → domain → policy → binding → process/action → receipt → surface → evidence/export`,
per [docs/architecture/ICN_OPERATING_MODEL.md](../architecture/ICN_OPERATING_MODEL.md)).

Four process-transition receipt classes already land under ADR-0026 Layer 2
(§4). The still-open portion of #1748 / #2141 is not more kernel receipt
plumbing — it is making those already-landed receipts legible to a human
organizer or steward on the existing member-shell surface, and exporting a
repo-safe evidence summary, behind an accessibility gate and a fixture-safe
privacy/visibility boundary. This document names the exact receipt path, human
surface, fixture data, evidence/export shape, privacy/redaction model, and
accessibility validation plan so the later implementation PR is a mechanical
build, not a fresh design.

## 3. Non-claims

This contract, and the implementation PR it plans, make **none** of the following claims:

- not production;
- not pilot;
- not organizer-ready;
- not member-ready;
- not live federation;
- not NYCN activation;
- not Phase 2 completion;
- not real partner / private / attendee / sponsor / accommodation data;
- not a new receipt class (the slice reads the four existing classes only);
- not raw process-spine completion (`ActivationCrossedReceipt`,
  `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`,
  `EvidencePacketProducedReceipt` remain unbuilt and out of scope);
- not #2081 / #2080 / #2274 progress;
- not a runtime `EvidencePacket` producer (none exists; see §7).

Receipts record institutional facts and grant zero authority. Nothing here
authorizes closing #1748, #2141, or #2041.

## 4. Existing grounded artifacts (verified this session)

All verified on `main` @ `df9d8a50`.

**Receipt structs** — defined in [`icn/crates/icn-governance/src/proof.rs`](../../icn/crates/icn-governance/src/proof.rs), each with its own blake3 `DOMAIN_TAG`, all already emitted, persisted, and retrievable:

| Struct | Domain tag | Key fields (verified) |
|---|---|---|
| `ProcessSessionOpenedReceipt` | `icn:gov:process_session_opened:v1` | `session_id`, `domain_id`, `opened_by`, `opened_at`, `record_hash` |
| `DeliberationEntryRecordedReceipt` | `icn:gov:deliberation_entry_recorded:v1` | `domain_id`, `session_id`, `entry_id`, `author`, `entry_kind`, `recorded_at`, `body_hash`, `record_hash` |
| `DecisionRecordedReceipt` | `icn:gov:decision_recorded:v1` | `domain_id`, `session_id`, `decision_id`, `recorded_by`, `recorded_at`, `body_hash`, `record_hash` |
| `ProcessGateResultReceipt` | `icn:gov:process_gate_result:v1` | `session_id`, `domain_id`, `gate_kind`, `result`, `recorded_by`, `recorded_at`, `record_hash` |

Note: the deliberation-entry and decision receipts store a `body_hash` only — the
raw body is **never stored**. This is load-bearing for the privacy model (§8).

**Surface** — the member-shell v0 reference client:

- [`web/member-shell/`](../../web/member-shell/)
- [`web/member-shell/index.html`](../../web/member-shell/index.html)
- [`web/member-shell/shell.js`](../../web/member-shell/shell.js) — has a `demo` / `live` mode boundary, holds rendered receipts in state as `{receipt, plainContext}`, and already labels a demo receipt hash as illustrative.
- [`web/member-shell/fixtures/`](../../web/member-shell/fixtures/) — carries `demo-completion-receipt.json`, `community-completion-receipt.json`, `community-standing.json`, `community-action-cards.json`; the shared demo pack lives under `web/pilot-ui/fixtures/icn-organizer-demo/`.

**Contracts / gate:**

- [`docs/contracts/rehearsal-evidence-export.schema.json`](../contracts/rehearsal-evidence-export.schema.json) — `urn:icn:contract:rehearsal-evidence-export:v1` (x-icn-status `rfc`).
- [`docs/contracts/preview-review.schema.json`](../contracts/preview-review.schema.json) — `urn:icn:contract:preview-review:v1`.
- [`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — 12 review categories §3.1–§3.12.
- [`docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md`](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) — the receipt/provenance envelope the four classes sit under (Layer 2). There is **no** single `ProcessTransitionReceipt` base trait/enum; the name is a conceptual family.

**Read-model precedent** — [`ops/ideas/dogfood/institutional-process-substrate-mvp.md`](../../ops/ideas/dogfood/institutional-process-substrate-mvp.md) composes spine shapes by hand and emits no receipt; per `ops/ideas/README.md` § "Dogfood slice variants" a read-model walk does not satisfy receipt-backed promotion thresholds. This slice is the **surface** variant over receipts that already emit — still fixture/local-safe, still not a promotion claim.

## 5. Chosen receipt path

The implementation PR renders exactly this sequence, using only the four
existing classes (no new class):

```text
ProcessSessionOpenedReceipt
  → DeliberationEntryRecordedReceipt
  → DecisionRecordedReceipt
  → ProcessGateResultReceipt
```

Contribution of each receipt to the organizer/steward evidence story:

1. **`ProcessSessionOpenedReceipt`** — anchors the story: *a process session was
   opened*, scoped to `(domain_id, session_id)`, by `opened_by` at `opened_at`.
   Establishes the "who / where / when it started" spine.
2. **`DeliberationEntryRecordedReceipt`** — *a deliberation input was recorded*
   against that session: `entry_kind`, `author`, `body_hash` (proof-of-content,
   not content). Establishes "what was considered", with the private text held
   only as a hash.
3. **`DecisionRecordedReceipt`** — *a decision artifact was recorded* against the
   session: caller-opaque `decision_id`, `recorded_by` (recorder-not-decider),
   `body_hash`. Establishes "what was decided as an institutional fact" —
   without claiming validity, bindingness, quorum, or vote semantics.
4. **`ProcessGateResultReceipt`** — *a process gate result was recorded*:
   `gate_kind`, `result`, `recorded_by`. This is where the **accessibility gate**
   result attaches (§9), giving the evidence surface a receipt-backed statement
   that the gate ran.

If audit during implementation proves this exact sequence is not reproducible
from an existing local/fixture path, the PR narrows to the largest reproducible
sub-sequence and records the gap — it does not invent a new receipt class to
fill it (that would be a separate issue).

## 6. Human surface contract

The evidence story is exposed on the existing [`web/member-shell/`](../../web/member-shell/)
surface (or a clearly-scoped sibling view inside it). No new framework, no build
step, no runtime dependency. The surface must provide:

- **Plain-language summary** first — a human reads *what happened* (session
  opened, entry recorded, decision recorded, gate result) before any raw
  structured data, honoring accessibility gate §3.11 "understandable in summary
  form before drilling into raw structured data".
- **Evidence-detail disclosure** — the record-level fields (`record_hash`,
  `*_id`, `body_hash`, `recorded_at`, `recorded_by`/`author`/`opened_by`) live
  behind a progressive-disclosure control, reusing the existing member-shell
  pattern of holding receipts as `{receipt, plainContext}` and labeling
  illustrative hashes.
- **Receipt / provenance fields** shown honestly: `record_hash` as the proof
  pointer; `body_hash` labeled as *proof-of-content, body not stored*; recorder
  DIDs labeled as *who recorded this fact*, not *who decided*.
- **Fixture / dry-run / live-local boundary labeling** — the surface must make
  the mode unmistakable (gate §3.11: a viewer must tell fictional / fixture-only
  / dry-run / local-non-production / live apart *without parsing JSON*). This
  slice is **fixture-only**; the label says so.
- **Explicit non-CLI reader path** — the browser member-shell is the reader; it
  contrasts with the existing CLI path (`icnctl receipts …`), which stays as-is.
- **No readiness claim** — no copy asserts organizer-ready or member-ready; the
  surface is a dev/fixture evidence view.

## 7. Evidence / export contract

The slice produces a **contract-conformant fixture export** — an *evidence
summary*, not a new runtime artifact class. It conforms to (maps onto)
`urn:icn:contract:rehearsal-evidence-export:v1`
([`docs/contracts/rehearsal-evidence-export.schema.json`](../contracts/rehearsal-evidence-export.schema.json)).

Classification of what is produced:

- it is an **evidence summary** (human-facing plain-language digest), and
- a **fixture export** (fixture-only inputs), and
- a **contract-conformant artifact** (validates against the rehearsal-evidence-export schema).

There is **no runtime `EvidencePacket` producer** today — the audit found only
the fixture-level `EvidencePacket` struct in `icn/crates/icn-baseline-lock/src/evidence.rs`
(not wired to these receipts) and the hand-composed read-model walk. This slice
therefore composes the export from the rendered receipts; it does **not** claim
or add an `EvidencePacketProducedReceipt`.

Mapping the receipt-backed facts into the contract's required fields:

| Contract field | Source in this slice |
|---|---|
| `rehearsal_mode` | `fixture-only` |
| `workflow_steps_completed` | derived from the §5 receipt sequence (e.g. `start`, `decide`, `surface-result`, `export-evidence`) |
| `decision_outcomes[]` | `DecisionRecordedReceipt` + `ProcessGateResultReceipt`, using the schema's closed `category` enum |
| `source_material[]` | committed fixtures (`kind: committed-fixture`, basename only) |
| `preview_review_boundary.enforced` | `true` (read-first, no mutation) |
| `mutation_boundary` | `{ executed: false, target: "none" }` |
| `privacy_review` | the §8 redaction boundary result |
| `export_safety_classification` | `repo-safe` |
| `non_claims` | the §3 non-claims |

The export must honor the schema's `x-icn-must-not-include` list (no real names,
emails, rolls, tokens, private paths, or fields implying pilot/production).

## 8. Privacy / visibility boundary

Modeled with **fixture data only**. The concrete fixture scenario for the
implementation PR:

- one `DeliberationEntryRecordedReceipt` (fixture) is **visible to the steward
  body** in full plain-language summary;
- the **same entry is redacted from the member / export view**;
- the export and the member view show the **redaction reason** and a **proof
  pointer** (`record_hash`, and `body_hash` as proof-of-content) **without
  leaking the private text**.

This is honest by construction: the deliberation-entry and decision receipts
store `body_hash` only — the raw body is never in the receipt, so the "redacted"
view is showing exactly what the receipt actually holds (a hash + metadata),
while the "steward" view supplies the plain summary from fixture context. No
real private text exists in any fixture; redaction is demonstrated on fictional
content. This exercises the schema's `privacy_review` field and gate §3.11's
fictional/fixture/live boundary labeling.

## 9. Accessibility gate

The implementation PR is gated by
[`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md).
The 12-category checklist (§3.1–§3.12) is applied at PR time on the real
surface and copied into the PR body with a per-category verdict
(`Pass` | `Pass with documented follow-ups #<issue>` | `Blocked` | `N/A with reason`).

Especially load-bearing for this surface:

- **§3.11 Receipts, provenance, and evidence access** — receipts understandable
  in summary before raw drill-in; evidence packet status visibly distinguishes
  fictional / fixture-only / dry-run / local-non-production / live without
  parsing JSON; proof surfaces explained in plain language. The gate doc itself
  states §3.11 "sits over" the rehearsal-evidence-export rendering surfaces.
- **§3.12 Governance and action access** — authority basis, status,
  reversibility, required decision, and consequence in plain language before any
  confirm step; receipt name stated pre-action.

The gate outcome should attach as the `ProcessGateResultReceipt` in the §5
sequence (`gate_kind` = the accessibility-review gate), so the evidence surface
carries a receipt-backed statement that the gate ran.

**#2041 interaction:** the human / assistive-technology categories still owed on
member-shell v0 — screen-reader (§3.2 / §3.9), low-vision 200% zoom (§3.3),
switch input (§3.5) — remain **visible as pending** (`Pass with documented
follow-ups #2041`) unless the implementation PR actually performs and records
that human/AT pass. This slice does not silently mark them Pass, and does not
close #2041.

## 10. Implementation PR plan (surfaces, not changed here)

The later implementation PR is expected to touch, at most:

- [`web/member-shell/index.html`](../../web/member-shell/index.html) — evidence-view markup (semantic HTML, landmarks, disclosure).
- [`web/member-shell/shell.js`](../../web/member-shell/shell.js) — render the §5 receipt sequence as `{receipt, plainContext}`, mode/boundary labeling, redaction handling.
- `web/member-shell/fixtures/<new fixture>.json` — a new fixture pack carrying the four-receipt sequence (wire-shaped per `proof.rs`) plus the redaction scenario.
- a repo-safe evidence-summary export artifact conforming to the rehearsal-evidence-export contract (location per convention — likely alongside the fixture or under `docs/demo/`).
- [`docs/demo/`](../../docs/demo/) — a new or extended walkthrough doc recording the evidence surface (e.g. alongside `JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md`).
- smoke/test scripts only if an existing member-shell test convention (e.g. the Playwright/axe suite referenced by #2041/#2239) supports it.

None of these files are modified by this design PR.

## 11. Validation obligations (for the later implementation PR)

- the new fixture loads in member-shell (`?mode=demo`) without network;
- the evidence-detail disclosure is keyboard reachable with visible focus;
- redaction is visible and non-leaky (no private text in DOM, state, or export);
- the export validates against `urn:icn:contract:rehearsal-evidence-export:v1`
  if an existing validator is available (else a documented manual check);
- the accessibility gate checklist (§3.1–§3.12) is applied and pasted into the PR body;
- no production / pilot / live-federation / NYCN / Phase 2 claim in code, copy, or PR;
- #1748 / #2141 / #2041 remain open unless separately reviewed.

## 12. Open questions / explicit deferrals

- **Exact fixture names** — not fixed here; the implementation PR names them
  (recommend a single `process-evidence-*.json` pack mirroring the existing
  `*-completion-receipt.json` convention).
- **Schema validator** — decide in the implementation PR whether to add a small
  validator (e.g. against the JSON Schema) or reuse an existing docs/contracts
  validation path; no new tooling should be introduced casually (AGENTS.md
  "No new tooling").
- **`ProcessGateResultReceipt` provenance** — decide whether the gate receipt is
  **fixture-simulated** (a wire-shaped fixture record) or **generated through an
  existing local path**. Default assumption: fixture-simulated, since this slice
  is fixture-only and must not stand up a live gateway.
- **#2041 human/AT pass** — decide whether the human assistive-technology pass is
  performed inside the implementation PR (and #2041 advanced only if actually
  executed) or remains a **linked blocker** with the pending categories marked
  `Pass with documented follow-ups #2041`. Default: linked blocker; do not close
  #2041 from the evidence-surface PR.

---

_Refs #2289. Refs #1748. Refs #2141. Refs #2041._
