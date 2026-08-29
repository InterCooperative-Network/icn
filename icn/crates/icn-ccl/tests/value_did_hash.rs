//! `Value::Did` / `Value::Set` hash compatibility across DID spellings.
//!
//! ICN accepts more than one textual encoding for the same 32-byte principal
//! identifier. `Did` equality is still spelling-sensitive today; N2-A (#2627)
//! intends to make it principal-sensitive (`IDENTITY_SEMANTICS.md` §11, I7).
//!
//! Rust's contract is one-way:
//!
//! ```text
//! a == b  =>  hash(a) == hash(b)
//! ```
//!
//! The converse is not required, so a hash may be *coarser* than equality. That
//! is what lets this land before the equality flip: two spellings of one
//! principal share a hash today while `Did` still calls them distinct — a
//! deliberate collision, which `HashSet` resolves by comparing — and the same
//! rule is already correct once they become equal.
//!
//! These tests do not change or simulate `Did` equality in production. Where a
//! test needs the post-I7 relation it builds it locally, from
//! `Did::identifier_bytes`.

#![allow(unknown_lints, clippy::unwrap_used, clippy::expect_used)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use icn_ccl::Value;
use icn_identity::{Did, KeyPair};

/// Hash one value with a fixed-seed hasher so two values can be compared.
fn hash_of(value: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// A second, equally valid textual encoding of the principal `did` names.
///
/// `did:icn:` identifiers are multibase, so the same 32 bytes have a base58btc
/// spelling and a base16 spelling. Both parse; both decode to one identifier.
fn alternate_spelling(did: &Did) -> Did {
    let bytes = did
        .identifier_bytes()
        .expect("a validated DID decodes to 32 identifier bytes");
    let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes)))
        .expect("a base16 multibase spelling of 32 bytes is a valid did:icn:");
    assert_ne!(
        did.as_str(),
        alias.as_str(),
        "the two spellings must differ, or the test proves nothing"
    );
    alias
}

fn a_principal() -> Did {
    KeyPair::generate().expect("keypair").did().clone()
}

/// Rewrite every `Did` in `value` to one chosen spelling, recursively.
///
/// This models the post-I7 world without touching production equality: after
/// I7 two values are equal exactly when their projections are equal under
/// today's derived `PartialEq`. A `Value::Set` is rebuilt, so aliases of one
/// principal collapse here exactly as they would collapse on insertion then.
fn project_to_one_spelling(value: &Value) -> Value {
    match value {
        Value::Did(did) => match did.identifier_bytes() {
            Ok(bytes) => Value::Did(
                Did::from_str(&format!("did:icn:f{}", hex::encode(bytes)))
                    .expect("32 bytes always spell a valid did:icn:"),
            ),
            Err(_) => Value::Did(did.clone()),
        },
        Value::List(items) => Value::List(items.iter().map(project_to_one_spelling).collect()),
        Value::Set(items) => Value::Set(items.iter().map(project_to_one_spelling).collect()),
        Value::Map(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), project_to_one_spelling(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn one_did_value_hashes_the_same_as_an_identical_copy() {
    let value = Value::Did(a_principal());
    assert_eq!(value, value.clone());
    assert_eq!(hash_of(&value), hash_of(&value.clone()));
}

#[test]
fn two_spellings_of_one_principal_hash_equally() {
    let canonical = a_principal();
    let alias = alternate_spelling(&canonical);

    assert_eq!(
        hash_of(&Value::Did(canonical)),
        hash_of(&Value::Did(alias)),
        "one principal must have one hash, whatever spelling names it"
    );
}

#[test]
fn two_distinct_principals_stay_distinct_in_a_hash_set() {
    let p = a_principal();
    let q = a_principal();

    let mut set = HashSet::new();
    set.insert(Value::Did(p.clone()));
    set.insert(Value::Did(q.clone()));

    assert_eq!(set.len(), 2, "two principals are two members");
    assert!(set.contains(&Value::Did(p)));
    assert!(set.contains(&Value::Did(q)));
}

#[test]
fn a_hash_map_still_resolves_each_spelling_under_the_deliberate_collision() {
    // Both spellings hash alike now but are still unequal under today's `Did`
    // equality, so the map holds two entries in one bucket. Lookup must not
    // confuse them: a colliding hash is resolved by `Eq`, not by the hash.
    let canonical = a_principal();
    let alias = alternate_spelling(&canonical);

    let mut map = HashMap::new();
    map.insert(Value::Did(canonical.clone()), "canonical");
    map.insert(Value::Did(alias.clone()), "alias");

    assert_eq!(map.get(&Value::Did(canonical)), Some(&"canonical"));
    assert_eq!(map.get(&Value::Did(alias)), Some(&"alias"));
}

/// Two principals whose canonical spellings sort in a known order.
///
/// The order matters. A base16 alias always begins `f` and a base58btc
/// canonical spelling always begins `z`, so an alias sorts before every
/// canonical spelling. Pinning `greater > lesser` canonically guarantees that
/// re-spelling the greater one *flips* the order a spelling-derived sort would
/// impose — which is what makes the set tests below fail deterministically
/// against a spelling-sorted aggregate rather than half the time.
fn two_principals_in_pinned_canonical_order() -> (Did, Did) {
    loop {
        let a = a_principal();
        let b = a_principal();
        if a.as_str() > b.as_str() {
            return (a, b);
        }
        if b.as_str() > a.as_str() {
            return (b, a);
        }
    }
}

#[test]
fn a_set_value_hashes_independently_of_how_its_members_are_spelled() {
    // The aggregate is the half that is easy to miss: principalising the
    // element hash is not enough while the order members are fed to the hasher
    // is still derived from their spelling.
    let (greater, lesser) = two_principals_in_pinned_canonical_order();
    let greater_alias = alternate_spelling(&greater);

    let spelled_one_way = Value::Set(HashSet::from([
        Value::Did(greater),
        Value::Did(lesser.clone()),
    ]));
    let spelled_the_other_way = Value::Set(HashSet::from([
        Value::Did(greater_alias),
        Value::Did(lesser),
    ]));

    assert_eq!(
        hash_of(&spelled_one_way),
        hash_of(&spelled_the_other_way),
        "a set of principals must hash by its principals, not by their spelling"
    );
}

#[test]
fn nested_aggregates_hash_independently_of_member_spelling() {
    let (greater, lesser) = two_principals_in_pinned_canonical_order();

    let nest = |did: Did| {
        Value::Map(HashMap::from([(
            "members".to_string(),
            Value::List(vec![Value::Set(HashSet::from([
                Value::Did(did),
                Value::Did(lesser.clone()),
            ]))]),
        )]))
    };

    assert_eq!(
        hash_of(&nest(greater.clone())),
        hash_of(&nest(alternate_spelling(&greater))),
        "spelling must not survive nesting inside a map, list or set"
    );
}

#[test]
fn a_principal_hashes_the_same_however_the_did_was_constructed() {
    // `from_anchor_id` builds a base58btc spelling without parsing; `from_str`
    // parses a base16 one. Same 32 identifier bytes, two construction paths,
    // two spellings — one principal, so one hash.
    let bytes = a_principal()
        .identifier_bytes()
        .expect("a validated DID decodes to 32 identifier bytes");
    let anchored = Did::from_anchor_id(&bytes);
    let parsed = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes)))
        .expect("a base16 spelling of a valid key parses");

    assert_ne!(
        anchored.as_str(),
        parsed.as_str(),
        "the two spellings must differ, or the test proves nothing"
    );
    assert_eq!(
        hash_of(&Value::Did(anchored)),
        hash_of(&Value::Did(parsed)),
        "construction path must not change which principal a value keys under"
    );
}

#[test]
fn an_anchor_derived_did_that_is_not_a_key_still_keys_by_its_identifier() {
    // `from_anchor_id` bypasses parsing, so its 32 bytes need not decompress to
    // an Ed25519 point — and such a DID has no second *parseable* spelling,
    // because `from_str` rejects a non-point. It must still key by its
    // identifier rather than dropping to the spelling fallback.
    let anchor = Did::from_anchor_id(&[2u8; 32]);

    assert!(
        anchor.to_verifying_key().is_err(),
        "control: this anchor id is deliberately not a valid Ed25519 point"
    );
    assert_eq!(
        anchor
            .identifier_bytes()
            .expect("the identifier still resolves"),
        [2u8; 32],
        "control: the decoded branch is the one this DID takes"
    );
    assert_ne!(
        hash_of(&Value::Did(anchor)),
        hash_of(&Value::Did(Did::from_anchor_id(&[3u8; 32]))),
        "two anchors are two principals"
    );
}

#[test]
fn a_did_value_does_not_hash_as_the_string_that_spells_it() {
    // Guard against over-collapsing: the fix must narrow spelling out of the
    // DID hash without merging `Value::Did` into `Value::String`.
    let did = a_principal();
    assert_ne!(
        hash_of(&Value::Did(did.clone())),
        hash_of(&Value::String(did.as_str().to_string())),
        "a DID and the string that spells it are different values"
    );
}

#[test]
fn non_did_values_keep_hashing_by_their_own_content() {
    // Determinism for the variants this change is not about.
    for (a, b) in [
        (Value::Int(7), Value::Int(7)),
        (Value::String("x".into()), Value::String("x".into())),
        (Value::Bool(true), Value::Bool(true)),
        (Value::None, Value::None),
        (
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        ),
        (
            Value::Set(HashSet::from([Value::Int(1), Value::Int(2)])),
            Value::Set(HashSet::from([Value::Int(2), Value::Int(1)])),
        ),
        (
            Value::Map(HashMap::from([("k".to_string(), Value::Int(1))])),
            Value::Map(HashMap::from([("k".to_string(), Value::Int(1))])),
        ),
    ] {
        assert_eq!(a, b, "control: these values are equal");
        assert_eq!(hash_of(&a), hash_of(&b), "equal values must hash equally");
    }

    assert_ne!(
        hash_of(&Value::Int(1)),
        hash_of(&Value::String("1".into())),
        "distinct variants must not be conflated"
    );
    assert_ne!(
        hash_of(&Value::List(vec![Value::Int(1)])),
        hash_of(&Value::Set(HashSet::from([Value::Int(1)]))),
        "a list is not a set"
    );
}

#[test]
fn the_hash_eq_contract_holds_under_simulated_principal_equality() {
    // The whole point of the tranche, stated as the law it enforces:
    //
    //     post_i7_equal(a, b)  =>  hash(a) == hash(b)
    //
    // `post_i7_equal` is built here from `identifier_bytes`; production `Did`
    // equality is untouched. Every value in the corpus is well-formed after
    // I7 — no set holds two spellings of one principal, because such a set
    // cannot be built once insertion collapses them.
    let p = a_principal();
    let q = a_principal();
    let p_alias = alternate_spelling(&p);
    let q_alias = alternate_spelling(&q);

    let corpus = vec![
        Value::Did(p.clone()),
        Value::Did(p_alias.clone()),
        Value::Did(q.clone()),
        Value::Did(q_alias.clone()),
        Value::List(vec![Value::Did(p.clone()), Value::Did(q.clone())]),
        Value::List(vec![
            Value::Did(p_alias.clone()),
            Value::Did(q_alias.clone()),
        ]),
        Value::Set(HashSet::from([
            Value::Did(p.clone()),
            Value::Did(q.clone()),
        ])),
        Value::Set(HashSet::from([
            Value::Did(p_alias.clone()),
            Value::Did(q_alias.clone()),
        ])),
        Value::Set(HashSet::from([
            Value::Did(p.clone()),
            Value::Did(q_alias.clone()),
        ])),
        Value::Map(HashMap::from([("owner".to_string(), Value::Did(p))])),
        Value::Map(HashMap::from([("owner".to_string(), Value::Did(p_alias))])),
        Value::Int(1),
        Value::String("did:icn:not-a-did".into()),
        Value::None,
    ];

    let mut proved_a_cross_spelling_pair = false;
    for a in &corpus {
        for b in &corpus {
            let post_i7_equal = project_to_one_spelling(a) == project_to_one_spelling(b);
            if !post_i7_equal {
                continue;
            }
            assert_eq!(
                hash_of(a),
                hash_of(b),
                "values equal after I7 must already hash equally:\n  {a:?}\n  {b:?}"
            );
            if a != b {
                proved_a_cross_spelling_pair = true;
            }
        }
    }

    assert!(
        proved_a_cross_spelling_pair,
        "the corpus must contain a pair that is unequal today but equal after I7, \
         or this test passes vacuously"
    );
}
