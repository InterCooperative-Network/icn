---
id: "0017"
title: "Tool Install Infrastructure (ToolManifest, ToolBinding, ToolInstall lifecycle)"
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

# RFC 0017: Tool Install Infrastructure (ToolManifest, ToolBinding, ToolInstall lifecycle)

## Status

`draft` — being written, not yet ready for review. This RFC explores the L2 install infrastructure already named in [`docs/architecture/COOPERATIVE_TOOL_COMMONS.md`](../architecture/COOPERATIVE_TOOL_COMMONS.md) § *Missing buildout*: `ToolManifest`, `ToolBinding`, `ToolInstall` lifecycle, tool capability registry per `InstitutionalDomain`, and service identity audit trail.

**Accepted RFC does not mean implemented.** Implementation lands under follow-up ADR(s) and issues with code/test evidence.

## Summary

`COOPERATIVE_TOOL_COMMONS.md` (status: design-direction, last reviewed 2026-04-27) names a base-tool catalog (`icn-domain-admin`, `icn-member-directory`, `icn-governance`, `icn-meetings`, `icn-action-cards`, `icn-drive`, `icn-docs`, `icn-tables`, `icn-forms`, `icn-calendar`, `icn-publish`, `icn-search`, `icn-agreements`, `icn-budget`, `icn-signals`, `icn-compute-jobs`, `icn-directory`) and a *Missing buildout* section listing five infrastructure objects that need to land before any tool can be installed by a coop on its own ICN node:

- `ToolManifest` — declares capabilities, data touched, storage needs, privacy classes, UI surfaces, compute jobs, schemas, receipts emitted.
- `ToolBinding` — per-institution configuration record. Carries the institution's specific values that fill a generic tool's slots.
- `ToolInstall` lifecycle — submit → review → approve → bind → run → suspend → upgrade → fork → remove.
- Tool capability registry per `InstitutionalDomain` — what tools exist in this domain, what scopes they hold, what receipts they emit.
- Service identity audit trail — per-tool history visible through the existing receipt query path.

This RFC explores how to land those five infrastructure objects coherently, integrated with the existing receipt envelope (ADR-0026), action card contract (ADR-0027), authority scope plumbing (PRs #1626/#1627/#1630), and the meaning firewall (`KERNEL_APP_SEPARATION.md`). The recommended direction is to land all five as a single substrate package (one new crate or extension of an existing crate, decided by architecture review), with the lifecycle integrated as governance-driven `Activity` records — install is a governance act, not a marketplace transaction.

## Problem statement

ICN today has no mechanism for an institution to declare which cooperative tools it has installed, what scopes those tools hold, or how those tools' actions are audited. Every base tool named in `COOPERATIVE_TOOL_COMMONS.md` (none of which exist as deployed code today) will need this infrastructure to exist *before* it can ship.

The downstream consequence is that the second-adopter narrative — "RegionalCoopNet spins up an ICN node and installs `icn-member-directory` + `icn-tables`" — has no install verb today. Without `ToolManifest`/`ToolBinding`/`ToolInstall`, the only adoption path is "fork the entire NYCN repo and run it," which is exactly the failure mode the cooperative-tool architecture is meant to avoid.

The five infrastructure objects above were named in the parent doc explicitly: *"Each of these will land via its own ADR or RFC. This document does not pre-commit shapes."* This RFC takes that pre-commitment.

## Goals

- Define `ToolManifest` such that a tool author can declare what their tool needs (capabilities, data scopes, storage, privacy classes, UI surfaces, schemas, receipts) without the kernel pattern-matching on tool-specific keys.
- Define `ToolBinding` such that an institution can record how *their* domain has chosen to use a generic tool (e.g. "NYCN binds `icn-tables` to its sponsor pipeline schema with these field overrides"). Per `INSTITUTION_PACKAGE_BOUNDARY.md`: institution-specific values stay in the binding, never in the tool.
- Define the `ToolInstall` lifecycle states and the governance-act mapping for each transition.
- Define the tool capability registry: a per-`InstitutionalDomain` view of installed tools, granted scopes, and emitted receipts.
- Define service identity audit trail integration with the ADR-0026 receipt envelope so that every tool action is provenance-bearing.
- Preserve the anti-capture rule (per `COOPERATIVE_TOOL_COMMONS.md`): a tool that cannot answer "how does the institution leave you cleanly?" is not installable.
- Preserve the no-marketplace stance: tool install is a governance act, not a transaction.

## Non-goals

- **No tool runtime sandboxing details.** WASM, OS-level isolation, capability-based security mechanics belong in adjacent RFCs/ADRs and reference the existing `icn-compute-jobs` substrate (ADR-0030).
- **No specific tool implementation.** This RFC is about the install/binding mechanism; `icn-member-directory`, `icn-tables`, etc. land under their own implementation work.
- **No marketplace, app store, ranking, or recommendation logic.** Install is governance.
- **No third-party tool registry beyond the per-domain `InstitutionalDomain` capability list.** A federation-wide tool catalog may emerge later but is not in this RFC's scope.

## Background / current state

- [`COOPERATIVE_TOOL_COMMONS.md`](../architecture/COOPERATIVE_TOOL_COMMONS.md) — names the buildout. Defines the install flow conceptually (`tool package submitted → manifest declares capabilities → institution reviews authority request → governance / admin approves install → tool receives ServiceIdentity → tool accesses only granted scopes → tool actions produce receipts → tool can be suspended, upgraded, forked, removed`). Defines tool runtime modes (local node service, frontend-only view, compute workload, bridge adapter, shared service, workstation app, mobile app, package template).
- [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](../architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) — parent doc; `InstitutionalDomain` is the per-coop runtime container that tools install into.
- [`INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) — pins the rule: generic shapes in ICN, institution-specific values in packages. `ToolManifest` is generic; `ToolBinding` is the institution's binding.
- [`KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md) — kernel never branches on tool-specific keys; capabilities and scopes are typed; meaning firewall preserved.
- ADR-0026 (Receipt and Provenance Proof Envelope) — the receipt path tools emit into.
- ADR-0027 (Action Card Contract) — action cards are derived views over institutional state; tools may compose action cards; this RFC must not contradict the contract.
- ADR-0030 (Compute Workload Manifest and Authority Boundary) — bridge adapter and compute-job tool runtime modes plug into existing compute manifest constraints.
- ADR-0031 (Commons Compute Admission and Settlement Policy) — shared-service tool runtime mode references this.
- PR #1626 (person-directory overlay), PR #1627 (`/me/standing`), PR #1630 (authority scope plumbing) — substrate that `ToolBinding` capability grants extend.

## Design options

### Option A — Single combined RFC, single substrate crate

Land all five infrastructure objects (`ToolManifest`, `ToolBinding`, `ToolInstall` lifecycle, capability registry, service identity audit trail) as one substrate package. New crate `icn-tools` (location TBD; may be `icn/crates/icn-tools/` or a new module under existing crate per architecture review). Single PR per phase; one mental model for adopters.

### Option B — Split into separate RFCs and crates

Each of the five objects gets its own RFC and its own crate. Slower but lower per-PR review surface. Risk: bindings depend on manifests, lifecycle depends on bindings, registry depends on lifecycle — sequencing dependencies make the split painful.

### Option C — Hybrid: one RFC, multiple crates

Single RFC for coherent design; implementation split across two or three crates (e.g. `icn-tool-manifest`, `icn-tool-runtime`, `icn-tool-registry`) if the crate review concludes a single crate is too large.

## Tradeoffs

| Option | Easier | Harder | Invariants preserved | Invariants stressed | New failure modes |
|---|---|---|---|---|---|
| A — single crate | one mental model; one review thread; coherent type contract | larger initial PR; one place that holds all five concepts | meaning firewall (single-crate discipline easy to enforce) | none new beyond crate-size norms | a single point of failure in the crate's evolution |
| B — split RFCs/crates | smaller per-PR surface; independent evolution | sequencing pain (manifest → binding → lifecycle → registry → audit); five separate review threads; risk of inconsistent shape | none new | meaning firewall (more places to enforce) | divergence across the five surfaces; "where is `ToolManifest` defined" question |
| C — hybrid | single design coherence; smaller crate granularity | crate boundaries must be carefully drawn; slightly more wiring | same as A | same as A | crate-boundary drift over time |

## Core/package boundary

- **What lives in ICN core** under this RFC: the five infrastructure objects (manifest, binding, lifecycle, registry, audit trail). All five are generic shapes, not institution-specific.
- **What lives in institution packages**: the binding *content* — NYCN's specific schemas for the bound tools, the institution's specific capability requests, the institution's specific approval-flow values per its charter. NYCN's `tool_bindings` block in `institution/package.yaml` is exactly this.
- **What stays opaque to the kernel**: the contents of capability declarations beyond their type-safe shape. Tools may declare `custom_capability: "review-sponsor-recognition"` and the kernel must not branch on the string.

## Accessibility implications

The install flow is governance-mediated — it surfaces in `icn-domain-admin` and the institution's governance process. Per ADR-0028 (accessibility baseline): the install flow's UI must be plain-language, mobile-first, and operable on low-bandwidth devices for organizers reviewing tool authority requests.

## Conflict / dispute path

- **Disputed install approval.** Reverse the install through a new governance act (lifecycle: `installed` → `suspended` → `removed`). The audit trail preserves the original install.
- **Disputed tool action.** Tools emit receipts; the receipt envelope (ADR-0026) is the authoritative log. Disputes resolve through the existing dispute surface (ADR-0029 candidate).
- **Tool fork after disagreement.** The lifecycle includes `fork`; an institution may fork a tool's binding without removing the canonical tool. The fork has its own `ToolBinding`.

## Security / privacy implications

- **Capability over-grant risk.** A `ToolManifest` declaring excessive scopes could be approved by inattentive governance. Mitigation: standardized scope declarations, plain-language summaries surfaced at approval time, and a default deny-by-default posture that requires explicit grant per scope.
- **Service identity capture.** A compromised tool with a service identity could act on the institution's behalf. Mitigation: receipts on every action, suspension lifecycle state, and a capability registry that lets the domain answer "what does this tool currently hold?" at any time.
- **Bridge adapter exfiltration risk.** Tool runtime mode "bridge adapter" moves data between ICN and external systems. Mitigation: every bridge action emits a `BridgeImportReceipt` (per `COOPERATIVE_TOOL_COMMONS.md`); no tool can move data silently.

## Compute / automation boundary

- Tools may compute over institution-owned state with declared capabilities. Tools may not mutate state without producing a receipt.
- Compute workloads (per ADR-0030) used by tools follow the existing compute manifest constraints; this RFC does not introduce a parallel compute path.
- Determinism and fuel bounds apply where compute is invoked (per ADR-0030 / ADR-0031).

## Website / public truth implications

Once accepted and implemented, the canonical site can claim that ICN supports cooperative-tool install and binding as a substrate primitive. The site does **not** claim a marketplace, an app store, a tool ranking, or any tool catalog beyond the per-domain capability list. Maturity band moves from "design-direction only" to "early" once the install lifecycle has at least one shipped tool flowing through it (probably PR C1 in the build plan: `icn-member-directory`).

## Migration / compatibility

Greenfield substrate. No migration cost for existing deployments, governance proposals, CCL contracts, API consumers, or test surfaces. The first tools to consume this infrastructure (`icn-member-directory`, `icn-tables`) are themselves unbuilt at the time of this RFC.

## Open questions

1. **Crate placement** — Option A (single new crate) vs Option B (split) vs Option C (hybrid). Architecture review per `INSTITUTION_PACKAGE_BOUNDARY.md` *Reusable Primitive Set* discipline should decide. Default recommendation: Option A.
2. **Manifest format** — YAML, JSON, CCL, or a Rust-typed declaration that compiles into a manifest? Tradeoffs across authoring ergonomics, kernel-discipline review surface, and CI validation cost.
3. **Lifecycle as `Activity`?** Should the install lifecycle reuse the existing `Activity + Milestone` substrate, or is it a new lifecycle type? Existing primitive likely suffices; verify.
4. **Capability declaration vocabulary** — Should ICN ship a starter set of capability strings (`read-relationships`, `emit-action-cards`, `bridge-import-from-external`, etc.) as documentation only, or leave declarations entirely free?
5. **Service identity rotation and revocation** — When a tool is suspended or removed, how is its service identity invalidated across the network? Reuses existing key rotation patterns, or needs new mechanism?
6. **Cross-domain tool installation** — A federation may want to share a tool install across member entities. Out of scope for this RFC; flag as future work.
7. **Manifest signing** — Who signs a `ToolManifest` (tool author? distributor? institution at install time)? Implications for trust graph and supply-chain security.

## Decision criteria

- Pick **A (single crate)** if the architecture review concludes the five concepts evolve coherently and the crate is small enough to maintain. Default expectation.
- Pick **B (split)** if architecture review identifies distinct lifecycles for the five objects (e.g. `ToolManifest` is mostly static while audit trail evolves continuously). Unlikely.
- Pick **C (hybrid)** if architecture review concludes a single crate would exceed reasonable size after first-cycle implementation; split along natural seams (manifest+binding vs lifecycle vs registry+audit).

## Outcome

To be filled when the RFC moves to `accepted` or `rejected`.

## Follow-up ADRs

If accepted: at least one new ADR recording the crate-placement decision and the type contract. Possibly additional ADRs for capability vocabulary and service identity audit trail integration with ADR-0026.

## Follow-up implementation issues

If accepted:

- New issue: implement `ToolManifest` per chosen option.
- New issue: implement `ToolBinding` and the per-domain capability registry.
- New issue: implement `ToolInstall` lifecycle with governance-act integration.
- New issue: integrate service identity audit trail with ADR-0026 receipt envelope.
- New issue: add `tool_bindings` block to NYCN's `institution/package.yaml` once binding format is concrete (this is NYCN-side work that depends on PR B landing).

## Validation / proof plan

- Unit tests in the new crate(s) covering: manifest parse, binding validation, lifecycle state transitions, capability registry queries, audit trail receipt formation.
- Integration test: an end-to-end install of a fictional tool (smallest possible scope) goes through the lifecycle, emits receipts at each transition, appears in the capability registry, and can be cleanly suspended and removed.
- CI check: kernel crates do not import the tools crate (firewall preserved).
- CI check: forbidden module paths (`icn-governance::sponsor`, etc., per `INSTITUTION_PACKAGE_BOUNDARY.md`) are not introduced via tool-binding back-doors.
- Documentation update: `COOPERATIVE_TOOL_COMMONS.md` § *Missing buildout* table marked as landed (or partially landed per option chosen).

---

## Notes

This RFC is a stub. The design space is sketched; the recommended option is named (Option A — single combined RFC and crate, default; Option C as fallback if size demands) but not committed. Drafting will deepen the type contract sketches, the install-flow sequence diagram, and the integration with ADR-0026 / ADR-0027 in particular.
