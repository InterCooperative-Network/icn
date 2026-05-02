---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-01
---

# Auth Bridge and DID Login

> **Status: design direction.** This document defines how conventional identity-provider infrastructure should bridge into ICN DID identity and authority. It is not an implementation report and it does not select a specific IdP product.

## Summary

ICN should support conventional web login for conventional web services, but without letting the identity-provider layer become the source of institutional authority.

The rule:

```text
OIDC authenticates sessions.
ICN authorizes institutional power.
Receipts prove institutional transitions.
```

Keycloak, authentik, ZITADEL, Ory, Kanidm, Forgejo login sources, and similar systems may help browsers and services establish sessions. They do not replace DIDs, standing, role assignments, authority scopes, governance decisions, service identities, or receipts.

## Why this exists

Many useful services speak ordinary identity-provider protocols:

- OIDC
- OAuth2
- SAML
- LDAP / directory bridges
- passkeys / WebAuthn
- session cookies

A sovereign forge, status page, metrics dashboard, docs service, registry, or operator console should not each invent its own login system. A shared auth bridge is the right service boundary.

But ICN is not building another corporate tenant stack. The bridge exists so ordinary tools can understand ICN identity; it must not become the authority source.

## Identity stack

```text
DID / ICN identity
  = durable cryptographic identity

Device keys / recovery
  = how that identity survives device change or loss

Auth bridge / IdP
  = how web services receive login sessions

ICN governance / standing / authority
  = what the DID may do inside a domain

Receipts
  = proof that a meaningful transition was authorized and happened
```

A user may log into a web service through an IdP. The institution still knows the person or service by DID and domain-scoped authority.

## Correct authority direction

Correct:

```text
DID proves control
  ↓
ICN checks standing / role / authority scope
  ↓
auth bridge issues short-lived OIDC claims
  ↓
service grants local session
  ↓
important action emits receipt
```

Incorrect:

```text
IdP group membership
  ↓
service-local admin role
  ↓
institutional authority
```

The incorrect pattern lets a web admin panel silently become the constitution.

## Candidate service shape

The first auth bridge should likely live at:

```text
auth.intercooperative.network
```

Possible implementation choices:

| Candidate | Useful for | Caution |
|---|---|---|
| Keycloak | Mature OIDC/OAuth2/SAML bridge, extensibility, enterprise patterns | Heavier operational surface. Must not become authority source. |
| authentik | Friendly self-hosted SSO and proxy-provider patterns | Still an IdP projection; ICN authority remains elsewhere. |
| ZITADEL | Organization/multitenant identity patterns | Needs evaluation against ICN domain model. |
| Ory Hydra/Kratos | More composable identity stack | More assembly; probably not first deployment unless custom UX is desired. |
| Kanidm | Modern identity management with Rust alignment | Needs maturity/fit review for ICN service integration. |

The implementation choice is operational. The architecture rule is product-agnostic.

## DID login flow

A future `icn-id-bridge` or equivalent service should support a DID challenge-response login flow.

```text
1. User opens forge.intercooperative.network.
2. Forge redirects to auth.intercooperative.network.
3. Auth bridge offers DID login.
4. Bridge issues a nonce.
5. User signs nonce with an authorized DID/device key.
6. Bridge verifies DID authentication.
7. Bridge queries ICN standing / role / authority scope for the target domain.
8. Bridge issues short-lived OIDC claims.
9. Forge creates or updates a service-local session.
10. Important service actions later map back to ICN receipts.
```

The session is temporary. The DID is durable.

## Example claims

An OIDC token may project ICN state into claims like:

```json
{
  "sub": "did:icn:z...",
  "icn_did": "did:icn:z...",
  "icn_domain": "intercooperative-network",
  "icn_standing": "active",
  "icn_roles": ["maintainer", "infra-operator"],
  "icn_scopes": ["repo:write", "release:publish"],
  "icn_claims_version": "v1"
}
```

Claims are projections. They must be short-lived and refreshable from ICN state.

## External account bindings

External identities may be useful as login conveniences:

- GitHub account
- Google account
- email/password account
- passkey account
- institutional directory account

But they must be bound to an ICN DID through an explicit ceremony.

Correct pattern:

```text
external account login
  ↓
DID challenge signature
  ↓
binding record
  ↓
optional receipt / audit record
  ↓
external account may help enter a session
```

External account binding does not make the external provider sovereign.

## Binding classes

| Binding | Meaning | Authority posture |
|---|---|---|
| Login convenience | External account can initiate a session. | No authority by itself. |
| Recovery aid | External account participates in recovery flow. | Must be bounded and revocable. |
| Migration bridge | Legacy account maps to DID during transition. | Temporary; should expire or be reviewed. |
| Service account binding | Non-human service identity maps to tool/service role. | Requires service identity and scoped capabilities. |

## Service identity login

Services also need to authenticate.

A service identity may represent:

- forge event adapter
- CI runner
- release builder
- backup job
- bridge importer
- metrics collector
- model runner
- registry publisher

Service login must be scoped more tightly than human login. A service should receive only the capabilities needed for its job, for the shortest useful duration.

## What the auth bridge may do

The auth bridge may:

- verify DID challenge-response login
- broker external IdP login
- issue OIDC claims
- map ICN standing/roles/scopes into service claims
- maintain short-lived sessions
- support passkeys or MFA as session-hardening
- record sensitive access changes for audit
- help ordinary tools use ICN identity without custom integrations

## What the auth bridge must not do

The auth bridge must not:

- replace DIDs as identity roots
- decide membership
- decide standing
- assign institutional authority
- become the canonical role database
- silently grant repository, release, or admin power
- become the only recovery path
- trap identity data without export
- require all members to use a developer-oriented login flow

## Access changes and receipts

Not every login needs a receipt. Routine logins may be operational logs.

Actions that should become receipt-bearing or at least ICN-audited include:

- grant admin access
- revoke admin access
- bind external account to DID
- rotate service identity credential
- create OIDC client for a governed service
- change claim-mapping policy
- change MFA/passkey policy for privileged scopes
- disable a service identity
- recover a privileged account

The receipt taxonomy is future work and should align with ADR-0026 rather than create a parallel audit store.

## Browser and mobile posture

The bridge must support ordinary browser flows and future mobile/member-shell flows.

Member-facing participation must not require:

- command line use
- developer accounts
- special browser extensions
- crypto-wallet metaphors
- long-lived bearer tokens copied by hand
- laptop-only workflows

The auth bridge is infrastructure. It exists to make member access easier, not to make technical users feel clever.

## Relationship to protocol selection

For browser services, the bridge should prefer:

- HTTP-only secure cookies for ordinary browser sessions.
- Short-lived tokens for service-to-service calls.
- SSE streams authorized by cookie or short-lived stream token where appropriate.
- WebSockets only where a service has a real bidirectional live-state requirement.

See [`PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`](PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md).

## Migration posture

The first deployment can be pragmatic:

1. Deploy an IdP bridge for ordinary service login.
2. Connect Forgejo via OIDC.
3. Manually map initial maintainers/admins.
4. Add DID claim-mapping later.
5. Add external account binding records later.
6. Add access-change receipts later.
7. Move local role truth out of service-local groups and into ICN standing/authority as runtime support lands.

Do not block the first service deployment on a perfect DID-native flow. Do not pretend a conventional IdP is the final authority model.

## Non-goals

- No product selection in this document.
- No runtime implementation.
- No claim that DID login exists today.
- No replacement of ICN identity or authority with IdP groups.
- No requirement that ordinary members interact with Keycloak-like administration surfaces.
- No new cryptographic method definition here.

## One-line rule

**The auth bridge translates ICN identity into web sessions; it does not translate web sessions into ICN authority.**
