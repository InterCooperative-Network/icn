---
id: "0088"
title: "N1 runtime authority-store initialization and recovery boundary"
status: "proposed"
date: "2026-10-24"
deciders: ["Matt Faherty"]
tags: ["identity", "n1", "authority-log", "persistence", "startup", "recovery", "icnd"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented in draft PR #2843 (not merged)"
references:
  - "ADR-0087 (the library persistence contract this ADR builds on and does not change)"
  - "ADR-0018 (ADR lifecycle)"
  - "icn#2843 (N1-R1, the implementing PR)"
  - "icn/crates/icn-core/src/authority_registry.rs"
  - "icn/bins/icnd/src/main.rs (startup ordering)"
  - "docs/guides/operations/backup-and-recovery.md (operator recovery)"
  - "docs/architecture/n2-a-migration-gate.md (the startup gate that audits the store)"
---

# ADR-0088: N1 runtime authority-store initialization and recovery boundary

## Status

**`proposed`.** Drafted with #2843. Not yet the project's decision. The
implementation exists in that draft PR. Acceptance needs the maintainer's
decision; the authorization to draft it is not one.

Decision status and implementation status are separate axes (ADR-0018).

## Context

ADR-0087 (accepted) fixes the library contract: the record layout, the
durability acknowledgment, and fail-closed rehydration. It deliberately leaves
open what a daemon does with a store: where it lives, when it must exist, and
what a refusal looks like to an operator.

That gap is a safety question. A store that vanished and a node that never had
one look the same from inside the store. Unless something outside the store
records that it once existed, `icnd` would start a node whose store was lost
with an empty authority state, and every persisted fact would disappear
without anything saying so.

This ADR decides the runtime boundary that #2843 implements. It does not reopen
ADR-0087's semantics.

## Decision

### 1. Where the state lives

| Path, under the data directory | What |
|---|---|
| `store/authority/` | the sled database holding ADR-0087's records (`authority/fact/…`) and the instance row |
| `n1-authority-initialized.json` | the initialization record |
| `n1-authority-receipt.json` | the startup receipt (§10), diagnostic only |

All three live inside the data directory, behind its storage lock and the
incomplete-restore refusal. `icnctl backup` and `restore` already treat the
data directory as one unit, so they carry the three together.

### 2. Why the record is a separate file

A marker inside the database disappears with the database, so it cannot detect
the database's loss. The record is a small file beside the stores, so:

- it survives the store directory being removed or replaced;
- it can be read before sled is opened (sled has no read-only open, and opening
  creates a fresh database in an empty directory);
- it is written with the atomic-file pattern the N2-A receipt already uses.

### 3. The record format (version 1)

```json
{
  "format": "icn.n1.authority-store-initialized",
  "version": 1,
  "instance": "<32 lowercase hex characters>",
  "store": "store/authority"
}
```

- A regular file, created owner-only (0600) whatever the process umask.
- Unknown fields, another `format`, another `version`, another `store`, or a
  malformed `instance` make the record **unreadable**, and an unreadable record
  refuses startup. It is never treated as absent.
- A link or any other non-regular file at its name is refused, not followed.

### 4. The instance row

- Key `authority/meta/instance`, value 16 random bytes. It sits outside ADR-0087's
  `authority/fact/` namespace, so `rehydrate` never sees it as a fact.
- It identifies **this store**: one database's lifetime. It is not a node
  identity, a principal, a subject or a key. It is not secret and is derived
  from nothing.
- Its key holds no `did:icn:` text, so the N2-A gate does not treat it as
  principal-bearing.

The record and the row carry the same instance. That shared value is what lets
a start tell "my store" from "a store".

### 5. Order and barriers

Initialization runs these steps in this order. Any failure stops before the
record exists:

1. Reuse the instance row an interrupted start left, or write a new one behind
   the store's durability barrier (ADR-0087 §4: survives a crash of the process,
   not power loss).
2. `fsync` each regular file directly in the store directory (sled's `db`,
   `conf` and any snapshots, not its `blobs/` subdirectory, which a 16-byte row
   never uses), then the store directory and its parent, through descriptors
   of their own. This also runs when adopting a row, because the
   process that wrote it may have died before this step.
3. Confirm the data-directory lock is still anchored, then commit the record:
   synced temporary file, rename, directory sync.

The record is the commit point. The implementation pins this order with
call-order tests and fault injection. Those prove the calls happen in this
order; they cannot prove what a disk keeps through a power cut.

The store is not closed and reopened to force a sync. sled can release its file
lock a moment after a handle drops, so an immediate reopen could refuse a
first start, and its close path panics on a failed `fsync`.

### 6. States

| record | store | outcome |
|---|---|---|
| absent | absent | **fresh**: initialize (§5) |
| absent | present, no facts | **interrupted initialization**: adopt or write the row, then §5 steps 2–3 |
| present | matching instance | **rehydrate** |
| present | absent | **refuse**: `StoreMissing`; nothing is created |
| present | no instance row, or a different one | **refuse**: `StoreReplaced` |
| absent | present, with facts | **refuse**: `RecordMissing` |
| unreadable | any | **refuse**: `RecordUnreadable`; the store is not opened |
| store cannot be inspected | | **refuse**: only `NotFound` counts as absence |

A start that fails after the record is committed is not a fresh node next time:
the record exists, so the next start rehydrates or refuses.

### 7. Startup ordering in `icnd`

1. data-directory lock (refuses an incomplete restore);
2. discard the previous start's receipt;
3. N2-A startup gate (audits `store/authority` with every other store);
4. the registry: §6, then ADR-0087 rehydration, then the receipt;
5. services.

Every refusal exits nonzero before any service starts. There is no flag that
starts with an empty authority state.

### 8. Compatibility and future formats

- The record's `format` and `version`, the store path, the instance row's key
  and length, and ADR-0087's record layout are compatibility surfaces from the
  first real node on.
- A binary meeting a record version it does not know refuses (fail closed). A
  downgrade past a format change therefore refuses rather than guessing.
- A change to any of these needs a new version and an explicit migration step,
  decided by an amendment to this ADR. Rows are never reinterpreted in place.
- A new reader must keep reading every earlier version it may meet, or say in
  its amendment that it does not.

### 9. What cannot be detected

These limits are not bugs to be fixed inside the data directory:

- **The whole data directory, record included, lost or replaced by an empty
  one.** Indistinguishable from a fresh node. Detecting it needs an anchor
  retained outside the directory, which does not exist.
- **The store rolled back to an earlier copy of itself.** The instance matches,
  and there are fewer facts. Restoring an older backup of the whole directory
  does exactly this.
- **Individual facts vanishing from an intact store** (ADR-0087 §9).
- **Power loss beyond the barriers described in §5.**

### 10. The startup receipt is a diagnostic

`n1-authority-receipt.json` (format `icn.n1.authority-receipt`, version 1)
records what the last start that got through the registry rehydrated. That
start may still have failed afterwards, at the keystore or a service. It records:
- the instance;
- how the directory was found (`initialized`,
  `completed-interrupted-initialization` or `rehydrated`);
- body and witness counts;
- for each subject, a summary of what `derive` returns;
- a SHA-256 digest of those summaries under the domain
  `icn.n1.authority-receipt.decisions.v1\0`. `icn-core`'s authority registry
  owns that domain, and the workspace domain-separation audit registers it.

What it is not:

- **Not an authority certificate.** It is unsigned, and nothing may make an
  identity or authorization decision from it. Live decisions come from
  `derive` over the rehydrated state, inside the process.
- **Not proof that a daemon is running.** It has no clock or process data, by
  design, so it is byte-identical across restarts of the same state. The daemon
  discards the previous receipt as soon as it holds the lock, so a start
  refused at the gate or by the registry leaves none. A start refused before
  the lock (another holder, an incomplete restore) cannot touch the directory,
  and an earlier receipt may remain. Liveness comes from the process and its
  log.
- **Not a protocol.** Nothing in the runtime reads it back. Its readers are
  tests and operators.

It names subjects, public authority keys and device grants. Those are not
cryptographic secrets, but they are metadata a node has no reason to share, so
the file is created owner-only (0600) and stays local. The record is created
the same way.

That protects these two files only. The store's own files, which hold the same
facts, are created by sled with no explicit mode, as every other store in the
data directory is, so they follow the process umask. The shipped unit sets
`UMask=0077`, which makes all of them owner-only. A daemon started by hand with
a lax umask (022) leaves the data directory and the stores readable by other
local accounts. Hardening the data directory itself is outside this ADR.

So the confidentiality of the authority facts against other local accounts is a
**deployment-boundary dependency**, not something this code enforces:

- **Assumption:** the daemon, and any `icnd --init` or `icnctl` run against its
  data directory, run as the data directory's account with umask `0077`.
- **Enforcement point:** the shipped units pin `UMask=0077` (ADR-0086), and an
  `icnctl` test fails if either unit stops pinning it.
- **Operational check:** `find <data-dir> -perm /077 -not -type l` prints
  nothing below the data directory.
- **Not covered:** hand-started processes with a lax umask, and the generic
  OCI image and development network, which ADR-0086 does not treat as custody
  profiles.

Creating `store/authority/` with mode 0700 regardless of umask would close
the gap for this store alone. It is left to the maintainer, because no other
store does so and it would split the data directory's permission convention.

Its JSON fields may change in a later version. A change bumps `version`. Only
the format name, the version field and the digest domain are fixed by this ADR.

### 11. Manual recovery

Every refusal in §6 needs an operator. Recovery is never automatic. The
operator procedure is `docs/guides/operations/backup-and-recovery.md`,
"Scenario 5". Its rules:

- Preserve before acting: copy the whole data directory, record and store
  together, before changing anything.
- Restoring the original authority history means restoring the data directory
  from a backup that holds it.
- Deleting the record, or the record and the store, to get past a refusal
  starts a **new** authority history on this node. It is not a repair. It is
  acceptable only where the store provably held no facts, or where the
  maintainer decides to abandon that history.

## Consequences

- A node that loses or swaps its authority store refuses to start instead of
  running without its facts.
- One more refusal class for operators, with a documented procedure.
- Three new files in every data directory.
- The record format and the instance row become compatibility surfaces.

## Alternatives considered

| Alternative | Why not |
|---|---|
| No record; treat a missing store as a fresh node | Silently drops every persisted fact when a store is lost. |
| A marker row inside the sled database | Disappears with the database it is meant to vouch for. |
| An instance derived from the node's key | Couples store identity to key custody and rotation. A store copied between nodes with the same key would be accepted. |
| Close and reopen sled to force an `fsync` | Lock-release race on first start, and a panic path in sled's close. |
| Put a timestamp or nonce in the receipt | Breaks byte-identical comparison across restarts, and still would not prove liveness. |
| Sign the receipt | Would make it look like an authority certificate. It is a diagnostic. |
| Start empty after a refusal, behind a flag | A flag that silently drops authority facts is the failure this ADR exists to prevent. |
