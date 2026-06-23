---
id: "0083"
title: "Institutional Domain and Domain Policy runtime root"
status: "proposed"
date: "2026-06-23"
deciders: ["Matt Faherty"]
tags: ["governance", "institutional-domain", "domain-policy", "authority", "meaning-firewall", "arch-invariants"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "not-started"
references:
  - "docs/spec/institutional-domain.md (the spec this ADR turns into a minimal runtime root; #1794)"
  - "docs/architecture/ICN_OPERATING_MODEL.md (Domain = governed jurisdiction that holds authority)"
  - "docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md (core generic shape vs package vocabulary)"
  - "docs/adr/ADR-0014-constitutional-object-model.md (AuthorityClass, AuthorityGrant, TypedScope, Mandate)"
  - "icn/crates/icn-governance/src/domain.rs (GovernanceDomain — existing decision space)"
  - "icn/crates/icn-governance/src/charter.rs (Charter — existing founding doc, ratify lifecycle)"
  - "icn/crates/icn-governance/src/authority.rs (AuthorityClass/AuthorityGrant/TypedScope)"
  - "icn/apps/governance/src/mandate_gate.rs (MandateGate::require — fail-closed authority gate)"
  - "GitHub #2142 (this runtime MVP), #1794 (spec), #1748 (process substrate), #1817 (CCL policy registry)"
---

# ADR-0083: Institutional Domain and Domain Policy runtime root

> **Note on ADR id:** numbered `0083` (next free id) because `ADR-0036` is reserved
> in the constitutional roadmap candidate registry for "federation agreement support"
> (`docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md`). The maintainer may reassign per the
> ADR-candidate-registry doctrine (ADR-0034).

## Status

`proposed`. `implementation_status: not-started` — this ADR pins the model, the
minimal runtime slice, and the migration boundary **before** any runtime type lands.
The follow-up implementation PR (acceptance criteria below) carries the code and is
what satisfies #2142. This ADR PR uses `Refs #2142`, not `Closes`.

## Context

The operating model (`docs/architecture/ICN_OPERATING_MODEL.md:113`) names **Domain**
as *"the governed jurisdiction that holds authority,"* and **Policy** (`:114`) as
*"adopted rule state … inert until a domain adopts it."* The spec
`docs/spec/institutional-domain.md` (status: spec, WIP; #1794) details
`InstitutionalDomain` as the governed jurisdiction (a ref-carrying object: canonical
identifier, owning entity class, charter refs, adopted CCL policy refs, membership /
service / storage / routing policy refs, receipts) and `DomainPolicy` as *"the
persistent shape of the rules a domain has adopted … An unadopted `DomainPolicy` is
inert"* (`institutional-domain.md:169`).

Today neither object exists in runtime code (verified: no `struct/enum
InstitutionalDomain` or `DomainPolicy` anywhere under `icn/crates` or `icn/apps`; the
only `DomainPolicy*` token is the unrelated kernel `AuthorityBasis::DomainPolicyClause`
in `icn-kernel-api/src/proofs.rs`). What does exist:

- **`GovernanceDomain` / `GovernanceDomainId`** (`icn-governance/src/domain.rs:9,33`) —
  doc-commented as *"the decision space for a community"* (`:1`). Fields: `id`, `name`,
  `description`, `config: GovernanceConfig`, timestamps. It is a **config holder with no
  lifecycle flag**, created via `GovernanceManager::create_domain` (`apps/governance/src/manager.rs:3000`).
  It is **not** the governed jurisdiction; it is the decision space inside one.
- **`Charter`** (`icn-governance/src/charter.rs:97`) — *"Founding document for
  jurisdictions"* (`:1`). Carries `charter_id` (sha256 of content), `org_type`
  (`Cooperative|Community|Federation`), a `domain_id: String`, `status: CharterStatus`
  (`Draft|Active|Suspended|Dissolved`), founders, policies, amendments. Adoption is one
  step: `ratify()` moves `Draft → Active` (`charter.rs:308`). This is the closest
  existing object to a "jurisdiction," but it is a *founding document*, not the standing
  authority root, and it does not carry an adopted-policy *pointer*.
- **Authority basis (ADR-0014)** — `AuthorityClass` (`Representation|Execution|Attestation`),
  `AuthorityGrant` (`authority.rs:269`: class, grantor, grantee, `scope: TypedScope`,
  validity), `TypedScope` (`authority.rs:135`: optional `domain: Option<GovernanceDomainId>`
  + proposal_class / action_kind / amount_ceiling / time_window), and `Mandate`
  (`mandate.rs:175`, per-decision composition of grant ids). `MandateGate::require()`
  (`apps/governance/src/mandate_gate.rs:419`) is the **existing, synchronous, fail-closed**
  authority gate (rejects bad status / past-deadline / empty-grants before the actor check).

So ICN already has authority primitives and a decision space, but no object that
*holds* a domain's authority and *points at its currently-adopted policy*. #2142 asks
for the smallest runtime slice that closes that gap for one domain adopting one policy.

## Decision

Introduce two minimal **governance-app-layer** runtime objects and one adoption act.
All of this lives in `icn-governance` (and/or `apps/governance`); **none of it enters
any kernel crate** (`.claude/rules/kernel-boundary.md`).

1. **`InstitutionalDomain`** — a thin standing authority object for a governed
   jurisdiction. For the MVP it is keyed by the **existing** `GovernanceDomainId` (no
   rename, no fork of `GovernanceDomain`) and carries only:
   - the **owning entity class** — the governed entity taxonomy (`Individual` /
     `Cooperative` / `Community` / `Federation`). The implementation reuses an
     existing governance-layer enum rather than introducing a new one:
     `BootstrapEntityType` (`icn-governance/src/bootstrap.rs:139`, exactly these
     four) or `Charter`'s `OrgType` (`charter.rs:60`, three — omits `Individual`).
     Note `icn-entity::EntityType` is broader (it carries additional variants such
     as `Unknown`) and is **not** the intended shape. Exact type chosen at
     implementation (see Open questions),
   - an optional **adopted charter reference** (`CharterId`),
   - a single **`current_policy: Option<DomainPolicyRef>`** — the adopted-policy pointer.

   It embeds no charter text, no policy text, no membership rolls — only references and
   the adoption record, exactly as the spec requires (`institutional-domain.md:117`).
   `GovernanceDomain` stays the decision space; `InstitutionalDomain` is the authority
   wrapper that references it. A domain with no `current_policy` is a valid, declared-but-
   unbound domain.

2. **`DomainPolicy` / `DomainPolicyRef`** — a minimal, content-addressed reference to a
   policy version. The MVP stores a `DomainPolicyRef` (a stable, content-addressed
   identifier for a policy version) plus the minimal metadata needed to assert adoption;
   it does **not** store or interpret CCL text. **Inertness is structural:** a
   `DomainPolicyRef` confers authority/constraint **only** when it is the
   `InstitutionalDomain.current_policy`. Any other policy ref — never adopted, or
   superseded — is inert: referencing it yields no authority, no constraint, no effect.
   There is at most one current policy per domain; prior versions are history.

3. **Adoption is a governance act gated by the existing authority basis.** Setting or
   changing `current_policy` (and creating/activating the domain) is a transition that
   **requires an authority basis** expressed through the existing ADR-0014 objects
   (`AuthorityGrant` / `Mandate` / `TypedScope`) and checked through the existing
   `MandateGate::require()`. We do **not** invent a new authority primitive. The rule is
   **fail-closed**: a transition presented with a missing, empty, or ambiguous authority
   basis is rejected (the `MandateGate` already rejects empty-grants and bad status), and
   the domain's policy state is left unchanged. Capability scope is not a mandate
   (ADR-0035 / ABUSE_CASE_HARDENING) — adoption authority is a mandate check, not merely a
   `governance:write` token.

4. **Meaning firewall.** `InstitutionalDomain` and `DomainPolicy` carry only generic ICN
   vocabulary. No NYCN/Summit package nouns (sponsor, session catalog, summit track, …)
   appear in these core types — those stay in the package repo
   (`INSTITUTION_PACKAGE_BOUNDARY.md:21,217`). The types never enter kernel crates; the
   kernel continues to see only `ConstraintSet`/`KernelEffect`. Policy **evaluation**
   (CCL) stays outside the kernel and outside this MVP (`institutional-domain.md:175`).

## Minimal runtime MVP (the follow-up PR, not this ADR)

The smallest honest slice — TDD, in `icn-governance` / `apps/governance` only:

- Add `InstitutionalDomain { domain_id: GovernanceDomainId, owning_entity_class:
  <governed entity class — e.g. BootstrapEntityType>, charter_ref: Option<CharterId>,
  current_policy: Option<DomainPolicyRef> }`
  and `DomainPolicyRef` (content-addressed id + minimal metadata). Names/fields may be
  refined in review; the shape is what matters.
- Add the adoption act on `GovernanceManager` (e.g. `declare_institutional_domain` and
  `adopt_domain_policy`) that (a) requires an authority basis via the existing
  `MandateGate`/`AuthorityGrant` path and (b) sets `current_policy` only on success.
- Persist through the existing governance store; surface nothing new over HTTP in this
  slice (a `/me`-style read surface is a separate follow-up).

This MVP deliberately implements **only** the `Declare` and `Adopt charter/policy`
stages of the spec's domain lifecycle (`institutional-domain.md:200-210`) — not standing,
services, routing, federation, exit, or the full reference set.

## Relationship to existing types / migration

- **`GovernanceDomain`** — kept as-is (the decision space). `InstitutionalDomain`
  references it by `GovernanceDomainId`; we do **not** rename or merge it in this lane.
- **`Charter`** — referenced (`charter_ref: Option<CharterId>`), not absorbed. Charter
  `ratify` (`Draft→Active`) remains the charter's own lifecycle; domain-policy adoption is
  a **separate** act so a domain can re-adopt/amend policy without re-ratifying a charter.
- **`Mandate` / `AuthorityGrant` / `TypedScope` / `MandateGate`** — reused unchanged as
  the authority basis and fail-closed gate. `TypedScope.domain: Option<GovernanceDomainId>`
  already keys on the same identifier, so no identifier migration is required for the MVP.
- **`GovernedServiceBinding`** — out of scope; remains spec-only (#1815). `current_policy`
  is the only binding this MVP models.
- **`InstitutionPackage`** — remains a docs/boundary concept; the MVP adds no package
  runtime. Package vocabulary is supplied externally, never embedded in these core types.
- **Kernel `AuthorityBasis::DomainPolicyClause`** (`icn-kernel-api`) — unrelated repair-
  authority basis; this ADR does not touch it and must not be confused with `DomainPolicy`.

## Non-goals

- No full **CCL policy registry**, versioning, or evaluator-selection runtime (#1817).
- No CCL **evaluation** of policy (policy stays an inert reference in the MVP).
- No auth-model change; no entity-aware auth enforcement cutover (ADR-0035 lane).
- No standing/membership, service/tool/route/DNS bindings, federation, or exit runtime.
- No new receipt class and no kernel change.
- No NYCN/Summit-specific nouns in ICN core; no package-activation completion.
- No production / pilot / organizer / federation readiness; no live federation; no
  workflow engine.

## Acceptance criteria for the follow-up runtime PR

- Minimal generic `InstitutionalDomain` and `DomainPolicy`/`DomainPolicyRef` types exist
  in `icn-governance` (app layer), re-exported as needed; **not** in any kernel crate.
- A domain can be **declared** and can **adopt one** `DomainPolicyRef`, with the adopted
  ref retrievable as the domain's `current_policy`.
- **Unadopted policy is inert:** a `DomainPolicyRef` that is not `current_policy` yields no
  authority/constraint/effect (proven by test).
- **Ambiguous/missing authority fails closed:** an adoption attempt with no/empty/ambiguous
  authority basis is rejected and leaves policy state unchanged (proven by test, reusing
  the existing `MandateGate` fail-closed behavior).
- Existing `Mandate` / `AuthorityGrant` / `TypedScope` semantics are respected (reused, not
  reinvented).
- Tests are **generic** — no NYCN/Summit nouns; the `Meaning Firewall Check` +
  `Kernel Forbidden Dependencies` required CI gates stay green.
- A documented `GovernanceDomain` ↔ `InstitutionalDomain` relationship (this ADR) is linked
  from `docs/spec/institutional-domain.md`.

## Open questions (flagged, not decided here)

1. **Identifier:** the MVP keys `InstitutionalDomain` on `GovernanceDomainId` (a string).
   The spec wants a DID-style canonical identifier surviving node/route changes
   (`institutional-domain.md:64`). Whether to introduce a distinct `InstitutionalDomainId`
   (and migrate `TypedScope.domain`) is deferred to a follow-up.
   **Resolved for the persistence MVP — see the Addendum below (keep keying on
   `GovernanceDomainId`; the DID-style identifier stays deferred).**
2. **Domain vs decision space:** long-term, does `GovernanceDomain` become a sub-part of
   `InstitutionalDomain`, or do they stay sibling references? The MVP chooses references to
   avoid churn; the consolidation decision is deferred.
   **Resolved for the persistence MVP — see the Addendum below (stay sibling references;
   no consolidation).**
3. **One adoption act or two:** charter `ratify` vs per-policy `adopt` are separate in the
   MVP. Whether founding should atomically adopt an initial policy
   (`institutional-domain.md:203`) is deferred.
4. **DomainPolicy: stored object vs derived view:** the MVP stores a minimal ref; whether
   the full object is a stored record or a view over adoption receipts lands with the CCL
   policy registry (#1817).
   **Resolved for the persistence MVP — see the Addendum below (persist only the adopted
   `DomainPolicyRef` on the domain record; the `DomainPolicy` body stays with #1817).**
5. **`Coop`-prefixed vocabulary debt** (`DataLocality::CoopReplicated`, etc.,
   `ICN_OPERATING_MODEL.md:247`) is **not** renamed here; deferred.
6. **Entity-class type:** which existing governance enum the MVP reuses for
   `owning_entity_class` — `BootstrapEntityType` (four variants) or `Charter`'s
   `OrgType` (three) — and whether to unify them; deferred to implementation.

## Consequences

- **Easier:** one package can declare a governed domain and adopt one policy reference
  with a real, fail-closed authority check — the first runtime rung of
  `package → domain → policy` on the institutional spine. The model is pinned before code,
  so the follow-up PR is a small, reviewable slice rather than an open-ended build.
- **Harder / deferred:** the full jurisdiction object (standing, services, routing,
  federation, exit) and CCL policy *evaluation* remain spec-only; this ADR explicitly does
  not deliver them. The identifier and `GovernanceDomain`-consolidation questions are left
  open, which a later ADR must close before the model is considered stable.
- **Risk:** introducing `InstitutionalDomain` alongside `GovernanceDomain` and `Charter`
  adds a third domain-adjacent object. The migration section and open questions bound that
  risk by choosing references over renames and by deferring consolidation explicitly.

## Addendum (2026-06-23): InstitutionalDomain persistence model

> **Status: design decision, not yet implemented.** Resolves open questions Q1, Q2,
> and Q4 *enough to unblock the next implementation lane* (a persisted adoption path).
> This addendum amends ADR-0083 in place (the ADR is still `proposed`); it adds **no
> code**. The HTTP adoption route remains blocked until the seam below lands — see
> `docs/spec/institutional-domain.md` §"Domain-policy adoption: app boundary and
> HTTP-surface sequencing" (#2168).

### Why this is needed

The #2142 adoption path is complete and tested up to the governance app boundary:
the runtime root (#2162), gate-resolved adoption (#2164), and the
`GovernanceManager::adopt_domain_policy` seam (#2166). That seam takes a **caller-held
`&mut InstitutionalDomain`** because no durable `InstitutionalDomain` persistence
exists: there is no store, no load/save path, and no declare/create path
(`InstitutionalDomain::declare` is exercised only in tests). An HTTP route must
load → mutate → persist a domain, so it cannot be added honestly until a persistence
seam exists. Settling the seam touches the three open questions below, so they are
resolved here rather than implicitly in code.

### Decisions

1. **Identity / keying (Q1).** Persist `InstitutionalDomain` keyed by the **existing
   `GovernanceDomainId`**. Do **not** introduce a parallel `InstitutionalDomainId` for
   this lane. Rationale: the runtime root already keys on it, and
   `TypedScope.domain: Option<GovernanceDomainId>` already binds to the same identifier,
   so no identifier migration is needed. The spec's DID-style canonical identifier
   (`institutional-domain.md:64`) stays **deferred** — it becomes relevant only when a
   domain needs an identity that survives independently of its governance domain.

2. **Relationship to `GovernanceDomain` (Q2).** Store `InstitutionalDomain` as a
   **separate, sibling state record** keyed by `GovernanceDomainId`. Do **not** embed it
   into, or collapse, `GovernanceDomain` (the decision-space config object). They remain
   sibling references, exactly as the ADR's "Relationship to existing types" section
   already states. Rationale: preserves compatibility, avoids a broad migration, and
   keeps the authority wrapper separable from the decision-space config.

3. **`DomainPolicy` storage (Q4).** The persisted `InstitutionalDomain` record carries
   only its **adopted `current_policy: Option<DomainPolicyRef>`** (a content-addressed
   pointer) — exactly the runtime-root shape. Whether the full `DomainPolicy` **body** is
   a stored record or a view derived from adoption receipts stays **deferred to the CCL
   policy registry (#1817)**. This lane persists the domain's adopted *pointer*, not a
   policy registry.

   Q3 (one adoption act vs two) and Q5 (`Coop`-prefixed vocabulary debt) are **unaffected
   and remain deferred**.

### Smallest store seam (for the implementation lane, not this PR)

Prefer **extending the existing `GovernanceStateStore`** over a new store, because it is
already sled-backed and already wired into `GovernanceManager` as the `domain_store`
field — no new store, trait, manager field, or builder:

- Add two methods, mirroring how `save_close_intent` / `flush` were added as
  **default-implemented** trait methods (so the single existing implementor —
  `SledGovernanceStateStore`, the only `GovernanceStateStore` impl today, exercised in
  tests via a temporary sled store — keeps compiling and overrides only as needed):
  - `get_institutional_domain(&self, id: &GovernanceDomainId) -> Result<Option<InstitutionalDomain>>`
  - `save_institutional_domain(&self, domain: &InstitutionalDomain) -> Result<()>`
  - The default impls fail **closed** (return an `Err("institutional_domain persistence not implemented")`-style result), never silently succeed; `SledGovernanceStateStore` provides the real implementation (serde — `InstitutionalDomain` already derives `Serialize`/`Deserialize`). If a lighter in-memory `GovernanceStateStore` is wanted for unit tests, it would be added by the implementation lane — none exists today.

### Declare / create path (for the implementation lane)

- `GovernanceManager::declare_institutional_domain(domain_id, owning_entity_class, charter_ref)`:
  refuse if a record already exists for `domain_id`; persist a freshly `declare`d
  `InstitutionalDomain`. **Flagged sub-question (decide in the implementation lane):**
  declaring a governed domain is itself an authority-bearing act — whether `declare` must
  be mandate-gated (like adoption) or may be a bootstrap-time seam should be settled when
  the path lands; it must not silently permit unauthorized domain creation on a routable
  surface.

### Persisted adoption flow (for the implementation lane)

`GovernanceManager` gains a persisted variant that composes the existing pieces:
`load InstitutionalDomain by GovernanceDomainId` → existing
`adopt_domain_policy(&mut domain, policy, actor, now)` (real `DefaultMandateGate`
resolution + pure-core structural commit) → `save_institutional_domain` on success.
Returns the adopted `DomainPolicyRef`. Only **after** this lands is a thin HTTP route
honest (route → persisted manager method, never bypassing the seam).

### Non-goals (unchanged)

No broad CRUD subsystem; no `GovernanceDomain` migration/collapse; no CCL runtime or
policy-evaluator selection; no full policy registry; no service-binding runtime; no
package activation; **no HTTP route and no persistence code in this PR**; no auth-model
change; no entity-aware auth cutover; no production / pilot / organizer / federation
readiness.

## References

See frontmatter. Primary: `docs/spec/institutional-domain.md` (#1794),
`docs/architecture/ICN_OPERATING_MODEL.md`, ADR-0014, and the existing
`icn-governance` authority/domain/charter types cited inline above. Addendum context:
`apps/governance/src/state_store.rs` (`GovernanceStateStore`),
`apps/governance/src/domain_policy_adoption.rs` (the manager seam, #2166), and the
HTTP-surface sequencing note in `docs/spec/institutional-domain.md` (#2168).
