# ADR-0015: Service Discovery Auth Semantics — Auth-gated with Enumeration-Safe 404

**Date**: 2026-03-21
**Status**: accepted (amended 2026-07-13 — see Amendment section)
**Implementation status**: implemented (verified 2026-07-13; enforcement `icn/crates/icn-gateway/src/server.rs` `/services` scope bearer wrap, behavior pinned by `icn/crates/icn-gateway/tests/services_auth_boundary.rs`, PR #2417)
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

**Status codes: RESOLVED by the 2026-07-13 amendment (uniform pre-lookup 401 retained).**
The implemented behavior matches the amended decision exactly and is pinned by
`icn/crates/icn-gateway/tests/services_auth_boundary.rs`: no token → 401 on all five
routes with the same status for existing and nonexistent ids; invalid token → 401,
likewise uniform; valid token + nonexistent → 404 for `GET/DELETE /{id}` and for
`/discover` with no matches; the list route returns 200 with an empty set (list
semantics); valid token + existing → 200; withdraw by a non-owning provider → 403.
(The uniformity assertions compare HTTP status codes — the pre-lookup gate makes
body-level divergence structurally impossible for credential failures, since no
resource data is loaded before rejection.) No custom 401→404 mapper exists and none is
needed; runtime behavior was not changed by the verification or the amendment.

**Note on the 2026-04-26 verification recipe below:** its test recipe was internally
contradictory (it asked for both "401 without a JWT" and "404, not 401, for a
nonexistent resource without a JWT" — impossible for the same request). The amendment
dissolves the contradiction; the landed test pins the amended semantics.

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
   *(superseded by the 2026-07-13 amendment below: uniform pre-lookup **401** is the
   accepted behavior; the enumeration-prevention goal this point encoded is preserved)*

Point 3 was the enumeration-prevention rule: a response that differs between "exists but
unauthorized" and "doesn't exist" would let unauthenticated callers learn which service IDs are
valid by probing the status code. The amendment keeps that rule but binds it to response
*uniformity* rather than to the literal 404.

This is consistent with ICN's adversarial-by-default invariant applied to the HTTP API surface.

## Amendment (2026-07-13): authentication, authorization, and absence are distinct; uniform pre-lookup 401 is compliant

Adopted maintainer decision (issue #1642). The three failure families are distinct outcomes,
and the enumeration-safety invariant binds the first to *uniformity*, not to a specific code:

1. Every `/v1/services/*` route requires a valid JWT.
2. Missing or invalid credentials return a uniform **401** *before* any resource lookup.
3. An authenticated request for a missing individual resource returns **404**.
4. An authenticated request that reaches an operation-specific authority boundary may return
   **403** (e.g. `DELETE /{service_id}` by a non-owning provider).
5. **No response to missing or invalid credentials may depend on whether the requested service
   ID exists.** Because authentication precedes lookup, this holds by construction and is pinned
   by tests.
6. If service visibility later becomes scoped by member, cooperative, community, or federation
   boundaries, authenticated existence-disclosure behavior must be reviewed again — a valid JWT
   must not become a license to enumerate services outside the caller's scope.

The original point 3 ("unauthorized → 404") is superseded: its stated goal was enumeration
prevention, which uniform pre-lookup 401 satisfies with standard HTTP semantics (the
`WWW-Authenticate` challenge is preserved and no custom 401→404 mapper is needed). Note on the
rejected **Option C** below: the variant it rejected is an existence-*dependent* 401 (auth
evaluated after lookup); that rejection stands and does not apply to the uniform pre-lookup 401
this amendment accepts.

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

**Implementation required** (as written 2026-03-21; status as of the 2026-07-13 amendment):
- Add JWT auth middleware to `POST /v1/services/announce` and `DELETE /v1/services/:id`
  — *done (the whole `/services` scope is bearer-wrapped in `server.rs`; predates this ADR)*
- Change 401 → 404 logic for missing-resource paths behind auth — *already correct
  (authenticated + missing → 404)*
- Change 401 → 404 for unauthenticated requests to prevent enumeration — *superseded by the
  amendment: uniform pre-lookup 401 is the accepted behavior; never implemented, no longer
  required*
- Update OpenAPI spec — *outstanding; tracked by the API-classification follow-up (the
  services routes are among the ~380 routes absent from the generated spec)*

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| **Option A: No auth on read endpoints (GET)** | Leaks service inventory to unauthenticated callers; inconsistent with adversarial-by-default posture |
| **Option C: Auth required but 401 on unauthorized** | Creates an enumeration oracle — caller can distinguish "missing" from "unauthorized" by status code |
| **Option D: Public read, auth for write** | Reasonable for public registries but premature for ICN's current pilot scope; cooperative membership model implies callers are authenticated participants |
