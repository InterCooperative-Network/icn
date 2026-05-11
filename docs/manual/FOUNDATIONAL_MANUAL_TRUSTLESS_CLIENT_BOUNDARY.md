---
Status: draft
Canonical: no
Last Reviewed: 2026-05-11
---

# Foundational Manual: Trustless Client Boundary

This note captures the current drafting doctrine for the ICN Foundational Manual around mobile clients, relays, light-node verification, outbound censorship, inbound withholding, and relay liability.

It is not a production-readiness claim.

It is not a Phase 2 completion claim.

It is not formal partner authorization.

It is not an implementation issue.

It complements:

- [`FOUNDATIONAL_MANUAL_BASELINE_LOCK.md`](FOUNDATIONAL_MANUAL_BASELINE_LOCK.md)
- [`FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md`](FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md)
- [`FOUNDATIONAL_MANUAL_IDENTITY_RECOVERY_AND_CAPABILITIES.md`](FOUNDATIONAL_MANUAL_IDENTITY_RECOVERY_AND_CAPABILITIES.md)

The core doctrine is simple:

> The operator routes; the operator does not filter.
>
> A mobile client cannot hold the whole institutional memory, but it must never blindly trust the relay that provides it.
>
> Submission requires cryptographic liability.
>
> State resolution requires multipath verification and cell-signed attestations.
>
> If an operator drops an action or hides a frontier, the protocol ensures the client holds the mathematical proof of their censorship.

## Problem statement

The mobile edge is where peer-to-peer infrastructure stops being theory.

Most people will not run a full institutional node in their pocket.

A phone cannot hold the entire Merkle-DAG.

A mobile client cannot maintain continuous gossip liveness.

A member shell will often reach ICN through gateways, relays, cooperative infrastructure, commons utilities, or other service providers.

That creates an asymmetry.

If the relay controls the mobile shell's view of the network, the relay can quietly become a monarch by technical default.

ICN must not allow that.

Relays are not trusted oracles.

Relays are cryptographically liable routing tubes.

The operator routes.

The operator does not filter.

The operator does not decide what the member sees.

The operator does not become the source of institutional memory.

The mobile shell cannot hold the whole institution, but it must never blindly trust the relay that serves it.

That is the trustless client boundary.

## Outbound censorship and relay liability

Outbound censorship is the first mobile-edge attack.

A member submits a vote, objection, deliberation entry, recovery challenge, or other signed receipt through a relay.

The relay quietly drops it.

The member thinks they acted.

The institution never sees it.

That cannot be treated as ordinary network failure.

Submission must create cryptographic liability.

When the Commons Shell sends an outbound receipt to a relay, the relay must immediately return a signed acceptance receipt.

Candidate receipt:

```text
RelayAcceptanceReceipt
  relay_node_id
  relay_operator_ref
  submitting_identity_anchor
  submitting_device_or_key
  submitted_receipt_hash
  target_process_id
  target_scope_or_agreement
  accepted_at
  relay_liveness_policy
  gossip_obligation
  inclusion_deadline
  signature
```

This receipt says:

> I received this signed action and accept the obligation to gossip it according to this service rule.

If the relay refuses to provide acceptance, the shell treats the relay as hostile or degraded.

It drops the route.

It tries another discovered peer.

Relay acceptance is not finality.

It is liability.

The member-facing Action Card should move into:

```text
Pending Network Inclusion
```

The shell later requests a proof that the submitted receipt entered the relevant causal frontier.

Candidate proof:

```text
GossipInclusionProof
  submitted_receipt_hash
  relay_acceptance_receipt_hash
  included_frontier_hash
  inclusion_path
  cell_or_scope_attestation
  observed_at
  signature_set
```

If inclusion is proven, the Action Card resolves.

If inclusion is missing after the liveness window, the shell now holds the relay's signed acceptance receipt.

That becomes a censorship fraud proof.

Candidate proof:

```text
CensorshipFraudProof
  relay_acceptance_receipt_hash
  submitted_receipt_hash
  expected_inclusion_deadline
  observed_frontier_hashes
  missing_inclusion_evidence
  reporter_identity_anchor
  report_timestamp
  signature
```

The network can verify that the relay accepted an obligation and failed to perform it.

Consequences may include service deprecation, routing avoidance, operator standing challenge, federation service review, or other institution-defined sanctions.

The point is not revenge.

The point is that the relay cannot censor quietly.

If it drops the member's action, it leaves fingerprints.

## Inbound withholding and frontier attestations

Inbound withholding is the second mobile-edge attack.

A member asks a relay for the latest state needed to process an Action Card.

The relay serves an old frontier.

Maybe it hides a `LostKeyRevokedReceipt`.

Maybe it hides a challenge.

Maybe it hides a new vote receipt.

Maybe it hides the fact that a process already closed.

The member makes a decision against manipulated memory.

ICN must not let a single relay declare reality.

A relay should not send "the state."

It should send a signed frontier attestation and the proofs required to verify it.

Candidate attestation:

```text
StateFrontierAttestation
  scope_or_agreement_ref
  frontier_hash
  latest_receipt_hashes
  membership_root
  authority_root
  revocation_root
  capability_root
  process_roots
  snapshot_refs
  finality_class
  produced_at
  liveness_window
  signer_set
  signature_threshold
  signatures
```

For governance-grade frontiers, a single relay signature is not enough.

A cell, institution, federation, or commons service should define the required attestation threshold.

A cooperative cell may require three of five service nodes.

A federation clearing process may require member-entity attestations.

A high-risk identity or treasury action may require a fresh revocation frontier.

The mobile shell verifies the signatures and finality class before treating the frontier as usable.

If the relay serves a stale but valid old frontier, that is still not enough.

The shell performs multipath cross-checking.

It asks multiple discovered endpoints:

> What is your highest known frontier attestation hash for this scope or process?

If one relay reports frontier `A`, but other unrelated endpoints report frontier `B`, the shell detects divergence.

The shell requests the missing DAG segments, checkpoint proofs, or frontier attestations from other peers.

A withholding attempt fails unless the attacker controls every route the client can reach.

This is why service discovery must not collapse to one backend URL.

The Commons Shell needs a multiplexer.

## Commons Shell as ephemeral light node

The Commons Shell cannot be built like a normal web app talking to a backend server.

A normal web app trusts the backend.

ICN cannot.

The Commons Shell must behave as an ephemeral light node.

It does not hold the whole institutional memory.

It does not maintain full gossip.

It does not become a full node.

But it does verify what it is shown.

Minimum mobile shell responsibilities:

1. **Local key enclave** — keys are generated, stored, and used locally where possible; the relay does not hold signing authority.
2. **Local fact store** — the shell caches verified frontier attestations, capability tokens, revocation roots, Action Card state, and recent receipts.
3. **Proof verifier** — the shell verifies frontier attestations, inclusion proofs, receipt chains, and relevant Merkle paths.
4. **Local WASM execution** — the shell can run `evaluate(input_bytes) -> output_bytes` for bounded checks so the relay cannot simply lie about rule evaluation.
5. **Multiplexer** — the shell routes requests across multiple relays, cells, gateways, commons utilities, and public service endpoints.
6. **Route suspicion memory** — the shell tracks relay failures, stale-frontier patterns, refusal-to-accept events, and fraud proofs.
7. **User-legible status** — the shell explains whether an action is drafted, signed, relay-accepted, frontier-included, final, disputed, stale, or pending reconciliation.

The user should not have to understand all of this.

But the shell must.

A mobile shell that cannot distinguish relay accepted, network included, and institution final is not safe enough for governance.

## Mobile Action finality states

Action Cards need more than complete/incomplete.

They need network-aware finality states.

Candidate states:

```text
DraftLocal
SignedLocal
RelayAccepted
PendingNetworkInclusion
IncludedAtFrontier
InstitutionFinal
FederationPending
FederationReconciled
StaleFrontierWarning
DivergentFrontierDetected
CensorshipSuspected
CensorshipProofAvailable
RejectedAtFrontier
ChallengeOpen
Expired
```

This makes mobile participation honest.

A user should know the difference between:

> I signed this.

and:

> A relay accepted this.

and:

> The institution included this.

and:

> This is final under the relevant agreement.

Those are different states.

Conflating them recreates platform trust.

## Relay and frontier receipts

Trustless client operation needs its own receipt classes.

Candidate receipts and proofs:

```text
RelayAcceptanceReceipt
RelayRefusalReceipt
RelayDeliveryAttemptReceipt
GossipInclusionProof
StateFrontierAttestation
FrontierDivergenceReport
CensorshipFraudProof
StateWithholdingProof
RelayDeprecationReceipt
ServiceDiscoveryUpdateReceipt
ClientRouteAuditReceipt
```

A `RelayRefusalReceipt` may be useful when a relay refuses for a legitimate reason:

```text
rate_limited
unsupported_scope
invalid_signature
expired_capability
stale_client_frontier
policy_denied
service_degraded
```

That distinction matters.

Not every failed submission is censorship.

But every failure should become legible.

## Service discovery must be anti-capture

A trustless client boundary fails if the client only knows one relay.

The Commons Shell needs signed service discovery lists.

Candidate structure:

```text
ServiceDiscoveryList
  scope_or_institution_ref
  relay_endpoints
  gateway_endpoints
  archive_endpoints
  frontier_attestation_endpoints
  public_utility_endpoints
  operator_refs
  service_capabilities
  freshness_policy
  deprecation_list
  produced_at
  expires_at
  signature_set
```

The shell should cache multiple routes.

It should prefer diversity across operators, networks, geographies, infrastructure providers, and institutional roles.

For high-risk actions, the shell should cross-check frontiers through multiple unrelated endpoints before presenting an action as safe to execute.

The point is not maximum paranoia for every tap.

The point is risk-scaled verification.

Reading a public bulletin is one thing.

Signing a federation vote, treasury action, emergency authority, role revocation, or identity recovery finalization is another.

## Local WASM verification at the edge

The relay may provide the Canonical Fact Snapshot.

The relay may provide the State Resolution Capsule.

The relay may provide the Execution Input Envelope.

But the phone should be able to verify and re-run the bounded rule where practical.

The same rule applies at the edge:

```text
evaluate(input_bytes) -> output_bytes
```

The phone does not need the whole institution.

It needs enough facts, hashes, frontier attestations, and proofs to verify the specific Action Card it is being asked to display or sign.

If the relay says:

> This vote is open and you are eligible.

The shell should be able to verify:

```text
process frontier is attested
membership/standing root includes the member or credential proof
capability is fresh enough
notice rule is satisfied
challenge/finality state is as claimed
rule evaluation output matches the provided result
```

The relay can help compute.

It cannot be the only witness.

## Field demonstration implication

Baseline lock cannot be evaluated only from server-side correctness.

The mobile/member shell path must prove that a nontechnical member can:

- sign locally;
- receive relay acceptance;
- distinguish acceptance from inclusion;
- verify or receive verified frontier attestations;
- detect stale or divergent frontiers where risk requires it;
- route around a bad relay;
- export fraud proof when censorship occurs;
- understand the status of their Action Card.

A field demonstration that requires members to trust one hosted backend has not demonstrated ICN.

It has demonstrated a platform-shaped prototype.

## First implementation slice

Do not begin with a universal mobile anti-censorship mesh.

Begin with one bounded flow:

```text
single-institution Action Card
mobile/light client local signature
RelayAcceptanceReceipt
PendingNetworkInclusion status
GossipInclusionProof
StateFrontierAttestation from institution cell
multipath hash check against at least two endpoints
CensorshipFraudProof test fixture
```

Then expand to:

```text
high-risk capability frontier checks
identity recovery finalization on mobile
federation vote Action Card
service discovery rotation
relay deprecation path
public utility relay support
```

The first proof is not every mobile feature.

The first proof is that a relay cannot quietly lie to the member.

## Manual insertion points

When integrating this into the full Foundational Manual:

1. Add the trustless client boundary after capability/frontier doctrine and before field-demonstration readiness.
2. Treat the Commons Shell as an ephemeral light node, not a web frontend.
3. Add RelayAcceptanceReceipt and GossipInclusionProof to the outbound action lifecycle.
4. Add StateFrontierAttestation and multipath cross-checking to inbound state resolution.
5. Add network-aware Action Card states.
6. Require service discovery diversity for high-risk actions.
7. Add local WASM verification where practical.
8. Preserve the no-overclaim rule: this document is doctrine and drafting guidance, not a claim that the runtime is complete.

## Closing doctrine

Relays route.

They do not rule.

Operators carry packets.

They do not own memory.

The mobile shell cannot hold the whole institution.

It can still refuse to be lied to.

Submission creates liability.

Inclusion requires proof.

Frontiers require attestation.

High-risk state requires multipath verification.

The Meaning Firewall must reach all the way to the glass.
