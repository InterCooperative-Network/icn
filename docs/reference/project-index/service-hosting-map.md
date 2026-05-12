---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Service Hosting Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This map routes the service-hosting design layer; it is not an implementation claim.

## Purpose

Service hosting is the layer where externally useful services become institutionally legible. A service being reachable is not enough. ICN's design direction is that services should be governed, scoped, auditable, exportable, and accountable to the institution using them.

This map prevents three mistakes:

1. treating a hosted service as automatically ICN-native;
2. treating service-hosting design docs as runtime implementation proof;
3. treating service operators as the source of institutional authority.

## Lifecycle model

| Stage | Meaning | Current posture |
|---|---|---|
| Hosted service | A service runs somewhere and may be useful. | Operational concept; not automatically ICN-native. |
| Governed service | Access, custody, upgrades, backups, restore, routes, failures, and operator powers are institutionally legible. | Design direction unless tied to specific runtime evidence. |
| ICN-native service | The service has service identity, scoped capabilities, receipt-emitting transitions, event streams, and binding/install lifecycle. | Future/design-direction unless specific evidence exists. |

## Source paths

| Area | Path |
|---|---|
| Service hosting doctrine | `docs/architecture/SERVICE_HOSTING_MODEL.md` |
| Domain infrastructure | `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` |
| Tool install infrastructure | `docs/rfcs/RFC-0017-tool-install-infrastructure.md` |
| Ops/deploy surfaces | `deploy/`, `ops/`, `monitoring/` |
| Identity/capability dependencies | `icn/crates/icn-identity/`, `icn/crates/icn-authz/` |
| Runtime/API dependencies | `icn/crates/icn-gateway/`, `icn/apps/` |

## Service questions

A service-hosting plan should answer:

| Question | Why it matters |
|---|---|
| Who operates the service? | Operator power must be visible. |
| Which institution authorizes it? | Services do not create authority by existing. |
| Which scope grants access? | Access must be constrained. |
| What data is canonical? | Prevents SaaS-style institutional memory capture. |
| What data is mirrored? | Clarifies recoverability and export. |
| Which actions require receipts? | Makes service transitions auditable. |
| How are backups created and tested? | Prevents operational hostage-taking. |
| What is the restore path? | Institutions need continuity. |
| What is the exit path? | Institutions must be able to leave. |
| Which event streams exist? | Services should produce evidence, not just state. |

## Candidate service classes

| Service class | Purpose | Status band |
|---|---|---|
| Forge/code hosting | Institution-controlled code collaboration. | design-direction / ops-adjacent |
| Identity/auth bridge | Web access bridge for identity/session handling. | design-direction |
| Status page | Public service health and incidents. | design-direction / ops-adjacent |
| Metrics/monitoring | Operator observability. | implemented but partial in ops context |
| Artifact/registry service | Releases, receipts, packages, or artifacts. | design-direction |
| Docs/publishing service | Institution-controlled documentation/publication. | design-direction |
| Tool services | Services behind Tool Commons tools. | design-direction |

## Safe claims

Safe, current claims:

- ICN has a service-hosting design model.
- The design model distinguishes hosted, governed, and ICN-native services.
- ICN-native service hosting requires identity, capabilities, receipts, event streams, and lifecycle/binding concepts.
- Current service-hosting docs should not be read as proof that those services are live.

## Claims to avoid

Avoid claiming:

- service hosting is implemented as a complete runtime feature;
- ICN already provides a live service marketplace;
- a hosted service is ICN-native merely because it runs near ICN;
- operator custody equals institutional authority;
- service identity is fully implemented across live services;
- institutions have proven exit/export guarantees for every service class.

## Relationship to Tool Commons

Tool Commons depends on the service-hosting layer, but it is not the same thing.

```text
service hosting = how services are governed and operated
tool commons = how reusable tools bind to institution-owned state
```

A future tool may use a service. That service still needs service identity, scoped authority, receipts, event visibility, backup/restore posture, and exit rights.

## Relationship to domains

Service hosting should be scoped through institutional domains. A DNS route or web URL does not create institutional authority. Authority comes from governance, standing, roles, scopes, and agreements.

## Follow-ups

- Add concrete service rows only when source evidence exists.
- Tie future service maps to `ToolManifest`, `ToolBinding`, service identity, and receipt classes.
- Keep public website copy from implying implemented service hosting.
- Add service-hosting checks to a future stale/public-surface drift pass.
