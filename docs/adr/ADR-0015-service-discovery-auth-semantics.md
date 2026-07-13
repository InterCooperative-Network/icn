# ADR-0015: Service Discovery Auth Semantics — Auth-gated with Enumeration-Safe 404

**Date**: 2026-03-21
**Status**: accepted
**Implementation status**: auth enforcement verified (2026-07-13); one divergence pending decision — unauthorized callers receive uniform 401, not the literal 404 this ADR mandates (see Implementation Status below)
**Tags**: gateway, api, security, service-discovery
**Supersedes**: N/A
**Note**: Originally filed as ADR-0009 in `ops/state/decisions/` (collided with another decision sharing that number). Renumbered to 0015 when ADRs were canonicalized under `docs/adr/`.

## Implementation status (2026-07-13)

**Auth enforcement: VERIFIED.** A full mounting-chain trace (issue #1642) proved all five
`/v1/services/*` routes are wrapped by `HttpAuthentication::bearer(jwt_auth)` at the
`web::scope("/services")` level in `server.rs`, with auth outermost (it runs before the
trust rate-limit middleware and before any handler or resource lookup). The wrap is
unconditional — it holds in default, dev (`ICN_DEV_MODE`), and loopback-only
`--insecure-gateway-no-jwt` configurations (the latter only installs a well-known JWT
secret; the bearer middleware still runs). The 2026-04-26 concern below is resolved: the
gating lives in `server.rs` scope wiring, not in `api/services.rs`, which is why a
handler-file read could not see it.

**Anti-oracle property: HOLDS.** Because authentication precedes any lookup, the
unauthenticated/invalid-token response is uniform (identical for existing and
nonexistent `service_id`s) — no enumeration oracle exists.

**Status-code divergence: PENDING DECISION.** Unauthorized callers receive **401**, not
the **404** this ADR's decision point 3 literally mandates. Both are enumeration-safe;
conforming to the literal 404 would require a custom auth-failure mapper for this scope.
The choice — conform the code to literal-404, or amend point 3 to accept uniform-401 as
satisfying its anti-oracle intent — is a maintainer decision tracked on issue #1642.
Until it is resolved, the current (most restrictive, uniform-401) behavior is pinned by
`icn/crates/icn-gateway/tests/services_auth_boundary.rs`, which asserts: no token → 401
on all five routes (uniform across existence); invalid token → 401 (uniform); valid
token + nonexistent → 404 for `GET/DELETE /{id}` and empty `/discover`; the list route
returns 200 with an empty set (list semantics); valid token + existing → 200.

**Note on the 2026-04-26 verification recipe below:** its test recipe was internally
contradictory (it asked for both "401 without a JWT" and "404, not 401, for a
nonexistent resource without a JWT" — impossible for the same request). The landed test
pins the uniform-401 reality and must be updated in the same PR as any future
semantics change.

## Prior implementation-status note (2026-04-26, superseded)

**`needs verification`.** A code read on 2026-04-26 found:

- `icn/crates/icn-gateway/src/api/services.rs` registers the service-discovery
  routes (`announce`, `discover`, `query`, `get`, `withdraw`) without any
  visible JWT extractor (`JwtUser`, `auth_wrap`) at the route level. The
  `configure(cfg: &mut web::ServiceConfig)` block mounts the routes directly:
  ```text
  cfg.service(announce_service)
      .service(discover_services)
      .service(query_services)
      .service(get_service)
      .service(withdraw_service);
  ```
  No per-route auth middleware was found in the file.
- A "no results → 404" code path is documented at
  `icn/crates/icn-gateway/src/api/services.rs:387` (in `query_services`),
  matching half of the ADR's decision (the missing-resource → 404 case).
- The auth-boundary tests file `icn/crates/icn-gossip/tests/service_discovery_auth_boundary.rs`
  exists but exercises gossip-side auth, not the gateway HTTP route gating
  this ADR mandates.
- The original auth gating may live in route configuration outside
  `api/services.rs` (e.g. in `server.rs` or via a higher-level scope
  middleware). This was not traced in the 2026-04-26 read.

**What this means for the ADR:** the *decision* (Option B — all
`/v1/services/*` endpoints require JWT, with enumeration-safe 404 for
unauthorized callers) is unchanged and remains accepted. Whether the
*implementation* fully matches that decision today requires a focused
verification pass against the live gateway: an integration test that
asserts each of `POST /v1/services/announce`, `GET /v1/services`,
`GET /v1/services/{id}`, `DELETE /v1/services/{id}` returns 401 (or
the configured equivalent) without a JWT, returns 404 for a
non-existent resource with a valid JWT, and returns 404 (not 401) for
a non-existent resource without a JWT.

**Follow-up issue (suggested):** open an ICN issue
`audit(gateway): verify ADR-0015 service-discovery auth + enumeration-safe 404 in code`
before marking implementation `implemented`. The current 2026-04-26 read is
inconclusive enough that a guess either way would falsify the ADR.

## Context

During end-to-end demo validation (PR #1115), `GET /v1/services/:id` returned **401** when a
service had been withdrawn and no JWT was present. The expected behavior for a missing resource
is **404**.

ICN's "adversarial by default" invariant (documented in CLAUDE.md and ARCHITECTURE.md) applies to
the product surface as much as to internal peer communication. Inconsistent auth boundaries —
some service discovery endpoints requiring auth, others not — leak information and create an
unintended enumeration oracle.

The current behavior is inconsistent:
- `POST /v1/services/announce` → 200 (no auth required)
- `GET /v1/services/:id` → 401 (auth required)
- `DELETE /v1/services/:id` → 200 (no auth required)

This inconsistency was not intentional; it reflects incomplete auth middleware wiring, not a
deliberate design.

## Decision

**Option B: All service discovery endpoints require JWT consistently.**

Specific behavior:
1. All `/v1/services/*` endpoints require a valid JWT (`Authorization: Bearer <token>`)
2. For authorized callers, a missing resource returns **404** (not 401)
3. For **unauthorized callers**, a missing OR existing resource returns **404** — not 401

Point 3 is the enumeration-prevention rule: returning 401 for "exists but unauthorized" vs 404
for "doesn't exist" would allow unauthenticated callers to learn which service IDs are valid by
probing the status code. Returning 404 uniformly prevents the endpoint from becoming an oracle.

This is consistent with ICN's adversarial-by-default invariant applied to the HTTP API surface.

## Consequences

**Easier**:
- Auth model is simple and auditable: every service discovery call requires a JWT
- No information leaks about resource existence to unauthenticated callers
- Demo scripts can treat 401 and 404 as equivalent for "service gone" (current workaround becomes
  unnecessary)

**Harder**:
- Read-only public service directories are not possible without auth, which may be limiting for
  some future use cases (public cooperative service catalogs)
- OpenAPI spec must be updated to document required auth on all `/v1/services/*` endpoints

**Implementation required** (separate PR from this decision):
- Add JWT auth middleware to `POST /v1/services/announce` and `DELETE /v1/services/:id`
- Change 401 → 404 logic for missing-resource paths behind auth
- Change 401 → 404 for unauthenticated requests to prevent enumeration
- Update OpenAPI spec

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| **Option A: No auth on read endpoints (GET)** | Leaks service inventory to unauthenticated callers; inconsistent with adversarial-by-default posture |
| **Option C: Auth required but 401 on unauthorized** | Creates an enumeration oracle — caller can distinguish "missing" from "unauthorized" by status code |
| **Option D: Public read, auth for write** | Reasonable for public registries but premature for ICN's current pilot scope; cooperative membership model implies callers are authenticated participants |
