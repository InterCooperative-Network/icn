# icn.zone Domain Routing Plan

**Status:** Plan — not yet implemented
**Last Updated:** 2026-04-04

## Domain Distinction

| Domain | Role | Status |
|--------|------|--------|
| `intercooperative.network` | Canonical public website | Live (GitHub Pages + CNAME) |
| `icn.zone` | Utility / short-link domain | Subdomains active; root redirect planned (Phase 1) |
| `api.icn.zone` | Gateway REST + WebSocket (K3s) | Active |
| `pilot.icn.zone` | Pilot UI (K3s) | Active |
| `metrics.icn.zone` | Prometheus / Grafana (K3s) | Active |

`icn.zone` is **not** a second marketing website. The root redirects to `intercooperative.network`.
Subdomains serve K3s cluster endpoints and future short-link utilities.

---

## Root Behavior

Root `icn.zone` (no subdomain, any path) redirects to `intercooperative.network`.

**Implementation: Cloudflare Redirect Rule**

```
Zone:    icn.zone
Match:   hostname eq "icn.zone"
Action:  301 redirect → https://intercooperative.network${uri.path}
```

Path-preserving: `icn.zone/docs` → `intercooperative.network/docs`.

Do **not** deploy a second static site at `icn.zone` root. The redirect is the entire root behavior.

---

## Short-Link Path Model

Reserve the following path prefixes at `icn.zone` for future utility use.
These paths will **not** fall through to the root redirect — they will be handled by a Cloudflare Worker.

| Prefix | Purpose | Status |
|--------|---------|--------|
| `/j/<code>` | Cooperative join codes | Future (Worker) |
| `/i/<code>` | Member invite links | Future (Worker) |
| `/n/<name>` | Human-readable cooperative name shortcuts | Future (Worker) |

All other paths → root redirect rule above.

---

## Implementation Order

### Phase 1 — Root redirect (do now)
Add a Cloudflare Redirect Rule in the `icn.zone` zone:

1. Log into Cloudflare → `icn.zone` zone
2. Rules → Redirect Rules → Create Rule
3. Match: `hostname eq "icn.zone"`
4. Action: Dynamic redirect, `https://intercooperative.network${uri.path}`, 301
5. Save and verify: `curl -I https://icn.zone` should return `301 → https://intercooperative.network/`

### Phase 2 — Short-link Worker (build when needed)

> **Resolved 2026-09-15: Model B — ICN-authoritative resolution.** Step 2 below
> previously read "Looks up code in Cloudflare KV **or** queries the ICN gateway
> API." Those are not two spellings of one design: the first makes Cloudflare an
> authoritative store for admission-adjacent state, and the second does not.
> The decision, its alternatives, and the responsibility matrix behind it are in
> [ICN_ZONE_JOIN_TRUST_BOUNDARY.md](../design/ICN_ZONE_JOIN_TRUST_BOUNDARY.md) §4.
> **No authoritative mapping is stored at Cloudflare, and no edge cache is
> permitted until invite revocation exists** — caching would set a revocation
> latency floor before the revocation mechanism it trades against is built.

Deploy a Cloudflare Worker to `icn.zone` that:
1. Matches `/j/*`, `/i/*`, `/n/*` paths
2. **Relays the code to the ICN gateway's read-only resolution endpoint.** No
   Cloudflare KV. No cache. ICN answers which institution the admission context
   belongs to; the Worker originates nothing.
3. Returns a 302 redirect to the ICN-supplied destination, with `no-store`,
   `Referrer-Policy: no-referrer`, and `X-Robots-Tag: noindex, nofollow`
4. Falls through to the redirect rule for all unmatched paths

Worker deployment does not require changes to the ICN Rust codebase — true for
the Phase 1 redirect, and **a warning sign for Phase 2**: anything that resolves
a code needs an ICN-side endpoint that does not exist yet, and resolution logic
living only in a Worker would be unversioned relative to the kernel it fronts.
Phase 2 is blocked on that endpoint plus durable, revocable invite records; see
the trust-boundary doc §3 and §9.

**The route resolves to a join page, never to an outcome.** A code identifies an
admission context; it does not grant membership. Possession must never mint a
session, mark a code used, or mutate institutional state.

---

## Implementation Layer

| Component | Where | Not here |
|-----------|-------|----------|
| Root redirect | Cloudflare Redirect Rules | Not in K3s, not in Astro site |
| Short-link handler | Cloudflare Worker + KV | Not in ICN daemon crate |
| Subdomain DNS (`api.`, `pilot.`, `metrics.`) | Cloudflare DNS A/CNAME records | Unchanged |

**K3s ingress is unaffected.** Cloudflare routes by full hostname — subdomain records take precedence over wildcard rules.

---

## Why Not a Second Website

- Maintains single canonical public site at `intercooperative.network`
- Zero build/deploy pipeline to maintain at `icn.zone`
- Cloudflare Redirect Rules are instant, free-tier eligible, zero-latency
- Short-link Workers are independently deployable and don't touch the Astro or Rust codebase
- Preserves the subdomain pattern already in use for K3s services
