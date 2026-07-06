# Governance Broad-Fallback Observability and Retirement Evidence

**Status:** draft - design/control map
**Truth class:** descriptive
**Canonical:** no - implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-06
**Source basis:** read against `origin/main` at `ec6e8c718b1cfa0c37ac49e14823f90d3671ae96`
**Related:** #2341, #1868, #2061, #2080, #2081, #1748, #2141, #2041, #1907

> This document is a control map for a *future* privacy-safe observability capability. It defines nothing that runs. After #2340, every known governance mutation surface gates class-first and retains `governance:write` as an accepted-also compatibility fallback, but the repository has no bounded way to measure which accepted candidate scope callers actually present. This map specifies that measurement, the privacy budget that constrains it, and the evidence that a *separate, later* proposal would need before broad fallback could be retired. It changes no capability, handler, route, receipt, authorization rule, or token. It does not authorize fallback removal. The Meaning Firewall stays intact: the kernel enforces opaque scope strings and constraints; governance and gateway apps evaluate institutional authority. A capability is not a mandate. A receipt records evidence and grants zero authority.

## 1. Purpose and status
<!-- truth: descriptive -->

This is a control map for future observability, not an implementation. Its predecessor, [`governance-write-authority-decomposition.md`](governance-write-authority-decomposition.md), enumerated the governance mutation surface for #1868 and *proposed* both `governance:process:write` for the four process-recording handlers and a charter-class mapping for the two constitutional handlers. Issue #2340 *landed* those six migrations. As a result the surface is now uniformly class-first: no governance mutation handler is broad-only, and none is narrow-only-without-fallback.

That uniformity creates a new, specific evidence gap:

- Authorization tests prove **acceptance behavior**. They show that a class scope is accepted, that the broad `governance:write` fallback is still accepted, and that unrelated or missing scopes are rejected.
- They do **not** prove **migration**. They say nothing about which candidate scope real callers actually present in production — whether trusted issuers and clients have moved to class scopes, or whether most live traffic still leans on the broad fallback.
- Fallback retirement therefore requires more than green acceptance tests. It requires **measured compatibility** (evidence that broad-only use has fallen to an agreed level across every observed surface) **plus** trusted positive class-scope issuance (#2080) **plus** an entity-aware subject/target authorization model (#2061). Removing the fallback before that would either break legitimate callers or narrow the technical gate without the entity-aware decision that should accompany it.

This document specifies the bounded, privacy-safe signal that would close the measurement half of that gap. It is deliberately narrow: it describes *what to observe and how to bound it*, and it stops short of any runtime, metric, or enforcement change. Nothing here is a readiness or completion claim for any protected issue.

## 2. Current class-first surface inventory
<!-- truth: descriptive -->

### 2.1 Code anchors

- Broad scope: `icn/crates/icn-rpc/src/auth.rs:947` (`GOVERNANCE_WRITE = "governance:write"`).
- Eight class scopes: `icn/crates/icn-rpc/src/auth.rs:965-972`; iterable bundle `GOVERNANCE_CLASS_WRITE` at `icn/crates/icn-rpc/src/auth.rs:979-988`.
- JSON-RPC accepted-also mapping: `icn/crates/icn-rpc/src/auth.rs:1055-1069`.
- HTTP gates: `icn/apps/governance/src/http/handlers.rs`.
- Direct-migration regression test: `icn/apps/governance/tests/direct_scope_migration.rs`.
- Gateway scope allowlist: `icn/crates/icn-gateway/src/validation.rs:59-74`; per-coop mint list `icn/crates/icn-gateway/src/validation.rs:838-845`.
- Gateway aliases: `cast_vote_alias` at `icn/crates/icn-gateway/src/api/flow_c.rs:47`; decision-registry `create_meeting` at `icn/crates/icn-gateway/src/api/registry.rs:492`; `index_decision_endpoint` at `icn/crates/icn-gateway/src/api/registry.rs:599`.

### 2.2 Scope vocabulary

Eight class-level governance write scopes are landed (`auth.rs:965-972`), all mirrored into the gateway allowlist and the per-coop mint list:

```text
governance:charter:write
governance:proposal:write
governance:steward:write
governance:federation:write
governance:meeting:write
governance:activity:write
governance:comment:write
governance:process:write
```

`governance:process:write` is the eighth; it landed in #2340. `governance:write` remains as the broad accepted-also fallback on every mutation surface below.

### 2.3 Governance HTTP mutation handlers (51)

The observed HTTP surface is **51 governance mutation handlers**, all class-first with `governance:write` retained as fallback. Forty-four carry an inline `require_any_scope` array of the form `["governance:<class>:write", "governance:write"]`; the seven federation-proposal handlers share a single class gate through `extract_federation_common`. By family:

- **Charter/domain (6, `governance:charter:write`):** `create_domain`, `add_domain_member`, `remove_domain_member`, `activate_charter`, `adopt_domain_policy`, `declare_institutional_domain`. The last two were broad-only before #2340; they now prefer `governance:charter:write` with `governance:write` fallback (`handlers.rs:3481`, `handlers.rs:3622`).
- **Proposal/delegation/steward-proposal (8, `governance:proposal:write`):** `create_proposal`, `open_proposal`, `close_proposal`, `cast_vote`, `create_delegation`, `revoke_delegation`, `create_appoint_steward_proposal`, `create_remove_steward_proposal`.
- **Federation proposal helper (7, `governance:federation:write` via `extract_federation_common`):** `create_join_federation_proposal`, `create_leave_federation_proposal`, `create_establish_clearing_proposal`, `create_terminate_clearing_proposal`, `create_vouch_proposal`, `create_revoke_vouch_proposal`, `create_update_federation_policy_proposal`.
- **Steward direct mutation (1, `governance:steward:write`):** `assign_role`.
- **Comments/reactions (5, `governance:comment:write`):** `add_comment`, `edit_comment`, `delete_comment`, `add_reaction`, `remove_reaction`.
- **Meetings/action items (12, `governance:meeting:write`):** `create_action_item`, `update_action_item`, `delete_action_item`, `update_action_item_status`, `add_action_item_note`, `create_meeting`, `start_meeting`, `end_meeting`, `add_attendee`, `mark_attendance`, `add_agenda_item`, `update_agenda_item`.
- **Activities/programs/structures/milestones (8, `governance:activity:write`):** `create_structure`, `create_activity`, `create_program`, `create_milestone`, `update_milestone_status`, `link_activity_to_program`, `unlink_activity_from_program`, `update_program_status`.
- **Process recording (4, `governance:process:write`):** `record_process_gate_result`, `open_process_session`, `record_deliberation_entry`, `record_decision`. All four were broad-only before #2340; they now prefer `governance:process:write` with `governance:write` fallback (`handlers.rs:3111`, `handlers.rs:3177`, `handlers.rs:3255`, `handlers.rs:3358`).

Class totals: charter 6, proposal 8, federation 7, steward 1, comment 5, meeting 12, activity 8, process 4 = **51**.

### 2.4 Gateway alias surfaces (3)

Three non-app gateway surfaces gate the residual governance routes class-first with broad fallback:

- `cast_vote_alias` (`flow_c.rs:47`) -> `["governance:proposal:write", "governance:write"]`.
- decision-registry `create_meeting` (`registry.rs:492`) -> `["governance:meeting:write", "governance:write"]`.
- `index_decision_endpoint` (`registry.rs:599`) -> `["governance:proposal:write", "governance:write"]`.

### 2.5 Governance JSON-RPC mappings (7) — count corrected from source

The `required_scopes_for_method` mapping (`auth.rs:1055-1069`) gates governance write methods class-first with `governance:write` fallback. Read against live `main`, there are **seven** such methods across three match arms — not five:

- `governance.domain.create` -> `[GOVERNANCE_CHARTER_WRITE, GOVERNANCE_WRITE]`.
- `governance.proposal.create`, `governance.proposal.open`, `governance.proposal.close`, `governance.vote.cast` -> `[GOVERNANCE_PROPOSAL_WRITE, GOVERNANCE_WRITE]`.
- `governance.delegation.create`, `governance.delegation.revoke` -> `[GOVERNANCE_PROPOSAL_WRITE, GOVERNANCE_WRITE]`.

Earlier control-map prose (and the #2341 problem statement) counted **five**: `governance.domain.create` plus the four `#1868 A2b` proposal/vote methods. The live count is **seven** because the two `governance.delegation.create` / `governance.delegation.revoke` writes added under #2113 are also class-first with broad fallback. Observability must cover all seven; a future compatibility report that measured only five would under-report broad-fallback use on the delegation methods. Read-only governance methods (`governance.domain.list/get`, `governance.proposal.list/get`, `governance.delegation.list`) map to `GOVERNANCE_READ` and are out of scope.

### 2.6 Confirmations

- **No broad-only mutation handlers remain.** A search for a single-element `["governance:write"]` gate over `handlers.rs` returns nothing; every mutation gate names a class scope first.
- **No direct-only narrow-scope mutation handlers remain.** Every class-first gate retains `governance:write` as the second accepted-also candidate; a class-only gate with no fallback exists nowhere in this inventory.
- **Read scope is out of scope.** `governance:read` admission handlers are not observed by this map.

## 3. Matched-scope behavior model
<!-- truth: descriptive -->

A future implementation should classify each governance mutation admission into exactly one **candidate-match outcome**, computed at the authorization helper/gate seam, before handler logic runs:

- `class` — accepted by the required class scope (the token presented the class scope; broad fallback was not needed).
- `fallback` — accepted **only** by broad `governance:write` (the token did not present the required class scope).
- `class_preferred` — the token presented **both** the class scope and the broad scope; the class scope is the classified/preferred match.
- `rejected_sibling` — the token presented a *different* governance class scope (e.g. `governance:meeting:write` at a proposal route) but neither the required class nor the broad fallback.
- `rejected_unrelated` — the token presented only unrelated scopes (e.g. `ledger:write`).
- `rejected_missing` — the token presented no usable scope at all.

The three accepted outcomes (`class`, `fallback`, `class_preferred`) map onto the acceptance behavior already exercised by `direct_scope_migration.rs`, which asserts that the four process handlers accept only the process class (plus broad), the two constitutional handlers accept only the charter class (plus broad), all six accept the broad fallback, and unrelated/missing tokens are `FORBIDDEN`. The three rejected outcomes correspond to the test's `FORBIDDEN` cases, refined to distinguish *why* the request was rejected.

Constraints on a future implementation:

- **This PR does not implement this enum.** It names the outcomes so that a later slice can implement them against a fixed vocabulary.
- The matched candidate should be captured at the authorization helper/gate seam (the `require_any_scope` / `require_any_scope_matched` layer), **before** handler logic, and **without** reading token contents or actor/resource identifiers. The observation needs only *which candidate matched*, not *who presented it*.
- Rejected cases must **not** emit allow/fallback signals. A rejection is not a compatibility data point about class-vs-broad acceptance; conflating them would corrupt the retirement evidence.

## 4. Bounded signal schema
<!-- truth: descriptive -->

A future observation must use **stable, closed enums only**. No dimension may carry an open-ended or caller-supplied value. The proposed dimensions:

- **`surface_kind`** — which admission surface produced the observation:
  - `governance_http`
  - `gateway_alias`
  - `json_rpc`
- **`route_family`** — the mutation family, stable and closed:
  - `charter_domain`
  - `proposal_delegation`
  - `federation_proposal_helper`
  - `steward_direct`
  - `comments_reactions`
  - `meetings_action_items`
  - `activities_programs_structures_milestones`
  - `process_recording`
  - `gateway_governance_alias`
  - `json_rpc_governance`
- **`required_class`** — the class scope the surface requires, using the landed scope vocabulary:
  - `charter`
  - `proposal`
  - `steward`
  - `federation`
  - `comment`
  - `meeting`
  - `activity`
  - `process`
- **`match_outcome`** — the §3 candidate-match outcome:
  - `class`
  - `fallback`
  - `class_preferred`
  - `rejected_sibling`
  - `rejected_unrelated`
  - `rejected_missing`
- **`observation_outcome`** — whether the side-band observation itself succeeded:
  - `observed`
  - `observation_failed`

The signal is an **aggregate counter or bounded audit event** keyed on these enums — for example, a count of admissions per `(surface_kind, route_family, required_class, match_outcome)` tuple. The cardinality is the product of small closed sets, not a function of traffic volume, caller population, or resource population. No dimension may be a route ID, a per-user key, a domain ID, a resource ID, or a payload-derived value. Free-form labels are prohibited (see §5).

## 5. Explicit privacy budget
<!-- truth: descriptive -->

The observation is bounded to the §4 enum dimensions and aggregate counts/events. Nothing else may be recorded. A future implementation must **never** record any of the following, in any dimension, label, key, or event field:

- raw token contents;
- token hashes, if they could become tracking identifiers;
- DIDs;
- entity identifiers;
- subject identifiers;
- actor identifiers;
- domain identifiers;
- resource identifiers;
- proposal IDs;
- meeting IDs;
- activity IDs;
- program IDs;
- milestone IDs;
- receipt IDs;
- payloads;
- request bodies;
- deliberation content;
- comments;
- private process data;
- IP addresses;
- user agents;
- arbitrary or caller-supplied labels;
- free-form error strings;
- stack traces;
- any high-cardinality value.

The privacy budget is the closed set of §4 enums and the aggregate counts/events computed over them. If a proposed signal cannot be expressed within that budget, it is out of scope for this control map and must be re-designed, not smuggled in as a label. This preserves the Meaning Firewall at the observability layer: the measurement learns *that a class-vs-fallback admission occurred on a family of routes*, never *who acted on what*.

## 6. Observe-only behavior
<!-- truth: descriptive -->

Observation must not alter authorization or handler outcomes. This is absolute.

A future observation failure must not:

- allow a rejected request;
- reject an otherwise allowed request;
- change response status;
- change response body;
- change route behavior;
- change payload parsing;
- change receipt creation;
- change mandate behavior;
- change membership behavior;
- change manager behavior;
- change persistence behavior.

Observation is **side-band only**. If observation fails — the sink is unavailable, a counter cannot be incremented, an event cannot be emitted — the request must continue exactly as it would have without observation, and the failure is recorded (at most) as `observation_outcome = observation_failed` on the same bounded schema. The authorization decision is computed and enforced independently of whether it was successfully observed. Observation reads the already-computed match outcome; it never participates in computing it.

## 7. Test matrix for future implementation
<!-- truth: descriptive -->

These tests belong to a *future* implementation slice, not to this docs PR. When the matched-scope enum and observation sink are implemented, they should be covered by:

- narrow/class-only scope accepted and classified as `class`;
- broad-only fallback accepted and classified as `fallback`;
- both class and broad presented, accepted and classified as `class` / `class_preferred`;
- sibling class scope rejected, with **no** allow/fallback signal emitted;
- unrelated scope rejected, with **no** allow/fallback signal emitted;
- missing scope rejected, with **no** allow/fallback signal emitted;
- request status and body byte-equivalent to the pre-observation path;
- downstream handler / manager / receipt / mandate behavior equivalent to the pre-observation path;
- emitted dimensions are only the bounded approved enums of §4;
- prohibited identifiers and payload data (§5) are absent from every emitted signal;
- observation-sink failure cannot change the authorization outcome;
- observation-sink failure cannot change the handler outcome.

The existing `direct_scope_migration.rs` acceptance matrix is the correctness anchor for the first three and the three rejection cases: a matched-scope enum must not change any of its asserted `OK` / `FORBIDDEN` results.

## 8. Retirement criteria
<!-- truth: descriptive -->

**#2341 does not authorize fallback removal.** This document defines only the evidence a removal would require; it grants no permission to remove `governance:write` from any handler candidate list.

Any future proposal to retire the broad fallback must, as a separate issue and PR, require all of:

- **measured compatibility evidence across every observed surface** — all 51 HTTP handlers, all three gateway aliases, and all seven JSON-RPC methods, not a sampled subset;
- **a defined observation window** — a stated duration over which compatibility was measured, not a single snapshot;
- **explicit candidate thresholds for acceptable fallback usage**, labeled as *policy requiring approval* unless already defined elsewhere (see the candidate-only example below);
- **trusted positive class-scope issuance deployed and validated** — an explicit dependency on #2080; DID control and self-assertion do not qualify;
- **an entity-aware subject/target authorization model readiness gate** — an explicit dependency on #2061;
- **review of every remaining broad-fallback user** surfaced by the measurement;
- **a separate enforcement issue and PR**, distinct from the observability work;
- **a rollback strategy** for re-accepting the broad fallback if migration proves incomplete;
- **a protected-issue audit** confirming no protected issue is closed or overclaimed by the change;
- **no readiness or completion overclaims** in the process.

The thresholds are **not** set here. As a *candidate* illustration only — not accepted policy — a removal proposal might argue for "zero unexpected broad-fallback admissions observed across all surfaces for N consecutive days." The value of N, the definition of "unexpected," and whether zero is the right target are policy questions for a separate approval, not facts this map establishes.

## 9. Future implementation boundary
<!-- truth: descriptive -->

The work should proceed as independently reviewable, reversible slices:

1. **Docs/control-map only.** This PR.
2. **Matched-scope internal enum/return type**, with no metric emission — the authorization seam returns the §3 outcome; nothing observes it yet.
3. **Bounded in-memory / test observation sink** — the outcome is captured behind an interface exercised only by tests.
4. **Privacy-safe aggregate metric/audit emission** — the §4 schema is emitted to a real sink under the §5 budget and §6 observe-only guarantee.
5. **Compatibility report tooling** — a read-only report over the emitted aggregates across every surface.
6. **Separate fallback-removal proposal** — only after the #2080 and #2061 prerequisites and measured compatibility evidence of §8, as its own issue and PR.

Each slice must be reversible and independently reviewable. No slice past step 1 is authorized by #2341.

## 10. Dependencies and non-goals
<!-- truth: descriptive -->

Dependencies and context:

- **#1868** is the authority-decomposition owner and context for this map.
- **#2061** owns the entity-aware subject/target authorization model that fallback retirement depends on.
- **#2080** owns the trusted positive class-scope issuance that fallback retirement depends on.
- **#2081** is the observe/measure/enforce migration precedent: treasury keeps its existing guard authoritative while entity-aware decisions are measured, then enforces. Governance broad-fallback retirement should follow the same discipline — accept both during a bounded window, measure which candidate matched, then narrow — and this map adopts only that discipline, not treasury's action taxonomy.
- **#1748**, **#2141**, **#2041**, and **#1907** are protected open issues. This document makes no claim about their state and must not be read as advancing or closing any of them.

Non-goals (repeated for the record):

- No `governance:write` fallback removal.
- No runtime observability implementation.
- No trusted issuance.
- No entity-aware authorization cutover.
- No `AccessReceipt` runtime.
- No new receipt class.
- No mandate redesign, and no new mandate act or target.
- No route, OpenAPI, or SDK expansion.
- No vault implementation.
- No encryption implementation.
- No provider-import work.
- No NYCN package work.
- No `icn-learn` work.
- No `icn-infra` work.
- No production, pilot, organizer-ready, member-ready, live-federation, NYCN, Phase-2, or #2041 completion claim.

## References
<!-- truth: descriptive -->

- [`governance-write-authority-decomposition.md`](governance-write-authority-decomposition.md)
- [`governance-write-decomposition.md`](governance/governance-write-decomposition.md)
- [`mandate-gate-design.md`](governance/mandate-gate-design.md)
- [`entity-aware-auth-control-map.md`](entity-aware-auth-control-map.md)
- [`ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md)
- [RFC-0018](../rfcs/RFC-0018-entity-aware-request-authorization.md)

Refs #2341.
Refs #1868.
Refs #2061.
Refs #2080.
Refs #2081.
Refs #1748.
Refs #2141.
Refs #2041.
Refs #1907.
