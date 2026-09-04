# N2-A0 — stored-key inventory before `Did` canonicalization (#2623)

**Status:** living — investigation record for the N2-A0 tranche
**Truth class:** descriptive
**Canonical:** no — `docs/architecture/IDENTITY_SEMANTICS.md` owns the semantic contract; this
document owns only the *measured* stored-key surface that gates I7
**Last reviewed:** 2026-08-19 — fresh-context adversarial review (second pass); every headline
measurement re-run, verdict classes made mechanical, missed keyspaces added, #54 resolved
**Source basis:** live `origin/main` at `798c8d54422d9916bca8261d0540fd0755a1fc1f` (the Rust
workspace is byte-identical to `bca3dd0e`, the first pass's basis; only docs/scripts moved)
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
`multibase::decode(encoded_part)` and stores the string **as spelled** (`Ok(Did(s.to_string()))`).
Multibase is *self-describing*: `multibase::decode` reads the first character as a base code and
dispatches across every base the crate supports. The dependency is **`multibase` v0.9.2**, pinned
in `icn/Cargo.lock` (`grep -A1 'name = "multibase"' icn/Cargo.lock`; direct dependency of
`icn-identity` and `icn-trust`); its `multibase::Base` enum declares **24** bases (the
`build_base_enum!` table in the crate's `base.rs`: `Identity`, `Base2`, `Base8`, `Base10`,
`Base16Lower/Upper`, `Base32Lower/Upper/PadLower/PadUpper/HexLower/HexUpper/HexPadLower/HexPadUpper/Z`,
`Base36Lower/Upper`, `Base58Flickr/Btc`, `Base64/Pad/Url/UrlPad`, `Base256Emoji`). Nothing in
`from_str` pins base58btc. Only `Did::from_public_key` *emits* base58btc; nothing requires an
incoming DID to use it.

`impl Deserialize for Did` (`lib.rs:183`) routes through `from_str`; `Serialize` is derived over the
inner string. So every alternate encoding is reachable by ordinary deserialization — wire, API body,
or persisted JSON read back off disk — and survives a round trip unchanged.

**Measured** against the real type (probe §13.4; identical at `bca3dd0e` and `798c8d54`):

| Measurement | Result |
|---|---|
| Multibase bases tried for one Ed25519 key | 24 (every base the crate declares) |
| Accepted by `Did::from_str` unconditionally | **23** |
| Accepted conditionally | **1** — `Identity` (`\0` + raw bytes) parses when the 32 key bytes happen to be valid UTF-8 (0 of 2,000 random keys; parser path proven with a constructed ASCII-valued point) |
| Distinct `Did` strings for one key | **23** (24 for such a key) |
| `HashMap<Did, _>` entries holding those 23 | **23** |
| All 23 resolve to the same `VerifyingKey` bytes | yes |
| `Display`/`to_string` preserves each alternate spelling | yes |
| `serde_json` round-trip preserves each alternate spelling, and the 23-entry map | yes |

So **every ICN public key has at least 23 valid, storable, wire-acceptable spellings today** (the
first pass said 22 — it did not try `Base256Emoji`, which is always accepted, or `Identity`), and
they are 23 distinct keys in every `Did`-keyed structure. Under I7 they become one. The merge
factor for a fully exercised aliasing attack is at least 23:1, not 2:1.

### 2.2 The aliasing is attacker-chosen, and signatures do not constrain it

`SignedEnvelope::verify_classical` (`icn/crates/icn-net/src/envelope.rs:267`) derives the
verifying key from **`self.from.to_verifying_key()`** — the DID *as spelled on the wire* — and
`to_verifying_key` also uses `multibase::decode`. `SignedEnvelope::new` takes `from` as a parameter
independent of the signing keypair. And `canonical_encoding()` (`envelope.rs:403`), the bytes that
are actually signed, covers `sequence ‖ timestamp ‖ payload_type ‖ payload` — **`from` is not
signed** (already noted in #2480). The signature binds the *key*, not the *spelling*.

**Measured** (probe §13.5): a sender signed sequence `5` under its canonical DID and sequence `5`
again under a base16 spelling of the same key — both verify, the `from` fields differ, and the
same-sequence envelope signed by a *different* key does not verify (the control that proves the
signature still binds the key). Then, with **no key material**, a captured envelope had only its
`from` rewritten to the base16 spelling: signature bytes unchanged, still verifies.

Run through the real `ReplayGuard` (`replay_guard.rs`, keyed by `envelope.from`; production caller
`handlers/signed.rs:105 check_replay_only`): canonical seq 5 accepted; a second canonical seq 5
**rejected** ("Replay detected … sequence 5 already seen" — the control); the alias seq 5
**accepted**; `peer_count()` = 2, two independent `max_seq` floors. In persistent mode the store
holds two `replay_max_seq:` and two `replay_finalized:` rows — one per spelling — and a fresh guard
reloads **2** windows. Nothing on the `handle_signed` path requires `envelope.from` to equal the
connection's Hello-bound DID (the connection-level `authenticated` peer is used for rate limiting
only), and the `#2506` self-drop `envelope.from == self.own_did` is string equality too.

So not only does a sender choose its own key's spelling per message — **any party holding a
captured envelope can re-spell it and replay it**, up to 22 more times (23 spellings). This makes
the aliasing surface adversarial rather than accidental, which changes the verdict for every store
keyed off a wire-supplied DID. What the probe does *not* show is that a particular downstream
consumer acts twice on the replayed payload (gossip payloads may be de-duplicated by content hash
further down); it shows the sequence-number replay protection at the envelope layer is bypassable.

### 2.3 Anchor-derived DIDs are not round-trippable

`Did::from_anchor_id` (`icn/crates/icn-identity/src/anchor.rs:194`) base58btc-encodes 32 bytes of
**SHA-256 anchor id** and wraps them with `new_unchecked`, bypassing validation. The result is
syntactically indistinguishable from a key-derived DID — `is_anchor_did()` returns `true` for both.

But `Deserialize` validates as Ed25519. A uniformly random 32-byte string decompresses to a valid
curve point with probability ≈ ½, so the expected failure rate is **about half** — not a corner case.

**Measured** (probe §13.4): over 200 deterministic SHA-256 anchor ids, **90 round-tripped, 110
failed** (the first pass reported this as "55%"; that is sampling noise at n = 200). At n = 20,000
the rate is **9,949 / 20,000 = 49.7%**; through the real `Anchor::from_vui(..).to_did()` path it is
**989 / 2,000 = 49.5%**; and for cooperative treasury DIDs exactly as production constructs them
(`icn-coop/src/actor.rs:493, :788` — `Did::from_anchor_id(derive_treasury_anchor(coop_id) ‖
[0u8; 16])`) it is **2,459 / 5,000 = 49.2%**. An anchor-derived DID can be written to durable
storage and then fail to deserialize on read-back; `icn-ledger/src/treasury.rs:980
load_from_store` does `if let Ok(treasury) = serde_json::from_slice(..)` and **silently skips**
such rows. This is a live defect independent of I7 (§10.1), owned by N2-B / #2628.

### 2.4 Reproducing these measurements

The probes are not committed (this tranche adds no code). To reproduce, add a temporary integration
test to `icn-identity` and `icn-net` respectively; the exact sources used are recorded verbatim in
**§13**. **Delete them before taking any repo-wide count** — they contain `Did::from_str` and
`HashMap<Did, _>` and inflate S1/S7 while present.

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
| Wrapper with hand-written `Ord` over the string, or hand-written `Hash` over the string with derived `Eq` | **No** | Diverges from its own derived `Eq` — see #52 / §10.5 (`PeerId`), and `icn-ccl` `Value` (§10.5) |
| `EntityId(String)` / `StewardId` / `icn-commons` holder id derived from a DID spelling | **No** | Diverges from `Did` after I7 — see #33–#35, #62, #63, #65 / §10.6 |
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
plus an explicit `UNRESOLVED` for candidates that cannot be classified on repository evidence.
Every row also carries a **liveness** note where it matters: *live* (reachable from a production
binary), *dormant* (writer and reader exist in library code but no binary constructs the store —
e.g. `icn-governance`'s `SledGovernanceStore` and `StewardStore`, which nothing outside their own
modules and tests instantiates; the live twins are `apps/governance/src/state_store.rs` and
`icn-commons/src/store.rs`), or *dead* (write path unreachable from any binary). The migration list
N2-A inherits is **`NEEDS MIGRATION` ∧ live**; dormant `NEEDS MIGRATION`-shaped stores are listed
separately so N2-A fixes the shape without a data migration.

### 3.1 Verdict classes — the mechanical rule

The three #2623 classes are assigned by one rule, checkable per row, not by judgement of how
bad a merge would feel. Throughout, "moved by I7" means *changes behaviour when `Eq`/`Hash`
become key equality*; `Display`, `as_str`, `Serialize` and every durable byte are **not** moved.

- **`NEEDS MIGRATION`** — I7 alone leaves the store inconsistent, so a re-key/de-dupe step must
  ship with or before it. Exactly one of:
  **(a)** durable rows keyed by a DID *spelling* **and** at least one production consumer
  re-keys those rows by `Did` (or by a wrapper whose derived `Eq`/`Hash` delegates to `Did`,
  or by `Vec<Did>::contains`) — so after I7 a rebuild collapses N aliasing rows into one entry
  lossily (overwrite / sum / first- or last-writer) while the N durable rows persist and are
  re-read on every start;
  **(b)** a structure serialized wholesale (`Serialize`/`Deserialize` derive) whose map or set is
  keyed by `Did`, so decode collapses aliasing keys silently (wire or disk);
  **(c)** a `String`-keyed, DID-shaped durable structure that a production path parses back into
  `Did` and inserts into a `Did`-keyed map or set;
  **(d)** a durable key derived from the spelling (hash or splice) that sits behind an
  authorization or identity gate which compares by `Did` — after I7 the gate and the key
  disagree and the divergence is *created* by the change.
- **`SILENT-MERGE RISK`** — durable rows (or `String`-keyed DID-shaped structures) keyed by a
  spelling with **no** `Did`-re-keying production consumer. I7 leaves them physically unchanged;
  point lookups by an alternate spelling miss, as they do today. No re-key step is forced by I7
  itself, **but** N2-A's pre-migration scan must still enumerate their aliasing rows, and if
  N2-A chooses the *pin-at-parse* mechanism (§12) every alternate-spelled row in this class
  becomes unreachable. Also in this class: in-memory `Did`-keyed structures that I7 moves out of
  step with a partner construct it does not move (`PeerId`'s `Ord`, #52).
- **`SAFE`** — ephemeral only: no `Serialize`, no store write, no load-from-durable rebuild; or
  already key equality (#55); or a durable write path that is unreachable from any production
  binary (marked *dead* — flagged for deletion, not migration).
- **`UNRESOLVED`** — evidence not obtainable from the repository. After this review: **none**.

A row's class therefore follows from three facts a reviewer can re-check: *is the key durable or
serialized; is it keyed by spelling or by `Did`; does any production path re-key it by `Did`.*
Where a consumer row in §6 re-keys a keyspace row in §5, the **keyspace is counted once** in the
migration list and the consumer row is the evidence of mechanism.

---

## 4. Search methodology (re-runnable)

Run from the repository root (counts below re-verified at `798c8d54`, **with no probe files present**
— §13.4/§13.5 contain `Did::from_str` and `HashMap<Did, _>` and inflate S1/S7 while they exist).
Every class was run independently; no single grep is load-bearing. S1–S10 are the first pass;
S11–S14 are the second-pass classes that found what S1–S10 missed.

**S1 — structural, type-driven.** Collection types parameterized by `Did`:
`rg -g '*.rs' 'HashMap<\s*(&\s*)?Did\b|BTreeMap<\s*(&\s*)?Did\b|DashMap<\s*(&\s*)?Did\b|HashSet<\s*(&\s*)?Did\b|BTreeSet<\s*(&\s*)?Did\b|IndexMap<\s*(&\s*)?Did\b|LruCache<\s*(&\s*)?Did\b'`
→ **108** sites, **0** under a `tests/` directory (the first pass's "105 outside tests" does not
reproduce: nothing is excluded by path). One of the 108 (`icn-kernel-api/src/proofs.rs:490`) is the
kernel `type Did = String` alias, not `icn_identity::Did` — which is why a `BTreeMap<Did, _>`
can compile at all (`icn_identity::Did` has no `Ord`) — so **107** are real. The first pass also
silently dropped the three `LruCache<Did, _>` hits from its tally (`icn-federation
attestation_store.rs:22`, `icn-trust precompute.rs:34`, `icn-trust trust_cache.rs:39`); they are
counted here (§7).

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
ranked by identity-term density (**§13.1**) to order the read.

**S6 — serialized maps.** An AST-approximating pass (**§13.2**) matching `Did`-keyed
collections inside `#[derive(…Serialize/Deserialize…)]` structs → class **C** members.

**S7 — free-form DID parsing (class E).** `rg -g '*.rs' -g '!**/tests/**' 'Did::from_str\(|\.parse::<Did>\(\)' -c .`
→ **168 sites outside `tests/` directories across 20 workspace members** (15 crates, 4 apps,
1 binary; **46** in `icn-gateway`, 22 `icn-ledger`, 20 `icn-rpc`, 18 `icn-core`; 183 including
`tests/`). The first pass's "176 / 47" does not reproduce under any exclusion tried and is
withdrawn. Every one accepts all 23 spellings. These are *parse* sites, not sinks; the sinks they
reach are the §5 keyspaces.

**S8 — wrapper types.** **§13.3** matching newtype structs over `Did`, enum variants holding a
`Did`, and structs with a `Did` field → **147 distinct structs** carrying a `Did` field and **5**
distinct newtype/enum-variant wrappers (`PeerId`; the `Did` / `Person` / `Query` / `Remote`
variants). This class is invisible to S1 and produced findings #25 (`Grantee::Person`) and #52
(`PeerId`); `AccountId::Did` led the first pass to #9, though #9's physical key turned out to be
`serde_json(Did)`, not `AccountId`.

**S9 — non-Rust persistence.** `rg -g '*.ts' -g '*.tsx' -g '*.js' 'localStorage|IndexedDB|AsyncStorage|SecureStore|\.setItem\('`
over `sdk/ web/ website/ apps/` → no DID-keyed durable store; see §9 coverage limits.

**S10 — legacy/compatibility loaders.** `rg -i 'migrat|legacy_|from_legacy|schema_version|LEGACY'`
and targeted reads of the known migration paths (§8).

**S11 — persistence-first (second pass).** Start from every durable root (S5's 69 files plus every
`icn_store::Store` consumer and every `sled::Db` holder in `apps/` and `bins/`), enumerate its key
builders, and ask of each key whether it embeds a DID spelling — directly or through `EntityId`,
`StewardId`, `member_id`, `voter`, `author`, `owner`, `holder`, `grantee`, `assignee`,
`treasury_did`, `claims.sub`. This is the pass that found §5's review additions: S1 cannot see a
key that is already a `String` or `Vec<u8>` (S3), and S4's `fn *key*(… &Did)` pattern misses
builders that take `&str`, `String` or a wrapper.

**S12 — consumer-first (second pass).** Every `scan(` / `load_*` / `restore_*` / `rebuild_*`
followed by `Did::from_str` or `.parse::<Did>()` and an `insert` into a `Did`-keyed map or set
(`rg -n -A12 'fn (load|restore|rebuild)' … | rg 'from_str|parse::<Did>'`). This is what resolved
#54 and found the `icn-net` snapshot restore (#57) — the consumer side that decides `NEEDS
MIGRATION` under §3.1.

**S13 — file-backed durable state (second pass).** `icn-snapshot/src/lib.rs` field by field (every
`String`-keyed map and what restores it), `save_snapshot`/`load_snapshot` callers, `std::fs::write`
/ `File::create` / `serde_json::to_writer` with DID-shaped content, keystore and data-dir writers.
The only DID-keyed file-backed state is the snapshot (§8).

**S14 — liveness.** For every store in §5/§6, whether any production binary constructs it
(`rg 'TypeName::(new|open|with_store)'` outside the defining module and `tests/`). This is what
reclassified `icn-governance`'s `SledGovernanceStore`/`StewardStore` rows as dormant and four
`icn-ledger`/`icn-store`/`icn-gateway` stores as dead.

---

## 5. Inventory — durable keyspaces (class D/E)

Key encoding column reads left to right as the physical byte layout. The *consumer* column is the
§3.1 test: a production path that re-keys these rows by `Did` makes the row `NEEDS MIGRATION`;
"none" means the rows stay physically separate under I7. No write path anywhere re-encodes a DID
canonically before persisting (S11: `Did::from_public_key` has 42 non-test call sites and every one
takes a keypair or `VerifyingKey` directly; zero re-derive from a parsed `Did`; no
`canonical()`/`normalize()` helper exists), so the old *Normalization* column was "none" for
every row and is replaced by the consumer column.

**Rows #1–#3 have since been discharged** for the `ReplayGuard` — see the §10.2 status note.
They are left as measured here, because this table is the record of what was true at the basis
commit and the fix is a later event.

| # | Crate | Source + symbol | Storage | Logical key | Physical key encoding | Class | DID enters via | `Did`-re-keying consumer (§3.1) | Live? | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | icn-net | `replay_guard.rs:1800 make_sender_regime_key` | sled via `Store` | sender regime | `b"…" ‖ did.as_str()` | D | wire `from_str` | #37 — `load_persisted_state :713` → `parse_sender_regime_key :1806` → `sequences.entry(did)` | live | **NEEDS MIGRATION** |
| 2 | icn-net | `replay_guard.rs:1812 make_max_seq_key` | sled via `Store` | replay floor per sender | `b"…" ‖ did.as_str()` | D | wire `from_str` | #37 — `:735–760` → `parse_max_seq_key :1819` → `window.max_seq = entry.max_seq` (overwrite; last `sled` key wins, a **lower** floor can win) | live | **NEEDS MIGRATION** |
| 3 | icn-net | `replay_guard.rs:1828 make_finalized_key` | sled via `Store` | (sender, seq) finalized | `b"…" ‖ did.as_str() ‖ ':' ‖ seq` | D | wire `from_str` | #37 — `:1000–1013` → `parse_finalized_key :1837` → `window.finalized.insert(seq)` into the collapsed window | live | **NEEDS MIGRATION** |
| 4 | icn-net | `sequence_tracker.rs:325 make_key` | sled via `Store` | (sender, recipient) outgoing seq | `pfx ‖ sender ‖ "\|\|" ‖ recipient` | D | recipient = Hello `from_str`; sender = local | same module `:98 cache: HashMap<(Did,Did),u64>` via `load_and_apply_safety_gap :158–193` (prod `icn-core …/init_send_callback.rs:84`) — overwrite; a **lower** outgoing sequence can win ⇒ nonce-counter regression for that pair (tuple key, invisible to S1) | live | **NEEDS MIGRATION** |
| 5 | icn-security | `misbehavior.rs:911 KEY_PREFIX_REPUTATION` (written `:927`) | sled via `Store` | reputation per DID | `"security:reputation:" ‖ Display` | D | wire `from_str` | #38 — `load_from_store :988` → `parse::<Did>() :994` → `reputation_scores.insert` (prod `apps/trust-app/src/init.rs:61`); `save_to_store :925` then writes back only the survivor | live | **NEEDS MIGRATION** |
| 6 | icn-security | `misbehavior.rs:912 KEY_PREFIX_BANNED` | sled via `Store` | ban + timestamp | `"security:banned:" ‖ Display` | D | wire `from_str` | #38 — `:1015` → `banned.insert` (overwrite; ban timestamp arbitrary) | live | **NEEDS MIGRATION** |
| 7 | icn-security | `misbehavior.rs:913 KEY_PREFIX_QUARANTINE` | sled via `Store` | quarantine + ts | `"security:quarantine:" ‖ Display` | D | wire `from_str` | #38 — `:1037` → `quarantined.insert` | live | **NEEDS MIGRATION** |
| 8 | icn-security | `misbehavior.rs:914 KEY_PREFIX_VIOLATION` | sled via `Store` | violation records | `"security:violation:" ‖ Display` | D | wire `from_str` | #38 — `:1059` → `violations.insert` (whole `Vec` replaced, not merged) | live | **NEEDS MIGRATION** |
| 9 | icn-ledger | `ledger.rs:233 BALANCE_PREFIX`; writer `ledger_impl/balances.rs:233 save_cached_balances` | sled via `Store` | settlement balances per account | `"ledger:balance:" ‖ serde_json::to_string(&Did)` — the *quoted spelling*; **not** `AccountId` (first pass was wrong: `icn-ledger` never uses `AccountId`) | D | API/RPC journal `AccountDelta.account_id: Did` (`types.rs:143`) | #39 `load_cached_balances :209` (overwrite + orphan on save); #40 `compute_all_balances` (sum) | live | **NEEDS MIGRATION** |
| 10 | icn-ledger | `treasury.rs:1112 TREASURY_PREFIX` | sled via `Store` | treasury record | `"ledger:treasury:" ‖ treasury_did` | D | local `Did::from_anchor_id` only — `GatewayTreasuryManager::register_treasury` with a caller DID has no HTTP caller | #41 — `treasury.rs:1021 insert` on load (prod `ledger-app/init.rs:139`): NM-shaped, but no wire/API spelling can reach the writer today | live | **SILENT-MERGE RISK** (flips to NM the day a wire spelling reaches the writer; its `treasury_did` is anchor-derived and ~50% unreadable, §10.1) — *2026-09-04:* the loader now classifies every primary row before adopting any (migration gate §4.2, #2627 M1): an alias pair, an unreadable key or value, a key/body spelling disagreement and a disagreeing `idx:coop` row each refuse hydration with a typed, payload-free error before the first map is touched, whether or not the startup gate ran (`icnctl` opens the same store with none); registered in `n2a_keyspaces` as `icn-ledger/treasury` (prefix through the DID scheme, `did_ends_key`), fail closed, and consumed by the #2700 startup gate under the same collision unit. No byte re-keyed; no merge rule. The ~50% unreadable anchor-derived rows now refuse instead of vanishing — still #2628's defect, now visible |
| 11 | icn-ledger | `patronage.rs:231 account_key` | sled via `Store` | (coop, member) patronage | `pfx ‖ coop_id ‖ ':' ‖ did.as_str()` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 12 | icn-ledger | `patronage.rs:235 entry_key` | sled via `Store` | (coop, member, ref) entry | `pfx ‖ coop ‖ ':' ‖ did ‖ ':' ‖ ref` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 13 | icn-ledger | `asset_types.rs:306 owner_index_key` | sled via `Store` | assets by owner | `pfx ‖ Display ‖ ':'` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 14 | icn-ledger | `obligation.rs:291 creditor_index_key` | sled via `Store` | obligations by creditor | `pfx ‖ Display ‖ ':'` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 15 | icn-ledger | `obligation.rs:295 debtor_index_key` | sled via `Store` | obligations by debtor | `pfx ‖ Display ‖ ':'` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 16 | icn-ledger | `membership.rs:93 since_key` | sled via `Store` | member-since | `pfx ‖ Display` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 17 | icn-governance | `store.rs:413 vote_key` | sled (`SledGovernanceStore`) | (proposal, voter) vote | `"vote:" ‖ proposal ‖ ':' ‖ Display` | D | wire `from_str` | `compute_tally :650` → `VoteTally::from(list_votes)` counts every row; #44 | **dormant** — `SledGovernanceStore` is constructed nowhere outside its module and tests; live twin is #23 | **SILENT-MERGE RISK** (dormant; NM-shaped) |
| 18 | icn-governance | `store.rs:426 vote_index_key` value | sled (`SledGovernanceStore`) | voters per proposal | value = JSON `Vec<String>`, **string** de-dupe (`:606–614`) | D | `to_string` | same as #17 | dormant | **SILENT-MERGE RISK** (dormant) |
| 19 | icn-governance | `store.rs:437 delegation_from_index_key` | sled (`SledGovernanceStore`) | delegations by delegator | `"index:delegations:from:" ‖ Display` | D | `from_str` | none (#43 is **not** rebuilt from these rows) | dormant | **SILENT-MERGE RISK** (dormant) |
| 20 | icn-governance | `store.rs:441 delegation_to_index_key` | sled (`SledGovernanceStore`) | delegations by delegate | `"index:delegations:to:" ‖ Display` | D | `from_str` | none | dormant | **SILENT-MERGE RISK** (dormant) |
| 21 | icn-governance | `steward_store.rs:204 PREFIX_BY_DID` | `StewardStoreBackend` | steward by operational DID | `pfx ‖ did.as_str()` | D | `from_str` | none | **dormant** — no backend is constructed in production; live twin is #64 (`icn-commons`) | **SILENT-MERGE RISK** (dormant) |
| 22 | icn-governance | `steward_store.rs:218 PREFIX_BY_HOLDER` | `StewardStoreBackend` | steward by holder DID | `pfx ‖ did.as_str()` | D | `from_str` | none | dormant | **SILENT-MERGE RISK** (dormant) |
| 23 | apps/governance | `state_store.rs:222 vote_key` | sled | (proposal, voter) vote | `"gov:vote:" ‖ proposal ‖ ':' ‖ Display` | D | wire/HTTP `from_str` | `actor.rs:3862–3864` `direct_voters: HashSet<&Did>`, `vote_by_did: HashMap<&Did,&Vote>` in `apply_delegation_to_tally` (close path `:2277`) — `vote_by_did` last-writer; the direct double-count (§10.3) persists; `manager.rs:~4771` guard is string-eq | live | **NEEDS MIGRATION** (§7.5 gate) |
| 24 | apps/governance | `replication_sequence.rs:282 sequence_key` | sled | (author, domain) seq | `pfx ‖ len ‖ author ‖ len ‖ domain` | D | local author DID (the node's own `from_public_key`) | none — state is `HashMap<Vec<u8>, PairState>` keyed by the raw key bytes (`:110`) | live | **SILENT-MERGE RISK** (first pass said NM — no `Did` re-key exists) |
| 25 | icn-gateway | `receipt_store.rs:1090 grant_by_grantee_key` | sled | authority grants by grantee | `pfx ‖ len(tag ‖ did.as_str()) ‖ valid_from ‖ grant id` (ADR-0014 layout, §10.4) | D | `from_str` | none | live | **SILENT-MERGE RISK** — *2026-09-04:* dispositioned as a **derived projection** of the canonical `adr0014:grant:<uuid>` records (migration gate §11.5, #2627 M2): a Person query decodes every Person-tagged spelling to its principal, proves each candidate against the primary `AuthorityGrant` before returning it, and de-duplicates by `AuthorityGrantId` so two spellings of one grant are one grant and two grant ids stay two grants; Entity-tagged rows keep exact-string identity and are never principalized; malformed projection rows refuse rather than vanish. Registered in `n2a_keyspaces` as `icn-gateway/adr0014_grant_by_grantee` under a third structural descriptor, `LengthPrefixedTagged { principal_tag: 0x01 }` (a big-endian `u32` frame) — the two existing regions cannot read a binary key — `Equivalent`/`Established`, consumed by the #2700 startup gate, which one ordinary Person grant previously made refuse. No byte re-keyed |
| 26 | icn-gateway | `listings_mgr.rs:699 interest_index_key` | sled | interest by (listing, from) | `pfx ‖ listing ‖ from_did` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 27 | icn-federation | `attestation_store.rs:38 attestation_key` | `Store` | (member, source coop) attestation | `pfx ‖ did.as_str() ‖ '/' ‖ coop_id` | D | `from_str` | #59 `LruCache<Did,_>` read-through (`:80–113`) — a per-lookup cache, not a start-up rebuild | live | **SILENT-MERGE RISK** (re-key the cache in N2-A, #59) — *2026-09-04:* the cache is gone and every read classifies the namespace by `Did` equality, with a same-source alias pair refused rather than merged and a revocation removing every spelling of its pair atomically (#2704, #2703); registered in `n2a_keyspaces` as `icn-federation/attestations` (member spelling anchored, source an opaque discriminator), fail closed, and consumed by the #2700 startup gate under the same collision unit |
| 28 | icn-federation | `agreement/store.rs party_index_key` | `Store` | agreements by party | `pfx ‖ did.as_str() ‖ '/' ‖ agreement` | D | `from_str` | none | live | **SILENT-MERGE RISK** — *2026-09-04:* dispositioned as a **derived projection** of `federation/agreements/` (migration gate §11.3, #2707): the store answers a party lookup from the canonical `parties` under `Did` equality, retires superseded rows on replacement, deletes every spelling on delete, refuses malformed projection rows and unreadable or key/value-inconsistent canonical rows, and can rebuild the projection; registered in `n2a_keyspaces` as `icn-federation/agreement_party_index` (party spelling anchored, agreement id an opaque discriminator), `Equivalent`/`Established`, and consumed by the #2700 startup gate under the same collision unit. No byte re-keyed |
| 29 | icn-rpc | `auth.rs:508 make_challenge_key` | `Store` | auth challenge per DID | `"auth:challenge:" ‖ to_string()` | D/E | wire `from_str` | `auth.rs:433 load_challenges` → `:378 challenges: HashMap<Did, Challenge>` (prod `init_rpc.rs:148`) — overwrite; TTL 5 min, fail-closed | live | **NEEDS MIGRATION** (low — TTL-bounded, fail-closed) |
| 30 | icn-trust | `lib.rs:569 edge_key` | `Store` | trust edge (source→target) | `pfx ‖ "/edges/" ‖ src ‖ ':' ‖ tgt` | D | `from_str` | `lib.rs:869 get_all_known_dids → HashSet<Did>` (and `multi_graph.rs:280`), then per-survivor spelling-prefix `get_outgoing_edges` (`apps/trust-app/src/service_tokio.rs:193, 757`) — the dropped spellings' edges vanish from enumeration/provenance input | live | **NEEDS MIGRATION** |
| 31 | icn-identity | `personhood_store.rs:160 did_index_key` | `Store` | personhood anchor by DID | `pfx ‖ did.as_str()` | D | written under the locally generated `anchor.to_did()` (canonical spelling); `link_did` has only a `#[cfg(test)]` caller | none — point lookup `get_by_did :273` | live | **SILENT-MERGE RISK** (first pass said NM — no `Did` re-key; the exposure here is the anchor-DID read defect §10.1, not I7) |
| 32 | icn-identity | `commons_store.rs:73 did_index_key` | `Store` | commons holder by DID | `pfx ‖ did.as_str()` | D | `from_str` | none | live | **SILENT-MERGE RISK** |
| 33 | icn-entity | `sled_registry.rs:120 entity_key` | sled | entity record | `"entity:" ‖ EntityId(String)` — `EntityId::from_did` splices the spelling into `entity:icn:individual:<spelling>` | D | `EntityId::from_did` | none (`EntityId` is `String`-equal) | live | **SILENT-MERGE RISK** — **not moved by I7**; §10.6 namespace decision (first pass said NM, which cannot differ from #34/#35) |
| 34 | icn-entity | `sled_registry.rs:124 membership_key` | sled | (parent, member) membership | `"membership:" ‖ parent ‖ ':' ‖ member` | D | `EntityId::from_did` | none | live | **SILENT-MERGE RISK** — not moved by I7 (§10.6) |
| 35 | icn-entity | `sled_registry.rs:128 type_index_key` | sled | entities by type | `"type:" ‖ type ‖ ':' ‖ EntityId` | D | `EntityId::from_did` | none | live | **SILENT-MERGE RISK** — not moved by I7 (§10.6) |
| 36 | icn-coop | `store.rs:59` member key | sled | (coop, member) | `"member:" ‖ coop_id ‖ ':' ‖ did` | D | `from_str` | none | live | **SILENT-MERGE RISK** |

### 5.1 Durable keyspaces the first pass missed (found by S11–S14)

Numbering continues after §6/§7 so earlier references stay stable. Most are point-lookup splits
(`SILENT-MERGE RISK`); #65, #68 and #69 are `NEEDS MIGRATION`.

| # | Crate | Source + symbol | Storage | Logical key | Physical key encoding | Class | DID enters via | `Did`-re-keying consumer (§3.1) | Live? | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 62 | icn-entity | `sled_registry.rs:132 member_of_index_key` (written `:400`, `:651`) | sled | (member, parent) reverse index | `"member_of:" ‖ member EntityId ‖ ':' ‖ parent EntityId` | D | `EntityId::from_did` | none | live | **SILENT-MERGE RISK** — not moved by I7 (§10.6; sibling of #33–#35) |
| 63 | icn-commons | `store.rs:827` steward primary key (`StewardRecord::new`, `steward.id().to_hex()`) | `Store` (sled; live via `CommonsHandle::with_sled_path`, `icn-core lifecycle.rs:120`) | steward record | `"commons/stewards/" ‖ hex(StewardId)` where `StewardId::from_did = SHA-256("steward:" ‖ did.as_str())` (`icn-governance/src/steward.rs:29–38`) | D | API `from_str` | none — `[u8;32]` id, `String` cache | live | **SILENT-MERGE RISK** — **not moved by I7** (hash over the spelling); two spellings ⇒ two steward records permanently (§10.6) |
| 64 | icn-commons | `store.rs:43–44 STEWARD_BY_DID_PREFIX` (written `:828/:836`) | `Store` | steward by holder DID | `"commons/stewards/by_did/" ‖ holder_did.to_string()` | D | API `from_str` | none (`get_steward_by_did`, `api/steward/mod.rs:161`; `LruCache<String,_>`) | live — the live twin of dormant #21/#22 | **SILENT-MERGE RISK** |
| 65 | icn-commons | `inner.rs:346–362 update_display_name` | `Store` | weak-holder record (minted on first display-name update) | `"commons/holders/" ‖ hex(SHA-256(did.to_string()))` + a by_did row | D | path `{did}` (`api/members.rs:141`) gated by `caller_did != did` and `coop.members.iter().any(\|m\| m.did == did)` — **`Did` equality** | the gate itself: after I7 the `Did`-equality gate passes for an aliased spelling while the spelling-hashed store misses ⇒ a **second** `CommonsHolderRecord` is minted for one key | live | **NEEDS MIGRATION** (§3.1 clause (d) — I7 makes the authorization equality and the storage key disagree) |
| 66 | icn-commons | `store.rs:37 ANCHOR_BY_DID_PREFIX` (`:480, :530, :557`) | `Store` | anchor id by DID | `"commons/anchors/by_did/" ‖ anchor.to_did().to_string()` | D | written from `Anchor::to_did()` (canonical spelling, ~50% unreadable §10.1); read with a wire-parsed `Did` (`commons_mgr.rs:113`, `api/commons/mod.rs:131`) | none (`LruCache<String,_>` keyed by hex id) | live | **SILENT-MERGE RISK** |
| 67 | icn-commons | `store.rs:39 HOLDER_BY_DID_PREFIX` (`:608, :662, :691`) | `Store` | holder id by DID | `"commons/holders/by_did/" ‖ holder_did.to_string()` | D | wire `from_str` (`inner.rs:301 create_holder_from_anchor`) | none — but it is an **authorization gate**: `icn-gateway authority.rs:44, 89 require_office_in_jurisdiction`, `api/membership/mod.rs:152, 195, 719`, `api/steward/mod.rs:517` do `get_holder_by_did(caller_did)` → not-found ⇒ `AuthorizationFailed` | live | **SILENT-MERGE RISK** (fail-closed split) |
| 68 | icn-ledger | `freeze.rs:44 FREEZE_PREFIX` (`persist_frozen :397`, `remove_frozen :404`) | `Store` (`ledger.rs:373 FreezeManager::with_store`) | frozen member | `"ledger:frozen:" ‖ Display` | D | `freeze(did)` from governance/gateway | #42 — `load_from_store :378–384` → `frozen.insert(record.did)`; enforcement `ledger.rs:2907, 2915 is_frozen`; post-I7 `unfreeze(alias)` removes the merged entry but deletes only that spelling's row ⇒ the freeze **resurrects on restart** | live | **NEEDS MIGRATION** (the durable side of #42, absent from the first pass's §5) |
| 69 | icn-ledger | `ledger.rs:239 CLEARED_VOLUME_PREFIX`; `ledger_impl/balances.rs:248–292` | `Store` | cleared volume per (account, currency) | `"ledger:cleared_volume:" ‖ Display ‖ ':' ‖ currency` | D | journal `delta.account_id` | `load_cleared_volume_index :248` → `Did::from_str` → `cleared_volume_index: HashMap<(Did,String),i64>` (`ledger.rs:291`, loaded `:414`, saved `:1721` — write-back orphans the losers; tuple key, invisible to S1) | live | **NEEDS MIGRATION** |
| 70 | icn-ledger | `treasury/audit.rs:289, 317` | `Store` | treasury audit trail | `"ledger:treasury:audit:" ‖ Display ‖ ':' ‖ ts ‖ ':' ‖ id` (read by `scan_reverse_paginated` on the `Display` prefix) | D | `record.treasury_did: Did` | none | live | **SILENT-MERGE RISK** (audit history split across spellings) — *2026-09-04:* not dispositioned by #2627 M1, which registers only the primary `ledger:treasury:<did>` row; these rows embed the spelling as key structure and remain *uncovered* to the #2700 startup gate (migration gate §4.2), as does `idx:budgets:<did>:<budget>` (#84) — the next treasury follow-up |
| 71 | apps/trust-app | `sequence.rs:72, 94, 136` (`:54` issuer side) | `Store` (`service_tokio.rs:58`) | receiver replay floor per attestation issuer | `"trust/sequences/receiver/" ‖ Display` | D | `attestation.issuer: Did` from gossip (`service_tokio.rs:425, 497`) — any spelling | none (read/write by key only) | live | **SILENT-MERGE RISK** — a **fourth instance of the §10.2 replay-window class** (the first pass listed only icn-net #1–#4) |
| 72 | icn-community | `types.rs:68 Community.members: HashMap<MemberId = String, Member>`, persisted wholesale `store.rs:24 COMMUNITY_PREFIX ‖ id` → `serde_json(Community)` | `Store` (live: `bins/icnd community_wiring.rs`, gateway `community_mgr.rs`) | members per community | `String`-keyed map inside the record ("Can be DID or CooperativeId") | C (String) | `req.member_id: String` (`api/communities.rs:216–241`), gated by `claims.sub != req.member_id` (string) | none (all operations by `String` key) | live | **SILENT-MERGE RISK** — not moved by I7; diverges from any `Did`-typed membership view (§10.6) |
| 73 | icn-store | `lib.rs:72–99 ReplicaMetadata` (`put_replica_metadata :911`, keyed by content hash) | `Store` | replica accounting per content hash | value: `replicas: Vec<ReplicaInfo { peer_did: String }>` de-duped by string `==` (`:186–189`); `last_challenged/last_verified: HashMap<String,u64>` | C (String) | gossip `offering_peer.to_string()` (`icn-gossip handlers/replica.rs:57, 106, 169`) | none by `Did` — `icn-core replication/manager.rs:201 replicas.len()`, `:382 HashSet<String>` | live | **SILENT-MERGE RISK** — not moved by I7; a peer announcing under N spellings counts as N replicas today |
| 74 | icn-gateway | `identity_mgr.rs:15 DID_DOCUMENT_PREFIX` (`:75, :112, :130`) | `Store` (`server.rs:1573 new_with_storage`) + `LruCache<String,_>` | DID document (multi-device verification methods) | `"did_doc:" ‖ did.as_str()` | D | path `{did}` parsed `from_str` after a `claims.sub != did_str` **string** gate (`api/devices.rs:85–106`) | none (lookups by `as_str`) | live | **SILENT-MERGE RISK** (fail-closed split: a device registered under spelling A is not a valid signer for requests presenting spelling B) |
| 75 | icn-gateway | `notification_store.rs:95, 116` (`register_device` / `get_device_tokens`) | sled (`NotificationStore.db`) | device tokens by owner | `"idx_device_owner:" ‖ did.to_string() ‖ ':' ‖ token` | D | JWT `web::ReqData<Did>` → `to_string()` (`api/notifications.rs:37–46`) | none (prefix scan by the current spelling; `notification_processor.rs:293` re-`to_string()`s the recipient) | live | **SILENT-MERGE RISK** (split: devices registered under one spelling invisible to another) |
| 76 | icn-gateway | `notification_store.rs:141, 159, 191, 236` (`store_notification`, `get_notifications`, `get_unread_count`, `mark_all_read`) | sled | in-app notifications by recipient | `"idx_notif_recipient:" ‖ recipient String ‖ ':' ‖ created_at ‖ ':' ‖ id` | D/E | `InAppNotification.recipient: String` from event payloads (`notification_processor.rs:604`); read side `did.to_string()` of the JWT principal | none | live | **SILENT-MERGE RISK** |
| 77 | apps/ledger-app | `recurring.rs:182, 188` (+ `list_by_owner :141–151`) | sled (`Db` from gateway/icnd) | recurring settlements by owner | `"idx_owner:" ‖ owner String ‖ ':' ‖ id` | D/E | `RecurringPayment.owner: String` = gateway `claims.sub` (`api/recurring_settlements.rs:104`) — the JWT `sub` is minted as `did.to_string()` (`auth.rs:405`), so the wire spelling propagates | none (ownership checks are string compares `:154, 184, 233`) | live | **SILENT-MERGE RISK** |
| 78 | apps/ledger-app | `escrow.rs:187, 191, 198, 201` (+ `list_by_user :145–170`) | sled | escrows by creator / beneficiary | `"idx_escrow_creator:" ‖ creator ‖ ':' ‖ id`, `"idx_escrow_beneficiary:" ‖ to_account ‖ ':' ‖ id` | D/E | request body + `claims.sub` (`api/escrow.rs:96, 137, 286`) | none (`escrow.creator != user_did` string compare `:146, 295`) | live | **SILENT-MERGE RISK** |
| 79 | apps/ledger-app | `budgets.rs:193, 199` (+ `list_by_owner :171–180`) | sled | budgets by owner | `"idx_budget_owner:" ‖ owner ‖ ':' ‖ id` | D/E | `Budget.owner: String` = `claims.sub` (`api/budgets.rs:101`) | none | live | **SILENT-MERGE RISK** |
| 80 | apps/ledger (`icn-ledger-actor`, ← `icn-core lifecycle.rs:979`) | `resource_access.rs:24, 35, 58, 75` (`SledResourceAccessStore::key`, tree `exec:resource`) | sled (`<store_path>/resource_access/`, `init.rs:45–48`) | resource access grants by grantee | `"access:" ‖ resource_type ‖ ':' ‖ grantee_did String` | E | `ResourceEffect::GrantAccess { grantee_did: String }` (kernel-api `effects.rs:448`) via `governance_executor.rs:760–815` — a free-form wire string **never parsed as `Did`** | none (get/revoke by raw string) | live | **SILENT-MERGE RISK** (class E: the string is never validated as a DID at all) |
| 81 | apps/governance | `manager.rs:310–325 assignee_idx_key`, reader `:443 list_by_assignee` | sled | action items by assignee | `"action_item_by_assignee:" ‖ did.as_str() ‖ ':' ‖ domain ‖ ':' ‖ item` | D | `ActionItem.assignee: Option<Did>` (`icn-governance action_item.rs:107`) from HTTP/governance wire `from_str` | none (`digest_overdue_items :5441`, `list_work_for_person :5557` prefix-scan by `as_str`) | live | **SILENT-MERGE RISK** (the person digest / "my work" views miss items assigned under another spelling) |
| 82 | apps/governance | `manager.rs:582–587, 872–877, 1062–1071` | sled | structures / activities / programs by parent entity | `"structure_by_entity:" ‖ parent_entity_id ‖ ':' ‖ id` (and `activity_by_entity:`, `program_by_entity:`) | E | `parent_entity_id: String` verbatim from HTTP (`http/handlers.rs:5037`), never validated as `EntityId` or `Did` | none | live | **SILENT-MERGE RISK** (low — only if an individual `entity:icn:individual:<did>` is ever a parent; parents are institutions in practice) |
| 83 | icn-gateway (+ apps/governance writer) | `receipt_store.rs:2291–2296` (write), `:2317–2335` (read); v2 `receipt_backend.rs:926–940 put_meeting_attendance_v2` → `opaque_by_key_key :1680` | sled (gateway receipt store) | meeting attendance by (meeting, attendee) | `MEETING_ATTENDANCE_BY_PAIR ‖ len‖meeting_id ‖ len‖attendee_did ‖ …`; opaque `(class, meeting_id, attendee_did)` | E | `MeetingAttendanceReceipt.attendee_did: String` set from HTTP `req.did` (`http/handlers.rs:4807` → `manager.rs:5878`), matched to `meeting.attendees[].did` by string `==` | none | live | **SILENT-MERGE RISK** (low; a receipt index, matched by string throughout) |
| 84 | (dead write paths) | `icn-ledger use_access.rs:1020–1048` second `SledResourceAccessStore` (only `tests/use_access_integration.rs`); `icn-ledger dynamic_limits.rs:215` (`set_dynamic_limit_manager` test-only); `icn-ledger progressive_limits.rs:241` (`set_progressive_limit_manager` has no caller); `icn-ledger treasury/budgets.rs:405 persist_budget_index` (`TREASURY_IDX_BUDGETS_PREFIX`: one `put`, zero scans — write-only); `icn-store peer_cache.rs:257 make_key` (`PeerCache`/`CachedPeer` have no users outside `icn-store`); `icn-gateway commons_store.rs:37, 39, 44` (`SledCommonsStore` twin of #64/#66/#67 — re-exported, never constructed); `icn-entity sled_registry.rs:828–877 rel:from/rel:to` (`store_relationship` has zero production callers); `icn-entity coop_entity_map.rs:85 entity_coop:` (institution ids by construction); `apps/membership` (`icn-membership-app`) duplicates of #33–#36 and #62 — the crate has **no dependents** | sled / `Store` | — | DID-spelling-keyed | D | various | none reachable | **dead / unwired** | **SAFE** (dead — flag for deletion, not migration; an N2-A scan that finds rows here is finding test residue) |

## 6. Inventory — reconstructed indexes and serialized maps (class B/C)

These are where I7 actually collapses rows, because durable rows are re-keyed by `Did` on load or
on decode. A consumer row and its keyspace row are **one** entry in the migration list (§12).

| # | Crate | Source + symbol | Class | Rebuilt from / serialized how | Collapse behaviour under I7 | Live? | Verdict |
|---|---|---|---|---|---|---|---|
| 37 | icn-net | `replay_guard.rs:525 sequences: HashMap<Did, SequenceWindow>` | B | rows #1–#3 via `parse_*_key` | N windows → 1; survivor = last key in `sled` lexicographic order — the `Base256Emoji` (`🚀…`) spelling sorts after even the canonical `z…`, so the survivor is **attacker-selectable**; a **lower** floor can win | live | **NEEDS MIGRATION** (consumer of #1–#3; one keyspace) |
| 38 | icn-security | `misbehavior.rs:415–430` four `HashMap<Did,_>`, `load_from_store :988` | B | rows #5–#8 via `did_str.parse::<Did>()` | N penalty rows → 1 (overwrite; a whole violation `Vec` replaced); `save_to_store` then orphans the losers on disk | live | **NEEDS MIGRATION** (consumer of #5–#8) |
| 39 | icn-ledger | `ledger.rs:287 cached_balances`, `balances.rs:209 load_cached_balances` | B | row #9 | N balance rows → 1; `save_cached_balances` writes back only the survivor, **orphaning** the others; `verify_integrity` then spuriously mismatches | live | **NEEDS MIGRATION** (consumer of #9) |
| 40 | icn-ledger | `balance.rs:11 compute_all_balances` | B | journal entries (durable) | two aliasing `account_id`s **sum** into one account | live | **NEEDS MIGRATION** (with #9) |
| 41 | icn-ledger | `treasury.rs:215–254` five `HashMap<Did,_>` | B | rows #10 + budgets/rules/shares (`:1021 insert` on load) | N treasuries → 1 — but no wire spelling reaches #10's writer today | live | **SILENT-MERGE RISK** (consumer of #10; see its flag) — *2026-09-04:* closed with #10 (migration gate §4.2, #2627 M1): the five maps are populated only after the whole primary keyspace and the cooperative index classify cleanly, and an alias pair refuses; the `persist_treasury` write-back addresses the spelling the row was loaded under |
| 42 | icn-ledger | `freeze.rs:141 frozen: HashMap<Did, FrozenMember>` | B (+ D: its durable rows are #68) | #68 via `load_from_store :378` | overwrite on load; `unfreeze(alias)` deletes one spelling's row only → freeze resurrects on restart | live | **NEEDS MIGRATION** (first pass said SMR; its durable side was missing) |
| 43 | icn-governance | `delegation.rs:339/342 delegations_from/to` | **A** (not B) | — (no `Serialize`, no store, no load path; "rebuilt from #19–#20" was unsupported) | in-memory merge only — I7's intended effect | live | **SAFE** (first pass said SMR) |
| 44 | icn-governance | `tally.rs:108/111 vote_map`, `counted` | B | row #17 (dormant) | `vote_map` keeps the last vote; pass 1 counts **both** regardless (§10.3) | **dormant** — only `#[cfg(test)]` callers | **SILENT-MERGE RISK** (dormant; NM-shaped with #17) |
| 45 | icn-gossip | `vector_clock.rs:56 SerializedClock { clock: HashMap<Did,u64> }` | C | wire (`GossipMessage`) **and** durable (`GossipEntry.clock`) | custom `Deserialize :280–292`: serde map decode keeps the **last** duplicate silently — causality under-counted, **no error** | live | **NEEDS MIGRATION** (§3.1 clause (b)) |
| 46 | icn-gossip | `scalability.rs:106 CompressedVectorClock { deltas: HashMap<Did,i64> }` | C | — (zero users outside `scalability.rs`; `lib.rs:75 pub use` only) | same collapse, structurally | **dormant** | **NEEDS MIGRATION** (clause (b), dormant — fix the shape, no data step) |
| 47 | icn-gossip | `quotas.rs:105 quotas: HashMap<Did, StorageQuota>` | **A** | — (`StorageQuotaManager` has no `Serialize`, no store/scan/load; "quota records" do not exist) | in-memory | live | **SAFE** (first pass said SMR) |
| 48 | icn-gossip | `handlers/blob_nonce_guard.rs:83 peers` | **A** | — (no `Serialize`, no store) | in-memory | live | **SAFE** (first pass said SMR) |
| 49 | icn-identity | `sync.rs:69 DidDocumentCache { HashMap<Did, CachedDidDocument> }` | **A** | — (`#[derive(Debug, Clone)]`, no persistence) | in-memory | live | **SAFE** (first pass said SMR) |
| 50 | icn-steward | `recovery.rs:162 revoked_dids: HashMap<Did, RevocationRecord>` | **A** | — (`RecoveryService` has no `Serialize`, no store, no load path) | in-memory; the collapse is I7's intended effect | live | **SAFE** (first pass said NM — a false positive) |
| 51 | icn-snapshot | `protocol.rs:141/143 channel_states`, `participant_hashes`; `coordinator.rs:47 participant_states` | **A** (not C) | — (these structs are `#[derive(Debug, Clone)]` only; the serde types in the module are `SnapshotId`/`SnapshotMessage`) | in-memory | live | **SAFE** (first pass said class C / SMR) |
| 52 | icn-net | `topology.rs:42–55 PeerId(pub Did)` in 4 `BTreeSet` + 1 `HashMap` | A (partner) | — | derived `Eq`/`Hash` move to bytes, hand-written `Ord` over `to_string()` stays ⇒ **`Ord`/`Eq` contract violation** (§10.5) | live | **SILENT-MERGE RISK** (not durable; N2-A must fix it in the same change) |
| 53 | icn-kernel-api | `proofs.rs:490 BTreeMap<Did,u64>` (`RawVectorClockProjection`) — **`Did` here is `crate::types::Did = String`** (`types.rs:9`); `icn-kernel-api` does not depend on `icn-identity`, and `icn_identity::Did` has no `Ord` so this could not compile otherwise | — (String) | wire | **not moved by I7** (String keys; `from_entries` keeps the max per key) | live | **SAFE** / not moved (first pass said class E SMR — wrong type; I12/N2-H family, §10.6) |
| 54 | icn-snapshot → icn-gossip | `lib.rs:174 GossipState.vector_clock: HashMap<String,u64>` (snapshot file, `save_snapshot :470`, written at shutdown `icn-core shutdown.rs:109`) | C (String on disk, `Did`-keyed on restore) | export = `to_string()` projection of the `Did`-keyed clock (`gossip.rs:1298–1301`); restore `gossip.rs:1369–1382 restore_state`: `Did::from_str(&did_str)` → `self.clock.insert(did, count)` (prod `icn-core supervisor/init_gossip.rs:383`) | N clock entries → 1 on restore (overwrite; `HashMap`-iteration survivor, so a **lower** count can win); and one unparseable DID aborts the whole restore (`.context(..)?`) | live | **NEEDS MIGRATION** (§3.1 clause (c)) — **RESOLVED** (first pass: UNRESOLVED) |

### 6.1 Structures the first pass missed (found by S11–S14)

| # | Crate | Source + symbol | Class | Rebuilt from / serialized how | Collapse behaviour under I7 | Live? | Verdict |
|---|---|---|---|---|---|---|---|
| 56 | icn-governance / apps/governance | `membership.rs:32–34 MembershipSource::StaticList(Vec<Did>)` (serde-derived), persisted wholesale inside the domain record (`icn-governance store.rs:405 domain_key` — dormant; **live:** `apps/governance state_store.rs:192 "gov:domain:"`) | C (`Vec`, not a map) | API add-member (`manager.rs:3345–3358`: `contains` guard then `push`) | a `Vec` decode is **not** lossy — N aliasing entries survive. Post-I7: `contains` (`manager.rs:4730`, `resolver.rs:113/143`, `handlers.rs:130/1207/1310`, `rehearsal.rs:755`) becomes alias-tolerant; `position(..)+remove` (`:3355–3356`) removes the *first* equal entry; `eligible_count = len()` (`actor.rs:2247–2248`, the quorum denominator) still counts N — a pre-existing over-count I7 does not change; `eligible_voters: HashSet<Did>` (`handlers.rs:1326`) collapses | live | **SILENT-MERGE RISK** (membership storage — **§7.5 gate**; N2-A needs a de-dupe pass for denominators) |
| 57 | icn-snapshot → icn-net | `lib.rs:243 NetworkState.peer_connections: HashMap<String, PeerConnectionInfo>` and legacy `:249 peer_x25519_keys: HashMap<String,[u8;32]>` (snapshot file) → `actor/mod.rs:827 restore_state` (prod `icn-core supervisor/lifecycle.rs:645`) | C (String on disk, `Did`-keyed on restore) | `Did::from_str :834` → `connections_write.insert(did, info) :848` into `:1098 peer_connections: HashMap<Did, PeerConnectionInfo>`; legacy arm `:854–860 entry(did).or_insert_with` | N peer records → 1 (overwrite; legacy arm first-wins); the x25519 / ML-KEM material of the dropped spelling is lost; `peer_addresses` (`:254`) is exported empty and never restored | live | **NEEDS MIGRATION** (clause (c)) — the first pass listed `actor/mod.rs:1098` as SAFE ("no load-from-durable rebuild"), which is false |
| 58 | icn-snapshot → icn-gossip | `lib.rs:182 GossipState.subscriptions: HashMap<String, Vec<String>>` → `gossip.rs:1469–1489 restore_state` (`Did::from_str :1481`; `if !sub_list.contains(&did) { push }` `:1487–1488`) into `gossip.rs:108 subscriptions: HashMap<String, Vec<Did>>` | C (String on disk) | snapshot of wire-learned subscribers | `Vec<Did>::contains` de-dupe: N subscriber spellings → 1 (first wins). Identity-only — no per-subscriber state is lost and routing is `Did`-keyed, so the collapse is benign | live | **SILENT-MERGE RISK** (identity-only de-dupe; no data step) |
| 59 | icn-federation | `attestation_store.rs:22 cache: RwLock<LruCache<Did, Vec<FederatedTrustAttestation>>>` read-through over rows #27 (`:80–113`) | B (lazy cache, not a start-up rebuild) | rows #27 by spelling-prefix scan on miss | first-loader-wins staleness: post-I7 a lookup under spelling B hits the slot filled by spelling A's prefix scan, so B's own durable rows are invisible until eviction / `:60` invalidation; no durable row is lost or orphaned | live | **SILENT-MERGE RISK** (no data step; N2-A must key the cache by decoded bytes or bypass it) — *2026-09-04:* resolved by bypass; the cache no longer exists (#2704) |
| 60 | icn-net | `session.rs:58 SessionManager.connections: HashMap<String, quinn::Connection>` (keyed by `from.to_string()`, `:906–912`) **vs** `actor/mod.rs:1098 peer_connections: HashMap<Did,_>` — both filled from the same Hello `from: Did` (`handlers/hello.rs:234–273`), joined in `actor/messages.rs:474–491 send_message_to_peer` (`Did`-keyed `get`, then `connections().find(\|(peer_did,_)\| peer_did == did.as_str())`); also `actor/mod.rs:1352`, `handlers/onion.rs:61`, `handlers/peer_exchange.rs:132` | A (partner pair) | — | I7 moves the `Did` map (alias-tolerant `get`) but not the `String` map (`== as_str()` misses) ⇒ post-I7 "connection info found, QUIC connection not found" for an alias spelling — the same shape as #52 | live | **SILENT-MERGE RISK** (partner-invariant desync; fix alongside #52 in N2-A, §12.1 item 6) |
| 61 | icn-ccl | `types.rs:110–131 enum Value` derives `PartialEq, Eq` (`Value::Did(Did)` moves) but hand-implements `Hash` (`:200–220`) and hashes `Value::Did` as `format!("{did:?}")` — a string, which does not move; `Value::Set(HashSet<Value>)` (`:128`) is used for `participants` and `in` checks (`interpreter.rs:403–410, 484–485`) | A (partner) | contract state is in-memory only (`runtime.rs:25`); `Value::Set` is persisted inside `Contract.state_vars[].initial_value` via `icn_encoding` (`ast.rs:69`); `ContractRegistry` persists `participants: Vec<Did>` value-position and `owner: String` | post-I7 `HashSet<Value>` violates the `Hash`/`Eq` contract for `Value::Did`: `List.contains` (Eq, moves) and `Set.contains` (Hash, does not) disagree | live | **SILENT-MERGE RISK** (partner invariant; fix in N2-A) — closes §11 item 4: `icn-ccl` persists no `Did`-keyed map, but its value type is a partner type |

## 7. Inventory — assessed and found safe (class A)

Purely ephemeral, not reconstructed from durable state, and not serialized. I7 changes their
contents only for the lifetime of a process, which is the intended effect of I7 rather than a
hazard.

**The count is derived, not curated.** Search class S1 returns **108** `Did`-keyed collection
lines (none under a `tests/` directory). Attribute each line, not each file:

| S1 lines | Where they go |
|---|---|
| 1 | `icn-kernel-api/src/proofs.rs:490` — the kernel `type Did = String` alias (#53); not `icn_identity::Did` |
| 39 | consumers of `NEEDS MIGRATION` / `SILENT-MERGE RISK` rows in §5/§6: `replay_guard.rs:26, 525` (#37) · `misbehavior.rs:415–430` (#38) · `ledger.rs:287, 2374`, `ledger_impl/balances.rs:67` (#39) · `balance.rs:11, 12, 74` (#40) · `treasury.rs:215–254` (#41) · `freeze.rs:141` (#42) · `tally.rs:108, 111, 165, 166` (#44) · `vector_clock.rs:50, 56, 272, 500` (#45) · `scalability.rs:106` (#46) · `apps/governance/src/actor.rs:3862, 3864` (#23) · `icn-rpc/src/auth.rs:378` (#29) · `icn-trust/src/lib.rs:872`, `multi_graph.rs:280` (#30) · the `peer_connections` `Arc` in `actor/mod.rs:178, 1098`, `handlers/mod.rs:37, 171`, `actor/connection.rs:213, 402` (#57) · `attestation_store.rs:22` (#59) |
| 10 | rows the review found `SAFE` but which are listed in §6 so the correction is visible: `delegation.rs:339, 342` (#43) · `quotas.rs:105` (#47) · `blob_nonce_guard.rs:83` (#48) · `sync.rs:69` (#49) · `recovery.rs:162` (#50) · `snapshot protocol.rs:125, 141, 143`, `coordinator.rs:47` (#51) |
| **58** | **the `SAFE` set below** |

Re-derive with S1 and subtract the lines above — the arithmetic is the audit, so nothing is
excluded by assertion. (The first pass reported "105 → 39 → 66": it attributed by *file*, dropped
the three `LruCache<Did, _>` lines, and counted `actor/mod.rs:1098`, `icn-rpc auth.rs:378` and
`icn-trust lib.rs:872` as safe — each of which is in fact the consumer that makes its keyspace
`NEEDS MIGRATION`.) By crate:

| Crate | SAFE sites | Crate | SAFE sites |
|---|---|---|---|
| icn-trust | 16 | icn-governance | 5 |
| apps/governance | 14 | icn-gossip | 5 |
| icn-core | 7 | icn-privacy | 2 |
| icn-net | 6 | icn-ledger | 2 |
| icn-gateway | 1 | | |

Representative fields, so the shape of the set is legible:

`icn-net`: `rate_limit.rs:1026 buckets` + `:1187`, `blob_registry.rs:137 per_peer_size` + `:355 peer_counts`,
`actor/mod.rs:1106 relay_proxies`, `candidate_cache.rs:24 candidates` · `icn-gossip`: `sync.rs:223 states`,
`partition.rs:39 last_seen` + `:356 healing_in_progress`,
`handlers/blob_transfer_state.rs:170 per_peer_counts`, `handlers/provider_registry.rs:156 exclude` ·
`icn-core`: `init_notifications.rs:26 ProfileCache`, `version_tracker.rs:21 peer_versions`,
`upgrade.rs:29` + `upgrade_actor.rs:117/118`, `trust_propagation.rs:93/95` ·
`icn-trust`: `anomaly.rs` traversal sets (`:160/161/320/601/602/638/673/715/723/798–801`),
`pathfinder.rs:144 visited`, `trust_cache.rs:39` and `precompute.rs:34` (`LruCache`s — ephemeral,
though post-I7 `TrustCache` serves a score computed over spelling-A edges to a spelling-B lookup) ·
`icn-governance`: `handle.rs:140/171/172`, `discussion.rs:376 participants`,
`delegation.rs:856 visited` · `apps/governance`: `actor.rs` and `manager.rs` / `http/handlers.rs`
eligible-voter / excluded-delegator sets · `icn-privacy`: `onion_routing.rs:101 peer_public_keys`,
`:365 trust_scores` · `icn-gateway`: `auth.rs:160` in-memory `challenges` (the `icn-rpc` twin is
store-backed and is #29) · `icn-ledger`: `ledger_impl/witness_ops.rs:82, 163 transaction_parties`.

**Count: 58 sites**, verdict `SAFE`. Every one is process-local: no `Serialize` derive, no
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
| `Anchor::to_did` / `from_anchor_id` historical rows | yes | §2.3 — about half are unreadable on deserialization today (49.2% for cooperative treasury DIDs as produced). Any historical anchor-derived DID in a durable store is already at risk, before I7; `treasury.rs:980` skips them silently (#2628). |
| `AccountId` untagged deserialization (`entity.rs:917`) | yes | `#[serde(untagged)]` tries `Did` first, then `EntityId(String)`, which accepts **any** string. An invalid DID silently becomes an `AccountId::Entity` rather than erroring. Recorded as a contract observation; the account domain is N2-C′'s and is **not** resolved here. |
| Non-Rust clients (`sdk/`, `web/`, `website/`) | yes | No DID-keyed durable client store found (S9). Clients hold DIDs but do not key persistent structures by them. |
| Second durable engine | yes | None among databases. `sled` only (S2) — but **the `icn-snapshot` file is a second durable DID-keyed medium** (S13): `save_snapshot`/`load_snapshot` (`icn-snapshot/src/lib.rs:470–497`), written at shutdown (`icn-core supervisor/shutdown.rs:109`), restored at `init_gossip.rs:383` / `lifecycle.rs:645`. Its `GossipState.vector_clock` (#54) and `NetworkState.peer_connections` / legacy `peer_x25519_keys` (#57) are `String`-keyed on disk and re-keyed by `Did` on restore (`NEEDS MIGRATION`); `subscriptions` (#58) is an identity-only de-dupe; `topics[].access_control` (`Participants:[did,…]`) is a value-position `Vec<Did>`; `peer_addresses` is exported empty and never restored. |
| Other file-backed state (S13) | yes | `icn-identity` keystore (`keypair.age`) is one keypair; `icn-core config/genesis.rs` `initial_dids` / `founding_members` and `bins/icnctl` JSON exports (`governance_setup.json`, charter, DID-doc, appliance manifest) are value lists with no Rust reader that keys by them; `icn-core config/ledger.rs:212 quorum_witnesses` (TOML) → `WitnessPolicy::Quorum { witnesses: Vec<Did> }` is a value set (operator-authored; two spellings of one witness would inflate `M` — config hygiene, not a store); `icn-store` blob store and `icn-gossip` chunk files are keyed by hash. |
| Canonical re-encoding on any write path (S11) | yes | **None.** 42 non-test `Did::from_public_key` sites, every one fed by a keypair/`VerifyingKey`; zero re-derive a `Did` from a parsed `Did`; the gateway JWT `sub` is minted as `did.to_string()` (`auth.rs:405`), so a wire spelling propagates into every `claims.sub`-keyed row (#77–#79). |
| Crates with no production dependents | yes | `apps/membership` (`icn-membership-app`) duplicates #33–#36 and #62 but nothing depends on it — excluded as dead (#84). `icn-ledger-actor` ← `icn-core`; `icn-ledger-app` ← `icnd`, `icn-gateway`; `icn-governance-actor` ← gateway, `icnd`, `icnctl`, `icn-core`; `icn-trust-app` ← `icnd`, `icn-core`; `icn-community` ← `icnd`. |

---

## 9. Identifier-domain classification (descriptive only)

Per #2623 §5 and IDENTITY_SEMANTICS §3. **This retypes nothing and does not answer N2-C′.**

| Domain (§3) | Stores in this inventory |
|---|---|
| Cryptographic principal | #1–#4, #21, #25, #29, #30, #37, #48, #52, #55, #57, #59, #60, #71, #74 |
| Context subject (human) | #17, #18, #23, #31, #34, #36, #44, #56, #62, #65, #66, #67, #75, #76, #81, #83 |
| Governed entity (institution) | #27, #28, #33, #35, #63, #64, #72, #82 |
| Infrastructure / node | #45, #46, #47, #51, #53, #54, #58, #73 |
| Account / resource | #9, #10, #11, #12, #13, #14, #15, #16, #39, #40, #41, #42, #68, #69, #70, #77, #78, #79, #80 |
| Unknown / legacy-mixed | #5–#8, #19, #20, #22, #26, #32, #38, #43, #49, #50, #61, #84 |

**Observations, recorded and not acted on.** The *node* domain has no durable type (§3 says so
explicitly), so #45/#54 key node identity by `Did`/`String` with nothing distinguishing it from a
person. `icn-security`'s four keyspaces sit squarely in *unknown/legacy-mixed*: a reputation row
may describe a node, a device or a person and nothing in the key says which — which is why their
merge consequence cannot be reasoned about per-domain and they are classified by exposure instead.

---

## 10. Findings that exist independently of I7

Per #2623's scope rule, these are **documented, not fixed**; none is in N2-A0's or N2-A's scope.
The review's disposition of each is recorded at the end of its entry (owner issue, or why no issue).

### 10.1 Anchor-derived DIDs are unreadable about half the time
`from_anchor_id` writes via `new_unchecked` (no validation); `Deserialize` reads via `from_str`
(Ed25519 validation). Measured ≈ 50% failure (§2.3: 110/200, 9,949/20,000, 989/2,000 via
`Anchor::from_vui`, and **2,459/5,000 for cooperative treasury DIDs exactly as `icn-coop`
constructs them**). Any durable store holding an anchor-derived DID in a serde-decoded field can
fail to load; `icn-ledger/src/treasury.rs:980 load_from_store` skips such rows **silently**, so
about half of cooperative treasuries would drop out of the treasury manager's maps on reload;
`icn-gossip/src/gossip.rs:1380 restore_state` is the opposite failure mode — one unparseable DID
aborts the whole gossip restore. This is a *live read-path* defect, not constructor hygiene.
**Disposition:** owned by I8 / N2-B (#2628, "compatibility-only reads come first … closes the
deserialization-path defect"); the measurements above are recorded **on #2628** (comment of
2026-08-19) rather than in a duplicate issue, with the note that the compat-read half may deserve
to be pulled ahead of N2-A.

### 10.2 Any party can re-spell a captured envelope and replay it
§2.2 proves both envelopes verify *and* that the signature does not cover `from`, so a third party
with no key material can rewrite `from` to an alternate spelling and the `ReplayGuard` opens a fresh
window for it. Rows #1–#4 and map #37 are keyed by the wire spelling, so one principal's traffic has
up to 23 independent replay floors, and `apps/trust-app/src/sequence.rs` (#71) is a fourth instance
of the same class for attestation issuers. Replay protection at the envelope-sequence layer is
bypassable **today**. This is the highest-severity finding in this document and it is **not**
caused by I7 — equality-over-bytes would *reduce* it (one in-memory window per key) but not close
it (the durable rows #1–#3 are spelling-keyed and `from` stays unsigned); pin-at-parse or signing
`from` closes it. **Disposition:** no existing issue owned it (#2480 observed "`from` is not in the
signed bytes" and concluded it did not matter; it did not consider spellings). **Filed by this
review as #2640** (`epic:trust-hardening`, `security-review`).

#### 10.2 status — closed at the replay-protection boundary (2026-08-19, #2640)

The measurements above stand as taken; this records what changed underneath them.

**Fixed, in `icn-net` only.** The `ReplayGuard`'s identity is now the sender's decoded Ed25519
key (`SenderPrincipal`), not the wire spelling. In memory, map #37 is keyed by it. Durably, rows
**#1 (`make_sender_regime_key`)**, **#2 (`make_max_seq_key`)** and **#3 (`make_finalized_key`)**
are written under the principal's canonical base58btc spelling, and a migration pass at
`load_persisted_state` collapses pre-existing spelling-distinct rows onto that one key **wherever
one key can carry their combined meaning**: high-water rows merge to the **maximum** only inside a
single `(semantic_version, sender_regime)` group — the unit within which two numbers are
comparable at all — provenance rows join to the **strongest established** regime, and finalized
sequences take the **union**. The canonical row
is flushed before any alias row is retired. A re-spelled captured envelope is therefore rejected by
the replay guard, and the self-DID drop in `handlers/signed.rs` (§10.2's "related sub-instance")
now compares keys rather than strings.

**Rows #1 and #2 are also joined *across* keyspaces, under a rule of their own.** Row #2 carries a
number together with a `sender_regime` field naming the namespace that produced it; row #1 is one
version-less `u32` naming the namespace the sender was last known to have established, written at
state transitions and deliberately outliving the number beside it (`cleanup()` retires row #2 and
keeps row #1). Row #1 therefore settles the common aged-out case, where it is the only evidence —
but where a *current-version* row #2 has already placed the number in a different namespace, both
facts are true and they disagree, and row #1 may re-establish the regime without re-tagging the
number (#2644). Durable provenance meeting a legacy-tagged current-version floor enters the
ordinary sender-regime migration rather than reinterpreting that floor as a durable bound; a
promotion discards the retained number only when the namespace being retired is the one that
produced it. Detaching a number from its namespace fails in whichever direction the detachment
runs: relabelling a legacy floor as durable turns an honest peer's legitimate low sequences into
scored `Violation::ReplayAttack` events, while discarding a durable floor under a fossil
transition record hands an authenticated sender back every sequence that floor rejected.

**Canonicalization is deliberately partial; two shapes stay physically distinct.** There is exactly
one canonical key per `SenderPrincipal`, so a merged row can record only one interpretation. Where a
principal's rows carry two, converging them destructively would lose one, so the pass writes nothing
and deletes nothing in these cases:

- **Readable rows spanning several semantic versions** (`canonicalize_max_seq_rows`).
  `semantic_version` selects *how* a persisted number is to be read, not how large it is, so "most
  restrictive version" is not a safe tie-break here: the legacy version wins that comparison, and
  collapsing onto it would replace a current-version durable floor with a legacy row whose number
  the load pass discards — leaving a floor of 0 once the bounded migration hold expires.
- **Readable rows spanning several sender regimes, within one version** (#2644). The same argument
  one axis over, and the sharper case: `sender_regime` names the namespace that produced the
  number, and numbers from two namespaces are not comparable at all. An earlier pass resolved such
  a pair to a single `TransitionToDurableV1` row carrying `max(durable, legacy)` — a state that
  never existed, in which a *durable* high-water of 10 wore the legacy label, so the promotion
  ending that migration discarded it and every durable sequence at or below 10 became replayable.
  There is no correct scalar to collapse them into, so they are not collapsed.

  The load pass groups by `(principal, semantic_version, sender_regime)` on the same rule and
  **composes the groups' effects** onto one window instead, through `HighWaterEvidence`: each floor
  stays under the namespace that produced it (`NumericNamespace`), legacy-namespace numbers beside
  a durable floor contribute a bounded `MigratingSenderRegime` obligation rather than a number, and
  holds join by `PeerHold::stronger_of`. The composition is order-independent, which matters
  because the order `sled` hands the rows over is a property of the spellings an attacker picked.
- **A group whose canonical row is itself unreadable** (`install_canonical_row`). Writing the merge
  over it would erase the evidence that quarantines the sender and replace it with a floor derived
  only from the spellings that could be read. Every readable alias in that group is left standing
  and the load pass joins them — for row #1 through a provenance-specific join that ranks
  `DurableV1` above a superseded `TransitionToDurableV1`, because the latter installs a
  migration hold whose promotion retires the retained number, and a fossil row must not impose
  that on a principal a durable sibling already proved (#2644).

**Consequence, and it is a real cost.** Alias rows left by any of these cases are never retired, and
`cleanup()` deletes only the canonical key, so normal cleanup cannot reach them. A principal with a
surviving legacy-version alias row therefore pays a bounded `MigratingFromLegacy` hold **again on
every restart**, and one with a surviving legacy-regime alias row pays a bounded
`MigratingSenderRegime` hold on every restart, for as long as that row exists. The mixed-regime case
does not converge on its own: a completed promotion writes the canonical key and never touches an
alias, so the pair re-forms on the next start. The store converges — and the recurring hold stops —
only once the principal's rows are back to a single `(semantic_version, sender_regime)` group, at
which point the next start canonicalizes them normally. That is the deliberate trade: a bounded, self-clearing hold that
can recur, against a permanent loss of replay protection.

**Unreadable rows quarantine in two of the three keyspaces, not all three.** For row #1
(`replay_sender_regime:`) and row #2 (`replay_max_seq:`), the load pass turns an unreadable value
into a quarantine hold on the merged sender. Row #3 (`replay_finalized:`) is the exception: an
unparseable value is left in place but *skipped*, with no hold, so a finalized block whose
`(sender, sequence)` has no other readable row is lost silently. That arm is byte-identical to
base (`5f86610f:icn/crates/icn-net/src/replay_guard.rs:1008`) — the behaviour predates #2640 and
is not a #2640 bypass — and `ReplayGuard::finalize` / `is_finalized` have **zero production
callers** workspace-wide, so the keyspace is unreachable machinery today. Making the finalized
load pass quarantine-consistent with rows #1–#2 is a prerequisite before `finalize()` gains a
production caller.

**Not fixed, and unchanged by it.** `SignedEnvelope::canonical_encoding` still does not cover
`from`, so the field is still unauthenticated and a re-spelled envelope still *verifies*; only
its second acceptance is refused. `Did::from_str` still accepts all 23 spellings and `Did`
equality is still string equality — I7 / N2-A (#2627) still owns both, and this fix deliberately
did not begin it. Every other row in §5–§7 is untouched, including `apps/trust-app/src/sequence.rs`
(#71), the fourth instance of this class named in §10.2 above.

**What N2-A must preserve.** When I7 lands, `Did` equality becomes key equality and the in-memory
window map would collapse spellings anyway — but the durable rows would not, so the canonical
*write* here must stay. N2-A must either keep `SenderPrincipal` as the guard's key or, if it
replaces it with a canonical `Did`, keep writing rows #1–#3 under a key-derived spelling and keep
the merge pass until no store can contain a legacy alias row. Dropping the migration would
silently restore the durable half of this defect.

Row #57 (`peer_connections`) carries a second invariant, surfaced by the independent security
review of this change. That map is spelling-keyed — `handlers/hello.rs` inserts under the wire
spelling of `from` — so a re-spelled envelope from a sender that has established `DurableV1`
missed its row-#57 lookup and read as `LegacyOrUnproven`.

An earlier revision of this section called that redundant defence in the safe direction, on the
grounds that the canonical replay floor rejects the same envelope on its own in both regime arms.
**That is true only where a floor already exists, and it is the wrong half of the case.** A
receiver that has authenticated the sender but not yet accepted a message from it holds an *empty*
window, so there is no floor to be redundant with: `(LegacyOrUnproven, LegacyOrUnproven)` is
steady state, and a still-fresh envelope captured from the sender's pre-upgrade numbering was
accepted and forwarded to the application, instead of entering the `MigratingSenderRegime` hold
that `(LegacyOrUnproven, DurableV1)` installs and that exists precisely because old-namespace
captures can still be inside their validity window. Only the established-window half was
redundantly defended. Both halves are pinned in `handlers/signed.rs`
(`a_respelled_envelope_cannot_launder_a_durable_sender_into_the_legacy_steady_state` and
`a_respelled_replay_below_an_established_durable_floor_is_still_rejected`).

The lookup itself is fixed rather than deferred: `handlers/signed.rs` now resolves the sender's
authenticated capabilities by `SenderPrincipal` — the same equivalence class the signature check
and `ReplayGuard` already use — joining across the rows that decode to that principal with
"any such row proving `DURABLE_SIGNING_SEQUENCE` proves it". The map's *key* is unchanged, so the
other row-#57 and row-#60 consumers are untouched.

**That join no longer reads row #57 at all.** Row #57 is a *cache*, not a live-session
registry: nothing removes an entry when a connection ends (`actor/connection.rs` returns on both
the application-close and the error path without touching the map; the only removal anywhere is
the administrative `NetworkHandle::disconnect_peer`), and `restore_state` recreates entries from
the snapshot at startup with their capability bits intact and no Hello behind them. Joining on
principal alone over such a map let a peer that proved `DurableV1` under spelling A,
disconnected, and returned under spelling B *without* the capability keep reading as `DurableV1`
from A's abandoned row — the `(DurableV1, LegacyOrUnproven)` downgrade never fired, and a
captured old-namespace sequence above the retained durable floor was admitted as though it were
a durable number. Reproduced end to end at floor 10 with a captured sequence of 100; the
snapshot variant needs no key at all, since `actor/connection.rs` dispatches
`MessagePayload::Signed` without binding `envelope.from` to the connection's authenticated peer.

Sender-regime attribution therefore moved out of row #57 entirely, to
`icn-net/src/capability_evidence.rs`. `LiveCapabilityRegistry` is indexed by `SenderPrincipal`
and holds an entry only while some connection is *leasing* it: `handle_hello` takes a
`LiveCapabilityClaim` after the three #2520 DID-TLS checks pass, the claim lives in that
connection's `ConnectionContext`, and the connection handler owns that context on its own stack
frame — so every exit path releases it, including ones added later. This is the same RAII shape
`preauth_admission::AdmissionGuard` already uses for #2547 slots, and it is what makes the two
axes independent by construction: the index answers *which* key, and the lease's lifetime
answers *whether the claim is still true*. Claims are reference counted, because a key holder may
authenticate under several spellings of itself and cross-dialling gives one pair two connections
at once; the join across live claims is `any`, which is the only join that cannot be suppressed
by adding one.

Two consequences worth recording for N2-A. First, this removes a per-envelope scan of an
unbounded structure: the interim fix walked row #57 once per signed message, and since nothing
prunes that map, a peer reconnecting under one-off DIDs could grow it permanently and make every
other peer's traffic pay for the scan. The registry is one hash lookup, and its cardinality is
the number of connections currently *claiming* — each of which is a live QUIC connection the
peer has to keep standing up, where the cache kept its entry after the connection was gone.
**This is deliberately not a #2547 bound**, and should not be recorded as one:
`record_authenticated_peer` drops the admission guard the moment a connection authenticates,
while the capability claim is taken afterwards, so pre-authentication admission limits how many
connections a source may hold *while anonymous* and says nothing about authenticated ones. A
post-authentication connection bound, if one is wanted, is a separate piece of work this does
not supply. Second, row #57's *remaining* consumers are unchanged — cached version, X25519 and PQ material,
and #2504-era reconnection — so the migration this row still owes is unaffected. What changed is
that replay-regime attribution is no longer one of them.

Row #57 keeps one unfixed consumer of the same class: `verify_with_cached_pq_key` resolves the
cached ML-DSA key with the same textual `connections.get(&envelope.from)`, so a re-spelled hybrid
envelope misses and downgrades to Ed25519-only. Latent behind `#[cfg(feature = "post-quantum")]`
and tracked as **#2646** — deliberately not folded in here, because a capability is a boolean
that joins with `any` while a key is a value, and "pick any live claim" is not obviously right.

N2-A therefore still owes (a) the canonical replay floor independently rejecting the re-spelled
replay once the map key itself becomes alias-tolerant, and (b) keeping these regressions. The
earlier concern that this path misclassified an attacker-chosen input as the local fault
`SenderRegimeDowngrade` no longer applies to re-spelling — the lookup no longer misses — and
`SenderRegimeDowngrade` is again what it is named for, an operator rollback. §12.1 item 6 covers
the #57/#60 partner-map *desync*; it does not cover this.

### 10.3 Vote double-counting, and a re-cast guard that cannot fire
`GovernanceError::AlreadyVoted` (`icn-governance/src/error.rs:33`) has **zero constructors anywhere
in the workspace** — confirmed live, matching IDENTITY_SEMANTICS §7.5's "declared but never
constructed". The production tally paths count every stored row: `GovernanceStore::compute_tally`
(`store.rs:298`, `:650`) is `list_votes` → `VoteTally::from(votes)`, and
`compute_tally_with_delegations` (`tally.rs:114–118`, no non-test caller today) calls
`tally.add_vote(vote)` unconditionally in pass 1 with `counted` gating only *delegation*
resolution. Two votes from one person under two spellings are **both counted**; row #18's
de-duplication is over `Vec<String>`, so it does not catch it; and `apps/governance`'s own guard
(`manager.rs:~4771`, `v.voter == voter`) is `Did` string equality, so the live store (#23) takes the
second row too. Because `icn-governance`'s sled store is dormant (§3.1), the *live* double-count is
#23. §7.5 anticipated this for a *re-key*; it is reachable now without one, and I7 alone does not
fix it (the rows are `Display`-keyed). **Disposition:** no issue owned it (§7.5 is stated as a gate,
not materialised; #2623/#2626/#2627 disclaim governance storage). **Filed by this review as
#2641.**

**Resolved 2026-08-28 (#2641).** The measurement above stands as taken at `798c8d54`. Governance
now resolves a voter to the bytes its `did:icn:` identifier decodes to (`icn-governance`'s
`VotingPrincipal`): `AlreadyVoted` has a constructor (`ensure_has_not_voted`), both
`GovernanceStore` implementations supersede a principal's prior row across spellings, and every
tally path in the workspace uses `VoteTally::try_from_votes`, which collapses agreeing alias rows
and **fails closed** on conflicting ones rather than electing a survivor. That covers the live
actor paths — the read-only `get_vote_tally` *and* the decision-producing tally inside
`CloseProposal`, including its close-time delegation expansion — and the `GovernanceStore` library
surface, whose only in-workspace consumer is `ProposalCleanupTask::archive_proposal`. Three
corrections to the note above, found while fixing it: the live exploit is not only #23 via
`manager.rs`'s guard —
`GovernanceManager::cast_vote` delegates to `governance_handle` as its *first* statement, so in
actor-backed deployments the membership gate and that guard never ran, and the actor's `CastVote`
handler had **no** duplicate-vote guard at all; and the identity used for the fix is the decoded
identifier bytes rather than a `VerifyingKey`, because anchor-derived DIDs (`Did::from_anchor_id`)
need not decompress to an Edwards point. Third, the delegation hazard is not confined to
`compute_tally_with_delegations` (which has no production caller): its production twin,
`apply_delegation_to_tally`, was spelling-keyed, so a member could delegate from one spelling of
their key to another, vote as the delegate, and have that single vote resolved back to them as a
second one at close — reachable on a fresh deployment with no legacy rows. Nothing was re-keyed: vote rows remain `Display`-keyed and
membership lists remain spelling-compared, so §7.5's re-key gate is untouched and this row stays
`NEEDS MIGRATION` for N2-A.

### 10.4 `grantee_canonical_bytes` canonicalizes the *layout*, not the spelling
`receipt_store.rs:1071` is named `*_canonical_bytes` and emits a tag byte followed by
`did.as_str()` — the raw spelling. A lookup with a differently-spelled DID misses the grant (fails
closed), while two grants for one principal can coexist. On re-reading, its doc comment defines
"canonical" precisely: the ADR-0014 tag-byte + length-prefix *layout* that keeps `Person` and
`Entity` in separate keyspaces and prevents prefix aliasing (PR #1576) — it does not claim DID
spelling canonicalization. The first pass's "the name asserts a property the function does not
have" is therefore withdrawn. **Disposition:** not an independent defect — it is row #25's ordinary
spelling-keyed index, in the same class as every other `SILENT-MERGE RISK` row; handled by N2-A's
scan, no issue.

### 10.5 Partner invariants that I7 would *introduce* (N2-A must fix atomically)
Not current defects — today every one of these is string-consistent. I7 moves one half and not the
other. **Disposition:** no "current defect" issue; tracked as N2-A constraints (§12.1 item 6) and
recorded on #2627 (comment of 2026-08-19).

- `PeerId(pub Did)` (`icn-net/src/topology.rs:42–55`) derives `PartialEq`/`Eq`/`Hash` — which
  delegate to `Did` and so move under I7 — while hand-implementing `Ord` over
  `self.0.to_string()`, which does not move (`Did` itself has no `Ord`). After I7, two aliasing
  `PeerId`s satisfy `a == b` while `a.cmp(&b) != Equal`, violating the standard library's
  `Ord`/`Eq` consistency requirement. `PeerId` is used in four `BTreeSet`s and one `HashMap`
  (`topology.rs:23–35`), where an inconsistent `Ord` yields unspecified lookup and insertion
  behaviour. (Row #52.)
- `icn-ccl` `Value` (`icn-ccl/src/types.rs:110–131`) derives `PartialEq`/`Eq` — `Value::Did`
  moves — but hand-implements `Hash` (`:200–220`) and hashes `Value::Did` as
  `format!("{did:?}")`, a string, which does not move. `HashSet<Value>` (`Value::Set`, used for
  `participants` and `in` checks in `interpreter.rs:403–410, 484–485`) then violates the
  `Hash`/`Eq` contract: `List.contains` (Eq, moves) and `Set.contains` (Hash, does not) disagree.
  (Row #61.) Contract *state* is in-memory only, but `Value::Set` is persisted inside
  `Contract.state_vars[].initial_value` via `icn_encoding` (`ast.rs:69`).
- The `icn-net` connection-map pair: `SessionManager.connections: HashMap<String, quinn::Connection>`
  keyed by `from.to_string()` (`session.rs:58`, `:906–912`) and `NetworkActor.peer_connections:
  HashMap<Did, PeerConnectionInfo>` (`actor/mod.rs:1098`), both filled from the same Hello `from`
  and joined in `actor/messages.rs:474–491 send_message_to_peer` (`Did`-keyed lookup, then
  `connections().find(|(peer_did, _)| peer_did == did.as_str())`). Post-I7 the `Did` map finds an
  alias, the `String` map does not. (Row #60.)
- Every production `did.as_str() == string` / `did.to_string() != string` comparison standing in
  for `Did` equality stays spelling-sensitive post-I7: `icn-gateway` `api/commons/mod.rs:242, 278,
  358` (`claims.sub != did.to_string()`), `websocket.rs:220`, `api/sdis/simple_enrollment.rs:1096,
  1127`, `listings_mgr.rs:929`, `trust_mgr.rs:1159`; `icn-core storage_challenge.rs:526`,
  `supervisor/init_network.rs:315, 370`; `icn-gossip handlers/storage_challenge.rs:58`; `icn-store
  pos.rs:251, 490, 596`; `icn-trust lib.rs:1180` (`Did` vs `Did` via `as_str`); `bins/icnctl
  main.rs:6242`. Not defects today; N2-A must decide per site whether string comparison is the
  intended semantics.

### 10.6 Namespaces that I7 does not reach, and therefore desynchronises
Three constructs embed or duplicate a DID but keep string equality, so I7 moves `Did` out from
under them:

- **`EntityId(String)`** — `EntityId::from_did` (`entity.rs:51`) splices the multibase spelling
  verbatim into `entity:icn:individual:<spelling>`. Rows #33–#35 are durable membership keys.
  After I7, `a == b` as `Did` while `EntityId::from_did(a) != EntityId::from_did(b)` — two member
  rows for one principal, permanently, and I7 alone will not fix it.
- **`icn-kernel-api::Did = String`** (`types.rs:9`) — the alias family I12/N2-H owns. Unaffected by
  I7 by construction.
- **`StewardId::from_did`** (`icn-governance/src/steward.rs:29–38`) = SHA-256 of
  `"steward:" ‖ did.as_str()` — a primary key hashed over the *spelling*, used by the live
  `icn-commons` steward store (`commons/stewards/<hex>`, #63). Two spellings → two steward records,
  and I7 does not move a `[u8; 32]`.
- **`icn-commons` weak-holder id** (`icn-commons/src/inner.rs:346–362 update_display_name`) =
  SHA-256 of `did.to_string()`, minted behind a `Did`-equality authorization gate (#65) — the one
  namespace construct where I7 *creates* a durable split (§3.1 clause (d)).
- **`icn-snapshot::vector_clock: HashMap<String,u64>`** (#54) and
  **`NetworkState.peer_connections: HashMap<String,_>`** (#57) — `String`-keyed on disk, but
  both are parsed back into `Did`-keyed maps on restore, so they *are* moved (lossily) by I7 at
  the restore boundary. #54 is therefore resolved `NEEDS MIGRATION`, not `UNRESOLVED` (§11).
- **`icn-community` `Community.members: HashMap<String, Member>`** (#72) — `String`-keyed
  ("Can be DID or CooperativeId"), never parsed to `Did`; diverges from any `Did`-typed
  membership view.

**N2-A must state explicitly which of these it is and is not fixing** (§12.1 item 4(iv)).

---

## 11. Coverage limits — what this inventory does not establish

Stated plainly, per #2623's sixth acceptance criterion.

1. **No live data was examined.** Every verdict is derived from source and from probes against the
   real types. Whether any deployed store *currently contains* two aliasing spellings is unknown and
   unknowable from the repository. The inventory therefore bounds the **reachable** hazard, not the
   **realised** one. A pre-migration scan of live keyspaces remains necessary and is N2-A's to run
   (§12.1 item 3).
2. **Row #54 is resolved** (`NEEDS MIGRATION`, §3.1 clause (c)): the snapshot vector clock is
   parsed back into the `Did`-keyed gossip clock on restore (`gossip.rs:1369–1382`). No row is
   `UNRESOLVED` after this review. The two `String`-keyed structures that are *not* re-keyed (#72
   `Community.members`, #73 `ReplicaMetadata`) are classified, not left open: not moved by I7,
   flagged for N2-A's namespace decision (§10.6).
3. **Value-position DIDs were not exhaustively enumerated.** S8 found 155 struct-field rows (147
   distinct structs) with a `Did` field. Those used as *keys* are covered; those appearing only in
   *values* are not individually listed, on the ground that I7 does not change value semantics. If
   N2-A adds normalization at the parse boundary rather than at `Eq`/`Hash`, that assumption breaks
   and all 147 need re-review.
4. **`icn-ccl` is now traced** (first pass: untraced). Contract *state* is in-memory only
   (`runtime.rs:25`); `ContractRegistry` persists `participants: Vec<Did>` value-position and
   `owner: String` (its doc header claims an `index:owner:<did>` keyspace that is **not written** —
   comment drift, noted); `Value::Set` is persisted only inside `Contract.state_vars[].initial_value`.
   No `Did`-keyed map is persisted, but the `Value` type is a partner type (#61, §10.5).
5. **Test-only stores were excluded by construction** (`/tests/`, `#[cfg(test)]`). A durable store
   reachable only from test harnesses would be missed; the S5 root scan found no such case, but this
   is an exclusion, not a proof. Conversely, S14 found *library* stores that no binary constructs
   (dormant #17–#22, #44, #46) and write paths no binary reaches (dead, #84).
6. **The 23-spelling figure is a floor, not a ceiling.** It is what multibase v0.9.2 (24 declared
   bases: 23 unconditional + `Identity`) accepts through `Did::from_str`. A dependency bump that
   adds a base widens the aliasing surface with no code change in this repo; a key whose raw bytes
   are valid UTF-8 (~1 in 10⁸ random keys, grindable) has a 24th.
7. **Probe sources are recorded but not committed** — this tranche adds no code, so the
   integration-test probes live in §13.4 and §13.5 rather than in `tests/`. Re-add them as temporary
   integration tests to reproduce §2; delete them afterwards and **before** re-taking any S1/S7 count.
8. **`sled` iteration order is lexicographic by key bytes** (resolved; first pass: "treated as
   unspecified"). `SledStore::scan` (`icn-store/src/lib.rs:811`) iterates `db.scan_prefix(prefix)`,
   which yields keys in byte order, so every "last-writer-wins" rebuild in §6 has a
   **deterministic, attacker-selectable** survivor: the multibase code character decides — in ASCII
   `'B' < 'C' < … < 'Z' < 'b' < 'c' < 'f' < 'h' < 'k' < 'm' < 't' < 'u' < 'v' < 'z'`, and the
   `Base256Emoji` prefix `🚀` (`F0 9F 9A 80`) sorts after all of them, so an attacker who wants
   their row to survive spells it in `Base256Emoji`; the canonical `z…` spelling beats every other
   ASCII alternate. This does not change any verdict under §3.1 (the class is decided by whether a
   `Did`-re-keying consumer exists, not by who wins) but it changes the severity wording: "arbitrary
   survivor" in #37, #38, #39 is "chosen survivor". `HashMap`-sourced restores (#54, #57) have a
   nondeterministic survivor instead.
9. **Line numbers are as of `798c8d54`.** Symbols, not line numbers, are the citation; the review
   re-found every cited symbol at or near its line.

---

## 12. Result

| Verdict | Count |
|---|---|
| `SAFE` | **67** — 58 ephemeral S1 sites not belonging to any row (§7) + `PrincipalKey` (#55) + 8 rows the review reclassified as safe or dead (#43, #47, #48, #49, #50, #51, #53, #84) |
| `SILENT-MERGE RISK` | **51** — 24 in §5, 19 in §5.1, 3 in §6, 5 in §6.1 |
| `NEEDS MIGRATION` | **24** rows — 12 in §5, 3 in §5.1, 8 in §6, 1 in §6.1 |
| `UNRESOLVED` | **0** |
| **Total candidate stores inspected** | **142** — 36 durable keyspaces (§5) + 23 added (§5.1) + 18 reconstructed/serialized (§6) + 6 added (§6.1) + 58 safe sites (§7) + #55 |

(First pass: 121 candidates — 67 / 31 / 22 / 1. The review added 29 rows, reclassified 18, and
resolved the open one. Counts are produced by the generator that renders the tables, and
re-verified by counting the rendered verdict cells — see the closing comment on #2623.)

**Concrete list N2-A inherits — the `NEEDS MIGRATION` rows**, 24 in all: #1, #2, #3, #4, #5, #6,
#7, #8, #9, #23, #29, #30, #37, #38, #39, #40, #42, #45, #46, #54, #57, #65, #68, #69.
Folding each consumer row into its keyspace, that is **13 live keyspaces / structures plus one
dormant shape**:

| # | Keyspace / structure | Consumer that makes it lossy | Mechanism |
|---|---|---|---|
| 1 | icn-net replay state #1–#3 | #37 `load_persisted_state` | overwrite, chosen survivor; lower floor can win |
| 2 | icn-net sequence tracker #4 | own `:98 cache` | overwrite; lower outgoing seq ⇒ nonce regression |
| 3 | icn-security misbehavior #5–#8 | #38 `load_from_store` | overwrite + orphan on save |
| 4 | icn-ledger balances #9 (+ journal) | #39 `load_cached_balances`, #40 `compute_all_balances` | overwrite + orphan; sum |
| 5 | icn-ledger cleared volume #69 | own `load_cleared_volume_index` | overwrite + orphan (tuple key) |
| 6 | icn-ledger freeze #68 | #42 `load_from_store` | overwrite; unfreeze deletes one spelling only |
| 7 | apps/governance votes #23 (**§7.5 gate**) | `actor.rs:3862–3864` | last-writer; direct double-count |
| 8 | icn-rpc auth challenges #29 (low) | `load_challenges` | overwrite; TTL-bounded |
| 9 | icn-trust edges #30 | `get_all_known_dids` → spelling scans | dropped spellings' edges vanish |
| 10 | icn-commons weak-holder id #65 | the `Did`-equality gate | second record minted post-I7 (clause (d)) |
| 11 | class C: `VectorClock` #45 | serde map decode | last duplicate kept silently |
| 12 | class C: snapshot `vector_clock` #54 | `gossip.rs restore_state` | overwrite on restore |
| 13 | class C: snapshot `peer_connections` #57 | `actor/mod.rs restore_state` | overwrite / first-wins on restore |
| — | class C: `CompressedVectorClock` #46 (**dormant**) | none today | fix the derive shape; no data step |

Dropped from the first pass's list, with the reason in each row: #17/#18 (dormant store — the live
vote keyspace is #23), #24 (raw-byte-keyed state, no `Did` re-key), #31 (local canonical write
path, point lookup only), #33/#34 (`EntityId` is not moved by I7 — a §10.6 namespace decision, and
cannot differ from #35), #50 (ephemeral). Added: #29, #30, #42/#68, #46, #54, #57, #65, #69.

**What the evidence says about the mechanism choice.** IDENTITY_SEMANTICS §11 (I7) permits either
equality over decoded bytes or encoding pinned at parse. This inventory does not choose — that is
N2-A's decision and its rationale is N2-A's to state — but it records three constraints the choice
must satisfy:

1. Pinning at parse changes what `Did::from_str` *accepts*, which is a wire-compatibility change
   affecting all 168 class-E parse sites (§4 S7), and would make currently-loadable persisted rows
   unloadable. Equality over decoded bytes changes no acceptance and no durable byte — but it is
   the only one of the two that leaves the third-party re-spelling replay (§2.2) open, because
   `from` is unsigned and the durable replay rows stay spelling-keyed.
2. Neither mechanism fixes §10.6 — `EntityId`, `StewardId`, the `icn-commons` holder id, the
   kernel `String` alias, `Community.members` and `ReplicaMetadata` keep string equality either way.
3. Neither mechanism repairs a durable keyspace by itself. Every §5 row is built by `Display`, which
   I7 does not touch, so the physical rows survive I7 unchanged and a **separate re-keying step** is
   required for the list above. `PrincipalKey` (#55) is the in-repo precedent for the decoded-byte
   form, `VectorClockProjection::from_entries` (max-per-key) for a decode merge rule, and
   `legacy_proposal_index_key` (§8) for the rewrite mechanics.

**§7.5 remains a separate hard gate.** Rows #23 (live), #17/#18 (dormant), #56 (`StaticList`
membership) and finding §10.3 are membership and vote storage. §7.5 requires migration ordering,
alias/transition recognition, duplicate-act prevention and final cutover to be designed before any
live re-key. Nothing here discharges that gate, and N2-A must not treat these rows as ordinary
migrations.

### 12.1 What N2-A must design against — the surface, not the design

This inventory does not design the migration. It hands N2-A the surface the migration must be
designed against, as a checklist it can be reviewed against:

1. **Persisted-keyspace list** — §5 (rows #1–#36 plus the review additions). Every durable
   keyspace whose key embeds a DID spelling, with its write path (locally-generated canonical vs
   wire/API spelling) and whether it is live, dormant or dead.
2. **Migration-required list** — §12 *Concrete list*, with the mechanism per row in §5/§6. A
   keyspace appears once; its re-keying consumer is the evidence.
3. **Pre-migration collision scan** — mandatory, because no live data was examined (§11 item 1).
   For every keyspace in §5, group rows by decoded key bytes (not by spelling) and report groups
   of size > 1. `Base256Emoji` (`🚀…`) spellings sort after every ASCII spelling under `sled`'s
   lexicographic order, so report the *order* of rows in each group too — it decides the
   survivor in every last-writer rebuild (§11 item 8).
4. **Ordering constraints** —
   (i) for `NEEDS MIGRATION` keyspaces with a `Did`-keyed rebuild (#1–#3/#37, #4, #5–#8/#38,
   #9/#39/#40, #23, #29, #30, #42/#68, #54, #57, #69), de-dupe the durable rows **before** the first start of a
   binary with key-equality `Did`, because that first start performs the lossy rebuild and the
   write-back (`save_to_store`, `save_cached_balances`) then orphans the losers;
   (ii) for class-C structures (#45, #46) the merge rule for aliasing keys (max / sum / reject)
   must be chosen before decode collapses them — `VectorClockProjection::from_entries` already
   picks *max*, which is a precedent;
   (iii) membership and vote keyspaces (#17/#18 dormant, #23 live, `StaticList` row) are behind
   the **§7.5 hard gate** — migration ordering, alias/transition recognition, duplicate-act
   prevention and final cutover — and are not ordinary migrations;
   (iv) the namespaces I7 does not move (`EntityId::from_did`, `StewardId::from_did`,
   `icn-commons` weak-holder id = SHA-256 of the spelling, the kernel `Did = String` alias,
   `String`-keyed snapshot maps) need an explicit decision — follow `Did` (canonicalise at
   construction + de-dupe rows) or stay spelling-keyed — stated in N2-A, per namespace.
5. **Compatibility and rollback** — equality-over-bytes changes no durable byte and no
   acceptance, so a binary rolled back to string equality reads the migrated (de-duplicated)
   rows unchanged; pin-at-parse changes acceptance and makes every alternate-spelled row
   unloadable (§12 constraint 1), so it needs a compatibility-read window first — and it is the
   only mechanism that closes the third-party re-spelling replay (§2.2, #2640) without a wire change.
6. **Partner-type invariants that move out of step** (all must be fixed in the same change):
   `PeerId` `Ord` over `to_string()` (#52, §10.5); `icn-ccl` `Value` derives `PartialEq`/`Eq`
   but hand-hashes `Value::Did` via `format!("{did:?}")` — `HashSet<Value>` (`Value::Set`,
   `participants`, `in` checks) breaks the `Hash`/`Eq` contract post-I7; the `String`/`Did`
   peer-map pair in `icn-net` (`SessionManager.connections: HashMap<String,_>` keyed by
   `from.to_string()` vs `peer_connections: HashMap<Did,_>`, joined in
   `actor/messages.rs send_message_to_peer`) which disagree post-I7; every production
   `did.as_str() == some_string` comparison standing in for `Did` equality (icn-gateway commons
   `claims.sub != did.to_string()`, websocket, sdis enrollment, listings, trust_mgr; icn-core
   storage_challenge, init_network; icn-store pos; icnctl) stays spelling-sensitive.
7. **Unresolved-data policy** — none of the inventory's rows is `UNRESOLVED` after review. If
   the pre-migration scan finds an aliasing group whose store has no stated merge rule, the
   policy is **fail closed**: refuse to start the key-equality binary on that store until the
   rule is chosen, rather than let the rebuild pick a survivor.

---

## 13. Appendix — the search and probe sources

Recorded verbatim so §4's methodology and §2's measurements are re-runnable rather than merely
described. None of this is committed as code; all of it is reproducible from a clean checkout at
`798c8d54` (and at `bca3dd0e` — the Rust tree is identical). The three scanners run from the
repository root and are read-only; they are a *read list* for the human pass, not results — the
load-bearing results are the S1–S14 commands in §4 and the two probes below. Every expected count
in this appendix was re-run by the review and reproduced exactly, with no probe files present.

### 13.1 S5 — rank durable roots by identity-term density

```python
import re, os
ROOT = "."
files = []
for dp, dn, fn in os.walk(ROOT):
    if '/.git' in dp or '/target' in dp or '/node_modules' in dp:
        continue
    files += [os.path.join(dp, f) for f in fn if f.endswith('.rs')]
durable = [f for f in files
           if re.search(r'sled::(Db|Tree|open|Config)',
                        open(f, encoding='utf8', errors='replace').read())]
IDENT = re.compile(
    r'\b(did|dids|_did|did_|member_id|member_did|owner|owner_did|actor|actor_did|'
    r'peer_id|peer_did|subject|subject_id|principal|voter|issuer|holder|signer|'
    r'operator_did|node_did|org_did|treasury_did|coop_id|entity_id|account)\b', re.I)
rows = sorted(((len(IDENT.findall(open(f, encoding='utf8', errors='replace').read())),
                os.path.relpath(f, ROOT)) for f in durable), reverse=True)
print("rust files:", len(files), "| touching sled:", len(durable))
for n, p in rows:
    print(f"{n:5d}  {p}")
```

Expected at `bca3dd0e`: **1033** Rust files, **69** touching `sled`.

### 13.2 S6 — `Did`-keyed collections inside serde-derived structs (class C)

```python
import re, os
COLL = re.compile(r'(HashMap|BTreeMap|DashMap|HashSet|BTreeSet)<\s*&?\s*Did\b')
DERIVE = re.compile(r'#\[derive\(([^)]*)\)\]')
for dp, dn, fn in os.walk("."):
    if '/.git' in dp or '/target' in dp or '/node_modules' in dp:
        continue
    for f in fn:
        if not f.endswith('.rs'):
            continue
        p = os.path.join(dp, f)
        if '/tests/' in p:
            continue
        lines = open(p, encoding='utf8', errors='replace').read().split('\n')
        for i, l in enumerate(lines):
            if not COLL.search(l):
                continue
            for j in range(i, max(-1, i - 60), -1):          # nearest enclosing item
                if re.match(r'\s*(pub )?(struct|enum) ', lines[j]):
                    for k in range(j - 1, max(-1, j - 8), -1):
                        m = DERIVE.search(lines[k])
                        if m and ('Serialize' in m.group(1) or 'Deserialize' in m.group(1)):
                            print(f"{p}:{i+1}  {lines[j].strip()[:70]}  [{m.group(1)[:40]}]")
                        if m:
                            break
                    break
```

**Known limitations, stated because they changed verdicts:** (i) the backward walk stops at the
nearest enclosing `struct`/`enum`, so a `let` binding inside an `impl` block is attributed to that
block's type — two of the five raw hits (`icn-trust/src/anomaly.rs:601–602`) are such false
positives; (ii) the scanner cannot tell `icn_identity::Did` from the kernel `type Did = String`
alias — a third hit, `icn-kernel-api/src/proofs.rs:490`, is the alias (#53); (iii) it matches only
map/set types, so a serialized `Vec<Did>` with set semantics (#56) is invisible to it. The two real
class-C members are #45 and #46. Treat the output as a *read list*, never as a result.

### 13.3 S8 — wrapper types that hide a `Did` (the second pass)

```python
import re, os
for dp, dn, fn in os.walk("."):
    if '/.git' in dp or '/target' in dp or '/node_modules' in dp:
        continue
    for f in fn:
        if not f.endswith('.rs'):
            continue
        p = os.path.join(dp, f)
        if '/tests/' in p:
            continue
        src = open(p, encoding='utf8', errors='replace').read()
        for m in re.finditer(r'pub struct (\w+)\s*\(\s*(?:pub\s+)?Did\s*\)', src):
            print("newtype-struct        ", m.group(1), p)
        for m in re.finditer(r'(\w+)\s*\(\s*Did\s*\)\s*,', src):
            print("enum-variant          ", m.group(1), p)
        for m in re.finditer(r'pub struct (\w+)\s*\{[^}]{0,400}?\bpub \w+: Did\b', src, re.S):
            print("struct-with-did-field ", m.group(1), p)
```

Expected at `bca3dd0e`: **155** `struct-with-did-field` rows (**147** distinct struct names — the
script prints every occurrence, not distinct names) and **8** newtype/enum-variant rows resolving to
**5** distinct wrappers: `PeerId`, and the `Did` / `Person` / `Query` / `Remote` variants. Of those,
`PeerId(pub Did)` produced finding #52, `AccountId::Did(Did)` produced #9 and §10.6, and
`Grantee::Person(Did)` produced #25. **This class is invisible to S1** — it is the reason §9 exists
as a separate pass.

### 13.4 Probe A — encoding aliasing and anchor round-trip (§2.1, §2.3)

Add as `icn/crates/icn-identity/tests/n2a0_probe.rs`, run
`cargo test -p icn-identity --test n2a0_probe -- --nocapture`, then **delete it**. The first test
tries every base `multibase` v0.9.2 declares (the `Identity` base is built by hand because
`multibase::encode(Identity, …)` panics on non-UTF-8 bytes); the second proves the `Identity`
parser path with a constructed ASCII-valued point.

```rust
use icn_identity::anchor::{Anchor, EnrollmentPathway};
use icn_identity::Did;
use std::collections::HashMap;

#[test]
fn probe() {
    let kp = icn_identity::KeyPair::generate().unwrap();
    let canonical = kp.did().clone();
    let (_b, bytes) = multibase::decode(&canonical.as_str()[8..]).unwrap();

    use multibase::Base::*;
    // Every base multibase 0.9.2 declares (the first pass's probe tried 22 of them).
    let all = [Identity, Base2, Base8, Base10, Base16Lower, Base16Upper, Base32Lower, Base32Upper,
               Base32PadLower, Base32PadUpper, Base32HexLower, Base32HexUpper,
               Base32HexPadLower, Base32HexPadUpper, Base32Z, Base36Lower, Base36Upper,
               Base58Flickr, Base58Btc, Base64, Base64Pad, Base64Url, Base64UrlPad, Base256Emoji];
    let mut distinct: std::collections::HashSet<String> = Default::default();
    let mut rejected = vec![];
    for b in all {
        let enc = if b == Identity {
            match std::str::from_utf8(&bytes) { Ok(s) => format!("\u{0}{s}"), Err(_) => { rejected.push("Identity: key bytes not valid UTF-8 (encode would panic)".into()); continue; } }
        } else { multibase::encode(b, &bytes) };
        let alt = format!("did:icn:{enc}");
        match Did::from_str(&alt) {
            Ok(d) => {
                // key bytes identical?
                assert_eq!(d.to_verifying_key().unwrap().as_bytes(), &bytes[..], "{b:?}");
                // serde round trip preserves spelling?
                let j = serde_json::to_string(&d).unwrap();
                let back: Did = serde_json::from_str(&j).unwrap();
                assert_eq!(back.as_str(), alt, "serde must preserve spelling for {b:?}");
                // Display preserves spelling
                assert_eq!(d.to_string(), alt);
                distinct.insert(alt);
            }
            Err(e) => rejected.push(format!("{b:?}: {e}")),
        }
    }
    println!("TRIED={} ACCEPTED={} REJECTED={:?}", all.len(), distinct.len(), rejected);

    let mut m: HashMap<Did, u32> = HashMap::new();
    for (i, d) in distinct.iter().enumerate() { m.insert(Did::from_str(d).unwrap(), i as u32); }
    println!("HASHMAP_LEN={}", m.len());
    let j = serde_json::to_string(&m).unwrap();
    let back: Result<HashMap<Did, u32>, _> = serde_json::from_str(&j);
    println!("MAP_ROUNDTRIP_LEN={:?}", back.map(|b| b.len()));

    // Identity base over many keys: how often is the raw 32-byte key valid UTF-8?
    let mut id_ok = 0u32;
    for _ in 0..2000 {
        let k = icn_identity::KeyPair::generate().unwrap();
        let (_b, kb) = multibase::decode(&k.did().as_str()[8..]).unwrap();
        if let Ok(s) = std::str::from_utf8(&kb) {
            let alt = format!("did:icn:\u{0}{s}");
            if Did::from_str(&alt).is_ok() { id_ok += 1; }
        }
    }
    println!("IDENTITY_BASE_ACCEPTED_OF_2000_KEYS={id_ok}");

    // Anchor round-trip — artifact construction (sha256 of counter), 200 then 20000
    for n in [200u32, 20000u32] {
        let (mut ok, mut fail) = (0u32, 0u32);
        for i in 0..n {
            let mut id = [0u8; 32];
            id.copy_from_slice(&<sha2::Sha256 as sha2::Digest>::digest(i.to_le_bytes()));
            let d = Did::from_anchor_id(&id);
            let j = serde_json::to_string(&d).unwrap();
            if serde_json::from_str::<Did>(&j).is_ok() { ok += 1 } else { fail += 1 }
        }
        println!("ANCHOR_ROUNDTRIP_SHA_CTR n={n} OK={ok} FAIL={fail}");
    }
    // Real Anchor::from_vui(...).to_did() path
    let (mut ok, mut fail) = (0u32, 0u32);
    for i in 0u32..2000 {
        let mut vui = [0u8; 32]; vui[..4].copy_from_slice(&i.to_le_bytes());
        let genesis = [7u8; 32];
        let a = Anchor::from_vui(&vui, EnrollmentPathway::Genesis { reason: "probe".into() }, &genesis);
        let d = a.to_did();
        let j = serde_json::to_string(&d).unwrap();
        if serde_json::from_str::<Did>(&j).is_ok() { ok += 1 } else { fail += 1 }
    }
    println!("ANCHOR_ROUNDTRIP_FROM_VUI n=2000 OK={ok} FAIL={fail}");
}

#[test]
fn identity_base_parser_path() {
    // Find a 32-byte ASCII string that is a valid Ed25519 compressed point (~1/2 of strings are),
    // to prove the parser accepts the multibase Identity ('\0') form when bytes are valid UTF-8.
    for i in 0u32..10000 {
        let s: String = (0..32).map(|j| (b'A' + ((i.wrapping_mul(31).wrapping_add(j*7)) % 26) as u8) as char).collect();
        let alt = format!("did:icn:\u{0}{s}");
        if let Ok(d) = Did::from_str(&alt) {
            let (base, bytes) = multibase::decode(&alt[8..]).unwrap();
            println!("IDENTITY_ACCEPTED base={base:?} len={} key_ok={}", bytes.len(), d.to_verifying_key().is_ok());
            let canon = Did::from_public_key(&d.to_verifying_key().unwrap());
            println!("IDENTITY_CANONICAL_NE={}", canon != d);
            let j = serde_json::to_string(&d).unwrap();
            println!("IDENTITY_SERDE_RT_OK={}", serde_json::from_str::<Did>(&j).map(|x| x == d).unwrap_or(false));
            return;
        }
    }
    panic!("no ascii point found");
}
```

Observed at `798c8d54` (identical at `bca3dd0e`):
`TRIED=24 ACCEPTED=23 REJECTED=["Identity: key bytes not valid UTF-8 (encode would panic)"]`,
`HASHMAP_LEN=23`, `MAP_ROUNDTRIP_LEN=Ok(23)`, `IDENTITY_BASE_ACCEPTED_OF_2000_KEYS=0`,
`ANCHOR_ROUNDTRIP_SHA_CTR n=200 OK=90 FAIL=110`, `ANCHOR_ROUNDTRIP_SHA_CTR n=20000 OK=10051 FAIL=9949`,
`ANCHOR_ROUNDTRIP_FROM_VUI n=2000 OK=1011 FAIL=989`;
`IDENTITY_ACCEPTED base=Identity len=32 key_ok=true`, `IDENTITY_CANONICAL_NE=true`,
`IDENTITY_SERDE_RT_OK=true`.

The treasury-DID variant (§2.3, §10.1) goes in `icn/crates/icn-coop/tests/n2a0_treasury_probe.rs`
(`cargo test -p icn-coop --test n2a0_treasury_probe -- --nocapture`, then delete):

```rust
use icn_identity::Did;
#[test]
fn coop_treasury_did_roundtrip_rate() {
    let (mut ok, mut fail) = (0u32, 0u32);
    for i in 0..5000u32 {
        let anchor = icn_coop::lifecycle::derive_treasury_anchor(&format!("coop-{i}"));
        let mut anchor_32 = [0u8; 32];
        anchor_32[..16].copy_from_slice(&anchor);
        let d = Did::from_anchor_id(&anchor_32);
        let j = serde_json::to_string(&d).unwrap();
        if serde_json::from_str::<Did>(&j).is_ok() { ok += 1 } else { fail += 1 }
    }
    println!("COOP_TREASURY_DID_ROUNDTRIP n=5000 OK={ok} FAIL={fail}");
}
```

Observed: `COOP_TREASURY_DID_ROUNDTRIP n=5000 OK=2541 FAIL=2459`.

### 13.5 Probe B — envelope verification and replay windows under an aliased sender (§2.2)

Add as `icn/crates/icn-net/tests/n2a0_replay.rs`, run
`cargo test -p icn-net --test n2a0_replay -- --nocapture --test-threads=1`, then **delete it**.
`icn-net` does not depend on `multibase`, so the base16 form is built by hand — the leading `f` is
the multibase code for base16-lower.

```rust
use icn_identity::{Did, KeyPair};
use icn_net::envelope::{PayloadType, SignedEnvelope};
use icn_net::replay_guard::{ObservedSenderRegime, ReplayGuard};
use std::sync::Arc;

fn alias_of(canonical: &Did) -> Did {
    let key_bytes = canonical.to_verifying_key().unwrap().as_bytes().to_vec();
    let hex: String = key_bytes.iter().map(|x| format!("{x:02x}")).collect();
    Did::from_str(&format!("did:icn:f{hex}")).expect("alias parses today")
}

#[test]
fn probe_signature() {
    let kp = KeyPair::generate().unwrap();
    let canonical = kp.did().clone();
    let alias = alias_of(&canonical);
    let key_bytes = canonical.to_verifying_key().unwrap().as_bytes().to_vec();
    println!("CONTROL_ALIAS_NE_CANONICAL={}", alias != canonical);
    println!("ALIAS_KEY_SAME={}", alias.to_verifying_key().unwrap().as_bytes() == &key_bytes[..]);
    let e1 = SignedEnvelope::new(&canonical, &kp, 1, PayloadType::Gossip, b"m".to_vec()).unwrap();
    let e2 = SignedEnvelope::new(&alias, &kp, 1, PayloadType::Gossip, b"m".to_vec()).unwrap();
    println!("CANONICAL_VERIFIES={}", e1.verify(3600).is_ok());
    println!("ALIAS_VERIFIES={}", e2.verify(3600).is_ok());
    println!("FROM_FIELDS_DIFFER={}", e1.from != e2.from);
    // Cross-check: alias envelope signed by a DIFFERENT key must NOT verify (signature really binds the key)
    let other = KeyPair::generate().unwrap();
    let e3 = SignedEnvelope::new(&alias, &other, 1, PayloadType::Gossip, b"m".to_vec()).unwrap();
    println!("ALIAS_WRONG_KEY_VERIFIES={}", e3.verify(3600).is_ok());
}

#[test]
fn probe_replay_guard_in_memory() {
    let kp = KeyPair::generate().unwrap();
    let canonical = kp.did().clone();
    let alias = alias_of(&canonical);
    let mut guard = ReplayGuard::new(300, 3600);
    let e1 = SignedEnvelope::new(&canonical, &kp, 5, PayloadType::Gossip, b"m".to_vec()).unwrap();
    let e1b = SignedEnvelope::new(&canonical, &kp, 5, PayloadType::Gossip, b"m2".to_vec()).unwrap();
    let e2 = SignedEnvelope::new(&alias, &kp, 5, PayloadType::Gossip, b"m".to_vec()).unwrap();
    let r1 = guard.check(&e1, ObservedSenderRegime::LegacyOrUnproven);
    let r1b = guard.check(&e1b, ObservedSenderRegime::LegacyOrUnproven); // CONTROL: same spelling, same seq
    let r2 = guard.check(&e2, ObservedSenderRegime::LegacyOrUnproven);  // alias spelling, same seq
    println!("MEM_CANONICAL_SEQ5_ACCEPTED={}", r1.is_ok());
    println!("MEM_CONTROL_SAME_SPELLING_SEQ5_REPLAY_REJECTED={} ({:?})", r1b.is_err(), r1b.as_ref().err().map(|e| e.to_string()));
    println!("MEM_ALIAS_SEQ5_ACCEPTED={}", r2.is_ok());
    println!("MEM_PEER_COUNT={}", guard.peer_count());
    println!("MEM_MAX_SEQ canonical={:?} alias={:?}", guard.get_max_seq(&canonical), guard.get_max_seq(&alias));
}

#[test]
fn probe_replay_guard_persistent() {
    let kp = KeyPair::generate().unwrap();
    let canonical = kp.did().clone();
    let alias = alias_of(&canonical);
    let store = Arc::new(icn_store::SledStore::temporary().unwrap());
    let mut guard = ReplayGuard::new_persistent(300, 3600, store.clone());
    let e1 = SignedEnvelope::new(&canonical, &kp, 5, PayloadType::Gossip, b"m".to_vec()).unwrap();
    let e2 = SignedEnvelope::new(&alias, &kp, 5, PayloadType::Gossip, b"m".to_vec()).unwrap();
    let r1 = guard.check(&e1, ObservedSenderRegime::LegacyOrUnproven);
    let r2 = guard.check(&e2, ObservedSenderRegime::LegacyOrUnproven);
    println!("PERSIST_CANONICAL_SEQ5={:?}", r1.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    println!("PERSIST_ALIAS_SEQ5={:?}", r2.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    let _ = guard.finalize(&canonical, 5);
    let _ = guard.finalize(&alias, 5);
    use icn_store::Store;
    let rows = store.scan(b"").unwrap();
    let mut keys: Vec<String> = rows.iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).collect();
    keys.sort();
    println!("PERSIST_ROW_COUNT={}", keys.len());
    for k in &keys { println!("PERSIST_ROW {}", k.chars().take(60).collect::<String>()); }
    // Reload into a fresh guard: how many distinct windows come back?
    let mut guard2 = ReplayGuard::new_persistent(300, 3600, store.clone());
    let n = guard2.load_persisted_state().unwrap();
    println!("PERSIST_RELOADED_WINDOWS={n} peer_count={}", guard2.peer_count());
}

#[test]
fn probe_third_party_respell_replay() {
    // Attacker has NO key material: captures a validly-signed envelope and only rewrites `from`.
    let kp = KeyPair::generate().unwrap();
    let canonical = kp.did().clone();
    let alias = alias_of(&canonical);
    let captured = SignedEnvelope::new(&canonical, &kp, 9, PayloadType::Gossip, b"real-payload".to_vec()).unwrap();
    let mut forged = captured.clone();
    forged.from = alias.clone();              // only change: spelling of the sender
    println!("TP_SIGNATURE_BYTES_UNCHANGED={}", forged.signature == captured.signature);
    println!("TP_FORGED_VERIFIES={}", forged.verify(3600).is_ok());
    let mut guard = ReplayGuard::new(300, 3600);
    let a = guard.check(&captured, ObservedSenderRegime::LegacyOrUnproven);
    let b = guard.check(&captured, ObservedSenderRegime::LegacyOrUnproven); // control: true replay rejected
    let c = guard.check(&forged, ObservedSenderRegime::LegacyOrUnproven);   // re-spelled replay
    println!("TP_ORIGINAL_ACCEPTED={} TP_CONTROL_REPLAY_REJECTED={} TP_RESPELLED_REPLAY_ACCEPTED={}", a.is_ok(), b.is_err(), c.is_ok());
    // And a from-rewrite to a DIFFERENT key's DID must fail (signature binds the key bytes, not the spelling)
    let other = KeyPair::generate().unwrap();
    let mut forged2 = captured.clone();
    forged2.from = other.did().clone();
    println!("TP_OTHER_KEY_FROM_VERIFIES={}", forged2.verify(3600).is_ok());
}
```

Observed at `798c8d54` (identical at `bca3dd0e`):

```text
CONTROL_ALIAS_NE_CANONICAL=true   ALIAS_KEY_SAME=true
CANONICAL_VERIFIES=true   ALIAS_VERIFIES=true   FROM_FIELDS_DIFFER=true   ALIAS_WRONG_KEY_VERIFIES=false
MEM_CANONICAL_SEQ5_ACCEPTED=true
MEM_CONTROL_SAME_SPELLING_SEQ5_REPLAY_REJECTED=true (Some("Replay detected from did:icn:z…: sequence 5 already seen (max: 5)"))
MEM_ALIAS_SEQ5_ACCEPTED=true   MEM_PEER_COUNT=2   MEM_MAX_SEQ canonical=Some(5) alias=Some(5)
PERSIST_CANONICAL_SEQ5=Ok(())   PERSIST_ALIAS_SEQ5=Ok(())   PERSIST_ROW_COUNT=4
PERSIST_ROW replay_finalized:did:icn:f…   PERSIST_ROW replay_finalized:did:icn:z…
PERSIST_ROW replay_max_seq:did:icn:f…     PERSIST_ROW replay_max_seq:did:icn:z…
PERSIST_RELOADED_WINDOWS=2 peer_count=2
TP_SIGNATURE_BYTES_UNCHANGED=true   TP_FORGED_VERIFIES=true
TP_ORIGINAL_ACCEPTED=true   TP_CONTROL_REPLAY_REJECTED=true   TP_RESPELLED_REPLAY_ACCEPTED=true
TP_OTHER_KEY_FROM_VERIFIES=false
```

**The controls matter.** `CONTROL_ALIAS_NE_CANONICAL` must print `true` *before* I7 — it proves
the two spellings really are distinct `Did`s today, so `ALIAS_VERIFIES=true` demonstrates the
bypass rather than passing vacuously on two values that were equal all along.
`MEM_CONTROL_SAME_SPELLING_SEQ5_REPLAY_REJECTED` and `TP_CONTROL_REPLAY_REJECTED` prove the guard
does reject a genuine same-spelling replay, so `…ALIAS…ACCEPTED` is a bypass and not a guard that
accepts everything. `ALIAS_WRONG_KEY_VERIFIES=false` and `TP_OTHER_KEY_FROM_VERIFIES=false` prove
the signature still binds the *key* — only the *spelling* is free. After I7 the first control
inverts, and the probe must be re-read accordingly rather than re-run unchanged. (When run under the
`DurableV1` regime instead of `LegacyOrUnproven`, the persistent guard answers both envelopes with
the #2517 "holding sequence … for 600s" transition error — the namespace hold, orthogonal to this
finding — while still writing one `replay_sender_regime:` row per spelling.)
