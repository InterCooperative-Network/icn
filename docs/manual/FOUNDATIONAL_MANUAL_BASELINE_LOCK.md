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

It exists to keep the manual aligned with the architecture ICN is actually trying to build: receipt-backed institutional process, governance by affectedness and agreement, compute subordinate to mandate, and Rust/WASM execution boundaries that preserve the Meaning Firewall.

## Baseline lock doctrine

ICN does not ship half a government.

Human testing is not a substitute for protocol completeness.

A field demonstration begins only after the core loop can execute, verify, challenge, recover, export, and federate without founder magic.

The purpose of a field demonstration is not to discover whether ICN works.

The purpose is to demonstrate that a completed loop can survive real institutional conditions.

Baseline lock means the protocol vocabulary, authority model, receipt model, rights floor, governance-scope model, role lifecycle, evidence model, and first complete institutional process loop are internally coherent before human-facing field demonstration.

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
  authority_context_hash
  standing_context_hash
  mandate_ref
  rule_ref
  determinism_class
  privacy_class
  fuel_limit
  input_artifact_refs
  canonical_facts
  expected_output_schema
```

`canonical_facts` is not the database.

It is the smallest set of pre-validated facts the host chose to disclose.

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
6. Select the rule or workload.
7. Build the minimal input envelope.
8. Hash and persist the input envelope reference.
9. Execute WASM under fuel and runtime limits.
10. Parse the output envelope.
11. Validate the output schema.
12. Verify output matches the expected process, target, workload, and transition kind.
13. Reject unexpected fields or unauthorized transition kinds.
14. Bind the result to a process-transition receipt.
15. Persist the receipt.
16. Only then allow downstream state mutation, if separately authorized.

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
input_envelope_hash
output_envelope_hash
determinism_class
privacy_class
fuel_limit
fuel_used
runner_identity
authority_context_hash
mandate_ref
result_kind
prior_receipt_hash
record_hash
signature
```

That lets a later auditor ask:

- What rule ran?
- Against what facts?
- Under what mandate?
- With what module?
- Producing what result?
- Bound to what process transition?

That is how execution becomes memory instead of vibes.

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
9. Preserve the no-overclaim rule: this document is doctrine and drafting guidance, not a claim that the runtime is complete.

## Closing doctrine

Governance scales by affectedness, domain, competence, and agreement.

Compute executes under mandate.

Services provide capacity without becoming sovereign.

Receipts prove the chain.

Rust owns authority, persistence, evidence, and state transition.

WASM computes over bounded facts.

The kernel enforces constraints.

Institutions supply meaning.

Field demonstration waits for baseline lock.
