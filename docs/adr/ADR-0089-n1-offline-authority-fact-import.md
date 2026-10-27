---
id: "0089"
title: "N1 offline authority-fact import and node-store verification"
status: "proposed"
date: "2026-10-24"
revised: "2026-10-27"
deciders: []
tags: ["identity", "n1", "n4", "authority-log", "producer", "import", "retention", "icnctl"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "not implemented"
references:
  - "ADR-0087 (N1 fact persistence: per-fact durability acknowledgment, fail-closed rehydration)"
  - "ADR-0088 (runtime store initialization, startup receipt, §6 states, §9 limits, §11 access, §12 recovery)"
  - "ADR-0018 (ADR lifecycle)"
  - "docs/architecture/IDENTITY_SEMANTICS.md §4 (signed is not authorized), §5 (context scope and correlation)"
  - "docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md .. N4E_CURRENT_ADMISSION.md (normative; #2807, #2808 merged)"
  - "icn#2598 (N3 replication), icn#2599 (N4), icn#2602 (GEN), icn#2694 (Home identity slices)"
---

# ADR-0089: N1 offline authority-fact import and node-store verification

## Status

**PROPOSED.** Nothing here is implemented, and nothing here is accepted. It stays proposed until
the maintainer explicitly approves its contract (ADR-0018). If accepted, it would also amend two
statements in ADR-0088 (see "Effect on ADR-0088").

**Revision of 2026-10-27.** Redrafted against `main` at `f29d04a2`, where N1 persistence (#2800),
the daemon authority registry (#2843), N4-A (#2807) and N4-B..E (#2808) are merged and the five N4
contracts are normative. The maintainer approved the following **directions for proposal
development** (not acceptance): a separate facts-only history-segment carrier, with `EvidenceBundle`
kept for act verification (§3); crash-safe, resumable, fail-closed import (§6); fixture Subjects only,
with explicit context and retention scope (§8); the §8 limits as provisional defaults; preserving
conflicting evidence and halting the Subject (§9); import reports kept distinct from startup
receipts (§11); ownership and permission validation before opening the store (§5); bounded
owner-only diagnostic reports (§11). No real participant data may be imported, and no operational
authorization consumer is approved.

It was drafted and revised by an agent. Independent agent review is recorded with the revision; no
human review is claimed.

## Context

`main` can persist N1 facts durably (ADR-0087), rehydrate them fail closed at every daemon start
(ADR-0088), and answer `derive` over the result. It has no **producer**: `AuthorityRegistry::ingest`
has no production caller, and every fact on any node was written by a test. It has no **consumer**
either: nothing makes an authorization decision from N1 state.

N4-A answers "did device D hold capability C from Subject S at position E" over an `AuthorityStore`.
N4-B's `EvidenceBundle` carries the evidence for **one device act**: the facts `derive_prefix`
reads up to the act's evaluation position E, plus the act. N4-E answers the same question at a
relying party's current position.

A bundle is the wrong carrier for history. It cannot exist without an act, and it cannot carry a
fact after E. A node fed only bundles would learn a Subject's history only up to positions where
some act happened, and would never learn a later revocation unless another act's bundle happened to
include it. N4-E current admission over that node's facts would then admit a device its Subject has
revoked. This revision therefore introduces a facts-only carrier for history (§3) and keeps the
bundle for what it is: evidence for one act (§10).

The smallest producer that adds no network surface is an offline operator tool that imports a
history segment into a stopped node's store. This ADR defines what that tool may do and, as
importantly, what it may not claim.

## Decision (proposed)

### 1. Seven properties that must stay separate

| Property | Meaning | Established by | Does **not** imply |
|---|---|---|---|
| Cryptographic validity | key K signed these canonical bytes | `verify_strict` inside N1 admission | that K had any authority |
| N1 admission | the body is canonical, in bounds, and verifies under its inline signer | `admissible` (ADR-0087 §5) | authority; that the history is complete |
| Durable persistence | the record survived the store's durability barrier | `put_fact` Ok (ADR-0087 §4) | power-loss durability; that the node may keep it |
| Retention authorization | this node is permitted to keep this Subject's facts, in this context | §8: an operator record made before import | anything about the Subject's authority; consent |
| Completeness and freshness | the node holds every fact that exists, up to the Subject's real tip | **nothing** (§4) | — |
| Derived authority | what `derive` concludes from the retained facts | `derive` / `derive_prefix` over the rehydrated store | that the node holds every fact that exists |
| N4 relying-party enforcement | a device act is accepted at a relying-party-chosen position | `verify_device_act` / current admission (N4-A, N4-E) | that any effect is gated on it today |

The import tool establishes admission, durable persistence and (by checking §8) that a retention
record exists, for each fact it writes. It never evaluates, and never reports, completeness,
derived authority or enforcement as a property of the import.

### 2. Scope of the producer

- One command, `icnctl authority import <segment-file>`, run by the node operator against a
  **stopped** node. It takes the data-directory lock; if `icnd` holds it, the import is refused.
- Input is exactly one **history segment** v1 file (§3). `EvidenceBundle` is not an import input
  in this version.
- No network listener, IPC socket, gateway route, gossip or N3 ingest, and no credentials. A running
  daemon sees imported facts only at its next start, through the existing rehydration.
- Fixture Subjects only (§8).

### 3. The history segment (candidate format, v1)

A history segment is a canonical, facts-only excerpt of **one Subject's** N1 history over a
contiguous position range. It carries no device act and asserts nothing about authority.

**Fields.** `subject: SubjectId`, `from: u64`, `to: u64`, and `facts`: a map from `EventId` to
`{ body: AuthorityBody, witnesses: set of WitnessSignature }` (the same `BundledFact` shape as N4-B).

**Canonical encoding** (the N1/N4 `Writer` conventions: big-endian integers, `lp` = `u32` length
prefix, `b32`/`b64` fixed arrays):

```
lp("icn.n1.history-segment")      domain (proposed literal; registered in the
                                  domain-separation audit before any implementation)
u16  version = 1
b32  subject
u64  from
u64  to
u32  fact_count
repeat fact_count, ascending by event_id:
  b32  event_id
  lp   body.canonical_bytes()
  u32  witness_count            (>= 1)
  repeat witness_count, ascending:
    b64  witness signature
```

`segment_id = SHA-256(canonical bytes)`. Strict decode: the encoder's output is the only accepted
form, and trailing bytes are refused.

**Validity rules** (all checked by decode or by whole-segment admission, before anything is
written):

1. `0 <= from <= to <= MAX_POSITION`.
2. Every fact's `body.subject()` equals `subject`, and its `event_id` equals the hash of its body.
3. Every fact's position lies in `from..=to`.
4. **Continuity:** every position in `from..=to` holds at least one fact.
5. **Linkage:** every fact at position `p > from` names in `prev_digest` a fact this segment holds
   at `p - 1`. If `from == 0` the segment holds the inception, whose event id is `subject`.
6. Facts ascend strictly by `event_id`; witnesses ascend strictly within a fact; every fact has at
   least one witness.
7. Every fact passes N1 admission, and the segment as a whole admits into an empty in-memory store
   (`admit`), all or nothing.

**At import** (against the node's retained facts), one more rule: if `from > 0`, every fact at
`from` must name in `prev_digest` a fact the node **already retains** at `from - 1`. A segment that
would leave an unanchored fragment is refused. Retaining facts that `derive` cannot reach would
cost retention without any use.

**Competing candidates.** Several admissible facts at one position are allowed in a segment (rule 4
says "at least one"). They are evidence of a fork and are kept (§9).

**Refusals** (each a distinct error): framing; version; range out of bounds; fact for another
Subject; event id mismatch; fact outside range; gap in continuity; broken linkage; ordering
(facts or witnesses); missing witness; inadmissible fact; unanchored at import; any bound in §8.

### 4. What a segment cannot establish: completeness and freshness

Holding a valid segment proves only that these facts are admissible and form a contiguous, linked
excerpt over `from..=to`. It does **not** prove:

- that `to` is the Subject's tip. Later facts, including revocations, may exist.
- that no competing candidate exists at any position the segment covers. An omitted fork candidate
  is undetectable at or below `to` (the same limit as N4-B §3.3).
- freshness. There is no clock or authority that dates a segment, and a segment can be replayed
  from any earlier time.

A node therefore holds a **retained tip** per Subject, never a "current" one. Every answer computed
from a node's store is "as of the facts this node retains" (§10, §12). Nothing here changes how a
relying party must treat incompleteness; any future consumer must address it in its own ADR.

### 5. Opening the store offline

`AuthorityRegistry::open` is the daemon's start: it may initialize a fresh history, and it always
writes the startup receipt. An offline tool must do neither. A separate entry point is proposed,
`AuthorityRegistry::open_offline(data_dir, lock, access)` with `access` one of `Inspect`, `Import`
or `Resume`:

- It requires the storage lock and refuses an incomplete restore, like `open`.
- The caller has run the N2-A gate before it, matching ADR-0088 §7's order.
- **Ownership and permissions** are checked before sled opens anything: the effective uid must own
  the data directory and the authority store directory, and neither may grant group or other
  permissions. Otherwise it refuses. sled has no read-only open and creates files as it runs; an
  offline tool run by another account, or by root, would leave files the daemon's account may not
  be able to open, wedging the node.
- It opens only an **initialized** store whose record and instance row agree. Every other ADR-0088
  §6 state (no record, missing store, replaced store, interrupted initialization) is refused. A new
  authority history is created by `icnd` only.
- It writes no startup receipt. `Import` and `Resume` remove the previous receipt with the existing
  `AuthorityRegistry::discard_receipt` before writing any fact, because once facts change that
  receipt misdescribes the store. `Inspect` leaves it alone.
- `Inspect` and `Import` refuse while an import is incomplete (§6). `Resume` requires it.

### 6. Atomicity, interruption and resumption

**Per-fact durability stays as ADR-0087 defines it.** A segment is not made atomic in the store, and
nothing may describe an import as atomic. Whole-segment admission in memory proves the segment is
well formed; it does not make the writes atomic.

**Why a partial import must not be used silently.** A partial import is not wrong authority in
itself: a gap stops the `derive` frontier. What it breaks is the operator's knowledge of what the
node holds. If an import stops after a device's `authorize` is durable but before the later `revoke`
is, the retained tip is behind the facts the node was handed, and N4-E at that tip still admits the
device. Durable partial state is therefore permitted on disk but must be **fail-closed and
resumable**:

1. Decode the whole segment strictly, apply §3 and §8, and admit it in memory. Anything refused
   writes nothing.
2. Write the segment bytes to the **staged file** (a regular, owner-only file in the data directory)
   and `fsync` it. Only then write the **import-incomplete marker** atomically (synced temporary
   file, rename), naming the segment id, Subject and fact count, and `fsync` the directory. A
   durable marker therefore always has a durable staged copy behind it.
3. Remove the startup receipt (§5).
4. Write each fact through `ingest` (admission, durability barrier, publication), in ascending
   position order, then ascending `event_id`.
5. After the last barrier, remove the marker and `fsync` the directory. Only then remove the staged
   copy. A stop between the two leaves an orphan staged copy and no marker, which is harmless and is
   removed by the next import.

**Existence is the safety property**, as for the restore marker: any entry at the marker's name
refuses, and so does any error other than "not found" while looking. It is removed only by a
completed import or resume. Nothing removes it on drop.

Recovery is `icnctl authority import --resume`. It re-reads the staged copy (or the original file if
the operator supplies it), checks its id against the marker, re-applies §3 and §8 (either may have
changed), and writes the facts again. Duplicate writes are semantically idempotent (ADR-0087 §3), so
resuming after any interruption point is safe. If §8 now refuses the Subject, resume refuses and the
node stays refused: that conflict needs the operator.

There is no "abandon" in this version. Facts already written cannot be removed (the store has no
erasure path), so abandoning would leave exactly the stale partial history the marker exists to
expose. Whether a governed abandon or erasure exists is a governance decision (§14).

**Never wedged (an acceptance obligation, not a hope).** An implementation is acceptable only with a
fault-injection suite that stops the import after staging, after the marker, after `k` of `n` facts
(`k = 0, 1, n-1, n`), between the last barrier and the marker's removal, and between the marker's
removal and the staged copy's removal. In every case the daemon must refuse while the marker exists,
`--resume` must complete, the final store must equal a clean import's, and no case may leave the node
permanently refused.

### 7. Reconciliation with daemon startup: what changes

`icnd` starts in this order (main.rs): take the data-directory lock, `discard_receipt`, run the N2-A
gate, `AuthorityRegistry::open`. The marker fits that order without moving any step:

| Component | Change |
|---|---|
| `icn-core` `import_marker` (new) | Create, read and complete the marker and staged file. Modelled on `restore_marker`: existence refuses; durable create with `fsync` of file and directory; no removal on drop. |
| `icn-core` `AuthorityRegistry::open` | After `refuse_incomplete_restore`, refuse `ImportIncomplete` while the marker exists. The daemon's start then fails closed, naming `icnctl authority import --resume`. |
| `icn-core` `AuthorityRegistry::open_offline` (new) | §5. |
| `icn-core` `RegistryError` | New `ImportIncomplete { marker, subject, segment_id }`. |
| `icn-core` `DataDirLock` | **Unchanged.** Unlike the restore marker, the import marker is not checked at lock acquisition: resume must take the lock, and the condition only makes the authority store untrustworthy, not the rest of the data directory. |
| `icn-core` `discard_receipt` | Unchanged. An import has already removed the receipt before its first write. |
| `icn-store` N2-A gate | Unchanged. The marker and staged file are top-level regular files, never directories, so they are not discovered as sled roots. A test must show this. |
| `icnd` main.rs | No ordering change. The startup error surfaces the new refusal and its recovery command. |
| `icnctl` | New `authority import [--resume]` and `device-authority verify --node-store` (§10). Backup and restore carry the marker and staged file with the data directory, so a node restored from a mid-import backup refuses until resumed (intended). Reporting the marker in `verify-backup` is a follow-up. |
| ADR-0088 | §6 gains the import-incomplete state; §11's exposure sentence changes (see "Effect on ADR-0088"). |

### 8. Retention: fixture Subjects only, with explicit context and scope

Admission accepts any self-incepted Subject, and the store only grows. Without a retention rule, any
segment the operator is handed becomes a permanent record on the node, and a node collecting
Subjects across contexts works against IDENTITY_SEMANTICS §5.1 (one `SubjectId` per institutional
context) and §5.3 (the identifier is public to its context only).

- **Retention file.** An owner-only (0600) file in the data directory lists each Subject the node
  may keep. Each entry records the `subject`, the `context` it is retained for, the `basis` (a
  reference, never personal data), who added it, and when. A segment whose Subject has no entry is
  refused before anything is written.
- **Fixture only (v1).** An entry's `basis` must begin `fixture:`. Any other basis is refused. No
  real participant may be retained until §13 is met and a later revision of this ADR permits it.
- **Context is recorded per Subject, never assumed per node.** A node may host several autonomous
  institutional domains, and a person may hold context-specific Subjects in overlapping communities,
  cooperatives, federations and other institutions. Until GEN (#2602) binds a Subject to a context
  structurally, `context` is the operator's recorded claim, not a verified fact. It is never used to
  join or compare Subjects. A first implementation may support one context per store as an
  implementation limit; that limit must not become an ICN substrate invariant.
- **The retention file is the operator's record of a retention decision.** It is not the person's
  consent or the institution's authorization, and nothing may treat an entry as either.
- **Provisional bounds**, checked before reading or allocating: segment file at most 1 MiB; at most
  4,096 facts per import; at most 65,536 retained facts per Subject. Configuration may lower them,
  never raise them. Values to be confirmed by testing.
- **Withdrawal.** Removing an entry stops further imports for that Subject. It does **not** remove
  retained facts: the fact store has no deletion API, sled can keep deleted bytes, an account that
  held an open descriptor keeps it (ADR-0088 §11), and backups keep their own copies.
- **No new index.** Reports and the retention file name `SubjectId`s only. Nothing maps a legacy
  `Did` or any other identifier to a Subject.

### 9. Forks are evidence

An import never filters facts by what `derive` would conclude. If the retained facts, or the union
of retained and imported facts, contain admissible competing candidates at one position, all are
kept, and `derive` reports the Subject as halted. The import report states the halt. Dropping a
candidate would hide the duplicity that halting exists to expose. A segment that leads to a halted
Subject is not an import failure.

### 10. `icnctl device-authority verify --node-store`

An offline check of a device act, given as an N4-B `EvidenceBundle`, against the node's own retained
facts. It uses `open_offline(Inspect)` under the same lock, refuses while an import is incomplete,
writes no facts, and neither writes nor removes the startup receipt. It reports the historical
answer (N4-A, at the act's position) and the current answer (N4-E, at the node's retained tip)
separately, names the retained tip, and labels both "as of the facts this node retains". The bundle's
own facts are not imported. sled may still write its own files when opened (§5).

### 11. Import reports

An import writes an owner-only **import report**, a different file name and format from the
startup receipt and never called one: segment id, Subject, `from..=to`, facts and witnesses
written, facts already present, the retained tip, and the `derive` view afterwards, labelled as a
diagnostic exactly as ADR-0088 §10 labels the receipt. The last 8 reports are kept; older ones are
removed by the next import. Reports and logs carry digests, counts and `SubjectId`s only, never fact
bodies, keys or payloads.

### 12. How the limits are communicated

Every import report and every `--node-store` answer states, in fixed text:

- Imported facts are admitted, not authorized. Authority is what `derive` concludes, at the point of
  use.
- A segment cannot prove it is complete or current (§4). The node holds only what it was given.
  There is no replication (N3).
- Rolling the data directory back, or losing it entirely, cannot be detected (ADR-0088 §9).
- Persistence survives a process crash. Surviving power loss is best effort (ADR-0087 §4, §9).
- If the store was ever readable by another account, anything imported later is exposed to an open
  descriptor that account may still hold (ADR-0088 §11).

### 13. Prerequisites that need people, not code

Before any fact about a real participant is retained:

1. Maintainer acceptance of this ADR, and of a revision lifting §8's fixture-only rule.
2. The institution's decision on whose facts a node may keep, in which context, on what basis, with
   what notice that retained facts cannot currently be erased, and who may run the import.
3. A withdrawal and erasure procedure, or an explicit institutional decision to proceed without one.
4. A custody decision for the real Subject and device keys that sign the facts. The producer of a
   segment is the Subject's own client, never the node; hosting a store confers no authority.
5. A security review of the import implementation before it runs against a real store.

### 14. Decisions still open

- Whether a governed abandon or erasure procedure exists, and who may invoke it (governance).
- Whether the retention file stays an operator-edited file or becomes a governed record
  (maintainer, then governance).
- Who may run an import for a real institution, and how consent is recorded and withdrawn
  (governance).
- Final values of the §8 bounds after testing (maintainer).
- How a Subject's own client produces segments: out of scope here, and needs its own design.

## Irreversible commitments

1. The segment's domain literal and layout become permanent once published; canonical vectors and an
   independent reference implementation must exist before any implementation merges.
2. A new fail-closed state in every registry open; operators handle it for the life of the node.
   §6's fault-injection obligation is what makes it acceptable.
3. Retained facts cannot be erased. The fixture-only rule exists because of this.
4. The retention file's semantics. They must never be read as consent or authority, by this or any
   later code.

## Effect on ADR-0088, if accepted

- §11's "Today no runtime producer writes facts, so nothing is written after the tightening" stops
  being true for imported facts. The exposure statement would be updated to cover imports.
- §6 gains one fail-closed state, import incomplete, which `open` refuses.
- §10 is unchanged in substance: the startup receipt stays a daemon diagnostic. Offline tools that
  write facts remove it and never write one.

ADR-0087 is unchanged.

## Security and privacy controls (required of an implementation)

- Data-directory lock held for the whole import; refused while `icnd` runs.
- N2-A gate before open; incomplete-restore and incomplete-import refusals before any write.
- Effective uid owns the data directory and store directory; no group or other permission bits;
  checked before sled opens anything.
- The input must be a regular file, not a link. Its size is checked against the bound before
  reading, and decoding is strict, with no allocation beyond the input.
- Whole-segment admission in memory before the marker. Per-fact admission, then barrier, then
  publication.
- Marker, staged copy, import report and retention file are each created owner-only (0600) as
  top-level regular files in the data directory.
- Logs carry digests, counts and the `SubjectId` only. Never fact bodies, keys or payloads.
- No network, IPC or credential dependency. No real identity material in tests.
- Mutation tests that remove the marker write, skip whole-segment admission, publish before the
  barrier, filter by verdict, write a startup receipt from the offline path, let `open` ignore the
  marker, accept a non-`fixture:` basis, skip the ownership check, check the size after reading, or
  resume without comparing the segment id, must each be caught.

## Consequences

- A node can hold N1 history for fixture Subjects without any new network surface, and the daemon's
  existing fail-closed rehydration covers it.
- Revocations reach a node through the same carrier as everything else, so a node's retained tip can
  include them; it still cannot prove it holds the latest.
- An operator handles a new refusal (import incomplete), with a resume path instead of a manual
  procedure.
- Retention becomes an explicit, recorded decision instead of a side effect of admission.
- Nothing here creates a consumer. N4 enforcement over a node's store stays a separate decision.

## Alternatives considered

| Alternative | Why not now |
|---|---|
| Import `EvidenceBundle`s | Act-centred and prefix-bounded: cannot carry a revocation after the act's position, cannot exist without an act. Kept for act verification only (§10). |
| Atomic batch in the store | Changes ADR-0087's accepted contract. Possible later by amendment. |
| Check the marker at `DataDirLock` acquisition | Would block resume and the rest of the data directory for a condition local to the authority store. |
| Import through a running daemon (IPC, socket, gateway) | A new authority write surface. Needs its own design and security review. |
| Reuse `AuthorityRegistry::open` | Would initialize histories outside `icnd` and write a false startup receipt. |
| Filter out facts that make a Subject halted | Hides duplicity. Halting is the intended outcome. |
| No retention rule, admit anything | Unbounded permanent storage, and a cross-context correlation surface. |
| Accept unanchored segments (`from > 0` with no retained predecessor) | Retention cost with no use: `derive` cannot reach the facts. |
