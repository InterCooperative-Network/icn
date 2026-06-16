---
id: "0018"
title: "Entity-Aware Request Authorization (EntityId + membership/hierarchy/delegation, migrate off flat coop_id guards)"
status: "draft"
created: "2026-06-16"
updated: "2026-06-16"
authors: ["Matt Faherty"]
reviewers: []
related_adr_candidates: []
related_issues: ["#2061", "#1868", "#1642", "#2052", "#2056", "#2058", "#2063"]
supersedes: []
superseded_by: []
---

# RFC 0018: Entity-Aware Request Authorization

## Status

`draft` — first authored version, opened for review. This RFC **explores** the design and
specifies a migration path; it is **not** a decision and **not** an implementation commitment.
Per the RFC/ADR split, the decision is recorded later in a follow-up ADR, and implementation
lands under issues with code/test evidence. **Accepted RFC does not mean implemented.**

It owns the future request-authorization model for #2061. Prior PRs #2052/#2056/#2058 are
deliberately flat-regime *hardening* and must not be expanded into hierarchy-aware authz; this
RFC is where that future model is designed.

## Summary

ICN's gateway authorizes requests with a **flat cooperative-namespace check** —
`require_coop_access` = `claims.coop_id == coop_id` string equality. That check is blind to
entity type, membership, and hierarchy, so it cannot express legitimate hierarchical or delegated
access (a federation steward acting on a member coop; a community spanning its members; a
`FederatedMember` relationship). Meanwhile a richer **entity model** (`icn-entity`) already
exists and is already used for authorization in the entity-management endpoints
(`require_entity_write_access` gates on `MembershipRole::Founder | BoardMember`).

This RFC proposes an entity-aware request-authorization primitive —
`require_entity_access(caller, target_entity, action)` — that decides from `EntityId`/`EntityType`,
the membership graph and roles, federation/community delegation, and per-action capability. Flat
same-namespace equality becomes the **degenerate same-entity case** of this primitive. It
specifies the identifier reconciliation and a **staged, site-by-site migration** that keeps the
existing flat guards as defense-in-depth until each site is converted. **Design-only: no code in
this RFC.**

## Motivation

The issue (#2061) framed this as "two parallel authorization regimes that don't meet." The
current-state inventory (below) confirms that and refines it: there are **four** identifier spaces
in play, not two, and the entity model is already partly wired into authorization — the seam is
visible and mid-migration.

Flat `coop_id` equality is **correct for today** (no hierarchical/delegated request flow exists
yet; federations operate via separate `federation:*` meta-endpoints, and tokens carry a flat
`coop_id`, not an `EntityId`). But the data model is moving to typed entities and the
authorization layer has not followed. The treasury endpoints already surface a typed `entity_id`
("type-safe entity reference, preferred over `coop_id`") while still authorizing via flat
`require_coop_access`. The model clearly anticipates relationships the guard cannot express.

This RFC exists so we reconcile the identifier spaces and define the target primitive **before**
any endpoint is converted — "architecture, not a sweep."

## Current-state inventory (verified against `main` @ 0265648e)

### Four authorization mechanisms / identifier spaces

| Layer | Mechanism | Identifier | Location |
|---|---|---|---|
| Flat coop namespace | `require_coop_access` = `claims.coop_id == coop_id` (string equality) | `coop_id` — flat slug, `[A-Za-z0-9_-]`, ≤64, **no colons** | `icn-gateway/src/middleware.rs:138`; `validate_coop_id` in `validation.rs:189` |
| Scopes / capabilities | `require_scope` / `require_any_scope` / `has_scope` (exact match, no wildcards); `ALLOWED_SCOPES` incl. the `governance:*` class split for #1868 | scope strings (`ledger:write`, `governance:proposal:write`, …) | `middleware.rs:72-134`; `validation.rs:49-99` |
| Entity / role (**already live**) | `require_entity_write_access(entity_mgr, entity_id, caller_id)` → `get_members` → gate on `Founder \| BoardMember` | `EntityId` = `entity:icn:<type>:<slug>` | `icn-gateway/src/api/entity.rs:260-294` |
| Commons / jurisdiction | `require_office_in_jurisdiction` / `require_membership_in_jurisdiction` | `JurisdictionId` (commons holder) | `icn-gateway/src/authority.rs:22-100` |

The token (`TokenClaims`, `icn-gateway/src/auth.rs:109-116`) is
`{ sub (DID), iat, exp, coop_id (flat slug), scopes[] }`. **There is no `EntityId` or
entity-type claim today.** The authz subject is therefore split: a DID (`sub`) *and* a flat coop
slug (`coop_id`), with no typed entity binding.

### The entity model already available (Regime B)

`icn-entity` provides the material an entity-aware primitive needs:

- `EntityType { Individual, Cooperative, Community, Federation, Unknown }` (`entity.rs:262`).
- `EntityId` = `entity:icn:<type>:<slug>`, with `EntityId::from_did` for individuals
  (`entity.rs:24-55`).
- `RelationType { MemberOf, FederatedWith, ParentOf }` + `EntityRelationship` (`entity.rs:523`).
- `MembershipRole { Founder, BoardMember, FederatedMember, AssociateMember, … }` and
  `MembershipCapability { Vote, Propose, TreasuryAccess, Invite, ManageSubEntities, Sign, … }`
  (`membership.rs:290-400, 643-670`).
- An `EntityRegistry` membership graph: `get_members(parent)`, `get_memberships_of(member)`,
  `list_children(parent)`, `get_parent(entity)`, `get_relationships_from/to` (`registry.rs:25-136`).

**The proven seed pattern.** `require_entity_write_access` (`api/entity.rs:260-294`) is
`require_entity_access` in embryo: it resolves the caller `claims.sub → Did →
EntityId::from_did → caller_id`, loads `get_members(entity_id)`, and gates the action on the
caller's `MembershipRole`. Generalizing this — over arbitrary target entities, the membership
graph (incl. hierarchy/delegation), and a per-action capability — is the heart of this RFC.

### Supporting layers that exist but are not yet bridged

- **Authority (governance):** `AuthorityClass { Representation, Execution, Attestation }`,
  `TypedScope { domain, proposal_class, action_kind, amount_ceiling, time_window }`,
  `AuthorityGrant`, `DecisionProvenance` (`icn-governance/src/authority.rs`). Grantors are
  cooperatives/communities/federations — never the platform.
- **Kernel:** `PolicyOracle` / `PolicyDecision` / `ConstraintSet` (`icn-kernel-api/src/authz.rs`)
  — the meaning-firewall enforcement seam.

These three (entity membership, typed authority, kernel policy oracle) exist independently. **No
bridge ties them into one request-authz primitive** — that bridge is the implementation target
this RFC scopes.

### Flat `require_coop_access` call sites (~30) and classification

Enumerated across `icn-gateway/src/api/`:

| Family | Sites | Classification |
|---|---|---|
| ledger | 6 (`get_position`, `create_settlement`, `get_history`, `get_entries_by_decision`, `create_cross_settlement`, `get_cross_settlement_quote`) | **all 6 same-entity.** Note: `create_cross_settlement` (`ledger.rs:558`) + `get_cross_settlement_quote` (`ledger.rs:666`) are the `/{coop_id}/settle/convert` **cross-UNIT FX** endpoints (`from_unit ≠ to_unit`, e.g. hours→USD) operating **within a single coop** — they pass the same path `coop_id` to `create_cross_payment`/`get_cross_payment_quote`. They are **not** cross-coop, despite the "cross" in the name. |
| treasury | 10 (status, position, nonce, budgets×2, audit, deposit, spend-propose, rules, budget-create) | same-entity today; **carries `entity_id` in its response model already** (`treasury.rs:85-106`), authorizes flat (`treasury.rs:300`) → **first reference conversion** |
| coops | 6 (get, update_settings, delete, add_member, remove_member, update_member_role) | same-entity; membership mutations overlap with the entity-model role checks already in `api/entity.rs` |
| escrow | 2 (`create_escrow` `&req.coop_id`, `release_escrow` `&escrow.coop_id`) | same-entity (hardened in #2058) |
| invites | 2 (`create_invite`, `list_invites`) | same-entity (hardened in #2058) |
| recurring | 1 (`create_recurring_settlement`) | same-entity (hardened in #2058) |
| registry | 2 (`create_meeting`, `index_decision_endpoint`) | same-entity (hardened in #2052/#2056) |

**All ~30 sites are same-entity self-access.** There is **no cross-coop delegation request
surface among the flat-guarded sites today** — consistent with #2061's own note that "no such
flow exists" (federations operate via separate `federation:*` meta-endpoints). The "cross-settlement"
rows are cross-**unit** FX within one coop, not cross-coop (corrected from an earlier draft).
**Consequence:** the delegation model below is **net-new / forward-looking** — it does not retrofit
existing cross-coop endpoints (there are none); delegated flows would first appear as *new* routes
(federation/community), not as conversions of these flat sites. The full per-site `file:line` table
is maintained in the migration tracker (issue #2061); the families above are the migration units.

> **Correction to the issue framing (no readiness-laundering):** treasury carries `entity_id` in
> its **response** model, not yet in the **request** body. The seam is present but one step
> earlier than #2061 stated — the request still arrives keyed on `coop_id`.

## Proposed model

### The primitive

```text
require_entity_access(caller, target_entity, action) -> Result<(), AuthzError>
```

decides from:

- `EntityId` + `EntityType` of caller and target (`icn-entity`);
- the membership graph and roles (`MemberOf`, `MembershipRole`, incl. `FederatedMember`);
- federation/community delegation (`ParentOf` / `FederatedWith`, parent↔child);
- per-action capability (converges with #1868 — broad scopes decomposed into per-action caps);
- (where applicable) standing / `AuthorityGrant` + `TypedScope` for delegated execution.

**Flat same-namespace equality becomes the degenerate same-entity case — defined on the token's
authorized coop, NOT the caller individual.** Today's `require_coop_access` compares the *token's*
coop scope (`claims.coop_id`) to the path `coop_id` (coop ↔ coop). The behavior-preserving
degenerate case is therefore: project the token's authorized coop
(`claims.coop_id → entity:icn:cooperative:<slug>`) and require it to equal the target entity.

It must **not** be defined as "caller individual == target": a normal `food-coop` member token
resolves via `EntityId::from_did(claims.sub)` to `entity:icn:individual:*`, which never equals the
`entity:icn:cooperative:food-coop` target — defining the degenerate case on individual identity
would deny valid requests, or push implementations toward minting an org-entity subject without a
DID binding. The general case then **widens** the degenerate one: a caller individual who is a
*member* of the target entity (with the required role/capability), or who holds delegated
authority over it, is also allowed. (This means the authz subject is two-part — the caller
individual *and* the coop the token authorizes for — see Identifier reconciliation below.)

### Layering (meaning firewall preserved)

- **Generic machinery in ICN core.** Entity IDs are opaque coordinates; the membership-graph
  traversal and the `require_entity_access` shape are generic. The kernel keeps seeing only
  `PolicyDecision`/`ConstraintSet` — it never learns "this is a federation steward."
- **Institution-local meaning in packages/CCL.** Rules like "organizers may veto" or
  "a federation may aggregate member treasuries" are **institution policy**, expressed in the
  governance app / CCL charters / institution packages — **never** baked into ICN core or the
  kernel. (This is the standing meaning-firewall invariant: AGENTS.md "ICN invariants";
  `docs/architecture/KERNEL_APP_SEPARATION.md`; `THE_COMMONS.md` "not one social doctrine in the
  kernel".) **No NYCN-specific rule enters ICN core via this work.**

### Relationship to existing layers

- **Scopes/#1868** answer "does the token carry the capability for this action class?"
  `require_entity_access` answers "may this caller act on *this entity* for this action?" The two
  compose: scope-gate first (coarse), then entity-gate (fine), mirroring the existing two-layer
  pattern in `api/entity.rs` (scope `entity:write` + role check).
- **Authority/`TypedScope`** is how *delegated* authority (federation acting on a member) is
  expressed; since no flat-guarded site is cross-coop today, delegation first matters for
  **net-new** federation/community routes, not for converting any existing endpoint.
- **`PolicyOracle`** is the kernel-side enforcement seam the app-layer decision ultimately maps to.

## Identifier reconciliation (must be decided before implementation)

Four identifiers name the authz subject/target today: **`coop_id`** (flat slug), **`EntityId`**
(`entity:icn:<type>:<slug>`), **DID** (`claims.sub`), **`JurisdictionId`** (commons). The RFC
proposes:

1. **Canonical authz target = `EntityId`.** It is typed (carries `EntityType`) and already the
   currency of the membership graph. A flat `coop_id` maps to `entity:icn:cooperative:<slug>` —
   **but this mapping is not a free string transform, and is not lossless/reversible in general.**
   The two namespaces differ: `validate_coop_id` (`validation.rs:189-213`) accepts Unicode
   alphanumerics, **uppercase**, **underscores**, and length **1–64**, whereas an `EntityId` slug
   (`entity.rs:84-135`) requires **lowercase-ASCII** alphanumerics + hyphens only, length **4–64**,
   must **start with a letter**, and forbids **consecutive hyphens**. So currently-valid coops like
   `coop_A`, `abc` (len 3), `café`, or `1coop` have **no** valid slug, and naive normalization can
   **collide** (`coop_A` and `coop-a` → `coop-a`). The migration must therefore define an explicit
   **normalization + rejection + stored bidirectional mapping (backfill)** policy: a canonical
   `coop_id ↔ EntityId` mapping **persisted per coop at activation** (so it survives normalization
   collisions and stays reversible), with non-mappable ids **explicitly rejected** rather than
   silently remapped. Only the subset of `coop_id`s that already satisfy the slug rules project
   directly; everything else needs the stored mapping. This sub-task gates the migration (see
   step 1 below) and must land before any guard is converted.
2. **Canonical authz subject is two-part: (caller individual, authorized coop).** The caller
   individual is `EntityId::from_did(claims.sub)` (an `entity:icn:individual:*`); the authorized
   coop is the token's `claims.coop_id` projected to `entity:icn:cooperative:<slug>`. This split
   matters because today's two regimes use *different* subjects — the flat guards compare the
   **token's coop** to the target (coop↔coop), while the entity-management endpoints compare the
   **caller individual's** membership/role in the target. `require_entity_access` must honor both:
   the degenerate case is "authorized-coop projection == target" (preserving flat semantics), and
   the general case is "caller individual is a member of (or holds delegated authority over) the
   target," resolved via `get_memberships_of`. Conflating the two into a single subject is the
   bug that would otherwise break behavior-preservation.
3. **Token claim shape:** add an optional `entity_id` (and entity-type) claim **alongside**
   `coop_id` during migration; `coop_id` stays as the back-compat projection until all sites are
   converted, then can be deprecated. (Mirrors how treasury already pairs the two.)
4. **Scope of `JurisdictionId`:** the commons/jurisdiction layer is a *separate* authority
   surface (governance offices). This RFC **does not** fold it into `require_entity_access`;
   reconciling entity-authz with jurisdiction-authz is called out as an open question, not solved
   here.

## Migration path (staged, defense-in-depth, no big-bang)

1. **Identifier + claim groundwork.** Decide the canonical subject/target (above); add the
   optional `entity_id`/entity-type token claim; add the missing resolution helpers. **The
   `coop_id ↔ EntityId` mapping is the gating sub-task** — it is normalization + rejection of
   non-mappable ids + a stored bidirectional mapping/backfill per §1 (the namespaces don't line
   up and naive projection collides), **not** a naive string transform. Also add a DID→org-membership
   convenience over `get_memberships_of`. No guard changes yet.
2. **Generalize the proven helper.** Lift `require_entity_write_access` →
   `require_entity_access(caller, target_entity, action)` with the degenerate case defined on the
   token's coop projection (above) + a per-action capability argument. Land it **with tests** for:
   same-coop token → matching target (allow); different-coop token → target (deny); caller
   individual is a member of the target with the required role (allow); non-member / insufficient
   role (deny); delegated authority (allow); no authority (deny). Note the tests are built around
   the token's coop projection and membership — **not** caller-individual==target identity.
3. **First reference conversion: treasury.** Treasury already pairs `coop_id` + `entity_id`;
   convert its sites to `require_entity_access` as the worked example, keeping the flat guard as
   an assertion alongside until the entity path is proven.
4. **Site-by-site, family-by-family.** Convert same-entity families (coops, escrow, invites,
   recurring, registry, ledger-self) where the new primitive's degenerate case is provably
   equivalent; keep #2052/#2056/#2058 flat guards as defense-in-depth until each site's
   entity-path tests pass.
5. **Delegation last.** Cross-coop / federation / community delegated flows land only once the
   delegation/`AuthorityGrant` path is specified and tested. These are **net-new routes** (no
   existing flat-guarded site is cross-coop — see the call-site classification), the reason the
   model exists, and the highest-risk to get wrong.

**Hard rule:** no flat guard is removed without an **equivalent-or-stronger** entity-aware test at
that site. Authorization is never weakened in a migration step.

## Open questions / alternatives

- **Claim vs lookup.** Carry `entity_id` in the token, or resolve caller→entity at request time
  from the membership graph? (Token claim is simpler but must be minted correctly; lookup is
  authoritative but adds a registry read per request.)
- **Jurisdiction reconciliation.** Should `require_entity_access` and
  `require_*_in_jurisdiction` converge, or remain distinct authority surfaces? (Deferred.)
- **Where the decision runs.** App-layer helper (like `require_entity_write_access` today) vs a
  full entity-aware `PolicyOracle` returning a `ConstraintSet`. The latter is more firewall-pure
  but heavier; the former matches current practice.
- **Per-action capability source.** Roles' default `MembershipCapability` set vs explicit
  `AuthorityGrant`/`TypedScope` vs decomposed scopes (#1868) — likely a layered combination.

Identifier-change due-diligence (per `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md`):
introducing an `entity_id` claim is a new identifier surface; the convenience-vs-authority and
back-compat (`coop_id` projection) questions are addressed above and are the reason migration is
staged rather than a sweep.

## Follow-up slices (after this RFC)

1. **ADR** recording the decision (canonical identifier + primitive shape) once this RFC converges.
2. **Implementation issue(s):** the claim/resolution groundwork; `require_entity_access` +
   tests; treasury conversion; then per-family conversions; delegation last.
3. Convergence with **#1868** (per-action capability decomposition) at the capability axis.

## Related

- **#2061** — this RFC is the design artifact for that issue.
- **#1868** — decompose `governance:write` into per-action capabilities (the capability axis).
- **#1642** — gateway service-discovery auth + enumeration-safe 404.
- **#2052, #2056, #2058** — flat-regime hardening this generalizes (kept as defense-in-depth).
- **#2063** — the recurring `list_by_owner` DID-colon fix (already merged; the reliability bug
  #2061 noted is resolved and is **not** part of this authz model).
- `docs/architecture/{KERNEL_APP_SEPARATION,MEMBER_STANDING,INSTITUTION_PACKAGE_BOUNDARY,THE_COMMONS}.md`;
  `docs/adr/{ADR-0014,ADR-0019,ADR-0020,ADR-0027}`.
