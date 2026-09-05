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

use anyhow::Context as _;
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

impl RuleBasis {
    /// A short stable label for reports.
    pub fn label(self) -> &'static str {
        match self {
            RuleBasis::Established => "established",
            RuleBasis::AwaitingDomainSignOff => "awaiting-domain-sign-off",
        }
    }
}

/// One durable keyspace to scan.
///
/// A descriptor names the prefix to read and the disposition that applies to a
/// collision found under it. By default it describes no more of the key than
/// that: DID spellings are located by scanning for the `did:icn:` scheme, which
/// is layout-independent and so cannot drift out of step with a keyspace that
/// changes its separator.
///
/// That default holds wherever every key component is protocol-generated. It
/// fails where a key carries an identifier another domain chose, because the
/// scheme scan cannot tell a principal-bearing component from an opaque one
/// that merely contains the same text. [`KeyspaceDescriptor::principal_region`]
/// is the narrow exception: a layout may state where its principal lives, and
/// nothing more.
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
    ///
    /// Read only under [`PrincipalRegion::WholeKey`], for the same reason as
    /// [`KeyspaceDescriptor::slash_ends_did`].
    pub did_ends_key: bool,
    /// Whether this keyspace's own parser treats `/` as ending a DID.
    ///
    /// `/` is the only separator that is also a multibase body character, so
    /// where a spelling may be followed by one is a property of the individual
    /// key layout — not something the scanner can infer.
    ///
    /// Read only under [`PrincipalRegion::WholeKey`], and no whole-key keyspace
    /// puts `/` immediately after a DID (`trust/edges/<a>:<b>` uses `:`;
    /// `trust/sequences/issuer/<did>` ends there), so every descriptor sets this
    /// `false` and `<did>/junk` is correctly unreadable rather than a readable
    /// principal with residual bytes the real loader would reject.
    ///
    /// `federation/attestations/<did>/<source>` and
    /// `idx_agreement_party/<did>/<agreement id>` do put `/` after a spelling,
    /// and are not exceptions to that: each declares an anchored region, which
    /// takes its terminator from the region itself. Saying it here as well
    /// would be two owners for one fact, so
    /// `a_descriptor_with_an_anchored_region_leaves_the_whole_key_flags_off`
    /// pins that anchored descriptors leave this `false`.
    pub slash_ends_did: bool,
    /// Which part of a key of this layout may carry a principal spelling.
    pub principal_region: PrincipalRegion,
    /// Why that rule, in one line — so a report explains itself.
    pub rationale: &'static str,
}

/// Where in a stored key a keyspace's principal spellings can appear.
///
/// The scan locates spellings by their `did:icn:` scheme, which is
/// layout-independent — and therefore cannot tell a principal-bearing key
/// component from a domain identifier that merely *contains* that text. For
/// most keyspaces every component is protocol-generated and the distinction
/// does not arise. Where a key ends in an opaque identifier chosen by another
/// domain it does, and getting it wrong is not cosmetic: a scan that
/// canonicalized a DID inside such an identifier would group two rows the
/// owning store holds apart, and would call a row unreadable that the store
/// reads without difficulty. The scan and the store would then disagree about
/// what a collision *is* (icn#2704).
///
/// This describes key structure, not federation behaviour: any layout of the
/// same shape can declare it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalRegion {
    /// Every `did:icn:` occurrence anywhere in the key names a principal.
    ///
    /// Correct wherever the whole key is built from protocol-generated
    /// components, which is every registered keyspace but the two federation
    /// layouts that end in an identifier another domain chose.
    WholeKey,
    /// Exactly one spelling, starting immediately after
    /// [`KeyspaceDescriptor::prefix`] and ending at the first `terminator`
    /// that leaves a decodable spelling behind. Everything from that
    /// terminator onward is an opaque discriminator: it is carried into the
    /// canonical shape byte-for-byte and never parsed, so two discriminators
    /// are the same one only when their bytes are equal.
    ///
    /// The spelling may itself contain the terminator — `Base64` bodies
    /// contain `/` — so the boundary is decided by decoding, not by finding
    /// the first occurrence.
    ///
    /// A row whose anchor does not hold a decodable spelling is **unreadable**
    /// rather than principal-free: the layout says one belongs there, so its
    /// absence is a row no migration can classify, exactly as the owning
    /// store cannot read it.
    AnchoredThenOpaque {
        /// The byte that must follow the spelling.
        terminator: u8,
    },
    /// Exactly one **length-framed, tag-discriminated** region, starting
    /// immediately after [`KeyspaceDescriptor::prefix`]: a big-endian `u32`
    /// length field, then that many bytes of region, then an opaque tail. The
    /// region's first byte is a variant tag; only `principal_tag` introduces a
    /// principal spelling, and the spelling is the rest of the region exactly —
    /// its extent is stated by the length field, not inferred from the
    /// alphabet.
    ///
    /// Two facts make this layout need its own rule rather than reuse of the
    /// two above. Searching for a terminator byte would run through the
    /// binary length field and through an opaque tail whose bytes are
    /// arbitrary, so no terminator names the boundary. And treating the whole
    /// key as principal-bearing would carry the length field into the
    /// canonical shape verbatim — a field *derived from the spelling*, so two
    /// spellings of one principal would differ in it, land in different
    /// shapes, and form no collision group at all. That is a silent
    /// false-clear, which is worse than a refusal.
    ///
    /// The region including its own framing is therefore what the canonical
    /// shape replaces: the length field and tag are derivable from the
    /// spelling and are not discriminators, while the tail after the region
    /// is carried byte-for-byte and never parsed.
    ///
    /// A region under any other tag names **no principal** — it holds a value
    /// some other domain chose, and one that happens to spell `did:icn:` is
    /// still that domain's value. A row whose framing does not parse, or
    /// whose `principal_tag` region holds no decodable spelling, is
    /// **unreadable** rather than principal-free: the layout says a principal
    /// belongs there, so its absence is a row no migration can classify.
    LengthPrefixedTagged {
        /// The tag under which the region holds a principal spelling. Every
        /// other tag names a value some other domain chose, which this
        /// registry does not decode — the discrimination that keeps an
        /// entity id spelling `did:icn:` from being read as a principal.
        principal_tag: u8,
    },
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
        let limit = munch(key, i + needle.len());

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
        let (end, identifier) =
            resolve_spelling(key, start, limit, Remainder::WholeKeyRun { slash_ends_did });

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

/// The longest run of multibase body bytes starting at `from`, bounded.
///
/// Bounded because backtracking retries the decode once per byte removed. A
/// 32-byte identifier is longest in `Base2` — one character per bit, 256 plus
/// a sigil — so nothing beyond this can decode to one, and without the bound a
/// single key carrying tens of thousands of base58 characters would make the
/// audit quadratic in a length the writer of that row chose.
fn munch(key: &[u8], from: usize) -> usize {
    const MAX_IDENTIFIER_CHARS: usize = 300;
    let ceiling = key.len().min(from + MAX_IDENTIFIER_CHARS);
    let mut limit = from;
    while limit < ceiling && is_multibase_body_byte(key[limit]) {
        limit += 1;
    }
    limit
}

/// The one spelling a [`PrincipalRegion::AnchoredThenOpaque`] layout carries.
///
/// The spelling starts at `start` — immediately after the keyspace prefix —
/// and ends at the first `terminator` that leaves a decodable spelling behind.
/// Everything after that is the discriminator, and is never searched: a
/// `did:icn:` occurrence inside it belongs to the domain that chose the
/// identifier, not to this keyspace's key structure.
///
/// Always returns one token. A row of this layout that carries no readable
/// spelling at its anchor is unreadable, not principal-free — the layout says
/// one belongs there.
fn anchored_spelling(key: &[u8], start: usize, terminator: u8) -> EmbeddedDid {
    // Where an unreadable token ends: the first terminator after the anchor,
    // or the end of the key. Reported so the row is accounted for; an
    // unreadable row never reaches the shape that would use these bounds.
    let opaque_at = || {
        key[start.min(key.len())..]
            .iter()
            .position(|b| *b == terminator)
            .map_or(key.len(), |off| start + off)
    };
    let unreadable = |end: usize| EmbeddedDid {
        offset: start,
        end,
        spelling: String::from_utf8_lossy(&key[start.min(key.len())..end]).into_owned(),
        identifier: None,
    };

    if start >= key.len() || !key[start..].starts_with(DID_PREFIX.as_bytes()) {
        return unreadable(opaque_at());
    }

    let body = start + DID_PREFIX.len();
    let (end, identifier) = resolve_spelling(
        key,
        start,
        munch(key, body),
        Remainder::Terminated(terminator),
    );
    match identifier {
        Some(bytes) => EmbeddedDid {
            offset: start,
            end,
            spelling: String::from_utf8_lossy(&key[start..end]).into_owned(),
            identifier: Some(bytes),
        },
        None => unreadable(opaque_at()),
    }
}

/// The principal spellings `descriptor`'s own key structure puts in `key`.
fn principal_spellings(descriptor: &KeyspaceDescriptor, key: &[u8]) -> Vec<EmbeddedDid> {
    match descriptor.principal_region {
        PrincipalRegion::WholeKey => find_embedded_dids_with(key, descriptor.slash_ends_did),
        PrincipalRegion::AnchoredThenOpaque { terminator } => {
            // The scan reads this keyspace by prefix, so the anchor is always
            // in place. A key that somehow is not under the prefix cannot be
            // parsed by this layout's rule, and saying so is the fail-closed
            // answer.
            let start = if key.starts_with(descriptor.prefix) {
                descriptor.prefix.len()
            } else {
                key.len()
            };
            vec![anchored_spelling(key, start, terminator)]
        }
        PrincipalRegion::LengthPrefixedTagged { principal_tag } => length_prefixed_tagged_spelling(
            key,
            descriptor.prefix.len(),
            principal_tag,
            key.starts_with(descriptor.prefix),
        ),
    }
}

/// The principal a [`PrincipalRegion::LengthPrefixedTagged`] key carries, if
/// its tag says it carries one.
///
/// Returns an empty vector when the region is well-formed under another tag —
/// that row genuinely names no principal — and a single unreadable
/// [`EmbeddedDid`] when the framing does not parse or the principal-tagged
/// region holds no decodable spelling.
fn length_prefixed_tagged_spelling(
    key: &[u8],
    start: usize,
    principal_tag: u8,
    under_prefix: bool,
) -> Vec<EmbeddedDid> {
    /// Width of the big-endian length field. Fixed rather than a descriptor
    /// field: one layout declares this region, it frames with a `u32`, and a
    /// configurable width would buy a hand-rolled accumulator — and its
    /// silent truncation above eight bytes — for no present caller.
    const LEN_WIDTH: usize = 4;

    // The region spans its own framing: the length field is derived from the
    // spelling, so it must be replaced along with it or two spellings of one
    // principal would never share a canonical shape.
    let unreadable = |end: usize| {
        vec![EmbeddedDid {
            offset: start.min(key.len()),
            end,
            spelling: String::from_utf8_lossy(&key[start.min(key.len())..end]).into_owned(),
            identifier: None,
        }]
    };

    if !under_prefix {
        return unreadable(key.len());
    }
    let Some(rest) = key.get(start..) else {
        return unreadable(key.len());
    };
    let Some(len_bytes) = rest
        .get(..LEN_WIDTH)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
    else {
        return unreadable(key.len());
    };
    // A `u32` length can never overflow `usize` on any target this builds for,
    // so the region end is computed without a widening step to get wrong.
    let region_len = u32::from_be_bytes(len_bytes) as usize;
    let region_start = start + LEN_WIDTH;
    let Some(region_end) = region_start.checked_add(region_len) else {
        return unreadable(key.len());
    };
    let Some(region) = key.get(region_start..region_end) else {
        return unreadable(key.len());
    };
    let Some((tag, body)) = region.split_first() else {
        return unreadable(region_end);
    };
    if *tag != principal_tag {
        // Another variant's value. The layout does not claim a principal here,
        // so the row is principal-free rather than unreadable.
        return Vec::new();
    }

    // The length field says exactly where the spelling ends, so there is no
    // maximal-munch backtracking to do: the bytes either decode whole or the
    // row is unreadable.
    let identifier = std::str::from_utf8(body)
        .ok()
        .and_then(|s| identifier_bytes_of_spelling(s).ok());
    match identifier {
        Some(bytes) => vec![EmbeddedDid {
            offset: start,
            end: region_end,
            spelling: String::from_utf8_lossy(body).into_owned(),
            identifier: Some(bytes),
        }],
        None => unreadable(region_end),
    }
}

/// What a layout permits immediately after a spelling.
///
/// The candidate run is always the longest sequence of multibase body bytes,
/// because the alphabet alone cannot say where a spelling ends. What differs
/// between layouts is which shorter-than-maximal match is legitimate, and that
/// is a statement about the key structure rather than about the alphabet.
#[derive(Debug, Clone, Copy)]
enum Remainder {
    /// The spelling runs to the end of the candidate run, or — where this
    /// layout's own parser says so — up to a `/`.
    WholeKeyRun { slash_ends_did: bool },
    /// The spelling must be followed by exactly this byte. Nothing else is a
    /// spelling of this layout, including running to the end of the key.
    Terminated(u8),
}

/// Find the longest prefix of `key[start..limit]` that decodes to a 32-byte
/// identifier and leaves a remainder `remainder` permits, returning its end
/// offset and the bytes.
///
/// When nothing decodes, the full run is returned with `None` so the caller
/// reports one unreadable token rather than silently dropping the row.
fn resolve_spelling(
    key: &[u8],
    start: usize,
    limit: usize,
    remainder: Remainder,
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
                let permitted = match remainder {
                    Remainder::WholeKeyRun { slash_ends_did } => {
                        end == limit || (slash_ends_did && key.get(end) == Some(&b'/'))
                    }
                    // An anchored spelling is followed by its discriminator, so
                    // running to the end of the key is not a spelling of this
                    // layout either — the owning store could not have written
                    // it, and a migration cannot classify what it finds.
                    Remainder::Terminated(byte) => key.get(end) == Some(&byte),
                };
                if !permitted {
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
        let embedded = principal_spellings(descriptor, &key);

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
        // last spelling is material its own parser would refuse. Read only for
        // a whole-key scan: an anchored region already decided where its one
        // spelling ends and what may follow it, and consulting a second rule
        // there would make the descriptor answer the same question twice.
        if matches!(descriptor.principal_region, PrincipalRegion::WholeKey)
            && descriptor.did_ends_key
        {
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

/// What a collision inside a deferred namespace does to a key-equality
/// binary that is about to start.
///
/// Deferral says who owns the *merge rule*. It says nothing about whether the
/// binary's own load path is lossy, and that is a fact about the loader, not a
/// judgement about the domain: a loader that folds alias rows into one
/// principal-keyed map and writes the survivor back has already merged them,
/// whoever was supposed to decide. So each deferral records, separately from
/// its gate, whether an observed collision may be started over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredCollisionPosture {
    /// The runtime tolerates alias rows without loss: it reads them on demand,
    /// refuses conflicting acts at the point of use, and writes no merged
    /// survivor back. A collision is reported so the owning gate sees it, and
    /// does not block startup.
    ReportOnly,
    /// The load path collapses alias rows into one in-memory entry and a later
    /// write-back orphans the losers. No rule authorizes that merge, so a
    /// collision must stop the binary before its loader runs.
    BlockStartup,
}

impl DeferredCollisionPosture {
    /// A short stable label for reports.
    pub fn label(self) -> &'static str {
        match self {
            DeferredCollisionPosture::ReportOnly => "report-only",
            DeferredCollisionPosture::BlockStartup => "block-startup",
        }
    }
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
/// A deferred namespace is **never dispositioned** here: no merge rule is
/// asserted for it. Its rows are still grouped by principal, because whether
/// two stored rows name one principal is a fact about the data that the owning
/// gate needs to see, and looking away from it would let the collision reach
/// the loader unexamined. The [`DeferredCollisionPosture`] says what the
/// binary does with that fact.
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
    /// What an observed collision does to a starting key-equality binary.
    pub posture: DeferredCollisionPosture,
    /// Why that posture, in one line, citing the loader behaviour it rests on.
    pub posture_rationale: &'static str,
}

/// The namespaces N2-A deliberately does not disposition, each behind a named
/// gate.
///
/// The entries are decisions recorded elsewhere, not judgements made here:
/// governance votes are behind the §7.5 membership/vote migration gate, and the
/// security and auth-challenge namespaces belong to the dedicated security
/// workflow. The posture on each is a statement about that namespace's *load
/// path* in this checkout, and cites it.
pub fn n2a_deferred_namespaces() -> Vec<DeferredNamespace> {
    vec![
        DeferredNamespace {
            name: "governance/votes",
            prefix: b"gov:vote:",
            gate: "IDENTITY_SEMANTICS §7.5 membership/vote migration gate",
            inventory_rows: &[23],
            posture: DeferredCollisionPosture::ReportOnly,
            posture_rationale: "Votes are read per proposal on demand and tallied through \
                                VoteTally::try_from_votes, which fails closed on conflicting \
                                rows for one principal (#2641/#2677); nothing at startup \
                                rebuilds or writes vote rows back, so alias rows survive intact \
                                for the §7.5 gate to disposition.",
        },
        DeferredNamespace {
            name: "rpc/auth-challenges",
            prefix: b"auth:challenge:",
            gate: "dedicated security workflow (TTL-bounded; contents not inspected)",
            inventory_rows: &[29],
            posture: DeferredCollisionPosture::ReportOnly,
            posture_rationale: "A challenge row is a TTL-bounded nonce, not durable state. \
                                Collapsing two spellings at load drops an in-flight nonce, which \
                                the client re-requests; nothing is written back. Blocking here \
                                would also trap a daemon that alone expires these rows.",
        },
        DeferredNamespace {
            name: "security/misbehavior",
            prefix: b"security:",
            gate: "dedicated security workflow (contents not inspected)",
            inventory_rows: &[5, 6, 7, 8, 38],
            posture: DeferredCollisionPosture::BlockStartup,
            posture_rationale: "MisbehaviorDetector::load_from_store inserts every row into \
                                principal-keyed maps, so under key-equality Did the later \
                                spelling's reputation, ban, quarantine and violation rows \
                                overwrite the earlier one's, and save_to_store at shutdown \
                                writes the survivor back and orphans the losers. No domain rule \
                                authorizes that merge (#2676).",
        },
    ]
}

/// One deferred namespace's rows, grouped by principal, with its posture.
///
/// The embedded [`KeyspaceReport`] carries [`MergeDisposition::FailClosed`] and
/// [`RuleBasis::AwaitingDomainSignOff`] by construction: that is the honest
/// statement of a namespace nobody has dispositioned, and it keeps the report's
/// own `must_fail_closed` from ever reading as authority this scan does not
/// have. Whether the namespace *blocks* is decided by [`DeferredReport::blocks`]
/// from the posture, not from the report's disposition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredReport {
    pub name: String,
    pub gate: String,
    pub posture: DeferredCollisionPosture,
    pub posture_rationale: String,
    pub report: KeyspaceReport,
}

impl DeferredReport {
    /// Whether this namespace holds something a starting binary may not open.
    ///
    /// Only a `BlockStartup` posture can block, and it blocks on exactly what a
    /// registered keyspace would: a principal named by more than one row, or a
    /// row whose principal cannot be read at all.
    pub fn blocks(&self) -> bool {
        self.posture == DeferredCollisionPosture::BlockStartup
            && (!self.report.collision_groups.is_empty() || self.report.rows_unreadable > 0)
    }

    /// Principal-bearing rows under this namespace, readable or not.
    pub fn did_bearing_rows(&self) -> usize {
        self.report.rows_with_readable_did + self.report.rows_unreadable
    }
}

/// Group every deferred namespace's rows by principal. Read-only.
///
/// Uses the same engine as a registered keyspace, so a group here is exactly a
/// group the key-equality `Did` would form — the point being that "deferred"
/// must never come to mean "unexamined".
pub fn scan_deferred(
    store: &dyn Store,
    deferrals: &[DeferredNamespace],
) -> anyhow::Result<Vec<DeferredReport>> {
    let mut out = Vec::with_capacity(deferrals.len());
    for d in deferrals {
        let descriptor = KeyspaceDescriptor {
            name: d.name,
            prefix: d.prefix,
            inventory_rows: d.inventory_rows,
            // No rule is asserted for a deferred namespace, and none may be
            // inferred from this report.
            disposition: MergeDisposition::FailClosed,
            basis: RuleBasis::AwaitingDomainSignOff,
            // The scan does not own these grammars, so it is permissive about
            // what follows a spelling; a spelling that does not decode is still
            // unreadable, whatever follows it.
            slash_ends_did: false,
            did_ends_key: false,
            principal_region: PrincipalRegion::WholeKey,
            rationale: d.gate,
        };
        out.push(DeferredReport {
            name: d.name.to_string(),
            gate: d.gate.to_string(),
            posture: d.posture,
            posture_rationale: d.posture_rationale.to_string(),
            report: scan_keyspace(store, &descriptor)?,
        });
    }
    Ok(out)
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
    /// Every deferred namespace's rows grouped by principal, with the posture
    /// that says what a starting binary does about a collision there.
    pub deferred_reports: Vec<DeferredReport>,
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

    /// Rows a named gate defers. Deferred is neither dispositioned nor cleared.
    pub fn deferred_did_rows(&self) -> usize {
        self.deferred.iter().map(|(_, n)| *n).sum()
    }

    /// Deferred namespaces whose posture forbids starting over what they hold.
    pub fn blocking_deferred(&self) -> Vec<&DeferredReport> {
        self.deferred_reports
            .iter()
            .filter(|d| d.blocks())
            .collect()
    }

    /// The store is clear only when every principal-bearing row it holds was
    /// accounted for, every keyspace that accounted for one can be migrated
    /// without a human deciding an outcome, and no deferred namespace holds a
    /// collision its own loader would silently merge.
    ///
    /// A principal-bearing row is accounted for in exactly one of three ways,
    /// and there is deliberately no fourth:
    ///
    /// 1. a registered keyspace interpreted it, so the collision result speaks
    ///    for it;
    /// 2. a named gate defers it — [`n2a_deferred_namespaces`] says which, and
    ///    that exclusion was reviewed — but its rows are still grouped, and a
    ///    collision under a `BlockStartup` posture blocks exactly as an unruled
    ///    collision in a registered keyspace does;
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
        self.report.is_clear()
            && self.unreachable_did_rows == 0
            && self.uncovered_did_rows() == 0
            && self.blocking_deferred().is_empty()
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
        deferred_reports: scan_deferred(store, deferrals)?,
        uncovered: uncovered_did_key_shapes(store, descriptors, deferrals)?,
        unreachable_did_rows,
    })
}

/// One sled database's full audit: the coverage audit plus the per-tree facts
/// that established whether every principal-bearing row was reachable.
///
/// This is the unit both the offline `did-collision-scan` tool and the
/// in-process startup gate ([`crate::n2a_startup_gate`]) render. They share it
/// so that the verdict an operator reads from a scan and the verdict a binary
/// enforces at startup are one computation, not two that can drift.
#[derive(Debug, Clone)]
pub struct SledStoreAudit {
    pub audit: CoverageAudit,
    /// Row count per sled tree, including the default tree.
    pub trees: Vec<(String, usize)>,
    /// Rows per tree whose key embeds a `did:icn:` spelling.
    pub did_rows: Vec<(String, usize)>,
}

impl SledStoreAudit {
    /// The gate. Not recomputed by any renderer.
    pub fn is_clear(&self) -> bool {
        self.audit.is_clear()
    }
}

/// Audit one opened sled database against the canonical N2-A registries.
/// Read-only.
///
/// [`Store::scan`] reads only sled's default tree, so every tree is counted as
/// well: a principal-bearing row in a named tree is one the scan could not
/// examine, and it is reported as *unreachable* — which blocks — rather than
/// as absent.
pub fn audit_sled_store(store: &crate::SledStore) -> anyhow::Result<SledStoreAudit> {
    let trees = store.tree_row_counts()?;
    let did_rows = store.did_bearing_rows_per_tree()?;
    let unreachable: usize = did_rows
        .iter()
        .filter(|(name, _)| name != "__sled__default")
        .map(|(_, n)| *n)
        .sum();

    let audit = audit_store(
        store as &dyn Store,
        &n2a_keyspaces(),
        &n2a_deferred_namespaces(),
        unreachable,
    )?;

    Ok(SledStoreAudit {
        audit,
        trees,
        did_rows,
    })
}

/// Collect every sled database root beneath `dir`, in path order.
///
/// A directory is a root when it holds sled's `conf` file, which sled writes
/// on creation for every database, empty or not. A root is recorded **and**
/// walked through, because a database can hold databases: `icnctl init-coop`
/// opens `<data_dir>/store` itself as a database, while `icnd` keeps
/// `store/ledger`, `store/trust`, `store/cooperative`, … beneath it. Stopping
/// at the first `conf` would leave every nested domain database unaudited and
/// let a CLEAR receipt be written over a blocker the daemon opens moments
/// later. Sled's own subdirectory (`blobs/`) carries no `conf`, so nothing
/// inside a database is mistaken for one, and the walk is bounded in depth so
/// a stray cycle cannot make it endless.
///
/// **The sweep is all-or-nothing.** An unreadable directory, an unreadable
/// entry, a symlink, or the depth bound each return an error rather than a
/// shorter list. A caller cannot tell a partial list from a complete one, and
/// the startup gate treats the list as the full set of databases: a directory
/// that is searchable but not readable would otherwise let an omitted database
/// pass as absent and earn a CLEAR receipt, which is precisely the lossy merge
/// the gate exists to prevent.
///
/// This is how a caller finds the databases a deployment actually keeps, rather
/// than the ones it expected: a deployment holds one database per domain under
/// its store directory plus several at the data-directory level, and any store
/// added after this list was written is found the same way. Enumerating by
/// `conf` also means a directory that is *not* a database is never opened,
/// because `sled::open` on such a directory creates one.
pub fn find_sled_roots(dir: &std::path::Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    const MAX_DEPTH: usize = 4;

    fn walk(
        dir: &std::path::Path,
        depth: usize,
        out: &mut Vec<std::path::PathBuf>,
    ) -> anyhow::Result<()> {
        if depth > MAX_DEPTH {
            // Silently stopping here would report the databases found so far as
            // if they were all of them.
            anyhow::bail!(
                "sled discovery hit its depth bound of {MAX_DEPTH} at {}; the sweep is \
                 incomplete and its result cannot be treated as the full set",
                dir.display()
            );
        }

        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("cannot enumerate {}", dir.display()))?;

        for entry in entries {
            let entry =
                entry.with_context(|| format!("cannot read an entry of {}", dir.display()))?;
            let child = entry.path();

            // `file_type` does not follow symlinks, unlike `Path::is_dir`. A
            // symlinked directory is refused rather than skipped or followed:
            // following it lets the walk leave the intended subtree, and
            // skipping it would omit a database the daemon can still open —
            // which is the fail-open this whole function must not have.
            let file_type = entry
                .file_type()
                .with_context(|| format!("cannot stat {}", child.display()))?;
            if file_type.is_symlink() {
                anyhow::bail!(
                    "sled discovery found a symlink at {}; refusing to decide whether it \
                     names a database inside or outside this data directory",
                    child.display()
                );
            }
            if !file_type.is_dir() {
                continue;
            }

            // Record a database and keep walking beneath it: a root can hold
            // roots (see the doc comment), and a database's own subdirectories
            // hold no `conf`, so descending never lists one twice.
            if child.join("conf").is_file() {
                out.push(child.clone());
            }
            walk(&child, depth + 1, out)?;
        }
        Ok(())
    }

    let mut out = Vec::new();
    if dir.join("conf").is_file() {
        out.push(dir.to_path_buf());
    }
    walk(dir, 0, &mut out)?;
    // Deterministic order: a receipt that lists stores must list them the same
    // way on every run, whatever order the filesystem returned them in.
    out.sort();
    Ok(out)
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
            rationale: "Journal entries are content-addressed by entry hash; DIDs appear inside \
                        the value, not the key. Scanned to confirm the key carries no spelling.",
        },
        KeyspaceDescriptor {
            name: "trust-app/sequences_receiver",
            prefix: b"trust/sequences/receiver/",
            inventory_rows: &[71],
            disposition: MergeDisposition::MaxMonotonic,
            // Asserted by precedent, not implemented: `SequenceTracker`
            // (`apps/trust-app/src/sequence.rs`) reads and writes the exact
            // spelling and folds nothing, so two spellings of one issuer are
            // two independent replay floors and the issuer may submit under
            // whichever is lower. `replay_max_seq` earns `Established` because
            // its loader performs the fold (#2644); this one does not, so a
            // collision here must refuse until a trust-domain loader does.
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: true,
            principal_region: PrincipalRegion::WholeKey,
            rationale: "Last-seen attestation sequence per issuer — a replay floor. A lower \
                        survivor accepts stale attestations, so the merge would keep the \
                        maximum, as replay_max_seq does; but the receiver tracker reads and \
                        writes the exact spelling and implements no fold, so the rule is \
                        asserted, not established.",
        },
        KeyspaceDescriptor {
            name: "trust-app/sequences_issuer",
            prefix: b"trust/sequences/issuer/",
            inventory_rows: &[71],
            disposition: MergeDisposition::MaxMonotonic,
            basis: RuleBasis::AwaitingDomainSignOff,
            slash_ends_did: false,
            did_ends_key: true,
            principal_region: PrincipalRegion::WholeKey,
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
            principal_region: PrincipalRegion::WholeKey,
            rationale: "Cooperative membership. Merging two rows decides who is a member of an \
                        institution, which is an institutional judgement no identity-layer rule \
                        authorizes; it is also adjacent to the separate §7.5 membership gate. \
                        Fail closed pending a governance-domain decision.",
        },
        KeyspaceDescriptor {
            name: "icn-federation/attestations",
            prefix: b"federation/attestations/",
            inventory_rows: &[27, 59],
            disposition: MergeDisposition::FailClosed,
            basis: RuleBasis::Established,
            // `federation/attestations/<member-did>/<source_coop_id>`. The
            // member spelling is anchored right after the prefix; the source is
            // a federation-domain identifier this crate does not own and must
            // not parse. Nothing constrains a cooperative from choosing an id
            // that contains `did:icn:`, and `AttestationStore` compares source
            // ids as exact strings — so a whole-key scan would both group rows
            // the store holds apart and call rows unreadable that it reads
            // fine (#2704 review). The whole-key flags below say nothing here.
            slash_ends_did: false,
            did_ends_key: false,
            principal_region: PrincipalRegion::AnchoredThenOpaque { terminator: b'/' },
            rationale: "Federated trust attestations keyed by (member principal, source \
                        cooperative); the source stays in the canonical shape, so rows from \
                        different cooperatives about one principal are the ordinary union and \
                        never a group. Two rows from one cooperative about one principal can \
                        only differ by disagreeing, and no federation-domain rule authorizes \
                        choosing or combining them. The live store already refuses to read, \
                        write or sweep over such a pair (#2703); a migration must not decide \
                        it either.",
        },
        KeyspaceDescriptor {
            name: "icn-federation/agreement_party_index",
            prefix: b"idx_agreement_party/",
            inventory_rows: &[28],
            disposition: MergeDisposition::Equivalent,
            basis: RuleBasis::Established,
            // `idx_agreement_party/<party-did>/<agreement id>`: the attestation
            // layout's shape. The party spelling is anchored right after the
            // prefix and ends at the `/`; the agreement id is an identifier the
            // agreement's creator chose (`AgreementId::new` takes any string),
            // which this crate does not own and must not parse, and which
            // `AgreementStore` compares as exact bytes — its own parser anchors
            // the split on the id the row's value names (#2707). The whole-key
            // flags below say nothing here.
            slash_ends_did: false,
            did_ends_key: false,
            principal_region: PrincipalRegion::AnchoredThenOpaque { terminator: b'/' },
            rationale: "Secondary index projected from the canonical federation/agreements/ rows: \
                        key = (party spelling, agreement id), value = agreement id. The id stays \
                        in the canonical shape, so one party in two agreements is two shapes and \
                        never a group. The store answers a party lookup from the canonical \
                        parties under Did equality and can recompute the projection (#2707), so \
                        two spellings of one party for one agreement are two derivations of one \
                        canonical fact and keeping any one loses nothing. A projection row can \
                        never create, omit, preserve or alter membership on its own.",
        },
        KeyspaceDescriptor {
            name: "icn-gateway/adr0014_grant_by_grantee",
            prefix: b"adr0014:grant:by_grantee:",
            inventory_rows: &[25],
            disposition: MergeDisposition::Equivalent,
            basis: RuleBasis::Established,
            // `adr0014:grant:by_grantee:` ‖ u32-BE len ‖ tag ‖ grantee bytes
            // ‖ u64-BE valid_from ‖ 36-byte grant id. The length field, not a
            // delimiter, ends the grantee region — a terminator search would
            // run through the binary length bytes and through a `valid_from`
            // whose bytes are arbitrary. The tag, not the look of the bytes,
            // says whether a principal is there: tag 0x02 is an `Entity` id
            // the granting domain chose, and one that spells `did:icn:` is
            // still an entity id. The whole-key flags say nothing here.
            slash_ends_did: false,
            did_ends_key: false,
            principal_region: PrincipalRegion::LengthPrefixedTagged {
                principal_tag: 0x01,
            },
            rationale: "Secondary index projected from the canonical \
                        adr0014:grant:<uuid> records: key = (grantee region, valid_from, grant \
                        id), value = grant id. valid_from and the grant id stay in the canonical \
                        shape, so one principal holding two grants is two shapes and never a \
                        group — a principal may legitimately hold several. Two spellings of one \
                        principal for one grant are two derivations of one canonical fact: \
                        ReceiptStore answers a grantee lookup by reading the whole projection, \
                        decoding every Person-tagged spelling to its principal, and proving each \
                        candidate against the primary AuthorityGrant record before returning it \
                        (#2627 M2), so keeping any one row loses nothing. A projection row can \
                        never create or hide authority on its own. Entity-tagged rows carry no \
                        principal and are outside the I7 boundary.",
        },
        KeyspaceDescriptor {
            name: "icn-commons/holder_by_did",
            // `commons/holders/by_did/<spelling>` is the whole key: the writer
            // appends the DID and nothing else (`CommonsStore::put_holder`), so
            // `did_ends_key` states the writer's exact shape. The two sibling
            // holder subspaces are outside this prefix rather than members of
            // it — `commons/holders/<hex holder id>` and
            // `commons/holders/by_anchor/<hex anchor id>` are both keyed by
            // opaque hex and carry no spelling in the key at all, so neither
            // can be swallowed by this descriptor and neither is cleared by it.
            // A `did:icn:` that appears in a sibling's stored *value* is not
            // key material and is invisible to a key scan.
            prefix: b"commons/holders/by_did/",
            inventory_rows: &[67],
            disposition: MergeDisposition::FailClosed,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: true,
            principal_region: PrincipalRegion::WholeKey,
            rationale:
                "Holder-by-DID index over the weak-holder identity contract. A weak holder's \
                        durable id is SHA-256 of the textual spelling it was minted from, so two \
                        spellings of one principal name two independent CommonsHolderRecords \
                        with their own status, personhood level, affiliations and baseline \
                        rights — and the index rows that reach them cannot be merged without \
                        first deciding which holder survives, which is a question about a \
                        member's standing that no identity-layer rule answers. Two rows \
                        pointing at one holder id are refused on the same ground: no domain \
                        rule authorizes collapsing the spellings, and a rebuild must not pick. \
                        The live mint seam refuses the same state before it can be created \
                        (icn_commons::store::classify_holder_mint, #2627 M3); a migration must \
                        not decide it either. Already-derived duplicate holders are not \
                        dispositioned here.",
        },
        KeyspaceDescriptor {
            name: "icn-ledger/treasury",
            // `ledger:treasury:<did>` is the authoritative treasury record and
            // the only row beneath `ledger:treasury:` keyed by the treasury
            // principal alone. The lexical parent is shared with the budget,
            // rule, audit, index and velocity-limit subspaces, whose keys
            // carry no treasury principal in this position (two of them —
            // `audit:` and `idx:budgets:` — embed the spelling as key
            // structure further along), so the registered prefix runs through
            // the DID scheme and claims only the primary rows: a sibling row
            // is outside this descriptor, not a member of it, and a sibling
            // that carries a spelling stays *uncovered* until its own
            // disposition is argued. The whole-key tokenizer finds the one
            // spelling at the prefix boundary; `did_ends_key` says nothing
            // may follow it, which is the writer's exact shape (#2627 M1).
            prefix: b"ledger:treasury:did:icn:",
            inventory_rows: &[10, 41],
            disposition: MergeDisposition::FailClosed,
            basis: RuleBasis::Established,
            slash_ends_did: false,
            did_ends_key: true,
            principal_region: PrincipalRegion::WholeKey,
            rationale:
                "Authoritative treasury record keyed by the treasury principal. Two rows for \
                        one principal are two treasury records that can disagree about every \
                        field — cooperative, currency, creator, activity — and no economics rule \
                        authorizes choosing, summing or combining them. The live loader classifies \
                        every primary row and refuses to hydrate over such a pair \
                        (icn_ledger::principal_rows, #2627 M1); a migration must not decide it \
                        either.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SledStore;

    /// The whole-key layouts in registry order: the twelve the §3 evidence was
    /// gathered with, unchanged, then the treasury primary rows (#2627 M1).
    const WHOLE_KEY_NAMES: [&str; 14] = [
        "icn-net/replay_max_seq",
        "icn-net/replay_finalized",
        "icn-net/replay_sender_regime",
        "icn-net/outgoing_seq",
        "icn-ledger/balance",
        "icn-ledger/cleared_volume",
        "icn-ledger/frozen",
        "icn-trust/edges",
        "icn-ledger/journal",
        "trust-app/sequences_receiver",
        "trust-app/sequences_issuer",
        "icn-coop/member",
        "icn-commons/holder_by_did",
        "icn-ledger/treasury",
    ];

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
            principal_region: PrincipalRegion::WholeKey,
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
            "trust-app/sequences_receiver",
        ] {
            assert!(
                pending.contains(&expected),
                "{expected} has no domain sign-off and must not be automatable"
            );
        }
        assert_eq!(
            pending.len(),
            7,
            "the sign-off set is pinned exactly: {pending:?}"
        );
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
        // auditable rather than merely convenient — and must say what a
        // starting binary does about a collision there, and why.
        for d in n2a_deferred_namespaces() {
            assert!(!d.gate.is_empty(), "{} must name its gate", d.name);
            assert!(
                !d.inventory_rows.is_empty(),
                "{} must cite inventory",
                d.name
            );
            assert!(
                !d.posture_rationale.is_empty(),
                "{} must justify its collision posture",
                d.name
            );
        }
    }

    #[test]
    fn a_collision_in_a_block_startup_deferred_namespace_blocks() {
        // The security namespace is deferred for *disposition*, not for
        // detection: its loader folds alias rows into one principal-keyed map
        // and its shutdown save writes the survivor back. A collision there is
        // exactly the silent merge the gate exists to stop.
        let (a, b) = two_spellings(131);
        let store = store_with(&[
            (&format!("security:reputation:{a}"), b"v"),
            (&format!("security:reputation:{b}"), b"v"),
        ]);

        let a_ = audit(
            &store,
            &[descriptor(MergeDisposition::Sum)],
            &n2a_deferred_namespaces(),
        );

        assert_eq!(a_.uncovered_did_rows(), 0, "deferred is not uncovered");
        assert_eq!(a_.deferred_did_rows(), 2);
        let blocking = a_.blocking_deferred();
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0].name, "security/misbehavior");
        assert_eq!(blocking[0].report.collision_groups.len(), 1);
        assert!(!a_.is_clear(), "a lossy loader's collision must block");
    }

    #[test]
    fn a_block_startup_deferred_namespace_without_collisions_does_not_block() {
        // Control: the posture blocks on a collision, not on the namespace's
        // mere presence. Two different principals under the security prefix
        // are two rows, not a group.
        let one = spell(&principal(132), multibase::Base::Base58Btc);
        let two = spell(&principal(133), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("security:banned:{one}"), b"v"),
            (&format!("security:banned:{two}"), b"v"),
        ]);

        let a_ = audit(
            &store,
            &[descriptor(MergeDisposition::Sum)],
            &n2a_deferred_namespaces(),
        );

        assert_eq!(a_.deferred_did_rows(), 2);
        assert!(a_.blocking_deferred().is_empty());
        assert!(a_.is_clear());
    }

    #[test]
    fn a_collision_in_a_report_only_deferred_namespace_is_visible_but_does_not_block() {
        // Votes stay behind §7.5 and their loader writes nothing back, so a
        // collision is reported for that gate to see and does not stop the
        // binary. "Reported" is the load-bearing word: it must appear in the
        // audit, not vanish into a row count.
        let (a, b) = two_spellings(134);
        let store = store_with(&[
            (&format!("gov:vote:proposal-1:{a}"), b"v"),
            (&format!("gov:vote:proposal-1:{b}"), b"v"),
        ]);

        let a_ = audit(
            &store,
            &[descriptor(MergeDisposition::Sum)],
            &n2a_deferred_namespaces(),
        );

        let votes = a_
            .deferred_reports
            .iter()
            .find(|d| d.name == "governance/votes")
            .expect("vote namespace is reported");
        assert_eq!(votes.posture, DeferredCollisionPosture::ReportOnly);
        assert_eq!(
            votes.report.collision_groups.len(),
            1,
            "the collision is visible in the deferred report"
        );
        assert!(!votes.blocks());
        assert!(a_.blocking_deferred().is_empty());
        assert!(a_.is_clear());
    }

    #[test]
    fn an_unreadable_row_in_a_block_startup_deferred_namespace_blocks() {
        // A row whose principal cannot be read cannot be classified, so it
        // blocks under a blocking posture exactly as it would in a registered
        // keyspace.
        let store = store_with(&[("security:quarantine:did:icn:zNOTAKEY", b"v")]);

        let a_ = audit(
            &store,
            &[descriptor(MergeDisposition::Sum)],
            &n2a_deferred_namespaces(),
        );

        let security = a_
            .deferred_reports
            .iter()
            .find(|d| d.name == "security/misbehavior")
            .expect("security namespace is reported");
        assert_eq!(security.report.rows_unreadable, 1);
        assert!(security.blocks());
        assert!(!a_.is_clear());
    }

    #[test]
    fn deferred_reports_assert_no_merge_rule() {
        // A deferred report must never read as authority: whatever a renderer
        // does with its disposition, it says FAIL-CLOSED and unsigned-off.
        let one = spell(&principal(135), multibase::Base::Base58Btc);
        let store = store_with(&[(&format!("auth:challenge:{one}"), b"v")]);

        for d in scan_deferred(&store, &n2a_deferred_namespaces()).unwrap() {
            assert_eq!(
                d.report.disposition,
                MergeDisposition::FailClosed,
                "{}",
                d.name
            );
            assert_eq!(
                d.report.basis,
                RuleBasis::AwaitingDomainSignOff,
                "{}",
                d.name
            );
        }
    }

    #[test]
    fn sled_discovery_refuses_an_unreadable_directory_rather_than_shortening() {
        // The failure this pins: a directory that is searchable but not
        // readable. `read_dir` fails, and a walker that returned what it had so
        // far would report the databases it did find as if they were all of
        // them — so the startup gate would audit a subset, find it clean, and
        // write a CLEAR receipt over a store it never opened.
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();

        let visible = data_dir.join("visible");
        std::fs::create_dir_all(&visible).unwrap();
        std::fs::write(visible.join("conf"), b"x").unwrap();

        let hidden = data_dir.join("hidden");
        std::fs::create_dir_all(hidden.join("db")).unwrap();
        std::fs::write(hidden.join("db").join("conf"), b"x").unwrap();

        // Searchable but not readable: `hidden/db` can still be opened by path,
        // which is exactly why omitting it is unsafe rather than harmless.
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o111)).unwrap();

        let result = find_sled_roots(data_dir);

        // Restore before asserting, so a failure cannot leave an unreadable
        // directory behind for the tempdir cleanup to trip over.
        std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err = result.expect_err("an unreadable directory must refuse, not shorten the list");
        assert!(
            format!("{err:#}").contains("cannot enumerate"),
            "the refusal must name the enumeration failure, got: {err:#}"
        );
    }

    #[test]
    fn sled_discovery_refuses_a_symlink_rather_than_following_or_skipping_it() {
        // Following would let the sweep leave the data directory; skipping
        // would omit a database the daemon can still open through the link.
        // Neither is safe, so the sweep declines to decide.
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();

        let real = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("conf"), b"x").unwrap();

        std::os::unix::fs::symlink(&real, data_dir.join("linked")).unwrap();

        let err = find_sled_roots(data_dir).expect_err("a symlink must refuse");
        assert!(
            format!("{err:#}").contains("symlink"),
            "the refusal must name the symlink, got: {err:#}"
        );
    }

    #[test]
    fn audit_sled_store_uses_the_canonical_registries_and_every_tree() {
        // The shared entry point both the offline tool and the startup gate
        // render: it must consult the real registries, and a principal row in
        // a named tree must surface as unreachable rather than as absent.
        let (a, b) = two_spellings(136);
        let store = store_with(&[
            (&format!("ledger:balance:\"{a}\""), b"v"),
            (&format!("ledger:balance:\"{b}\""), b"v"),
        ]);
        let named = store.db().open_tree(b"aside").unwrap();
        named
            .insert(format!("x:{a}").as_bytes(), b"v".as_slice())
            .unwrap();

        let audit = audit_sled_store(&store).unwrap();

        let balance = audit
            .audit
            .report
            .keyspaces
            .iter()
            .find(|k| k.keyspace == "icn-ledger/balance")
            .expect("registry keyspace scanned");
        assert_eq!(balance.collision_groups.len(), 1);
        assert!(balance.must_fail_closed(), "balance rule is not signed off");
        assert_eq!(audit.audit.unreachable_did_rows, 1);
        assert!(audit
            .trees
            .iter()
            .any(|(name, n)| name == "aside" && *n == 1));
        assert!(!audit.is_clear());
    }

    #[test]
    fn find_sled_roots_finds_databases_at_any_level_including_inside_one() {
        let base = tempfile::tempdir().unwrap();
        let data_dir = base.path();

        // `store/` is itself a database (as `icnctl init-coop` leaves it), and
        // holds a database, a non-database directory, and a database nested
        // inside a non-database; one more database sits at the data-dir level.
        for rel in [
            "store",
            "store/ledger",
            "commons.sled",
            "store/deeper/nested",
        ] {
            let path = data_dir.join(rel);
            std::fs::create_dir_all(&path).unwrap();
            let db = sled::open(&path).unwrap();
            db.insert(b"k", b"v").unwrap();
            db.flush().unwrap();
        }
        std::fs::create_dir_all(data_dir.join("store/not-a-db")).unwrap();
        std::fs::write(data_dir.join("identity.age"), b"not a database").unwrap();

        let roots = find_sled_roots(data_dir).expect("a readable fixture tree enumerates");
        let rel: Vec<String> = roots
            .iter()
            .map(|r| r.strip_prefix(data_dir).unwrap().display().to_string())
            .collect();

        assert_eq!(
            rel,
            vec![
                "commons.sled",
                "store",
                "store/deeper/nested",
                "store/ledger"
            ],
            "path-ordered; a non-database directory is walked through, not listed; \
             a database inside a database is listed; a database's own \
             subdirectories (`blobs/`) are never listed as databases"
        );
        assert!(
            data_dir.join("store/blobs").is_dir(),
            "the fixture must exercise sled's own subdirectory"
        );
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
        //
        // `federation/attestations/` and `idx_agreement_party/` do put `/`
        // after a spelling, and do *not* appear here: each declares an anchored
        // region instead, which ends the spelling at the terminator by
        // construction. Saying it twice would be two owners for one fact.
        for d in n2a_keyspaces() {
            assert!(
                !d.slash_ends_did,
                "{} claims `/` ends a DID; confirm its parser really does",
                d.name
            );
        }
    }

    #[test]
    fn the_anchored_layouts_are_the_two_federation_keyspaces_in_registry_order() {
        // Pins which registered layouts end their one spelling at a terminator
        // and carry the remainder as an opaque discriminator. Both are
        // federation keyspaces of the shape `<prefix><did>/<domain id>`, and
        // they are pinned together because they differ in exactly the fact
        // the disposition records: an attestation pair is two claims and fails
        // closed; a party-index pair is two derivations of one canonical
        // agreement row and is equivalent. A new anchored layout must be added
        // here deliberately, with its own disposition argued (#2704, #2707).
        let anchored: Vec<(&str, u8, MergeDisposition)> = n2a_keyspaces()
            .iter()
            .filter_map(|d| match d.principal_region {
                PrincipalRegion::AnchoredThenOpaque { terminator } => {
                    Some((d.name, terminator, d.disposition))
                }
                PrincipalRegion::WholeKey | PrincipalRegion::LengthPrefixedTagged { .. } => None,
            })
            .collect();
        assert_eq!(
            anchored,
            vec![
                (
                    "icn-federation/attestations",
                    b'/',
                    MergeDisposition::FailClosed
                ),
                (
                    "icn-federation/agreement_party_index",
                    b'/',
                    MergeDisposition::Equivalent
                ),
            ]
        );
    }

    #[test]
    fn the_length_prefixed_layout_is_the_one_adr0014_projection() {
        // Pins the third layout the registry knows: a length-framed,
        // tag-discriminated region. It exists because neither of the other two
        // can read a binary key — a terminator search runs through the length
        // bytes, and a whole-key scan carries the spelling-derived length
        // field into the canonical shape and so groups nothing. A second such
        // layout must be added here deliberately, with its framing and its
        // disposition argued (#2627 M2).
        let framed: Vec<(&str, u8, MergeDisposition)> = n2a_keyspaces()
            .iter()
            .filter_map(|d| match d.principal_region {
                PrincipalRegion::LengthPrefixedTagged { principal_tag } => {
                    Some((d.name, principal_tag, d.disposition))
                }
                PrincipalRegion::WholeKey | PrincipalRegion::AnchoredThenOpaque { .. } => None,
            })
            .collect();
        assert_eq!(
            framed,
            vec![(
                "icn-gateway/adr0014_grant_by_grantee",
                0x01,
                MergeDisposition::Equivalent
            )]
        );
    }

    #[test]
    fn the_whole_key_layouts_are_the_pre_existing_keyspaces_unchanged() {
        // The anchored region is an addition, not a reinterpretation: every
        // keyspace registered before it still scans its whole key, in the
        // order it always had, so no existing descriptor's semantics moved
        // when the two federation layouts were added — nor when the treasury
        // primary rows were registered after them as the thirteenth
        // whole-key layout (#2627 M1), nor when the Commons holder-by-DID
        // index joined them as the fourteenth (#2627 M3).
        let whole_key: Vec<&str> = n2a_keyspaces()
            .iter()
            .filter(|d| matches!(d.principal_region, PrincipalRegion::WholeKey))
            .map(|d| d.name)
            .collect();
        assert_eq!(whole_key, WHOLE_KEY_NAMES);
    }

    #[test]
    fn a_descriptor_with_an_anchored_region_leaves_the_whole_key_flags_off() {
        // `slash_ends_did` and `did_ends_key` describe a whole-key scan, and
        // `build_report` reads neither under an anchored region. A descriptor
        // that set one anyway would be asserting something nothing acts on,
        // which is how a registry starts lying.
        for d in n2a_keyspaces() {
            if matches!(d.principal_region, PrincipalRegion::WholeKey) {
                continue;
            }
            assert!(
                !d.slash_ends_did && !d.did_ends_key,
                "{} declares an anchored region; the whole-key flags do not \
                 apply and must stay false",
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
            principal_region: PrincipalRegion::WholeKey,
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
            "icn-ledger/treasury", // `ledger:treasury:<did>`, nothing after
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
    // ----- federation/attestations (#2703) -----------------------------------
    //
    // Fixtures write the exact bytes `icn_federation::AttestationStore` writes:
    // `federation/attestations/<did spelling>/<source_coop_id>`. They use the
    // real registry rather than a test descriptor, so what they prove is what
    // the shipped scan — and, through `audit_store`, the startup gate — does.

    fn federation_descriptor() -> KeyspaceDescriptor {
        n2a_keyspaces()
            .into_iter()
            .find(|d| d.name == "icn-federation/attestations")
            .expect("federation/attestations/ is registered (#2703)")
    }

    fn attestation_key(spelling: &str, source: &str) -> String {
        format!("federation/attestations/{spelling}/{source}")
    }

    #[test]
    fn federation_alias_rows_from_one_source_are_a_blocking_collision() {
        // The #2703 hazard: one principal, two spellings, one source
        // cooperative. Two persisted claims that can only differ by disagreeing.
        let (a, b) = two_spellings(61);
        let store = store_with(&[
            (&attestation_key(&a, "food-coop"), b"{}"),
            (&attestation_key(&b, "food-coop"), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(report.collision_groups[0].rows.len(), 2);
        assert_eq!(report.collision_groups[0].representation_counts, vec![2]);
        assert_eq!(report.disposition, MergeDisposition::FailClosed);
        assert!(report.must_fail_closed(), "no rule may elect a survivor");

        // And the whole-store verdict the gate consumes says the same.
        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(!audit.is_clear());
        assert_eq!(
            audit.uncovered_did_rows(),
            0,
            "the rows are classified, not merely unaccounted for"
        );
        assert_eq!(
            audit
                .report
                .blocking_keyspaces()
                .iter()
                .map(|k| k.keyspace.as_str())
                .collect::<Vec<_>>(),
            vec!["icn-federation/attestations"]
        );
    }

    #[test]
    fn federation_alias_rows_from_different_sources_are_the_union_not_a_group() {
        // Same principal, two spellings, two source cooperatives. The source
        // stays in the canonical shape, so these are two different claims and
        // the store treats them as its ordinary union.
        let (a, b) = two_spellings(62);
        let store = store_with(&[
            (&attestation_key(&a, "food-coop"), b"{}"),
            (&attestation_key(&b, "housing-coop"), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(
            report.distinct_principals, 2,
            "two (principal, source) tuples"
        );
        assert!(report.collision_groups.is_empty());
        assert!(report.is_automatable());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.is_clear());
    }

    #[test]
    fn federation_rows_for_distinct_principals_do_not_collide() {
        let one = spell(&principal(63), multibase::Base::Base58Btc);
        let two = spell(&principal(64), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&attestation_key(&one, "food-coop"), b"{}"),
            (&attestation_key(&two, "food-coop"), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
        assert!(report.is_automatable());
    }

    #[test]
    fn federation_rows_are_covered_by_the_registry_not_reported_as_uncovered() {
        // Before registration a populated attestation row could only surface
        // as an *uncovered* shape — blocking, but unclassified. Now it is a
        // registered keyspace's row and appears nowhere else.
        let one = spell(&principal(65), multibase::Base::Base58Btc);
        let store = store_with(&[(&attestation_key(&one, "food-coop"), b"{}")]);

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.uncovered.is_empty(), "{:?}", audit.uncovered);
        assert_eq!(audit.deferred_did_rows(), 0);
        let ks = audit
            .report
            .keyspaces
            .iter()
            .find(|k| k.keyspace == "icn-federation/attestations")
            .unwrap();
        assert_eq!(ks.rows_scanned, 1);
        assert_eq!(ks.rows_with_readable_did, 1);
        assert_eq!(ks.inventory_rows, vec![27, 59]);
        assert!(audit.is_clear());
    }

    #[test]
    fn federation_source_after_the_slash_is_key_structure_not_a_bad_spelling() {
        // The layout's `/` is the one remainder the descriptor explains. A
        // source id made entirely of multibase-alphabet characters — every
        // real one is — must not be swallowed into the spelling or turn the
        // row unreadable.
        for base in [
            multibase::Base::Base58Btc,
            multibase::Base::Base16Lower,
            multibase::Base::Base64,
            multibase::Base::Base64Url,
        ] {
            let one = spell(&principal(66), base);
            let store = store_with(&[(&attestation_key(&one, "abc123XYZ-_"), b"{}")]);
            let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
            assert_eq!(report.rows_with_readable_did, 1, "{base:?}");
            assert_eq!(report.rows_unreadable, 0, "{base:?}");
            assert_eq!(report.distinct_principals, 1, "{base:?}");
        }
    }

    #[test]
    fn federation_malformed_spelling_is_unreadable_and_blocks() {
        // Two ways a key can fail to name a principal: junk where the spelling
        // goes, and a valid spelling with bytes glued on before the `/`. The
        // store's loader rejects both; the scan must not report either as a
        // readable principal, or the unreadable count that exists to fail
        // closed would be quietly lowered.
        let one = spell(&principal(67), multibase::Base::Base58Btc);
        let store = store_with(&[
            ("federation/attestations/did:icn:!!!!/food-coop", b"{}"),
            (
                &format!("federation/attestations/{one}junk/food-coop"),
                b"{}",
            ),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_unreadable, 2);
        assert_eq!(report.rows_with_readable_did, 0);
        assert!(report.must_fail_closed());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(!audit.is_clear());
    }

    #[test]
    fn federation_scan_order_pins_the_last_writer_survivor() {
        // Reported so an operator can see which row an unguarded rebuild would
        // have kept — and so the report is a function of the store, not of
        // insertion order.
        let (a, b) = two_spellings(68);
        let forward = store_with(&[
            (&attestation_key(&a, "food-coop"), b"{}"),
            (&attestation_key(&b, "food-coop"), b"{}"),
        ]);
        let reversed = store_with(&[
            (&attestation_key(&b, "food-coop"), b"{}"),
            (&attestation_key(&a, "food-coop"), b"{}"),
        ]);

        let f = scan_keyspace(&forward, &federation_descriptor()).unwrap();
        let r = scan_keyspace(&reversed, &federation_descriptor()).unwrap();
        assert_eq!(
            f, r,
            "same rows, same report, whatever order they were written"
        );
        let survivor = f.collision_groups[0].last_writer_survivor().unwrap();
        let mut sorted = [a.clone(), b.clone()];
        sorted.sort();
        assert_eq!(survivor.spellings, vec![sorted[1].clone()]);
    }

    // ----- the source is a discriminator, not a spelling (#2704 review, P2) --
    //
    // `AttestationStore` compares `source_coop_id` as exact bytes, and nothing
    // in the federation domain forbids a cooperative identifier that contains
    // `did:icn:`. A scan that canonicalized inside the source would disagree
    // with the store in both directions at once: grouping rows the store holds
    // apart, and calling rows unreadable that the store reads without trouble.

    #[test]
    fn a_source_id_containing_two_spellings_of_one_did_is_still_two_sources() {
        // Two source identifiers that are different strings are two claims,
        // even when the text inside them names one principal. Only the
        // federation domain could say otherwise, and it has not.
        let member = spell(&principal(70), multibase::Base::Base58Btc);
        let (source_a, source_b) = two_spellings(71);
        let store = store_with(&[
            (
                &attestation_key(&member, &format!("coop-{source_a}")),
                b"{}",
            ),
            (
                &attestation_key(&member, &format!("coop-{source_b}")),
                b"{}",
            ),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(
            report.distinct_principals, 2,
            "one member, two source ids — two (principal, source) tuples"
        );
        assert!(
            report.collision_groups.is_empty(),
            "grouping these would be a source-id normalization rule nobody wrote"
        );
        assert!(report.is_automatable());
    }

    #[test]
    fn a_source_id_that_is_itself_a_valid_spelling_is_still_just_a_source() {
        let member = spell(&principal(72), multibase::Base::Base58Btc);
        let (source_a, source_b) = two_spellings(73);
        let store = store_with(&[
            (&attestation_key(&member, &source_a), b"{}"),
            (&attestation_key(&member, &source_b), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.is_clear());
    }

    #[test]
    fn a_malformed_did_inside_a_source_id_leaves_the_row_readable() {
        // The store reads this row: it rebuilds the key from the value and
        // compares bytes. Reporting it unreadable would refuse a start over a
        // store that is fine — a scan stricter than the loader it stands for.
        let member = spell(&principal(74), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&attestation_key(&member, "did:icn:!!!!"), b"{}"),
            (&attestation_key(&member, "coop/did:icn:zzz"), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_unreadable, 0, "the source is never parsed");
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.distinct_principals, 2);
        assert!(report.is_automatable());
    }

    #[test]
    fn the_same_member_and_the_same_exact_source_collide_however_the_source_reads() {
        // The collision unit is unchanged by any of the above: one principal,
        // one *byte-identical* source, two spellings of the member.
        let (a, b) = two_spellings(75);
        let source = format!("coop-{}", spell(&principal(76), multibase::Base::Base58Btc));
        let store = store_with(&[
            (&attestation_key(&a, &source), b"{}"),
            (&attestation_key(&b, &source), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(
            report.collision_groups[0].representation_counts,
            vec![2],
            "two spellings at the one principal position"
        );
        assert!(report.must_fail_closed());
    }

    #[test]
    fn a_member_segment_that_names_no_principal_is_unreadable_not_absent() {
        // Three ways the anchor fails, and none of them is "this row has no
        // principal": the layout says one belongs there, and `AttestationStore`
        // refuses every one of these rows. Counting them as principal-free
        // would be non-blocking — a start over state nobody can classify.
        let one = spell(&principal(77), multibase::Base::Base58Btc);
        let store = store_with(&[
            ("federation/attestations/did:icn:!!!!/food-coop", b"{}"),
            (
                &format!("federation/attestations/{one}junk/food-coop"),
                b"{}",
            ),
            ("federation/attestations/not-a-did-at-all/food-coop", b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 3);
        assert_eq!(report.rows_unreadable, 3);
        assert_eq!(report.rows_with_readable_did, 0);
        assert_eq!(report.rows_without_did, 0, "no row here lacks a principal");
        assert!(report.must_fail_closed());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(!audit.is_clear());
    }

    #[test]
    fn a_member_spelling_with_no_source_after_it_is_unreadable() {
        // `AttestationStore` always writes the terminator and a source, so a
        // key that stops at the spelling is one no revocation, lookup or sweep
        // could attribute to a source cooperative.
        let one = spell(&principal(78), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("federation/attestations/{one}"), b"{}"),
            (&attestation_key(&one, ""), b"{}"),
        ]);

        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(
            report.rows_unreadable, 1,
            "the terminated row is readable; the bare spelling is not"
        );
        assert_eq!(report.rows_with_readable_did, 1);
    }

    /// A seed whose `Base64` spelling actually contains `/`.
    ///
    /// Searched rather than hard-coded so the hard case is *exercised*: a test
    /// that merely says "Base64 bodies can contain `/`" proves nothing if the
    /// one spelling it happens to build does not.
    fn seed_whose_base64_spelling_contains_a_slash() -> u8 {
        (0u8..=255)
            .find(|s| spell(&principal(*s), multibase::Base::Base64).contains('/'))
            .expect("some seed's Base64 body contains `/`")
    }

    #[test]
    fn a_member_spelling_containing_the_terminator_still_ends_at_the_right_slash() {
        // The case an alphabet alone cannot decide. A `Base64` body legally
        // contains `/`, so the first `/` after the prefix is not the end of the
        // spelling — only decoding says where it ends. Getting this wrong would
        // cut a real spelling in half and report a principal nobody wrote.
        let one = spell(
            &principal(seed_whose_base64_spelling_contains_a_slash()),
            multibase::Base::Base64,
        );
        assert!(
            one[8..].contains('/'),
            "fixture guard: this spelling must contain `/`"
        );

        for source in ["food-coop", "a/b", "did:icn:zzz", ""] {
            let store = store_with(&[(&attestation_key(&one, source), b"{}")]);
            let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
            assert_eq!(report.rows_with_readable_did, 1, "source {source:?}");
            assert_eq!(report.rows_unreadable, 0, "source {source:?}");
            assert_eq!(report.distinct_principals, 1, "source {source:?}");
        }

        // And the spelling ended where the source began, not somewhere inside
        // it: two rows differing only in the source are two tuples, and the
        // same source twice under one spelling is one row.
        let store = store_with(&[
            (&attestation_key(&one, "food-coop"), b"{}"),
            (&attestation_key(&one, "housing-coop"), b"{}"),
        ]);
        let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
    }

    #[test]
    fn the_scanner_and_the_store_agree_on_where_the_member_spelling_ends() {
        // Every base the production parser accepts. The `/`-in-the-body case
        // has its own fixture above, which searches for a spelling that really
        // contains one rather than assuming this seed's does.
        for base in [
            multibase::Base::Base58Btc,
            multibase::Base::Base16Lower,
            multibase::Base::Base64,
            multibase::Base::Base64Url,
            multibase::Base::Base32Lower,
        ] {
            for source in ["food-coop", "a/b", "did:icn:zzz", ""] {
                let one = spell(&principal(79), base);
                let store = store_with(&[(&attestation_key(&one, source), b"{}")]);
                let report = scan_keyspace(&store, &federation_descriptor()).unwrap();
                assert_eq!(
                    report.rows_with_readable_did, 1,
                    "{base:?} spelling with source {source:?}"
                );
                assert_eq!(report.rows_unreadable, 0, "{base:?} / {source:?}");
                assert_eq!(report.distinct_principals, 1, "{base:?} / {source:?}");
            }
        }
    }

    #[test]
    fn a_row_that_is_not_under_the_prefix_cannot_be_parsed_by_this_layout() {
        // `scan_keyspace` reads by prefix so this cannot arise there. The rule
        // is stated anyway: an anchored parser handed a key it does not
        // describe reports an unreadable row, never a confident one.
        let one = spell(&principal(80), multibase::Base::Base58Btc);
        let report = build_report(
            &federation_descriptor(),
            vec![(format!("elsewhere/{one}/food-coop").into_bytes(), 2)],
        );
        assert_eq!(report.rows_unreadable, 1);
        assert_eq!(report.rows_with_readable_did, 0);
    }

    // ----- idx_agreement_party/ (#2627 row #28, #2707) -----------------------
    //
    // Fixtures write the exact bytes `icn_federation::agreement::AgreementStore`
    // writes: `idx_agreement_party/<did spelling>/<agreement id>` valued by the
    // agreement id. They use the real registry entry rather than a test
    // descriptor, so what they prove is what the shipped scan — and, through
    // `audit_store`, the startup gate — does.
    //
    // The layout has the attestation layout's shape — one anchored spelling,
    // the terminator, an opaque discriminator another domain chose — under the
    // opposite disposition. An attestation pair is two persisted claims and
    // fails closed; a party-index pair for one agreement is two derivations of
    // one canonical `federation/agreements/` row, which the store proves
    // membership from on every read, so keeping any one loses nothing.

    fn party_index_descriptor() -> KeyspaceDescriptor {
        n2a_keyspaces()
            .into_iter()
            .find(|d| d.name == "icn-federation/agreement_party_index")
            .expect("idx_agreement_party/ is registered (#2627 row #28)")
    }

    fn party_index_key(spelling: &str, agreement_id: &str) -> String {
        format!("idx_agreement_party/{spelling}/{agreement_id}")
    }

    #[test]
    fn party_index_rows_are_readable_through_their_agreement_id_suffix() {
        // `/` follows the spelling and a generated agreement id
        // (`agr-<uuid>`) is made entirely of multibase body bytes. A whole-key
        // scan of this layout would swallow the id into the candidate run and
        // report the row unreadable; the anchored region ends the spelling at
        // the terminator and never reads what follows.
        let one = spell(&principal(81), multibase::Base::Base58Btc);
        let store = store_with(&[(
            &party_index_key(&one, "agr-0b1a4c1e-6b0d-4c1c-9a1d-3f6e2d1c0b9a"),
            b"agr-0b1a4c1e-6b0d-4c1c-9a1d-3f6e2d1c0b9a",
        )]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 1);
        assert_eq!(report.rows_with_readable_did, 1);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.distinct_principals, 1);
        assert!(report.collision_groups.is_empty());
    }

    #[test]
    fn party_index_alias_rows_for_one_agreement_are_equivalent_and_automatable() {
        // One party, two spellings, one agreement: two derivations of one
        // canonical fact. The scan sees the group; the registry says it needs
        // no human to resolve; the whole-store verdict the gate consumes is
        // clear with the rows classified rather than unaccounted for.
        let (a, b) = two_spellings(82);
        let store = store_with(&[
            (&party_index_key(&a, "agr-1"), b"agr-1"),
            (&party_index_key(&b, "agr-1"), b"agr-1"),
        ]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(report.collision_groups[0].rows.len(), 2);
        assert_eq!(report.collision_groups[0].representation_counts, vec![2]);
        assert_eq!(report.disposition, MergeDisposition::Equivalent);
        assert_eq!(report.basis, RuleBasis::Established);
        assert!(
            report.is_automatable(),
            "a projection collision needs no adjudication"
        );
        assert!(!report.must_fail_closed());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(
            audit.is_clear(),
            "an equivalent group does not block a start"
        );
        assert_eq!(
            audit.uncovered_did_rows(),
            0,
            "the rows are classified, not unaccounted for"
        );
        assert!(audit.report.blocking_keyspaces().is_empty());
    }

    #[test]
    fn party_index_rows_for_different_agreements_never_group() {
        // The agreement id stays in the canonical shape: one party in two
        // agreements is two facts, not a collision — under one spelling or
        // under two.
        let one = spell(&principal(83), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&party_index_key(&one, "agr-1"), b"agr-1"),
            (&party_index_key(&one, "agr-2"), b"agr-2"),
        ]);
        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(
            report.distinct_principals, 2,
            "two (party, agreement) tuples"
        );
        assert!(report.collision_groups.is_empty());

        // Same principal, alternate spellings, different agreements: the one
        // fact that differs from the alias-pair fixture is the discriminator,
        // and that is enough to keep them apart.
        let (a, b) = two_spellings(84);
        let store = store_with(&[
            (&party_index_key(&a, "agr-1"), b"agr-1"),
            (&party_index_key(&b, "agr-2"), b"agr-2"),
        ]);
        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.distinct_principals, 2);
        assert!(
            report.collision_groups.is_empty(),
            "grouping these would erase which agreement each row belongs to"
        );

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.is_clear());
    }

    #[test]
    fn party_index_rows_for_distinct_principals_do_not_collide() {
        let one = spell(&principal(85), multibase::Base::Base58Btc);
        let two = spell(&principal(86), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&party_index_key(&one, "agr-1"), b"agr-1"),
            (&party_index_key(&two, "agr-1"), b"agr-1"),
        ]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
        assert!(report.is_automatable());
    }

    #[test]
    fn party_index_rows_are_covered_by_the_registry_not_reported_as_uncovered() {
        // Before registration a populated party-index row could only surface
        // as an *uncovered* shape — blocking, but unclassified. Now it is a
        // registered keyspace's row and appears nowhere else.
        let one = spell(&principal(87), multibase::Base::Base58Btc);
        let store = store_with(&[(&party_index_key(&one, "agr-1"), b"agr-1")]);

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.uncovered.is_empty(), "{:?}", audit.uncovered);
        assert_eq!(audit.deferred_did_rows(), 0);
        let ks = audit
            .report
            .keyspaces
            .iter()
            .find(|k| k.keyspace == "icn-federation/agreement_party_index")
            .unwrap();
        assert_eq!(ks.rows_scanned, 1);
        assert_eq!(ks.rows_with_readable_did, 1);
        assert_eq!(ks.inventory_rows, vec![28]);
        assert!(audit.is_clear());
    }

    #[test]
    fn a_party_segment_that_names_no_principal_is_unreadable_not_absent() {
        // Three ways the anchor fails, and none of them is "this row has no
        // principal": the layout says one belongs there, and `AgreementStore`
        // refuses every one of these rows as malformed. The scan must not
        // lower the unreadable count — which exists to fail closed — by
        // calling `<did>junk` a readable prefix plus residue.
        let one = spell(&principal(88), multibase::Base::Base58Btc);
        let store = store_with(&[
            (
                &party_index_key("did:icn:zthisnamesnoprincipal", "agr-1"),
                b"agr-1",
            ),
            (&party_index_key("did:icn:!!!!", "agr-1"), b"agr-1"),
            (&format!("idx_agreement_party/{one}junk/agr-1"), b"agr-1"),
        ]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 3);
        assert_eq!(report.rows_unreadable, 3);
        assert_eq!(report.rows_with_readable_did, 0);
        assert_eq!(report.rows_without_did, 0, "no row here lacks a principal");
        assert!(report.must_fail_closed());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(!audit.is_clear());
    }

    #[test]
    fn a_party_spelling_with_no_agreement_id_after_it_is_unreadable() {
        // `AgreementStore` always writes the terminator and the agreement id,
        // so a key that stops at the spelling is one no lookup, replacement or
        // rebuild could attribute to an agreement. A terminated key with an
        // empty id is readable to the scan — the anchor holds a spelling and
        // the terminator follows it — and is refused by the store, whose
        // parser knows the id may not be empty: the loader is the stricter
        // layer there, exactly as §10.6 of the migration gate describes for
        // the ledger.
        let one = spell(&principal(89), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("idx_agreement_party/{one}"), b"agr-1"),
            (&party_index_key(&one, ""), b""),
        ]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(
            report.rows_unreadable, 1,
            "the terminated row is readable; the bare spelling is not"
        );
        assert_eq!(report.rows_with_readable_did, 1);
    }

    // ----- the agreement id is a discriminator, not a spelling ----------------
    //
    // `AgreementId::new` accepts any string and `AgreementStore` compares ids
    // as exact bytes, anchoring its own parse on the id the row's value names.
    // Nothing in the federation domain forbids an id that contains — or is —
    // a `did:icn:` spelling. A scan that canonicalized inside the id would
    // disagree with the store in both directions at once: grouping rows the
    // store holds apart, and calling rows unreadable that the store reads.

    #[test]
    fn an_agreement_id_containing_a_did_spelling_is_a_discriminator_not_a_principal() {
        let party = spell(&principal(90), multibase::Base::Base58Btc);
        let (other_a, other_b) = two_spellings(91);
        let store = store_with(&[
            (
                &party_index_key(&party, &format!("agr-{other_a}")),
                format!("agr-{other_a}").as_bytes(),
            ),
            (
                &party_index_key(&party, &format!("agr-{other_b}")),
                format!("agr-{other_b}").as_bytes(),
            ),
            (&party_index_key(&party, &other_a), other_a.as_bytes()),
            (&party_index_key(&party, "did:icn:!!!!"), b"did:icn:!!!!"),
            (&party_index_key(&party, "a/b"), b"a/b"),
        ]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 5);
        assert_eq!(
            report.rows_unreadable, 0,
            "the agreement id is never parsed"
        );
        assert_eq!(report.rows_with_readable_did, 5);
        assert_eq!(
            report.distinct_principals, 5,
            "one party, five agreement ids — five (party, agreement) tuples"
        );
        assert!(
            report.collision_groups.is_empty(),
            "grouping these would be an agreement-id normalization rule nobody wrote"
        );
        assert!(report.is_automatable());

        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.is_clear());
    }

    #[test]
    fn the_same_party_and_the_same_exact_agreement_id_collide_however_the_id_reads() {
        // The collision unit is unchanged by any of the above: one principal,
        // one *byte-identical* agreement id, two spellings of the party — and,
        // unlike the attestation layout, the group is equivalent.
        let (a, b) = two_spellings(92);
        let id = format!("agr-{}", spell(&principal(93), multibase::Base::Base58Btc));
        let store = store_with(&[
            (&party_index_key(&a, &id), id.as_bytes()),
            (&party_index_key(&b, &id), id.as_bytes()),
        ]);

        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(
            report.collision_groups[0].representation_counts,
            vec![2],
            "two spellings at the one principal position"
        );
        assert_eq!(report.disposition, MergeDisposition::Equivalent);
        assert!(report.is_automatable());
    }

    #[test]
    fn a_party_spelling_containing_the_terminator_still_ends_at_the_right_slash() {
        // A `Base64` body legally contains `/`, so the first `/` after the
        // prefix is not the end of the spelling — only decoding says where it
        // ends. This is the case the store's own parser handles by anchoring
        // on the id its value names, and the scan must agree with it.
        let one = spell(
            &principal(seed_whose_base64_spelling_contains_a_slash()),
            multibase::Base::Base64,
        );
        assert!(
            one[8..].contains('/'),
            "fixture guard: this spelling must contain `/`"
        );

        for id in ["agr-1", "a/b", "did:icn:zzz", ""] {
            let store = store_with(&[(&party_index_key(&one, id), id.as_bytes())]);
            let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
            assert_eq!(report.rows_with_readable_did, 1, "agreement id {id:?}");
            assert_eq!(report.rows_unreadable, 0, "agreement id {id:?}");
            assert_eq!(report.distinct_principals, 1, "agreement id {id:?}");
        }

        let store = store_with(&[
            (&party_index_key(&one, "agr-1"), b"agr-1"),
            (&party_index_key(&one, "agr-2"), b"agr-2"),
        ]);
        let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
    }

    #[test]
    fn the_scanner_and_the_store_agree_on_where_the_party_spelling_ends() {
        // Every base the production parser accepts, against ids of every shape
        // the store admits. The `/`-in-the-body case has its own fixture above.
        for base in [
            multibase::Base::Base58Btc,
            multibase::Base::Base16Lower,
            multibase::Base::Base64,
            multibase::Base::Base64Url,
            multibase::Base::Base32Lower,
        ] {
            for id in ["agr-1", "a/b", "did:icn:zzz", ""] {
                let one = spell(&principal(94), base);
                let store = store_with(&[(&party_index_key(&one, id), id.as_bytes())]);
                let report = scan_keyspace(&store, &party_index_descriptor()).unwrap();
                assert_eq!(
                    report.rows_with_readable_did, 1,
                    "{base:?} spelling with agreement id {id:?}"
                );
                assert_eq!(report.rows_unreadable, 0, "{base:?} / {id:?}");
                assert_eq!(report.distinct_principals, 1, "{base:?} / {id:?}");
            }
        }
    }

    #[test]
    fn party_index_scan_order_does_not_change_the_report() {
        // The report is a function of the store, not of insertion order — and
        // for an equivalent group the survivor is reported, never elected.
        let (a, b) = two_spellings(95);
        let forward = store_with(&[
            (&party_index_key(&a, "agr-1"), b"agr-1"),
            (&party_index_key(&b, "agr-1"), b"agr-1"),
        ]);
        let reversed = store_with(&[
            (&party_index_key(&b, "agr-1"), b"agr-1"),
            (&party_index_key(&a, "agr-1"), b"agr-1"),
        ]);

        let f = scan_keyspace(&forward, &party_index_descriptor()).unwrap();
        let r = scan_keyspace(&reversed, &party_index_descriptor()).unwrap();
        assert_eq!(
            f, r,
            "same rows, same report, whatever order they were written"
        );
        assert_eq!(f.collision_groups.len(), 1);
        assert!(f.is_automatable());
    }

    // ----- ledger:treasury:<did> (#2627 M1) ----------------------------------
    //
    // Fixtures write the exact bytes `TreasuryManager::persist_treasury`
    // writes: `ledger:treasury:<did spelling>`, nothing after. They use the
    // real registry descriptor, so what they prove is what the shipped scan —
    // and, through `audit_store`, the startup gate — does. The loader-side
    // pin, against the ledger's own prefix constants, is in `icn-ledger`
    // (`treasury.rs` tests); the loader fixtures are
    // `icn-ledger/tests/treasury_principal_rows.rs`.

    fn treasury_descriptor() -> KeyspaceDescriptor {
        n2a_keyspaces()
            .into_iter()
            .find(|d| d.name == "icn-ledger/treasury")
            .expect("the treasury keyspace is registered")
    }

    fn treasury_row(spelling: &str) -> String {
        format!("ledger:treasury:{spelling}")
    }

    const TREASURY_SIBLING_PREFIXES: [&str; 6] = [
        "ledger:treasury:budget:",
        "ledger:treasury:rule:",
        "ledger:treasury:audit:",
        "ledger:treasury:idx:coop:",
        "ledger:treasury:idx:budgets:",
        "ledger:treasury:vlimit:",
    ];

    #[test]
    fn the_treasury_descriptor_runs_through_the_did_scheme_and_claims_no_sibling() {
        // The registered prefix is the primary prefix plus the DID scheme, so
        // it matches every key the writer produces and no key beneath any
        // sibling subspace — a sibling prefix is neither inside nor around
        // it. The authoritative version of this pin, against the ledger's own
        // constants, is in `icn-ledger`; this one keeps the registry honest
        // on its own.
        let d = treasury_descriptor();
        assert_eq!(d.prefix, b"ledger:treasury:did:icn:");
        for sibling in TREASURY_SIBLING_PREFIXES {
            assert!(
                !sibling.as_bytes().starts_with(d.prefix)
                    && !d.prefix.starts_with(sibling.as_bytes()),
                "{sibling}"
            );
        }
        assert_eq!(d.disposition, MergeDisposition::FailClosed);
        assert_eq!(d.basis, RuleBasis::Established);
        assert!(d.did_ends_key, "nothing follows the spelling");
        assert!(!d.slash_ends_did);
        assert!(matches!(d.principal_region, PrincipalRegion::WholeKey));
        assert_eq!(d.inventory_rows, &[10, 41]);
    }

    #[test]
    fn treasury_alias_rows_are_a_blocking_collision() {
        let (a, b) = two_spellings(50);
        let store = store_with(&[(&treasury_row(&a), b"{}"), (&treasury_row(&b), b"{}")]);

        let report = scan_keyspace(&store, &treasury_descriptor()).unwrap();

        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert_eq!(report.collision_groups[0].representation_counts, vec![2]);
        assert!(!report.is_automatable());
        assert!(report.must_fail_closed());
    }

    #[test]
    fn treasury_rows_for_distinct_principals_do_not_collide() {
        let one = spell(&principal(51), multibase::Base::Base58Btc);
        let two = spell(&principal(52), multibase::Base::Base16Lower);
        let store = store_with(&[(&treasury_row(&one), b"{}"), (&treasury_row(&two), b"{}")]);

        let report = scan_keyspace(&store, &treasury_descriptor()).unwrap();

        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
        assert!(report.is_automatable());
    }

    #[test]
    fn treasury_sibling_rows_are_outside_the_descriptor_not_members_of_it() {
        // Every sibling subspace beneath the lexical parent, including the two
        // that embed a spelling as key structure — spelled here as the *alias*
        // of the primary row's principal, which is the strongest way to show
        // the descriptor never reads them as a second spelling of that row.
        let (a, b) = two_spellings(53);
        let store = store_with(&[
            (&treasury_row(&a), b"{}"),
            ("ledger:treasury:budget:budget-1", b"{}"),
            ("ledger:treasury:rule:rule-1", b"{}"),
            ("ledger:treasury:idx:coop:food-coop", a.as_bytes()),
            ("ledger:treasury:vlimit:vlimit-1", b"{}"),
            (
                &format!("ledger:treasury:audit:{b}:1700000000:audit-1"),
                b"{}",
            ),
            (
                &format!("ledger:treasury:idx:budgets:{b}:budget-1"),
                b"budget-1",
            ),
        ]);

        let report = scan_keyspace(&store, &treasury_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 1, "only the primary row is a member");
        assert_eq!(report.distinct_principals, 1);
        assert!(
            report.collision_groups.is_empty(),
            "a sibling's spelling is key structure there, never a treasury alias"
        );

        // The two siblings that carry a spelling are what they were before
        // M1 — principal-bearing rows under no registered keyspace. The
        // descriptor does not claim a disposition it has not argued; they
        // stay uncovered until their own registration (follow-up).
        let uncovered =
            uncovered_did_key_shapes(&store, &n2a_keyspaces(), &n2a_deferred_namespaces()).unwrap();
        assert_eq!(uncovered.len(), 2, "{uncovered:?}");
        assert_eq!(
            uncovered.get("ledger:treasury:audit:<did>:1700000000:audit-1"),
            Some(&1)
        );
        assert_eq!(
            uncovered.get("ledger:treasury:idx:budgets:<did>:budget-1"),
            Some(&1)
        );
    }

    #[test]
    fn a_did_looking_coop_id_in_the_treasury_index_is_not_a_treasury_spelling() {
        // Opaque-discriminator control. The cooperative index is keyed by a
        // coop id the ledger never validates, so one can be a DID spelling —
        // even an alias of the primary row's principal. To this descriptor it
        // is not a member (one row scanned, no group). Carrying a spelling
        // under no registered prefix, it surfaces as uncovered — unclassified,
        // exactly as before M1 — and never as a treasury collision.
        let (a, b) = two_spellings(54);
        let store = store_with(&[
            (&treasury_row(&a), b"{}"),
            (&format!("ledger:treasury:idx:coop:{b}"), a.as_bytes()),
        ]);

        let report = scan_keyspace(&store, &treasury_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 1);
        assert!(report.collision_groups.is_empty());

        let uncovered =
            uncovered_did_key_shapes(&store, &n2a_keyspaces(), &n2a_deferred_namespaces()).unwrap();
        assert_eq!(uncovered.get("ledger:treasury:idx:coop:<did>"), Some(&1));
        assert_eq!(uncovered.len(), 1);
    }

    #[test]
    fn a_treasury_key_with_material_after_the_spelling_is_unreadable() {
        // `did_ends_key`: the writer puts nothing after the spelling, and the
        // loader's `Did::from_str` consumes the whole remainder, so trailing
        // material is a row neither can read.
        let one = spell(&principal(55), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&format!("ledger:treasury:{one}junk"), b"{}"),
            (&format!("ledger:treasury:{one}:x"), b"{}"),
            ("ledger:treasury:did:icn:zNOTAKEY", b"{}"),
        ]);

        let report = scan_keyspace(&store, &treasury_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 3);
        assert_eq!(report.rows_unreadable, 3);
        assert_eq!(report.rows_with_readable_did, 0);
        assert!(report.must_fail_closed());
    }

    #[test]
    fn treasury_rows_are_covered_by_the_registry_not_reported_as_uncovered() {
        // Before this registration an ordinary treasury row could only
        // surface as an uncovered shape — blocking, and unclassified.
        let one = spell(&principal(56), multibase::Base::Base58Btc);
        let store = store_with(&[
            (&treasury_row(&one), b"{}"),
            ("ledger:treasury:idx:coop:food-coop", one.as_bytes()),
        ]);

        let uncovered =
            uncovered_did_key_shapes(&store, &n2a_keyspaces(), &n2a_deferred_namespaces()).unwrap();
        assert!(uncovered.is_empty(), "{uncovered:?}");
        let audit = audit_store(&store, &n2a_keyspaces(), &n2a_deferred_namespaces(), 0).unwrap();
        assert!(audit.is_clear());
    }

    #[test]
    fn treasury_scan_order_pins_the_last_writer_survivor() {
        // What the pre-M1 loader elected: `Store::scan` is lexicographic, so
        // the base58 (`z`) row scans after the base16 (`f`) row whichever was
        // written first, and its value was the one that survived the fold.
        let (z, f) = two_spellings(57);
        let store = store_with(&[(&treasury_row(&z), b"{}"), (&treasury_row(&f), b"{}")]);

        let report = scan_keyspace(&store, &treasury_descriptor()).unwrap();
        let group = &report.collision_groups[0];
        assert_eq!(group.rows.len(), 2);
        assert_eq!(group.last_writer_survivor().unwrap().spellings, vec![z]);
    }

    // ---- ADR-0014 by-grantee projection (#2627 M2) ----
    //
    // Same shape as the party index above — a projection whose alias rows are
    // equivalent derivations of one canonical row — but a *binary* layout: the
    // grantee region is length-framed and tag-discriminated, so neither the
    // whole-key tokenizer nor a terminator search can read it. These fixtures
    // pin the structural rule, not ADR-0014's authority semantics.

    fn grant_by_grantee_descriptor() -> KeyspaceDescriptor {
        n2a_keyspaces()
            .into_iter()
            .find(|d| d.name == "icn-gateway/adr0014_grant_by_grantee")
            .expect("adr0014:grant:by_grantee: is registered (#2627 row #25)")
    }

    fn store_with_raw(rows: &[(Vec<u8>, &[u8])]) -> SledStore {
        let store = SledStore::temporary().unwrap();
        for (key, value) in rows {
            store.put(key, value).unwrap();
        }
        store
    }

    /// Reproduce `ReceiptStore::grant_by_grantee_key` byte-for-byte.
    fn grantee_key(tag: u8, body: &[u8], valid_from: u64, grant_id: &str) -> Vec<u8> {
        let mut region = vec![tag];
        region.extend_from_slice(body);
        let mut key = b"adr0014:grant:by_grantee:".to_vec();
        key.extend_from_slice(&(region.len() as u32).to_be_bytes());
        key.extend_from_slice(&region);
        key.extend_from_slice(&valid_from.to_be_bytes());
        key.extend_from_slice(grant_id.as_bytes());
        key
    }

    fn person_key(spelling: &str, valid_from: u64, grant_id: &str) -> Vec<u8> {
        grantee_key(0x01, spelling.as_bytes(), valid_from, grant_id)
    }

    const GRANT_A: &str = "11111111-1111-4111-8111-111111111111";
    const GRANT_B: &str = "22222222-2222-4222-8222-222222222222";

    #[test]
    fn a_person_grant_row_is_readable_through_its_binary_framing() {
        // The length field ends the spelling and the 8-byte `valid_from` plus
        // 36-byte grant id after it are never parsed. A whole-key scan would
        // swallow part of that tail into the candidate run.
        let one = spell(&principal(91), multibase::Base::Base58Btc);
        let store = store_with_raw(&[(person_key(&one, 1_000, GRANT_A), GRANT_A.as_bytes())]);

        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 1);
        assert_eq!(report.rows_with_readable_did, 1);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.distinct_principals, 1);
        assert!(report.collision_groups.is_empty());
    }

    #[test]
    fn grantee_alias_rows_for_one_grant_are_equivalent_and_automatable() {
        // One principal, two spellings, one grant: two derivations of one
        // canonical `adr0014:grant:<uuid>` record. The varying u32 length
        // field must not keep them apart — that is what the region-spanning
        // canonical shape buys.
        let (a, b) = two_spellings(92);
        let store = store_with_raw(&[
            (person_key(&a, 1_000, GRANT_A), GRANT_A.as_bytes()),
            (person_key(&b, 1_000, GRANT_A), GRANT_A.as_bytes()),
        ]);

        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 2);
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.distinct_principals, 1);
        assert_eq!(report.collision_groups.len(), 1);
        assert!(
            report.is_automatable(),
            "an equivalent projection pair needs no human to resolve"
        );
    }

    #[test]
    fn control_two_grants_for_one_principal_are_two_shapes_not_a_collision() {
        // A principal may legitimately hold several distinct grants. The grant
        // id stays in the canonical shape, so this must never group.
        let (a, b) = two_spellings(93);
        let store = store_with_raw(&[
            (person_key(&a, 1_000, GRANT_A), GRANT_A.as_bytes()),
            (person_key(&b, 2_000, GRANT_B), GRANT_B.as_bytes()),
        ]);

        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.rows_with_readable_did, 2);
        assert_eq!(report.distinct_principals, 2);
        assert!(
            report.collision_groups.is_empty(),
            "two grants for one principal are two grants"
        );
    }

    #[test]
    fn control_two_principals_stay_separate() {
        let one = spell(&principal(94), multibase::Base::Base58Btc);
        let two = spell(&principal(95), multibase::Base::Base58Btc);
        let store = store_with_raw(&[
            (person_key(&one, 1_000, GRANT_A), GRANT_A.as_bytes()),
            (person_key(&two, 1_000, GRANT_A), GRANT_A.as_bytes()),
        ]);

        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.distinct_principals, 2);
        assert!(report.collision_groups.is_empty());
    }

    #[test]
    fn an_entity_row_that_spells_a_did_is_not_a_principal() {
        // Tag 0x02 is an entity id the granting domain chose. Its bytes are
        // not this registry's to decode, however much they look like a DID.
        let looks_like = spell(&principal(96), multibase::Base::Base58Btc);
        let store = store_with_raw(&[(
            grantee_key(0x02, looks_like.as_bytes(), 1_000, GRANT_A),
            GRANT_A.as_bytes(),
        )]);

        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.rows_scanned, 1);
        assert_eq!(
            report.rows_with_readable_did, 0,
            "an entity id is not a principal"
        );
        assert_eq!(report.rows_unreadable, 0, "and it is not unreadable either");
        assert_eq!(report.distinct_principals, 0);
    }

    #[test]
    fn a_person_row_whose_spelling_names_no_principal_is_unreadable() {
        let store = store_with_raw(&[(
            person_key("did:icn:not-a-spelling!!", 1_000, GRANT_A),
            GRANT_A.as_bytes(),
        )]);

        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.rows_unreadable, 1);
        assert_eq!(report.rows_with_readable_did, 0);
    }

    #[test]
    fn broken_binary_framing_is_unreadable_not_principal_free() {
        // A truncated length field, a length that overruns the key, and an
        // unknown tag are three different ways to be a row this writer could
        // not have produced. The first two cannot be classified at all; the
        // third names no principal by the layout's own rule.
        let one = spell(&principal(97), multibase::Base::Base58Btc);

        let mut truncated = b"adr0014:grant:by_grantee:".to_vec();
        truncated.extend_from_slice(&[0u8, 0u8]);

        let mut overrun = b"adr0014:grant:by_grantee:".to_vec();
        overrun.extend_from_slice(&u32::MAX.to_be_bytes());
        overrun.extend_from_slice(b"\x01did:icn:z");

        for (label, key) in [("truncated", truncated), ("overrun", overrun)] {
            let store = store_with_raw(&[(key, GRANT_A.as_bytes())]);
            let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
            assert_eq!(report.rows_unreadable, 1, "{label} must be unreadable");
        }

        // An undefined tag is well-framed and simply carries no principal.
        let store = store_with_raw(&[(
            grantee_key(0x09, one.as_bytes(), 1_000, GRANT_A),
            GRANT_A.as_bytes(),
        )]);
        let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
        assert_eq!(report.rows_unreadable, 0);
        assert_eq!(report.rows_with_readable_did, 0);
    }

    #[test]
    fn every_framing_boundary_is_classified_and_none_panics() {
        // The region is consumed from binary framing, so each way the framing
        // can be wrong gets its own row. None may panic or slice unchecked,
        // and a broken Person region must never fall through to being read as
        // another tag's opaque value.
        let pfx = b"adr0014:grant:by_grantee:";

        // No length field at all.
        let no_len = pfx.to_vec();
        // A length field one byte short of its width.
        let mut short_len = pfx.to_vec();
        short_len.extend_from_slice(&[0u8, 0, 0]);
        // A zero-length region: framed, but not even a tag inside.
        let mut zero_len = pfx.to_vec();
        zero_len.extend_from_slice(&0u32.to_be_bytes());
        zero_len.extend_from_slice(&1_000u64.to_be_bytes());
        zero_len.extend_from_slice(GRANT_A.as_bytes());
        // Person tag with an empty body.
        let mut empty_person = pfx.to_vec();
        empty_person.extend_from_slice(&1u32.to_be_bytes());
        empty_person.push(0x01);
        empty_person.extend_from_slice(&1_000u64.to_be_bytes());
        empty_person.extend_from_slice(GRANT_A.as_bytes());
        // Person tag whose body is not UTF-8.
        let mut bad_utf8 = pfx.to_vec();
        bad_utf8.extend_from_slice(&4u32.to_be_bytes());
        bad_utf8.extend_from_slice(&[0x01, 0xff, 0xfe, 0xfd]);
        bad_utf8.extend_from_slice(&1_000u64.to_be_bytes());
        bad_utf8.extend_from_slice(GRANT_A.as_bytes());
        // A length field claiming the whole address space.
        let mut huge = pfx.to_vec();
        huge.extend_from_slice(&u32::MAX.to_be_bytes());
        huge.push(0x01);

        for (label, key) in [
            ("no length field", no_len),
            ("short length field", short_len),
            ("zero-length region", zero_len),
            ("empty person body", empty_person),
            ("non-utf8 person body", bad_utf8),
            ("huge declared length", huge),
        ] {
            let store = store_with_raw(&[(key, GRANT_A.as_bytes())]);
            let report = scan_keyspace(&store, &grant_by_grantee_descriptor()).unwrap();
            assert_eq!(report.rows_scanned, 1, "{label}");
            assert_eq!(
                report.rows_unreadable, 1,
                "{label} must be unreadable, not silently principal-free"
            );
            assert_eq!(report.rows_with_readable_did, 0, "{label}");
        }
    }

    #[test]
    fn a_person_grant_row_is_never_reported_as_uncovered() {
        // Before registration a single ordinary Person grant produced an
        // uncovered shape, which the startup gate treats as a blocker.
        let one = spell(&principal(98), multibase::Base::Base58Btc);
        let store = store_with_raw(&[(person_key(&one, 1_000, GRANT_A), GRANT_A.as_bytes())]);

        let shapes =
            uncovered_did_key_shapes(&store, &n2a_keyspaces(), &n2a_deferred_namespaces()).unwrap();
        assert!(
            shapes.is_empty(),
            "the registered prefix must claim this row; got {shapes:?}"
        );
    }
}
