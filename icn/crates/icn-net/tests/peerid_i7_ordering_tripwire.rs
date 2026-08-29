//! #2627 / I7 — `PeerId::Ord` is atomic with the `Did` equality change.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! # What this file is for
//!
//! `IDENTITY_SEMANTICS.md` §11 I7 intends `Did` equality to become *key* equality — two
//! textual spellings that decode to one 32-byte identifier become one principal. `PeerId`
//! derives `PartialEq`/`Eq`/`Hash` from `Did`, so those follow automatically. Its `Ord` does
//! not: `topology.rs` compares `self.0.to_string()`, the spelling.
//!
//! Rust's ordered collections rely on the two agreeing:
//!
//! ```text
//! a == b   <=>   a.cmp(b) == Ordering::Equal
//! ```
//!
//! Today both sides are *false* for a re-spelled pair, so they agree. After I7 the left side
//! becomes true; unless `PeerId::Ord` changes in the same patch, the right side stays false and
//! a `BTreeSet<PeerId>` will hold two entries that compare equal while a `HashMap<PeerId, _>`
//! holds one. `NeighborSets` uses **both** for the same peers, so the two halves of one struct
//! would then disagree about how many peers exist, whether a removal succeeded, and whether a
//! neighbour-class limit is reached.
//!
//! # Why `PeerId::Ord` cannot simply be fixed ahead of time
//!
//! Consistency has to hold in whichever regime production is in:
//!
//! * **today** `a == b` is false for a re-spelled pair, so `cmp` must **not** return `Equal`;
//! * **after I7** `a == b` is true for that same pair, so `cmp` **must** return `Equal`.
//!
//! No single comparator satisfies both while `Did::Eq` is unchanged. Ordering by identifier
//! bytes today would make `cmp` return `Equal` for values that are still `!=`, and a
//! `BTreeSet` would silently drop one of two currently-distinct peers — the inverse violation,
//! and a live regression rather than a latent one. So the comparator moves *with* I7, and this
//! file exists to make that dependency mechanical instead of remembered.
//!
//! # How the tripwire is built so it is never wrong
//!
//! [`peerid_eq_and_ord_must_agree_in_whatever_regime_production_is_in`] asserts the
//! biconditional itself rather than either side's current value. It therefore passes on today's
//! `main`, passes again after a *correct* I7 patch that moves `Did::Eq` and `PeerId::Ord`
//! together, and fails **only** in a half-completed state. A test that instead asserted
//! "these are unequal today" would go red on a correct future patch, which would train the next
//! implementer to delete it.
//!
//! That property is easy to lose by accident, so it is measured rather than assumed. Every
//! test here except [`post_i7_regime_record_every_spelling_is_one_peer`] — which is
//! deliberately regime-dependent and says so — was run against a simulated correct I7 patch
//! (`Did` equality and hash over `identifier_bytes`, `PeerId::Ord` over the same, both with a
//! defined order for identifiers that do not decode) and stayed green. Anything added here
//! should be checked the same way: a guard that fires on the *correct* patch is worse than no
//! guard, because it argues for its own deletion at exactly the wrong moment.
//!
//! One trap worth naming for whoever re-runs that check: restoring a mutated file with a tool
//! that preserves mtime (`cp -p`, `shutil.copy2`) sets the timestamp backwards, so cargo
//! considers the crate fresh and silently keeps the mutated rlib. Confirm `Compiling
//! icn-identity` and `Compiling icn-net` actually appear before believing a result.
//!
//! # The identity source
//!
//! The projection helpers key on [`Did::identifier_bytes`] — the accessor N2-A itself intends
//! to use, and the only one defined across the whole accepted `Did` domain.
//! `SenderPrincipal::from_did` is deliberately **not** used: it is narrower, failing for any
//! DID that does not decode to an Ed25519 point, and
//! [`an_anchor_derived_principal_is_a_first_class_peer`] proves that difference is real rather
//! than theoretical.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use icn_identity::{Did, KeyPair};
use icn_net::{NeighborLimitsConfig, NeighborSets, NodeRole, PeerId, TopologyInfo};

/// Every base that can spell an Ed25519 key *other than* the canonical base58btc.
///
/// Kept identical to the list in `tests/respelled_envelope_replay.rs` and
/// `handlers/signed.rs`, which is the repository's measured acceptance class for
/// `Did::from_str` (`docs/architecture/n2-a0-stored-key-inventory.md`). `Identity` is absent
/// because `multibase::encode` panics on non-UTF-8 bytes; `Base58Btc` is absent because it is
/// what `Did::from_public_key` emits.
const ALTERNATE_SPELLINGS: [(&str, multibase::Base); 22] = [
    ("base2", multibase::Base::Base2),
    ("base8", multibase::Base::Base8),
    ("base10", multibase::Base::Base10),
    ("base16-lower", multibase::Base::Base16Lower),
    ("base16-upper", multibase::Base::Base16Upper),
    ("base32-lower", multibase::Base::Base32Lower),
    ("base32-upper", multibase::Base::Base32Upper),
    ("base32-pad-lower", multibase::Base::Base32PadLower),
    ("base32-pad-upper", multibase::Base::Base32PadUpper),
    ("base32-hex-lower", multibase::Base::Base32HexLower),
    ("base32-hex-upper", multibase::Base::Base32HexUpper),
    ("base32-hex-pad-lower", multibase::Base::Base32HexPadLower),
    ("base32-hex-pad-upper", multibase::Base::Base32HexPadUpper),
    ("base32-z", multibase::Base::Base32Z),
    ("base36-lower", multibase::Base::Base36Lower),
    ("base36-upper", multibase::Base::Base36Upper),
    ("base58-flickr", multibase::Base::Base58Flickr),
    ("base64", multibase::Base::Base64),
    ("base64-pad", multibase::Base::Base64Pad),
    ("base64-url", multibase::Base::Base64Url),
    ("base64-url-pad", multibase::Base::Base64UrlPad),
    ("base256-emoji", multibase::Base::Base256Emoji),
];

/// The identifier a DID names, by the rule `icn-identity` owns and N2-A intends to key on.
fn principal_of(did: &Did) -> [u8; 32] {
    did.identifier_bytes()
        .expect("every DID reachable through a public constructor decodes to 32 bytes")
}

/// The equality relation I7 intends to install, computed locally.
///
/// Production `Did::PartialEq` is **not** touched, monkey-patched or shadowed anywhere in this
/// file. This is only how the tests know which pairs the transition will move.
fn same_principal_after_i7(a: &PeerId, b: &PeerId) -> bool {
    principal_of(&a.0) == principal_of(&b.0)
}

/// Re-spell one principal under one base.
///
/// A spelling that stops parsing is a failure, not a skip: under current policy every base
/// here is accepted, and letting a rejection quietly reduce coverage is how this kind of suite
/// rots. If N2-A pins encodings at parse instead, that is a real behaviour change and belongs
/// here explicitly.
fn alias_in(base: multibase::Base, label: &str, canonical: &Did) -> Did {
    let bytes = principal_of(canonical);
    let alias = Did::from_str(&format!("did:icn:{}", multibase::encode(base, bytes)))
        .unwrap_or_else(|e| {
            panic!(
                "the {label} spelling is accepted by `Did::from_str` under current policy and \
                 this suite's coverage depends on it; it was rejected: {e}. If that is an \
                 intentional tightening (N2-A / #2627), move {label} out of \
                 ALTERNATE_SPELLINGS and assert its rejection explicitly."
            )
        });
    assert_ne!(
        alias.as_str(),
        canonical.as_str(),
        "CONTROL: the {label} alias must be a different *string*, or it proves nothing"
    );
    assert_eq!(
        principal_of(&alias),
        bytes,
        "CONTROL: the {label} alias must name the *same principal*, or it is a different peer"
    );
    alias
}

/// One principal, spelled every accepted way: canonical first, then all 22 aliases.
fn one_principal_every_spelling() -> Vec<(&'static str, PeerId)> {
    let canonical = KeyPair::generate().unwrap().did().clone();
    let mut out = vec![("base58btc-canonical", PeerId(canonical.clone()))];
    out.extend(
        ALTERNATE_SPELLINGS
            .iter()
            .map(|(label, base)| (*label, PeerId(alias_in(*base, label, &canonical)))),
    );
    out
}

fn a_peer() -> PeerId {
    PeerId(KeyPair::generate().unwrap().did().clone())
}

fn topology(region: &str, cluster: &str) -> TopologyInfo {
    TopologyInfo {
        region: region.to_string(),
        cluster_id: cluster.to_string(),
        role: NodeRole::Edge,
    }
}

// ---------------------------------------------------------------------------
// 1. Container agreement, and a record of the current regime.
// ---------------------------------------------------------------------------

/// Regime-independent: whatever a re-spelled principal counts as, both containers must agree.
///
/// This never needs editing. One principal spelled 23 ways is 23 peers today and 1 peer after
/// I7; either answer is coherent, and the two containers disagreeing is not.
#[test]
fn ordered_and_hashed_agree_on_one_respelled_principal() {
    let spellings = one_principal_every_spelling();

    for (_, peer) in &spellings {
        assert_eq!(
            principal_of(&peer.0),
            principal_of(&spellings[0].1 .0),
            "CONTROL: every spelling names one principal, so the counts below are about \
             spelling alone"
        );
    }

    // Built the way production builds them: `NeighborSets::add_neighbor` calls
    // `BTreeSet::insert`, one peer at a time.
    let mut ordered = BTreeSet::new();
    let mut hashed = HashSet::new();
    for (_, peer) in &spellings {
        ordered.insert(peer.clone());
        hashed.insert(peer.clone());
    }

    assert_eq!(
        ordered.len(),
        hashed.len(),
        "a BTreeSet sees {} spellings of one principal and a HashSet sees {}. See \
         `peerid_eq_and_ord_must_agree_in_whatever_regime_production_is_in` for the cause: \
         `PeerId::Ord` and the derived `Hash`/`Eq` have stopped agreeing.",
        ordered.len(),
        hashed.len()
    );

    // A second, independent consequence of the same incoherence, worth asserting because it is
    // so easy to miss: `BTreeSet` has two construction paths, and they only agree while `Ord`
    // is a correct total order consistent with `Eq`. `FromIterator` sorts and bulk-builds;
    // `insert` descends the tree. Measured under a simulated I7 half-completion, the same 23
    // values gave 1 by `collect` and 23 by `insert` — the same container, built two ways,
    // disagreeing about its own contents.
    let collected: BTreeSet<PeerId> = spellings.iter().map(|(_, p)| p.clone()).collect();
    assert_eq!(
        collected.len(),
        ordered.len(),
        "`BTreeSet` built by `collect` holds {} peers and the same values inserted one at a \
         time hold {}. A container cannot disagree with itself unless `PeerId::Ord` is no \
         longer a total order consistent with `PeerId::Eq` — see the tripwire test in this \
         file, and #2627.",
        collected.len(),
        ordered.len()
    );
}

/// A record of which regime production is in. **Edited by the I7 patch, as designed.**
///
/// Unlike every other test here this one is deliberately regime-*dependent*, so that the count
/// change is visible in the I7 diff rather than implicit. It read 23 — one peer per accepted
/// spelling — until `Did` equality became principal equality (#2627); it now reads 1.
///
/// The flip was made only after every regime-agnostic test in this file was green under the
/// new implementation, which is the order the pre-I7 version of this test demanded. It is the
/// single expectation in this suite that I7 was permitted to move.
#[test]
fn post_i7_regime_record_every_spelling_is_one_peer() {
    let spellings = one_principal_every_spelling();
    assert_eq!(spellings.len(), 23, "canonical plus 22 aliases");

    let mut ordered = BTreeSet::new();
    for (_, peer) in &spellings {
        ordered.insert(peer.clone());
    }

    assert_eq!(
        ordered.len(),
        1,
        "one principal spelled 23 ways counts as {} peers, not 1.\n\
         \n\
         If it is 23, `Did` equality has reverted to comparing spellings and I7 (#2627) has \
         been undone — the 23 accepted multibase spellings of one identifier are one \
         cryptographic principal, and `PeerId` must hold one peer for them. Any other value \
         means `PeerId::Ord` and the derived `Eq`/`Hash` disagree; see \
         `peerid_eq_and_ord_must_agree_in_whatever_regime_production_is_in`.",
        ordered.len()
    );
}

// ---------------------------------------------------------------------------
// 2. The tripwire.
// ---------------------------------------------------------------------------

#[test]
fn peerid_eq_and_ord_must_agree_in_whatever_regime_production_is_in() {
    // Rust's law for ordered collections, asserted as a biconditional so it holds before and
    // after I7 and fails only in between.
    let spellings = one_principal_every_spelling();
    let unrelated = a_peer();

    let mut population: Vec<(&str, PeerId)> = spellings.clone();
    population.push(("unrelated-principal", unrelated));

    let mut transition_relevant_pairs = 0usize;

    for (label_a, a) in &population {
        for (label_b, b) in &population {
            let eq_says_same = a == b;
            let ord_says_same = a.cmp(b) == std::cmp::Ordering::Equal;

            assert_eq!(
                eq_says_same, ord_says_same,
                "`PeerId` `Eq` and `Ord` disagree about {label_a} vs {label_b}.\n\
                 \n\
                 If `Eq` says equal and `Ord` does not, `Did` equality has adopted I7 \
                 (#2627, IDENTITY_SEMANTICS.md §11) while `PeerId::Ord` in \
                 `icn-net/src/topology.rs` still compares `self.0.to_string()`. Those two \
                 changes are ATOMIC: a `BTreeSet<PeerId>` would hold two entries that compare \
                 equal, and `NeighborSets` would disagree with its own metadata map.\n\
                 \n\
                 Fix: order `PeerId` by `Did::identifier_bytes()` in the same patch that \
                 changes `Did` equality, with a total tie-break for any identifier that does \
                 not decode. Do NOT relax this assertion.\n\
                 \n\
                 If `Ord` says equal and `Eq` does not, someone made `PeerId::Ord` \
                 principal-aware BEFORE `Did::Eq` — that is the inverse violation and it \
                 silently drops peers from ordered collections today. Revert it."
            );

            // Counted on properties that do not move with the regime: two *different
            // spellings* of *one principal*. Counting "currently unequal but one principal
            // after I7" instead would reach zero once `Did::Eq` adopts I7 — the non-vacuity
            // guard would then fire on the correct patch and tell its implementer, wrongly,
            // that the coherence assertion above proved nothing.
            if a.0.as_str() != b.0.as_str() && same_principal_after_i7(a, b) {
                transition_relevant_pairs += 1;
            }
        }
    }

    // Non-vacuity: the assertion above is only load-bearing if the population contains more
    // than one spelling of some principal. If a future edit removes the alias generation, this
    // fails rather than letting the suite pass on trivially-distinct pairs.
    assert!(
        transition_relevant_pairs > 0,
        "no two members of the population are different spellings of one principal, so the \
         coherence assertion above was only exercised on pairs no regime change can move"
    );
}

// ---------------------------------------------------------------------------
// 3. The paired-container law, on bare collections.
// ---------------------------------------------------------------------------

/// Ordered and hashed containers must partition the same peers the same way.
///
/// This is the invariant `NeighborSets` depends on, isolated from it. It is stated without
/// reference to which regime is current, so it holds before and after I7.
fn assert_containers_partition_identically(population: &[(&str, PeerId)]) {
    for (label_a, a) in population {
        for (label_b, b) in population {
            let mut ordered = BTreeSet::new();
            ordered.insert(a.clone());
            ordered.insert(b.clone());

            let mut hashed = HashSet::new();
            hashed.insert(a.clone());
            hashed.insert(b.clone());

            assert_eq!(
                ordered.len(),
                hashed.len(),
                "a BTreeSet and a HashSet disagree about whether {label_a} and {label_b} are \
                 one peer or two. Ordered containers key on `PeerId::Ord`, hashed ones on the \
                 derived `Hash`/`Eq`; when `Did` equality moves to identifier bytes (I7, \
                 #2627) the derived half follows and `Ord` does not unless it is changed in \
                 the same patch."
            );

            let mut ordered_map = BTreeMap::new();
            ordered_map.insert(a.clone(), "a");
            ordered_map.insert(b.clone(), "b");

            let mut hashed_map = HashMap::new();
            hashed_map.insert(a.clone(), "a");
            hashed_map.insert(b.clone(), "b");

            assert_eq!(
                ordered_map.len(),
                hashed_map.len(),
                "a BTreeMap and a HashMap disagree about {label_a} vs {label_b} — same cause \
                 as the set case above"
            );
        }
    }
}

#[test]
fn ordered_and_hashed_containers_partition_peers_identically() {
    let mut population = one_principal_every_spelling();
    population.push(("second-principal", a_peer()));
    population.push(("third-principal", a_peer()));
    assert_containers_partition_identically(&population);
}

// ---------------------------------------------------------------------------
// 4. The real structure: NeighborSets holds one identity under both regimes.
// ---------------------------------------------------------------------------

/// The metadata key set, observed through the public API.
///
/// `metadata` is private, but `peers_needing_rtt_refresh` enumerates it: `is_rtt_stale()`
/// returns true when no RTT was ever recorded, so with none recorded this is every metadata
/// row. No sleeping and no timing dependency.
fn metadata_rows(sets: &NeighborSets) -> Vec<PeerId> {
    sets.peers_needing_rtt_refresh()
}

fn ordered_rows(sets: &NeighborSets) -> Vec<PeerId> {
    let mut all: Vec<PeerId> = Vec::new();
    all.extend(sets.local_cluster.iter().cloned());
    all.extend(sets.regional.iter().cloned());
    all.extend(sets.backbone.iter().cloned());
    all.extend(sets.trusted.iter().cloned());
    all
}

#[test]
fn neighbour_sets_ordered_and_hashed_views_report_the_same_peers() {
    let mut sets = NeighborSets::new(topology("eu", "c1"));
    let limits = NeighborLimitsConfig::default();

    for (_, peer) in one_principal_every_spelling() {
        sets.add_neighbor(peer, topology("eu", "c1"), None, 0.1, &limits);
    }

    let ordered = ordered_rows(&sets);
    let metadata = metadata_rows(&sets);

    assert_eq!(
        ordered.len(),
        metadata.len(),
        "`NeighborSets` ordered sets hold {} peers while its metadata map holds {}. The four \
         `BTreeSet<PeerId>` fields key on `PeerId::Ord` and `metadata: HashMap<PeerId, _>` \
         keys on the derived `Hash`/`Eq`. After I7 (#2627) the map follows `Did` into \
         principal identity and the sets do not, unless `PeerId::Ord` changes in the same \
         patch — at which point `enforce_limit`, which counts a class with `set_ref.len()` \
         (`topology.rs`), and `remove_neighbor` disagree with the metadata map.",
        ordered.len(),
        metadata.len()
    );
    assert_eq!(
        sets.total_count(),
        metadata.len(),
        "`total_count()` sums the four ordered sets, so it must agree with the metadata map \
         for the same reason"
    );
}

#[test]
fn no_neighbour_is_left_without_metadata_and_no_metadata_is_orphaned() {
    let mut sets = NeighborSets::new(topology("eu", "c1"));
    let limits = NeighborLimitsConfig::default();
    let spellings = one_principal_every_spelling();

    for (_, peer) in &spellings {
        sets.add_neighbor(peer.clone(), topology("eu", "c1"), None, 0.1, &limits);
    }

    // A second, unrelated principal that survives the removal below. Without it, a correct I7
    // patch would collapse the 23 spellings to one peer, the removal would empty the struct,
    // and the final assertion would compare two empty sets — green, and proving nothing.
    let bystander = a_peer();
    sets.add_neighbor(bystander.clone(), topology("eu", "c1"), None, 0.1, &limits);

    // Remove under one spelling. `remove_neighbor` deletes from the ordered sets by `Ord` and
    // from `metadata` by `Hash`/`Eq`; if those stop agreeing, one of the two deletions misses.
    sets.remove_neighbor(&spellings[0].1);

    let ordered: HashSet<PeerId> = ordered_rows(&sets).into_iter().collect();
    let metadata: HashSet<PeerId> = metadata_rows(&sets).into_iter().collect();

    assert_eq!(
        ordered, metadata,
        "after a removal the ordered sets and the metadata map name different peers. A peer in \
         an ordered set with no metadata row is scored `unwrap_or(0.0)` by `enforce_limit` and \
         becomes a permanent eviction candidate; a metadata row with no ordered entry is \
         unreachable state that nothing prunes. Both appear at I7 (#2627) if `PeerId::Ord` is \
         not moved to identifier bytes alongside `Did` equality."
    );
}

#[test]
fn a_peer_lands_in_at_most_one_neighbour_class() {
    // `add_neighbor` starts by calling `remove_neighbor`, precisely so re-adding a peer under
    // new topology moves it rather than duplicating it. That step removes from the ordered
    // sets by `Ord` and from `metadata` by `Hash`/`Eq`. When those disagree, the ordered
    // removal misses and the peer ends up in two classes at once.
    let mut sets = NeighborSets::new(topology("eu", "c1"));
    let limits = NeighborLimitsConfig::default();
    let spellings = one_principal_every_spelling();

    // Same principal, first as a local-cluster peer, then re-added as a backbone peer.
    sets.add_neighbor(
        spellings[0].1.clone(),
        topology("eu", "c1"),
        None,
        0.1,
        &limits,
    );
    sets.add_neighbor(
        spellings[1].1.clone(),
        topology("us", "c9"),
        None,
        0.1,
        &limits,
    );

    // Regime-agnostic control: the second `add_neighbor` used a different region, so it must
    // have placed its peer in `backbone` whether or not that peer was recognised as the same
    // one already in `local_cluster`. Asserting the *number* of occupied classes instead would
    // be regime-dependent — two today, one after a correct I7 patch.
    assert!(
        !sets.backbone.is_empty(),
        "CONTROL: the re-add under a different region must have exercised the promotion path, \
         or the assertion below proves nothing about it"
    );

    // The invariant, phrased so it is both regime-agnostic and able to fail.
    //
    // Two phrasings do not work. "One *principal* occupies at most one class" is a post-I7
    // invariant: today two spellings are two peers, so one principal legitimately sits in
    // `local_cluster` and `backbone` at once, and asserting otherwise is red on `main`. "One
    // `PeerId` occupies at most one class", tested with `BTreeSet::contains`, can never fail —
    // `contains` dispatches on `Ord`, and `remove_neighbor` and the insert use `Ord` too, so it
    // answers "one" in every regime including the broken one.
    //
    // What works is comparing entries *across* classes with `Eq`. Today the two spellings are
    // `!=`, so no cross-class pair is equal. After a correct I7 patch there is only one entry,
    // so there is no cross-class pair at all. In the half-completed state the ordered removal
    // misses — `Ord` still separates the spellings — while `Eq` has already merged them, so two
    // entries in two classes compare equal. That is precisely "one peer in two classes".
    let classes: [(&str, &BTreeSet<PeerId>); 4] = [
        ("local_cluster", &sets.local_cluster),
        ("regional", &sets.regional),
        ("backbone", &sets.backbone),
        ("trusted", &sets.trusted),
    ];
    for (name_a, set_a) in &classes {
        for (name_b, set_b) in &classes {
            if name_a == name_b {
                continue;
            }
            for peer_a in set_a.iter() {
                for peer_b in set_b.iter() {
                    assert_ne!(
                        peer_a, peer_b,
                        "the same peer is in `{name_a}` and `{name_b}` at once. \
                         `add_neighbor` removes from every class before inserting, so this can \
                         only happen when the ordered removal fails to match a peer the hashed \
                         side already considers the same — `Did` equality has adopted I7 \
                         (#2627) while `PeerId::Ord` still compares `self.0.to_string()`. Move \
                         `PeerId::Ord` to `Did::identifier_bytes()` in the same patch."
                    );
                }
            }
        }
    }

    // The same divergence seen from the other side: the ordered rows outnumber the metadata
    // rows, because the missed ordered removal left an entry the hashed side had merged away.
    assert_eq!(
        ordered_rows(&sets).len(),
        metadata_rows(&sets).len(),
        "the promotion path left {} ordered entries against {} metadata rows",
        ordered_rows(&sets).len(),
        metadata_rows(&sets).len()
    );
}

// ---------------------------------------------------------------------------
// 5. Controls.
// ---------------------------------------------------------------------------

#[test]
fn distinct_principals_stay_distinct_under_both_notions() {
    let a = a_peer();
    let b = a_peer();

    assert_ne!(
        principal_of(&a.0),
        principal_of(&b.0),
        "CONTROL: two generated keys are two principals"
    );
    assert_ne!(a, b, "different identifier bytes must never be `Eq`");
    assert_ne!(
        a.cmp(&b),
        std::cmp::Ordering::Equal,
        "different identifier bytes must never compare `Equal`, in either regime"
    );
    assert!(
        !same_principal_after_i7(&a, &b),
        "and I7 must not merge them either"
    );

    let ordered: BTreeSet<PeerId> = [a.clone(), b.clone()].into_iter().collect();
    let hashed: HashSet<PeerId> = [a, b].into_iter().collect();
    assert_eq!(ordered.len(), 2);
    assert_eq!(hashed.len(), 2);
}

#[test]
fn an_anchor_derived_principal_is_a_first_class_peer() {
    // `Did::from_anchor_id` names a principal whose 32 bytes need not decompress to an Ed25519
    // point, and roughly half do not. Such a DID is accepted by the type and can key a peer
    // container, so any I7 projection must be defined for it.
    let anchor = PeerId(Did::from_anchor_id(&[2u8; 32]));

    assert!(
        anchor.0.to_verifying_key().is_err(),
        "CONTROL: this anchor id is deliberately not a valid Ed25519 point"
    );
    assert_eq!(
        principal_of(&anchor.0),
        [2u8; 32],
        "`identifier_bytes` resolves it, which is why the projection keys on that and not on a \
         verifying key"
    );

    // This is why `SenderPrincipal` must not stand in for general principal identity: it is
    // built from a `VerifyingKey` and fails closed here by design (`replay_guard.rs`). Using it
    // as the peer-identity source would silently drop this population.
    assert!(
        icn_net::replay_guard::SenderPrincipal::from_did(&anchor.0).is_err(),
        "CONTROL: `SenderPrincipal` is narrower than a `Did` principal — it must not be used as \
         the identity source for peer containers"
    );

    let other = PeerId(Did::from_anchor_id(&[3u8; 32]));
    assert_ne!(anchor, other);
    assert_ne!(anchor.cmp(&other), std::cmp::Ordering::Equal);

    let population = [
        ("anchor-2", anchor),
        ("anchor-3", other),
        ("keypair", a_peer()),
    ];
    assert_containers_partition_identically(&population);
}

#[test]
fn the_alternate_spellings_are_exactly_the_class_the_parser_accepts() {
    // Non-vacuity for the list itself: if `Did::from_str` tightens, `alias_in` panics rather
    // than letting this suite quietly cover fewer spellings.
    let canonical = KeyPair::generate().unwrap().did().clone();
    let mut seen = HashSet::new();
    for (label, base) in ALTERNATE_SPELLINGS {
        let alias = alias_in(base, label, &canonical);
        assert!(
            seen.insert(alias.as_str().to_string()),
            "{label} produced a spelling already generated by another base"
        );
    }
    assert_eq!(seen.len(), 22, "all 22 alternate spellings are distinct");
    assert!(
        !seen.contains(canonical.as_str()),
        "CONTROL: none of them is the canonical base58btc spelling"
    );
}
