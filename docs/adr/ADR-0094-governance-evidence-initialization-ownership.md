---
id: "0094"
title: "Governance evidence initialization is owned by the supervisor, not the gateway"
status: "accepted"
date: "2026-11-28"
deciders: ["Matt Faherty"]
tags: ["governance", "evidence", "supervisor", "gateway", "durability", "topology"]
supersedes: []
superseded_by: []
amends: ["ADR-0011"]
implementation_status: "not yet implemented (stage S1 in PR #2980, unmerged)"
references:
  - "ADR-0011 (amended: §Fallback / Standalone Mode for governance; Audit Results Domain Map, Governance row)"
  - "ADR-0014 (mandates and authority grants written through the receipt backend)"
  - "ADR-0019 (one mandate per proposal)"
  - "ADR-0018 (ADR lifecycle vocabulary)"
  - "PR #2980 (stage S1 implementation; must not merge before this ADR is accepted)"
---

# ADR-0094: Governance evidence initialization is owned by the supervisor, not the gateway

## Status

`accepted` (2026-11-28). Implementation is staged; stage S1 is PR #2980.

## Context

A governance decision's institutional evidence is the durable record of what was decided. It comprises:
- decision receipts;
- ADR-0014 mandates and authority grants;
- institutional effect records;
- dispatch evidence.

That evidence is written through the governance receipt backend. A crash between a close's write-ahead intent and its completion is repaired by **close recovery** (`recover_incomplete_closes`), which replays journaled closes.

Today all of this is initialized by the **HTTP gateway**, an optional API layer:
- the gateway opens `<data_dir>/gateway_store`;
- builds the receipt store;
- installs it on the governance actor;
- installs the dispatch-evidence backend;
- runs close recovery and the evidence backfills.

Consequences observed on main:

1. **A daemon with an identity and the gateway disabled never runs close recovery and never installs a receipt store.** The gateway is the only production caller of recovery. Crash-left close intents are never replayed, and accepted closes write no store-gated evidence. The gateway is disabled by default.
2. **Evidence readiness is keyed to gateway startup.** An HTTP-layer failure is conflated with an evidence failure.
3. **The gateway can silently use temporary storage.** Constructed without an event broadcaster, it drops its data directory and installs a temporary in-memory receipt store on the actor while reporting ready.
4. **Nodes without an identity** run a separate in-memory governance implementation on temporary storage. Nothing survives a restart.
5. **ADR-0011 records governance's fallback store as `data_dir/gateway_store`.** For the identity-less path the store is temporary. For identity nodes, `gateway_store` is the real evidence store, but only when the gateway runs.

The architectural requirement is that institutional evidence persistence and recovery must not depend on optional API exposure or on network broadcasting.

## Decision

1. **Ownership.** For nodes with an identity, the composition root (`icnd`) opens `<data_dir>/gateway_store` exactly once and constructs the single governance receipt store. The supervisor performs governance evidence initialization; the gateway does not. The gateway receives the already-open database and receipt store and never re-initializes evidence.
2. **Order.** Evidence initialization runs after the governance actor and effect subscription exist, and before any ingress (RPC, HTTP) accepts requests:
   1. construct the receipt store (including its index migration);
   2. by-grantee index backfill;
   3. install the receipt store on the actor (this also binds decision commitments);
   4. install the dispatch-evidence backend;
   5. close recovery;
   6. dispatch-evidence backfill;
   7. retention cleanup, only if that backfill succeeded.

   The by-grantee backfill precedes recovery, because recovered lifecycle acceptances read that index.
3. **Readiness.**
   - Governance evidence readiness is determined by the evidence-initialization outcome, not by the gateway. Successful initialization yields Ready even if HTTP later fails to bind.
   - A failure to open or establish the evidence store yields Failed: scheduled closes are held and terminal transitions are refused.
   - "Ready" means close recovery ran. It does not mean every pending journal entry completed. Residual entries and journal-scan failures are surfaced by logs and metrics, and do not block readiness.
4. **No implicit ephemeral fallback.** A gateway given a governance actor but no evidence store refuses to start.
5. **Not provisioned.** "No evidence path" is not a normal state for nodes with an identity. It remains only as a defensive state.
6. **Nodes without an identity** expose no governance authority surface (separate stage, gated on fixing deployment paths that start nodes before creating an identity). An ephemeral per-boot identity is rejected.
7. **Paths and keys are unchanged** (`<data_dir>/gateway_store`). No data migration is required. The store name is kept for compatibility and documented as the governance evidence store.

## Amendments to ADR-0011

This ADR **amends** ADR-0011 for the governance domain only. It does not supersede ADR-0011 as a whole.
- **§"Fallback / Standalone Mode":** for governance, the gateway-local `GovernanceManager` fallback is no longer an acceptable path for authoritative governance. Nodes with an identity always use the governance actor with supervisor-initialized evidence. Nodes without an identity provide no governance authority.
- **§"Audit Results", Domain Map, Governance row:** the fallback column ("`GovernanceManager` (sled fallback)") and store column ("`data_dir/gateway_store`") are replaced by: canonical owner `GovernanceActor` with supervisor-initialized evidence at `data_dir/gateway_store`; no standalone fallback.

The other domains in ADR-0011 are unaffected.

## Consequences

- Evidence and recovery work with the gateway disabled, failed, or absent.
- There is one owner of the evidence database. The receipt-store migration runs once.
- A new binary dependency `icnd → icn-gateway` is added (permitted for binaries by the firewall taxonomy). icn-core gains no new Cargo dependency, but it does carry one additional gateway type through its handle structs (`SharedGatewayStore`, `icn_gateway::` references 30 → 33; no pinned ratchet covers this). This is accepted as part of the existing core→gateway (B11) debt, and ledgered there.
- Existing tests that encode gateway-keyed readiness are rewritten to the new semantics.
- The deployment paths that start a node before creating its identity must be fixed before the identity-less stage ships.

## Staging

| Stage | Scope |
|---|---|
| S1 | Evidence initialization ownership and ordering (this ADR's core) |
| S2 | The actor refuses terminal transitions while evidence is Pending |
| S3 | No governance authority without an identity (gated on deployment-path fixes) |
| S4 | Test-only standalone constructors |

Each stage is independently landable.

## Open questions

- Whether a journal-scan failure should eventually be treated as Failed rather than Ready with a surfaced residual. It is kept as Ready for S1.
- Whether backfill failures should eventually block readiness.
