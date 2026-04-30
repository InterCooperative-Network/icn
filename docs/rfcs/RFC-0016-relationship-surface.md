---
id: "0016"
title: "Generic Relationship Surface (RelationshipRecord primitive)"
status: "draft"
created: "2026-04-30"
updated: "2026-04-30"
authors: ["Matt Faherty"]
reviewers: []
related_adr_candidates: []
related_issues: []
supersedes: []
superseded_by: []
---

# RFC 0016: Generic Relationship Surface (RelationshipRecord primitive)

## Status

`draft` — being written, not yet ready for review. This RFC explores adding a generic relationship/contact substrate primitive (`RelationshipRecord`) to the ICN reusable primitive set, and is a stated prerequisite of the `icn-member-directory` base tool named in [`docs/architecture/COOPERATIVE_TOOL_COMMONS.md`](../architecture/COOPERATIVE_TOOL_COMMONS.md).

**Accepted RFC does not mean implemented.** Implementation lands under follow-up ADR(s) and issues with code/test evidence.

## Summary

ICN today models *authority* relationships through `RoleAssignment` (DID + structure + capability strings) and *entity participation* through `Federation.member_entities`. It does not model the much larger surface of **non-authority relationships**: who is the relationship owner for a partner co-op, when was an organization last contacted, who is the next follow-up due from, what consent boundaries apply to a contact.

This RFC explores a single generic substrate primitive, `RelationshipRecord`, that captures non-authority relationships between any two DIDs (subject and counterparty), with an opaque `relationship_kind: String` so institutions can carry their own taxonomy without leaking institution-specific vocabulary into ICN core. The recommended direction is to add `RelationshipRecord` to the *Reusable Primitive Set* table in [`INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) and implement it with the same anti-ontology-laundering discipline already used by `ProgramKind::Custom(String)` and `Milestone::completion_criteria: Vec<String>`.

## Problem statement

Multiple cooperative-tool surfaces currently in the design pipeline need to point at a non-authority relationship between DIDs:

- The `icn-member-directory` base tool (planned per `COOPERATIVE_TOOL_COMMONS.md`) renders contact directories, relationship pipeline views, and follow-up queues for organizers. None of these surfaces are pure authority assertions; they are relational state.
- Specialized suites composed at L2/L3 (sponsor / fundraising, organizer recruitment, partner workspace) all need to track *who owns the relationship*, *when did we last touch it*, and *when is the next action due*. These are not `RoleAssignment` data — they are observation/intention records.
- `INSTITUTION_PACKAGE_BOUNDARY.md` § *Stays Out of ICN Core* explicitly puts CRM out of bounds: "CRM shape and field semantics vary too much across institutions." That is correct as a boundary on **CRM**. But the underlying primitive (a generic relationship record between DIDs) is institution-agnostic if it carries no institution-specific enum variants.

Without a generic primitive, every consumer (every base tool, every specialized suite) reinvents a relationship surface in its own crate, drifting toward institution-specific shapes. With a generic primitive, the L2 tools point at one type and the L3 institution-specific vocabulary stays in package fields and CCL semantics.

## Goals

- Define a `RelationshipRecord` primitive that lives in ICN core and is consumable by base tools without leaking institution semantics.
- Preserve the firewall: no institution-specific enum variants. `relationship_kind` is `String` (or equivalent free-form opaque tag).
- Carry only the fields that are universal across institutions: subject DID, counterparty DID, owner DID, lifecycle state, last-contact timestamp, next-followup timestamp, consent scopes.
- Emit receipts on lifecycle-state-change events through the existing receipt envelope (ADR-0026).
- Be consumable by `icn-member-directory` and by future base tools (`icn-tables`, `icn-action-cards`) without each tool re-deriving relationship state.

## Non-goals

- **No CRM logic in ICN.** Pipeline stages, contact-history rendering, follow-up cadence rules, and any field whose meaning is institution-specific belong at L3 in the institution package.
- **No participation taxonomy in ICN core.** `kind: Speaker | Sponsor | Attendee` would import institution vocabulary into the kernel; that is forbidden per `INSTITUTION_PACKAGE_BOUNDARY.md`.
- **No private contact data in core records.** Real contact info lives in the institution's private overlay or future ScopedVault. The primitive carries DIDs and references, not phone numbers, emails, or notes.
- **No replacement for `RoleAssignment`.** Authority grants stay in the existing `RoleAssignment` shape; this primitive does not overlap.

## Background / current state

- **Relevant existing primitives:** `RoleAssignment` (`icn-governance::structure`); `BootstrapEntityRecord` and `EntityKind` (`icn-governance::bootstrap`, `icn-entity`); person-directory overlay (PR #1626); `/me/standing` (PR #1627).
- **Relevant boundary docs:** [`INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) § Reusable Primitive Set (lists existing primitives) and § Stays Out (explicitly excludes CRM).
- **Relevant tool catalog:** [`COOPERATIVE_TOOL_COMMONS.md`](../architecture/COOPERATIVE_TOOL_COMMONS.md) names `icn-member-directory` as a planned base tool that reads from the person-directory overlay and `/me/standing` and renders member profiles.
- **Relevant kernel discipline:** [`KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md) — kernel reads pure data types; semantic interpretation is confined to apps. `RelationshipRecord` must not require kernel pattern-matching on `relationship_kind`.

## Design options

### Option A — New substrate crate `icn-relationships`

Create a new substrate crate that defines `RelationshipRecord`, persistence, and receipt emission. Apps consume it through a service trait analogous to `TrustService`. Stays out of `icn-governance` to avoid coupling relationship state to governance state.

### Option B — New module under `icn-governance`

Add `icn-governance::relationship` alongside `::structure`, `::activity`, `::program`. Reuses existing storage, governance receipt machinery, and HTTP plumbing in `apps/governance`. Risk: blurs the line between governance state and relational state; could invite future drift.

### Option C — Defer; keep relationship state at L3 only

Do nothing in core; require each L3 institution package to carry its own relationship records. Tools at L2 receive relationship data through package config rather than a typed primitive. Lower architectural cost; higher per-tool reinvention cost.

## Tradeoffs

| Option | Easier | Harder | Invariants preserved | Invariants stressed | New failure modes |
|---|---|---|---|---|---|
| A — new crate | clean isolation, easier to evolve, easier kernel-discipline review | new crate to maintain, new receipt path to wire, more places to look for "where does relationship state live" | meaning firewall, kernel/app separation | none new | crate proliferation; service-injection complexity |
| B — module under `icn-governance` | reuses governance machinery, no new crate | governance state and relationship state co-located; review burden on every governance change | reuses existing patterns | meaning firewall (relationship semantics are not governance semantics) | governance receipts and relationship receipts could be confused; cleanup later is costly |
| C — defer | zero substrate work | every L2 tool reinvents relationship records; second-adopter story breaks ("you have to write a relationship type yourself") | none new | reusable-primitive principle (ICN has the generic shape institutions need) | per-tool drift; relationship state never receipt-bearing |

## Core/package boundary

Per [`INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md):

- **What lives in ICN core** under this RFC: the generic `RelationshipRecord` type with opaque-string `relationship_kind`. Storage. Receipt emission on lifecycle-state-change. Service trait for app consumption.
- **What stays in institution packages**: the actual taxonomy (which `relationship_kind` values exist for *this* institution), the lifecycle rules (when does a sponsor relationship close), the rendering vocabulary (do we call them sponsors, funders, partners, member co-ops), the consent enforcement specifics.
- **What stays opaque to the kernel**: every value of `relationship_kind`. Kernel must not pattern-match on it. The pattern follows `ProgramKind::Custom(String)` and `Milestone::completion_criteria: Vec<String>`, both of which the program module already demonstrates.

## Accessibility implications

`RelationshipRecord` is a substrate primitive with no member-facing surface of its own. Accessibility lands in the consumer tool surfaces (`icn-member-directory`, future suite UIs). The primitive must not block low-bandwidth operation: lifecycle-state-change receipts should be small enough that a member shell can render the relationship history without paginating large auxiliary data.

## Conflict / dispute path

Disputes about a `RelationshipRecord`:

- **Wrong owner_did asserted.** Same path as governance disputes — challenge the record through the institution's existing dispute surface (ADR-0029 candidate).
- **Disputed lifecycle-state-change.** The receipt envelope (ADR-0026) is the authoritative log; reverting requires a new state-change receipt referencing the prior one.
- **Consent scope dispute.** The institution's privacy/redaction policy governs (ADR candidate for ScopedVault).

## Security / privacy implications

- DIDs are public; `RelationshipRecord` linking two DIDs is itself public unless scoped under a vault.
- Real contact info never appears in the primitive. Phone, email, and free-text notes live in the institution's private overlay or future ScopedVault.
- `consent_scopes: Vec<String>` lets institutions tag a relationship with consent boundaries (e.g. `public_listing`, `action_card_delivery`, `photography`); the kernel does not interpret these strings, but tools can refuse to act outside declared scopes.

## Compute / automation boundary

Tools may compute over `RelationshipRecord` (rendering pipelines, follow-up queue derivations, search). Compute may not mutate a record without producing a lifecycle-state-change receipt. Determinism and fuel bounds apply where compute jobs are invoked (per ADR-0030).

## Website / public truth implications

Once accepted and implemented, the maturity band for the relationship surface goes from "missing" to "early"; the canonical site can claim that ICN supports a generic relationship primitive that institutional packages compose specialized contact / partner / sponsor surfaces from. The site does **not** claim ICN provides a CRM.

## Migration / compatibility

Greenfield primitive. No migration cost for existing deployments, governance proposals, CCL contracts, API consumers, or test surfaces. The first consumer (`icn-member-directory`) is itself unbuilt at the time of this RFC.

## Open questions

1. **Crate placement** — Option A (new `icn-relationships` crate) vs Option B (module under `icn-governance`). Architecture review per `INSTITUTION_PACKAGE_BOUNDARY.md` *Reusable Primitive Set* discipline should decide.
2. **Service trait shape** — Should `RelationshipService` mirror `TrustService` (per `KERNEL_APP_SEPARATION.md`)? Or is direct consumption from a store handle sufficient at the tool layer?
3. **Lifecycle states** — Is the lifecycle a fixed enum (`active | dormant | closed`) or an opaque string per the anti-ontology-laundering rule? Recommended: opaque string with a documented set of common states, but the kernel does not branch on them.
4. **Consent scope vocabulary** — Should ICN ship a starter set of consent scopes (`public_listing`, `photography`, etc.) as documentation only, or leave the field entirely free?
5. **Subject vs counterparty asymmetry** — Are relationships directional (subject → counterparty) or symmetric? Implications for query patterns and receipt routing.
6. **Receipt class** — Does `RelationshipReceipt` need to be a new receipt class in the ADR-0026 envelope, or does an existing class suffice with a new `kind` discriminator?

## Decision criteria

- Pick **A (new crate)** if the architecture review concludes that relationship state and governance state should evolve independently (likely outcome — relationship surface will grow features that have nothing to do with governance).
- Pick **B (under `icn-governance`)** if the architecture review concludes that the maintenance cost of a new crate outweighs the conceptual cleanup of separating concerns.
- Pick **C (defer)** if the build plan slows enough that no L2 tool consumer materializes within the next implementation window. (Unlikely given the build plan.)

## Outcome

To be filled when the RFC moves to `accepted` or `rejected`.

## Follow-up ADRs

If accepted: at least one new ADR recording the crate-placement decision and the type contract. Possibly a second ADR for the service trait if Option A wins.

## Follow-up implementation issues

If accepted:

- New issue: implement `RelationshipRecord` per chosen option.
- New issue: integrate with ADR-0026 receipt envelope for lifecycle-state-change receipts.
- New issue: add to `INSTITUTION_PACKAGE_BOUNDARY.md` *Reusable Primitive Set* table.
- New issue: prerequisite for `icn-member-directory` tool implementation.

## Validation / proof plan

- Unit tests in the new crate (or new module) covering: record creation, lifecycle-state-change emit, receipt formation, opaque `relationship_kind` round-trip, opaque `consent_scopes` round-trip.
- Integration test: a fictional relationship goes from `created` → `active` → `dormant` → `closed`, each transition produces a receipt, the receipt envelope can replay the history.
- CI check: kernel crates do not import the relationship crate (firewall preserved).
- Documentation update: `INSTITUTION_PACKAGE_BOUNDARY.md` *Reusable Primitive Set* table extended.

---

## Notes

This RFC is a stub. The design space is sketched; the recommended option is named (Option A — new substrate crate) but not committed. Drafting will deepen Options A and B in particular, and may add a sketch of the type contract in Rust pseudocode once architecture review converges.
