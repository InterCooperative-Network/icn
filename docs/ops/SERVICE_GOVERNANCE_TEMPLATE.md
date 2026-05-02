---
Status: operational
Canonical: no
Last Reviewed: 2026-05-01
---

# Service Governance Template

> **Status: operational template.** Copy this template when proposing a new ICN-hosted or ICN-governed service. Filling out this template does not deploy the service or make it canonical.

## Purpose

This template forces every service proposal to answer the same questions:

- What is the service?
- Who operates it?
- Who governs it?
- What data does it hold?
- How does identity work?
- What protocol promises does it make?
- What produces receipts?
- How is it backed up?
- How does the institution leave cleanly?

A service that cannot answer these questions is not ready to become an ICN dependency.

---

## Service definition

```yaml
service: <short-name>
status: planned | rehearsal | active | canonical | deprecated
route: <host-or-route>
service_kind: hosted_service | governed_service | icn_native_service
runtime: <software/runtime>
operator_scope: <who runs it>
governance_scope: <who authorizes policy changes>
identity_provider: <auth route or none>
canonical_status: none | rehearsal | canonical | mirror | projection
```

## Summary

Describe what the service does in one paragraph.

## Why this service exists

Explain the institutional need. Avoid tool-first justification. The service should reduce burden, preserve memory, improve access, create resilience, or make authority/provenance clearer.

## Non-goals

List what this service does not do.

Examples:

- Does not become ICN authority.
- Does not replace DIDs.
- Does not hold private data beyond stated scopes.
- Does not make GitHub obsolete on day one.
- Does not introduce institution-specific nouns into ICN core.

## Data custody

| Data class | Canonical / projection / cache / mirror / external | Notes |
|---|---|---|
|  |  |  |

Data classes:

- **Canonical** — source of truth for a bounded area.
- **Projection** — derived view of ICN or another source.
- **Cache** — disposable performance copy.
- **Mirror** — copy for discovery, redundancy, or compatibility.
- **External** — imported or linked from outside ICN.

## Authority model

Who may use the service?

Who may administer it?

Which authority scopes are required?

Which actions require governance approval?

Which actions are ordinary operator actions?

## Identity and login

```yaml
auth:
  browser: cookie-session | oidc | local-temporary | none
  did_login: planned | implemented | not_applicable
  external_idp_bindings: allowed | disallowed | transition_only
  service_identities: planned | implemented | not_applicable
```

State clearly whether service-local users/groups are canonical or projections.

Preferred doctrine:

```text
DID / ICN standing / authority_scope = authority
IdP / OIDC / service-local account = session projection
```

## Protocol posture

```yaml
protocols:
  commands: HTTP | other
  queries: HTTP | other
  events: SSE | polling | none
  realtime: WebSocket | none
replay:
  event_ids: true | false | not_applicable
  resume_after_disconnect: true | false | not_applicable
fallback:
  polling: true | false | not_applicable
payload_policy:
  event_streams_carry: pointers | payloads | none
receipt_source_of_truth: true | false
```

If `realtime: WebSocket`, justify why HTTP + SSE is insufficient.

## Receipt posture

List actions that should be receipt-bearing.

| Action | Receipt candidate | Implemented now? | Notes |
|---|---|---|---|
|  |  | no |  |

Receipt candidates must eventually align with the ADR-0026 receipt envelope. Do not invent a parallel audit system.

## Compute posture

Can this service trigger compute jobs?

Examples:

- scans
- indexing
- summarization
- release builds
- docs freshness
- accessibility tests
- translation
- model-assisted deliberation packets

Rule:

```text
compute produces evidence; governance / authorized humans decide
```

## Backups

```yaml
backup:
  required: true | false
  frequency: daily | weekly | manual | other
  includes:
    - database
    - files
    - configuration
    - repositories
  offsite_copy: true | false
  encryption: required | not_required | unknown
  restore_test_required: true | false
```

Describe where backups go and who can restore them.

## Restore test

Describe how restore is tested.

Minimum questions:

- Can the service start from backup?
- Is canonical data present?
- Are permissions/session projections restored or intentionally recreated?
- Are secrets handled safely?
- Can clients reconnect or reauthenticate?
- Is mirror direction preserved or intentionally reset?

## Incident plan

Describe what happens when:

- service is down
- operator loses access
- host fails
- database corrupts
- credentials leak
- route/TLS fails
- upgrade fails
- mirror diverges
- unauthorized access is suspected

## Exit / migration path

A service that traps institutional state should not be installed.

Explain:

- How data exports.
- Which formats are used.
- How another service can import it.
- What must be retained for audit.
- Which data can be discarded.
- How users are notified.

## Privacy / access boundaries

List the most sensitive data the service can see.

List the data it must never receive.

List scopes or roles that can access sensitive data.

## Observability

Describe:

- metrics
- logs
- uptime checks
- alerting
- status-page integration
- who can see operational logs
- whether logs include sensitive data

## Deployment target

```yaml
deploy:
  target: vps | k3s | kubernetes | docker-compose | systemd | other
  persistence: pvc | host-path | object-store | database | other
  tls: acme | manual | reverse-proxy | other
  ssh_or_special_ports: []
```

## Cutover gates

For services that replace an external dependency, list cutover gates.

Example gates:

- backup passes
- restore tested
- auth works
- route stable
- mirror tested
- contributor docs updated
- incident rollback documented
- service owners named
- export path confirmed

## Review checklist

- [ ] Service definition is complete.
- [ ] Authority model is clear.
- [ ] Data custody is classified.
- [ ] Backup plan exists.
- [ ] Restore test exists.
- [ ] Exit path exists.
- [ ] Protocol posture is justified.
- [ ] WebSocket use, if any, is justified.
- [ ] Receipt candidates are named.
- [ ] Compute use is evidence-only.
- [ ] Auth bridge does not become authority.
- [ ] No secrets are committed.
- [ ] No institution-specific nouns are introduced into ICN core.
- [ ] Accessibility / mobile implications are considered.

## One-line rule

**A service proposal is not ready until the institution can explain how to operate it, govern it, recover it, audit it, and leave it.**
