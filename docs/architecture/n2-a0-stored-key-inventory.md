# N2-A0 — stored-key inventory before `Did` canonicalization (#2623)

**Status:** living — investigation record for the N2-A0 tranche
**Truth class:** descriptive
**Canonical:** no — `docs/architecture/IDENTITY_SEMANTICS.md` owns the semantic contract; this
document owns only the *measured* stored-key surface that gates I7
**Last reviewed:** 2026-08-19
**Source basis:** live `origin/main` at `bca3dd0e4b889a597e7b41e73360e42e03aa8756`
**Gates:** N2-A / #2627 (`Did` canonicalization, I7)
**Contract:** IDENTITY_SEMANTICS §3, §7.5, §11 (I7, I8), §14 (`N2-A0`, marked `HARD GATE`)

---

## 1. What this document is, and is not

**Is.** An enumeration of every durable or durability-adjacent keyspace in the workspace in which
a `Did`, a string representation of one, bytes derived from one, or a composite key containing one
is used as an identity-bearing key — together with what happens to each if I7 changes `Did`
equality from inner-string equality to decoded-key equality.

**Is not.** It changes no code. `Did`, its `Eq`, its `Hash`, its constructors and every encoding
are untouched. Nothing is canonicalized, re-keyed, migrated or rewritten. It selects no account
domain (that is N2-C′ / #2625) and retypes nothing. Completing it does not make I7 safe — it makes
I7 *attemptable*, which is the whole of what §14 asks of a `HARD GATE`.

**Do not infer production readiness, migration readiness, or deployment status from this document.**

---

## 2. The hazard, measured rather than assumed

§14 states the hazard as a possibility: distinct persisted rows *can* silently merge. The first
job of this tranche was to find out whether that possibility is reachable in practice. It is, and
by a wider margin than the contract assumes.

### 2.1 `Did` validation accepts *any* multibase encoding

`Did::from_str` (`icn/crates/icn-identity/src/lib.rs:211`) validates by calling
`multibase::decode(encoded_part)`. Multibase is *self-describing*: `multibase::decode` reads the
first character as a base code and dispatches across every base the crate supports
(`multibase-0.9.2/src/base.rs`). It is not pinned to base58btc. Only `Did::from_public_key`
*emits* base58btc; nothing requires an incoming DID to use it.

`impl Deserialize for Did` (`lib.rs:183`) routes through `from_str`. So every alternate encoding is
reachable by ordinary deserialization — wire, API body, or persisted JSON read back off disk.

**Measured** against the real type at `bca3dd0e`:

| Measurement | Result |
|---|---|
| Multibase encodings tried for one Ed25519 key | 22 |
| Encodings accepted by `Did::from_str` | **22** |
| Distinct `Did` strings for one key | **22** |
| `HashMap<Did, _>` entries holding those 22 | **22** |
| All 22 resolve to the same `VerifyingKey` bytes | yes |
| `serde_json` round-trip of that map preserves all 22 | yes |

So **every ICN public key has at least 22 valid, storable, wire-acceptable spellings today**, and
they are 22 distinct keys in every `Did`-keyed structure. Under I7 they become one. The merge
factor for a fully exercised aliasing attack is 22:1, not 2:1.

### 2.2 The aliasing is attacker-chosen, and signatures do not constrain it

`SignedEnvelope::verify_classical` (`icn/crates/icn-net/src/envelope.rs:267`) derives the
verifying key from **`self.from.to_verifying_key()`** — the DID *as spelled on the wire* — and
`to_verifying_key` also uses `multibase::decode`. `SignedEnvelope::new` takes `from` as a parameter
independent of the signing keypair.

**Measured**: a sender signed sequence `1` under its canonical DID and sequence `1` again under a
base16 spelling of the same key. `CANONICAL_ENVELOPE_VERIFIES=true`,
`ALIAS_ENVELOPE_VERIFIES=true`, `ENVELOPE_FROM_FIELDS_DIFFER=true`.

A sender therefore chooses its own key's spelling per message, and every spelling authenticates.
This makes the aliasing surface adversarial rather than accidental, which changes the verdict for
every store keyed off a wire-supplied DID.

### 2.3 Anchor-derived DIDs are not round-trippable

`Did::from_anchor_id` (`icn/crates/icn-identity/src/anchor.rs:194`) base58btc-encodes 32 bytes of
**SHA-256 anchor id** and wraps them with `new_unchecked`, bypassing validation. The result is
syntactically indistinguishable from a key-derived DID — `is_anchor_did()` returns `true` for both.

But `Deserialize` validates as Ed25519. A hash is a valid curve point only about half the time.

**Measured** over 200 deterministic anchor ids: **90 round-tripped, 110 failed** (55% failure).
An anchor-derived DID can be written to durable storage and then fail to deserialize on read-back.
This is a live defect independent of I7 (§10.1).

### 2.4 Reproducing these measurements

The probes are not committed (this tranche adds no code). To reproduce, add a temporary integration
test to `icn-identity` and `icn-net` respectively; the exact sources used are recorded in §11.

---

## 3. The classification frame — what I7 actually moves

I7 changes `Did`'s `Eq` and `Hash`. **It does not change `Display`, `to_string`, `as_str`, or any
serialization.** That distinction decides every row below:

| Construct | Moved by I7? | Consequence |
|---|---|---|
| Durable key built via `format!("…{did}")` / `did.as_str().as_bytes()` | **No** | Physical rows stay distinct — N aliasing rows remain N rows |
| In-memory `HashMap<Did,_>` / `HashSet<Did>` / `Vec<Did>::contains` | **Yes** | Collapses to one entry, last-writer-wins |
| `Vec<String>` de-duplication over DID strings | **No** | Unaffected — stays N |
| Wrapper deriving `Eq`/`Hash` over a `Did` field | **Yes** (inherited) | Moves silently, invisible to a `Did` type search |
| Wrapper with hand-written `Ord` over the string | **No** | Diverges from its own derived `Eq` — see #10 |
| `EntityId(String)` embedding a DID spelling | **No** | Diverges from `Did` after I7 — see #33 |
| Kernel `type Did = String` | **No** | Diverges from `icn-identity::Did` after I7 |

**The migration surface is therefore not "everything containing a DID".** It is precisely the
stores whose **durable rows are keyed by a DID string while their consumers re-key by `Did`** —
where a rebuild collapses N durable rows into one map entry *lossily* — plus the constructs above
that I7 moves out of step with a partner construct it does not move.

Class labels used in the tables (from #2623):

- **A** purely ephemeral · **B** ephemeral but reconstructed from durable state ·
  **C** serialized wholesale · **D** directly a durable key ·
  **E** externally round-tripped through API/wire, then used as a durable key

Verdicts are exactly the three #2623 classes: `SAFE`, `SILENT-MERGE RISK`, `NEEDS MIGRATION`,
plus an explicit `UNRESOLVED` for candidates this pass could not classify on evidence.

---

## 4. Search methodology (re-runnable)

Run from the repository root at `bca3dd0e`. Every class was run independently; no single grep is
load-bearing.

**S1 — structural, type-driven.** Collection types parameterized by `Did`:
`rg -g '*.rs' 'HashMap<\s*(&\s*)?Did\b|BTreeMap<…|DashMap<…|HashSet<…|BTreeSet<…|IndexMap<…|LruCache<…'`
→ 109 sites, 105 outside `tests/`.

**S2 — durable engines.** `rg -l -g '*.rs' '\bsled\b'` → 174 files across 17 crates;
`rg -g '*.rs' -g '*.toml' 'rusqlite|sqlx|rocksdb|redb|heed|lmdb|sqlite'` → no second durable
engine in the Rust workspace. `sled` is the only durable KV engine.

**S3 — the store seam.** `icn-store::Store` (`icn/crates/icn-store/src/lib.rs:312`) is **byte-keyed**
(`get(&self, key: &[u8])`, `put(&self, key: &[u8], …)`). *This is why S1 alone is insufficient*:
by the time a DID reaches storage it is already a `String` or `Vec<u8>`, so a type-driven search
structurally cannot see it. Every later class exists because of this.

**S4 — key-construction sites.** Key-builder functions taking a `Did`:
`rg -g '*.rs' 'fn [a-z_]*key[a-z_]*\([^)]*: *&?Did'` → 30 production builders.
Plus the raw idiom `rg -g '*.rs' 'extend_from_slice\([a-z_]*did[a-z_]*\.as_str\(\)\.as_bytes\(\)\)'`
→ 23 sites.

**S5 — durable roots.** `rg -l -g '*.rs' 'sled::Db|sled::Tree|sled::open|sled::Config'` → 69 files;
ranked by identity-term density with `scan1.py` (§11) to order the read.

**S6 — serialized maps.** An AST-approximating pass (`serde_maps.py`, §11) matching `Did`-keyed
collections inside `#[derive(…Serialize/Deserialize…)]` structs → class **C** members.

**S7 — free-form DID parsing (class E).** `rg -g '*.rs' 'Did::from_str\(|\.parse::<Did>\(\)'`
→ **176 production sites across 20 crates** (47 in `icn-gateway` alone). Every one accepts all 22
spellings.

**S8 — wrapper types (second pass, §9).** `wrappers.py` (§11) matching newtype structs over `Did`,
enum variants holding a `Did`, and structs with a `Did` field → **152 structs**, 5 newtype/variant
wrappers. This class is invisible to S1 and produced findings #10, #26 and #52.

**S9 — non-Rust persistence.** `rg -g '*.ts' -g '*.tsx' -g '*.js' 'localStorage|IndexedDB|AsyncStorage|SecureStore|\.setItem\('`
over `sdk/ web/ website/ apps/` → no DID-keyed durable store; see §9 coverage limits.

**S10 — legacy/compatibility loaders.** `rg -i 'migrat|legacy_|from_legacy|schema_version|LEGACY'`
and targeted reads of the known migration paths (§8).

---

## 5. Inventory — durable keyspaces (class D/E)

Key encoding column reads left to right as the physical byte layout.

| # | Crate | Source + symbol | Storage | Logical key | Physical key encoding | Class | DID enters via | Normalization | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| 1 | icn-net | `replay_guard.rs:1800 make_sender_regime_key` | sled via `Store` | sender regime | `b"…" ‖ did.as_str()` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 2 | icn-net | `replay_guard.rs:1812 make_max_seq_key` | sled via `Store` | replay floor per sender | `b"…" ‖ did.as_str()` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 3 | icn-net | `replay_guard.rs:1828 make_finalized_key` | sled via `Store` | (sender, seq) finalized | `b"…" ‖ did.as_str() ‖ ':' ‖ seq` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 4 | icn-net | `sequence_tracker.rs:325 make_key` | sled via `Store` | (sender, recipient) seq | `pfx ‖ sender ‖ "\|\|" ‖ recipient` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 5 | icn-security | `misbehavior.rs:925 security:reputation:` | sled via `Store` | reputation per DID | `"security:reputation:" ‖ Display` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 6 | icn-security | `misbehavior.rs security:banned:` | sled via `Store` | ban + timestamp | `"security:banned:" ‖ Display` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 7 | icn-security | `misbehavior.rs security:quarantine:` | sled via `Store` | quarantine + ts | `"security:quarantine:" ‖ Display` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 8 | icn-security | `misbehavior.rs security:violation:` | sled via `Store` | violation records | `"security:violation:" ‖ Display` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 9 | icn-ledger | `ledger_impl/balances.rs:233` | sled via `Store` | settlement balances per account | `"ledger:balance:" ‖ serde_json(AccountId)` | D | wrapper `AccountId::Did` | none | **NEEDS MIGRATION** |
| 10 | icn-ledger | `treasury.rs:1112 TREASURY_PREFIX` | sled via `Store` | treasury record | `"ledger:treasury:" ‖ treasury_did` | D | `from_str` / import | none | **SILENT-MERGE RISK** |
| 11 | icn-ledger | `patronage.rs:231 account_key` | sled via `Store` | (coop, member) patronage | `pfx ‖ coop_id ‖ ':' ‖ did.as_str()` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 12 | icn-ledger | `patronage.rs:235 entry_key` | sled via `Store` | (coop, member, ref) entry | `pfx ‖ coop ‖ ':' ‖ did ‖ ':' ‖ ref` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 13 | icn-ledger | `asset_types.rs:306 owner_index_key` | sled via `Store` | assets by owner | `pfx ‖ Display ‖ ':'` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 14 | icn-ledger | `obligation.rs:291 creditor_index_key` | sled via `Store` | obligations by creditor | `pfx ‖ Display ‖ ':'` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 15 | icn-ledger | `obligation.rs:295 debtor_index_key` | sled via `Store` | obligations by debtor | `pfx ‖ Display ‖ ':'` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 16 | icn-ledger | `membership.rs:93 since_key` | sled via `Store` | member-since | `pfx ‖ Display` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 17 | icn-governance | `store.rs:413 vote_key` | sled | (proposal, voter) vote | `"vote:" ‖ proposal ‖ ':' ‖ Display` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 18 | icn-governance | `store.rs:426 vote_index_key` value | sled | voters per proposal | value = JSON `Vec<String>` | D | `to_string` | **string** de-dupe | **NEEDS MIGRATION** |
| 19 | icn-governance | `store.rs:437 delegation_from_index_key` | sled | delegations by delegator | `"index:delegations:from:" ‖ Display` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 20 | icn-governance | `store.rs:441 delegation_to_index_key` | sled | delegations by delegate | `"index:delegations:to:" ‖ Display` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 21 | icn-governance | `steward_store.rs:204 PREFIX_BY_DID` | `Store` backend | steward by operational DID | `pfx ‖ did.as_str()` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 22 | icn-governance | `steward_store.rs:218 PREFIX_BY_HOLDER` | `Store` backend | steward by holder DID | `pfx ‖ did.as_str()` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 23 | apps/governance | `state_store.rs:222 vote_key` | sled | (proposal, voter) vote | `"gov:vote:" ‖ proposal ‖ ':' ‖ Display` | D | wire `from_str` | none | **NEEDS MIGRATION** |
| 24 | apps/governance | `replication_sequence.rs:282 sequence_key` | sled | (author, domain) seq | `pfx ‖ len ‖ author ‖ len ‖ domain` | D | wire `from_str` | length-prefix only | **NEEDS MIGRATION** |
| 25 | icn-gateway | `receipt_store.rs:1090 grant_by_grantee_key` | sled | authority grants by grantee | `pfx ‖ len(tag‖did.as_str()) ‖ …` | D | `from_str` | **none** (see §10.4) | **SILENT-MERGE RISK** |
| 26 | icn-gateway | `listings_mgr.rs:699 interest_index_key` | sled | interest by (listing, from) | `pfx ‖ listing ‖ from_did` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 27 | icn-federation | `attestation_store.rs:38 attestation_key` | `Store` | (member, source coop) attestation | `pfx ‖ did.as_str() ‖ '/' ‖ coop_id` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 28 | icn-federation | `agreement/store.rs:86 party_index_key` | `Store` | agreements by party | `pfx ‖ did.as_str() ‖ '/' ‖ agreement` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 29 | icn-rpc | `auth.rs:508 make_challenge_key` | `Store` | auth challenge per DID | `"auth:challenge:" ‖ to_string()` | D/E | wire `from_str` | none | **SILENT-MERGE RISK** |
| 30 | icn-trust | `lib.rs:569 edge_key` | `Store` | trust edge (source→target) | `pfx ‖ "/edges/" ‖ src ‖ ':' ‖ tgt` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 31 | icn-identity | `personhood_store.rs:160 did_index_key` | `Store` | personhood anchor by DID | `pfx ‖ did.as_str()` | D | `from_str` / anchor | none | **NEEDS MIGRATION** |
| 32 | icn-identity | `commons_store.rs:73 did_index_key` | `Store` | commons holder by DID | `pfx ‖ did.as_str()` | D | `from_str` | none | **SILENT-MERGE RISK** |
| 33 | icn-entity | `sled_registry.rs:120 entity_key` | sled | entity record | `"entity:" ‖ EntityId(String)` | D | `EntityId::from_did` | none | **NEEDS MIGRATION** |
| 34 | icn-entity | `sled_registry.rs:124 membership_key` | sled | (parent, member) membership | `"membership:" ‖ parent ‖ ':' ‖ member` | D | `EntityId::from_did` | none | **NEEDS MIGRATION** |
| 35 | icn-entity | `sled_registry.rs:128 type_index_key` | sled | entities by type | `"type:" ‖ type ‖ ':' ‖ EntityId` | D | `EntityId::from_did` | none | **SILENT-MERGE RISK** |
| 36 | icn-coop | `store.rs:59` member key | sled | (coop, member) | `"member:" ‖ coop_id ‖ ':' ‖ did` | D | `from_str` | none | **SILENT-MERGE RISK** |

## 6. Inventory — reconstructed indexes and serialized maps (class B/C)

These are where I7 actually collapses rows, because the durable rows above are re-keyed by `Did`
on load.

| # | Crate | Source + symbol | Class | Rebuilt from | Collapse behaviour under I7 | Verdict |
|---|---|---|---|---|---|---|
| 37 | icn-net | `replay_guard.rs:525 sequences: HashMap<Did, SequenceWindow>` | B | rows #1–#3 via `parse_max_seq_key` | N windows → 1, last-writer-wins by sled scan order; a **lower** floor can win | **NEEDS MIGRATION** |
| 38 | icn-security | `misbehavior.rs:415–430` four maps, `load_from_store` | B | rows #5–#8 via `did_str.parse::<Did>()` | N penalty rows → 1, surviving score/ban timestamp arbitrary | **NEEDS MIGRATION** |
| 39 | icn-ledger | `ledger.rs:287 cached_balances`, `balances.rs:209 load_cached_balances` | B | row #9 | N balance rows → 1; `save_cached_balances` then writes back only the survivor, **orphaning** the others on disk | **NEEDS MIGRATION** |
| 40 | icn-ledger | `balance.rs:11 compute_all_balances` | B | journal entries (durable) | two aliasing `account_id`s sum into one account | **NEEDS MIGRATION** |
| 41 | icn-ledger | `treasury.rs:215–254` five `HashMap<Did,_>` | B | rows #10 + budgets/rules | N treasuries → 1 | **SILENT-MERGE RISK** |
| 42 | icn-ledger | `freeze.rs:141 frozen: HashMap<Did, FrozenMember>` | B | freeze records | two freeze rows → 1 | **SILENT-MERGE RISK** |
| 43 | icn-governance | `delegation.rs:339/342 delegations_from/to` | B | rows #19–#20 | delegation edges merge; cycle detection input changes | **SILENT-MERGE RISK** |
| 44 | icn-governance | `tally.rs:108/111 vote_map`, `counted` | B | row #17 | `vote_map` keeps last vote; `counted` de-dupes correctly — but pass 1 counts **both** regardless (§10.2) | **SILENT-MERGE RISK** |
| 45 | icn-gossip | `vector_clock.rs:56 SerializedClock { clock: HashMap<Did,u64> }` | C | wire + `icn_encoding` round-trip | `.collect()` silently drops all but the last aliasing entry — causality under-counted, **no error** | **NEEDS MIGRATION** |
| 46 | icn-gossip | `scalability.rs:106 CompressedVectorClock { deltas }` | C | wire | same collapse on a delta clock | **SILENT-MERGE RISK** |
| 47 | icn-gossip | `quotas.rs:105 quotas: HashMap<Did, StorageQuota>` | B | quota records | N quotas → 1; storage accounting merges | **SILENT-MERGE RISK** |
| 48 | icn-gossip | `handlers/blob_nonce_guard.rs:83 peers` | B | nonce windows | replay-nonce windows merge | **SILENT-MERGE RISK** |
| 49 | icn-identity | `sync.rs:69 HashMap<Did, CachedDidDocument>` | B | remote DID documents | two cached documents → 1 | **SILENT-MERGE RISK** |
| 50 | icn-steward | `recovery.rs:162 revoked_dids: HashMap<Did, RevocationRecord>` | B | revocation records | two revocations → 1; a revocation can be **dropped** | **NEEDS MIGRATION** |
| 51 | icn-snapshot | `protocol.rs:141/143 channel_states`, `participant_hashes`; `coordinator.rs:47 participant_states` | C | snapshot payloads | participant entries merge inside a consistency snapshot | **SILENT-MERGE RISK** |
| 52 | icn-net | `topology.rs PeerId(pub Did)` in 4 `BTreeSet` + 1 `HashMap` | A | — | derived `Eq`/`Hash` move to bytes, hand-written `Ord` stays string ⇒ **`Ord`/`Eq` contract violation** (§10.3) | **SILENT-MERGE RISK** |
| 53 | icn-kernel-api | `proofs.rs:490 BTreeMap<Did,u64>` (`RawVectorClockProjection`) | E | wire | projection entries merge on decode | **SILENT-MERGE RISK** |
| 54 | icn-snapshot | `lib.rs:174 vector_clock: HashMap<String,u64>` | C | snapshot payloads | **not moved by I7** — diverges from #45, which is (§10.5) | **UNRESOLVED** |

## 7. Inventory — assessed and found safe (class A)

Purely ephemeral, not reconstructed from durable state, and not serialized. I7 changes their
contents only for the lifetime of a process, which is the intended effect of I7 rather than a
hazard.

**The count is derived, not curated.** Search class S1 found **105** production `Did`-keyed
collection sites. **39** of those lines belong to the class B/C stores enumerated in §6. The
remaining **66** are the `SAFE` set. Re-derive with S1 and subtract the §6 files — the arithmetic
is the audit, so nothing is excluded by assertion. By crate:

| Crate | SAFE sites | Crate | SAFE sites |
|---|---|---|---|
| icn-trust | 16 | icn-gossip | 5 |
| apps/governance | 16 | icn-privacy | 2 |
| icn-net | 12 | icn-ledger | 2 |
| icn-core | 7 | icn-rpc / icn-gateway | 2 |
| icn-governance | 4 | | |

Representative fields, so the shape of the set is legible:

`icn-net`: `rate_limit.rs:1026 buckets`, `blob_registry.rs:137 per_peer_size` + `:355 peer_counts`,
`handlers/mod.rs:37` and `actor/mod.rs:1098 peer_connections`, `actor/mod.rs:1106 relay_proxies`,
`candidate_cache.rs:24 candidates` · `icn-gossip`: `sync.rs:223 states`,
`partition.rs:39 last_seen` + `:356 healing_in_progress`,
`handlers/blob_transfer_state.rs:170 per_peer_counts`, `handlers/provider_registry.rs:156 exclude` ·
`icn-core`: `init_notifications.rs:26 ProfileCache`, `version_tracker.rs:21 peer_versions`,
`upgrade.rs:29` + `upgrade_actor.rs:117/118`, `trust_propagation.rs:93/95` ·
`icn-trust`: `anomaly.rs` traversal sets (`:160/161/320/601/602/715/723/798–801`),
`pathfinder.rs:144 visited`, `multi_graph.rs:280 all_dids`, `lib.rs:872 dids` ·
`icn-governance`: `handle.rs:140/171/172`, `discussion.rs:376 participants`,
`delegation.rs:856 visited` · `apps/governance`: `actor.rs` and `manager.rs` eligible-voter /
excluded-delegator sets · `icn-privacy`: `onion_routing.rs:101 peer_public_keys`, `:365 trust_scores` ·
`icn-gateway`/`icn-rpc`: in-memory `challenges` maps (the durable half is #29) ·
`icn-ledger`: `ledger_impl/witness_ops.rs:82 transaction_parties`.

**Count: 66 sites**, verdict `SAFE`. Every one is process-local: no `Serialize` derive, no
`Store`/sled write path, and no load-from-durable rebuild. Several (`eligible_voters`,
`excluded_delegators`, `visited`, `exclude`) are function parameters or traversal scratch rather
than stores at all; they are counted because S1 returns them and silently dropping grep hits is how
a coverage claim becomes unfalsifiable.

One further genuine `SAFE`, and the reason it is safe is the useful part:

| # | Crate | Symbol | Why safe |
|---|---|---|---|
| 55 | icn-identity | `authority_log::PrincipalKey` (`body.rs:125–150`) | **Already key equality.** `PrincipalKey(VerifyingKey)` is built by `try_from_bytes`, which applies canonical-encoding and weak-point checks, and its doc states N1 "does not inherit the legacy `Did` parser's more permissive ZIP-215 acceptance rules". N1 independently reached the same conclusion this inventory reaches, and rejects hash-derived DIDs explicitly. **This is the precedent N2-A should follow.** |

---

## 8. Legacy and compatibility paths examined

The inventory must cover data current code can *load*, not only data it creates.

| Path | Examined | Finding |
|---|---|---|
| `#2517` replay-state legacy regime (`replay_guard.rs:468`, `:624 MigratingFromLegacy`) | yes | Legacy entries are recognised by the *absence* of `semantic_version`. The migration re-reads legacy rows and re-keys nothing — the DID spelling in the key is carried through verbatim. Legacy rows therefore inherit rows #1–#3's exposure. |
| `legacy_proposal_index_key` backfill (`receipt_store.rs:3869–4017`) | yes | A real legacy→current index rewrite exists and is exercised. It rewrites *index shape*, not key identity, so a re-keying migration has a working precedent to copy but no DID normalization to inherit. |
| `dispatch_evidence_backfill.rs` (icn-gateway) | yes | Backfills by hash, not by DID. No exposure. |
| `Anchor::to_did` / `from_anchor_id` historical rows | yes | §2.3 — 55% are unreadable on deserialization today. Any historical anchor-derived DID in a durable store is already at risk, before I7. |
| `AccountId` untagged deserialization (`entity.rs:917`) | yes | `#[serde(untagged)]` tries `Did` first, then `EntityId(String)`, which accepts **any** string. An invalid DID silently becomes an `AccountId::Entity` rather than erroring. Recorded as a contract observation; the account domain is N2-C′'s and is **not** resolved here. |
| Non-Rust clients (`sdk/`, `web/`, `website/`) | yes | No DID-keyed durable client store found (S9). Clients hold DIDs but do not key persistent structures by them. |
| Second durable engine | yes | None. `sled` only (S2). |

---

## 9. Identifier-domain classification (descriptive only)

Per #2623 §5 and IDENTITY_SEMANTICS §3. **This retypes nothing and does not answer N2-C′.**

| Domain (§3) | Stores in this inventory |
|---|---|
| Cryptographic principal | #1–#4, #21, #25, #29, #30, #37, #48, #52, #55 |
| Context subject (human) | #17, #18, #23, #31, #34, #36, #44 |
| Governed entity (institution) | #27, #28, #33, #35 |
| Infrastructure / node | #45, #46, #47, #51, #53, #54 |
| Account / resource | #9, #10, #11, #12, #13, #14, #15, #16, #39, #40, #41, #42 |
| Unknown / legacy-mixed | #5–#8, #19, #20, #22, #26, #32, #38, #43, #49, #50 |

**Observations, recorded and not acted on.** The *node* domain has no durable type (§3 says so
explicitly), so #45/#54 key node identity by `Did`/`String` with nothing distinguishing it from a
person. `icn-security`'s four keyspaces sit squarely in *unknown/legacy-mixed*: a reputation row
may describe a node, a device or a person and nothing in the key says which — which is why their
merge consequence cannot be reasoned about per-domain and they are classified by exposure instead.

---

## 10. Findings that exist independently of I7

Per #2623's scope rule, these are **documented, not fixed**. Each warrants its own issue; none is
in N2-A0's or N2-A's scope.

### 10.1 Anchor-derived DIDs are unreadable ~55% of the time
`from_anchor_id` writes via `new_unchecked` (no validation); `Deserialize` reads via `from_str`
(Ed25519 validation). Measured 110/200 failures (§2.3). Any durable store holding an anchor-derived
DID in a serde-decoded field can fail to load. **Recommend a separate issue.** Note this is adjacent
to I8/N2-B (#2628, "remove the unvalidated constructors") but is a *live read-path* defect, not a
constructor-hygiene one, and N2-B's stated scope is compatibility-only reads.

### 10.2 A sender can obtain N independent replay windows by re-spelling its own DID
§2.2 proves both envelopes verify. Rows #1–#4 and map #37 are keyed by the wire spelling, so one
principal holds up to 22 independent replay floors. Replay protection is bypassable **today**.
This is the highest-severity finding in this document and it is **not** caused by I7 — I7 would
*reduce* it. **Recommend a separate security issue.**

### 10.3 Vote double-counting, and a re-cast guard that cannot fire
`GovernanceError::AlreadyVoted` (`icn-governance/src/error.rs:33`) has **zero constructors anywhere
in the workspace** — confirmed live, matching IDENTITY_SEMANTICS §7.5's "declared but never
constructed". Separately, `compute_tally_with_delegations` (`tally.rs:114–118`) calls
`tally.add_vote(vote)` unconditionally for every vote in pass 1; `counted` only gates *delegation*
resolution. Two votes from one person under two spellings are **both counted**, and row #18's
de-duplication is over `Vec<String>`, so it does not catch it either. §7.5 anticipated this for a
*re-key*; it is reachable now without one. **Recommend a separate issue.**

### 10.4 `grantee_canonical_bytes` performs no canonicalization
`receipt_store.rs:1071` is named `*_canonical_bytes` but emits a tag byte followed by
`did.as_str()` — the raw spelling. A lookup with a differently-spelled DID misses the grant (fails
closed), while two grants for one principal can coexist (fails open on the count). The **name
asserts a property the function does not have**, which is the kind of thing a later reader trusts.
Minor, but worth its own issue precisely because the name suppresses scrutiny.

### 10.5 `Ord`/`Eq` inconsistency that I7 would *introduce*
`PeerId(pub Did)` (`topology.rs`) derives `PartialEq`/`Eq`/`Hash` — which delegate to `Did` and so
move under I7 — while hand-implementing `Ord` over `self.0.to_string()`, which does not move. After
I7, two aliasing `PeerId`s satisfy `a == b` while `a.cmp(&b) != Equal`, violating the standard
library's `Ord`/`Eq` consistency requirement. `PeerId` is used in four `BTreeSet`s and one
`HashMap` (`topology.rs:23–35`), where an inconsistent `Ord` yields unspecified lookup and
insertion behaviour. **This one is N2-A's to fix, in the same change**, because I7 creates it.

### 10.6 Namespaces that I7 does not reach, and therefore desynchronises
Three constructs embed or duplicate a DID but keep string equality, so I7 moves `Did` out from
under them:

- **`EntityId(String)`** — `EntityId::from_did` (`entity.rs:51`) splices the multibase spelling
  verbatim into `entity:icn:individual:<spelling>`. Rows #33–#35 are durable membership keys.
  After I7, `a == b` as `Did` while `EntityId::from_did(a) != EntityId::from_did(b)` — two member
  rows for one principal, permanently, and I7 alone will not fix it.
- **`icn-kernel-api::Did = String`** (`types.rs:9`) — the alias family I12/N2-H owns. Unaffected by
  I7 by construction.
- **`icn-snapshot::vector_clock: HashMap<String,u64>`** (#54) — the same concept as #45 but
  `String`-keyed, so the two diverge post-I7.

**N2-A must state explicitly which of these it is and is not fixing.** Row #54 is left
`UNRESOLVED` for exactly this reason (§11).

---

## 11. Coverage limits — what this inventory does not establish

Stated plainly, per #2623's sixth acceptance criterion.

1. **No live data was examined.** Every verdict is derived from source and from probes against the
   real types. Whether any deployed store *currently contains* two aliasing spellings is unknown and
   unknowable from the repository. The inventory therefore bounds the **reachable** hazard, not the
   **realised** one. A pre-migration scan of live keyspaces remains necessary and is N2-A's to run.
2. **Row #54 is `UNRESOLVED`.** `icn-snapshot`'s `HashMap<String,u64>` vector clock is DID-shaped
   but `String`-typed. Whether it must be normalized in step with #45 depends on whether the two are
   ever reconciled against each other, which this pass did not trace. Missing evidence: the
   snapshot↔gossip clock reconciliation path.
3. **Value-position DIDs were not exhaustively enumerated.** S8 found 152 structs with a `Did`
   field. Those used as *keys* are covered; those appearing only in *values* are not individually
   listed, on the ground that I7 does not change value semantics. If N2-A adds normalization at the
   parse boundary rather than at `Eq`/`Hash`, that assumption breaks and all 152 need re-review.
4. **`icn-ccl` was not traced.** `Value::Did(Did)` exists in the contract language's value enum.
   Whether contract state persists `Did`-keyed maps was not established.
5. **Test-only stores were excluded by construction** (`/tests/`, `#[cfg(test)]`). A durable store
   reachable only from test harnesses would be missed; the S5 root scan found no such case, but this
   is an exclusion, not a proof.
6. **The 22-encoding figure is a floor, not a ceiling.** It counts what `multibase-0.9.2` supports.
   A dependency bump that adds a base widens the aliasing surface with no code change in this repo.
7. **Probe sources are not committed** — this tranche adds no code. They are retained in the session
   scratchpad and are ~40 lines each; the measurements in §2 are reproducible by re-adding them as
   temporary integration tests to `icn-identity` and `icn-net`.
8. **`sled` iteration order is treated as unspecified.** Every "last-writer-wins" verdict assumes an
   attacker cannot choose the winner. If scan order is in fact lexicographic and stable, some merges
   become *deterministically* attacker-selectable, which would raise several `SILENT-MERGE RISK`
   rows to `NEEDS MIGRATION`. Not verified this pass.

---

## 12. Result

| Verdict | Count |
|---|---|
| `SAFE` | **67** — 66 ephemeral sites (§7) + `PrincipalKey` (#55) |
| `SILENT-MERGE RISK` | **31** — 20 in §5, 11 in §6 |
| `NEEDS MIGRATION` | **22** — 16 in §5, 6 in §6 |
| `UNRESOLVED` | **1** — #54 |
| **Total candidate stores inspected** | **121** — 36 durable keyspaces (§5) + 18 reconstructed/serialized (§6) + 67 safe (§7) |

**Concrete list N2-A inherits** — the 22 `NEEDS MIGRATION` rows: #1, #2, #3, #4, #5, #6, #7, #8,
#9, #17, #18, #23, #24, #31, #33, #34, #37, #38, #39, #40, #45, #50.

**What the evidence says about the mechanism choice.** §11 permits either equality over decoded
bytes or encoding pinned at parse. This inventory does not choose — that is N2-A's decision and its
rationale is N2-A's to state — but it records three constraints the choice must satisfy:

1. Pinning at parse changes what `Did::from_str` *accepts*, which is a wire-compatibility change
   affecting all 176 class-E parse sites, and would make currently-loadable persisted rows
   unloadable. Equality over decoded bytes changes no acceptance and no durable byte.
2. Neither mechanism fixes §10.6 — `EntityId`, the kernel `String` alias, and #54 keep string
   equality either way.
3. Neither mechanism repairs a durable keyspace by itself. Rows #1–#36 are built by `Display`, which
   I7 does not touch, so the physical rows survive I7 unchanged and a **separate re-keying step** is
   required for the 22 rows above. `PrincipalKey` (#55) is the in-repo precedent for the decoded-byte
   form, and `legacy_proposal_index_key` (§8) is the in-repo precedent for the rewrite mechanics.

**§7.5 remains a separate hard gate.** Rows #17, #18, #23 and finding §10.3 are membership and vote
storage. §7.5 requires migration ordering, alias/transition recognition, duplicate-act prevention and
final cutover to be designed before any live re-key. Nothing here discharges that gate, and N2-A
must not treat these rows as ordinary migrations.
