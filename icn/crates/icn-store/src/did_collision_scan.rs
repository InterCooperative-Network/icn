//! Read-only pre-migration collision scan for `Did`-keyed persisted rows (N2-A, #2627).
//!
//! # Why this exists
//!
//! `Did` derives equality and hashing over its inner `String`, so two spellings
//! of one principal are two distinct keys today. `did:icn:` identifiers are
//! multibase, and multibase admits many encodings of the same 32 bytes, so a
//! store can already hold several rows that name one principal.
//!
//! N2-A makes `Did` equality key equality. On the first start of a binary that
//! carries that change, every `Did`-keyed rebuild collapses those rows — and
//! the write-back that follows orphans the losers. The
//! [N2-A0 inventory](../../../../docs/architecture/n2-a0-stored-key-inventory.md)
//! §12.1 item 3 therefore makes a pre-migration collision scan **mandatory**,
//! and item 7 makes the posture on an unruled collision **fail closed**.
//!
//! This module is that scan. It answers one question per keyspace — *do two
//! stored rows name one principal, and if so what happens when they merge* —
//! and it answers it from data rather than from source reading.
//!
//! # Guarantees
//!
//! * **Read-only.** The scan takes `&dyn Store` and calls only [`Store::scan`].
//!   Nothing here writes, deletes, or opens a store for writing.
//! * **Payload-free.** A [`CollisionReport`] carries counts, key shapes and
//!   dispositions. It never carries a stored value, and principals appear as a
//!   truncated hex fingerprint rather than a spelling or a full identifier.
//! * **Decode-faithful.** Rows are grouped by
//!   [`icn_identity::identifier_bytes_of_spelling`], the same decode
//!   `Did::identifier_bytes` uses, so a group here is exactly a group the
//!   post-N2-A equality would form.
//!
//! # What a group means
//!
//! Rows are grouped by their *principal-canonical shape*: the raw key with every
//! embedded `did:icn:` spelling replaced by the identifier bytes it decodes to.
//! Everything that is not a DID stays in the shape, so
//! `ledger:cleared_volume:<did>:USD` and `ledger:cleared_volume:<did>:EUR` stay
//! apart while two spellings of that account under `USD` come together. That
//! generalises to tuple keys — `outgoing_seq:<sender>||<recipient>` collides
//! only when *both* ends resolve to the same pair.
//!
//! Rows within a group are reported in [`Store::scan`] order, which is
//! lexicographic by key bytes. That order is not cosmetic: it decides the
//! survivor of every last-writer rebuild, and `Base256Emoji` (`🚀…`) spellings
//! sort after every ASCII one, so the survivor is attacker-selectable. The
//! ordinals are preserved so a reader can see which row would win.

use std::collections::BTreeMap;

/// The identity of a principal-canonical shape.
///
/// The substituted byte ranges travel with the shape rather than being implied
/// by it, because the shape alone is ambiguous: a key holding a literal 32-byte
/// sequence canonicalises to the same bytes as one holding an *encoded* DID in
/// that position. Grouping those together compared rows that are not the same
/// key at all — and, when the arities differed, indexed one row's spellings by
/// another's arity and aborted the gate.
type ShapeKey = (Vec<usize>, Vec<u8>);

/// Rows sharing one principal-canonical shape, with the identifiers that shape
/// decodes to.
type ShapeGroups = BTreeMap<ShapeKey, Vec<(RowRef, Vec<[u8; 32]>)>>;

use crate::Store;
use icn_identity::identifier_bytes_of_spelling;

/// The `did:icn:` scheme prefix every ICN principal spelling starts with.
const DID_PREFIX: &str = "did:icn:";

/// How a collision group may be resolved when the rows are merged.
///
/// This is the keyspace's *domain* rule, not a property of the data. It comes
/// from the N2-A0 inventory's mechanism column, and it is what decides whether
/// a group can be migrated automatically or must stop the migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MergeDisposition {
    /// Keep the maximum. Safe where the value is a monotonic floor or
    /// high-water mark and a lower survivor would *weaken* a guard — a replay
    /// floor, an outgoing sequence number, a vector-clock component.
    MaxMonotonic,
    /// Add the values. Safe where the value is an accumulated quantity and
    /// dropping a row would lose recorded volume.
    Sum,
    /// Keep every member. Safe where the value is a set or a set of edges and
    /// dropping a spelling would silently discard its members.
    Union,
    /// The rows are equivalent by construction; keeping any one loses nothing.
    Equivalent,
    /// The rows expire on their own, so the collision resolves without a
    /// migration step.
    ExpiresNaturally,
    /// **No authorized merge rule.** Two rows may encode contradictory state and
    /// no domain rule permits choosing or combining them. A group here must stop
    /// the migration for manual disposition rather than let a rebuild pick.
    FailClosed,
}

impl MergeDisposition {
    /// Whether a group under this disposition can be migrated without a human
    /// deciding the outcome.
    pub fn is_automatable(self) -> bool {
        !matches!(self, MergeDisposition::FailClosed)
    }

    /// A short stable label for reports.
    pub fn label(self) -> &'static str {
        match self {
            MergeDisposition::MaxMonotonic => "max-monotonic",
            MergeDisposition::Sum => "sum",
            MergeDisposition::Union => "union",
            MergeDisposition::Equivalent => "equivalent",
            MergeDisposition::ExpiresNaturally => "expires-naturally",
            MergeDisposition::FailClosed => "FAIL-CLOSED",
        }
    }
}

/// Whether a keyspace's merge rule carries authority yet.
///
/// A rule can be written down long before anyone with standing has approved it,
/// and the two must not look alike to the gate. Recording the difference is what
/// stops this crate — a generic storage layer — from authorizing a merge of
/// economic or institutional state on nothing but its own say-so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleBasis {
    /// The rule is established: implemented in live code, fixed by a canonical
    /// contract, or forced by an invariant. The rationale names which.
    Established,
    /// The rule is proposed and reads plausibly, but the domain that owns the
    /// state has not signed off. A collision here **fails closed** regardless of
    /// the disposition: a plausible rule is not an authorized one.
    AwaitingDomainSignOff,
}

/// One durable keyspace to scan.
///
/// A descriptor names the prefix to read and the disposition that applies to a
/// collision found under it. It deliberately does **not** describe how to parse
/// the key beyond its prefix: DID spellings are located by scanning for the
/// `did:icn:` scheme, which is layout-independent and so cannot drift out of
/// step with a keyspace that changes its separator.
#[derive(Debug, Clone)]
pub struct KeyspaceDescriptor {
    /// Stable identifier used in reports, e.g. `icn-net/replay_max_seq`.
    pub name: &'static str,
    /// Key prefix handed to [`Store::scan`].
    pub prefix: &'static [u8],
    /// Inventory row numbers this keyspace covers, for traceability.
    pub inventory_rows: &'static [u32],
    /// The merge rule that applies to a collision here.
    pub disposition: MergeDisposition,
    /// Whether that rule has been authorized by the domain that owns the state.
    pub basis: RuleBasis,
    /// Whether this keyspace's keys end with the DID.
    ///
    /// Where they do, the keyspace's own parser hands everything after the
    /// prefix to `Did::from_str`, so a key with anything trailing the spelling
    /// is one the real loader rejects. The generic scan cannot see that: its
    /// candidate run stops at the first non-body byte, so
    /// `replay_max_seq:<did>:junk` looked like a clean spelling plus residual
    /// key material. Stating it per keyspace keeps the scanner from having to
    /// reimplement each grammar while still catching the case.
    pub did_ends_key: bool,
    /// Whether this keyspace's own parser treats `/` as ending a DID.
    ///
    /// `/` is the only separator that is also a multibase body character, so
    /// where a spelling may be followed by one is a property of the individual
    /// key layout — not something the scanner can infer. No registered keyspace
    /// currently puts `/` immediately after a DID (`trust/edges/<a>:<b>` uses
    /// `:`; `trust/sequences/issuer/<did>` ends there), so every descriptor sets
    /// this `false` and `<did>/junk` is correctly unreadable rather than a
    /// readable principal with residual bytes the real loader would reject.
    pub slash_ends_did: bool,
    /// Why that rule, in one line — so a report explains itself.
    pub rationale: &'static str,
}

/// A DID spelling found inside a stored key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedDid {
    /// Byte offset of the spelling within the raw key.
    pub offset: usize,
    /// Byte offset one past the spelling within the raw key.
    ///
    /// Carried explicitly rather than derived as `offset + spelling.len()`,
    /// because `spelling` is lossy: a key holding invalid UTF-8 after
    /// `did:icn:` expands each bad byte into a three-byte replacement
    /// character, so the rendered length is not the length consumed from the
    /// key. Slicing with it walked off the end and panicked — turning an
    /// untrusted malformed row into a crashed gate rather than a blocked one.
    pub end: usize,
    /// The spelling exactly as stored, lossily rendered for display.
    pub spelling: String,
    /// The identifier bytes it decodes to, or `None` when it does not decode.
    pub identifier: Option<[u8; 32]>,
}

/// One stored row's membership in a group, carrying no payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRef {
    /// Position of this row in [`Store::scan`] order across the whole keyspace.
    ///
    /// `Store::scan` yields lexicographic key order, so a lower ordinal is a row
    /// an ascending last-writer rebuild sees *earlier* — and therefore one a
    /// later row overwrites.
    pub scan_ordinal: usize,
    /// The DID spellings embedded in this row's key, in key order.
    pub spellings: Vec<String>,
    /// Size of the stored value in bytes. Reported so an operator can judge
    /// effort; the value itself is never read into the report.
    pub value_len: usize,
}

/// Several stored rows that name one principal (or one principal tuple).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionGroup {
    /// Truncated hex fingerprints of the identifier bytes, in key order.
    ///
    /// Truncated because a report is an operational artifact: enough to tell
    /// groups apart and correlate across keyspaces, not enough to reconstruct
    /// the principals it describes.
    pub principal_fingerprints: Vec<String>,
    /// The rows in the group, in scan order. The last is the survivor of an
    /// ascending last-writer rebuild.
    pub rows: Vec<RowRef>,
    /// How many distinct stored spellings this group holds per DID position.
    pub representation_counts: Vec<usize>,
}

impl CollisionGroup {
    /// The row a last-writer rebuild would keep, in `Store::scan` order.
    pub fn last_writer_survivor(&self) -> Option<&RowRef> {
        self.rows.last()
    }
}

/// The result of scanning one keyspace. Aggregates only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyspaceReport {
    pub keyspace: String,
    pub inventory_rows: Vec<u32>,
    pub disposition: MergeDisposition,
    /// Whether the merge rule has been authorized by its owning domain.
    pub basis: RuleBasis,
    pub rationale: String,
    /// Rows read under the prefix.
    pub rows_scanned: usize,
    /// Rows whose key held at least one `did:icn:` spelling that decoded.
    pub rows_with_readable_did: usize,
    /// Rows whose key held a `did:icn:` spelling that did **not** decode.
    ///
    /// Never silently skipped: an unreadable key is a row a migration cannot
    /// classify, which is itself a fail-closed condition.
    pub rows_unreadable: usize,
    /// Rows under the prefix whose key held no `did:icn:` spelling at all.
    pub rows_without_did: usize,
    /// Distinct principals (or principal tuples, by canonical shape) present.
    pub distinct_principals: usize,
    /// Groups holding more than one stored row.
    pub collision_groups: Vec<CollisionGroup>,
}

impl KeyspaceReport {
    /// Rows that participate in a collision.
    pub fn rows_in_collisions(&self) -> usize {
        self.collision_groups.iter().map(|g| g.rows.len()).sum()
    }

    /// Whether this keyspace can be migrated without a human deciding an
    /// outcome.
    ///
    /// A keyspace with no collisions is automatable whatever its disposition —
    /// there is nothing to merge. A keyspace with collisions is automatable only
    /// when three things hold at once:
    ///
    /// * every row it holds was readable — a key that does not decode cannot be
    ///   classified, so it cannot be migrated on its own recognizance;
    /// * its disposition authorizes a merge at all;
    /// * that rule has been **authorized by the domain that owns the state**.
    ///
    /// The last condition is why [`RuleBasis`] exists. A merge rule that reads
    /// plausibly is not the same as one someone with standing has approved, and
    /// summing two balances because addition is the obvious arithmetic would be
    /// this crate deciding an economic question it has no authority over.
    pub fn is_automatable(&self) -> bool {
        if self.rows_unreadable > 0 {
            return false;
        }
        if self.collision_groups.is_empty() {
            return true;
        }
        self.disposition.is_automatable() && self.basis == RuleBasis::Established
    }

    /// Whether this keyspace must stop a migration for manual disposition.
    pub fn must_fail_closed(&self) -> bool {
        !self.is_automatable()
    }
}

/// A whole scan across every descriptor offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionReport {
    pub keyspaces: Vec<KeyspaceReport>,
}

impl CollisionReport {
    pub fn total_rows_scanned(&self) -> usize {
        self.keyspaces.iter().map(|k| k.rows_scanned).sum()
    }

    pub fn total_collision_groups(&self) -> usize {
        self.keyspaces
            .iter()
            .map(|k| k.collision_groups.len())
            .sum()
    }

    pub fn total_rows_in_collisions(&self) -> usize {
        self.keyspaces.iter().map(|k| k.rows_in_collisions()).sum()
    }

    pub fn total_rows_unreadable(&self) -> usize {
        self.keyspaces.iter().map(|k| k.rows_unreadable).sum()
    }

    /// Keyspaces that must stop a migration for manual disposition.
    pub fn blocking_keyspaces(&self) -> Vec<&KeyspaceReport> {
        self.keyspaces
            .iter()
            .filter(|k| k.must_fail_closed())
            .collect()
    }

    /// The scan's verdict: `true` only when every scanned keyspace can be
    /// migrated without a human deciding an outcome.
    pub fn is_clear(&self) -> bool {
        self.blocking_keyspaces().is_empty()
    }
}

/// Locate every `did:icn:` spelling embedded in a raw key.
///
/// A spelling runs from the scheme prefix to the first byte that cannot appear
/// in a multibase identifier. Multibase alphabets are alphanumeric plus a small
/// set of symbols, and every separator these keyspaces use (`:`, `/`, `|`, `"`,
/// and the raw bytes of a big-endian sequence number) falls outside the longest
/// such alphabet, so the scan terminates the token correctly without knowing the
/// layout.
///
/// Non-UTF-8 keys are handled: the search runs over the bytes and only the
/// matched token is required to be valid UTF-8.
pub fn find_embedded_dids(key: &[u8]) -> Vec<EmbeddedDid> {
    // Permissive by default. Callers without a descriptor — the store overview
    // and the uncovered-shape reporter — cannot know a key's layout, and a row
    // they identify is reported or blocked either way. A descriptor-driven scan
    // uses the keyspace's own rule instead.
    find_embedded_dids_with(key, true)
}

/// As [`find_embedded_dids`], with the keyspace's `/` policy.
pub fn find_embedded_dids_with(key: &[u8], slash_ends_did: bool) -> Vec<EmbeddedDid> {
    let needle = DID_PREFIX.as_bytes();
    let mut found = Vec::new();
    let mut i = 0usize;

    while i + needle.len() <= key.len() {
        if &key[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }

        let start = i;
        let mut limit = i + needle.len();
        // Bounded, because backtracking retries the decode once per byte
        // removed. A 32-byte identifier is longest in `Base2` — one character
        // per bit, 256 plus a sigil — so nothing beyond this can decode to one,
        // and without the bound a single key carrying tens of thousands of
        // base58 characters would make the audit quadratic in a length the
        // writer of that row chose.
        const MAX_IDENTIFIER_CHARS: usize = 300;
        let ceiling = key.len().min(limit + MAX_IDENTIFIER_CHARS);
        while limit < ceiling && is_multibase_body_byte(key[limit]) {
            limit += 1;
        }

        // Maximal munch, then validate and back off.
        //
        // The candidate alphabet has to include `+` and `/`, because the
        // production parser accepts `Base64`/`Base64Pad` spellings and those
        // contain both. But `/` is also a key separator in live keyspaces
        // (`trust/edges/<did>`, `.../<did>/suffix`), so an alphabet alone
        // cannot say where a spelling ends: the same byte is inside the token
        // in one keyspace and after it in another.
        //
        // So the longest candidate run is tried first and shortened from the
        // right until it decodes to a 32-byte identifier. Longest-match-wins
        // means a real spelling is never cut short by a character it legally
        // contains, while a spelling followed by `/suffix` still terminates at
        // the spelling. Only if nothing decodes is the whole run reported as
        // one unreadable token — a fact the scan must surface, never skip.
        let (end, identifier) = resolve_spelling(key, start, limit, slash_ends_did);

        let spelling = String::from_utf8_lossy(&key[start..end]).into_owned();
        found.push(EmbeddedDid {
            offset: start,
            end,
            spelling,
            identifier,
        });

        i = end.max(start + 1);
    }

    found
}

/// Find the longest prefix of `key[start..limit]` that decodes to a 32-byte
/// identifier, returning its end offset and the bytes.
///
/// When nothing decodes, the full run is returned with `None` so the caller
/// reports one unreadable token rather than silently dropping the row.
fn resolve_spelling(
    key: &[u8],
    start: usize,
    limit: usize,
    slash_ends_did: bool,
) -> (usize, Option<[u8; 32]>) {
    let mut end = limit;
    while end > start {
        // Only attempt at a UTF-8 boundary: `Base256Emoji` spellings are
        // multi-byte, and slicing one mid-character would both fail to decode
        // and misreport where the spelling ends.
        if let Ok(candidate) = std::str::from_utf8(&key[start..end]) {
            if let Ok(bytes) = identifier_bytes_of_spelling(candidate) {
                // A shorter-than-maximal match means bytes were left over, and
                // those bytes need an explanation. `/` is the only separator any
                // live keyspace uses that is also a multibase body character, so
                // it is the only remainder that can be attributed to key
                // structure rather than to a malformed spelling.
                //
                // Anything else — `…<valid-did>junk` — is ambiguous: the
                // keyspace's own parser would consume the whole suffix as the
                // identifier and reject it, so calling the prefix readable would
                // report a principal for a row the real loader cannot read, and
                // quietly lower the unreadable count that exists to fail closed.
                if end < limit && !(slash_ends_did && key.get(end) == Some(&b'/')) {
                    return (limit, None);
                }
                return (end, Some(bytes));
            }
        }
        end -= 1;
    }
    (limit, None)
}

/// Whether a byte may continue a multibase identifier body.
///
/// Covers every alphabet the production parser accepts — verified against
/// `Did::from_str` for all 23 `multibase::Base` variants, which includes
/// `Base64` (`+`, `/`), `Base64Url` (`-`, `_`) and their padded forms (`=`).
/// Being permissive here is safe because [`resolve_spelling`] decides where the
/// spelling actually ends by decoding, not by the alphabet alone.
fn is_multibase_body_byte(b: u8) -> bool {
    // Non-ASCII bytes continue the token: `Base256Emoji` spellings are
    // multi-byte UTF-8, and truncating one at its first continuation byte would
    // both fail to decode and split a real spelling.
    if b >= 0x80 {
        return true;
    }
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'=' | b'+' | b'/')
}

/// Group rows by principal-canonical shape and report the collisions.
///
/// `rows` must be in [`Store::scan`] order; the ordinals recorded against each
/// group depend on it.
fn build_report(descriptor: &KeyspaceDescriptor, rows: Vec<(Vec<u8>, usize)>) -> KeyspaceReport {
    // Keyed by the canonical shape: the raw key with each DID spelling replaced
    // by its decoded identifier bytes. `BTreeMap` so the report is deterministic
    // across runs regardless of hash seeding.
    let mut groups: ShapeGroups = BTreeMap::new();

    let mut rows_with_readable_did = 0usize;
    let mut rows_unreadable = 0usize;
    let mut rows_without_did = 0usize;
    let rows_scanned = rows.len();

    for (scan_ordinal, (key, value_len)) in rows.into_iter().enumerate() {
        let embedded = find_embedded_dids_with(&key, descriptor.slash_ends_did);

        if embedded.is_empty() {
            rows_without_did += 1;
            continue;
        }

        // One unreadable spelling makes the whole row unclassifiable: its
        // canonical shape cannot be computed, so it cannot be grouped, and a
        // migration cannot know what it would merge into.
        //
        // Collecting into `Option<Vec<_>>` short-circuits on the first
        // unreadable spelling and hands back the decoded bytes on success, so
        // the "every spelling decoded" invariant is carried by the type rather
        // than re-asserted with an `expect` further down.
        let Some(identifiers) = embedded
            .iter()
            .map(|d| d.identifier)
            .collect::<Option<Vec<[u8; 32]>>>()
        else {
            rows_unreadable += 1;
            continue;
        };

        // The keyspace says its keys end with the DID, so anything after the
        // last spelling is material its own parser would refuse.
        if descriptor.did_ends_key {
            if let Some(last) = embedded.last() {
                if last.end != key.len() {
                    rows_unreadable += 1;
                    continue;
                }
            }
        }

        rows_with_readable_did += 1;

        let mut shape = Vec::with_capacity(key.len());
        let mut cursor = 0usize;
        let mut spellings = Vec::with_capacity(embedded.len());
        let mut substitutions = Vec::with_capacity(embedded.len());

        for (did, bytes) in embedded.iter().zip(&identifiers) {
            shape.extend_from_slice(&key[cursor..did.offset]);
            substitutions.push(shape.len());
            shape.extend_from_slice(bytes);
            cursor = did.end;
            spellings.push(did.spelling.clone());
        }
        shape.extend_from_slice(&key[cursor..]);

        groups.entry((substitutions, shape)).or_default().push((
            RowRef {
                scan_ordinal,
                spellings,
                value_len,
            },
            identifiers,
        ));
    }

    let distinct_principals = groups.len();

    let collision_groups = groups
        .into_values()
        .filter(|members| members.len() > 1)
        .map(|members| {
            let identifiers = members[0].1.clone();
            let positions = identifiers.len();

            // How many distinct spellings appear at each DID position. A tuple
            // key can collide because one end was re-spelled while the other was
            // not, and that count is what says which end did it.
            let representation_counts = (0..positions)
                .map(|pos| {
                    members
                        .iter()
                        .map(|(row, _)| row.spellings[pos].as_str())
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                })
                .collect();

            CollisionGroup {
                principal_fingerprints: identifiers.iter().map(fingerprint).collect(),
                rows: members.into_iter().map(|(row, _)| row).collect(),
                representation_counts,
            }
        })
        .collect();

    KeyspaceReport {
        keyspace: descriptor.name.to_string(),
        inventory_rows: descriptor.inventory_rows.to_vec(),
        disposition: descriptor.disposition,
        basis: descriptor.basis,
        rationale: descriptor.rationale.to_string(),
        rows_scanned,
        rows_with_readable_did,
        rows_unreadable,
        rows_without_did,
        distinct_principals,
        collision_groups,
    }
}

/// A short, stable, non-reversible label for a principal.
///
/// Eight hex characters of the identifier: enough to tell groups apart in a
/// report and to correlate one principal across keyspaces, and not enough to
/// reconstruct the principal from the report.
fn fingerprint(identifier: &[u8; 32]) -> String {
    identifier[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// What a store actually contains, independent of the descriptors.
///
/// A per-keyspace report of "0 rows" is indistinguishable from a scanner that
/// read nothing at all, so a scan that cannot say how many rows the store holds
/// is not evidence. This makes an empty result falsifiable: if `total_rows` is
/// non-zero and `namespaces` lists prefixes the descriptors do not cover, the
/// zeros are a real absence rather than a broken read.
///
/// Only the leading namespace segment of each key is retained — the text before
/// the first `:` or `/`. That is store structure, not stored data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreOverview {
    /// Every row in the store, under any prefix.
    pub total_rows: usize,
    /// Leading namespace segment to row count, ascending by segment.
    pub namespaces: BTreeMap<String, usize>,
    /// Rows anywhere in the store whose key embeds a `did:icn:` spelling.
    ///
    /// Non-zero here with zero in every descriptor means the inventory's
    /// keyspace list does not cover where this deployment keeps its principals.
    pub rows_with_embedded_did: usize,
}

/// Read a store's shape. Read-only.
pub fn store_overview(store: &dyn Store) -> anyhow::Result<StoreOverview> {
    // Keys only: this pass counts and classifies, and never needs a payload.
    let keys = store.scan_keys(b"")?;
    let mut namespaces: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows_with_embedded_did = 0usize;

    for key in &keys {
        let cut = key
            .iter()
            .position(|b| *b == b':' || *b == b'/')
            .unwrap_or(key.len())
            .min(48);
        let segment = String::from_utf8_lossy(&key[..cut]).into_owned();
        *namespaces.entry(segment).or_insert(0) += 1;

        if !find_embedded_dids(key).is_empty() {
            rows_with_embedded_did += 1;
        }
    }

    Ok(StoreOverview {
        total_rows: keys.len(),
        namespaces,
        rows_with_embedded_did,
    })
}

/// A namespace deliberately left outside this tranche, behind a named gate.
///
/// Deferral is not coverage and it is not a clean result. It is a recorded
/// decision that some *other* gate owns the namespace, and it exists so that an
/// intentionally excluded keyspace is distinguishable from one nobody noticed.
/// Without that distinction the only safe verdict on any uncovered row would be
/// "blocked", and the gate would be unusable; with it, an accidental omission
/// still blocks while a reviewed exclusion does not.
///
/// A deferred namespace is **never inspected**: only the fact that it exists,
/// how many principal-bearing rows it holds, and which gate owns it.
#[derive(Debug, Clone)]
pub struct DeferredNamespace {
    /// Stable identifier used in reports.
    pub name: &'static str,
    /// Key prefix whose rows this deferral accounts for.
    pub prefix: &'static [u8],
    /// The gate that owns the namespace — named, so the deferral is auditable.
    pub gate: &'static str,
    /// Inventory rows this namespace corresponds to.
    pub inventory_rows: &'static [u32],
}

/// The namespaces N2-A deliberately does not scan, each behind a named gate.
///
/// Both entries are decisions recorded elsewhere, not judgements made here:
/// governance votes are behind the §7.5 membership/vote migration gate, and the
/// security namespace belongs to its own dedicated workflow.
pub fn n2a_deferred_namespaces() -> Vec<DeferredNamespace> {
    vec![
        DeferredNamespace {
            name: "governance/votes",
            prefix: b"gov:vote:",
            gate: "IDENTITY_SEMANTICS §7.5 membership/vote migration gate",
            inventory_rows: &[23],
        },
        DeferredNamespace {
            name: "rpc/auth-challenges",
            prefix: b"auth:challenge:",
            gate: "dedicated security workflow (TTL-bounded; contents not inspected)",
            inventory_rows: &[29],
        },
        DeferredNamespace {
            name: "security/misbehavior",
            prefix: b"security:",
            gate: "dedicated security workflow (contents not inspected)",
            inventory_rows: &[5, 6, 7, 8, 38],
        },
    ]
}

/// Principal-bearing rows per deferred namespace. Counts only.
pub fn deferred_did_row_counts(
    store: &dyn Store,
    deferrals: &[DeferredNamespace],
) -> anyhow::Result<Vec<(String, usize)>> {
    let mut out = Vec::with_capacity(deferrals.len());
    for d in deferrals {
        let n = store
            .scan_keys(d.prefix)?
            .into_iter()
            .filter(|key| !find_embedded_dids(key).is_empty())
            .count();
        out.push((d.name.to_string(), n));
    }
    Ok(out)
}

/// Principal-bearing rows that no registered keyspace covers, grouped by the
/// *shape* of their key.
///
/// A per-keyspace report of zero collisions only ever speaks for the rows those
/// keyspaces matched. A store can hold principal-keyed rows under a prefix the
/// registry does not name — and those rows collapse on the same first start of a
/// key-equality binary, unexamined. Reporting "clean" without accounting for
/// them would be exactly the false all-clear this tool exists to prevent.
///
/// The returned map is keyed by a **masked shape**: the key with every
/// `did:icn:` spelling replaced by `<did>` and every non-printable byte by `.`,
/// truncated. That reveals the namespace structure a reviewer needs in order to
/// decide whether the prefix belongs in the registry, and reveals no identifier
/// and no payload.
pub fn uncovered_did_key_shapes(
    store: &dyn Store,
    descriptors: &[KeyspaceDescriptor],
    deferrals: &[DeferredNamespace],
) -> anyhow::Result<BTreeMap<String, usize>> {
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();

    for key in store.scan_keys(b"")? {
        let embedded = find_embedded_dids(&key);
        if embedded.is_empty() {
            continue;
        }
        let covered = descriptors.iter().any(|d| key.starts_with(d.prefix));
        let deferred = deferrals.iter().any(|d| key.starts_with(d.prefix));
        if covered || deferred {
            continue;
        }
        *shapes.entry(mask_key(&key, &embedded)).or_insert(0) += 1;
    }

    Ok(shapes)
}

/// Render a key as a structural shape: DID spellings become `<did>`,
/// non-printable bytes become `.`, and the result is truncated.
fn mask_key(key: &[u8], embedded: &[EmbeddedDid]) -> String {
    const MAX: usize = 72;

    let mut out = String::new();
    let mut cursor = 0usize;

    for did in embedded {
        push_printable(&mut out, &key[cursor..did.offset]);
        out.push_str("<did>");
        cursor = did.end;
    }
    push_printable(&mut out, &key[cursor..]);

    if out.chars().count() > MAX {
        out = out.chars().take(MAX).collect::<String>() + "…";
    }
    out
}

fn push_printable(out: &mut String, bytes: &[u8]) {
    for b in bytes {
        if b.is_ascii_graphic() || *b == b' ' {
            out.push(*b as char);
        } else {
            out.push('.');
        }
    }
}

/// Scan one keyspace. Read-only.
pub fn scan_keyspace(
    store: &dyn Store,
    descriptor: &KeyspaceDescriptor,
) -> anyhow::Result<KeyspaceReport> {
    // Key plus value *size*: the report needs how big a row is, never what is
    // in it, so nothing downstream of this line can read a stored payload even
    // by mistake — and on a large keyspace the values are never all held at
    // once.
    let rows = store.scan_key_sizes(descriptor.prefix)?;
    Ok(build_report(descriptor, rows))
}

/// Scan every descriptor against one store. Read-only.
pub fn scan_store(
    store: &dyn Store,
    descriptors: &[KeyspaceDescriptor],
) -> anyhow::Result<CollisionReport> {
    let mut keyspaces = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        keyspaces.push(scan_keyspace(store, descriptor)?);
    }
    Ok(CollisionReport { keyspaces })
}

/// Everything one store's scan established, and the verdict that follows.
///
/// The verdict lives here rather than in the runner because it *is* the gate:
/// a binary that rendered a report and decided separately what it meant could
/// drift from the library, and the first symptom would be an exit status that
/// disagreed with the text above it.
#[derive(Debug, Clone)]
pub struct CoverageAudit {
    pub report: CollisionReport,
    pub overview: StoreOverview,
    /// Principal-bearing rows per deliberately deferred namespace.
    pub deferred: Vec<(String, usize)>,
    /// Principal-bearing rows under no registered keyspace and no named gate.
    pub uncovered: BTreeMap<String, usize>,
    /// Principal-bearing rows in named trees `Store::scan` cannot reach.
    pub unreachable_did_rows: usize,
}

impl CoverageAudit {
    /// Rows accounted for by no registered keyspace and no named gate.
    pub fn uncovered_did_rows(&self) -> usize {
        self.uncovered.values().sum()
    }

    /// Rows a named gate defers. Deferred is neither scanned nor cleared.
    pub fn deferred_did_rows(&self) -> usize {
        self.deferred.iter().map(|(_, n)| *n).sum()
    }

    /// The store is clear only when every principal-bearing row it holds was
    /// accounted for, and every keyspace that accounted for one can be migrated
    /// without a human deciding an outcome.
    ///
    /// A principal-bearing row is accounted for in exactly one of three ways,
    /// and there is deliberately no fourth:
    ///
    /// 1. a registered keyspace interpreted it, so the collision result speaks
    ///    for it;
    /// 2. a named gate defers it — [`n2a_deferred_namespaces`] says which, and
    ///    that exclusion was reviewed;
    /// 3. nothing did, which **blocks** — a row nobody has classified is
    ///    precisely the row that collapses unexamined on the first start of a
    ///    key-equality binary.
    ///
    /// Case 3 is why uncovered rows are consulted here. Without them a keyspace
    /// added after this tool was written, or simply left out of the registry,
    /// would pass the migration gate in silence — the exact failure this tool
    /// exists to prevent, and one that already happened once (§5 rows #71 and
    /// #36 were live and unregistered).
    pub fn is_clear(&self) -> bool {
        self.report.is_clear() && self.unreachable_did_rows == 0 && self.uncovered_did_rows() == 0
    }
}

/// Audit one store: collisions, coverage, deferrals and uncovered rows.
///
/// `unreachable_did_rows` is supplied by the caller because reaching named
/// trees requires the concrete backend, not the [`Store`] trait.
pub fn audit_store(
    store: &dyn Store,
    descriptors: &[KeyspaceDescriptor],
    deferrals: &[DeferredNamespace],
    unreachable_did_rows: usize,
) -> anyhow::Result<CoverageAudit> {
    Ok(CoverageAudit {
        report: scan_store(store, descriptors)?,
        overview: store_overview(store)?,
        deferred: deferred_did_row_counts(store, deferrals)?,
        uncovered: uncovered_did_key_shapes(store, descriptors, deferrals)?,
        unreachable_did_rows,
    })
}

/// The non-security-sensitive durable keyspaces N2-A must clear before `Did`
/// equality becomes key equality.
///
/// Sourced from the N2-A0 inventory's §12 *Concrete list* (`NEEDS MIGRATION`)
/// **and** the `SILENT-MERGE RISK` durable rows of §5. Scoping the registry to
/// the `NEEDS MIGRATION` list alone was a mistake found by scanning real
/// deployment data: `SILENT-MERGE RISK` is precisely the class that merges
/// without announcing itself, and live rows were found in two such keyspaces
/// (§5 rows #71 and #36) that the first registry did not cover.
///
/// Two inventory rows are
/// deliberately absent and are **not** cleared by this scan:
///
/// * the misbehavior keyspace (rows #5–#8/#38) and the auth-challenge keyspace
///   (row #29) are security-sensitive namespaces. Their existence and their
///   migration dependency are recorded here; their inspection belongs to the
///   dedicated security workflow, not to this tool.
/// * the governance vote keyspace (row #23) is behind the separate §7.5
///   membership/vote migration gate and must not be migrated as part of N2-A.
///
/// Class-C structures (rows #45, #54, #57) hold their DIDs inside serialized
/// *values*, not keys, so they are not prefix-scannable and are dispositioned in
/// the tranche documentation rather than here.
pub fn n2a_keyspaces() -> Vec<KeyspaceDescriptor> {
    vec![
        KeyspaceDescriptor {
            name: "icn-net/replay_max_seq",
            prefix: b"replay_max_seq:",
            inventory_rows: &[1, 37],
            disposition: MergeDisposition::MaxMonotonic,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: true,
            rationale: "Replay floor. A lower survivor weakens the guard, so the merge keeps the \
                        maximum, which can only reject more than any single row did.",
        },
        KeyspaceDescriptor {
            name: "icn-net/replay_finalized",
            prefix: b"replay_finalized:",
            inventory_rows: &[2, 37],
            disposition: MergeDisposition::Union,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "Finalized-sequence set. Dropping a spelling's rows would re-open replay \
                        for the sequences it recorded, so the merge is a union.",
        },
        KeyspaceDescriptor {
            name: "icn-net/replay_sender_regime",
            prefix: b"replay_sender_regime:",
            inventory_rows: &[3, 37],
            disposition: MergeDisposition::FailClosed,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: true,
            rationale: "Two rows can assert different regimes for one sender, which is a \
                        contradiction no domain rule resolves. The live loader already declines \
                        to collapse these (#2644); a migration must not decide it either.",
        },
        KeyspaceDescriptor {
            name: "icn-net/outgoing_seq",
            prefix: b"outgoing_seq:",
            inventory_rows: &[4],
            disposition: MergeDisposition::MaxMonotonic,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "Outgoing sequence high-water for a (sender, recipient) pair. A lower \
                        survivor is a nonce regression, so the merge keeps the maximum.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/balance",
            prefix: b"ledger:balance:",
            inventory_rows: &[9, 39, 40],
            disposition: MergeDisposition::Sum,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "Accumulated balances. Overwriting drops a spelling's recorded position \
                        entirely, so the merge sums rather than elects a survivor.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/cleared_volume",
            prefix: b"ledger:cleared_volume:",
            inventory_rows: &[69],
            disposition: MergeDisposition::Sum,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "Accumulated cleared volume per (account, currency). Currency stays in the \
                        canonical shape, so only same-currency rows merge, and they sum.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/frozen",
            prefix: b"ledger:frozen:",
            inventory_rows: &[42, 68],
            disposition: MergeDisposition::Union,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: true,
            rationale: "Freeze records. Unfreeze deletes one spelling only, so electing a \
                        survivor can fail open; the merge is a union of the freezes.",
        },
        KeyspaceDescriptor {
            name: "icn-trust/edges",
            prefix: b"trust/edges/",
            inventory_rows: &[30],
            disposition: MergeDisposition::Union,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "Trust edges keyed by (source, target). A dropped spelling takes its edges \
                        with it, so the merge unions the edge sets.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/journal",
            prefix: b"ledger:journal:",
            inventory_rows: &[39, 40],
            disposition: MergeDisposition::Equivalent,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "Journal entries are content-addressed by entry hash; DIDs appear inside \
                        the value, not the key. Scanned to confirm the key carries no spelling.",
        },
        KeyspaceDescriptor {
            name: "trust-app/sequences_receiver",
            prefix: b"trust/sequences/receiver/",
            inventory_rows: &[71],
            disposition: MergeDisposition::MaxMonotonic,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: true,
            rationale: "Last-seen attestation sequence per issuer — a replay floor. A lower \
                        survivor accepts stale attestations, so the merge keeps the maximum, \
                        matching the established replay_max_seq precedent.",
        },
        KeyspaceDescriptor {
            name: "trust-app/sequences_issuer",
            prefix: b"trust/sequences/issuer/",
            inventory_rows: &[71],
            disposition: MergeDisposition::MaxMonotonic,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: true,
            rationale: "This node's own outgoing attestation sequence. A lower survivor re-issues \
                        a sequence number already used, which the uniqueness invariant forbids, \
                        so the merge keeps the maximum.",
        },
        KeyspaceDescriptor {
            name: "icn-coop/member",
            prefix: b"member:",
            inventory_rows: &[36],
            disposition: MergeDisposition::FailClosed,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: true,
            rationale: "Cooperative membership. Merging two rows decides who is a member of an \
                        institution, which is an institutional judgement no identity-layer rule \
                        authorizes; it is also adjacent to the separate §7.5 membership gate. \
                        Fail closed pending a governance-domain decision.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SledStore;

    /// Build a `did:icn:` spelling of `bytes` in the given multibase base.
    fn spell(bytes: &[u8; 32], base: multibase::Base) -> String {
        format!("did:icn:{}", multibase::encode(base, bytes))
    }

    fn principal(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    /// The two spellings the whole hazard rests on: one principal, two keys.
    fn two_spellings(seed: u8) -> (String, String) {
        let bytes = principal(seed);
        (
            spell(&bytes, multibase::Base::Base58Btc),
            spell(&bytes, multibase::Base::Base16Lower),
        )
    }

    fn descriptor(disposition: MergeDisposition) -> KeyspaceDescriptor {
        descriptor_with(disposition, RuleBasis::Established)
    }

    fn descriptor_with(disposition: MergeDisposition, basis: RuleBasis) -> KeyspaceDescriptor {
        KeyspaceDescriptor {
            name: "test/keyspace",
            prefix: b"test:",
            inventory_rows: &[0],
            disposition,
            basis,
            slash_ends_did: false,
            did_ends_key: false,
            rationale: "test fixture",
        }
    }

    /// Fixtures run against a real `sled` store rather than a hand-rolled
    /// double. The scan's central claim — that `Store::scan` order decides
    /// which row a last-writer rebuild keeps — is a claim about the actual
    /// backend, and a simulated store would only restate the test's own sort.
    fn store_with(rows: &[(&str, &[u8])]) -> SledStore {
        let store = SledStore::temporary().unwrap();
        for (key, value) in rows {
            store.put(key.as_bytes(), value).unwrap();
        }
        store
    }

    #[test]
    fn distinct_spellings_of_one_principal_decode_to_one_identifier() {
        // Guards the fixtures themselves: if these two spellings were not
        // genuinely distinct strings naming one principal, every collision test
        // below would pass vacuously.
        let (a, b) = two_spellings(7);
        assert_ne!(a, b, "fixture must use two distinct spellings");
        assert_eq!(
            identifier_bytes_of_spelling(&a).unwrap(),
            identifier_bytes_of_spelling(&b).unwrap(),
            "fixture spellings must name one principal"
        );
    }

    #[test]
    fn no_collision_data_reports_no_groups() {
        let one = spell(&principal(1), multibase::Base::Base58Btc);
        let two = spell(&principal(2), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("test:{one}"), b"v"),
            (&format!("test:{two}"), b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
        assert_eq!(report.rows_in_collisions(), 0);
        assert!(report.is_automatable());
        assert!(!report.must_fail_closed());
    }

    #[test]
    fn two_representations_of_one_principal_form_one_group() {
        let (a, b) = two_spellings(3);
        let other = spell(&principal(9), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("test:{a}"), b"vv"),
            (&format!("test:{b}"), b"v"),
            (&format!("test:{other}"), b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        assert_eq!(report.rows_scanned, 3);
        // Two spellings collapse to one principal, plus the unrelated one.
        assert_eq!(report.distinct_principals, 2);
        assert_eq!(report.collision_groups.len(), 1);

        let group = &report.collision_groups[0];
        assert_eq!(group.rows.len(), 2);
        assert_eq!(group.representation_counts, vec![2]);
        assert_eq!(report.rows_in_collisions(), 2);
    }

    #[test]
    fn several_representations_of_one_principal_form_one_group() {
        let bytes = principal(5);
        let spellings = [
            spell(&bytes, multibase::Base::Base58Btc),
            spell(&bytes, multibase::Base::Base16Lower),
            spell(&bytes, multibase::Base::Base32Lower),
            spell(&bytes, multibase::Base::Base64),
        ];
        let rows: Vec<(String, &[u8])> = spellings
            .iter()
            .map(|s| (format!("test:{s}"), b"v".as_slice()))
            .collect();
        let store = store_with(
            &rows
                .iter()
                .map(|(k, v)| (k.as_str(), *v))
                .collect::<Vec<_>>(),
        );

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::MaxMonotonic)).unwrap();

        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(report.collision_groups[0].rows.len(), 4);
        assert_eq!(report.collision_groups[0].representation_counts, vec![4]);
    }

    #[test]
    fn malformed_keys_are_counted_not_skipped_and_block_automation() {
        let good = spell(&principal(4), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("test:{good}"), b"v"),
            // Valid multibase prefix, but not 32 bytes once decoded.
            ("test:did:icn:z6Mkh", b"v"),
            // Not multibase at all.
            ("test:did:icn:!!!!", b"v"),
            // DID-shaped with an empty identifier.
            ("test:did:icn:", b"v"),
            // No DID in the key at all.
            ("test:housekeeping", b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        assert_eq!(report.rows_scanned, 5);
        assert_eq!(report.rows_with_readable_did, 1);
        // `!!!!` and the empty identifier are not multibase-body bytes, so both
        // rows yield a DID-shaped token that fails to decode.
        assert_eq!(report.rows_unreadable, 3);
        assert_eq!(report.rows_without_did, 1);
        // An unreadable row cannot be classified, so the keyspace cannot be
        // migrated on its own recognizance even though nothing collided.
        assert!(report.collision_groups.is_empty());
        assert!(!report.is_automatable());
        assert!(report.must_fail_closed());
    }

    #[test]
    fn collision_group_with_a_known_merge_rule_is_automatable() {
        let (a, b) = two_spellings(11);
        let store = store_with(&[(&format!("test:{a}"), b"v"), (&format!("test:{b}"), b"v")]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::MaxMonotonic)).unwrap();

        assert_eq!(report.collision_groups.len(), 1);
        assert!(report.disposition.is_automatable());
        assert!(report.is_automatable());
        assert!(!report.must_fail_closed());
    }

    #[test]
    fn collision_group_with_no_authorized_merge_rule_fails_closed() {
        let (a, b) = two_spellings(13);
        let store = store_with(&[(&format!("test:{a}"), b"v"), (&format!("test:{b}"), b"v")]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::FailClosed)).unwrap();

        assert_eq!(report.collision_groups.len(), 1);
        assert!(!report.disposition.is_automatable());
        assert!(report.must_fail_closed());

        // And the whole-report verdict must inherit it, not average it away.
        let full = CollisionReport {
            keyspaces: vec![report],
        };
        assert!(!full.is_clear());
        assert_eq!(full.blocking_keyspaces().len(), 1);
    }

    #[test]
    fn fail_closed_disposition_without_collisions_does_not_block() {
        // The disposition is a rule about merging, so it must only bite when
        // there is something to merge. Otherwise every clean store with a
        // fail-closed keyspace would block the tranche forever.
        let one = spell(&principal(21), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("test:{one}"), b"v")]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::FailClosed)).unwrap();

        assert!(report.collision_groups.is_empty());
        assert!(report.is_automatable());
    }

    #[test]
    fn residual_key_fields_keep_distinct_principals_apart() {
        // `ledger:cleared_volume:<did>:<currency>` must not merge two currencies
        // for one account. The currency is not a DID, so it stays in the shape.
        let (a, b) = two_spellings(17);
        let store = store_with(&[
            (&format!("test:{a}:USD"), b"v"),
            (&format!("test:{a}:EUR"), b"v"),
            (&format!("test:{b}:USD"), b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        // Two shapes: (principal, USD) and (principal, EUR).
        assert_eq!(report.distinct_principals, 2);
        // Only the USD pair collides.
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(report.collision_groups[0].rows.len(), 2);
    }

    #[test]
    fn tuple_keys_collide_only_when_both_ends_match() {
        // `outgoing_seq:<sender>||<recipient>`: a re-spelled sender against the
        // same recipient collides; a different recipient does not.
        let (sender_a, sender_b) = two_spellings(23);
        let recipient = spell(&principal(24), multibase::Base::Base58Btc);
        let other_recipient = spell(&principal(25), multibase::Base::Base58Btc);

        let store = store_with(&[
            (&format!("test:{sender_a}||{recipient}"), b"v"),
            (&format!("test:{sender_b}||{recipient}"), b"v"),
            (&format!("test:{sender_a}||{other_recipient}"), b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::MaxMonotonic)).unwrap();

        assert_eq!(report.distinct_principals, 2);
        assert_eq!(report.collision_groups.len(), 1);

        let group = &report.collision_groups[0];
        assert_eq!(group.principal_fingerprints.len(), 2);
        // The sender end was re-spelled; the recipient end was not.
        assert_eq!(group.representation_counts, vec![2, 1]);
    }

    #[test]
    fn group_rows_are_reported_in_scan_order_so_the_survivor_is_visible() {
        // `Store::scan` yields lexicographic key order, and that order decides
        // which row a last-writer rebuild keeps. The report must expose it
        // rather than leave the survivor implicit.
        let bytes = principal(31);
        let base16 = spell(&bytes, multibase::Base::Base16Lower); // 'f' sigil
        let base58 = spell(&bytes, multibase::Base::Base58Btc); // 'z' sigil
        assert!(base16 < base58, "fixture assumes 'f' sorts before 'z'");

        let store = store_with(&[
            (&format!("test:{base58}"), b"v"),
            (&format!("test:{base16}"), b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::MaxMonotonic)).unwrap();
        let group = &report.collision_groups[0];

        assert_eq!(group.rows[0].spellings[0], base16);
        assert_eq!(group.rows[1].spellings[0], base58);
        assert!(group.rows[0].scan_ordinal < group.rows[1].scan_ordinal);
        // The last row in scan order is the one a last-writer rebuild keeps.
        assert_eq!(group.last_writer_survivor().unwrap().spellings[0], base58);
    }

    #[test]
    fn a_non_ascii_spelling_sorts_last_and_would_win_a_last_writer_rebuild() {
        // Base256Emoji spellings are non-ASCII, so they sort after every ASCII
        // spelling and win an ascending last-writer rebuild. That is the
        // attacker-selectable survivor the inventory warns about (§12.1 item 3),
        // and the scan must surface it rather than hide it.
        let bytes = principal(41);
        let ascii = spell(&bytes, multibase::Base::Base58Btc);
        let emoji = spell(&bytes, multibase::Base::Base256Emoji);

        let store = store_with(&[
            (&format!("test:{ascii}"), b"v"),
            (&format!("test:{emoji}"), b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::MaxMonotonic)).unwrap();

        assert_eq!(report.distinct_principals, 1, "emoji spelling must decode");
        let group = &report.collision_groups[0];
        assert_eq!(group.rows.len(), 2);
        assert_eq!(
            group.last_writer_survivor().unwrap().spellings[0],
            emoji,
            "the non-ASCII spelling sorts last and wins the rebuild"
        );
    }

    #[test]
    fn the_scan_writes_nothing() {
        let (a, b) = two_spellings(51);
        let store = store_with(&[(&format!("test:{a}"), b"v"), (&format!("test:{b}"), b"v")]);
        let before = store.scan(b"").unwrap();

        let _ = scan_store(&store, &n2a_keyspaces()).unwrap();
        let _ = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        assert_eq!(before, store.scan(b"").unwrap(), "scan must not mutate");
    }

    #[test]
    fn reports_carry_no_stored_payload() {
        let (a, b) = two_spellings(61);
        let store = store_with(&[
            (&format!("test:{a}"), b"SENSITIVE-PAYLOAD"),
            (&format!("test:{b}"), b"ALSO-SENSITIVE"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();
        let rendered = format!("{report:?}");

        assert!(!rendered.contains("SENSITIVE-PAYLOAD"));
        assert!(!rendered.contains("ALSO-SENSITIVE"));
        // The length is kept, because effort estimation needs it.
        assert_eq!(report.collision_groups[0].rows[0].value_len, 14);
    }

    #[test]
    fn store_overview_makes_an_empty_descriptor_result_falsifiable() {
        // A store holding principals under a namespace no descriptor covers
        // must report zero collisions AND a non-zero store — otherwise "0 rows"
        // could not be told apart from a scanner that read nothing.
        let one = spell(&principal(71), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("unlisted:{one}"), b"v"),
            ("unlisted:housekeeping", b"v"),
        ]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();
        assert_eq!(report.rows_scanned, 0, "descriptor prefix matches nothing");

        let overview = store_overview(&store).unwrap();
        assert_eq!(overview.total_rows, 2);
        assert_eq!(overview.rows_with_embedded_did, 1);
        assert_eq!(overview.namespaces.get("unlisted"), Some(&2));
    }

    #[test]
    fn store_overview_reports_an_genuinely_empty_store_as_empty() {
        let store = SledStore::temporary().unwrap();
        let overview = store_overview(&store).unwrap();
        assert_eq!(overview.total_rows, 0);
        assert_eq!(overview.rows_with_embedded_did, 0);
        assert!(overview.namespaces.is_empty());
    }

    #[test]
    fn uncovered_principal_rows_are_reported_by_shape_not_hidden() {
        // The decisive case: a store whose registered keyspaces are clean but
        // which also holds principal-keyed rows under an unregistered prefix.
        // Reporting only the clean keyspaces would be a false all-clear.
        let (a, b) = two_spellings(81);
        let store = store_with(&[
            (&format!("test:{a}"), b"v"),
            (&format!("unregistered/members/{a}/role"), b"v"),
            (&format!("unregistered/members/{b}/role"), b"v"),
            ("unregistered/housekeeping", b"v"),
        ]);

        let descriptors = [descriptor(MergeDisposition::Sum)];
        let report = scan_keyspace(&store, &descriptors[0]).unwrap();
        assert!(
            report.collision_groups.is_empty(),
            "registered keyspace is clean"
        );

        let shapes = uncovered_did_key_shapes(&store, &descriptors, &[]).unwrap();
        assert_eq!(
            shapes.get("unregistered/members/<did>/role"),
            Some(&2),
            "both uncovered principal rows must surface, masked, under one shape"
        );
        // The row with no DID is not principal-bearing and must not appear.
        assert_eq!(shapes.len(), 1);
    }

    #[test]
    fn covered_rows_are_not_reported_as_uncovered() {
        let one = spell(&principal(83), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("test:{one}"), b"v")]);
        let descriptors = [descriptor(MergeDisposition::Sum)];

        let shapes = uncovered_did_key_shapes(&store, &descriptors, &[]).unwrap();
        assert!(
            shapes.is_empty(),
            "a registered row is covered, not uncovered"
        );
    }

    #[test]
    fn masked_shapes_carry_no_identifier_or_payload() {
        let one = spell(&principal(87), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("elsewhere/{one}"), b"SECRET-VALUE")]);
        let descriptors = [descriptor(MergeDisposition::Sum)];

        let shapes = uncovered_did_key_shapes(&store, &descriptors, &[]).unwrap();
        let rendered = format!("{shapes:?}");

        assert!(rendered.contains("<did>"));
        assert!(!rendered.contains(&one), "the spelling must be masked out");
        assert!(!rendered.contains("SECRET-VALUE"));
    }

    // ---- verdict: the three accounted-for states, and no fourth ----

    fn audit(
        store: &SledStore,
        d: &[KeyspaceDescriptor],
        f: &[DeferredNamespace],
    ) -> CoverageAudit {
        audit_store(store, d, f, 0).unwrap()
    }

    #[test]
    fn an_uncovered_principal_row_blocks_even_with_no_collisions() {
        // The regression this exists for: registered keyspaces are clean, but
        // the store holds a principal-bearing row under a prefix nobody
        // registered. Reporting CLEAR here would pass the migration gate on a
        // row that was never collision-scanned.
        let one = spell(&principal(91), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("test:{one}"), b"v"),
            (&format!("nobody_registered_this:{one}"), b"v"),
        ]);

        let descriptors = [descriptor(MergeDisposition::Sum)];
        let a = audit(&store, &descriptors, &[]);

        assert!(a.report.is_clear(), "registered keyspaces are clean");
        assert_eq!(a.uncovered_did_rows(), 1);
        assert!(!a.is_clear(), "an unclassified principal row must block");
    }

    #[test]
    fn a_deferred_namespace_is_classified_not_cleared_and_does_not_block() {
        // A reviewed exclusion must be distinguishable from an accidental
        // omission: it does not block, but it is reported as deferred rather
        // than folded into the scanned/clear count.
        let one = spell(&principal(93), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("test:{one}"), b"v"),
            (&format!("gov:vote:abc:{one}"), b"v"),
        ]);

        let descriptors = [descriptor(MergeDisposition::Sum)];
        let deferrals = n2a_deferred_namespaces();
        let a = audit(&store, &descriptors, &deferrals);

        assert_eq!(a.deferred_did_rows(), 1, "the vote row is deferred");
        assert_eq!(a.uncovered_did_rows(), 0, "deferred is not uncovered");
        assert!(a.is_clear(), "a reviewed exclusion must not block");

        // And it is not counted as scanned by any registered keyspace.
        let scanned: usize = a.report.keyspaces.iter().map(|k| k.rows_scanned).sum();
        assert_eq!(scanned, 1, "only the registered row was scanned");
    }

    #[test]
    fn every_principal_row_lands_in_exactly_one_of_three_states() {
        // covered | deferred | uncovered — and nothing may fall through.
        let p = spell(&principal(95), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("test:{p}"), b"v"),            // covered
            (&format!("gov:vote:x:{p}"), b"v"),      // deferred
            (&format!("security:banned:{p}"), b"v"), // deferred
            (&format!("stray/{p}"), b"v"),           // uncovered
            ("test:no-did-here", b"v"),              // not principal-bearing
        ]);

        let descriptors = [descriptor(MergeDisposition::Sum)];
        let deferrals = n2a_deferred_namespaces();
        let a = audit(&store, &descriptors, &deferrals);

        let covered: usize = a
            .report
            .keyspaces
            .iter()
            .map(|k| k.rows_with_readable_did)
            .sum();
        assert_eq!(covered, 1);
        assert_eq!(a.deferred_did_rows(), 2);
        assert_eq!(a.uncovered_did_rows(), 1);
        assert_eq!(
            covered + a.deferred_did_rows() + a.uncovered_did_rows(),
            a.overview.rows_with_embedded_did,
            "no principal-bearing row may fall outside the three states"
        );
        assert!(!a.is_clear());
    }

    #[test]
    fn full_coverage_with_no_collisions_is_clear() {
        let one = spell(&principal(97), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("test:{one}"), b"v")]);
        let a = audit(&store, &[descriptor(MergeDisposition::Sum)], &[]);
        assert!(a.is_clear());
    }

    #[test]
    fn a_collision_needing_manual_disposition_is_not_clear() {
        let (x, y) = two_spellings(99);
        let store = store_with(&[(&format!("test:{x}"), b"v"), (&format!("test:{y}"), b"v")]);
        let a = audit(&store, &[descriptor(MergeDisposition::FailClosed)], &[]);
        assert!(!a.is_clear());
    }

    #[test]
    fn unreachable_named_tree_rows_block() {
        let one = spell(&principal(101), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("test:{one}"), b"v")]);
        let descriptors = [descriptor(MergeDisposition::Sum)];

        assert!(audit_store(&store, &descriptors, &[], 0)
            .unwrap()
            .is_clear());
        assert!(
            !audit_store(&store, &descriptors, &[], 1)
                .unwrap()
                .is_clear(),
            "a principal row in an unreachable tree must block"
        );
    }

    // ---- tokenizer: every encoding the production parser accepts ----

    #[test]
    fn every_accepted_multibase_encoding_is_captured_whole_and_decodes() {
        use multibase::Base::*;

        // An identifier chosen so its Base64 spelling actually contains `+`
        // and its Base64Url spelling contains `-`; an all-equal identifier
        // exercises neither, and this test would pass vacuously with the old
        // tokenizer if it did not.
        let mut id = [0u8; 32];
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(251);
        }

        let bases = [
            Base2,
            Base8,
            Base10,
            Base16Lower,
            Base16Upper,
            Base32Lower,
            Base32Upper,
            Base32PadLower,
            Base32PadUpper,
            Base32HexLower,
            Base32HexUpper,
            Base32HexPadLower,
            Base32HexPadUpper,
            Base32Z,
            Base36Lower,
            Base36Upper,
            Base58Flickr,
            Base58Btc,
            Base64,
            Base64Pad,
            Base64Url,
            Base64UrlPad,
            Base256Emoji,
        ];

        let mut saw_plus = false;
        for base in bases {
            let spelling = spell(&id, base);

            // The corpus is only meaningful if production actually accepts it.
            assert!(
                icn_identity::Did::from_str(&spelling).is_ok(),
                "fixture assumes the production parser accepts {spelling:?}"
            );
            if spelling.contains('+') {
                saw_plus = true;
            }

            let key = format!("test:{spelling}").into_bytes();
            let found = find_embedded_dids(&key);

            assert_eq!(found.len(), 1, "one spelling in {spelling:?}");
            assert_eq!(
                found[0].spelling, spelling,
                "the whole spelling must be captured, not truncated"
            );
            assert_eq!(
                found[0].identifier,
                Some(id),
                "{spelling:?} must decode to the fixture identifier"
            );
        }

        assert!(
            saw_plus,
            "corpus must include a spelling containing '+', or it does not \
             discriminate the tokenizer fix"
        );
    }

    #[test]
    fn a_spelling_followed_by_a_slash_separator_still_terminates() {
        // `/` is both a Base64 character and a live key separator, so the
        // alphabet alone cannot decide where a spelling ends. Longest-match
        // with decode validation must stop at the spelling.
        let one = spell(&principal(103), multibase::Base::Base58Btc);
        let key = format!("ns/{one}/role").into_bytes();

        let found = find_embedded_dids(&key);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].spelling, one, "must not swallow '/role'");
        assert_eq!(found[0].identifier, Some(principal(103)));
    }

    #[test]
    fn a_malformed_did_shaped_token_stays_unreadable() {
        // The tokenizer became more permissive; it must not have become
        // "accept anything and call it a principal".
        for bad in [
            "test:did:icn:z6Mkh", // valid multibase, wrong length
            "test:did:icn:!!!!",  // not multibase
            "test:did:icn:",      // empty identifier
            "test:did:icn:++++",  // newly-allowed chars, still junk
            "test:did:icn:////",  // ditto
        ] {
            let found = find_embedded_dids(bad.as_bytes());
            assert_eq!(found.len(), 1, "{bad:?} yields one DID-shaped token");
            assert!(
                found[0].identifier.is_none(),
                "{bad:?} must not resolve to a principal"
            );
        }
    }

    // ---- a plausible merge rule is not an authorized one ----

    #[test]
    fn a_collision_under_an_unsigned_off_rule_is_not_automatable() {
        // The rule reads fine and the disposition permits a merge, but the
        // domain that owns the state has not approved it. Summing two balances
        // because addition is the obvious arithmetic would be this crate
        // deciding an economic question it has no standing to decide.
        let (x, y) = two_spellings(111);
        let store = store_with(&[(&format!("test:{x}"), b"v"), (&format!("test:{y}"), b"v")]);

        let signed_off = descriptor_with(MergeDisposition::Sum, RuleBasis::Established);
        let pending = descriptor_with(MergeDisposition::Sum, RuleBasis::AwaitingDomainSignOff);

        assert!(scan_keyspace(&store, &signed_off).unwrap().is_automatable());
        assert!(
            !scan_keyspace(&store, &pending).unwrap().is_automatable(),
            "an unapproved rule must fail closed even though Sum permits a merge"
        );
    }

    #[test]
    fn an_unsigned_off_rule_without_collisions_still_does_not_block() {
        // The basis is a statement about merging. With nothing to merge it must
        // not bite, or every clean store would block forever.
        let one = spell(&principal(113), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("test:{one}"), b"v")]);
        let pending = descriptor_with(MergeDisposition::Sum, RuleBasis::AwaitingDomainSignOff);

        assert!(scan_keyspace(&store, &pending).unwrap().is_automatable());
    }

    #[test]
    fn economic_keyspaces_do_not_claim_authority_they_have_not_been_given() {
        // Pins the registry against the migration document: every rule the doc
        // records as "asserted here, needs domain sign-off" must be marked as
        // such in code, so the two cannot drift into disagreeing about what has
        // been authorized.
        let pending: Vec<&str> = n2a_keyspaces()
            .iter()
            .filter(|d| d.basis == RuleBasis::AwaitingDomainSignOff)
            .map(|d| d.name)
            .collect();

        for expected in [
            "icn-ledger/balance",
            "icn-ledger/cleared_volume",
            "icn-ledger/frozen",
            "icn-net/outgoing_seq",
            "icn-trust/edges",
            "trust-app/sequences_issuer",
        ] {
            assert!(
                pending.contains(&expected),
                "{expected} has no domain sign-off and must not be automatable"
            );
        }
    }

    #[test]
    fn the_deferral_registry_covers_every_namespace_the_docs_defer() {
        let names: Vec<&str> = n2a_deferred_namespaces().iter().map(|d| d.name).collect();
        // Inventory rows 23 (§7.5), 5-8/38 and 29 are all documented as owned
        // by another gate; a documented deferral with no entry here would make
        // ordinary live rows block the gate instead.
        assert!(names.contains(&"governance/votes"));
        assert!(names.contains(&"security/misbehavior"));
        assert!(names.contains(&"rpc/auth-challenges"));

        // Every deferral must name the gate that owns it, so the exclusion is
        // auditable rather than merely convenient.
        for d in n2a_deferred_namespaces() {
            assert!(!d.gate.is_empty(), "{} must name its gate", d.name);
            assert!(
                !d.inventory_rows.is_empty(),
                "{} must cite inventory",
                d.name
            );
        }
    }

    #[test]
    fn a_live_auth_challenge_row_is_deferred_rather_than_blocking() {
        // Regression for the exact reported case: an ordinary TTL-lived RPC
        // challenge must not make the documented gate unusable.
        let one = spell(&principal(117), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("auth:challenge:{one}"), b"v")]);

        let a = audit_store(
            &store,
            &[descriptor(MergeDisposition::Sum)],
            &n2a_deferred_namespaces(),
            0,
        )
        .unwrap();

        assert_eq!(a.uncovered_did_rows(), 0);
        assert_eq!(a.deferred_did_rows(), 1);
        assert!(a.is_clear());
    }

    #[test]
    fn trailing_junk_after_a_valid_spelling_stays_unreadable() {
        // `replay_max_seq:<valid-did>junk` must not be reported as a readable
        // principal with `junk` as residual key material: the keyspace's own
        // parser consumes the whole suffix as the identifier and rejects it, so
        // treating the prefix as readable would report a principal for a row the
        // real loader cannot read — and lower the unreadable count that exists
        // to fail closed.
        let one = spell(&principal(121), multibase::Base::Base58Btc);
        let key = format!("test:{one}junk").into_bytes();

        let found = find_embedded_dids(&key);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].identifier.is_none(),
            "an unexplained suffix makes the token ambiguous, not readable"
        );

        // And it must reach the report as an unreadable row, which blocks.
        let store = store_with(&[(&format!("test:{one}junk"), b"v")]);
        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();
        assert_eq!(report.rows_unreadable, 1);
        assert!(report.must_fail_closed());
    }

    #[test]
    fn a_registered_keyspace_rejects_a_slash_suffix_it_does_not_use() {
        // `replay_max_seq:<did>/junk`: the keyspace's own parser consumes the
        // whole suffix as the identifier and rejects it, so the scanner must
        // not report a readable principal with `/junk` as residual bytes. No
        // registered keyspace puts `/` after a DID, so none permits this.
        let one = spell(&principal(141), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("test:{one}/junk"), b"v")]);

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();
        assert_eq!(report.rows_unreadable, 1, "the row is uninterpretable");
        assert_eq!(report.rows_with_readable_did, 0);
        assert!(report.must_fail_closed(), "and it must block");
    }

    #[test]
    fn no_registered_keyspace_claims_a_slash_terminated_did() {
        // Pins the registry: if a future keyspace does put `/` after a DID it
        // must say so deliberately, rather than inheriting a permissive default
        // that would make `<did>/junk` look readable.
        for d in n2a_keyspaces() {
            assert!(
                !d.slash_ends_did,
                "{} claims `/` ends a DID; confirm its parser really does",
                d.name
            );
        }
    }

    #[test]
    fn a_slash_remainder_is_still_attributed_to_key_structure() {
        // The one remainder that is explainable: `/` is the only separator a
        // live keyspace uses that is also a multibase body character. This must
        // keep working, or `trust/sequences/issuer/<did>`-shaped keys with a
        // trailing field would all become unreadable.
        let one = spell(&principal(123), multibase::Base::Base58Btc);
        let key = format!("ns/{one}/role").into_bytes();

        // The descriptor-free reader is permissive, because a caller with no
        // descriptor cannot know the layout — and such rows block as uncovered
        // regardless.
        let found = find_embedded_dids(&key);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].spelling, one);
        assert_eq!(found[0].identifier, Some(principal(123)));

        // Under a keyspace that does not use `/` after a DID, the same key is
        // uninterpretable rather than readable.
        let strict = find_embedded_dids_with(&key, false);
        assert!(strict[0].identifier.is_none());
    }

    #[test]
    fn a_non_utf8_key_is_reported_as_blocking_not_a_panic() {
        // A sled key is arbitrary bytes. One holding invalid UTF-8 after
        // `did:icn:` used to expand into replacement characters whose rendered
        // length exceeded the bytes consumed, so masking it sliced past the end
        // of the key and crashed the process — the one outcome a gate must not
        // have on untrusted input, because a caller sees a non-zero exit without
        // knowing whether anything was scanned.
        let key = b"stray:did:icn:\xff".to_vec();

        let embedded = find_embedded_dids(&key);
        assert_eq!(embedded.len(), 1);
        assert!(
            embedded[0].identifier.is_none(),
            "it decodes to no principal"
        );

        // Must not panic, and must render something a reviewer can act on.
        let shape = mask_key(&key, &embedded);
        assert!(shape.starts_with("stray:"), "shape was {shape:?}");

        // And it must reach a store scan as an unreadable row, which blocks.
        let store = SledStore::temporary().unwrap();
        store.put(b"test:did:icn:\xff", b"v").unwrap();
        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();
        assert_eq!(report.rows_unreadable, 1);
        assert!(report.must_fail_closed());
    }

    #[test]
    fn masking_is_offset_correct_for_multi_byte_spellings() {
        // `Base256Emoji` spellings are multi-byte, so a masker that confused
        // rendered length with consumed bytes would also mis-slice here.
        let bytes = principal(131);
        let emoji = spell(&bytes, multibase::Base::Base256Emoji);
        let key = format!("ns/{emoji}/tail").into_bytes();

        let embedded = find_embedded_dids(&key);
        assert_eq!(mask_key(&key, &embedded), "ns/<did>/tail");
    }

    #[test]
    fn rows_with_different_did_arity_do_not_group_or_panic() {
        // A key holding a literal 32-byte sequence canonicalises to the same
        // bytes as one holding an encoded DID in that position. Grouping them
        // compared rows that are not the same key, and indexing the shorter
        // row's spellings by the longer row's arity aborted the gate — a panic
        // reachable from ordinary sled keys, which are arbitrary bytes.
        let a = principal(151);
        // Raw bytes chosen to sort *after* `did:icn:`, so the two-DID row is
        // first in scan order — the ordering that triggered the abort.
        let c = [0xf0u8; 32];

        let sa = spell(&a, multibase::Base::Base58Btc);
        let sc = spell(&c, multibase::Base::Base58Btc);

        let store = SledStore::temporary().unwrap();
        let mut two = format!("test:{sa}:").into_bytes();
        two.extend_from_slice(sc.as_bytes());
        store.put(&two, b"v").unwrap();

        let mut one = format!("test:{sa}:").into_bytes();
        one.extend_from_slice(&c);
        store.put(&one, b"v").unwrap();

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        assert_eq!(report.rows_scanned, 2);
        assert_eq!(
            report.distinct_principals, 2,
            "different DID arity is a different key, not one principal"
        );
        assert!(
            report.collision_groups.is_empty(),
            "structurally different rows must not be reported as a collision"
        );
    }

    #[test]
    fn identical_shapes_with_different_did_positions_do_not_group() {
        // Same arity, same canonical bytes, different substitution boundaries:
        // `<did-A><raw-B>` and `<raw-A><did-B>` are not the same key.
        let a = [0xf1u8; 32];
        let b = [0xf2u8; 32];
        let sa = spell(&a, multibase::Base::Base58Btc);
        let sb = spell(&b, multibase::Base::Base58Btc);

        let store = SledStore::temporary().unwrap();
        let mut left = b"test:".to_vec();
        left.extend_from_slice(sa.as_bytes());
        left.extend_from_slice(&b);
        store.put(&left, b"v").unwrap();

        let mut right = b"test:".to_vec();
        right.extend_from_slice(&a);
        right.extend_from_slice(sb.as_bytes());
        store.put(&right, b"v").unwrap();

        let report = scan_keyspace(&store, &descriptor(MergeDisposition::Sum)).unwrap();

        // Whichever way each row resolves — `<did-A>` followed by raw bytes is
        // unreadable, since the raw bytes are absorbed as multi-byte body bytes
        // and leave an unexplained remainder — the two must never be reported as
        // one principal seen twice.
        assert!(
            report.collision_groups.is_empty(),
            "different substitution boundaries are different keys"
        );
        assert_eq!(
            report.rows_with_readable_did + report.rows_unreadable,
            2,
            "both rows are accounted for, none silently dropped"
        );
        assert!(report.must_fail_closed(), "an unreadable row blocks");
    }

    #[test]
    fn the_audit_passes_never_materialize_stored_values() {
        // A store whose values are large: the key-only passes must not pull
        // them in. Asserted through `scan_keys` agreeing with `scan` on keys
        // while the overview still reports correctly, so a backend override
        // cannot silently diverge from the default.
        let one = spell(&principal(161), multibase::Base::Base58Btc);
        let big = vec![b'x'; 64 * 1024];
        let store = SledStore::temporary().unwrap();
        store.put(format!("test:{one}").as_bytes(), &big).unwrap();
        store.put(b"other:plain", &big).unwrap();

        let keys = store.scan_keys(b"").unwrap();
        let pairs = store.scan(b"").unwrap();
        assert_eq!(keys.len(), pairs.len());
        assert!(
            keys.iter().zip(&pairs).all(|(k, (pk, _))| k == pk),
            "scan_keys must agree with scan on keys and order"
        );

        let overview = store_overview(&store).unwrap();
        assert_eq!(overview.total_rows, 2);
        assert_eq!(overview.rows_with_embedded_did, 1);
    }

    #[test]
    fn trailing_material_in_a_did_terminated_keyspace_is_unreadable() {
        // `replay_max_seq:<did>:junk`. The candidate run stops at `:` because it
        // is not a body byte, so the generic scan saw a clean spelling plus
        // residual key material — but that keyspace hands everything after the
        // prefix to `Did::from_str`, which rejects it.
        let one = spell(&principal(171), multibase::Base::Base58Btc);
        let strict = KeyspaceDescriptor {
            did_ends_key: true,
            ..descriptor(MergeDisposition::Sum)
        };

        let store = store_with(&[(&format!("test:{one}:junk"), b"v")]);
        let report = scan_keyspace(&store, &strict).unwrap();
        assert_eq!(report.rows_unreadable, 1);
        assert_eq!(report.rows_with_readable_did, 0);
        assert!(report.must_fail_closed());

        // The well-formed row in the same keyspace stays readable.
        let clean = store_with(&[(&format!("test:{one}"), b"v")]);
        let ok = scan_keyspace(&clean, &strict).unwrap();
        assert_eq!(ok.rows_with_readable_did, 1);
        assert_eq!(ok.rows_unreadable, 0);
    }

    #[test]
    fn structured_keyspaces_still_accept_their_trailing_fields() {
        // The flag must be per keyspace: `cleared_volume:<did>:<currency>` has
        // legitimate material after the DID and must not become unreadable.
        let one = spell(&principal(173), multibase::Base::Base58Btc);
        let structured = descriptor(MergeDisposition::Sum); // did_ends_key: false
        let store = store_with(&[(&format!("test:{one}:USD"), b"v")]);

        let report = scan_keyspace(&store, &structured).unwrap();
        assert_eq!(report.rows_with_readable_did, 1);
        assert_eq!(report.rows_unreadable, 0);
    }

    #[test]
    fn the_registry_marks_did_terminated_keyspaces_accurately() {
        // Checked against the live key builders, not assumed: these end with
        // the DID, and the structured ones do not.
        let ks = n2a_keyspaces();
        let ends = |n: &str| ks.iter().find(|d| d.name == n).map(|d| d.did_ends_key);

        for n in [
            "icn-net/replay_max_seq",
            "icn-net/replay_sender_regime",
            "icn-ledger/frozen",
            "trust-app/sequences_issuer",
            "trust-app/sequences_receiver",
            "icn-coop/member",
        ] {
            assert_eq!(ends(n), Some(true), "{n} keys end with the DID");
        }
        for n in [
            "icn-net/replay_finalized",  // `<did>:<sequence>`
            "icn-net/outgoing_seq",      // `<sender>||<recipient>`
            "icn-ledger/cleared_volume", // `<did>:<currency>`
            "icn-ledger/balance",        // JSON-quoted, so a `"` follows
            "icn-trust/edges",           // `<source>:<target>`
        ] {
            assert_eq!(ends(n), Some(false), "{n} has material after the DID");
        }
    }

    #[test]
    fn a_pathologically_long_did_token_is_rejected_promptly() {
        // Backtracking retries the decode once per byte removed, so an
        // unbounded candidate made the audit quadratic in a length chosen by
        // whoever wrote the row. Nothing longer than a `Base2` spelling can
        // decode to 32 bytes, so the run is capped there.
        let mut key = b"test:did:icn:z".to_vec();
        key.extend(std::iter::repeat_n(b'A', 50_000));

        let found = find_embedded_dids(&key);
        assert_eq!(found.len(), 1);
        assert!(found[0].identifier.is_none(), "it decodes to nothing");
        assert!(
            found[0].spelling.len() < 400,
            "the candidate must be bounded, was {}",
            found[0].spelling.len()
        );
    }

    #[test]
    fn the_registry_covers_only_keyspaces_this_tranche_may_touch() {
        let names: Vec<&str> = n2a_keyspaces().iter().map(|d| d.name).collect();

        // Security-sensitive namespaces are deliberately absent: their
        // inspection belongs to the dedicated security workflow.
        assert!(!names.iter().any(|n| n.contains("misbehavior")));
        assert!(!names.iter().any(|n| n.contains("challenge")));
        // The governance vote keyspace is behind the separate §7.5 gate.
        assert!(!names.iter().any(|n| n.contains("governance")));
        assert!(!names.iter().any(|n| n.contains("vote")));
    }
}
