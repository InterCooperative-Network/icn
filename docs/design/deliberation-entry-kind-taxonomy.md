# DeliberationEntry kind taxonomy — Q3 decision

**Status**: draft — design/audit decision (taxonomy decision, not runtime implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-02
**Source basis**: read against `main` @ `e02c8519`. Code anchors were verified at that commit — re-verify before relying on exact numbers.
**Related**: issues #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · PR #2277 (`docs/design/deliberation-entry-recorded-receipt.md`, the blocked contract this decision unblocks) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) · `ops/ideas/framing/institutional-process-substrate.md` (idea-0019 framing brief, open question 3) · `ops/ideas/framing/democratic-authority-primitives.md` (idea-0020, consumes entry kinds)

> Decides framing-brief open question 3 for the purpose of
> `DeliberationEntryRecordedReceipt` v1: **closed, ADR-controlled enum hashed
> by ordinal (Option A)**, with a scrutinized ten-kind initial list and an
> explicit discriminant/evolution rule. This is a design decision document —
> no runtime code. Receipts record institutional facts. They grant zero
> authority.

## 1. Problem

The merged #2277 contract (`docs/design/deliberation-entry-recorded-receipt.md`)
deliberately blocked `DeliberationEntryRecordedReceipt` implementation on Q3:

> Should `DeliberationEntry` kinds be a closed taxonomy locked by an ADR, or
> extensible via charter? (framing brief, open question 3)

The blocker is concrete, not procedural: the two branches produce **different
canonical hash layouts** — a closed enum hashes as a single ordinal byte (the
landed `ProcessGateKind` convention), a charter-extensible kind hashes as a
length-prefixed string. `icn:gov:deliberation_entry_recorded:v1` cannot be
pinned until one branch is chosen. This document chooses.

## 2. Options

**Option A — closed, ADR-controlled enum, hashed by ordinal.**
Kinds are a fixed enum in `icn-governance` proof code, mapped to explicit
`u8` discriminants by a hand-written match (the landed pattern:
`gate_kind_ordinal`, `icn/crates/icn-governance/src/proof.rs` ~L2254, single
byte into the hasher). Evolution requires an ADR/addendum (append-only
discriminant addition) or a new tag version. Stronger meaning firewall; less
charter flexibility at the receipt layer.

**Option B — charter-extensible namespaced string, length-prefix hashed.**
Kind is a caller-supplied string (e.g. `coop.example/block`), hashed like the
other variable-length fields. Charters or domains can mint kinds without a
core change. More flexible; requires namespace/validation discipline that
does not exist today, makes the accessibility/privacy gate handles
open-ended, and lets institution-specific vocabulary flow into generic core
receipts — the drift path toward loose comments/chat semantics.

## 3. Decision: Option A — closed ADR-controlled enum for v1

Four reasons, each grounded in landed material:

1. **The framing brief already leans closed, on purpose.** "The
   closed-taxonomy approach is deliberate: it keeps deliberation a structured
   institutional record rather than a free-form text dump, and it gives the
   accessibility gate, privacy review, and conflict-routing paths concrete
   handles to act on." Gates need *stable typed handles*; an open string set
   gives them none.
2. **The landed precedent chose closed for the same shape of problem.**
   `ProcessGateKind` is a closed six-kind taxonomy with the doc comment "The
   taxonomy is closed: the runtime never emits a gate kind not listed here.
   Adding a kind requires a separate change." The deliberation-entry kind is
   the same design pressure one rung later.
3. **Charter flexibility lives at a different layer — and stays there.** The
   framing brief's vocabulary firewall is explicit: "a package may name
   'block' instead of `DeliberationEntry: blocker`. The substrate stays
   generic; the institution maps." Charters decide *which kinds are required
   or permitted* per session kind; institution packages map local names onto
   generic kinds at the app layer. Option A preserves exactly that split.
   Option B would invert it, moving institutional vocabulary into the
   generic receipt.
4. **Extension paths remain open without loosening v1.** A genuinely new
   generic kind arrives by ADR/addendum with an append-only discriminant
   (§5); institution-specific richness arrives as app-layer metadata or,
   if ever justified, a `:v2` tag. Nothing about Option A forecloses
   Option B's use cases — it relocates them to the right layer.

## 4. Initial v1 kind list — scrutinized, not rubber-stamped

Starting set: the framing brief's eleven candidates. Scrutiny outcome: **ten
kinds enter v1; `resolution` is deferred in writing.**

| Discriminant | Kind | Rationale / evidence of need |
|---|---|---|
| 0 | `question` | Core deliberative input; used in both fixture walks (idea-0019 Step 2 entry 1; idea-0020). |
| 1 | `concern` | Core deliberative input (framing candidate list). |
| 2 | `objection` | Core deliberative input; idea-0020 dogfood records an `objection` entry with consensus-held semantics preserved as a typed fact. |
| 3 | `amendment` | Used in both fixture walks (idea-0019 entry 3; idea-0020 `ConflictDisclosure` attaches to an `amendment` entry). |
| 4 | `blocker` | Framing candidate; the vocabulary-firewall example kind ("a package may name 'block' instead of `DeliberationEntry: blocker`"). |
| 5 | `facilitator_summary` | idea-0020 depends on it by name ("the `facilitator_summary` `DeliberationEntry` kind"); facilitator input is non-decisional by construction. |
| 6 | `privacy_review` | The privacy gate's deliberation-side handle; gates are "explicit `ProcessGateResult` producers, not afterthoughts". |
| 7 | `accessibility_review` | The accessibility gate's deliberation-side handle; idea-0019 Step 2 entry 2 is exactly this kind. |
| 8 | `conflict_signal` | Conflict-routing handle. The idea-0020 `ConflictDisclosure` object is a DAP-layer construct that *attaches to* entries; `conflict_signal` is the generic core kind it routes through. |
| 9 | `record_only` | The semantically-neutral institutional note; the minimal kind that keeps "an entry was recorded" expressible without overloading another kind. |

**Deferred from v1: `resolution`.** It is the one candidate whose plain
reading straddles the input/outcome boundary: "a proposed resolution offered
into deliberation" is input, but "the resolution of the matter" is an
outcome — and outcomes are Q4 / `DecisionRecord` territory that this receipt
class must not absorb (§7). Neither fixture walk uses it; nothing landed
depends on it. Deferring costs one ADR/addendum with discriminant `10` if a
sharp input-only definition is later agreed; including it now would put the
most Q4-ambiguous kind into the hash layout permanently. This deferral is
the Q4 firewall applied to the taxonomy itself.

## 5. Stable discriminant policy

Mirrors and tightens the landed `gate_kind_ordinal` pattern:

- Discriminants are **explicit** in a hand-written match (`u8`), exactly as
  pinned in §4. No `as u8` casts of compiler-assigned ordinals, no derive
  magic.
- **Never reorder. Never reuse.** A removed kind (should that ever happen)
  retires its discriminant permanently; the match arm goes away, the number
  never comes back.
- **Adding a kind** requires an ADR or a reviewed addendum to this document,
  and takes the next unused discriminant (append-only). This is
  hash-compatible: existing receipts' hashes never change, because each
  receipt hashes only its own discriminant byte.
- **Semantic changes** to an existing kind (meaning, boundary, gate
  consumption) are not additions — they require a new receipt tag version,
  not a discriminant edit.
- **Unknown kinds fail closed**, at both boundaries: the recording API
  rejects a kind outside the enum (unrepresentable by construction in Rust),
  and deserialization of a persisted payload with an unknown kind string is
  an error, never a silent default.
- A **golden test vector** pins the full hash layout including the
  discriminant byte (§8).

## 6. Relationship to Q1 (`ProcessTargetRef`)

Unchanged and untouched. This decision covers the entry-kind axis only.
Entries bind to the `(domain_id, session_id)` anchor per the #2277 contract;
`target_ref` remains deferred exactly as #2275 and #2277 deferred it. Nothing
here decides `ProcessTargetRef` polymorphism.

## 7. Relationship to Q4 (`HumanDecisionSet` vs proposal/vote)

- **No approve/vote/outcome kind exists in the taxonomy**, and none may be
  added by discriminant append — an outcome-bearing kind is a semantic
  boundary change requiring its own design decision (and almost certainly a
  different receipt class: `DecisionRecord` / `HumanDecisionSet` territory).
- The `resolution` deferral (§4) is this rule applied at v1.
- The test-only `icn-baseline-lock` namesake
  (`BaselineReceiptBody::DeliberationEntryRecorded { member_pubkey, approve }`,
  domain `icn:baseline:deliberation_entry:v1`) remains disjoint and is not
  prior art, per the #2277 contract audit. Its approve flag is precisely
  what this taxonomy refuses to contain.

## 8. Privacy / accessibility discipline

- `entry_kind` is a **routing and audit handle, not content**. It classifies
  the institutional act; it says nothing about, and reveals little of, the
  body.
- The body remains stored **only as `body_hash`** in the future receipt
  (#2277 contract §4/§6 unchanged by this decision).
- The privacy/redaction evidence-export run and the real accessibility-gate
  run remain open #1748 gates. `privacy_review` and `accessibility_review`
  kinds give those gates deliberation-side handles; they do not complete
  them.

## 9. Instruction to the future implementation PR

When Matt scopes the implementation rung (and not before):

- Implement `DeliberationEntryRecordedReceipt` per the merged #2277 contract
  with `entry_kind` added as decided here. The implementation PR should make
  the minimal contract-sync edit to
  `docs/design/deliberation-entry-recorded-receipt.md` §4 (add the
  `entry_kind` row and remove the blocker language), citing this document.
- Canonical hash layout for v1 (golden vector required): domain tag
  `icn:gov:deliberation_entry_recorded:v1`, then length-prefixed (u64 LE)
  `domain_id`, `session_id`, `entry_id`, `author`, then **one `entry_kind`
  discriminant byte** (§4 table), then `recorded_at` (u64 LE), then
  `body_hash` appended raw as a fixed 32-byte field (no length prefix, per
  the #2277 contract as amended in review).
- Tests the implementation must add beyond the #2277 §8 matrix:
  - discriminant stability: the §4 table's kind→byte mapping asserted
    exhaustively (a match-arm change breaks a test, not just a hash);
  - each distinct kind produces a distinct `record_hash` for otherwise
    identical fields;
  - unknown/out-of-enum kind fails closed at deserialization;
  - no vote/approval/outcome kind exists (exhaustive-match vocabulary
    assertion);
  - no chat/comment/moderation vocabulary in kind names or API surface;
  - golden vector covering the full layout including the discriminant byte.

## 10. Explicit non-goals

No runtime implementation in this PR. No stored `DeliberationThread`. No
decision records, mutation plans, evidence packets, or action-card triggers.
No moderation/chat/comments/social system. No accessibility/privacy gate
completion. No new ADR number claimed (this is a design decision document;
if review prefers ADR form, that is a rename, not a scope change). No #1748
closure; no #2141 closure; no #2081/#2080/#2274 work; no process-runtime,
Phase-2, production, pilot, member/organizer-readiness, live-federation,
service-hosting, or OpenAPI-completeness claims.

## 11. Validation (this document's PR)

Docs-only change set: this file, a `docs/registry.toml` row, a
`docs/INDEX.md` link, and the regenerated `docs/DOCUMENT_REGISTRY.md`.
Validation: `git diff --check`, `python3 docs/scripts/doc_control_check.py
--repo . --registry docs/registry.toml` (no new warnings vs the 66-warning
baseline), `python3 scripts/check-state-lag.py`. No route, OpenAPI, or
generated-inventory files are touched, so none are regenerated.
