# ICN Multi-Export Continuity Fixture — Conductor Handoff

Five **real consecutive outputs of ICN's restricted observation exporter**, from fictional
registries whose state was driven through ICN's own lifecycle code, so a consumer can test how
ICN actually changes over time without connecting to ICN. Nothing is live: every path, host,
conversation id and session id is invented (`/synthetic/series-root/...`, `series-host-a/b`).

Profile `observation`, policy `observation-v1`, sidecar format `icn-ops-registry-snapshot-meta/v1`,
registry schema `5` — **the same producer contract as the golden fixture** (`reference/contract.json`,
`reference/test-vectors.json`, unchanged). Only *time* is new.

> **No database or sidecar here was edited after export, and no sidecar was re-sealed.** What is
> *controlled* is stated below. Anything not stated there is the real exporter's behaviour.

## What is real and what is controlled

| Real | Controlled by the generator (and why) |
|---|---|
| `exportRegistrySnapshot({profile:"observation"})` for every snapshot | `Date`: a **virtual clock**, so acquisition times and a simulated hour are coherent. Acquisition times are therefore *not* wall-clock time at generation |
| `registerSession` incl. its resume/dedupe, lane-move and generation-change paths; `recordHeartbeat`; `recordProgress`; `releaseSession` | SQLite's `datetime('now')` cannot be replaced, so the **row an operation touched is re-stamped** with the virtual instant (the technique ICN's own tests use to age rows) |
| the sidecar contents, hashes and bindings | `crypto.randomUUID` → a counter, so session ids are deterministic labels |
| claim-token salting (random per export) | `os.hostname()` per registry → fictional claimed hosts |
| the registry's schema, indexes and WAL (committed rows were un-checkpointed at every export) | the **claim rule is mirrored** (the real `claim_files` handler runs on SQLite's real clock); a test pins the mirror to the real handler |

The release also wrote ICN's real `session-log.jsonl` history line — to a **temporary** root, never
shipped. (The runtime resolves that log from `ICN_ROOT` at import, so the generator redirects it
first; otherwise a release would append to the real checkout's log.)

## The five exports

| Label | Registry | Claimed host | `acquisition.started_at` (virtual) | Sessions | Claims (withheld rows) |
|---|---|---|---|---|---|
| A1 | A | `series-host-a` | 2026-01-01T10:10:00Z | 4 | 3 (1) |
| A2 | A | `series-host-a` | 2026-01-01T10:25:00Z | 5 | 4 (1) |
| B1 | **B** | `series-host-b` | 2026-01-01T10:30:00Z | 2 | 2 (1) |
| A3 | A | `series-host-a` | 2026-01-01T11:10:00Z | 5 | 5 (2) |
| A4 | A | `series-host-a` | 2026-01-01T10:50:00Z | 5 | 5 (2) |

Generation order is A1, A2, B1, A3, A4. **A4 is adversarial**: the clock stepped back before an
unchanged re-export of A. Registry stamps (`schema_version.applied_at`): A `2026-01-01 08:00:00`,
B `2026-01-02 09:30:00`. Session labels → ids are in `series-contract.json` (`session_labels`):
`s-a…0001, s-b…0002, s-e…0003, s-l…0004, s-c…0005, b-1…0006, b-2…0007, s-d…0008`.

## Transitions (what actually happens, from `reference/series-contract.json`)

| Step | Outcome |
|---|---|
| A1 → A2 | `s-c` appears (new activation, lane beta). `s-a` progress 3→5 on the same id. Claims: +`other.rs` (s-c). All rows live under a 30-min TTL |
| A2 → A3 | `s-c` **absent**: it was *released* (row deleted, its claim dropped). `s-d` **new id, same conversation** as `s-c`, `progress_count` restarts (1). `s-a`: **only** `worktree_generation` changed, same id (lane recreated at the same path). `s-e`: **lane moved** — `worktree_id`, `worktree_name` and `worktree_generation` changed together on the same id, `repo_id` did not. `s-b` and `s-l` are **stale** (ages 65/67 min) **but still present** |
| A2 → A3 claims | `s-d` was **refused** `lib.rs` (holder `s-a` live) and **granted** `notes.md` and a withheld path (holder `s-b` stale) ⇒ a genuine **coexisting-holder conflict**: `notes.md` held by stale `s-b` and live `s-d`; the withheld path likewise (one token shared by both rows *within A3*) |
| A3 → A4 | **refused as a successor**: A4's `started_at` is earlier. Three rows carry timestamps *after their own export's acquisition* (ages −8, −12, −18). Content is otherwise identical to A3 |
| A2 / B1 / A3 | **refused**: B is a different registry (`applied_at` differs) although it shares a conversation id (`s-a`'s) and the lane digests with A. That overlap is coincidence, never identity |

Rows `s-b` (NULL conversation id) and `s-l` (NULL `repo_id`/`worktree_id`/generation/conversation)
are unknown identities: they are never equal to anything, including each other.

## Answers from ICN's source (not from assumption)

1. **Can `provider`, `provider_session_id` or `worktree_generation` change for an existing session id?**
   * `provider`, `provider_session_id`, `host`, `parent_session_id`, `repo`, `started_at` are set **only in the `INSERT`**
     (`runtime/session-runtime.ts:324-352`). No `UPDATE` anywhere in production touches them (`:244,285,292,372,441` are the only
     `UPDATE sessions`). Re-registering a known conversation with a different `provider` or `host` leaves the row unchanged
     (pinned by a new test).
   * `worktree_generation` **can** change on an existing id, only through `registerSession` **re-registering a known
     `provider_session_id`** (the dedupe path, `:224-321`): (a) **lane move** (`worktree_id` differs): `repo_id`, `worktree_id`,
     `worktree_path`, `worktree_name`, `worktree` **and** `worktree_generation` are overwritten together (`:241-265`); (b) **same lane,
     new generation** (a lane removed and recreated at the same path): `worktree_generation` alone is overwritten, **including with
     `NULL`** when the token cannot be minted (`:266-289`). A session registered **without** a conversation id never dedupes, so its lane
     columns never change. Pinned by `worktree-identity.test.ts:565` and `:491`.
2. **Release / retire?** `releaseSession` **deletes the row** and its `file_claims` (`:581`, `:569`). There is **no lifecycle-state
   column** (v3 deliberately has none; `tools/sessions.ts:175`). "Retire" is not a registry operation. Stale rows are **never
   reaped**: `DELETE FROM sessions` exists in no other production path. History survives **only** in
   `ops/state/session-log.jsonl`, outside the registry and outside every export. A released conversation re-registered later gets
   a **new activation** (`session-runtime.test.ts:405`, `worktree-identity.test.ts:293`); heartbeat/progress cannot resurrect a
   released id (`session-runtime.test.ts:175`).
3. **Is the acquisition start timestamp monotonic?** **No — it is a sampled system clock.** `acquisition.started_at` is `new Date()` at
   the start of the export (`state/snapshot-export.ts:565`). Nothing compares it with a previous export, persists a high-water mark,
   or refuses a regression. Registry row stamps are SQLite `datetime('now')`, **one-second resolution**, also unguarded. A4 shows the
   consequence. Per-row `progress_count` is the only monotonic counter, and only within one row.
4. **Is there a registry-instance identifier?** **No.** No `registry_id`, no `PRAGMA user_version`/`application_id` (both 0), no
   per-registry UUID. The `randomUUID` calls mint session ids, lane-generation tokens (stored in a *git admin directory*, not the
   registry) and CLI harness keys. The sidecar deliberately carries no source identity (a hash of guessable path metadata was
   removed as a fingerprint). The **only** registry-scoped value is `schema_version.applied_at`, written once per registry:
   **differing stamps prove "different"; equal stamps prove nothing** (a restored copy keeps its stamps).

## Rules a reader must follow (also in `series-contract.json`, `rules`)

R1 sameness is never derivable — assert it out of band; differing `applied_at` refutes it. R2 a later `started_at` is not authenticated
freshness; refuse non-increasing order and flag row stamps after the export's own acquisition. R3 NULL identity = unknown, never equal.
R4 claim tokens are per-export: never compare token values across snapshots. R5 `repo, provider, provider_session_id,
parent_session_id, host, started_at` never change for an id — if they do, refuse. R6 lane fields change only as described above.
R7 `progress_count` never decreases for one id; a new id restarts at 0. R8 absence means the row was deleted; **do not conclude the
session "ended" from absence alone**. R9 a stale row is not an ended session. R10 a conversation id is on at most one row per export.

**Absence versus unknown.** The exporter gives *complete* content within one export (one read transaction, row counts bound). It
gives **no** evidence across exports about identity, order or completeness relative to a prior state. So an absent row is a fact about
that snapshot only; "released" is an inference valid under an out-of-band same-registry assertion.

**Staleness.** The TTL (default 30 min, `ICN_SESSION_TTL_MINUTES`, minimum 5) is **not in the snapshot**. Ages are computed against
`acquisition.started_at`; without an agreed TTL, staleness is UNKNOWN.

## ICN facts that contradict assumptions a consumer could plausibly hold

(I have not seen Conductor's assumptions; these are facts, stated so they can be checked against them.)
* A session id's **lane is not immutable**: it can move, and its generation can change, on the same id.
* **No "ended" state exists**; absence ≠ ended, stale ≠ ended.
* **Two live-looking holders of one file can coexist**: a claim is refused only if the other holder's heartbeat is fresher than the TTL.
* A claim **leaves the registry only with its session** (no unclaim tool); re-claiming a path replaces its row and resets `claimed_at`.
* **No registry identity, no monotonic clock, no TTL in the data.**
* Equal digests / conversation ids across registries are **coincidence** (unkeyed digests of path strings that two hosts can share).

## Using it offline

```
python3 -I verify_series.py            # in the unpacked directory; exits 0 only if every check passes
sha256sum -c SHA256SUMS
```
`verify_series.py` is standard-library Python. It imports no ICN code and **recomputes every per-export fact and every pairwise
verdict from the databases**, comparing them with `reference/series-contract.json`. Passing proves integrity and consistency, not
authenticity: nothing is signed, and the files carry no registry identity.

Regeneration needs an ICN checkout and Node ≥ 22.6 (`series.ts`, command in `MANIFEST.json`). `series-contract.json` regenerates
byte-identically; the snapshots do not (random claim-token salt), so treat any one generated set as a sample.
