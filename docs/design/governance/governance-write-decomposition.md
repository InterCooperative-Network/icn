# Governance:Write Decomposition — Design

**Status**: Design draft (Phase A — design only; no code change in this PR)
**Last Updated**: 2026-05-18
**Issue**: #1868
**Doctrine source**: [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) §4.1, §7

This document picks the decomposition path for the single broad
`governance:write` capability, lists every handler currently gated by it,
and names the per-action scope strings and the mandate-binding surface that
follow-up PRs will land. It is design only; no runtime behavior changes
here.

## 0. Non-claims

<!-- truth: descriptive -->

- This is a design proposal. It does not change runtime behavior.
- It does not claim production readiness, live federation readiness, or
  NYCN/partner activation.
- It does not widen the meaning firewall. The kernel still enforces strings
  blindly. The mandate-binding surface lives in apps, not in the kernel.
- It does not introduce or alter regulatory terminology. The vocabulary
  (obligation / allocation / settlement / unit / position / receipt /
  provenance) is unchanged.
- It does not commit any handler to a specific implementation. The map in
  §6 is the proposed binding; per-handler scope and mandate work lands in
  named follow-up PRs (§10).
- It does not decide whether an ADR is required. §11 names the trigger.

## 1. Problem

<!-- truth: descriptive -->

The kernel-side capability scope `GOVERNANCE_WRITE = "governance:write"`
(`icn/crates/icn-rpc/src/auth.rs:947`) is the single string required by 45
distinct governance mutation handlers in
`icn/apps/governance/src/http/handlers.rs`. Any holder of a
`governance:write`-bearing capability call any of them: domain creation,
charter activation, membership mutation, proposal lifecycle transitions,
federation-treaty proposals, mandate operations, meeting lifecycle,
action items, comments, reactions.

The kernel cannot distinguish these acts. No mandate-bundle layer above
the capability sits between bearer and effect. Whether the long-term fix
is per-action capabilities (kernel-side decomposition) or mandate-bundle
gating (app-side enforcement) is the open question recorded in
`ABUSE_CASE_HARDENING_STRATEGY.md` §4.1.

The original issue text undercounted the gated handlers as roughly 12;
a fresh enumeration finds **45** — 38 with inline `require_scope` calls,
plus 7 federation-proposal handlers (`create_join_federation_proposal`,
`create_leave_federation_proposal`, `create_establish_clearing_proposal`,
`create_terminate_clearing_proposal`, `create_vouch_proposal`,
`create_revoke_vouch_proposal`, `create_update_federation_policy_proposal`)
that route through the `extract_federation_common` helper
(`handlers.rs:277`), whose own `require_scope` call (line 283) gates the
entire family.

## 2. Doctrine

<!-- truth: normative -->

From `ABUSE_CASE_HARDENING_STRATEGY.md`:

- §2.7 — *A token is not a mandate.* A capability scope describes what the
  kernel will allow the bearer to attempt; it does not describe an
  institutional authorization.
- §4.1 — *A capability scope must describe an act the institution can
  authorize, not a directory of routes the gateway can dispatch.*
- §4.1 — *The kernel-side enforcement primitive is the capability; the
  institutional gate that produces the capability is the mandate. The
  split is where the institution authorizes versus how the kernel
  enforces.*
- §7 — `governance:write` is listed as an authority shortcut whenever it
  is used to exercise the bootstrap-only paths (`add_domain_member`,
  `remove_domain_member`, `activate_charter`).

The doctrine does not pit scope decomposition and mandate gating against
each other. They are complementary: scope is what the kernel sees;
mandate is what the institution proves.

## 3. Current handler inventory

<!-- truth: descriptive -->

Enumerated from `icn/apps/governance/src/http/handlers.rs` by two passes:
(1) walking each `require_scope::<BasicClaims>(_, "governance:write")`
call site to its enclosing handler, and (2) finding every public handler
that calls `extract_federation_common` (`handlers.rs:277`), which itself
holds a `require_scope` at line 283. The second pass is necessary because
federation-proposal handlers delegate their scope check to that helper
and therefore have no inline `require_scope` line of their own. **45**
unique handlers in total — 38 inline + 7 via the helper.

| Group | Handlers |
|---|---|
| **Charter / domain lifecycle** | `create_domain`, `activate_charter`, `add_domain_member`, `remove_domain_member` |
| **Proposal lifecycle** | `create_proposal`, `open_proposal`, `close_proposal`, `cast_vote` |
| **Steward proposals** | `create_appoint_steward_proposal`, `create_remove_steward_proposal`, `assign_role` |
| **Delegation** | `create_delegation`, `revoke_delegation` |
| **Federation-treaty proposals** (gated via `extract_federation_common`) | `create_join_federation_proposal`, `create_leave_federation_proposal`, `create_establish_clearing_proposal`, `create_terminate_clearing_proposal`, `create_vouch_proposal`, `create_revoke_vouch_proposal`, `create_update_federation_policy_proposal` |
| **Meeting lifecycle** | `create_meeting`, `start_meeting`, `end_meeting`, `add_agenda_item`, `update_agenda_item`, `add_attendee`, `mark_attendance` |
| **Action items** | `create_action_item`, `update_action_item`, `update_action_item_status`, `delete_action_item`, `add_action_item_note` |
| **Activities / programs / structures / milestones** | `create_activity`, `create_program`, `create_structure`, `create_milestone`, `update_milestone_status`, `update_program_status`, `link_activity_to_program`, `unlink_activity_from_program` |
| **Comments / reactions (low-stakes social)** | `add_comment`, `edit_comment`, `delete_comment`, `add_reaction`, `remove_reaction` |

Blast-radius classification:

- **High blast radius** — alters who governs or what authority exists:
  `create_domain`, `activate_charter`, `add_domain_member`,
  `remove_domain_member`, `assign_role`, and the federation-treaty
  proposal handlers (cross-cooperative authority).
- **Medium blast radius** — alters decisions, mandates, or delegations:
  proposal lifecycle, steward proposals, delegation, milestone status.
- **Low blast radius** — alters institutional record but not authority:
  meeting lifecycle, action items, activities/programs/structures.
- **Very low blast radius** — social interaction overlay: comments,
  reactions.

### 3.1 Additional `governance:write` surfaces outside `apps/governance`

A workspace-wide search for `"governance:write"` finds 74 references
total. The 45 handlers in `apps/governance/src/http/handlers.rs` are
the bulk of those, but three more *gateway* surfaces and one *JSON-RPC*
surface also gate on `governance:write` and must be migrated (or
explicitly scoped out) before the retirement step in §10 can run:

- `icn/crates/icn-gateway/src/api/flow_c.rs:52` — `cast_vote_alias`
  (REST alias for `cast_vote`) → maps to `governance:proposal:write`.
- `icn/crates/icn-gateway/src/api/registry.rs:497` — `create_meeting`
  (decision-registry meeting create) → maps to
  `governance:meeting:write`.
- `icn/crates/icn-gateway/src/api/registry.rs:593` —
  `index_decision_endpoint` (decision-registry indexing) → likely maps
  to `governance:proposal:write` (it records a decision); confirm with
  the registry owner during migration.
- `icn/crates/icn-rpc/src/auth.rs:1009-1014` —
  `required_scope_for_method` maps the JSON-RPC methods
  `governance.domain.create` → `governance:charter:write`;
  `governance.proposal.create`, `governance.proposal.open`,
  `governance.proposal.close`, `governance.vote.cast` →
  `governance:proposal:write`. The mapping table is the migration
  point for all five JSON-RPC methods at once.

The retirement step in §10 cannot succeed until each of these is
migrated or explicitly scoped out. None of them are in the original
45-handler inventory above because they live outside
`apps/governance/src/http/handlers.rs`.

## 4. The three paths

<!-- truth: descriptive -->

### 4.1 Per-action capability scopes (pure kernel decomposition)

Each handler gets its own scope string. Kernel sees finer evidence.

- *Pro.* Kernel evidence is precise; tokens describe specific acts.
- *Pro.* Mechanically simple — a single-line const plus a per-handler
  string change.
- *Con.* Scope explosion. 45 handlers means 45 scopes, with churn every
  time a handler is added.
- *Con.* A capability holder still has unbounded authority *within* the
  chosen scope. `governance:proposal:cast_vote` lets the bearer vote on
  every proposal, in every domain, indefinitely. The kernel cannot bind
  the capability to a target, a time window, or a role.
- *Con.* Capability ≠ mandate. Per §2.7, this path alone does not produce
  a governance gate; it produces a finer-grained authentication gate.

### 4.2 Mandate-bundle gating (pure app-side enforcement)

Keep `governance:write` as the kernel capability. Add an app-layer policy
oracle that consults a mandate registry: "does this actor hold a ratified
mandate authorizing this act on this target at this height?"

- *Pro.* This is the ICN-native path. The mandate is the governance
  artifact; the capability is just the kernel's pre-authentication.
- *Pro.* The kernel learns nothing new. The meaning firewall stays
  exactly where it is.
- *Pro.* Per-target, per-time, per-role binding fall out of the mandate
  shape; the kernel does not have to model any of it.
- *Con.* The kernel-level capability remains broad. Capability leakage
  still grants broad write authority *if the mandate-check is bypassed by
  a bug*. Defense in depth is weaker.
- *Con.* Mandate machinery does not exist yet in the form this gating
  requires. It is design and implementation work, not a flag flip.

### 4.3 Hybrid — narrow capability classes + mandate gate

Decompose `governance:write` into a small, finite set of **class-level**
scopes (5–7), each of which still requires a mandate for high- and
medium-blast-radius acts; low-blast-radius acts may not require a mandate
beyond ordinary membership.

- *Pro.* Kernel evidence becomes meaningful at the class level (records
  which *kind* of write happened, not just that a governance write
  happened) without leaking semantics into the kernel — the class is just
  a string.
- *Pro.* Defense in depth: a capability leak limits the bearer to one
  class; the mandate gate still blocks specific acts.
- *Pro.* Scope set is bounded and stable. Adding a 39th handler does not
  add a 39th scope.
- *Pro.* Honors the §4.1 doctrine literally — capability decomposition
  *and* mandate gating, both at the layer they belong.
- *Con.* More moving parts than either pure path. Each class adds one
  rotation surface and one error path.
- *Con.* Class boundaries are a judgment call. The class set proposed in
  §6 is open to refinement.

## 5. Recommended path

<!-- truth: normative -->

**Adopt the hybrid (§4.3).** Decompose to a small class-level scope set
(seven proposed classes — see §6) plus an app-side mandate gate for high-
and medium-blast-radius acts. Low-blast-radius acts (comments, reactions,
ordinary meeting record-keeping) are mandate-exempt but still class-gated.

Rationale:

1. The doctrine explicitly endorses the layered structure (§4.1: "the two
   are not in tension"). The hybrid is the literal reading of the
   strategy.
2. Per-action scopes (§4.1) trade one big problem (broad capability) for
   45 small problems (per-action authority that still cannot bind to a
   target). The hybrid keeps the kernel surface small.
3. Pure mandate gating (§4.2) leaves the broad kernel capability in place;
   one missed mandate-check call is a full bypass. The hybrid narrows the
   blast radius of that mistake to one class.
4. A finite class set survives churn. Handler-level scope strings
   wouldn't.

## 6. Proposed scope set and handler mapping

<!-- truth: descriptive -->

Seven class-level scopes. The strings are proposals; the follow-up issue
that mints them will RFC the names. The federation class is sanctioned by
`ABUSE_CASE_HARDENING_STRATEGY.md` §4.1 (candidate
`governance:federation:propose`); the binding here uses the broader
`:write` suffix for symmetry with the other six classes.

| Scope (proposed) | Handlers | Mandate required (in production) |
|---|---|---|
| `governance:charter:write` | `create_domain`, `activate_charter`, `add_domain_member`, `remove_domain_member` | Yes — every act is high blast radius. For bootstrap, see §4.4 of `ABUSE_CASE_HARDENING_STRATEGY.md` (administrative shortcut artifact). |
| `governance:proposal:write` | `create_proposal`, `open_proposal`, `close_proposal`, `cast_vote`, `create_appoint_steward_proposal`, `create_remove_steward_proposal`, `create_delegation`, `revoke_delegation` | Yes for close/cast, steward proposals, and delegation lifecycle (consistent with the medium-blast-radius classification in §3 and §5). Proposal *creation* may use a lighter mandate equivalent to membership-in-good-standing; the exact mandate shape per act is left to §12 Q4 and the follow-up PR that wires the gate. |
| `governance:steward:write` | `assign_role` (and any future direct-mutation steward operations) | Yes — steward authority cannot be granted by a bare capability. |
| `governance:federation:write` | `create_join_federation_proposal`, `create_leave_federation_proposal`, `create_establish_clearing_proposal`, `create_terminate_clearing_proposal`, `create_vouch_proposal`, `create_revoke_vouch_proposal`, `create_update_federation_policy_proposal` | Yes — every act alters cross-cooperative authority. Mandate equivalent to "ratified domain authority to bind this domain into a federation treaty". The `extract_federation_common` helper at `handlers.rs:277` is the single migration point for this class. |
| `governance:meeting:write` | `create_meeting`, `start_meeting`, `end_meeting`, `add_agenda_item`, `update_agenda_item`, `add_attendee`, `mark_attendance`, `create_action_item`, `update_action_item`, `update_action_item_status`, `delete_action_item`, `add_action_item_note` | No mandate beyond membership-in-good-standing for routine meeting record-keeping. Steward-only meeting acts (if any are added later) escalate to `governance:steward:write`. |
| `governance:activity:write` | `create_activity`, `create_program`, `create_structure`, `create_milestone`, `update_milestone_status`, `update_program_status`, `link_activity_to_program`, `unlink_activity_from_program` | No mandate beyond membership-in-good-standing for routine record-keeping (create/link/unlink). Status-transition acts (`update_milestone_status`, `update_program_status`) match the §3 medium-blast classification — they assert a milestone or program has reached a new state — and require the mandate gate. |
| `governance:comment:write` | `add_comment`, `edit_comment`, `delete_comment`, `add_reaction`, `remove_reaction` | No mandate beyond membership-in-good-standing. Edit/delete is restricted to the original author; this is an app-level check, not a kernel check. |

### 6.1 Mandate-binding surface (app-side)

The app-side mandate gate exposes a trait roughly shaped as:

```text
trait MandateGate {
    fn require(
        actor: &Did,
        domain: &DomainId,
        act: MandateAct,
        target: MandateTarget,
        at_height: BlockHeight,
    ) -> Result<MandateGrant, MandateRejection>;
}
```

- `MandateAct` enumerates the institutional acts (a finite, named set —
  e.g. `AddDomainMember`, `RemoveDomainMember`, `ActivateCharter`,
  `CastVote`, `CloseProposal`, `AppointSteward`).
- `MandateTarget` binds the mandate to the subject of the act (domain,
  proposal, role).
- `MandateGrant` is a signed reference returned to the handler; the
  handler records its hash in the resulting receipt so that the artifact
  trail captures both *which mandate* authorized the act and *which
  capability* the bearer used.
- `MandateRejection` carries a structured reason (no-mandate,
  expired-mandate, wrong-target, wrong-actor, suspended) for the surface
  to render.

The trait, its types, and its persistence backing are unbuilt. They are
named here so that the follow-up implementation PRs have a fixed target.

### 6.2 Read-side scopes

This document only touches the write side. Read scopes
(`governance:read`) are unchanged.

## 7. Kernel evidence delta

<!-- truth: descriptive -->

The "After" column assumes the receipt-schema work in §10 step 2 has
landed; until then, the kernel-side audit log carries the scope string
but the receipt body does not. The split is named in §7.1.

| Property | Before | After (assumes §10 step 2 has landed) |
|---|---|---|
| Capability strings recorded per write (audit log) | 1 (`governance:write`) | 1 of 7 class-level scopes |
| Per-class evidence on the kernel side (audit log) | None | Yes — audit log records which class scope was presented |
| Capability scope recorded in *receipt body* | No — receipt body has no scope field | Yes — new `capability_scope_presented` field; canonical decision/action/attendance hash is recomputed over the extended field set |
| Mandate reference in receipt body | No — receipt body has no mandate field | High/medium-blast acts: `MandateGrant` hash recorded; low-blast: explicit `no_mandate_required` discriminator. Old receipts without these fields remain replayable; new receipts include them under a new domain-separation tag (`icn:gov:decision:v2`, …) |
| Per-target binding visible to kernel | None | None — still in the app. The kernel does not learn what `domain_id` or `proposal_id` mean. |
| Per-act semantics in kernel | None | None — class names are opaque strings to the kernel |
| Forensic question "which kind of write happened" | Cannot be answered from kernel evidence alone | Answered from kernel evidence alone *after* §10 step 2; before that, answerable from the audit log but not from the receipt body |
| Forensic question "was this act authorized by governance" | Cannot be answered from kernel evidence | Answered from mandate reference in the artifact chain (app + kernel together) *after* §10 step 2 |

### 7.1 Why the receipt body is in scope

The existing governance receipts (`GovernanceDecisionReceipt`,
`ActionItemCompletionReceipt`, `MeetingAttendanceReceipt` in
`icn/crates/icn-governance/src/proof.rs:224, :509, :628`) hash only
proposal/action/meeting data plus actors and tallies; they do not carry
the capability scope or any mandate reference. The current
`require_scope` helper is an admission gate, not a record on the
receipt. Without the receipt-schema work, the forensic claim "you can
read which kind of write happened from the receipt alone" is false —
even after handler migration — because the receipt body never carried
that field. That work is therefore lifted into §10 step 2.

The meaning firewall is preserved: the kernel sees only strings and
hashes. App-side mandate semantics remain in the app.

## 8. What does not change

<!-- truth: descriptive -->

- The kernel/app meaning firewall. The kernel still enforces strings
  blindly; only apps know what `governance:charter:write` *means*.
- `governance:read` and all other capability scopes
  (`network:write`, `ledger:write`, `contract:write`, …).
- The `require_scope` helper in `icn-rpc`. It still takes a `&str`. The
  seven class-level scopes are added as constants alongside
  `GOVERNANCE_WRITE`; the old constant remains until every handler is
  migrated. The `extract_federation_common` helper at `handlers.rs:277`
  switches from `governance:write` to `governance:federation:write` as
  the single change point that migrates all seven federation-proposal
  routes.
- The gateway scope allowlist
  (`icn/crates/icn-gateway/src/validation.rs:42-54`) and the scope-issuing
  auth endpoint (`icn/crates/icn-gateway/src/api/auth.rs:87-94`) keep
  their existing shape; only the contents of `ALLOWED_SCOPES` grow with
  each class. The validation function itself does not change.
- The administrative-shortcut artifact shape from
  `ABUSE_CASE_HARDENING_STRATEGY.md` §4.2 / §4.4. The hybrid path is
  orthogonal to whether `add_domain_member`/`activate_charter` remain
  bootstrap-only; this design assumes they remain bootstrap-only per §7.
- Existing tests gated on `governance:write`. They continue to pass while
  the old constant is retained; per-handler tests migrate alongside each
  handler's scope change.
- The CCL policy oracle's input shape. Mandate-gate calls land beside it,
  not inside it.

## 9. Out of scope for this design

<!-- truth: descriptive -->

- The exact wire format of `MandateGrant`. Picked in the follow-up PR
  that builds the mandate persistence backing.
- Whether mandates are stored as opaque receipts under
  `(class, record_hash) → bytes` (consistent with the receipt cascade) or
  as their own typed surface. Picked at implementation time.
- The migration sequence — which class moves first. §10 lists the
  proposed sequence; the exact PR boundaries are picked when the first
  implementation PR lands.
- Read-side scope decomposition. Not relevant to the abuse story in §4.1.
- Network-layer or ledger-layer capability work. Out of scope for the
  governance epic.

## 10. Follow-up PRs

<!-- truth: descriptive -->

1. **Mint the seven class-level scope constants and extend the gateway
   allowlist.** Pure addition; old constant retained. No handler changes.
   The seven new strings are added as constants in
   `icn/crates/icn-rpc/src/auth.rs` alongside `GOVERNANCE_WRITE` *and* as
   new entries in the `ALLOWED_SCOPES` list in
   `icn/crates/icn-gateway/src/validation.rs:42-54` so that the gateway
   auth endpoint (`icn/crates/icn-gateway/src/api/auth.rs:87-94`) will
   issue capabilities with the new strings. Without the gateway allowlist
   update, clients cannot request a capability containing the new class
   scope and migrated routes become unreachable. Validation tests in
   `icn-gateway` cover the new strings.
2. **Extend governance receipt types to carry capability scope and
   mandate-grant hash.** Wire-format migration on
   `GovernanceDecisionReceipt`, `ActionItemCompletionReceipt`, and
   `MeetingAttendanceReceipt` in
   `icn/crates/icn-governance/src/proof.rs:224, :509, :628`. New fields:
   `capability_scope_presented: String` and
   `mandate_grant: Option<MandateGrantRef>` (where `MandateGrantRef`
   carries the grant hash and a structured discriminator for
   `no_mandate_required` low-blast cases). Each receipt's canonical
   hash function is forked: existing receipts continue to verify under
   the v1 domain-separation tag (`icn:gov:decision:v1`, etc.); receipts
   produced by migrated handlers use a new v2 tag
   (`icn:gov:decision:v2`, etc.). Verifiers accept either tag. Without
   this step, the strong forensic claims in §7 are unachievable even
   after every handler is migrated, because the receipt body has
   nowhere to record the scope.
3. **Migrate `governance:charter:write`** — `create_domain`,
   `activate_charter`, `add_domain_member`, `remove_domain_member`. Pairs
   with #1869 (direct charter activation bootstrap-path labeling) and
   #1870 (TrustThreshold fail-open on direct membership mutation), both
   already open.
4. **Migrate `governance:steward:write`** — `assign_role`. Small.
5. **Migrate `governance:federation:write`** — single edit to
   `extract_federation_common` at `handlers.rs:277` migrates all seven
   federation-proposal handlers at once. Pairs naturally with any
   federation-treaty hardening (`icn-federation` invariants).
6. **Build the `MandateGate` trait, types, and persistence backing.**
   Independent of any handler migration; lands before any handler starts
   calling it.
7. **Wire the mandate-check for `governance:charter:write` and
   `governance:federation:write` acts.** First two classes to require
   mandates.
8. **Migrate `governance:proposal:write`.** Pairs with mandate-check for
   close/cast/steward-proposal acts.
9. **Migrate `governance:meeting:write`.** No mandate-check beyond
   membership.
10. **Migrate `governance:activity:write`.** Same.
11. **Migrate `governance:comment:write`.** Same.
12. **Migrate non-app surfaces.** Before retirement, migrate the
    three additional gateway routes
    (`icn-gateway/src/api/flow_c.rs:52`,
    `icn-gateway/src/api/registry.rs:497, 593`) and update the JSON-RPC
    method→scope mapping in `icn-rpc/src/auth.rs:1009-1014` so the five
    governance JSON-RPC methods route to their class-level scopes per
    §3.1.
13. **Retire `governance:write` constant** once no production code
    references it anywhere in the workspace (verified by a workspace-wide
    `rg '"governance:write"'` returning only test fixtures, archived
    docs, or this design document).

Each follow-up PR is independently mergeable; the order above is a
recommendation, not a constraint. The retire step is the only one that
must be last.

## 11. Does this need an ADR?

<!-- truth: descriptive -->

Yes, after this design lands and the class-level scope names are RFC-ed.
The ADR records:

- The class-level scope set as a frozen contract surface (consumers of
  capability tokens may key off these names).
- The `MandateGate` trait shape as a frozen app-facing interface.
- The retirement schedule for `governance:write`.

The ADR follows the implementation, not this design. This design is the
*draft*; the ADR is the *commitment* once the strings are stable and
nothing in the implementation phase has invalidated them.

## 12. Open questions for review

<!-- truth: descriptive -->

1. Are seven classes the right granularity, or should `activity` and
   `meeting` collapse into one `governance:org:write`?
2. Should `assign_role` move under `governance:charter:write` (since it
   alters who has authority) instead of its own `governance:steward:write`?
3. Should federation-treaty proposals route through `governance:proposal:write`
   instead of their own class, since each is technically a *proposal*?
   Counter-argument: the blast radius is cross-cooperative, which is
   categorically different from intra-domain proposals, and the
   `extract_federation_common` helper already makes them a natural unit.
4. Should the mandate-check for `cast_vote` be the same shape as the
   mandate-check for `close_proposal`, or do they need different
   `MandateAct` variants?
5. Comment/reaction edit-restriction-to-author is an app-level
   per-resource check. Should it route through `MandateGate` for
   symmetry, or stay as a direct ownership check?

Reviewers and follow-up PRs answer these. None of them block this design.

## Anchors

- `icn/crates/icn-rpc/src/auth.rs:947` — `GOVERNANCE_WRITE` constant
- `icn/crates/icn-gateway/src/validation.rs:42-54` — `ALLOWED_SCOPES` gateway allowlist (must grow with each new class scope; mint step's co-requisite)
- `icn/crates/icn-gateway/src/api/auth.rs:85-94` — gateway scope-validating auth endpoint that consumes `ALLOWED_SCOPES`
- `icn/crates/icn-gateway/src/api/flow_c.rs:52` — `cast_vote_alias`, additional `governance:write` site outside `apps/governance`
- `icn/crates/icn-gateway/src/api/registry.rs:497, 593` — `create_meeting` and `index_decision_endpoint`, additional `governance:write` sites
- `icn/crates/icn-rpc/src/auth.rs:1009-1014` — `required_scope_for_method` JSON-RPC method→scope mapping; five governance methods bind to `GOVERNANCE_WRITE` and must rebind to class scopes
- `icn/apps/governance/src/http/handlers.rs:283` — `require_scope` call inside `extract_federation_common` (lines 277-) that gates the seven federation-treaty proposal handlers at `:2927, :2967, :2997, :3034, :3062, :3097, :3125`
- `icn/apps/governance/src/http/handlers.rs:391, 554, 599, 699, 756, 1090, 1182, 1701, 2211–2812, 3178, 3258, 3421, 3476, 3517, 3684, 3762, 3812, 3854, 3910, 3956, 3997, 4141, 4223, 4304` — inline call sites
- `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` §2.7, §4.1, §4.2, §4.4, §7
- Related open issues: #1869 (direct charter activation bootstrap labeling), #1870 (TrustThreshold fail-open), #1871 (production startup guard for optional standing checkers), #1872 (receipt backend non-atomic mandate/grant boundary), #1873 (ReconciliationStatus accepted-is-not-applied)
