---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-01
---

# Service Hosting Model

> **Status: design direction.** This document names how ICN should treat hosted services such as a forge, identity-provider bridge, status page, registry, documentation surface, metrics surface, and future tool services. It is not an implementation report and it does not declare any service live.

## Summary

ICN services should not merely be deployed. They should be **governed**.

A service is legitimate when its authority, custody, access, failure modes, backups, upgrades, event streams, compute jobs, and receipts are legible. Running a service is an operational fact; governing a service is an institutional fact.

This document adds a service-hosting layer to the existing architecture:

```text
public / learning surfaces
        ↓
institution packages
        ↓
sovereign service hosting layer  ← this document
        ↓
tool / service adapter layer
        ↓
ICN runtime apps
        ↓
ICN substrate
        ↓
ops / infrastructure
```

The immediate forcing case is a sovereign forge, but the model is generic. The same service skeleton should eventually apply to the forge, auth bridge, status page, registry, docs, receipts, metrics, and institution-facing hosted tools.

## Relationship to existing architecture

This document extends, but does not replace:

- [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) — institutional domains, service identities, devices, workspaces, vaults, agreements, routing, and commons resource sharing.
- [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) — installable / forkable tools that operate on institution-owned state and emit receipts.
- [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md) — DNS routes are bindings to authority; DNS is not authority itself.
- [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md) — compute assists institutions; it does not govern them.
- [RFC-0017](../rfcs/RFC-0017-tool-install-infrastructure.md) — future `ToolManifest`, `ToolBinding`, service identity audit trail, and tool install lifecycle.

This document focuses on the operational bridge between those designs: how an actual service should be described before it becomes ICN-native.

## Core doctrine

```text
ICN owns authority.
Services expose functions.
Adapters translate service events.
Receipts prove transitions.
Compute produces evidence.
Governance authorizes power.
```

A service may host workflows, files, repositories, dashboards, or login screens. It does not become the source of institutional truth unless ICN explicitly records and proves the transition that made something true.

## Hosted → governed → native

Services should move through three stages.

### 1. Hosted service

The service runs on infrastructure operated by ICN or an ICN-aligned operator.

Examples:

- Forgejo running at `forge.intercooperative.network`.
- An identity-provider bridge running at `auth.intercooperative.network`.
- Grafana behind authenticated access.

At this stage, the service is operationally useful but may not yet emit ICN receipts or consume ICN-native service identities.

### 2. Governed service

Changes to the service are bound to explicit authority.

Examples:

- Admin access changes require the right operator scope.
- Backup policy is documented and tested.
- Upgrade policy names who may apply updates.
- Route changes produce binding records.
- Access grants and revocations are auditable.

At this stage, the service is no longer just a sysadmin artifact. It has institutional custody.

### 3. ICN-native service

The service participates directly in ICN primitives.

Examples:

- The service has a `ServiceIdentity`.
- Its actions are capability-scoped.
- Meaningful transitions emit receipts.
- Event streams carry receipt pointers.
- Compute jobs are represented as constrained workloads.
- Installation and suspension eventually map to `ToolManifest` / `ToolBinding` lifecycle records.

The first deployment of a service should not pretend to be stage 3. The path should be explicit.

## Service definition

Every ICN-governed service should have a service definition before it is treated as a durable dependency.

```yaml
service: forge
status: planned
route: forge.intercooperative.network
service_kind: hosted_service
runtime: Forgejo
operator_scope: ICN Infrastructure
governance_scope: ICN Core Maintainers
identity_provider: auth.intercooperative.network
canonical_data:
  - repositories                                  # rehearsal-stage canonical (forge holds working-tree mirrors)
  - issues (after cutover from GitHub)            # GitHub remains canonical until explicit cutover
  - pull requests (after cutover from GitHub)     # GitHub remains canonical until explicit cutover
  - releases (after cutover from GitHub)          # GitHub remains canonical until explicit cutover
external_mirrors:
  - github.com/InterCooperative-Network/icn       # pre-cutover canonical for issues, pull requests, releases
receipts_initial:
  - BackupReceipt
  - RepoMirrorReceipt
  - ReleaseReceipt
receipts_future:
  - AccessGrantReceipt
  - AccessRevocationReceipt
  - MergeReceipt
  - ToolBindingReceipt
protocols:
  commands: HTTP
  events: SSE
  realtime: WebSocket only when justified
backup_required: true
restore_test_required: true
exit_path_required: true
```

The service definition is not the runtime implementation. It is the institutional declaration of what the service is, who operates it, what data it touches, what authority it requires, and how the institution leaves cleanly if the service fails or is replaced.

## Required questions for any service

A service definition must answer:

- Who operates the service?
- Which governance scope authorizes it?
- Which route points to it?
- What data does it hold?
- Which data is canonical and which is cached or mirrored?
- Which identities may administer it?
- Which identities may use it?
- Which actions require receipts?
- Which actions merely produce operational logs?
- How is the service backed up?
- Where are backups stored?
- How is restore tested?
- What happens if the operator disappears?
- What happens if the host fails?
- How does the institution export and leave?
- Which protocol surfaces does it expose?
- Which critical features degrade to polling or ordinary HTTP?

## Authority model

A hosted service must not become a private root of power. ICN authority remains expressed through DIDs, standing, roles, authority scopes, governance decisions, and receipts.

A service may maintain local accounts as an operational projection. Those accounts are not sovereign identity.

Correct pattern:

```text
member DID / service DID
        ↓
ICN standing + authority scope
        ↓
auth bridge or service adapter
        ↓
service-local account/session
        ↓
action
        ↓
receipt if the action changes institutional state
```

Incorrect pattern:

```text
service admin panel
        ↓
service-local role
        ↓
institutional authority
```

The second pattern rebuilds a platform tenant model and must not become ICN doctrine.

## Event model

Service events are not truth by themselves. They are notifications or evidence.

A service event should usually become one of three things:

1. **Ignored operational noise** — routine logs, health checks, view events.
2. **Evidence** — a CI result, scan result, backup report, or audit summary.
3. **Institutional transition** — an access grant, release, mirror update, publication, or other state change that must emit a receipt.

For member-facing and operator-facing streams, events should generally carry pointers rather than private payloads:

```json
{
  "event_id": "18492",
  "type": "receipt.created",
  "domain_id": "icn-core",
  "receipt_id": "rec_abc",
  "source": "forge.release"
}
```

The client then fetches the receipt or state object through ordinary authorization.

## Protocol rule

Protocol selection is part of service design.

Default posture:

```text
HTTP is for institutional facts.
SSE is for institutional awareness.
WebSockets are for shared live activity.
Receipts are the source of truth.
Polling is the fallback.
```

See [`PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`](PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md).

## Compute rule

Services may ask compute to produce evidence:

- run tests
- run accessibility checks
- scan dependencies
- generate release artifacts
- summarize logs
- produce repo maps
- generate source-linked review packets

Compute does not decide. A compute result may inform a maintainer or governance body; it does not merge, release, grant access, revoke access, publish, or settle anything by itself.

## Data custody

A service must distinguish:

| Class | Meaning |
|---|---|
| Canonical state | The service currently holds the authoritative working copy for the institution. Requires backup, export, restore, and future receipt path. |
| Projection | Service-local view of ICN state. Can be regenerated from ICN. |
| Cache | Performance copy. Disposable. |
| Mirror | Copy pushed outward for discovery or redundancy. Not canonical. |
| External source | Data imported from a non-ICN surface. Requires bridge markers and review boundaries. |

For the forge, GitHub should become a mirror and contributor funnel, not the canonical home of ICN development.

## Initial services to model

The first services worth modeling are:

| Service | Role | Why first |
|---|---|---|
| `forge` | Sovereign code forge | ICN should not depend on a corporate forge as its only workshop. |
| `auth` | Web identity-provider bridge | Normal web services need OIDC/OAuth2-style sessions; ICN DIDs remain the authority. |
| `status` | Public status surface | Makes outages legible without requiring service access. |
| `metrics` | Internal observability | Existing monitoring should be behind governed access. |
| `registry` | Future artifact / release / receipt registry | Connects releases, proofs, and artifacts. |
| `docs` | Future documentation/publishing service | Public/civic documents with publication receipts. |

## Non-goals

- No runtime implementation in this document.
- No declaration that any service is currently live.
- No requirement to abandon GitHub immediately.
- No treatment of Keycloak, authentik, ZITADEL, Forgejo, Grafana, or any other tool as ICN authority.
- No claim that service hosting is already ICN-native.
- No institution-specific nouns introduced as ICN core primitives.

## Build order

Recommended order:

1. Land service-hosting doctrine and templates.
2. Define the sovereign forge plan.
3. Define the auth bridge plan.
4. Deploy non-production services.
5. Add backups and restore tests.
6. Add OIDC login for the forge.
7. Add service event adapters.
8. Add receipt classes or map to existing receipt envelope where appropriate.
9. Add compute jobs as evidence producers.
10. Later, map mature services into `ToolManifest` / `ToolBinding` lifecycle once RFC-0017 lands.

## One-line rule

**ICN services should be operated like infrastructure, governed like institutions, and verified like receipts.**
