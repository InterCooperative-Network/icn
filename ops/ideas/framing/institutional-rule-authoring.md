# Institutional rule authoring through CCL — framing brief

**Idea cards:** `ops/ideas/ideas.yaml` (`idea-0017`, `idea-0018`)
**Author / session:** 2026-04-28 session
**Date:** 2026-04-28
**Status:** pre-RFC / pre-ADR framing. Not a design doc. Not a decision.
Not a schema commitment.

> **Seed-brief discipline.** This is a seed framing brief. If future
> passes add detailed primitive inventories, cross-institution research
> findings, or rule-language capability maps, split those into separate
> source-review or framing artifacts rather than letting this brief
> become a design doc.

## What this is

ICN should allow cooperatives, communities, federations, and other
democratic institutions to encode customized structures, processes,
and needs through CCL. CCL composes ICN primitives into
institution-specific bylaws, charters, constitutions, policies,
agreements, workflows, constraints, and receipt requirements.

> **Co-ops are one specimen class. ICN is for institutions.** CCL is
> the rule-authoring layer for whatever governing text that institution
> actually uses: bylaws, charters, constitutions, policies, compacts,
> rules, governance manuals, or federation agreements.

This brief uses an anonymized cooperative bylaws specimen reviewed
out-of-repo as **one early primitive-discovery input**. It does not
imply that cooperatives are the only target institution type, and it
does not treat bylaws as the only governing-document form. Later
research should compare public bylaws, charters, constitutions, rules,
policies, agreements, compacts, and governance manuals across
cooperatives, communities, federations, land trusts, associations,
mutual-aid networks, and other democratic institutions.

## Why this matters

Bylaws are institutional operating systems written in legal prose. If
bylaws remain PDFs beside the system, ICN cannot prove authority,
process, or accountability — the runtime will diverge from what the
institution has actually agreed to. If bylaws are over-automated, the
system pretends legal and human judgment are code, which they are not.

The right path is structured encoding with explicit classes:
enforceable, checkable, discretionary, record-only, external-law,
private-overlay, not-encoded. Each clause is mapped to one of these,
not coerced into all-executable.

## Core thesis

> ICN should ship primitives. CCL should let institutions compose
> those primitives into their own bylaws, policies, workflows,
> constraints, and receipts.

- ICN primitives are generic.
- CCL binds them into rules.
- Institution packages customize them per institution.
- Runtime emits receipts and provenance when actions occur.

The intended stack:

```text
human bylaws / charter / policies
    │
    ▼
institution package
    │
    ▼
CCL institutional rules
    │
    ▼
ICN primitives
    │
    ▼
runtime state + receipts
```

A bylaws specimen feeds the top of this stack as source material; it
does not define any layer below it. The same is true for charters,
constitutions, compacts, federation agreements, and any other
governing-document form.

## Governing-document forms

Different institutions express their rules through different
documents. ICN's rule-authoring layer should not care what the human
document is called; it should identify the institutional rules,
boundaries, authority, evidence requirements, and state transitions
that the document enacts.

Forms encountered in practice include:

- bylaws
- charters
- constitutions
- rules
- policies
- member agreements
- operating agreements
- community compacts / covenants
- federation agreements
- inter-institutional agreements
- governance manuals
- assembly procedures
- stewardship rules
- shared-resource rules

Mapped to institution type:

- **Cooperatives** often present this as bylaws, rules, policies, and
  member agreements.
- **Communities** may present this as charters, compacts, assembly
  rules, access rules, shared-resource rules, care obligations, or
  local governance procedures.
- **Federations** may present this as constitution-level governance:
  member institutions, delegates, councils or chambers, recognition
  rules, dues, jurisdiction, amendments, exit, and dissolution.

The rule-authoring layer must accept that the source text is
heterogeneous and must avoid privileging any single form.

## Primitive families surfaced by the specimen and broader institution frame

The cooperative bylaws specimen surfaced recurring institutional
dimensions. The categories below merge those with dimensions visible
in community charters and federation constitutions, so the family list
does not collapse to one institution type. Categories only — no
clauses quoted, no source named:

- **Membership / participation / standing.** Eligibility, admission,
  classes, lapse, reinstatement, termination. ICN may need to
  represent membership and participation state with auditable
  transitions and standing predicates.
- **Member or participant classes.** Voting / non-voting members,
  affiliate or associate members, candidate or probationary members,
  honorary members, member institutions (federation case), residents
  vs guests (community case). ICN may need to represent typed classes
  with per-class rights and obligations.
- **Governance rights.** Voting, nomination, recall, candidacy,
  petition, delegate selection. ICN may need to represent rights as
  composable predicates attached to classes, not hardcoded.
- **Notice / quorum / proxy / remote participation.** Notice periods,
  quorum thresholds, proxy rules, virtual / hybrid participation
  rules. ICN may need to represent these as parameterized rules per
  meeting type, not as global constants.
- **Assemblies / meetings / councils / chambers.** Convening rules,
  agendas, decision modes (consensus / majority / supermajority),
  delegate vs direct chambers. ICN may need to represent multiple
  decision-making bodies under one institution.
- **Board / council / committee / circle structure.** Seat counts,
  term lengths, election cadence, vacancies, classes of seats. ICN
  may need to represent seat inventories with election windows and
  succession rules.
- **Role / officer / delegate authority.** Authority granted to
  officers, delegates, or stewards; scope-limits; signing authority;
  delegation. ICN may need to represent authority grants as scoped,
  time-bounded, revocable capabilities.
- **Due process / removal / termination / remedy.** Notice, hearing,
  appeal, remedy. ICN may need to represent process flows where
  human-discretion steps are first-class, not approximated as
  if-else.
- **Shared-resource rules.** Access, allocation, stewardship,
  maintenance obligations, depletion limits. Especially relevant to
  communities, land trusts, and commons-managing institutions.
- **Cooperative or institutional economics.** Member equity, share
  classes, dues, fees, surplus rules, dividends, federation dues. ICN
  may need to represent economic positions distinctly from generic
  ledger balances.
- **Dues / allocations / reserves / obligations.** Recurring
  obligations on members or member institutions, allocations of
  surplus, mandatory reserves.
- **Equity / patronage where applicable.** Issuance, holding rules,
  redemption, transferability, forfeiture; patronage measurement and
  allocation. Communities may not need this family at all; producer
  and consumer co-ops need it strongly; federations may instead use
  dues and capital contributions from member institutions.
- **Amendment / ratification lifecycle.** Proposal, notice,
  deliberation, vote, ratification, effective date. ICN may need to
  represent the rules-amending-themselves loop with explicit version
  transitions.
- **Exit / withdrawal / dissolution.** Trigger, asset resolution,
  member-equity treatment, residual disposition; member-institution
  withdrawal in federations. ICN may need to represent these paths
  even though they are rarely exercised.
- **Records / minutes / notices / receipts.** What must be kept,
  who may inspect, what must be produced on demand. ICN may need to
  represent record-only obligations distinctly from enforceable
  rules.
- **Federation / inter-institutional agreements.** Recognition,
  membership in a federation, treaties between cooperatives or
  federations, shared services, cost-sharing.
- **Jurisdiction / recognition / dispute routing.** Which body decides
  which question; which institution recognizes which other; how
  disputes traverse institutional boundaries.

These are primitive families, not commitments. None of them is a
schema yet, and not every institution needs every family.

## Variation across institutions

This is not consumer-coop-only and is not US-only. Different
institution types bind the primitive families differently:

- consumer cooperatives
- worker cooperatives
- housing cooperatives
- producer cooperatives
- platform cooperatives
- purchasing cooperatives
- multi-stakeholder cooperatives
- communities / neighborhood assemblies / mutual-aid networks
- land trusts / community land institutions
- associations / clubs with democratic governance
- federations / secondary cooperatives / networks of organizations

Examples of variation, kept generic:

- A consumer co-op may use purchase-based patronage measurement.
- A worker co-op may use labor contribution and worker-member
  candidacy with probationary periods.
- A housing co-op may care about occupancy rights and maintenance
  obligations as governance-relevant standing inputs.
- A producer co-op may use supply commitments and quality grades as
  membership obligations.
- A platform co-op may bind member status to platform participation
  and revenue share rather than equity.
- A purchasing co-op may bind voting weight to volume bands.
- A multi-stakeholder cooperative may use multiple member classes
  with different rights composed into a single governance body.
- A community might not need patronage or equity primitives at all;
  it may instead need assembly rules, access rules, shared-resource
  rules, care obligations, and local governance procedures.
- A land trust may bind decision rights to stewardship obligations
  rather than membership tenure.
- An association or club with democratic governance may need only a
  thin slice (membership, meetings, officers, dues, amendments).
- A federation might not need individual consumer-member rules; it
  may instead need member institutions, delegates, councils or
  chambers, recognition, dues, jurisdiction, and inter-institutional
  agreements at constitution-level complexity.

The takeaway: ICN should provide primitive families. CCL should let
each institution compose only what it needs. A useful
institutional-rule layer must accommodate all of these without
forcing one shape and without requiring institutions to adopt
primitives that do not apply to them.

## Encoding classes

Not every bylaw clause becomes executable code. Each clause should be
explicitly classed:

- `machine_enforceable` — runtime can refuse the action that violates
  the rule (example: vote outside open polling window).
- `machine_checkable` — runtime can verify after the fact and produce
  a finding, but does not block (example: notice given on time).
- `human_discretion` — the rule grants a human or body the authority
  to decide; runtime records the decision but does not make it
  (example: termination for cause).
- `record_only` — the rule mandates a record, not an action (example:
  meeting minutes retained N years).
- `external_law_reference` — the rule defers to statute, regulator,
  or contract; runtime does not interpret it (example: state cooperative
  statute compliance).
- `private_overlay_required` — the rule's implementation requires
  data that must not enter public Git (example: member contact
  information).
- `not_encoded` — the rule is intentionally left in prose and not
  represented in CCL.

Encoding classes are part of the rule authoring surface, not a
post-hoc audit. Authors choose them up front.

## Boundary check

- Belongs in the ICN idea refinery for now.
- Not NYCN-specific. NYCN may eventually be one of many institution
  packages built on this surface, but the framing must remain generic.
- Not a public website claim.
- Not icn-learn material yet. Teaching follows canonical framing.
- Not an RFC or ADR yet.
- No runtime claims. Nothing here is shipping.
- The bylaws specimen stays out of repo and unnamed.
- A later multi-source research pass may compare public cooperative
  bylaws across cooperative types and feed back into this brief.

## Existing surface

What already exists in the repo that this idea touches:

- Idea refinery: `ops/ideas/`, including `idea-0017` (bylaws primitive
  scan) and `idea-0018` (this layer). Promotion thresholds in
  `ops/ideas/README.md`.
- CCL direction: `docs/adr/ADR-0023-ccl-institutional-process-language.md`
  (`proposed`); `idea-0012` is the runtime-details framing.
- Institution package boundary: `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`.
- Manifest direction: `docs/adr/ADR-0024-institution-package-manifest-schema.md`
  (`proposed/partial`); `idea-0013` is the per-section framing.
- Effect record direction: `docs/adr/ADR-0025-institutional-effect-record-canonical-schema.md`
  (`proposed`); `idea-0014` is the framing.
- Conflict object model direction: `docs/adr/ADR-0029-conflict-resolution-object-model.md`
  (`proposed/partial`); `idea-0016` is the framing.
- Action-card proof loop: shipping for `proposal/vote`,
  `action_item/complete`, `meeting/attend` (per ADR-0027
  `implementation_status`). Cited here only as an example that runtime
  state and receipts already exist for some institutional actions, not
  as the whole rule authoring model.

This brief does not mutate any of those documents.

## Open questions

1. What is the minimum CCL surface needed to express institutional
   rules across the institution types listed above?
2. Which primitives belong in ICN runtime versus institution package
   YAML versus CCL rule code?
3. How does CCL distinguish `machine_enforceable` from
   `machine_checkable` so authors do not accidentally over-promise
   enforcement?
4. How are `human_discretion` decisions recorded with sufficient
   provenance without being coerced into automation?
5. How should amendments version the institution's active rule set,
   and how do in-flight processes inherit (or not) the new rules?
6. How do private overlays bind sensitive member, finance, and
   personal data without entering public Git?
7. How should a future research pass compare bylaws across cooperative
   types without importing private or copyrighted material wholesale?

## Privacy and boundary risks

- The bylaws specimen contained organization-identifying language;
  none of it is reproduced here. The source remains anonymous and
  out-of-repo.
- Member, equity, and patronage clauses imply sensitive data classes
  (PII, financial position) that must stay in private overlay.
- The temptation to absorb one cooperative's clause shape as ICN
  doctrine is the boundary risk this brief exists to manage.

## Proposed next artifact

Pick exactly one (this brief picks one):

- [ ] another framing brief (decompose first)
- [x] later multi-source research / source-review pass across public
      institutional governing documents — bylaws, charters,
      constitutions, rules, policies, agreements, compacts, governance
      manuals, federation agreements — covering cooperatives,
      communities, federations, land trusts, associations, mutual-aid
      networks, and other democratic institutions, feeding back into
      this brief and into `idea-0017`
- [ ] dogfood slice
- [ ] promotion review → RFC candidate
- [ ] promotion review → ADR candidate
- [ ] promotion review → GitHub issue
- [ ] promotion review → NYCN package task
- [ ] promotion review → icn-learn packet
- [ ] promotion review → website claim
- [ ] park
- [ ] reject

A source-review template instance covering multiple public,
license-clean institutional governing documents — including but not
limited to cooperative bylaws — is acceptable as the next artifact
shape.

Do not promote to RFC yet.

## Non-goals

- Do not create a consumer-coop schema. Or any single-type schema.
- Do not name or commit the bylaws specimen.
- Do not implement CCL syntax in this brief.
- Do not modify runtime.
- Do not change any ADR or RFC status.
- Do not create public-website claims.
- Do not assume all institutions need the same primitives.
- Do not pretend legal, accounting, or human judgment is executable
  code.
- Do not bundle this brief with `idea-0012`, `idea-0013`, `idea-0014`,
  `idea-0015`, or `idea-0016` — those framings stand on their own and
  may produce different decisions.

## Receipts / evidence (if relevant)

Eventually, an institutional rule authoring surface will need:

- A worked example: one institution package whose CCL rules cover at
  least one primitive family from each section above (membership,
  authority, meeting, economics, amendment, dissolution).
- Evidence that `machine_enforceable` rules actually refuse violating
  actions in runtime, with receipts.
- Evidence that `human_discretion` decisions produce auditable
  records that name the deciding body and the legal basis without
  pretending the decision was automated.
- Evidence that an amendment can ratify a new rule version and that
  the runtime correctly applies the new version going forward.

None of this evidence exists today. Producing it is downstream of
later promotion reviews, not of this framing brief.
