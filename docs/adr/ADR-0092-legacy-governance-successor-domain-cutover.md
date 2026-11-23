---
id: "0092"
title: "Legacy governance to Subject-mode successor-domain cutover (IDENTITY_SEMANTICS §7.5)"
status: "proposed"
date: "2026-11-23"
deciders: ["Matt Faherty"]
tags: ["governance", "identity", "migration", "cutover", "authority-log", "n2", "gen", "subject-mode"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "not implemented (design + test contract only)"
references:
  - "docs/architecture/IDENTITY_SEMANTICS.md §4, §6, §7.2–§7.5 (normative; owner of the hard migration gate)"
  - "docs/architecture/GOVERNANCE_HUMAN_ACTOR_MODEL.md §2–§8 (normative; HumanActorModel, #2862)"
  - "docs/architecture/GEN_SUBJECT_CONTEXT_GENESIS.md, docs/architecture/GEN_SUBJECT_RECOGNITION.md (normative; GEN-A / GEN-B)"
  - "ADR-0018 (ADR lifecycle)"
  - "ADR-0083 (institutional domain runtime root)"
  - "icn#2884, icn#2885, icn#2886, icn#2887, icn#2888 (experimental authority-to-effect stack, Context Authority Log v0; draft, unmerged)"
  - "icn#2626 (N2-E2 legacy-bridge evidence object), icn#2694 (Subject semantics into governance), icn#2600 (G1), icn#2602 (GEN), icn#2469 (governance replication), icn#2641 (VotingPrincipal duplicate-act prevention)"
---

# ADR-0092: Legacy governance to Subject-mode successor-domain cutover

## Status

`proposed`. Nothing here is in force. It proposes contract changes to registered owners
(listed under "Contract changes requested"); each owner decides its own part. Decision status and
implementation status are independent (ADR-0018). No persisted format, signed byte layout or wire
format is changed by this ADR.

## Context

### The gate this ADR addresses

`IDENTITY_SEMANTICS.md` §7.5 forbids any live membership or vote re-key until four things are
designed: **migration ordering**, **alias/transition recognition**, **duplicate-act prevention**
and **final cutover**. Only duplicate-act prevention is delivered (#2641, `VotingPrincipal`).
Until the other three exist, legacy `Did`-keyed governance cannot converge on Subject-based
governance, and every later convergence step waits behind it: one authorization mechanism for
governance, shared resource operations, settlement and federation needs institutional authority
that is evidence rather than one node's word.

### Owner direction (2026-11-23, maintainer)

The maintainer chose the **Context Authority Log** (`icn_authz::effect_v0::log`, draft #2885) and
its authority-to-effect stack (#2884–#2888) as the **target direction** for institutional authority
transitions. This is an architectural direction, not a ratified protocol format and not a
production-readiness claim. A separate node-sequenced "per-domain decision log" over legacy
governance is **not** to be built. The experimental types are not merged into one universal
structure.

### What already exists (source-verified at `origin/main` 951e62aa5 unless marked)

| Concern | Status | Where |
|---|---|---|
| Legacy domain, electorate | **main** | `GovernanceDomain` (`icn-governance/src/domain.rs`), `config.membership: MembershipSource::{StaticList(Vec<Did>), TrustThreshold(f64)}` (`membership.rs`); row `gov:domain:{id}` (`apps/governance/src/state_store.rs`) |
| Legacy votes | **main** | `Vote { proposal_id, voter: Did, choice, weight, timestamp, comment }` (`vote.rs`): no authorship evidence; authenticated only by the submitting session (`claims.sub`, `apps/governance/src/http/handlers.rs`); row `gov:vote:{proposal}:{voter}` |
| Legacy close atomicity | **main** | write-ahead close journal `gov:close-intent:{id}` (`apps/governance/src/close_journal.rs`); proof `governance:proof:{proposal}` |
| Alias collapse / one effective vote per principal | **main** | `VotingPrincipal`, `effective_votes` (`icn-governance/src/vote_principal.rs`, #2641) |
| Per-domain human actor model, legacy refusal in Subject mode | **main** | `HumanActorModel::{LegacyPrincipalV0, ContextSubjectV1}` (#2862); monotonic `save_domain` via `compare_and_put`; unknown mode value is a deserialization error |
| Context-scoped Subject inception | **main** | GEN-A `SubjectContextGenesisV1` (`icn-identity::subject_context`), kind `GovernanceDomainV1` binds a Subject to one governance-domain id |
| Domain recognition of a Subject | **main** | GEN-B `SubjectRecognitionV1` (`icn-governance/src/subject_recognition.rs`); only basis `BootstrapRehearsalV1`, **not production-grade**; legacy migration (case B) explicitly reserved for #2626 |
| Subject authority log, device acts | **main** | N1 `authority_log` (`SubjectId`, `PrincipalKey`), N4 `DeviceActV1` / `verify_device_act`; daemon opens `AuthorityRegistry` (ADR-0088) with **no production consumer** |
| No `Did → SubjectId` conversion | **main** | `icn-identity/tests/authority_log_bridge_prohibitions.rs` |
| Institutional transition log (standing, rules, writer) | **experimental (#2885)** | `TransitionV0`, `SignedLogEntryV0`, `replay_context_log`, `ContextStateV0`, `RelyingFrontierV0`, `find_log_fork` / `LogForkProofV0` |
| Effect authorization, admission, idempotence | **experimental (#2884, #2886, #2887)** | `AuthorizationV0`, `verify_checkpointed_authorization`, `admit_checkpointed`, `ConsumedSet`, durable `DurableConsumedSet` (`icn-authz-store`) |
| Successor binding, seal state, cutover decision payload, production recognition basis, bridge evidence object, durable context-log store, restore-time frontier check | **absent** | this ADR names them as seams; it does not implement them |

Local feasibility evidence (not an integration claim): #2884–#2887 merge onto `origin/main`
951e62aa5 with no conflicts, and `icn-authz` lib (48), `icn-governance` lib (748),
`authz_threshold` (30) and `context_log` (58) pass on the merge. #2888 was not included.

### Constraint from the existing actor-model contract

`GOVERNANCE_HUMAN_ACTOR_MODEL.md` §3: a domain with legacy members **cannot** become Subject-mode,
there is **no** in-place mode switch for a persisted domain, and no membership is converted. This
ADR keeps that rule. It does not propose relaxing it.

## Decision (proposed)

### 1. Successor domain, not in-place conversion

A legacy domain `L` never changes into a Subject-mode domain. The cutover creates a **successor**
`S`: a new `GovernanceDomainId`, created directly as `ContextSubjectV1` with the inert
`StaticList(vec![])` (§3 of the actor-model contract, already supported). `S` is bound to a Context
Authority Log context `C`. When the cutover completes, `L` is **sealed**: it refuses every rail,
keeps its records byte-for-byte, and points to `S`.

Three identifiers stay distinct and are **bound, never equated**:

- `L`, `S`: governance-domain ids (`GovernanceDomainId`). GEN-A Subjects for the new electorate are
  incepted for `S`'s id (`SubjectContextKind::GovernanceDomainV1`).
- `C`: the context ref (`ContextRef = H(ContextInceptionV0)`). Its inception `label_hash` commits to
  `S`'s exact id bytes, and the binding `S → C` is recorded on `S` (seam B1). Neither id is derived
  from the other, and no institution, Subject or node shares an identifier with another.
- Each member's legacy `Did` remains a **Principal** (IDENTITY_SEMANTICS §7.2). It never becomes,
  names or seeds a `SubjectId`.

### 2. Migration states

Per legacy domain `L` (persisted on `L`'s domain record, monotonic):

```text
            accepted CutoverProposal(L→S, C, h1, R, M)          all L proposals terminal
 Legacy ─────────────────────────────────────────▶ CutoverPending ──────────────────────▶ Sealed(S, C, h1, F)
   ▲                                                    │                                   (terminal)
   └──────────── accepted AbortCutover (only proposal ──┘
                 kind admitted while pending)
```

| State of `L` | Legacy rails in `L` | `S` / `C` acts |
|---|---|---|
| `Legacy` | unchanged | `S` may exist (inert) but admits **no** effect; `C` may hold a genesis |
| `CutoverPending` | votes and closes on **already-open** proposals continue under legacy rules; **creating** a proposal is refused except `AbortCutover`; membership/electorate/config edits refused | none admitted |
| `Sealed` | **every** rail refused, as for Subject mode (§4 of the actor-model contract), plus proposal creation | admitted, subject to `C`'s rules |

Allowed transitions are exactly `Legacy → CutoverPending`, `CutoverPending → Legacy` (abort) and
`CutoverPending → Sealed`. `Sealed` has no exit. The monotonic rule already used for
`ContextSubjectV1` in `save_domain` (`compare_and_put` against exactly the bytes read) is extended to
this order (seam B2).

**Activation point.** The single durable write `CutoverPending → Sealed` on `L`. `S` holds no
separate "active" bit: `S` admits an effect only if it reads `L` as `Sealed` with `successor == S`
(seam B3). There is therefore no instant at which both formats are authoritative, and no second
write that can be lost between them.

### 3. Who may initiate, and what authorizes the cutover

Two independent authorities must agree. Neither is a node signature.

1. **The legacy institution consents**, through an ordinary accepted legacy proposal in `L` with a
   new payload `CutoverProposal` (seam B4) committing to:
   - `S` (successor domain id) and `C` (context ref);
   - `h1`, the exact head of `C`'s genesis entry (seq 1);
   - `R`, the roster commitment: `roster_hash` (#2884) over the genesis standing;
   - `M`, a commitment to the private recognition map (§4).
   Anyone able to propose in `L` may propose it. Authority comes only from acceptance under `L`'s
   current rule. This half is **exactly as strong as legacy governance**: votes in `L` are
   session-attested records, not signed acts, and the cutover cannot make past consent stronger
   evidence than it was. That limit is stated, not hidden.
2. **The new context is founded by humans, not by a node.** `C`'s genesis is signed by a founder
   quorum of `ContextInceptionV0` (#2884/#2885). Founders are **fresh context-specific device
   Principals** of recognized Subjects (§7.2 rule 2), never legacy `Did` keys.

The commitment to `h1` binds the two halves. A founder-equivocated second genesis for `C`
(detectable by `find_log_fork`) is not the one `L` accepted, so it is refused without any
tiebreak.

An actor who can neither win a legacy vote in `L` nor gather a founder quorum cannot initiate a
cutover. A writer of `C` cannot initiate one, because the writer sequences and never authorizes
(#2885).

### 4. Recognition, eligibility and the electorate transition

For each legacy electorate member with principal `p = VotingPrincipal(did)` who enrolls:

- **FACT A (personal continuity).** The member incepts a GEN-A Subject `s` for `S` and signs
  bridge evidence with the legacy key binding `(S, s)`. Signing it is not an enrolment
  (§7.2 rule 3). The object's contents are #2626's to define (seam B5).
- **FACT B (institutional recognition).** Inclusion of `(p → s)` in the map committed by `M` in the
  accepted `CutoverProposal`. It is expressed as a GEN-B recognition under a **new, production
  basis kind** (seam B6, owned by GEN-B / #2626). It is never a reinterpretation of
  `BootstrapRehearsalV1`.
- **Electorate.** `C`'s genesis standing is the recognized set, one `(s, role)` per member, with
  the role label naming the electoral standing the legacy domain conferred. Institutional
  membership and electoral eligibility stay distinct roles (opaque labels, chosen by the
  institution's rules, not by the substrate).
- **Uniqueness of the map.** `M` must be a function on alias-collapsed principals and injective
  on Subjects: one `p` maps to at most one `s` and one `s` comes from at most one `p`. The map is
  built only after the existing #2641 alias collapse. No new alias logic is written.
- **Late enrolment.** A member who has not enrolled when the proposal is accepted is not in the
  genesis standing. They join later through an authorized `GrantStanding` transition in `C`
  with their own bridge evidence. The admission rule must check `p` against `M` and against every
  later grant (seam B7). Until then they hold no vote in either format, which is the safe failure.
- **Privacy.** The public log carries `SubjectId`s only. `M`'s rows (legacy principal ↔ Subject)
  are held by `S`'s custodians, disclosed only to parties that already hold the legacy `Did`
  (§7.2 rule 4), retention-limited and erasable (§7.3). Only the commitment is published.
- **`TrustThreshold` domains are out of scope.** Their electorate is computed from one node's
  trust graph and cannot be frozen into `R`. They need owner decision D5 (#2931) first.

### 5. Ballot uniqueness across the transition

Legacy ballots (`ProposalId` in `L`) and context operations (`(C, op_id)`) are **disjoint
spaces**, and no ballot is admissible in both:

- `CutoverPending` admits no new legacy proposal, so no new legacy ballot opens.
- `Sealed` requires every `L` proposal to be terminal. Open legacy ballots therefore finish under
  their own rules and are never carried into `C`, re-cast or re-counted.
- Inside `L`, #2641 keeps one effective vote per principal. Inside `C`, votes are N4 acts by
  Subjects, bound to `(C, op_id)` and consumed once (`ConsumedSet`, shared legacy/checkpointed
  key, #2886).
- One person cannot reach two effective votes on one ballot through a legacy `Did` and a new
  `SubjectId`, because no ballot accepts both identifier kinds.

Pre-existing roster defects carry over unchanged. For example, two distinct legacy keys held by
one human and both listed in `L` were already counted as two members. The cutover preserves
those counts. It does not repair them, and the roster review before acceptance is where they are
caught.

**Open-proposal rule: drain, not terminate.** Terminating open legacy processes would void
decisions the institution was entitled to make. Draining finishes them under legacy rules. A
proposal that can never close (for example one stuck below quorum) blocks the seal until the
institution closes it through existing legacy means. That is visible and safe, never silent.

### 6. Historical evidence

Nothing in `L` is rewritten. Votes, tallies, close journals, receipts (v1/v3) and proofs keep
their original meaning and are verified under their original contracts (including #2928's offline
bundle where available). No historical vote is re-signed, re-keyed or presented as a Subject act.
The sealed record keeps `L`'s membership list for interpretation. Legacy verifiers are **never**
retired with the legacy voting path.

### 7. Persistence and crash recovery

| Step | Atomic line | Crash behaviour |
|---|---|---|
| `CutoverProposal` accepted → `L` `CutoverPending` | inside the existing close-journal commit (same store as the proposal state), idempotent (seam B8) | replay of the journal re-applies the same transition; at most once by monotonic CAS |
| Proposal creation refused while pending | actor reads `L`'s mode before mutation (as for Subject mode) | nothing written on refusal |
| `CutoverPending → Sealed` | one `compare_and_put` on `L`'s record, guarded by "every `L` proposal terminal". The record also commits `F`, a hash of `L`'s final terminal proposal set | before: still pending, retry later. After: sealed. No partial state |
| `S` activation | none: derived from `L`'s record | cannot diverge from `L` |
| `C` entries and frontier | **absent seam B9**: durable context-log store plus retained relying frontier (not provided by #2887, which persists consumption only) | until it exists, `S` cannot admit effects |

A failed or incomplete migration leaves `L` in `Legacy` or `CutoverPending`. Neither state admits
`S` effects, and neither re-opens anything that was refused.

### 8. Replication, forks and stale nodes

- Governance replication is observe-only today (#2469). No peer applies `L`'s state, so the seal
  is not a replicated authority write. Relying parties learn it from exported evidence: the
  accepted `CutoverProposal` receipt, `C`'s log and `F`.
- **Forks in `C`** are handled by the existing #2885 discipline: pinned frontiers refuse other
  branches, and `LogForkProofV0` attributes equivocation. Detection is not prevention, and the
  censorship limit still applies (#2885 `known_limit_*`).
- **Stale or replayed legacy decisions.** Any decision of `L` whose proposal is not in `F` was
  closed after the seal (or on a forked or restored copy) and is invalid to a relying party holding
  `F`. This rule uses set membership, not timestamps.
- **A second cutover** for `L` cannot start: `CutoverPending` admits no further `CutoverProposal`.

### 9. Rollback and downgrade

- **Older binaries.** Rung-4+ binaries treat an unknown mode value as a deserialization error, so
  they fail closed on a sealed or pending record. Pre-rung-4 binaries ignore the field (existing
  downgrade warning, actor-model §8). **Activation prerequisite:** every binary that can open the
  governance store is rung-4+ and seal-aware, and seal-aware refusal ships at least one release
  before the first activation.
- **Restore from a pre-cutover backup.** A store restored from before the seal reads `L` as
  `Legacy` and would re-open legacy voting locally. **The node cannot detect this from its own
  data.** This is the same limit class as #2885's freshness limit. Required (seam B10): `icnctl`
  restore refuses to resume governance until the operator supplies the latest retained frontier
  for every bound context, or the domain is frozen pending reconciliation. Relying parties holding
  `F` reject any post-seal `L` decision the restored node produces.
- **Raw-store writers** can still bypass the typed monotonic line (actor-model §3, #2864 A2). That
  residual must be resolved or explicitly accepted before any real activation. This ADR does not
  close it.

### 10. Retiring the legacy voting path

The legacy **write** path for a deployment may be removed only when all of the following hold:

1. every legacy domain in it is `Sealed` (or explicitly frozen with an owner decision);
2. every binary that can open the governance store is seal-aware;
3. seams B9 and B10 exist and are exercised by a restart and restore witness;
4. legacy HTTP/RPC write routes are removed, not merely refused;
5. bridge data (`M` rows) has a retention and erasure procedure (§7.3).

Legacy **verification** (receipts, proofs, alias collapse used to interpret history) is retained
permanently.

### 11. How this connects the shared authority lifecycle

After cutover, one chain carries authority with no parallel membership or authority database:

```text
GEN-A Subject (S) ──▶ N1 device authority ──▶ N4 act (vote) ──▶ C log: standing + rules (derived)
     ──▶ AuthorizationV0 under C's effect rule at a checkpoint ──▶ admit_checkpointed + ConsumedSet
     ──▶ effect (governance decision, resource reservation #2888, later settlement / federation acts)
```

Derived views (an `S` electorate listing, a dashboard roster) are projections of `C` and are never
written independently. The actor-model §7 prerequisites map onto existing experimental parts
rather than new types: roster = `C` standing; frozen snapshot = `LogCheckpointV0` plus
`roster_hash`; Subject vote action = N4 `InputActV0`; tally = an `AdoptedRule` (e.g.
`authz_threshold`); decision receipt = `OutcomeReceiptV0`. Whether G1-A and N5-A accept this
mapping is an owner decision (Q4).

## Contract changes requested (each owner decides)

| Owner | Change |
|---|---|
| `GOVERNANCE_HUMAN_ACTOR_MODEL.md` | add the legacy-side states `CutoverPending` and `Sealed` (monotonic, refusal semantics of §2), the successor binding `S → C`, and the "`S` admits only if `L` sealed to `S`" rule. The no-in-place-conversion rule is unchanged |
| `GEN_SUBJECT_RECOGNITION.md` / #2626 | a production recognition basis for legacy cutover (case B) and the bridge-evidence object contents |
| `IDENTITY_SEMANTICS.md` §7.5 | record migration ordering, alias/transition recognition and cutover as designed by this ADR once accepted (status update only; no change to §7.2 invariants) |
| #2885 stack | a durable context-log store and retained frontier (B9) before any `S` admits effects; context-log formats stay experimental until ratified |

## Executable acceptance contract

Status is **verified** (test exists and passed at this ADR's evidence run), **experimental**
(exists on the unmerged #2885 stack, passed on the local trial merge), or **seam** (cannot run
yet; the named seam is required). No seam test is claimed to pass.

| # | Condition | Status / test |
|---|---|---|
| 1 | One person cannot cast two effective votes across old and new identifier kinds | within `L`: **verified** `icn-governance` lib `sled_alias_spelled_second_vote_does_not_add_voting_weight`, `in_memory_conflicting_historical_alias_rows_fail_closed` (#2641). Within `C`: **experimental** `context_log::l_legacy_and_checkpointed_cannot_both_consume_one_operation`, `j_identical_replay_is_refused`. Across: **seam B3/B4** `cutover_ballot_spaces_are_disjoint` (a sealed `L` refuses the vote; `S` refuses before seal) |
| 2 | A legacy Principal is never silently substituted for a Subject | **verified** `authority_log_bridge_prohibitions::{no_implicit_conversion_from_legacy_did_into_subject_id, legacy_principal_bytes_wrapped_as_a_subject_derive_to_nothing, signing_evidence_does_not_enroll_the_legacy_key}`; `subject_recognition::a_principal_is_not_a_subject` |
| 3 | An unauthorized actor cannot initiate cutover | **seam B4** `cutover_requires_accepted_legacy_decision` and `cutover_rejects_genesis_not_committed_by_decision`. Founder half **experimental** `context_log::e2_genesis_needs_the_founder_quorum` |
| 4 | An electorate transition cannot approve itself under newly adopted rules | **experimental** `context_log::e9_a_rule_cannot_approve_its_own_adoption`, `e3_unauthorized_rule_replacement_is_refused`, `e2_writer_cannot_grant_standing_on_its_own_word` |
| 5 | Pre-cutover historical votes keep their original meaning | **verified (no-rewrite basis)** `actor_model_firewall::create_domain_cannot_overwrite_a_subject_domain`, `state_store_refuses_to_persist_a_mixed_domain`. **Seam B2** `sealing_preserves_every_legacy_row_byte_for_byte` |
| 6 | An open legacy ballot cannot be double-counted after transition | **seam B2/B4** `seal_refused_while_any_legacy_proposal_open` and `pending_refuses_new_legacy_proposals_but_drains_open_ones` |
| 7 | Stale nodes reject an invalid or incomplete cutover | **seam B3** `successor_admits_nothing_unless_predecessor_sealed_to_it`. **Seam B10** `restore_without_frontier_does_not_resume_governance` |
| 8 | Restart, replication and replay cannot reactivate retired legacy authority | Subject-mode analogue **verified** `actor_model_firewall::t8b_journal_replay_via_command_cannot_finalise_subject_proposal`, `t8c_startup_recovery_cannot_finalise_subject_proposal`. **Seam B2/B8** `sealed_domain_survives_journal_replay_and_restart` and `post_seal_legacy_decision_not_in_F_is_rejected` |
| 9 | A conflicting authority history cannot silently displace a pinned valid history | **experimental** `context_log::h_history_conflicting_with_the_pin_is_refused`, `e4_a_checkpoint_from_another_history_is_refused`, `e6_writer_equivocation_yields_an_attributable_proof`. Known limit **experimental** `known_limit_unpinned_party_accepts_either_branch` |
| 10 | No institution-level authorization relies solely on a node signature | **experimental** `context_log::e2_writer_cannot_grant_standing_on_its_own_word`, `e7_self_appointment_and_third_party_sequencing_are_refused`. Cutover: **seam B4** (founder quorum plus legacy decision; the closing node's receipt alone is insufficient) |

## Consequences

- **Easier:** there is one target authority mechanism. The legacy path gets a defined end (§7.3
  terminus) instead of an indefinite dual stack. History is untouched. The cutover reuses #2641,
  #2862, GEN-A/B and the #2885 stack rather than adding parallel stores.
- **Harder:** two halves must agree (legacy decision plus founder quorum), drain can stall a
  seal, restore needs an external frontier, and `TrustThreshold` domains wait on D5.
- **Risk accepted by design:** the legacy half of the cutover is only node-attested consent, and
  censorship and hidden alternative histories remain detectable-not-preventable limits of #2885.

## Open owner questions

- **Q1** Drain (recommended) vs terminate for open legacy proposals at seal.
- **Q2** Where the `S → C` binding lives: a `GovernanceDomain` field next to `human_actor_model`
  (recommended, monotonic like the mode) or the ADR-0083 `InstitutionalDomain` root.
- **Q3** Whether founders of `C` must themselves be recognized members of `R` (recommended) or
  may include designated stewards outside it.
- **Q4** Whether G1-A (frozen process snapshot) and N5-A (Subject vote action) adopt the §11
  mapping onto #2884/#2885 types, or keep separate formats that then need a bridge.
- **Q5** Restore policy (B10): refuse to resume, or freeze bound domains pending reconciliation.

## Implementation status

Not implemented. Design and test contract only. The evidence runs behind the **verified** and
**experimental** rows (cited main tests at 951e62aa5; local trial merge of #2884–#2887) are
recorded in the pull request that proposes this ADR.
