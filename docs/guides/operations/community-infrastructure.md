---
Status: operational
Canonical: no
Last Reviewed: 2026-05-18
---

# ICN Community Infrastructure

This document describes the public-safe shape of ICN community infrastructure:
what people should use, what is intentionally invite-only, and what remains
internal or experimental.

Detailed host inventory, private network addresses, firewall rules, tunnel
tokens, backup paths, and operational secrets live outside this public repo.

## Current Posture

Matrix is the serious ICN community coordination service.

Forgejo is a lab/playground for now. It is useful for learning service hosting,
mirroring, backup/restore, auth integration, and ops discipline, but it is not
the canonical project forge yet.

The current public/community posture is:

| Surface | Status | Purpose |
| --- | --- | --- |
| `chat.icn.zone` | live, invite-only | Preferred Element Web entry point |
| `chat.intercooperative.network` | live alias | Long-form chat URL |
| `matrix.intercooperative.network` | live client API | Matrix homeserver client endpoint |
| `intercooperative.network` | canonical identity domain | Institutional/project identity domain |
| `join.icn.zone` | planned | Future access request / onboarding surface |
| `forge.intercooperative.network` | planned/rehearsal | Future Forgejo lab/rehearsal surface |

## Matrix

Matrix is for realtime coordination:

- contributor discussion
- developer coordination
- co-op organizer coordination
- help and onboarding
- announcements
- small invite-only test cohorts

Matrix is not governance authority. Important decisions should be promoted to
GitHub issues, docs, ADRs, RFCs, signed receipts, or other durable project
records.

Current Matrix policy:

- registration is closed
- access is invite-only
- federation is not enabled
- TURN/calls are not enabled
- Discord bridge is not deployed
- bots are not part of the first public cohort
- Matrix accounts are local accounts for now

The preferred durable Matrix identity domain is:

```text
@user:intercooperative.network
```

## Public Launch Boundaries

The public chat surface is early infrastructure, not a broad public launch.

Before larger community launch, ICN should have:

- restore drill confidence
- monitoring and alert routing
- published onboarding/moderation expectations
- a clear invite approval process
- room doctrine that says chat is coordination, not governance
- a small first cohort before broader invitations

## Private Developer Access

Private developer access is being designed around least privilege.

The intended direction:

- private transport for approved contributors and operators
- group-scoped access rather than broad network access
- separate roles for admins, maintainers, contributors, organizers, and guests
- no default access to personal homelab, storage, hypervisor, or unrelated admin
  surfaces

Connection details, peer configuration, firewall aliases, and private network
topology are intentionally not documented in this repo.

## Onboarding Workflow

The first onboarding flow should stay manual:

1. A person requests access.
2. An ICN operator reviews context and fit.
3. If approved, an account is created or invited.
4. The person receives welcome instructions.
5. Access can be revoked if needed.

Automation may help with intake, reminders, review queues, and welcome messages.
Automation should not decide membership, standing, governance authority, or
privileged access in the first phase.

## Forgejo

Forgejo is not the community front door yet.

Near-term role:

- service-hosting sandbox
- Git mirror rehearsal
- backup and restore practice
- OIDC/auth bridge rehearsal
- future sovereign forge preparation

Current canonical project record remains GitHub until a deliberate cutover.

Future progression:

1. GitHub canonical, Forgejo mirror.
2. Forgejo rehearsal with backups, restore tests, OIDC, and monitoring.
3. Explicit cutover decision, if and when Forgejo becomes canonical.
4. GitHub mirror after cutover.

## Future ICN-Native Integration

The community infrastructure should eventually become a projection of ICN-native
authority, not an independent authority system.

Target direction:

```text
ICN DID / standing / authority scope
-> auth bridge claims
-> service access in Matrix / Forgejo / VPN / workflow tools
-> receipts for meaningful changes
```

Candidate future receipt types:

- `AccessGrantReceipt`
- `AccessRevocationReceipt`
- `ServiceIdentityCredentialRotatedReceipt`
- `RouteBindingReceipt`
- `BackupReceipt`
- `RestoreTestReceipt`

Until this exists, service-local admin panels and accounts are operational
state. They must not be treated as institutional truth.

## Non-Goals

This document does not define:

- private hostnames or IP addresses
- firewall rules
- VPN peer configuration
- backup paths
- secrets
- tunnel credentials
- production authority rules
- public federation policy
- meeting/call infrastructure

Those belong in private operations records until they are safe and useful to
publish.
