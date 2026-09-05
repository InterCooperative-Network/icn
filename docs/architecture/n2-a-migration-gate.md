# N2-A — migration gate: collision scan, dispositions and partner invariants (#2627)

**Status:** living — design and evidence record for the N2-A tranche
**Truth class:** descriptive
**Canonical:** no — `docs/architecture/IDENTITY_SEMANTICS.md` owns the semantic contract and
`docs/architecture/n2-a0-stored-key-inventory.md` owns the measured stored-key surface; this
document owns only N2-A's *dispositions and design*
**Last reviewed:** 2026-09-04
**Source basis:** live `main` at `2f9def5d5aca40c8b2c357df9abd2a731f542cc7` (§1 baseline rows
still cite the `83682563` measurement they were taken at)
**Gates:** N2-A / #2627 (`Did` canonicalization, I7)
**Contract:** IDENTITY_SEMANTICS §3, §7.5, §11 (I7), §14 (`N2-A`)

---

**Tranche state, in five separate claims.**

1. **I7 code has landed.** `Did` `PartialEq`/`Eq`/`Hash` compare the decoded identifier bytes
   (#2686, `0defbde5`). The design in §6 was applied; `Display`/`as_str`/`Serialize` are
   byte-identical to before, and no persisted byte moved. The sentence this document carried
   until 2026-09-02 — *"`Did` `Eq`/`Hash` is unchanged and must stay unchanged"* — was stale from
   that merge onward and is corrected here.
2. **The fail-closed check now lives inside the binary** (§10). `icnd` runs the N2-A startup
   gate over every sled database beneath its data directory before it opens the first one, and
   refuses to start over an unruled alias collision, an uncovered principal row, an unreadable
   row, or a receipt from a newer generation. This discharges §3.5's consequence and §12.1 item 7
   of the inventory *for the binary*; it is not deployment evidence.
3. **The ledger's own loaders classify before they adopt** (§4.1). The three principal-keyed
   rebuilds in `Ledger::new` — `ledger:balance:`, `ledger:cleared_volume:`, `ledger:frozen:` —
   classify their whole keyspace and refuse with a typed `PrincipalRowsRefusal` rather than
   collapse two spellings of one account, whether or not the startup gate ran in front of them.
   That is the `icn-ledger` half of §9 row 3. It authorizes no merge rule; §10.6 states how the
   gate and the loaders divide the work.
4. **The first derived projection is dispositioned as a projection, and the remaining
   boundaries are classified by the mechanism that proves them** (§11). The `icn-federation`
   agreement party index (`idx_agreement_party/`, §4 row 20) is written only from the canonical
   `federation/agreements/` rows, and since #2707 its store answers a party lookup from canonical
   membership under `Did` equality, retires superseded rows, refuses what it cannot attribute and
   can rebuild the projection — so a two-spelling pair there is registered `Equivalent`, a
   redundancy and not an authoritative conflict, and the startup gate clears it under the same
   descriptor. §11.1 states the persistence-boundary classes P1–P5 that keep that case apart from
   the attestation store's fail-closed rows and the ledger's guarded folds; §11.4 dispositions the
   rest of the inventory by class. Nothing in §11 authorizes a merge rule for authoritative state.
5. **The treasury loader classifies before it adopts** (§4.2, #2627 M1). `TreasuryManager::with_store`
   classifies every primary `ledger:treasury:<did>` row through the same `icn_ledger::principal_rows`
   guard, proves the key spelling and the body spelling are the same bytes, checks the persisted
   cooperative index against the primary rows, and refuses with a typed error before the first
   in-memory map is touched — whether or not the startup gate ran in front of it (`icnctl` opens
   the same store with no gate). The primary row is registered in the scanner as
   `icn-ledger/treasury`, fail closed, so the gate refuses the same alias pair and an ordinary
   treasury row is no longer *uncovered*; the audit and budget-index siblings that embed a
   spelling remain uncovered and are the next treasury follow-up. It authorizes no merge rule.
6. **Cutover is not complete.** The rest of the load/rebuild/write-back audit (§9 row 3: the
   loaders in §6.5 and the boundaries in §11.4), fresh point-in-time evidence on quiesced stores,
   the two unscanned deployments, the §5 decision-**A** namespace splits, the peer-map pair (§6.3)
   and everything behind §7.5 remain open. Nothing here is a deployment-readiness claim.

This document is the design and evidence surface for N2-A. It does not migrate any store or
discharge the §7.5 membership/vote gate. It records what was measured, what was decided, what
is enforced, and what is still missing — deliberately in separate sections, so a decision is
never mistaken for evidence.

Companion documents:

* `docs/architecture/IDENTITY_SEMANTICS.md` — §11 I7, §14 DAG node N2-A. Owns the contract.
* `docs/architecture/n2-a0-stored-key-inventory.md` — the N2-A0 inventory (#2623). Owns the
  keyspace list this document dispositions.

---

## 1. Baseline

| Item | Value |
|---|---|
| Merged `main` at entry | `836825632ebb5b7b9d8d16354974503a7c576569` |
| Inventory measured at | `798c8d54` |
| `Did` `Eq`/`Hash` | **principal-keyed** since #2686 — compares the decoded 32 identifier bytes (`icn/crates/icn-identity/src/lib.rs`) |
| Governance #2641 / PR #2677 | merged; runtime vote interpretation is principal-safe, no persisted byte moved |

### 1.1 Drift from the prior evidence pass

The prior pass listed inventory row #1 (`icn-net` replay state) as *"migrate in tranche, merge
rule = max floor"*. That is now **partly discharged in-tree** and the row's hazard has changed
shape rather than disappearing.

PRs #2644, #2647 and #2649 re-keyed `icn-net` replay protection onto a `SenderPrincipal`
(`icn/crates/icn-net/src/replay_guard.rs`), and the load path already:

* folds several spelling-distinct rows for one sender into one window, taking the **maximum**
  floor, so a lower floor cannot win;
* **declines to collapse** rows whose readable interpretations disagree, preserving each
  interpretation for the load pass rather than electing a survivor;
* rewrites survivors onto a canonical key only where a single interpretation is established.

Two consequences for N2-A:

1. Rows #1–#3 no longer need N2-A to invent a merge rule — the live loader already implements
   one, and its `replay_sender_regime` behaviour is *fail closed*, matching §12.1 item 7.
2. `SenderPrincipal` keys on `VerifyingKey`, **not** on decoded identifier bytes. Roughly half of
   anchor-derived DIDs do not decompress to an Edwards point (inventory §2.3), so a DID that
   `Did::identifier_bytes` resolves may be one `SenderPrincipal` rejects. The two principal
   notions are therefore not interchangeable, and N2-A must not assume replay-guard coverage
   extends to anchor-derived principals.

Row #2 (`icn-net` outgoing sequence tracker) is **unchanged**: its key is still built from
`sender.as_str()` / `recipient.as_str()` (`icn/crates/icn-net/src/sequence_tracker.rs`).

No other merged PR since `83682563` touched an N2-A migration surface.

---

## 2. The collision scanner (evidence tool)

### 2.1 What it is

`icn/crates/icn-store/src/did_collision_scan.rs` — a reusable, read-only scan engine, plus a
runner binary `icn/crates/icn-store/src/bin/did-collision-scan.rs`.

It is not a one-off script and it is not a migration. It answers one question per keyspace:
*do two stored rows name one principal, and what happens when they merge?*

### 2.2 Design properties, and why each is load-bearing

| Property | Mechanism | Why it matters |
|---|---|---|
| Decode-faithful | Groups by `icn_identity::identifier_bytes_of_spelling`, the function `Did::identifier_bytes` itself delegates to | A scan that grouped by a *reimplementation* of the decode would prove nothing about the equality it gates |
| Read-only | Copies the store directory to scratch and opens the **copy**; the source is never opened | `sled::open` takes an exclusive lock and runs recovery *writes* on an unclean directory. A tool that opened a live store directly would violate its own guarantee |
| Payload-free | Values are reduced to their length at the scan boundary; principals appear as an 8-hex-character fingerprint | The report is an operational artifact that will be pasted into issues |
| Layout-independent | DID spellings are located by scanning for the `did:icn:` scheme, not by parsing each keyspace's separators | A keyspace that changes its separator cannot silently fall out of scan coverage |
| Falsifiable | Always reports total store rows, namespace counts, and per-tree row counts alongside the per-keyspace zeros | "0 rows" from a broken scanner and "0 rows" from an empty store are otherwise indistinguishable |
| Gate-shaped | Exit status `0` only when every principal-bearing row is accounted for **and** every keyspace accounting for one is automatable | Makes the tool usable as the migration gate, not merely as a report. The verdict lives in `CoverageAudit::is_clear`, and the runner renders it rather than recomputing it, so exit status and report cannot disagree |

### 2.3 The three-state accounting — and why there is no fourth

Every principal-bearing row in a scanned store is in exactly one of three
states, and the verdict follows from that partition:

1. **Covered** — a registered keyspace interpreted it, so the collision result
   speaks for it.
2. **Deferred** — a *named gate* owns it: `n2a_deferred_namespaces` records the
   governance vote keyspace (§7.5), the auth-challenge namespace and the
   security namespace (dedicated workflow). Deferred is neither dispositioned
   nor cleared; it is a reviewed exclusion, reported separately and never folded
   into the scanned counts. Since 2026-09-02 a deferred namespace's rows are
   still **grouped by principal** — deferral names who owns the merge rule, and
   looking away from the data would let a collision reach the loader
   unexamined — and each deferral carries a `DeferredCollisionPosture` saying
   what a starting binary does about a collision there (§10.2).
3. **Uncovered** — nothing accounted for it, which **blocks**.

State 3 is the point. A keyspace added after this tool was written, or simply
left out of the registry, is indistinguishable from a clean store unless
unaccounted rows block — and that has already happened once: §5 rows #71 and #36
were live and unregistered (§3.4). Without the deferral registry the only safe
verdict on any uncovered row would be "blocked", which would make the gate
unusable; with it, an accidental omission still blocks while a reviewed
exclusion does not.

A deferred namespace is **never dispositioned**: its report carries
`FailClosed` / `AwaitingDomainSignOff` by construction, so nothing downstream
can read a merge rule out of it. Its key shapes are grouped exactly as a
registered keyspace's are; its values are never read.

### 2.4 A plausible merge rule is not an authorized one

The disposition table below distinguishes rules that are *established* from
rules this document merely *asserts*. That distinction is now encoded, not just
written down: `KeyspaceDescriptor::basis` carries `RuleBasis::Established` or
`RuleBasis::AwaitingDomainSignOff`, and a collision under an unsigned-off rule is
**not automatable** regardless of its disposition.

This closes a real contradiction found in review. §4 row 5 said summing ledger
balances still needed economics sign-off, while the code marked that keyspace
`Sum` — so a generic storage crate would have authorized a merge of monetary
state on nothing but its own say-so. Seven keyspaces are currently
`AwaitingDomainSignOff`: `icn-ledger/{balance,cleared_volume,frozen}`,
`icn-net/outgoing_seq`, `icn-trust/edges`, `trust-app/sequences_{issuer,receiver}`. A test
pins that list against this document so the two cannot drift. `sequences_receiver` moved into
this set on 2026-09-03: its max rule had been marked established "by precedent", but precedent is
not implementation — the receiver tracker reads and writes the exact spelling and folds nothing,
so the gate now refuses a collision there rather than clearing it on a rule no loader performs.

The basis only bites when there is something to merge; a keyspace with no
collisions stays automatable whatever its basis.

### 2.5 What a CLEAR verdict is conditional on

A recursive file copy of a **live** store is not a point-in-time snapshot. Writes
can land between one file being copied and the next, so sled may recover the copy
successfully while omitting a row that existed in the source — including an
aliasing row. A CLEAR verdict therefore describes a state the source may never
have held at any single instant.

This is a limit of the evidence, not a defect to code around: quiescing a store
means stopping a workload, which this tranche must not do. It is printed with
every report and carried in the JSON. For a verdict that is binding rather than
indicative, scan a quiesced store or a coherent volume snapshot with writes held
until the cutover — and note that this is a further reason the fail-closed check
belongs *inside* the key-equality binary (§3.5).

### 2.6 Fail-closed reads

Two paths could previously turn missing evidence into apparent absence, and both
now propagate:

* **Tree iteration.** `tree_row_counts` and `did_bearing_rows_per_tree`
  propagate a sled iterator error instead of discarding the unreadable item. The
  case that matters: if the unreadable key were the only principal-bearing row
  in a named tree, discarding it would drop the count to zero and pass a store
  that was never finished. Corruption and an incomplete copy are evidence, not
  absence.
* **Spelling extraction.** The tokenizer takes the longest candidate run, then
  shortens it from the right until it decodes to a 32-byte identifier. The
  alphabet must include `+` and `/` — the production parser accepts all 23
  `multibase::Base` encodings and `Base64` spellings contain both — but `/` is
  also a live key separator (`trust/edges/<did>`), so the alphabet alone cannot
  say where a spelling ends. Deciding by decoding rather than by alphabet
  captures a base64 spelling whole *and* still terminates `<did>/suffix` at the
  spelling. Nothing that fails to decode is silently dropped: the whole run is
  reported as one unreadable token, which itself blocks.

### 2.7 Grouping rule

Rows are grouped by their **principal-canonical shape**: the raw key with every embedded
`did:icn:` spelling replaced by the 32 identifier bytes it decodes to. Non-DID key material stays
in the shape, so:

* `ledger:cleared_volume:<did>:USD` and `…:EUR` stay apart, while two spellings of that account
  under `USD` come together;
* tuple keys such as `outgoing_seq:<sender>||<recipient>` collide only when **both** ends resolve
  to the same pair, and the per-position representation counts say which end was re-spelled.

Rows within a group are reported in `Store::scan` order — lexicographic by key bytes — because
that order decides the survivor of every last-writer rebuild. `Base256Emoji` spellings are
non-ASCII and therefore sort after every ASCII spelling, so **the survivor is
attacker-selectable**. The scan surfaces the survivor explicitly rather than leaving it implicit.

### 2.8 Coverage limit found while building it

`Store::scan` reads only sled's **default tree**. `icn-gateway`'s service discovery uses a
*named* tree, which a `Store`-trait scan can never reach. A scan reporting zeros on such a store
would be a false negative.

The runner therefore also reports per-tree row counts and per-tree DID-bearing row counts
(`SledStore::tree_row_counts`, `SledStore::did_bearing_rows_per_tree`), and **treats
principal-keyed rows in a named tree as blocking**, not as a clean result.

### 2.9 Fixture tests

17 tests in `did_collision_scan::tests`, run against a real `SledStore` rather than a hand-rolled
double — the ordering claim is a claim about the actual backend, and a simulated store would only
restate the test's own sort. They cover: no-collision data; two representations of one principal;
several representations; malformed/unreadable keys; a group with a known merge rule; a group with
no authorized merge rule; residual key fields; tuple keys; scan-order survivor; the non-ASCII
survivor case; read-only-ness; payload absence; and registry scope.

**Discrimination evidence.** Mutating the grouping to today's spelling-keyed behaviour (group by
raw key instead of canonical shape) fails **9 of 15** collision tests and leaves green exactly the
6 that should not depend on grouping. The suite therefore discriminates rather than passing
vacuously.

### 2.10 How to run it

```bash
cd icn && cargo build -p icn-store --bin did-collision-scan
./target/debug/did-collision-scan <sled-db-path> [<sled-db-path> ...] [--json]
```

Each path must be a **sled database root**, not a directory above one. A
deployment keeps one database per domain, so `/data` is the wrong level and
`/data/store/ledger` is the right one. The tool refuses a wrong-level path and
names the databases beneath it, because `sled::open` on a non-database directory
*creates* one — which would otherwise mean zero rows and a false CLEAR.

Against a Kubernetes deployment, extract the volume first — the tool must be given a directory it
can copy:

```bash
kubectl cp <namespace>/<pod>:/data ./deployment-data
# one database per domain — scan each, not the parent
find ./deployment-data -name conf -printf '%h\n' | xargs ./target/debug/did-collision-scan
```

Exit status: `0` clear, `1` at least one keyspace or blocking deferred namespace must fail
closed, `2` tool error. The verdict is `SledStoreAudit::is_clear` — the same computation the
startup gate enforces (§10), so what the offline tool reports is what the binary will do.

---

## 3. Scan coverage actually achieved (evidence)

**This section records only what was measured.**

### 3.1 Deployments scanned

Extracted read-only with `kubectl cp` from the running pod and scanned from the copy; the live
volumes were never opened. Extraction date 2026-08-29.

| Deployment | Extracted | sled DBs scanned | Result |
|---|---|---|---|
| `icn-coop-beta` | yes | 32 | scanned |
| `icn-coop-gamma` | yes | 31 | scanned |
| `icn-coop-delta` | yes | 31 | scanned |
| `icn-coop-alpha` | **no** | — | container `CrashLoopBackOff`, not attachable |
| `icn` / `icn-daemon` | **no** | — | container `CrashLoopBackOff`, not attachable |

94 sled databases scanned in total. Workloads were **not** restarted to obtain evidence.

A store topology finding worth recording: a deployment does not keep one shared store. It keeps
**one sled database per domain** under `/data/store/` (`ledger`, `network`, `trust`, `governance`,
`security`, `cooperative`, `gossip`, …) plus `gateway_store`, `commons.sled`, `federation_store`,
`identity_store`, and `state.snapshot*` files. Encrypted keystore material (`identity.age*`) is
present and was **not** read.

### 3.2 Aggregate result

| Measure | Value |
|---|---|
| Rows matched by registered N2-A keyspaces | **24** |
| **Collision groups** | **0** |
| Rows participating in collisions | 0 |
| Unreadable / malformed DID keys | 0 |
| Named-tree coverage gaps | 0 |
| Keyspaces requiring manual disposition | 0 |

| Keyspace | Rows | Collisions |
|---|---|---|
| `icn-trust/edges` | 12 | 0 |
| `icn-net/outgoing_seq` | 4 | 0 |
| `icn-coop/member` | 3 | 0 |
| `icn-net/replay_max_seq` | 2 | 0 |
| `icn-net/replay_sender_regime` | 2 | 0 |
| `trust-app/sequences_issuer` | 1 | 0 |

Every other registered keyspace matched zero rows, and those are **real absences**: each store's
total row count and per-tree count were reported alongside, and the raw sled files of the empty
local stores were independently checked for `did:icn:` byte occurrences (zero) to distinguish an
empty store from a failed read.

**Registry scope at the time of this scan.** The registry then held twelve keyspaces.
`federation/attestations/` (§4 row 19, #2703), `idx_agreement_party/` (§4 row 20, #2707) and the
primary `ledger:treasury:<did>` row (§4 row 21, #2627 M1) were registered afterwards and are not
represented in these figures; any row of any of them present then would have counted under
*uncovered* (§2.3), and none did. See the row-19, row-20 and row-21 notes in §4 for what that does
and does not show. A registry expansion does not retroactively enlarge this
evidence: the historical scan scope is what was registered then, and the current registry scope
is what is registered now.

### 3.3 Principal-bearing rows behind a named gate — 63

A per-keyspace zero only speaks for the rows that keyspace matched. Reconciling *all* DID-bearing
rows against the registry found 63 the registry does not cover. Each is now accounted for by a
**named gate** (§2.3 state 2) rather than left as an unexplained remainder:

| Family | Rows | Why out of scope |
|---|---|---|
| `gov:vote:<uuid>:<did>` | 32 | inventory row #23 — **§7.5 gate**, not N2-A |
| `security:reputation:<did>` | 10 | security namespace — deferred to the dedicated security workflow |
| `security:violation:<did>` | 10 | as above |
| `security:banned:<did>` | 7 | as above |
| `security:quarantine:<did>` | 4 | as above |

The `icn-rpc` auth-challenge namespace (`auth:challenge:<did>`, inventory row
#29) is also a registered deferral. No live challenge existed in the scanned
deployments — they are TTL-bounded — but without the entry an ordinary challenge
row would have been classified uncovered and blocked the gate.

These 63 rows are **deferred, not cleared**. Their existence and their migration dependency are
recorded; their contents were not inspected.

Re-running the scan after the deferral registry landed reconciles exactly: **24 covered + 63
deferred = 87** principal-bearing rows, **0 uncovered**, 0 blocked, across all 94 databases. The
three-state partition holds on real data, which is the property the verdict depends on.

### 3.4 A registry scope error the scan caught

The first registry was built from the inventory's §12 *Concrete list* — the `NEEDS MIGRATION` rows
only. Scanning real data found live rows in two keyspaces classified **`SILENT-MERGE RISK`** in §5
and therefore absent from that list:

* `trust/sequences/issuer/<did>` and `.../receiver/<did>` — inventory row **#71**;
* `member:<coop>:<did>` — inventory row **#36**.

`SILENT-MERGE RISK` is precisely the class that merges without announcing itself, so scoping the
scanner to `NEEDS MIGRATION` alone was wrong. The registry now covers both classes, and the three
keyspaces are included in the §3.2 figures above.

This is the concrete value of reporting uncovered principal-bearing rows by masked key shape
rather than reporting per-keyspace zeros: the gap was found from data, not from re-reading source.

### 3.5 What this evidence does and does not establish

**Does:** no aliased principal rows exist today in any registered N2-A keyspace of the three
scanned deployments, and no malformed or unreachable principal rows exist there either.

**Does not:**

1. **Two deployments are unscanned** (`alpha`, `icn-daemon`).
2. **The sample is small.** 24 principal-bearing rows across a handful of principals is a pilot
   cluster, not a populated production deployment. Absence of collisions here is weak evidence
   about a large store.
3. **The result is point-in-time.** Aliasing is attacker-chosen (§2.2) and `from` is unsigned, so a
   peer can write an alternate-spelled row at any time. A clean scan today does not imply a clean
   store at migration time.

Consequence for the migration design: the scan must be re-run **immediately before** the cutover, and
the fail-closed check belongs *in the binary* — a key-equality build should refuse to start against
a store whose rows alias under an unruled keyspace, rather than trusting a scan run earlier.
**Implemented** as the startup gate (§10) on 2026-09-02, and enforced again for the three
`icn-ledger` keyspaces inside `Ledger::new` (§4.1, §10.6); the point-in-time limits above still
apply to any evidence gathered *outside* the binary.

## 4. Keyspace dispositions (decisions)

Merge rules are stated per keyspace and encoded in the scanner registry
(`did_collision_scan::n2a_keyspaces`), so a scan reports the rule alongside the collision. No rule
was invented to unblock the tranche: where two rows may encode contradictory state and no domain
rule authorizes choosing or combining them, the disposition is **fail closed**.

| # | Keyspace | Key encoding | Collisions observed | Merge rule | Rule already established by domain semantics? | Lossless? | Alias/dual-read window? | Order | Rollback | Tranche | Status |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `icn-net` `replay_max_seq:` | `Display` | **0 in 3 scanned** | max floor | **yes** — implemented in `replay_guard` (#2644) | yes | no | 4 | safe (no byte moves) | N2-A | rule live in-tree |
| 2 | `icn-net` `replay_finalized:` | `Display` | **0 in 3 scanned** | union | yes (#2644) | yes | no | 4 | safe | N2-A | rule live in-tree |
| 3 | `icn-net` `replay_sender_regime:` | `Display` | **0 in 3 scanned** | **fail closed** | yes — loader already declines to collapse | n/a | no | 4 | safe | N2-A | fail-closed by design |
| 4 | `icn-net` `outgoing_seq:` | `as_str` ×2 | **0 in 3 scanned** | max | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs review** |
| 5 | `icn-ledger` `ledger:balance:` | JSON-quoted | **0 in 3 scanned** | sum | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs economics sign-off** |
| 6 | `icn-ledger` `ledger:cleared_volume:` | `Display` + currency | **0 in 3 scanned** | sum | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs economics sign-off** |
| 7 | `icn-ledger` `ledger:frozen:` | `Display` | **0 in 3 scanned** | union | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs review** |
| 8 | `icn-trust` `trust/edges/` | `as_str` ×2 | **0 in 3 scanned** | union | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs review** |
| 9 | `icn-ledger` `ledger:journal:` | content hash | **0 in 3 scanned** | equivalent | yes — key carries no spelling | yes | no | — | safe | N2-A | scanned to confirm |
| 10 | `icn-security` misbehavior | `Display` | **not inspected** | — | — | — | — | — | — | **security workflow** | deferred; migration dependency preserved |
| 11 | `icn-rpc` auth challenges | `Display` | **not inspected** | — | — | — | — | — | — | **security workflow** | deferred; TTL-bounded |
| 12 | `apps/governance` votes (#23) | `Display` | **0 in 3 scanned** | §7.5 re-key | n/a | n/a | **required** | after N2-A | n/a | **§7.5 gate** | not N2-A |
| 13 | `icn-commons` weak-holder id (#65) | SHA-256 of spelling | I7 *creates* the split | — | no | **no** | — | before 6 | n/a | N2-A | **namespace decision — see §5**; the *mint* is guarded before persistence and the derivation is unchanged (§11.7, #2627 M3) — existing derived ids are not migrated |
| 14 | `VectorClock` (#45), snapshot `vector_clock` (#54) | serialized map | **0 in 3 scanned** | max | yes — `VectorClockProjection::from_entries` | yes | no | 4 | safe | N2-A | rule established |
| 15 | snapshot `peer_connections` (#57) | serialized map | **0 in 3 scanned** | **fail closed** | no | no | — | 4 | safe | N2-A | **no authorized rule** |
| 16 | `trust-app` `trust/sequences/receiver/` (#71) | `Display` | 0 in 3 scanned | max | **no** — asserted by precedent; `SequenceTracker` reads and writes the exact spelling and implements no fold | yes | no | 4 | safe | N2-A | **rule needs a trust-domain loader that folds; fail closed at the gate until then** |
| 17 | `trust-app` `trust/sequences/issuer/` (#71) | `Display` | 0 in 3 scanned | max | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs trust-domain confirmation** |
| 18 | `icn-coop` `member:` (#36) | `Display` | 0 in 3 scanned | **fail closed** | no | n/a | — | — | safe | N2-A / §7.5 boundary | **institutional decision required** |
| 19 | `icn-federation` `federation/attestations/` (#27, #59) | `as_str` + `/` + source coop | **not measured** — outside the registry when §3 was scanned (#2703) | **fail closed** | yes — the live store refuses to read, write or sweep over such a pair, and revokes one atomically (#2704); the merge rule itself is undecided | n/a | no | — | safe (no byte moves) | N2-A | fail-closed in code **and at the gate**; **merge rule awaits a federation-domain decision** |
| 20 | `icn-federation` `idx_agreement_party/` (#28) | `as_str` + `/` + agreement id | **not measured** — outside the registry when §3 was scanned (#2707) | **equivalent** (projection) | **yes** — the rows are derived from `federation/agreements/` and the store proves membership from the canonical row, retires superseded rows, refuses what it cannot attribute and can rebuild the projection (§11.3) | yes | no | — | safe (no byte moves) | N2-A | registered `Equivalent`/`Established` in code **and at the gate**; **projection, not authority** |
| 21 | `icn-ledger` `ledger:treasury:<did>` (#10, #41) | `Display` | **not measured** — outside the registry when §3 was scanned (#2627 M1) | **fail closed** | yes — the loader classifies every primary row and refuses an alias pair, an unreadable key or value, a key/body spelling disagreement and a disagreeing cooperative index before adopting anything (§4.2); the merge rule itself is undecided | n/a | no | — | safe (no byte moves) | N2-A | fail-closed in code **and at the gate**; **no merge rule authorized** |
| 22 | `icn-commons` `commons/holders/by_did/` (#67) | `Display` | **not measured** — outside the registry when §3 was scanned (#2627 M3) | **fail closed** | yes — two spellings reach two independent `CommonsHolderRecord`s with their own status, personhood level and rights, so a merge decides a member's standing; the live mint seam refuses the same state before it can be created (§11.7) | n/a | no | — | safe (no byte moves) | N2-A | fail-closed in code **and at the gate**; **no merge rule authorized**, existing duplicates not dispositioned |
| — | `CompressedVectorClock` (#46) | dormant | n/a | derive-shape fix | n/a | yes | no | 3 | safe | N2-A | no data step |

Rows 10 and 11 are security-specific namespaces. Their **existence and migration dependency are
preserved here**; their contents were not inspected and their disposition belongs to the dedicated
security workflow, not to this tranche.

Row 18 (`icn-coop` cooperative membership) is new to this table. Merging two membership rows
decides **who is a member of an institution**, which no identity-layer rule authorizes, and it sits
next to the §7.5 membership gate without the inventory having placed it there. It is therefore
**fail closed** pending an explicit governance-domain decision about which side of the §7.5
boundary it falls on. N2-A must not resolve that by default.

Row 19 (`icn-federation` federated trust attestations, #2703) was outside the registry when the
§3 evidence was gathered, so the three-deployment result says nothing about it. Its key is
`federation/attestations/<member-did spelling>/<source_coop_id>` and its collision unit is
**(member principal, source cooperative)**, because the source stays in the canonical shape: rows
from different cooperatives about one principal are the federation's ordinary union and never a
group, while two rows from one cooperative about one principal can only differ by disagreeing. No
federation-domain rule authorizes choosing or combining such a pair, and this document authorizes
none: the disposition is **fail closed**. Since #2704 the store itself enforces that posture — it
reads the whole namespace by `Did` equality rather than by spelling prefix, refuses any operation
that would interpret or mutate an ambiguous pair (a lookup for that principal, a listing for that
source, a write to that pair, the expiry sweep), surfaces an unreadable or key/value-inconsistent
row as a typed error instead of skipping it, and revokes every alias row a removal names in one
atomic deletion, so an interrupted revocation cannot leave a lone alias row that the next read
would accept. It re-keys, merges and normalizes nothing; persisted bytes are unchanged.

The startup gate (#2700, merged) consumes the same `n2a_keyspaces()` registry, so a federation
alias pair refuses the node start exactly as any `FailClosed` keyspace does; the gate fixtures
proving it are in `icn/crates/icn-store/tests/n2a_startup_gate.rs`, with a one-fact-different
control in which the two spellings carry different source cooperatives and the start is clear.
The descriptor states the key structure rather than letting the scan guess it: the member spelling
is anchored immediately after the prefix and ends at the `/`, and everything after that `/` is an
opaque discriminator compared byte-for-byte. The source is a federation-domain identifier this
registry does not own, nothing forbids one that contains `did:icn:`, and the store compares source
ids as exact strings — so a scan that canonicalized inside the source would group rows the store
holds apart and call rows unreadable that the store reads without difficulty. A populated
attestation row in a store scanned
*before* this registration could only have surfaced as an **uncovered** shape (§2.3) — blocking,
but unclassified. §3.3 reported zero uncovered rows across the three scanned deployments, which is
consistent with those stores holding no attestations at the time and is not a measurement of this
keyspace. The first scan that includes it yields new evidence, not a regression.

Row 20 (`icn-federation` agreement party index, inventory #28, #2707) is the first keyspace
dispositioned as a **projection** rather than as authoritative state, and the distinction is the
point of §11. Its key is `idx_agreement_party/<party-did spelling>/<agreement id>` and its value
repeats the agreement id; every row is written from the canonical `Agreement.parties` vector and
from nothing else, so two spellings of one party for one agreement are two derivations of one
canonical fact. `Equivalent` is the existing vocabulary for exactly that ("the rows are equivalent
by construction; keeping any one loses nothing") and no new disposition was invented for it. The
rule is `Established` because the live store now enforces it: a party lookup reads the whole
projection, keeps the rows whose spelling names the queried principal, and returns only agreements
whose canonical `parties` contain that principal under `Did` equality — so an index row can never
create, omit, preserve or alter membership on its own. What the scanner sees under this prefix is
a redundancy, never a contradiction, which is why a collision here is automatable.

The descriptor is the attestation layout's shape under the opposite disposition: the party spelling
is anchored immediately after the prefix and ends at the `/`, and the agreement id after it is an
opaque discriminator compared byte-for-byte — an identifier the agreement's creator chose, which
the registry does not own and the scan never parses, so an id that contains or is a `did:icn:`
spelling is still just an id, and no agreement id is normalized. The collision unit is therefore
**(party principal, exact agreement-id bytes)**: one party in two agreements is two shapes and
never a group; only two spellings of one party for one agreement group, as an `Equivalent`,
automatable pair. The startup gate (§10) consumes the same descriptor, so such a pair is recorded
and the start is clear, an unreadable party spelling refuses, and no row is moved, rewritten,
normalized or deleted; the fixtures are in `icn/crates/icn-store/tests/n2a_startup_gate.rs`. A
CLEAR there means this spelling collision is safe under the registered projection disposition. It
does not mean the projection is complete, current or authoritative — §11.2.

Row 21 (`icn-ledger` treasury record, inventory #10 and #41, #2627 M1) is the first P4 fold
(§11.1) closed after §11.4 named it the highest-severity open boundary. Its key is
`ledger:treasury:<treasury-did spelling>` with nothing after the spelling, and its collision unit
is the treasury principal alone. Two rows for one principal are two treasury records that can
disagree about every field; no economics rule authorizes choosing, summing or combining them, and
this document authorizes none: the disposition is **fail closed**, `Established` because the
loader implements it. The descriptor's prefix runs through the DID scheme
(`ledger:treasury:did:icn:`) so that it claims exactly the primary rows and none of the budget,
rule, audit, index or velocity-limit siblings that share the lexical parent — two of which embed
the spelling as key structure and remain uncovered until dispositioned on their own. §4.2 records
the loader, the key/body rule, the cooperative-index rule and the gate agreement.

Rows 14–15 hold DIDs inside serialized *values*, not keys, so they are not prefix-scannable and
are not covered by the scanner registry. Their merge rule must be chosen before decode collapses
them (§12.1 item 4-ii).

Every "unmeasured" cell is a direct consequence of §3.1 and is the substance of the open gate.

### 4.1 The ledger loaders now enforce their own dispositions (#2627)

Rows 5–7 name three keyspaces whose disposition is "fail closed until the economics owner signs
off". Until this change that disposition existed in the scanner registry, in this table and —
since #2700 — in the startup gate (§10), which refuses to start `icnd` over a collision there
before any store is opened. It bound nothing at all inside `icn-ledger`: a `Ledger::new` reached
with no gate in front of it — every embedder and test that is not `icnd` — would have collapsed a
collision silently, whatever the registry said; and the gate never reads a value, so a row it
cannot see as a problem was adopted unexamined. `icn_ledger::principal_rows` closes that gap: the
three rebuilds classify their whole keyspace before adopting a row and refuse with a typed
`PrincipalRowsRefusal` rather than electing a survivor. §10.6 states how the two layers divide
the work.

The audit that produced it found the collapse is **worse than last-writer-wins**, in a way worth
recording because it generalises to every `HashMap<Did, _>` rebuilt from spelling-keyed rows:

* `HashMap::insert` replaces the **value** but retains the **first-inserted key**. Rows arrive in
  `Store::scan` order — lexicographic by key bytes — so the surviving value comes from the
  byte-greatest row while the surviving key spelling comes from the byte-least one. The two halves
  of the in-memory entry come from **different rows**.
* `save_cached_balances` then re-derives the key from that map key and writes the *other* row's
  balances under it, leaving the byte-greatest row on disk holding stale state. The next start
  reads both again. The divergence is permanent, silent, and repeats every boot.
* `save_*` is also reached from `recompute_balances`, whose maps are keyed by the spelling the
  **journal entries** carry rather than the spelling the stored row uses — so a write-back could
  open a second row for an account that already had one. Both rebuild paths now adopt the stored
  spelling (`HashMap::get_key_value`) instead of re-deriving one.
* `FreezeManager::remove_frozen` deleted only the caller's spelling, so an unfreeze naming a second
  spelling left the freeze row live and the member came back frozen on the next start — a
  revocation that does not revoke. Removal now goes through `HashMap::remove_entry` so it deletes
  the row the map actually held.

Refusal is scoped as narrowly as the semantics allow: two currencies of one account are two rows of
state and not a collision, and an **expired** freeze row is not live state, so a lapsed alias does
not block a start.

A fourth refusal has no row in the table above because nothing writes it deliberately: a
`ledger:balance:` row whose key spells the account differently from the `account_id` inside the row.
That is the residue of a collapse that already happened, and loading it would adopt one row's money
under another row's name.

This discharges the `icn-ledger` half of blocker 3 (§9). It authorizes **no** merge rule: rows 5–7
stay `AwaitingDomainSignOff`, and a test that asserted a sum or a union would be asserting an
economic decision no domain owner has made.

### 4.2 The treasury loader classifies before it adopts (#2627 M1)

§11.4 classified `ledger:treasury:<did>` as a P4 fold over P1 rows and the highest-severity open
boundary: `TreasuryManager::load_from_store` folded every row into `HashMap<Did, Treasury>` plus
the `Did`-keyed coop, entity, budget and rule indexes, and its two fail-closed hydration guards
compared `Did`s — principal equality since I7 — so two spellings of one treasury were not an
inconsistency to them. The defect was reproduced on unchanged `main` before it was fixed, with a
real sled store holding `ledger:treasury:<A>` and `ledger:treasury:<B>` for one principal, each
row individually valid and naming the same cooperative:

* hydration **succeeded**, and one semantic entry survived;
* the surviving *value* was always the scan-last row — `z…` over `f…`, `f…` over `F…` — so
  `Store::scan` order, which whoever writes the second row chooses, elected it;
* the coop index collapsed to one entry naming that survivor, and both spellings answered
  `is_treasury_account`;
* a later write-back through the public `populate_entity_id_at_creation` seam rewrote only the
  survivor's row; the loser stayed on disk, stale, to be read again on the next start.

**Mechanism.** The loader now reads everything before it adopts anything:

```text
physical rows beneath ledger:treasury:
→ classify every key by shape: primary / sibling subspace / unreadable
→ read a value only behind a key that names a treasury principal
→ prove one spelling per principal            (icn_ledger::principal_rows, unchanged)
→ prove key spelling == body spelling, as bytes
→ prove one treasury per coop_id and per entity_id
→ check every ledger:treasury:idx:coop: row against the classified primaries
→ only then adopt into the Did-keyed maps
```

Every refusal is typed and payload-free — `PrincipalRowsRefusal::{AliasCollision, UnreadableKey,
KeyValueSpellingMismatch}` for what the treasury shares with the three §4.1 keyspaces, and a
treasury-local `TreasuryHydrationRefusal::{UnreadablePrimaryValue, DuplicateCoopId,
DuplicateEntityId, CoopIndexUnreadable, CoopIndexSpellingMismatch}` for what it does not — and
every refusal is raised before the first map mutation, so a failure on row *N* never leaves the
maps hydrated from rows 1..*N*−1 (pinned in-module against the private maps; every scan, the
sibling subspaces included, completes before adoption begins). `principal_rows` itself gained
one constant, `TREASURY_KEYSPACE`; its guard and its three existing callers are unchanged.

**Key/body identity is physical, not semantic.** `persist_treasury` derives the key from
`treasury.treasury_did`'s `Display`, so the row the writer claims to have written is the one whose
key bytes equal its body's spelling bytes. The comparison is `as_str() != key_spelling`, never
`Did` equality — under I7 `key A, body B` for one principal compares equal, and accepting it would
adopt a row every later `persist_treasury` addresses under the *other* spelling, opening a second
row while the first stayed on disk. A single such row refuses; its control (key `f`, body `f`)
loads, because a non-base58 spelling is a legal stored spelling and nothing canonicalizes.

**Unreadable primary rows refuse; they do not vanish.** A key beneath the parent that is neither a
registered sibling prefix nor a `Did::from_str`-valid spelling is `UnreadableKey`; a primary row
whose value does not deserialize is `UnreadablePrimaryValue`; both are reported before the alias
classification, because a classification over an incomplete view proves nothing. One consequence
deserves its own sentence: an anchor-derived cooperative treasury whose 32 bytes are no Ed25519
point — about half of them, inventory §10.1 — fails `Did::from_str` at the key and `Deserialize`
at the value. Before M1 the loader skipped such a row with `if let Ok(..)` and the treasury
silently dropped out of the maps on every reload; after M1 hydration **refuses** it, and a node
holding such a row does not start. M1 makes that state visible; it does not repair it, and the
read-path fix stays with #2628 (N2-B).

**Sibling subspaces are classified by key shape, never by whether a value parses.** The lexical
parent is shared with `budget:`, `rule:`, `audit:`, `idx:coop:`, `idx:budgets:` and `vlimit:`
rows. The last was missing from the loader's old skip list and survived only because a velocity
limit does not deserialize as a `Treasury`; it is now named, and a `vlimit:` row whose value
*does* parse as a treasury record is still a sibling. A key beneath the parent that begins with
none of the six is a primary row and must name a principal, so an unnamed subspace refuses
hydration rather than being tolerated by accident. The sibling loaders themselves are unchanged
and still skip a row that does not parse — pre-existing permissiveness, recorded here as
follow-up and not absorbed.

**`ledger:treasury:idx:coop:<coop_id>` is a write-only projection, and M1 gives it no
authority.** `persist_coop_index` writes it at registration; hydration rebuilds the coop map from
the primary rows and nothing reads the index. It is still persisted evidence that can preserve a
representation disagreement, so before adoption every index row must decode as a treasury
principal spelling (else `CoopIndexUnreadable`) and must agree byte-for-byte with the physical key
spelling of the primary row it names — whether it names it through the coop id it is filed under
or through the principal its value decodes to (else `CoopIndexSpellingMismatch`). The fixture the
reassessment required — primary `A`, index → `B`, one principal — refuses; its control (index →
`A`) loads; an index pointing at a different registered treasury refuses by the same byte
comparison; an orphan index naming no known coop and no known principal is tolerated, adopted
from nowhere, and grants nothing. The order in which the rows were written changes no outcome.

**The write path cannot open an alias pair.** Reproduced before concluding: with one treasury
hydrated, `register_treasury` and `register_treasury_with_entity` under the other spelling — with
the same coop id or a new one — are refused by the existing `contains_key` check, which is
principal equality, and write no row; `populate_treasury_entity_id_for_did` locates the row by
principal and persists under the record's own spelling, so addressing it by the alias rewrites
exactly the stored row. No writer-side guard was needed and none was added.

**Startup gate.** The primary row is registered as `icn-ledger/treasury` (§4 row 21):
`FailClosed`, `Established` — the disposition the loader implements — `PrincipalRegion::WholeKey`
with `did_ends_key`, and prefix `ledger:treasury:did:icn:`. The prefix runs through the DID scheme
deliberately: it matches every key `persist_treasury` writes and no key beneath any sibling
subspace, so the descriptor claims exactly the primary rows and says nothing about the two
siblings that embed a spelling as key structure. Those — `audit:<did>:<ts>:<id>` (inventory #70)
and `idx:budgets:<did>:<budget>` — keep the status they had before M1: **uncovered** to the gate,
which refuses a store that holds them until each is registered under its own argued disposition.
That is the honest boundary, not a regression, and it is the next treasury follow-up: a store with
a registered treasury and its cooperative index is clear at the gate; a store in which a budget
has been created — `create_budget` writes `idx:budgets:<did>:<budget>` beside the principal-free
`budget:<id>` row, and it is the index row that is uncovered — or an audit record has been
recorded is not, exactly as before. A bare `budget:`, `rule:`, `idx:coop:` or `vlimit:` row
carries no principal in its key and blocks nothing. A DID-looking `coop_id` in an `idx:coop:`
key is likewise uncovered and never a treasury spelling. Registry pins hold the descriptor to this shape from both
sides — in `icn-store` against literal sibling prefixes, in `icn-ledger` against the ledger's own
prefix constants — so the two cannot drift apart silently.

**Division of labour**, extending §10.6:

```text
startup gate        — can the node safely open this persisted namespace?
                      keys only; alias pair → refuse; unreadable spelling → refuse;
                      sibling rows outside the descriptor; never a survivor
TreasuryManager     — can this loader safely adopt these exact rows?
                      keys and values; the same alias unit; key/body bytes; institutional
                      duplicates; refuses before any map mutation; never a survivor
idx:coop validation — does the persisted projection agree with primary physical evidence?
                      value must decode and equal the primary row's key bytes; no authority
```

Where the two layers differ they differ in the direction §10.6 already allows — the loader is the
stricter: a spelling that decodes but is no Ed25519 point is readable to the gate and
`UnreadableKey` to the loader; a key beneath the parent with no `did:icn:` in it at all is a row
without a principal to the gate and `UnreadableKey` to the loader. Neither layer asserts a merge
rule and neither normalizes a spelling, so they cannot disagree about a survivor.

**Reach.** `TreasuryManager::with_store` is constructed in production by `apps/ledger-app` init
(inside `icnd`, behind the gate) and by the `icnctl treasury` maintenance commands
(`treasury_entity_backfill_report` and its apply path), which open the ledger store with no gate
in front of them. Both reach the guarded loader; `icnctl`'s gate policy is unchanged and is a
separate maintainer decision.

**Non-goals.** No merge rule — `Sum`, `Union`, `Max`, `Latest`, canonical-wins and every
equivalent stay unauthorized, and `RuleBasis::Established` here records only that fail-closed is
implemented; no re-key, no normalization, no deletion, no repair of observed rows; no change to
the audit, budget, rule, labor-share, bond or allocation loaders beyond naming their prefixes; no
`icnctl` gate policy change; nothing of M2–M4 or §7.5.

**Mutation evidence** — each mutation applied alone to the committed tree, the focused suites
run, and both files restored byte-exactly (sha256-checked); every other test in the run stayed
green. Observed, not hypothetical:

| Mutation | Applied | Observed failures |
|---|---|---|
| A — bypass the principal-row alias guard | the `refuse_unless_one_spelling_per_principal` call removed | 5 loader fixtures (the alias pair, the pair under two coop ids, three spellings, every insertion/scan order, scanner-and-loader agreement) and the in-module no-partial-adoption pin; 27 + 69 others pass |
| B — compare key and body as `Did` (principal equality) | `Did::from_str(key).is_ok_and(\|k\| k != body)` in place of the byte comparison | exactly one: the key-`f`/body-`z` fixture; its control and every other test (31 + 70) pass |
| C — skip `idx:coop` spelling integrity | the index scan replaced by an empty vector | 5 loader fixtures (alias value, alias under another coop id, different registered treasury, undecodable value, write order) and the two in-module index pins |
| D — scanner disposition `FailClosed` → `Equivalent` | the treasury descriptor's disposition only | the scanner's descriptor pin and alias-blocking fixture; the gate's alias-pair fixture; the loader-side agreement fixture and the in-module registry pin |
| E — descriptor prefix widened to `ledger:treasury:` | the prefix only | three scanner fixtures (siblings outside, DID-looking coop id, descriptor pin); four gate fixtures (single-row clear-and-covered, DID-looking coop id, siblings not misread, spelling-bearing siblings uncovered); two loader-side fixtures and the in-module pin |

Evidence: `icn/crates/icn-ledger/tests/treasury_principal_rows.rs` — 32 fixtures on real sled
stores, each refusal with its one-fact-different control; six in-module tests in `treasury.rs`
(the no-partial-adoption pin against the private maps, the key classifier, the sibling list, the
registry agreement pin, and the payload-free diagnostics); eight scanner fixtures and the
registry pins in `did_collision_scan.rs`; seven startup-gate fixtures (§10.5). Fixture evidence
only.

---

## 5. Namespace decisions (Phase 5)

I7 moves `Did` equality. It does **not** move any namespace that derives or stores identity in
another representation. Each is decided explicitly below; none is left implicit.

| Namespace | Site | Decision | Reason |
|---|---|---|---|
| `EntityId::from_did` | `icn-entity/src/entity.rs:51` | **A — follows principal identity** | An entity derived from a principal must not fork when the principal is re-spelled. Must canonicalize at construction and de-duplicate rows. |
| `StewardId::from_did` | `icn-steward` | **A — follows principal identity** | Same argument; a steward is a principal in a role. |
| `icn-commons` weak-holder id | `icn-commons/src/inner.rs:357`, `Sha256::digest(did_str)` | **A — follows principal identity**, and is a **blocking prerequisite** | Hashing the *spelling* means one principal under two spellings mints two holder records. I7 makes the `Did`-equality gate treat them as one principal while the derived id still says two — a split I7 actively **creates**. Must be resolved before the equality flip. |
| Kernel `Did = String` alias | N2-H (#2629) | **B — remains representation-sensitive for now** | Out of N2-A scope by the tranche contract; N2-H owns it. Recorded so the mixed domain is not accidental. |
| `Community.members` | `icn-community/src/types.rs:6`, `MemberId = String` (*"Can be DID or CooperativeId"*) | **B — remains representation-sensitive**, and is **§7.5-adjacent** | The type deliberately holds two identifier domains. Principalizing it is a membership change, which §7.5 gates. Must not be smuggled into N2-A. |
| `ReplicaMetadata` | `icn-store` | **B — remains representation-sensitive** | Replica placement is a storage-locality concern, not a principal-identity one. Documented rather than changed. |
| `SenderPrincipal` (`icn-net`) | `replay_guard.rs:78` | **B — intentionally key-based, not byte-based** | Keys on `VerifyingKey` because the replay guard and the signature check must agree. It is *narrower* than `Did::identifier_bytes` (anchor DIDs may not decompress). Divergence is deliberate and must be documented, not "fixed". |

Decision **A** namespaces gated the equality flip when this was written. The flip has since landed
(#2686) without them, so the split each names now *exists* rather than being anticipated, and they
gate the **cutover** instead. Decision **B** namespaces gate neither, but each is now a stated
choice rather than an omission.

---

## 6. Partner invariants (re-verified against `83682563`)

These had to change **with or before** the `Eq`/`Hash` flip. As of 2026-09-03: §6.1 moved with the
flip (#2686), §6.2 moved before it (#2681), and §6.3 has not moved and now gates the cutover. The
designs below are kept as written.

### 6.1 `PeerId` ordering — `icn/crates/icn-net/src/topology.rs:51`

```rust
impl Ord for PeerId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.to_string().cmp(&other.0.to_string())   // spelling
    }
}
```

`PeerId(pub Did)` derives `PartialEq`/`Eq`/`Hash`. Post-I7 equality becomes byte-based while
ordering stays spelling-based, so two `PeerId`s can be `==` yet `cmp` to `Ordering::Less` —
breaking the standard-library requirement that `Ord` be consistent with `Eq`, and letting a
`BTreeMap<PeerId, _>` hold two entries that compare equal.

**Design:** order by decoded identifier bytes, with a total, deterministic tie-break for spellings
that do not decode (they must still order, and must not order equal to a decodable peer).
Landable **before** the flip; doing so shrinks the atomic surface.

### 6.2 CCL `Value::Did` — `icn/crates/icn-ccl/src/types.rs:110` and `:218`

`Value` derives `PartialEq`/`Eq` (so `Value::Did` equality follows `Did`), but `Hash` is
hand-written and hashes `format!("{did:?}")` — the `Debug` of `Did(String)`, i.e. the spelling.

Post-I7 two `Value::Did` values can be equal with different hashes. That is not a behaviour
difference; it is a **violated `Hash`/`Eq` contract**, and it is silent. `HashSet<Value>` backs
`Value::Set`, `participants` and `in` checks, so membership tests would begin to return wrong
answers with no error anywhere.

**Design:** hash `Value::Did` over `identifier_bytes`, falling back to the spelling only where the
DID does not decode (so equal values hash equally in both populations). The same applies to the
`Value::Set` hash at `:229`, which sorts members by `format!("{a:?}")` — also spelling-derived.
This is the sharpest of the three and **must be atomic with the flip**.

Required regression tests: a `HashSet<Value>` and a `HashMap<Value, _>` holding two alternate
representations of one principal must observe exactly one member.

### 6.3 Networking peer maps

* `SessionManager.connections: HashMap<String, quinn::Connection>` — `icn-net/src/session.rs:58`,
  keyed by the remote peer DID *spelling*.
* `NetworkActor.peer_connections: HashMap<Did, PeerConnectionInfo>` — `icn-net/src/actor/mod.rs:178`.

Both hold remote-peer connection state: **the same identity domain, two keying regimes**. Post-I7
the `Did`-keyed map becomes principal-keyed while the `String`-keyed map stays spelling-keyed, and
they disagree.

**Design:** converge on one principal identity. Per the #2641 lesson, converting a single lookup
call is not sufficient — the audit must cover **cardinality** (`.len()` used as a peer count),
**iteration**, **deletion**, **replacement** and **lifecycle** (insert on connect, remove on
disconnect), because a disconnect that removes one spelling's entry while the other map keeps its
principal entry leaks connection state in exactly the direction that is hard to observe.

### 6.4 String-versus-`Did` comparison classification

| Site | Classification |
|---|---|
| `icn-gateway/src/api/commons/mod.rs:242,278,358` — `claims.sub != did.to_string()` | **should become principal-aware** — an authorization comparison that a re-spelling defeats |
| `icn-trust/src/lib.rs:1180` | **should become principal-aware** — trust identity |
| `icn-gossip/.../storage_challenge.rs:58` | **should become principal-aware** — challenge attribution |
| `icn-compute/src/receipt.rs:691` | **should become principal-aware** — receipt attribution |
| `icn-store` pos, `icnctl` display/CLI paths | **intentionally representation-sensitive** — echoing what the operator typed |
| kernel `Did = String` alias sites | **belongs to another tranche** — N2-H (#2629) |

Not mass-rewritten: each needs its semantic classification confirmed at the site before change.

### 6.5 Loader findings the ledger pass did not fix

The §4.1 audit was run across the other principal-keyed loaders at the same time. These are
**findings, not fixes** — no code below has been changed, and each is stated so the next slice
starts from evidence rather than from a re-derivation.

**`icn-federation` `AttestationStore` — closed by #2704.** This finding is recorded as history
because the paragraph above says these are findings, not fixes, and this one is now the exception.
The defect was four things at once: `federation/attestations/<did-spelling>/<coop>` appeared in no
row of §4 and in no descriptor of `n2a_keyspaces`, so the scanner could not report a collision
there and §3.2's "zero collisions" never covered it; the in-memory cache was `LruCache<Did, _>` and
therefore principal-keyed while `member_prefix` scanned `did.as_str()` and was therefore
spelling-keyed, so looking up spelling A then spelling B of one principal returned **A's
attestations for B** and the opposite order returned the opposite answer; `remove_attestation`
deleted one spelling's row, so a revocation naming the other spelling left the attestation live;
and every read path dropped an undecodable row with `if let Ok(..)`, turning a corrupt attestation
into an absent one. None of it needed a migration to appear.

The domain answer was never in doubt — an attestation is a claim about a **principal** — so the fix
is a principal-consistent read, not a re-keying. Since #2704 the store reads the whole namespace,
validates that every row's key is exactly the key its own value implies, groups by `Did` equality,
and refuses any operation that would interpret or mutate an ambiguous `(principal, source_coop)`
pair; the cache is gone rather than re-keyed; an unreadable or key/value-inconsistent row is a
typed refusal instead of an absence; and a revocation names the semantic pair, removing every
spelling of it in **one atomic deletion** (`Store::delete_atomic`), so no crash or I/O failure can
leave one alias row behind as an unambiguous — and therefore acceptable — survivor. The keyspace is
registered as `icn-federation/attestations` (§4 row 19) and **fail closed**: it authorizes no merge
rule, and neither does this document.

Registration is also what puts the keyspace in front of the gate. Before it, a principal-bearing
row under an unregistered prefix was *uncovered* to the gate, which refuses rather than classify
(§10.2) — blocking, but with nothing to say about what it found. The gate now reads this keyspace
through the same descriptor the store's own refusals agree with, so both layers use one collision
unit: **(member principal, exact `source_coop_id` bytes)**. The source is a federation-domain
identifier this crate does not own and does not parse, so two source ids are the same source only
when their bytes are equal — including when both happen to contain spellings of one DID. §10.6's
division of labour is unchanged by any of this: the gate answers whether the node may open the
state, the store answers whether an operation may interpret it, and neither answer is a merge rule.

**`icn-security` `MisbehaviorDetector` (#2676) — the survivor is attacker-selectable.** Four
spelling-keyed keyspaces (`security:{reputation,banned,quarantine,violation}:`) load independently
into four principal-keyed `HashMap`s, so a principal can end up with its reputation from one
spelling and its ban timestamp from another. The winner in each is the byte-greatest key, i.e. the
multibase base character, which the writer chooses. `save_to_store` writes only surviving entries
and deletes nothing, so the losing rows persist and can win again on the next start. Since #2700
the startup gate refuses to start `icnd` over a collision or an unreadable row in `security:*`
(`DeferredCollisionPosture::BlockStartup`, §10.2) precisely because this loader folds; the loader
itself is unchanged, and the fold remains reachable wherever the detector loads with no gate in
front of it. The sharpest
detail is not in the loader at all: `handle_signed` already derives a canonical `SenderPrincipal`
for its replay and capability decisions, then passes the raw wire **spelling** to
`record_violation` — the replay guard and the detector disagree about who the sender is. Security
identity should resolve through the decoded principal at the recording site, which makes the
loader's job smaller rather than larger.

**`icn-net` peer maps (blocker 7) — and a snapshot restore whose winner is hash order.** §6.3's
design is confirmed against live code, with two additions. First, nothing removes an entry from
either map on transport error or close, so a peer that reconnects under a second spelling leaves
`SessionManager.connections` (String-keyed) holding two entries — one dead, one live — while
`NetworkActor.peer_connections` (`Did`-keyed) holds one; `connections_active` then over-reports,
`peer_exchange` advertises both spellings to the federation as distinct peers, and a send naming
the first spelling selects the dead connection. Second, and not previously recorded: snapshot
restore (`actor/mod.rs`) parses each stored `String` key back into a `Did` and inserts, so two
spellings collapse with **`HashMap` iteration order** deciding which x25519/ML-KEM key material and
capability set the node comes up with — nondeterministic across restarts. Row 15 already says
`peer_connections` has no authorized merge rule; restore is currently implementing one by accident.
This remains the larger change (~20 call sites plus a restore policy) and stays its own slice.

* `FreezeManager::load_from_store` (pre-existing, unchanged by this tranche): expired freeze rows
  are dropped at load rather than adopted, exactly as the loader did before I7, so a row that
  expired before a start is never deleted from disk by `cleanup_expired` and accumulates. It is
  not state and it is not a collision, so it blocks nothing; deleting it at start-up would add a
  writer to the load path, which this tranche deliberately does not do. Follow-up: #2683.

---

## 7. Site taxonomy carried in from #2641

#2641 produced six defects across four review rounds; only two were equality comparisons. Any
N2-A audit restricted to `Did == Did` would have missed the other four. Every site N2-A touches is
therefore classified across nine axes: **equality/comparison; hash/map/set keying; persisted key
representation; cardinality/counting; iteration-order semantics; paired computations/joins;
collision/migration behaviour; twin implementations; guard reachability.**

Three questions are asked explicitly at each site:

* **Partial principalization** — if one half of a computation becomes principal-aware, does its
  counterpart still count, filter, order or join by spelling? (#2641: a `.len()` quorum
  denominator over a DID-keyed collection whose numerator had become principal-reduced.)
* **Twins** — does the same rule exist separately in actor, manager, library, persisted store,
  query/read model and restore/snapshot paths? (#2641 fixed one side of a pair twice in a row.)
  Prefer a shared primitive over per-site fixes.
* **Guard reachability** — can an earlier filter, map lookup, de-duplication, early `continue`,
  first-match lookup or restore/rebuild step discard contradictory alias evidence *before* a
  fail-closed guard observes it? Having a conflict check is not enough if the bad input cannot
  reach it.

The §6.3 peer-map work and the §5 weak-holder decision are both twin-shaped, and the
`replay_sender_regime` fail-closed path is a guard-reachability case that #2644 already got right.

---

## 8. Migration sequence

**Status 2026-09-03.** Steps 6 and 7 of the sequence as originally planned — the flip and its
discriminating tests — have landed (#2681, #2686). The planned order is kept in §8.1 as the
historical record; the sequence to follow now is the post-flip cutover order in §8.2. Nothing in
§8.1 is an instruction any more.

### 8.1 As planned before the flip (historical)

Unchanged in structure from the prior pass; step 4's content is narrowed by §1.1 (replay rows
already carry their rule in-tree) and widened by §5 (the weak-holder id is a blocking prerequisite,
not a follow-up).

1. **Run the collision scan** read-only on every live deployment. *Blocker for everything below.*
2. **Settle the namespace decisions** — §5 decision-**A** namespaces, above all the `icn-commons`
   weak-holder id, whose split I7 actively creates.
3. **Fix the partner invariants that can land independently** — `PeerId` `Ord` (§6.1), the
   `String`/`Did` peer-map pair (§6.3), and the dormant `CompressedVectorClock` derive shape. These
   reduce the atomic surface.
4. **De-duplicate durable collision-bearing rows** *before* the first start of a key-equality
   binary: that first start performs the lossy rebuild and its write-back orphans the losers.
   Choose the class-C merge rules (§4 rows 14–15) before decode collapses them.
5. **Validate rollback/compatibility.** Equality-over-bytes moves no durable byte and changes no
   acceptance, so a binary rolled back to string equality reads the de-duplicated rows unchanged.
6. **Flip `Did` `Eq`/`Hash`** to decoded identifier bytes, **atomically with** the CCL
   `Value::Did` hash correction (§6.2). *Done: #2681 (CCL hash), #2686 (flip, `PeerId` ordering).*
7. **Run broad discriminating tests**, including the mutation check that the new tests fail under
   the old equality. *Done for the flip in #2686; each loader guard carries its own (§4.1).*
8. **Membership and vote migration stay behind §7.5** and are not part of N2-A.

### 8.2 The remaining cutover order (live)

1. **Re-run the collision scan** read-only on every live deployment — the two not yet scanned
   (§3.1) first — and again immediately before the cutover (§3.5). The scan gates the cutover; it
   did not gate the flip, which moved no persisted byte. The same audit now runs unconditionally
   at every `icnd` start (§10), which makes §3.5's point-in-time limit a limit on evidence
   gathered *outside* the binary.
2. **Settle the §5 decision-A namespaces**, above all the `icn-commons` weak-holder id, whose
   split now exists rather than being anticipated.
3. **Close the remaining partner invariant** — the `String`/`Did` peer-map pair (§6.3). `PeerId`
   ordering and the CCL `Value::Did` hash are done.
4. **Guard or de-duplicate before a deployment's first start on a build that still folds.** The
   three `icn-ledger` loaders now refuse rather than fold (§4.1), so for them this step is the
   refusal itself, and the startup gate (§10) refuses the `icnd` start in front of every loader
   over an unruled collision, an unreadable or uncovered principal row and a `security:*`
   collision. The loaders in §6.5 still fold; what the gate does not stand in front of — a
   report-only deferral (§10.2), a database outside `icnd`'s data directory, a loader reached
   without `icnd` — is exactly where, for each of them, either the guard lands first or that
   deployment's collision-bearing rows are de-duplicated before its first start on a build
   carrying I7. Choose the class-C merge rules (§4 rows 14–15) before decode collapses them.
5. **Rollback/compatibility** holds as stated: equality-over-bytes moved no durable byte, so a
   binary rolled back to string equality reads the same rows.
6. **Membership and vote migration stay behind §7.5** and are not part of N2-A.

---

## 9. Remaining cutover blockers

The equality flip (§8.1 step 6) has landed (#2686). The **cutover** — the point at which persisted
spelling-keyed state is proven safe under principal equality and the in-binary refusals (§10, §4.1)
can be trusted to hold — may be declared only when **all** of the following hold, re-stated against
`main` at `ceec4820` (the startup gate) plus the ledger loaders. Until 2026-09-03 this section gated
the flip itself.

| # | Blocker | State | Evidence |
|---|---|---|---|
| 1 | Collision scans run against live deployment data | **PARTIAL** | 3 of 5 deployments scanned, 94 sled DBs, 24 registered rows, **0 collisions**. `alpha` and `icn-daemon` unscanned (`CrashLoopBackOff`); sample is small and point-in-time (§3.5) |
| 2 | Every observed collision group has an authorized disposition | **CLEARED (vacuously)** | zero collision groups observed. Vacuous truth — it does not validate any merge rule |
| 3 | Every required keyspace migration has a safe sequence | **PARTIAL** | with zero collisions, step 4 is empty for the scanned deployments. The load/rebuild/write-back audit (§8.1 step 4) is done for the three `icn-ledger` keyspaces, whose loaders now refuse rather than collapse (§4.1); `icn-federation`'s `AttestationStore` is audited to the same standard and now refuses rather than fold (§6.5, #2704); the `icn-federation` agreement party index is proven a derived projection and its store answers from canonical membership (§11.3, #2707); the `icn-ledger` treasury loader classifies its primary rows, checks its cooperative index and refuses before adopting (§4.2, #2627 M1); the remaining §6.5 loaders — `icn-security`'s misbehaviour detector and the `icn-net` peer maps — have not been |
| 4 | Namespace splits created by principal equality resolved | **OPEN** | `icn-commons` weak-holder id decision stated (§5) but unimplemented; #2627 correction 2 records that I7 opens a lower-privilege route to it |
| 5 | `PeerId` ordering | **DONE** | `Ord` over identifier bytes, non-interleaving classes, landed with the flip in #2686 (`icn-net/src/topology.rs`); #2684 had added the `peerid_i7_ordering_tripwire` that pins it |
| 6 | CCL `Value::Did` `Hash`/`Eq` | **DONE** | hash over identifier bytes in #2681, before the flip; #2686 pins the contract in `value_did_hash` |
| 7 | `String`/`Did` peer-map semantics | **OPEN** | design complete (§6.3), unimplemented — `SessionManager.connections` is still keyed by the peer spelling (`icn-net/src/session.rs`) |
| 8 | No §7.5 migration smuggled in | **HELD** | `gov:vote:` rows and the `icn-coop` membership row are excluded, not migrated; the startup gate reports vote collisions and does not act on them (§10.2) |
| 9 | Broad discriminating tests for the flip | **DONE for the flip itself** | #2686 — `did_principal_equality`, fifteen tests flipped and three re-scoped (#2627 records the count), the `PeerId` tripwire; the ledger loaders carry the §4.1 fixtures. What remains untested is what remains unimplemented (rows 4 and 7) |
| 10 | Fail-closed check inside the key-equality binary | **DONE** | §10 — `icnd` refuses to start over an unruled collision, uncovered row, unreadable row, unverifiable store or newer-generation receipt; 49 fixture tests plus the scanner's. Inside `icn-ledger`, the three loaders refuse again for their own keyspaces (§4.1; 30 fixtures), and the treasury loader for its primary rows and cooperative index (§4.2; 32 fixtures) |
| 11 | Persisted principal-identity generation boundary | **DONE (generation 1)** | §10.3 — the receipt records the generation; a newer generation's receipt is refused. Generation 2 (any re-key) is *not* designed; the ledger loaders re-key nothing and leave it at 1 |

Blockers **3, 4 and 7 are independent of collision evidence** and would each remain even if every
deployment scanned clean; rows 5, 6 and 9 are closed, and row 3 is done for its ledger half. Rows
4 and 7, then the rest of row 3, are the shortest path forward. Row 10 changes their consequence
rather than their status: until they are done, a store that trips one of them **refuses to
start** instead of merging silently — and for the three ledger keyspaces the loader refuses again
(§4.1), with or without the gate in front of it.

The evidence gate (#1) has moved from *no evidence* to *no collisions in three deployments*, which
is real but bounded, and §3.5 explains why a point-in-time clean scan cannot license anything on
its own. Row 10 is what makes that acceptable: the binding check runs at the moment it matters.

---

## 10. The startup gate (implemented 2026-09-02)

`icn_store::n2a_startup_gate::enforce` runs in `icnd` after the data directory exists and
**before the first store is opened** — the ledger, trust and parameter stores the daemon builds
in `main`, then everything the supervisor opens. It is the fail-closed check §3.5 and inventory
§12.1 item 7 call for, placed where those documents say it belongs.

### 10.1 What it does, in order

1. Reads `<data_dir>/n2a-startup-gate.json` if present. A receipt it cannot parse, or one whose
   `generation` exceeds the binary's, **refuses** — and is left untouched.
2. Finds every sled database beneath the data directory by its `conf` file (`find_sled_roots`):
   one per domain under `store/`, plus the data-directory-level databases a deployment keeps
   (`commons.sled`, …). A directory that is not a database is never opened, because `sled::open`
   would create one. A database added after this document was written is found the same way —
   there is no list to fall out of date.

   **The sweep is all-or-nothing.** An unreadable directory, an unreadable entry, a symlink, or
   the depth bound each **refuse** rather than returning a shorter list. Review found the original
   walk swallowed all four: a directory that is searchable but not readable — the daemon can still
   open `…/store/ledger` by path while being unable to enumerate `…/store` — made an existing
   database look absent, and the gate would then have written a CLEAR receipt over a store it never
   audited. A caller cannot distinguish a partial list from a complete one, so the function must
   not produce one. A symlink is refused rather than followed or skipped: following lets the sweep
   leave the intended subtree, and skipping omits a database the daemon can still reach through it.
3. Opens each database, runs `audit_sled_store` — the **same** computation `did-collision-scan`
   renders — and closes it so the daemon's own open can take the lock.
4. Writes the receipt atomically (sibling temporary, `fsync`, rename) **once a verdict exists** —
   for `clear` and for `refused`. The refusals that occur before any store is audited (an
   unreadable or newer-generation receipt, a missing data directory, an incomplete sweep, an
   unverifiable store) return without recording anything: there is no verdict yet to record, and
   overwriting a prior receipt would destroy the last one that meant something.
5. Returns the receipt on `clear`; otherwise a `GateRefusal` that `icnd` turns into a non-zero
   exit with the payload-free summary in §10.4.

It **never writes to a domain store**. The only mutation while it runs is sled's own recovery on
open, which the daemon would perform moments later regardless.

### 10.2 What refuses, and what only reports

| Condition | Effect | Why |
|---|---|---|
| Collision in a registered keyspace whose rule is `Established` and automatable (`replay_max_seq`, `replay_finalized`, `journal`, `agreement_party_index`) | **clear**, group recorded | the live loader already implements the merge (§1.1, §4) — or, for the party index, proves membership from the canonical row on every read, so the group is a redundancy and not a decision (§11.3) |
| Collision in a registered keyspace whose rule is `AwaitingDomainSignOff` (§2.4's seven, including `sequences_receiver`) | **refuse** | a plausible rule is not an authorized one, and a rule written down by precedent is not one a loader performs |
| Collision in a `FailClosed` keyspace (`replay_sender_regime`, `icn-coop/member`) | **refuse** | no rule exists |
| Unreadable principal row in a registered keyspace | **refuse** | cannot be classified, so cannot be merged on its own recognizance |
| Principal-bearing row under no registered keyspace and no named gate | **refuse**, masked shape reported | a keyspace nobody classified is the one that collapses unexamined |
| Principal-bearing row in a named tree | **refuse** | `Store::scan` cannot examine it |
| Collision or unreadable row in `security:*` | **refuse** | `DeferredCollisionPosture::BlockStartup` — `MisbehaviorDetector::load_from_store` folds alias rows into principal-keyed maps and `save_to_store` at shutdown orphans the losers (#2676) |
| Collision in `gov:vote:*` | **report only** | `ReportOnly` — votes are read per proposal, tallied through `VoteTally::try_from_votes` which fails closed on conflicting rows (#2641/#2677), and nothing at startup writes vote rows back. Deciding this is §7.5's business |
| Collision in `auth:challenge:*` | **report only** | `ReportOnly` — a TTL-bounded nonce; collapse drops an in-flight challenge the client re-requests, nothing is written back, and blocking would trap the daemon that alone expires the rows |
| A store that cannot be opened (held elsewhere, corrupt) or read completely | **refuse** | a store nothing can be said about is not one to start over |

The postures are recorded on each `DeferredNamespace` with a one-line rationale citing the loader
behaviour they rest on, and a test pins that every deferral carries one. They are statements about
the load paths **in this checkout**; a loader that changes must revisit its posture.

### 10.3 The receipt and the generation

`n2a-startup-gate.json` is a **record, not a skip token**. The audit runs at every start whatever
the receipt says; a test inserts an alias row after a clear receipt and proves the next start
refuses. The receipt exists so an operator can see what was inspected, which keyspace refused and
why, and when — and so a later generation has a boundary this one detects.

`PRINCIPAL_IDENTITY_GENERATION = 1` means I7: principal-byte equality, spelling-preserving
persistence, no row re-keyed. A receipt carrying a higher generation was written by a binary that
may have re-keyed rows; a generation-1 binary refuses to open that data directory rather than
guess, and does not overwrite the receipt. A lower or absent generation simply means the audit has
not been recorded under generation 1 yet.

**Rollback.** A pre-gate binary knows nothing of the receipt and ignores it; I7 moved no byte, so
it reads the same rows. A generation-1 binary rolled forward again re-audits. What no binary can
retroactively enforce is the interval in which a pre-gate binary ran over a store that has since
acquired alias rows — which is exactly why the audit is unconditional.

**Crash safety.** An interrupted receipt write leaves only `n2a-startup-gate.json.tmp`, which is
never read as a receipt; the next start audits and replaces it.

### 10.4 What an operator does with a refusal

The daemon exits non-zero with one line per blocker, of the form

```text
<store>: keyspace icn-ledger/balance (sum, awaiting-domain-sign-off): 1 collision group(s) over 2 row(s), 0 unreadable; principals [1a2b3c4d]
```

carrying the keyspace, the rule and its authority status, counts, and eight-hex principal
fingerprints — never a stored value and never a full identifier. Then:

1. run `did-collision-scan <store>` for the full report, including which spelling a last-writer
   rebuild would keep;
2. take the disposition to the domain that owns the keyspace (§4): economics for the ledger sums
   and freezes, trust for the edge union and issuer sequence, governance for membership, the
   security workflow for `security:*`;
3. apply that disposition to the rows by hand, or flip the descriptor's `RuleBasis` to
   `Established` with the approval cited, and restart.

There is **no bypass flag**, by design: the alternative to refusing is the silent merge the whole
tranche exists to prevent.

### 10.5 Evidence

`icn/crates/icn-store/tests/n2a_startup_gate.rs` — 49 fixtures on real sled databases (32 with
#2700, five more for the attestation keyspace with #2704, five for the agreement party index with
#2707, §11.3, seven for the treasury primary rows with #2627 M1, §4.2): every
row of §10.2 with its one-fact-different control (same spelling twice, two different principals,
a single security row, an uncovered row without a principal); the fixture guard that the two
spellings are distinct strings and `==` as `Did` with equal hashes; unreadable and
newer-generation receipts refused and preserved; older-generation receipt superseded; the
record-not-token property in both directions; idempotence with every store's rows byte-identical
across runs; a stale `.tmp` not read; no database created where none existed; a store held open
elsewhere unverifiable and nothing recorded; the receipt and the refusal message carrying neither
payload nor identifier. `icn/crates/icn-store/src/did_collision_scan.rs` adds eight tests on the
deferred postures and the shared audit. All fixture evidence; no deployment was scanned by the
gate.

### 10.6 The gate and the ledger loaders (#2701)

The startup gate and the `icn-ledger` loaders of §4.1 are two enforcement layers, not one check
in two places, and the cutover relies on both.

The gate asks whether this node may safely open **all** persisted principal-bearing state beneath
its data directory. It runs in `icnd` before the first store is opened, reads keys and never
values, knows a keyspace only by its registered prefix (§2.7), and refuses or clears the whole
directory. The loaders ask whether **this** keyspace's rows can be interpreted and adopted by the
code that owns them. They run inside `Ledger::new` — in `icnd` after the gate has cleared, and in
every other embedder and test with no gate in front of them — know the exact shape their writer
produces, and read a value only after its key has been accepted.

For `icn-ledger/balance`, `icn-ledger/cleared_volume` and `icn-ledger/frozen` the two layers agree
wherever they observe the same thing, and differ only where they do not:

| Question | Startup gate (§10) | Ledger loader (§4.1) |
|---|---|---|
| Collision unit | the raw key with each `did:icn:` spelling replaced by its decoded 32 identifier bytes (§2.7); the currency stays in a `cleared_volume` shape, so only same-currency rows group | the decoded identifier bytes plus the discriminator — the currency for `cleared_volume`, empty otherwise — through `icn_ledger::principal_rows`: **the same unit** |
| Merge rule | none asserted; a collision under `AwaitingDomainSignOff` refuses (§10.2) | none asserted; a collision is `PrincipalRowsRefusal::AliasCollision`. No sum, union, latest-wins, normalization, re-key or de-duplication |
| Readable spelling | one the layout-independent tokenizer can delimit and decode to 32 identifier bytes (§2.6); any other `did:icn:` token is an unreadable row and refuses | the writer's grammar plus `Did::from_str`, which also requires an Ed25519 point. Within the alphabets the tokenizer walks the loader is the stricter: a spelling that decodes but is no principal is `UnreadableKey` here and readable there. A spelling the tokenizer cannot delimit at all — an Identity-base body, whose sigil and raw bytes fall outside those alphabets — is unreadable to the gate and parses in the loader (§4.1's Identity fixture) |
| Physical shape | prefix match plus the spelling; `did_ends_key` for `frozen` only. A key under `ledger:balance:` with no `did:icn:` in it is a row without a principal and does not block | the writer's exact shape: canonical JSON quoting for `balance` (a bare or non-canonically escaped key is `UnreadableKey`), a currency delimiter after the spelling for `cleared_volume`, strict UTF-8 for `frozen`; anything else refuses |
| Values | never read | read only after the key is accepted; a `balance` or `frozen` body whose spelling disagrees with its own key is `KeyValueSpellingMismatch` |
| Expired freeze rows | not distinguishable without reading the value, so two spellings of one principal under `ledger:frozen:` are a collision whatever their expiry | dropped before grouping: an expired row is not live state, so a lapsed alias does not block a start |
| Write-back | none — the gate writes to no domain store | keeps the stored row identity (`HashMap::get_key_value`, `remove_entry`); no spelling is normalized and no row is re-keyed, so `PRINCIPAL_IDENTITY_GENERATION` stays 1 |
| Diagnostic | store, keyspace, rule and its authority, counts, four-byte (eight-hex) principal fingerprints (§10.4) | keyspace, counts, eight-byte principal fingerprints, and for `cleared_volume` an escaped and bounded currency discriminator; never a spelling, never a value |

Three consequences follow, each deliberate:

* **Gate CLEAR does not mean the loader has nothing left to check.** A row the gate cannot see
  as a problem — a bare `ledger:balance:` spelling, a non-canonical JSON escape, a spelling that
  decodes but is no Ed25519 point, a `cleared_volume` key with no currency delimiter, a body that
  spells its account differently from its key — is refused by the loader. The gate is complete
  over *principals across the directory*; the loader is complete over *this keyspace's grammar*.
* **A loader refusal never contradicts the gate's disposition.** The loader's collision unit is
  the gate's, and every token the gate can *delimit* but not decode fails `Did::from_str` too, so
  a row the gate refuses as a collision or as an undecodable spelling is one the loader refuses as
  well — with the two exceptions below, where the gate refuses and the loader does not, never the
  reverse. Neither layer asserts a merge rule and neither normalizes a spelling away, so they
  cannot disagree about a survivor: there is none.
* **Where the gate is the stricter layer, that is a boundary, not a conflict.** Two cases. An
  expired alias freeze is a collision to the gate, which must not read the value that would show
  it lapsed, and is not state to the loader. A spelling the tokenizer cannot delimit is unreadable
  to the gate and a legal account to the loader — a limit of the scan's tokenizer (§2.6), not of
  the keyspace. In `icnd` the gate runs first, so in both cases the node does not start and the
  loader's answer is never reached; it is observable only where `Ledger::new` runs with no gate in
  front of it. Neither case involves a merge rule: the operator's remedy (§10.4) is to remove a
  lapsed row the loader would not have adopted, or to take the tokenizer limit to the scanner.

Neither layer replaces the other. The gate covers every principal-bearing keyspace in the
directory — the loaders that still fold (§6.5) and any keyspace nobody registered included — and
the loaders cover exactly four keyspaces — the three above and, since M1, the treasury primary
rows (§4.2) — with knowledge the gate must not have. Without the gate,
§6.5's loaders run unguarded; without the loaders, every opener of a ledger store that is not
`icnd`, and every row the gate cannot see, is unguarded.

Evidence: `icn/crates/icn-ledger/tests/principal_keyed_rebuild.rs` — 30 fixtures on real sled
stores, one per row of the table above and its one-fact-different control (§4.1), plus the
`principal_rows` unit tests; `icn/crates/icn-ledger/tests/treasury_principal_rows.rs` for the
treasury loader (§4.2); `icn/crates/icn-store/tests/n2a_startup_gate.rs` for the gate (§10.5).
Fixture evidence only.

---

## 11. Persistence-boundary classes — what the common scanner proves, and what it cannot

Three N2-A fixes in a row — the ledger balance fold (#2701), the federation attestation store
(#2703/#2704) and the agreement party index (§11.3) — failed by three *different* mechanisms, and a
fix copied from one would have been wrong for the next. Patching one prefix at a time therefore does
not converge. This section classifies every persisted principal boundary by the **mechanism that
proves it safe**, so the remaining inventory (§11.4) is finite: each boundary is dispositioned by its
class, and a boundary's class can be re-checked from three facts a reviewer can read off the code —
*how the principal is represented at rest, what the loader does with it, and whether the row is
authoritative or derived*.

The labels are `P1`–`P5`, deliberately not the inventory's `A`–`E`. The inventory's letters (§3 of
`n2-a0-stored-key-inventory.md`) describe *what I7 moves* — ephemeral, reconstructed, serialized,
durable key, wire round-trip. The classes below describe *how a boundary is proven*. One keyspace
can carry an inventory letter and several `P` classes at once, and conflating the two alphabets is
how a `SILENT-MERGE RISK` row came to be treated as though a scanner pass had cleared it.

### 11.1 The classes

| Class | Mechanism at rest | What proves the boundary safe | What the common scanner can say |
|---|---|---|---|
| **P1 — spelling-visible key** | The persisted key retains a `did:icn:` spelling (`as_str`, `Display`, JSON-quoted) | A registered descriptor with an authorized disposition (§4), **plus** a loader that either never folds by `Did` or folds under a P4 guard. The startup gate (§10, #2700) refuses an unruled alias pair at every start | Can group rows by principal and prove *no alias rows exist today* under the prefix. That is evidence about the rows, not about the loader |
| **P2 — derived / opaque key** | The key is a function of the spelling the scanner cannot reverse: `hash(did.as_str())`, `EntityId::from_did` (scheme-stripped), `StewardId::from_did` (SHA-256 over the spelling), the weak-holder id | Divergence must be prevented *before* derivation — derive from identifier bytes (§5 decision **A**) with a de-duplication/migration step for rows already split — or a higher-level loader must fail closed on two derived ids that resolve to one principal. Nothing else is evidence | **Nothing.** These keys hold no `did:icn:` literal, so the scanner does not count them as principal-bearing at all: they are absent from the covered, deferred *and* uncovered totals. A CLEAR verdict over a store full of them is silence, not safety |
| **P3 — value-carried principal** | The key names no principal; the serialized value carries a `Did`, a `Did`-keyed map or set, or a spelling `String` | A value-aware audit or the loader's own interpretation of the value under `Did` equality; the merge rule for a decode that collapses aliasing keys must be chosen before decode (§4 rows 14–15) | **Nothing** — the scanner never reads a value (§2.2, payload-free) |
| **P4 — principal-fold boundary** | Spelling-keyed rows are loaded into `HashMap<Did, _>`, `HashSet<Did>` or `Vec<Did>::contains`, and the result is written back | Classification of every row **before** any reaches the map, refusing an ambiguous keyspace whole (the `principal_rows` shape of #2701); a P1 scan is necessary evidence but cannot see the fold | Sees the rows, not the fold: it can say two spellings exist, and cannot say that the loader will collapse them, orphan the loser on write-back and make the survivor attacker-selectable (§2.7) |
| **P5 — derived projection / secondary index** | Rows are a deterministic function of a canonical object stored elsewhere — `idx_*`, `by_did/`, `*_owner:` indexes | A source-of-truth declaration; reads that prove the fact from the canonical row rather than from the index; superseded-row cleanup on replacement; deletion of every spelling on delete; a deterministic rebuild from canonical rows; and a rule that malformed projection rows are surfaced, never read around. The projection is never authority | Sees the rows. A collision under a projection prefix is a redundancy, not a contradiction, so its disposition is `Equivalent` once the loader is projection-correct — and *only* then |

A boundary may carry several classes. `commons/holders/by_did/` is P1 + P5 over a P2 canonical
row; `ledger:treasury:` is P1 + P3 + P4. The class of the **canonical** row decides the
disposition; projection classes add consistency machinery but never a merge rule of their own.

### 11.2 "Scanner clear" is not "persistence boundary proven safe"

The two claims must never be written as one. Precisely:

* **Scanner clear** means: among rows under *registered* prefixes whose key contains a decodable
  `did:icn:` spelling, no two rows share a principal-canonical shape; every registered keyspace
  with a collision carries an `Established`, automatable rule; and every `did:icn:`-bearing row in
  the store is either covered or behind a named deferral (§2.3). It is a statement about P1 rows on
  the day of the scan (§2.5, §3.5).
* **Persistence boundary proven safe** means: the boundary's class is stated, the mechanism that
  class requires is implemented in the live loader/store, and discriminating tests exist for it —
  including the mutation check that the tests fail with the mechanism removed.

The scanner discharges the P1 evidence obligation and nothing else. It cannot see P2 keys, P3
values or P4 folds, cannot tell a projection from an authoritative row, and cannot speak for a
store it did not scan. Any sentence of the form "keyspace X is safe" must therefore name the class
and the mechanism; a scan verdict alone is never that sentence.

### 11.3 Disposition of `idx_agreement_party/` — P5, `Equivalent` / `Established`

**Finding: derived, not authoritative.** `AgreementStore::store_agreement` writes one
`idx_agreement_party/<party-did spelling>/<agreement id>` row per entry of the canonical
`Agreement.parties` vector, valued by the agreement id; nothing else writes under the prefix and
the row carries no fact the canonical `federation/agreements/<id>` row does not already state.
Every membership decision in the federation crate — proposer checks, signature admission, gossip
`is_party_to`, suspension, termination, amendment rights — reads `agreement.parties` under `Did`
equality; none reads the index. The index is therefore a P5 projection and the correct disposition
is to prove and maintain it as one, not to treat two index spellings as two competing facts.

**Defects verified against `main` before the fix** (all reproduced by tests that failed on the
unchanged code):

1. a lookup under an alternate spelling of a party found nothing — the projection *omitted*
   membership canonical state contains;
2. replacing an agreement whose party set shrank (a ratified `RemoveParty` amendment, or a gossip
   sync replacement) never retired the old row — the projection *preserved* membership canonical
   state no longer contains, and the lookup returned it;
3. a well-formed index row naming a non-party returned that agreement — the projection could
   *create* membership;
4. an index row whose value named a different agreement than its key was read by the value;
5. malformed index rows were silently skipped, and so were malformed canonical rows in
   `list_agreements`;
6. deletion removed only the rows the current party set implied, leaving alias rows behind;
7. two rows for one `(principal, agreement)` would have produced the agreement twice once
   lookups became principal-wide.

**Fix (#2707).** Persisted encodings are unchanged; no row is re-keyed, merged or
normalized. The store now holds one invariant: *the party index may accelerate discovery of
canonical agreements and may never create, omit, preserve or alter agreement-party membership
independently of canonical agreement state.*

* **Reads prove membership from canonical rows.** `list_agreements_for_party` reads the whole
  projection, keeps every row whose spelling decodes to the queried principal (the same decode
  `Did` equality uses), de-duplicates by agreement id, loads each canonical row once, and returns
  only agreements whose `parties` contain the principal under `Did` equality. The answer is the
  same under every spelling, in either insertion order, from a warm or a cold handle, and after
  reopen; results are ordered by `(created_at desc, id)` so order is a function of the data.
* **Two kinds of inconsistency are told apart, by whether the write protocol can produce them.** A
  row pointing at a missing agreement, or at one that no longer lists the principal, is *stale*: the
  protocol below can leave it behind, so reads filter it and the rebuild removes it. A row the store
  could never have written — a key that does not parse as `idx_agreement_party/<spelling>/<id>`, a
  spelling that names no principal, a value naming a different agreement than the key — is
  *malformed*: every operation that interprets the projection refuses with
  `AgreementPartyIndexMalformed { rows, first_reason }` (every row is classified before refusing;
  no spelling or payload travels in the error). Canonical evidence is held to the same standard:
  an unreadable canonical row refuses every operation that needs it with
  `AgreementStoreUnreadable`, including `list_agreements`, which no longer skips it; and a
  canonical row that deserializes but carries a *different agreement's id than its key names* is
  attributed to neither agreement and refuses with `AgreementStoreKeyValueMismatch` — such a row
  is one row's value under another row's key, and using it would let a replacement retire the
  other agreement's projection rows, a rebuild call the named agreement's real rows stale, or a
  lookup report a party absent from a row it never read. Every mutating path (`store_agreement`,
  `delete_agreement`, `rebuild_party_index`) reads canonical evidence before it moves a byte, so
  both refusals leave the store byte-for-byte unchanged. An agreement id is never empty:
  `store_agreement` refuses one before any byte moves (`AgreementIdEmpty`), and a canonical row
  that carries one — a shape a pre-#2707 store could hold, and one the gossip handler would
  otherwise accept from any peer — is unreadable canonical evidence, refused by every operation
  that needs it including the rebuild, because the projection cannot encode it and a rebuild
  over it would oscillate (integration review finding, generation 2).
* **Writes keep the projection a superset of the truth, never a subset.** `store_agreement`
  writes the new party rows, then the canonical row, then retires the rows the previous canonical
  version implied and the new one does not (a removed party, or a party the new version spells
  differently). `delete_agreement` removes the canonical row and then every projection row naming
  that agreement under any spelling. A crash at any point leaves extra rows, which reads filter,
  and never a canonical row without its rows. Reads and writes of the namespace are serialized in-process — a process-wide `RwLock`, the
  #2704 pattern: writers exclude each other, so two concurrent replacements of one agreement
  cannot interleave their cleanup and strand a canonical party, and a party lookup holds the
  namespace across its projection scan *and* its canonical loads, because `Store::scan` on sled
  is not a snapshot (the concurrency note below).
* **The projection can be recomputed.** `AgreementStore::rebuild_party_index` derives the expected
  rows from every canonical row and makes the projection equal to them, reporting rows kept, added,
  removed as stale and removed as malformed. It refuses before mutating anything if any canonical
  row is unreadable. No canonical byte is touched.
* **The scanner registers the prefix** as `icn-federation/agreement_party_index`, `Equivalent`,
  `Established`, under the structural descriptor #2704 made canonical:
  `PrincipalRegion::AnchoredThenOpaque { terminator: b'/' }`. The party spelling is anchored
  immediately after the prefix and ends at the `/`; the agreement id after it is an opaque
  discriminator carried into the canonical shape byte-for-byte and never parsed — an identifier
  the agreement's creator chose, which the registry does not own, so an id that contains or is a
  `did:icn:` spelling is still just an id and no id is normalized. The collision unit is **(party
  principal, exact agreement-id bytes)**. `slash_ends_did` is claimed by no descriptor: the
  anchored region ends the spelling by construction, and the registry pins that. A federation-side
  test pins the registration to the store's own separator; scanner fixtures prove the alias pair
  is one automatable group, that different agreement ids never group, that an id containing a
  DID spelling stays an id, and that the rows are covered rather than uncovered; and registry pins
  fix the anchored layouts as exactly the two federation keyspaces in registry order and the
  whole-key layouts as the twelve pre-existing ones, unchanged.

**Startup gate (added on integration onto #2700).** `icn/crates/icn-store/tests/n2a_startup_gate.rs`
carries five party-index fixtures on real sled databases: an alias pair for one agreement is
classified as one `Equivalent` group and the start is **clear**, with every row byte-identical
afterwards; the same two spellings across two agreements are two shapes and no group; a party
spelling that names no principal **refuses**; an agreement id containing a DID spelling does not
refuse; and a party-index row is never reported uncovered. The fixtures discriminate on exactly
the two facts they exist to pin: with the disposition changed to `FailClosed` the alias-pair
fixture refuses, and with the region changed to `WholeKey` the alias-pair and opaque-id fixtures
turn unreadable. A CLEAR from the gate means this spelling collision is safe under the registered
projection disposition. It does not mean the projection is complete, current or authoritative
(§11.2).

**Rebuild stays an explicit operation.** The gate is read-only (§10.1) and the daemon has no
post-open repair hook; none is introduced. `rebuild_party_index` is a domain projection-repair
operation invoked deliberately, never a startup step, so no silent migration runs on daemon
start. A legacy store whose index was left incomplete by an old torn write is made complete by
the rebuild, not by reads.

**Concurrency (found on integration onto #2704).** #2704 established that `Store::scan` on sled
is not a snapshot: its iterator takes its read lock once per item, so a mutation that commits
partway through a namespace read is visible to the rows not yet visited and invisible to the ones
already collected. The write protocol above keeps the projection a superset of canonical
membership at every instant, but a view assembled half before and half after a commit is a
superset of nothing. The one shape that bites is a replacement that only *re-spells* a party: it
writes `idx/<new>/X`, writes the canonical row, retires `idx/<old>/X`; a scan that had passed the
new row's position before the write and reaches the old row's position after it collects neither,
and canonical verification cannot recover a row the scan never saw — the lookup would report the
party absent from an agreement it belongs to before, during and after, an answer no valid state
gives. The `[a] → [b]` replacement and the delete have no such hybrid: every candidate is checked
against the canonical row, and both the pre- and the post-state answer are valid. The per-store
writer mutex the branch originally carried could not close this — it coordinated no reader, and
nothing between two handles — so the store now holds a process-wide `RwLock` in the #2704 pattern:
writers exclusive; party lookups shared, and held across the scan and the canonical loads; the
rebuild exclusive from its first read to its last write. Deterministic tests in `agreement::store`
model the straddling iterator with a `Store` double paused at a barrier and read whether the
namespace is held from the lock itself, never from timing; the re-spelling fixture fails with the
read guard removed. `icnd` constructs one `AgreementStore` over a dedicated sled database, so the
process-wide lock is equivalent to a per-instance one there and stricter only where several
handles share a backend. Production behaviour of the store changed in exactly this respect.

**What this does not claim.** The `Equivalent` disposition is correct *because* the loader is now
projection-correct; on a binary without this fix the same scan verdict would have been
meaningless, which is §11.2 in one sentence. No live store was scanned or repaired; every claim
above is fixture evidence.

### 11.4 Remaining N2-A boundaries, by class and disposition (classification pass of 2026-09-03)

A bounded adversarial pass over the boundaries the inventory and the #2703 audit had left open,
re-verified against `main` at `5add7a48`. *Live* means a production binary constructs the store;
*dormant* means only library code or tests do. None of these is fixed here; each is dispositioned
by mechanism so it can be closed by the proof its class requires. At the time of the pass the
scanner registry covered none of the prefixes in this table; `adr0014:grant:by_grantee:` has since
been registered and closed (§11.6), and the rest are unchanged.

**P5 projections over a P1 or P3 canonical row — close with the §11.3 mechanism (source-of-truth
declaration, canonical-membership reads, superseded-row cleanup, every-spelling delete, rebuild):**

| Boundary | Reach | Canonical row | Consumer today | Notes |
|---|---|---|---|---|
| `apps/governance` `action_item_by_assignee:<did>:<domain>:<item>` | live | the action-item row (`assignee: Option<Did>`) | spelling prefix scan, stale rows skipped | **regressed by I7**: `save` decides stale-index removal with `existing.assignee != item.assignee`, now principal equality, so re-saving under another spelling leaks the old row. Actionable now |
| ~~`icn-gateway` `adr0014:grant:by_grantee:`~~ **closed in M2, §11.6** and `receipt:meeting_attendance:by_pair:` | live | `adr0014:grant:<uuid>` / `receipt:meeting_attendance:rec:<hash>` | spelling prefix scan; the rebuild write-back adds and never deletes | authorization enumeration by grantee missed alias rows, and alias-issued authority survived alias-spelled revocation. The by-grantee half is dispositioned in §11.6 and registered; the meeting-attendance pair index is untouched and remains open |
| `icn-gateway` `idx_device_owner:` and `idx_notif_recipient:` | live | `device:<token>` / `notif:<id>` | `String` prefix scan; `mark_read`/`delete_notification` authorize by raw `String` compare against the JWT `sub` | fail-closed for the caller (empty inbox), not principal-correct |
| `icn-gateway` `v1:interest_idx:<listing>:<did>` | live | the `v1:interest:` row | a sled compare-and-swap on the spelling key *is* the one-interest-per-member rule | alias defeats the de-dup; needs the guard on the canonical side, then the projection rule |
| `apps/ledger-app` `idx_owner:`, `idx_escrow_creator:`, `idx_escrow_beneficiary:`, `idx_budget_owner:` (constructed by the gateway) | live | `payment:<id>` / `escrow:<id>` / `budget:<id>` | `String` prefix scan against the raw JWT `sub` | the clearest `sub`-versus-stored-spelling surface; normalize `sub` through `Did` before index construction |
| `icn-commons` `commons/{anchors,holders,stewards}/by_did/` — the **holders** third is **registered and mint-guarded in M3, §11.7**; anchors and stewards are untouched (twin: `icn-gateway/src/commons_store.rs`, dead) | live | `commons/{anchors,holders,stewards}/<id>` — **P2 rows** | `String` lookup; `delete_holder` removes one spelling; the governance `FreezeMember` side-effect no-ops on an alias | the projection can be made correct, but the canonical id is hashed from the spelling: see the P2 group |
| `icn-ledger` `asset_owner:<did>:<asset>` | dormant | `asset:<id>` | read re-checks `owner_did` by `Did`; `transfer_custody` authorizes by `Did` and removes the index by the caller's spelling — an orphan the read re-check then hides | the §7 *partial principalization* pattern, verbatim |
| `icn-ledger` `obligation_creditor:` / `obligation_debtor:` | dormant | `obligation:<id>` | prefix scan, no re-check, no delete path | |
| `icn-identity` `personhood/by_did/` (dormant) and `commons/by_did/` (dead) | — | `personhood/anchors/<id>` / `commons/holders/<id>` | exact-key | delete the dead store; rebuild rule for the dormant one |
| `icn-governance` `index:delegations:from:` / `index:delegations:to:` (dormant, write-only) and `steward/by_did/` / `steward/by_holder/` (dormant, no persistent backend) | — | `delegation:<id>` / `steward/records/<hex>` | none / exact-key | delete-dead-code candidates; live twins exist |

**P2 derived / opaque keys — scanner-blind; close only by canonical derivation (§5 decision A)
plus de-duplication, or by a fail-closed loader:**

| Boundary | Reach | Derivation | Consumer today | Disposition |
|---|---|---|---|---|
| `icn-entity` `entity:<EntityId>`, `membership:<parent>:<member>`, `member_count:`, and the `member_of:` / `type:` projections | live (icnd `init_entity`; gateway `api/entity.rs`) | `EntityId::from_did` = `"entity:icn:individual:" ‖ spelling with `did:icn:` stripped` | `EntityId` **string** equality throughout; `add_membership` refuses only the exact key | one principal, two `EntityId`s: duplicate individuals, doubled membership and `member_count`, a self-removal that misses. Absent from every scanner total. Needs derivation over identifier bytes and a de-dup step — a data-format change, not a merge rule |
| `icn-commons` `commons/stewards/<hex(StewardId)>` | live | `StewardId::from_did` = SHA-256(`"steward:"` ‖ spelling) | spelling lookup through `by_did/`; no `delete_steward` | two independent steward records with independent bonds and sanctions; a suspended steward attests under the alias. Fail-closed loader guard now; derivation fix with migration later. Independent defect: the index is written from `holder_did` and queried by `steward_did` |
| `icn-commons` `commons/holders/<hex>` (weak-holder path) | live; authorization consumer (`require_membership_in_jurisdiction`, `require_office_in_jurisdiction`) | SHA-256(`did.to_string()`) as the holder/anchor id | spelling lookup | the §5 decision-A prerequisite, unchanged: I7 *creates* this split. A member enrolled under one spelling fails the standing gate under another |

**P4 folds over P1 rows — close by reusing `icn_ledger::principal_rows` (#2701, merged): classify
every row before the map, refuse ambiguity whole:**

| Boundary | Reach | Fold | Why the existing guards do not catch it |
|---|---|---|---|
| `icn-ledger` `ledger:treasury:<did>` (+ `ledger:treasury:idx:coop:<coop>` valued by a spelling) | live (`apps/ledger-app` init; `icnctl treasury` maintenance) | *was*: `load_from_store` folded every row into `HashMap<Did, Treasury>` plus `Did`-keyed budget/rule maps, and `persist_treasury` wrote back under the survivor's `Display`; the two hydration guards compared `existing_did != treasury.treasury_did`, principal equality since I7, so two spellings collapsed at insert with the scan-last row winning | **closed by M1 (§4.2, #2627)**, reproduced first: the loader classifies every primary row through `principal_rows`, proves key/body bytes, validates the index and refuses before any map mutation; registered as `icn-ledger/treasury`, fail closed, at the gate (§4 row 21). The `audit:` and `idx:budgets:` siblings that embed a spelling remain uncovered — the next treasury follow-up |
| `icn-governance` bare `vote:<proposal>:<voter>` and `index:votes:<proposal>` (P3 value of spellings) | dormant (`#[allow(dead_code)]`; live twin is `gov:vote:`) | `store_vote` resolves the writer's `VotingPrincipal`, walks the index and deletes alias rows in one batch | correct as a duplicate-act guard, but the prefix `vote:` does not match the `gov:vote:` deferral, so any such row is *uncovered*. Add a second `DeferredNamespace` under the §7.5 gate, or delete the dormant store |

**P1 authoritative rows awaiting a disposition — close by registering with the rule the owning
domain authorizes (or `FailClosed` until it does):**

| Boundary | Reach | Consumer today | Proposed rule | Owner |
|---|---|---|---|---|
| `icn-gateway` `did_doc:<did>` | live | exact key behind a `String`-keyed LRU; `revoke_device` mutates only the caller's spelling; no delete | `FailClosed` / `Established` — merging two key sets decides who may sign | identity |
| `icn-ledger` `membership:since:<did>` | live | write-once-if-absent; an alias re-registers tenure at today | `FailClosed` until credit policy speaks — `MaxMonotonic` is the *wrong* rule (max = latest join = tenure loss) and the safe rule (min) is not in the vocabulary | ledger credit policy |
| `icn-ledger` `patronage:account:` / `patronage:entry:` | dormant | exact key; the entry de-dup is spelling-scoped | `AwaitingDomainSignOff` (plausibly sum / union) | cooperative finance |
| `icn-core` `federation:provenance:<raw coop_did String>` | service live; durable store wired only in tests | `String` map lookup; leave under one spelling does not delete the other | validate `coop_did` as a `Did` at the boundary first; `FailClosed` if registered — two records name two governance decisions | federation |
| `icn-trust` `sybil:verification:<did>` and `sybil:flag:<did>:<ts>:<type>` (P3 values) | dormant, absent from the inventory | exact key / spelling prefix; `clear` deletes one spelling | delete-dead-code, else `FailClosed` — `Revoked` versus `Verified` under two spellings is contradictory by construction, and a permissive rule would weaken an anti-sybil control | trust |

Three of these findings are **regressions introduced by I7 itself** rather than legacy rot: the
action-item stale-index condition, the treasury hydration guards (closed by M1, §4.2), and the
asset-transfer index removal. Each is the §7 *partial principalization* pattern — one half of a computation became
principal-aware while its partner still counts, deletes or keys by spelling.

### 11.5 Non-claims, and reconciliation with the merged gate, ledger and attestation work

* **N2-A is not complete.** §9 stands. This section makes the remaining work finite; it closes one
  boundary, and M1 (§4.2) closes a second.
* **No merge rule was invented.** Row 20 uses the existing `Equivalent` disposition because the
  rows are derivations of one canonical fact; nothing here authorizes a merge of authoritative
  state anywhere else, and the seven `AwaitingDomainSignOff` keyspaces are unchanged.
* **Registry scope.** At this pass `n2a_keyspaces()` held fifteen descriptors: the twelve
  whole-key keyspaces the §3 evidence was gathered with, the two anchored federation layouts (§4
  rows 19 and 20), and the treasury primary rows (§4 row 21, #2627 M1) as the thirteenth whole-key
  layout. Two have been added since — the length-prefixed tag-discriminated grant-by-grantee
  projection (#2627 M2, §11.6) and the Commons holder-by-DID index (§4 row 22, #2627 M3, §11.7) as
  the fourteenth whole-key layout — for **seventeen**. The §3 figures are not re-stated in either
  case: a registry expansion does not enlarge old evidence (§3.2).
* **#2700 (startup gate, merged).** The gate-level fixtures this section owed are in
  `icn/crates/icn-store/tests/n2a_startup_gate.rs` (§11.3). The gate consumes the party-index
  descriptor read-only; the rebuild is not wired into daemon start, and no post-open repair hook
  exists or is introduced.
* **#2701 (ledger loaders, merged).** Nothing copied; the treasury fold reuses
  `icn_ledger::principal_rows` as §11.4 required — done in M1 (§4.2), with one added constant
  and no change to the guard or its three existing callers.
* **#2704 (attestation store, merged).** Both sets of `FederationError` variants coexist; §4
  carries rows 19 and 20; `slash_ends_did` is claimed by no descriptor — both federation layouts
  declare anchored regions, and a registry pin fixes them as the two anchored layouts in registry
  order. The attestation store stays fail-closed (authoritative rows, P1); the party index stays
  `Equivalent` (derived projection, P5). They are different classes and are deliberately not made
  to look alike. #2704's non-snapshot-scan finding is applied here as the namespace lock (§11.3).

### 11.6 Disposition of `adr0014:grant:by_grantee:` — P5, `Equivalent` / `Established` (#2627 M2)

**The authority model.** ADR-0014 grant storage is three keyspaces, and only one of them is
authority:

```text
adr0014:grant:<uuid>            = the AuthorityGrant. Canonical authority fact.
adr0014:grant:by_decision:...   = derived lookup projection.
adr0014:grant:by_grantee:...    = derived lookup projection.
```

A **Person** grantee's semantic key is the decoded Principal — the same relation `Did` equality
itself defines (§11 I7). An **Entity** grantee's semantic key is the exact entity string under
current semantics; nothing here changes that, and the two are not collapsed into one rule.

**Defects verified against `main` @ `10fafafc` before the fix**, each reproduced by a test that
failed on the unchanged code:

1. a grant issued to `Person(A)` was invisible to a query for `Person(B)` when A and B are two
   accepted spellings of one principal — `grant_by_grantee_scan_prefix` builds its scan boundary
   from `did.as_str()`, so it selects one spelling of a principal that has many. The control
   under the issuing spelling returned the grant, so the projection *omitted* authority the
   canonical record states;
2. an accepted `RevokeAuthority` decision naming that principal under the other spelling **left
   the grant active**. This is the consequence that matters: the SDIS payloads carry a DID and
   never a grant id, so `revoke_active_grants_for_person` can only find its targets by
   enumeration. Alias-issued authority survived alias-spelled revocation;
3. the live `MandateGate` domain-target path, which resolves actor-first through the same
   enumeration, returned `MandateRejection::NoMandate` for an actor spelled otherwise than the
   grant was issued to. This is a **false negative** — authority that exists is not found — not a
   privilege escalation;
4. neither reader checked that the primary record it loaded actually named the grantee asked
   for. A projection row naming `Person(A)` but pointing at a grant whose canonical record names
   a different principal returned that grant to A: the projection could *create* authority;
5. a single ordinary Person grant row made `icnd` **refuse to start**. The row carries a
   `did:icn:` literal, the prefix was in no scanner descriptor and no deferral, and an uncovered
   principal row is a startup blocker (§10). The gateway opens its store at
   `<data_dir>/gateway_store`, inside the tree the gate audits.

**Fix (#2627 M2).** Persisted encodings are unchanged; no row is re-keyed, merged, normalized or
deleted, and no preferred spelling is chosen. The store now holds one invariant: *the by-grantee
projection may accelerate discovery of canonical grants and may never create, hide or alter
authority independently of the canonical `AuthorityGrant` records.*

* **Reads prove authority from canonical rows.** A Person query reads the whole projection,
  structurally parses every row, decodes each Person-tagged spelling with the production parser,
  keeps every row naming the requested principal under `Did` equality, de-duplicates by
  `AuthorityGrantId`, loads each canonical record, and returns only grants whose primary
  `grantee` is that principal. An Entity query keeps the exact-prefix scan — Entity identity is
  the exact string, the region is length-framed so no id can be a prefix of another, and no alias
  relation exists to miss — and is verified against the primary identically. Liveness still comes
  from the primary's `revoked_at`, as it always did. Order is `(valid_from, grant id)`, so the
  reinstatement seam's "most recent revoked grant" is a function of the data and not of scan
  order.
* **Distinct grants stay distinct.** The de-duplication unit is the canonical `AuthorityGrantId`,
  never the grantee. A principal may legitimately hold several grants, and two ids are two
  grants; only rows naming *one* id collapse.
* **Stale and malformed evidence are told apart by whether the write protocol can produce them.**
  A row pointing at a missing primary, or at a primary naming another grantee, is *stale*: reads
  filter it and it confers nothing. Filtering cannot hide a real grant — the only grant such a row
  could name is the one its own id names, whose canonical record does not exist or does not name
  this grantee, and every other row is judged on its own primary; a live grant can never present
  this shape, because row and primary are written in one transaction and no path deletes a grant
  primary. A row this writer could never have produced — framing that
  does not parse, a length field overrunning the key, a suffix that is not
  `valid_from ‖ grant id`, an undefined variant tag, a Person region naming no principal, or a
  value naming a different grant than its own key — is *malformed*, and every read refuses with
  `grant_by_grantee_index_malformed: rows=<n> reason=<class>`. Refusing is the only answer that
  neither invents authority nor hides it: an uninterpretable row cannot be attributed to a
  principal, so it cannot be ruled out as the row naming the one being asked about, and dropping
  it would answer "no authority exists" on evidence never read. No spelling, entity id or grant
  payload travels in the error.
* **No writer-side guard was needed, and the reason was reproduced rather than assumed.** The
  projection is append-only: `put_authority_grant` and `put_mandate_with_grants_atomic` insert
  primary and index in one transaction, the startup backfill derives its key from the primary's
  own grantee and so cannot introduce a spelling the primary does not carry, and revocation
  touches the primary alone. Equivalent duplicate rows for one canonical grant are therefore
  harmless after verification and de-duplication, which a fixture pins.
* **No new lock, for a stated reason.** `sled`'s scan is not a snapshot, but because nothing
  deletes or re-keys a projection row, a straddling enumeration can only miss rows written after
  it began and never lose one written before — and every row it sees is proven against its own
  primary. That is precisely the guarantee #2704 and #2707 needed a namespace lock to obtain,
  because those projections retire rows on replacement and a scan could miss the old row and its
  replacement both. A behavioural fixture holds the property under concurrent writes. What remains
  is a *subset* read — a scan racing the multi-grant atomic commit may return a set matching no
  single committed state. **This read is not a linearizable snapshot of a grantee's grant set and
  is not claimed to be.** What is claimed: authority is never invented, because each returned
  grant was loaded from its own primary and checked there; and every outcome reachable under a
  subset read is also reachable by reordering the concurrent commits. The gate only becomes more
  restrictive. Revocation can miss a grant minted concurrently — the schedule where it ran first —
  and can never miss one committed before it began. Reinstatement's `has_active_in_domain`
  precheck is the one consumer a subset read makes more *permissive*: it can mint where it would
  have declined, which is the same two active grants two concurrently accepted minting decisions
  produce with no race at all, and the precheck is best-effort against a duplicate proposal rather
  than mutual exclusion. None of this is new — the pre-M2 reader scanned and loaded each primary
  with the same absence of a snapshot. M2 changes which rows are discovered and adds the
  primary-grantee check; it touches snapshot semantics not at all, and removes the part that was
  representation-dependent.
* **The scanner registers the prefix** as `icn-gateway/adr0014_grant_by_grantee`, `Equivalent`,
  `Established`, under a **third** structural descriptor,
  `PrincipalRegion::LengthPrefixedTagged { principal_tag: 0x01 }`. Neither existing
  layout can read this key. `AnchoredThenOpaque` looks for a terminator byte, and here the
  spelling is preceded by a binary length field and followed by an arbitrary `u64`, so no
  terminator names the boundary — every row would be unreadable and the gate would refuse
  unconditionally. `WholeKey` is worse than a refusal: the canonical shape would carry the `u32`
  length prefix verbatim, and that field is *derived from the spelling*, so two aliases of one
  principal would differ in it, land in different shapes and form no collision group at all — a
  silent false-clear. The new variant therefore replaces the region *including its own framing*,
  which is derivable from the spelling, while the `valid_from` and grant id after it are carried
  byte-for-byte and never parsed. The descriptor learns no ADR-0014 authority semantics: it
  knows a big-endian `u32` length field, a tag value, a principal subregion and an opaque tail.
  The width is fixed rather than a descriptor field: one layout declares this region and it frames
  with a `u32`, so a configurable width would buy a hand-rolled accumulator — and its silent
  truncation above eight bytes — for no present caller. `principal_tag` stays a parameter because
  it is the discrimination that keeps an entity id spelling `did:icn:` from being read as a
  principal. The
  collision unit is **(grantee principal, exact `valid_from`, exact grant id)**, so two spellings
  for one grant are one group and two grants for one principal are two shapes — *not* an
  assertion that a principal holds one grant, which would be false. An Entity-tagged region names
  no principal and is reported principal-free rather than unreadable; a region whose framing does
  not parse is unreadable and blocks, as the gate's fail-closed rule requires.

**What this does not claim.** The `Equivalent` disposition is correct *because* the reader is now
projection-correct; on a binary without this fix the same scan verdict would be meaningless
(§11.2). No live store was scanned or repaired. No rebuild for this projection is added: the
existing `backfill_grant_by_grantee_index` remains an additive recovery for pre-index databases,
not a reconciliation, and a verified read path is sufficient for software closure — retiring
historical rows, if it is ever wanted, is a separate explicit operation. Every claim here is
fixture evidence. No merge rule is authorized for authoritative state, and no grant body or
projection row was re-keyed.

### 11.7 Disposition of `commons/holders/by_did/` — P5 index over a P2 row, `FailClosed` / `Established` (#2627 M3)

**The defect, at the seam rather than in either half.** A member's profile update reaches
`CommonsInner::update_display_name` through two gateway callers —
`api/members.rs::update_member_profile` (`PUT /v1/members/{coop_id}/{did}/profile`, self-service)
and `api/coops.rs`'s member-add, which sets a display name best-effort. Both authorization checks
on the first route compare `Did`s, and since I7 that is *principal* equality: the self-service
check `caller_did != did` and the membership check `coop.members.iter().any(|m| m.did == did)` both
correctly accept a second textual spelling of an enrolled member. Immediately below them the mint
decided existence with a single exact-key `get` on `commons/holders/by_did/<spelling>` and, on a
miss, derived the new holder's permanent id as `SHA-256(did.to_string())`. Neither half is wrong on
its own. Composed, they are:

```text
principal-level authorization
+ spelling-level existence test
+ spelling-derived durable identity
= a second durable Commons holder for one principal
```

Reproduced on `12902244` before any fix, through the production route: the alias-spelled request
returned **`200 OK`** — not `403`, not `404`, so both gates did accept it — the
`commons/holders/by_did/` row count went **1 → 2**, two primary holder records existed, their ids
differed, and the first holder survived with its display name intact. This is not a failed lookup
whose damage is a missing read; it is a **created fact**. Two concurrent alias-spelled updates for
one principal both succeeded.

**What M3 establishes.** Classification before mint, and nothing else:

```text
exact index row for this spelling?
  value is not 64 lowercase hex digits            → REFUSE  holder_index_malformed
  primary proven, filed under this same spelling  → ordinary update
  primary cannot be resolved                      → REFUSE  holder_index_primary_missing
  primary filed under a different spelling        → REFUSE  holder_index_primary_mismatch
no exact row → read the whole holder-by-DID namespace
  a row under any spelling names this principal   → REFUSE  holder_principal_already_indexed
  a row cannot be parsed as a spelling at all     → REFUSE  holder_index_malformed
  every row read, none names this principal       → mint, byte-for-byte as before
```

Absence of a *principal* is a claim about every spelling, so it is proven against the namespace and
never inferred from the one key that missed. The exact-spelling read happens first so the ordinary
case — one canonical spelling looked up by itself — does not pay for a scan; only the branch that
is about to create durable identity reads the namespace.

The value's shape is checked before the primary is looked up, because a value that is merely UTF-8
would otherwise fail to resolve and be reported as a *stale* index whose primary is missing. Both
refuse, so nothing is unsafe either way — but a dangling reference and an unreadable one are
different defects with different remedies, and the classification must not conflate them.

The exact-hit arm requires the primary to be filed under the **byte-identical** spelling, not merely
to name the same principal. `get_holder_by_did` resolves an index row and loads the primary without
re-checking it, so a crossed row would otherwise have handed one principal's caller another
principal's record to mutate; and a primary carrying the same principal under a *different* spelling
would, on write-back, have filed a second index row under the body's spelling — a normalization no
rule here authorizes. Every row `put_holder` writes agrees with its body by construction, so no
healthy row reaches that arm.

**Writer bytes unchanged.** A proven-absent spelling still derives
`anchor_bytes = holder_id = SHA-256(did.to_string())`, `holder_did = Did(spelling)`,
`personhood_level = Weak`. Changing that derivation would re-key every weak holder already
persisted, which is a migration and not a guard. It is pinned by a regression fixture precisely so
M3 cannot smuggle one in.

**Registration.** `commons/holders/by_did/<spelling>` is registered in `n2a_keyspaces` as
`icn-commons/holder_by_did`, `FailClosed` / `Established`, `PrincipalRegion::WholeKey` with
`did_ends_key` — the writer appends the spelling and stops — as the fourteenth whole-key layout.
Fail-closed is the disposition because two spellings reach two independent `CommonsHolderRecord`s
with their own status, personhood level, affiliations and baseline rights: merging them decides
which holder is a member in good standing, and no identity-layer rule decides that. Two rows naming
one holder id are refused on the same ground — the equal values make a rebuild's choice *look* free,
and `Equivalent` is a claim about derivation only the owning domain can make.

The prefix claims the index subspace and nothing lexically near it. The two siblings —
`commons/holders/<hex holder id>` and `commons/holders/by_anchor/<hex anchor id>` — are keyed by
opaque hex, carry no spelling in the key, and do not start with the registered prefix; a `did:icn:`
appearing in a sibling's stored *value* is not key material and is invisible to a key scan. Sibling
isolation is pinned by fixture.

**Gate and runtime agree.** Before registration the gate blocked a store holding a single weak
holder, as `UNCOVERED` — and blocked two *distinct* principals for the same reason, a false
refusal. Registered, one valid row and two distinct principals are clear, and the alias pair refuses
as `icn-commons/holder_by_did` / `FAIL-CLOSED`. **This unblocks the `by_did` holder rows and nothing
else:** a deployment whose holders came through enrollment also carries
`commons/anchors/by_did/<anchor DID>` rows, which no descriptor covers, so such a store still
refuses to start after M3. The fixtures build the anchorless weak-holder store the profile-update
path produces; the anchor and steward indexes are M4's. A fixture drives one real `commons.sled`
through both layers: the state the mint seam produces is a state the gate opens, the alias pair the
gate refuses is the state the seam refuses to create, and the seam then refuses to add a third
spelling to it. Both wrappers write sled's default tree, so the gate reads exactly the bytes Commons
wrote.

**Concurrency, bounded honestly.** `CommonsHandle` owns commons state behind one
`tokio::sync::RwLock` and every mutation takes the write lock; both gateway callers reach the seam
only through it. `CommonsInner::update_display_name` contains no `.await` between classifying and
writing, so a task that holds the guard runs the check and the write to completion: **there is no
interleaving window, and none was closed here.** The guard is concurrency-correct because of that
ordering, not because it defends a race. A multi-threaded fixture spawns two same-Principal updates
released together by a barrier and asserts exactly one mints — an outcome check under real lock
contention, not a race detector. It did fail before the guard existed, for the plain alias reason
above rather than because any interleaving was observed. Production opens
`commons.sled` once: `icn_core::supervisor::lifecycle` creates the handle and injects it, and
`icn_gateway::server` uses the injected handle rather than opening a second store; the standalone
`with_sled_path` fallback runs only when no handle was injected, and sled holds an exclusive `flock`
on the database directory, so a second process cannot open it. **M3 therefore claims serialization
within the one handle production constructs, and claims nothing about hypothetical independent
handles over one path — that configuration is not production-reachable, and no new lock was added.**

**Crash ordering — a nonclaim.** `put_holder` writes the primary, then the by-DID index, then the
by-anchor index as three separate durable operations. A crash between the first and second leaves a
primary record that this by-DID guard cannot discover, so a later mint under another spelling would
not see it. That is **pre-existing crash-partial persistence debt**, not something M3 introduces or
repairs, and no transaction layer is added here. The claim M3 makes is therefore the narrow one:

> An I7 alternate spelling can no longer cause a second weak-holder mint through a coherent
> holder-by-DID namespace.

It is **not** the claim that two holders for one principal are impossible under every failure model.

**What this does not do.** No holder is merged, adopted, re-keyed or deleted; no alias row is
removed or renamed; no index is rebuilt; no DID spelling is preferred; no weak holder is migrated.
Existing duplicate holders are not dispositioned — where the registered index proves the collision
the gate refuses and runtime mutation refuses, and choosing among already-derived duplicate holder
ids stays a separate domain decision. `get_holder_by_did` is **not** made alias-transparent: a
caller presenting spelling B still fails to find a holder indexed only as A, and the authorization
consumers of that lookup — `authority.rs::require_office_in_jurisdiction`,
`api/membership/mod.rs`, `api/steward/mod.rs` — remain fail-closed exactly as before. Commons
identity is not Principal-transparent, and M3 does not claim it is. The sibling
`commons/anchors/by_did/` and `commons/stewards/by_did/` indexes, the `StewardId` derivation and
the spelling-derived holder id itself are untouched and remain as §11.4 records them.

**The guard is at one seam, and only one.** `get_or_create_holder` →
`create_holder_from_anchor_with_name`, reached from SDIS enrollment, also constructs a fresh holder
(`Weak` when the anchor carries no attestations) and de-duplicates by **anchor**, not by principal.
Two enrollments of one principal under two spellings therefore still produce two holders — exactly
the state the new descriptor refuses at startup. That is a pre-existing behaviour and not a
regression: before M3 this keyspace was `UNCOVERED` and blocked startup for *any* holder row. The
claim is bounded accordingly — at most one newly-minted holder per decoded principal **at the
profile-update seam** — and the enrollment seam is left for its own slice.
