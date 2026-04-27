---
Status: design-direction
Authority: architecture (forward-direction; not yet normative)
Canonical: no
Last Reviewed: 2026-04-27
---

# Domain Routing and DNS Bindings

> **Status: draft / design direction / roadmap.** Names a future routing model for ICN. Items marked _planned_ are not in the repo today. Companion to [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md).

## Summary

Three concepts, often conflated, must stay distinct:

| Concept | Example | Authority |
|---|---|---|
| **Institutional domain** | `nycn` | ICN authority/security/governance boundary. Owns members, roles, standing, data, tools, agreements, receipts. |
| **ICN utility route** | `nycn.icn.zone` | ICN-managed default; bootstrap namespace; **utility, not canonical truth**. |
| **Custom public domain** | A cooperative's own DNS, e.g. NYCN's public site | Institution-owned. The public face of the institution on the open web. |

Plus a fourth, optional surface:

| Concept | Example |
|---|---|
| **Short route** | `icn.zone/n/nycn` — human-friendly redirect to whichever route the institution wants to expose |

### Why this distinction matters

Cooperative institutions need an authority boundary that does **not** depend on a vendor's DNS. Treating "ICN domain" and "DNS domain" as the same thing rebuilds the platform-landlord pattern: institutions become tenants of whoever owns the DNS namespace.

The institutional domain is the ICN object (`InstitutionalDomain`, planned). The DNS layer is just how packets find an HTTP surface. They bind to each other through an explicit, receipted process; they do not collapse into one thing.

## Routing analogy (not a precedent)

Microsoft gives tenants an `example.onmicrosoft.com` domain and lets them verify `example.org`. ICN can use `icn.zone` similarly as a default utility namespace — but ICN must not become a landlord platform. The institution's real authority is the ICN `InstitutionalDomain`, and the institution may use its own custom public DNS.

The analogy stops at "utility namespace." ICN is not a tenancy.

## Surfaces

| Surface | Use |
|---|---|
| `icn.zone` | **Primary public ICN surface.** Authority over what ICN is, plus the namespace under which institutional utility routes (e.g. `nycn.icn.zone`) and short-links (e.g. `icn.zone/n/nycn`) live. |
| `learn.icn.zone` | Teaching surface (the icn-learn repo's eventual public face). Teaches ICN; does not define it. |
| Custom institution domain | Each institution's own DNS, owned by the institution. Bound to the institutional domain via a verified `DnsBinding`. |

> `icn.zone` is the primary public ICN surface. Institutional sub-routes under it (`nycn.icn.zone`, `icn.zone/n/nycn`, etc.) are utility routes for finding institutions; they are not the institution's own authority. Learning surfaces live at `learn.icn.zone`.

## Future objects

### `DnsBinding`

A binding between an `InstitutionalDomain` and a routable DNS name.

Future fields (illustrative shape — not yet committed):

- `binding_id`
- `institutional_domain` (the ICN scope, e.g. `nycn`)
- `host` (e.g. `nycn.icn.zone` or a custom public domain)
- `purpose` (`default_icn_zone` / `short_route` / `custom_public_domain` / `event_route` / etc.)
- `verification` (TXT challenge value, verification method, verification time)
- `status` (`pending` / `verified` / `revoked`)
- `binding_receipt_hash` (link into the receipt envelope)
- `valid_from` / `valid_until` (optional)

A binding is approved by domain governance and emits a receipt. Reversal is a counter-record, not a mutation (consistent with [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)).

### Verification flow (planned)

```
request custom domain                ──► proposal / charter act
   ↓
add TXT challenge                    ──► institution adds DNS record
   ↓
verify control                       ──► ICN node confirms TXT match
   ↓
approve binding                      ──► governance / domain admin act
   ↓
provision route / cert               ──► routing layer + ACME or equivalent
   ↓
emit binding receipt                 ──► receipt envelope (ADR-0026 Layer 2)
```

Each step is auditable. The binding's existence is provable from the receipt envelope alone, without trusting the routing layer.

### `PublicationReceipt` (planned)

When content is published to a routed surface (custom domain or `icn.zone` route), the publish action emits a `PublicationReceipt` carrying:

- domain
- binding the publication used
- content artifact hash
- approving governance reference (proposal id or charter id, as applicable)
- timestamp
- runner DID

Public surfaces become **provably approved** — the published content can be verified back to a governance act.

## NYCN example (illustrative)

NYCN-correct facts (preserve these — they replace prior placeholder text):

```yaml
institutional_domain:
  id: nycn
  canonical_name: "New York Cooperative Network"

dns_bindings:
  default_icn_zone:
    host: nycn.icn.zone
    purpose: "ICN utility route / bootstrap namespace"

  short_route:
    host: icn.zone/n/nycn
    purpose: "Human-friendly ICN short link"

  custom_public_domain:
    # NYCN's own public domain (held by NYCN, not ICN). Documented in the
    # NYCN repo, not duplicated into ICN core.
    purpose: "NYCN public website / public identity"

  possible_summit_route:
    # Public route for the 2026 Summit (Schenectady, NY, 2026-10-17),
    # if NYCN chooses to expose one under their custom domain.
    purpose: "Public route for 2026 Summit (planned/example)"
```

NYCN-specific binding files (the `institution/domain-bindings.example.yaml` referenced in the parent doc) live in the NYCN repo. ICN core does **not** carry NYCN's specific public domain — that stays where it belongs, in the package.

> **Important:** the prior placeholder `newyorkcooperative.network` is stale and incorrect. NYCN's actual public domain is documented in the NYCN repo (`README.md`, `institution/package.yaml`). Do not introduce NYCN-specific public-domain text into the ICN public website.

## Routing semantics

A request to `nycn.icn.zone/...` should resolve to the routing layer for the `nycn` institutional domain. The routing layer:

1. Checks the requested path against the domain's installed tools (each tool exposes UI surfaces declared in its `ToolManifest`).
2. Checks the request's authentication against the domain's `DomainPolicy` (a `DomainSession` is established for member traffic; service-identity traffic uses scoped capabilities).
3. Forwards to the appropriate tool service, or returns the public-facing surface for unauthenticated requests.

For a custom public domain, the same flow applies — the difference is only the host header that arrived. The institution's authority is the same in either case.

## Boundary discipline

- The routing layer **never** owns institutional truth. It is a router, not a database.
- A `DnsBinding` does not grant authority over the institutional domain. Authority is governed by charter / role / standing; the binding only routes.
- `icn.zone` is **utility, not authority**. ICN as a project must not become a landlord of institutional names.
- Custom domains never override the primary ICN public surface (`icn.zone`). A cooperative's site can describe ICN; only canonical surfaces under `icn.zone` define ICN.

## Existing repo support

What exists today that this design will sit on:

- [RFC-0015](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) — public surface and learning repo architecture (governs `icn.zone`, `learn.icn.zone`, and any other ICN-managed surfaces; this design refers to it for canonical-surface roles)
- Receipt envelope for binding receipts and publication receipts ([ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md))
- Charter/role/`authority_scope` plumbing for governance acts that approve bindings (existing)
- Public ICN website source under `website/src` — **does not** carry NYCN/Summit nouns

## Missing buildout

- `DnsBinding` object + verification record
- `PublicationReceipt` class
- Routing layer for `*.icn.zone` and custom institutional domains
- Per-tool UI-surface declarations in `ToolManifest`
- Provisioning automation (ACME or equivalent) gated on a verified binding
- Per-domain admin UI for managing bindings (future tool: `icn-domain-admin`)

## Non-goals

- **No** runtime implementation in this PR.
- **No** public website changes.
- **No** new NYCN-specific text in `website/src`.
- **No** `learn.icn.zone` link on the public ICN website (RFC-0015 governs the public surface; this doc does not change it).
- **No** treatment of `icn.zone` as canonical truth.
- **No** vendor lock-in to a specific DNS provider, certificate authority, or routing stack — the design is provider-agnostic.

## References

- [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) — parent design (institutional domains, agreements, receipts)
- [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) — companion: tools that publish to routed surfaces
- [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md) — companion: advisory compute
- [RFC-0015](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md)
- [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md), [ADR-0027](../adr/ADR-0027-action-card-contract.md)
