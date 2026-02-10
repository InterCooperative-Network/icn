# Development Journal: Phase 0 Bootstrap

**Date:** 2025-11-10
**Phase:** 0 - Scaffold
**Status:** Complete ✓

---

## Summary

Successfully bootstrapped ICNd with complete Tokio-based actor runtime. All core infrastructure is in place and functional.

---

## What We Built

### Workspace Structure
- **11 crates:** core, identity, trust, net, gossip, ledger, ccl, store, rpc, obs, testkit
- **2 binaries:** icnd (daemon), icnctl (CLI)
- Clean dependency graph with workspace-level version management

### Core Runtime ([icn-core](../../icn/crates/icn-core/))
- Tokio multi-threaded runtime
- Supervisor pattern for actor lifecycle
- Config management (TOML + env overrides)
- Graceful shutdown with broadcast channel
- Ctrl+C handling

### Identity Layer ([icn-identity](../../icn/crates/icn-identity/))
- Ed25519 key generation
- DID format: `did:icn:<base58btc-pubkey>`
- Zeroized memory for secret keys (secure drop)
- Sign/verify primitives
- **Working:** `icnctl id generate` produces valid DIDs

### Storage ([icn-store](../../icn/crates/icn-store/))
- Pluggable trait for backend independence
- Sled implementation (embedded, pure Rust)
- Temporary database for testing

### Observability ([icn-obs](../../icn/crates/icn-obs/))
- `tracing` subscriber initialized
- Structured logging (env filter)
- Metrics exporter stub (Prometheus)

### Binaries
- **icnd:** Boots, runs supervisor, waits for shutdown
- **icnctl:** Subcommands for `status`, `id`, `peers`

---

## Technical Decisions Made

### 1. Runtime: Tokio
**Why:** Ecosystem maturity, best-in-class async primitives, wide library support (tonic, quinn, etc.)

**Alternatives considered:** async-std (cleaner but smaller ecosystem)

### 2. Identity: Ed25519
**Why:** Standard, audited, fast, 32-byte keys

**Future:** X25519 for ECDH, multi-key support

### 3. DID Format: `did:icn:<base58btc-pubkey>`
**Why:** Self-certifying, no registry, verifiable by anyone

**Tradeoff:** Key compromise = identity loss (mitigated by rotation protocol in Phase 1)

### 4. Storage: Sled
**Why:** Pure Rust, embedded, transactional, good for prototyping

**Future:** RocksDB for production (more mature, faster)

### 5. Zeroized Keys
**Implementation:** Store key bytes in `Zeroizing<[u8; 32]>` instead of `SigningKey` directly
**Why:** `SigningKey` doesn't implement `Zeroize` trait; manual byte management ensures secure drop

---

## Code Structure

```
icn/
├── Cargo.toml (workspace)
├── bins/
│   ├── icnd/
│   │   └── src/main.rs (daemon entry point)
│   └── icnctl/
│       └── src/main.rs (CLI tool)
└── crates/
    ├── icn-core/
    │   ├── config.rs (TOML config)
    │   ├── runtime.rs (Tokio runtime + shutdown)
    │   └── supervisor.rs (actor supervisor)
    ├── icn-identity/
    │   └── lib.rs (DID, KeyPair, sign/verify)
    ├── icn-store/
    │   └── lib.rs (trait + Sled impl)
    ├── icn-obs/
    │   └── lib.rs (tracing init)
    └── [trust, net, gossip, ledger, ccl, rpc, testkit]
        └── lib.rs (stubs)
```

---

## Tests Performed

### Build
```bash
cargo build --release
# Success: 32.47s
```

### Run icnd
```bash
timeout 3 ./target/release/icnd
# Output:
# INFO icnd: ICNd starting
# INFO icnd: Data directory: "/home/matt/.local/share/icn"
# INFO icnd: Log level: info
# INFO icn_core::runtime: ICNd runtime starting
# INFO icn_core::supervisor: Supervisor starting
```

### Generate DID
```bash
./target/release/icnctl id generate
# Output:
# Generating new identity...
# DID: did:icn:z2emnkKDB1nvAANKNipKCMygBgzA2qdmJQhNexKKLXzQE
# Note: Key storage not yet implemented
```

### CLI Help
```bash
./target/release/icnctl --help
# Shows all subcommands (status, id, peers)
```

---

## Git Commits

```
4c3afa5 Fix .gitignore to properly ignore target directories
4a72665 Remove target directory from git
aa85201 Initial commit: ICNd Phase 0 scaffold
```

**Repository:** `github.com:InterCooperative-Network/icn.git`

---

## Issues Encountered

### 1. Zeroizing SigningKey
**Problem:** `Zeroizing<SigningKey>` failed to compile - `SigningKey` doesn't implement `Zeroize` trait

**Solution:** Store raw bytes in `Zeroizing<[u8; 32]>` and reconstruct `SigningKey` on each use

**Code:**
```rust
pub struct KeyPair {
    secret_bytes: Zeroizing<[u8; 32]>,  // Secure
    verifying_key: VerifyingKey,
    did: Did,
}

pub fn sign(&self, message: &[u8]) -> Signature {
    let signing_key = SigningKey::from_bytes(&self.secret_bytes);
    signing_key.sign(message)
}
```

### 2. Target Directory in Git
**Problem:** Accidentally committed 3941 build artifact files

**Solution:** Fixed `.gitignore` from `/target/` to `target/` (match at any level), removed from git

---

## Next Phase: Phase 1 - Identity & Trust

### Goals
1. **Persistent key storage** with Age encryption
2. **Key rotation** protocol with signed transitions
3. **Trust graph** storage backend
4. **DID import/export** for backup/restore

### Open Questions (to be resolved in architecture doc)
- Multi-device identity: delegate keys vs shared keys?
- Trust computation: local PageRank-like vs simpler heuristic?
- Bootstrap trust: invite codes, proof-of-work, manual vouching?

---

## Metrics

- **Lines of code:** ~800 (excluding tests, stubs)
- **Dependencies:** 323 crates
- **Build time:** 32.47s (release)
- **Binary size:** icnd ~12MB, icnctl ~11MB (release)

---

## Architecture Insights

### Actor Model Works Well
The supervisor + broadcast shutdown pattern is clean:
```rust
let (shutdown_tx, _) = broadcast::channel(1);
// Each actor subscribes
let mut shutdown_rx = runtime.shutdown_rx();
// Select on work vs shutdown
select! {
    _ = do_work() => {},
    _ = shutdown_rx.recv() => {},
}
```

### Config Hot-Reloading Ready
Config is loaded once but structure supports SIGHUP reload:
```rust
// Future:
signal::unix::signal(SignalKind::hangup())?;
// Reload config, notify actors
```

### Storage Trait Flexibility
Easy to swap Sled for RocksDB or SQLite:
```rust
let store: Box<dyn Store> = if use_rocksdb {
    Box::new(RocksDbStore::open(path)?)
} else {
    Box::new(SledStore::open(path)?)
};
```

---

## Documentation Created

- [README.md](../../README.md) - Project overview
- [ARCHITECTURE.md](../ARCHITECTURE.md) - Comprehensive design doc
- This journal entry

---

## Team Notes

**For future contributors:**
- The scaffold is intentionally minimal - stubs over speculation
- All architectural decisions documented in ARCHITECTURE.md
- Build should be ~30s on modern hardware
- `cargo clippy` clean, a few warnings expected in stubs

**For security review:**
- Key material is zeroized on drop
- No network code active yet (safe to run)
- No persistent state yet (daemon is stateless)

---

## Reflections

**What went well:**
- Clean separation of concerns (crates)
- Tokio actor model feels natural
- DID generation works first try

**What to improve:**
- More unit tests (only identity has tests)
- CI/CD not set up yet
- No benchmarks yet

**Surprises:**
- Zeroizing SigningKey required manual byte management
- Git included target/ despite .gitignore (needed `target/` not `/target/`)

---

## References

- Original architecture chat: ChatGPT transcript (2025-11-10)
- Tokio docs: https://tokio.rs/
- Ed25519 Dalek: https://docs.rs/ed25519-dalek/
- Sled: https://docs.rs/sled/

---

**Next journal entry:** Phase 1 kickoff (TBD)
