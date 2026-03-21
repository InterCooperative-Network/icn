# ADR-0009: Service Discovery Auth Semantics — Auth-gated with Enumeration-Safe 404

**Date**: 2026-03-21
**Status**: accepted
**Tags**: gateway, api, security, service-discovery
**Supersedes**: N/A

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
