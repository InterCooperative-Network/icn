# N2-A — migration gate: collision scan, dispositions and partner invariants (#2627)

**Status:** living — design and evidence record for the N2-A tranche
**Truth class:** descriptive
**Canonical:** no — `docs/architecture/IDENTITY_SEMANTICS.md` owns the semantic contract and
`docs/architecture/n2-a0-stored-key-inventory.md` owns the measured stored-key surface; this
document owns only N2-A's *dispositions and design*
**Last reviewed:** 2026-08-29
**Source basis:** live `main` at `836825632ebb5b7b9d8d16354974503a7c576569`
**Gates:** N2-A / #2627 (`Did` canonicalization, I7)
**Contract:** IDENTITY_SEMANTICS §3, §7.5, §11 (I7), §14 (`N2-A`)

---

**Tranche gate: IMPLEMENTATION STILL BLOCKED.** `Did` `Eq`/`Hash` is unchanged and must stay
unchanged until the gate below clears.

This document is the design surface for N2-A. It does not change `Did`, migrate any store, or
discharge the §7.5 membership/vote gate. It records what was measured, what was decided, and
what is still missing — deliberately in separate sections, so a decision is never mistaken for
evidence.

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
| `Did` `Eq`/`Hash` | **unchanged** — still derived over the inner `String` (`icn/crates/icn-identity/src/lib.rs`) |
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
| Gate-shaped | Exit status `0` only when every keyspace is automatable **and** nothing principal-keyed lies outside scan reach | Makes the tool usable as the migration gate, not merely as a report |

### 2.3 Grouping rule

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

### 2.4 Coverage limit found while building it

`Store::scan` reads only sled's **default tree**. `icn-gateway`'s service discovery uses a
*named* tree, which a `Store`-trait scan can never reach. A scan reporting zeros on such a store
would be a false negative.

The runner therefore also reports per-tree row counts and per-tree DID-bearing row counts
(`SledStore::tree_row_counts`, `SledStore::did_bearing_rows_per_tree`), and **treats
principal-keyed rows in a named tree as blocking**, not as a clean result.

### 2.5 Fixture tests

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

### 2.6 How to run it

```bash
cd icn && cargo build -p icn-store --bin did-collision-scan
./target/debug/did-collision-scan <store-path> [<store-path> ...] [--json]
```

Against a Kubernetes deployment, extract the volume first — the tool must be given a directory it
can copy:

```bash
kubectl cp <namespace>/<pod>:/data ./deployment-data
./target/debug/did-collision-scan ./deployment-data/<store_dir>
```

Exit status: `0` clear, `1` at least one keyspace must fail closed, `2` tool error.

---

## 3. Scan coverage actually achieved (evidence)

**This section records only what was run. It does not clear the gate.**

| Deployment / store | Rows | Result |
|---|---|---|
| `~/.icn/icnd-dev/{gateway,oracle,identity,entity_audit}_store` | 0 | Empty across all trees |
| `~/.local/share/icn/{gateway,oracle,identity,entity_audit}_store` | 0 | Empty across all trees |
| `~/agent-runs/nycn-organizer-gate-dryrun-2026-07-12/icnd-data/*` | 0 | Empty across all trees |

Every locally reachable store is **genuinely empty** — 0 rows in the default tree and 0 in every
named tree. These are dev scaffolding directories. They demonstrate the tool runs; they are
**not evidence about collisions**, and must not be cited as such.

### 3.1 Evidence that remains unavailable

The populated stores are the K3s cooperative deployments:

| Deployment | Volume | Status |
|---|---|---|
| `icn-coop-alpha` | PVC `icn-alpha-data` | **NOT SCANNED** |
| `icn-coop-beta` | PVC `icn-beta-data` | **NOT SCANNED** |
| `icn-coop-gamma` | PVC `icn-gamma-data` | **NOT SCANNED** |
| `icn-coop-delta` | PVC `icn-delta-data` | **NOT SCANNED** |
| `icn` (`icn-daemon`) | PVC `icn-data` | **NOT SCANNED** |

Volumes are NFS-backed (`nfs.csi.k8s.io`, `atlas.faherty.network:/mnt/ssd_pool/icn-vols/…`).
Both access routes attempted from this environment — `kubectl exec` / `kubectl cp`, and mounting
the NFS export — were refused by the session's permission envelope. No result was inferred,
substituted or estimated.

**The collision gate is therefore closed on absence of evidence, not on adverse evidence.**

---

## 4. Keyspace dispositions (decisions)

Merge rules are stated per keyspace and encoded in the scanner registry
(`did_collision_scan::n2a_keyspaces`), so a scan reports the rule alongside the collision. No rule
was invented to unblock the tranche: where two rows may encode contradictory state and no domain
rule authorizes choosing or combining them, the disposition is **fail closed**.

| # | Keyspace | Key encoding | Collisions observed | Merge rule | Rule already established by domain semantics? | Lossless? | Alias/dual-read window? | Order | Rollback | Tranche | Status |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `icn-net` `replay_max_seq:` | `Display` | **unmeasured** | max floor | **yes** — implemented in `replay_guard` (#2644) | yes | no | 4 | safe (no byte moves) | N2-A | rule live in-tree |
| 2 | `icn-net` `replay_finalized:` | `Display` | **unmeasured** | union | yes (#2644) | yes | no | 4 | safe | N2-A | rule live in-tree |
| 3 | `icn-net` `replay_sender_regime:` | `Display` | **unmeasured** | **fail closed** | yes — loader already declines to collapse | n/a | no | 4 | safe | N2-A | fail-closed by design |
| 4 | `icn-net` `outgoing_seq:` | `as_str` ×2 | **unmeasured** | max | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs review** |
| 5 | `icn-ledger` `ledger:balance:` | JSON-quoted | **unmeasured** | sum | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs economics sign-off** |
| 6 | `icn-ledger` `ledger:cleared_volume:` | `Display` + currency | **unmeasured** | sum | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs economics sign-off** |
| 7 | `icn-ledger` `ledger:frozen:` | `Display` | **unmeasured** | union | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs review** |
| 8 | `icn-trust` `trust/edges/` | `as_str` ×2 | **unmeasured** | union | no — asserted here | yes | no | 4 | safe | N2-A | **rule needs review** |
| 9 | `icn-ledger` `ledger:journal:` | content hash | **unmeasured** | equivalent | yes — key carries no spelling | yes | no | — | safe | N2-A | scanned to confirm |
| 10 | `icn-security` misbehavior | `Display` | **not inspected** | — | — | — | — | — | — | **security workflow** | deferred; migration dependency preserved |
| 11 | `icn-rpc` auth challenges | `Display` | **not inspected** | — | — | — | — | — | — | **security workflow** | deferred; TTL-bounded |
| 12 | `apps/governance` votes (#23) | `Display` | **unmeasured** | §7.5 re-key | n/a | n/a | **required** | after N2-A | n/a | **§7.5 gate** | not N2-A |
| 13 | `icn-commons` weak-holder id (#65) | SHA-256 of spelling | I7 *creates* the split | — | no | **no** | — | before 6 | n/a | N2-A | **namespace decision — see §5** |
| 14 | `VectorClock` (#45), snapshot `vector_clock` (#54) | serialized map | **unmeasured** | max | yes — `VectorClockProjection::from_entries` | yes | no | 4 | safe | N2-A | rule established |
| 15 | snapshot `peer_connections` (#57) | serialized map | **unmeasured** | **fail closed** | no | no | — | 4 | safe | N2-A | **no authorized rule** |
| — | `CompressedVectorClock` (#46) | dormant | n/a | derive-shape fix | n/a | yes | no | 3 | safe | N2-A | no data step |

Rows 10 and 11 are security-specific namespaces. Their **existence and migration dependency are
preserved here**; their contents were not inspected and their disposition belongs to the dedicated
security workflow, not to this tranche.

Rows 14–15 hold DIDs inside serialized *values*, not keys, so they are not prefix-scannable and
are not covered by the scanner registry. Their merge rule must be chosen before decode collapses
them (§12.1 item 4-ii).

Every "unmeasured" cell is a direct consequence of §3.1 and is the substance of the open gate.

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

Decision **A** namespaces gate the equality flip. Decision **B** namespaces do not, but each is now
a stated choice rather than an omission.

---

## 6. Partner invariants (re-verified against `83682563`)

These must change **with or before** the `Eq`/`Hash` flip. All three were re-verified live; none
has moved since the prior pass.

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
   `Value::Did` hash correction (§6.2).
7. **Run broad discriminating tests**, including the mutation check that the new tests fail under
   the old equality.
8. **Membership and vote migration stay behind §7.5** and are not part of N2-A.

---

## 9. Remaining implementation blockers

Implementation may begin only when **all** of the following hold. None is currently satisfied in
full.

| # | Blocker | State |
|---|---|---|
| 1 | Collision scans actually run against live deployment data | **OPEN** — tool ready, cluster access refused in this environment (§3.1) |
| 2 | Every observed collision group has an authorized non-lossy or fail-closed disposition | **OPEN** — no group has been observed yet; four merge rules (§4 rows 4–8) are asserted, not domain-established |
| 3 | Every required keyspace migration has a safe sequence | **PARTIAL** — §8 exists; step 4 cannot be specified without §3.1 |
| 4 | Unresolved namespace identity domains settled | **CLOSED for classification** (§5), **OPEN for implementation** — the `icn-commons` weak-holder id decision is stated but unimplemented |
| 5 | `PeerId` ordering disposition settled | **CLOSED for design** (§6.1), unimplemented |
| 6 | CCL `Value::Did` hash/equality compatibility settled | **CLOSED for design** (§6.2), unimplemented |
| 7 | Mixed `String`/`Did` peer-map semantics settled | **CLOSED for design** (§6.3), unimplemented |
| 8 | No §7.5-gated membership/vote migration smuggled into N2-A | **HELD** — §4 rows 12 and `Community.members` in §5 are explicitly excluded |

The single hard blocker is **#1**. Blockers #5–#7 are design-complete and could land as
independent PRs that reduce the atomic surface without touching `Did` equality.
