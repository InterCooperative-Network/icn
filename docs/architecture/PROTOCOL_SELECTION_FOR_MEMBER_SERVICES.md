---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-01
---

# Protocol Selection for Member Services

> **Status: design direction.** This document defines protocol-selection doctrine for ICN member-facing and operator-facing services. It is not an implementation report.

## Summary

Protocol choice is infrastructure design. Every protocol carries promises that the network, load balancers, proxies, browsers, mobile devices, and service operators must live with.

ICN should use the least stateful protocol that preserves the institutional function.

Default doctrine:

```text
HTTP is for institutional facts.
SSE is for institutional awareness.
WebSockets are for shared live activity.
Receipts are the source of truth.
Polling is the fallback.
```

This doctrine applies to ICN runtime surfaces, member shells, service-hosting surfaces, operator dashboards, future tool commons services, and institution packages that expose member-facing flows.

## Why this matters

ICN must work for people on phones, older laptops, public Wi-Fi, rural connections, cellular networks, shared devices, screen readers, and interrupted sessions. A fragile real-time stack privileges people with stable hardware and stable networks.

Protocol choice is therefore an accessibility issue, not just an engineering preference.

## Commands, queries, events

ICN services should separate three things.

### Commands

Commands intentionally change state.

Examples (illustrative; consult the gateway's current routes for the canonical surface):

```text
POST /v1/gov/proposals                                              # current implemented route
POST /v1/gov/proposals/{proposal_id}/vote                           # current implemented route
PUT  /v1/gov/domains/{domain_id}/action-items/{item_id}/status      # illustrative future shape
POST /v1/services/{service_id}/access-grants                        # illustrative future shape
```

Commands should use ordinary HTTP. They should be idempotent where possible and receipt-bearing where the transition matters.

### Queries

Queries read canonical state.

Examples (illustrative; current gateway routes are the canonical surface):

```text
GET /v1/gov/me/standing                  # current implemented route
GET /v1/gov/me/action-cards              # current implemented route
GET /v1/receipts/{receipt_id}            # illustrative
GET /v1/services/{service_id}            # current implemented route
```

Queries should use ordinary HTTP. They should not require a live socket to answer basic membership, governance, or receipt questions.

### Events

Events notify clients that something changed.

Examples (conceptual stream shapes; not an existing API contract — at least the governance `/events` resource is not currently exposed in the gateway):

```text
GET /v1/events/stream                              # conceptual global stream
GET /v1/gov/domains/{domain_id}/events             # conceptual domain-scoped stream
GET /v1/services/forge/builds/{build_id}/events    # conceptual build-scoped stream
```

Events should usually use SSE if they are one-way server-to-client updates. Implementation should use whatever streaming endpoints the gateway already exposes; the names above describe the *shape* of streams a future gateway would offer, not endpoints that exist today.

## SSE default for awareness

Server-Sent Events are the default for one-way update streams:

- action card became available
- receipt was created
- proposal changed state
- build step progressed
- compute job completed
- backup finished
- mirror sync finished
- service incident updated

SSE keeps the update channel close to ordinary HTTP. It supports replay through event IDs and works well for awareness streams where the server talks and the client listens.

## WebSocket justification rule

A service must justify WebSockets with a real bidirectional requirement.

Use WebSockets for:

- collaborative editing
- live meeting room presence where both sides send independent events
- interactive operator shell or console sessions
- multiplayer-style deliberation surfaces if they ever exist
- low-latency bidirectional workflows where HTTP + SSE is insufficient

Do not use WebSockets for:

- action card refresh
- ordinary notifications
- receipt updates
- build progress
- governance event feeds
- status dashboards
- most admin dashboards

Those should be HTTP + SSE until proven otherwise.

## Event streams carry pointers, not secrets

Event streams should usually carry pointers rather than full private payloads.

Preferred shape:

```json
{
  "event_id": "18492",
  "type": "receipt.created",
  "domain_id": "icn-core",
  "receipt_id": "rec_abc",
  "source": "action_item.complete"
}
```

Then the client fetches:

```text
GET /v1/receipts/rec_abc
```

That lets normal authorization protect the full object. It also reduces risk from logs, proxies, browser buffers, and cross-scope leaks.

## Replayability

Any event stream that carries institutional awareness must be replayable.

Required event fields:

```text
event_id
sequence or cursor
source
domain_id or scope
type
timestamp
object pointer
```

A reconnecting client should be able to resume:

```text
Last-Event-ID: 18492
```

or equivalent cursor semantics.

A disconnected member must not lose institutional state because the network dropped.

## Disconnection is normal

ICN services must assume:

- phones sleep
- laptops close
- Wi-Fi changes
- tunnels kill connections
- cellular NATs evict idle sessions
- browsers refresh
- deployments restart services
- gateways fail

Therefore:

- writes should be idempotent where possible
- client UX should distinguish pending / sent / confirmed
- receipt confirmation should be explicit
- streams should replay
- critical functions should have polling fallback
- reconnect should use backoff and jitter where the client controls reconnection

## Polling fallback

Critical member functions must not depend exclusively on live transport.

If SSE or WebSockets fail, the client should fall back to ordinary HTTP polling for critical surfaces:

```text
GET /v1/gov/me/action-cards
GET /v1/gov/me/standing
GET /v1/receipts/{receipt_id}
GET /v1/services/{service_id}/status
```

Polling is not elegant. It is resilient. ICN should prefer member access over architectural vanity.

## Auth posture by protocol

### HTTP commands and queries

Use ordinary session or capability-token authorization, depending on client class.

Browser path:

```text
OIDC login → secure HTTP-only session cookie → HTTP command/query
```

Native/mobile path:

```text
DID/device auth → short-lived capability token → HTTP command/query
```

### SSE streams

Browser EventSource does not support arbitrary custom request headers. Therefore, choose deliberately:

- secure HTTP-only cookie session for browser streams;
- short-lived stream token if cookies are not available;
- fetch-based SSE only when custom headers are necessary;
- never put sensitive long-lived bearer tokens in URLs.

SSE streams should send pointers, not private payloads.

### WebSockets

WebSockets must authenticate at connection establishment and must re-check authorization for sensitive per-message actions. A long-lived connection must not become a long-lived authority grant.

WebSocket clients must implement:

- heartbeat / ping-pong or equivalent liveness check
- exponential backoff
- jitter
- reset-on-success
- state resync after reconnect
- authorization refresh or disconnect on privilege change

## Service template fields

Any service definition that exposes live updates should include:

```yaml
protocols:
  commands: HTTP
  queries: HTTP
  events: SSE
  realtime: none
replay:
  event_ids: true
  resume_after_disconnect: true
fallback:
  polling: true
payload_policy:
  event_streams_carry: pointers
receipt_source_of_truth: true
auth:
  browser: cookie-session
  mobile: short-lived-capability-token
```

For WebSocket services:

```yaml
protocols:
  realtime: WebSocket
why_websocket: collaborative editing requires bidirectional low-latency presence
heartbeat_required: true
reconnect:
  backoff: exponential
  jitter: true
  reset_on_success: true
state_recovery: document log / CRDT / receipt replay
fallback: read-only HTTP view
```

## Examples

| Surface | Preferred protocol | Reason |
|---|---|---|
| `/v1/gov/me/standing` | HTTP | Canonical read. |
| `/v1/gov/me/action-cards` | HTTP | Canonical read. |
| action-card updates | SSE | Awareness stream. |
| proposal vote | HTTP command | Intentional state transition. |
| receipt retrieval | HTTP | Canonical read. |
| receipt-created notification | SSE | Pointer to canonical record. |
| forge build progress | SSE | One-way progress stream. |
| forge PR discussion comments | HTTP + refresh/SSE | Not inherently bidirectional socket state. |
| collaborative document editing | WebSocket or equivalent | Shared live activity. |
| operator shell | WebSocket | Interactive bidirectional session. |
| service status page | HTTP + optional SSE | Public awareness. |

## Relationship to compute

Compute jobs should produce records and receipts, not hidden socket state.

Preferred flow:

```text
POST compute job
  ↓
HTTP returns job id
  ↓
SSE emits progress pointers
  ↓
GET job status / receipts for canonical state
```

A WebSocket may be justified for an interactive compute session, but batch jobs should not require one.

## Relationship to service hosting

Every hosted service should declare its protocol posture in its service definition. See [`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md).

A service that uses WebSockets must also declare the operational tax:

- connection limits
- backend routing model
- shared message bus if needed
- heartbeat interval
- reconnect behavior
- deploy/restart behavior
- state recovery path
- degraded mode

## Non-goals

- No ban on WebSockets.
- No runtime implementation.
- No protocol API spec.
- No requirement that every service expose SSE immediately.
- No claim that polling is the ideal UX.

## One-line rule

**Make institutional truth boring, make awareness replayable, and make live state justify itself.**
