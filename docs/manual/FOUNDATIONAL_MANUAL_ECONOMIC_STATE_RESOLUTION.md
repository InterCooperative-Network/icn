---
Status: draft
Canonical: no
Last Reviewed: 2026-05-11
---

# Foundational Manual: Economic State Resolution

This note captures the current drafting doctrine for the ICN Foundational Manual around economic state, double-spend prevention, scarce claims, mutual-credit positions, federation clearing, bridge receipts, and the first executable baseline-lock loop.

It is not a production-readiness claim.

It is not a Phase 2 completion claim.

It is not formal partner authorization.

It is not an implementation issue.

It complements:

- [`FOUNDATIONAL_MANUAL_BASELINE_LOCK.md`](FOUNDATIONAL_MANUAL_BASELINE_LOCK.md)
- [`FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md`](FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md)
- [`FOUNDATIONAL_MANUAL_IDENTITY_RECOVERY_AND_CAPABILITIES.md`](FOUNDATIONAL_MANUAL_IDENTITY_RECOVERY_AND_CAPABILITIES.md)
- [`FOUNDATIONAL_MANUAL_TRUSTLESS_CLIENT_BOUNDARY.md`](FOUNDATIONAL_MANUAL_TRUSTLESS_CLIENT_BOUNDARY.md)
- [`../rfcs/RFC-0001-obligation-allocation-settlement-primitives.md`](../rfcs/RFC-0001-obligation-allocation-settlement-primitives.md)

Core doctrine:

> ICN does not prevent economic double-spend by pretending the whole network shares one global balance.
>
> Scarce claims are reserved and consumed against a causal frontier.
>
> Mutual-credit positions are projected from obligations and settlements under explicit exposure limits.
>
> Federation clearing reconciles positions across agreement boundaries.
>
> External settlement closes through bridge receipts.
>
> If a partition produces conflicting economic branches, the system records the conflict, refuses fake finality, and routes repair through counter-receipts and challenge paths.

## No mutable balances

ICN cannot treat cooperative economics as a mutable balance in a database.

That is how banking language sneaks back in.

That is how double-spend bugs become institutional disasters.

That is how a cooperative substrate accidentally becomes a payment system with better adjectives.

The host should never ask:

> What is Alice's balance?

It should ask:

> What is Alice's projected position at this economic frontier?

Position is a projection over receipted obligations, allocations, settlements, reservations, bridge instructions, bridge receipts, reversals, and counter-receipts.

It is not a mutable account number.

The economic layer must be split into different state machines because different economic meanings have different safety rules.

Correct doctrine:

> Scarce claims use reservation and consumption.
>
> Mutual credit uses exposure limits and clearing.
>
> External settlement uses bridge receipts.
>
> None of these are balances in a wallet.

## Scarce claims use reservation semantics

Some things cannot be used twice.

Examples:

- room booking;
- vehicle slot;
- equipment reservation;
- compute quota;
- food share;
- reserved budget allocation;
- external settlement instruction.

These are scarce claims.

Scarce claims need single-consumption semantics.

That does not make ICN a blockchain.

It means scarcity requires a causal lifecycle.

Candidate lifecycle:

```text
Available
Reserved
Committed
Consumed
Released
Expired
Cancelled
Challenged
Reversed
```

Candidate receipts:

```text
ResourceAvailabilityDeclaredReceipt
AllocationAuthorizedReceipt
ResourceReservationOpenedReceipt
ResourceReservationExpiredReceipt
ResourceReservationReleasedReceipt
ResourceReservationConsumedReceipt
ResourceOverReservationRejectedReceipt
AllocationCounterReceipt
DoubleReservationDetectedReceipt
ConflictingConsumptionReceipt
```

For scarce objects, the host does not ask whether a person has enough balance.

The host asks:

```text
What claim is being consumed?
At what frontier?
Has this claim already been reserved?
Has the reservation expired?
Was it consumed on another branch?
Does the reservation authority cover this target?
Is the claim local, institutional, federation-scoped, commons-scoped, or external-bridge-scoped?
```

If two branches attempt to consume the same scarce claim, the host does not silently merge them.

It emits conflict evidence and routes repair.

No fake finality.

No invisible deletion.

No pretending a physical room exists twice because the network was partitioned.

## Mutual credit uses exposure limits

Mutual credit is not scarce-token accounting.

It is governed reciprocal obligation.

Every credit has a debit.

The system nets to zero.

No magical money printer.

No speculative asset.

No wallet balance.

In mutual credit, the question is not:

> Did Alice spend the same 500 twice?

The question is:

> Did Alice create obligations beyond the exposure limits this institution or federation agreed to tolerate?

Mutual credit needs:

```text
credit limit
exposure limit
counterparty limit
scope limit
time limit
clearing cycle
settlement rule
challenge path
```

Candidate receipts:

```text
CreditLimitGrantedReceipt
CreditLimitUpdatedReceipt
CreditLimitRevokedReceipt
ObligationIssuedReceipt
ObligationAcceptedReceipt
PositionUpdatedReceipt
ExposureLimitExceededReceipt
SettlementRecordedReceipt
ClearingCycleOpenedReceipt
ClearingNettingReceipt
ClearingCycleClosedReceipt
OverextensionChallengeReceipt
SettlementRepairRequiredReceipt
```

The host asks:

```text
What is the projected position at this frontier?
What obligations are open?
What obligations are accepted?
What settlements are recorded?
What credit limit applies?
What exposure limit applies?
What clearing cycle is active?
What finality class is required?
Is this new obligation valid under the agreement?
```

If partitioned action produces overextension, the system does not normalize it as a negative balance.

It records the overextension.

It routes the repair.

The institution may accept the overextension as an emergency exception, reverse one obligation, require settlement, suspend further capability leases, route to conflict, or adjust credit limits through governance.

But the system does not hide it.

A breach of agreed exposure is not arithmetic trivia.

It is an institutional fact.

## Federation clearing reconciles across agreement boundaries

Federation clearing is not global consensus.

It is agreement-bound reconciliation.

A federation may define clearing cycles for member institutions.

During a cycle, obligations and positions accumulate inside the agreed rules.

At the clearing boundary, the federation projects positions, nets where valid, identifies unresolved conflicts, records settlement requirements, and emits receipts.

Candidate receipts:

```text
FederationClearingCycleOpenedReceipt
MemberPositionSubmittedReceipt
MemberPositionAttestedReceipt
ClearingFrontierSealedReceipt
ClearingNettingComputedReceipt
ClearingConflictDetectedReceipt
ClearingSettlementRecordedReceipt
FederationClearingCycleClosedReceipt
```

A federation clearing process should ask:

```text
What agreement governs this clearing cycle?
Which member institutions are included?
What position attestations were submitted?
What frontier did each member attest?
What obligations are eligible for netting?
What conflicts remain unresolved?
What finality class can be claimed?
```

Federation clearing does not erase local sovereignty.

It does not absorb member institutions.

It reconciles shared consequences across an agreement boundary.

## External settlement uses bridge receipts

External settlement is a boundary.

ICN may authorize an external settlement instruction.

An external bridge institution may execute in the day-currency or legal-financial world.

The bridge returns a receipt.

ICN records the authority chain and the returned proof.

ICN does not become the bank.

ICN does not move money.

ICN does not promise convertibility.

ICN does not turn obligations into currency.

Candidate receipts:

```text
SettlementInstructionIssuedReceipt
BridgeInstructionAcceptedReceipt
BridgeExecutionReceipt
BridgeExecutionFailedReceipt
BridgeReceiptRecordedReceipt
BridgeDisputeOpenedReceipt
BridgeCounterReceipt
```

The bridge receipt closes the loop.

Not a fake on-chain balance.

## Economic State Resolution Capsule

Before sealing an economic execution envelope, the Rust host should derive an Economic State Resolution Capsule.

```text
EconomicStateResolutionCapsule
  process_id
  target_ref
  economic_object_kind
  unit_ref
  position_ref
  obligation_refs
  allocation_refs
  reservation_refs
  settlement_refs
  credit_limit_ref
  exposure_limit_ref
  clearing_cycle_ref
  bridge_instruction_ref
  bridge_receipt_refs
  economic_frontier_hash
  position_frontier_hash
  reservation_frontier_hash
  clearing_frontier_hash
  bridge_frontier_hash
  finality_class
  freshness_policy
  unresolved_conflicts
  validation_status
```

Only if:

```text
validation_status = sealed
```

can the host build the WASM execution envelope.

The WASM guest may evaluate bounded economic facts:

```text
reservation_not_consumed = true
credit_limit_not_exceeded = true
clearing_cycle_open = true
bridge_instruction_unique = true
position_after_within_allowed_exposure = true
```

But Rust resolves the economic frontier first.

Rust authorizes.

Rust emits the transition receipt.

WASM does not become the ledger.

## Economic finality classes

Economic state needs explicit finality.

Candidate classes:

```text
LocalProvisional
InstitutionReserved
InstitutionCommitted
InstitutionFinal
FederationPendingClearing
FederationReconciled
BridgeInstructionIssued
BridgeAccepted
BridgeExecuted
BridgeDisputed
Expired
Reversed
```

A local cooperative may claim institution-final allocation.

A federation may claim pending clearing.

A bridge institution may claim external execution completed.

Those are different states.

The member shell must not collapse them into paid or balance updated.

## Economic partition rule

During a partition, not every economic action has the same safety profile.

Low-risk local obligations may continue inside explicit local limits.

High-risk scarce-resource reservations require a fresh reservation frontier.

Federation clearing remains pending until the required federation frontier is available.

External settlement instructions should not be issued unless the bridge and frontier requirements are satisfied.

Correct doctrine:

> Local continuity is allowed only inside explicit exposure limits.
>
> Scarce resources require reservation.
>
> Federation settlement requires reconciliation.
>
> External settlement requires bridge receipts.
>
> No partition gets to hallucinate finality.

If a member reserves the same scarce claim on two partitioned branches, reconciliation should produce:

```text
DoubleReservationDetectedReceipt
ConflictingConsumptionReceipt
EconomicConflictActionCard
AllocationChallengeOpenedReceipt
```

If a member creates two mutual-credit obligations during a partition and the projected position exceeds their limit, reconciliation should produce:

```text
ExposureLimitExceededReceipt
OverextensionChallengeReceipt
SettlementRepairRequiredReceipt
```

If a member issues two external settlement instructions against one authorized allocation, the bridge instruction envelope must reference the allocation or reservation root, and the bridge partner should reject duplicate content hashes or already-consumed instruction refs.

If it does not, ICN records the mismatch as a bridge dispute.

## Economic projection pipeline

Do not build one generic ledger.

Build economic projections over receipt classes.

```text
Receipt graph
  ↓
Economic projection function
  ↓
Position / reservation / clearing / bridge frontiers
  ↓
EconomicStateResolutionCapsule
  ↓
ExecutionInputEnvelope
  ↓
WASM validation
  ↓
Rust authorization
  ↓
Economic transition receipt
```

This preserves the same boundary as the rest of ICN.

The institution supplies meaning.

The substrate enforces constraints.

The host resolves frontier-bound facts.

The guest evaluates bounded rules.

Receipts remember the act.

## Economic receipts and repair

Economic conflict should not be hidden.

It should be receipted and routed.

Candidate repair receipts:

```text
EconomicConflictDetectedReceipt
EconomicConflictActionCardIssuedReceipt
AllocationChallengeOpenedReceipt
ObligationChallengeOpenedReceipt
SettlementRepairRequiredReceipt
SettlementRepairCompletedReceipt
EconomicCounterReceipt
ExposureExceptionAcceptedReceipt
CreditLimitReviewOpenedReceipt
CapabilitySuspendedForEconomicRiskReceipt
```

A double-spend during a network outage is not only a software bug.

It may be a human breach of trust, an emergency exception, an operator failure, a relay failure, a policy failure, or a process-design failure.

The system should not decide that meaning by itself.

It should capture the facts and route the conflict.

That is the economic Meaning Firewall.

## First executable baseline-lock loop

The immediate next step is not every economic feature.

It is the smallest executable loop that proves the Rust/WASM/process boundary works.

Start with one institution-local proposal decision that creates one bounded economic allocation.

Recommended first loop:

1. Create a test institution with a charter rule.
2. Open a process session.
3. Generate a proposal targeting one small allocation.
4. Resolve standing and eligible voters.
5. Record notice receipts.
6. Record vote receipts.
7. Seal a State Resolution Capsule.
8. Project canonical facts.
9. Run a deterministic WASM gate that validates quorum and threshold.
10. Rust validates the output envelope.
11. Rust emits a `ProcessGateResultReceipt`.
12. Rust applies a `MutationPlan` creating an `AllocationAuthorizedReceipt`.
13. Rust derives an Economic State Resolution Capsule.
14. WASM validates that the allocation does not exceed the local limit and does not double-reserve a scarce claim.
15. Rust emits the economic transition receipt.
16. Export an evidence packet.
17. Render the member-facing Action Card status.

This is the first executable proof of the whole doctrine.

Identity does not need every recovery path yet.

Federation clearing does not need to exist yet.

External bridge receipts can be stubbed.

The mobile shell can start with one trustless inclusion path.

But this loop must prove the core law:

```text
state frontier -> canonical facts -> WASM validation -> Rust authorization -> receipt -> evidence export -> member legibility
```

If that works, baseline lock has a runtime spine.

## Manual insertion points

When integrating this into the full Foundational Manual:

1. Add economic state resolution after trustless client or state projection doctrine and before field-demonstration readiness.
2. Remove mutable balance language from ICN-native economics.
3. Treat positions as projections over receipt classes.
4. Model scarce claims with reservation and consumption semantics.
5. Model mutual credit with exposure limits and clearing.
6. Model external settlement through bridge receipts only.
7. Add EconomicStateResolutionCapsule before economic WASM evaluation.
8. Add economic finality classes to member-facing Action Card language.
9. Preserve the no-overclaim rule: this document is doctrine and drafting guidance, not a claim that the runtime is complete.

## Closing doctrine

Not blockchain global consensus.

Not CRDT money soup.

A typed economic state machine:

```text
reservation for scarcity
exposure limits for credit
clearing for federation
bridge receipts for external execution
counter-receipts for repair
```

That gives ICN the economic spine without turning it into a bank or a blockchain in a fake mustache.
