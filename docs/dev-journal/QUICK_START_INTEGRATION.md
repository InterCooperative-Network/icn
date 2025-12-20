# Quick Start: CoopActor Integration
**For:** Next development session
**Goal:** Wire CoopActor into supervisor for persistent cooperative state

## 🎯 Immediate Next Steps (Start Here!)

### Step 1: Create Init Module (30 min)
```bash
# Create new file
touch icn/crates/icn-core/src/supervisor/init_coop.rs
```

Copy this template:
```rust
//! Cooperative actor initialization

use anyhow::Result;
use icn_coop::{CoopActor, CoopHandle, CoopStore};
use icn_gossip::GossipHandle;
use icn_store::Store;
use std::sync::Arc;
use tracing::info;

pub struct CoopServices {
    pub coop_handle: CoopHandle,
}

pub async fn init_coop_services(
    store: Arc<dyn Store>,
    gossip_handle: GossipHandle,
) -> Result<CoopServices> {
    info!("Initializing cooperative services");

    let coop_store = Arc::new(CoopStore::new(store)?);
    
    gossip_handle
        .subscribe("coop:updates".to_string(), icn_gossip::AccessControl::Public)
        .await?;

    let tx = CoopActor::spawn(coop_store, Some(gossip_handle));
    let coop_handle = CoopHandle::new(tx);

    info!("✓ Cooperative actor spawned");
    Ok(CoopServices { coop_handle })
}
```

### Step 2: Add to Supervisor (15 min)

1. Add module declaration in `supervisor/mod.rs`:
```rust
pub mod init_coop;
```

2. Add after line 187 (after ledger init):
```rust
// Initialize cooperative services
let coop_services = init_coop::init_coop_services(
    sled_store.clone(),
    gossip_handle.clone(),
)
.await?;
icn_obs::metrics::supervisor::actor_spawned_inc("coop");
let coop_handle = coop_services.coop_handle.clone();
```

### Step 3: Test It (15 min)
```bash
cd icn
cargo build
cargo run --bin icnd

# In another terminal
cargo run --bin icnctl -- status
```

If it compiles and runs, CoopActor is spawned! ✅

---

## 📋 Full Integration Checklist

- [ ] Step 1: Create init_coop.rs (30 min)
- [ ] Step 2: Wire into supervisor (15 min)
- [ ] Step 3: Basic smoke test (15 min)
- [ ] Step 4: Update gateway to use handle (3 hours)
- [ ] Step 5: Add gossip sync handler (2 hours)
- [ ] Step 6: Add icnctl commands (2 hours)
- [ ] Step 7: Integration tests (2 hours)
- [ ] Step 8: Documentation (1 hour)

**Total:** ~11 hours (1.5 days)

---

## 📚 Reference Documents

- **Detailed Plan:** `COOP_INTEGRATION_PLAN.md`
- **Architecture Audit:** `ARCHITECTURE_WIRING_AUDIT.md`
- **Technical Debt:** `TECHNICAL_DEBT_ANALYSIS_2025-12-17.md`
- **Session Notes:** `SESSION_SUMMARY_2025-12-17_ARCHITECTURE.md`

---

## 🐛 Common Issues & Solutions

### Issue: "icn_coop not found"
**Solution:** Add to Cargo.toml dependencies:
```toml
icn-coop = { path = "../icn-coop" }
```

### Issue: "CoopStore::new() error"
**Solution:** CoopStore needs a Sled store, passed from supervisor

### Issue: Actor not receiving messages
**Solution:** Check that CoopHandle is properly cloned and shared

---

## ✅ Success Criteria

You'll know it's working when:
1. `cargo build` succeeds
2. icnd starts without errors
3. Log shows: "✓ Cooperative actor spawned"
4. Metrics show: `actor_spawned{actor="coop"}=1`

---

## 🚀 After Integration

Next priorities:
1. Update gateway CoopManager to use CoopHandle
2. Add icnctl coop commands
3. Test multi-node cooperative synchronization
4. Enable StewardActor with --enable-steward flag
5. Full E2E testing with UI

**Timeline:** Week 1 = CoopActor + Steward, Week 2 = E2E testing

Ready to ship! 🎉
