# Governance Write Authority Decomposition

**Status:** draft - design/control map
**Truth class:** descriptive
**Canonical:** no - implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-05
**Source basis:** read against `origin/main` at `fbb3f00bb28dea3b6e13f34b194343982f7786d5`
**Related:** #1868, #2061, #2080, #2081, #2337, #2336, #1748, #2141, #2041

> This document is a current-state completion map for the `governance:write` decomposition. It supplements, and does not replace, [`governance-write-decomposition.md`](governance/governance-write-decomposition.md) and [`mandate-gate-design.md`](governance/mandate-gate-design.md). It chooses no runtime behavior in this PR and changes no capability, handler, route, receipt, authorization rule, or token. The design keeps the Meaning Firewall intact: the kernel enforces opaque constraints and scope strings, while governance and gateway apps evaluate institutional authority. A capability is not a mandate. A receipt records evidence and grants zero authority.

## 1. Why this design exists
<!-- truth: descriptive -->

Issue #1868 began from a broad technical gate: `GOVERNANCE_WRITE = "governance:write"` let one bearer scope reach many unrelated governance mutation families. The original decomposition design chose a hybrid path and several implementation slices have landed since then:

- seven class-level governance scope strings exist in `icn-rpc` and the gateway allowlist;
- most governance HTTP mutations accept a class scope first and retain broad `governance:write` as an accepted-also compatibility fallback;
- five governance JSON-RPC methods prefer charter or proposal class scopes and retain the broad fallback;
- `MandateGate`, `MandateRequest`, `MandateGrant`, and `MandateGrantRef` exist;
- v2/v3 governance receipts can bind `capability_scope_presented` and an explicit mandate-attestation posture;
- selected handlers capture the scope actually matched and selected authority-sensitive domain operations use `DefaultMandateGate`.

The work is therefore partially landed, not undecided. The remaining problem is that broad fallback still authorizes every handler in the current inventory, and six newer handlers use the broad scope directly with no class alternative. A token with only `governance:write` still has an unnecessarily large technical blast radius.

This matters directly to the private-access lane established by #2337. A future `AccessReceipt` may cite an opaque authority-basis fingerprint, but it cannot substitute for a clear act-time authority decision. The subject, target, action, scope, validity, and revocation posture must be evaluated before the receipt records the result. Otherwise strong-looking evidence would wrap a weak gate.

## 2. Current state and complete handler inventory
<!-- truth: descriptive -->

### 2.1 Code anchors

- Broad scope: `icn/crates/icn-rpc/src/auth.rs:947`.
- Existing class scopes: `icn/crates/icn-rpc/src/auth.rs:966-986`.
- JSON-RPC accepted-also mapping: `icn/crates/icn-rpc/src/auth.rs:1023-1069`.
- HTTP gates: `icn/apps/governance/src/http/handlers.rs`.
- HTTP route registration: `icn/apps/governance/src/http/configure.rs:573-872`.
- App-side authority resolver: `icn/apps/governance/src/mandate_gate.rs`.
- Entity-aware route authority: `icn/crates/icn-gateway/src/authority.rs` and [RFC-0018](../rfcs/RFC-0018-entity-aware-request-authorization.md).

The inventory below was produced from the live handler source, then matched to route registration. It contains **51 HTTP handlers**: 44 with inline broad scope or broad accepted-also fallback, plus seven federation-proposal handlers gated through `extract_federation_common`.

### 2.2 Evidence notation

The final column uses these short forms:

- **Admission only:** the scope is checked, but this handler does not capture which accepted-also candidate matched for a receipt.
- **Matched-scope receipt:** `require_any_scope_matched` returns the actual matched scope and the downstream typed receipt can bind it.
- **Fact receipt only:** the handler emits a bounded process receipt, but that receipt does not itself establish or grant the recorder's authority.
- **MandateGate:** a real app-side `DefaultMandateGate` resolves actor, domain, act, target, and time before persistence.

### 2.3 Handler table

The proposed authority posture is descriptive and must be frozen by a later implementation/ADR review. `Representation`, `Execution`, and `Attestation` use ADR-0014 vocabulary. `Standing/ownership` means the operation may remain mandate-exempt but still needs class scope plus existing membership or resource-ownership checks.

| Handler/function | Route or operation | Current gate | Mutation family | Proposed authority posture | Proposed next gate | Evidence/receipt implication |
|---|---|---|---|---|---|---|
| `create_domain` | `POST /gov/domains` | charter class or broad fallback | domain creation | Execution | charter class only; label bootstrap/administrative path | Admission only |
| `add_domain_member` | `POST /gov/domains/{domain_id}/members` | charter class or broad fallback | membership mutation | Execution | charter class plus `MandateGate` for production | Admission only |
| `remove_domain_member` | `DELETE /gov/domains/{domain_id}/members` | charter class or broad fallback | membership mutation | Execution | charter class plus `MandateGate` for production | Admission only |
| `activate_charter` | `POST /gov/charters` | charter class or broad fallback | charter activation | Execution | charter class plus `MandateGate`; retain explicit bootstrap labeling | Admission only; shortcut provenance is separate |
| `adopt_domain_policy` | `POST /gov/domains/{domain_id}/domain-policy/adopt` | broad only | domain policy adoption | Execution | charter class plus existing `DefaultMandateGate` | MandateGate already enforced; broad capability remains |
| `declare_institutional_domain` | `POST /gov/domains/{domain_id}/institutional-domain/declare` | broad only | institutional-domain declaration | Execution | charter class plus existing `DefaultMandateGate` | MandateGate already enforced; broad capability remains |
| `create_proposal` | `POST /gov/proposals` | proposal class or broad fallback | proposal lifecycle | Representation | proposal class only plus membership/standing | Admission only |
| `open_proposal` | `POST /gov/proposals/{proposal_id}/open` | proposal class or broad fallback | proposal lifecycle | Representation | proposal class only plus target-domain check | Admission only |
| `close_proposal` | `POST /gov/proposals/{proposal_id}/close` | proposal class or broad fallback | decision finalization | Execution/process authority | proposal class only; preserve process-authorized or grant attestation | Matched-scope receipt; v3 decision receipt binds presented scope |
| `cast_vote` | `POST /gov/proposals/{proposal_id}/vote` | proposal class or broad fallback | voting | Representation | proposal class only plus entity/standing/target checks | Admission only; vote record is not a mandate |
| `create_delegation` | `POST /gov/delegations` | proposal class or broad fallback | vote delegation | Representation | proposal class only plus actor/target binding | Admission only |
| `revoke_delegation` | `DELETE /gov/delegations/{delegation_id}` | proposal class or broad fallback | vote delegation | Representation | proposal class only plus ownership/authority check | Admission only |
| `create_appoint_steward_proposal` | `POST /gov/proposals/sdis/appoint-steward` | proposal class or broad fallback | steward proposal | Representation at proposal; Execution at effect | proposal class only; downstream effect requires mandate/grant | Admission only at proposal creation |
| `create_remove_steward_proposal` | `POST /gov/proposals/sdis/remove-steward` | proposal class or broad fallback | steward proposal | Representation at proposal; Execution at effect | proposal class only; downstream effect requires mandate/grant | Admission only at proposal creation |
| `assign_role` | `POST /gov/structures/{structure_id}/roles` | steward class or broad fallback | direct role assignment | Execution | steward class only plus `MandateGate` or explicit administrative shortcut | Admission only |
| `create_join_federation_proposal` | `POST /gov/proposals/federation/join` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `create_leave_federation_proposal` | `POST /gov/proposals/federation/leave` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `create_establish_clearing_proposal` | `POST /gov/proposals/federation/clearing/establish` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `create_terminate_clearing_proposal` | `POST /gov/proposals/federation/clearing/terminate` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `create_vouch_proposal` | `POST /gov/proposals/federation/vouch` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `create_revoke_vouch_proposal` | `POST /gov/proposals/federation/vouch/revoke` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `create_update_federation_policy_proposal` | `POST /gov/proposals/federation/policy` | federation class or broad fallback via helper | federation proposal | Representation at proposal; Execution at effect | federation class only; mandate at binding effect | Admission only via shared helper |
| `add_comment` | `POST /gov/proposals/{proposal_id}/discussion/comments` | comment class or broad fallback | deliberation comment | Standing/ownership | comment class only plus membership | Admission only |
| `edit_comment` | `PUT /gov/proposals/{proposal_id}/discussion/comments/{comment_id}` | comment class or broad fallback | deliberation comment | Standing/ownership | comment class only plus author ownership | Admission only |
| `delete_comment` | `DELETE /gov/proposals/{proposal_id}/discussion/comments/{comment_id}` | comment class or broad fallback | deliberation comment | Standing/ownership | comment class only plus author ownership | Admission only |
| `add_reaction` | `POST /gov/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions` | comment class or broad fallback | deliberation reaction | Standing/ownership | comment class only plus membership | Admission only |
| `remove_reaction` | `DELETE /gov/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions` | comment class or broad fallback | deliberation reaction | Standing/ownership | comment class only plus actor ownership | Admission only |
| `create_action_item` | `POST /gov/domains/{domain_id}/action-items` | meeting class or broad fallback | action-item record | Execution/standing | meeting class only plus membership | Admission only |
| `update_action_item` | `PUT /gov/domains/{domain_id}/action-items/{item_id}` | meeting class or broad fallback | action-item mutation | Execution/ownership | meeting class only plus ownership; completion keeps receipt path | Matched scope is captured; receipt only on completion transition |
| `delete_action_item` | `DELETE /gov/domains/{domain_id}/action-items/{item_id}` | meeting class or broad fallback | action-item mutation | Execution/ownership | meeting class only plus ownership | Admission only |
| `update_action_item_status` | `PUT /gov/domains/{domain_id}/action-items/{item_id}/status` | meeting class or broad fallback | action-item transition | Execution/attestation | meeting class only plus creator/assignee check | Matched-scope completion receipt when transition requires it |
| `add_action_item_note` | `POST /gov/domains/{domain_id}/action-items/{item_id}/notes` | meeting class or broad fallback | action-item note | Standing/ownership | meeting class only plus membership | Admission only |
| `create_meeting` | `POST /gov/domains/{domain_id}/meetings` | meeting class or broad fallback | meeting record | Execution/standing | meeting class only plus membership | Admission only |
| `start_meeting` | `POST /gov/meetings/{meeting_id}/start` | meeting class or broad fallback | meeting lifecycle | Execution | meeting class only plus target-domain membership | Admission only |
| `end_meeting` | `POST /gov/meetings/{meeting_id}/end` | meeting class or broad fallback | meeting lifecycle | Execution | meeting class only plus target-domain membership | Admission only |
| `add_attendee` | `POST /gov/meetings/{meeting_id}/attendees` | meeting class or broad fallback | attendee roster | Execution/standing | meeting class only plus target-domain membership | Admission only |
| `mark_attendance` | `PUT /gov/meetings/{meeting_id}/attendance` | meeting class or broad fallback | attendance fact | Attestation | meeting class only plus target-domain membership | Matched-scope v2 attendance receipt for receipted transitions |
| `add_agenda_item` | `POST /gov/meetings/{meeting_id}/agenda` | meeting class or broad fallback | agenda mutation | Execution/standing | meeting class only plus target-domain membership | Admission only |
| `update_agenda_item` | `PUT /gov/meetings/{meeting_id}/agenda/{item_id}` | meeting class or broad fallback | agenda mutation | Execution/standing | meeting class only plus target-domain membership | Admission only |
| `create_structure` | `POST /gov/entities/{entity_id}/structures` | activity class or broad fallback | structure record | Execution | activity class only plus entity-aware target check | Admission only |
| `create_activity` | `POST /gov/entities/{entity_id}/activities` | activity class or broad fallback | activity record | Execution | activity class only plus entity-aware target check | Admission only |
| `create_program` | `POST /gov/domains/{domain_id}/programs` | activity class or broad fallback | program record | Execution/standing | activity class only plus membership | Admission only |
| `create_milestone` | `POST /gov/programs/{program_id}/milestones` | activity class or broad fallback | milestone record | Execution/standing | activity class only plus program-domain membership | Admission only |
| `update_milestone_status` | `PATCH /gov/milestones/{milestone_id}` | activity class or broad fallback | milestone transition | Execution | activity class only plus program-domain authority | Admission only |
| `link_activity_to_program` | `PUT /gov/programs/{program_id}/activities/{activity_id}` | activity class or broad fallback | program/activity linkage | Execution | activity class only plus same-domain target checks | Admission only |
| `unlink_activity_from_program` | `DELETE /gov/programs/{program_id}/activities/{activity_id}` | activity class or broad fallback | program/activity linkage | Execution | activity class only plus same-domain target checks | Admission only |
| `update_program_status` | `PATCH /gov/programs/{program_id}/status` | activity class or broad fallback | program transition | Execution | activity class only plus program-domain authority | Admission only |
| `record_process_gate_result` | `POST /gov/domains/{domain_id}/process-sessions/{session_id}/gate-results` | broad only | process fact recording | Attestation | candidate process class plus domain membership and recorder authority | Fact receipt only; receipt grants zero authority |
| `open_process_session` | `POST /gov/domains/{domain_id}/process-sessions/{session_id}/open` | broad only | process session fact | Attestation/Execution boundary | candidate process class plus domain membership and process authority | Fact receipt only; receipt grants zero authority |
| `record_deliberation_entry` | `POST /gov/domains/{domain_id}/process-sessions/{session_id}/deliberation-entries/{entry_id}/record` | broad only | deliberation fact | Attestation | candidate process class plus domain membership and recorder authority | Fact receipt only; no deliberation body stored |
| `record_decision` | `POST /gov/domains/{domain_id}/process-sessions/{session_id}/decisions/{decision_id}/record` | broad only | process decision fact | Attestation | candidate process class plus domain membership and process authority | Fact receipt only; recording is not authority to decide |

The table deliberately excludes read-only handlers whose comments mention `governance:write` but whose implementation does not require it. It also excludes test-only injected claims.

## 3. Design choice
<!-- truth: descriptive -->

**Choose and complete the hybrid path.** This confirms the original #1868 decision against current code rather than reopening it.

1. **Mechanical class scope at the RPC/HTTP boundary.** The technical gate uses a small, bounded class taxonomy. The class scope limits which mutation family a token may attempt. It is opaque to the kernel.
2. **App-side authority evaluation for institutionally consequential acts.** `MandateGate`, domain membership/standing, resource ownership, process authority, and entity-aware subject/target checks decide whether the actor may perform this act on this target now.
3. **Evidence records the basis without becoming the basis.** A receipt may bind the matched class scope and a `MandateGrantRef`, `ProcessAuthorized` posture, or explicit `NoMandateRequired` reason. The receipt does not validate itself into authority.

Pure per-handler scope strings are rejected because 51 route strings would still fail to bind actor, target, domain, time, delegation, or revocation. Pure mandate gating is rejected because a missed app check would leave the broad technical capability as a full bypass. The hybrid gives defense in depth at two different layers.

## 4. Proposed scope taxonomy
<!-- truth: descriptive -->

Seven class strings are landed:

```text
governance:charter:write
governance:proposal:write
governance:steward:write
governance:federation:write
governance:meeting:write
governance:activity:write
governance:comment:write
```

This map proposes one additional class for the four real process-receipt handlers added after the original 45-handler inventory:

```text
governance:process:write
```

It is a candidate until a separate implementation review freezes it. It means technical permission to attempt process-fact recording. It does not mean permission to open a session, speak for a deliberating body, decide, make evidence available, or access private contents. Those remain app-side authority questions.

The two newer constitutional handlers map to the existing charter class:

```text
adopt_domain_policy          -> governance:charter:write
declare_institutional_domain -> governance:charter:write
```

No `governance:evidence:export` or `governance:access:write` scope is proposed here because no corresponding handler is in this inventory. Scope names should follow real enforcement surfaces, not anticipated products.

`governance:write` remains a compatibility fallback today. The completion target is to remove it from production handler candidate lists only after trusted issuance can mint all required class scopes and clients have migrated. Retirement must be measured, tested, and fail-closed, not a flag-day string replacement.

## 5. Mandate-bundle and authority-basis surface
<!-- truth: descriptive -->

The app-side foundation is landed. `MandateRequest` currently carries:

```text
actor
domain
act
target
at
```

`MandateGate::require` validates the actor's active grants, authority class, explicit domain binding, act token, target, mandate status, deadline, and revocation posture. It returns `MandateGrant`, which can be converted to a wire-recordable `MandateGrantRef` carrying:

```text
mandate_id
decision_hash
act
target
granted_at
```

The receipt side already has an explicit `ReceiptMandateAttestation` taxonomy:

```text
Grant { grant_ref }
NoMandateRequired { reason }
ProcessAuthorized
```

This is the correct pattern for future authority evidence. Absence must never ambiguously mean "no mandate required."

For a future `AccessReceipt`, the generic receipt should record only an opaque, deterministic `authority_basis_hash` or a stable reference hash derived from an app-validated basis. The app/gateway decision surface, not the receipt, owns the richer evaluation inputs:

- authenticated actor DID and future `actor_entity_id`;
- target domain/entity and `PrivateObjectRef`;
- requested action and purpose;
- mandate/grant/decision reference where applicable;
- adopted policy and `policy_clause_ref` where applicable;
- typed scope and delegation chain;
- validity, expiry, and revocation posture.

These are not all landed fields on one object. The current `MandateRequest`/`MandateGrantRef` is the implemented mandate slice; `actor_entity_id`, private-object target, policy-clause evidence, and access-purpose binding belong to later #2061/private-access design. This map does not pretend they already exist.

## 6. Relationship to #2061 entity-aware authorization
<!-- truth: descriptive -->

The two lanes answer different halves of one authorization question:

- **#1868 asks what action is technically permitted and what institutional basis must cover it.** Its class scope narrows the action family; its mandate/process/standing posture explains the act-time authority basis.
- **#2061 asks which authenticated actor/entity may perform that action on which target entity.** It owns `EntityId`, membership, standing, hierarchy, delegation, and subject/target binding.

Both must agree before private-object access runtime:

```text
authenticated actor
  + trusted entity binding
  + action-family capability
  + entity-aware subject/target authorization
  + app-side mandate/policy/process authority
  -> allow or deny
  -> then record evidence
```

Flat `coop_id` equality remains the enforced same-namespace baseline on many gateway routes. It is the degenerate same-entity case, not the final delegation model. Federation/community delegation must never be inferred from federation sync, peer trust, route reachability, or receipt propagation.

## 7. Relationship to #2080 and #2081
<!-- truth: descriptive -->

- **#2080 supplies trusted positive issuance.** The system cannot retire broad fallback safely until a trusted source can mint the correct class scope and entity binding from verified membership, invitation, enrollment, or privileged bootstrap state. DID key control and self-assertion are insufficient.
- **#2081 supplies an observe-to-enforce migration precedent.** Treasury currently keeps the flat guard authoritative while entity-aware decisions are measured. Governance class-scope retirement should follow the same discipline: accept both during a bounded compatibility window, record which matched, measure broad-only callers, then enforce the narrower class.

Neither issue is solved by this document. This document also does not copy treasury's entity-action taxonomy into governance. It adopts only the migration discipline.

## 8. What changes in kernel and receipt evidence
<!-- truth: descriptive -->

### Current

- The RPC/kernel-side authorization layer knows `governance:write` and seven opaque class strings.
- Five JSON-RPC methods prefer charter/proposal class strings but accept broad fallback.
- Most HTTP mutations prefer a class scope but accept broad fallback; six direct-only handlers still ask for broad scope.
- Most handlers call `require_any_scope`, so the handler proceeds without retaining which candidate matched.
- `close_proposal`, action-item completion paths, and `mark_attendance` use `require_any_scope_matched` and can bind the presented scope into v2/v3 receipts.
- `adopt_domain_policy` and `declare_institutional_domain` use a real `DefaultMandateGate`, but still require the broad technical scope.
- Process receipts record bounded facts and actor/time/proof links, but their existence does not prove that `governance:write` was an adequate authority basis.

### Completion target

- The technical gate identifies one class scope per mutation family, including a separately reviewed process class if accepted.
- Broad fallback is retired after trusted issuance and compatibility measurement.
- Handlers that produce authority-relevant receipts capture the actual matched scope rather than hardcoding or discarding it.
- Consequential acts carry an app-validated grant/process/policy basis into the evidence chain by stable reference or hash.
- `AccessReceipt` may later cite that basis by fingerprint only after #2061 subject/target authorization and private-object policy enforcement have decided the request.

The kernel still does not parse `MandateAct`, `EntityId` membership meaning, policy clauses, process meaning, or private-object semantics. It enforces opaque strings and constraints. The app supplies the decision and evidence.

## 9. What does not change
<!-- truth: descriptive -->

- No runtime behavior changes in this PR.
- No handler, route, OpenAPI description, or SDK changes.
- No scope constant or gateway allowlist changes.
- No token issuance or enforcement cutover.
- No new receipt class or receipt wire change.
- No `MandateGate` implementation change.
- No entity-aware authorization implementation.
- No `AccessReceipt`, `DisclosureDecisionReceipt`, or `RedactionAppliedReceipt` runtime.
- No vault or encryption implementation.
- No Meaning Firewall widening.
- No authority is granted by a receipt, sync result, trust score, or routing proof.
- No production, pilot, organizer-ready, member-ready, live-federation, NYCN, Phase-2, or #2041 completion claim.

## 10. Implementation sequence after this document
<!-- truth: descriptive -->

1. Land this current-state control map without closing #1868.
2. Open a narrow implementation issue for the new direct-only gaps: map domain-policy adoption and institutional-domain declaration to the existing charter class; decide and, if accepted, mint `governance:process:write` for the four process-recording handlers.
3. Add accepted-also compatibility gates and tests for missing, wrong-class, narrow-only, broad-only, and both-scope tokens. Capture matched scope wherever downstream evidence consumes it.
4. Instrument or audit broad-fallback use so retirement has evidence. Do not infer migration readiness from green tests alone.
5. Extend existing `MandateGate` use to one uncovered high-blast mutation family, with actor, domain, act, target, status, deadline, and revocation tests.
6. Connect the #2061 entity-aware subject/target model for entity- and domain-bound governance writes. Keep existing membership/ownership checks as defense in depth during migration.
7. Wire #2080 trusted issuance for the class and entity claims needed by migrated clients; follow observe, measure, enforce discipline before removing broad fallback.
8. Freeze the stable scope/authority contract in an ADR only after the process-class decision and compatibility evidence are reviewed.
9. Only then open the `AccessReceipt` runtime decision rung. Private access additionally requires artifact registry, `PrivateObjectRef`, scoped-vault enforcement, disclosure policy, enumeration-safe outcomes, and visibility/retention decisions.

The recommended first implementation lane after this docs PR is step 2: close the six direct-only handler gaps without changing the meaning of any handler.

## 11. Non-goals
<!-- truth: descriptive -->

- No runtime changes.
- No Rust receipt class.
- No route, OpenAPI, or SDK change.
- No gateway or authorization implementation.
- No token issuance.
- No enforcement cutover.
- No `AccessReceipt` runtime.
- No `DisclosureDecisionReceipt` runtime.
- No `RedactionAppliedReceipt` runtime.
- No vault implementation.
- No encryption implementation.
- No operator dashboard or member-shell implementation.
- No fixture changes.
- No NYCN package update.
- No icn-learn update.
- No icn-infra update.
- No downstream repository work.
- No production, pilot, organizer-ready, member-ready, live-federation, NYCN, Phase-2, or #2041 completion claim.
- No closure of #1748, #2141, #2041, #1868, #2061, #2080, #2081, or #1907.

## References
<!-- truth: descriptive -->

- [`governance-write-decomposition.md`](governance/governance-write-decomposition.md)
- [`mandate-gate-design.md`](governance/mandate-gate-design.md)
- [`entity-aware-auth-control-map.md`](entity-aware-auth-control-map.md)
- [`made-available-federation-access-boundary-map.md`](made-available-federation-access-boundary-map.md)
- [`access-made-available-disclosure-receipt-decision-rung.md`](access-made-available-disclosure-receipt-decision-rung.md)
- [`ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md)
- [`effect-dispatch-contract.md`](../spec/effect-dispatch-contract.md)
- [`institutional-domain.md`](../spec/institutional-domain.md)
- [`ccl-policy-registry.md`](../spec/ccl-policy-registry.md)
- [RFC-0018](../rfcs/RFC-0018-entity-aware-request-authorization.md)

Refs #1868.
Refs #2061.
Refs #2080.
Refs #2081.
Refs #2337.
Refs #2336.
Refs #1748.
Refs #2141.
Refs #2041.
