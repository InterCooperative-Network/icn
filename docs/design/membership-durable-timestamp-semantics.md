# Membership durable timestamp semantics — Design/Audit Contract

**Status**: draft — design / audit contract (not runtime implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-03
**Source basis**: read against `main` @ `e8057cc6` (the #2284 merge commit). Code anchors were verified at that commit — re-verify before relying on exact line numbers.
**Related**: issue #2286 (durable Member records persist node-local timestamps — the tracker this contract serves) · issue #2283 (closed — the `state_change_hash` determinism flake) · PR #2284 (merged — the narrow fingerprint fix) · `icn/crates/icn-core/tests/federated_two_node_pilot.rs` (the two-node determinism intent) · issue #282 (deprecated proposal-level delayed execution, cited below)

> Narrow decision document for #2286 only. It decides the *target semantics*
> for the durable membership timestamp fields after #2284 split the world
> into a deterministic decision-identity fingerprint and a still-divergent
> durable record. It changes nothing at runtime: no schema change, no
> effect change, no request change, no test change. The implementation is a
> future PR under #2286.

## 1. Status / truth class

- **Status**: draft design/audit contract.
- **Truth class**: descriptive — this document describes verified current
  behavior and pins a target contract; it does not claim any of the target
  behavior exists yet.
- Canonical current-implementation truth remains `docs/STATE.md`,
  `docs/PHASE_PROGRESS.md`, and live code.
- Related: #2284, #2283, #2286, `federated_two_node_pilot`.

## 2. Current implementation audit (post-#2284)

All anchors verified at `main` @ `e8057cc6`.

### 2.1 The fingerprint is deterministic

`MembershipServiceImpl::compute_state_change_hash`
(`icn/crates/icn-core/src/services/membership_service.rs:108`) is now a
**decision-identity fingerprint**: SHA-256 over the domain tag
`membership:v2:` followed by four length-prefixed (u64 LE) fields —
`operation`, `entity_id`, `member_did`, `decision_receipt_id`. It hashes
only decision-derived inputs and contains no wall-clock read. Its own doc
comment (same file, above the function) states the contract explicitly:
cross-node deterministic, injective via length prefixes, and — quoting the
part that motivates this document —

> the persisted `Member` record still carries node-local wall-clock audit
> fields (`joined_at` on add; `removed_at`/`frozen_at`/`freeze_expires_at`/
> `unfrozen_at` metadata on the other ops), so the durable record is not
> itself byte-identical across nodes. […] Making the durable record fully
> deterministic requires a decision-carried `effective_at` threaded through
> `MembershipEffect` (a schema change) — tracked in issue #2286.

### 2.2 The durable record is not deterministic

Every membership execution path still reads node-local wall-clock and
persists it into the durable `Member` record
(`icn/crates/icn-coop/src/types.rs:353` — `joined_at: DateTime<Utc>` is a
first-class field; the other timestamps live in the string-keyed
`metadata: HashMap<String, String>`):

| Operation | Wall-clock read | Durable write |
|---|---|---|
| add | `Member::new` → `joined_at: Utc::now()` (`icn-coop/src/types.rs:721`) | `joined_at` field |
| remove | `current_timestamp_secs()` (`membership_service.rs:262`) | `metadata["removed_at"]` (`:286`) |
| freeze | `current_timestamp_secs()` (`membership_service.rs:469`) | `metadata["frozen_at"]` (`:507`); `metadata["freeze_expires_at"] = timestamp + duration_secs` (`:523–:527`) |
| unfreeze | `current_timestamp_secs()` (`membership_service.rs:602`) | `metadata["unfrozen_at"]` (`:635`); removes `freeze_expires_at` (`:636`) |
| update | `current_timestamp_secs()` (`membership_service.rs:371`) | **none** — the read feeds only in-memory provenance |

Two nodes replaying the same governance decision across a seconds boundary
therefore persist different durable bytes, even though their
`state_change_hash` values now agree.

### 2.3 Three distinct things, kept distinct

The audit confirms three separable notions that this contract must not
conflate:

1. **Decision-identity fingerprint convergence** — `state_change_hash`
   (`membership:v2:` layout). Cross-node deterministic as of #2284. Its
   layout is pinned; this contract does not touch it.
2. **Durable `Member` record byte convergence** — NOT currently guaranteed;
   the subject of #2286 and of this contract.
3. **Node-local audit metadata** — `MembershipProvenance`
   (`membership_service.rs:36`), an in-memory `RwLock<HashMap>` holding a
   per-execution `timestamp`. It is not durable, not replicated, and not
   part of any convergence claim. It may legitimately stay node-local.

### 2.4 The effect and request schemas carry no timestamp

- `MembershipEffect` (`icn/crates/icn-kernel-api/src/effects.rs:191`) —
  `AddMember`/`RemoveMember`/`UpdateMember`/`FreezeMember`/`UnfreezeMember`
  — carries no `effective_at`. `FreezeMember` carries the decision-derived
  `duration_secs: Option<u64>`. The `decision_hash` fields use
  `#[serde(default)]`, the crate's existing add-a-field compatibility
  precedent.
- The service request structs (`icn/crates/icn-kernel-api/src/services.rs:1528`
  onward: `AddMemberRequest`, `RemoveMemberRequest`, `UpdateMemberRequest`,
  `FreezeMemberRequest`, `UnfreezeMemberRequest`) carry
  `decision_receipt_id` + `decision_hash` but no timestamp.
- `KernelMembershipExecutor::execute_membership_operation`
  (`icn/crates/icn-core/src/supervisor/governance_executor.rs:1905`)
  converts effect → request 1:1; membership effects are produced in
  `icn/apps/governance/src/handlers/execution.rs` (`:594`/`:603` add/remove,
  `:108`/`:119` freeze/unfreeze).

### 2.5 The protocol precedent exists — with one honest caveat

`ProtocolEffect::SetParameter` (`icn/crates/icn-kernel-api/src/effects.rs:249`)
carries `effective_at: u64`, and `KernelProtocolExecutor::apply_protocol_change`
(`governance_executor.rs:1038`) does exactly what this contract wants for
membership: it persists the **decision-carried** timestamp into the durable
record (`updated_param.updated_at = change.effective_at`) and its
`compute_state_change_hash` (`:1017`) hashes `effective_at`, never a `now()`
read. The two-node test (`federated_two_node_pilot.rs:539`) samples
`effective_at` **once** and carries the same value to both nodes — the
replication pattern in miniature.

**Caveat**: the production value is currently degenerate. The producer sets
`effective_at: proposal.effective_at.unwrap_or(0)`
(`icn/apps/governance/src/handlers/execution.rs:160`), and
`ProtocolChangeProposal::effective_at` is deprecated/must-be-`None` pending
delayed execution (#282) — so real protocol changes carry `0` today. That
is still cross-node deterministic (every node sees the same `0`), which is
the property being mirrored; but it means the precedent proves the
*architecture* (decision-carried, sampled at most once, replicated as
bytes), not a production-quality *source* for the timestamp value. The
membership implementation must pick its source deliberately (§5).

### 2.6 What the existing test does and does not check

`test_two_node_membership_add_determinism`
(`icn/crates/icn-core/tests/federated_two_node_pilot.rs:287`) executes the
same serialized `MembershipEffect::AddMember` on two independent nodes and
compares only the extracted `state_change_hash` (plus an `is_member`
liveness check). It never compares durable `Member` records — which is why
#2284 could make it deterministic without touching the durable divergence.

## 3. Problem

Post-#2284, the convergence story is split: the *fingerprint* of a
membership state change is cross-node deterministic, but the *durable state*
it fingerprints is not, because five timestamp fields are read from local
wall-clock at execution time. This matters because:

- The `federated_two_node_pilot` intent is replicated-state determinism:
  same decision in, same state out. A durable record that differs by
  execution instant silently breaks any future durable-state convergence
  check, state sync comparison, or byte-level audit across nodes.
- The AGENTS.md **Determinism** invariant ("Protocol state transitions …
  must be deterministic. Same inputs → same outputs") is currently satisfied
  only for the hash, not for the state the hash stands in front of.
- Freeze expiry is *behavior*, not just bookkeeping: `freeze_expires_at`
  derives from the local read, so two nodes can disagree about **when a
  member's suspension ends** — a real divergence in effective membership
  state, not merely in audit trivia.

It was correct not to fold this into #2284: fixing it requires a schema
change to `MembershipEffect` and the request structs plus changes to every
effect producer — a cross-cutting change with compatibility consequences,
exactly what a one-function flake fix must not smuggle in. #2284's merge
kept that boundary explicit; this contract exists so the boundary is
decided deliberately rather than re-litigated inside an implementation
patch.

## 4. Decision

**Deterministic durable timestamps via a decision-carried `effective_at`
are the target semantics** (option 1 of #2286). The audit found no
architectural contradiction — the protocol executor already persists a
decision-carried timestamp into its durable record, and `MembershipEffect`'s
`#[serde(default)] decision_hash` fields show the effect schema is designed
to grow decision-derived fields.

The contract:

1. **Membership state treated as durable protocol state must be
   replay-deterministic across nodes.** Two nodes executing the same
   governance decision must persist byte-identical durable membership
   convergence state — including its timestamps.
2. **Local audit timestamps may still exist, but are separated from
   protocol/durable convergence state.** The in-memory
   `MembershipProvenance.timestamp` stays node-local audit metadata. If any
   node-local execution instant is ever persisted, it must live in a
   clearly-labeled audit location excluded from every convergence
   comparison — never in the fields below.
3. **The future implementation threads `effective_at` through membership
   effects/requests and uses it for the durable fields**: `joined_at`,
   `removed_at`, `frozen_at`, `freeze_expires_at`, and `unfrozen_at`. No
   durable membership timestamp is sourced from `Utc::now()` /
   `current_timestamp_secs()` on the replay path.
4. **The `state_change_hash` `membership:v2:` layout is unchanged by this
   contract.** It is already deterministic and pinned by #2284; whether
   `effective_at` should ever join the fingerprint would be a separate
   explicit `membership:v3:` decision, which this document neither makes
   nor recommends.

## 5. Future implementation sketch (no code in this PR)

Likely surfaces, in dependency order:

- **`MembershipEffect`** (`icn-kernel-api/src/effects.rs`): add
  `effective_at` to the variants that persist timestamps (`AddMember`,
  `RemoveMember`, `FreezeMember`, `UnfreezeMember`). Compatibility posture
  for old serialized effects is §6's decision.
- **Service request structs** (`icn-kernel-api/src/services.rs`): mirror
  the field on `AddMemberRequest`, `RemoveMemberRequest`,
  `FreezeMemberRequest`, `UnfreezeMemberRequest`.
- **Effect producers** (`icn/apps/governance/src/handlers/execution.rs`):
  stamp `effective_at` from a decision-derived source, sampled **at most
  once per decision** before fan-out, so every node receives identical
  bytes. The source is an open sub-decision for the implementation design:
  candidate anchors are the decision receipt's recorded timestamp, an
  activation-time value, or an explicit proposal-carried field. The
  invariant is source-independence per node — never a per-node `now()`.
  (The protocol lane's deprecated proposal-level `effective_at`, #282, is a
  cautionary sibling: don't resurrect that field accidentally; pick a
  source that exists on the decided path.)
- **Governance executor** (`KernelMembershipExecutor`): pass the field
  through effect → request conversion.
- **Membership service** (`MembershipServiceImpl`): use `effective_at` for
  `joined_at` (this requires the add path to stop relying on
  `Member::new`'s internal `Utc::now()` — e.g. setting `joined_at`
  explicitly after construction, leaving `Member::new` itself untouched for
  non-governance callers, or a deliberate constructor-signature decision in
  the implementation PR), `removed_at`, `frozen_at`, `freeze_expires_at`,
  `unfrozen_at`.
- **Tests**: `federated_two_node_pilot.rs` grows a durable-record
  comparison (§7).
- **OpenAPI/SDK**: expected **none**, but the implementation PR must
  verify. The gateway's `AddMemberRequest`
  (`icn-gateway/src/models.rs:107`) is a different, non-governance shape
  (`did`/`role`/`display_name`) — the kernel-api request structs are not
  the OpenAPI surface. If any effect/request shape turns out to be exposed,
  the standard drift chain applies (OpenAPI regen + TS types).

**Freeze expiration becomes deterministic for free**: `duration_secs` is
already decision-carried on `FreezeMember`, so
`freeze_expires_at = effective_at + duration_secs` is a pure function of
decision inputs. (Use saturating arithmetic; both are u64 seconds.)

**Update-member needs no durable timestamp.** The audit (§2.2) shows the
update path persists no timestamp today — its wall-clock read feeds only
the in-memory provenance map. There is nothing to make deterministic, and
adding a new durable `updated_at` would be new surface, not determinism
repair. `UpdateMember`/`UpdateMemberRequest` therefore do not need
`effective_at` unless a durable update timestamp is deliberately introduced
later — a separate decision.

**Adjacent, explicitly out of scope**: `icn-entity/src/membership.rs`
(`joined_at: now` at `:99`, `updated_at` bumps throughout) shows the same
wall-clock pattern on the entity-level membership record. It is a different
store behind a different lane; this contract covers only the
`MembershipService`/`CoopStore` path named by #2286. If entity-level
durable convergence is ever claimed, that lane needs its own audit.

## 6. Compatibility and migration questions

Existing serialized `MembershipEffect` values (in stores, WALs, gossip
replay, or fixtures) lack `effective_at`. The implementation PR must decide
— explicitly, in its design section — one of:

1. **Fail closed**: replayed governance effects without `effective_at` are
   rejected on the convergence path. Strongest determinism; requires
   knowing whether any pre-schema effects legitimately still replay.
2. **Versioned effect shapes**: a v2 membership effect (mirroring how the
   hash layout was versioned `membership:v2:`), with v1 effects handled by
   an explicit legacy path.
3. **Legacy local-timestamp behavior confined to clearly non-convergent
   legacy paths**: old effects keep old behavior, but no convergence claim
   is ever made over records they produced.

What the implementation must **not** do is silently default the field —
`#[serde(default)]` on `effective_at` would stamp epoch-zero (or
worse, a hidden `now()`) into durable records and smuggle the compatibility
decision into a serde attribute. The `decision_hash` `#[serde(default)]`
precedent is not automatically transferable: an empty-string hash degrades
provenance, but a defaulted timestamp silently *fabricates* state. This
document deliberately does not pick among the three options; it only
requires that the choice be explicit, written down, and tested.

## 7. Test obligations for the implementation PR

The implementation PR is expected to carry at least:

- **Two-node durable-record comparison**: extend the
  `federated_two_node_pilot` membership tests to compare durable `Member`
  records (or a normalized durable-state projection that includes the
  timestamp fields) across both nodes — not just `state_change_hash`.
- **Decision-carried persistence per operation**: add/remove/freeze/
  unfreeze each persist exactly the decision-carried timestamp
  (`joined_at`/`removed_at`/`frozen_at`/`unfrozen_at` equal `effective_at`,
  not a node-local read).
- **Deterministic freeze expiration**:
  `freeze_expires_at == effective_at + duration_secs` on every node.
- **Audit/convergence separation**: if a local audit timestamp is retained
  anywhere durable, a test proves it is excluded from the durable
  convergence comparison.
- **Legacy behavior is explicit**: whichever §6 option is chosen, a test
  pins it (missing `effective_at` → rejected, or versioned path taken, or
  legacy path marked non-convergent).
- **Fingerprint stability**: `state_change_hash` for the existing
  `membership:v2:` layout is byte-for-byte unchanged by the implementation
  (regression against the #2284 contract).

## 8. Non-claims

- **No schema change in this PR** — `MembershipEffect`, the request
  structs, and `Member` are untouched.
- **No runtime behavior change** — every timestamp is still read from
  local wall-clock exactly as audited in §2.
- **No OpenAPI/SDK change.**
- **No production, pilot, organizer-ready, member-ready, live-federation,
  or Phase 2 completion claim.**
- **No #1748/#2141 closure** — both remain open; this lane is adjacent to,
  not part of, the process-receipt spine.
- **No #2081/#2080/#2274 progress.**
- **No durable timestamp implementation yet** — this document is the
  decision, not the change.

## 9. Follow-up implementation issue / PR plan

**#2286 remains open as the implementation tracker.** No child issue is
proposed: the audit surfaced exactly one lane of work (thread
`effective_at` through effect → request → executor → service, plus the §6
compatibility choice and §7 tests), which fits a single design-first
implementation PR referencing this contract. The one sub-decision that
could justify a split — the *source* of `effective_at` on the deciding path
(§5) — is small enough to be pinned in the implementation PR's design
section; split it out only if it turns out to be contested.
