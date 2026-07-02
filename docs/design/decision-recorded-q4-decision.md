# DecisionRecordedReceipt Q4 decision — recorded fact vs proposal/vote lineage

**Status**: draft — design / audit decision (not runtime implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-02
**Source basis**: read against `main` @ `2b2622ca`. Code anchors were verified at that commit — re-verify before relying on exact numbers.
**Related**: issues #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · PR #2280 (merged `DecisionRecordedReceipt` design/audit contract, `docs/design/decision-recorded-receipt.md`) · PR #2278 (Q3 taxonomy decision, `docs/design/deliberation-entry-kind-taxonomy.md` — the sibling decision-doc pattern) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt envelope, Layer 2) · [ADR-0014](../adr/ADR-0014-constitutional-object-model.md) (decision-receipt provenance) · `ops/ideas/framing/institutional-process-substrate.md` (idea-0019 framing brief; open question Q4) · `docs/spec/effect-dispatch-contract.md` (effect dispatch keyed on the proposal/vote lineage)

> Narrow decision document closing framing-brief open question Q4 for the
> `DecisionRecordedReceipt` slice only. The merged #2280 contract pinned the
> Q4-independent skeleton and blocked implementation on four Q4 branches:
> (a) decision representation, (b) reference posture toward the
> proposal/vote `GovernanceDecisionReceipt` lineage, (c) deciding-body
> handle, (d) the fate of the deferred `resolution` deliberation kind. This
> document decides all four. It decides nothing else: no runtime change, no
> new receipt fields, no proposal/vote migration. Receipts record
> institutional facts. They grant zero authority.

## 1. Problem

PR #2280 defined the fourth `ProcessTransitionReceipt` class,
`DecisionRecordedReceipt` (proposed tag `icn:gov:decision_recorded:v1`),
and deliberately refused to pin its runtime `:v1` layout until Q4 is
decided. The framing brief's Q4, verbatim:

> How does `HumanDecisionSet` relate to the existing proposal/vote
> machinery? Is the existing path one specialization of
> `HumanDecisionSet`, or a parallel surface that the spine names but does
> not absorb?

The question is live because ICN already has a decision receipt: the
proposal/vote `GovernanceDecisionReceipt` lineage under
`icn:gov:decision:v1/v2/v3`. If the process-spine class silently absorbed,
referenced, or duplicated that lineage, it would either fork a load-bearing
audit chain or become proposal/vote outcome machinery by accident — the
exact failure the #2280 audit warns against. And the decision must land
before implementation: every unresolved branch changes the `:v1` canonical
hash layout or the class's lineage relationships, and the landed rule (from
the #2278 review cycle) is that hash-participating structure is decided in
writing before a tag is pinned, never silently in an implementation PR.

## 2. Prior-art audit (verified at `2b2622ca`)

**The proposal/vote decision lineage — outcome machinery, load-bearing.**
`icn/crates/icn-governance/src/proof.rs`: `GovernanceDecisionReceipt`
(~L226, tag `icn:gov:decision:v1` ~L251) carries `proposal_id`,
`domain_id`, `outcome` (Accepted / Rejected / NoQuorum), `vote_tally`,
`vote_hash`, and the canonical `decision_hash`;
`GovernanceDecisionReceiptV2` (~L541, `:v2`) adds
`capability_scope_presented` and the frozen mandate-attestation taxonomy;
`GovernanceDecisionReceiptV3` (~L820, `:v3`) adds the `ProcessAuthorized`
attestation mode; `GovernanceDecisionAttestation` (~L1037,
`icn:gov:attest:v1`) is the node-local witness. Downstream, this lineage is
structural: `decision_hash` is the load-bearing identifier of the effect
dispatch chain (`docs/spec/effect-dispatch-contract.md`: decision →
`InstitutionalEffectRecord` → `EffectManifest` → subsystem dispatch); the
gateway `ReceiptStore` keeps secondary indexes of **mandates and authority
grants by `decision_hash`** (`icn/crates/icn-gateway/src/receipt_store.rs`
~L80–84) plus lookups by `proposal_id` and `domain_id`; action cards
(contract: [ADR-0027](../adr/ADR-0027-action-card-contract.md); the
surface ADR-0020 promised, with the end-to-end proposal/vote proof-loop
verification recorded at ADR-0020 step 7) close the proposal/vote proof
loop against it; ADR-0014 records its
authority posture ("authoritative for 'the decision happened,' but it does
not itself bind authority-to-execute"). This is outcome/tally/proposal
lineage — a *specific decision rule* (vote over a proposal) with its
consequences wired in. It is not, and was never designed as, the generic
recorded-decision fact of the process spine.

**The process-transition receipt lineage — recorded facts, landed.**
Three classes, all wired end to end through the opaque cascade:
`ProcessGateResultReceipt` (`icn:gov:process_gate_result:v1`, #2144/#1755),
`ProcessSessionOpenedReceipt` (`icn:gov:process_session_opened:v1`,
#2275/#2276), `DeliberationEntryRecordedReceipt`
(`icn:gov:deliberation_entry_recorded:v1`, #2277/#2278/#2279). All are
body-blind or content-fingerprinting, session-or-domain-anchored,
idempotent-on-stable-identity, fail-closed on conflict, and grant zero
authority. The #2278 taxonomy decision already operated the Q4 firewall
inside this lineage: no approve/vote/outcome kind may enter
`DeliberationEntryKind` by append — "an outcome-bearing kind is a semantic
boundary change requiring its own design decision (and almost certainly a
different receipt class: `DecisionRecord` / `HumanDecisionSet` territory)"
— and `resolution` was deferred with discriminant `10` reserved.

**The framing brief's own definitions.** `HumanDecisionSet` is "the
structured set of decision moments inside one `ProcessSession`: who
decided, in what role, by what rule (vote threshold, consensus, mandate
from a parent body, delegated authority, emergency authority, etc.), with
what tally or attestation, and over which proposed outcome" — and the
brief's dogfood sketch positions it explicitly as **"a read-model that
names who decided by what rule with what tally or attestation."**
`DecisionRecord` is "the persisted, signed artifact that names the chosen
outcome and binds it to the deliberation that produced it." The brief also
separates deciding from doing: "a decision is **not** a mutation. A
decision authorizes a mutation" — with `ActivationRequest` as the explicit
boundary crossing.

**No `DecisionRecordedReceipt` runtime exists.** The strings
`DecisionRecordedReceipt` and `decision_recorded` appear nowhere under
`icn/` at `2b2622ca`. The only definitions are the merged #2280 contract
and status/backlog references.

## 3. The four Q4 axes — options and decision

### Axis A — representation of the recorded decision in `:v1`

Options considered:

1. **Opaque recorded-decision fact** — `body_hash` fingerprint only, no
   typed outcome/kind field. **CHOSEN.**
2. Typed `DecisionRecord` in the receipt — outcome enum / chosen-outcome
   naming inside the receipt payload. Rejected: the framing brief's
   `DecisionRecord` "names the chosen outcome"; putting outcome naming in
   the receipt makes the receipt the outcome artifact, which is exactly
   the semantic boundary #2278 §7 placed outside the process-input
   lineage, and it drags outcome-taxonomy design (accepted? adopted?
   consensus-reached? deferred?) into a hash layout before any consumer
   exists.
3. Typed `HumanDecisionSet` in the receipt — who/role/rule/tally
   structure. Rejected: the brief itself positions `HumanDecisionSet` as
   a **read-model**, not a receipt payload; embedding it would freeze
   who-decided-by-what-rule semantics (idea-0020 `AuthorityBasis`
   territory) into a v1 hash before that design exists.
4. Hybrid (opaque body + minimal typed outcome flag). Rejected: inherits
   the objections to 2 at reduced fidelity — a one-bit outcome is still
   outcome machinery, still hash-participating, still undesigned.

**Decision A: `:v1` records an opaque recorded-decision fact.** The
receipt witnesses that a decision artifact was recorded — attributed
(`recorded_by`), timestamped (`recorded_at`), content-fingerprinted
(`body_hash`) — and nothing about what the decision says. The
outcome-naming content lives in the (never-stored, fingerprinted) decision
body; structured who/rule/tally legibility arrives later as a read-model
or a future tag version, not in this hash layout.

### Axis B — relationship to `GovernanceDecisionReceipt`

Options considered:

1. Absorb — make the proposal/vote lineage a specialization of the
   process-spine class. Rejected: `icn:gov:decision:v1/v2/v3` is
   load-bearing (effect dispatch keying, mandate and authority-grant
   indexes by `decision_hash`, action-card proof loops, 20+ documented
   callers); absorbing means retagging or adapter-wrapping live audit
   chains for zero consumer benefit.
2. Reference — an optional field carrying the lineage's `decision_hash`
   when a session's decision came from a proposal/vote. Rejected **for
   `:v1`**: an optional hash-participating field is still a layout
   commitment, and it pre-answers the posture question inside a receipt
   slice instead of an ADR. A session whose decision was produced by a
   proposal/vote can already be evidenced without a receipt field: the
   decision body (fingerprinted by `body_hash`) can name the
   `proposal_id`/`decision_hash`, and read-models can join the two
   lineages at the app layer.
3. **Parallel with explicit non-convergence. CHOSEN.**
4. Conversion/adapters now. Rejected as premature: no consumer exists;
   adapters without consumers are speculative surface.

**Decision B: the answer to the framing brief's question is "a parallel
surface that the spine names but does not absorb."** The proposal/vote
path stays exactly what it is — one concrete, heavily-wired decision rule
with its own receipt lineage. `DecisionRecordedReceipt` records the
generic institutional fact that a process session's decision was recorded,
whatever rule produced it. The two lineages never converge tags, never
share fields, never share stores. Any future formal reference or adapter
between them is a `:v2`-or-later change (or a separate adapter class)
requiring its own explicit ADR — never a silent addition to `:v1`.

### Axis C — deciding-body handle in `:v1`

Options considered:

1. **No deciding-body field in v1. CHOSEN.**
2. Opaque `deciding_body_ref` string. Rejected: even an opaque ref bakes
   "there is a single nameable deciding body" into the hash layout —
   which is false for consensus processes, delegated chains, and
   emergency authority — and it reads as an authority claim the receipt
   explicitly does not make.
3. Typed body/member/quorum structure. Rejected: this is
   `HumanDecisionSet` by another name (Axis A, option 3).
4. Defer to future `HumanDecisionSet` work. **This is the same choice as
   1, stated forward**: when idea-0020/`HumanDecisionSet` design lands, it
   composes with this receipt as a read-model or future tag version.

**Decision C: `recorded_by` remains the only actor field in `:v1`, and it
remains recorder evidence — not `decider`, not `author`, not `approver`.**
Who decided, in what role, by what rule is deferred to the
`HumanDecisionSet` read-model / idea-0020 lane, exactly where the framing
brief puts it.

### Axis D — fate of the deferred `resolution` deliberation kind

Options considered:

1. **Keep deferred (discriminant `10` stays reserved). CHOSEN.**
2. Add `resolution` to `DeliberationEntryKind` v1 now. Rejected: #2278
   deferred it precisely because it straddles the input/outcome boundary;
   nothing about this decision sharpens the *input* reading, and v1 kind
   appends require their own ADR/addendum anyway.
3. Move it to decision territory as a typed decision kind. Rejected:
   Axis A chose no typed decision kinds in `:v1`; and with
   `DecisionRecordedReceipt` existing, "the resolution of the matter" is
   naturally the content of a recorded decision body — no new kind
   needed to express it.
4. App-layer mapping only. **Already legal and unaffected**: the #2278
   vocabulary firewall lets charters map institutional vocabulary (e.g.
   "proposed resolution offered into deliberation") onto existing input
   kinds such as `amendment` or `record_only` at the app layer, with no
   core change. This decision reaffirms that path and adds nothing to
   core.

**Decision D: `resolution` stays deferred and MUST NOT be added to
`DeliberationEntryKind` v1.** The outcome reading of "resolution" now has
a home (the recorded decision's fingerprinted body); the input reading
("resolution offered") remains expressible via app-layer mapping. A sharp
input-only `resolution` kind may still be appended later via the #2278
evolution rule if a real consumer demonstrates the need — this document
neither performs nor forecloses that append.

## 4. Decision summary

**Option A — `DecisionRecordedReceipt` `:v1` is a generic recorded-decision
fact, parallel to and explicitly non-convergent with
`GovernanceDecisionReceipt`.**

Binding consequences:

- `:v1` stays body-hash-only. No vote, proposal, tally, outcome,
  quorum, or effect-dispatch field — ever, in this tag version.
- `:v1` uses `recorded_by` (recorder evidence), never `decider`,
  `author`, or `approver`.
- `:v1` carries no deciding-body handle.
- `:v1` carries exactly the merged #2280 §4 field set — `domain_id`,
  `session_id`, caller-opaque `decision_id`, `recorded_by`, `recorded_at`,
  `body_hash`, `record_hash` — and nothing else.
- **Stable duplicate identity is narrower than the field set**, exactly as
  #2280 §4 pins it: same `(domain_id, session_id, decision_id)` + same
  `recorded_by` + same `body_hash` → idempotent retry returning the
  ORIGINAL receipt. `recorded_at` and the derived `record_hash` are **not**
  identity inputs — a retry never fails, restamps, or conflicts merely
  because time moved; a mismatch on `recorded_by` or `body_hash` is a
  fail-closed `decision_recorded_conflict` 409.
- The relationship to `GovernanceDecisionReceipt` stays **out of the v1
  hash layout entirely**. `icn:gov:decision_recorded:v1` and
  `icn:gov:decision:v1/v2/v3` never converge, never share fields, never
  share typed stores.
- Any future reference to the proposal/vote lineage, deciding-body
  handle, or typed outcome is a `:v2`-or-later tag version or a separate
  adapter class, after its own explicit ADR — and per #2280's
  contract-sync instruction, any such hash-participating field joins
  stable duplicate identity (mismatch = fail-closed 409, never a silent
  idempotent return).
- `resolution` remains deferred out of `DeliberationEntryKind` v1;
  discriminant `10` stays reserved.

## 5. Hash-layout consequence

**No change to #2280. The merged contract is implementation-ready as
written.** This decision adds zero fields, removes zero fields, and
changes zero encodings: the `:v1` canonical layout is exactly #2280 §4
(domain tag first; string fields length-prefixed u64 LE; `recorded_at` LE;
`body_hash` raw fixed-32 with no length prefix; equality anchored to
`record_hash`). #2280 does not need revision, amendment, or a
contract-sync edit — its "deliberately absent" list is hereby confirmed
absent for `:v1`, and its §4 contract-sync instruction (Q4-added fields
join duplicate identity) now applies to any **future** tag version rather
than to `:v1`. There are no silent layout changes available to the
implementation PR: it implements #2280 §4–§8 byte-for-byte or it stops and
comes back to design.

## 6. Boundaries (what this receipt is and is not)

- `DecisionRecordedReceipt` records that a decision fact was recorded
  against an opened process session. That is all.
- It does **not** prove the decision was valid, binding, voted, approved,
  quorum-backed, authorized, legitimate, or effect-dispatched. Bindingness
  and authority are future charter/policy/`HumanDecisionSet`/idea-0020
  work; the deciding→doing boundary stays with the framing brief's
  `ActivationRequest` concept and the future `ActivationCrossedReceipt`
  class, neither of which this document touches.
- The proposal/vote lineage — `GovernanceDecisionReceipt` v1/v2/v3,
  attestations, effect dispatch, mandate/authority-grant indexes, action
  cards — remains intact, separate, and untouched.
- Receipts record institutional facts. They grant zero authority.

## 7. Implementation instruction

With Q4 decided as above, the implementation blocker recorded in #2280
§10/§11 is cleared. The next PR in this lane may implement
`DecisionRecordedReceipt` **exactly** from the merged #2280 contract
(§4–§8) plus this document — one focused PR, mirroring how #2279
implemented #2277 + #2278. Required tests beyond #2280's §8 matrix
(restated here as binding):

1. Family-tag separation from `icn:gov:decision:v1`, `:v2`, and `:v3`
   (identical field bytes, different hashes), with a comment stating the
   lineages must never converge.
2. Payload audit: no proposal, vote, tally, outcome, quorum, mandate, or
   effect-dispatch field anywhere in the persisted payload.
3. `recorded_by` semantics: recorder evidence only; participates in
   stable duplicate identity; distinct recorders on the same
   `decision_id` are a fail-closed conflict.
4. `body_hash` only: raw body never persisted; fixed-32, no length
   prefix, per the landed convention.
5. Duplicate/idempotency/conflict behavior per #2280 §4 (original-return
   on stable-identity retry; `decision_recorded_conflict` 409 otherwise).
6. Session-open precondition (`decision_recorded_session_not_opened`,
   404; no silent session creation).
7. No `resolution` kind creep: `DeliberationEntryKind` remains exactly
   the #2278 ten-kind list; discriminant `10` remains unassigned.
8. Existing `GovernanceDecisionReceipt` v1/v2/v3 behavior byte-identical
   (typed store, effect dispatch, action-card suites untouched and
   green).
9. Existing process receipts unchanged (session-open, gate-result,
   deliberation-entry suites green, byte-identical semantics).

## 8. Explicit non-goals

No runtime implementation in this PR. No `DecisionRecordedReceipt` code,
route, proof type, backend method, manager method, or generated-inventory
change. No `HumanDecisionSet` runtime or read-model implementation. No
proposal/vote migration, retagging, or adapter. No effect-dispatch
changes. No action-card triggers. No activation-crossed / mutation-plan /
mutation-applied / evidence-packet classes. No served-OpenAPI or SDK
publication. No privacy/redaction evidence export; no accessibility-gate
run. No #1748 closure; no #2141 closure; no #2081/#2080/#2274 work. No
process-runtime, Phase-2, production, pilot, member/organizer-readiness,
live-federation, or service-hosting claims.

## 9. Validation (this document's PR)

Docs-only change set: this file, a `docs/registry.toml` row, a
`docs/INDEX.md` link, and the regenerated `docs/DOCUMENT_REGISTRY.md`
(regeneration after registry row additions is required by that file's
header). Validation: `git diff --check`, `python3
docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml`
(no new warnings), `python3 scripts/check-state-lag.py`. No route,
OpenAPI, or generated-inventory files are touched, so none are
regenerated.
