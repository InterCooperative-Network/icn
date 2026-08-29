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

/// Rows sharing one principal-canonical shape, with the identifiers that shape
/// decodes to. Keyed by the shape itself.
type ShapeGroups = BTreeMap<Vec<u8>, Vec<(RowRef, Vec<[u8; 32]>)>>;

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
    /// Why that rule, in one line — so a report explains itself.
    pub rationale: &'static str,
}

/// A DID spelling found inside a stored key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedDid {
    /// Byte offset of the spelling within the raw key.
    pub offset: usize,
    /// The spelling exactly as stored.
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
    /// when its disposition authorizes a merge, and only when every row it holds
    /// was readable: a key that does not decode cannot be classified, so it
    /// cannot be migrated on its own recognizance.
    pub fn is_automatable(&self) -> bool {
        if self.rows_unreadable > 0 {
            return false;
        }
        self.collision_groups.is_empty() || self.disposition.is_automatable()
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
    let needle = DID_PREFIX.as_bytes();
    let mut found = Vec::new();
    let mut i = 0usize;

    while i + needle.len() <= key.len() {
        if &key[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }

        let start = i;
        let mut end = i + needle.len();
        while end < key.len() && is_multibase_body_byte(key[end]) {
            end += 1;
        }

        // A prefix with nothing after it names no principal, but it is still a
        // DID-shaped token: report it as unreadable rather than skipping it,
        // so an empty identifier cannot hide from the scan.
        let spelling = String::from_utf8_lossy(&key[start..end]).into_owned();
        let identifier = identifier_bytes_of_spelling(&spelling).ok();
        found.push(EmbeddedDid {
            offset: start,
            spelling,
            identifier,
        });

        i = end.max(start + 1);
    }

    found
}

/// Whether a byte may continue a multibase identifier body.
///
/// Deliberately permissive across multibase alphabets — base16 through
/// base256emoji — because the scan must not truncate a spelling it does not
/// recognise and then decode a fragment. Anything permissive enough to over-read
/// a separator would break grouping, so the set stops short of every separator
/// these keyspaces use.
fn is_multibase_body_byte(b: u8) -> bool {
    // Non-ASCII bytes continue the token: `Base256Emoji` spellings are
    // multi-byte UTF-8, and truncating one at its first continuation byte would
    // both fail to decode and split a real spelling.
    if b >= 0x80 {
        return true;
    }
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'='
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
        let embedded = find_embedded_dids(&key);

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

        rows_with_readable_did += 1;

        let mut shape = Vec::with_capacity(key.len());
        let mut cursor = 0usize;
        let mut spellings = Vec::with_capacity(embedded.len());

        for (did, bytes) in embedded.iter().zip(&identifiers) {
            shape.extend_from_slice(&key[cursor..did.offset]);
            shape.extend_from_slice(bytes);
            cursor = did.offset + did.spelling.len();
            spellings.push(did.spelling.clone());
        }
        shape.extend_from_slice(&key[cursor..]);

        groups.entry(shape).or_default().push((
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
    let pairs = store.scan(b"")?;
    let mut namespaces: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows_with_embedded_did = 0usize;

    for (key, _) in &pairs {
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
        total_rows: pairs.len(),
        namespaces,
        rows_with_embedded_did,
    })
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
) -> anyhow::Result<BTreeMap<String, usize>> {
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();

    for (key, _) in store.scan(b"")? {
        let embedded = find_embedded_dids(&key);
        if embedded.is_empty() {
            continue;
        }
        if descriptors.iter().any(|d| key.starts_with(d.prefix)) {
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
        cursor = did.offset + did.spelling.len();
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
    let pairs = store.scan(descriptor.prefix)?;
    // Values are reduced to their length at the boundary: nothing downstream of
    // this line can read a stored payload even by mistake.
    let rows = pairs
        .into_iter()
        .map(|(key, value)| (key, value.len()))
        .collect();
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
            rationale: "Replay floor. A lower survivor weakens the guard, so the merge keeps the \
                        maximum, which can only reject more than any single row did.",
        },
        KeyspaceDescriptor {
            name: "icn-net/replay_finalized",
            prefix: b"replay_finalized:",
            inventory_rows: &[2, 37],
            disposition: MergeDisposition::Union,
            rationale: "Finalized-sequence set. Dropping a spelling's rows would re-open replay \
                        for the sequences it recorded, so the merge is a union.",
        },
        KeyspaceDescriptor {
            name: "icn-net/replay_sender_regime",
            prefix: b"replay_sender_regime:",
            inventory_rows: &[3, 37],
            disposition: MergeDisposition::FailClosed,
            rationale: "Two rows can assert different regimes for one sender, which is a \
                        contradiction no domain rule resolves. The live loader already declines \
                        to collapse these (#2644); a migration must not decide it either.",
        },
        KeyspaceDescriptor {
            name: "icn-net/outgoing_seq",
            prefix: b"outgoing_seq:",
            inventory_rows: &[4],
            disposition: MergeDisposition::MaxMonotonic,
            rationale: "Outgoing sequence high-water for a (sender, recipient) pair. A lower \
                        survivor is a nonce regression, so the merge keeps the maximum.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/balance",
            prefix: b"ledger:balance:",
            inventory_rows: &[9, 39, 40],
            disposition: MergeDisposition::Sum,
            rationale: "Accumulated balances. Overwriting drops a spelling's recorded position \
                        entirely, so the merge sums rather than elects a survivor.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/cleared_volume",
            prefix: b"ledger:cleared_volume:",
            inventory_rows: &[69],
            disposition: MergeDisposition::Sum,
            rationale: "Accumulated cleared volume per (account, currency). Currency stays in the \
                        canonical shape, so only same-currency rows merge, and they sum.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/frozen",
            prefix: b"ledger:frozen:",
            inventory_rows: &[42, 68],
            disposition: MergeDisposition::Union,
            rationale: "Freeze records. Unfreeze deletes one spelling only, so electing a \
                        survivor can fail open; the merge is a union of the freezes.",
        },
        KeyspaceDescriptor {
            name: "icn-trust/edges",
            prefix: b"trust/edges/",
            inventory_rows: &[30],
            disposition: MergeDisposition::Union,
            rationale: "Trust edges keyed by (source, target). A dropped spelling takes its edges \
                        with it, so the merge unions the edge sets.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/journal",
            prefix: b"ledger:journal:",
            inventory_rows: &[39, 40],
            disposition: MergeDisposition::Equivalent,
            rationale: "Journal entries are content-addressed by entry hash; DIDs appear inside \
                        the value, not the key. Scanned to confirm the key carries no spelling.",
        },
        KeyspaceDescriptor {
            name: "trust-app/sequences_receiver",
            prefix: b"trust/sequences/receiver/",
            inventory_rows: &[71],
            disposition: MergeDisposition::MaxMonotonic,
            rationale: "Last-seen attestation sequence per issuer — a replay floor. A lower \
                        survivor accepts stale attestations, so the merge keeps the maximum, \
                        matching the established replay_max_seq precedent.",
        },
        KeyspaceDescriptor {
            name: "trust-app/sequences_issuer",
            prefix: b"trust/sequences/issuer/",
            inventory_rows: &[71],
            disposition: MergeDisposition::MaxMonotonic,
            rationale: "This node's own outgoing attestation sequence. A lower survivor re-issues \
                        a sequence number already used, which the uniqueness invariant forbids, \
                        so the merge keeps the maximum.",
        },
        KeyspaceDescriptor {
            name: "icn-coop/member",
            prefix: b"member:",
            inventory_rows: &[36],
            disposition: MergeDisposition::FailClosed,
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
        KeyspaceDescriptor {
            name: "test/keyspace",
            prefix: b"test:",
            inventory_rows: &[0],
            disposition,
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

        let shapes = uncovered_did_key_shapes(&store, &descriptors).unwrap();
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

        let shapes = uncovered_did_key_shapes(&store, &descriptors).unwrap();
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

        let shapes = uncovered_did_key_shapes(&store, &descriptors).unwrap();
        let rendered = format!("{shapes:?}");

        assert!(rendered.contains("<did>"));
        assert!(!rendered.contains(&one), "the spelling must be masked out");
        assert!(!rendered.contains("SECRET-VALUE"));
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
