# Institutional Process Substrate — MVP dogfood slice

**Idea card:** `ops/ideas/ideas.yaml#idea-0019`
**Framing brief:** `ops/ideas/framing/institutional-process-substrate.md`
**Coordination issue:** [#1748](https://github.com/InterCooperative-Network/icn/issues/1748)
**Owner / session:** 2026-05-05 session
**Date:** 2026-05-05
**Status:** read-model / fixture-walk dogfood. Not runtime. Not a
schema. Not a decision. Not a pilot. Not production.

> **Slice discipline.** The repo's standard dogfood-slice template
> (`ops/ideas/templates/dogfood-slice.md`) defines a **NYCN-real,
> receipt-backed** dogfood slice as the primary pattern. This
> artifact is the documented **read-model fixture-walk variant**
> formalized in `ops/ideas/README.md` § "Dogfood slice variants"
> (added in this PR alongside the slice). The variant uses
> fictional, repo-safe material exclusively, composes against
> already-committed contract examples and shipping ADRs without
> modification, emits no receipts, contacts no gateway, performs no
> mutation, and writes nothing outside the `ops/ideas/` refinery.
> Its purpose is to prove the spine **composes** as a read-model
> shape, not that runtime works. A read-model fixture walk does
> **not** satisfy promotion thresholds that require receipt-backed
> runtime evidence — see "Promotion gate" below for the full list.
> A future canonical (NYCN-real) dogfood slice is the next artifact
> after this one.

## What this slice proves

The Institutional Process Substrate spine named in `idea-0019`
(`ProcessTargetRef` → `ProcessSession` → `PreviewReviewPacket` →
`DeliberationThread` → `DeliberationEntry` → `HumanDecisionSet` /
`DecisionRecord` → `MutationPlan` → `ActionCardTrigger` →
`ProcessGateResult` → `EvidencePacket`) can be walked end-to-end as a
**read-model composition** against existing committed contract
artifacts (`urn:icn:contract:preview-review:v1`,
`urn:icn:contract:rehearsal-evidence-export:v1`) and existing ADRs
(`ADR-0026` proof envelope, `ADR-0027` action-card contract,
`ADR-0028` accessibility baseline) — without modifying any kernel,
runtime, gateway, ledger, governance, or SDK code, and without
introducing any new schema.

The composition is the proof. If the spine could not be walked using
already-shipped pieces, the framing brief would be incoherent. It can
be. The slice records the walk so future readers can replay it.

## What this slice does NOT prove

- Not runtime. No process is actually executed; no state machine
  ticks; no API is called.
- Not a schema. No new JSON Schema, no new TOML row, no new contract
  identifier is introduced.
- Not a full deliberation system. `DeliberationThread` /
  `DeliberationEntry` are walked as a read-model only.
- Not production deployment. Nothing in this slice ships to any
  runtime surface.
- Not a formal NYCN pilot. The fictional institution is "Example
  Cooperative", not NYCN. NYCN remains the intended first cooperative
  partner, not a committed pilot.
- Not live federation. No cross-cooperative coordination is exercised.
- Not private-overlay implementation. Holder-label to DID activation
  is referenced as a separate concern (`#1730`); the slice does not
  implement it.
- Not a CCL syntax decision. CCL hooks are named where they would
  attach; CCL syntax is not specified.
- Not a conflict-resolution implementation. `ADR-0029` is referenced
  as the path that would handle disputes about a `DecisionRecord`;
  conflict objects are not exercised.
- Not a binding on partner repositories or partner data. All material
  is fictional, repo-safe, and committed under `docs/contracts/`.

## Fictional scenario

**Institution:** Example Cooperative (fictional). Generic
institutional language only; no NYCN/Summit-specific nouns.

**Body running the process:** sample committee (a generic
`structure`-scoped body — see `preview-review.schema.json` `scope`
enum).

**Source material:** the already-committed fictional examples
- `docs/contracts/preview-review.example.json` — sample committee
  meeting notes preview (`preview_kind: meeting_notes_action_items`).
- `docs/contracts/rehearsal-evidence-export.example.json` — sample
  committee fixture-only rehearsal evidence packet.

**Process target:** one fictional `ProcessTargetRef` of kind
`meeting_artifact` — the sample committee meeting notes — with two
proposed action items derived from the notes (sample agenda working
group; sample obligation working group). One proposed action item is
approved as-is. One is revised before approval. One concern surfaced
in deliberation is deferred to a follow-up cycle.

No real names. No real organization. No private organizer / member /
sponsor / attendee data. No real Drive / Groups / Sheets paths.

## Slice steps (process spine walk)

The walk reads each existing or proposed spine object as a read-model.
"Existing contract" means a JSON Schema with a non-DNS URN and a
companion notes file. "ADR-shipping" means the concept ships in an
ADR with `implementation_status` recording reality. "Name candidate"
means the framing brief proposed the name; no schema or runtime
exists. "Sketch" means the slice writes the read-model shape inline
as plain language only.

### Step 0. Open `ProcessSession` and bind a `ProcessTargetRef`

- **`ProcessSession`** — *name candidate from framing brief.* Read-
  model only. The slice records: session opened by `facilitator`
  role, scope `structure`, target = sample committee meeting notes
  artifact, lifecycle state `deliberating`. No persistence. The
  session is a paper context that lets every subsequent step refer to
  the same target.
- **`ProcessTargetRef`** — *name candidate.* The slice records:
  `kind = meeting_artifact`, `local_label = "sample committee meeting
  notes"`, no DID binding (label only — DID binding is `#1730`).

### Step 1. Render the `PreviewReviewPacket`

- **`PreviewReviewPacket`** — *existing contract*
  `urn:icn:contract:preview-review:v1` (landed in PR #1745).
- **Input:** `docs/contracts/preview-review.example.json` exactly as
  committed. The slice does not author a new packet; it composes
  against the one already in the repo.
- **Composition:** the slice points at the existing example as the
  packet that the spine's *review* boundary would render at this
  step. `preview_kind: meeting_notes_action_items`,
  `proposed_artifact.kind: action_item_set`,
  `review_status: ready_for_review`, `repo_safety.classification:
  repo-safe`.
- **What the spine pins:** the `PreviewReviewPacket` is the first
  human review boundary in the session. The reviewer sees what would
  be produced — they do not produce it.
- **Receipts:** none today. The framing brief lists
  `ProcessSessionOpenedReceipt` (already cited at Step 0) as the
  receipt that would attach to the existing `ADR-0026` envelope when
  a session opens; the framing brief does **not** define a
  per-preview receipt class, and this slice does not invent one.
  Whether per-preview receipts are needed is a future open question.

### Step 2. Walk a `DeliberationThread` with three entries

- **`DeliberationThread`** — *name candidate.* Read-model only.
  Bound to one `ProcessTargetRef` inside one `ProcessSession`; not
  free-form chat. Visibility is governed by the institution's
  charter; the slice asserts default-conservative visibility (sample
  committee body only).
- **`DeliberationEntry`** — *name candidate.* The slice walks three
  entries, each tagged with a closed-taxonomy kind from the framing
  brief and each carrying generic role provenance (no names, no
  emails, no DIDs).

The three entries:

| # | Entry kind            | Author role  | Plain-language content (fictional)                                                                                                                                              |
|---|-----------------------|--------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 1 | `question`            | `reviewer`   | "Does the proposed sample obligation note specify *unit* and *provenance* explicitly, or is it deferred to the follow-up cycle?"                                                |
| 2 | `accessibility_review`| `facilitator`| "The agenda draft presented in plain language at large enough font is fine for the room; receipt-export rendering for follow-up should pair status text with status icon, not color alone." |
| 3 | `amendment`           | `organizer`  | "Propose narrowing scope of action item 1 to drafting the agenda only — leave the schedule build to a separate cycle so the deliberation is reviewable in one sitting."         |

- **Privacy boundary:** every entry uses generic role labels and
  fictional content. No real names. No private overlay references.
- **Accessibility boundary:** entry 2 is itself an `accessibility_review`
  entry — the spine uses deliberation as a first-class accessibility
  surface, not an afterthought. The
  `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` checklist
  applies to any UI that renders these entries.
- **Receipts:** none today. A future
  `DeliberationEntryRecordedReceipt` would attach to `ADR-0026`.

### Step 3. Record a `HumanDecisionSet` and a `DecisionRecord`

- **`HumanDecisionSet`** / **`DecisionRecord`** — *name candidates.*
  Read-model only. The slice walks one decision moment with three
  outcomes; a real institution may have one or many decision moments
  per session. The decision rule is recorded; no rule is invented.

The decision row (fictional). Outcome labels match the closed enum at
`urn:icn:contract:rehearsal-evidence-export:v1` →
`decision_outcomes[].category` (`approved | rejected | deferred |
edit-and-resubmit | out-of-scope | withdrawn`):

| Outcome              | Target                                        | Decision rule (recorded, not invented)                | Plain-language summary                                                                                |
|----------------------|-----------------------------------------------|--------------------------------------------------------|-------------------------------------------------------------------------------------------------------|
| `approved`           | proposed action item 2 (sample obligation)    | committee consensus, with no `objection` entry filed   | Sample obligation working group drafts a sample obligation note clarifying unit and provenance fields |
| `edit-and-resubmit`  | proposed action item 1 (sample agenda)        | committee consensus, after the `amendment` entry above | Sample agenda working group drafts the agenda only, schedule build deferred                            |
| `deferred`           | concern raised by `accessibility_review` entry| committee consensus, deferred to follow-up cycle       | Receipt-export rendering accessibility concern carried into a future cycle                              |

- **Boundary:** a decision is **not** a mutation. The decision
  authorizes a downstream `MutationPlan`; it does not execute one.
- **Receipts:** none today. A future `DecisionRecordedReceipt`
  attaches to `ADR-0026`.

### Step 4. Sketch a `MutationPlan`

- **`MutationPlan`** — *name candidate.* Read-model only. The slice
  writes the plan inline; no JSON Schema is introduced.

Plan (fictional, as a `pending_publish_summary` shape would render
under `urn:icn:contract:preview-review:v1`):

```text
plan kind:    pending_publish_summary  (preview_kind from the existing contract)
plan target:  sample committee process session
operations:
  - create:  ActionCard (kind from ADR-0027) — routes attention to the agenda
             working group's holder label for the revised action item 1
  - create:  ActionCard (kind from ADR-0027) — routes attention to the obligation
             working group's holder label for the approved action item 2
  - record:  follow-up cycle item — receipt-export accessibility rendering, no
             ActionCard yet
expected receipts:
  - one ActionItemCompletionReceipt per completed action item (existing receipt
    family under ADR-0026 envelope; not new)
  - one EvidencePacketProducedReceipt (name candidate; would attach to the
    existing envelope)
not in plan:
  - no holder-label → DID binding (deferred to #1730)
  - no live cloud sync (no Drive/Groups/Sheets touched)
  - no K3s / DNS / Forgejo mutation
```

- **Mutation boundary:** the plan is a read-model. Executing the plan
  is performed by separate, named, documented endpoints. **The
  schema's `review_status: approved_for_next_step` records that a
  human review boundary was crossed; it does not authorize any
  mutation.**
- **Receipts:** none today. A future
  `MutationPlanRecordedReceipt` attaches to `ADR-0026`.

### Step 5. Cross an `ActivationRequest` gate

- **`ActivationRequest`** — *name candidate.* Read-model only. The
  slice records: an activation request would be issued at this step,
  conditional on every relevant `ProcessGateResult` passing. The
  slice does not issue one; it walks the shape.
- **`ProcessGateResult`** — *name candidate.* The slice writes a
  small gate table:

| Gate                   | Producer (role)         | Result    | Notes                                                                                                                  |
|------------------------|-------------------------|-----------|------------------------------------------------------------------------------------------------------------------------|
| `privacy_review`       | `facilitator`           | `pass`    | All deliberation entries and the decision row are fictional and repo-safe; no private fields anywhere in the session. |
| `accessibility_review` | `facilitator`           | `pass`    | Plain language and large-font rendering confirmed; deferred future-cycle item for color-vs-icon distinction recorded.  |
| `repo_safety_review`   | `reviewer`              | `pass`    | Source material is committed example-snippets only; classification `repo-safe`.                                        |
| `scope_confirmation`   | `organizer`             | `pass`    | Confirmed `structure` scope; sample committee body authored the session.                                               |
| `no_mutation_check`    | `steward`               | `pass`    | Slice asserts no gateway contact, no write attempt, no network egress — matches the `mutation_boundary.executed: false` shape used by the existing evidence-export contract. |
| `second_reviewer_signoff` | `reviewer` (alternate)| `n/a`     | Sample committee charter does not require a second reviewer for this session kind; recorded as not asserted, not failed. |

- **Boundary:** activation **does not occur** in this slice.
  `ActivationRequest` is sketched, not issued. A real session would
  cross the gate only when every required `ProcessGateResult` is
  `pass`.

### Step 6. Sketch `ActionCardTrigger`s

- **`ActionCardTrigger`** — *binding sketch only.* The
  ADR-0027-shipping ActionCard contract carries the triggers; this
  slice does not extend `ADR-0027` and does not propose a new
  ActionCard kind. It pins which spine transitions are legitimate
  emit-points.

The legitimate emit-points exercised (read-model only):

- `ProcessSessionOpened` → action card to the facilitator (sketch).
- `DeliberationEntry: amendment` filed → action card to the
  organizer who proposed the amendment (sketch).
- `DecisionRecord: revise` recorded → action card to the agenda
  working group's holder label (sketch).
- `DecisionRecord: approve` recorded → action card to the obligation
  working group's holder label (sketch).
- `DecisionRecord: defer` recorded → action card to the facilitator
  for the follow-up cycle (sketch).

All sketches; no ActionCard is actually emitted in this slice.

### Step 7. Produce an `EvidencePacket`

- **`EvidencePacket`** — *existing contract*
  `urn:icn:contract:rehearsal-evidence-export:v1`.
- **Input:** the spine state above.
- **Composition claim — precise:** the slice composes against the
  **same contract URN** as the committed example
  `docs/contracts/rehearsal-evidence-export.example.json`. The
  committed example is **one valid two-outcome instance** of that
  contract (`approved` + `deferred`). This slice would produce a
  **different valid three-outcome instance** of the same contract
  (`approved` + `edit-and-resubmit` + `deferred`), each outcome
  drawn from the same closed `decision_outcomes[].category` enum.
  The slice does **not** claim the committed example file would be
  re-emitted byte-for-byte; it claims the committed example's shape
  and the slice's three-outcome shape are both valid against the
  same schema, which is what "composability" means in a read-model
  walk.
- **Reused shape elements** (those that *would* be identical between
  the committed example and a packet produced by this slice):
  `rehearsal_mode: fixture-only`, `preview_review_boundary.enforced:
  true`, `mutation_boundary.executed: false`,
  `export_safety_classification: repo-safe`, `proof_loop_references[]
  .status: not-attempted` for a fixture walk, generic
  `audience_categories`, fictional `rehearsal_label`. Schema
  validation against the committed example today plus schema
  validation against the three-outcome shape this slice describes
  are the two pieces of evidence; both already pass under the
  shipping `urn:icn:contract:rehearsal-evidence-export:v1` schema.
- **Repo safety:** All material is fictional.
  Holder-label → DID activation is recorded as a follow-up. No real
  partner data.
- **Receipts:** for any packet produced from this slice,
  `proof_loop_references[].status: not-attempted` would be the
  correct value (matching the committed example's posture for
  fixture-only walks). A future runtime slice would flip that.

## Trace table (one-row-per-step)

| Step | Spine object              | Status                 | Input                                                | Human review boundary               | Privacy boundary                                                                  | Receipt / evidence relationship                                                                                  | Implementation status                                                          |
|------|---------------------------|------------------------|------------------------------------------------------|-------------------------------------|-----------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| 0    | `ProcessSession` / `ProcessTargetRef` | name candidate          | meeting-notes artifact (fictional, label only)        | session opens; reviewer not yet engaged | label only; no DID; no real org                                                | `ProcessSessionOpenedReceipt` — name candidate, would attach to ADR-0026 envelope                                | not runtime; read-model only                                                   |
| 1    | `PreviewReviewPacket`     | existing contract       | `docs/contracts/preview-review.example.json` (committed) | first review boundary; reviewer sees the packet | repo-safe; `additionalProperties: false` enforces                                | none — framing brief does not name a per-preview receipt class; the session-open receipt at Step 0 covers the boundary | shipping read-model contract; rendered as-is in slice                          |
| 2    | `DeliberationThread` / `DeliberationEntry` | name candidate (closed-taxonomy proposed) | three fictional entries (question / accessibility_review / amendment) | object-bound entries; not chat       | charter-governed visibility default-conservative; no PII                          | future `DeliberationEntryRecordedReceipt` (name candidate) on existing envelope                                  | not runtime; read-model only                                                   |
| 3    | `HumanDecisionSet` / `DecisionRecord` | name candidate          | three deliberation entries + decision rule (consensus, no objection) | second review boundary; decision is not mutation | generic role labels; no names                                                    | future `DecisionRecordedReceipt` (name candidate) on existing envelope                                           | not runtime; read-model only                                                   |
| 4    | `MutationPlan`            | name candidate (sketch) | decision record                                       | preview-shape (`pending_publish_summary`) | repo-safe by construction; no live mutation                                       | future `MutationPlanRecordedReceipt` (name candidate) on existing envelope                                       | not runtime; sketch only                                                       |
| 5    | `ActivationRequest` / `ProcessGateResult` | name candidate          | mutation plan + 6 gate results                        | activation gate; not crossed in slice | privacy/accessibility/repo-safety/scope/no-mutation gates all `pass` (slice value) | future `ProcessGateResultReceipt` and `ActivationCrossedReceipt` (name candidates) on existing envelope          | not runtime; gate values asserted by slice                                     |
| 6    | `ActionCardTrigger`       | sketch                  | spine transitions                                     | n/a — attention routing only          | inherits ADR-0027 privacy posture                                                | sketches only; no ActionCard emitted                                                                             | ADR-0027 ActionCard contract is shipping; slice does not extend it             |
| 7    | `EvidencePacket`          | existing contract       | `docs/contracts/rehearsal-evidence-export.example.json` (committed) | export boundary; reviewer signs off  | `export_safety_classification: repo-safe`; no PII                                | committed example carries `proof_loop_references[].status: not-attempted` — correct for fixture-only walk        | shipping contract; composed against as-is                                      |

## Boundary check

- Generic ICN substrate is not modified to fit the slice.
- No new schema introduced.
- No NYCN-specific meaning leaks in. The fictional institution is
  generic ("Example Cooperative", "sample committee").
- No private data committed. Every named role, body, action item,
  and entry is fictional.
- No public website change.
- No runtime/code/SDK file edited.

## What this proves (against the framing brief's claims)

- **The preview/review contract can serve as the first review
  boundary.** Step 1 reuses the committed example exactly; the
  spine's review surface fits inside the contract without
  modification.
- **Deliberation can be object-bound instead of chat.** Step 2 walks
  three entries each bound to one `ProcessTargetRef` inside one
  `ProcessSession`, with closed-taxonomy entry kinds and generic
  role provenance. None of the entries are free-form messages.
- **Decisions can be recorded as human review outputs before
  mutation.** Step 3 records three outcomes with an explicit
  decision rule and explicit decision-vs-mutation separation. No
  mutation occurs.
- **Mutation planning can remain a sketch / read-model before any
  endpoint exists.** Step 4 writes the plan as plain language; the
  preview-review contract already carries the `pending_publish_summary`
  preview kind that a future runtime would render.
- **Action cards can route attention without expanding the ActionCard
  contract.** Step 6 sketches five legitimate emit-points, all under
  the existing `ADR-0027` contract; no new ActionCard kind is
  proposed.
- **Evidence export can summarize the process without private data.**
  Step 7 reuses the committed evidence export example exactly;
  `export_safety_classification: repo-safe` holds; the
  `proof_loop_references[].status: not-attempted` correctly records
  that this is a fixture walk, not a runtime exercise.
- **The spine can compose without touching kernel/runtime code.**
  Zero kernel, runtime, gateway, ledger, governance, or SDK files
  are edited by this slice. The composition is the proof.

## Promotion gate

This slice is the read-model dogfood that the framing brief's
"Proposed next artifact" called for. Promotion of `idea-0019` to RFC
candidate requires evidence beyond a fixture walk.

### What evidence would justify RFC promotion

1. **Runtime dogfood slice.** A second dogfood slice that runs
   against a real or fixture-equivalent gateway and emits at least
   one process-transition receipt under the existing `ADR-0026`
   envelope. Receipt class names from the framing brief
   (`ProcessSessionOpenedReceipt`, `DeliberationEntryRecordedReceipt`,
   `DecisionRecordedReceipt`, `MutationPlanRecordedReceipt`,
   `ActivationCrossedReceipt`, `ProcessGateResultReceipt`,
   `EvidencePacketProducedReceipt`) are name candidates only until
   then. Any one of them produced as a real receipt would be
   sufficient evidence; all of them are not required.
2. **Visibility/privacy boundary exercised.** A second slice that
   walks a `DeliberationEntry` visible to body A but not to body B,
   with redaction in the evidence export. Default-conservative
   visibility must be defended by an actual run, not by paper.
3. **Accessibility gate produces a real `ProcessGateResult`.** The
   slice asserts `pass` values; a real run produces them through
   the `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`
   checklist applied at PR time.
4. **One open question from the framing brief** (see below) is
   answered concretely enough to enumerate options, which is the
   threshold the RFC candidate registry uses.

### Which open questions from the framing brief remain

The 10 open questions in `ops/ideas/framing/institutional-process-substrate.md`
all remain open after this slice. The slice surfaces a few of them
more sharply, but answers none of them:

- Q1 (`ProcessTargetRef` polymorphism) — the slice uses
  `kind: meeting_artifact` and works without committing to a
  polymorphic vs typed-family decision.
- Q3 (`DeliberationEntry` kinds — closed taxonomy vs charter-
  extensible) — the slice uses the framing brief's proposed closed
  taxonomy and shows it sufficient for one session shape; insufficient
  evidence to settle the question across institution types.
- Q4 (`HumanDecisionSet` vs existing proposal/vote machinery) — the
  slice's decision rule is "committee consensus, no objection
  filed," which fits both shapes; insufficient evidence to settle.
- Q6 (`ProcessGateResult` minimal shape) — the slice's gate table
  uses six gate kinds; whether they share one shape or split into a
  small family remains unanswered.

### What would justify a future schema PR

A schema PR for any process-spine object would require:

- The runtime dogfood evidence above for that object class.
- A second, independent walk that exercises the schema against a
  different `ProcessTargetRef` kind.
- A `docs/contracts/schema-id-audit.md` row entry showing the audit
  table convention extended to the new schema.
- A non-DNS URN per the schema-id-audit's rule (`urn:icn:contract:
  <short-name>:v1`).

A schema PR before runtime evidence would re-create the "named-but-
empty objects" risk listed in `idea-0019` itself.

### What would justify runtime work later

Runtime work on the spine would be justified when:

- At least three institution types (e.g. cooperative, community
  assembly, federation chamber) have walked a paper slice like this
  one, and all three walks compose against the same spine without
  divergence.
- A runtime dogfood has produced at least one receipt class for at
  least one spine transition, under the existing `ADR-0026` envelope.
- The framing brief's open questions Q1, Q3, and Q4 have been
  resolved or explicitly deferred in writing.
- A runtime dogfood has demonstrated the visibility/privacy boundary
  in a real run — not a paper assertion.

Until all four of those conditions are met, the spine remains a
read-model surface that real institutions can author against without
any ICN code change.

## Acceptance criteria for this slice

- [x] Slice document is committed to `ops/ideas/dogfood/`.
- [x] Slice walks all spine **transition** objects from `idea-0019`
      (`ProcessSession`, `ProcessTargetRef`, `PreviewReviewPacket`,
      `DeliberationThread`, `DeliberationEntry`, `HumanDecisionSet`,
      `DecisionRecord`, `MutationPlan`, `ActivationRequest`,
      `ProcessGateResult`, `ActionCardTrigger`, `EvidencePacket`)
      end-to-end as a read-model.
- [x] Receipt classes named in `idea-0019` (the
      `ProcessTransitionReceipt` family —
      `ProcessSessionOpenedReceipt`,
      `DeliberationEntryRecordedReceipt`,
      `DecisionRecordedReceipt`, `ActivationCrossedReceipt`,
      `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`,
      `EvidencePacketProducedReceipt`, `ProcessGateResultReceipt`)
      are **referenced** at the right transition points but **not
      exercised** in this slice — emitting any of them is the next
      artifact (runtime dogfood). The slice does not claim runtime
      receipt evidence.
- [x] Slice composes against existing committed examples
      (`docs/contracts/preview-review.example.json`,
      `docs/contracts/rehearsal-evidence-export.example.json`) by
      pointing at the same shipping contract URNs and walking
      shapes that validate against the same schemas. The slice
      produces a three-outcome instance of
      `urn:icn:contract:rehearsal-evidence-export:v1` whose
      categories all come from that contract's closed
      `decision_outcomes[].category` enum
      (`approved` + `edit-and-resubmit` + `deferred`); the
      committed example file is a separate two-outcome instance of
      the same contract.
- [x] Slice introduces no new schema, no new contract URN, no
      runtime code change, no SDK change, no website change.
- [x] All material is fictional and repo-safe; no private data.
- [x] Vocabulary held to *obligation*, *allocation*, *settlement*,
      *unit*, *position*, *receipt*, *provenance*, *evidence*; no
      ICN-native *payment* / *currency* / *balance* / *wallet*
      framing.
- [x] Coordination issue [#1748](https://github.com/InterCooperative-Network/icn/issues/1748)
      and showcase milestone [#1746](https://github.com/InterCooperative-Network/icn/issues/1746)
      are linked.

## Coda

The slice does not build the Institutional Process Substrate. It
proves the named spine can be walked end-to-end as a repo-safe
fixture/read-model artifact without scattering preview/review,
deliberation, mutation planning, action cards, receipts, evidence,
accessibility gates, and private overlays into unrelated features.

> ICN can carry an institution's reasoning, consent, objection,
> assignment, execution plan, and evidence as scoped process —
> without centralizing authority, leaking private data, or
> pretending a runtime exists before it does.
