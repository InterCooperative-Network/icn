# DecisionRecordedReceipt — Design/Audit Contract

**Status**: draft — design / audit (implementation contract, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-02
**Source basis**: read against `main` @ `d0e87aec`. Code anchors were verified at that commit — re-verify before relying on exact numbers.
**Related**: issues #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2144 / PR #1755 (first `ProcessTransitionReceipt` class, landed) · PR #2275 / PR #2276 (second class — session anchor, landed) · PR #2277 / PR #2278 / PR #2279 (third class — deliberation entry contract, Q3 taxonomy decision, implementation, all landed) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt envelope, Layer 2) · [ADR-0014](../adr/ADR-0014-constitutional-object-model.md) (constitutional object model; decision-receipt provenance) · `ops/ideas/framing/institutional-process-substrate.md` (idea-0019 framing brief) · `docs/spec/effect-dispatch-contract.md` (effect dispatch keyed on the proposal/vote lineage)

> Contract for the fourth institutional-process receipt slice — a
> `DecisionRecordedReceipt` recording that one decision was recorded against
> an already-opened process session. Receipts record institutional facts.
> They grant zero authority. This document decides what the implementation
> must do; it is not the implementation. It names one explicit
> implementation blocker (§10, Q4): the representation of the recorded
> decision and its boundary with the existing proposal/vote decision-receipt
> lineage must be decided narrowly, in writing, before any `:v1` hash layout
> can be pinned — the same discipline #2277 applied to Q3.

## 1. Current implementation audit (verified at `d0e87aec`)

- **Three `ProcessTransitionReceipt` classes exist.**
  `icn/crates/icn-governance/src/proof.rs`: `ProcessGateResultReceipt`
  (~L2159, tag `icn:gov:process_gate_result:v1`, #2144 / PR #1755);
  `ProcessSessionOpenedReceipt` (~L2302, tag
  `icn:gov:process_session_opened:v1`, #2275 / #2276);
  `DeliberationEntryRecordedReceipt` (~L2470, tag
  `icn:gov:deliberation_entry_recorded:v1`, #2277 / #2278 / #2279) with the
  closed ten-kind `DeliberationEntryKind` taxonomy (~L2395). All three are
  wired end to end: proof type → manager seam → `GovernanceReceiptBackend`
  opaque methods with fail-closed defaults
  (`icn/apps/governance/src/receipt_backend.rs`; class constants
  `process_session_opened` ~L22, `deliberation_entry_recorded` ~L45) →
  gateway opaque cascade with atomic uniqueness
  (`icn/crates/icn-gateway/src/receipt_store.rs`, `put_opaque_if_absent`
  ~L1894) → HTTP routes behind `governance:write` + domain membership.
- **A separate, older "decision" receipt lineage exists and is NOT this
  class.** `proof.rs` defines `GovernanceDecisionReceipt` (~L226, tag
  **`icn:gov:decision:v1`** ~L251): the cross-node canonical receipt for a
  **proposal/vote outcome**, carrying `proposal_id`, `domain_id`, `outcome`
  (Accepted / Rejected / NoQuorum), `vote_tally`, `vote_hash`, and a
  canonical `decision_hash`. Its extensions `GovernanceDecisionReceiptV2`
  (~L541, tag `icn:gov:decision:v2`, adds `capability_scope_presented` +
  frozen mandate-attestation taxonomy) and `GovernanceDecisionReceiptV3`
  (~L820, tag `icn:gov:decision:v3`, adds the `ProcessAuthorized`
  attestation mode) are schema-ready (no handler emits them yet).
  `GovernanceDecisionAttestation` (~L1037, tag `icn:gov:attest:v1`) is a
  node-local witness over a canonical decision hash. ADR-0014 pins the
  lineage's authority posture: the receipt is *"authoritative for 'the
  decision happened,' but it does not itself bind authority-to-execute."*
- **The proposal/vote lineage is load-bearing downstream.** Its
  `decision_hash` seeds effect dispatch
  (`docs/spec/effect-dispatch-contract.md`: decision →
  `InstitutionalEffectRecord` → `EffectManifest` → subsystem dispatch), it
  has typed persistence and lookup paths in the gateway `ReceiptStore`
  (by `decision_hash`, `proposal_id`, `domain_id`), it anchors action-card
  linkage for proposal/vote cards, and ADR-0014's `Mandate` provenance
  (`granted_by: { proposal_id, decision_hash }`) points at it. Any new
  class that duplicated or forked these fields would fork an audit lineage
  with 20+ documented external callers.
- **No `DecisionRecordedReceipt` exists anywhere.** The exact strings
  `DecisionRecordedReceipt`, `decision_recorded`, and
  `icn:gov:decision_recorded` appear in no Rust code. The name appears only
  in the idea-0019 framing brief's candidate list, in STATE/PHASE status
  text, and in handoff backlogs — as a class candidate, never a schema. The
  framing brief is explicit: *"These are class candidates, not schemas. The
  brief does **not** lock their shapes."*
- **No `HumanDecisionSet` or `DecisionRecord` object exists.** Both names
  appear only in idea-0019/idea-0020 framing documents. Framing-brief open
  question Q4 (verbatim): *"How does `HumanDecisionSet` relate to the
  existing proposal/vote machinery? Is the existing path one specialization
  of `HumanDecisionSet`, or a parallel surface that the spine names but
  does not absorb?"* — unresolved, undeferred, untriaged before this
  document.
- **The Q4 firewall is already load-bearing in landed contracts.** The Q3
  taxonomy decision (#2278) deferred the `resolution` entry kind from
  deliberation-entry v1 precisely because it *"straddles the input/outcome
  boundary … outcomes are Q4 / `DecisionRecord` territory that this receipt
  class must not absorb"*, reserving discriminant `10`. The deliberation
  class records inputs; the decision node was deliberately left to this
  rung.
- **Name-collision audit — `icn-baseline-lock`.**
  `icn/crates/icn-baseline-lock/src/receipt_types.rs` (test-only fixture
  DAG) has no decision-named variant; its closest shape is
  `DeliberationEntryRecorded { member_pubkey, approve }` under
  `icn:baseline:deliberation_entry:v1` — an approval vote in substance,
  already disambiguated by the #2277 contract. It is not prior art here
  either; it stands as the cautionary contrast for folding vote semantics
  into process receipts.
- **No activation / mutation-plan / mutation-applied / evidence-packet
  receipt classes exist. No full process runtime exists.** Session-open and
  gate-result and deliberation-entry semantics are landed and unchanged.

## 2. Problem

The process substrate now records three institutional facts: a session was
opened, deliberation entries were recorded against it, and gate results were
recorded. But the substrate cannot yet witness the fact those inputs exist
to produce: **that a decision was recorded**. Without a decision node, every
later class in the framing brief's ladder dangles — an
`ActivationCrossedReceipt` would claim the institution moved past a decision
the substrate never witnessed, and a `MutationPlanRecordedReceipt` would
claim a plan-of-record with no recorded decision behind it. The audit trail
would jump from deliberation inputs straight to consequences.

The trap on the other side is just as real: ICN already has a decision
receipt — the proposal/vote `GovernanceDecisionReceipt` lineage
(`icn:gov:decision:v1/v2/v3`), which carries outcome, tally, and vote hash
and seeds effect dispatch. Reusing that lineage for generic process
decisions would silently answer Q4 ("the existing path absorbs the spine");
duplicating its fields in a new class would fork a load-bearing audit
lineage. The framing brief deliberately left this relationship open. A
design rung must therefore do two things: pin the narrow, Q4-independent
shape of the recorded-decision fact, and refuse to pin anything Q4 owns.

## 3. Proposed next slice

Introduce **`DecisionRecordedReceipt`** as the fourth
`ProcessTransitionReceipt` class, mirroring the landed #2276/#2279 pattern
end to end (proof type → manager seam → backend trait method with
fail-closed default → opaque storage cascade → HTTP route → runtime-proof
tests):

- **Receipt-only.** No stored `DecisionRecord` object, no stored
  `HumanDecisionSet` object, no decision read-model beyond the
  session-scoped receipt list (§7).
- **Bound to `(domain_id, session_id)`** — the anchor #2276 created.
- **Requires an already-opened session** (§4, precondition). No silent
  session creation. Deliberation entries are NOT a precondition (§5 —
  whether a decision may be recorded without deliberation is a
  charter/policy question, not a substrate invariant).
- **Records that a decision was recorded — never that it was validly made,
  adopted, binding, or executable.** Bindingness, legitimacy, and
  authority-basis are charter/policy/idea-0020 territory.
- **No outcome, tally, vote, approval, mandate, or execution semantics in
  this slice.** The representation of the decision's content and the
  reference posture toward the proposal/vote lineage are the Q4 blocker
  (§10).

## 4. Proposed `DecisionRecordedReceipt` contract

Proposed domain tag: **`icn:gov:decision_recorded:v1`** — following the
landed class-name → snake_case convention
(`process_session_opened`, `deliberation_entry_recorded`). It must
hash-separate from every existing tag, and in particular it must NEVER
converge with the proposal/vote lineage's `icn:gov:decision:v1/v2/v3`;
the two lineages record different facts and must stay
cryptographically disjoint forever.

Fields — the narrowest safe set, all Q4-independent:

| Field | Type | Notes |
|---|---|---|
| `domain_id` | string | governance domain; same posture as session-opened |
| `session_id` | string | caller-opaque; meaningful only with `domain_id` |
| `decision_id` | string | caller-supplied opaque identifier; unique within `(domain_id, session_id)` |
| `recorded_by` | string (DID) | authenticated actor evidence for **who recorded the fact** — deliberately NOT named `author` or `decider` (§5) |
| `recorded_at` | u64 | record-time stamp; NOT part of identity (§4 duplicates) |
| `body_hash` | 32 bytes | content fingerprint of the decision body; the body itself is never stored (§6 privacy) |
| `record_hash` | 32 bytes | canonical blake3 over the above under the domain tag |

Canonical hashing follows the landed convention exactly: domain tag first,
every string field length-prefixed (u64 LE) to prevent aliasing,
`recorded_at` as LE bytes, and `body_hash` appended raw as a fixed 32-byte
field with no length prefix (the landed convention length-prefixes only
variable-length fields). Equality anchored to `record_hash`. If the Q4
decision adds fields (below), they enter the layout at that rung — this
contract does not pre-assign positions or discriminants.

**Deliberately absent fields:**

- **`decision_kind` / `outcome` — BLOCKED ON Q4 (§10); this is the
  implementation blocker.** Whether the recorded decision carries a typed
  representation (closed kind/outcome taxonomy, ordinal-hashed like the
  landed gate and entry taxonomies) or stays an opaque fingerprinted body
  is exactly the shape of the `DecisionRecord` object Q4 owns. The
  branches produce different canonical hash layouts, so `:v1` cannot be
  pinned without deciding it — and deciding it inside this design rung
  would smuggle an ADR-scale decision past review. Same structural
  argument, same remedy as Q3 → #2278.
- **Reference to the proposal/vote lineage — BLOCKED ON Q4.** An optional
  link (e.g. carrying a `GovernanceDecisionReceipt.decision_hash` when the
  session's decision came from a proposal/vote) looks innocent but is a
  silent Q4 answer: it commits the spine to "names and references, does not
  absorb". Omitting it forever is the opposite answer. Either way it is
  Q4's call, not this contract's. What IS pinned now: this class never
  **duplicates** the lineage's fields — no `proposal_id`, no `vote_tally`,
  no `vote_hash`, no `outcome` enum copied over (§1 fork risk).
- **Deciding-body representation (`HumanDecisionSet`) — BLOCKED ON Q4.**
  Who decided, by what composition, under what authority basis is the
  `HumanDecisionSet` / idea-0020 (`AuthorityBasis`) surface. `recorded_by`
  is deliberately NOT that field (§5).
- **`mandate_attestation` / `capability_scope_presented` — not in this
  class.** Those belong to the proposal/vote lineage's v2/v3 authority
  posture. If the Q4 rung concludes process decisions need attestation
  semantics, that is a Q4 outcome, not a default.
- **`target_ref` — stays deferred (Q1), exactly as #2275 and #2277
  deferred it.** Decisions bind to the session anchor only.
- **References to deliberation entries — deferred.** Linking a decision to
  the entry receipts that preceded it is read-model/evidence-export
  territory (the session-scoped lists already co-locate them); a typed
  entry-ref list would also collide with the Q4 question of what a
  `DecisionRecord` contains. Deferred in writing, same posture as
  threading was for entries.

**Session precondition.** Recording a decision requires that
`get_process_session_opened(domain_id, session_id)` returns an existing
opening receipt; otherwise fail closed with stable error prefix
`decision_recorded_session_not_opened` (HTTP 404, mirroring the landed
absent-anchor posture). Safe without an atomic cross-check for the same
reason as #2277: session openings are permanent append-only facts, so
observe-then-write is monotonic. Gate-result and deliberation-entry
semantics are untouched.

**Duplicate semantics** (mirrors the landed identity rule — idempotency
pinned to stable identity fields, never to the timestamp):

- Same `(domain_id, session_id, decision_id)` **and** same `recorded_by`
  **and** same `body_hash` → idempotent retry: return the ORIGINAL receipt
  (original `recorded_at` and `record_hash`, never restamped).
- Same `(domain_id, session_id, decision_id)` with a **different
  `recorded_by` or `body_hash`** → fail-closed conflict, stable prefix
  `decision_recorded_conflict`, surfaced as HTTP 409.
- **Contract-sync instruction for the Q4 rung**: any field Q4 adds to the
  `:v1` layout (kind, outcome, lineage reference, deciding-body handle)
  MUST also join stable duplicate identity — a mismatch on a
  hash-participating field must 409, never silently return the original.
  This is the #2278 `entry_kind` lesson, recorded in advance.
- Uniqueness must be atomic under concurrency, reusing the landed
  `put_opaque_if_absent` unique-marker pattern (§6). Multiple decisions per
  session are permitted at the substrate layer — `decision_id` is the unit
  of uniqueness, not the session. Whether an institution allows one, many,
  or zero decisions per session is charter policy, not substrate shape.
  This design PR does not implement any of it.

## 5. Authority discipline

- Recording a decision **grants zero authority**. The receipt proves the
  institution recorded that a decision exists — at a specific time,
  fingerprinting specific content, attributed to a specific authenticated
  recorder. It does not prove the decision was validly made, that quorum
  existed, that the deciding body was legitimate, or that anything may be
  executed. ADR-0014 already draws this line for the proposal/vote lineage
  ("does not itself bind authority-to-execute"); this class sits strictly
  below even that: it does not carry an outcome at all in this contract.
- **`recorded_by` is recorder evidence, not decider identity.** In real
  institutional processes the person who records a decision (facilitator,
  secretary, clerk) is routinely not the deciding body. Naming this field
  `author` or `decider` would quietly conflate recording with deciding —
  the exact conflation Q4 and idea-0020 (`AuthorityBasis`) exist to keep
  legible. Who decided is `HumanDecisionSet` territory (§4, §10).
- **No deliberation precondition.** The substrate does not require
  deliberation entries before a decision may be recorded. Whether a charter
  requires deliberation, a facilitator summary, an accessibility review, or
  a privacy review before deciding is gate/charter territory — that is what
  `ProcessGateResultReceipt` records. Encoding process-completeness rules
  into the receipt would turn a record of fact into an enforcement gate.
- Route posture: mirror the deliberation-entry route exactly —
  `governance:write` scope + `check_domain_membership` (403 non-member),
  400 on whitespace/empty ids, 404 on unopened session, 409 on conflict.
  **Do not invent a new production permission**: no `decision:*` capability
  exists today, and creating one is a policy design question, not a receipt
  slice. Proposed route shape:
  `POST /gov/domains/{domain_id}/process-sessions/{session_id}/decisions/{decision_id}/record`
  — distinct from every existing proposal/vote decision surface (the
  gateway's decision-index endpoints serve the `icn:gov:decision:vN`
  lineage and are untouched).

## 6. Storage / persistence / privacy discipline

- **Privacy: the receipt never stores the body.** Decision bodies can carry
  sensitive institutional content (personnel matters, conflict outcomes,
  privacy-review conclusions). The receipt stores a caller-supplied 32-byte
  `body_hash` fingerprint; where the body lives and who may read it are
  visibility-policy questions this slice does not touch. The receipt proves
  a decision was recorded; it does not prove all audiences may read it. The
  privacy/redaction evidence-export run remains an open #1748 gate,
  untouched by this slice.
- **Opaque cascade, fail closed.** New apps-layer class constant
  (proposed: `decision_recorded`) routed through the existing
  `GovernanceReceiptBackend` opaque methods with fail-closed defaults.
  Backend errors abort recording; no partial writes; append-only; no
  overwrite. **Explicitly NOT the typed `GovernanceDecisionReceipt` store**:
  the gateway's typed decision persistence (keyed by `decision_hash` /
  `proposal_id`) belongs to the proposal/vote lineage and is not touched,
  extended, or shared.
- **Key layout — same three-dimensions-over-two-keys solution as #2279.**
  `key1` = injective netstring-style composite of `(domain_id, session_id)`
  (same encoding discipline as the landed
  `deliberation_entry_composite_key1`; whether the implementation shares a
  generalized helper or adds a sibling is an implementation detail, but the
  encoding MUST be injective and covered by an anti-aliasing test), `key2`
  = `decision_id`. Per-session decision list is one `list_opaque_for`
  call; cross-domain mixing impossible by construction; the landed
  `put_opaque_if_absent` enforces per-decision uniqueness with no gateway
  change.
- **`decision_id` is caller-supplied and opaque**, like `session_id` and
  `entry_id`. Server-derived ids were already considered and rejected in
  the #2277 contract (restamp-on-retry trap; silent merging); the same
  arguments apply unchanged. Collision resistance is enforced at the
  storage layer (atomic uniqueness + conflict), not assumed of the id.

## 7. Query / read behavior

- Primary read: list decision receipts for `(domain_id, session_id)` in the
  store's deterministic chronological order — `(recorded_at, record_hash)`,
  exactly the landed `list_opaque_for` contract; explicitly NOT
  arrival/insertion order. Never list by `session_id` alone — session ids
  are not globally unique (#2276 pinned this; same rule here).
- Point read: get one decision receipt by
  `(domain_id, session_id, decision_id)`.
- Reads return receipts (ids, recorder, timestamps, hashes) — never body
  content, which the store does not hold.
- No cross-session decision feed, no per-recorder feed, no global decision
  log. The institutional read is session-scoped. Cross-referencing
  decisions with the proposal/vote lineage's typed reads is Q4-posture
  territory and out of scope.
- Future evidence export consumes the domain-scoped session read; export
  redaction discipline stays with the #1748 privacy gate, not this slice.

## 8. Test matrix for the future implementation PR

1. Canonical hash stable for a fixed input vector (golden vector required,
   as #2278 required for the entry class).
2. Each identity field (`domain_id`, `session_id`, `decision_id`,
   `recorded_by`, `body_hash`) independently changes `record_hash`.
3. Family-tag separation: identical field bytes under
   `icn:gov:deliberation_entry_recorded:v1`,
   `icn:gov:process_session_opened:v1`, and
   `icn:gov:decision_recorded:v1` produce different hashes; plus an
   explicit test that the tag is disjoint from `icn:gov:decision:v1`
   (proposal/vote lineage) with a comment stating the two lineages must
   never converge.
4. Length-prefix anti-aliasing: shifting bytes between adjacent string
   fields changes the hash.
5. Composite-key anti-aliasing at the storage layer: `("ab", "c")` and
   `("a", "bc")` as `(domain_id, session_id)` never collide in `key1`.
6. Empty/whitespace `domain_id` / `session_id` / `decision_id` /
   `recorded_by` rejected before persistence.
7. Raw body is never persisted: the stored payload round-trips to the
   receipt struct and contains only `body_hash`; no outcome, tally, vote,
   proposal, or mandate field exists anywhere in the payload.
8. Recording against a never-opened `(domain_id, session_id)` fails closed
   with `decision_recorded_session_not_opened`; nothing persisted.
9. Same-identity retry returns the ORIGINAL receipt: `recorded_at` and
   `record_hash` identical to the first insert; exactly one persisted
   record.
10. Same `decision_id`, different `recorded_by` →
    `decision_recorded_conflict` / 409; original untouched.
11. Same `decision_id` + `recorded_by`, different `body_hash` → conflict /
    409; original untouched.
12. Concurrent duplicate race (multi-thread, same identity): exactly one
    winner persisted; losers observe the winner.
13. Multiple distinct `decision_id`s under one session all persist and list
    deterministically; two domains sharing a `session_id` never mix.
14. Backend failure fails closed; the manager surfaces the error; nothing
    half-written.
15. Existing session-open, gate-result, and deliberation-entry behavior
    unchanged (their suites still green, byte-identical semantics);
    existing proposal/vote decision receipt behavior unchanged (typed
    store, effect dispatch, action cards untouched).
16. Route: 200 on member record; idempotent retry returns identical body;
    409 different-recorder; 403 non-member with nothing persisted; 400
    whitespace ids; 404 unopened session.
17. Vocabulary: receipt/docs/tests contain no settlement-prohibited terms,
    no vote/ballot/tally/approve vocabulary as API/field names in this
    class, and no NYCN-specific vocabulary in generic core.
18. Docs/claim lint clean (`doc_control_check.py`, state-lag guard).

## 9. Explicit non-goals

No runtime implementation in this PR. No Q4 decision in this PR (§10 —
this document triages Q4 as the blocker; it does not resolve it). No stored
`DecisionRecord` or `HumanDecisionSet` object. No outcome, tally, vote,
approval, quorum, or bindingness semantics. No change to the proposal/vote
`GovernanceDecisionReceipt` lineage, its typed store, effect dispatch, or
action cards. No activation-crossed / mutation-plan / mutation-applied /
evidence-packet classes. No accessibility/privacy gate completion. No
charter/CCL sequencing. No served-OpenAPI publication (a family-level
decision for all process receipts, explicitly declined as a side effect of
receipt rungs — see #2279 review record). No #1748 closure; no #2141
closure; no #2081/#2080/#2274 work; no process-runtime, Phase-2,
production, pilot, member/organizer-readiness, live-federation,
service-hosting, or SDK/OpenAPI completeness claims.

## 10. Open-question triage (Q4, and its neighbors)

- **Q4 — `HumanDecisionSet` / `DecisionRecord` vs proposal/vote: NEEDS A
  DESIGN DECISION; it is the implementation blocker for this class.**
  Triage outcome: neither promote-to-RFC nor reject nor park — Q4 blocks
  this specific slice and must be closed first, narrowly. The decision
  rung must answer, in writing:
  **(a)** the representation of the recorded decision in `:v1` — opaque
  fingerprinted body only, or a typed kind/outcome taxonomy (and if typed:
  closed-by-ADR with ordinal hashing per the landed pattern, initial list,
  evolution rule);
  **(b)** the reference posture toward the `icn:gov:decision:vN`
  proposal/vote lineage — absorb (the existing path becomes one
  specialization), reference (an optional lineage link field), or parallel
  (the spine names but never links) — the framing brief's exact question;
  **(c)** whether the deciding body gets a v1 handle (a minimal
  `HumanDecisionSet` reference) or stays out of the receipt entirely;
  **(d)** the fate of the deferred `resolution` deliberation-entry kind
  (discriminant `10` reserved by #2278) — whether a sharp input-only
  definition becomes possible once the decision node exists, or it stays
  deferred.
  Each branch of (a) and (b) changes the `:v1` hash layout or the class's
  lineage relationships, so the decision precedes implementation, not
  follows it.
- **Q1 — `ProcessTargetRef` polymorphism: stays deferred**, exactly as
  #2275 and #2277 deferred it. This class binds to the session anchor
  only; no `target_ref` field (§4). Note the pressure is rising: a
  decision that cannot say what it is *about* leans harder on Q1 than an
  entry does — the Q4 rung should say whether Q1 must be co-triaged or
  can stay independent.
- **Q3 — entry-kind taxonomy: CLOSED by #2278.** Nothing here reopens it.
  The `resolution` deferral hand-off is item (d) above.

## 11. Recommendation

**Option C — defer implementation until Q4 is narrowly closed; everything
else in this contract is implementation-ready.**

- **Option A** (receipt-only `DecisionRecordedReceipt`, opaque body-hash
  v1, no typed outcome) is the likely implementation shape — but shipping
  it now was considered and rejected: an outcome-less, reference-less v1
  silently decides Q4 branches (a) and (b) toward "opaque and parallel"
  without the decision being reviewed as such, and if Q4 later lands
  typed-or-referencing, the tag bumps immediately. The #2277 precedent is
  exact: the field the class exists to record must be decided before `:v1`
  pins the layout.
- **Option B** (fold decision recording into the existing proposal/vote
  lineage, e.g. a v4 of `GovernanceDecisionReceipt`) is rejected: it
  absorbs Q4 by accident in the other direction, imports outcome/tally
  semantics the process substrate must not carry, and couples the process
  spine to a lineage with live effect-dispatch consumers.
- **Concrete next rung**: a narrow Q4 decision document (small ADR or a
  sibling decision doc like `deliberation-entry-kind-taxonomy.md`,
  whichever review prefers) answering §10 (a)–(d). After it lands,
  implementation proceeds as one focused PR against this contract
  (§4–§8), mirroring how #2279 implemented #2277 + #2278.

## 12. Validation (this document's PR)

Docs-only change set: this file, a `docs/registry.toml` row, a
`docs/INDEX.md` link, and the regenerated `docs/DOCUMENT_REGISTRY.md`
(regeneration after registry row additions is required by that file's
header). Validation: `git diff --check`, `python3
docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml`
(no new warnings), `python3 scripts/check-state-lag.py`. No route, OpenAPI,
or generated-inventory files are touched, so none are regenerated.
