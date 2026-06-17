---
Status: normative
Authority: spec (scopes the Community Proof Spine 0.1 slice for issue #2084; defines what the proof demonstrates and, explicitly, what it does NOT; consumes the entity-authority primitive from RFC-0018 / #2079 and the receipt-binding pattern of ADR-0026)
Canonical: no
Last Reviewed: 2026-06-17
---

# Community Proof Spine 0.1

> **This is not the final community domain model; it is the first proof that the
> entity-authority spine can carry a community-shaped civic action.**

Issue: #2084. Scope: a small, honest, fixture/dev proof. Not a whole app, not a
claim of live community governance.

## What this proves

The civic loop, end to end, for a **community-shaped entity**:

```
belonging (entity membership) -> standing -> authority (require_entity_access)
  -> action -> receipt (blake3 bind) -> verification (recompute) -> plain-language explanation
```

Concretely, the runtime proof (`icn-gateway` `authority.rs`, module
`community_proof_spine`):

1. Registers a community as an `icn-entity` `Community` entity via
   `EntityId::community("maple-street-mutual-aid")`.
2. Gives it two individual members: a Steward (`Founder` membership) and a plain
   `Member`.
3. Runs the **real** `require_entity_access(.., EntityAction::ModifyEntity)`
   (RFC-0018 / #2079): the Steward is **allowed**, the plain Member is **denied**.
4. Binds the discharged community action card as a completion receipt — a `blake3`
   hash over a domain-separated, length-prefixed canonical record (the ADR-0026
   Layer-2 binding pattern).
5. **Verifies** by recomputing the hash, and asserts that **tampering** with any
   bound field changes the hash.
6. Produces a plain-language **explanation** of the authority basis (allow and
   deny).

A human-viewable **mirror** lives in the member shell
(`web/member-shell/`, `?mode=demo&set=community`): a community standing card, a
community action card with a civic `authority_basis`, and a completion receipt —
rendered by the existing renderers, no renderer rewrite.

## What this explicitly does NOT do (boundary)

- **It does not finish communities.** It does not implement a live community
  governance loop, endpoints, or resource pools.
- **It models a community as an `icn-entity` `Community` entity, not the
  `icn-community` crate.** `icn-community::Member` (a flat `{id, member_type,
  voting_weight, active}` list) and `icn-entity::Membership` are **not reconciled**.
  Communities are not yet synced into the entity registry. This proof deliberately
  uses the entity model so it can exercise the real `require_entity_access`.
- **No new endpoint.** No HTTP surface is added.
- **No new persisted production receipt type.** The receipt is a self-contained
  ADR-0026-style hash computed in the proof; it is not stored and is not a new
  governance/ledger receipt class.
- **No new `EntityAction` variant.** The community action reuses the existing
  `ModifyEntity` (Founder/BoardMember threshold) — community Stewards map to that
  role gate. Civic-specific action semantics are future work.
- **No enforcement change.** No live authorization decision is altered anywhere.
- **No kernel change; Meaning Firewall preserved.** Authority is decided at the
  gateway/app layer over generic entity memberships; the kernel sees nothing of
  "community."
- **The member-shell fixture is a viewable mirror, not a live loop.** Nothing is
  signed; the fixture receipt hash is illustrative (the shell labels it as a demo
  hash). The canonical binding is proven only by the runtime test.

## Future work (not this slice)

- Reconcile `icn-community` ↔ `icn-entity` (a community lifecycle that registers an
  entity and syncs memberships) — candidate for its own ADR.
- Community-native action cards / standing / completion-receipt surfaces.
- Whether community civic actions warrant their own `EntityAction` semantics (only
  when a real endpoint family needs it, per the RFC-0018 discipline).

## References

- #2084 (this slice); RFC-0018 (#2074) + ADR-0035 (entity-aware authorization,
  `require_entity_access`); ADR-0026 (receipt/provenance proof envelope, blake3
  binding); ADR-0027 (action-card contract); `icn-entity` (`EntityId::community`,
  `EntityKind::Community`).
