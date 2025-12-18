# Priority Gaps to Fix Before Pilot
**Date:** 2025-12-17

## 🔴 CRITICAL (Must fix before pilot)

### 1. Integrate Cooperative Lifecycle into Daemon
**Problem:** `icn-cooperative` crate exists but not spawned in supervisor

**Fix:**
- [ ] Create `CooperativeActor` in `icn-cooperative/src/actor.rs`
- [ ] Spawn in `icn-core/src/supervisor.rs`
- [ ] Wire up to gateway via `CoopManager`
- [ ] Add CLI commands: `icnctl coop create/list/dissolve`
- [ ] Expose full API endpoints in gateway

**Impact:** Right now users can only create "coops" as Gateway metadata. No actual cooperative governance or lifecycle.

### 2. Complete Steward Integration
**Problem:** Steward actor exists but not in default icnd, UI incomplete

**Fix:**
- [ ] Add `--enable-steward` flag to icnd
- [ ] Spawn StewardActor optionally in supervisor
- [ ] Complete steward dashboard UI (VUI registry, stats)
- [ ] Add recovery ceremony UI flow
- [ ] CLI commands: `icnctl steward enroll/recover/verify`

**Impact:** SDIS is half-baked. Users can enroll but can't complete recovery or see steward network.

### 3. End-to-End Testing with Real UI
**Problem:** Tests pass, but no e2e flow verification

**Fix:**
- [ ] Spin up icnd locally
- [ ] Deploy pilot-ui (nginx or just `npx serve`)
- [ ] Walk through EVERY UI flow manually
- [ ] Document working flows vs broken
- [ ] Fix UI bugs (API mismatches, error handling)

**Impact:** Likely many bugs when real users try the UI.

## 🟡 IMPORTANT (Pilot would be better with these)

### 4. Cooperative Formation Ceremony
**Problem:** No multi-stakeholder signatory approval for formation

**Fix:**
- [ ] Add `FormationProposal` workflow
- [ ] Require N founding members to sign
- [ ] Create formation smart contract (CCL)
- [ ] UI for formation wizard (multi-step)

**Impact:** Current coop creation is instant and unilateral. Real coops need consensus.

### 5. Membership Tier Management
**Problem:** All members are equal, no tier progression

**Fix:**
- [ ] Implement tier transitions (Probationary → Full)
- [ ] Add sponsorship requirement (existing member vouches)
- [ ] Voting rights tied to tier
- [ ] UI for membership applications

**Impact:** Real coops have membership levels. Current system is flat.

### 6. Economic Safety Rails
**Problem:** Credit limits are static, no velocity limits

**Fix:**
- [ ] Dynamic credit limits based on trust score + history
- [ ] Transaction velocity limits (max X credits per day)
- [ ] Alert system for unusual patterns
- [ ] Graduated sanctions (warnings → suspension)

**Impact:** System is vulnerable to abuse. One bad actor can drain credits.

### 7. Dispute Resolution Workflow
**Problem:** Disputes can be filed but no resolution process

**Fix:**
- [ ] Mediation request API
- [ ] Arbitrator selection (random from trusted members)
- [ ] Evidence submission
- [ ] Binding resolution that adjusts ledger
- [ ] UI for dispute management

**Impact:** No way to resolve conflicts except off-platform.

## 🟢 NICE-TO-HAVE (Can defer to Phase 2)

### 8. Community Multi-Coop Governance
**Problem:** Communities not integrated

**Fix:** Defer to Phase 2. Focus on single-coop pilot first.

### 9. Advanced Governance (Quadratic, Delegation)
**Problem:** Only simple 1-person-1-vote

**Fix:** Defer to Phase 2. Basic voting is sufficient for pilot.

### 10. Native Mobile Apps
**Problem:** PWA only, not native iOS/Android

**Fix:** Defer to Phase 2. PWA is adequate for pilot.

### 11. Zero-Knowledge Proofs
**Problem:** ZKP crate is empty stub

**Fix:** Defer to Phase 3. Privacy is important but not critical for initial pilot.

## 🎯 RECOMMENDED ACTION PLAN

### Week 1: Integration Sprint
- Integrate cooperative lifecycle into daemon
- Complete steward actor integration
- Add missing CLI commands

### Week 2: E2E Testing & Bug Fixes
- Full manual testing of UI flows
- Fix API/UI mismatches
- Polish error handling and edge cases

### Week 3: Economic Safety
- Implement dynamic credit limits
- Add velocity limits and monitoring
- Create admin dashboard for safety alerts

### Week 4: Dispute Resolution
- Build mediation workflow
- Add arbitrator selection
- Create dispute UI

### Week 5: Formation Ceremonies
- Multi-sig formation process
- Membership tier transitions
- Application approval workflow

### Week 6: Final Testing & Documentation
- End-to-end pilot testing with real users
- User documentation and guides
- Deployment automation

## ✅ WHEN TO LAUNCH PILOT

Launch pilot when:
1. ✅ Cooperative lifecycle fully integrated (CLI + API + UI)
2. ✅ Steward system complete (enrollment → recovery → verification)
3. ✅ All UI flows tested and working
4. ✅ Economic safety rails in place (credit/velocity limits)
5. ✅ Basic dispute resolution working
6. ⚠️ Formation ceremonies (can launch without, but note limitation)
7. ⚠️ Membership tiers (can launch with flat structure initially)

**Minimum viable pilot:** Items 1-5 above (4-6 week timeline)

**Full-featured pilot:** All items (6-8 week timeline)

---

**My honest recommendation:** You have a solid foundation. Focus the next 4-6 weeks on integration and testing, NOT on new features. The code quality is high, but the pieces aren't fully connected.
