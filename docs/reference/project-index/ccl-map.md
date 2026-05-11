---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# CCL Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md), accepted ADRs, and [`source-of-truth-map.md`](source-of-truth-map.md). This map routes CCL's current and future roles; it is not a language specification.

## Purpose

CCL is the Cooperative Contract Language. It is the rule/expression layer that lets institutional meaning be compiled into bounded, deterministic forms that ICN can enforce or execute safely.

This map prevents three mistakes:

1. treating CCL as a general-purpose programming language;
2. treating future process-language ambitions as current implementation;
3. blurring CCL's role with the kernel's role.

## Boundary rule

```text
CCL expresses bounded rules.
Apps and hosts interpret institutional meaning.
Policy oracles compile meaning into constraints.
The kernel enforces generic constraints.
```

The kernel should not become a domain-meaning interpreter. CCL helps institutions express rules, but the meaning firewall still matters.

## Source paths

| Area | Path |
|---|---|
| CCL crate | `icn/crates/icn-ccl/` |
| Charter runtime | `icn/apps/charter/` |
| Governance integration | `icn/apps/governance/`, `icn/crates/icn-governance/` |
| Contract templates | `contracts/templates/` |
| ADR: CCL determinism/fuel/capability safety | `docs/adr/ADR-0021-ccl-determinism-fuel-and-capability-safety.md` |
| ADR: schema bridge / ConstraintSet compilation | `docs/adr/ADR-0022-ccl-schema-bridge-and-constraintset-compilation.md` |
| Architecture | `docs/ARCHITECTURE.md`, `docs/architecture/KERNEL_APP_SEPARATION.md` |

## Status bands

| Role | Status | Public/demo visibility | Drift risk |
|---|---|---|---|
| Deterministic contract/rule language | implemented | architecture/developer docs | medium |
| Fuel/capability bounded execution | implemented | developer docs | medium |
| Charter rule expression | implemented | public architecture narrative | low-medium |
| Schema bridge / constraint compilation | implemented but should be verified per path | developer docs | medium |
| Compute task packaging | implemented but not public-demo central | mostly internal/developer | medium |
| Institutional process language | design-direction / future | not public capability claim | high |
| Signals/escalation/indicator rules | design-direction / future | not public capability claim | high |
| Obligation/allocation lifecycle rules | design-direction / RFC-gated depending path | not complete | high |

## Current safe claims

Safe, current claims:

- ICN has a dedicated CCL crate.
- CCL is deterministic and bounded by design.
- CCL participates in charter/rule expression and constraint compilation.
- CCL is not the kernel; it is part of the expression/translation layer above mechanical enforcement.
- Future institutional process-language roles should be described as future or design-direction unless runtime evidence exists.

## Claims to avoid

Avoid claiming:

- CCL is a general-purpose application language;
- every institutional process can already be expressed in CCL;
- CCL directly performs unrestricted side effects;
- CCL replaces governance, legal judgment, facilitation, or institutional care;
- signal rules, obligation lifecycle rules, or full process sessions are complete unless the relevant RFC/runtime evidence exists.

## Relationship to charters

Charters are the clearest current CCL-adjacent path:

```text
institutional rule text / template
  -> CCL or schema bridge
  -> ConstraintSet / policy shape
  -> kernel enforcement
```

This is one of the strongest examples of the meaning firewall working: institutional meaning stays in app/policy layers; the kernel receives enforceable constraints.

## Relationship to compute

CCL can participate in bounded compute packaging and rule execution, but compute is not governance authority.

Correct frame:

```text
compute executes bounded work
CCL constrains/expresses rules
receipts record outcomes
governance authorizes power
```

## Relationship to future process primitives

Future or design-direction uses include:

- signal rules;
- escalation policies;
- indicators;
- temporary authority grants;
- obligation/allocation lifecycle rules;
- support/care process rules;
- institutional process sessions.

These should not be described as complete runtime capabilities unless backed by current code, tests, and state docs.

## Follow-ups

- Add a deeper CCL implementation map if CCL changes become a near-term workstream.
- Link each CCL-related future primitive to its RFC/issue before making public claims.
- Keep CCL claims tied to accepted ADRs and current code, not long-arc strategy docs.
