# ICN Phase Progress
**Last Updated:** 2026-03-18
**Current Phase:** Phase 0 — Close the Demo

---

### Phase 0: Close the Demo
**Status:** 🚧 In Progress
**Started:** 2026-03-18
**Completed:** —
**Sprint(s):** S16

**Objective:** All 4 demo flows run end-to-end with ExecutionReceiptGate, correct scopes, and proof signing.

**Deliverables:**
- [x] ExecutionReceiptGate (#1310) — governance → execution proof linkage — PR #1327 merged 2026-03-18
- [x] Add treasury/ledger scopes to demo flow auth calls — fixed in lib-demo-ports.sh (settlements:*, treasury:*)
- [x] Deploy proof signing key to K3s pods — init container keystore fix deployed 2026-03-18
- [x] Verify K3s cluster + CI runner operational — VMs restarted, all nodes Ready, cluster healthy
- [x] All 4 demo flows pass — governance 19/19 (demo-governance.py), flows 1-4 all green 2026-03-18
- [ ] Recorded demo for async audiences — content asset (Matt records, not engineering)
- [x] Layer 3 handoff: someone other than Matt can run the demo — demo/RUNBOOK.md K3s section added 2026-03-18

**Blockers:**
- (none — all ops blockers resolved)

**Decisions Made:**
- (2026-03-18) Treasury scopes are already in ALLOWED_SCOPES; demo scripts just need to request them in auth calls. Not an engineering problem.
- (2026-03-18) Mana terminology is deprecated. Fuel is the correct term for compute metering.
- (2026-03-18) t3 (IPv6 bind defaults, #1296) is the sole remaining S14 task but is not demo-critical — parked.
- (2026-03-18) Deployed icn:20260318 image tag — IfNotPresent pull policy requires unique tags per deploy, not :latest, to force pull on K3s nodes.
- (2026-03-18) Init container keystore fix deployed — busybox:1.36 copies /data/.icn/identity.age → /data/identity.age on every pod start. Eliminates manual copy-after-restart toil.
- (2026-03-18) Flow 2 Step 11 (receipts/allocations 400, missing decision_hash) and Flow 3 clearing ID capture are non-blocking bugs — tracked in GitHub issues, not blocking Phase 0.

**Metrics:**
- Tests added: 0 (ops session)
- Lines changed: ~50 (demo scripts, deployment YAML, Dockerfile.fast)
- Kernel infection delta: 0

---

### Phase 1: The Charter Engine
**Status:** ✅ Complete
**Started:** 2026-03-18
**Completed:** 2026-03-18
**Sprint(s):** S17–S18

**Objective:** YAML charter documents produce kernel-enforced constraints. Cooperatives define their own rules.

**Deliverables:**
- [x] `charter_to_constraints()` bridge function — `icn-ccl/src/schema/bridge.rs`
- [x] `CharterContext` runtime bindings (member count, balances, trust scores)
- [x] `CharterPolicyOracle` — new `apps/charter` crate
- [x] Wire charter app into `icnd` daemon startup
- [x] Integration test: YAML → ConstraintSet → kernel enforcement (20/20 passing)
- [x] Worker cooperative charter template — `contracts/templates/worker-coop.yaml`
- [x] Consumer cooperative charter template — `contracts/templates/consumer-coop.yaml`
- [x] Housing cooperative charter template — `contracts/templates/housing-coop.yaml`
- [x] Community organization charter template — `contracts/templates/community-org.yaml`
- [x] Regional federation charter template — `contracts/templates/federation.yaml`
- [x] Charter ratification flow (governance vote triggers charter deployment) — PRs #1336 + #1337
- [x] `icnctl charter validate/inspect/deploy` subcommands
- [x] Demo Flow 1 updated to use real charter document — demo-governance.py Phase 2 now submits Charter payload with CCL YAML

**Blockers:**
- (none blocking — ratification flow and demo update are additive)

**Decisions Made:**
- (2026-03-18) YAML schema system is the v1 charter interface. No custom text parser.
- (2026-03-18) Expression strings (`"0.67 * members"`) parsed by existing `parse_expr()`. No new parser needed.
- (2026-03-18) Start with governance thresholds + credit limits mapping. Expand incrementally.
- (2026-03-18) `community-org` template uses `entity.type: cooperative / subtype: purpose` — `community` is an entity type (for `icn-community`), not a valid cooperative subtype.
- (2026-03-18) Charter ratification flow is a separate PR: governance has no effect execution hook today. `GovernanceProposalClosed` event is logged only — no `deploy_charter()` call exists anywhere. Wiring requires: (a) add `Charter` variant to `ProposalPayload`, (b) listen for `Accepted` outcome in gateway, (c) call `charter_oracle.deploy_charter()` from gateway handler.
- (2026-03-18) Charter ratification uses type-erased hook (`Arc<dyn Fn(String, String) + Send + Sync>`) threaded through `BootstrapHandles → GatewayActorHandles → GatewayHandles → GatewayServer`. Kernel (`icn-core`, `icn-gateway`) never imports `icn-charter-app`. The daemon (`icnd`) builds the concrete closure from `Arc<CharterPolicyOracle>` and injects it at the boundary.

**Metrics:**
- Tests added: 32+ (12 bridge unit, 9 oracle unit, 11 oracle unit, 1 template integration ratchet; icn-charter-app lib = 11 total; icn-ccl integration = 20 total)
- Lines changed: ~1,068 (bridge 350, oracle 200, daemon wiring 50, templates 350, CLI 90, ratification flow 168, demo 30)
- Kernel infection delta: 0 (charter oracle is an app — kernel sees only ConstraintSet; hook is type-erased at boundary)

### Schema → Constraint Mapping Status

| Schema Type | Field | Expression Example | Constraint Key | Status |
|-------------|-------|--------------------|----------------|--------|
| GovernanceSchema | VoteThreshold | `"0.67 * members"` | `custom["min_votes_<name>"]` | ✅ |
| GovernanceSchema | DecisionType.quorum | `"0.25 * members"` | `custom["min_quorum_<name>"]` | ✅ |
| GovernanceSchema | DelegationConfig.transitive | bool | `custom["delegation_transitive"]` | ✅ |
| GovernanceSchema | TermDuration | literal | `custom["term_years"]` | ✅ |
| EconomicsSchema | CreditConfig.limit | `"min(1000, patronage * 0.5)"` | `custom["credit_limit"]` | ✅ |
| EconomicsSchema | MemberEquity.minimum | literal | `custom["equity_min"]` | ✅ |
| EconomicsSchema | SurplusConfig.allocation | `"0.20"` | `custom["surplus_reserves_pct"]` | ✅ |
| AgreementSchema | SettlementConfig.cycle | enum | `custom["settlement_cycle"]` | ✅ |
| AgreementSchema | DisputeResolution.ladder | structured | `custom["dispute_stages"]` | ✅ |

---

### Phase 2: Pilot Launch
**Status:** ⏳ Planned
**Started:** —
**Completed:** —
**Sprint(s):** S19–S20

**Objective:** 3–5 real cooperatives operating on ICN for governance and/or time-credit tracking.

**Deliverables:**
- [ ] Pilot runbook (#1222)
- [ ] One-command deployment script per cooperative
- [ ] Charter customization workflow documented
- [ ] Pilot onboarding guide (non-technical audience)
- [ ] Deploy nodes for 3–5 pilot cooperatives
- [ ] Weekly check-in process established
- [ ] Pilot case study written (for grant/funder audiences)

**Blockers:**
- Requires Phase 1 complete (charter engine must work)
- Requires cooperative partners identified and committed

**Decisions Made:**
- (none yet)

---

### Phase 3: Federation Depth
**Status:** ⏳ Planned
**Sprint(s):** S21–S24

**Objective:** Cross-organizational coordination end-to-end with real agreements, clearing, and trust bridging.

**Deliverables:**
- [ ] Federation Agreement lifecycle (AgreementSchema → live agreement)
- [ ] Cross-org credential recognition
- [ ] Federation clearing end-to-end
- [ ] Dispute resolution flow
- [ ] NAT traversal for WAN federation (#1299)
- [ ] 10+ node scale test
- [ ] Federation dashboard in pilot UI
- [ ] Multi-federation support
- [ ] 3 federation agreement templates

---

### Phase 4: Institution-in-a-Box
**Status:** ⏳ Planned
**Sprint(s):** S25–S28

**Objective:** Non-technical person starts a cooperative using ICN in under 1 hour.

**Deliverables:**
- [ ] `icnctl init-coop` interactive wizard
- [ ] Web-based charter builder (React)
- [ ] One-click Docker deployment
- [ ] Member invitation flow (QR/link)
- [ ] Mobile app (React Native)
- [ ] Offline-first sync
- [ ] Activity dashboard ("what decisions exist, what money moved, who authorized it")

---

### Phase 5: The Commons Layer
**Status:** ⏳ Planned
**Sprint(s):** S29–S36

**Objective:** Cooperatives pool resources and share services. The network becomes self-sustaining.

**Deliverables:**
- [ ] Commons resource contribution accounting (#925)
- [ ] Resource metering
- [ ] Commons credit formula via CCL (#1308)
- [ ] Shared service registry + marketplace
- [ ] WASM app deployment
- [ ] Resource allocation governance
- [ ] Commons dashboard

---

### Phase 6: Civilization Tools
**Status:** ⏳ Horizon

**Objective:** ICN infrastructure replaces coordination functions of state and corporation.

Emerges from Phases 1–5. Municipal governance, cooperative health networks, climate coordination, education cooperatives, mutual aid at scale.

---

## Cross-Cutting Metrics

### Kernel Infection Ratchet

| Date | icn-core governance refs | icn-core ledger refs | icn-core CCL refs | Infected crates |
|------|--------------------------|----------------------|--------------------|-----------------|
| 2026-03-18 (baseline) | 43 | 31 | 32 | 11 |

### Test Count

| Date | Total Tests | Delta |
|------|-------------|-------|
| 2026-03-18 (baseline) | 4,287 | — |

### Codebase Size

| Date | Rust Lines (crates+bins) | App Lines | Delta |
|------|--------------------------|-----------|-------|
| 2026-03-18 (baseline) | ~420,000 | ~7,000 | — |
