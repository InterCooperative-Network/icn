---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-01
---

# Sovereign Forge Strategy

> **Status: strategy / design direction.** This document defines why ICN should operate its own forge and how the transition should be staged. It does not deploy a forge, change the canonical repository location, or deprecate GitHub today.

## Summary

ICN should operate a sovereign forge because a project building cooperative infrastructure should not depend on a corporate platform as the only canonical home of its own workshop.

The strategic direction:

```text
ICN-controlled forge = canonical institutional workshop
GitHub = public mirror, discovery surface, and contributor funnel
```

The immediate target is not purity. The immediate target is infrastructure sovereignty without breaking the workflows that currently work.

## Why this matters

Git hosting is not just storage. A forge holds:

- source code
- issues
- pull requests
- releases
- contributor records
- access control
- review history
- project memory
- CI hooks
- artifacts
- governance-adjacent decision traces

For ICN, that is institutional nervous tissue. Leaving it exclusively on a proprietary corporate platform contradicts the project's own premise: communities need infrastructure they can actually govern.

The claim is not "GitHub is useless." GitHub remains useful for visibility, social proof, contributor discovery, issue references, and public mirroring.

The claim is:

> GitHub should not be the homeland.

## The forge as first sovereignty service

The forge should be treated as ICN's first serious sovereignty service.

A sovereignty service is a hosted service that forces ICN to answer real institutional questions:

- Who controls the domain?
- Who controls the server?
- Who controls deploy keys?
- Who can restore from backup?
- Who can revoke access?
- Who pays for storage?
- Who signs releases?
- Who is allowed to merge?
- What happens if a maintainer disappears?
- What happens if the host fails?
- What is canonical: GitHub, Forgejo, ICN state, or receipts?

This is why the forge matters. It forces ICN to host itself.

## Recommended first runtime

Use Forgejo as the initial forge candidate unless a separate evaluation rejects it.

Reasons:

- open-source forge aligned with self-hosting
- lighter than GitLab
- supports Git repos, issues, pull requests, releases, teams, SSH, HTTP Git, webhooks
- can integrate with OIDC
- operationally realistic for a small infrastructure team

GitLab may be evaluated later if ICN needs the larger integrated platform. Do not start with the heaviest possible system unless the need is proven.

## Candidate route

Preferred route:

```text
forge.intercooperative.network
```

This reads as public/cooperative infrastructure, not startup branding.

Alternative routes:

```text
git.intercooperative.network
code.intercooperative.network
forge.icn.zone
```

The route is a DNS binding, not authority. The service's authority comes from ICN governance and service custody, not from the host name.

## Staged transition

### Stage 0 — GitHub canonical, forge planned

Current posture.

- GitHub remains canonical.
- Service-hosting docs land.
- Forge deployment plan lands.
- No workflow disruption.

### Stage 1 — Forge deployed as mirror / rehearsal

- Deploy Forgejo.
- Create ICN organization / project namespace.
- Mirror `icn`, `nycn`, and `icn-learn` as appropriate.
- Configure auth bridge or temporary local admin accounts.
- Configure backups.
- Verify restore.
- Keep GitHub canonical.

### Stage 2 — Dual-home development

- Developers can clone from the forge.
- GitHub still accepts contribution traffic.
- Mirror policies are explicit.
- CI relationship is documented.
- Issues/PRs may remain GitHub-first until migration gates are met.

### Stage 3 — Forge canonical, GitHub public mirror

- Canonical remotes point to the ICN forge.
- GitHub mirrors outward for visibility and contribution discovery.
- Release artifacts are signed and published from the ICN-governed flow.
- Important forge actions begin to map into receipts.

### Stage 4 — ICN-native forge integration

- Forge has service identity.
- Forge events pass through an adapter.
- Merge/release/access actions are receipt-bearing where appropriate.
- Compute jobs produce evidence for CI, security, docs freshness, and release artifacts.
- GitHub is clearly labeled as a mirror / public intake surface.

## MVP requirements

The first forge MVP should include:

```text
1. Forgejo service
2. persistent storage
3. SSH Git access
4. HTTPS access
5. route / TLS
6. admin bootstrap
7. OIDC path identified or temporary auth documented
8. backups
9. restore test
10. GitHub mirror policy
11. repo import procedure
12. observability hooks
13. incident/rollback plan
```

No ICN-native receipt integration is required for the first MVP. The first win is governed custody.

## Canonical data posture

The forge service definition should declare what it holds:

```yaml
canonical_data:
  - git repositories
  - issue tracker state once cutover happens
  - pull request state once cutover happens
  - release metadata once cutover happens
mirror_data:
  - GitHub mirrors
projection_data:
  - OIDC-local account/session records
cache_data:
  - rendered repo pages
  - search indexes
```

Before cutover, GitHub remains canonical. After cutover, GitHub is a mirror.

## Access model

The forge should eventually consume ICN authority through the auth bridge:

```text
DID / ICN identity
  ↓
standing / role / authority_scope
  ↓
auth bridge OIDC claims
  ↓
Forgejo local session / team mapping
  ↓
service action
  ↓
receipt if institutional transition matters
```

Initial deployment may use local admin accounts or manually configured OIDC groups, but that must be marked as transitional.

## Events and receipts

Forge events should flow through an adapter:

```text
Forgejo webhook
  ↓
forge adapter validates event
  ↓
ICN checks authority if needed
  ↓
compute job may run evidence checks
  ↓
receipt emitted for institutional transition
```

Candidate receipt-bearing transitions:

| Event | Receipt candidate |
|---|---|
| repo mirror updated | `RepoMirrorReceipt` |
| release published | `ReleaseReceipt` |
| access granted | `AccessGrantReceipt` |
| access revoked | `AccessRevocationReceipt` |
| backup completed | `BackupReceipt` |
| restore test completed | `RestoreTestReceipt` |
| PR merged under governed authority | `MergeReceipt` |

These receipt names are candidates. They must align with ADR-0026 and future receipt-envelope work rather than create a parallel audit system.

## Compute integration

The forge is an excellent first service for compute evidence.

Compute may run:

- Rust CI
- TypeScript SDK checks
- website build checks
- docs freshness checks
- accessibility tests
- compliance linter
- license scans
- dependency scans
- SBOM generation
- release artifact builds
- repo atlas generation
- PR summary / source-linked review packets

Compute does not decide. It produces evidence. Maintainers or governance decide.

Correct flow:

```text
PR opened
  ↓
compute runs checks
  ↓
ComputeReceipt / evidence artifact
  ↓
maintainer reviews
  ↓
merge action
  ↓
MergeReceipt if governed merge receipts are implemented
```

## Protocol posture

The forge should follow the protocol-selection doctrine:

- HTTP for commands and canonical reads.
- SSE for build progress, mirror progress, receipt-created pointers, and service-awareness streams.
- WebSockets only for genuinely interactive live development sessions, if any.
- Polling fallback for critical views.

Do not make forge usability depend on live sockets when ordinary HTTP + SSE is enough.

## GitHub mirror posture

GitHub should remain useful:

- public discovery
- inbound issues / discussions during transition
- contributor familiar path
- package ecosystem references
- social proof
- emergency mirror

But GitHub should be labeled and treated as a mirror once cutover happens.

Mirroring direction after cutover:

```text
forge.intercooperative.network/ICN/icn
        ↓
github.com/InterCooperative-Network/icn
```

Before cutover, direction may remain GitHub → Forgejo while the forge is being rehearsed.

## Risks

| Risk | Mitigation |
|---|---|
| Overbuilding before use | Start Forgejo MVP, not ICN-native forge from day one. |
| Losing GitHub contributor flow | Keep GitHub as mirror and public intake. |
| Admin capture | Document service governance, access revocation, and backup custody. |
| Fragile self-hosting | Require backups and restore tests before canonical cutover. |
| Confusing canonical state | Explicit cutover checklist; clear mirror labels. |
| Auth bridge becomes authority | Keep DID/standing/authority in ICN; IdP is session bridge only. |
| CI disruption | Stage CI migration separately; do not block cutover on perfect CI federation. |

## Cutover gates

Forge should not become canonical until:

- backups run successfully
- restore has been tested
- admin access has at least two authorized custodians or an explicit break-glass policy
- SSH and HTTPS clone/push work
- TLS and route are stable
- GitHub mirror direction is tested
- contributor instructions are updated
- release process is documented
- incident rollback path exists
- service definition exists
- auth posture exists
- export path exists

## Non-goals

- No immediate GitHub abandonment.
- No runtime implementation in this document.
- No claim that Forgejo is already selected by formal ADR.
- No ICN-native merge/release receipt implementation in this document.
- No replacement of RFC-0017 tool-install work.
- No requirement that all contributors use the sovereign forge immediately.

## One-line rule

**GitHub can remain the billboard; ICN should own the workshop.**
