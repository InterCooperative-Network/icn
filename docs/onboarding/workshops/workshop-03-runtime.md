# Workshop 3: Startup Path Walkthrough

## Goal
Trace the daemon startup path and identify actor initialization order.

## Steps
1. Open `icn/bins/icnd/src/main.rs`
2. Identify config loading and validation flow
3. Open `icn/crates/icn-core/src/runtime.rs`
4. Open `icn/crates/icn-core/src/supervisor/mod.rs`
5. List the order of subsystem initialization

## Checkpoints
- You can explain where the keystore is opened
- You can describe the order of trust, gossip, ledger, and network init
