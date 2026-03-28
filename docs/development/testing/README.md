# Testing Documentation

This directory contains testing guides and plans for ICN.

## Documents

**Verification patterns:**
- **[governance-proof-layers.md](governance-proof-layers.md)** — Four-layer proof stack for governance (HTTP lifecycle, sled write, same-runtime reopen, cross-process restart). The reference pattern for verifying any ICN subsystem.
- **[ledger-proof-layers.md](ledger-proof-layers.md)** — Ledger proof stack (all 4 layers complete: direct sled write, store-backed write, service-layer reopen, cross-process restart).
- **[gossip-proof-layers.md](gossip-proof-layers.md)** — Gossip proof stack (Layer 1 complete: GossipActor state snapshot persistence via icn-snapshot). Layers 2–4 pending. Note: gossip uses JSON snapshot files, not sled.

**From root:**
- **TESTING_QUICKSTART.md** - Quick start guide for testing
- **testing-rpc.md** - RPC testing guide
- **INTERNAL_TESTING_PLAN.md** - Internal testing plan
- **BETA_TESTING_GUIDE.md** - Beta testing guide

**Existing testing docs:** (see directory listing)

## Related Documentation

- [Development Documentation](../)
- [Developer Guides](../../guides/developer/)
- [CI Documentation](../../ci/)
