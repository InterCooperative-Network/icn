# DeliberationEntryRecordedReceipt — Design/Audit Contract

**Status**: draft — design / audit (implementation contract, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-02
**Source basis**: read against `main` @ `5e1b7c97`. Code anchors were verified at that commit — re-verify before relying on exact numbers.
**Related**: issues #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2144 / PR #1755 (first `ProcessTransitionReceipt` class, landed) · PR #2275 / PR #2276 (second class — session anchor contract + implementation, landed) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt envelope, Layer 2) · `ops/ideas/framing/institutional-process-substrate.md` (idea-0019 framing brief) · `ops/ideas/dogfood/institutional-process-substrate-mvp.md`

> Contract for the third institutional-process receipt slice — a
> `DeliberationEntryRecordedReceipt` recording that one deliberation entry was
> recorded against an already-opened process session. Receipts record
> institutional facts. They grant zero authority. This document decides what
> the implementation must do; it is not the implementation. It originally
> named one explicit implementation blocker (§10, Q3) — **decided by #2278**
> (`docs/design/deliberation-entry-kind-taxonomy.md`); §4 carries the
> contract-sync notes the implementation follows.

## 1. Current implementation audit (verified at `5e1b7c97`)

- **`ProcessGateResultReceipt` exists** (first `ProcessTransitionReceipt`
  class; #2144 / PR #1755). `icn/crates/icn-governance/src/proof.rs` (~L2159),
  domain tag `icn:gov:process_gate_result:v1`, closed six-gate taxonomy hashed
  by ordinal, blake3 canonical hash with length-prefixed (u64 LE) string
  fields, equality anchored to `record_hash`.
- **`ProcessSessionOpenedReceipt` exists** (second class; contract PR #2275
  `31bb52b0`, implementation PR #2276 `5e1b7c97`). `proof.rs` (~L2302), domain
  tag `icn:gov:process_session_opened:v1`. It anchors a process session by
  `(domain_id, session_id)`: atomic uniqueness is enforced inside the storage
  transaction (gateway `ReceiptStore::put_opaque_if_absent`,
  `icn/crates/icn-gateway/src/receipt_store.rs` ~L1894, point-keyed unique
  marker `receipt:opaque:unique:` ~L207); a same-opener retry returns the
  ORIGINAL receipt (never restamped); a different-opener duplicate is a
  fail-closed conflict (stable prefix `process_session_open_conflict`,
  HTTP 409). Route `POST /gov/domains/{domain_id}/process-sessions/{session_id}/open`
  (`icn/apps/governance/src/http/configure.rs` ~L692) behind
  `governance:write` + domain membership (403 non-member). Backend read
  `get_process_session_opened` exists
  (`icn/apps/governance/src/receipt_backend.rs` ~L766/812).
- **Domain-scoped gate-result reads exist**
  (`list_process_gate_results_for_session_in_domain`, `receipt_backend.rs`
  ~L737): session ids are NOT globally unique; two domains sharing a
  `session_id` never mix.
- **No `DeliberationEntryRecordedReceipt` exists in the governance path.** No
  proof type, no backend class, no route, no manager seam.
- **No stored `DeliberationThread` or `DeliberationEntry` runtime object
  exists.** The names appear only in idea-0019/idea-0020 framing and dogfood
  documents (read-model fixture walks, explicitly "not runtime") and in test
  documentation comments.
- **Name-collision audit — `icn-baseline-lock`.**
  `icn/crates/icn-baseline-lock/src/receipt_types.rs` defines
  `BaselineReceiptBody::DeliberationEntryRecorded { member_pubkey, approve }`
  under domain tag `icn:baseline:deliberation_entry:v1`. That crate is a
  deterministic signed fixture DAG for the executable-baseline loop
  (test-only, per its own module doc), and its "deliberation entry" is in
  substance an approval vote consumed by a projector. It is **not** prior art
  for this class: different namespace (`icn:baseline:` vs `icn:gov:`),
  different envelope (hash-chained signed DAG vs ADR-0026 Layer 2 opaque
  cascade), different semantics. Two consequences bind this design: the
  governance class must NOT carry an approve/vote flag (decision recording is
  Q4 / `DecisionRecord` territory, §10), and the two domain tags must never
  converge.
- **No decision / mutation-plan / evidence-packet receipt ladder exists. No
  full process runtime exists.** Gate-result recording neither requires nor
  silently creates opened sessions (#2276 preserved that).

## 2. Problem

Process sessions can now be opened as recorded institutional facts, and gate
results can be recorded against session ids. But there is no generic receipt
for recording **deliberation input**. Without one, any future
`DecisionRecord` would claim to conclude a discussion the substrate never
witnessed — the audit trail would begin at the decision, not at the
institutional input that produced it.

At the same time, generic ICN core must not become a chat system, a comments
feature, a social feed, or moderation infrastructure. The framing brief is
explicit: deliberation is a **structured institutional record**, not
free-form text. The receipt slice must record that an entry was recorded —
by whom, when, against which session, with which content fingerprint — and
nothing more.

## 3. Proposed next slice

Introduce **`DeliberationEntryRecordedReceipt`** as the third
`ProcessTransitionReceipt` class, mirroring the landed #2144 / #2276 pattern
end to end (proof type → manager seam → backend trait method with fail-closed
default → opaque storage cascade → HTTP route → runtime-proof tests):

- **Receipt-only.** No stored `DeliberationThread` object, no stored
  `DeliberationEntry` object, no read-model beyond the session-scoped receipt
  list (§7). A thread read-model can come later when a consumer needs it; the
  receipt itself establishes the first audit trail.
- **Bound to `(domain_id, session_id)`** — the anchor #2276 created.
- **Requires an already-opened session** (§4, precondition). No silent
  session creation.
- **No lifecycle, no decision outcome, no mutation authorization, no evidence
  export, no accessibility/privacy gate completion** in this slice.

## 4. Proposed `DeliberationEntryRecordedReceipt` contract

Proposed domain tag: **`icn:gov:deliberation_entry_recorded:v1`** (new family
tag; must hash-separate from both `icn:gov:process_session_opened:v1` and
`icn:baseline:deliberation_entry:v1`).

Fields — the narrowest safe set, all Q3-independent:

| Field | Type | Notes |
|---|---|---|
| `domain_id` | string | governance domain; same posture as session-opened |
| `session_id` | string | caller-opaque; meaningful only with `domain_id` |
| `entry_id` | string | caller-supplied opaque identifier; unique within `(domain_id, session_id)` |
| `author` | string (DID) | authenticated actor evidence; grants nothing (§5) |
| `entry_kind` | closed enum | #2278 v1 taxonomy (ten kinds, `resolution` deferred); hash-only `u8` discriminant, serde `snake_case` wire — see `deliberation-entry-kind-taxonomy.md` |
| `recorded_at` | u64 | record-time stamp; NOT part of identity (§4 duplicates) |
| `body_hash` | 32 bytes | content fingerprint of the entry body; the body itself is never stored (§6 privacy) |
| `record_hash` | 32 bytes | canonical blake3 over the above under the domain tag |

Canonical hashing follows the landed convention exactly: domain tag first,
every string field length-prefixed (u64 LE) to prevent aliasing, then one
explicit `u8` discriminant byte for `entry_kind` (per #2278), `recorded_at`
as LE bytes, and `body_hash` appended raw as a fixed 32-byte field — **no
length prefix**, because the landed convention length-prefixes only
variable-length fields (see `MandateGrantRef::compute_ref_hash`, which
appends its fixed 32-byte `decision_hash` and 16-byte UUID without prefixes;
a fixed-size field cannot alias). Equality anchored to `record_hash`.

**Deliberately absent fields:**

- **`entry_kind` — DECIDED (contract-sync note, post-#2278).** This field
  was originally blocked on Q3: the closed-vs-charter-extensible branches
  produce different canonical hash layouts, so `:v1` could not be pinned
  without deciding it, and deciding it silently inside this design rung
  would have smuggled an ADR-scale decision past review. The decision
  landed as `docs/design/deliberation-entry-kind-taxonomy.md` (#2278):
  closed, ADR-controlled enum, explicit `u8` discriminants 0–9 for the
  ten-kind v1 list (`resolution` deferred as Q4-ambiguous), hash-only
  discriminant with serde `snake_case` wire form, append-only evolution,
  retired kinds decodable forever. `entry_kind` is now a v1 field (table
  above) and **participates in duplicate identity** (below).
- **`parent_entry_id` — deferred in writing.** Threading implies
  discussion-structure semantics that belong to a future read-model, not to
  the recorded fact. Adding it later is a new field in a new tag version, not
  a break.
- **`target_ref` — stays deferred (Q1), exactly as #2275 deferred it.**
  Entries bind to the session anchor only. Binding entries to targets would
  import Q1 into this slice.
- **No approve/vote/outcome flag of any kind** (§1 baseline-lock contrast;
  Q4 territory).

**Session precondition.** Recording an entry requires that
`get_process_session_opened(domain_id, session_id)` returns an existing
opening receipt; otherwise fail closed with stable error prefix
`deliberation_entry_session_not_opened`. This check is safe without an atomic
cross-check: session openings are permanent append-only facts (never revoked,
never overwritten), so observe-then-write is monotonic — a session observed
open stays open. Gate-result recording keeps its landed semantics (neither
requires nor creates sessions); the precondition applies to this new class
only, where there is no legacy behavior to preserve.

**Duplicate semantics** (mirrors the #2275-revised identity rule —
idempotency pinned to stable identity fields, never to the timestamp):

- Same `(domain_id, session_id, entry_id)` **and** same `author` **and** same
  `body_hash` **and** same `entry_kind` (post-#2278: the kind participates in
  stable identity) → idempotent retry: return the ORIGINAL receipt (original
  `recorded_at` and `record_hash`, never restamped).
- Same `(domain_id, session_id, entry_id)` with a **different `author`,
  `body_hash`, or `entry_kind`** → fail-closed conflict, stable prefix
  `deliberation_entry_conflict`, surfaced as HTTP 409 — a kind mismatch whose
  canonical hash differs must never be swallowed as a retry.
- Uniqueness must be atomic under concurrency, reusing the landed
  `put_opaque_if_absent` unique-marker pattern (§6). This design PR does not
  implement it.

## 5. Authority discipline

- Recording a deliberation entry **grants zero authority**. The receipt
  proves the institution recorded an input at a specific time attributed to a
  specific authenticated actor — nothing about whether the input must be
  heeded, counted, or acted on.
- `author` is actor evidence, not entitlement. Whether an actor is *supposed*
  to deliberate in a given session is a charter/policy question, out of scope
  here (the framing brief places entry-kind requirements and speaker rules in
  charters, interpreted by apps and policy oracles).
- Route posture: mirror the session-open route exactly — `governance:write`
  scope + `check_domain_membership` (403 non-member), 400 on
  whitespace/empty ids, 409 on conflict. **Do not invent a new production
  permission**: no `deliberation:*` capability exists today, and creating one
  is a policy design question, not a receipt slice. A future narrower
  capability may replace `governance:write` here; that is explicitly future
  work.

## 6. Storage / persistence / privacy discipline

- **Privacy: the receipt never stores the body.** Deliberation entries can
  carry the most sensitive content the system records (objections, conflict
  signals, privacy reviews — framing brief, "Privacy and boundary risks").
  The receipt stores a 32-byte `body_hash` fingerprint supplied by the
  caller; where the body lives (private overlay per #1730, app storage) and
  who may read it are visibility-policy questions this slice does not touch.
  The receipt proves an entry existed; it does not prove all audiences may
  read it. The privacy/redaction evidence-export run remains an open #1748
  gate, untouched by this slice.
- **Opaque cascade, fail closed.** New apps-layer class constant
  (proposed: `deliberation_entry_recorded`) routed through the existing
  `GovernanceReceiptBackend` opaque methods with fail-closed defaults
  (`opaque_storage_not_implemented` sentinel posture unchanged). Backend
  errors abort recording; no partial writes; append-only; no overwrite.
- **Key layout — three logical dimensions over a two-key store.** The opaque
  store keys on `(class, key1, key2)`. Entries need `(domain_id, session_id,
  entry_id)`. Recommended: **`key1` = injective composite of
  `(domain_id, session_id)`** — length-prefixed/netstring-style encoding
  (e.g. `"{len(domain_id)}:{domain_id}{session_id}"`), never bare
  concatenation — and **`key2` = `entry_id`**. This makes the session's entry
  list a single `list_opaque_for(class, key1)` call, makes cross-domain
  mixing impossible by construction, and lets the landed
  `put_opaque_if_absent` enforce per-entry uniqueness with no gateway change.
  The encoding MUST be injective (anti-aliasing test required: §8). The
  alternative — `key1 = domain_id` with payload-side session filtering, as
  the #2276 domain-scoped gate read does — is workable but scans every entry
  in the domain per session read; acceptable for gate results (small,
  legacy-keyed), wrong default for the class whose primary read is the
  per-session list.
- **`entry_id` is caller-supplied and opaque**, like `session_id`. A
  server-derived id was considered and rejected: deriving from `recorded_at`
  reproduces the restamp-on-retry trap #2275's review caught, and deriving
  from `(author, body_hash)` silently merges an author's distinct entries
  that share identical content. Collision resistance is therefore enforced at
  the storage layer (atomic uniqueness + conflict), not assumed of the id.

## 7. Query / read behavior

- Primary read: list entry receipts for `(domain_id, session_id)` in the
  store's **deterministic chronological order** — sorted by
  `(recorded_at, record_hash)`, which is what `list_opaque_for` returns
  today. This is explicitly NOT arrival/insertion order: entries sharing a
  `recorded_at` second order by `record_hash`. If strict insertion order
  ever becomes contractually required (e.g. deliberation replay), that
  needs an explicit sequence field — deferred alongside threading (§4); no
  consumer in this slice requires it. Never list by `session_id` alone —
  session ids are not globally unique (#2276 pinned this; same rule here).
- Point read: get one entry receipt by `(domain_id, session_id, entry_id)`.
- Reads return receipts (ids, author, timestamps, hashes) — never body
  content, which the store does not hold.
- No cross-session feed, no per-author feed, no global timeline. A feed is a
  social-media shape; the institutional read is session-scoped.
- Future evidence export consumes the domain-scoped session read; export
  redaction discipline stays with the #1748 privacy gate, not this slice.

## 8. Test matrix for the future implementation PR

1. Canonical hash stable for a fixed input vector.
2. Each identity field (`domain_id`, `session_id`, `entry_id`, `author`,
   `body_hash`) independently changes `record_hash`.
3. Family-tag separation: identical field bytes under
   `icn:gov:process_session_opened:v1` and under
   `icn:gov:deliberation_entry_recorded:v1` produce different hashes; note in
   the test why `icn:baseline:deliberation_entry:v1` (test-only fixture DAG)
   is a different lineage and must never share a tag.
4. Length-prefix anti-aliasing: shifting bytes between adjacent string fields
   changes the hash.
5. Composite-key anti-aliasing at the storage layer: `("ab", "c")` and
   `("a", "bc")` as `(domain_id, session_id)` never collide in `key1`.
6. Empty/whitespace `domain_id` / `session_id` / `entry_id` / `author`
   rejected before persistence.
7. Raw body is never persisted: the stored payload round-trips to the receipt
   struct and contains only `body_hash`, no body field anywhere.
8. Recording against a never-opened `(domain_id, session_id)` fails closed
   with `deliberation_entry_session_not_opened`; nothing persisted.
9. Same-identity retry returns the ORIGINAL receipt: `recorded_at` and
   `record_hash` identical to the first insert; exactly one persisted record.
10. Same `entry_id`, different `author` → `deliberation_entry_conflict` /
    409; original untouched.
11. Same `entry_id` + `author`, different `body_hash` → conflict / 409;
    original untouched.
12. Concurrent duplicate race (multi-thread, same identity): exactly one
    winner persisted; losers observe the winner.
13. Backend failure fails closed; the manager surfaces the error; nothing
    half-written.
14. Existing session-open behavior unchanged (its suite still green,
    byte-identical semantics); existing gate-result behavior unchanged
    (still neither requires nor creates sessions).
15. Domain-scoped entry list does not mix two domains sharing a
    `session_id`; list order is deterministic `(recorded_at, record_hash)`
    and stable across re-reads, including the same-`recorded_at` hash-tiebreak
    case (documented as chronological order, not arrival order).
16. Route: 200 on member record; idempotent retry returns identical body;
    409 different-author; 403 non-member with nothing persisted; 400
    whitespace ids.
17. Vocabulary: receipt/docs/tests contain no settlement-prohibited terms,
    no chat/social-media/moderation vocabulary (no "chat", "comment",
    "post", "like", "follower", "moderate" as API/field names), and no
    NYCN-specific vocabulary in generic core.
18. Docs/claim lint clean (`doc_control_check.py`, state-lag guard).

## 9. Explicit non-goals

No runtime implementation in this PR. No stored `DeliberationThread` or
`DeliberationEntry` object. No discussion system, no chat, no comments, no
moderation, no social feed. No decision records, no mutation plans, no
evidence packets, no action-card triggers. No accessibility/privacy gate
completion. No charter/CCL sequencing. No #1748 closure; no #2141 closure; no
#2081/#2080/#2274 work; no process-runtime, Phase-2, production, pilot,
member/organizer-readiness, live-federation, service-hosting, or OpenAPI
completeness claims.

## 10. Open-question triage (Q3, and its neighbors)

- **Q3 — `DeliberationEntry` kind taxonomy: NEEDS A DESIGN DECISION; it is
  the implementation blocker for this class.** Triage outcome: neither
  promote-to-RFC nor reject nor park — Q3 blocks this specific slice and
  must be closed first, narrowly. The decision required: **(a)** closed
  taxonomy locked by an ADR (enum, ordinal-hashed like the landed gate
  taxonomy, with an explicit append-only evolution rule), or **(b)**
  charter-extensible kinds (length-prefixed string, charter-namespace
  discipline, and a statement of what the accessibility/privacy gates may
  assume). The framing brief leans closed ("the closed-taxonomy approach is
  deliberate") but explicitly withholds commitment; the tradeoff is real —
  closed is safer for accessibility/privacy gate handles, extensible is
  safer for institutional variation. The decision changes the `:v1` hash
  layout, so it precedes implementation, not follows it.
- **Q1 — `ProcessTargetRef` polymorphism: stays deferred**, exactly as
  #2275 deferred it. This class binds to the session anchor only; no
  target_ref field (§4).
- **Q4 — `HumanDecisionSet` vs proposal/vote: untouched.** This receipt
  records input facts, not approvals or outcomes. The baseline-lock fixture
  (whose "deliberation entry" carries an approve flag) is the cautionary
  contrast: folding decision semantics into entry recording would pre-answer
  Q4 by accident.

## 11. Recommendation

**Option C — defer implementation until Q3 is narrowly closed; everything
else in this contract is implementation-ready.**

- **Option A** (receipt-only `DeliberationEntryRecordedReceipt`, no stored
  thread) is the right implementation shape — but only after the Q3 decision
  pins the `entry_kind` representation. Shipping v1 without `entry_kind`
  was considered and rejected: the framing brief defines a deliberation
  entry as a *typed* entry (the kind is the handle the accessibility gate,
  privacy review, and conflict routing act on), so an untyped v1 would
  record the wrong fact and force a tag bump immediately after Q3 closes.
  Shipping a one-kind or free-string `entry_kind` was also rejected: both
  silently decide Q3 (closed-by-default or extensible-by-default
  respectively) without the decision being reviewed as such.
- **Option B** (receipt + minimal thread read-model) is rejected for now: no
  consumer exists, and a read-model can be added later without changing the
  receipt.
- **Concrete next rung**: a narrow Q3 decision document (small ADR or an
  addendum section to this contract, whichever review prefers) choosing
  closed-vs-extensible and, if closed, the initial kind list and evolution
  rule. After it lands, implementation proceeds as Option A in one focused
  PR against this contract (§4–§8), mirroring how #2276 implemented #2275.

## 12. Validation (this document's PR)

Docs-only change set: this file, a `docs/registry.toml` row, a
`docs/INDEX.md` link, and the regenerated `docs/DOCUMENT_REGISTRY.md`
(regeneration after registry row additions is required by that file's
header). Validation: `git diff --check`, `python3
docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml`
(no new warnings), `python3 scripts/check-state-lag.py`. No route, OpenAPI,
or generated-inventory files are touched, so none are regenerated.
