# ICN Phase Progress
**Last Updated:** 2026-06-10
**Current Phase:** Phase 2 — Pilot Launch. NYCN is the intended first cooperative partner (active partnership track, not yet a formally committed pilot); the next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery + the architecture-spec ladder + the Civic Shell v0 composition spec to NYCN organizers. Subsequent gates: pilot formalization, then first operator rehearsal. The exact organizer/operator gate is defined in the NYCN rehearsal gate (in the partner repo). Institutional-operability infrastructure, the action-card runtime, the action-item completion-receipt retrieval endpoint, the NYCN drive-ingest operator ladder, the opaque receipt storage stack, the May-14/May-15 architecture-spec ladder (twelve design-level spec documents covering integrated operating model, effect dispatch, institutional domain, CCL policy registry, governed service binding, storage durability, ArtifactRegistry/ScopedVault, scope vocabulary, compute placement, anti-entropy proof loops, member shell, steward cockpit), the 2026-05-22 governance-hardening cycle (governance:write decomposition design #1880, class-level scope constants minted in `icn-rpc` #1881, governance context build-mode production guard #1901), the Civic Shell v0 composition surface spec #1899, the Debian-13 appliance real-smoke verification #1900, and the Claude Design seed-review + truth-label + icon-candidate + federation-operator-surface + member-shell action-card-refinement design cycle (#1885–#1889) are all in place. The 2026-06-10 truth-sync additionally records (PR-by-PR record in the matching `docs/STATE.md` block): the previously unrecorded 2026-05-15 → 2026-05-17 network-schema + Slice A fixture stack (#1843–#1874 — all three Slice A fixture rehearsals #1838/#1839/#1840 closed as completed, including the member-shell read-only rendering rehearsal #1848 with its ADR-0028 accessibility assertions), and the 2026-05-23 → 2026-06-10 window (#1903–#2016): governance:write decomposition executed across all seven handler families with broad scope kept as accepted-also fallback (#1903–#1951, #1984), MandateGate + mandate-attested v2/v3 governance receipts (#1925–#1938), TrustThreshold fail-open closures (#1911/#1916/#1917), the live local 13/13 receipt-chain audit path (#1979/#1985, dev-gated self-trust seed) with durable EffectDispatchEvidence recovery (#1990) and its paging/index follow-ups (#1986/#1988/#1993/#1996), the dev-gated demo/rehearsal layer (#1953/#1954/#1980/#1981/#1997/#1999), the passport/keyring vocabulary and custody-surface remediation (#1957–#1978), the pending-publish summary row contract (#1998), the proof-level taxonomy + rehearsal capability matrix (#2000), the honest-introduction materials stack with evidence map (#2002/#2003/#2008), agent worktree policy (#2009), and dependency/CI maintenance including the explicit accepted-risk decision to ignore unmaintained pqcrypto-* advisories (#1983). None of the architecture-spec ladder is runtime implementation; the closure batch (nine sibling issues closed at docs/spec level on 2026-05-15) and the first-batch follow-up filings (#1834–#1840) reflect that boundary. The 2026-05-22 governance-hardening cycle is mixed truth class — #1901 and #1881 are real Rust changes, the rest is design/docs. The Civic Shell spec is composition only; no app, endpoint, auth, deployment, DNS, or K3s mutation lands with it. The Debian-13 appliance smoke verified the positive firstboot path (#1900, re-verified as baseline 2026-06-10); one negative fail-closed scenario (missing-firstboot-exec) was operator-verified on 2026-06-10 — firstboot failure propagates via the 10-firstboot-gate.conf Requires= and icnd never starts; all other appliance failure modes remain unverified. May-cycle repo-governance, licensing, RFC, repo-record, service-hosting, dependency/CI maintenance, bootstrap, state-sync docs, the May-5 institutional-process-substrate framing sequence, the May-6/May-7 opaque receipt storage stack runtime work (#1755 / #1757 / #1758 / #1759 + `OPAQUE_HASH_BIND_PREFIX` invariant), the May-7 close-out cycle, and the 2026-05-18 → 2026-05-22 hardening + Civic-Shell + appliance + design + strategy cycle have all landed. None of these mark Phase 2 complete or imply production readiness, live federation integration, implemented service hosting, K3s/DNS/GitHub/Forgejo mutation, NYCN private-data handling, partner activation, an implemented Civic Shell, an implemented member shell, an implemented steward cockpit (member shell and cockpit have fixture-level rendering proofs only), a completed mandate-authority migration (MandateGate landed as code in #1925–#1931, but capability scopes with broad accepted-also fallback still authorize every handler), retirement of broad `governance:write`, appliance fail-closed coverage beyond the single verified missing-firstboot-exec scenario, signed/immutable/partner-distributable appliance images, or resolved licensing.

<!-- [sync edit] 2026-06-10 (appliance negative firstboot smoke verified — this PR): one narrow slice. Adds deploy/appliance/smoke/negative-firstboot-smoke.sh + README section + operator handoff. Operator-run on icn-dev 2026-06-10 against the same artifact image #1900 built (SHA256 re-verified against its manifest): positive baseline re-run PASS first, then the negative run — firstboot ExecStart deleted from a disposable overlay → unit failed (status=203/EXEC) → journal: "Dependency failed for icnd.service" / "Job icnd.service/start failed with result 'dependency'" → marker absent → icnd never active and /v1/health never answered during the observation window. Appliance claim moves to: positive path verified + ONE negative fail-closed scenario verified; everything else (other failure modes, signing, immutability, partner distribution, production readiness) remains unverified/not claimed. Full evidence in docs/dev/handoff-2026-06-10-appliance-negative-firstboot-smoke.md and the matching docs/STATE.md block. Phase model unchanged. Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-06-10 (post #1903 → #2016 window plus gap correction #1843 → #1874): mixed truth class. Records 91 commits merged 2026-05-23 → 2026-06-10 plus 17 previously unrecorded PRs merged 2026-05-15 → 2026-05-17 (the #1902 sync verified an external review brief covering #1875 → #1901 and did not sweep the full git window). Real-Rust highlights: governance:write decomposition executed across all seven handler families with accepted-also broad fallback (42 `require_any_scope` call sites verified in `icn/apps/governance/src/http/handlers.rs` at `9012ba5c`; zero bare broad-scope mutation gates remain; broad scope NOT retired); MandateGate + MandateGrantRef + mandate-attested v2/v3 receipts (#1925–#1938); TrustThreshold fail-open closures (#1911/#1916, closes #1913); live local 13/13 receipt-chain audit path (#1985, dev-gated `ICN_DEV_SELF_TRUST` seed — not a partner deployment); durable EffectDispatchEvidence recovery via the dispatcher ExecutionRecord write-ahead handle (#1990, closes #1987); Slice A fixture rehearsals all completed (#1845/#1846/#1848 closing #1838/#1840/#1839). Docs/control-plane highlights: passport/keyring vocabulary migration (#1957–#1978), pending-publish summary row contract (#1998), proof-level taxonomy (#2000), honest-introduction materials + evidence map (#2002/#2003), agent worktree policy (#2009). No new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway API widening, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim. #1868 remains OPEN (steps 1–6 landed; broad-fallback retirement is a future decision). Appliance negative fail-closed firstboot path remains UNVERIFIED. Detailed stack-by-stack breakdown, code anchors, issue-state record, open-PR inventory, truth-class corrections (whitepaper crate count 38 → 37 fixed; status.toml re-verified correct), and named candidate next moves are in the matching `docs/STATE.md [sync edit] 2026-06-10` block; not duplicated here to preserve registry-controlled length. Phase model unchanged. Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-05-22 (post #1834 → #1901 cycle): mixed truth class. Records the 28-PR window 2026-05-17 → 2026-05-22, including two real-Rust changes (#1881 class-level governance scope constants minted in `icn/crates/icn-rpc/src/auth.rs:966-979`; #1901 `GovernanceContextBuildMode { Bootstrap, Production, Test }` + `ICN_GOVERNANCE_BUILD_MODE` env read at `icn/apps/governance/src/http/configure.rs:58/95/455/459`) and the rest docs/control-plane. Closes #1871; #1870 was already closed; #1868 remains OPEN (decomposition step 2 — handler migration off broad `governance:write` — not started; 41 broad call sites in `icn/apps/governance/src/http/handlers.rs` remain). No new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway API widening, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim. Detailed PR-by-PR breakdown, code anchors, open Dependabot inventory, truth-class corrections (status.toml workspace count fixed from 41 → 44), and named candidate next moves are recorded in the matching `docs/STATE.md [sync edit] 2026-05-22` block; not duplicated here to preserve registry-controlled length. Phase model unchanged. Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-05-15 (post architecture-spec sprint, PRs #1814 / #1819 / #1820 / #1821 / #1822 / #1823 / #1824 / #1825 / #1826 / #1827 / #1829 / #1830 / #1831 / #1832 / #1833):
     Truth-sync for the architecture-spec sprint completion. **Doc/control-plane only**: no Rust code, no schema fields changed, no new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway/runtime mutation, no K3s/DNS/Forgejo mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     Phase 2 deliverables list extended to record the thirteen merged spec PRs + one process-doc PR + one wrap-up PR:
       - #1814 docs(architecture): add integrated cooperative operating model spine — the ladder root naming all the per-spec slots that subsequent PRs filled.
       - #1819 docs(spec): add accepted-proposal effect dispatch contract — five-stage chain (decision → mandate → effect plan → dispatch → application + evidence). Closed #1797 on merge.
       - #1820 docs(spec): define institutional domain and policy primitive — InstitutionalDomain + DomainPolicy.
       - #1821 docs(spec): define CCL policy registry and hook contract — policy_version_id, adoption contract, evaluator-selection contract.
       - #1822 docs(spec): define governed service binding, workload manifest, and runtime provider — three primitives + ten-state lifecycle + seven runtime classes.
       - #1823 docs(spec): define storage durability policy objects — StorageSpec / BackupPolicy / ReplicationPolicy / RecoveryPolicy / ArchivePolicy / IntegrityPolicy + restore-test receipts + locality/privacy inheritance.
       - #1824 docs(spec): define ArtifactRegistry v0 and ScopedVault boundary — artifact-class taxonomy + opaque-reference framing.
       - #1825 docs(architecture): define entity-scope vocabulary boundary — LocalDomain not Coop. Doctrine that flows down through every subsequent spec.
       - #1826 docs(spec): define compute placement policy — seven placement classes, eighteen decision inputs, five candidate outputs, fourteen-field operator/steward dashboard, seven-string member-shell vocabulary.
       - #1827 docs(agents): reconcile handoff path with template — process-doc that fixed the AGENTS.md / HANDOFF_TEMPLATE.md path drift to match actual sprint practice.
       - #1829 docs(spec): define network anti-entropy proof loops — eight-phase institutional evidence loop + thirteen forward-direction proof-artifact identifiers + eighteen divergence classes + nine-field steward cockpit surface + seven-string member-shell sync vocabulary.
       - #1830 docs(spec): define member shell v0 — mobile-first / offline-tolerant / accessibility-first / plain-language-first rendering contract over ADR-0020 standing + ADR-0027 ActionCard + ADR-0026 receipts + the twelve-category ADR-0028 accessibility gate. Ten design principles led by "Mobile-first."
       - #1831 docs(spec): define steward cockpit v0 — twelve cockpit surfaces, fourteen operator action-card scenarios, member-impact summary mapping into member-shell vocabulary (Design principle 9). Post-merge: nine late reviewer threads required #1832 to land in main.
       - #1832 fix(spec): correct steward cockpit review drift — four rounds of fixes: initial 5-drift round + 8-field count finish + ADR-0027 14-field requirement replacement with rendering-analog set + PlacementFallbackReceipt attribution + handoff timing correction + IA-row no longer routes through ADR-0027.
       - #1833 docs(dev): wrap architecture spec sprint closure review — single review-handoff documenting the sprint PR roster, per-issue closure analysis, paste-ready closure-comment drafts, deduplicated follow-up roster of 34 drafts grouped by concern area, truth-sync target list, cross-sprint non-claims, and recommended next-decision sequence.

     Closure batch (after #1833 merged): nine sprint sibling issues closed at docs/spec level — #1794 (institutional domain), #1795 (steward cockpit), #1798 (ArtifactRegistry/ScopedVault), #1799 (network anti-entropy), #1801 (compute placement), #1815 (governed service binding), #1816 (storage durability), #1817 (CCL policy registry), #1818 (member shell). #1797 (effect dispatch) was already closed by #1819. Every closure comment explicitly names which acceptance criteria are satisfied, what remains out of scope, and that the closure is docs/spec-level only — none implies runtime-implementation completion.

     First batch of follow-up issues filed from the wrap-up's deduplicated roster (#1834–#1840): three schema follow-ups (AntiEntropyProbe/StateDigest #1834; DivergenceEvidence/RepairPlan #1835; PlacementDecision/ExecutorAdmissionDecision #1836), one contract follow-up (#1837 steward required-action card contract — the ADR-0027-doesn't-cover-operator-scenarios gap the cockpit drift fix surfaced), and three first-slice fixture follow-ups (#1838 anti-entropy Slice A; #1839 member shell Slice A; #1840 cockpit Slice A). Twenty-seven additional follow-up drafts remain in the wrap-up doc for separate batch decisions.

     Cross-sprint disciplines preserved verbatim (these are not new claims; they are the disciplines the sprint affirmed and that the closure batch preserves):
       - No new ADR-0026 receipt classes. Proof-artifact identifiers (PlacementDecision, RepairReceipt, DivergenceEvidence, PlacementFallbackReceipt, etc.) ride inside existing Stage 5 EffectDispatchEvidence or Layer 2 ArtifactReceipt envelopes.
       - No ADR-0027 support for steward required-action cards. ADR-0027 covers member ActionCards; #1837 carries the operator-scenario gap.
       - LocalDomain vocabulary (not Coop) per #1825 §C3. Existing serialized Coop-prefixed identifiers (DataLocality::CoopReplicated, ADR-0030 Coop(coop_id), bonds:payments gossip topic) preserved with naming notes pending rename follow-up.
       - Execution budget is policy-facing; fuel_limit is the runtime field; capacity is reserved for executor/node resource availability.
       - Settlement / position / obligation / allocation / receipt / provenance — never payment / wallet / currency / balance / token / crypto / blockchain / timebank — for ICN-native compute / settlement / federation surfaces.
       - Member shell shows plain participation status; steward cockpit shows technical detail. When cockpit shows degraded, member shell must show degraded too — v0 violation otherwise.
       - Privacy is posture, not content. Body bytes of PrivateEvidence artifacts never reach any rendering layer.
       - The kernel never imports app-side rendering.

     `idea-0019` (#1748) acceptance gates remain unchanged from the previous (May-7) sync — none of the architecture-spec sprint advances or retreats those gates. Phase 2 status remains ⏳ (still partner-bound). Next pre-RFC architecture move is **not yet selected**; this sync preserves optionality. Candidate next moves (descriptive, not selected): batch-file remaining 27 follow-up drafts; implementation slice (feat(compute) placement oracle read-only proof-loop); first fixture rehearsal (one of #1838 / #1839 / #1840); next spec-ladder doc (#1837 steward required-action card contract); DAP runtime dogfood emitting at least one ADR-0026 receipt for one DAP primitive (carried); idea-0019 runtime dogfood emitting additional ProcessTransitionReceipt classes (carried); idea-0019 visibility/privacy-boundary run with redaction (carried); idea-0019 accessibility-gate ProcessGateResult on a real surface (carried); idea-0019 open-question triage (carried).

     Hard rule preserved: this cycle does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet, does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, and does NOT start any Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work. -->

<!-- [sync edit] 2026-05-07 (post-#1761 / #1762 / #1763 / #1764):
     Truth-sync for the May-7 close-out cycle plus the ActionCard contract publication landing. **Doc/control-plane only** (with one runtime fix in `icn-commons`): no schema fields changed, no new contract URN, no new ADR, no new RFC, no new `ProcessTransitionReceipt` class, no `ProcessSession` storage, no DAP runtime dogfood, no Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work, no NYCN edits.
     Phase 2 deliverables list extended to record:
       - #1761 fix(commons): retry sled open on WouldBlock to bridge flusher shutdown. Bounded retry-with-backoff in `SledCommonsStore::open` (8 attempts max, 500ms total budget cap, 10ms initial backoff, only matches `io::ErrorKind::WouldBlock` so genuine errors are not masked). Two new unit tests pin the new behavior. Single-file change. Closes #1760 (sled 0.34 flusher-thread shutdown race surfaced by #1759's CI Test job).
       - #1762 docs(state): sync opaque receipt storage stack landing. Records #1755/#1756/#1757/#1758/#1759 in `docs/STATE.md` and `docs/PHASE_PROGRESS.md`. Adds `docs/dev/handoff-2026-05-07.md`. Doc/control-plane only.
       - #1763 deps(ts-sdk): bump the dev-dependencies group across 1 directory with 4 updates. Dependabot maintenance of `sdk/typescript/`. No runtime change.
       - #1735 deps(pilot-ui): bump @axe-core/playwright in /web/pilot-ui. Dependabot maintenance. No runtime change.
       - #1764 docs(contracts): publish ActionCard contract for institution packages. Adds `docs/contracts/institution-package/action-card.example.json` (fictional sample card validating against the existing schema) and `docs/scripts/validate-action-card.py` (draft-2020-12 JSON Schema validator mirroring the convention used by `validate-preview-review.py` and `validate-rehearsal-evidence.py`); expands `docs/contracts/institution-package/README.md` with stability rationale citing ADR-0027 § Card kind taxonomy ("growable by ADR amendment"), schema-id-audit linkage to the retain-temporarily decision (review by 2026-06-30 tracked by #1742), explicit CLI validation commands, regulatory-safe vocabulary enumeration (obligation, allocation, settlement, unit, position, receipt, provenance, evidence — explicitly not payment / wallet / balance / currency), explicit "institution-specific semantics belong in institution packages, not in ICN core" guidance, and partner-package vendor-or-invoke-from-CI guidance. One substantive Copilot review finding addressed pre-merge (CLI arg `packet` -> `card`; mechanical rename only). Closes #1713 with all six acceptance criteria met (generic schema matches runtime fields; honest stability/status marker; emitted-vs-gated source kinds distinguished; NYCN-specific nouns absent; regulatory-safe vocabulary preserved; package validation path documented). The schema's `$id` remains DNS-backed under the audit's retain-temporarily decision; migration to `urn:icn:contract:action-card:v<N>` is a separate single-schema PR under audit §5 rules.
     `idea-0019` (#1748) acceptance gates after this sync:
       - (a) **runtime dogfood emitting at least one `ProcessTransitionReceipt` class under `ADR-0026`** — unchanged from #1762: still partial (`ProcessGateResultReceipt` emitted via #1755 and durably persisted via the opaque cascade since #1759). Additional classes remain candidates.
       - (b) real visibility/privacy-boundary run with redaction in evidence export — unchanged: not started.
       - (c) accessibility-gate `ProcessGateResult` on a real surface — unchanged: not started.
       - (d) open-question triage (Q1, Q3, or Q4) — unchanged: not started.
     #1713 closure is independent of #1748's gates: #1713 is the ActionCard *contract publication* gate; #1748 is the institutional-process-substrate *runtime* gate.
     Phase 2 status remains ⏳ (still partner-bound). The next concrete human gate is unchanged: organizer presentation -> pilot formalization -> first operator rehearsal per the NYCN rehearsal gate (in the partner repo). Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves are enumerated descriptively in `docs/STATE.md` "Current status" paragraph — none is selected here. One less open follow-up than at the previous sync since #1761 closed #1760.
     Hard rule preserved: this cycle does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet (baseline 10 known violations preserved, 0 new), does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, and does NOT start any Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work. -->

<!-- [sync edit] 2026-05-07 (post-#1755 / #1756 / #1757 / #1758 / #1759):
     Truth-sync for the opaque receipt storage stack landing. Unlike the May-5 sync edits, this is **runtime/implementation truth** — real Rust changes landed in `icn-gateway` and `apps/governance`. Phase 2 status remains ⏳ (still partner-bound).
     Phase 2 deliverables list extended to record:
       - #1755 first runtime dogfood emitting one of the eight named `ProcessTransitionReceipt` classes from the `idea-0019` framing brief: `ProcessGateResultReceipt`. Emitted by `GovernanceManager::record_process_gate_result`. Surfaced a production durability gap on the sled-backed `ReceiptStore`.
       - #1757 meaning-blind opaque receipt storage primitive on the gateway: `put_opaque` / `get_latest_opaque` / `list_opaque_for` keyed on `(class, key1, key2_opt, recorded_at, record_hash)`. Three substantive review findings addressed in `cb9d6daf` (write-once-by-hash on the primary record with stable sentinel `opaque_record_hash_collision`; atomic primary + secondary index writes; distinct `key2 = None` vs `key2 = Some("")` tag-byte encoding; deterministic `(recorded_at, record_hash)` tie-breaker). One additional codex P2 raised against `cb9d6daf` and addressed in `a8fbb1a6`: new `OPAQUE_HASH_BIND_PREFIX` keyspace binds each `(class, record_hash)` to exactly one canonical `(key1, key2_opt, recorded_at)` tuple at first write; divergent re-binds abort with stable sentinel `opaque_record_hash_index_collision`. Bind, primary, and secondary writes are atomic inside the same sled transaction.
       - #1758 `GovernanceReceiptBackend` trait extended with fail-closed opaque method surface; sled-backed `ReceiptStore` overrides them via thin delegates to its inherent opaque methods.
       - #1759 `put_process_gate_result` trait default rewritten to attempt the opaque cascade first and fall back to the explicit `process_gate_result_backend_not_implemented` sentinel only when the underlying `put_opaque` itself returns the opaque-not-implemented sentinel. Production gateway-backed `ReceiptStore` therefore now durably persists `ProcessGateResultReceipt` through the opaque cascade without any new typed governance import on `icn-gateway`. Test-suite determinism follow-up applied in the same PR (replaced three `std::thread::sleep(1100ms)` calls with explicit `recorded_at` timestamps; suite finishes in 0.01s).
       - #1756 repo-tooling fix for scope-guard / todo-guard exec bit + todo-guard pipeline. No runtime / contract / schema / API change.
       - Issue #1760 filed and PR #1761 opened (`fix(commons): retry sled open on WouldBlock to bridge flusher shutdown`) for a pre-existing sled 0.34 flusher-thread shutdown race surfaced under CI load on `test_commons_charter_survives_sled_drop_and_reopen`. Open at sync write-time.
     `idea-0019` (#1748) acceptance gates after this sync:
       - (a) **runtime dogfood emitting at least one `ProcessTransitionReceipt` class under `ADR-0026`** — partial: `ProcessGateResultReceipt` emitted (#1755) and now durably persisted via the opaque cascade (#1759). Additional classes remain candidates.
       - (b) real visibility/privacy-boundary run with redaction in evidence export — unchanged: not started.
       - (c) accessibility-gate `ProcessGateResult` on a real surface — unchanged: not started.
       - (d) open-question triage (Q1, Q3, or Q4) — unchanged: not started.
     Phase 2 status remains ⏳ (still partner-bound). The next concrete human gate is unchanged: organizer presentation -> pilot formalization -> first operator rehearsal per the NYCN rehearsal gate (in the partner repo). Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves are enumerated descriptively in `docs/STATE.md` "Current status" paragraph — none is selected here.
     Hard rule preserved: this stack does NOT widen gateway typed governance imports; the opaque storage primitive is bytes-in / bytes-out and adds zero new domain types. Meaning-firewall ratchet unchanged: baseline 10 known violations preserved, 0 new. -->

<!-- [sync edit] 2026-05-05 (post-#1753):
     Truth-sync for the Democratic Authority Primitives read-model fixture-walk dogfood landing. Doc/control-plane and idea-refinery only; no runtime, no schema, no contract URN, no ADR, no RFC, no implementation issue, no runtime dogfood, no Phase 2 advance.
     Phase 2 deliverables list extended to record:
       - Read-model fixture-walk dogfood slice for `idea-0020` landed at `ops/ideas/dogfood/democratic-authority-primitives-mvp.md` alongside an `ops/ideas/ideas.yaml` row update (#1753) — read-model fixture-walk variant per `ops/ideas/README.md` § "Dogfood slice variants"; composes the six DAP primitive families named in the framing brief's §17 follow-up (`AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`, `ConflictDisclosure`, `MinorityReport`, `DeliberationContext`) end-to-end against the merged `idea-0019` read-model fixture walk; references `OperatorExecutionAuthority` as the strictly-downstream-of-decision operator handle at the activation gate; emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN; receipt class candidates `FacilitatorSummaryRecordedReceipt`, `ConflictDisclosureAcceptedReceipt`, and `MinorityReportRecordedReceipt` are slice-local candidates only and not committed as canonical.
     Phase 2 status remains ⏳ (still partner-bound). The next concrete human gate is unchanged: organizer presentation -> pilot formalization -> first operator rehearsal per the NYCN rehearsal gate (in the partner repo). The prior sync (post-#1751) named the DAP read-model composition slice as the most directly named candidate next move; #1753 has now landed it. Per `ops/ideas/README.md` § "Dogfood slice variants" and per the DAP framing brief's §16.1, a read-model fixture walk does NOT satisfy receipt-backed promotion thresholds; promotion of `idea-0020` to RFC still requires (1) a separate runtime dogfood emitting at least one receipt under `ADR-0026` for one of the named primitives, (2) a real visibility/privacy-boundary run with redaction in evidence export, (3) an accessibility-gate `ProcessGateResult` produced through `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real surface, and (4) Q1 (`AuthorityBasis` polymorphism) or Q5 (`ConflictDisclosure`/`MinorityReport` placement) **resolved** in writing (deferral is not sufficient for the RFC gate per §16.1; the lenient resolved-or-deferred standard at §16.3 applies only to the broader runtime-justification threshold). Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves are enumerated descriptively in `docs/STATE.md` "Current status" paragraph — none is selected here. -->

<!-- [sync edit] 2026-05-05 (post-#1751):
     Truth-sync for the Democratic Authority Primitives framing landing. Doc/control-plane and idea-refinery only; no runtime, no schema, no contract URN, no ADR, no RFC, no implementation issue, no runtime dogfood, no Phase 2 advance.
     Phase 2 deliverables list extended to record:
       - `idea-0020` Democratic Authority Primitives framing brief landed at `ops/ideas/framing/democratic-authority-primitives.md` and matching `ops/ideas/ideas.yaml` row (#1751) — pre-RFC framing only; names two generic primitive families (authority/participation and deliberation context / educational reference) institutions adopt and constrain through CCL, charters, and packages; composes orthogonally with `idea-0019` (Institutional Process Substrate); promotion to RFC requires (per the brief's §16.1) read-model composition slice with `idea-0019`, runtime dogfood emitting at least one receipt under `ADR-0026`, real visibility/privacy-boundary run, accessibility-gate `ProcessGateResult` on a real surface, and at least one open question — Q1 (`AuthorityBasis` polymorphism) or Q5 (`ConflictDisclosure`/`MinorityReport` placement) — **resolved** in writing (deferral is not sufficient for the RFC gate per §16.1; the resolved-or-deferred standard at §16.3 applies only to the broader runtime-justification threshold) — none of those follow-ups is started in this sync.
     Phase 2 status remains ⏳ (still partner-bound). The next concrete human gate is unchanged: organizer presentation -> pilot formalization -> first operator rehearsal per the NYCN rehearsal gate (in the partner repo). Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves are enumerated descriptively in `docs/STATE.md` "Current status" paragraph — none is selected here. -->

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
     Phase 2 status remains ⏳ (still partner-bound). The next concrete human gate is unchanged: organizer presentation -> pilot formalization -> first operator rehearsal per the NYCN rehearsal gate (in the partner repo). Next pre-RFC architecture move (not started in this sync): Democratic Authority Primitives. -->

<!-- [sync edit] 2026-05-02 (post-#1695/#1696/#1697/#1698/#1699/#1700/#1701):
     Follow-up May-cycle queue is now merged on main: Dependabot
     Actions major-version bumps (#1695-#1698), wasmtime security bump
     (#1699), unified dev-environment bootstrap (#1700), and prior state
     sync (#1701). Open PR queue is empty at this sync. Phase 2 remains
     in progress. NYCN remains intended first cooperative partner, not
     a formally committed pilot. The exact next gate is defined in
     the NYCN rehearsal gate (in the partner repo): organizer
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
**Status:** ⏳ In progress (NYCN is the intended first cooperative partner — active partnership track, not yet a formally committed pilot; next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers; see the NYCN rehearsal gate in the partner repo)
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
- [x] Local HTTP proof loop closure documented in the NYCN action-item receipt path (in the partner repo) (#1676)
- [x] K3s smoke proof closure (operator-authorized, deployed image 91a63eec) recorded in the NYCN K3s proof path (in the partner repo) (#1677)
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
- [x] NYCN Phase 2 pilot rehearsal gate defined (in the partner NYCN repo) — organizer presentation -> pilot formalization -> first operator rehearsal.
- [x] Rehearsal evidence export schema landed under `urn:icn:contract:rehearsal-evidence-export:v1` (#1734) — contract definition only; no live evidence pipeline runs.
- [x] Architecture due-diligence checklist landed at `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` (#1739) — reflex/process artifact only.
- [x] Contract schema-identifier audit landed at `docs/contracts/schema-id-audit.md` (#1741) — inventory/discipline only.
- [x] Organizer/member accessibility gate definition landed at `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` (#1743) — PR-time gate definition only.
- [x] Preview/review read-model contract `urn:icn:contract:preview-review:v1` landed under `docs/contracts/preview-review.md` and `docs/contracts/preview-review.schema.json` (#1745) — contract definition only; no read-model serves over a gateway today.
- [x] `idea-0019` Institutional Process Substrate framing brief landed at `ops/ideas/framing/institutional-process-substrate.md` and matching `ops/ideas/ideas.yaml` row (#1747) — pre-RFC framing only; not an RFC, not a schema commitment, not a backlog commitment.
- [x] Read-model fixture-walk dogfood slice for `idea-0019` landed at `ops/ideas/dogfood/institutional-process-substrate-mvp.md`, plus `ops/ideas/README.md` § "Dogfood slice variants" convention (#1749) — read-model only; emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN; does NOT satisfy receipt-backed promotion thresholds.
- [x] Coordination/control milestone issue #1748 (`milestone(process): define Institutional Process Substrate`) opened with `epic:arch-invariants` + `type:spec` — coordinates spine composition only; not implementation.
- [x] `idea-0020` Democratic Authority Primitives framing brief landed at `ops/ideas/framing/democratic-authority-primitives.md` and matching `ops/ideas/ideas.yaml` row (#1751) — pre-RFC framing only; not an RFC, not an ADR, not a schema, not a contract URN, not a backlog commitment. Names two generic primitive families (authority/participation; deliberation context / educational reference) institutions adopt and constrain through CCL, charters, and institution packages. Composes orthogonally with `idea-0019` (Institutional Process Substrate). Promotion to RFC requires (per the brief's §16.1) read-model composition slice with `idea-0019`, runtime dogfood emitting at least one receipt under `ADR-0026`, real visibility/privacy-boundary run, accessibility-gate `ProcessGateResult` on a real surface, and at least one open question — Q1 (`AuthorityBasis` polymorphism) or Q5 (`ConflictDisclosure`/`MinorityReport` placement) — **resolved** in writing (deferral is not sufficient for the RFC gate per §16.1; the resolved-or-deferred standard at §16.3 applies only to the broader runtime-justification threshold).
- [x] Read-model fixture-walk dogfood slice for `idea-0020` landed at `ops/ideas/dogfood/democratic-authority-primitives-mvp.md` alongside an `ops/ideas/ideas.yaml` row update (#1753) — read-model fixture-walk variant per `ops/ideas/README.md` § "Dogfood slice variants" (formalized in #1749). Composes the six DAP primitive families named in the framing brief's §17 follow-up (`AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`, `ConflictDisclosure`, `MinorityReport`, `DeliberationContext` exercising three of its twelve reference families: `CharterRuleReference`, `PriorDecisionReference`, `AccessibilityNote`) end-to-end against the merged `idea-0019` read-model fixture walk (`ops/ideas/dogfood/institutional-process-substrate-mvp.md`). References `OperatorExecutionAuthority` as the strictly-downstream-of-decision operator handle at the activation gate (Step 5), typed to point at the `DecisionRecord` plus the `ProcessGateResult` set plus the steward's `RoleAssignment`. Emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN, modifies no kernel/runtime/contract/schema/ADR file. Receipt class candidates `FacilitatorSummaryRecordedReceipt`, `ConflictDisclosureAcceptedReceipt`, and `MinorityReportRecordedReceipt` are slice-local candidates only and not committed as canonical. Per `ops/ideas/README.md` § "Dogfood slice variants" and per the DAP framing brief's §16.1, **a read-model fixture walk does NOT satisfy receipt-backed promotion thresholds**; receipt-backed promotion of `idea-0020` to RFC still requires the four DAP §16.1 conditions (runtime dogfood emitting at least one receipt under `ADR-0026`, real visibility/privacy-boundary run, accessibility-gate `ProcessGateResult` on a real surface, and Q1 or Q5 resolved in writing — none of those is started in this sync).
- [x] First runtime dogfood emitting one of the eight named `ProcessTransitionReceipt` classes from the `idea-0019` framing brief: `ProcessGateResultReceipt` (#1755). Emitted by `GovernanceManager::record_process_gate_result` and persisted through the `GovernanceReceiptBackend` trait. **Partial credit toward `idea-0019` (#1748) acceptance gate (a)**; the seven remaining `ProcessTransitionReceipt` classes (`ProcessSessionOpenedReceipt`, `DeliberationEntryRecordedReceipt`, `DecisionRecordedReceipt`, `ActivationCrossedReceipt`, `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`, `EvidencePacketProducedReceipt`) remain candidates. Surfaced a production durability gap: the sled-backed `ReceiptStore` had not yet overridden `put_process_gate_result`, so production callers received an explicit `process_gate_result_backend_not_implemented` sentinel — addressed by the #1757/#1758/#1759 stack.
- [x] Hook tooling fix for scope-guard / todo-guard exec bit + todo-guard pipeline (#1756) — repo tooling only; no runtime / contract / schema / API change.
- [x] Meaning-blind opaque receipt storage primitive on the gateway (#1757) at `icn/crates/icn-gateway/src/receipt_store.rs`. Adds `put_opaque(class, key1, key2_opt, recorded_at, record_hash, payload)` plus `get_latest_opaque` and `list_opaque_for` inherent methods on `ReceiptStore`. Three substantive review findings addressed in `cb9d6daf` (write-once-by-hash on the primary record with stable sentinel `opaque_record_hash_collision`; atomic primary + secondary index writes via single sled transaction; distinct `key2 = None` vs `key2 = Some("")` tag-byte encoding; deterministic `(recorded_at, record_hash)` tie-breaker). One additional codex P2 raised against `cb9d6daf` and addressed in `a8fbb1a6`: new `OPAQUE_HASH_BIND_PREFIX` keyspace binds each `(class, record_hash)` to exactly one canonical `(key1, key2_opt, recorded_at)` tuple at first write; divergent re-binds abort with stable sentinel `opaque_record_hash_index_collision`. Bind, primary, and secondary writes are atomic inside the same sled transaction. Adds zero new typed governance imports on `icn-gateway`; meaning-firewall ratchet unchanged.
- [x] Opaque storage exposed on `GovernanceReceiptBackend` trait (#1758) at `icn/apps/governance/src/receipt_backend.rs`. Three new fail-closed-default trait methods (`put_opaque` / `get_latest_opaque` / `list_opaque_for`) returning the stable sentinel `opaque_storage_not_implemented`. The sled-backed `ReceiptStore` overrides them via thin delegates to its inherent opaque methods. Existing typed test backends are unaffected; opaque methods are only exercised when callers explicitly route through them.
- [x] `ProcessGateResultReceipt` routed through opaque storage cascade (#1759). Trait default for `put_process_gate_result` rewritten to attempt the opaque cascade first (encoding the typed envelope as JSON, calling `put_opaque` with class `"process_gate_result"`, `key1 = session_id`, `key2 = Some(gate_kind)`, the typed `recorded_at` and `record_hash`), and to surface the explicit `process_gate_result_backend_not_implemented` sentinel only when the underlying `put_opaque` itself returns the opaque-not-implemented sentinel. Production gateway-backed `ReceiptStore` therefore now durably persists `ProcessGateResultReceipt` through the opaque cascade. Test-backend coverage: a new `OpaqueOnlyBackend` overrides only `put_opaque` and exercises the typed-default → opaque cascade end-to-end. Test-suite determinism follow-up applied in the same PR (replaced three `std::thread::sleep(Duration::from_millis(1100))` calls with explicit, strictly-increasing `recorded_at` timestamps; suite finishes in 0.01s).
- [x] Sled-open retry-on-`WouldBlock` shipped (#1761). Bounded retry-with-backoff in `SledCommonsStore::open` (`icn/crates/icn-commons/src/store.rs`): 8 attempts max, 500ms total budget cap, 10ms initial backoff, only matches `io::ErrorKind::WouldBlock` so genuine errors (NotFound, PermissionDenied, etc.) are not masked. Two new unit tests pin the new behavior. Single-file change. Closes #1760 (sled 0.34 flusher-thread shutdown race surfaced by #1759's CI Test job; initial actor-drop diagnosis was wrong, corrected pre-merge to sled-flusher-flock-shutdown).
- [x] Truth-sync of opaque receipt storage stack landing (#1762). Records #1755/#1756/#1757/#1758/#1759 in `docs/STATE.md` and `docs/PHASE_PROGRESS.md`; adds `docs/dev/handoff-2026-05-07.md`. Doc/control-plane only.
- [x] Dependabot dev-dependency maintenance — `sdk/typescript/` four updates (#1763) and `web/pilot-ui/` `@axe-core/playwright` 4.11.2 → 4.11.3 (#1735). No runtime change.
- [x] ActionCard contract publication for institution packages (#1764). Adds `docs/contracts/institution-package/action-card.example.json` (fictional `proposal`/`vote` sample card validating against the existing schema with all required fields plus optional `deadline` and `domain_id`; uses fictional ids; contains no NYCN-specific nouns) and `docs/scripts/validate-action-card.py` (draft-2020-12 JSON Schema validator mirroring `validate-preview-review.py` and `validate-rehearsal-evidence.py`). Expands `docs/contracts/institution-package/README.md` with: Files table for the schema + example + validator; stability rationale citing ADR-0027 § Card kind taxonomy ("growable by ADR amendment") explaining why `"x-icn-status": "rfc"` is honest; schema-id-audit linkage documenting the retain-temporarily decision (`docs/contracts/schema-id-audit.md`; review by 2026-06-30 tracked by #1742); explicit CLI validation commands; regulatory-safe vocabulary list (obligation, allocation, settlement, unit, position, receipt, provenance, evidence — explicitly not payment / wallet / balance / currency); explicit "institution-specific semantics belong in institution packages, not in ICN core" guidance; partner-package vendor-or-invoke-from-CI guidance. Updates `docs/registry.toml` with the README's `last_updated` and `last_reviewed` advanced 2026-05-04 → 2026-05-07 and a refreshed description mentioning the new files and the schema-id-audit retention decision. One substantive Copilot review finding addressed pre-merge (CLI arg `packet` -> `card`, `DEFAULT_PACKET` -> `DEFAULT_CARD`, internal vars and error messages aligned). Closes #1713 with all six acceptance criteria met (manually closed after merge with a comment enumerating each gate). No schema fields changed; the schema's `$id` remains DNS-backed under the audit's retain-temporarily decision.
- [ ] Runtime dogfood slice for `idea-0020` emitting at least one receipt under `ADR-0026` for one DAP primitive — receipt-backed; required before promotion to RFC.
- [ ] Visibility/privacy-boundary run with redaction in evidence export for one DAP primitive — required before promotion to RFC.
- [ ] Accessibility-gate `ProcessGateResult` produced through the `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` checklist on a real surface that renders any DAP primitive — required before promotion to RFC.
- [ ] DAP open-question triage on framing-brief Q1 (`AuthorityBasis` polymorphism vs typed family) or Q5 (`ConflictDisclosure`/`MinorityReport` placement) — at least one resolved in writing before promotion to RFC (deferral is not sufficient for the RFC gate per §16.1).
- [ ] Additional `idea-0019` `ProcessTransitionReceipt` classes beyond `ProcessGateResultReceipt` (the first emitted under `ADR-0026` via #1755 + #1759 cascade): `ProcessSessionOpenedReceipt`, `DeliberationEntryRecordedReceipt`, `DecisionRecordedReceipt`, `ActivationCrossedReceipt`, `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`, `EvidencePacketProducedReceipt`. All eligible through the same opaque storage cascade landed in #1757–#1759 — adding a class no longer requires expanding gateway typed governance imports.
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
- Next concrete step: present the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers, as defined in the NYCN rehearsal gate (in the partner repo)
- Subsequent gates: pilot formalization, then first operator rehearsal against real (or fixture-equivalent) organizer material

**Decisions Made:**
- (2026-05-07, post-#1761 / #1762 / #1763 / #1764) The May-7 close-out cycle plus the ActionCard contract publication landed: #1761 closed the surfaced sled-flusher race (#1760) with bounded `WouldBlock` retry in `SledCommonsStore::open`; #1762 truth-synced STATE.md and PHASE_PROGRESS.md for the opaque receipt storage stack landing post-#1755/#1759 and added `docs/dev/handoff-2026-05-07.md`; #1763 / #1735 bumped Dependabot dev-dependency groups (sdk/typescript and web/pilot-ui) with no runtime change; and #1764 published the generic ActionCard contract surface for institution packages by adding a bundled fictional example, a draft-2020-12 validator script, and an expanded README — mirroring the existing convention used by `validate-preview-review.py` and `validate-rehearsal-evidence.py`. #1764 closed #1713 with all six acceptance criteria met (generic schema matches runtime fields; honest `"x-icn-status": "rfc"` marker explained via ADR-0027 § Card kind taxonomy; emitted-vs-gated source kinds distinguished with tracking-issue cites for `signal_rule` → #1631 / #1711 and `obligation_lifecycle` → #1634 / #1712; NYCN-specific nouns absent; regulatory-safe vocabulary preserved; package validation path documented with CLI commands and partner-package guidance). One substantive Copilot review finding addressed pre-merge (CLI arg `packet` -> `card` mechanical rename). The schema's `$id` remains DNS-backed (`https://intercooperative.network/contracts/institution-package/action-card.schema.json`) under the schema-id-audit retain-temporarily decision; review by 2026-06-30 tracked by #1742. `idea-0019` (#1748) acceptance gate (a) status is unchanged from #1762 (still partial — `ProcessGateResultReceipt` emitted via #1755 + durably persisted via the opaque cascade since #1759; additional classes remain candidates); gates (b)-(d) unchanged: not started. #1713 closure is independent of #1748's gates. Phase 2 status remains ⏳ (still partner-bound). Hard rule preserved: this cycle does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet, does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, and does NOT start any Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work. Phase 2 deliverables list extended with five new `[x]` entries (#1761, #1762, #1763, #1735, #1764). One less open follow-up than at the previous sync since #1761 closed #1760. Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves are listed descriptively in `docs/STATE.md` "Current status" paragraph — none is selected here.
- (2026-05-07, post-#1755 / #1756 / #1757 / #1758 / #1759) The opaque receipt storage stack landed: #1755 added the first `ProcessGateResultReceipt` runtime slice (the first runtime dogfood emitting one of the eight named `ProcessTransitionReceipt` classes from the `idea-0019` framing brief); #1756 fixed scope-guard / todo-guard hook tooling; #1757 added the meaning-blind `put_opaque` / `get_latest_opaque` / `list_opaque_for` primitive on the gateway; #1758 extended the `GovernanceReceiptBackend` trait surface and added the sled-backed `ReceiptStore` overrides; #1759 rewrote the `put_process_gate_result` trait default to attempt the opaque cascade first. Production gateway-backed `ReceiptStore` therefore now durably persists `ProcessGateResultReceipt` through the opaque cascade without any new typed governance import on `icn-gateway` — adding a new receipt class becomes a one-file change in apps. New invariant added inside the merge cycle: `OPAQUE_HASH_BIND_PREFIX` keyspace binds each `(class, record_hash)` to exactly one canonical `(key1, key2_opt, recorded_at)` tuple; divergent re-binds abort with stable sentinel `opaque_record_hash_index_collision`; bind/primary/secondary writes atomic inside the same sled transaction. Substantive review findings (codex + Copilot) addressed in `cb9d6daf` and `a8fbb1a6` before merge: write-once-by-hash, atomic primary + secondary, distinct `None` vs `Some("")`, deterministic equal-timestamp ordering, hash-bound canonical index tuple. CI on #1759 surfaced a pre-existing sled 0.34 flusher-thread shutdown race on `test_commons_charter_survives_sled_drop_and_reopen`; filed as issue #1760 (initial actor-drop diagnosis was wrong; corrected to sled-flusher-flock-shutdown) and fix opened as PR #1761 (bounded retry-on-`WouldBlock` in `SledCommonsStore::open`, 8 attempts max, 500ms total budget cap, 10ms initial backoff, only matches `WouldBlock` so genuine errors are not masked). PR #1761 open at sync write-time. `idea-0019` (#1748) acceptance gate (a) — runtime dogfood emitting at least one `ProcessTransitionReceipt` class under `ADR-0026` — is now partially satisfied (one class emitted and durably persisted); gates (b)–(d) (visibility/privacy-boundary run, accessibility-gate `ProcessGateResult` on a real surface, open-question triage on Q1/Q3/Q4) remain unchanged: not started. Phase 2 status remains ⏳ (still partner-bound). Hard rule preserved: this stack does NOT widen gateway typed governance imports; the opaque storage primitive is bytes-in / bytes-out and adds zero new domain types. Meaning-firewall ratchet unchanged: baseline 10 known violations preserved, 0 new. Phase 2 deliverables list extended with five new `[x]` entries (#1755, #1756, #1757, #1758, #1759); the first-class `[ ]` for `idea-0019` runtime dogfood was replaced with an "additional classes" `[ ]` enumerating the seven remaining `ProcessTransitionReceipt` classes. Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves are listed descriptively in `docs/STATE.md` "Current status" paragraph — none is selected here.
- (2026-05-05, post-#1753) The DAP read-model fixture-walk dogfood slice landed in #1753 at `ops/ideas/dogfood/democratic-authority-primitives-mvp.md`, with a matching `ops/ideas/ideas.yaml` row update. Read-model fixture-walk variant per `ops/ideas/README.md` § "Dogfood slice variants" (formalized in #1749). Composes the six DAP primitive families named in the framing brief's §17 follow-up (`AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`, `ConflictDisclosure`, `MinorityReport`, `DeliberationContext` — the latter exercising three of its twelve reference families: `CharterRuleReference`, `PriorDecisionReference`, `AccessibilityNote`) end-to-end against the merged `idea-0019` read-model fixture walk (`ops/ideas/dogfood/institutional-process-substrate-mvp.md`). The slice walks `Step 0` through `Step 7` of the existing `idea-0019` slice without re-describing the spine; only DAP primitive additions are recorded. References `OperatorExecutionAuthority` as the strictly-downstream-of-decision operator handle at the activation gate (Step 5), typed to point at the `DecisionRecord`, the `ProcessGateResult` set, and the steward's `RoleAssignment`. Emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN, modifies no kernel/runtime/contract/schema/ADR file. Receipt class candidates referenced at the right transition points (`FacilitatorSummaryRecordedReceipt`, `ConflictDisclosureAcceptedReceipt`, `MinorityReportRecordedReceipt`) are slice-local class candidates only — the framing brief's §16.1 names a `ConflictDisclosure` accept receipt and a `MinorityReport` recorded receipt generically without attaching concrete class identifiers, and the slice does not commit any of these names as canonical. Per `ops/ideas/README.md` § "Dogfood slice variants" and per the DAP framing brief's §16.1, **a read-model fixture walk does NOT satisfy receipt-backed promotion thresholds**; receipt-backed promotion of `idea-0020` to RFC still requires the four §16.1 conditions enumerated above. Phase 2 status remains ⏳ (still partner-bound). Phase 2 deliverables list extended with one `[x]` entry crediting `idea-0020` read-model fixture-walk dogfood and four `[ ]` entries naming the unstarted runtime dogfood, visibility/privacy-boundary run, accessibility-gate `ProcessGateResult`, and DAP open-question triage. The next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality. Candidate next moves the next session may pick from are listed descriptively in `docs/STATE.md` "Current status" paragraph: (a) DAP runtime dogfood emitting at least one receipt under `ADR-0026` for one DAP primitive — the next artifact called for by the slice's promotion gate; (b) `idea-0019` runtime dogfood toward receipt-backed promotion (one of four #1748 acceptance gates); (c) `idea-0019` visibility/privacy-boundary run; (d) `idea-0019` accessibility-gate `ProcessGateResult`; (e) `idea-0019` open-question triage on Q1/Q3/Q4; (f) one of the DAP §17 follow-up framing briefs (CCL hook-point catalog; expert/advisory across institution types; conflict object model; federation tally semantics; delegation runtime); (g) control-plane cleanup including review-thread hygiene. None is selected here.
- (2026-05-05, post-#1751) Democratic Authority Primitives framing landed in #1751 as `idea-0020` with framing brief at `ops/ideas/framing/democratic-authority-primitives.md` and matching `ops/ideas/ideas.yaml` row. Pre-RFC framing only; no runtime, no schema, no contract URN, no ADR, no RFC, no implementation issue, no runtime dogfood. The brief names two generic primitive families — authority/participation (`AuthorityBasis`, `ParticipationRole`, `DelegationGrant`, `RepresentationMandate`, `ExpertStatement`, `AdvisoryOpinion`, `ConflictDisclosure`, `FacilitatorSummary`, `StewardReview`, `OperatorExecutionAuthority`, `MinorityReport`, `ChallengePath`, `RevocationPath`, `RecallPath`) and deliberation context / educational reference (`DeliberationContext`, `ContextReference`, `LearningReference`, `EvidenceReference`, `PriorDecisionReference`, `CharterRuleReference`, `CCLRuleReference`, `AccessibilityNote`, `PrivacyNote`, `RiskNote`, `CounterargumentReference`, `GlossaryReference`) — that institutions adopt and constrain through CCL, charters, and institution packages. The brief composes orthogonally with `idea-0019` (Institutional Process Substrate): the spine names *what gets processed*; these primitives fill the spine's records with the authority and context typing the spine deliberately deferred. Phase 2 status remains ⏳ (still partner-bound). Phase 2 deliverables list extended with one `[x]` entry crediting `idea-0020` framing. Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality for the next session. Candidate next moves the next session may pick from are listed descriptively in `docs/STATE.md` "Current status" paragraph and include: (1) `idea-0020` read-model composition slice (DAP brief's `[x]` next artifact), (2) `idea-0019` runtime dogfood toward receipt-backed promotion, (3) one of the remaining #1748 process-control gates (visibility/privacy-boundary run, accessibility-gate `ProcessGateResult`, or Q1/Q3/Q4 triage), (4) another sync/control cleanup or one of the DAP §17 follow-up framing briefs (CCL hook-point catalog; expert/advisory across institution types; conflict object model connecting `ConflictDisclosure` to `idea-0016`/ADR-0029; federation tally semantics composing `RepresentationMandate` with #1609; delegation runtime gated on #1632). None is selected here.
- (2026-05-05, post-#1734/#1739/#1741/#1743/#1745/#1747/#1749, with open #1748) The May-5 institutional-process-substrate sequence is documentation/control-plane and idea-refinery only. (a) Five contract/design/architecture docs landed: rehearsal evidence export schema (#1734), architecture due-diligence checklist (#1739), contract schema-identifier audit (#1741), organizer/member accessibility gate definition (#1743), and the preview/review read-model contract `urn:icn:contract:preview-review:v1` (#1745). (b) `idea-0019` Institutional Process Substrate was named in the idea refinery with a framing brief (#1747) and a read-model fixture-walk dogfood slice (#1749) that walks a fictional Example Cooperative process session against the SAME shipping contract URNs as the committed examples without modifying any kernel, runtime, gateway, ledger, governance, or SDK code and without minting any new contract URN. The new `ops/ideas/README.md` § "Dogfood slice variants" convention formalizes that a read-model fixture walk does NOT satisfy receipt-backed promotion thresholds. (c) Coordination/control milestone issue #1748 is open with `epic:arch-invariants` + `type:spec`; no implementation issue is opened from it. (d) Phase 2 status is unchanged; the next concrete human gate remains organizer presentation -> pilot formalization -> first operator rehearsal per the NYCN rehearsal gate (in the partner repo). (e) The next pre-RFC architecture move is **Democratic Authority Primitives** (delegation, representation, expert/advisory input, deliberation context / educational references, conflict disclosure, facilitator and steward/operator authority, and revocation/recall/challenge paths) — generic primitives institutions adopt and constrain through CCL, charters, and institution packages, not ICN app features and not a runtime commitment. Not started in this sync.
- (2026-05-02, post-#1695/#1696/#1697/#1698/#1699/#1700/#1701) May-cycle repo-governance, strategy, dependency/CI maintenance, bootstrap, and state-sync work has merged through #1701. These are truth/control-plane, planning, or maintenance landings only. They do not complete Phase 2, formally commit NYCN as a pilot, claim production readiness, claim live federation integration, implement service hosting, mutate DNS/K3s/GitHub/Forgejo state, implement RFC-0017, handle NYCN private data, or resolve licensing. The next concrete human gate is now explicitly documented in the NYCN rehearsal gate (in the partner repo): organizer presentation, pilot formalization, then first operator rehearsal. Open PR queue was empty at this sync.
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
