---
Status: operational
Canonical: no
Last Reviewed: 2026-05-01
---

# Forgejo Deployment Plan

> **Status: operational planning.** This document describes the proposed first deployment path for an ICN sovereign forge. It does not deploy Forgejo, change DNS, mutate infrastructure, or make the ICN forge canonical.

## Summary

The first sovereign forge deployment should be a conservative Forgejo MVP:

```text
forge.intercooperative.network
  → Forgejo
  → persistent storage
  → SSH + HTTPS Git
  → backups + restore test
  → GitHub mirroring
  → OIDC path identified
  → monitoring
```

The first goal is governed custody, not full ICN-native integration.

## Inputs from architecture

This plan implements the first operational slice of:

- [`SERVICE_HOSTING_MODEL.md`](../architecture/SERVICE_HOSTING_MODEL.md)
- [`AUTH_BRIDGE_AND_DID_LOGIN.md`](../architecture/AUTH_BRIDGE_AND_DID_LOGIN.md)
- [`PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`](../architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md)
- [`../strategy/SOVEREIGN_FORGE.md`](../strategy/SOVEREIGN_FORGE.md)

## Deployment posture

Recommended initial posture:

```yaml
service: forge
route: forge.intercooperative.network
runtime: Forgejo
service_kind: hosted_service
canonical_status: rehearsal
github_role: canonical_for_now
cutover_status: not_started
```

The `service_kind` value here uses the same vocabulary the rest of this PR uses for the hosted → governed → ICN-native classification (`hosted_service` | `governed_service` | `icn_native_service`). A forge in rehearsal stage is intentionally `hosted_service`, not yet `governed_service`.

Do not start by claiming the forge is canonical. Start by making it real, backed up, restorable, observable, and documented.

## Deployment target options

| Target | Use | Caution |
|---|---|---|
| VPS / dedicated host | Best first public deployment if available. Stable networking and simple blast radius. | Needs backup and host custody policy. |
| Existing K3s / homelab | Useful if consistent with current ICN deploy conventions. | Residential/network fragility must be acknowledged. |
| Kubernetes / Helm | Good if ICN wants service parity with existing deploy patterns. | More moving parts; avoid overcomplication. |
| Docker Compose | Good for rehearsal or single-node MVP. | Less aligned with future service hosting if production moves to K3s/Helm. |

A boring VPS or a clearly managed K3s service is preferable to a fragile experimental deployment. The forge will become part of project continuity.

## MVP components

Required:

- Forgejo web service
- persistent repository storage
- database storage, if not using embedded mode
- SSH Git endpoint
- HTTPS Git endpoint
- TLS certificate
- route / DNS record
- admin bootstrap procedure
- backup job
- restore test procedure
- service monitoring
- GitHub mirror procedure
- incident rollback notes

Optional first wave:

- OIDC login against `auth.intercooperative.network`
- Prometheus metrics if available
- object storage for attachments or artifacts
- email notifications
- runner integration

Defer:

- ICN-native service identity
- merge receipts
- release receipts
- access receipts
- compute-settled CI
- full issue migration
- GitHub canonical cutover

## Service data classes

Declare data custody explicitly:

| Data | Initial class | Notes |
|---|---|---|
| Git repos | Mirror / rehearsal at first; canonical only after cutover | Must be backed up before cutover. |
| Issues | GitHub canonical at first | Migrate only with explicit plan. |
| Pull requests | GitHub canonical at first | Do not disrupt active contribution flow. |
| Releases | GitHub canonical at first | Forge releases may be rehearsal until cutover. |
| Users / sessions | Projection | Auth bridge should eventually project ICN identity. |
| Attachments | Service state | Requires backup if enabled. |
| Webhooks | Operational config | Must be documented if they trigger actions. |

## Initial service definition

```yaml
service: forge
status: planned
route: forge.intercooperative.network
service_kind: hosted_service
runtime: Forgejo
operator_scope: ICN Infrastructure
governance_scope: ICN Core Maintainers
identity_provider: auth.intercooperative.network # planned; may be manual at MVP
canonical_data:
  - repositories after cutover
external_mirrors:
  - github.com/InterCooperative-Network/icn
  - github.com/InterCooperative-Network/nycn
  - github.com/InterCooperative-Network/icn-learn
receipts_initial: []
receipts_future:
  - RepoMirrorReceipt
  - ReleaseReceipt
  - AccessGrantReceipt
  - AccessRevocationReceipt
  - MergeReceipt
protocols:
  commands: HTTP
  events: SSE where available
  realtime: none for MVP
backup_required: true
restore_test_required: true
exit_path_required: true
```

## Bootstrap checklist

### Preflight

- [ ] Confirm route choice.
- [ ] Confirm deployment target.
- [ ] Confirm storage location.
- [ ] Confirm backup destination.
- [ ] Confirm admin bootstrap identity.
- [ ] Confirm whether OIDC is available for first pass.
- [ ] Confirm firewall/ports: HTTPS and SSH Git.
- [ ] Confirm monitoring target.
- [ ] Confirm GitHub mirror direction for rehearsal.

### Deploy

- [ ] Deploy Forgejo.
- [ ] Provision persistent storage.
- [ ] Configure route and TLS.
- [ ] Configure SSH Git.
- [ ] Create initial admin.
- [ ] Create organization / namespace.
- [ ] Import or mirror `icn`.
- [ ] Import or mirror `nycn` if appropriate.
- [ ] Import or mirror `icn-learn` if appropriate.
- [ ] Configure outbound mirror or inbound mirror.
- [ ] Configure backup job.
- [ ] Configure monitoring.

### Verify

- [ ] Web UI loads over HTTPS.
- [ ] SSH clone works.
- [ ] HTTPS clone works.
- [ ] Push works for authorized admin.
- [ ] Mirror sync works.
- [ ] Backup artifact exists.
- [ ] Restore test succeeds in isolated environment.
- [ ] Logs are accessible to authorized operator.
- [ ] Service status is visible.

## Auth posture

MVP options:

1. **Temporary local admin** — acceptable only for first bootstrap; must be documented and minimized.
2. **OIDC through auth bridge** — preferred if `auth.intercooperative.network` is ready.
3. **External GitHub login** — acceptable only as convenience and bootstrap, not authority.

Target posture:

```text
DID / ICN identity
  ↓
ICN standing / role / authority scope
  ↓
auth bridge OIDC claims
  ↓
Forgejo local session / team projection
```

## Mirroring posture

Before cutover:

```text
GitHub canonical → Forgejo rehearsal mirror
```

After cutover:

```text
Forgejo canonical → GitHub public mirror
```

Cutover must not happen until backups, restore, auth, contributor instructions, mirror direction, and rollback are documented.

## Backup requirements

Minimum:

- daily database backup
- repository data backup
- attachment/LFS backup if enabled
- configuration backup excluding secrets or with encrypted secret handling
- off-host copy
- restore test before canonical cutover

A forge without restore is not sovereignty. It is a login page with a hostage situation waiting inside it.

## Restore test

A restore test must verify:

- Forgejo starts from backup.
- Repositories are present.
- refs/tags/branches are intact.
- users/orgs/teams are present or documented as intentionally excluded.
- issues/PRs/releases are present if those are in scope.
- SSH/HTTPS clone works after restore.
- mirror configuration is either restored or intentionally re-created.

## Incident and rollback posture

Before canonical cutover, rollback is simple: GitHub remains canonical.

After canonical cutover, rollback requires:

- recent backup
- GitHub mirror freshness check
- DNS rollback or status notice
- contributor communication
- freeze window for pushes if needed
- receipt/audit note for the incident, once receipt support exists

## Integration path

### Phase 1 — hosted service

- Forgejo runs.
- GitHub remains canonical.
- Backups and restore test exist.

### Phase 2 — governed service

- Service definition is ratified or accepted by maintainers.
- Access policy exists.
- Backup policy exists.
- Upgrade policy exists.
- Incident policy exists.

### Phase 3 — adapted service

- Forgejo webhooks feed a service adapter.
- Adapter classifies events as noise, evidence, or institutional transition.
- Compute jobs can be triggered for CI/evidence.

### Phase 4 — ICN-native service

- Forge has a `ServiceIdentity`.
- Meaningful transitions emit receipts.
- Merge/release/access records bind to ICN authority.
- GitHub is public mirror only.

## Non-goals

- No deployment in this document.
- No DNS mutation.
- No secrets committed.
- No GitHub deprecation.
- No canonical cutover.
- No CI runner migration.
- No claim that Forgejo is formally selected by ADR.

## One-line rule

**Deploy the forge boring first; make it governed before making it canonical; make it native only after the service proves it can be trusted.**
