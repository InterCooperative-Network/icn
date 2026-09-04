# N2-A — migration gate: collision scan, dispositions and partner invariants (#2627)

**Status:** living — design and evidence record for the N2-A tranche
**Truth class:** descriptive
**Canonical:** no — `docs/architecture/IDENTITY_SEMANTICS.md` owns the semantic contract and
`docs/architecture/n2-a0-stored-key-inventory.md` owns the measured stored-key surface; this
document owns only N2-A's *dispositions and design*
**Last reviewed:** 2026-09-04
**Source basis:** live `main` at `ceec48200cd2f99021befab90b308a052df62336` (§1 baseline rows
still cite the `83682563` measurement they were taken at)
**Gates:** N2-A / #2627 (`Did` canonicalization, I7)
**Contract:** IDENTITY_SEMANTICS §3, §7.5, §11 (I7), §14 (`N2-A`)

---

**Tranche state, in four separate claims.**

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
4. **Cutover is not complete.** The rest of the load/rebuild/write-back audit (§9 row 3: the
   loaders in §6.5), fresh point-in-time evidence on quiesced stores, the two unscanned
   deployments, the §5 decision-**A** namespace splits, the peer-map pair (§6.3) and everything
   behind §7.5 remain open. Nothing here is a deployment-readiness claim.

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
| 13 | `icn-commons` weak-holder id (#65) | SHA-256 of spelling | I7 *creates* the split | — | no | **no** | — | before 6 | n/a | N2-A | **namespace decision — see §5** |
| 14 | `VectorClock` (#45), snapshot `vector_clock` (#54) | serialized map | **0 in 3 scanned** | max | yes — `VectorClockProjection::from_entries` | yes | no | 4 | safe | N2-A | rule established |
| 15 | snapshot `peer_connections` (#57) | serialized map | **0 in 3 scanned** | **fail closed** | no | no | — | 4 | safe | N2-A | **no authorized rule** |
| 16 | `trust-app` `trust/sequences/receiver/` (#71) | `Display` | 0 in 3 scanned | max | **no** — asserted by precedent; `SequenceTracker` reads and writes the exact spelling and implements no fold | yes | no | 4 | safe | N2-A | **rule needs a trust-domain loader that folds; fail closed at the gate until then** |
| 17 | `trust-app` `trust/sequences/issuer/` (#71) | `Display` | 0 in 3 scanned | max | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs trust-domain confirmation** |
| 18 | `icn-coop` `member:` (#36) | `Display` | 0 in 3 scanned | **fail closed** | no | n/a | — | — | safe | N2-A / §7.5 boundary | **institutional decision required** |
| — | `CompressedVectorClock` (#46) | dormant | n/a | derive-shape fix | n/a | yes | no | 3 | safe | N2-A | no data step |

Rows 10 and 11 are security-specific namespaces. Their **existence and migration dependency are
preserved here**; their contents were not inspected and their disposition belongs to the dedicated
security workflow, not to this tranche.

Row 18 (`icn-coop` cooperative membership) is new to this table. Merging two membership rows
decides **who is a member of an institution**, which no identity-layer rule authorizes, and it sits
next to the §7.5 membership gate without the inventory having placed it there. It is therefore
**fail closed** pending an explicit governance-domain decision about which side of the §7.5
boundary it falls on. N2-A must not resolve that by default.

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

**`icn-federation` `AttestationStore` — an unregistered keyspace, and order-dependent reads.**
`federation/attestations/<did-spelling>/<coop>` appears in **no** row of §4 and in no descriptor of
`n2a_keyspaces`, so the scanner cannot report a collision there at all: §3.2's "zero collisions"
never covered it. To the startup gate a principal-bearing row under a prefix nothing registered is
*uncovered*, and it refuses the start rather than classify it (§10.2) — but only for a database
beneath `icnd`'s data directory and only at `icnd`'s start; nothing in this store's own reads
changed. The live defect does not need a migration to appear. The in-memory cache is
`LruCache<Did, _>` and therefore principal-keyed, while `member_prefix` scans `did.as_str()` and is
therefore spelling-keyed, so looking up spelling A then spelling B of one principal returns **A's
attestations for B**, and the opposite order returns the opposite answer — the first non-empty
lookup latches until eviction or invalidation. Long-lived instances (`icn-gateway`'s
`federation_mgr`) are exposed; the RPC handlers construct a fresh store per call and are not.
Separately, `remove_attestation` deletes one spelling's row, so a revocation naming the other
spelling leaves the attestation live, and every read path drops an undecodable row with
`if let Ok(..)`, turning a corrupt attestation into an absent one. The domain answer is not in
doubt — an attestation is a claim about a **principal** — so the fix direction is a
principal-consistent read, not a re-keying, with two rows for one `(principal, source_coop)`
refused because that pair can only differ by disagreeing.

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
| 3 | Every required keyspace migration has a safe sequence | **PARTIAL** | with zero collisions, step 4 is empty for the scanned deployments. The load/rebuild/write-back audit (§8.1 step 4) is done for the three `icn-ledger` keyspaces, whose loaders now refuse rather than collapse (§4.1); the loaders in §6.5 — the `AttestationStore` cache/prefix mismatch recorded on #2627 among them — have not been audited to that standard |
| 4 | Namespace splits created by principal equality resolved | **OPEN** | `icn-commons` weak-holder id decision stated (§5) but unimplemented; #2627 correction 2 records that I7 opens a lower-privilege route to it |
| 5 | `PeerId` ordering | **DONE** | `Ord` over identifier bytes, non-interleaving classes, landed with the flip in #2686 (`icn-net/src/topology.rs`); #2684 had added the `peerid_i7_ordering_tripwire` that pins it |
| 6 | CCL `Value::Did` `Hash`/`Eq` | **DONE** | hash over identifier bytes in #2681, before the flip; #2686 pins the contract in `value_did_hash` |
| 7 | `String`/`Did` peer-map semantics | **OPEN** | design complete (§6.3), unimplemented — `SessionManager.connections` is still keyed by the peer spelling (`icn-net/src/session.rs`) |
| 8 | No §7.5 migration smuggled in | **HELD** | `gov:vote:` rows and the `icn-coop` membership row are excluded, not migrated; the startup gate reports vote collisions and does not act on them (§10.2) |
| 9 | Broad discriminating tests for the flip | **DONE for the flip itself** | #2686 — `did_principal_equality`, fifteen tests flipped and three re-scoped (#2627 records the count), the `PeerId` tripwire; the ledger loaders carry the §4.1 fixtures. What remains untested is what remains unimplemented (rows 4 and 7) |
| 10 | Fail-closed check inside the key-equality binary | **DONE** | §10 — `icnd` refuses to start over an unruled collision, uncovered row, unreadable row, unverifiable store or newer-generation receipt; 28 fixture tests plus the scanner's. Inside `icn-ledger`, the three loaders refuse again for their own keyspaces (§4.1; 30 fixtures) |
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
| Collision in a registered keyspace whose rule is `Established` and automatable (`replay_max_seq`, `replay_finalized`, `journal`) | **clear**, group recorded | the live loader already implements the merge (§1.1, §4) |
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

`icn/crates/icn-store/tests/n2a_startup_gate.rs` — 28 fixtures on real sled databases: every
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
the loaders cover exactly three keyspaces with knowledge the gate must not have. Without the gate,
§6.5's loaders run unguarded; without the loaders, every opener of a ledger store that is not
`icnd`, and every row the gate cannot see, is unguarded.

Evidence: `icn/crates/icn-ledger/tests/principal_keyed_rebuild.rs` — 30 fixtures on real sled
stores, one per row of the table above and its one-fact-different control (§4.1), plus the
`principal_rows` unit tests; `icn/crates/icn-store/tests/n2a_startup_gate.rs` for the gate
(§10.5). Fixture evidence only.
