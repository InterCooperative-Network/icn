# Democratic Authority Primitives — framing brief

**Idea card:** `ops/ideas/ideas.yaml#idea-0020`
**Author / session:** 2026-05-05 session
**Date:** 2026-05-05
**Status:** pre-RFC framing. Not a design doc. Not a decision. Not a
schema commitment. Not a runtime claim.

> **Seed-brief discipline.** This brief names a family of generic
> institutional primitives that recur across cooperatives, communities,
> federations, mutual-aid networks, land trusts, and associations. It
> does not invent a new domain. If future passes add per-primitive
> schemas, runtime patterns, or capability maps, those split into
> separate framing or RFC artifacts rather than letting this brief
> become a design doc. ICN does not prescribe one democratic model. It
> provides the primitives by which institutions define, constrain,
> prove, and revise their own democratic authority structures through
> CCL, charters, and institution packages.

## 1. Purpose

Name a small set of generic primitives — **authority / participation**
and **deliberation context / educational reference** primitives — so
that institutional reasoning, authority basis, evidentiary context,
expert and advisory input, conflict disclosure, facilitator and
steward/operator roles, and revocation/recall/challenge paths can be
recorded as scoped, typed, signed, governable surfaces rather than as
implicit assumptions, out-of-band conventions, or chat threads ICN
cannot prove.

These are **generic institutional primitives**, not ICN app features.
Institutions adopt and constrain them through CCL, charters, and
institution packages. Runtime, if and when later built, only stores
records, checks constraints, emits receipts, and routes action cards.
UI explains authority and context to humans. The kernel never reads
authority semantics; it enforces constraints downstream of policy
oracle decisions.

## 2. Why this matters

The Institutional Process Substrate (`idea-0019`) names *what gets
processed*: a target, a session, a deliberation, a decision, an
activation, a mutation plan, an evidence packet. It does not yet name
**who is speaking, on what basis, in what role, with what context, to
which body, under what limits, contestable by which path**. Without
those, every spine record imports its authority assumptions silently.

ICN's claim to be infrastructure for democratic institutional
coordination becomes hollow at exactly this layer if it ships. A
deliberation surface that does not carry authority basis is a chat
room with timestamps. A decision record that does not carry
authority basis is a verdict from nowhere. A delegation that does
not carry scope, expiry, and revocation is quiet aristocracy. A
representation that does not carry mandate, constituency, term,
duties, reporting, recall, and conflict disclosure is unaccountable
proxy sovereignty. An expert opinion that lands as a vote is
expertocracy. An expert opinion that is filtered out of the record
is anti-expertise. A facilitator whose process authority is not
distinguished from outcome authority quietly captures decisions. A
steward or operator whose execution authority is not distinguished
from democratic sovereignty quietly substitutes for it.

The deliberation surface itself also fails without **context**.
Democratic deliberation does not only require a place to speak. It
requires a shared context layer that lets people understand what they
are being asked to decide: prior decisions, charter rules, CCL rules,
educational references, evidence references, counterarguments,
glossaries, accessibility and privacy notes, risk notes. Without that
layer, "deliberation" becomes a vote on an unfamiliar object —
accidentally rule by whoever already had context.

The thesis ICN must defend, restated:

- **Member voice is not delegated authority.**
- **Delegation is not representation.**
- **Representation is not expertise.**
- **Expertise is advisory by default unless CCL/charter says
  otherwise.**
- **Facilitator process authority is not outcome authority.**
- **Steward/operator execution authority is not democratic
  sovereignty.**
- **Authority must always carry its basis.**

## 3. Relationship to Institutional Process Substrate / idea-0019

Authority Primitives are **orthogonal** to the spine named in
`idea-0019`. The spine names *what gets processed*. Authority
primitives name *who, on what basis, in what role, under what limits,
contestable by which path*. They do not extend the spine; they fill
its records with the typing the spine deliberately deferred.

Worked composition (read-model only — no schema commitment):

- A `DeliberationEntry` (`idea-0019` candidate) carries an
  `AuthorityBasis` and a `ParticipationRole` per author.
- A `DeliberationThread` may attach a `DeliberationContext` of
  `ContextReference[]`, `LearningReference[]`,
  `PriorDecisionReference[]`, `CharterRuleReference[]`,
  `CCLRuleReference[]`, `EvidenceReference[]`,
  `CounterargumentReference[]`, `GlossaryReference[]`,
  `AccessibilityNote[]`, `PrivacyNote[]`, `RiskNote[]`.
- A `FacilitatorSummary` is the typed shape of `idea-0019`'s proposed
  `facilitator_summary` `DeliberationEntry` kind; the primitive carries
  facilitator process authority explicitly so it cannot be confused
  with an outcome-grade decision.
- A `HumanDecisionSet` / `DecisionRecord` carries the `AuthorityBasis`
  of the deciding body and the `ParticipationRole` of each decider.
- An `ActivationRequest` carries an `OperatorExecutionAuthority`
  reference distinguishing democratic authorization (decision side)
  from execution authority (operator side).
- An `ExpertStatement` and an `AdvisoryOpinion` enter
  `DeliberationThread` as first-class entries, with explicit
  advisory-by-default posture and an explicit `ConflictDisclosure`.
- A `DelegationGrant` and a `RepresentationMandate` are the
  authority records that explain a `ParticipationRole` of kind
  `delegate` or `representative`, and they carry their own
  scope, term, and revocation/recall path.
- A `MinorityReport` records dissent that survived a decision,
  attached to the same `ProcessSession` as the `DecisionRecord`.
- `ChallengePath`, `RevocationPath`, and `RecallPath` declare the
  routes by which a decision, a grant, a representative, an
  indicator, an expert claim, or a charter rule can be contested.

What this brief does **not** do for `idea-0019`:

- Does not relabel the spine.
- Does not change the spine's transitions.
- Does not commit any spine schema.
- Does not pre-empt `idea-0019`'s open questions Q1, Q3, Q4 (those
  remain the gating questions for `idea-0019` runtime promotion).

## 4. Primitive candidates

All names are candidates. None is a schema. None is a backlog
commitment. A future RFC, ADR, or framing brief may rename, fold,
split, retype, or reject any of them. Naming each here only declares
that the institutional concept is plausibly generic across multiple
institution types and would benefit from a typed surface rather than
remaining implicit.

The set is split into two families:

- **Authority / participation** — *who is speaking or acting, on what
  basis, in what role, under what limits, accountable by which path.*
- **Deliberation context / educational reference** — *what shared
  understanding, prior record, rule, evidence, or risk a deliberation
  binds to.*

Both families are generic; both are CCL/charter/package-shaped. ICN
core may eventually model a record, store, lifecycle, and receipt for
each — but only after a second institution would need each one with
the same shape, per the graduation rule in
`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`.

### 4.1 Authority / participation primitives

| Primitive | One-line shape | Why it earns a generic surface |
|-----------|----------------|--------------------------------|
| `AuthorityBasis` | A typed reference to the source of an actor's authority for a specific record (standing-grade role, moment-grade grant, mandate, charter rule, delegation, representation, expert advisory posture, facilitator process authority, steward/operator execution authority, member-as-self). | Every record an institution wants to defend later — entry, decision, grant, mutation — needs to be replayable to its authority source, not implicitly trusted. |
| `ParticipationRole` | The role an actor is acting in for a specific record (member-as-self, delegate, representative, expert, advisor, facilitator, steward, operator, witness, observer). Distinct from standing-grade `RoleAssignment`. | The same person may speak in multiple roles across the same session. The role is per-record, not per-person. |
| `DelegationGrant` | A scoped, time-bounded, revocable grant of one specific authority from one delegator to one delegate. Receipted at issuance, exercise, expiry, and revocation. | Delegation must not become quiet aristocracy. A grant without scope, expiry, and revocation is a private oligarchy in the substrate. |
| `RepresentationMandate` | A mandate carrying constituency, selection method, term, duties, reporting cadence, recall path, and conflict-of-interest disclosure for a representative acting on behalf of others. | Representation must not become unaccountable proxy sovereignty. Without mandate fields, a representative becomes a private power. |
| `ExpertStatement` | A typed claim by an actor in role `expert` carrying field/scope, claim, evidence references, limits, confidence/disclosure posture, conflicts of interest, requested-by, and advisory-by-default flag. | Experts inform power. They do not automatically become power. The substrate must record both the claim and its limits. |
| `AdvisoryOpinion` | An opinion entered into a `DeliberationThread` in role `advisor` with no decisional authority unless CCL/charter explicitly elevates it for that session kind. | Advisory input is recorded as advisory. Elevation to decisional authority is an explicit charter act, never a default. |
| `ConflictDisclosure` | A typed self- or third-party-recorded disclosure of a conflict of interest attached to a `DeliberationEntry`, `ExpertStatement`, `AdvisoryOpinion`, `DelegationGrant`, `RepresentationMandate`, `DecisionRecord`, or `OperatorExecutionAuthority`. | Conflict disclosure is the price of speaking. Without a typed shape, conflicts become whisper networks. |
| `FacilitatorSummary` | A signed summary of process state by an actor in role `facilitator`, carrying explicit *process authority* and explicit non-decisional posture. | Facilitator authority over the conversation is not authority over the outcome. The substrate must encode that distinction. |
| `StewardReview` | A signed review by an actor in role `steward` that confirms procedural conditions for an `ActivationRequest`, but does not authorize the underlying decision. | Stewards are technical/procedural runners. Their review unlocks execution; it does not substitute for the decision. |
| `OperatorExecutionAuthority` | The typed record that an operator is authorized to execute a `MutationPlan` because (a) a `DecisionRecord` exists, (b) every required `ProcessGateResult` passed, and (c) the operator holds the relevant scope. Strictly downstream of decision. | Operator authority must never appear without the decision and gate results that authorize it. Conflating decision with execution is a primary failure mode of "dashboards as governance." |
| `MinorityReport` | A typed dissent record attached to a `DecisionRecord`, signed by a member or body, preserving the dissenting view and its rationale on the institutional record. | A record that captures only what won is a record that erases what disagreed. Minority reports are how dissent survives the decision moment. |
| `ChallengePath` | A typed declaration that a record (decision, grant, representative, indicator, expert claim, charter rule) is contestable by a defined route — which body, which signal kind, which timeline, which evidence requirements. | If a record cannot be challenged, it cannot be governed. A challenge path is the visible inverse of the decision moment. |
| `RevocationPath` | A typed declaration of how a `DelegationGrant` may be revoked — by whom, with what receipt, under what notice. | A grant without a revocation path is an open-ended transfer of authority. Revocation must be a first-class record, not a private deletion. |
| `RecallPath` | A typed declaration of how a `RepresentationMandate` may be recalled — by which constituency, by which procedure, with what notice, with what consequences for in-flight decisions. | Representation without recall is unaccountable proxy. Recall is the constituency's authority surface, not a UI option. |

### 4.2 Deliberation context / educational reference primitives

| Primitive | One-line shape | Why it earns a generic surface |
|-----------|----------------|--------------------------------|
| `DeliberationContext` | A typed bundle of context references attached to a `DeliberationThread` (or to a single `DeliberationEntry` when the context is entry-scoped). Names what shared understanding the thread assumes. | Deliberation without context is rule by whoever already had context. The substrate must let an institution attach the shared frame explicitly. |
| `ContextReference` | A typed reference to any contextual artifact — prior session, charter section, rule, indicator, evidence packet, accessibility note, glossary entry. The polymorphic root of the context family. | Most context is reference, not new content. A polymorphic reference type lets the spine compose without duplicating storage. |
| `LearningReference` | A reference to an educational artifact (icn-learn packet, partner explainer, glossary entry, model law, regulatory primer) that explains what is at stake or how the rule works. | Members must be able to learn what they are being asked to decide. Educational scaffolding is part of legitimacy, not polish. |
| `EvidenceReference` | A reference to an `EvidencePacket`, `Receipt`, indicator sample, audit artifact, or external evidence source supporting or disputing a claim in the deliberation. | Claims must be paired with their evidence. A reference type lets evidence travel with the deliberation without leaking content. |
| `PriorDecisionReference` | A reference to one or more prior `DecisionRecord`s relevant to the current target — precedent, parent decision, superseded decision, related decision. | Institutions reason in cycles. Prior decisions are first-class context, not memory hazards. |
| `CharterRuleReference` | A pinned reference to the charter section governing the current session kind — entry kind requirements, decision rule, gate requirements, activation authority, mutation scope. | Charter is institutional law. References must be explicit and pinned to a version, not implied. |
| `CCLRuleReference` | A reference to one or more CCL rules that govern transitions in the session — quorum, threshold, escalation, expiry, recall. | CCL rules are machine-checked law. The deliberation surface must be able to point at exactly which rule applies. |
| `AccessibilityNote` | A typed note about an accessibility constraint or accommodation relevant to the session — language access, plain-language gloss, screen-reader path, low-bandwidth path, captioning, motor-access path, cognitive-load mitigation. | The accessibility gate (`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`) treats accessibility as architecture, not polish. Notes are the substrate's surface for that. |
| `PrivacyNote` | A typed note declaring privacy posture for an entry, a context reference, or the session as a whole — visibility default, redaction requirement, private-overlay binding, evidence-export disposition. | Privacy decisions must be visible and reviewable, not implicit. |
| `RiskNote` | A typed note declaring a known risk relevant to the current decision — operational, legal, reputational, security, accessibility, privacy, financial. Distinct from a `concern` deliberation entry. | Risk attaches to the target, not just to the conversation. Risk notes survive turnover where conversation does not. |
| `CounterargumentReference` | A reference to a prior argument, position paper, dissent, model critique, or external rebuttal that the deliberation must consider. | The institution must record that a counter-position was offered, not only that one position won. Counterargument references are the proof of intellectual honesty. |
| `GlossaryReference` | A reference to a glossary entry — local institutional vocabulary, ICN substrate term, regulatory term, legal definition, accounting definition. | Plain-language access is participation infrastructure. A glossary reference is the smallest unit of that scaffolding. |

## 5. CCL / charter adoption model

The substrate names the primitives. CCL and charters govern their
**creation, validity, scope, expiration, challenge, revocation,
recall, disclosure, visibility, and transition rules**. Institution
packages provide local names, templates, explanatory copy, role
labels, and domain vocabulary. This is the same kernel/app boundary
that already governs every other ICN primitive
(`docs/architecture/KERNEL_APP_SEPARATION.md`,
`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`).

### 5.1 What CCL / charter governs (illustrative — not a contract)

- **Authority basis validity.** Which `AuthorityBasis` kinds are
  acceptable for which session kinds, target kinds, and decision rules.
- **Participation role gating.** Which `ParticipationRole` values are
  legitimate for which entry kinds, decision moments, and activation
  steps.
- **Delegation rules.** Maximum scope, maximum term, redelegation
  permission, conflict-of-interest disclosure requirement, revocation
  path, expiry behavior, receipt requirements.
- **Representation rules.** Constituency definition, selection method,
  term length, mandate scope, reporting cadence, recall procedure,
  conflict-of-interest baseline, decision power vs deliberation power.
- **Expert and advisory rules.** Who may register as an expert in
  which fields, advisory-by-default exceptions, conflict disclosure
  requirements, allowed elevation paths, evidence requirements.
- **Facilitator authority bounds.** What process authority a
  facilitator holds, what they cannot do (decide outcomes), how the
  facilitator role is assigned and revoked.
- **Steward / operator authority bounds.** What execution authority a
  steward or operator holds, what they cannot do (substitute for a
  decision), how they are accountable for execution receipts.
- **Minority report defaults.** Whether a minority report is required
  on decisions of this kind, who may sign it, how long it is preserved,
  which downstream surfaces must surface it.
- **Challenge / revocation / recall paths.** Which signal kinds open
  challenges, which bodies receive them, which timelines apply, which
  evidence is required, what receipts close them.
- **Context attachment requirements.** Which `DeliberationContext`
  references are required before a `HumanDecisionSet` may be recorded
  — e.g. mandatory `CharterRuleReference`, mandatory
  `AccessibilityNote` for member-facing changes, mandatory
  `PrivacyNote` for sessions touching private-overlay-bound targets.
- **Visibility and redaction.** Default visibility for each primitive
  kind; redaction rules for evidence export.

### 5.2 What institution packages provide

- **Local vocabulary.** "Steering committee," "delegates' council,"
  "council of stewards," "general assembly" — these names map onto
  generic primitives without leaking into them.
- **Role labels.** "Treasurer," "convenor," "ombudsperson,"
  "secretariat," "elder," "rapporteur" — same mapping discipline.
- **Templates.** Mandate templates, grant templates, conflict
  disclosure templates, facilitator summary templates, minority report
  templates, recall petition templates.
- **Explanatory copy.** Plain-language explanations of why a
  primitive exists, what it requires, and how to use it.
- **Domain vocabulary.** Institution-specific signal kinds, indicator
  kinds, evidence kinds, glossary entries.

### 5.3 What ICN core / app substrate eventually owns

When and only when a second unrelated institution would need each
primitive with the same shape, the substrate may eventually model:

- record shape, store, indexing
- lifecycle state machine
- receipt classes (under existing `ADR-0026` envelope; no new
  envelope)
- gateway HTTP routes (read-model first; mutation only when justified)
- meaning-firewall-clean conversion of authority signals into generic
  `ConstraintSet` values that the kernel enforces blindly

None of that runtime is in scope for this brief. The brief names the
shape so future runtime work can hang off a defined surface rather
than each feature inventing one.

## 6. Delegation

### 6.1 What delegation is

A `DelegationGrant` is one delegator extending one specific authority
to one delegate, for a bounded purpose, for a bounded time, revocable
by a defined path, receipted at every transition.

### 6.2 What delegation is not

- It is **not** representation. Delegation passes a single authority
  for a single purpose; representation carries a constituency mandate
  with reporting and recall.
- It is **not** sovereignty transfer. A delegate cannot escalate the
  grant into membership, treasury, or charter authority.
- It is **not** an open-ended proxy. Open-ended proxies are
  unaccountable; they are the failure mode this primitive exists to
  refuse.

### 6.3 Required properties

Scoped, time-bounded, revocable, receipted, conflict-disclosable,
visible to delegator and delegate, surfaced in `/me/standing` and
`/me/action-cards` for both parties. These align with the
`TemporaryAuthorityGrant` doctrine in
[`docs/architecture/INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md`](../../../docs/architecture/INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md)
§5 and the umbrella tracking issue [#1632](https://github.com/InterCooperative-Network/icn/issues/1632).
A `DelegationGrant` may compose with a `TemporaryAuthorityGrant` when
the underlying authority being delegated is itself a moment-grade
grant; this brief does not commit to whether they are one type or
two.

### 6.4 Doctrinal rules

- **No silent permanence.** Renewals are explicit, never automatic.
- **No invisible scope.** A grant without scope is rejected at
  construction.
- **Scope ≠ sovereignty.** A grant cannot escalate a structure into
  an entity, cannot create a new entity, cannot move treasury beyond
  the granted scope.
- **Conflict disclosure is mandatory** for grants that touch
  resource allocation, settlement, or governance authority above a
  charter-defined threshold.
- **Default term is short.** Long-term grants exist; defaults are
  short to avoid drift into permanence.
- **Revocation is its own receipt class candidate.** The act of
  revoking is institutional truth, not a private deletion.

## 7. Representation

### 7.1 What representation is

A `RepresentationMandate` is the institutional record by which one
actor (or body) acts on behalf of others. It carries the elements
that make representation accountable rather than proxy:

- **Constituency** — who the representative speaks for.
- **Selection method** — election, appointment, sortition, rotation,
  delegation chain, hybrid.
- **Term** — start, end, renewal procedure.
- **Duties** — what the representative is expected to do; what they
  must report; what they may decide; what they may not decide.
- **Reporting cadence** — how often, to whom, in what form.
- **Recall path** — by which constituency, by which procedure, with
  what notice.
- **Conflict-of-interest baseline** — disclosure requirements at
  selection and at each significant decision.

### 7.2 What representation is not

- It is **not** delegation. A delegate carries one authority for one
  purpose; a representative carries a constituency mandate.
- It is **not** expertise. A representative may also be an expert in
  the field; the roles remain distinct.
- It is **not** unilateral authority. Representatives act under
  mandate; mandate sets and recalls the authority.

### 7.3 Why this matters for federation

`#1609` (entity-level voting eligibility and federation tally
semantics) is the upstream gate for federation-tier representation.
Without `RepresentationMandate` typed, federation tally semantics
remain implicit: a DID acting on behalf of an entity has no
explicit mandate record. A `RepresentationMandate` is the record
type that closes that gap.

This brief does **not** decide federation tally semantics. It names
the primitive a future RFC for `#1609` would use to make the
representation visible.

### 7.4 Doctrinal rules

- **No mandate, no representation.** A representative without a
  recorded mandate is acting privately, not institutionally.
- **Recall is a first-class authority of the constituency.** Recall
  must be possible; the path may be hard, but it must exist.
- **Conflict disclosure is part of the mandate, not an afterthought.**
- **Term limits are charter-bound, not substrate-bound.** ICN does
  not prescribe term length; CCL and charters do.

## 8. Expert and advisory input

### 8.1 What expert input is

An `ExpertStatement` is a claim by an actor in role `expert` carrying:
field/scope, claim, evidence references, limits, confidence /
disclosure posture, conflicts of interest, requested-by, and
**advisory-by-default flag**.

An `AdvisoryOpinion` is an opinion entered into a deliberation
by an actor in role `advisor` with no decisional authority unless
CCL/charter explicitly elevates it for that session kind.

### 8.2 What expert input is not

- It is **not** a vote. An expert statement entering a deliberation
  does not cast a decision unless the charter elevates it.
- It is **not** a verdict. An expert claim is paired with its
  evidence and its limits.
- It is **not** an exclusion. Filtering an expert claim from the
  record is anti-expertise; recording it (with limits and conflicts)
  is the substrate's job.

### 8.3 Doctrinal rules

- **Experts inform power. They do not automatically become power.**
- **Advisory by default.** Elevation requires explicit charter rule.
- **Conflict disclosure mandatory.** No exceptions for prestige.
- **Limits and confidence required.** A claim without recorded
  limits is over-reach; the substrate must accept the limits.
- **Evidence references travel with the claim.** A claim without
  evidence is editorial; record it as `AdvisoryOpinion`, not
  `ExpertStatement`.
- **Anti-expertise refused.** A deliberation surface that cannot
  carry expert claims at all imports the same opacity it claims to
  refuse.

## 9. Deliberation context / educational references

### 9.1 What context is

A `DeliberationContext` is the typed bundle of references that says
*what shared understanding the deliberation assumes*. Members read
the context before forming their position; the context is part of
the institutional record.

Context families (see §4.2):

- prior decisions (`PriorDecisionReference`)
- charter rules (`CharterRuleReference`)
- CCL rules (`CCLRuleReference`)
- educational references (`LearningReference`)
- evidence references (`EvidenceReference`)
- counterarguments (`CounterargumentReference`)
- glossary entries (`GlossaryReference`)
- accessibility notes (`AccessibilityNote`)
- privacy notes (`PrivacyNote`)
- risk notes (`RiskNote`)
- generic context references (`ContextReference`)

### 9.2 Why context is institutional infrastructure

Educational and contextual references attach **to deliberation
objects, not to a generic resource pile**. The point is not a global
library; it is *what is at stake here*. A reference type that lives
free of any deliberation is a documentation artifact; a reference
that attaches to a deliberation entry, thread, or session is part of
institutional reasoning.

This is the participation half of the architecture due-diligence
checklist (`docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` §3.B)
becoming first-class on the deliberation surface. Members who lack
the assumed context are silently excluded from the deliberation. A
`DeliberationContext` makes the assumed context **explicit**, so
exclusion becomes visible and addressable.

### 9.3 Doctrinal rules

- **Context attaches to objects.** A free-floating "knowledge base"
  is documentation, not deliberation context.
- **Plain-language access required.** A `LearningReference` or
  `GlossaryReference` exists so members without insider vocabulary
  can participate.
- **Counterargument references are required by charter or by the
  facilitator** when a session's stakes warrant. Records that only
  cite supporting evidence are propaganda.
- **Privacy and accessibility notes are part of context, not
  retrofit.** They influence what evidence may be exported, what
  deliberation entries are visible to whom, and what UI must render.
- **References are pinned.** Pin to URN, repo path, content hash, or
  signed receipt — not to a hosted URL alone (see
  `ARCHITECTURE_DUE_DILIGENCE.md` §3.A).

## 10. Conflict disclosure and accountability

### 10.1 What conflict disclosure is

A `ConflictDisclosure` is a typed self- or third-party-recorded
disclosure of a conflict of interest, attached to one of:
`DeliberationEntry`, `ExpertStatement`, `AdvisoryOpinion`,
`DelegationGrant`, `RepresentationMandate`, `DecisionRecord`,
`OperatorExecutionAuthority`.

It carries:

- the actor whose conflict is disclosed
- the nature of the conflict (financial, familial, professional,
  prior-relationship, jurisdictional, identity-based, ideological-
  but-declared, other)
- the affected target
- the proposed mitigation (recusal, partial recusal, declared
  position, observer-only, none-required, other)
- the body that accepted the disclosure and the receipt of acceptance

### 10.2 Why this matters

Conflict disclosure is the price of speaking. Without a typed
shape, conflicts become whisper networks. The substrate must let an
institution record disclosures alongside claims, decisions, and
grants — and let downstream readers (auditors, federation peers,
challengers) replay the disclosure record without depending on
private conversations.

### 10.3 Doctrinal rules

- **Disclosure is paired, not separate.** A `ConflictDisclosure` is
  a relation, not a stand-alone log.
- **Mitigation is part of the disclosure.** A bare disclosure with
  no proposed mitigation is information; an institution must record
  what it intends to do.
- **Acceptance is receipted.** The body that accepted the disclosure
  must produce a receipt naming what was disclosed and what was
  accepted.
- **No retroactive erasure.** Disclosures that turn out to have been
  insufficient are addended, not erased; the original stays on the
  record.
- **Privacy-overlay binding optional.** Some disclosures must be
  generally visible; others are properly private to a designated
  body. Charter rules govern which is which.

## 11. Revocation, recall, challenge, and expiration

These four concepts are distinct surfaces with overlapping shape.

### 11.1 Revocation

`RevocationPath` declares how a `DelegationGrant` can be revoked —
by whom, with what receipt, under what notice. Revocation is an
authority-ending act by the original delegator (or by a body
charter-designated to revoke).

### 11.2 Recall

`RecallPath` declares how a `RepresentationMandate` can be ended
early — by which constituency, by which procedure, with what
notice, with what consequences for in-flight decisions. Recall is
the constituency's authority surface, not the institution's.

### 11.3 Challenge

`ChallengePath` declares that a record (decision, grant,
representative, indicator, expert claim, charter rule) is contestable
by a defined route — which body, which signal kind (per
[`#1631`](https://github.com/InterCooperative-Network/icn/issues/1631)),
which timeline, which evidence requirements. Challenge is the
substrate's anti-priesthood surface
([`#1633`](https://github.com/InterCooperative-Network/icn/issues/1633)
makes this concrete for indicators).

### 11.4 Expiration

Expiry is a property of grants, mandates, role assignments, and
charter rules. The substrate must support **silent inertness after
expiry** — the record exists, but it cannot be exercised. Renewal
requires a fresh record. Permanent grants are an anti-pattern this
brief refuses, consistent with the `TemporaryAuthorityGrant` doctrine
already recorded.

### 11.5 Doctrinal rules

- **Revocation, recall, and challenge are first-class records, not
  UI options.**
- **No record without a contest path.** If a record cannot be
  challenged, recalled, or revoked, name *why* in charter — and
  expect that exception to be challenged.
- **Receipts at every transition.** Issuance, exercise, renewal,
  revocation, recall, challenge open, challenge resolved.
- **Expiry is silent inertness.** Expired records are visible but
  inert.
- **Permanent emergency authority is the failure mode this layer
  refuses.**

## 12. Layer fit

The primitives respect the kernel/app boundary explicitly.

- **Kernel** stays meaning-blind. Kernel code never reads an
  `AuthorityBasis`, `DelegationGrant`, `RepresentationMandate`,
  `ExpertStatement`, `AdvisoryOpinion`, `ConflictDisclosure`,
  `FacilitatorSummary`, `StewardReview`, `OperatorExecutionAuthority`,
  `MinorityReport`, `ChallengePath`, `RevocationPath`, `RecallPath`,
  `DeliberationContext`, `ContextReference`, `LearningReference`,
  `EvidenceReference`, `PriorDecisionReference`,
  `CharterRuleReference`, `CCLRuleReference`, `AccessibilityNote`,
  `PrivacyNote`, `RiskNote`, `CounterargumentReference`, or
  `GlossaryReference` for its semantics. The kernel only sees
  `ConstraintSet` and `PolicyDecision`.
- **ICN platform / app layer** owns the generic primitive records,
  stores, lifecycle, and receipts. Apps implement them. Apps render
  them. Apps publish them. Apps materialize the binding between the
  primitives and the spine objects from `idea-0019`.
- **CCL / charters** govern valid creation, validity, scope, expiry,
  challenge, revocation, recall, disclosure, visibility, and
  transition rules, per §5.
- **Institution packages** provide local names, templates,
  explanatory copy, role labels, and domain vocabulary, per §5.
- **Nodes** store, sign, scope, and sync the records subject to
  visibility policy.
- **ActionCards** route attention. A `RecallPath` opening, a
  `ChallengePath` opening, a `DelegationGrant` approaching expiry,
  a `RepresentationMandate` reporting cadence elapsed — each is a
  legitimate emit-point under the existing `ADR-0027` contract.
  This brief does **not** propose a new ActionCard kind.
- **Receipts** prove transitions. Every primitive's lifecycle
  emits receipts under the existing `ADR-0026` envelope. This brief
  does **not** propose a new receipt envelope.
- **Evidence exports** redact according to charter and privacy
  notes, consistent with `urn:icn:contract:rehearsal-evidence-export:v1`.

The brief **does not erode the meaning firewall**. Apps and oracles
convert authority signals into constraints when constraints are
needed (rate limits on grant issuance, capability checks on
representative actions, gate-result encoding into `ConstraintSet`).

## 13. MVP / near-term vs Later

The primitives are named in full so the surface composes. Most are
**Later**.

### 13.1 MVP / near-term (read-model and binding only)

These integrate with already-merged or in-flight contracts and do
not require new substrate runtime work beyond what `idea-0019` and
`#1746` are already coordinating:

- **`AuthorityBasis` and `ParticipationRole` as read-model fields**
  on `DeliberationEntry` (`idea-0019` candidate) and `DecisionRecord`
  (`idea-0019` candidate). No persistence; no new schema; the spine's
  read-model dogfood (`ops/ideas/dogfood/institutional-process-substrate-mvp.md`)
  may extend its trace table to record them.
- **`FacilitatorSummary` typing of `idea-0019`'s
  `facilitator_summary` `DeliberationEntry` kind** as a read-model
  refinement only.
- **`DeliberationContext` sketch** as the surface a future runtime
  would attach to a `DeliberationThread`. A read-model fixture walk
  may demonstrate that the existing `EvidencePacket` and
  `PreviewReviewPacket` shapes can carry references without new
  contract.
- **`ConflictDisclosure` sketch** paired with one of the spine's
  `DeliberationEntry` examples in a future read-model dogfood.
- **`MinorityReport` sketch** as a `DeliberationEntry` kind candidate
  attached to a `DecisionRecord` in a future read-model dogfood.
- **`ChallengePath` / `RevocationPath` / `RecallPath` declarations**
  in charter examples; no runtime; no schema.

### 13.2 Later

These require runtime, additional governance work, additional CCL
work, or formal partnership decisions, and must not enter MVP scope:

- `DelegationGrant` runtime — the `TemporaryAuthorityGrant` umbrella
  ([`#1632`](https://github.com/InterCooperative-Network/icn/issues/1632))
  is the gating decision; this brief does not pre-empt it.
- `RepresentationMandate` runtime — gated on resolution of
  [`#1609`](https://github.com/InterCooperative-Network/icn/issues/1609)
  for federation tally semantics.
- `ExpertStatement` and `AdvisoryOpinion` runtime — gated on the
  signals umbrella ([`#1631`](https://github.com/InterCooperative-Network/icn/issues/1631))
  for the challenge-path semantics they share.
- `OperatorExecutionAuthority` runtime — gated on `idea-0019` runtime
  promotion (the `MutationPlan` and `ActivationRequest` runtime is the
  prerequisite).
- `ContextReference` runtime — gated on `RFC-0015` (public surface and
  learning repo architecture) for the cross-repo path semantics, and
  on `idea-0015` (accessibility runtime contracts per layer) for
  surface bindings.
- `ChallengePath` runtime — gated on the indicators umbrella
  ([`#1633`](https://github.com/InterCooperative-Network/icn/issues/1633))
  and the signals umbrella ([`#1631`](https://github.com/InterCooperative-Network/icn/issues/1631)).
- Federation-scope authority sync — gated on federation runtime work
  outside this brief.
- CCL syntax for any of the primitives — gated on `ADR-0023`,
  `idea-0012`, `idea-0018`.

The Later list is intentionally long. Each item is its own design
space and may produce its own framing brief, RFC candidate, or ADR
candidate. The brief names them so that downstream work knows where
to hang off; the brief does not commit to building them.

## 14. Explicit non-goals

This brief is **not**:

- runtime
- a schema
- an RFC by itself
- a voting-system decision
- a liquid-democracy commitment
- expertocracy
- anti-expertise
- representative government by default
- direct democracy by default
- chat
- social media
- a moderation platform
- an identity directory implementation
- a credential verification implementation
- a private-overlay implementation
- NYCN-specific
- a production-readiness claim
- a Phase 2 completion claim
- a formal NYCN pilot authorization
- a live federation claim
- a live Google Drive / Groups / Sheets sync claim
- a K3s / DNS / Forgejo mutation claim
- a private-data-handling claim
- a re-decoration of economic vocabulary; the brief uses
  *obligation*, *allocation*, *settlement*, *unit*, *position*,
  *receipt*, *provenance*, and *evidence* and avoids ICN-native
  *payment*, *currency*, *balance*, and *wallet* framing
- a new ActionCard contract (the existing `ADR-0027` contract carries
  the triggers any of these primitives would emit)
- a new receipt envelope (the existing `ADR-0026` envelope carries
  the receipt classes any of these primitives would produce)
- a binding on partner repositories or partner data shape
- an override of `idea-0019`'s open questions Q1, Q3, Q4

## 15. Open questions

1. Is `AuthorityBasis` one polymorphic handle, or a small typed
   family keyed by basis kind (standing-grade role, moment-grade
   grant, mandate, charter rule, delegation, representation, expert
   advisory posture, facilitator process authority, steward/operator
   execution authority, member-as-self)? The polymorphic option
   composes more cheaply with `idea-0019`'s `DeliberationEntry`; the
   typed-family option enforces stronger validation and mirrors the
   pattern used for `idea-0019` Q1 (`ProcessTargetRef` polymorphism).
2. Are `DelegationGrant` and `TemporaryAuthorityGrant` (`#1632`) one
   primitive or two? If one, the runtime work is gated on `#1632`. If
   two, the primitives compose: every `DelegationGrant` may reference
   a `TemporaryAuthorityGrant` for the underlying authority being
   delegated.
3. Does `RepresentationMandate` belong in `icn-governance` (alongside
   `RoleAssignment`, `Meeting`, `Program`, `ActionItem`) or in a new
   crate (`icn-representation` / `icn-mandate`)? Placement is a
   future ADR; this brief does not decide.
4. How does `ExpertStatement` relate to the `Indicator`'s
   `challenge_path` ([`#1633`](https://github.com/InterCooperative-Network/icn/issues/1633))?
   An expert claim is upstream of an indicator's caveats; a challenge
   on an indicator may produce an expert statement. The relationship
   needs runtime evidence before it is committed.
5. Are `ConflictDisclosure` and `MinorityReport` `DeliberationEntry`
   kinds (per `idea-0019`'s closed-taxonomy candidates) or distinct
   primitives that *attach to* a `DeliberationEntry`? Either choice
   composes; the substrate must pick one before runtime.
6. Does `OperatorExecutionAuthority` extend the existing
   `RoleAssignment.authority_scope` (with provenance discriminating
   execution-scoped roles from democratic-decision-scoped roles), or
   does it sit alongside as a distinct authority surface? The
   answer affects how `ActivationRequest` (idea-0019) finds its
   authority.
7. What is the smallest viable shape that `ChallengePath`,
   `RevocationPath`, and `RecallPath` share? They differ in actor
   (delegator vs constituency vs any-member), target (grant vs
   mandate vs decision/indicator/etc.), and consequence; they share
   "typed declaration of a contest path with required evidence and
   timeline."
8. How do federation-scope `RepresentationMandate`s reconcile when a
   constituency spans multiple cooperatives or includes
   members-of-members? `#1609` is the upstream gate; this question
   is named here so the brief acknowledges it.
9. What is the smallest viable accessibility mode for a
   `DeliberationContext` rendering — plain-language gloss, screen-
   reader path, low-bandwidth path, captioned-media path — and does
   that mode belong on the context or on each reference?
10. Does `LearningReference` couple ICN core to icn-learn
    (`RFC-0015`) at the schema level, or only at the URN/path level?
    Schema coupling is too tight; URN coupling is the safer default.

## 16. Promotion gate

This brief is pre-RFC framing. Promotion of `idea-0020` to RFC
candidate requires evidence beyond the brief.

### 16.1 What evidence would justify RFC promotion

1. **Read-model dogfood slice composition with `idea-0019`.** A
   read-model fixture walk that takes the spine's
   `DeliberationEntry` / `DecisionRecord` / `ActivationRequest`
   shapes and demonstrates that `AuthorityBasis`,
   `ParticipationRole`, and at least one `DeliberationContext`
   reference type compose against them without modifying any
   shipping contract URN. Per
   [`ops/ideas/README.md`](../README.md) § "Dogfood slice variants",
   a read-model fixture walk does **not** satisfy receipt-backed
   promotion thresholds — but it can demonstrate spine composition.
   This brief does **not** start that slice; it remains a separate
   future artifact.
2. **Runtime dogfood.** A second slice that runs against a real or
   fixture-equivalent gateway and emits at least one receipt under
   the existing `ADR-0026` envelope for one of the named primitives
   — preferably a `ConflictDisclosure` accept receipt or a
   `MinorityReport` recorded receipt, since both compose with
   `idea-0019` shapes already named. Receipt class names are
   candidates only until then. Producing any one as a real receipt
   would be sufficient evidence; all are not required.
3. **Visibility / privacy boundary exercised.** A run that walks a
   `ConflictDisclosure` or `DeliberationContext` reference visible
   to body A but not to body B, with redaction in evidence export.
4. **Accessibility-gate `ProcessGateResult` produced** through
   `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real
   surface that renders any of these primitives (e.g. a
   `DeliberationContext` viewer).
5. **One open question concretely answered.** Resolving Q1
   (`AuthorityBasis` polymorphism vs typed family) or Q5
   (`ConflictDisclosure` and `MinorityReport` placement) is the
   threshold the RFC candidate registry uses.

### 16.2 What would justify a future schema PR

A schema PR for any primitive in this brief would require:

- The runtime dogfood evidence above for that primitive class.
- A second, independent walk that exercises the schema across at
  least two unrelated institution types (cooperative, community,
  federation, mutual-aid network, land trust, association).
- A `docs/contracts/schema-id-audit.md` row entry showing the audit
  table convention extended to the new schema.
- A non-DNS URN per the schema-id-audit's rule
  (`urn:icn:contract:<short-name>:v1`).

A schema PR before runtime evidence would re-create the
"named-but-empty objects" risk listed in `idea-0010`.

### 16.3 What would justify runtime work later

Runtime work on these primitives would be justified when:

- At least three institution types (cooperative, community assembly,
  federation chamber) have walked a paper slice that exercises one
  primitive cluster (authority cluster, context cluster, or both),
  and all three compose against the same shape.
- A runtime dogfood has produced at least one receipt class for at
  least one primitive, under the existing `ADR-0026` envelope.
- The brief's open questions Q1, Q5, and Q7 have been resolved or
  explicitly deferred in writing.
- A runtime dogfood has demonstrated the visibility/privacy
  boundary on a real surface.

Until those conditions are met, the primitives remain a paper
surface that institutions can author against in CCL, charters, and
packages without any ICN code change.

## 17. Follow-ups

- A separate **read-model dogfood slice** that composes
  `AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`,
  `ConflictDisclosure`, `MinorityReport`, and a small
  `DeliberationContext` against the existing `idea-0019` read-model
  dogfood. Pre-RFC, no schema, no URN, no implementation.
- A separate **CCL hook-point catalog** (a future framing brief or
  RFC candidate) listing the exact transitions where CCL governs
  primitive lifecycle. Not started in this brief.
- A separate **expert/advisory framing brief** that decomposes
  expert input across institution types beyond cooperatives —
  community assemblies, federations, mutual-aid networks — to test
  whether the primitive shape generalizes. Not started.
- A separate **conflict object model framing brief** that connects
  `ConflictDisclosure` to `idea-0016` (institutional conflict object
  model, gated on `ADR-0029`). Not started.
- A separate **federation tally semantics framing brief** that
  composes `RepresentationMandate` with `#1609`'s eligibility and
  tally questions. Not started.
- A separate **delegation runtime framing brief** when
  [`#1632`](https://github.com/InterCooperative-Network/icn/issues/1632)
  (`TemporaryAuthorityGrant` umbrella) reaches its RFC step.
- An eventual **schema-id-audit row** when any primitive promotes to
  a schema PR. Not started; gated on schema PR justification per §16.2.

No new implementation issues are opened from this brief. No runtime
dogfood is started in this session. No partner repository is
modified by this brief.

## Boundary check

- Belongs in the ICN idea refinery for now.
- ICN core / app substrate, generic. Not NYCN-specific. Not partner-
  specific. Not institution-specific.
- Not an RFC or ADR yet.
- Not a website claim. Not an icn-learn packet.
- No private operational data in the brief or in the eventual
  artifacts.
- Drive / Sheets / Groups / private overlays are not addressed by
  this brief; they are bounded by the visibility / privacy /
  accessibility primitives (`PrivacyNote`, `AccessibilityNote`) and
  by `#1730`.

## Existing surface

What already exists in the repo that this brief touches (cited only,
not modified):

- `docs/architecture/INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md`
  — doctrine for `InstitutionalSignal`, `MemberSignal`,
  `TemporaryAuthorityGrant`, `Indicator`, and the support primitives
  this brief composes with.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — the boundary
  rule the primitives respect; in particular the §F enumeration of
  what ICN core may know vs must not know about institutional
  feedback / support, which this brief extends to authority and
  context primitives.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — the meaning firewall
  rule the primitives respect.
- `docs/architecture/MEMBER_STANDING.md` — the `/me/standing` design
  contract that downstream surfaces of these primitives compose with.
- `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` — the
  authority-vs-convenience and participation-access reflexes the
  primitives must respect; this brief extends the participation half
  to the deliberation surface.
- `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md` — the
  receipt envelope every primitive's transitions emit under.
- `docs/adr/ADR-0027-action-card-contract.md` — the action-card
  contract each primitive's attention-routing emits under.
- `docs/adr/ADR-0028-accessibility-baseline-for-member-interfaces.md`
  (`proposed`) — the accessibility floor any UI surface for these
  primitives must meet.
- `docs/adr/ADR-0029-conflict-resolution-object-model.md`
  (`proposed/partial`) — the conflict path that `ConflictDisclosure`,
  `MinorityReport`, and `ChallengePath` defer to for dispute routing.
- `docs/contracts/preview-review.md` and
  `docs/contracts/preview-review.schema.json` —
  `urn:icn:contract:preview-review:v1` (PR #1745).
- `docs/contracts/rehearsal-evidence-export.md` and
  `docs/contracts/rehearsal-evidence-export.schema.json` —
  `urn:icn:contract:rehearsal-evidence-export:v1`.
- `docs/contracts/schema-id-audit.md` — the audit table any future
  primitive schema would join.
- `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` — the PR-time
  gate every member-facing surface of these primitives must satisfy.
- `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md` — the
  organizer-facing path the primitives sit underneath.
- `ops/ideas/framing/institutional-process-substrate.md` — the
  spine these primitives compose with (`idea-0019`).
- `ops/ideas/dogfood/institutional-process-substrate-mvp.md` — the
  read-model fixture walk the next composition slice would extend.
- `ops/coordination/rfc_candidates.yaml` — pending design spaces the
  primitives intersect.
- `ops/coordination/adr_candidates.yaml` — likely future decisions
  the primitives touch.
- Idea cards: `idea-0010` (cooperative-domain-infrastructure cluster),
  `idea-0012` (CCL institutional process language runtime),
  `idea-0014` (EffectRecord canonical schema),
  `idea-0015` (accessibility runtime contracts per layer),
  `idea-0016` (institutional conflict object model),
  `idea-0017` (cooperative bylaws primitive scan),
  `idea-0018` (CCL institutional rule authoring),
  `idea-0019` (Institutional Process Substrate).
- Issues:
  [`#1748`](https://github.com/InterCooperative-Network/icn/issues/1748)
  (process substrate coordination milestone),
  [`#1746`](https://github.com/InterCooperative-Network/icn/issues/1746)
  (showcase milestone — organizer rehearsal operability),
  [`#1609`](https://github.com/InterCooperative-Network/icn/issues/1609)
  (federation tally semantics),
  [`#1632`](https://github.com/InterCooperative-Network/icn/issues/1632)
  (`TemporaryAuthorityGrant` umbrella),
  [`#1631`](https://github.com/InterCooperative-Network/icn/issues/1631)
  (institutional signals umbrella),
  [`#1633`](https://github.com/InterCooperative-Network/icn/issues/1633)
  (governed indicators umbrella),
  [`#1646`](https://github.com/InterCooperative-Network/icn/issues/1646)
  (action-card source-path umbrella).

This brief does not mutate any of those documents.

## Privacy and boundary risks

- **Authority records are sensitive.** A `DelegationGrant`,
  `RepresentationMandate`, `ConflictDisclosure`, or
  `MinorityReport` may carry institutional context that some bodies
  must see and others must not. Default visibility is conservative;
  broader visibility requires explicit charter rule.
- **Conflict disclosures must not become weapons.** Disclosure is
  the price of speaking, not a public hit list. Charter governs
  visibility; the substrate must support body-scoped visibility
  with redaction in evidence export.
- **Educational references can leak institutional identity.** A
  `LearningReference` to a partner-internal explainer is not
  repo-safe; the substrate must support pinning to URN, repo path,
  content hash, or signed receipt — not to a hosted URL alone.
- **Anonymous-but-authenticated mode is reserved.** Several
  primitives — `MemberSignal` (per
  `INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md` §3),
  `ConflictDisclosure`, `MinorityReport` — eventually need an
  anonymous-but-authenticated authorship mode that proves authorship
  to a designated body without public attribution. Out of scope for
  this brief; design space reserved.
- **Federation visibility leaks.** Cross-cooperative federation
  sync can leak who-acted-on-whose-mandate across institutions if
  visibility defaults are permissive. Default conservative.
- **Named-but-empty objects.** The same risk listed in `idea-0010`
  applies here. Naming primitives in this brief is **not** a backlog
  commitment to build them. Promotion review must enforce that.

## Proposed next artifact

Pick exactly one (this brief picks one):

- [ ] another framing brief (decompose first)
- [ ] source review
- [x] dogfood slice — author a read-model fixture walk that composes
      `AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`,
      `ConflictDisclosure`, `MinorityReport`, and a small
      `DeliberationContext` against the already-merged `idea-0019`
      read-model fixture walk. Pre-RFC. No schema. No URN. No
      runtime. Not in scope of this PR.
- [ ] promotion review → RFC candidate
- [ ] promotion review → ADR candidate
- [ ] promotion review → GitHub issue
- [ ] promotion review → NYCN package task
- [ ] promotion review → icn-learn packet
- [ ] promotion review → website claim
- [ ] park
- [ ] reject

A coordination milestone issue may **not** be opened from this
brief. The primitives compose with `idea-0019`'s milestone (#1748)
and with the showcase milestone (#1746); a separate milestone would
duplicate them.

Do not promote to RFC until at least one open question is answered
and a read-model dogfood slice has produced composition evidence.
Do not start runtime dogfood in this session.

## Receipts / evidence (if relevant)

Eventually, an authority/context primitive layer will need:

- A worked composition slice end-to-end against a fixture: a
  `DeliberationEntry` carrying `AuthorityBasis` and
  `ParticipationRole`, a `FacilitatorSummary` distinguishing process
  authority from outcome authority, a `ConflictDisclosure` paired
  with an `ExpertStatement`, a `MinorityReport` attached to a
  `DecisionRecord`, a `DeliberationContext` of plausible references
  attached to a `DeliberationThread`, all under the same
  `ProcessSession` from `idea-0019`. Receipts emitted under the
  existing envelope. Evidence exported repo-safe.
- Evidence that the primitives respect the meaning firewall:
  kernel-side audit that no primitive is read for its semantics by
  kernel code; oracle-side conversion of primitive signals into
  generic constraints.
- Evidence that visibility policy works: a `ConflictDisclosure`
  visible to body A but not to body B, redacted accordingly in the
  evidence export.
- Evidence that the accessibility gate produces a real
  `ProcessGateResult` for a primitive's UI surface.
- Evidence that revocation, recall, and challenge work end-to-end:
  a `DelegationGrant` revoked, a `RepresentationMandate` recalled,
  a `DecisionRecord` challenged, each producing the appropriate
  receipts.

None of this evidence exists today. Producing it is downstream of
later promotion reviews, not of this framing brief.

## Coda

ICN should not prescribe one democratic model. It should provide
the primitives by which institutions define, constrain, prove, and
revise their own democratic authority structures through CCL,
charters, and institution packages.

Democratic deliberation does not only require a place to speak. It
requires a shared context layer that lets people understand what
they are being asked to decide.

Democracy does not mean every statement enters the process naked
and equal. It means every statement enters with its authority,
limits, evidence, and accountability visible.

Experts inform power. They do not automatically become power.
Delegation must not become quiet aristocracy. Representation must
not become unaccountable proxy sovereignty.

Authority must always carry its basis.
