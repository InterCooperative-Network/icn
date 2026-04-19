---
id: "0014"
title: "Constitutional Object Model — AuthorityClass, AuthorityGrant, TypedScope, Mandate"
status: "proposed"
date: "2026-04-19"
context: "post-SDIS-evidence arc (#1571); post-institution-package-boundary (#1560); pre-executor-gating; pre-federation-authority-narrowing"
deciders: ["Matt Faherty"]
tags: ["governance", "architecture", "authority", "charter", "federation", "mandate", "meaning-firewall", "semantic-freeze"]
supersedes: []
superseded_by: []
references:
  - "ADR-0010 (App topology)"
  - "ADR-0012 (Federation state origin model)"
  - "ADR-0013 (Federation clearing adoption contract)"
  - "docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md"
  - "docs/architecture/KERNEL_APP_SEPARATION.md"
---

# ADR 0014: Constitutional Object Model

## Status

**Proposed (docs-only semantic freeze).** This ADR introduces no behavior changes, no
new code paths, no new crate public surface. It freezes the meaning of four constitutional
objects — `AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate` — so that a later
implementation tranche can land them without reopening what they are.

Implementation follows this ADR in a separate PR. Nothing in this ADR promotes any phase
status or claims completion.

---

## Context

The repository now has a working governance-to-evidence pipeline post-#1562/#1567/#1571:

- Proposals are deliberated and accepted.
- Acceptance translates a typed proposal payload into a sequence of `KernelEffect`s.
- Effects are dispatched; each returns an `EffectResult` carrying `EffectOutcome`
  (Applied / NoOp / Partial / Failed) and an opaque `receipt_ref`.
- `InstitutionalEffectRecord` and `EffectDispatchEvidence` are durably persisted in the
  gateway `ReceiptStore`, indexed by `proposal_id` and `effect_record_id`.
- Charter enforcement already exists at evaluation time: `apps/charter::CharterPolicyOracle`
  (wired at `icn/bins/icnd/src/main.rs:260`) calls `icn_ccl::schema::bridge::charter_to_constraints`
  to translate ratified charters into `ConstraintSet`s the kernel enforces without
  semantic awareness.

This pipeline is real. What is **not** real, and what is now the bottleneck, is a clean
typed institutional language for *authority itself*. Authority is currently smeared
across adjacent structures, each of which is correct for its immediate purpose but
none of which names authority as a first-class object:

| Where authority partially lives today | File / type | What's missing |
|---|---|---|
| `RoleAssignment.authority_scope: Vec<String>` | [icn/crates/icn-governance/src/structure.rs:189](icn/crates/icn-governance/src/structure.rs:189) | Untyped strings; no scope grammar; no validator; no consumer gate. |
| `Delegation` + `DelegationScope::{Blanket, Domain, Proposal}` | [icn/crates/icn-governance/src/delegation.rs](icn/crates/icn-governance/src/delegation.rs) | Strictly vote delegation. Module doc is explicit: "Vote delegation for liquid democracy." |
| `StewardRecord { governance_approval, attestations_issued, … }` | [icn/crates/icn-governance/src/steward.rs](icn/crates/icn-governance/src/steward.rs) | Appointment record + attestation counters exist, but no governance-aware gate ("is this DID currently appointed to issue this class of attestation?"). |
| `AgreementType::FederationMembership { terms }` | [icn/crates/icn-federation/src/agreement/types.rs:78](icn/crates/icn-federation/src/agreement/types.rs:78) | Typed compact-*shape* exists; federation-level *authority* is still admission policy (`Open | Vouched | Closed`), not a narrow enumerated grant. |
| `InstitutionalEffectRecord` | [icn/apps/governance/src/institutional_effect.rs](icn/apps/governance/src/institutional_effect.rs) | Self-describes as "what artifact did this decision actually create?" — it is the evidence-side translation record, not a constitutional authorization. |
| `GovernanceDecisionReceipt` | [icn/crates/icn-governance/src/proof.rs](icn/crates/icn-governance/src/proof.rs) | Canonical receipt anchored on `decision_hash`; authoritative for "the decision happened," but it does not itself bind authority-to-execute. |

Every day these partial structures ship without a unifying model, implicit meaning
hardens around the untyped surface (`Vec<String>` scopes, advisory steward records,
implicit federation admission authority). Migration cost grows roughly linearly with
new consumers. The natural downstream work — narrow federation authority, executor
gating, governance-validated attestation, member-visible "what am I authorized to do",
and proposal-to-execution closure — all depend on a frozen object model underneath.

---

## Animating principles

The object model below is shaped by six non-negotiable commitments. If a future
refinement of these types violates any of them, the refinement is wrong, not the
principle.

1. **Entity sovereignty.** Authority descends from sovereign entities (cooperatives,
   communities, federations) acting under their charters. It does not descend from
   the platform, the runtime, the gateway, a daemon instance, or a shared service.
   A grant without a sovereign grantor is not a grant.
2. **Typed separation as anti-capture.** Representation, execution, and attestation
   authority remain distinct at the type level so that aggregation across classes
   requires explicit, auditable action. Silent conflation of classes is the usual
   mechanism of capture; typing prevents it.
3. **Provenance as institutional memory.** `Mandate` is not a log line and not an
   index row. It is an institutional-memory record answering "under whose authority,
   grounded in which decision, was this action permitted?" That question must remain
   answerable long after the action lands.
4. **Constitutional legibility for ordinary members.** Typed grants and mandates
   are the substrate on which the member shell can answer, in plain terms: "what am
   I currently authorized to do, by whom, for how long, and how was that decided?"
   Mobile/member accessibility is downstream of having a typed surface to render.
   This ADR freezes that surface even though it does not implement the shell.
5. **Meaning Firewall integrity.** All four objects live at the governance app
   layer. The kernel enforces `ConstraintSet`s and dispatches `KernelEffect`s
   without learning what `AuthorityClass`, `AuthorityGrant`, `TypedScope`, or
   `Mandate` mean.
6. **Institutional, not financial, vocabulary.** Scope ceilings are institutional
   units (credit units, labor hours, resource allocations) carried by policy-
   governed positions — never hosted balances, operator-routed value, or
   platform-issued currency. The typed model must not smuggle a balance worldview
   into the constitutional layer.

---

## Decision (what this ADR freezes)

Introduce four constitutional objects at the **governance app layer**, with the meanings
below. Exact Rust representations may refine at implementation time (field names, enum
encoding, serialization tags). The *semantic categories* are frozen now.

### 1. `AuthorityClass`

A closed, enumerated kind of authority. Three variants, and only three, at the
constitutional level:

| Variant | Permits | Does NOT permit |
|---|---|---|
| `Representation` | Speaking or voting in place of another identity within a bounded scope — e.g. casting a vote for a delegator, representing a coop in a federation council seat, answering on behalf of a committee. | Executing effects. Issuing receipts/attestations. |
| `Execution` | Carrying out an action that mutates institutional state under a scope — e.g. spending from a budget, deploying a charter, freezing/unfreezing a member, settling a clearing agreement, appointing/revoking a steward. | Voting in place of others. Issuing primary attestations. |
| `Attestation` | Issuing signed statements of fact or status that downstream consumers may rely on — e.g. steward proof-of-personhood, uniqueness attestations, audit sign-offs. | Deciding policy. Executing effects. |

**These three are distinct and must not be silently conflated.** A representation grant
does not carry execution authority. An execution grant does not carry attestation
authority. An attestation grant does not decide policy. Conflating them is exactly the
anti-capture failure the typed model exists to prevent.

What `AuthorityClass` explicitly does **not** cover: general-purpose ACL/RBAC roles,
kernel-level capability bearer semantics (those remain in `icn-kernel-api::authz`),
trust score computations, or ordinary membership status. A member *being* a member is
not an authority grant; a member *appointed to execute* under a bounded scope is.

### 2. `AuthorityGrant`

A bounded, auditable grant of authority issued by a sovereign entity to a grantee,
in one and only one `AuthorityClass`. Conceptual fields:

- `id` — stable identifier for the grant (UUID or content-addressed).
- `class: AuthorityClass` — exactly one of Representation / Execution / Attestation.
- `grantor_entity` — the sovereign entity that issues the grant. This field makes
  the charter-first sovereignty explicit: grants descend from entities, never from
  platform conveniences or anonymous "roles." A valid grantor is a cooperative,
  community, or federation acting under its own charter's process; the ICN runtime,
  gateway, daemon, or a bare "system" identity is **not** a valid grantor. A grant
  lacking a sovereign grantor is malformed and must be rejected.
- `grantee` — the DID or entity identifier receiving the authority. The grantee is
  typically a person, but may be another entity (e.g. a federation grants a coop
  authority to clear on its behalf) or a shared-service entity acting as an
  institutional actor (e.g. a cooperative grants a commons service authority to
  issue personhood attestations bounded by scope). Shared-service entities are
  legitimate grantees precisely because they must be legible institutional actors,
  not invisible platform conveniences.
- `scope: TypedScope` — see §3. A grant with empty scope is meaningless; scope is
  mandatory.
- `granted_by: Option<DecisionRef>` — optional provenance back to a governance
  decision (`proposal_id` + `decision_hash`). Optional because some grants are
  charter-direct (e.g. a steward appointment ratified at charter adoption), but
  when a decision is the source, it must be named.
- `valid_from: Timestamp` — grant takes effect at or after this time.
- `valid_until: Option<Timestamp>` — grant expires at this time. `None` means
  charter-bounded or entity-lifetime-bounded; no grant is eternal without a named
  charter clause.
- `revoked_at: Option<Timestamp>` — explicit revocation record. Revocation is
  **always** possible by the grantor entity.

**`AuthorityGrant` is app-layer constitutional meaning, not a kernel primitive.**
Kernel-side `icn_kernel_api::authz::Capability` (bearer token with holder/issuer/
expiration/signature) remains the mechanical transport. `AuthorityGrant` is the
institutional source the app layer can point a future enforcement path at; it is
not imported into kernel crates.

### 3. `TypedScope`

The typed replacement path for today's `authority_scope: Vec<String>`. Minimal and
tranche-sized — designed to cover what existing consumers already express, not to
anticipate every future dimension.

Frozen scope categories (the *shape* — exact enum encoding may evolve slightly at
implementation):

- **Domain scope** — a `GovernanceDomainId` the grant is bounded to.
- **Proposal-class scope** — a class of proposal payloads the grant applies to
  (e.g. treasury spends, membership changes, charter deployments). Existing code
  already has proposal-payload tags to key off; scope is the set of those tags.
- **Action-kind scope** — a set of kernel-layer `ActionKind`s / `KernelEffect`
  variants the grant authorizes. For Execution-class grants only.
- **Amount / ceiling scope** — a numeric ceiling expressed as an institutional unit
  (credit units, labor hours, resource allocation units). These are policy-governed
  positions inside a treasury or obligation ledger, not hosted balances or
  operator-routed value. Applies to Execution grants bounded by magnitude (common
  for treasury grants).
- **Time-window scope** — a finer-grained time bound within the outer
  `valid_from`/`valid_until` (e.g. a budget grant that only applies during a
  specific fiscal window, or a meeting-quorum representation grant that only
  applies during the meeting).

A `TypedScope` is a **conjunction** of the categories that are present. A grant with
a domain scope *and* an amount scope authorizes only acts that satisfy both. Absent
categories are "unbounded along that axis" — but because grants require at least
one populated scope category to be meaningful, "unbounded on everything" is not
expressible.

`TypedScope` is not a subset of `DelegationScope`, and does not supersede it.
`DelegationScope` remains vote-delegation-specific (see §Mapping).

### 4. `Mandate`

`Mandate` is the constitutional mediation layer between a governance decision and
the execution of its effects. This is the object the repo is currently missing and
the one that this ADR most carefully defines, because its absence is what keeps
"proposal → decision → mandate → action → receipt → evidence" incomplete.

`Mandate` is also, in its own right, a **first-class institutional-memory record**.
It is the durable answer to the question "under whose authority, grounded in which
decision, was this action permitted?" That answer must survive personnel turnover,
platform upgrades, and the passage of time. A `Mandate` is therefore not reducible
to a log line, an index row, or a join between two other records; it is the record
that makes authority auditable as such.

A `Mandate` means:

> An accepted decision has produced a bounded institutional authorization to carry
> out one or more classes of action, optionally assigning an executor, a deadline,
> and one or more authority grants. Executing the mandated action under the mandate
> is what makes the effect legitimate — not merely the fact that the decision was
> recorded.

Conceptual fields:

- `id` — stable identifier for the mandate.
- `decision_receipt_id` — reference to the `GovernanceDecisionReceipt`.
- `decision_hash` — the anchoring hash from that receipt (redundant-by-design for
  fork detection and independent auditability).
- `payload_hash` — hash of the accepted proposal payload; binds the mandate to the
  concrete content of the decision, not just its identity.
- `grants: Vec<AuthorityGrantRef>` — references to one or more `AuthorityGrant`s
  that together confer the authority this mandate relies on. Typically Execution-
  class, but a mandate may bundle Representation or Attestation grants if the
  decision names them.
- `executor: Option<Did>` — the party (person or entity) named to carry out the
  action. Optional because some mandates are self-dispatching (the decision body
  itself executes) or addressed to a role rather than a DID.
- `deadline: Option<Timestamp>` — the point after which the mandate expires without
  further execution. Expiration is institutional memory, not an error: an expired
  mandate records that the action was not taken.
- `status` — lifecycle: `Pending → InProgress → Discharged → Expired → Revoked`.
  A revoked mandate blocks further execution even if prior effects have already
  landed (those remain recorded in the evidence layer).

**What `Mandate` is not:**

- Not a `Delegation`. Delegation is vote-delegation (Representation, proposal-bounded);
  Mandate is broader and typically binds Execution.
- Not a `RoleAssignment`. A role assignment places a DID in an ongoing structural
  position (e.g. Treasurer). A mandate is issued per-decision (or per-charter-clause)
  and is bounded.
- Not a `Structure::Office`. An office is an institutional seat; a mandate is an
  authorization to act, often by a seat-holder, but distinct from the seat itself.
- Not an `InstitutionalEffectRecord`. That record is *evidence* that a translation
  from accepted payload to effect occurred. Mandate is *authority* to perform that
  translation in the first place.
- Not a raw `GovernanceDecisionReceipt`. The receipt records that a decision was
  reached. The mandate binds that decision to a bounded authorization.
- Not a federation `Agreement`. An agreement is a bilateral/multilateral compact
  between entities. A mandate is an internal authorization within an entity (or
  from an entity to a named executor) to carry out a decision.
- Not a `Charter`. The charter is the sovereign constitutional source. A mandate
  is an authorization grounded in the charter's process.

**Mandate is the missing object between proposal/decision and execution/effect/evidence.**
With it present, the full chain becomes expressible: charter ratified → decision
taken under charter process → mandate issued binding that decision to one or more
authority grants → executor (acting under those grants) applies kernel effect →
`InstitutionalEffectRecord` and `EffectDispatchEvidence` record that the mandate was
discharged. Without it, authority-to-execute is implicit and managerial, and the
evidence layer records *what happened* without being able to answer *who was
authorized to do it and under which grant*.

---

## Mapping against existing repo structures

This section is the authoritative "what stays, what becomes adjacent, what is merely
proto-form" reference for the implementation PR.

### Charter (`icn-governance::charter`, `apps/charter::CharterPolicyOracle`, `icn-ccl::schema::bridge`)

**Stays.** The charter remains the sovereign constitutional source of the entity. This
ADR does not touch charter representation, ratification, or the `charter_to_constraints`
bridge. `CharterPolicyOracle` continues to translate ratified charter documents into
`ConstraintSet`s at evaluation time.

Relationship to the new objects: `AuthorityGrant`s descend from the charter's process.
The charter defines who may issue which grants under what governance process.
`Mandate`s are issued when decisions made under the charter direct an action.

### Delegation (`icn-governance::delegation`)

**Stays as representation-specific.** `DelegationScope::{Blanket, Domain, Proposal}`
remains the typed surface for vote delegation. A vote delegation is conceptually a
time-bounded `AuthorityGrant` of class `Representation` with a scope matching
`DelegationScope`, but we do not rewrite `Delegation` in terms of `AuthorityGrant` in
this tranche — doing so would be effect-family widening.

**What's explicitly excluded:** general "act-on-behalf-of" authority. Delegation is
not Execution authority and must not be conflated with it. An entity that needs to
authorize an executor does so via a `Mandate` (and its referenced `AuthorityGrant`s),
not by delegating votes.

### RoleAssignment (`icn-governance::structure`)

**Proto-authority surface.** `RoleAssignment.authority_scope: Vec<String>` is the
closest current analogue of `AuthorityGrant.scope` and is the primary migration target
(see §Migration). `RoleAssignment.assigned_by_decision: Option<ProposalId>` is the
closest current analogue of `AuthorityGrant.granted_by`.

`RoleAssignment` is not deprecated by this ADR. Its role as a structural placement
(this DID holds this role in this structure) remains. Its authority-scoping surface
becomes a typed projection into `AuthorityGrant`.

### Structure::Office (`icn-governance::structure`)

**Adjacent.** `Structure { kind: Office }` names an institutional seat. An office is
not itself an `AuthorityGrant`; office-holders may be grantees of grants, and offices
may be the addressee of mandates (as a named role rather than a DID). This ADR does
not redefine offices.

### Agreement::FederationMembership (`icn-federation::agreement::types`)

**Adjacent, proto-compact.** `AgreementType::FederationMembership { terms }` has the
shape of a typed inter-entity compact. It is not yet the narrow, typed, revocable
federation authority system this ADR ultimately prepares. Federation-level
`AuthorityGrant`s (cooperative A grants federation B the authority to clear on
A's behalf, bounded and revocable) are downstream of this ADR and are explicitly
out of scope here.

This ADR does not alter `Agreement` types. Later work to narrow federation authority
will emit `AuthorityGrant`s alongside (or attached to) `Agreement`s; that narrowing
belongs in a subsequent ADR and PR.

### StewardRecord + StewardAttestation (`icn-governance::steward`)

**Adjacent, proto-attestation-authority.** `StewardRecord { governance_approval,
status, term_start, term_end, attestations_issued, … }` already carries most of the
shape of an `AuthorityGrant` of class `Attestation`: a grantor's decision reference,
a bounded term, a grantee DID, and an attestation-counter surface.

What is missing and that this ADR prepares (but does not implement): a governance-aware
gate that validates "is this steward currently authorized, under a non-revoked grant,
to issue *this specific class* of attestation?" That gate is future work. The freeze
of `AuthorityClass::Attestation` gives that future gate a concept to consult.

### InstitutionalEffectRecord (`apps/governance::institutional_effect`)

**Evidence-side only.** [icn/apps/governance/src/institutional_effect.rs:1-19](icn/apps/governance/src/institutional_effect.rs:1) is explicit:
`InstitutionalEffectRecord` is "the governance-app answer to the question 'what
artifact did this decision actually create?'" — it is the translation evidence, not
the authorization.

`Mandate` is the missing object upstream of `InstitutionalEffectRecord`. The intended
future wiring: `GovernanceDecisionReceipt` (decision happened) → `Mandate` (authorized
to execute) → `InstitutionalEffectRecord` (translation occurred) → `EffectDispatchEvidence`
(execution reported). This ADR does not implement that wiring; it freezes the
meaning of the missing link.

### GovernanceDecisionReceipt (`icn-governance::proof`)

**Stays.** The receipt is the cryptographic anchor for "the decision was reached
under canonical process." `Mandate.decision_receipt_id` and `Mandate.decision_hash`
reference it. The receipt is not subsumed into `Mandate`; they are distinct records
with distinct purposes (receipt = decision identity; mandate = authorization to act
on the decision).

### CCL / charter bridge (`icn-ccl::schema::bridge::charter_to_constraints`)

**Stays unchanged.** The charter bridge operates at `PolicyOracle::evaluate()` time to
translate ratified charters into `ConstraintSet`s. It is not replaced by this ADR,
and it does not import any of the new objects.

Relationship: a mandate derived from a decision made under a charter is constrained
by whatever `ConstraintSet` that charter translates to. Nothing more. The kernel still
enforces constraints without understanding that the constraints came from a charter
and without understanding that a mandate mediates between decision and effect.

### Kernel authz (`icn-kernel-api::authz::Capability`, `PolicyOracle`, `ConstraintSet`)

**Stays unchanged and remains unimported.** The kernel capability is a mechanical
bearer token used for transport-level authorization. `AuthorityGrant` is the
constitutional source that a future implementation might *mint* kernel capabilities
from — but the two remain in distinct layers and the kernel does not learn the
meaning of `AuthorityClass`, `TypedScope`, or `Mandate`.

### Shared-service entities (`icn-commons`, `icn-compute`, future shared services)

**Become legible institutional actors under the same grammar.** Shared services
currently function as real actors (commons identity registry, commons-pool compute)
but are not yet modeled as grantees/grantors in a first-class way. The typed model
does not require any refactor of these crates in this tranche; it names the shape
they will be legible under:

- A shared service is a **grantee** of specific `AuthorityGrant`s issued by the
  sovereign entities that rely on it. Example (future): a federation grants its
  commons steward network an `Attestation`-class grant to issue personhood
  attestations, bounded by scope, revocable by the federation.
- A shared service may also be a **grantor** if — and only if — it is itself a
  sovereign entity under its own charter (e.g. a commons federation with its own
  ratified charter). Absent that, it is a grantee only. No shared service may be
  the grantor of its own authority.
- This framing is what makes shared services legible rather than invisible. An
  invisible platform-level permission that "just works" is exactly the drift this
  ADR exists to prevent.

No shared-service refactor is part of this ADR. Their future legibility through
this grammar is part of what the freeze prepares.

### Mobile/member constitutional surface (`sdk/react-native`, `examples/mobile-app`, `web/pilot-ui`)

**Not implemented by this ADR; directly enabled by it.** The strategic frame names
mobile/member interaction as mission-level, and the member shell as a constitutional
interface rather than cosmetic polish. This ADR does not add a single hook or
screen. What it does is freeze the typed substrate the shell needs in order to
render, in plain-language and accessible form:

- "Here are the grants you currently hold" (class, scope, grantor, expiration).
- "Here is the mandate behind the action you are about to take" (decision → grant → action).
- "Here is the provenance of the receipt you are reading" (receipt ← evidence ← mandate ← decision ← charter).

None of these surfaces is possible against `Vec<String>` capability smears,
advisory steward records, or implicit federation admission authority. A typed
surface is a prerequisite for accessibility; screen readers, plain-language
summaries, and member-facing explainers all need typed content to render
faithfully. This ADR protects that future surface by freezing its substrate now.

---

## Layer placement

All four objects (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) live at
the **governance app / domain layer**:

- Introduced in `icn-governance` (types) and consumed by `apps/governance` (persistence,
  HTTP surfaces, wiring).
- **Never imported by kernel crates.** `icn-kernel-api`, `icn-core`, `icn-net`,
  `icn-gossip`, `icn-store`, `icn-gateway`, `icn-rpc` must not depend on these types.
- The `check-meaning-firewall.sh` ratchet and `forbidden-deps-allowlist.txt` continue
  to apply. These objects are domain semantics; the kernel enforces constraints, not
  constitutional meaning.

Kernel effects, constraints, receipts, and `EffectDispatchEvidence` remain the shared
boundary types. Apps translate `Mandate`-authorized actions into `KernelEffect`s at
the existing translation seam; the kernel stays generic.

---

## Boundary rules between authority types

The ADR explicitly separates the three authority classes and names, for each, the
concrete repo locations of partial current implementation and the distinct
conflations to reject.

### Representation authority

- Permits: voting in place of another; speaking/answering in place of another within
  a bounded scope; occupying a federation council seat on behalf of a coop.
- Currently partially implemented by: `Delegation` + `DelegationScope` (vote-only);
  meeting attendance/proxy conventions (informal).
- Must not be silently conflated with: Execution. A member who delegates their vote
  is not authorizing the delegate to execute mandates on their behalf.
- Why typed separation matters: prevents delegation-to-dictatorship — a broad vote
  delegation cannot be upgraded to administrative authority without a distinct grant.

### Execution authority

- Permits: carrying out actions that mutate institutional state — budget spends,
  charter deployments, member freezes/unfreezes, steward appointments, clearing
  settlements, etc.
- Currently partially implemented by: `RoleAssignment.authority_scope: Vec<String>`
  (untyped); `StewardRecord.status` (advisory; not consulted by effect dispatcher);
  implicit "whoever the gateway accepted" at API boundaries.
- Must not be silently conflated with: Representation (voting power) or Attestation
  (signing power). An officer with Execution authority is not thereby authorized to
  issue personhood attestations.
- Why typed separation matters: prevents execution creep — the most likely source of
  platform capture is silent aggregation of execution authority under roles that
  were not explicitly granted it.

### Attestation authority

- Permits: issuing signed statements of fact or status that downstream consumers
  rely on — steward personhood attestations, uniqueness proofs, audit sign-offs,
  dispute findings.
- Currently partially implemented by: `StewardRecord` + `StewardAttestation`
  (cryptographically signed; not governance-validated); various ad-hoc `sign()`
  helpers at app boundaries.
- Must not be silently conflated with: policy decisions (Representation) or
  resource mutations (Execution). An attester's signature is a witness, not a
  decision, and not an action.
- Why typed separation matters: attestations are the trust-root for downstream
  automation (SDIS, trust graphs, clearing). Without typed separation, any
  sufficiently-privileged DID can produce attestations that downstream consumers
  treat as authoritative, with no constitutional trail back to an explicit grant.

These three must remain distinct at the type level. A `Mandate` that references a
`Representation` grant cannot be discharged by issuing an attestation; a grant of
`Attestation` cannot authorize an execution effect. The type system expresses the
institutional anti-capture rule.

---

## Scope and non-goals

This ADR is a semantic freeze. It does **not** do, and a future implementation PR
must not do under the banner of "the ADR said so," any of the following:

1. No executor or dispatcher gating. `icn-core` and `apps/governance::executor` do
   not learn to consult `Mandate` or `AuthorityGrant` in this tranche. Effect
   dispatch continues exactly as it does today.
2. No effect-family widening. No new `KernelEffect` variants, no new receipt/evidence
   families. The dispatch-evidence seam is left alone.
3. No changes to `receipt_ref` semantics. `EffectOutcome` / `receipt_ref` behavior
   is fixed by #1571 and is not re-opened here.
4. No federation-authority narrowing. `AgreementType::FederationMembership` keeps
   its current shape; `FederationPolicy::{Open, Vouched, Closed}` is not replaced
   by narrow typed grants in this tranche.
5. No shared-service / entity-kind refactor. `icn-commons`, `icn-compute`, and the
   shared-service-legibility question are explicitly out of scope.
6. No `icn-commons` centralization cleanup. L2 record hosting (charters / amendments
   / appeals) stays where it is for now.
7. No mobile SDK or mobile-shell implementation. No React Native hooks, no example
   app screens are added under this ADR.
8. No kernel meaning changes. `icn-kernel-api::authz::Capability`, `ConstraintSet`,
   `PolicyOracle` remain unchanged. The kernel does not learn about `AuthorityClass`.
9. No phase-status promotion. This ADR does not claim Phase 2, Phase 3, or any
   phase-level completion. `docs/PHASE_PROGRESS.md` is not touched.
10. No deprecation of existing surfaces in this ADR. `RoleAssignment.authority_scope:
    Vec<String>` remains present; migration is additive (see §Migration).
11. No platform-as-grantor. Under no refinement of the types may the ICN runtime,
    daemon, gateway, or any non-entity platform component appear as the
    `grantor_entity` of an `AuthorityGrant`. Authority originates from sovereign
    entities under their charters, full stop. Violating this collapses the model
    into precisely the managerial recentering the type system exists to prevent.
12. No generic balance worldview. `TypedScope.amount_ceiling` and adjacent fields
    describe policy-governed institutional positions (commitments, obligations,
    allocations, settlement records) — not hosted balances, operator-routed
    value, or platform-issued currency. The typed model does not provide a seam
    through which a balance abstraction can re-enter the constitutional layer.

---

## Migration guidance

### From `authority_scope: Vec<String>` toward `TypedScope`

The implementation PR will add `TypedScope` as a new, typed projection without
removing the string surface. Concretely:

1. `RoleAssignment` continues to carry `authority_scope: Vec<String>` unchanged.
   No wire-format break.
2. A new method (e.g. `RoleAssignment::typed_scope(&self) -> Option<TypedScope>`)
   parses known string patterns into the typed structure. Unrecognized strings
   yield `None` and are reported via metric, not error.
3. New consumers (future `AuthorityGrant` issuance, future execution gate) read
   the typed projection. Existing consumers that read the string vector continue
   to work.
4. Over subsequent tranches, call sites that mint `RoleAssignment`s begin minting
   `AuthorityGrant`s alongside, with `TypedScope` populated directly. The string
   vector becomes a legacy parallel representation and eventually a derived view.
5. No hard break is scheduled in this ADR. A later ADR will declare the string
   surface deprecated when the typed surface is the single source of truth.

### Intended future chain (not yet implemented)

```
Charter (ratified, active)
    │
    │  process produces →
    ▼
GovernanceDecisionReceipt         [exists today]
    │
    │  authorizes →
    ▼
Mandate                            [introduced by this ADR's implementation PR]
    │  (references AuthorityGrant(s) bounded by TypedScope)
    │
    │  discharged by executor →
    ▼
KernelEffect dispatch              [exists today]
    │
    ▼
InstitutionalEffectRecord          [exists today]
    │
    ▼
EffectDispatchEvidence             [exists today, post-#1571]
```

This chain is the *intended clarified model*. This ADR does not claim it exists end-
to-end. The missing step is `Mandate`; the other steps are live. Implementation
tranches will land the `Mandate` object and, subsequently, an execution gate that
consults it.

---

## Consequences

### What this freeze prepares (but does not itself implement)

- **Narrow federation authority.** Once `AuthorityGrant` exists, federation-level
  authority (e.g. "federation F may clear settlement receipts for coop C up to X
  credit units per week, revocable by C") becomes expressible as
  `AuthorityGrant { class: Execution, grantor_entity: C, grantee: F,
  scope: TypedScope { action_kind: clear_receipts, amount_ceiling: X credit units,
  time_window: weekly }, … }`. Note the directionality: C is the sovereign grantor,
  F is the grantee acting on C's behalf. The federation does not authorize itself.
- **Execution gating.** Once `Mandate` exists, the effect dispatcher can (in a
  later tranche) consult it before applying certain kernel effects. Effects that
  require a mandate will be rejected when executed without one.
- **Governance-validated attestation authority.** Once `AuthorityGrant { class:
  Attestation }` exists, steward attestations can be checked not only cryptographically
  (signature valid) but also governance-fully ("is this DID under a non-revoked
  Attestation grant for this class of attestation, at this time?").
- **Typed federation compacts built from grants.** A later ADR can declare that
  federation `Agreement`s are **composed from** `AuthorityGrant`s (not merely
  accompanied by them). A federation compact becomes legible as a bundle of
  narrow, typed, revocable grants exchanged between sovereign entities — which
  is exactly the "narrow, explicit, auditable, revocable" federation-authority
  shape the strategic frame requires.
- **Constitutional legibility in the member shell.** Once grants are typed, the
  member shell (CoopWallet React Native app, web/pilot-ui, and peer surfaces)
  becomes capable of presenting, in plain-language and accessible form, three
  questions every ordinary participant should be able to answer: *what am I
  currently authorized to do; under whose authority did a given action happen;
  how was that authority decided.* This ADR does not implement those surfaces;
  it freezes the substrate that makes them renderable.
- **Provenance strengthened as institutional memory.** `Mandate` adds a durable,
  first-class memory record between decision and effect. Audit bundles, review
  chains, and precedent queries all gain a principled join key. The evidence
  layer stops being merely "what happened" and becomes "what was authorized,
  by whom, and how it was discharged."
- **Cleaner proposal-to-execution closure.** With `Mandate` persisted between
  `GovernanceDecisionReceipt` and `InstitutionalEffectRecord`, a proposal's lifecycle
  becomes auditable end-to-end: which decision, which authorization, which execution,
  which evidence, which discharge.

### What this freeze costs

- Review attention on definitional precision. Freezing meaning before code is cheap
  *here* and expensive *later*, but the act of freezing has a cost now: it narrows
  some informal expressive space (e.g. a "role" cannot implicitly mean both
  representation and execution without a typed grant).
- One additional small surface area in `icn-governance` when implementation lands.
  That surface is purposely minimal.

### What this freeze explicitly does not do

Already enumerated in §Scope and non-goals. Reiterated here as a consequence: no
behavior changes land on the back of this ADR. If a PR claims to "implement this ADR,"
reviewers should expect type additions, an `authority_scope: Vec<String>` bridge, and
tests — not a new enforcement gate.

---

## Open questions left for implementation

These are intentionally unresolved here; they are implementation details, not
semantic decisions:

1. Exact Rust representation of `TypedScope` — whether each category is a separate
   optional field, a `Vec<ScopeClause>`, or a flat struct. Semantic categories are
   frozen; encoding is not.
2. Whether `Mandate.payload_hash` reuses the existing proposal payload hashing or
   introduces a new canonicalization. Likely reuse.
3. Whether `AuthorityGrant` is a separate store or an index projected from existing
   records (e.g. from `RoleAssignment` + proposal records). Either is acceptable;
   implementation should pick the simpler path first.
4. Whether `Mandate.status` is stored as a single field or reconstructed from
   events. Event sourcing is not mandated.
5. HTTP surface — whether `/gov/mandates/*` is introduced in this tranche or a
   later one. Not required for the types to be usable internally.

---

## References

- `ADR-0010` — App topology and canonical app roots
- `ADR-0012` — Federation state origin model
- `ADR-0013` — Federation clearing adoption contract
- [docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md](docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md)
- [docs/architecture/KERNEL_APP_SEPARATION.md](docs/architecture/KERNEL_APP_SEPARATION.md)
- [icn/crates/icn-governance/src/structure.rs](icn/crates/icn-governance/src/structure.rs) — RoleAssignment
- [icn/crates/icn-governance/src/delegation.rs](icn/crates/icn-governance/src/delegation.rs) — Delegation + DelegationScope
- [icn/crates/icn-governance/src/steward.rs](icn/crates/icn-governance/src/steward.rs) — StewardRecord
- [icn/crates/icn-governance/src/proof.rs](icn/crates/icn-governance/src/proof.rs) — GovernanceDecisionReceipt
- [icn/crates/icn-federation/src/agreement/types.rs](icn/crates/icn-federation/src/agreement/types.rs) — AgreementType
- [icn/apps/governance/src/institutional_effect.rs](icn/apps/governance/src/institutional_effect.rs) — InstitutionalEffectRecord
- [icn/apps/charter/src/oracle.rs](icn/apps/charter/src/oracle.rs) — CharterPolicyOracle
- [icn/crates/icn-ccl/src/schema/bridge.rs](icn/crates/icn-ccl/src/schema/bridge.rs) — charter_to_constraints
- [icn/crates/icn-kernel-api/src/authz.rs](icn/crates/icn-kernel-api/src/authz.rs) — kernel Capability / PolicyOracle
- [icn/bins/icnd/src/main.rs](icn/bins/icnd/src/main.rs) — CharterPolicyOracle wiring at startup
