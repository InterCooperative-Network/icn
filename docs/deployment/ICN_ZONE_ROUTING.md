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
Deploy a Cloudflare Worker to `icn.zone` that:
1. Matches `/j/*`, `/i/*`, `/n/*` paths
2. Looks up code in Cloudflare KV or queries the ICN gateway API
3. Returns a 302 redirect to the resolved destination
4. Falls through to the redirect rule for all unmatched paths

Worker deployment does not require changes to the ICN Rust codebase.

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
