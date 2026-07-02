# ProcessSession Receipt Anchor — Design/Audit Contract

**Status**: draft — design / audit (implementation contract, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-02
**Source basis**: read against `main` @ `3e387b1f`. Code anchors were verified at that commit — re-verify before relying on exact numbers.
**Related**: issues #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2144 / PR #1755 (first `ProcessTransitionReceipt` class, landed) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt envelope, Layer 2) · `ops/ideas/framing/institutional-process-substrate.md` (idea-0019 framing brief) · `ops/ideas/dogfood/institutional-process-substrate-mvp.md`

> **This document defines the implementation contract for the next
> institutional-process receipt slice — a `ProcessSessionOpenedReceipt` anchor —
> before any code touches this area.** It changes no runtime behavior. Receipts
> record institutional facts; they grant no authority, and nothing in this design
> makes opening a session a permission.

## 1. Current implementation audit (verified at `3e387b1f`)

What exists:

- **`ProcessGateResultReceipt`** (`icn-governance/src/proof.rs` ~L2091+) — the
  first `ProcessTransitionReceipt` class (ADR-0026 Layer 2), landed by #2144/PR
  #1755. Closed six-variant `ProcessGateKind`, closed `Pass`/`Fail` result,
  blake3 canonical `record_hash` under domain-separation tag
  `icn:gov:process_gate_result:v1` (length-prefixed strings, ordinal-hashed
  enums), equality anchored to `record_hash`.
- **`GovernanceManager::record_process_gate_result`** (`apps/governance/src/manager.rs`
  ~L5794+) — validates non-empty `session_id`, constructs the receipt, persists
  through `GovernanceReceiptBackend::put_process_gate_result`
  (`apps/governance/src/receipt_backend.rs`), fail-closed on backend rejection;
  append-only; re-records of the same `(session_id, gate_kind)` accumulate and
  are read back ordered by `recorded_at`.
- **HTTP surface** — `POST /domains/{domain_id}/process-sessions/{session_id}/gate-results`
  (`apps/governance/src/http/configure.rs` ~L684), with runtime-proof tests
  (`process_gate_result_receipt_runtime_slice.rs`, `process_gate_result_http_route.rs`).
- **Opaque persistence** (#1757–#1759) — gate-result receipts flow into the
  gateway's sled-backed opaque `ReceiptStore` cascade without widening
  `icn-gateway`'s typed governance imports (meaning firewall preserved).

What does **not** exist:

- `session_id` is **caller-provided opaque text**. The manager's own doc-comment
  states the runtime "does not (yet) model the surrounding `ProcessSession` as a
  stored object."
- No `ProcessSessionOpenedReceipt` (or any of the other six receipt classes the
  framing brief names).
- No stored or read-model `ProcessSession` object; no session lifecycle; no
  charter/CCL-governed gate sequencing at this layer; no full process runtime.

## 2. Problem

Gate-result receipts can exist for a `session_id` that nothing ever opened. The
identifier is a convention between callers, not an institutional record: there
is no receipt attesting *who opened this process run, in which domain, when*.
Every later spine object the framing brief binds to a session — deliberation
entries, decision records, mutation plans, evidence packets, the privacy/
visibility run and the accessibility gate that #1748 still lists as open — needs
a stable anchor to attach to. Without a session-open receipt, evidence export
has no starting boundary and cross-session audit reconstruction depends on
string discipline rather than recorded fact.

## 3. Proposed next slice

Design first (this document), then one narrow future implementation PR:
introduce **`ProcessSessionOpenedReceipt`** as the second
`ProcessTransitionReceipt` class under the existing ADR-0026 Layer 2 envelope —
generic, domain-bound, mirroring the landed gate-result pattern end to end
(proof type → manager seam → receipt backend → opaque cascade → HTTP route →
runtime-proof tests). No stored `ProcessSession` object in this slice (see §11);
no workflow engine; no charter/CCL evaluator; no NYCN-specific semantics in
generic core.

## 4. Proposed `ProcessSessionOpenedReceipt` contract

| Field | Type | Semantics |
|---|---|---|
| `session_id` | `String` | The identifier this receipt anchors. Non-empty; byte-preserved; still opaque to the runtime. |
| `domain_id` | `String` | Governance domain the session is scoped to. Non-empty; bound into the canonical hash. |
| `opened_by` | `String` (DID) | The authenticated caller of the manager method. Actor evidence only — see §5. |
| `opened_at` | `u64` | Unix-seconds timestamp. |
| `record_hash` | `[u8; 32]` | blake3 canonical hash of the fields above under a new domain tag `icn:gov:process_session_opened:v1`, using the landed length-prefix convention. Equality anchors here. |

**Deliberately deferred fields** (mark as deferred in the implementation PR's
doc-comments; do not invent):

- `target_ref` — the framing brief binds a session to one `ProcessTargetRef`,
  but #1748's Q1 (target-ref polymorphism) is unresolved. **This document is the
  written deferral for this slice**: the first anchor receipt carries no target
  reference; adding one after Q1 resolves is an additive v2 in the established
  receipt-versioning pattern (see `ActionItemCompletionReceipt` v1→v2).
- `purpose` / `process_kind` — no closed taxonomy exists anywhere in the landed
  docs; a free-text field would invite NYCN-specific vocabulary into generic
  core. Deferred until a closed, charter-derived taxonomy is specified.
- `metadata_hash` — no landed Layer 2 receipt carries one; not introduced here.

## 5. Authority discipline

Opening a process session **records institutional process context; it grants
nothing**. `opened_by` is evidence of which authenticated actor recorded the
opening — it is not proof the actor was *entitled* to open it. Whether an
opening was *institutionally* legitimate is a charter/policy question evaluated
by future charter/CCL gates, exactly as gate-result legitimacy is today.
**Route-level caller gating is a separate, mandatory matter:** the open route
MUST mirror the gate-result route's existing authorization posture
(`governance:write` scope plus domain membership, with a 403 non-member
rejection test) — because duplicate opens are sticky per
`(domain_id, session_id)`, an ungated route would let any authenticated
outsider preempt a session id in someone else's domain and force the legitimate
opener into a conflict. Route authorization gates who may *record* the fact; the
recorded receipt still grants no authority. `domain_id` must
be bound into the canonical hash (the landed anti-aliasing rule: identical
fields under different domains must hash differently). No kernel import widens:
the receipt lives in `icn-governance`, is emitted in `apps/governance`, and
crosses the gateway only through the opaque cascade.

## 6. Storage / persistence discipline

- Mirror the gate-result path: a `GovernanceReceiptBackend::put_process_session_opened`
  trait method with a fail-closed default (same `opaque_storage_not_implemented`
  sentinel discipline), routed into the sled-backed opaque cascade keyed by
  `(domain_id, session_id)`.
- **Backend errors fail closed** — the manager surfaces the error; no receipt is
  returned that was not durably accepted.
- **No silent session creation.** `record_process_gate_result` continues to
  accept opaque `session_id`s and does **not** implicitly open sessions;
  coupling gate results to opened sessions is a separate, explicit future
  decision (§7).
- **Duplicate-open semantics (recommendation):** one opening per
  `(domain_id, session_id)`. Idempotency is pinned to the **stable identity
  fields, not the timestamp**: a second open for an already-opened
  `(domain_id, session_id)` by the **same `opened_by`** is an **idempotent
  success that returns the original receipt** (original `opened_at` and
  `record_hash` preserved — the recorded fact is the first opening; the retry
  is never restamped). A second open by a **different `opened_by`** is
  **rejected** (fail-closed conflict; no overwrite, no second receipt). Pinning
  idempotency to byte-identical fields would be illusory: a manager that stamps
  `opened_at` at record time turns any post-timeout client retry in a later
  second into a spurious conflict. This keeps the anchor append-only in the
  only sense that matters — at most one opening fact per session — while making
  retries actually safe. Re-opening, closing, and lifecycle states are out of
  scope.

## 7. Query / read behavior

- Retrieval: by exact `(domain_id, session_id)` → at most one
  `ProcessSessionOpenedReceipt`, through a backend `get_process_session_opened`
  mirror of the existing gate-result reads.
- **Domain-scoped gate-result reads (required by this contract):** the current
  opaque adapter indexes gate results by `session_id` alone
  (`receipt_backend.rs` keys `key1 = receipt.session_id`; listing is by
  `session_id` only), so two domains reusing the same `session_id` would mix
  evidence in any session-anchored export. The implementation PR MUST add a
  **domain-scoped gate-result read** (keyed by `(domain_id, session_id)`,
  filtering on the receipt's hash-bound `domain_id`) and session-anchored
  consumers (future evidence export) MUST use it. The legacy
  `(session_id, gate_kind)` methods are retained unchanged for compatibility;
  globally-unique session ids are NOT assumed.
- **Backwards compatibility is mandatory:** existing gate-result recording and
  `(session_id, gate_kind)` queries remain byte-identical in behavior. Gate
  results remain recordable for sessions with no open-receipt in this slice.
- A future opt-in follow-up (not this slice, not decided here) may let charters
  require an opened session before gate results are accepted; that is a policy
  gate, not a storage default.

## 8. Test matrix for the future implementation PR

1. Canonical hash stable — fixed inputs produce a pinned `record_hash`
   (golden-value test, as the gate-result tests do).
2. Domain separation — identical fields under a different `domain_id` (and
   under the gate-result tag) hash differently.
3. Empty `session_id` rejected; 4. empty `domain_id` rejected (manager-level,
   mirroring the landed validation).
5. Duplicate open: a retry by the same `opened_by` at a **later timestamp**
   returns the original receipt unchanged (idempotent — no restamp, no second
   record); a second open by a different `opened_by` is rejected fail-closed
   with the original untouched.
6. Backend/storage failure fails closed — no receipt returned, no partial
   write.
7. No kernel import widening — gateway sees the receipt only through the opaque
   cascade (compile-level assurance plus the existing firewall CI gates).
8. No NYCN-specific vocabulary in the generic type or its errors.
9. Existing `record_process_gate_result` behavior unchanged (its runtime-proof
   suite passes unmodified).
10. HTTP route proof mirroring `process_gate_result_http_route.rs` (e.g.
    `POST /domains/{domain_id}/process-sessions/{session_id}/open`), with
    authenticated-caller `opened_by` extraction **and the gate-result route's
    authorization posture: `governance:write` + domain membership, including a
    403 non-member rejection test** (§5).
11. Domain-scoped gate-result read: gate results recorded under the same
    `session_id` in two different domains do **not** mix in the new
    `(domain_id, session_id)`-scoped read (§7); the legacy
    `(session_id, gate_kind)` reads are byte-identical in behavior.
12. Docs/claim lint clean — no readiness overclaim, vocabulary gates pass.

## 9. Explicit non-goals

No full process runtime; no deliberation, decision, mutation-plan, or
evidence-packet objects or receipts; no mutation authorization; no evidence
export; no accessibility-gate completion; no privacy/redaction run; no
charter/CCL evaluator or sequencing; no `ProcessSession` lifecycle states; no
#1748 closure; no #2141 closure; no #2081/#2080/#2274 work; no
production/pilot/member/organizer/live-federation/Phase-2 claims.

## 10. Relationship to #1748's open gates

This slice does not close any of #1748's remaining open gates (gate (a) was
already satisfied before this document — see below). It *enables* two of them: the
privacy/visibility evidence-export run and the real accessibility-gate run both
need a session anchor to scope their evidence. Note for the #1748 record: gate
(a) ("at least one receipt class emitted at runtime") was already satisfied by
#2144/PR #1755 — the issue's checklist can be truth-synced independently of this
design. §4's `target_ref` deferral is offered as the written Q1 deferral #1748
asks for, scoped to this slice.

## 11. Recommendation

**Option A — `ProcessSessionOpenedReceipt` only — is recommended.**

- Option B (receipt **plus** a minimal stored `ProcessSession` object) imports
  lifecycle-state management ("opened, deliberating, decided…") ahead of any
  consumer that reads it. The framing brief is explicit that a session is a
  named context, not a workflow engine; until a deliberation or decision slice
  actually needs to *read* session state, a stored object is speculative
  surface. Under Option A, **the persisted receipt itself is the session
  anchor** — duplicate-open semantics (§6) already give "does this session
  exist?" as a receipt-store lookup, with no second source of truth to keep
  consistent.
- Option C (update #1748's status/checkboxes first, since #2144 satisfied the
  literal first-receipt gate) is worth doing but is not an implementation
  direction — it is folded in here as §10's note and can ride any next PR that
  touches #1748.
- Option A keeps the implementation to the proven "one receipt class ≈ one
  focused change in `apps/governance` + one proof type in `icn-governance`"
  footprint (#1748's own sizing), with the full test matrix of §8.

## 12. Validation (this document's PR)

- Frontmatter + `docs/registry.toml` entry + `docs/INDEX.md` link per
  [docs-control-map — Adding a new doc](../reference/project-index/docs-control-map.md).
- `python3 docs/scripts/doc_control_check.py` and `scripts/check-state-lag.py`
  run locally before commit; vocabulary scan clean.
