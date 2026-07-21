---
id: "0085"
title: "SDIS enrollment vouch authority: trust as evidence, institutional policy as authority"
status: "proposed"
date: "2026-07-21"
deciders: []
tags: ["sdis", "enrollment", "authority", "trust", "standing", "receipts", "meaning-firewall", "security"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed"
references:
  - "GitHub #2447 (reconcile the two vouching-authority models on /v1/sdis)"
  - "icn/crates/icn-gateway/src/server.rs (production /v1/sdis route mounting)"
  - "icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs (SDIS enrollment handlers)"
  - "icn/crates/icn-gateway/tests/sdis_route_authority.rs (protected SDIS route-authority tests)"
  - "icn/crates/icn-entity/src/entity.rs (EntityId and EntityType)"
  - "icn/crates/icn-entity/src/membership.rs (generic Membership model)"
  - "icn/crates/icn-trust/src/lib.rs (TrustEdge, TrustClass, trust evidence vocabulary)"
  - "docs/adr/ADR-0014-constitutional-object-model.md"
  - "docs/adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md"
  - "docs/adr/ADR-0025-institutional-effect-record-canonical-schema.md"
  - "docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md"
  - "docs/adr/ADR-0035-entity-aware-request-authorization.md"
  - "docs/adr/ADR-0083-institutional-domain-and-domain-policy-runtime-root.md"
  - "docs/spec/institutional-domain.md"
  - "docs/spec/effect-dispatch-contract.md"
---

# ADR-0085: SDIS enrollment vouch authority

> **Note on ADR id:** numbered `0085` (next free issued id) because
> `ADR-0036` and the `0021`-`0082` tranche are reserved candidate ids in
> `ops/coordination/adr_candidates.yaml`. ADR-0083 and ADR-0084 skipped past
> the same reserved block.

## Status

`proposed` - architecture direction only. This ADR is not accepted,
ratified, final, or adopted. It does not change any route, does not resolve a
runtime vulnerability by itself, and does not constitute adoption by any real
cooperative, community, federation, or institution package.

`implementation_status: proposed` - no runtime implementation lands with this
document.

## Context

Issue #2447 exists because two production-mounted SDIS routes currently perform
the same level-2 enrollment-standing transition through different authority
models.

The production `/v1/sdis` route tree is split in `icn/crates/icn-gateway/src/server.rs`:
public SDIS routes are mounted under `web::scope("/sdis")`, including
`api::sdis::verify_level2` and `simple_enrollment::configure`; the
steward/moderation surface is nested under the same prefix but wrapped with
`jwt_auth` through `simple_enrollment::configure_protected`.

### Protected steward route

`POST /v1/sdis/vouch/{enrollment_id}` is implemented by
`simple_enrollment::steward_vouch`.

The protected route:

- is mounted behind `jwt_auth` via `configure_protected`;
- requires `governance:steward:write` or `governance:write` through
  `authorize_steward_act`;
- records the actor as the verified credential subject, not the body field;
- refuses a body-supplied steward DID that does not match the credential subject;
- calls `require_coop_access(&http_req, &session.coop_id)` before mutating the
  enrollment;
- sets `session.level = 2`, `session.steward_vouch`,
  `session.steward_did`, and `session.vouched_at`;
- persists the session.

The protected route-authority behavior is covered in
`icn/crates/icn-gateway/tests/sdis_route_authority.rs`: anonymous callers are
401, malformed/expired/revoked credentials are 401, authenticated non-stewards
are 403, both steward and broad governance scopes pass the authority gate, body
attribution mismatch is 403, cross-cooperative vouching is 403, and a
same-cooperative steward can vouch successfully with the verified credential
subject recorded.

### Trust-gated route

`POST /v1/sdis/enrollment/verify/level2` is implemented by
`simple_enrollment::verify_level2`.

The trust-gated route:

- is mounted in the public SDIS scope, not under the nested `jwt_auth` scope;
- manually extracts `Authorization: Bearer ...`;
- verifies the token through `SessionAuthority`;
- parses `claims.sub` as the steward DID;
- computes `effective_trust` from the steward's self-trust and incoming edges;
- requires `effective_trust >= STEWARD_MIN_TRUST_SCORE`;
- carries a `TODO(#396)` for steward-vouch rate limiting;
- does not require `governance:steward:write` or `governance:write`;
- does not call `require_coop_access`;
- does not bind the actor to the target cooperative or target entity beyond the
  enrollment session lookup;
- sets `session.level = 2`, `session.steward_vouch`,
  `session.steward_did`, and `session.vouched_at`;
- persists the session.

The source comment above `simple_enrollment::configure` states the caveat
directly: `/enrollment/verify/level2` performs the same state transition as the
protected `/vouch/{id}` but authorizes it with a trust-graph gate rather than a
steward capability and applies no cooperative binding.

### Downstream effect

`simple_enrollment::complete_enrollment` requires `session.level >= 2`. If
`session.steward_did` is set, it creates an initial trust edge labeled
`enrollment-vouch`, creates or gets a personhood anchor/holder, builds the
membership jurisdiction as `coop:{session.coop_id}`, joins that jurisdiction
with initial `Transact` and `Vote` capabilities, and auto-approves provisional
membership when a steward vouched.

Both level-2 routes therefore feed the same completion path and can affect
standing inside the enrollment cooperative.

### Missing authority record

Neither level-2 route currently emits a durable enrollment-decision receipt,
policy-version reference, appeal record, revocation or reconsideration
reference, or explicit rate-limit decision. `simple_enrollment.rs` contains
short audit hashes for VUI collision logging and session fields for vouch
attribution, but it does not produce an ADR-0026-style durable authority record
for the standing mutation.

This is not merely an authentication inconsistency. It is a constitutional
boundary. The system is deciding:

- who may admit or advance a member;
- whose policy governs the admission;
- what evidence counts;
- who may exercise the decision;
- what appeal, review, revocation, and correction paths exist.

The architectural danger is:

> A trust or reputation signal can become ambient authority to mutate standing
> inside an institution that did not delegate that power.

That danger is especially sharp because the lower ICN model is already more
generic than the current SDIS enrollment path. `icn-entity::EntityId` supports
individual, cooperative, community, and federation identifiers; `EntityType`
includes `Community`; and `icn-entity::membership::Membership` models a
`member_id` admitted into a `parent_id`. The current SDIS simple enrollment
surface, however, is explicitly `coop_id`-shaped and creates
`coop:{session.coop_id}` jurisdiction standing. Generalizing the flow must not
universalize today's cooperative-specific shortcut.

## Decision

Propose the following generic ICN architecture:

1. Trust is evidence, sponsorship, recommendation, risk signal, or an
   institution-adopted policy condition.

2. Trust is not ambient authority to mutate membership, enrollment level,
   admission, role, capability, or standing inside a target institution.

3. The target institution's adopted enrollment policy determines:
   - admissible evidence;
   - required trust or risk conditions;
   - authorized decision-makers;
   - quorum or ceremony;
   - resulting standing;
   - appeal, reconsideration, correction, and revocation.

4. A scoped capability, mandate, authority grant, or equivalent institutional
   grant proves that an actor may exercise the policy-defined authority.

5. External institutions and actors may submit signed sponsorship or evidence,
   but may not directly write standing into the target institution unless a
   ratified federation agreement explicitly delegates that authority.

6. Every standing-affecting enrollment decision must produce durable evidence
   or a receipt compatible with the ADR-0026 receipt/provenance envelope and
   the ADR-0025 effect-record direction. Do not invent a parallel receipt
   system when the existing envelope can carry the record.

The core distinction:

| Question | Architectural answer |
|---|---|
| What evidence or confidence exists? | Trust, sponsorship, attestations, and evidence answer this. |
| What rule governs this institution? | The target institution's adopted policy answers this. |
| Who may exercise the rule? | A scoped capability, mandate, authority grant, quorum, or equivalent institutional grant answers this. |
| What happened, under which authority, using which evidence? | A durable decision receipt or effect record answers this. |

The same effect must not have two authority models. All paths that produce the
same standing transition must pass through the same authority decision and
receipt contract.

## Allowed policy shapes

This decision does not require every institution to use the same admissions
model. A target institution may adopt:

- capability-only steward admission;
- capability plus a scoped trust threshold;
- quorum or committee approval;
- external sponsorship followed by local approval;
- automatic admission based on verifiable conditions;
- another package-defined ceremony.

The constraint is that the institution adopts the rule. A generic trust score
must not grant authority by itself. Capability plus trust is valid only when the
target institution's policy says both are required.

## Bootstrap

The no-steward problem is real: a new institution may not yet have ordinary
stewards, authority grants, or a populated trust graph.

Ambient trust must not become a hidden bootstrap administrator. Legitimate
bootstrap mechanisms include:

- founding charter signatories;
- a one-time genesis ceremony;
- a configured founding quorum;
- an institution-package bootstrap policy;
- a federation-issued invitation accepted under local policy;
- a time-bounded provisional authority grant;
- explicit operator-assisted development mode that cannot masquerade as
  production governance.

Bootstrap authority must be explicit, bounded, receipted, terminable, and
distinguishable from normal operation. ADR-0020 already distinguishes
institution-package activation from package-local meaning, and ADR-0083 treats
domain-policy adoption as an authority-bearing act rather than an inert config
write. Enrollment bootstrap should follow the same posture.

## External sponsorship and federation

External sponsorship is an attestation or evidence object. It should include:

- sponsor identity;
- sponsor institution;
- target institution;
- applicant identity;
- claim or recommendation;
- scope;
- timestamp;
- expiry;
- signature;
- evidence reference;
- revocation status.

The target institution's policy decides how much weight that sponsorship has.
It may treat sponsorship as sufficient evidence, partial evidence, a risk input,
or irrelevant.

A federation treaty may delegate stronger authority, but that delegation must be
explicit, scoped, reviewable, revocable, receipted, and attributable to the
agreement that created it.

Preserve the invariant:

> federation is treaty, not merger

A federation may create reciprocity and delegated authority by agreement. It
does not silently absorb the admission authority of its member institutions.

## Canonical transition

ICN should ultimately expose one canonical level-2 enrollment-standing
transition.

Possible migration outcomes include:

1. `/vouch/{id}` becomes canonical and `/enrollment/verify/level2` is
   deprecated;
2. both routes call one shared policy/authority service;
3. the trust-only route becomes a sponsorship-submission route rather than a
   standing mutation;
4. the legacy route is feature-gated or disabled until its authority model is
   ratified.

This ADR does not choose the exact API migration. That belongs in the
implementation issue after this proposal is reviewed. The mandatory invariant
is narrower and stronger:

> All paths that produce the same standing transition must pass through the same
> authority decision and receipt contract.

## Receipt contract

The minimum durable record for a standing-affecting enrollment decision should
include:

- decision ID;
- applicant;
- target `EntityId` or jurisdiction;
- institution type;
- decision actor or body;
- authority source;
- capability, mandate, authority grant, quorum, or federation-agreement
  reference;
- policy or package identifier and version;
- evidence references or hashes;
- trust conditions evaluated, if any;
- decision result;
- resulting membership or standing;
- timestamp;
- appeal deadline or route;
- revocation or reconsideration reference;
- causal correlation ID.

Likely reusable primitives:

- ADR-0026's receipt/provenance envelope for durable, immutable, verifiable
  records;
- ADR-0025's proposed effect-record taxonomy for the institutional outcome
  (`MembershipAdmitted` / similar future kind);
- ADR-0014's `AuthorityGrant`, `TypedScope`, and `Mandate` vocabulary for
  authority source;
- ADR-0083's `InstitutionalDomain` / `DomainPolicy` direction for the adopted
  policy version;
- `icn-entity::EntityId` and generic `Membership` for the target and resulting
  standing.

Remaining gaps:

- no SDIS enrollment-decision receipt type exists today;
- no generic enrollment target schema exists today;
- no holder-oriented generic receipt index is complete today;
- no policy-version reference is written by current SDIS enrollment handlers;
- no appeal/revocation/reconsideration link is written by current SDIS
  enrollment handlers.

## Appeal, revocation, and correction

Admission denial, provisional standing, and approval must be explainable. The
architecture must support:

- reason codes;
- evidence disclosure boundaries;
- an appeal route;
- correction of erroneous evidence;
- revocation or reconsideration;
- replacement receipts or counter-records rather than silent history mutation.

Reversal is a fresh institutional transition, not deletion. This follows the
ADR-0026 immutability rule and the effect-dispatch contract's challenge /
reversal / counter-receipt direction.

## Security and anti-domination properties

This ADR prevents:

- ambient authority: trust score alone cannot write target-institution standing;
- alternate weaker routes: duplicate endpoints cannot bypass the canonical
  authority decision for the same effect;
- confused-deputy behavior: an actor with reputation or authority in one
  institution does not automatically act for another;
- implicit federation capture: federation sponsorship does not become
  admission authority unless a treaty delegates it;
- bootstrap capture by hidden administrator: bootstrap authority must be
  explicit, bounded, and terminable;
- reputation laundering: external sponsorship must be attributable, scoped,
  expiring, and revocable;
- stale-trust shortcuts: if trust is a policy condition, the policy must define
  freshness and revocation behavior;
- receipt gaps: standing-affecting decisions must leave durable evidence.

This ADR does not by itself solve:

- Sybil resistance in SDIS;
- trust-graph scoring quality;
- collusion among authorized decision-makers;
- discriminatory or inaccessible ceremonies;
- privacy-preserving evidence disclosure;
- complete CCL policy registry implementation;
- generic `EntityId` enrollment implementation;
- community enrollment projection;
- route-level implementation of the canonical transition;
- live institutional adoption.

Institution packages must still design accessible ceremonies. A policy that is
formally adopted but practically inaccessible remains an institutional defect.

## Consequences

Positive consequences:

- target-institution sovereignty is preserved;
- one standing transition has one authority model;
- trust remains useful without becoming power;
- package-defined ceremonies are possible;
- federation boundaries stay explicit;
- enrollment decisions become auditable and challengeable;
- future cooperative, community, and federation enrollment flows can share a
  generic substrate without sharing one admissions politics.

Costs:

- more policy and receipt machinery;
- migration of existing clients that call `/enrollment/verify/level2` as a
  standing mutation;
- bootstrap design work;
- explicit institution-level adoption before a trust threshold can govern
  admission;
- inability to treat a generic trust threshold as a convenient shortcut.

## Alternatives considered

| Alternative | Why not preferred |
|---|---|
| Trust-only authority | Conflates evidence with power. Lets reputation mutate standing inside institutions that did not delegate that authority. |
| Capability-only authority | Safe and simple for some institutions, but too narrow as generic ICN architecture because an institution may legitimately adopt trust, sponsorship, quorum, or automatic evidence conditions. |
| Capability plus trust | Good as one institution-adopted policy shape, but unsafe as a universal rule unless the target institution adopted the trust condition. |
| Institution-ratified policy | Preferred. It separates evidence, rule, actor authority, and receipt while allowing many local ceremonies. |
| Disable or deprecate one route immediately | May be the safest implementation posture after review, but this ADR does not make a runtime migration decision. |

## Ratification boundary

This section is mandatory for the decision.

### Repository architecture/security maintainers may decide

- generic invariants;
- no ambient cross-institution authority;
- one canonical authority path per effect;
- fail-closed composition;
- required receipt and provenance interfaces;
- separation of trust evidence from authority.

### Institution packages may define

- local ceremony;
- evidence requirements;
- trust thresholds;
- roles and quorums;
- provisional standing;
- appeal windows.

### A target institution must adopt

- the policy that governs its own enrollment;
- who may exercise it;
- which external attestations it recognizes.

### A federation agreement must define

- delegated cross-institution authority;
- reciprocity;
- scope;
- exit;
- revocation;
- dispute handling.

This ADR itself does not constitute adoption by any real institution. If accepted
by repository maintainers, it establishes the generic ICN architecture invariant,
not the admissions policy of any cooperative, community, federation, or package.

## Migration direction

Staged, non-implementation sequence:

1. ratify the generic authority invariant;
2. consolidate duplicate level-2 mutation paths;
3. introduce a durable enrollment-decision receipt;
4. define generic target `EntityId` or jurisdiction;
5. add package-defined enrollment ceremonies;
6. migrate the cooperative path;
7. add community projection separately.

No code lands with this ADR.

## Implementation status

Proposed. Nothing is implemented by this ADR.

Current implementation evidence:

- production route mounting exists in `icn/crates/icn-gateway/src/server.rs`;
- current SDIS route logic exists in
  `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`;
- protected route-authority tests exist in
  `icn/crates/icn-gateway/tests/sdis_route_authority.rs`;
- generic entity and membership primitives exist in
  `icn/crates/icn-entity/src/entity.rs` and
  `icn/crates/icn-entity/src/membership.rs`;
- trust graph primitives and trust evidence vocabulary exist in
  `icn/crates/icn-trust/src/lib.rs`;
- receipt/provenance direction exists in ADR-0026 and
  `docs/spec/effect-dispatch-contract.md`.

Not implemented:

- canonical level-2 policy/authority service;
- SDIS enrollment-decision receipt;
- generic target-entity enrollment schema;
- community enrollment projection;
- institution package enrollment ceremony runtime;
- route migration or feature gating.

## Open questions

- Is `/v1/sdis/enrollment/verify/level2` an active compatibility surface or
  effectively dead code?
- Which existing receipt type or envelope extension best represents enrollment
  admission?
- Which effect-record kind should represent admission, denial, provisional
  standing, and revocation?
- How does bootstrap authority expire?
- How are external evidence privacy and selective disclosure handled?
- How are policy versions pinned at decision time?
- How do mixed-version nodes treat new enrollment-decision receipts?
- Are denial receipts public, private, or selectively disclosed?
- Should the first implementation deprecate `/enrollment/verify/level2`, route it
  through `/vouch/{id}` semantics, or convert it into sponsorship submission?
