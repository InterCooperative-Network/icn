# ICN Development Plan 2026

**Created**: 2026-01-08
**Target**: Pilot deployment with a real cooperative
**Author**: Development planning based on comprehensive code review

---

## Revised Assessment

The code review identified "4,618 unwraps" as a critical issue. **This was misleading.**

Actual breakdown:
| Location | Count | % |
|----------|-------|---|
| Test code | 4,323 | 97.6% |
| Production code | 107 | 2.4% |

**The codebase is in much better shape than initially assessed.**

### Real Issues (Corrected)

| Issue | Severity | Blocking Pilot? |
|-------|----------|-----------------|
| 107 production unwraps | LOW | No - mostly infallible ops |
| `block_in_place` in trust_mgr | LOW | No - correct Tokio pattern |
| Low test coverage (icn-coop, icn-core) | MEDIUM | No - but risky to change |
| No NAT traversal | HIGH | **Yes** - LAN only currently |
| No backup validation | HIGH | **Yes** - data loss risk |
| Empty icn-testkit | LOW | No - nice to have |
| 6 ignored integration tests | MEDIUM | No - known async issue |

### Actual Pilot Blockers

1. **Network**: ICN only works on LAN (mDNS). Need NAT traversal for internet.
2. **Operations**: No validated backup/restore. Unacceptable for real data.
3. **Documentation**: No runbooks for cooperative administrators.
4. **Onboarding**: No setup guide for new cooperatives.

---

## Goal

Deploy ICN with a **real cooperative** for a 3-month pilot, with:
- Mutual credit ledger tracking real transactions
- Governance proposals for real decisions
- Trust relationships between real members
- Ability to operate over the internet (not just LAN)

---

## Critical Path

```
M1: Operations Foundation (3 weeks)
         ↓
M2: Network Connectivity (4 weeks)
         ↓
M3: Documentation & Onboarding (2 weeks)
         ↓
M4: Pilot Deployment (ongoing)
```

**Total to pilot-ready: ~9 weeks**

---

## Milestones

### Milestone 1: Operations Foundation
**Duration**: 3 weeks
**Goal**: Production-grade operations without data loss risk

The current deployment runs but lacks operational safety nets. Before adding users with real data, we need backup validation and basic monitoring.

#### Tasks
- [ ] Backup validation tests (#224) - 3 days
  - Automated backup/restore cycle in CI
  - Verify ledger integrity after restore
  - Test keystore recovery
- [ ] Production runbooks (#230) - 4 days
  - Startup/shutdown procedures
  - Backup/restore procedures
  - Common troubleshooting
  - Emergency procedures
- [ ] Service Level Objectives (#220) - 2 days
  - Define availability targets
  - Define data durability targets
  - Alert thresholds
- [ ] Secrets rotation runbook (#189) - 1 day
  - Document key rotation process
  - Test rotation procedure

#### Done When
- [ ] CI runs backup/restore validation on every merge
- [ ] Runbooks exist for all common operations
- [ ] SLOs documented with corresponding alerts
- [ ] Successfully restored a backup on a fresh node

---

### Milestone 2: Network Connectivity
**Duration**: 4 weeks
**Goal**: ICN nodes can connect over the internet, not just LAN

Currently ICN uses mDNS for discovery, which only works on local networks. For a pilot with members in different locations, we need internet connectivity.

#### Tasks
- [ ] NAT traversal with STUN/TURN (#471) - 2 weeks
  - Integrate STUN for hole-punching
  - Add TURN relay fallback
  - Test behind various NAT types
- [ ] Bootstrap peer support - 3 days
  - Already partially implemented (supervisor parses `icn://` URLs)
  - Add DNS resolution for hostnames
  - Document bootstrap configuration
- [ ] Connection resilience - 3 days
  - Reconnection on disconnect
  - Handle network changes gracefully
- [ ] Dynamic Bloom filter sizing (#472) - 2 days
  - Scale with network size
  - Prevent saturation at scale

#### Done When
- [ ] Two nodes behind different NATs can connect
- [ ] Node survives network disconnection and reconnects
- [ ] Bootstrap peers work with DNS hostnames
- [ ] Tested with 10+ nodes across different networks

---

### Milestone 3: Documentation & Onboarding
**Duration**: 2 weeks
**Goal**: A cooperative can set up and use ICN without developer assistance

#### Tasks
- [ ] Cooperative setup guide (#229) - 3 days
  - Node installation
  - Identity creation
  - Connecting to network
  - Basic operations
- [ ] API versioning strategy (#497) - 1 day
  - Document current API version
  - Stability guarantees
- [ ] User-facing feature documentation - 3 days
  - Ledger operations
  - Governance proposals
  - Trust management
- [ ] Pilot onboarding materials - 2 days
  - Quickstart for pilot participants
  - FAQ
  - Support channels

#### Done When
- [ ] Non-developer can follow guide to join network
- [ ] All user-facing APIs documented
- [ ] Pilot participants have onboarding materials
- [ ] Support process defined

---

### Milestone 4: Pilot Deployment
**Duration**: Ongoing (3+ months)
**Goal**: Real cooperative using ICN for real transactions

#### Tasks
- [ ] Select pilot cooperative (#5) - 1 week
  - Identify candidate (worker coop, food coop, etc.)
  - Assess technical capacity
  - Define pilot scope
- [ ] Initial deployment - 1 week
  - Set up infrastructure
  - Onboard initial members
  - Verify connectivity
- [ ] Monitoring period - 2 weeks
  - Daily check-ins initially
  - Address issues as they arise
  - Gather feedback
- [ ] Stabilization - ongoing
  - Fix bugs discovered in production
  - Iterate on UX based on feedback

#### Done When
- [ ] Cooperative using ICN for real transactions
- [ ] 3 months of operation without critical failures
- [ ] Feedback collected and documented
- [ ] Decision made on broader rollout

---

## Parallel Work

These can happen alongside the critical path by anyone with spare cycles:

### Low-Risk Improvements
- [ ] Performance benchmark suite (#227) - establishes baseline
- [ ] Enhanced Grafana dashboards (#331) - better observability
- [ ] CLI improvements (#176-179) - developer experience

### Test Infrastructure
- [ ] Multi-node test harness (#319) - enables better integration tests
- [ ] Fuzz testing for CCL parser (#228) - security hardening
- [ ] Enable ignored integration tests - requires async refactoring

### Technical Debt
- [ ] Audit 107 production unwraps - most are likely fine
- [ ] Split config.rs into modules (#158) - code organization
- [ ] Increase icn-coop test coverage - safety for future changes

### Security (Post-Pilot)
- [ ] Sybil resistance (#470) - important but not pilot-blocking
- [ ] Real ZKP circuits (#196-199) - current simulation is documented
- [ ] HSM/TPM keystore (#481) - enterprise feature

---

## Risks

### 1. NAT Traversal Complexity
**Risk**: STUN/TURN integration takes longer than expected
**Mitigation**:
- Start with well-tested libraries (libp2p has STUN)
- Fallback: pilot with VPN tunnel between locations
**Impact if realized**: +2 weeks

### 2. Pilot Cooperative Availability
**Risk**: Can't find a cooperative willing to pilot
**Mitigation**:
- Start outreach during M1
- Have backup candidates
- Consider internal/simulated pilot first
**Impact if realized**: Delay pilot start

### 3. Unknown Production Issues
**Risk**: Real usage reveals problems not caught in testing
**Mitigation**:
- Conservative SLOs initially
- Close monitoring first 2 weeks
- Fast response capability
**Impact if realized**: Extended stabilization

### 4. Single Developer Bottleneck
**Risk**: Illness, burnout, other commitments
**Mitigation**:
- Document everything
- Keep milestones achievable
- Build in slack time
**Impact if realized**: Schedule slip

---

## Timeline

| Week | Milestone | Key Deliverable |
|------|-----------|-----------------|
| 1-3 | M1: Operations | Backup validation in CI |
| 4-7 | M2: Network | NAT traversal working |
| 8-9 | M3: Docs | Setup guide complete |
| 10+ | M4: Pilot | Cooperative onboarded |

**Target pilot start**: ~Week 10 (late March 2026)

---

## What's NOT in This Plan

Explicitly deferred to post-pilot:

1. **Mobile SDK** (#213-215) - web/CLI sufficient for pilot
2. **Federation** (#270, #268) - single-network pilot first
3. **Multi-region deployment** (#225) - overkill for pilot scale
4. **HSM/TPM** (#481) - enterprise security feature
5. **Full ZKP implementation** (#196-199) - simulated is documented
6. **GitOps/ArgoCD** (#191) - manual deployment fine for pilot
7. **Blue-green deployments** (#193) - downtime acceptable for pilot

These become relevant for production scale, not pilot.

---

## Success Criteria

**Pilot is successful if:**

1. ✅ Cooperative completes real transactions on ICN ledger
2. ✅ Governance proposals created and resolved
3. ✅ No data loss incidents
4. ✅ Members can operate with minimal support after onboarding
5. ✅ 3 months operation with >95% uptime
6. ✅ Clear feedback collected for next iteration

**Pilot teaches us:**
- What features cooperatives actually need
- What UX issues matter in practice
- What operational burden looks like
- Whether the architecture scales

---

## Questions Answered

### 1. What's the minimum viable state for pilot?
- Working backup/restore (M1)
- Internet connectivity (M2)
- Basic documentation (M3)
- One willing cooperative (M4)

### 2. How do you phase the unwrap cleanup?
**Don't.** Only 107 production unwraps exist, mostly in infallible operations. Not worth dedicated effort. Fix any that cause actual panics.

### 3. Is the blocking RwLock issue actually causing problems?
**No.** The code uses `block_in_place` which is the correct Tokio pattern. It's documented as "deprecated for async contexts" but isn't breaking anything. Low priority.

### 4. What security gaps are pilot-blocking vs. nice-to-have?
- **Pilot-blocking**: None (three-layer security is solid)
- **Nice-to-have**: Sybil resistance, HSM, real ZKP
- **Current state**: Sufficient for trusted pilot participants

### 5. How many milestones to pilot?
**4 milestones**, ~9 weeks of work. Could compress to 6-7 weeks if aggressive.

---

## Next Actions

1. [ ] Create GitHub issues for M1 tasks if not exists
2. [ ] Start backup validation work immediately
3. [ ] Begin pilot cooperative outreach in parallel
4. [ ] Update ROADMAP.md to align with this plan

---

*This plan prioritizes shipping a working pilot over perfection. Many "issues" from the code review are actually fine for pilot scale. We can iterate based on real feedback.*
