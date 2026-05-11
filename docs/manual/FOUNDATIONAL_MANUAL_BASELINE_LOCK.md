---
Status: draft
Canonical: no
Last Reviewed: 2026-05-11
---

# Foundational Manual Baseline Lock

This note captures the current drafting doctrine for the ICN Foundational Manual.

It is not a production-readiness claim.

It is not a Phase 2 completion claim.

It is not formal partner authorization.

It is not an implementation issue.

It exists to keep the manual aligned with the architecture ICN is actually trying to build: receipt-backed institutional process, governance by affectedness and agreement, compute subordinate to mandate, Rust/WASM execution boundaries that preserve the Meaning Firewall, and state resolution based on receipted causal frontiers rather than ambient database state.

## Baseline lock doctrine

ICN does not ship half a government.

Human testing is not a substitute for protocol completeness.

A field demonstration begins only after the core loop can execute, verify, challenge, recover, export, and federate without founder magic.

The purpose of a field demonstration is not to discover whether ICN works.

The purpose is to demonstrate that a completed loop can survive real institutional conditions.

Baseline lock means the protocol vocabulary, authority model, receipt model, rights floor, governance-scope model, state-resolution model, role lifecycle, evidence model, and first complete institutional process loop are internally coherent before human-facing field demonstration.

Baseline lock does not mean every future feature is complete.

It means the system's core claims are not still mush.

## First baseline-lock target

The first lockable primitive is the Institutional Process Substrate.

The baseline target is one generic institutional process that can move from preview to deliberation to decision to activation to mutation to receipt to evidence export to challenge path, while remaining legible to a nontechnical member through standing and Action Cards.

The minimum receipt-backed loop is:

1. A process opens.
2. A target object is identified.
3. Standing and authority are resolved.
4. Required notice is generated.
5. A preview/review packet is produced.
6. Deliberation attaches to the object.
7. A human or institutional decision is recorded.
8. Activation crosses the required gate.
9. A mutation plan is produced.
10. An Action Card is triggered where member action is required.
11. Execution occurs under mandate.
12. A process-transition receipt is emitted and persisted.
13. An evidence packet is produced.
14. Privacy boundaries and redaction rules are applied.
15. Accessibility gates are checked as real process constraints, not decorative compliance notes.
16. A challenge path opens, closes, or is explicitly out of scope for the bounded demonstration.
17. Proof can be exported.
18. The process can be reviewed by someone who was not in the room.

Without this spine, ICN risks becoming a pile of individually correct primitives that do not yet form an institution.

## Governance scaling doctrine

Subsidiarity is a governance doctrine.

It is not a compute scheduler.

It is not a service-placement rule.

It is not one universal territorial ladder where every decision climbs from local to regional to global.

Governance scales according to affectedness, domain, consequence, competence, rights, and agreement.

A neighborhood community may coordinate by block, neighborhood, municipality, watershed, region, or public consequence.

A cooperative may coordinate by industry, trade, labor market, production standard, supply chain, shared bargaining need, or market condition.

A housing institution may coordinate by building, tenant class, city, legal jurisdiction, repair network, financing need, or regional federation.

A food system may coordinate through growers, distributors, kitchens, retailers, land stewards, logistics routes, regional food sheds, or nutrition programs.

A care network may coordinate across households, neighborhoods, disability support, elder care, childcare, clinics, transportation, emergency response, and mutual-aid bodies.

A commons institution may coordinate around a shared resource: compute, storage, land, water, energy, knowledge, tools, transport, translation, repair capacity, or care capacity.

There is no single natural ladder.

There are consequences.

There are affected parties.

There are institutions competent to govern those consequences.

There are agreements that define how those institutions coordinate.

A federation is not inherently territorial.

A federation is an agreement layer.

It can be place-based, sector-based, market-based, industry-based, supply-chain-based, resource-based, care-based, legal-jurisdiction-based, or any other logical grouping the affected institutions legitimately adopt through agreement.

Governance rises, federates, or bridges only as far as the consequence requires and the agreement authorizes.

No farther.

## Meaning Firewall application

The Meaning Firewall applies to governance topology.

The kernel does not know what municipal, regional, industry, market, watershed, food shed, care network, or supply chain means politically.

The kernel does not decide which grouping is legitimate.

The kernel sees signed agreements, authority references, standing proofs, capability scopes, state transitions, replication rules, notice requirements, challenge windows, and receipts.

The institution supplies the political meaning.

The agreements supply the coordination topology.

The substrate enforces the constraints.

## Compute and service topology remain separate

Compute locality is not subsidiarity.

Service topology is not governance authority.

Execution locality is not mandate.

A commons compute service may run an authorized workload without gaining governance authority over the institution that authorized it.

A federation may host a shared service without owning the member institution's decision.

A support institution may assist a process without becoming the source of legitimacy.

A cell may provide continuity without becoming politically sovereign.

Correct doctrine:

> Subsidiarity governs authority.
>
> Service topology governs where capacity lives.
>
> Execution locality governs where work runs.
>
> Receipts prove the relationship between them.

Compute executes where it is authorized to execute.

Governance decides whether that execution is legitimate.

Receipts prove the chain.

## State resolution doctrine

The Rust host does not build execution envelopes from ambient current state.

Ambient current state is not a serious concept in a distributed institutional system.

The host builds execution envelopes from a sealed State Resolution Capsule: a bounded, receipted, causally-located snapshot of the facts required by one process under one authority.

If the frontier is stale, disputed, insufficient, or outside the agreement's finality rule, the host does not seal the envelope.

Correct doctrine:

> CRDTs can help people collaborate.
>
> Merkle roots can prove sets.
>
> Receipts prove transitions.
>
> Causal frontiers locate the snapshot.
>
> Agreements define what frontier is sufficient.
>
> Rust seals the facts.
>
> WASM evaluates them.
>
> Receipts remember the act.

The host does not ask:

> What does the database say right now?

The host asks:

> What receipted facts are available at this process frontier, and are they sufficient to evaluate this rule?

## State classes

ICN should distinguish three classes of state.

### Governance-grade transition state

Governance-grade transition state is append-only, receipt-backed, and Merkle-linked where set membership matters.

Examples:

- membership admitted;
- role accepted;
- decision recorded;
- mandate issued;
- allocation approved;
- settlement recorded;
- emergency authority activated;
- challenge resolved.

These records are not merged like a text document.

Reversal is a counter-record, not mutation.

### Collaborative deliberation state

Collaborative deliberation state may use CRDTs or CRDT-like structures where concurrent edits are legitimate.

Examples:

- notes;
- comments;
- collaborative proposal drafts;
- meeting annotations;
- working documents;
- argument maps.

The CRDT is the workspace.

The receipt is the institutional memory.

Once deliberation becomes part of the process record, the relevant state is snapshotted, hashed, and receipted.

### Derived read models

Derived read models exist for speed, legibility, and UX.

Examples:

- `/me/standing`;
- Action Cards;
- pending work;
- eligible voter counts;
- proposal summaries;
- quorum previews.

These are not authority.

They are cached views over authority-bearing records.

If a read model conflicts with the receipt chain, the receipt chain wins.

## State Resolution Capsule

Before the host builds an execution input envelope, it resolves a State Resolution Capsule.

The capsule locates the proposed execution at a known causal frontier.

```text
StateResolutionCapsule
  process_id
  target_ref
  requested_rule_ref
  required_authority_class
  required_scope_or_agreement
  causal_frontier
  receipt_frontier
  membership_frontier
  deliberation_frontier
  vote_frontier
  notice_frontier
  privacy_frontier
  conflict_frontier
  freshness_policy
  reconciliation_policy
  source_receipt_refs
  unresolved_conflicts
  resolution_status
```

Only if `resolution_status = sealed` can the host build a governance-grade WASM execution envelope.

The capsule does not claim eternal truth.

It says:

> I evaluated this process at this known causal boundary.

A local process may need only an institution-local frontier.

A federation process may require member-entity attestations, quorum of institutional receipts, clearing period receipts, notice delivery receipts, reconciliation state, and the federation agreement's own authority reference.

The kernel does not know what federation legitimacy means.

The agreement defines the required frontier.

The host enforces the constraint.

## Canonical Fact Snapshot

The host derives canonical facts from the sealed State Resolution Capsule.

Internally, the snapshot should carry provenance:

```text
CanonicalFactSnapshot
  process_id
  target_ref
  state_frontier_id
  membership_snapshot_root
  eligible_voter_set_root
  vote_set_root
  notice_receipt_root
  deliberation_thread_root
  authority_context_hash
  agreement_context_hash
  facts
  freshness_policy
  conflict_policy
  unresolved_conflicts
  source_receipts
```

The WASM guest does not need the full snapshot.

The guest receives only bounded facts and relevant hashes.

The host keeps the provenance.

## Seal rule and fail-closed conditions

The host refuses to seal the execution envelope when the fact snapshot is not clean enough for the process type.

Fail-closed conditions include:

- missing authority receipt;
- missing mandate reference where institutional effects are possible;
- disputed membership snapshot;
- unresolved vote-set fork;
- incomplete notice receipts;
- unresolved privacy boundary;
- stale federation agreement;
- required peer state unavailable;
- challenge window still open when closure requires finality;
- state frontier older than the process freshness policy;
- unresolved conflict outside the process's allowed conflict policy.

Partition does not magically become legitimacy.

Local continuity is allowed, but its authority class must be explicit.

A local institution can keep functioning during a network partition, but the receipt must say what kind of finality is actually being claimed.

Example:

```text
frontier_status = local_complete
federation_visibility = pending_reconciliation
external_finality = not_claimed
```

No fake global certainty.

## Finality classes

ICN does not need global consensus.

It does need explicit finality classes.

Candidate classes:

```text
LocalFinal
InstitutionFinal
FederationPending
FederationReconciled
CommonsVisible
Disputed
Expired
Revoked
```

The host can evaluate a rule against whatever finality class the process allows.

A neighborhood mutual-aid decision may only need `InstitutionFinal`.

A federation clearing decision may require `FederationReconciled`.

An emergency action may allow `LocalFinal` with mandatory later review.

A public commons standard may require broader agreement roots.

This prevents both brittle global-consensus machinery and local chaos pretending to be universal truth.

## Rust host, WASM guest

Rust is the authority-bearing host.

WASM is constrained execution.

A WASM module may compute a result, validation, recommendation, or proposed mutation geometry.

It may not authorize institutional action.

It may not mutate institutional state.

It may not write receipts.

It may not query arbitrary institutional memory.

It may not discover private overlays.

It may not become the lawgiver.

Correct rule:

> WASM may compute the proposed shape of a transition.
>
> Rust decides whether that transition is authorized.
>
> Receipts prove what happened.

Code may enforce a lawfully adopted constraint.

Code does not become the lawgiver.

## ABI capsule doctrine

The WASM guest should never receive the institution.

It should receive a bounded, canonical, schema-versioned context capsule.

That capsule says:

- here are the exact facts you are allowed to evaluate;
- here is the exact rule you are allowed to run;
- here is the exact output shape you may return;
- everything else is outside your universe.

The ABI should not expose rich Rust domain objects directly.

No raw `ProcessSession` object.

No database handle.

No callback that says `fetch proposal`.

No host function that says `write receipt`.

No magical context object.

The interface should be brutally small:

```text
evaluate(input_bytes) -> output_bytes
```

The input bytes are a canonical serialized execution input envelope.

The output bytes are a canonical serialized execution output envelope.

The guest gets bytes.

The host owns meaning.

## Execution input envelope

A governance-grade input envelope should include only bounded facts and references:

```text
ExecutionInputEnvelopeV1
  abi_version
  schema_id
  workload_id
  module_hash
  process_id
  target_ref
  state_resolution_capsule_hash
  canonical_fact_snapshot_hash
  authority_context_hash
  standing_context_hash
  agreement_context_hash
  mandate_ref
  rule_ref
  determinism_class
  privacy_class
  finality_class
  fuel_limit
  input_artifact_refs
  canonical_facts
  expected_output_schema
```

`canonical_facts` is not the database.

It is the smallest set of pre-validated facts the host chose to disclose from the sealed fact snapshot.

For example, a quorum-validation workload may receive:

```text
quorum_required
eligible_voters
votes_cast
approvals
rejections
abstentions
notice_delivered
challenge_window_days
```

That lets WASM answer whether this decision satisfied the adopted rule.

It does not let WASM discover members, inspect unrelated proposals, query private evidence, or mutate state.

## Execution output envelope

The guest returns a proposed evaluation result:

```text
ExecutionOutputEnvelopeV1
  abi_version
  schema_id
  workload_id
  process_id
  result_kind
  passed
  output_artifact_refs
  diagnostics
  proposed_transition_ref
  consumed_fuel
```

A proposed transition is not a state transition.

It is the computed geometry of a possible transition.

Rust still has to bind it to authority, mandate, receipt, persistence, evidence, and challenge path.

## Determinism and versioning

For governance-grade workloads, serialization must be deterministic.

Same input envelope bytes plus same module hash plus same runtime profile must produce the same output envelope bytes.

That means:

- no unordered maps unless canonically sorted;
- no floating point for governance-grade execution;
- no locale-dependent formatting;
- no host clock inside the guest;
- no random numbers;
- no network;
- no filesystem;
- no ambient environment variables;
- no current-state queries.

Governance-grade arithmetic should use integers or fixed-point arithmetic.

Floating point belongs in advisory compute, not receipt-bearing governance validation.

The ABI should version four things separately:

```text
ABI version       — host/guest calling convention
schema id         — input/output payload shape
module hash       — executable identity
runtime profile   — fuel, determinism, host capabilities
```

Historical workloads remain auditable only if those versions remain explicit.

## Host validation sequence

Rust treats WASM output as hostile until proven otherwise.

The host sequence is:

1. Resolve process session.
2. Resolve target object.
3. Resolve standing.
4. Resolve authority basis.
5. Resolve mandate if institutional effects are possible.
6. Resolve the governing agreement or scope rule.
7. Resolve the distributed state frontier.
8. Seal the State Resolution Capsule.
9. Derive the Canonical Fact Snapshot.
10. Select the rule or workload.
11. Build the minimal input envelope.
12. Hash and persist the input envelope reference.
13. Execute WASM under fuel and runtime limits.
14. Parse the output envelope.
15. Validate the output schema.
16. Verify output matches the expected process, target, workload, finality class, and transition kind.
17. Reject unexpected fields or unauthorized transition kinds.
18. Bind the result to a process-transition receipt.
19. Persist the receipt.
20. Only then allow downstream state mutation, if separately authorized.

The guest computes.

The host authorizes.

The receipt remembers.

## Receipt fields for baseline-lock execution

Every execution that participates in baseline lock should feed a receipt containing:

```text
process_id
target_ref
workload_id
module_hash
abi_version
input_schema_id
output_schema_id
state_resolution_capsule_hash
canonical_fact_snapshot_hash
input_envelope_hash
output_envelope_hash
determinism_class
privacy_class
finality_class
fuel_limit
fuel_used
runner_identity
authority_context_hash
agreement_context_hash
mandate_ref
result_kind
prior_receipt_hash
record_hash
signature
```

That lets a later auditor ask:

- What rule ran?
- Against what facts?
- At what causal frontier?
- Under what finality class?
- Under what mandate?
- Under what agreement or scope rule?
- With what module?
- Producing what result?
- Bound to what process transition?

That is how execution becomes memory instead of vibes.

## First implementation slice

Do not start by solving universal distributed truth.

Start by proving one bounded institutional truth can be sealed, executed, receipted, exported, and later challenged.

Recommended first slice:

```text
single-institution proposal decision
with membership snapshot
with notice receipts
with vote set root
with decision receipt
with one ProcessGateResultReceipt
with evidence export
```

Then expand to:

```text
same process across two institutions
with federation agreement receipt
with member-entity attestations
with reconciliation frontier
with federation provenance
```

Baseline lock is earned by one honest loop, not by a theory of everything.

## Manual insertion points

When integrating this into the full Foundational Manual:

1. Replace ladder-only subsidiarity with affectedness, domain, competence, and agreement.
2. Keep compute locality in the compute or service-topology section, not in the subsidiarity section.
3. Define federation as an agreement layer, not a territorial level.
4. Put the Meaning Firewall directly inside governance-topology language.
5. Replace pilot/MVP language with baseline-lock and field-demonstration language.
6. Make Institutional Process Substrate the first baseline-lock build target.
7. Treat `/me/standing` and Action Cards as the human-legibility layer immediately downstream of the process spine.
8. Treat Rust as the authority-bearing host and WASM as constrained execution.
9. Add State Resolution Capsule before Execution Input Envelope.
10. Preserve the no-overclaim rule: this document is doctrine and drafting guidance, not a claim that the runtime is complete.

## Closing doctrine

Governance scales by affectedness, domain, competence, and agreement.

Compute executes under mandate.

Services provide capacity without becoming sovereign.

Receipts prove the chain.

Causal frontiers locate the snapshot.

Agreements define what frontier is sufficient.

Rust owns authority, persistence, evidence, state resolution, and state transition.

WASM computes over bounded facts.

The kernel enforces constraints.

Institutions supply meaning.

Field demonstration waits for baseline lock.
