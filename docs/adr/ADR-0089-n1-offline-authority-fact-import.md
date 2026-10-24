---
id: "0089"
title: "N1 offline authority-fact import and node-store verification"
status: "proposed"
date: "2026-10-24"
deciders: []
tags: ["identity", "n1", "n4", "authority-log", "producer", "import", "retention", "icnctl"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "not implemented"
references:
  - "ADR-0087 (N1 fact persistence: per-fact durability acknowledgment, fail-closed rehydration)"
  - "ADR-0088 (runtime store initialization, startup receipt, §9 limits, §11 access, §12 recovery)"
  - "ADR-0018 (ADR lifecycle)"
  - "docs/architecture/IDENTITY_SEMANTICS.md §4 (signed is not authorized), §5 (context scope and correlation)"
  - "N4-A device authority (#2807) and N4-B..E portable evidence bundle and current admission (#2808), both unmerged drafts"
  - "icn#2599 (N4), icn#2598 (N3 replication), icn#2602 (GEN), icn#2694 (Home identity slices)"
---

# ADR-0089: N1 offline authority-fact import and node-store verification

## Status

**PROPOSED.** Nothing here is implemented, and nothing here is accepted. It stays proposed until
the maintainer explicitly approves its contract (ADR-0018). If it is accepted, it would also amend
two statements in ADR-0088 (see "Effect on ADR-0088").

It was drafted by an agent from code and documents on `main` at `94cf94d13` and from a local,
unpublished compatibility integration of #2807 and #2808. No human review is claimed.

## Context

`main` can persist N1 facts durably (ADR-0087), rehydrate them fail closed at every daemon start
(ADR-0088), and answer `derive` over the result. It has no **producer**: `AuthorityRegistry::ingest`
has no production caller, and every fact on any node today was written by a test. It has no
**consumer** either: nothing makes an authorization decision from N1 state.

N4-A (#2807) adds the relying-party question "did device D hold capability C from Subject S at
position E", evaluated over an `AuthorityStore`. N4-B (#2808) adds a canonical portable container,
`EvidenceBundle`, scoped to one Subject and one position, with a strict decoder and an all-or-nothing
re-admission. A local integration showed N4-A's verdicts over the store `AuthorityRegistry`
rehydrates from disk equal its verdicts over the same facts in memory.

The smallest producer that does not add a network surface is an offline operator tool that imports
a bundle into a stopped node's store. This ADR defines what that tool may do and, as importantly,
what it may not claim.

## Decision (proposed)

### 1. Six properties that must stay separate

| Property | Meaning | Established by | Does **not** imply |
|---|---|---|---|
| Cryptographic validity | key K signed these canonical bytes | `verify_strict` inside N1 admission | that K had any authority |
| N1 admission | the body is canonical, in bounds, and verifies under its inline signer | `admissible` (ADR-0087 §5) | authority; that the history is complete |
| Durable persistence | the record survived the store's durability barrier | `put_fact` Ok (ADR-0087 §4) | power-loss durability; that the node may keep it |
| Retention authorization | this node is permitted to keep this Subject's facts | §5 below: an operator decision recorded before import | anything about the Subject's authority |
| Derived authority | what `derive` concludes from the retained facts | `derive` / `derive_prefix` over the rehydrated store | that the node holds every fact that exists |
| N4 relying-party enforcement | a device act is accepted at a relying-party-chosen position | `verify_device_act` / current admission (N4-A, N4-E) | that any effect is gated on it today |

The import tool establishes the first four for each fact it writes. It never evaluates, and never
reports, the last two as a property of the import.

### 2. Scope of the producer

- One command, `icnctl authority import <bundle-file>`, run by the node operator against a **stopped**
  node. It takes the data-directory lock; if `icnd` holds it, the import is refused.
- Input is exactly one N4-B `EvidenceBundle` v1 file. No other carrier is introduced.
- No network listener, IPC socket, gateway route, gossip or N3 ingest, and no credentials. A running
  daemon sees imported facts only at its next start, through the existing rehydration.
- Fixture identities only until the prerequisites in §9 are met.

### 3. Opening the registry without a startup receipt

`AuthorityRegistry::open` is the daemon's start. It may initialize a fresh history, and it always
writes the startup receipt. An offline tool must do neither. A separate entry point is proposed:

- It requires the storage lock and asks the incomplete-restore refusal, like `open`.
- It runs the N2-A gate before opening, matching ADR-0088 §7's order.
- It opens only an **initialized** store whose record and instance row agree. Every other ADR-0088
  §6 state (no record, missing store, replaced store, interrupted initialization) is refused. A new
  authority history is created by `icnd` only.
- It writes no startup receipt. When the caller is about to **write** facts (the import), it first
  removes the previous startup receipt, because once facts change that receipt misdescribes the
  store; the daemon writes a fresh one at its next start. A caller that writes nothing (§7) leaves
  the receipt alone.
- The import writes a separate owner-only **import report** (a different file name and format, never
  called a startup receipt): bundle digest, Subject, evaluation position named by the bundle, facts
  and witnesses written, facts already present, and the `derive` view after import, labeled as a
  diagnostic exactly as ADR-0088 §10 labels the receipt.

### 4. Atomicity, interruption and resumption

**Per-fact durability stays as ADR-0087 defines it.** A batch is not made atomic in the store.

**Why a partial import must not be used silently.** A partial import is not wrong authority in
itself. Under N4-A's prefix rule, a gap stops the frontier, so nothing past it is read as
authorized. A partial prefix is the same state as a complete import of an older bundle. What it
breaks is the operator's knowledge of what the node holds. If an import stops after a device's
`authorize` is durable but before the later `revoke` is, the node's retained tip is behind the
facts it was handed. N4-E current admission at that tip then still admits the device, and the
operator believes the revocation was imported. The marker exists so that "handed but not retained"
is never mistaken for "retained". Durable partial state is therefore permitted on disk but must be
**fail-closed and resumable**:

1. Before writing anything, decode the whole bundle strictly and run `EvidenceBundle::admit` in
   memory. A malformed, out-of-scope or inadmissible bundle writes nothing.
2. Write the bundle bytes to an owner-only staging file in the data directory and `fsync` it. Only
   then write the **import-incomplete marker** atomically (synced temporary file, rename), naming
   the bundle digest, Subject and fact count, and `fsync` the directory. A durable marker therefore
   always has a durable staged copy behind it.
3. Write each fact through `ingest` (admission, then the durability barrier, then publication), in
   canonical position order.
4. After the last barrier, remove the marker and `fsync` the directory. Only then remove the staged
   copy. A stop between the two leaves an orphan staged copy and no marker, which is harmless and
   is removed by the next import.

While the marker exists, **every** registry open, including the daemon's, refuses with a new
fail-closed state (import incomplete). Recovery is `icnctl authority import --resume`. It re-reads
the staged copy, or the original bundle file if the operator supplies it, checks its digest
against the marker, re-checks the retention allowlist and the bounds of §5 (either may have changed
since), and writes the facts again. If the allowlist now refuses the Subject, resume refuses too, and
the node stays refused: that conflict needs the operator. Duplicate writes
are semantically idempotent (ADR-0087 §3), so resuming after any interruption point is safe,
including a stop between the last barrier and the marker's removal.

There is no "abandon" in this version. Facts already written cannot be removed (the store has no
erasure path), so abandoning would leave exactly the stale partial history the marker exists to
hide. Whether a governed abandon procedure should exist is an open decision (§10).

The marker lives inside the data directory, so a backup taken mid-import restores the marker and the
restored node refuses until it is resumed.

**Alternatives rejected for this version:** an atomic batch write would change ADR-0087's accepted
`AuthorityFactStore` contract to add a batch method and a single barrier. A sled transaction does
not by itself give the barrier semantics ADR-0087 §4 requires. Both remain possible later, by an
ADR-0087 amendment.

### 5. Which Subjects a node may retain

Admission accepts any self-incepted Subject, and the store only grows. Without a retention rule, any
bundle the operator is handed becomes a permanent record on the node, and a node collecting
Subjects across contexts works against IDENTITY_SEMANTICS §5.1 (one `SubjectId` per institutional
context) and §5.3 (the identifier is public to its context only), by becoming a place where they
can be correlated.

Proposed rule:

- **Allowlist.** An owner-only retention file lists each Subject the node may keep, with the context
  it belongs to, the basis for keeping it (a reference, not personal data), who added it, and when.
  A bundle whose Subject is not listed is refused before anything is written. The allowlist is the
  operator's **record** of a retention decision. It is not, by itself, the person's consent or the
  institution's authorization, and nothing may treat an allowlist entry as either (§9).
- **Context is recorded per Subject, not assumed per node.** A node may host several autonomous
  institutional domains, and a person may belong to overlapping communities, cooperatives,
  federations and other institutions under context-specific Subjects. Each allowlist entry
  therefore names the context it is retained for. Until GEN (#2602) binds a Subject to a context
  structurally, that context is the operator's recorded claim, not a verified fact. A first
  importer may choose to accept a single context per store as an **implementation limitation**.
  That limitation must not become an ICN substrate invariant.
- **Bounds.** A maximum bundle size, a maximum number of facts per import, and a maximum number of
  facts per Subject, each checked before allocation or writing. Their values are a decision
  (§10). N1 deliberately left storage pressure open (O-N8), and N4-B deliberately added no cap at
  the transport layer. The node is where that policy belongs.
- **Withdrawal.** Removing a Subject from the allowlist stops further imports for it. It does **not**
  remove facts already retained. The authority fact store has no deletion API. sled can keep deleted
  bytes on disk. An account that held an open descriptor keeps it (ADR-0088 §11). Backups and copies
  keep their own facts. A deleted row is also undetectable (ADR-0087 §9, ADR-0088 §9), so deletion
  would need its own audit trail. ADR-0088 §12 covers only abandoning a node's whole history, not
  removing one Subject. A per-Subject erasure procedure is not designed. Until it is, facts about real participants must not be imported (§9).
- **No new index.** The import report and the allowlist name `SubjectId`s only. Nothing maps a
  legacy `Did`, or any other identifier, to a Subject.

### 6. Forks are evidence

An import never filters facts by what `derive` would conclude. If the retained facts, or the
union of retained and imported facts, contain admissible competing candidates at one position, both
are kept, and `derive` reports the Subject as halted. The import report states the halt. Dropping
either candidate would hide the duplicity that halting exists to expose. A bundle that leads to a
halted Subject is not an import failure.

### 7. `icnctl device-authority verify --node-store`

An offline check of a device act against the node's own retained facts, under the same lock and the
same open path as §3, and refused while an import is incomplete. It reports the historical answer
(N4-A, at the position the act binds) and the current answer (N4-E, at the node's retained tip)
separately, names the retained tip, and labels both as answers "as of the facts this node retains".

sled has no read-only open, so opening the store may cause sled to write its own files. The check
writes no facts, and it neither writes nor removes the startup receipt.

### 8. How the limits are communicated

Every import report and every `--node-store` answer states, in fixed text:

- Imported facts are admitted, not authorized. Authority is what `derive` concludes, at the point of
  use.
- A bundle cannot prove it is complete (N4-B §3.3). The node holds only what it was given. There
  is no replication (N3).
- Rolling the data directory back, or losing it entirely, cannot be detected (ADR-0088 §9).
- Persistence survives a process crash. Surviving power loss is best effort (ADR-0087 §4, §9).
- If the store was ever readable by another account, anything imported later is exposed to an open
  descriptor that account may still hold (ADR-0088 §11).

### 9. Prerequisites that need people, not code

Before any fact about a real participant is retained:

1. Maintainer acceptance of this ADR, and of the N4-A..E contracts (currently drafts; ADR-0083 drift
   is tracked in #2806).
2. The institution's decision on whose facts a node may keep, on what basis, with what notice that
   retained facts cannot currently be erased, and who may run the import.
3. A withdrawal and erasure procedure, or an explicit institutional decision to proceed without one.
4. A custody decision for the real Subject and device keys that sign the facts (the producer of the
   bundle is the Subject's own client, never the node).
5. A security review of the import implementation before it runs against a real store.

### 10. Decisions still open

- Values of the size and count bounds in §5.
- Whether a governed abandon or erasure procedure exists, and who may invoke it.
- Whether the allowlist is a file the operator edits, or a governed record.
- Whether the import report is kept, rotated or removed at the next daemon start.

## Open architectural concerns (recorded 2026-10-24, before any implementation)

These were raised by the maintainer on review of the first draft. They are reasons this ADR stays
PROPOSED, not items it claims to have resolved:

- **Node ≠ institution.** A node may host several autonomous institutional domains. Retention,
  bounds and reports must be scoped per domain or per context, never per node by assumption.
- **Overlapping, context-specific membership.** One person may hold Subjects in several
  communities and institutions on the same node. Retention for one context must never imply
  retention, or reveal membership, in another (IDENTITY_SEMANTICS §5).
- **Institution neutrality.** Nothing in the import may assume the governing institution is a
  cooperative; a community, cooperative or federation must be treated alike.
- **Operator allowlist ≠ consent.** Who may authorize retention of a person's facts in a context,
  and how that authorization is recorded and withdrawn, is an institutional governance question
  this ADR does not answer.
- **Append-only, no erasure.** Permanent retention with no erasure path is a serious privacy and
  governance constraint, not a detail. It may block real-participant use until an erasure or
  re-keying design exists.
- **Crash safety without wedging.** The recovery protocol in §4 must be shown, by fault injection,
  never to leave a node permanently refused. An abandon path may be needed.
- **Validation ≠ atomic persistence.** Admitting the whole bundle in memory proves the bundle is
  well-formed. It does not make the store write atomic; §4's marker covers the gap, and nothing
  may describe an import as atomic.
- **Read-only checks leave the receipt alone** (§3, §7).

## Effect on ADR-0088, if accepted

- §11's "Today no runtime producer writes facts, so nothing is written after the tightening" stops
  being true for imported facts. The exposure statement would be updated to cover imports.
- §6 gains one fail-closed state, import incomplete, which every open refuses.
- §10 is unchanged in substance: the startup receipt stays a daemon diagnostic. Offline tools remove
  it and never write one.

ADR-0087 is unchanged.

## Security and privacy controls (required of an implementation)

- Data-directory lock held for the whole import; refused while `icnd` runs.
- N2-A gate before open; incomplete-restore and incomplete-import refusals before any write.
- The input must be a regular file, not a link. Its size is checked against the bound before
  reading, and decoding is strict, with no allocation beyond the input.
- Whole-bundle admission in memory before the marker. Per-fact admission, then barrier, then
  publication.
- Marker, staged copy, import report and retention file are each created owner-only (0600). They
  live in the data directory, which ADR-0088 §11 does not harden (it relies on the units'
  `UMask=0077`), so their own mode is their protection.
- Logs carry digests, counts and the `SubjectId` only. Never fact bodies, keys or payloads.
- No network, IPC or credential dependency. No real identity material in tests.
- Mutation tests that remove the marker, skip admission, publish before the barrier, or filter by
  verdict must each be caught.

## Consequences

- A node can hold real N1 history without any new network surface, and the daemon's existing
  fail-closed rehydration covers it.
- An operator handles a new refusal (import incomplete), with a resume path instead of a manual
  procedure.
- Retention becomes an explicit, recorded decision instead of a side effect of admission.
- Nothing here creates a consumer. N4 enforcement over a node's store stays a separate decision.

## Alternatives considered

| Alternative | Why not now |
|---|---|
| Atomic batch in the store | Changes ADR-0087's accepted contract. Possible later by amendment. |
| Import through a running daemon (IPC, socket, gateway) | A new authority write surface. Needs its own design and security review. |
| Reuse `AuthorityRegistry::open` | Would initialize histories outside `icnd` and write a false startup receipt. |
| Filter out facts that make a Subject halted | Hides duplicity. Halting is the intended outcome. |
| No retention rule, admit anything | Unbounded permanent storage, and a cross-context correlation surface. |
