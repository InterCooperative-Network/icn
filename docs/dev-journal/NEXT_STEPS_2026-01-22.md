# Development Plan - 2026-01-22

## Current State Summary

| Metric | Value |
|--------|-------|
| **Last Completed Phase** | Phase 18 (Pre-Pilot Hardening) |
| **Phase 19 Status** | ✅ Appears complete (needs verification) |
| **Open PRs** | 0 |
| **High Priority Issues** | 1 (#769 - RPC Security) |
| **LOC** | ~272K |
| **Tests** | 2,287+ |

### Recent Completions (Last 48h)
- ✅ PR #762: Config module refactoring (gossip, compute, trust)
- ✅ PR #763: Supervisor module refactoring
- ✅ PR #760: CoopManager member operations wiring
- ✅ PR #761: QR code generation for device pairing
- ✅ PR #755: Real TPM sealing/unsealing

---

## Priority 1: Security Hardening (IMMEDIATE)

### #769: RPC Security Enforcement
**Priority**: HIGH | **Effort**: 1-2 days

The RPC layer has security gaps identified during API analysis:

| Gap | Risk | Fix |
|-----|------|-----|
| Optional coop isolation | Information leakage | Mandatory `require_coop_access()` |
| Unsanitized errors | Internal details exposed | Log full, return generic |

**Action Items**:
1. Add `ApiContext` to RPC server with mandatory coop validation
2. Wrap all coop-scoped handlers with isolation check
3. Sanitize error messages before sending to client
4. Add audit logging for blocked cross-coop attempts

---

## Priority 2: Phase 19 Verification & Roadmap Update

### Verify Phase 19 Completion
Phase 19 (Entity & Coop Integration) appears complete but roadmap shows "Planned".

**Verification Checklist**:
- [x] CoopActor spawned in supervisor (`init_coop.rs` exists)
- [x] Entity gossip topic defined (`COOP_UPDATES_TOPIC`)
- [x] SledStore integration (`CoopStore` uses sled)
- [x] Gateway endpoints (PR #760 merged)
- [ ] Multi-node convergence test (needs verification)

**Action**: Run multi-node coop test, then update ROADMAP.md status.

---

## Priority 3: Foundation for API Unification

### #767: Create `icn-api` Shared Service Layer
**Priority**: MEDIUM | **Effort**: 3-5 days

Creates foundation for eliminating RPC/Gateway code duplication:

```
icn/crates/icn-api/
├── src/
│   ├── error.rs      # Unified ApiError enum
│   ├── scopes.rs     # Permission constants
│   ├── context.rs    # Request context (coop, caller, scopes)
│   └── services/     # Shared business logic
```

**Benefits**:
- Single source of truth for business logic
- Consistent error handling
- Enables future transports (GraphQL, gRPC)

---

## Priority 4: Phase 20 - Release Infrastructure

### Parallel Track (can start now)
Phase 20 has no blockers and provides critical CI/CD capabilities:

| Issue | Description | Effort |
|-------|-------------|--------|
| #183 | Binary signing + SBOM | 2d |
| #186 | Benchmark regression CI | 2d |
| #227 | Performance benchmark suite | 3d |
| #228 | CCL parser fuzzing | 2d |
| #319 | Multi-node test harness | 3d |

**Recommendation**: Start #319 (multi-node harness) as it validates Phase 19 completion.

---

## Priority 5: Phase 21 - Network Connectivity

### After Phase 19/20 Verification
Enables ICN to work over the internet (not just LAN):

| Issue | Description | Effort |
|-------|-------------|--------|
| #471 | NAT traversal (STUN/TURN) | 5d |
| #483 | Connection pooling | 3d |
| #472 | Dynamic Bloom filter sizing | 2d |

**Blocker**: Should complete Phase 20 multi-node testing first.

---

## Recommended Execution Order

### Week 1 (Immediate)
```
Day 1-2: #769 - RPC Security Hardening (HIGH PRIORITY)
         - Enforce coop isolation
         - Sanitize error messages

Day 3:   Verify Phase 19 completion
         - Run multi-node coop convergence test
         - Update ROADMAP.md if complete

Day 4-5: #767 - Start icn-api crate foundation
         - Error types
         - Scope constants
         - Basic structure
```

### Week 2
```
Day 1-3: #767 - Complete icn-api foundation
         - ApiContext
         - Validation utilities

Day 4-5: #319 - Multi-node test harness
         - Foundation for Phase 20
         - Validates Phase 19
```

### Week 3+
```
- #768 - Extract ComputeService to shared layer
- #770 - Trust-gated rate limiting in Gateway
- Phase 20 remaining items
- Phase 21 when ready
```

---

## Issue Triage Summary

### High Priority (Do Now)
| Issue | Title |
|-------|-------|
| #769 | security(rpc): Enforce coop isolation and sanitize errors |

### Medium Priority (This Sprint)
| Issue | Title |
|-------|-------|
| #767 | Create icn-api shared service layer |
| #768 | Extract ComputeService to shared layer |
| #770 | Trust-gated rate limiting in Gateway |
| #319 | Multi-node test harness |

### Low Priority (Backlog)
| Issue | Title |
|-------|-------|
| #764 | TrustScore newtype wrapper |
| #765 | Config module split tracking |
| #766 | Large module analysis |

### Deferred (Future Phases)
| Issue | Phase | Title |
|-------|-------|-------|
| #471 | 21 | NAT traversal |
| #267 | 22 | Protocol self-governance |
| #269 | 24 | SDIS ZK voting |
| #268 | 25 | Inter-cooperative economics |

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Security issues fixed | #769 closed |
| Phase 19 verified | ROADMAP.md updated |
| API foundation | icn-api crate compiles |
| Test coverage | Multi-node harness working |

---

*Generated: 2026-01-22*
*Next Review: 2026-01-29*
