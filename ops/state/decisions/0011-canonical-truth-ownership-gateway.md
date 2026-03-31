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

### The Federation Manager Write-Path Architecture

`FederationManager` is the gateway's own federation state layer for **API-originated** federation
operations. It wraps `CooperativeRegistry`, `AttestationStore`, and `ClearingManager` from
`icn-federation`, all sharing a single sled store.

**Two origin paths for federation state (intentionally separate):**

| Origin | Path | Store | Owner |
|--------|------|-------|-------|
| Governance effects (CCL execution) | `establish_clearing`, `join_federation`, `vouch_for_cooperative` via `FederationServiceImpl` | `store_path/{federation,clearing,attestations,agreements}` | Supervisor |
| Direct API (gateway endpoints) | `POST /clearing`, `POST /coops`, `POST /attestations`, etc. | `data_dir/federation_store` | Gateway |
| Compute receipts | clearing callbacks via `AgreementManagerHandle` | `store_path/clearing` | Supervisor |

Both supervisor stores are populated at runtime; both persist across restarts. The gateway's store
was originally ephemeral (temp sled) — **this was fixed**: `GatewayServer::setup()` now calls
`FederationManager::new_with_storage(data_dir)` when `data_dir` is available.

**The remaining separation**: the gateway's FederationManager and the supervisor's FederationService
are separate stores. They share domain types but not state. A clearing agreement created via
`POST /clearing` lives only in the gateway's store; `GET /clearing/{id}/position` reads from the
supervisor's service (ADR 0011), and will return 404 for gateway-API-created agreements when
the daemon is connected.

This is acceptable for the current phase: production clearing agreements should originate from
governance execution (which writes to the supervisor's stores). The gateway direct-write API is
the standalone / direct-management path. Users calling `POST /clearing` in daemon mode should
be aware this creates an agreement that the supervisor's scheduler does not manage.

**Future unification path** (not yet implemented): The FederationService trait would need to grow
`get_agreement(id)`, `list_agreements()`, `list_coops()`, `get_vouches()` read methods before write
unification can make the gateway API into a full proxy for the supervisor's state. Alternatively,
the gateway could gossip-sync its store with the supervisor's store at startup.

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

5. **`FederationManager::new_with_storage()`** is now wired in `GatewayServer::setup()`. Gateway API
   federation state persists across restarts when `data_dir` is provided.

### Future Signals

If a future PR adds a supervisor service for a domain that the gateway previously managed locally:
1. Check: is the service threaded through GatewayActorHandles? If not, it will be ignored.
2. Check: does the route handler prefer the service? If not, reads will still use the local manager.
3. Check: are writes also routed through the service? If not, the "fixed" read path will see stale state.

## References

- PR #1474: `feat(federation): settlement execution + correctness fixes`
- PR #1476: `feat(compute): receipt pipe to clearing (federated task accounting)`
- PR #1477: `feat(federation): expose clearing position via service-owned state at gateway layer` (also contains ADR + persistence fix)
- `crates/icn-gateway/src/server.rs` — GatewayServer setup, all manager initialization
- `crates/icn-gateway/src/federation_mgr.rs` — FederationManager (gateway's federation state layer)
- `crates/icn-core/src/supervisor/actors.rs` — GatewayActorHandles (add new fields here)
- `crates/icn-core/src/supervisor/init_gateway.rs` — GatewayHandles (mirrors GatewayActorHandles)
- `crates/icn-core/src/supervisor/lifecycle.rs` — wiring logic for all supervisor→gateway bridges
