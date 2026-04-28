# Institutional rule authoring through CCL — framing brief

**Idea cards:** [idea-0017](../ideas.yaml), [idea-0018](../ideas.yaml)
**Date:** 2026-04-28
**Status:** pre-RFC / pre-ADR framing. Not a design doc. Not a decision.
Not a schema commitment.

## What this is

ICN should allow cooperatives, communities, federations, and other
democratic institutions to encode customized structures, processes,
and needs through CCL. CCL composes ICN primitives into
institution-specific bylaws, policies, workflows, constraints, and
receipt requirements.

This brief uses an anonymized bylaws specimen reviewed out-of-repo
only as primitive-discovery input. It does not name the cooperative,
does not commit the source document, and does not treat the specimen
as a universal cooperative model. A later multi-source research pass
is the right venue for generalization across cooperative types.

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
does not define any layer below it.

## Primitive families surfaced by the specimen

The specimen surfaced recurring institutional dimensions. Categories
only — no clauses quoted, no source named:

- **Membership / standing.** Eligibility, admission, classes, lapse,
  reinstatement, termination. ICN may need to represent membership
  state with auditable transitions and standing predicates.
- **Member classes / tiers.** Voting members, non-voting members,
  affiliate or associate members, candidate / probationary members,
  honorary members. ICN may need to represent typed member classes
  with per-class rights and obligations.
- **Governance rights.** Voting, nomination, recall, candidacy,
  petition. ICN may need to represent rights as composable predicates
  attached to membership classes, not hardcoded.
- **Notice / quorum / proxy.** Notice periods, quorum thresholds,
  proxy rules, virtual-meeting rules. ICN may need to represent these
  as parameterized rules per meeting type, not as global constants.
- **Board / council structure.** Seat counts, term lengths, election
  cadence, vacancies, classes of seats. ICN may need to represent seat
  inventories with election windows and succession rules.
- **Role / officer authority.** Authority granted to officers,
  scope-limits, signing authority, delegation. ICN may need to
  represent authority grants as scoped, time-bounded, revocable
  capabilities.
- **Due process / removal / termination.** Notice, hearing, appeal,
  remedy. ICN may need to represent process flows where
  human-discretion steps are first-class, not approximated as if-else.
- **Cooperative economics.** Member equity, share classes, dues,
  fees, surplus rules, dividends. ICN may need to represent economic
  positions distinctly from generic ledger balances.
- **Equity lifecycle.** Issuance, holding rules, redemption,
  transferability, forfeiture. ICN may need to represent equity-class
  state machines with redemption windows and waterfall rules.
- **Patronage / allocation / reserves.** Patronage measurement,
  allocation rules, retained vs distributed shares, reserves. ICN may
  need to represent patronage as a separate accounting class with its
  own policy parameters.
- **Amendment lifecycle.** Proposal, notice, deliberation, vote,
  ratification, effective date. ICN may need to represent the
  bylaws-amending-themselves loop with explicit version transitions.
- **Dissolution / liquidation waterfall.** Trigger, asset
  resolution, member-equity treatment, residual disposition. ICN may
  need to represent dissolution paths even though they are rarely
  exercised.
- **Records / minutes / notices / receipts.** What must be kept,
  who may inspect, what must be produced on demand. ICN may need to
  represent record-only obligations distinctly from enforceable
  rules.

These are primitive families, not commitments. None of them is a
schema yet.

## Variation across institutions

This is not consumer-coop-only and is not US-only. Different
institution types bind the primitive families differently:

- consumer cooperatives
- worker cooperatives
- housing cooperatives
- producer cooperatives
- platform cooperatives
- purchasing cooperatives
- communities / mutual aid networks
- land trusts
- federations
- multi-stakeholder cooperatives

Examples of variation, kept generic:

- A consumer co-op may use purchase-based patronage measurement.
- A worker co-op may use labor contribution and worker-member
  candidacy with probationary periods.
- A housing co-op may use occupancy rights and maintenance
  obligations as governance-relevant standing inputs.
- A producer co-op may use supply commitments and quality grades as
  membership obligations.
- A platform co-op may bind member status to platform participation
  and revenue share rather than equity.
- A purchasing co-op may bind voting weight to volume bands.
- A federation may use member organizations, delegate voting, dues,
  and inter-coop agreements rather than individual members.
- A multi-stakeholder cooperative may use multiple member classes
  with different rights composed into a single governance body.

A useful institutional-rule layer must accommodate all of these
without forcing one shape.

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
      cooperative bylaws, feeding back into this brief and into
      `idea-0017`
- [ ] dogfood slice
- [ ] promotion review → RFC candidate
- [ ] promotion review → ADR candidate
- [ ] promotion review → GitHub issue
- [ ] promotion review → NYCN package task
- [ ] promotion review → icn-learn packet
- [ ] promotion review → website claim
- [ ] park
- [ ] reject

A source-review template instance covering multiple bylaws specimens
(public and license-clean only) is acceptable as the next artifact
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
