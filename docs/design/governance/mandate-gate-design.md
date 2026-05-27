# MandateGate — Act-Time Resolver Design

**Status**: Design draft (Phase A — design only; no code change in this PR)
**Last Updated**: 2026-05-27
**Issue**: #1868 (step 6 of the `governance:write` decomposition ladder)
**Doctrine source**: [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) §4.1, §7; [`governance-write-decomposition.md`](./governance-write-decomposition.md) §6.1

This document specifies the `MandateGate` — the app-side, act-time resolver that
answers *"does this actor hold a valid mandate to perform this institutional act
on this target right now?"* It is the keystone the `governance:write`
decomposition ladder names as step 6. It is design only; no runtime behavior
changes here, and no trait or type is added to the codebase in this PR.

## 0. Non-claims

<!-- truth: descriptive -->

- This is a design proposal. It does not change runtime behavior, add code, or
  modify any handler.
- It does not retire `governance:write`, add receipt-body schema fields, or
  touch the `proposal:write` routes.
- No production-readiness, pilot-readiness, live-federation, or NYCN-activation
  claim. No regulatory-vocabulary change.

## 1. Problem statement

<!-- truth: descriptive -->

A capability scope proves *technical permission* ("the bearer may call this
class of route"). It does not prove *institutional authority* ("the institution
authorized this actor to perform this act on this target"). Doctrine §4.1 states
the split directly: the kernel-side enforcement primitive is the capability; the
institutional gate that *produces* the right to act is the mandate. The seven
class scopes minted in steps 3-11 narrow the technical surface, but a holder of
`governance:proposal:write` can still close any proposal or cast on behalf of
any decision — the capability does not bind to the specific institutional act.

The missing layer is an **act-time gate**: a check, called by a handler at the
moment it performs a high- or medium-blast act, that resolves whether a valid
mandate authorizes *this actor* for *this act* on *this target* at *this time* —
and returns a reference the handler records in the receipt so the artifact trail
captures both *which capability* the bearer used and *which mandate* authorized
the act.

## 2. Existing substrate (already built — do not rebuild)

<!-- truth: descriptive -->

The mandate *record* and its storage already exist. MandateGate is a resolver
over this substrate, not a new subsystem.

- **`Mandate` record** (`icn-governance` `src/mandate.rs`): `id` (`MandateId`),
  `decision` (`DecisionProvenance` — `proposal_id` + `decision_hash`), `grants`
  (`Vec<AuthorityGrantId>`), `status`, plus time bounds. Per ADR-0014.
- **`MandateStatus`** state machine: `Pending → InProgress → Discharged |
  Expired | Revoked`. The states exist; transition *enforcement* is future work.
- **Acceptance-time minting**: `grant_minting::mint_and_persist_for_accepted`
  mints a `Mandate` (strict or pending-grants) when a proposal is accepted, and
  the non-atomic boundary plus the sled-backed atomic override are covered by
  tests (#1872, PRs #1920/#1921).
- **Mandate store** (`GovernanceReceiptBackend` in
  `apps/governance`): `put_mandate`, `get_mandate_by_proposal`,
  `list_mandates_by_decision`, plus the authority-grant queries
  (`get_authority_grant`, `list_active_authority_grants_by_grantee`,
  `list_authority_grants_by_grantee`, `revoke_authority_grant`).
- **`AuthorityGrant`** (`icn-governance`): the bounded authorization composed
  into a mandate — `class`, `grantor`, `grantee`, `scope`, `granted_by`
  (provenance), `valid_from`, `valid_until`, `revoked_at`.
- **Posture model + fail-closed precedent** (`apps/governance`
  `GovernanceContextBuildMode`: `Bootstrap` / `Production` / `Test`). The
  membership path (`check_domain_membership` / `resolve_caller_membership`) is
  the template: an unconfigured resolver **rejects outright in `Production`**
  and stays permissive in `Bootstrap`/`Test`. Issue #1871 adds the matching
  production startup guard for optional standing checkers.

## 3. The missing piece: an act-time resolver

<!-- truth: descriptive -->

What is *not* built is the resolver that, at act time, takes
`(actor, domain, act, target, at)` and answers whether a live mandate authorizes
it. The acceptance-time seam records "this decision granted this bounded
authority"; the gate is the *consumer* that, later, validates "this actor is
exercising that authority correctly, now." The two are distinct moments:
minting is write-once at acceptance; the gate is read-mostly at each act.

MandateGate is **app-side** (`apps/governance`), never kernel. The kernel
continues to enforce only opaque capability strings and constraint sets; the
mandate semantics stay in the app. This preserves the meaning firewall.

## 4. Proposed trait shape

<!-- truth: descriptive -->

A synchronous, app-side trait (the same shape as a `PolicyOracle::evaluate`
call — `parking_lot`, no `.await`), so a handler can call it inline immediately
after `require_any_scope`:

```text
trait MandateGate {
    fn require(&self, req: &MandateRequest) -> Result<MandateGrant, MandateRejection>;
}
```

- Synchronous: handlers call it as an admission check beside the capability
  check; it must not hold a lock across `.await`.
- App-side only. The kernel never sees `MandateAct`, `MandateTarget`, or the
  resolution logic.
- Composable: it sits *beside* `require_any_scope` (capability) and
  `check_domain_membership` (standing), not inside or replacing them. See §6.

## 5. Proposed types

<!-- truth: descriptive -->

```text
struct MandateRequest {
    actor:  Did,
    domain: GovernanceDomainId,
    act:    MandateAct,
    target: MandateTarget,
    at:     Timestamp,            // seconds; see §11 (Timestamp over BlockHeight)
}

enum MandateAct {                  // finite, named institutional acts
    ActivateCharter,
    AddDomainMember,
    RemoveDomainMember,
    CloseProposal,
    CastVote,                      // distinct from CloseProposal — see §11
    AppointSteward,
    RemoveSteward,
    JoinFederation,
    LeaveFederation,
    // … one variant per high/medium-blast act, added as handlers are wired
}

enum MandateTarget {
    Domain(GovernanceDomainId),
    Proposal(ProposalId),
    Role { structure_id: StructureId, holder: Did },
    Federation(FederationId),
}

struct MandateGrant {              // returned to the handler; its hash → receipt
    mandate_id:    MandateId,
    decision_hash: Hash,
    act:           MandateAct,
    target:        MandateTarget,
    granted_at:    Timestamp,
}

enum MandateRejection {            // structured reason for the surface to render
    NoMandate,
    Expired,
    WrongTarget,
    WrongActor,
    Suspended,
    Revoked,
}
```

`MandateGrant` is a *reference*, not a new durable record: it points at the
existing `Mandate` and carries enough to (a) let the handler record a stable
hash in the receipt and (b) render the authorization in surfaces. Its exact wire
format is picked at implementation time (§9 of the decomposition doc lists this
as open).

## 6. Persistence / resolution model

<!-- truth: descriptive -->

**MandateGate is a thin resolver over the existing mandate store. It introduces
no new persistence subsystem and duplicates no record.**

Resolution algorithm (default impl, over `GovernanceReceiptBackend`):

1. Locate the mandate(s) bound to `(domain, target)` using the existing
   decision/proposal indexes (`get_mandate_by_proposal` /
   `list_mandates_by_decision`). Add a single secondary index
   `(domain, act, target) → mandate` **only if** implementation proves the
   existing keys cannot resolve the tuple cleanly — not by default.
2. Check `MandateStatus` is live (`Pending`/`InProgress`, not
   `Discharged`/`Expired`/`Revoked`) → `Revoked`/`Expired` rejection otherwise.
3. **Reject if past the mandate's own deadline** — `mandate.is_past_deadline(req.at)`
   → `Expired`. The mandate-level `deadline` is authoritative for expiry even
   when status-transition enforcement has not yet mechanically moved a
   `Pending`/`InProgress` mandate to `Expired` (that enforcement is future work,
   §12), and even if an attached grant has a wider or absent `valid_until`. The
   resolver must **not** rely on `MandateStatus` alone for expiry.
4. **Reject unbound (empty-grant) mandates fail-closed, before any actor
   check.** A mandate with `has_no_grants()` is a *pending-grants* record: it
   attests that a decision occurred but carries **no bounded authority**. These
   records exist precisely because typed grant minting/storage was unavailable
   (the #1872 fallback path in `write_pending_mandate`); per `Mandate`'s own
   contract (`icn-governance` `src/mandate.rs`) they are ineligible for
   execution authorization until real grants are attached. The resolver must
   therefore reject them with `NoMandate` *before* the actor step — never let an
   authorization-provenance-only record become act-time authority.
5. Check the actor is **named by an `AuthorityGrant.grantee` on this mandate** →
   `WrongActor`. Authorization is **grant-grantee-only**: `DecisionProvenance`
   carries only `proposal_id`/`decision_hash` (not a grantee or executor;
   `icn-governance` `src/authority.rs`), so it binds the mandate to its decision
   for audit but **never** authorizes an actor. Step 4 guarantees at least one
   grant is present, so a grantee match is always required — there is no
   provenance-only fallback that a bare capability holder could satisfy by
   pointing at the decision.
6. Check the target matches the mandate's bound subject → `WrongTarget`.
7. Check grant time validity against each grant's `valid_from`/`valid_until` →
   `Expired` (in addition to the mandate-level deadline in step 3).
8. Consult the existing `suspension_checker` for the actor as an **adjacent**
   fail-closed condition → `Suspended`. The gate does **not** re-implement
   suspension; it only reads the existing checker.

Separation of concerns is deliberate: capability scope (technical permission),
membership standing (`check_domain_membership`), mandate validity (this gate),
and suspension (existing checker) stay **composable checks**, not one god-auth
function. The gate owns only mandate validity, and surfaces `Suspended` by
delegating.

## 7. Receipt linkage and dependency on the receipt-body schema step

<!-- truth: descriptive -->

The handler records `MandateGrant`'s hash in the resulting receipt so the
artifact trail captures both the capability presented and the mandate that
authorized the act. This **depends on §10 step 2** of the decomposition doc (the
receipt-body schema work: `capability_scope_presented` + `mandate_grant:
Option<MandateGrantRef>` on `GovernanceDecisionReceipt` /
`ActionItemCompletionReceipt` / `MeetingAttendanceReceipt`, under a new
`icn:gov:…:v2` domain-separation tag; old receipts remain replayable).

Sequencing consequence: **step 6 (build the gate) lands and is unit-tested
without step 2.** The gate *returns* a `MandateGrant`; recording its hash in a
receipt is the downstream consumer wired in step 7, after step 2 has added the
field. Low-blast acts record an explicit `no_mandate_required` discriminator
rather than a grant.

## 8. Posture / failure behavior

<!-- truth: descriptive -->

Mirrors the membership precedent (`check_domain_membership`, #1871):

- **`Production`**: a required mandate that cannot be resolved → `MandateRejection`
  (fail-closed); the act is refused with a stable status (403/409) and a
  structured reason. An *unwired* gate where one is required → `GovernanceContext::validate`
  refuses to start (the #1871 production-startup-guard pattern, extended to the
  mandate gate).
- **`Bootstrap` / `Test`**: permissive — a missing/unwired gate allows the act,
  logged and explicitly labeled as non-production posture, preserving dev/devnet
  flow. This is an explicitly labeled carve-out, not a silent bypass.
- **`MandateRejection`** reasons map to stable HTTP codes + a machine-readable
  body so surfaces can render *why* (no-mandate, expired, wrong-target,
  wrong-actor, suspended, revoked).

## 9. Tests required before handler migration

<!-- truth: descriptive -->

- **Unit (step 6, in `apps/governance`)**: a fixture
  `GovernanceReceiptBackend` seeded with mandates →
  - valid `(actor, domain, act, target, at)` → `MandateGrant`;
  - each rejection reason exercised: no-mandate, time-expired, wrong-target,
    wrong-actor, revoked status, suspended (via the suspension checker);
  - **unbound pending-grants mandate** (status `Pending`, `has_no_grants()`) →
    rejected with `NoMandate` even when actor/target/time would otherwise
    match, and even though its status is live — a required case, since this is
    the authority-laundering path the gate must close;
  - **past-deadline mandate with a still-live status** (`Pending`/`InProgress`
    but `is_past_deadline(req.at)`, including the case where an attached grant's
    `valid_until` is wider/absent) → rejected `Expired` — proves the deadline is
    authoritative without assuming status-transition enforcement has run;
  - **provenance-only actor** (the actor is named in the decision provenance
    but is **not** a grantee of any attached grant) → rejected `WrongActor` —
    proves there is no provenance-only authorization path;
  - posture: `Production` fail-closed vs `Bootstrap` permissive;
  - `MandateGrant` hash determinism (stable across runs).
- **Integration (step 7)**: a migrated handler (e.g. `close_proposal`) calls
  `require()` after `require_any_scope`; rejected → 403/409; accepted → proceeds
  and (once step 2 lands) records the grant hash in the receipt.
- **Startup guard (with #1871)**: `Production` + a required-but-unwired gate →
  `validate()` refuses.

## 10. Migration order after this design

<!-- truth: descriptive -->

1. **Step 6 (next impl PR)** — build the `MandateGate` trait + types + the
   resolver + tests. **No handler changes; independent.** May run in parallel
   with step 2.
2. **Step 2** — receipt-body schema (mandate-grant reference +
   `capability_scope_presented`, v2 tag).
3. **Step 7** — wire `require()` into the **charter and federation** handlers
   first (highest / cross-cooperative blast radius); record the grant hash
   (needs step 2).
4. **Step 8** — wire `proposal:write` acts (close/cast/steward-proposal/
   delegation).
5. **ADR + step 13** — once the trait and scope set are stable, the ADR (§11 of
   the decomposition doc) freezes the `MandateGate` interface, the class scope
   set, and the `governance:write` retirement schedule.

## 11. Open decisions and recommended defaults

<!-- truth: descriptive -->

Recorded defaults (chosen for this design; reviewers/impl PRs may revisit):

- **Distinct `MandateAct` variants for `CastVote` and `CloseProposal`**
  (decomposition §12 Q4). Different target and semantics; do not collapse.
- **Comment `edit`/`delete` stay direct per-resource ownership checks, not
  routed through MandateGate** (§12 Q5). They are app-level resource ownership,
  not institutional authority.
- **`Timestamp` (seconds), not `BlockHeight`** for `MandateRequest.at`. Mandates
  and grants already carry `valid_from`/`valid_until` in seconds; aligning the
  gate with that avoids a unit mismatch.
- **Bootstrap / direct-administrative acts use the labeled bootstrap path**
  (already built — #1869's `activation_path: bootstrap`), **not** a
  `MandateGrant::AdministrativeShortcut` variant for now. MandateGate is for
  *ratified* acts; bootstrap acts do not claim a mandate.
- **An unbound (empty-grant) mandate never confers act-time authority.** A
  `has_no_grants()` pending-grants record attests a decision occurred but
  carries no bounded authority; the resolver rejects it fail-closed (`NoMandate`)
  before the actor check (§6 step 4). This is a hard invariant, not a default.
- **Actor authorization is grant-grantee-only.** `DecisionProvenance` (only
  `proposal_id`/`decision_hash`) never authorizes an actor; the actor must be a
  grantee of an attached `AuthorityGrant` (§6 step 5). Hard invariant.
- **The mandate `deadline` is authoritative for expiry independent of
  `MandateStatus`.** The resolver rejects a past-deadline mandate (`Expired`)
  even if status-transition enforcement (future work) has not moved it off
  `Pending`/`InProgress`, and regardless of any grant's `valid_until` (§6 step 3).
  Hard invariant.
- **Resolver over the existing mandate store** (no separate persistence, no
  duplicate records; add a secondary index only if proven necessary).
- **MandateGate is app-side, not kernel-side.** Capability scope = technical
  permission; MandateGate = institutional authority. Membership standing stays
  its own layer.

## 12. Non-goals

<!-- truth: descriptive -->

- No code, trait, or type added to the codebase in this PR (design only).
- No handler changes; no `proposal:write` migration; no receipt-body schema
  fields added yet; no `governance:write` retirement.
- No mandate *transition* enforcement (the `MandateStatus` state machine
  transitions remain future work; this design covers resolution, not lifecycle
  mutation).
- No kernel changes; the meaning firewall is unchanged.
- No production/pilot/live-federation/NYCN readiness claim.
