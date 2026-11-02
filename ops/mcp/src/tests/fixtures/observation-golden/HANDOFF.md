# ICN Golden Observation Fixture — Conductor Handoff

A reproducible, **fully synthetic** sample of what ICN's *restricted observation* registry snapshot
looks like, produced by ICN's real exporter, so a consumer can write real compatibility tests
without any ICN data and without connecting to ICN.

**Authoritative files** (nothing else is normative):

| File | What it is | Committed? |
|---|---|---|
| `reference/contract.json` | machine-readable producer contract, *derived from ICN source and from the generated database* | yes — byte-stable text |
| `reference/test-vectors.json` | published digest / token-formula / path / verbatim-text vectors and fixture facts | yes — byte-stable text |
| `observation.db` | the SQLite snapshot, as the exporter wrote it | **no — generated** |
| `observation.db.meta.json` | its completion marker / provenance sidecar | **no — generated** |

The pair is generated, not committed: this repository ignores `*.db` (`.gitignore`), and a
committed sidecar whose hash named an absent file would mislead. Generate it (below), or take a
copy produced by ICN. A given copy's `sha256` is meaningful only for that copy.

Everything in the data is fictional (`/synthetic/golden-root/...`, `golden-host-a`, made-up UUIDs).
The only real strings are tool versions in the sidecar. If you find anything else, that is a bug.

> **This fixture is not the full recovery profile.** ICN has two distinct disclosure policies,
> `full` (recovery, inside ICN's trust boundary) and `observation` (this). A reader must select the
> observation profile explicitly and refuse anything else.

## What is in the database

Exactly three tables, plus indexes. **No views, triggers or virtual tables.**

| Present | Absent *by design* (not exported) |
|---|---|
| `schema_version`, `sessions`, `file_claims` | `mailbox`, `events`, `watchers_process`, `health_cache`, `decision_index` |

An absent table means **not exported**. It is never evidence that the table has no rows; do not
synthesise an empty one.

Header: `SQLite format 3`, page size 4096, UTF-8, `application_id` 0, `user_version` 0, file-format
versions `1/1` ⇒ **delete journal mode** (no `-wal`/`-shm` exist or are created by reading it).
Open it read-only.

### `sessions` (24 columns — the production schema, unchanged)

| Disposition | Columns | In the export |
|---|---|---|
| verbatim | `id`, `repo`, `worktree_name`, `worktree_generation`, `provider_session_id`, `parent_session_id`, `provider`, `agent_pid`, `host`, `started_at`, `last_heartbeat`, `last_progress`, `progress_count` | as the registry stored them |
| **digest** | `repo_id`, `worktree_id` | `sha256:` + hex (see below) |
| **withheld** | `worktree`, `worktree_path`, `branch_at_registration`, `head_at_registration`, `task_description`, `task_ref`, `pr_ref`, `transcript_path`, `current_activity` | **always `NULL`** |

Declared types and NOT NULL rules are in `contract.json`. The ones that matter:

* `repo` is `TEXT NOT NULL`. **`repo_id` is nullable** (added in schema v3 without `NOT NULL`).
  The exporter preserves `NULL` as `NULL` and never invents a digest; a non-NULL `repo_id`
  *always* exports as a valid `sha256:` digest and never as the original path. The fixture's
  `sess-legacy-1` is a pre-identity row with `repo_id`, `worktree_id`, `worktree_generation` and
  `provider_session_id` all `NULL`.
* `NULL` in a **withheld** column means **WITHHELD** — never "absent", "none" or "unknown to ICN".
* `worktree_generation` `NULL` = generation unknown (a pre-v5 row). `provider_session_id` `NULL` =
  harness conversation unknown for that activation.
* Timestamps are `TEXT 'YYYY-MM-DD HH:MM:SS'`, UTC.

### `file_claims`

`file_path`, `session_id`, `claimed_at`. A path is shown **as written only if** every `/`-segment
matches `[A-Za-z0-9._@+=,-]+`, none is `..`, and it is ≤ 512 bytes. Anything else (absolute,
`~`, `..`, `file://…`, drive letters, backslashes, **spaces, non-ASCII**) is replaced by
`withheld:` + 16 hex. **The row is always kept**, so a withheld path never turns a claim into a
non-claim, and two claims on the same withheld path share one token (a visible conflict).

### Identifiers — what is and is not secret

* **Digest:** `'sha256:' + lowercase-hex(SHA-256(UTF-8 bytes of the stored value))`. No salt, no key,
  no normalisation, no trailing newline. It is **unkeyed**: it is *not* secret, *not*
  authenticated and *not* unguessable. Anyone who knows or can enumerate the raw value can
  recompute it and so confirm a guess. It removes filesystem layout from the file; it is
  pseudonymisation, nothing more. Stored raw values are the canonical git paths ICN produced
  (`repo_id` = git common dir, `worktree_id` = git dir), so matching needs the *same canonical
  string*, obtained out of band.
* **Claim token:** `'withheld:' + first 16 hex of SHA-256( salt ‖ 0x00 ‖ UTF-8(path) )`, salt = 16
  random bytes **per export, never recorded**. Equal paths share a token **within one snapshot
  only**. **It cannot be recomputed from the snapshot**; do not try, and do not compare tokens
  across snapshots.

| Recomputable with no private input? | |
|---|---|
| Digest of a raw value you already hold | **Yes** — vectors below |
| Token *formula* with a published test salt | **Yes** — `test-vectors.json` |
| The fixture's own claim tokens | **No** — the salt was discarded |
| Anything about the real ICN host | **No**, and none is present |

Stable vectors (synthetic inputs; also checkable with `printf '%s' <raw> \| sha256sum`):

```
repo_id      /synthetic/golden-root/repos/icn-golden.git
          -> sha256:9154223820c838e1e605e14b99ca760b6b19ea96eac9d2d53e2364737c21ea42
worktree_id  /synthetic/golden-root/repos/icn-golden.git/worktrees/lane-alpha
          -> sha256:5e7dfa6ddf675d380bf46149b4663cc771937fd3c6f6c17ab70e1c87120d8326
worktree_id  /synthetic/golden-root/repos/icn-golden.git/worktrees/lane-beta
          -> sha256:288eedd18a70da9dd57da3b3e4db2a162cb98c46867d7a13cdc260b172e29e6f
(empty string, non-NULL)
          -> sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
token formula, test salt 000102030405060708090a0b0c0d0e0f:
  "~/GOLDEN-CANARY-tilde-claim"  -> withheld:20901a0db89b7648
```

## The fixture's facts (so tests can assert them)

| Fact | Where |
|---|---|
| Lane *alpha* has 3 activations: `sess-alpha-1` (conversation ✓, generation ✓), `sess-alpha-2` (conversation `NULL`, generation ✓, parent `sess-alpha-1`), `sess-alpha-3` (conversation ✓, generation `NULL`) | `worktree_id = digest(lane-alpha)` |
| Lane *beta* has 1 activation (`sess-beta-1`) that an alpha lookup must never return | `worktree_id = digest(lane-beta)` |
| `sess-legacy-1` has `NULL` for every identity column | `worktree_id IS NULL` |
| 7 claims; **ownership conflict on a disclosable path** `icn/crates/golden-core/src/lib.rs` (`sess-alpha-1` vs `sess-beta-1`) | cleartext |
| **Conflict on a withheld path**: the same token on both `sess-alpha-1` and `sess-beta-1` | `withheld:` |
| One withheld unique claim (`~/…`) and one tokenised path with a space | `withheld:` |
| The source registry held the committed rows **in the WAL, un-checkpointed** | generator asserts WAL bytes > 0 |

## The sidecar (`<snapshot>.meta.json`) — the completion marker

* Filename: the snapshot's file name + `.meta.json`, same directory. Format
  `icn-ops-registry-snapshot-meta/v1`, `profile: "observation"`, `disclosure.policy:
  "observation-v1"`, registry schema version `5`.
* **Published after the database.** A database with no sidecar is **incomplete** (this is what a
  hard kill between the two writes leaves). Treat it as *no observation*.
* `snapshot.sha256` = SHA-256 of **every byte of the database file**; `snapshot.bytes` its length.
  The sidecar does **not** cover itself. This is **integrity**, not authenticity: anyone who can
  write both files can write a matching pair. Nothing is signed.
* `host.claimed.hostname` is **self-asserted**; `host.authenticated` is `null`,
  `host.authenticity` is `"none"`. Never present it as an identity. (In this fixture it is the
  fictional `golden-fixture-host`.)
* No source path, device, inode or derived source identity is present, by design.
* `consistency.table_row_counts` is bound to the database: `{file_claims: 7, schema_version: 5, sessions: 5}`.
* Environment-dependent (differ on every regeneration): `acquisition.*_at`, `source.*_at_start`,
  `producer.*`, `snapshot.sha256/bytes`. The full field list with types is in `contract.json`.
* Resource limits and failure codes are in `contract.json` (`limits`, `failure_semantics`).

## What ICN's own checker enforces (so you can mirror it)

`snapshot-export --verify <snapshot>` / `verifySnapshotPair` requires: sidecar present, ≤ 1 MiB, a
JSON object, `format` known, `profile` known, `disclosure.classification` and `disclosure.policy`
**consistent with `profile`**, `snapshot.file` equal to the file name, `sha256` and byte length
matching, `schema_version_max` matching, clean `integrity_check` and `foreign_key_check`, delete
journal mode, exactly the observation tables, withheld columns `NULL`, digests well-formed, claim
paths either disclosable or tokens, verbatim text within its character class, and
`table_row_counts` equal to the database.

## Conductor-side requirements vs ICN today

| Conductor plan | ICN status |
|---|---|
| Explicit restricted-profile selection | **Already satisfied.** `profile` and `disclosure.policy` exist; pin them, do not read-and-trust. |
| Mandatory marker verification | **Already satisfied by the producer**; the reader must implement it. Order is DB-first, sidecar-last. |
| Strict profile/policy version matching | **Already satisfied**: `profile`, `disclosure.policy`, `format`, `schema_version_max`. ICN's `--verify` now also refuses a sidecar whose policy contradicts its profile. |
| Digest-based lane lookup only | **Satisfied.** You must obtain the canonical raw git-dir string out of band and hash it; ICN provides no other lookup key. |
| Provider/conversation correlation | **Satisfied**: `provider_session_id` verbatim, `NULL` = unknown. |
| UNKNOWN for unmapped host/worktree relations | **Conductor-side.** ICN supplies `host` only as a claim and `worktree_generation` `NULL` = unknown. |
| Fail-closed schema/metadata validation | **Conductor-side**, against `contract.json`. |

No change to the *profile* is needed for any of the above.

## Decisions that are still open (not for either side to assume)

1. Whether an **unkeyed** digest is acceptable as the cross-project lane key, or a keyed/registered
   identifier is wanted (that needs key custody ICN does not have today).
2. Whether the reader may rely on DB-first/sidecar-last, or needs an atomic single-artifact form.
3. Producer **authenticity**: nothing signs the pair or attests the host.

## Using it offline

Standard-library Python, run from the directory that contains the generated pair (this exact script was run
and returns the three lane-alpha activations, including the `None` conversation id):

```python
import hashlib, json, sqlite3

snap = "observation.db"
meta = json.load(open(snap + ".meta.json"))
assert meta["profile"] == "observation" and meta["disclosure"]["policy"] == "observation-v1"
assert hashlib.sha256(open(snap, "rb").read()).hexdigest() == meta["snapshot"]["sha256"]

lane = "/synthetic/golden-root/repos/icn-golden.git/worktrees/lane-alpha"
digest = "sha256:" + hashlib.sha256(lane.encode("utf-8")).hexdigest()
db = sqlite3.connect(f"file:{snap}?mode=ro", uri=True)
print(db.execute("SELECT id, provider_session_id FROM sessions WHERE worktree_id = ? ORDER BY id", (digest,)).fetchall())
```

Regenerating (ICN checkout, Node ≥ 22.6, no build, nothing written under `dist`):

```
cd ops/mcp
node --experimental-strip-types --no-warnings \
     --import ./src/tests/fixtures/ts-resolve-register.mjs \
     src/tests/fixtures/observation-golden/generate.ts <empty-output-dir>
```

`reference/contract.json` and `reference/test-vectors.json` regenerate **byte-identically** (ICN's tests compare a fresh generation against them). `observation.db` and its
sidecar do **not**: each export draws a fresh random token salt and records generation-time
fields, so the hash changes while the *logical* content (everything except token values) is
identical. ICN's test suite proves both halves. Treat `reference/` as the reference, and any one
generated pair as a sample of it.
