# Testing Documentation

This directory contains testing guides and plans for ICN.

## Verification Pattern

**[verification-pattern.md](verification-pattern.md)** — The canonical 4-layer verification pattern.
Read this first before writing persistence tests for any subsystem.
Defines what each layer proves, how to implement it, and what "verified" means.

## Subsystem Proof Layers

All three subsystems below have all 4 layers complete.

**[governance-proof-layers.md](governance-proof-layers.md)**
- Layer 1: HTTP lifecycle (actix-web integration test, real Ed25519 auth)
- Layer 2: Actor-backed sled write (GovernanceActor + SledGovernanceStateStore)
- Layer 3: Same-runtime close+reopen (GovernanceHandle::shutdown().await)
- Layer 4: Cross-process restart (governance_restart_helper binary)

**[ledger-proof-layers.md](ledger-proof-layers.md)**
- Layer 1: Direct sled write (Ledger::new + append_entry)
- Layer 2: Store-backed sled write (SledEscrowStore)
- Layer 3: Service-layer same-runtime reopen (LedgerServiceImpl)
- Layer 4: Cross-process restart (ledger_restart_helper binary)

**[gossip-proof-layers.md](gossip-proof-layers.md)**
- Layer 1: GossipActor snapshot persistence
- Layer 2: GossipHandle (Arc<RwLock>) snapshot persistence
- Layer 3: Same-runtime handle drop+recreate
- Layer 4: Cross-process restart (gossip_restart_helper binary)
- Note: gossip uses JSON snapshot files (icn-snapshot), not sled

## Reusable Infrastructure

**`crates/icn-testkit/src/subprocess.rs`** — `run_subprocess(binary, args)` helper.
Use this in Layer 4 integration tests to spawn helper binaries and assert success
with useful failure messages. See `verification-pattern.md` for usage example.

## Other Testing Docs

**From root:**
- **TESTING_QUICKSTART.md** - Quick start guide for testing
- **testing-rpc.md** - RPC testing guide
- **INTERNAL_TESTING_PLAN.md** - Internal testing plan
- **BETA_TESTING_GUIDE.md** - Beta testing guide

## Related Documentation

- [Development Documentation](../)
- [Developer Guides](../../guides/developer/)
- [CI Documentation](../../ci/)
