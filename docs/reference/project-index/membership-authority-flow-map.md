---
Status: descriptive
Canonical: no
Last Reviewed: 2026-11-23
---

# Membership Authority Flow Map

> Navigation only. This map routes membership questions to the code and documents that own them. It decides nothing: identity meaning is owned by [`IDENTITY_SEMANTICS.md`](../../architecture/IDENTITY_SEMANTICS.md); grant/mandate meaning by [ADR-0014](../../adr/ADR-0014-constitutional-object-model.md) and ADR-0019; request authorization by [ADR-0035](../../adr/ADR-0035-entity-aware-request-authorization.md); standing read model by [`MEMBER_STANDING.md`](../../architecture/MEMBER_STANDING.md) / ADR-0020; the legacy-rail firewall by [`GOVERNANCE_HUMAN_ACTOR_MODEL.md`](../../architecture/GOVERNANCE_HUMAN_ACTOR_MODEL.md). Paths are as of `main` 951e62aa5 — inspect the source before relying on a line number.

## The intended flow

```text
Institution (EntityId / governance domain; holds no key — IDENTITY_SEMANTICS §2.3)
  → governance decision (proposal → vote → close; GovernanceProofV2 receipt)
    → membership record / credential (today: CoopStore member row; no portable credential)
      → authorization (gateway guards, governance handlers)
        → other services (ledger, treasury, entity, federation, …)
```

In the code today these are **three separate membership stores with no synchronisation**, so the arrow from "decision" to "authorization" is not one path.

## Where membership is stored

| Store | Key / shape | Written by | Through governance? |
|---|---|---|---|
| Governance domain voter list | `MembershipConfig { StaticList(Vec<Did>) \| TrustThreshold(f64) }` inside `gov:domain:{id}` (`icn/crates/icn-governance/src/membership.rs`, `icn/apps/governance/src/state_store.rs`) | domain creation; `POST/DELETE /gov/domains/{id}/members` → `update_domain_membership` (`icn/apps/governance/src/http/handlers.rs` ~629/675) | **No** — direct HTTP by an existing member |
| Cooperative member records | `member:{coop_id}:{did}` (`icn/crates/icn-coop/src/store.rs`) | accepted `ProposalPayload::Membership` → `MembershipEffect` → `KernelMembershipExecutor` → `MembershipServiceImpl` (`icn/apps/governance/src/handlers/execution.rs`, `icn/crates/icn-core/src/services/membership_service.rs`); also `POST /coops/{id}/members` (`icn/crates/icn-gateway/src/api/coops.rs`) | **Both** — governed path and an admin path |
| Commons affiliation / standing | `CommonsHolderRecord.affiliations` (`icn/crates/icn-identity/src/commons.rs`) | gateway `/v1/membership/*` (office-holder approval), SDIS enrollment (`icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`) | **No** |
| Entity memberships | `icn_entity::Membership` | gateway entity routes (`entity:write`) | No |
| Federation membership | federation registry | accepted federation effects (`icn/crates/icn-core/src/services/federation_service.rs`) | Yes |

## Where each membership decision is taken

| Decision | Location | Reads |
|---|---|---|
| Cross-coop request guard (ledger, treasury, escrow, registry) | `require_coop_access` (`icn/crates/icn-gateway/src/middleware.rs`) | JWT `coop_id` claim only |
| Entity-aware authorization | `require_entity_access` (`icn/crates/icn-gateway/src/authority.rs`; ADR-0035) | entity memberships |
| Proposal submit / vote (HTTP) | `member_checker` + `suspension_checker` (`handlers.rs` ~857/1845) | commons standing; CoopStore suspension — **not** the domain voter list |
| Vote (daemon actor) | `GovernanceCommand::CastVote` (`icn/apps/governance/src/actor.rs`) | none on `main` (a StaticList check is proposed, not merged) |
| Close authorization (HTTP) | `handlers.rs` ~1309 | StaticList contains; TrustThreshold → allowed |
| Close-time revalidation | `handlers.rs` ~1326 → `eligible_voters` | commons standing of each voter |
| Quorum denominator | `resolver.resolve_members` (`actor.rs` ~2337) | domain list, or trust graph for TrustThreshold |
| Domain-scoped reads (~20 endpoints) | `check_domain_membership` (`handlers.rs` ~83) | domain list; TrustThreshold via resolver |

## Known gaps (each owned elsewhere)

- **The stores disagree.** An accepted Membership proposal writes only CoopStore; it never updates the domain voter list, and commons admission touches neither. A member admitted by governance is outside the quorum denominator; a commons Member outside a StaticList could vote over HTTP.
- **Unfiltered close counts every stored vote.** When `eligible_voters` is `None` (RPC/trait close, scheduler auto-close) the tally counts all stored rows, members or not — for StaticList and TrustThreshold alike.
- **TrustThreshold eligibility is unresolved.** No cast-time check exists in the actor or manager; the resolver result is used only as the quorum denominator; scores are computed from the closing node's own trust perspective. This bears directly on whether a TrustThreshold decision reflects its electorate. Owner decision pending.
- **No portable membership credential.** Offline verification can show that a node attested an accepted membership change (payload binding, in review), not institutional authorization or current membership. Institutional attestation and credential issuance belong to GEN [#2602](https://github.com/InterCooperative-Network/icn/issues/2602); authenticated institutional standing to [#2441](https://github.com/InterCooperative-Network/icn/issues/2441); the Subject → governance → receipt convergence to [#2694](https://github.com/InterCooperative-Network/icn/issues/2694).
- **Jurisdiction naming differs** between enrollment (`coop:{coop_id}`), `member_checker` (raw `domain_id`) and treasury (`domain_id == coop_id`).
- **Duplicate code:** `icn/apps/membership` duplicates icn-coop / icn-entity (same `member:` keys) and has no dependents.

## Related maps

- [`runtime-surface-map.md`](runtime-surface-map.md) · [`identity-crypto-map.md`](identity-crypto-map.md) · [`source-of-truth-map.md`](source-of-truth-map.md)
- Design records: `docs/design/entity-aware-auth-control-map.md`, `docs/design/coop-id-entity-resolver.md`, `docs/design/membership-coop-core-map-parity.md`, `docs/design/membership-durable-timestamp-semantics.md`, `docs/architecture/AUTHORITY_SPINE.md`, `docs/architecture/THE_COMMONS.md`.
