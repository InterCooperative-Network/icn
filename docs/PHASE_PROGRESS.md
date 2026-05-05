# ICN Phase Progress
**Last Updated:** 2026-05-05
**Current Phase:** Phase 2 — Pilot Launch. NYCN is the intended first cooperative partner (active partnership track, not yet a formally committed pilot); the next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers. Subsequent gates: pilot formalization, then first operator rehearsal. The exact organizer/operator gate is defined in [NYCN Phase 2 Pilot Rehearsal Gate](strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md). Institutional-operability infrastructure, the action-card runtime, the action-item completion-receipt retrieval endpoint, and the NYCN drive-ingest operator ladder are all in place; all currently emitted source paths are proof-bearing both locally and on K3s. May-cycle repo-governance, licensing, RFC, repo-record, service-hosting, dependency/CI maintenance, bootstrap, and state-sync docs have landed, plus the May-5 institutional-process-substrate framing sequence (rehearsal evidence export schema, due-diligence checklist, schema-id audit, accessibility gate, preview/review read-model contract, `idea-0019` framing brief, and read-model fixture-walk dogfood slice). None of these mark Phase 2 complete or imply production readiness, live federation integration, implemented service hosting, K3s/DNS/GitHub/Forgejo mutation, NYCN private-data handling, or resolved licensing.

<!-- [sync edit] 2026-05-05 (post-#1734/#1739/#1741/#1743/#1745/#1747/#1749, with open #1748):
     Truth-sync for the May-5 institutional-process-substrate sequence. Doc/control-plane and idea-refinery only; no runtime, no schema, no contract URN beyond `urn:icn:contract:preview-review:v1` (#1745), no implementation issue.
     Phase 2 deliverables list extended to record:
       - rehearsal evidence export schema landed under `urn:icn:contract:rehearsal-evidence-export:v1` (#1734);
       - architecture due-diligence checklist landed at `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` (#1739);
       - contract schema-identifier audit landed at `docs/contracts/schema-id-audit.md` (#1741);
       - organizer/member accessibility gate definition landed at `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` (#1743);
       - preview/review read-model contract `urn:icn:contract:preview-review:v1` landed under `docs/contracts/preview-review.md` (#1745);
       - `idea-0019` Institutional Process Substrate framing brief landed at `ops/ideas/framing/institutional-process-substrate.md` (#1747);
       - read-model fixture-walk dogfood slice for `idea-0019` landed at `ops/ideas/dogfood/institutional-process-substrate-mvp.md` (#1749), plus the new `ops/ideas/README.md` § "Dogfood slice variants" convention.
     Open coordination/control milestone issue #1748 (`milestone(process): define Institutional Process Substrate`) tracks spine composition under `epic:arch-invariants` + `type:spec`. A read-model fixture walk does NOT satisfy receipt-backed promotion thresholds; receipt-backed promotion of `idea-0019` to RFC still requires a runtime dogfood slice that emits at least one `ProcessTransitionReceipt` class under `ADR-0026`, a real visibility/privacy-boundary run, a real accessibility-gate `ProcessGateResult`, and at least one of framing-brief Q1/Q3/Q4 resolved or explicitly deferred.
     Phase 2 status remains ⏳ (still partner-bound). The next concrete human gate is unchanged: organizer presentation -> pilot formalization -> first operator rehearsal per `docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md`. Next pre-RFC architecture move (not started in this sync): Democratic Authority Primitives. -->

<!-- [sync edit] 2026-05-02 (post-#1695/#1696/#1697/#1698/#1699/#1700/#1701):
     Follow-up May-cycle queue is now merged on main: Dependabot
     Actions major-version bumps (#1695-#1698), wasmtime security bump
     (#1699), unified dev-environment bootstrap (#1700), and prior state
     sync (#1701). Open PR queue is empty at this sync. Phase 2 remains
     in progress. NYCN remains intended first cooperative partner, not
     a formally committed pilot. The exact next gate is defined in
     docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md: organizer
     presentation -> pilot formalization -> first operator rehearsal.
     This does not claim production readiness, live federation
     integration, service hosting implementation, K3s/DNS/GitHub/Forgejo
     mutation, NYCN private-data handling, or resolved licensing. -->

<!-- [sync edit] 2026-04-29 (post-NYCN-#32, ICN PR queue clean):
     Truth-sync extension. NYCN drive-ingest work has continued
     past #28: organizer briefing + simple summit demo (#29);
     start-here onboarding pass (#30); one-command local
     preflight runner (#31); whole-NYCN operating-surfaces
     inventory + Google-Groups boundary policy (#32). NYCN #33
     (steward-facing communication-groups directory tool) was
     open at last sync. ICN PR #1665 (Dependabot TS SDK
     dev-deps) merged 2026-04-29; ICN open-PR queue is empty.
     A cooperator-developer prep brief landed alongside this
     sync at
     docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md
     for conversations with cooperative developers (e.g.
     launch.coop). Phase 2 framing unchanged; the gate
     remains partner/operator-pilot-bound. Phase 2 status
     stays ⏳. Issue #1646 still open. -->

<!-- [sync edit] 2026-04-29 (post-#1675/#1677, post-NYCN-#28):
     Phase 2 framing change. NYCN is the intended first
     cooperative partner; not yet a formally committed pilot.
     We are no longer blocked on partner identification — the
     active partnership track is NYCN — but the next concrete
     step is presenting the merged drive-ingest ladder + ICN
     proof-loop machinery to NYCN organizers to formalize the
     pilot. Subsequent gates: partnership formalization, then
     first operator pilot rehearsal against real (or fixture-
     equivalent) organizer material. Phase 2 deliverables list
     extended to record: completion-receipt retrieval endpoint
     (GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt)
     shipped as #1675; local HTTP proof loop closure documented
     in #1676; K3s smoke proof closure against deployed image
     91a63eec recorded in #1677; NYCN drive-ingest operator
     ladder merged end-to-end (NYCN #21–#28 in fahertym/nycn):
     parser → review → decisions → publish dry-run → assignee
     binding → local publisher → local proof runner → federation
     surface bridge → operator pilot runbook + ladder checker.
     These landings do not flip Phase 2 to ✅ on their own. -->


<!-- [sync edit] 2026-04-27 (post-#1663): Phase 2 status unchanged (still
     blocked on cooperative partners). Action-card runtime now has
     proof-bearing receipt loops for all three currently emitted source
     paths: proposal/vote (#1660), action_item/complete (#1661), and
     meeting/attend (#1663). Issue #1646 remains open with two RFC-
     gated paths still pending: signal_rule (gated on #1631) and
     obligation_lifecycle (gated on #1634). These landings do not flip
     Phase 2 to ✅ on their own — that gate is partner-bound. -->

<!-- [sync edit] 2026-04-27: Phase 2 status unchanged (still blocked on
     cooperative partners). Phase 2 deliverables list extended to record
     the action-card runtime that landed 2026-04-27 (#1659/#1660/#1661):
     /me/action-cards endpoint, proposal/vote receipt proof loop verified,
     action_item/complete receipt proof loop verified. Issue #1646 remains
     open; meeting/attend, signal_rule, and obligation_lifecycle source
     paths remain pending. These do not flip Phase 2 to ✅ on their own —
     that gate is partner-bound. -->

<!-- [sync edit] 2026-04-26: Phase 2 status unchanged (still blocked on
     cooperative partners). Phase 2 deliverables list extended to record
     that institutional-operability infrastructure to support pilot
     deployment landed 2026-04-22 → 2026-04-26 (live charter activation,
     person-directory overlay, /me/standing, authority_scope plumbing,
     bootstrap-apply idempotency, persistent governance domains, NYCN
     bootstrap apply integration tests + live-validate runbook). These
     do not flip Phase 2 to ✅ on their own — that gate is partner-bound. -->

<!-- [sync edit] 2026-04-15: Updated metrics tables with current measurements.
     Phase model, phase definitions, and completion criteria unchanged.
     Phase classification for NYCN institutional work is deferred to PR C. -->

---

### Phase 0: Close the Demo
**Status:** ✅ Complete
**Started:** 2026-03-18
**Completed:** 2026-03-18
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
**Status:** ⏳ In progress (NYCN is the intended first cooperative partner — active partnership track, not yet a formally committed pilot; next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers; see [NYCN Phase 2 Pilot Rehearsal Gate](strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md))
**Started:** —
**Completed:** —
**Sprint(s):** S19–S20

**Objective:** 3–5 real cooperatives operating on ICN for governance and/or time-credit tracking. NYCN is the intended first; additional cooperatives are downstream of a successful first-partner rehearsal.

**Deliverables:**
- [x] Pilot runbook (#1222 ✅ closed)
- [x] Live charter activation endpoint (#1624) — pilots can activate a charter against a running gateway
- [x] Persistent governance domains across gateway restart (#1621)
- [x] Person-directory overlay for bootstrap role assignment (#1626) — DID binding from package-side person ids
- [x] `GET /me/standing` read model (#1627) — member-facing standing surface
- [x] `authority_scope` plumbed end-to-end through `assign_role` (#1630)
- [x] Generic institution bootstrap package path (#1586)
- [x] Bootstrap-apply 409 idempotency (#1617) — re-running bootstrap is safe
- [x] NYCN bootstrap apply integration tests + live-validate runbook (#1592, #1593)
- [x] `GET /v1/gov/me/action-cards` member endpoint with closed source/action enums (#1659)
- [x] Action card → `GovernanceDecisionReceipt` proof linkage for proposal/vote (#1660) — proof loop verified
- [x] `action_item`/`complete` source path emits append-only `ActionItemCompletionReceipt` (#1661) — proof loop verified
- [x] `meeting`/`attend` source path emits append-only `MeetingAttendanceReceipt` (#1663) — proof loop verified; `Present`/`Remote` are receipt-bearing transitions; `Absent` is not; steward-recorded attendance distinguished by `recorded_by` vs `attendee_did`
- [x] `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` retrieval endpoint (#1675) — closes the proof loop on the read side so a holder shell can fetch the persisted receipt over HTTP; `governance:read` scope + domain membership; cross-domain probes rejected
- [x] Local HTTP proof loop closure documented in `docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md` (#1676)
- [x] K3s smoke proof closure (operator-authorized, deployed image 91a63eec) recorded in `docs/dev/NYCN_K3S_PROOF_PATH.md` (#1677)
- [x] NYCN drive-ingest operator ladder merged end-to-end in `fahertym/nycn` (NYCN #21–#28): parser → review → decisions → publish dry-run → assignee binding → local publisher → local proof runner → federation surface bridge → operator pilot runbook + ladder checker. Procedural spine for walking organizer material into ICN action-item proofs without an agent in the loop. **Note:** the ladder runs against a localhost ICN gateway only; K3s exercise lives ICN-side under #1677, not in the NYCN repo.
- [x] NYCN organizer briefing + simple summit demo (NYCN #29) — partner-facing framing for first-rehearsal organizer-track meetings; civic tone, anti-pitch, no live-federation claims.
- [x] NYCN start-here onboarding pass (NYCN #30) — short cold-reader docs (`START_HERE.md`, `ORGANIZER_QUICKSTART.md`, `STEWARD_QUICKSTART.md`, `GLOSSARY.md`) plus a no-network artifact-ladder checker.
- [x] NYCN one-command local preflight runner (NYCN #31) — orchestrates the seven-stage chain in a single deterministic, no-network run; preserves both human-review boundaries; preflight only.
- [x] NYCN whole-system operating-surfaces inventory + Google-Groups boundary policy + repo-safe communication-groups fixture (NYCN #32) — modeling only; no live sync, no private data.
- [x] Licensing metadata and open questions documented (#1686) — documentation only; licensing is not resolved.
- [x] RFC-0017 moved to active for Tool Install Infrastructure (#1688) — active RFC only; infrastructure is not implemented.
- [x] Repo-record protocol/generator added (#1690) — documentation/control-plane generator work.
- [x] Generated ICN repo-record snapshot added (#1691) — mechanical inventory snapshot, not an interpretive atlas.
- [x] Licensing/autonomy strategy matrix added (#1693) — planning only; no relicensing.
- [x] Sovereign service hosting stack documented (#1694) — design direction only; no Forgejo deployment, DNS mutation, K3s mutation, hosted-service rollout, or GitHub cutover.
- [x] May-cycle follow-up queue merged (#1695–#1701) — CI action bumps, wasmtime security bump, unified bootstrap setup, and prior state sync; no Phase 2 completion claim.
- [x] NYCN Phase 2 pilot rehearsal gate defined (`docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md`) — organizer presentation -> pilot formalization -> first operator rehearsal.
- [x] Rehearsal evidence export schema landed under `urn:icn:contract:rehearsal-evidence-export:v1` (#1734) — contract definition only; no live evidence pipeline runs.
- [x] Architecture due-diligence checklist landed at `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` (#1739) — reflex/process artifact only.
- [x] Contract schema-identifier audit landed at `docs/contracts/schema-id-audit.md` (#1741) — inventory/discipline only.
- [x] Organizer/member accessibility gate definition landed at `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` (#1743) — PR-time gate definition only.
- [x] Preview/review read-model contract `urn:icn:contract:preview-review:v1` landed under `docs/contracts/preview-review.md` and `docs/contracts/preview-review.schema.json` (#1745) — contract definition only; no read-model serves over a gateway today.
- [x] `idea-0019` Institutional Process Substrate framing brief landed at `ops/ideas/framing/institutional-process-substrate.md` and matching `ops/ideas/ideas.yaml` row (#1747) — pre-RFC framing only; not an RFC, not a schema commitment, not a backlog commitment.
- [x] Read-model fixture-walk dogfood slice for `idea-0019` landed at `ops/ideas/dogfood/institutional-process-substrate-mvp.md`, plus `ops/ideas/README.md` § "Dogfood slice variants" convention (#1749) — read-model only; emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN; does NOT satisfy receipt-backed promotion thresholds.
- [x] Coordination/control milestone issue #1748 (`milestone(process): define Institutional Process Substrate`) opened with `epic:arch-invariants` + `type:spec` — coordinates spine composition only; not implementation.
- [ ] Runtime dogfood slice for `idea-0019` that emits at least one `ProcessTransitionReceipt` class under `ADR-0026` — receipt-backed; required before promotion to RFC.
- [ ] Visibility/privacy-boundary run with redaction in the evidence export for one `idea-0019` `ProcessSession` — required before promotion to RFC.
- [ ] Accessibility-gate `ProcessGateResult` produced through the `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` checklist on a real surface — required before promotion to RFC.
- [ ] Open-question triage on framing-brief Q1 (`ProcessTargetRef` polymorphism), Q3 (`DeliberationEntry` kind taxonomy), or Q4 (`HumanDecisionSet` vs proposal/vote) — at least one resolved or explicitly deferred in writing before promotion to RFC.
- [ ] NYCN steward-facing communication-groups directory tool (NYCN #33) — open at last sync; verify status before reading.
- [ ] Action-card runtime — remaining gates under #1646 (RFC-gated): `signal_rule` source path (gated on #1631); `obligation_lifecycle` source path (gated on #1634)
- [ ] One-command deployment script per cooperative
- [ ] Charter customization workflow documented (charter activation endpoint exists; non-technical workflow doc still missing)
- [ ] Pilot onboarding guide (non-technical audience)
- [ ] Deploy nodes for 3–5 pilot cooperatives
- [ ] Weekly check-in process established
- [ ] Pilot case study written (for grant/funder audiences)

**Blockers:**
- ~~Requires Phase 1 complete~~ ✅ Charter Engine is live
- ~~Requires bootstrap activation runtime~~ ✅ live charter activation + person-directory + standing read model landed 2026-04-22 → 2026-04-26
- ~~Requires cooperative partners identified~~ ✅ NYCN is the intended first cooperative partner (active partnership track)
- Next concrete step: present the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers, as defined in [NYCN Phase 2 Pilot Rehearsal Gate](strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md)
- Subsequent gates: pilot formalization, then first operator rehearsal against real (or fixture-equivalent) organizer material

**Decisions Made:**
- (2026-05-05, post-#1734/#1739/#1741/#1743/#1745/#1747/#1749, with open #1748) The May-5 institutional-process-substrate sequence is documentation/control-plane and idea-refinery only. (a) Five contract/design/architecture docs landed: rehearsal evidence export schema (#1734), architecture due-diligence checklist (#1739), contract schema-identifier audit (#1741), organizer/member accessibility gate definition (#1743), and the preview/review read-model contract `urn:icn:contract:preview-review:v1` (#1745). (b) `idea-0019` Institutional Process Substrate was named in the idea refinery with a framing brief (#1747) and a read-model fixture-walk dogfood slice (#1749) that walks a fictional Example Cooperative process session against the SAME shipping contract URNs as the committed examples without modifying any kernel, runtime, gateway, ledger, governance, or SDK code and without minting any new contract URN. The new `ops/ideas/README.md` § "Dogfood slice variants" convention formalizes that a read-model fixture walk does NOT satisfy receipt-backed promotion thresholds. (c) Coordination/control milestone issue #1748 is open with `epic:arch-invariants` + `type:spec`; no implementation issue is opened from it. (d) Phase 2 status is unchanged; the next concrete human gate remains organizer presentation -> pilot formalization -> first operator rehearsal per `docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md`. (e) The next pre-RFC architecture move is **Democratic Authority Primitives** (delegation, representation, expert/advisory input, deliberation context / educational references, conflict disclosure, facilitator and steward/operator authority, and revocation/recall/challenge paths) — generic primitives institutions adopt and constrain through CCL, charters, and institution packages, not ICN app features and not a runtime commitment. Not started in this sync.
- (2026-05-02, post-#1695/#1696/#1697/#1698/#1699/#1700/#1701) May-cycle repo-governance, strategy, dependency/CI maintenance, bootstrap, and state-sync work has merged through #1701. These are truth/control-plane, planning, or maintenance landings only. They do not complete Phase 2, formally commit NYCN as a pilot, claim production readiness, claim live federation integration, implement service hosting, mutate DNS/K3s/GitHub/Forgejo state, implement RFC-0017, handle NYCN private data, or resolve licensing. The next concrete human gate is now explicitly documented in `docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md`: organizer presentation, pilot formalization, then first operator rehearsal. Open PR queue was empty at this sync.
- (2026-04-29, post-#1675/#1677, post-NYCN-#28) The Phase 2 *machinery* is now in place end-to-end: (a) action-card runtime is proof-bearing for all currently emitted source paths, (b) the completion-receipt retrieval endpoint exists so a holder shell can read receipts over HTTP, (c) the local HTTP proof loop is closed and documented, (d) the K3s smoke proof loop is closed against deployed image `91a63eec` and documented, and (e) the NYCN drive-ingest operator ladder is merged end-to-end as a procedural spine. NYCN is the intended first cooperative partner (active partnership track); the next concrete step is **presenting the merged ladder + ICN proof-loop machinery to NYCN organizers** to formalize the pilot. Phase 2 remains ⏳ until that presentation, the partnership formalization that follows, and the first operator pilot rehearsal happen and are recorded. The two RFC-gated action-card source paths (`signal_rule`, `obligation_lifecycle`) remain open under #1646 and are independent of the partner gate.
- (2026-04-27, post-#1663) Action-card runtime is now proof-bearing for **all three currently emitted source paths**: `proposal`/`vote` (#1660), `action_item`/`complete` (#1661), and `meeting`/`attend` (#1663). Issue #1646 remains open for the two RFC-gated paths: `signal_rule` (#1631) and `obligation_lifecycle` (#1634). Phase 2 status is unaffected (still partner-bound).
- (2026-04-27) Action-card runtime is partial: `/me/action-cards` exists, `proposal`/`vote` and `action_item`/`complete` source paths have verified end-to-end receipt proof loops, and `meeting`/`attend`, `signal_rule`, `obligation_lifecycle` paths remain pending under #1646. Phase 2 status is unaffected.
- (2026-04-26) Pilot enablement infrastructure (bootstrap, charter activation, role binding, standing) is in place; Phase 2 remains ⏳ until partners run it for real.

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

*Note: Re-measurement deferred. NYCN governance work (#1540, #1543, #1547) added app-layer crates (icn-governance-actor), not kernel imports. Kernel infection delta is expected to be 0 but not yet verified.*

### Test Count

| Date | Total Tests | Delta |
|------|-------------|-------|
| 2026-03-18 (baseline) | 4,287 | — |
| 2026-04-15 | 6,463 | +2,176 |

### Codebase Size

| Date | Rust Lines (crates+bins) | App Lines | Delta |
|------|--------------------------|-----------|-------|
| 2026-03-18 (baseline) | ~420,000 | ~7,000 | — |
| 2026-04-15 | ~458,000 | ~35,000 | +38K crates/bins, +28K apps |
