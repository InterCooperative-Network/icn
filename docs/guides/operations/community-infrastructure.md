---
Status: operational
Canonical: no
Last Reviewed: 2026-08-01
---

# ICN community infrastructure

This document describes the public-safe shape of ICN community infrastructure:
what people use, what remains invite-only, and which project records are
durable. Private topology, credentials, backup locations, firewall rules, and
operator procedures live outside this public repository.

## Current posture

ICN has two different kinds of community surface:

| Surface | Role | Current status |
| --- | --- | --- |
| `chat.icn.zone` | Element Web entry point | live for existing invitees |
| `matrix.intercooperative.network` | Matrix client API | live |
| GitHub issues, discussions, pull requests, and releases | durable project work | canonical today |
| `intercooperative.network` | public truth and project identity | canonical public site |
| Forgejo | sovereign-forge rehearsal | internal; not canonical or a public front door |
| Public invitation form | onboarding intake | not live |

The Matrix pilot is being rebuilt before its first broader cohort. Self-service
registration remains closed during that work. Existing invitees can continue to
use the service; other contributors should use GitHub Discussions to introduce
themselves and name the work they want to join without posting private contact
details.

The project-wide [Code of Conduct](../../../CODE_OF_CONDUCT.md) applies to these
spaces. Its private reporting contact is still an unresolved placeholder, so
broader community access must remain closed until maintainers approve and
publish a real reporting channel and name a backup responder.

## What belongs where

Matrix is for realtime coordination:

- contributor questions and working conversation
- developer and organizer coordination
- help and onboarding for invited participants
- announcements and time-sensitive notices

Durable project records belong in the project work system:

- bugs and scoped tasks in issues
- proposed changes in pull requests
- architectural decisions in ADRs and RFCs
- releases and public artifacts in the canonical forge
- institutional authority in ICN state and receipts when those paths exist

Chat is not governance. A room role, reaction, poll, or administrator action
does not by itself create a binding project or institutional decision.

## Initial access model

The first cohort stays human-reviewed:

1. A prospective participant introduces themselves through an existing public
   project surface or an existing maintainer relationship.
2. A named operator reviews fit, room placement, and any moderation concerns.
3. The operator creates the Matrix account and sends welcome instructions out
   of band.
4. The participant joins the rooms relevant to their actual work.
5. Access can be revoked through the documented moderation/operator process.

Automation may help with reminders and welcome messages. It must not decide
membership, standing, governance authority, or privileged access.

## Launch gates

Before a broader invitation:

- encrypted or access-controlled off-host backup coverage exists
- an isolated restore has succeeded
- public and internal monitoring routes alerts to an operator
- moderation owners and escalation paths are published
- room purposes and the chat-versus-governance boundary are visible
- privacy and retention expectations are published
- a small contributor/organizer cohort has completed a review cycle

## Forge progression

Forgejo is not the community front door yet. Its progression is intentionally
separate:

1. GitHub remains canonical; Forgejo is an internal mirror/rehearsal.
2. Forgejo gains backups, restore tests, monitoring, and a documented mirror.
3. Operators test identity and contributor workflows without moving authority.
4. Any canonical cutover requires an explicit project decision and rollback
   plan; only then does GitHub become the outward mirror.

## Future direction

Hosted services should eventually project ICN-native authority rather than
inventing their own:

```text
ICN identity and standing
-> scoped auth/session projection
-> Matrix, Forgejo, and other service access
-> receipts for meaningful changes
```

Until that path exists, Matrix and Forgejo accounts are operational service
state. Their admin panels are not sources of institutional truth.

## See also

- [Service hosting model](../../architecture/SERVICE_HOSTING_MODEL.md)
- [Forgejo deployment plan](../../ops/FORGEJO_DEPLOYMENT_PLAN.md)
- [Get Involved](https://intercooperative.network/get-involved)
- [Community](https://intercooperative.network/community)
- [Code of Conduct](../../../CODE_OF_CONDUCT.md)
