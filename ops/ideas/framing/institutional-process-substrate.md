# Institutional Process Substrate — framing brief

**Idea card:** `ops/ideas/ideas.yaml#idea-0019`
**Author / session:** 2026-05-05 session
**Date:** 2026-05-05
**Status:** pre-RFC framing. Not a design doc. Not a decision. Not a
schema commitment. Not a runtime claim.

> **Seed-brief discipline.** This brief names a spine that is currently
> scattered across already-merged or in-flight contracts, ADRs, and
> design docs. It does not invent a new domain. If future passes add
> per-object schemas, runtime patterns, or capability maps, those split
> into separate framing or RFC artifacts rather than letting this brief
> become a design doc.

## What this is

Several institutional-process pieces are already merged or in flight in
ICN: the action-card contract (`ADR-0027`), the receipt and provenance
proof envelope (`ADR-0026`), the preview/review read-model contract
(`urn:icn:contract:preview-review:v1`, PR #1745), the rehearsal evidence
export contract (`urn:icn:contract:rehearsal-evidence-export:v1`), the
no-CLI organizer/member rehearsal workflow
(`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`), the
organizer/member accessibility gate
(`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`), and the
architecture due-diligence reflex
(`docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md`).

Each is well-bounded individually. None, alone or together, names the
**generic process spine** that ties them: preview → deliberation →
decision → activation → mutation plan → action cards → receipts →
evidence.

This brief names that spine. The spine is what apps write against and
what packages bind to; it is not what the kernel reads.

## Why this matters

ICN can already record decision *corpses*: a proposal was raised, a
vote was taken, an action item was assigned, a receipt was emitted. It
cannot yet record the **process** by which those decisions were
reached: the questions raised, the concerns and objections recorded,
the amendments offered, the blockers named, the facilitator summaries
written, the privacy and accessibility reviews performed, the human
decision moment captured, the activation crossed, the mutation planned,
the receipt produced, the evidence exported.

If deliberation remains implicit — captured in chat threads, private
meeting rooms, or partner spreadsheets that ICN cannot prove — then
ICN's claim to be "infrastructure through which democratic institutions
can govern and coordinate" is hollow at exactly the layer that matters
most. A system that only records outcomes is a system that imports
whatever politics produced those outcomes, opaque, behind the scenes.

Naming the spine first prevents the alternative: each feature
(preview/review, deliberation, mutation planning, action cards,
receipts, evidence, accessibility gates, private overlays) inventing
its own shape, in its own corner, with its own assumptions, until the
shapes do not compose. A coordination milestone (issue #1746) is
already in flight that depends on these pieces composing.

## Core thesis

> ICN can carry an institution's reasoning, consent, objection,
> assignment, execution, and evidence without centralizing authority or
> erasing human process.

- The **kernel** stays meaning-blind: it enforces constraints, it does
  not understand institutional process.
- The **ICN platform/app layer** owns the generic process objects.
- **CCL/charters** govern valid transitions between process states.
- **Institution packages** provide local vocabulary and rules over the
  generic objects.
- **Nodes** store, sign, scope, and sync process state.
- **ActionCards** route attention to the next process step.
- **Receipts** prove that important transitions happened.
- **Evidence exports** summarize repo-safe outcomes for partners and
  the public, without leaking private process content.

## The process spine

The spine is a sequence of object families. Each is named here as a
plain-language family, not as a schema. None of these is a commitment;
the names are candidates that a future RFC or ADR may rename, fold, or
split.

### ProcessTargetRef

A typed handle to the institutional thing a process is operating on:
a proposal, a charter draft, an action item, an obligation under
review, an allocation under reconsideration, a settlement under
challenge, a meeting agenda item, a charter amendment, a membership
class change, a tool install request, a relationship grant, an
accessibility complaint, a privacy review, etc.

`ProcessTargetRef` is the join point that lets every other spine
object scope itself to one institutional thing without the spine
needing to know what kind of thing it is. The kernel does not read it;
apps and oracles do.

### ProcessSession

A bounded run of the spine over one `ProcessTargetRef`. Holds the
identifier the rest of the spine binds to. Carries a session lifecycle
(opened, deliberating, decided, activating, mutated, closed,
abandoned, superseded) without dictating the content of any phase.

A `ProcessSession` is **not** a workflow engine. It is a named context
that says "this preview, these deliberation entries, this decision,
this activation, this mutation plan, these receipts, and this evidence
packet all refer to the same institutional process run."

### PreviewReviewPacket

Defined by the read-model contract
`urn:icn:contract:preview-review:v1` (landed in PR #1745, merged
2026-05-05). Listed here for spine completeness only. Defines the
human review boundary between source material and any subsequent
action. The spine pins it as the surface where the **review** half of
"preview → deliberation → decision" is captured.

### DeliberationThread

A scoped record of institutional reasoning bound to one
`ProcessTargetRef` inside one `ProcessSession`. **Object-bound**, not
free-form chat: every entry references the target and is governed by
the institution's rules for who may speak and what must be recorded.

A `DeliberationThread` is **not**:

- a chat room
- a generic comment system
- a notification feed
- a moderation platform
- a social media wall
- a discussion forum

It is a structured institutional record: questions, concerns,
objections, amendments, blockers, facilitator summaries, resolutions,
privacy reviews, and accessibility reviews, captured as scoped
institutional process. Visibility is governed by charter, not by
default.

### DeliberationEntry

A single typed entry inside a `DeliberationThread`. Closed taxonomy
candidates (not a commitment): `question`, `concern`, `objection`,
`amendment`, `blocker`, `facilitator_summary`, `resolution`,
`privacy_review`, `accessibility_review`, `conflict_signal`,
`record_only`.

Each entry has provenance (who, when, in what role) and may carry an
accessibility-mode marker (plain-language summary, alt text, locale
metadata) per the accessibility gate. Entries are signed; entries are
not anonymous unless the institution's charter specifies an anonymity
mode and the substrate supports it.

The closed-taxonomy approach is deliberate: it keeps deliberation a
structured institutional record rather than a free-form text dump, and
it gives the accessibility gate, privacy review, and conflict-routing
paths concrete handles to act on.

### HumanDecisionSet / DecisionRecord

`HumanDecisionSet` is the structured set of decision moments inside
one `ProcessSession`: who decided, in what role, by what rule (vote
threshold, consensus, mandate from a parent body, delegated authority,
emergency authority, etc.), with what tally or attestation, and over
which proposed outcome.

`DecisionRecord` is the persisted, signed artifact that names the
chosen outcome and binds it to the deliberation that produced it.

The brief deliberately separates these from `MutationPlan`: a decision
is **not** a mutation. A decision authorizes a mutation; a mutation
plan describes one; an activation request crosses the boundary; only
then does runtime mutate.

### ActivationRequest

The explicit boundary between *deciding* and *doing*. An
`ActivationRequest` declares: a decision has been recorded, the
authority to act is established, and the institution is now ready to
cross from review-only into mutation. This is the spine's
"second-screen confirm" — the place where the no-CLI workflow's "no
mutation without an explicit second screen" rule lives at the
substrate.

`ActivationRequest` is not the mutation. It is the gate. It can be
refused (process gate result fails, accessibility review pending,
privacy review pending, charter rule unmet, settlement window not yet
open).

### MutationPlan

The plan-of-record for what runtime should do as a consequence of the
activation. Names the affected objects, the specific operations
(create, update, retire, reassign, allocate, settle, install, bind),
and the expected receipts. The kernel does **not** read the plan
semantically; the kernel only enforces the constraints the policy
oracle returned. The plan is for human and partner review, audit, and
replay.

A `MutationPlan` is preview-shaped: a `PreviewReviewPacket` with
`preview_kind = pending_publish_summary` already covers part of this
surface. The brief intentionally pins the relationship: the plan **is**
the upstream artifact the pending-publish preview renders. Apps may
choose to materialize a `MutationPlan` only at activation time; the
preview packet is the read-model.

### ProcessGateResult

A typed pass/fail/blocked record from a gate that the spine consults
before transitioning. Examples: accessibility gate result, privacy
review gate result, repo-safety gate result, charter-rule gate result,
quorum gate result, second-reviewer signoff gate result, fixture-only
check gate result, no-mutation check gate result.

Gates are explicit and named. Gates are not implied. A gate that does
not produce a `ProcessGateResult` did not run.

### ActionCard triggers

Already shipping as `ADR-0027`. The spine pins the relationship: spine
transitions emit action cards to route attention to the next step. A
deliberation entry tagged `objection` may trigger an action card for
the facilitator. An `ActivationRequest` may trigger an action card for
the holder of activation authority. A `ProcessGateResult` of `blocked`
may trigger an action card for the reviewer who must clear the block.

The brief does **not** propose a new ActionCard contract. It pins
spine transitions as legitimate emit-points for action cards under the
existing contract, deferring schema work to `ADR-0027` and `#1713`.

### Receipt classes for process transitions

Already shipping in concept as `ADR-0026` (proof envelope). The spine
identifies a small set of process-transition receipt classes that the
existing envelope can carry without a new envelope:

- `ProcessSessionOpenedReceipt`
- `DeliberationEntryRecordedReceipt`
- `DecisionRecordedReceipt`
- `ActivationCrossedReceipt`
- `MutationPlanRecordedReceipt`
- `MutationAppliedReceipt` (already exists in concept as the
  action-item / governance receipt families)
- `EvidencePacketProducedReceipt`
- `ProcessGateResultReceipt`

These are class candidates, not schemas. The brief does **not** lock
their shapes. It pins them as the receipt classes a future process
runtime would emit so that downstream work knows the proof surface
exists at every transition, not only at the end.

### EvidencePacket integration

Already shipping as a contract
(`urn:icn:contract:rehearsal-evidence-export:v1`). The spine pins the
relationship: an `EvidencePacket` is the repo-safe summary of one
`ProcessSession`, suitable for public discussion, partner review, or
issue comments. Evidence is downstream of the spine, not parallel to
it. The contract already declares what an evidence export must not
carry; the spine names *what produces it*.

### CCL / charter hooks

The brief is not the right place to specify CCL syntax. The spine
identifies the **hook points** where CCL / charter rules govern spine
transitions:

- charter rule: who may open a `ProcessSession` for which
  `ProcessTargetRef` kinds
- charter rule: what `DeliberationEntry` kinds are required before a
  `HumanDecisionSet` may be recorded (e.g. mandatory privacy review on
  any session that touches a private-overlay-bound target)
- charter rule: what decision rule applies (vote threshold, consensus,
  mandated authority) per session kind
- charter rule: what gates must produce `ProcessGateResult: pass`
  before an `ActivationRequest` may be issued
- charter rule: what `MutationPlan` operations may be authorized for
  which session kinds and which deciding bodies
- charter rule: what `EvidencePacket` shape an institution exports for
  which session kinds

CCL design itself is gated on `idea-0012`, `idea-0018`, and the
in-flight `ADR-0023` framing. This brief does not foreclose any of
that work.

### Visibility / privacy model

The spine is **opaque to the kernel**. Visibility decisions are made
by app-layer policy oracles, governed by the institution's charter.
Default posture is conservative: a `DeliberationEntry` is visible to
the institution's body that authored it; broader visibility (other
bodies, members, the public) requires an explicit charter rule.

Private-overlay binding is first-class: a `DeliberationEntry` may
reference content held in a private overlay (per `#1730`), and the
spine must support that referencing without leaking the content into
the public substrate. Evidence exports redact accordingly. The
accessibility gate and privacy review are explicit
`ProcessGateResult` producers, not afterthoughts.

## Scope

- **In scope:** naming the spine, classifying MVP/near-term vs Later,
  cross-linking already-shipping pieces, identifying CCL/charter hook
  points, identifying receipt classes, identifying gate hooks, stating
  non-goals.
- **Out of scope:** schemas, runtime, gateway routes, kernel changes,
  SDK changes, website source, NYCN repo material, formal pilot
  authorization, production deployment claims, federation-scope
  process sync runtime.
- **Adjacent (named, not pursued):**
  - `idea-0012` (CCL institutional process language runtime details)
  - `idea-0014` (EffectRecord canonical schema)
  - `idea-0016` (institutional conflict object model)
  - `idea-0018` (CCL institutional rule authoring)
  - `RFC-0016` (RelationshipRecord)
  - `RFC-0017` (ToolManifest / ToolBinding / ToolInstall lifecycle)
  - `ADR-0023` (CCL institutional process language, proposed)
  - `ADR-0029` (conflict resolution object model, proposed/partial)

## MVP / near-term vs Later

The spine is named in full so the surface composes. Implementation
sequencing is **strict**, and most of the spine is **Later**.

### MVP / near-term

These integrate with already-merged or in-flight contracts and do not
require new substrate runtime work beyond what the rehearsal milestone
(#1746) is already coordinating:

- `PreviewReviewPacket` integration (contract
  `urn:icn:contract:preview-review:v1` landed in PR #1745; spine pins
  where in the process it lives)
- object-bound `DeliberationThread` / `DeliberationEntry` as a
  read-model only, paired with the existing `PreviewReviewPacket` —
  no chat semantics, no notification system
- `HumanDecisionSet` as a read-model that names who decided by what
  rule with what tally or attestation
- `MutationPlan` sketch as the upstream artifact of the existing
  `pending_publish_summary` preview kind
- `ActionCardTrigger` sketch (which spine transitions are legitimate
  action-card emit-points under `ADR-0027`)
- `EvidencePacket` relationship: spine names the
  `ProcessSession` → `EvidencePacket` binding; the contract is
  already shipping

These MVP items are **read-model and binding work**, not runtime work.
They are the smallest set that lets the rehearsal milestone (#1746)
compose end-to-end without inventing new shapes.

### Later

These require runtime, additional governance work, additional CCL
work, or formal partnership decisions, and must not enter MVP scope:

- `RelationshipRecord` integration with the spine (gated on
  `RFC-0016`)
- `ToolInstallRequest` integration with the spine (gated on
  `RFC-0017`)
- obligation / allocation / settlement lifecycle integration with the
  spine (gated on `RFC-0001`)
- conflict resolution / institutional care integration with the spine
  (gated on `ADR-0029` and `idea-0016`)
- federation-scope process sync (multi-cooperative deliberation,
  cross-institution decision recognition, federation-level evidence)
- automated `ProcessGateResult` producers beyond the manual checklist
  pattern (e.g. machine-checkable charter rule enforcement, automated
  accessibility scanning hooks)
- charter-amendment-as-process: using the spine to govern its own
  rules
- privacy-overlay-bound `DeliberationEntry` content addressing
- CCL syntax for spine transitions (gated on `ADR-0023`,
  `idea-0012`, `idea-0018`)

The Later list is intentionally long. Each item is its own design
space and may produce its own framing brief, RFC candidate, or ADR
candidate. The spine names them so that downstream work knows where to
hang off; the spine does not commit to building them.

## Non-goals

This brief is **not**:

- chat
- social media
- a generic comment system
- a notification system
- a moderation platform
- a workflow-engine mega-build
- an NYCN-specific surface
- a runtime implementation
- a pilot formalization
- a Phase 2 completion claim
- a production-readiness claim
- a public website claim
- a mutation API
- a new receipt envelope (the existing `ADR-0026` envelope carries the
  receipt classes named here)
- a new ActionCard contract (the existing `ADR-0027` contract carries
  the triggers named here)
- a CCL syntax decision
- a federation runtime decision
- a conflict-resolution decision (`ADR-0029` owns that)
- a binding on partner repositories or partner data shape
- a re-decoration of the existing economic vocabulary; the brief uses
  *obligation*, *allocation*, *settlement*, *unit*, *position*,
  *receipt*, *provenance*, and *evidence* and avoids ICN-native
  *payment*, *currency*, *balance*, and *wallet* framing

## Why deliberation is first-class

ICN should not merely record decision corpses after the real politics
happened elsewhere.

Deliberation captures the institutional process that produces a
decision — questions, concerns, objections, amendments, blockers,
facilitator summaries, resolutions, privacy reviews, and accessibility
reviews. Without an object-bound deliberation surface, ICN imports
whatever politics produced an outcome from chat threads, private
meeting rooms, and partner spreadsheets that ICN cannot prove. A
substrate that names *what was decided* but not *how it was decided*
is a substrate that defers institutional legitimacy to opaque
upstream tooling — exactly the failure mode the constitutional core
exists to prevent.

Object-bound deliberation is the inverse of chat: every entry refers
to one institutional thing, every entry is signed, every entry is
typed, every entry is governed by charter, every entry is
accessibility-aware, every entry is privacy-aware, and every entry
contributes to a `DecisionRecord` that an institution can later inspect
without private oral history.

This is what makes the `decisive test` from the 2026-04-15 handoff
livable: a new organizer can enter mid-cycle, see the current
`ProcessSession`, read the `DeliberationThread`, see what was decided
in the `DecisionRecord`, see what is blocked in pending
`ProcessGateResult` rows, see their assignments via action cards, see
why they exist via deliberation provenance, and continue the work.

## Layer fit

The spine respects the kernel/app boundary explicitly.

- **Kernel** remains meaning-blind. It enforces constraints; it does
  not read `DeliberationEntry` bodies, `DecisionRecord` rationales, or
  `MutationPlan` operation semantics. The kernel only sees
  `ConstraintSet` and `PolicyDecision` per the existing kernel/app
  separation rule.
- **ICN platform/app layer** owns generic process objects. Apps
  implement the spine; apps render previews; apps publish
  deliberation entries; apps record decisions; apps emit activation
  requests; apps materialize mutation plans; apps trigger action cards;
  apps produce evidence packets.
- **CCL/charters** govern valid transitions. Which entry kinds are
  required? Which decision rule applies? Which gates must pass? Which
  authorities may activate? Charter content is interpreted by apps and
  policy oracles, not by the kernel.
- **Institution packages** provide local vocabulary and rules. A
  package may name "agenda-item" instead of "ProcessTargetRef of kind
  meeting-item"; a package may name "block" instead of
  `DeliberationEntry: blocker`. The substrate stays generic; the
  institution maps.
- **Nodes** store, sign, scope, and sync process state. Process
  sessions live alongside other ICN state; signed process records
  participate in gossip and replication subject to visibility policy.
- **ActionCards** route attention. A spine transition that an
  authorized human must perform produces an action card. Action cards
  do not own the process; they surface the process's next demand on a
  human's attention.
- **Receipts** prove that important transitions happened. A
  `DeliberationEntryRecordedReceipt` is a small but real proof that
  the institution recorded an objection at a specific time by a
  specific authorized speaker.
- **Evidence exports** summarize repo-safe outcomes. Evidence is the
  outward face of the spine for partners and the public; evidence
  redacts what charter and privacy review say must not leave private
  scope.

The spine **does not erode the meaning firewall**. The kernel never
sees institutional meaning. Apps and oracles convert spine events into
constraints when constraints are needed (rate limits on deliberation
entry submission, capability checks on activation, gate-result
encoding into `ConstraintSet`).

## Boundary check

- Belongs in the ICN idea refinery for now.
- ICN core / app substrate, generic. Not NYCN-specific. Not partner-
  specific.
- Not an RFC or ADR yet.
- Not a website claim. Not an icn-learn packet.
- No private operational data in the brief or in the eventual
  artifacts.
- Drive / Sheets / Groups / private overlays are not addressed by this
  brief; they are bounded by the visibility/privacy model paragraph
  and by `#1730`.

## Existing surface

What already exists in the repo that this brief touches:

- `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md` —
  the receipt envelope the spine's process-transition receipts use.
- `docs/adr/ADR-0027-action-card-contract.md` — the action-card
  contract the spine's `ActionCardTrigger` binds to.
- `docs/adr/ADR-0023-ccl-institutional-process-language.md`
  (`proposed`) — the CCL process language direction; spine hook points
  defer to it.
- `docs/adr/ADR-0028-accessibility-baseline-for-member-interfaces.md`
  (`proposed`) — the accessibility floor any spine surface must meet.
- `docs/adr/ADR-0029-conflict-resolution-object-model.md`
  (`proposed/partial`) — the conflict path the spine defers to,
  including dispute routing on `DecisionRecord` and `EffectRecord`
  challenges.
- `docs/contracts/preview-review.md` and
  `docs/contracts/preview-review.schema.json` —
  `urn:icn:contract:preview-review:v1` (PR #1745).
- `docs/contracts/rehearsal-evidence-export.md` and
  `docs/contracts/rehearsal-evidence-export.schema.json` —
  `urn:icn:contract:rehearsal-evidence-export:v1`.
- `docs/contracts/schema-id-audit.md` — the audit table any future
  spine schema would join.
- `docs/contracts/institution-package/action-card.schema.json` — the
  action-card schema (under separate `$id` migration calendar at
  `#1742`).
- `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md` — the
  organizer-facing path the spine sits underneath.
- `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` — the PR-time
  gate the spine's surfaces must satisfy.
- `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` — the
  authority-vs-convenience and participation-access reflexes the
  spine must respect.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — the boundary
  rule the spine respects.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — the meaning firewall
  rule the spine respects.
- `ops/coordination/rfc_candidates.yaml` — pending design spaces the
  spine intersects (`0001`, `0016`, `0017`, `0023`).
- `ops/coordination/adr_candidates.yaml` — likely future decisions the
  spine touches.
- Idea cards: `idea-0012`, `idea-0014`, `idea-0015`, `idea-0016`,
  `idea-0017`, `idea-0018`.
- Issues: `#1746` (showcase milestone), `#1724` (no-CLI workflow
  parent), `#1728` (preview/review contract), `#1729` (evidence
  export), `#1730` (private-overlay / DID activation), `#1713`
  (action-card schema stabilization), `#1726` / `#1727` (rehearsal
  shell / fixture-backed demo).

This brief does not mutate any of those documents.

## Open questions

1. Is `ProcessTargetRef` one polymorphic handle, or a small family
   keyed by the kind of institutional thing being processed? The
   answer affects whether visibility and routing rules attach to one
   surface or to a family of surfaces.
2. Is `ProcessSession` runtime state, persisted state, or both? If
   persisted, what is the smallest viable persistence path that does
   not duplicate existing governance state machines?
3. Should `DeliberationEntry` kinds be a closed taxonomy locked by an
   ADR, or extensible via charter? The closed-taxonomy option is
   safer for accessibility/privacy gates; the extensible option is
   safer for institutional variation.
4. How does `HumanDecisionSet` relate to the existing proposal/vote
   machinery? Is the existing path one specialization of
   `HumanDecisionSet`, or a parallel surface that the spine names but
   does not absorb?
5. How does `MutationPlan` relate to `EffectRecord` (`idea-0014`,
   `ADR-0025`)? Is the plan upstream of the effect, or does the
   effect schema absorb the plan's structure?
6. What is the smallest viable `ProcessGateResult` shape that the
   accessibility gate, privacy review, repo-safety check, and
   charter-rule gate all share?
7. How does the spine compose with `RFC-0016` (RelationshipRecord) and
   `RFC-0017` (ToolInstall) without absorbing them?
8. How do federation-scope sessions reconcile when two cooperatives
   run parallel deliberations on shared targets? (Deferred to Later;
   listed here so the spine acknowledges the question.)
9. What is the privacy-overlay binding shape for a
   `DeliberationEntry` that references private content without leaking
   it to the public substrate?
10. What is the minimal CCL surface the spine actually needs from
    `idea-0012` / `idea-0018` to express valid transitions?

## Privacy and boundary risks

- Deliberation entries can carry the most sensitive institutional
  content the system records: objections, accessibility concerns,
  conflict signals, privacy reviews. The spine must support
  private-overlay binding **by design**, not by retrofit.
- Cross-cooperative federation sync can leak who-said-what across
  institutions if visibility defaults are permissive. Default posture
  is conservative; broader visibility requires explicit charter rule.
- `MutationPlan` previews are easy to over-share. The existing
  `preview-review.schema.json` `additionalProperties: false` discipline
  must extend to any spine-level shape.
- `EvidencePacket` redaction must remain producer-side: the schema's
  `repo_safety.classification` discipline already enforces this; spine
  documentation must not weaken it.
- Action-card triggers can leak process state to attention surfaces
  that should not see it. The action-card contract's existing
  privacy posture must be preserved at every trigger point.
- The "named-but-empty objects" risk from `idea-0010` applies here
  too. Naming spine objects in this brief is **not** a backlog
  commitment to build them. Promotion review must enforce that.

## Proposed next artifact

Pick exactly one (this brief picks one):

- [ ] another framing brief (decompose first)
- [ ] source review
- [x] dogfood slice — author the smallest end-to-end MVP slice
      (PreviewReviewPacket → DeliberationThread (read-model) →
      HumanDecisionSet (read-model) → MutationPlan sketch →
      ActionCardTrigger sketch → EvidencePacket) against an
      existing fixture path, using the existing action-card runtime
      and existing receipt envelope. The dogfood slice is the
      evidence required before this brief promotes to RFC.
- [ ] promotion review → RFC candidate
- [ ] promotion review → ADR candidate
- [ ] promotion review → GitHub issue
- [ ] promotion review → NYCN package task
- [ ] promotion review → icn-learn packet
- [ ] promotion review → website claim
- [ ] park
- [ ] reject

A coordination milestone issue (`milestone(process): define
Institutional Process Substrate`) may also be opened to track
cross-piece composition under `#1746`. The milestone is **not** a
substitute for the dogfood slice; it is a coordination surface across
already-open pieces.

Do not promote to RFC until the dogfood slice produces runtime
evidence against a real or fixture-equivalent process session.

## Receipts / evidence (if relevant)

Eventually, an institutional process substrate will need:

- One worked `ProcessSession` end-to-end against a fixture: a
  `ProcessTargetRef` of a small institutional thing, a
  `DeliberationThread` with at least one of every entry kind in the
  closed taxonomy, a `HumanDecisionSet` with one decision rule, an
  `ActivationRequest`, a `MutationPlan`, at least one
  `ProcessGateResult: pass` and one `ProcessGateResult: blocked`,
  action-card emissions at the right transitions, receipts emitted
  under the existing envelope, and an `EvidencePacket` exported
  repo-safe.
- Evidence that the spine respects the meaning firewall: kernel-side
  audit that no spine object is read for its semantics by kernel
  code; oracle-side conversion of every spine signal into generic
  constraints.
- Evidence that visibility policy works: a `DeliberationEntry`
  visible to body A but not to body B, redacted accordingly in the
  evidence export.
- Evidence that the accessibility gate produces a real
  `ProcessGateResult` that the spine consumes.
- Evidence that private-overlay binding works: a
  `DeliberationEntry` referencing private content that is correctly
  redacted in evidence and correctly routed to action cards without
  leaking content into the public substrate.

None of this evidence exists today. Producing it is downstream of
later promotion reviews and the dogfood slice, not of this framing
brief.

## Coda

Name the spine before building the limbs. The immediate goal is not
implementation. The immediate goal is to prevent preview/review,
deliberation, mutation planning, action cards, receipts, evidence,
accessibility gates, and private overlays from scattering into
disconnected features.

The architectural thesis: ICN can carry an institution's reasoning,
consent, objection, assignment, execution, and evidence without
centralizing authority or erasing human process.
