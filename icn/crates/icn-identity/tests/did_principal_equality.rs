//! #2627 / I7 — `Did` equality and hashing name the principal, not the spelling.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! # What this file pins
//!
//! A `did:icn:` identifier is a multibase encoding of 32 bytes, and multibase has many
//! accepted spellings of the same bytes. Before I7, `Did` derived `PartialEq`/`Hash` over
//! its inner `String`, so one cryptographic principal was up to 23 distinct map keys. I7
//! makes equality and hashing key on [`Did::identifier_bytes`] instead.
//!
//! Two things must both hold, and they pull in opposite directions:
//!
//! * **comparison** collapses the spellings of one principal to one identity;
//! * **representation** — `Debug`, `Display`, `as_str`, `Serialize` — keeps every spelling
//!   exactly as it was, because every durable key, wire byte and signing input is built
//!   from those. That is what makes I7 rollback-safe: a binary reverted to spelling
//!   equality reads the same stored rows
//!   (`docs/architecture/n2-a0-stored-key-inventory.md` §12.1 item 5).
//!
//! The representation half is asserted here as explicitly as the comparison half, because a
//! patch that "fixed" equality by canonicalizing the string would pass every equality test
//! in this file and silently re-key live deployments.
//!
//! # Non-vacuity
//!
//! Every test that claims two spellings are one principal first proves they are **different
//! strings** that decode to the **same** 32 bytes. A green result therefore cannot come from
//! the alias being unrepresentable or from the two values being literally identical.

use icn_identity::{Did, KeyPair};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Every multibase base `Did::from_str` accepts, minus `Identity` — which `multibase::encode`
/// cannot apply to arbitrary key bytes at all. Same corpus the #2640 replay suite and the
/// `PeerId` tripwire drive, so the three agree on what "an accepted spelling" means.
const ALTERNATE_SPELLINGS: &[(&str, multibase::Base)] = &[
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

fn a_principal() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// Re-spell one principal under one base, proving the alias is a different string naming the
/// same identifier before returning it.
///
/// A spelling that stops parsing is a failure, not a skip: letting a rejection quietly reduce
/// coverage is how this kind of suite rots.
fn alias_in(base: multibase::Base, label: &str, canonical: &Did) -> Did {
    let bytes = canonical.identifier_bytes().unwrap();
    let alias = Did::from_str(&format!("did:icn:{}", multibase::encode(base, bytes)))
        .unwrap_or_else(|e| {
            panic!(
                "the {label} spelling is accepted by `Did::from_str` under current policy and \
                 this suite's coverage depends on it; it was rejected: {e}. I7 changes \
                 comparison, not acceptance — if acceptance has been tightened, that is a \
                 different change and belongs asserted explicitly."
            )
        });
    assert_ne!(
        alias.as_str(),
        canonical.as_str(),
        "CONTROL: the {label} alias must be a different *string*, or it proves nothing"
    );
    assert_eq!(
        alias.identifier_bytes().unwrap(),
        bytes,
        "CONTROL: the {label} alias must name the *same principal*, or it is a different DID"
    );
    alias
}

/// One principal, spelled every accepted way: canonical first, then all 22 aliases.
fn one_principal_every_spelling() -> Vec<(&'static str, Did)> {
    let canonical = a_principal();
    let mut out = vec![("base58btc-canonical", canonical.clone())];
    out.extend(
        ALTERNATE_SPELLINGS
            .iter()
            .map(|(label, base)| (*label, alias_in(*base, label, &canonical))),
    );
    out
}

// ---------------------------------------------------------------------------
// 1. Two spellings of one principal are one identity.
// ---------------------------------------------------------------------------

#[test]
fn two_spellings_of_one_principal_compare_equal() {
    // Reflexivity first: every spelling equals its own clone.
    for (label, alias) in one_principal_every_spelling().into_iter().skip(1) {
        let same = alias.clone();
        assert_eq!(alias, same, "{label}: a value must equal itself");
    }

    let canonical = a_principal();
    for (label, base) in ALTERNATE_SPELLINGS {
        let alias = alias_in(*base, label, &canonical);
        assert_eq!(
            canonical, alias,
            "{label} spells the same 32 identifier bytes as the canonical form, so it names \
             the same principal and must compare equal (#2627 / I7)"
        );
        assert_eq!(alias, canonical, "{label}: equality must be symmetric");
    }
}

#[test]
fn two_spellings_of_one_principal_hash_identically() {
    let canonical = a_principal();
    for (label, base) in ALTERNATE_SPELLINGS {
        let alias = alias_in(*base, label, &canonical);
        assert_eq!(
            hash_of(&canonical),
            hash_of(&alias),
            "{label}: equal values must hash equally, or `HashMap<Did, _>` is unsound"
        );
    }
}

#[test]
fn every_spelling_of_one_principal_is_one_hash_set_member() {
    let spellings = one_principal_every_spelling();
    assert_eq!(spellings.len(), 23, "canonical plus 22 aliases");

    let set: HashSet<Did> = spellings.iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(
        set.len(),
        1,
        "23 accepted spellings of one principal must collapse to one `HashSet` member, got {}",
        set.len()
    );
}

#[test]
fn every_spelling_of_one_principal_is_one_map_identity() {
    let spellings = one_principal_every_spelling();

    // Insert under each spelling in turn; each must land on the same entry.
    let mut map: HashMap<Did, u32> = HashMap::new();
    for (_, did) in &spellings {
        *map.entry(did.clone()).or_insert(0) += 1;
    }

    assert_eq!(
        map.len(),
        1,
        "23 spellings of one principal must be one key, got {}",
        map.len()
    );
    assert_eq!(
        map.values().next(),
        Some(&23),
        "all 23 insertions must have accumulated on that one key"
    );

    // And a lookup under any spelling finds the entry stored under any other.
    for (label, did) in &spellings {
        assert!(
            map.contains_key(did),
            "{label} must find the entry stored under the canonical spelling"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Two distinct principals stay two.
// ---------------------------------------------------------------------------

#[test]
fn two_distinct_principals_stay_unequal_and_stay_two_keys() {
    let a = a_principal();
    let b = a_principal();
    assert_ne!(
        a.identifier_bytes().unwrap(),
        b.identifier_bytes().unwrap(),
        "CONTROL: two generated keypairs must actually differ"
    );

    assert_ne!(a, b, "distinct principals must not be merged");

    let set: HashSet<Did> = [a.clone(), b.clone()].into_iter().collect();
    assert_eq!(set.len(), 2, "two principals are two hash keys");

    // Over-collapse guard across the whole spelling class: no spelling of `a` may ever
    // equal any spelling of `b`.
    for (la, base_a) in ALTERNATE_SPELLINGS {
        let sa = alias_in(*base_a, la, &a);
        for (lb, base_b) in ALTERNATE_SPELLINGS {
            let sb = alias_in(*base_b, lb, &b);
            assert_ne!(
                sa, sb,
                "{la} of one principal must never equal {lb} of another"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. The non-decoding population stays discriminated.
// ---------------------------------------------------------------------------

#[test]
fn an_anchor_derived_principal_keys_by_its_identifier() {
    // `from_anchor_id` bypasses parsing, so its 32 bytes need not be an Ed25519 point — but
    // it still encodes exactly 32 bytes, so it takes the *decoded* arm, not the fallback.
    // This is the production constructor that could otherwise have reached the fallback.
    let anchor = Did::from_anchor_id(&[7u8; 32]);
    assert!(
        anchor.to_verifying_key().is_err(),
        "CONTROL: this anchor id is deliberately not a valid Ed25519 point"
    );
    assert_eq!(
        anchor.identifier_bytes().unwrap(),
        [7u8; 32],
        "CONTROL: the decoded arm is the one this DID takes"
    );

    assert_eq!(
        anchor,
        Did::from_anchor_id(&[7u8; 32]),
        "one anchor is one principal"
    );
    assert_ne!(
        anchor,
        Did::from_anchor_id(&[8u8; 32]),
        "two anchors are two principals"
    );
    assert_eq!(
        hash_of(&anchor),
        hash_of(&Did::from_anchor_id(&[7u8; 32])),
        "equal anchors must hash equally"
    );
}

#[test]
fn a_principal_and_an_anchor_naming_the_same_bytes_are_the_same_principal() {
    // The two construction paths must not disagree about identity: `from_anchor_id` and
    // `from_str` over the same 32 bytes name one principal.
    let key = a_principal();
    let bytes = key.identifier_bytes().unwrap();
    let anchored = Did::from_anchor_id(&bytes);

    assert_eq!(
        anchored.identifier_bytes().unwrap(),
        bytes,
        "CONTROL: both name the same identifier"
    );
    assert_eq!(
        key, anchored,
        "construction path must not change which principal a DID names"
    );
    assert_eq!(
        hash_of(&key),
        hash_of(&anchored),
        "equal values hash equally"
    );
}

// ---------------------------------------------------------------------------
// 4. Representation is untouched. This is the rollback guarantee.
// ---------------------------------------------------------------------------

#[test]
fn equality_is_principal_sensitive_but_representation_stays_spelling_sensitive() {
    let canonical = a_principal();

    for (label, base) in ALTERNATE_SPELLINGS {
        let alias = alias_in(*base, label, &canonical);

        // Equal as identities...
        assert_eq!(canonical, alias, "{label}: same principal");

        // ...and still distinguishable as representations. Every durable key, wire byte and
        // signing input in the workspace is built from one of these four, so if any of them
        // collapsed, I7 would be moving persisted bytes — which it must not.
        assert_ne!(
            canonical.as_str(),
            alias.as_str(),
            "{label}: `as_str` must still return the original spelling"
        );
        assert_ne!(
            canonical.to_string(),
            alias.to_string(),
            "{label}: `Display` must still render the original spelling"
        );
        assert_ne!(
            format!("{canonical:?}"),
            format!("{alias:?}"),
            "{label}: `Debug` must still render the original spelling — \
             `icn-ccl`'s contract code hash is SHA-256 over `Debug` of each participant"
        );
        assert_ne!(
            serde_json::to_string(&canonical).unwrap(),
            serde_json::to_string(&alias).unwrap(),
            "{label}: serde must still emit the original spelling"
        );
    }
}

#[test]
fn representation_round_trips_byte_for_byte_under_every_spelling() {
    // Pin the exact bytes rather than only their inequality: a `Did` renders as its spelling
    // and nothing else, and a serde round-trip returns the same spelling it was given.
    for (label, did) in one_principal_every_spelling() {
        let spelling = did.as_str().to_string();

        assert_eq!(
            did.to_string(),
            spelling,
            "{label}: `Display` is the spelling"
        );
        assert_eq!(
            format!("{did:?}"),
            format!("Did({spelling:?})"),
            "{label}: `Debug` is the derived newtype form over the spelling"
        );

        let json = serde_json::to_string(&did).unwrap();
        assert_eq!(
            json,
            serde_json::to_string(&spelling).unwrap(),
            "{label}: a `Did` serializes exactly as the string that spells it"
        );

        let back: Did = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.as_str(),
            spelling,
            "{label}: a serde round-trip must not canonicalize the spelling"
        );

        // `icn-encoding` (postcard) is the wire and storage codec, so pin it too.
        let wire = icn_encoding::encode(&did).unwrap();
        assert_eq!(
            wire,
            icn_encoding::encode(&spelling).unwrap(),
            "{label}: the wire encoding of a `Did` is the encoding of its spelling"
        );
        let back: Did = icn_encoding::decode(&wire).unwrap();
        assert_eq!(
            back.as_str(),
            spelling,
            "{label}: a wire round-trip must not canonicalize the spelling"
        );
    }
}

#[test]
fn acceptance_is_unchanged_by_i7() {
    // I7 changes comparison, not parsing. Every spelling that parsed before still parses,
    // and the things that were rejected still are. If a future tranche pins an encoding at
    // parse time, this test is where that shows up.
    let canonical = a_principal();
    let bytes = canonical.identifier_bytes().unwrap();
    for (label, base) in ALTERNATE_SPELLINGS {
        assert!(
            Did::from_str(&format!("did:icn:{}", multibase::encode(*base, bytes))).is_ok(),
            "{label} must still be accepted"
        );
    }

    for bad in [
        "did:key:z6MkhaXMJznR4sC15gTfA7b6jJ4i7b6jJ4i7b6jJ4i7b",
        "did:icn:",
        "did:icn:!!!not-multibase!!!",
        "not-a-did",
        "",
    ] {
        assert!(
            Did::from_str(bad).is_err(),
            "{bad:?} must still be rejected"
        );
    }
}
