# CoopActor Integration Plan
**Date:** 2025-12-17
**Priority:** HIGH
**Estimated Effort:** 1-2 days

## Current State

### ✅ What Exists
1. **icn-coop crate** - Full actor implementation
   - `CoopActor` with message-based API
   - `CoopHandle` for async operations
   - `CoopStore` with persistent storage (Sled)
   - `LifecycleManager` for state transitions
   - `MembershipManager` for member operations

2. **Gateway CoopManager** - In-memory workaround
   - HashMap-based storage (lost on restart)
   - REST API endpoints working
   - No gossip synchronization

### ❌ What's Missing
1. CoopActor NOT spawned in supervisor
2. No integration with gossip for distributed state
3. No persistent storage connection
4. Gateway still using in-memory CoopManager

---

## Integration Steps

### Step 1: Create Supervisor Init Module (2 hours)

**File:** `icn/crates/icn-core/src/supervisor/init_coop.rs`

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
    pub coop_store: Arc<CoopStore>,
}

pub async fn init_coop_services(
    store: Arc<dyn Store>,
    gossip_handle: GossipHandle,
) -> Result<CoopServices> {
    info!("Initializing cooperative services");

    // Create cooperative store
    let coop_store = Arc::new(CoopStore::new(store)?);

    // Subscribe to gossip topic for distributed coop updates
    gossip_handle
        .subscribe(
            "coop:updates".to_string(),
            icn_gossip::AccessControl::Public,
        )
        .await?;

    // Spawn CoopActor
    let tx = CoopActor::spawn(coop_store.clone(), Some(gossip_handle));
    let coop_handle = CoopHandle::new(tx);

    info!("✓ Cooperative actor spawned");

    Ok(CoopServices {
        coop_handle,
        coop_store,
    })
}
```

**Add to supervisor mod.rs:**
```rust
pub mod init_coop;
```

---

### Step 2: Spawn CoopActor in Supervisor (1 hour)

**File:** `icn/crates/icn-core/src/supervisor/mod.rs`

**Add after ledger initialization (around line 188):**

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

**Wire to gateway:**
```rust
// Later in the code where gateway is initialized
// Pass coop_handle to gateway app_data
```

---

### Step 3: Update Gateway to Use CoopHandle (3 hours)

**File:** `icn/crates/icn-gateway/src/coop.rs`

Option A: **Replace CoopManager** (breaking change)
```rust
// Remove: pub struct CoopManager with HashMap
// Add: Wrapper around CoopHandle

use icn_coop::CoopHandle;

#[derive(Clone)]
pub struct CoopManager {
    handle: CoopHandle,
}

impl CoopManager {
    pub fn new(handle: CoopHandle) -> Self {
        Self { handle }
    }

    pub async fn create_coop(
        &self,
        id: String,
        name: String,
        steward: Did,
        _timestamp: u64,
    ) -> Result<()> {
        self.handle
            .create_cooperative(name, icn_coop::CoopType::Worker, steward)
            .await
            .map_err(|e| GatewayError::InternalError(e.to_string()))?;
        Ok(())
    }

    pub async fn get_coop(&self, id: &str) -> Result<Coop> {
        let coop = self.handle
            .get_cooperative(id.to_string())
            .await
            .map_err(|e| GatewayError::NotFound(e.to_string()))?;
        
        // Convert icn_coop::Cooperative to gateway::Coop
        Ok(convert_coop(coop))
    }

    // ... implement other methods
}
```

Option B: **Dual Mode** (migration path)
```rust
pub struct CoopManager {
    handle: Option<CoopHandle>,
    fallback: Arc<RwLock<HashMap<CoopId, Coop>>>,
}

impl CoopManager {
    pub fn with_handle(handle: CoopHandle) -> Self {
        Self {
            handle: Some(handle),
            fallback: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_coop(...) -> Result<()> {
        if let Some(handle) = &self.handle {
            // Use actor
            handle.create_cooperative(...).await?;
        } else {
            // Use fallback
            let mut coops = self.fallback.write().unwrap();
            coops.insert(...);
        }
        Ok(())
    }
}
```

**Recommended:** Option A (clean break, simpler)

---

### Step 4: Update Gateway Initialization (1 hour)

**File:** `icn/crates/icn-gateway/src/lib.rs` or `bins/icnd/src/main.rs`

```rust
// When starting gateway, pass CoopHandle from supervisor

use icn_coop::CoopHandle;

pub struct GatewayDeps {
    pub ledger_handle: LedgerHandle,
    pub governance_handle: GovernanceHandle,
    pub coop_handle: CoopHandle,  // NEW
    pub compute_handle: Option<ComputeHandle>,
    pub event_broadcaster: Option<Arc<EventBroadcaster>>,
}

// In app_data setup:
let coop_mgr = CoopManager::new(deps.coop_handle);
app_data = app_data.app_data(web::Data::new(Arc::new(coop_mgr)));
```

---

### Step 5: Add Gossip Notification Handler (2 hours)

**File:** `icn/crates/icn-coop/src/actor.rs`

```rust
// Add to CoopActor
pub fn set_gossip_callback(&mut self, callback: GossipCallback) {
    self.gossip_callback = Some(callback);
}

// In supervisor, after spawning:
gossip_handle
    .on_entry("coop:updates", move |entry| {
        let coop_handle = coop_handle.clone();
        tokio::spawn(async move {
            // Deserialize coop update from entry.data
            // Apply to local store via coop_handle
        });
    })
    .await?;
```

This enables distributed coop state synchronization.

---

### Step 6: Update icnctl CLI (2 hours)

**File:** `bins/icnctl/src/main.rs`

```rust
// Add subcommand enum
enum CoopCommand {
    Create { name: String, type: String },
    List,
    Get { id: String },
    Activate { id: String, charter_hash: String },
    AddMember { coop_id: String, did: String, role: String },
    ListMembers { coop_id: String },
}

// Add to main match
Commands::Coop(cmd) => {
    match cmd {
        CoopCommand::Create { name, type } => {
            // Call RPC or HTTP API to create coop
        }
        CoopCommand::List => {
            // List all coops
        }
        // ... implement other commands
    }
}
```

---

## Testing Plan

### Unit Tests
1. ✅ CoopActor message handling (already exists in icn-coop)
2. ✅ CoopStore persistence (already exists)

### Integration Tests
Create: `icn/crates/icn-core/tests/coop_integration.rs`

```rust
#[tokio::test]
async fn test_coop_actor_in_supervisor() {
    // 1. Spawn supervisor with CoopActor
    // 2. Create cooperative via handle
    // 3. Verify persistence across restart
    // 4. Test gossip synchronization between two nodes
}

#[tokio::test]
async fn test_gateway_coop_api_with_actor() {
    // 1. Start gateway with CoopHandle
    // 2. POST /coops - create cooperative
    // 3. GET /coops/:id - retrieve cooperative
    // 4. POST /coops/:id/members - add member
    // 5. Verify state persisted in CoopStore
}
```

### Manual E2E Testing
1. Start icnd with CoopActor
2. Start gateway
3. Use pilot-ui to create cooperative
4. Verify in icnctl: `icnctl coop list`
5. Restart icnd
6. Verify cooperative still exists (persistence)

---

## Migration Strategy

### Phase 1: Dual Mode (Week 1)
- Implement Option B (dual mode CoopManager)
- CoopActor spawned but optional
- Gateway can work with or without actor
- Gradual rollout, low risk

### Phase 2: Actor-Only (Week 2)
- Switch to Option A (handle-only)
- Remove in-memory HashMap fallback
- All operations go through CoopActor
- Full persistence and gossip sync

### Phase 3: CLI Integration (Week 2)
- Add icnctl coop commands
- Update documentation
- User testing

---

## Risks & Mitigation

### Risk 1: API Compatibility
**Problem:** icn-coop types may not match gateway types exactly

**Mitigation:**
- Create conversion functions between types
- Update gateway types to match icn-coop if needed
- Version API responses properly

### Risk 2: Performance
**Problem:** Message passing adds latency vs direct HashMap access

**Mitigation:**
- CoopActor operations are async (non-blocking)
- Channel buffer size = 100 (configurable)
- Profile in testing, optimize if needed

### Risk 3: State Sync Complexity
**Problem:** Gossip-based state sync may have conflicts

**Mitigation:**
- Use timestamp-based last-write-wins for simple fields
- Use CRDT for complex state (future enhancement)
- Quarantine conflicting updates for manual resolution

---

## Success Criteria

- [ ] CoopActor spawned in supervisor
- [ ] Gateway uses CoopHandle instead of HashMap
- [ ] Coops persist across icnd restarts
- [ ] Multi-node coop creation syncs via gossip
- [ ] icnctl coop commands work
- [ ] All gateway coop tests pass
- [ ] No regressions in existing functionality
- [ ] Documentation updated

---

## Timeline

**Total Estimate:** 10-12 hours (1.5 days focused work)

| Task | Hours |
|------|-------|
| Step 1: Init module | 2 |
| Step 2: Supervisor integration | 1 |
| Step 3: Gateway refactor | 3 |
| Step 4: Gateway init | 1 |
| Step 5: Gossip handler | 2 |
| Step 6: CLI commands | 2 |
| Testing | 2 |
| **Total** | **13** |

**Buffer:** +3 hours for debugging and edge cases
**Final Estimate:** 16 hours (~2 days)

---

## Next Steps

1. **Immediate:** Create init_coop.rs module
2. **Day 1 Morning:** Integrate into supervisor, spawn actor
3. **Day 1 Afternoon:** Refactor gateway CoopManager
4. **Day 2 Morning:** Add gossip sync, test multi-node
5. **Day 2 Afternoon:** Add CLI commands, E2E testing

