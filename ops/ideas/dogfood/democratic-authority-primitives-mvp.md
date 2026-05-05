# Democratic Authority Primitives — read-model composition slice

**Idea card:** `ops/ideas/ideas.yaml#idea-0020`
**Framing brief:** `ops/ideas/framing/democratic-authority-primitives.md`
**Composes against:** `ops/ideas/dogfood/institutional-process-substrate-mvp.md` (the `idea-0019` read-model fixture walk, landed in PR #1749)
**Coordination issue:** [#1748](https://github.com/InterCooperative-Network/icn/issues/1748) (process substrate milestone — DAP composes against `idea-0019`'s open gates without absorbing them)
**Owner / session:** 2026-05-05 session
**Date:** 2026-05-05
**Status:** read-model / fixture-walk dogfood. Not runtime. Not a schema. Not a decision. Not a pilot. Not production.

> **Slice discipline.** Read-model fixture-walk variant per
> `ops/ideas/README.md` § "Dogfood slice variants" (formalized in
> PR #1749). Uses fictional, repo-safe material exclusively.
> Composes against already-committed contract examples
> (`urn:icn:contract:preview-review:v1`,
> `urn:icn:contract:rehearsal-evidence-export:v1`) and shipping
> ADRs (`ADR-0026` proof envelope, `ADR-0027` action-card contract,
> `ADR-0028` accessibility baseline, `ADR-0029` conflict resolution
> object model — `proposed/partial`) without modification. Emits no
> receipts, contacts no gateway, performs no mutation, writes
> nothing outside `ops/ideas/`. **Does NOT satisfy receipt-backed
> promotion thresholds** — those still require a runtime slice per
> the DAP framing brief's §16.1 strict RFC promotion gate (deferral
> not sufficient). A future runtime dogfood is the next artifact
> after this one.

## What this slice proves

The Authority + Context primitive families named in `idea-0020`
compose **orthogonally** with the Institutional Process Substrate
spine named in `idea-0019` — every spine record can carry the typed
authority and context fields named in DAP without modifying the
spine, the kernel, the gateway, the ledger, the governance crates,
the SDKs, or any shipping contract.

Specifically: every `DeliberationEntry` from `idea-0019` walks with
an `AuthorityBasis` and a `ParticipationRole` attached without
changing the entry's existing kind taxonomy (`question`,
`accessibility_review`, `amendment`, etc.). The `HumanDecisionSet` /
`DecisionRecord` carries an `AuthorityBasis` for the deciding body
and `ParticipationRole` per decider. The `accessibility_review`
entry from `idea-0019` is typed as a `FacilitatorSummary` with
explicit non-decisional posture. Dissent that did not reach an
`objection` entry kind (consensus held) is preserved as a typed
`MinorityReport` attached to the `DecisionRecord`. A typed
`ConflictDisclosure` attaches to the `amendment` entry that
proposed narrowing scope. A small `DeliberationContext` of three
references (charter rule, prior decision, accessibility note) is
attached to the `DeliberationThread`. The `ActivationRequest`
carries an `OperatorExecutionAuthority` reference distinguishing
democratic authorization from execution authority.

The composition is the proof. If the primitives could not attach
cleanly to the spine's existing record shapes, the framing brief's
"orthogonality" claim would be incoherent. They can. The slice
records the walk so future readers can replay it.

## What this slice does NOT prove

- **Not runtime.** No record is stored; no receipt emitted; no
  policy oracle evaluates; no `ConstraintSet` returns; no API is
  called.
- **Not a schema.** No new JSON Schema, no new TOML row, no new
  contract identifier.
- **Not promotion to RFC.** Per DAP §16.1, RFC promotion still
  requires (a) a runtime dogfood that emits at least one receipt
  under `ADR-0026` for one of the named primitives (preferably
  `ConflictDisclosure` accept receipt or `MinorityReport` recorded
  receipt), (b) a real visibility/privacy-boundary run with
  redaction in evidence export, (c) an accessibility-gate
  `ProcessGateResult` produced through
  `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real
  surface, and (d) Q1 (`AuthorityBasis` polymorphism vs typed
  family) or Q5 (`ConflictDisclosure` and `MinorityReport`
  placement) **resolved** in writing. Deferral is not sufficient
  for the RFC gate per §16.1; the lenient resolved-or-deferred
  standard at §16.3 applies only to the broader runtime-
  justification threshold.
- **Not a full primitive walk.** `DelegationGrant`,
  `RepresentationMandate`, `ExpertStatement`, `AdvisoryOpinion`,
  `StewardReview`, `ChallengePath`, `RevocationPath`, and
  `RecallPath` are referenced as legitimate attachment points but
  are not exercised — they require different fictional scenarios
  (a body delegating activation, a representative acting under
  mandate, an expert supplying an advisory claim, a steward
  performing procedural review, a member challenging a decision
  via a typed contest path) and are deferred to subsequent slices.
- **Not a CCL syntax decision.** CCL hooks are named where they
  would attach (charter rules govern when an `AuthorityBasis` of a
  given kind is acceptable, when a `MinorityReport` is required,
  when a `ConflictDisclosure` must be public vs body-scoped); CCL
  syntax is not specified.
- **Not a conflict-resolution implementation.** `ADR-0029`
  (`proposed/partial`) is referenced as the path that would handle
  disputes about a `DecisionRecord`. The slice exercises
  `ConflictDisclosure` (upstream of disputes — disclosure is the
  price of speaking) but does not exercise the conflict-resolution
  flow (`EffectChallenge`, evidence model, remedy taxonomy,
  mediation roles) that `idea-0016` and `ADR-0029` own.
- **Not a federation tally semantics commitment.** The slice's
  scenario is `structure`-scoped (a single committee inside one
  cooperative), not federation-scoped. `RepresentationMandate`'s
  federation-tier composition with `#1609` is explicitly deferred.
- **Not a `DelegationGrant` / `TemporaryAuthorityGrant` shape
  decision.** The slice records `AuthorityBasis` of kind
  `role_grade` (standing-grade) and `process_authority`
  (facilitator, non-decisional). The brief's Q2 question (whether
  `DelegationGrant` and `TemporaryAuthorityGrant` are one type or
  two) is not answered.
- **Not a binding on partner repositories or partner data.** All
  material is fictional, repo-safe, and committed under
  `ops/ideas/`. No NYCN-specific nouns. No real partner data.

## Fictional scenario

The slice **extends the Example Cooperative scenario** from
`ops/ideas/dogfood/institutional-process-substrate-mvp.md` (the
`idea-0019` read-model fixture walk, PR #1749). Same fictional
institution (Example Cooperative, generic). Same fictional body
running the process (sample committee, a `structure`-scoped body —
`scope: structure` per `urn:icn:contract:preview-review:v1`'s
`scope` enum). Same source material (the already-committed
fictional examples at `docs/contracts/preview-review.example.json`
and `docs/contracts/rehearsal-evidence-export.example.json`). Same
`ProcessTargetRef` (`kind: meeting_artifact`, `local_label = "sample
committee meeting notes"`). Same three decision outcomes (`approved`
/ `edit-and-resubmit` / `deferred` from the closed
`decision_outcomes[].category` enum at
`urn:icn:contract:rehearsal-evidence-export:v1`).

This slice **does not invent a new scenario**. It extends the
existing walk by attaching `AuthorityBasis`, `ParticipationRole`,
and the small DAP context/authority primitives to each step.

No real names. No real organization. No private organizer / member
/ sponsor / attendee data. No real Drive / Groups / Sheets paths.
No NYCN-specific nouns.

## DeliberationContext (attached to the thread before the walk begins)

The `DeliberationContext` for this `ProcessSession` attaches three
typed references — read-model only, no schema introduced:

| # | Reference primitive | Plain-language content (fictional) |
|---|---------------------|------------------------------------|
| 1 | `CharterRuleReference` | Pinned reference to "sample committee charter §3.2: amendments to a proposed action item are recorded by the proposer and may be debated for one cycle before the decision moment." Pinned by URN + repo path + content hash, not by hosted URL — per `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` §3.A authority/convenience rule. |
| 2 | `PriorDecisionReference` | Pinned reference to a fictional prior `DecisionRecord` from a previous Example Cooperative cycle in which a similar action item was scoped down to a single deliverable. The precedent the `amendment` entry in deliberation entry #3 invokes (see Step 2 below). Repo-safe: a sibling fictional cycle, no PII, no partner identity. |
| 3 | `AccessibilityNote` | Pinned reference to `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` plus a plain-language summary of what an accessible rendering of receipt-export status would look like (status text + status icon, not color alone) — the same accessibility concern that surfaces as deliberation entry #2. |

`LearningReference`, `EvidenceReference`, `CCLRuleReference`,
`PrivacyNote`, `RiskNote`, `CounterargumentReference`, and
`GlossaryReference` are **not** exercised in this slice but are
reserved as legitimate context-family attachment points. The
substrate must support all of them, but a single slice need not
exercise all of them.

## Slice steps (extending the idea-0019 walk)

The walk reads each existing or proposed primitive as a read-model.
"Existing contract" / "ADR-shipping" / "name candidate" / "sketch"
follow the same conventions as
`ops/ideas/dogfood/institutional-process-substrate-mvp.md`. The
numbering matches that slice's Step 0 through Step 7 so a future
reader can read the two slices side-by-side. **Spine objects are
not re-described**; only the DAP primitive additions are recorded
here.

### Step 0. Open `ProcessSession` and bind `ProcessTargetRef`

Spine: see idea-0019 dogfood Step 0 (unchanged).

DAP additions:

- **`AuthorityBasis`** (session opener). The facilitator opening
  the session is acting under standing-grade authority via a
  `RoleAssignment` of role `facilitator` granted by the sample
  committee charter. AuthorityBasis kind: **`role_grade`**
  (standing-grade authority — the existing
  `RoleAssignment.authority_scope` surface). Distinct from
  moment-grade temporary grants (DAP brief Q2: whether
  `DelegationGrant` and `TemporaryAuthorityGrant` are one type or
  two — this slice does not commit; both possibilities are
  consistent with `role_grade` as an `AuthorityBasis` kind).
- **`ParticipationRole`** (session opener). `facilitator` —
  per-record role. Distinct from `RoleAssignment`-grade standing.

### Step 1. Render `PreviewReviewPacket`

Spine: existing contract `urn:icn:contract:preview-review:v1`,
rendered as-is. (See idea-0019 dogfood Step 1.)

DAP additions:

- **`ParticipationRole`**: `reviewer`. The reviewer reading the
  packet acts under `AuthorityBasis: role_grade` (the committee's
  standing reviewer role, granted by charter).
- No `FacilitatorSummary`, `ConflictDisclosure`, `MinorityReport`,
  or other DAP primitives composed at this step — nothing is
  recorded yet.

### Step 2. Walk a `DeliberationThread` with three entries

Spine: three `DeliberationEntry` of kinds `question`,
`accessibility_review`, `amendment`. (See idea-0019 dogfood Step 2.)

DAP additions per entry:

| # | Entry kind (`idea-0019`) | `ParticipationRole` | `AuthorityBasis` (kind) | DAP primitive composed |
|---|--------------------------|---------------------|-------------------------|------------------------|
| 1 | `question` | `reviewer` | `role_grade` (committee reviewer role per charter) | (none — typed entry only; primitive walk continues) |
| 2 | `accessibility_review` | `facilitator` | `role_grade` + **`process_authority`** (facilitator process authority is typed but explicitly *non-decisional* per DAP brief §4.1 `FacilitatorSummary` row and §11 doctrinal rule) | **`FacilitatorSummary`** primitive composed: this entry IS a typed `FacilitatorSummary`. The substrate carries explicit non-decisional posture so a future reader cannot interpret the facilitator's process authority as outcome authority. |
| 3 | `amendment` | `organizer` | `role_grade` (committee organizer role) + **`member_voice`** (the proposing organizer is also a member of the sample obligation working group affected by the amendment — `member_voice` is the AuthorityBasis kind for member-as-self speech, distinct from delegated authority per DAP brief §6 doctrinal rule "Member voice is not delegated authority") | **`ConflictDisclosure`** primitive composed: see below. |

The `ConflictDisclosure` attached to entry #3 is a typed pairing
(not a stand-alone log) carrying:

- **Actor**: the organizer authoring the amendment entry.
- **Nature of conflict**: `professional_affiliation` (the organizer
  is a member of a working group whose scope is affected by the
  proposed amendment). One of the closed `nature` taxonomy
  candidates from DAP §10.1 (`financial`, `familial`,
  `professional`, `prior_relationship`, `jurisdictional`,
  `identity_based`, `ideological_but_declared`, `other`).
- **Affected target**: proposed action item 1 (sample agenda).
- **Proposed mitigation**: `declared_position` (the organizer
  declares the affiliation on the record but does not recuse — the
  affiliation is generally known and the affected scope is small).
- **Body accepting**: sample committee.
- **Acceptance receipt** (sketch): a future
  `ConflictDisclosureAcceptedReceipt` would attach to the existing
  `ADR-0026` envelope. **No receipt is emitted in this slice.**

The disclosure is the price of speaking. The substrate records both
the disclosure and (read-model only) the committee's accept; per
DAP brief §10.3 doctrinal rule "no retroactive erasure", this
record cannot later be deleted — disclosures that turn out to have
been insufficient are addended, not erased.

### Step 3. Record `HumanDecisionSet` / `DecisionRecord`

Spine: three outcomes (`approved` / `edit-and-resubmit` / `deferred`)
under committee consensus rule. (See idea-0019 dogfood Step 3.)

DAP additions:

- **`AuthorityBasis`** (deciding body). The sample committee acts
  under `AuthorityBasis` kind `role_grade` granted by Example
  Cooperative's charter for `structure`-scoped meeting decisions.
  The `HumanDecisionSet` carries this authority basis as the
  deciding body's typed handle. Per DAP brief §4.1, every
  `DecisionRecord` carries the `AuthorityBasis` of the deciding
  body and `ParticipationRole` per decider.
- **`ParticipationRole`** (per decider). Each member voting at the
  decision moment carries `ParticipationRole: member`. Roles are
  per-record; the same person voting in two different sessions may
  carry different roles (e.g., `member` in this session,
  `facilitator` in another).
- **`MinorityReport`** primitive composed. The decision moment for
  action item 1 (`edit-and-resubmit`) was reached by committee
  consensus after the `amendment` entry, but **one member
  dissented**: they preferred to keep the schedule build inside the
  sample agenda action item rather than narrow scope to the agenda
  alone. The dissent did **not** reach a `DeliberationEntry: objection`
  (consensus rule was not blocked), but the substrate records the
  dissent as a typed `MinorityReport` attached to the
  `DecisionRecord` for action item 1.

  The `MinorityReport` carries:
  - dissenting decider's `ParticipationRole`: `member`.
  - dissenting decider's `AuthorityBasis`: `role_grade` +
    `member_voice`.
  - dissenting view in plain language: "preserve the schedule
    build inside this cycle".
  - rationale (plain language): "splitting the work risks losing
    momentum on schedule readiness".
  - target: `DecisionRecord` for action item 1 (`edit-and-resubmit`).

  The `MinorityReport` is **not** an objection veto — consensus
  held — but the dissent survives the decision moment on the
  institutional record. Per DAP brief §4.1 `MinorityReport` row:
  "A record that captures only what won is a record that erases
  what disagreed."

  No `MinorityReport` is recorded for the `approved` outcome (no
  dissent) or the `deferred` outcome (the deferral itself is a
  consensus disposition).

### Step 4. Sketch `MutationPlan`

Spine: `pending_publish_summary` shape sketch. (See idea-0019
dogfood Step 4.)

DAP additions:

- The plan target carries no new DAP primitive at this step. The
  plan describes operations; operator execution authority enters
  at Step 5 (the activation gate), where the plan is bound to a
  decided-upon authorization. Per DAP brief §4.1
  `OperatorExecutionAuthority` row: "downstream of decision."

### Step 5. Cross `ActivationRequest` gate

Spine: 6-gate `ProcessGateResult` table; `ActivationRequest`
sketched, not issued. (See idea-0019 dogfood Step 5.)

DAP additions:

- **`OperatorExecutionAuthority`** primitive referenced (sketch
  only). The `ActivationRequest`, if issued, carries an
  `OperatorExecutionAuthority` reference distinguishing the
  *democratic authorization* (the sample committee's
  `DecisionRecord` from Step 3) from the *execution authority* (the
  steward who would actually execute the `MutationPlan`). The
  reference is typed: it points at
  - (a) the `DecisionRecord` that authorizes the action,
  - (b) the `ProcessGateResult` set that confirms gate-clearance,
  - (c) the steward's `RoleAssignment` carrying `authority_scope`
    for `structure`-scoped action-item creation.

  Operator execution authority is **strictly downstream of
  decision** — never appears without (a) and (b). Per DAP brief
  §4.1 doctrinal rule, conflating decision with execution is "a
  primary failure mode of dashboards as governance".
- All other DAP authority primitives (`DelegationGrant`,
  `RepresentationMandate`, `ExpertStatement`, `AdvisoryOpinion`,
  `StewardReview`) are **not** exercised at this step — they
  require different scenarios (a body delegating activation
  authority to one of its members for a specific cycle, a
  representative acting under mandate, an expert supplying a claim
  that informs the gate, a steward performing procedural review of
  the activation packet) and are deferred to subsequent slices.

### Step 6. Sketch `ActionCardTrigger`s

Spine: five legitimate emit-points sketched. (See idea-0019
dogfood Step 6.)

DAP additions — two new spine-transition emit-points surfaced by
DAP primitives:

- `ConflictDisclosure: filed` ↦ action card to the body designated
  to accept the disclosure (here: sample committee). Sketch only.
- `MinorityReport: recorded` ↦ action card to the dissenting member
  confirming their dissent is on the record. Sketch only.

Both are under the **existing** `ADR-0027` ActionCard contract —
no new ActionCard kind is proposed. Both are read-model only; no
card is emitted in this slice.

### Step 7. Produce `EvidencePacket`

Spine: existing contract `urn:icn:contract:rehearsal-evidence-export:v1`.
(See idea-0019 dogfood Step 7.)

DAP additions — how the new primitives compose into the evidence
packet (read-model only, no packet actually produced):

- **`MinorityReport` content** for the `edit-and-resubmit`
  decision is summarized in the packet as a single typed entry
  (dissenting view + rationale, no decider identity).
  Default-conservative redaction: the dissenter's
  `ParticipationRole` and `AuthorityBasis` kind are recorded; their
  identity (a real member name in any non-fictional run) is
  redacted. The slice's all-fictional posture means there's no
  identity to redact, but the redaction shape is what a real run
  would produce.
- **`ConflictDisclosure` content** is **default-conservative
  redacted**: the disclosure exists on the record (committee can
  replay it from receipts), but the evidence packet summarizes it
  as "one conflict disclosure filed; mitigation accepted by
  deciding body" without echoing the affiliation detail unless
  charter explicitly requires public disclosure. Per DAP brief
  §10.3 doctrinal rule "no retroactive erasure": redaction in
  evidence export is **not** erasure of the disclosure record —
  the source-of-truth record is preserved; only the export view is
  reduced.
- **`FacilitatorSummary` content** is **summarized**, not echoed
  verbatim: the packet records that a facilitator summary was
  produced and links to its receipt class candidate
  (`DeliberationEntryRecordedReceipt` from `idea-0019`) — sketch
  only, no receipt emitted.
- **`DeliberationContext` references** are summarized: charter
  rule cited (with URN), prior decision cited (with URN), and
  accessibility note cited. The packet's `audience_categories`
  field would consult charter to determine whether the
  prior-decision reference is repo-safe to cross-link; this slice
  asserts it is (the prior cycle is a sibling of this fictional
  cycle, both in the Example Cooperative; no PII).

The packet remains a different valid instance of
`urn:icn:contract:rehearsal-evidence-export:v1`, not a
byte-for-byte reuse of any committed example file. Reused shape
elements (`rehearsal_mode: fixture-only`,
`preview_review_boundary.enforced: true`,
`mutation_boundary.executed: false`,
`export_safety_classification: repo-safe`,
`proof_loop_references[].status: not-attempted`) hold for the
packet a real run from this slice would produce.

## Trace table (one row per step, extended for DAP)

The columns extend `idea-0019`'s trace table with three DAP-specific
columns: `AuthorityBasis kind`, `ParticipationRole`, and
`DAP primitive composed at this step`.

| Step | Spine object | Implementation status | `AuthorityBasis` kind | `ParticipationRole` | DAP primitive composed | Receipt / evidence relationship (DAP-specific) |
|------|--------------|-----------------------|-----------------------|---------------------|------------------------|-----------------------------------------------|
| 0 | `ProcessSession` / `ProcessTargetRef` | name candidate (idea-0019) | `role_grade` (facilitator's standing role per charter) | `facilitator` | (none — composition begins at Step 1) | future `ProcessSessionOpenedReceipt` (idea-0019 candidate) on `ADR-0026` envelope; no DAP-specific receipt at this step |
| 1 | `PreviewReviewPacket` | existing contract `urn:icn:contract:preview-review:v1` | `role_grade` (reviewer role per charter) | `reviewer` | (none — review boundary, no DAP record yet) | none — this step is review-boundary only |
| 2 | `DeliberationThread` / `DeliberationEntry` | name candidates (idea-0019); closed-taxonomy proposed | per entry: `role_grade`; `role_grade` + `process_authority`; `role_grade` + `member_voice` | per entry: `reviewer`; `facilitator`; `organizer` | `FacilitatorSummary` (entry 2); `ConflictDisclosure` (paired to entry 3) | future `FacilitatorSummaryRecordedReceipt`, `ConflictDisclosureAcceptedReceipt` (DAP candidates), `DeliberationEntryRecordedReceipt` (idea-0019 candidate) — all on existing `ADR-0026` envelope; not exercised in this slice |
| 3 | `HumanDecisionSet` / `DecisionRecord` | name candidates (idea-0019) | deciding body: `role_grade` (sample committee per charter); per decider: `role_grade` + `member_voice` | per decider: `member` | `MinorityReport` (attached to `DecisionRecord` for action item 1) | future `MinorityReportRecordedReceipt` (DAP candidate), `DecisionRecordedReceipt` (idea-0019 candidate) on existing envelope; not exercised |
| 4 | `MutationPlan` | name candidate (idea-0019, sketch) | n/a (plan target carries no new authority basis) | n/a | (none — authority enters at Step 5) | future `MutationPlanRecordedReceipt` (idea-0019 candidate); not exercised |
| 5 | `ActivationRequest` / `ProcessGateResult` | name candidates (idea-0019) | `OperatorExecutionAuthority` reference: points at decision (a) + gate-clearance (b) + steward's `RoleAssignment` (c) | n/a (operator role is referenced, not exercised) | `OperatorExecutionAuthority` (sketch — references decision and gates) | future `ActivationCrossedReceipt`, `ProcessGateResultReceipt` (idea-0019 candidates) on existing envelope; not exercised |
| 6 | `ActionCardTrigger` | sketch (idea-0019) | inherits ADR-0027 ActionCard contract authority posture | inherits ADR-0027 | two new emit-points surfaced: `ConflictDisclosure: filed` and `MinorityReport: recorded` (sketches) | sketches only; ActionCards not emitted; no new ActionCard kind |
| 7 | `EvidencePacket` | existing contract `urn:icn:contract:rehearsal-evidence-export:v1` | preserved on record; redacted in export per charter and `PrivacyNote` defaults | preserved on record; redacted in export per defaults | DAP primitive content composed into the packet (summarized, not echoed verbatim); per DAP §10.3 redaction is not erasure | committed example carries `proof_loop_references[].status: not-attempted` — correct for fixture-only walk |

## Boundary check

- **Generic ICN substrate is not modified to fit the slice.** No
  kernel, runtime, gateway, ledger, governance, SDK, or website
  file is edited.
- **No new schema introduced.** No new JSON Schema, no new TOML
  row, no new `urn:icn:contract:*`. Composes against
  already-committed contracts.
- **No NYCN-specific meaning leaks in.** The fictional institution
  is generic ("Example Cooperative", "sample committee",
  "sample obligation working group", "sample agenda working
  group"). No partner identity. No real partner data.
- **No private data committed.** Every named role, body, action
  item, deliberation entry, conflict disclosure, minority report,
  and context reference is fictional. No PII. No real Drive /
  Groups / Sheets paths.
- **No public website change.** No `website/`, `docs/STATE.md`,
  `docs/PHASE_PROGRESS.md`, or registry edit.
- **No runtime/code/SDK file edited.** No `.rs`, `.ts`, `.py`,
  `.yaml`-schema, or contract-schema file touched.
- **Vocabulary discipline preserved.** Uses *obligation*,
  *allocation*, *settlement*, *unit*, *position*, *receipt*,
  *provenance*, *evidence*. Avoids ICN-native *payment*,
  *currency*, *balance*, *wallet* framing.
- **Hard rules preserved per DAP framing brief §14**: not runtime,
  not a schema, not an RFC by itself, not a voting-system
  decision, not a liquid-democracy commitment, not expertocracy,
  not anti-expertise, not chat, not social media, not a moderation
  platform, not an identity directory implementation, not a
  credential verification implementation, not a private-overlay
  implementation, not NYCN-specific, not a production-readiness
  claim, not a Phase 2 completion claim, not a formal NYCN pilot
  authorization, not a live federation claim, not a live cloud
  sync claim, not a K3s/DNS/Forgejo mutation claim, not a
  private-data-handling claim, not a binding on partner
  repositories.

## What this proves (against the framing brief's claims)

- **Authority basis can attach to every spine record without
  modifying the spine.** Steps 0, 1, 2, 3, and 5 each carry an
  `AuthorityBasis` kind drawn from a small candidate set
  (`role_grade`, `process_authority`, `member_voice`,
  `OperatorExecutionAuthority` reference). The spine's
  `DeliberationEntry`, `HumanDecisionSet`, and `ActivationRequest`
  shapes accept the typed authority field without requiring a
  schema change.
- **Participation role is per-record, not per-person.** Step 0's
  facilitator, Step 1's reviewer, Step 2's three different
  authors, Step 3's deciders, and Step 5's referenced steward each
  carry their own `ParticipationRole` value scoped to the record
  they author or affect. The same person could carry different
  roles in different sessions; the substrate need not pin a person
  to a single role.
- **Facilitator process authority is typed and explicitly
  non-decisional.** The `accessibility_review` entry in Step 2 is
  a typed `FacilitatorSummary` carrying `process_authority` as an
  `AuthorityBasis` kind alongside `role_grade`. The substrate
  encodes that the facilitator's authority over the conversation
  is not authority over the outcome.
- **Conflict disclosure is paired, accountable, and non-erasable.**
  The `ConflictDisclosure` paired to entry 3 in Step 2 carries
  actor, nature, target, mitigation, accepting body, and
  (sketch-only) acceptance receipt. Per DAP §10.3 "no retroactive
  erasure", redaction in evidence export at Step 7 reduces the
  view but does not delete the source-of-truth record.
- **Dissent survives consensus.** Step 3's `MinorityReport`
  preserves a member's dissenting view against the
  `edit-and-resubmit` outcome even though consensus held. A record
  that captures only what won is a record that erases what
  disagreed; this slice shows the substrate does not erase.
- **Operator execution authority is strictly downstream of
  decision.** Step 5's `OperatorExecutionAuthority` reference is
  typed to point at the `DecisionRecord` and the gate-clearance
  set; it cannot appear without them. The substrate refuses the
  failure mode where execution substitutes for democratic
  authorization.
- **Deliberation context attaches to the thread, not to a free
  resource pile.** The `DeliberationContext` of three references
  (`CharterRuleReference`, `PriorDecisionReference`,
  `AccessibilityNote`) attaches to *this* `DeliberationThread`,
  not to a global library. Members read the context before forming
  their position; the context is part of the institutional record.
- **The composition fits inside `ADR-0027`'s existing ActionCard
  contract.** Step 6 surfaces two new spine-transition emit-points
  (`ConflictDisclosure: filed`, `MinorityReport: recorded`) under
  the existing contract — no new ActionCard kind is proposed.
- **Evidence export defaults conservative.** Step 7's redaction
  posture preserves identity and affiliation detail on the source
  record while reducing the export view to body-scoped summaries
  unless charter explicitly requires public disclosure.
- **The composition does not modify any kernel/runtime/contract
  file.** Zero kernel, runtime, gateway, ledger, governance, SDK,
  or contract files are edited by this slice. The composition is
  the proof.

## Promotion gate

This slice is the read-model dogfood that the DAP framing brief's
§17 "Follow-ups" called for as the immediate next artifact after
#1751.

**Per `ops/ideas/README.md` § "Dogfood slice variants" and per the
DAP framing brief's §16.1, a read-model fixture walk does NOT
satisfy receipt-backed promotion thresholds.** Promotion of
`idea-0020` to RFC candidate requires evidence beyond a fixture
walk:

### What evidence would justify RFC promotion (per DAP §16.1)

1. **Runtime dogfood slice.** A second dogfood slice that runs
   against a real or fixture-equivalent gateway and emits at least
   one receipt under the existing `ADR-0026` envelope for one of
   the named primitives — preferably a
   `ConflictDisclosureAcceptedReceipt` or a
   `MinorityReportRecordedReceipt`, since both compose with
   `idea-0019` shapes already named. Receipt class names are
   candidates only until then. Producing any one as a real receipt
   would be sufficient evidence; all are not required.
2. **Visibility/privacy-boundary exercised.** A run that walks a
   `ConflictDisclosure` or a `DeliberationContext` reference
   visible to body A but not to body B, with redaction in evidence
   export. Default-conservative visibility must be defended by an
   actual run, not by paper.
3. **Accessibility-gate `ProcessGateResult` produced** through
   `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real
   surface that renders any of these primitives (e.g. a
   `DeliberationContext` viewer, a `MinorityReport` reader, a
   `ConflictDisclosure` renderer).
4. **One open question concretely answered (resolved, not
   deferred).** Per DAP §16.1, the strict standard for the RFC
   gate is *resolution* of Q1 (`AuthorityBasis` polymorphism vs
   typed family) or Q5 (`ConflictDisclosure` and `MinorityReport`
   placement — `DeliberationEntry` kind vs distinct-primitive-that-
   attaches). Deferral is not sufficient for RFC promotion; the
   lenient resolved-or-deferred standard at §16.3 applies only to
   the broader runtime-justification threshold.

### What this slice surfaces about the open questions (read-model evidence only — does not resolve them)

- **Q1 (`AuthorityBasis` polymorphism vs typed family):** the
  slice uses a small typed family of kinds (`role_grade`,
  `process_authority`, `member_voice`,
  `OperatorExecutionAuthority` as a reference type rather than a
  kind). The polymorphic option would let any record carry one
  generic `AuthorityBasis` field; the typed-family option enforces
  per-kind validation. The slice composes successfully under the
  typed-family reading; insufficient evidence to settle whether
  polymorphic would compose equally well.
- **Q5 (`ConflictDisclosure` and `MinorityReport` placement):**
  the slice treats both as **distinct primitives that attach to a
  `DeliberationEntry` or `DecisionRecord`** rather than as
  `DeliberationEntry` kinds extending `idea-0019`'s closed
  taxonomy. This composition works; insufficient evidence to
  settle whether the kind-extension reading would also work.

The other open questions in the DAP brief (Q2 through Q4, Q6
through Q10) are not surfaced by this slice and remain open.

### What would justify a future schema PR

A schema PR for any DAP primitive would require, in addition to
the runtime evidence above:

- A second, independent walk that exercises the schema across at
  least two unrelated institution types (cooperative, community
  assembly, federation chamber, mutual-aid network, land trust,
  association).
- A `docs/contracts/schema-id-audit.md` row entry showing the
  audit table convention extended to the new schema.
- A non-DNS URN per the schema-id-audit's rule
  (`urn:icn:contract:<short-name>:v1`).

A schema PR before runtime evidence would re-create the
"named-but-empty objects" risk from `idea-0010`.

### What would justify runtime work later

Runtime work on the DAP primitives would be justified when:

- At least three institution types (cooperative, community
  assembly, federation chamber) have walked a paper slice that
  exercises one primitive cluster (authority cluster, context
  cluster, or both), and all three compose against the same shape
  without divergence.
- A runtime dogfood has produced at least one receipt class for
  at least one DAP primitive under the existing `ADR-0026`
  envelope.
- The DAP brief's open questions Q1, Q5, and Q7 have been
  resolved or explicitly deferred in writing (the §16.3 lenient
  standard).
- A runtime dogfood has demonstrated the visibility/privacy
  boundary on a real surface — not a paper assertion.

Until those conditions are met, the DAP primitives remain a paper
surface that real institutions can author against in CCL,
charters, and packages without any ICN code change.

## Acceptance criteria for this slice

- [x] Slice document is committed to `ops/ideas/dogfood/`.
- [x] Slice composes the six DAP primitive families named in the
      framing brief's §17 follow-up (`AuthorityBasis`,
      `ParticipationRole`, `FacilitatorSummary`,
      `ConflictDisclosure`, `MinorityReport`,
      `DeliberationContext` — the latter exercising three of its
      twelve reference families: `CharterRuleReference`,
      `PriorDecisionReference`, `AccessibilityNote`) end-to-end
      against the merged `idea-0019` read-model fixture walk.
- [x] Receipt classes named in `idea-0020`'s framing brief and in
      this slice (`FacilitatorSummaryRecordedReceipt`,
      `ConflictDisclosureAcceptedReceipt`,
      `MinorityReportRecordedReceipt`) are **referenced** at the
      right transition points but **not exercised** — emitting
      any of them is the next artifact (runtime dogfood). The
      slice does not claim runtime receipt evidence.
- [x] Slice composes against existing committed examples
      (`docs/contracts/preview-review.example.json`,
      `docs/contracts/rehearsal-evidence-export.example.json`)
      via the `idea-0019` walk; it does not modify them and does
      not introduce a new contract URN.
- [x] Slice introduces no new schema, no new contract URN, no
      runtime code change, no SDK change, no website change.
- [x] All material is fictional and repo-safe; no private data; no
      NYCN-specific nouns.
- [x] Vocabulary held to *obligation*, *allocation*, *settlement*,
      *unit*, *position*, *receipt*, *provenance*, *evidence*; no
      ICN-native *payment* / *currency* / *balance* / *wallet*
      framing.
- [x] Coordination issue [#1748](https://github.com/InterCooperative-Network/icn/issues/1748)
      and showcase milestone [#1746](https://github.com/InterCooperative-Network/icn/issues/1746)
      are linked.
- [x] Promotion gate restated per DAP §16.1 strict standard:
      deferral is not sufficient for RFC promotion; the
      lenient §16.3 standard applies only to the broader
      runtime-justification threshold.

## Coda

The slice does not build the Democratic Authority Primitives layer.
It proves the named primitive families can attach to the spine's
existing record shapes without modifying the spine, the kernel,
the gateway, the ledger, the governance crates, the SDKs, or any
shipping contract.

> Authority basis attaches; participation role attaches; facilitator
> process authority attaches and is explicitly non-decisional;
> conflict disclosure attaches and is non-erasable; minority report
> attaches and dissent survives consensus; operator execution
> authority is typed strictly downstream of decision; deliberation
> context attaches to the thread, not to a free resource pile —
> all without runtime, all without schema, all under the existing
> contract URNs and ADR envelopes, all on already-shipping spine
> shapes.

Authority must always carry its basis. The substrate now has a
read-model surface where it does.
