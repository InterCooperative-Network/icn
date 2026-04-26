---
id: "0024"
title: "Institution Package Manifest Schema"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["package", "manifest", "schema", "institution-boundary", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partial"
references:
  - "ADR-0017 (Monorepo Consolidation)"
  - "ADR-0020 (Bootstrap Activation and Standing Read Model)"
  - "docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md"
  - "icn/crates/icn-governance/src/bootstrap.rs"
  - "GitHub #1628 (operating_purpose at apply time)"
  - "GitHub #1635 (support-institution package pattern)"
---

# ADR 0024: Institution Package Manifest Schema

## Status

**Proposed (2026-04-26).** Names the forward direction. Back-fills the partial reality (BootstrapSeedManifest covers entity/structure/role primitives) and identifies the gap (no formal schema for institution-specific extensions like indicators, signal rules, allocation templates). Modifiable.

## Context

ADR-0020 named the activation chain from manifest-on-disk to a member-facing standing surface and locked the contract surface ICN owns at each step. Step 1 is "package manifest schema." Today, `BootstrapSeedManifest` covers the four primitive seeds (entity, structure, activity, role) and a person-directory overlay shape, with `operating_purpose: Option<String>` as the sanctioned home for institution-specific taxonomy.

What does **not** exist today is a formal schema for the broader institution-package contract:

- Indicator definitions (what does this institution count and how?)
- Signal rules (what observations trigger what attention?)
- Allocation templates (what shape does a fresh allocation request take here?)
- Locale and accessibility metadata (which languages, which assistive defaults?)
- Package-level operating intent and stewardship metadata.

Without a schema, packages extend ad-hoc. Different packages develop different shapes for the same concept; the boundary contract erodes.

## Decision (proposed)

Institution packages declare a `package.yaml` manifest validated against a stable schema with the following sections:

1. **Identity.** Package name, version, ICN-runtime-version-target, package authority chain.
2. **Bootstrap seeds.** Entity, structure, activity, role records as today (`BootstrapSeedManifest`).
3. **Person-directory overlay reference.** Pointer to the private overlay file (real-DID binding, never in public package).
4. **Indicator definitions.** Named indicators with kind (count, ratio, presence), source (which records to count), schedule (how often to recompute).
5. **Signal rules.** Named rules with trigger condition, attention target, escalation policy.
6. **Allocation templates.** Default shapes for allocation requests (fields, validators, default authority requirements).
7. **Process catalog (forward).** References to CCL process definitions (ADR-0023) the package activates.
8. **Locale and accessibility metadata.** Declared languages; assistive defaults; deadline-justice profile.
9. **Action card templates (forward).** Template references for `/me/action-cards` (ADR-0027).

Schema validation rejects unknown extension shapes by default. Packages may declare `extensions: { ... }` blocks under explicit opt-in for ICN-future-version fields.

## Boundary rules

- ICN owns the manifest schema and the four primitive seed kinds. Packages do not redefine them.
- Packages own the **content** of every section (which entities, which indicators, which signal rules). ICN does not learn institution-specific nouns.
- `operating_purpose` strings remain opaque to ICN; the manifest schema validates that they are present where required, not what they mean.
- Indicator and signal rule **content** is package-side; the **shape** (count vs ratio, threshold vs duration) is ICN-side.

## Consequences

- Packages become validatable: a package either matches the schema or does not. Drift becomes mechanically detectable.
- Cross-package learning becomes possible: shared indicator and signal-rule patterns can be lifted into the schema as new kinds.
- Trade-off: schema growth has a cost. Each new section is a future migration burden if the substrate evolves.
- Trade-off: validation has to be lenient enough that real institutions can express what they need. Over-strict validation produces packages that satisfy ICN but do not satisfy their institutions.

## Implementation status

Partial.

- **Implemented:** Bootstrap seeds, person-directory overlay binding, `operating_purpose` field. See [icn/crates/icn-governance/src/bootstrap.rs](../../icn/crates/icn-governance/src/bootstrap.rs).
- **Not implemented:** Indicator definitions, signal rules, allocation templates, process catalog, locale metadata, action-card templates as schema sections. Reference institution packages currently express these informally outside the validated manifest.

To call this `implemented` would require:
- Schema definition for each new section in a Rust type or JSON-Schema artifact under `icn-governance` or a new `icn-package` crate.
- Validator integration in the bootstrap-apply CLI path.
- Migration story for existing packages that don't declare these sections (the schema must be additive).

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Keep extensions ad-hoc per package | Erodes the boundary contract; every package becomes its own bespoke shape; cross-package patterns are unobservable. |
| Define one giant rigid schema covering every conceivable institution | Different institutions are differently shaped. Over-rigid schemas produce false friction at the boundary; institutions either fork or work around. |
| Use a generic config schema (JSON-Schema, OpenAPI) without ICN-specific structure | The point is not to validate YAML; it is to validate institutional shape. ICN owns the shape vocabulary. |
| Defer until a second institution package is in production | Two packages without a schema cement two different shapes. The schema work is cheaper before the second package than after. |
