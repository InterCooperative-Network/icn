---
id: "0088"
title: "N1 runtime authority-store initialization and recovery boundary"
status: "amended"
date: "2026-10-24"
accepted_date: "2026-10-24"
deciders: ["Matt Faherty"]
tags: ["identity", "n1", "authority-log", "persistence", "startup", "recovery", "icnd"]
supersedes: []
superseded_by: []
amends: []
amended_by: ["ADR-0089"]
implementation_status: "implemented; #2843 merged to main as 94cf94d13 on 2026-10-24. The ADR-0089 amendment (accepted 2026-10-27) is not implemented"
references:
  - "ADR-0087 (the library persistence contract this ADR builds on and does not change)"
  - "ADR-0018 (ADR lifecycle)"
  - "ADR-0086 (deployment profiles; the units' UMask=0077)"
  - "icn#2843 (N1-R1, the implementing PR)"
  - "icn/crates/icn-core/src/authority_registry.rs"
  - "icn/bins/icnd/src/main.rs (startup ordering)"
  - "docs/guides/operations/backup-and-recovery.md (operator recovery)"
  - "docs/architecture/n2-a-migration-gate.md (the startup gate that audits the store)"
---

# ADR-0088: N1 runtime authority-store initialization and recovery boundary

## Status

**`accepted` (2026-10-24); `amended` (2026-10-27).** Drafted with #2843. The maintainer (Matt
Faherty, the decider) accepted it on 2026-10-24:

- the substantive architecture as proposed: the initialization state machine
  (§5, §6), the compatibility commitments for the record and the instance row
  (§3, §4, §8), fail-closed recovery (§6, §7, §12) and the receipt as a
  diagnostic only (§10);
- one change to the proposal: `store/authority/` is owner-only (`0700`)
  whatever the process umask, as a deliberate defense-in-depth exception to
  the data directory's storage convention (§11). The proposal had left this
  to the maintainer.

The decision was given as a written instruction in #2843's delivery session.
ADR-0018 requires no separate signature. The reviews of the implementation
were by independent review agents; no other human review is claimed.

Decision status and implementation status are separate axes (ADR-0018). The
implementation is #2843, merged to main as `94cf94d13` on 2026-10-24.

> **Amendment (2026-10-27):** [ADR-0089](ADR-0089-n1-offline-authority-fact-import.md),
> accepted 2026-10-27 for a fixture-only offline import experiment, amends this
> ADR. The amendment is an **accepted future contract, not current behaviour**:
> no offline import producer exists, and `icnd` does not check any import marker.
>
> - **Implemented on `main` today:** everything in this ADR as written — the
>   initialization state machine and its fail-closed states (§5, §6), the
>   receipt (§10), the access rules and their exposure limits (§11), and manual
>   recovery (§12).
> - **§6, future:** one more fail-closed state, *import incomplete*. Once
>   ADR-0089's marker is implemented, `AuthorityRegistry::open` refuses while
>   it exists. It is not enforced today.
> - **§11, unchanged today:** "no runtime producer writes facts, so nothing is
>   written after the tightening" remains true until an importer exists. Once
>   one does, imported facts are written after the tightening, and an account
>   that ever held an open descriptor on the store can read them; ADR-0089 §12
>   requires that limit to be stated to operators.
> - **§12, future:** manual recovery gains the import-incomplete state and its
>   procedures (`icnctl authority import --resume`, re-obtaining the same
>   segment, or restoring a pre-import backup; ADR-0089 §6), when implemented.
> - **Unchanged:** the startup receipt stays a daemon diagnostic (§10); offline
>   tools never write one. ADR-0087 is unchanged.

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
| `store/authority/` | the sled database holding ADR-0087's records (`authority/fact/…`) and the instance row; the directory is owner-only (§11) |
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
| any | a link, not a directory, or another account's directory | **refuse**: `StoreUnprotected`; nothing in it is opened (§11) |

Before any row above that opens the store, its directory is checked and, if
needed, set owner-only (§11). That never changes which row applies.

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
the same way. Who can read the store holding the same facts is §11.

Its JSON fields may change in a later version. A change bumps `version`. Only
the format name, the version field and the digest domain are fixed by this ADR.

### 11. Who can read the authority store

**Decision.** `store/authority/` is owner-only (`0700`) whatever the process
umask. This is a deliberate exception: every other store in the data
directory, and the data directory itself, follow the umask.

sled creates its files with no explicit mode, so they follow the umask: under
umask `022` they are readable by every local account, under `000` writable
too. A directory no other account can search keeps them unreachable whatever
their own modes. So the guarantee is placed on the directory, at the one
boundary this ADR owns, and not on sled's files.

Before the registry opens it, under the data-directory lock:

1. **Absent:** created with mode `0700`, not through `create_dir_all`. Its
   parent `store/` is created if missing, with the default mode, as any store
   would create it.
2. **Present:** inspected without following a link. It must be a directory,
   not a link and not any other file, owned by the account the process
   creates files as. Anything else is refused with `StoreUnprotected`, and
   nothing in it is opened. A link to a valid store is refused too, and its
   target is not changed.
3. **This account's directory with another mode**, as a restore or a manual
   copy can leave it: set to exactly `0700` through a descriptor of the inode
   just checked, synced, and logged as a warning. Nothing inside is touched:
   no fact, instance row or record is removed or regenerated, and the start
   continues by §6.

The N2-A gate runs before this (§7). It never creates a store, so a new store
is created owner-only before sled writes anything in it. An existing store is
different: the gate's audit opens it first, while it still has the mode it was
found with, and sled may write files there then. Those files hold what the
wider directory already exposed. The guarantee below starts once this step has
run. The gate also refuses a link, and a directory it cannot read, anywhere in
its discovery walk, so through `icnd` those reach the operator as gate
refusals rather than `StoreUnprotected`.

**What it guarantees.** Once a start has passed this step, no local account
except the owner and root can list the store or open any file in it by path,
whatever the umask that created those files.

**What it does not cover:**

- **Every other store and the data directory itself.** Their confidentiality
  remains the deployment-boundary dependency of ADR-0086: the shipped units
  set `UMask=0077`, and an `icnctl` test fails if either stops doing so. The
  data root is not hardened by this ADR.
- **The directories above `store/authority`.** They are assumed writable only
  by the owning account. A daemon started by hand under umask `000` creates
  `store/` writable by everyone. Another account could then move
  `store/authority` aside and put its own directory there. A substitute
  present when the check runs is refused, being another account's. One
  swapped in between the check and sled's open is not detected. The next
  start refuses it. Through the same writable `store/`, a non-directory
  swapped in at the moment of the check could also stall the start rather
  than fail it.
- **Exposure before the mode was set.** A file another account opened, copied
  or linked while the directory was wider stays readable to it. Tightening
  prevents new access; it does not withdraw access already taken. This
  includes **facts written later**: sled keeps appending to the same `db`
  file, so a descriptor another account opened on it while the directory was
  wider keeps reading what is written after the tightening. That outlives
  the daemon: sled reuses the same `db` file across restarts, so the
  descriptor reads facts written by later runs too. The same holds for a
  handle another account kept on a subdirectory sled creates (`blobs/`),
  through which it can open files created there later, because the `0700`
  is checked on `store/authority` alone. No mode change can revoke an open
  descriptor. Only rebuilding the store into new files and directories
  would, and this decision does not rebuild a store to fix its permissions. A store that was ever readable by
  another account should be treated as exposed to it. Today no runtime
  producer writes facts, so nothing is written after the tightening.
- **The N2-A gate's audit of an existing store.** It runs before this step
  (§7), so for an existing store with a wider mode, sled opens the store, and
  may write files in it, while it is still wide. The audit only reads rows, so
  those files are sled's own bookkeeping for contents the store already held,
  not new facts. They take the umask, so they can be readable even when the
  older files were not. This adds no new class of exposure beyond the
  previous bullet. It does mean
  that on an existing store the protection is not in place before every sled
  open. Moving the check ahead of the gate would shorten that window by the
  gate's duration and close no other gap. The previous bullet is the
  material one.
- **Copies outside the data directory,** backup archives included. They are
  protected by wherever they are kept.
- **Root and the owning account.**
- **Non-Unix platforms,** where no mode claim is made.

The tests check the directory's mode bits and the refusals. They do not show
another account actually failing to traverse the directory, because the
development environment has no second account to run as.

### 12. Manual recovery

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
- `store/authority/` is the one store whose directory mode does not follow
  the umask. A tool that recreates the data directory's layout must keep it
  owner-only, or the next start sets it back and logs a warning.

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
| Rely only on the units' `UMask=0077` for the authority store | Leaves the facts readable by other local accounts whenever `icnd` is started by hand under a lax umask. |
| Make the whole data directory owner-only | A wider change to every store's convention and to tooling that reads the data directory. This decision is bounded to the store this ADR owns. |
| Refuse a store directory with a wider mode instead of setting it | Turns a restored backup into an outage for something the process can safely fix. Setting the mode keeps every fact, and the warning tells the operator. |
| Chmod sled's files as well as the directory | sled creates files later (snapshots, blobs) with no mode of its own. The directory covers files created later too. |
