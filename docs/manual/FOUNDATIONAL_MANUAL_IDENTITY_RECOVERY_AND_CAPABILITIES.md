---
Status: draft
Canonical: no
Last Reviewed: 2026-05-11
---

# Foundational Manual: Identity Recovery and Capability Leases

This note captures the current drafting doctrine for the ICN Foundational Manual around identity recovery, key rotation, capability leases, revocation frontiers, and the ghost-capability problem.

It is not a production-readiness claim.

It is not a Phase 2 completion claim.

It is not formal partner authorization.

It is not an implementation issue.

It complements:

- [`FOUNDATIONAL_MANUAL_BASELINE_LOCK.md`](FOUNDATIONAL_MANUAL_BASELINE_LOCK.md)
- [`FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md`](FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md)

The core doctrine is simple:

> Identity recovery is not administrative reset.
>
> Recovery restores continuity of personhood.
>
> It does not automatically restore every authority-bearing capability.

That distinction prevents the oldest failure mode in digital infrastructure: social engineering the recovery path and inheriting all the power.

## Tripartite identity split

ICN must keep three layers separate.

### Personhood / identity continuity

This is the claim that the same person or identity subject continues across devices, keys, rotations, recovery events, and institutional relationships.

This layer answers:

> Is this the same identity subject?

### Device / key authority

This is the claim that a specific key, device, or session may sign for that identity under defined conditions.

This layer answers:

> May this key sign for this identity right now?

### Institutional standing

This is the claim that a person has a membership, role, authority grant, obligation, right, or standing inside a specific institution or agreement.

This layer answers:

> What does this institution recognize this person as being able, required, or entitled to do?

These layers must never collapse.

A new key may recover identity continuity.

Institutions may recognize that continuity.

But authority-bearing capabilities must be reissued through scoped, receipted process.

Membership continuity may be recognized automatically under policy.

Role continuity may require review.

High-risk capabilities must be reissued fresh.

The recovery ceremony must never silently transfer treasury authority, emergency authority, steward authority, operator authority, federation representation, or other high-risk powers merely because a recovery threshold was met.

## Meaning Firewall application

The kernel does not know Alice.

The kernel sees:

```text
old_identity_anchor
candidate_new_key
recovery_policy_ref
threshold
attestation_set
delay_window
challenge_status
finalization_receipt
did_document_version
revocation_records
capability_reissue_records
```

The institution supplies the meaning:

> This is Alice recovering access to her identity.

The substrate enforces the constraints.

## Normal key rotation

Normal key rotation is the easy case.

The person still controls an authorized key.

Flow:

1. Current authorized device creates a new key pair.
2. Old key signs a `KeyRotationIntent`.
3. New key signs a `KeyPossessionProof`.
4. Host verifies both signatures.
5. DID document updates to add the new verification method.
6. Optional delay or notice period runs depending on policy.
7. Old key is revoked, deprecated, or left active depending on policy.
8. Receipt chain records the rotation.

Candidate receipts:

```text
KeyRotationIntentReceipt
NewKeyPossessionReceipt
DidDocumentUpdateReceipt
DeviceAddedReceipt
DeviceRevokedReceipt
KeyRotationFinalizedReceipt
StandingRecomputedReceipt
```

No steward is required.

No operator is required.

No institution gets to approve the person's own key rotation unless the key is tied to an institutional role requiring additional constraints.

The person controls identity.

The institution only updates what it recognizes.

## Identity recovery ceremony

Identity recovery is the hard case.

The person has lost signing devices or a key is compromised.

The system must answer:

> Is this new key controlled by the same person who held the old identity?

It must answer without a global admin, without steward monarchy, and without operator reset power.

### Phase 0 — Recovery policy exists before loss

Recovery should follow a policy that existed before loss whenever possible.

A recovery policy should define trustees, thresholds, delay windows, notice targets, cancellation authorities, affected scopes, privacy class, and review rules.

Candidate structure:

```text
RecoveryPolicy
  subject_did
  anchor_id
  recovery_methods
  trustee_set
  threshold
  delay_period
  notice_targets
  cancellation_authorities
  institutional_scopes
  privacy_class
  expires_or_review_after
```

Candidate receipt:

```text
RecoveryPolicyRegisteredReceipt
```

If no recovery policy exists, recovery becomes more restricted and institution-specific. It should require due process, not casual steward intervention.

### Phase 1 — Candidate key creation

The person creates a new key pair on a new device.

The new key proves possession by signing a challenge.

Candidate receipt:

```text
RecoveryCandidateKeyReceipt
  old_did_or_anchor
  candidate_new_did_or_key
  possession_proof_hash
  device_class
  created_at
```

This proves only one thing:

> Someone controls the new key.

It does not prove that the someone is the original person.

### Phase 2 — Recovery request opens

The person initiates recovery against the old identity anchor.

The request identifies:

```text
old_did_or_anchor
candidate_new_key
recovery_policy_ref
requested_scopes
reason
notice_targets
delay_period
```

Candidate receipt:

```text
IdentityRecoveryOpenedReceipt
```

At this point, no standing transfers.

No roles transfer.

No capabilities transfer.

No institutional authority transfers.

The process is open. That is all.

### Phase 3 — Notice is sent

Recovery must generate notice.

Notice may go to:

- existing devices, if any remain reachable;
- registered recovery trustees;
- affected institutions where the person has active roles;
- any steward or recovery body named by policy;
- optional public or semi-public recovery channel depending on privacy class.

Candidate receipts:

```text
RecoveryNoticeIssuedReceipt
RecoveryNoticeDeliveredReceipt
RecoveryNoticeFailedReceipt
```

This gives the real person or affected institution a chance to cancel a fraudulent recovery if an old valid key, recovery body, or challenge authority remains available.

### Phase 4 — Trustees attest continuity

Trustees do not grant identity.

They attest to a bounded claim:

> I verify, by this method, that the person requesting recovery of old identity X into new key Y is the same person I know.

Each trustee signs:

```text
old_did_or_anchor
candidate_new_key
recovery_event_id
verification_method
timestamp
scope
```

Candidate receipt:

```text
RecoveryAttestationReceipt
  recovery_event_id
  trustee_did
  old_did_or_anchor
  candidate_new_key
  verification_method_hash
  timestamp
  signature
```

A trustee attests continuity.

A trustee does not assign identity.

A trustee does not transfer roles.

A trustee does not mint capabilities.

### Phase 5 — Threshold and delay

When M-of-N valid attestations arrive, recovery enters a delay period.

Candidate receipts:

```text
RecoveryThresholdMetReceipt
RecoveryDelayStartedReceipt
```

During the delay period, cancellation is allowed.

Cancellation may come from:

- an old still-valid key;
- a higher-threshold recovery body defined in policy;
- an institutional challenge path if the recovery affects institutional roles;
- a fraud report process.

Candidate receipts:

```text
RecoveryChallengeOpenedReceipt
RecoveryCancelledReceipt
```

The delay window is not bureaucracy.

It is the air gap that prevents social-engineered recovery from becoming immediate institutional takeover.

### Phase 6 — Recovery finalizes

After the delay expires and unresolved challenges clear, recovery can finalize.

Finalization does not mean:

> The new DID magically becomes the person everywhere with every old power.

Correct finalization means:

> The identity anchor recognizes a new authorized verification method and marks prior lost or compromised keys as revoked, suspended, or compromised according to policy.

Candidate receipts:

```text
IdentityRecoveryFinalizedReceipt
DidDocumentVersionReceipt
VerificationMethodAddedReceipt
LostKeyRevokedReceipt
RecoveryContinuityReceipt
```

For anchor-derived identities, the identity remains anchored to the same personhood root. The DID document changes verification methods.

For legacy key-derived DIDs, the system may need a continuity record:

```text
old_did -> new_did
```

That continuity record is still receipted, challengeable, and scoped.

### Phase 7 — Institutional standing is re-bound

Affected institutions update recognition.

Institutional standing does not move because an operator updates a database.

It moves because the institution recognizes the recovery receipt chain under its own rules.

Each institution asks:

```text
Does this recovery satisfy our recognition policy?
Does the subject anchor match the member record?
Were required notices delivered?
Were challenge windows satisfied?
Are any roles high-risk?
Do some capabilities require re-acceptance?
Do some roles require steward review or member confirmation?
```

Candidate receipts:

```text
MembershipContinuityRecognizedReceipt
RoleContinuityRecognizedReceipt
AuthorityGrantReboundReceipt
CapabilityReissuedReceipt
StandingRecomputedReceipt
```

Membership continuity may be automatic after valid recovery.

Authority continuity may require stricter review.

Capability tokens should usually be reissued, not blindly transferred.

A membership record says:

> This person is still recognized as the same member.

A capability says:

> This key may do this thing now.

Those are not the same.

### Phase 8 — Member-facing confirmation

The person receives Action Cards:

```text
Recovery finalized.
Review restored memberships.
Review restored roles.
Re-accept sensitive role.
Rotate remaining devices.
Export recovery receipt packet.
Challenge anything wrong.
```

Candidate receipts:

```text
RecoveryActionCardIssuedReceipt
RecoveryAcknowledgedReceipt
RecoveryEvidenceExportedReceipt
```

The person gets legibility, not just cryptographic restoration.

## Full recovery receipt chain

Candidate chain:

```text
RecoveryPolicyRegisteredReceipt
RecoveryCandidateKeyReceipt
IdentityRecoveryOpenedReceipt
RecoveryNoticeIssuedReceipt
RecoveryNoticeDeliveredReceipt
RecoveryNoticeFailedReceipt
RecoveryAttestationReceipt
RecoveryThresholdMetReceipt
RecoveryDelayStartedReceipt
RecoveryChallengeOpenedReceipt
RecoveryCancelledReceipt
IdentityRecoveryFinalizedReceipt
DidDocumentVersionReceipt
VerificationMethodAddedReceipt
LostKeyRevokedReceipt
RecoveryContinuityReceipt
MembershipContinuityRecognizedReceipt
RoleContinuityRecognizedReceipt
AuthorityGrantReboundReceipt
CapabilityReissuedReceipt
StandingRecomputedReceipt
RecoveryEvidenceExportedReceipt
```

This is not reset password.

This is a receipted continuity process.

## Steward and operator boundaries

Stewards may:

- verify identity continuity;
- sign recovery attestations if named in the recovery policy;
- help route recovery;
- help review fraud challenges;
- assist accessibility and evidence collection;
- participate in a threshold process.

Stewards may not:

- unilaterally assign a new key to a person;
- mutate an identity root;
- transfer roles by personal discretion;
- reissue capabilities outside policy;
- override the delay period;
- erase the old receipt chain.

Operators may:

- relay recovery messages;
- maintain storage;
- restore backups;
- make recovery data available;
- run the node infrastructure.

Operators may not:

- decide identity continuity;
- change DID documents by admin action;
- transfer memberships;
- reissue capabilities;
- suppress recovery challenges;
- rewrite old keys out of history.

Running the node is not owning the person.

Holding the fire extinguisher is still not owning the building.

## Capability tokens are leases, not permanent powers

Authority-bearing capability tokens should be treated as short-lived leases.

A capability token is not the source of authority.

It is a bounded, revocable, receipted instrument that allows a specific key, device, or session to perform a specific action under a specific authority context for a limited time.

Durable authority belongs in institutional records:

```text
membership
role standing
authority grant
mandate
charter rule
agreement
```

Executable power belongs in fresh capability leases:

```text
capability token
session grant
device grant
delegated action capability
```

Do not confuse them.

A durable role may exist for one year.

The capability to move treasury funds should not last one year on a phone sitting in somebody's pocket like a loaded gun with a constitution taped to it.

Correct split:

```text
identity continuity        -> recovered by recovery ceremony
membership standing        -> recognized by institution
role standing              -> recognized or re-confirmed by institution
authority grants           -> rebound only if policy allows
capability tokens          -> reissued, scoped, fresh, receipted
high-risk powers           -> require current revocation frontier and stricter review
```

## Capability token structure

Candidate capability token fields:

```text
capability_id
subject_identity_anchor
holder_key_or_device
issuing_institution
issuing_authority_ref
authority_grant_ref
mandate_ref
allowed_action
allowed_target
allowed_scope_or_agreement
risk_class
privacy_class
finality_requirement
identity_frontier_ref
revocation_frontier_ref
issued_at
expires_at
max_offline_duration
refresh_policy
challenge_policy
signature
```

The token says:

> This key may do this bounded thing, for this reason, until this time, if the required frontiers are fresh enough.

It does not say:

> This key is the person forever.

## Ghost capability problem

A stolen old key is the ghost capability problem.

Example:

1. Alice's phone is stolen.
2. Alice initiates recovery from a new device.
3. The recovery finalizes and a `LostKeyRevokedReceipt` is published.
4. Eve takes the stolen phone to a network-partitioned cell that has not seen the revocation.
5. Eve attempts to use the old key and an unexpired-looking local capability token.

ICN must not blindly trust local token possession for authority-bearing transitions.

Possession of an old key is not enough.

Possession of an old token is not enough.

Authority-bearing execution requires a fresh enough identity and revocation frontier.

For low-risk actions, a local host may allow bounded offline continuity under explicit finality limits.

For high-risk actions, the Rust host must prove that its identity and revocation frontier is fresh enough before sealing the State Resolution Capsule.

If the answer is insufficient, stale, forked, or disputed, the host fails closed.

WASM is never invoked.

The host does not pass `facts.capability_is_valid = true` unless the identity frontier, revocation frontier, mandate, and capability lease all satisfy the process rule.

## State Resolution Capsule integration

Capability freshness belongs in the State Resolution Capsule.

Fields to include:

```text
identity_frontier
revocation_frontier
capability_frontier
capability_token_ref
capability_risk_class
capability_freshness_policy
max_offline_duration
non_revocation_proof_ref
```

If the institution's policy says treasury movement requires a revocation frontier no older than twelve hours, a partitioned host with a three-day-old identity frontier cannot seal the envelope.

It does not matter that the old key can still sign.

The signature proves possession.

It does not prove current authority.

## Risk classes and offline continuity

Not every action needs the same freshness rule.

Candidate capability risk classes:

```text
ReadOnly
PersonalDraft
LowRiskLocal
InstitutionalRoutine
RoleBearing
Treasury
Emergency
FederationRepresentative
IdentityRecovery
OperatorCritical
```

Each class may define:

```text
max_token_lifetime
max_offline_duration
required_identity_frontier_freshness
required_revocation_frontier_freshness
required_finality_class
required_notice
required_multisig_or_threshold
challenge_window
automatic_expiration
```

A low-risk local draft may work offline for days.

A member reading cached documents may work offline until sync returns.

A routine internal action may allow local finality with later reconciliation.

A treasury allocation, emergency activation, federation vote, role revocation, operator-critical action, or identity recovery finalization should require strict freshness and fail closed if the identity or revocation frontier is stale.

No fake finality.

No ghost capabilities.

No "the phone still had the token" excuse for institutional compromise.

## Capability Validation Capsule

Before an authority-bearing execution, the host should derive a Capability Validation Capsule as part of state resolution.

```text
CapabilityValidationCapsule
  subject_identity_anchor
  signing_key
  device_ref
  capability_token_ref
  authority_grant_ref
  mandate_ref
  allowed_action
  allowed_target
  risk_class
  identity_document_version
  identity_frontier_hash
  revocation_frontier_hash
  capability_frontier_hash
  non_revocation_proof_ref
  issued_at
  expires_at
  max_offline_duration
  freshness_policy
  finality_requirement
  validation_status
```

Only if `validation_status = valid_at_frontier` can the host include the capability fact in canonical facts.

The WASM guest should never validate authority by calling back into the host.

It should only receive bounded facts such as:

```text
actor_is_eligible = true
capability_valid_at_frontier = true
required_threshold = 0.66
notice_requirement_satisfied = true
```

The host owns authority resolution.

The guest evaluates bounded facts.

## Capability receipts

Capability lifecycle must be receipted.

Candidate receipts:

```text
CapabilityIssuedReceipt
CapabilityRefreshedReceipt
CapabilityUsedReceipt
CapabilityExpiredReceipt
CapabilitySuspendedReceipt
CapabilityRevokedReceipt
CapabilityReissuedReceipt
CapabilityValidationFailedReceipt
CapabilityNonRevocationCheckedReceipt
GhostCapabilityRejectedReceipt
```

A `CapabilityUsedReceipt` should bind:

```text
capability_id
subject_identity_anchor
signing_key
identity_frontier_hash
revocation_frontier_hash
capability_frontier_hash
state_resolution_capsule_hash
process_id
target_ref
action_kind
risk_class
finality_class
receipt_hash
```

That makes later audit possible.

If a compromised old key attempts action after recovery but before gossip reaches every node, later reconciliation should reveal the attempt as a ghost-capability event.

The system should not erase it.

It should record and route it.

## Ghost capability rule

Decisive rule:

> Capability tokens are leases over authority, not authority itself.
>
> High-risk capability use requires proof of current non-revocation at the required frontier.
>
> If the host cannot prove the frontier, it cannot seal the execution envelope.

That is how ICN survives stolen devices without becoming a centralized account-reset system.

The person remains recoverable.

The institution remains protected.

The operator never becomes God.

The steward never becomes the helpdesk king.

The old key never gets to haunt the institution just because the network was injured.

## First implementation slice

Do not start by solving all recovery modes.

Start with one bounded ceremony:

```text
M-of-N social recovery
single identity anchor
candidate new key
notice receipts
threshold receipts
delay receipt
LostKeyRevokedReceipt
RecoveryContinuityReceipt
one institution recognizing membership continuity
one high-risk capability requiring reissue
one GhostCapabilityRejectedReceipt when stale revocation frontier is tested
```

Then expand to:

```text
multi-institution recognition
federation representative capability
operator-critical capability
identity recovery conflict path
privacy-preserving recovery evidence export
```

Baseline lock requires the recovery path to prove both sides:

- a person can recover without begging an administrator;
- a stolen old key cannot keep acting as a ghost authority during a partition.

## Manual insertion points

When integrating this into the full Foundational Manual:

1. Add identity recovery after state projection and before field-demonstration readiness.
2. Keep the tripartite split explicit: personhood continuity, device/key authority, institutional standing.
3. State that recovery restores continuity, not every capability.
4. Treat capability tokens as short-lived leases over durable authority records.
5. Add identity, revocation, and capability frontiers to the State Resolution Capsule.
6. Add Capability Validation Capsule before WASM execution.
7. Require high-risk actions to fail closed when non-revocation cannot be proven at the required frontier.
8. Preserve the no-overclaim rule: this document is doctrine and drafting guidance, not a claim that the runtime is complete.

## Closing doctrine

Identity recovery is not administrative reset.

Trustees attest; they do not own.

Stewards assist; they do not assign.

Operators relay; they do not decide.

Institutions recognize recovery under their own rules; they do not capture the person.

New keys may recover identity continuity.

Authority-bearing capabilities must be reissued through scoped, receipted process.

Capability tokens are leases over authority, not authority itself.

High-risk capability use requires proof of current non-revocation at the required frontier.

If the host cannot prove the frontier, it cannot seal the execution envelope.
