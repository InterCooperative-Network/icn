---
id: "0011"
title: "Canonical Truth Ownership — Gateway vs Supervisor"
status: "accepted"
date: "2026-03-31"
context: "federation-clearing-position-api / PR #1477"
deciders: ["Matt Faherty"]
tags: ["gateway", "architecture", "federation", "clearing", "truth-ownership"]
---

# ADR 0011: Canonical Truth Ownership — Gateway vs. Supervisor

## Status

Accepted (2026-03-31)

## Context

During work on the federation clearing settlement feature (PRs #1474, #1476), a category-level architectural
bug was discovered and fixed: `GET /v1/federation/clearing/{id}/position` was reading clearing state from
the gateway's own `FederationManager` (backed by a temporary sled store) rather than the supervisor-owned
`FederationService` (backed by a persistent sled at `store_path/clearing`).

This was not merely a "wrong file path" bug. It was an instance of a broader failure mode: **the gateway
presenting state from a parallel, divergent store as if it were the authoritative answer**. In ICN, where
every output of the API represents the system's institutional reality, returning divergent state is a
legitimacy failure — not a minor inconsistency.

After fixing the specific bug (PR #1477), an architectural audit was conducted to determine whether similar
patterns existed elsewhere in the gateway.

## Decision

### The Invariant

> **No gateway-local authoritative state for supervisor-owned domains.**

In daemon mode, every mutable domain has exactly one canonical owner of truth:
- The supervisor (via `spawn_actors_with_identity`) creates and owns the authoritative service instances.
- The gateway is a read/write interface that **routes through** those service instances.
- Gateway-local managers are **compatibility/standalone fallback paths only**, not normative architecture.

### Canonical Truth Chain

For each supervisor-owned domain, the correct wiring chain is:

```
supervisor::spawn_actors_with_identity()
    → sets gateway_handles.<domain> = Some(service.clone())
    → lifecycle.rs builds init_gateway::GatewayHandles { <domain>: gateway_handles.<domain> }
    → init_gateway::spawn_gateway() wires it into GatewayServer via with_<domain>()
    → GatewayServer::setup() injects it as app_data
    → route handler prefers it, falls back to local manager only when absent
```

This is the same pattern used for `LedgerService`, `TrustService`, `CommonsHandle`, `GovernanceHandle`,
`NamingService`, `TreasuryHandle`, `EntityHandle`, and (as of PR #1477) `FederationService`.

### Fallback / Standalone Mode

Gateway-local managers (CommonsManager, TrustManager, GovernanceManager, etc.) serve two roles:
1. **Testing**: unit tests create GatewayServer without supervisor-provided services.
2. **Standalone operation**: icn-gateway running without icnd (rare, intentional edge case).

These are **degraded modes**. They must be:
- Clearly logged with `info!("... running standalone (in-memory only)")` or similar.
- Never silently mixed into production daemon deployments.
- Documented in the server setup code at the initialization point.

## Audit Results (2026-03-31)

### Domain Map

| Domain | Canonical Owner | Gateway Local Manager | Fallback Store | Risk | Status |
|--------|----------------|----------------------|----------------|------|--------|
| Federation clearing | `FederationServiceImpl` (sled at `store_path/clearing`) | `FederationManager` (TEMP store) | ephemeral | MEDIUM | **FIXED for reads** (PR #1477) |
| Commons | `CommonsHandle` (sled at `data_dir/commons.sled`) | `CommonsManager` (sled fallback) | `data_dir/commons.sled` | LOW | ✅ Handle always wired in daemon |
| Trust | `TrustService` (in-memory, gossip-synced) | `TrustManager` (in-memory) | in-memory | LOW | ✅ TrustManager delegates to TrustService when present |
| Governance | `GovernanceActor` (sled) | `GovernanceManager` (sled fallback) | `data_dir/gateway_store` | LOW | ✅ Handle always wired in daemon |
| Ledger (treasury) | `LedgerService` (daemon's sled) | `LedgerManager` (own sled) | `data_dir/ledgers/<coop>/` | NOTE | See two-plane note below |
| Naming | `NamingService` (sled) | `LocalNamingService` (sled fallback) | `data_dir/store/naming` | LOW | ✅ Service always wired in daemon |
| Treasury | `TreasuryHandle` (via ledger) | `GatewayTreasuryManager` (in-memory) | in-memory | LOW | ✅ Handle wired in daemon |
| Entity | `EntityHandle` (via icn-entity) | `EntityManager` (in-memory) | in-memory | LOW | ✅ Handle wired in daemon |
| Service discovery | gossip-wired instance | local in-memory | in-memory | NONE | ✅ Gossip-wired always preferred |

### The Two-Ledger Architecture (intentional)

ICN operates two separate accounting planes:

**Member-level ledger** (`LedgerManager`, gateway-owned):
- Manages per-cooperative member-to-member mutual credit (direct transfers, balances).
- Populated by API calls: `POST /ledger/transfer`, `POST /ledger/settle`.
- Persisted at `data_dir/ledgers/<coop_id>/`.
- This IS the source of truth for member balances — it is not a shadow of the daemon's ledger.

**Treasury/kernel ledger** (`LedgerService`, supervisor-owned):
- Manages commons-credit settlement, governance-triggered transfers, clearing settlement entries.
- Populated by governance effects and clearing settlement callbacks.
- Persisted at the daemon's store path.
- Gateway uses this for treasury nonce queries and clearing settlement verification.

These planes serve different stakeholders and are intentionally separate. They are **not** in a
supervisor/fallback relationship — they are parallel accounting systems. This is the intended design.
Future work should define where and how these planes are reconciled (e.g., inter-cooperative settlement
reflecting back to member ledgers).

### The Federation Manager Gap

`FederationManager` in the gateway **always** uses a temporary sled store (`FederationManager::new()`),
regardless of whether a `FederationService` is present. This means:

- **Writes through the gateway API** (agreements, vouches, attestations) are ephemeral — lost on restart.
- **The supervisor's clearing state** is populated by compute-layer clearing callbacks, not gateway API writes.
- `POST /clearing` → gateway's temp FederationManager → ephemeral
- `GET /clearing/{id}/position` → supervisor's FederationService → persistent ✅ (post-fix)

This is the intended architecture for the current phase: federation agreements originate in the compute
layer (when tasks are executed and clearing callbacks fire), not from direct API creation. The gateway
federation creation API endpoints are **testing/demo paths** until the full agreement lifecycle is
implemented.

`FederationManager::new_with_storage()` exists but is not called from `GatewayServer::setup()`. This is
dead code / future intent. If federation agreement lifecycle management becomes a production concern,
these writes should route through `FederationService`, not `FederationManager`.

## Consequences

### Rules Enforced

1. **New supervisor-owned domains** must follow the full wiring chain:
   `GatewayActorHandles` field → `init_gateway::GatewayHandles` field → `GatewayServer` builder method →
   `app_data` injection → route handler prefers service, falls back to local manager.

2. **Fallback/standalone mode must be logged** with explicit `warn!` or `info!` noting it is degraded.

3. **Read endpoints for supervisor-owned domains must prefer the service** over local managers.
   Write endpoints for domains currently served by gateway-local managers are acceptable standalone
   behavior but must be clearly documented as such.

4. **The two-ledger architecture is intentional** and not a bug. Document it; do not conflate the planes.

5. **`FederationManager::new_with_storage()`** should either be wired or removed. It is currently dead code.

### Future Signals

If a future PR adds a supervisor service for a domain that the gateway previously managed locally:
1. Check: is the service threaded through GatewayActorHandles? If not, it will be ignored.
2. Check: does the route handler prefer the service? If not, reads will still use the local manager.
3. Check: are writes also routed through the service? If not, the "fixed" read path will see stale state.

## References

- PR #1474: `feat(federation): settlement execution + correctness fixes`
- PR #1476: `feat(compute): receipt pipe to clearing (federated task accounting)`
- PR #1477: `feat(federation): expose clearing position via service-owned state at gateway layer`
- `crates/icn-gateway/src/server.rs` — GatewayServer setup, all manager initialization
- `crates/icn-core/src/supervisor/actors.rs` — GatewayActorHandles (add new fields here)
- `crates/icn-core/src/supervisor/init_gateway.rs` — GatewayHandles (mirrors GatewayActorHandles)
- `crates/icn-core/src/supervisor/lifecycle.rs` — wiring logic for all supervisor→gateway bridges
