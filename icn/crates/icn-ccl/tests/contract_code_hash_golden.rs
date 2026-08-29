//! Golden and discrimination evidence for the contract code hash (Rule A).
//!
//! # What this file is for
//!
//! `icn_ccl::compute_contract_code_hash` replaced ten independently written
//! copies of one rule — three production sites (`ContractActor`, and two
//! `icnctl` signing paths) and seven test/protocol twins. That consolidation is
//! only safe if it changed **ownership** and not **behaviour**, because the
//! resulting hash is signed, gossiped, and accepted verbatim from remote peers.
//!
//! Every `GOLDEN_*` constant below was produced by running the **unmodified**
//! `ContractActor::compute_code_hash` at `main`
//! `b0c1130a102c5f15189358b01c9f9193932fc8ba`, before any duplicate was
//! removed. None of them was derived by calling the canonical implementation.
//! They are historical facts about the deployed rule, not a restatement of the
//! current code.
//!
//! # What these tests are protecting
//!
//! Two of the pinned properties look like bugs, and are pinned *because* they
//! look like bugs. Someone will eventually be tempted to fix them here:
//!
//! * **Spelling sensitivity.** One principal has many accepted multibase
//!   spellings, and today each yields a different contract identity. That is
//!   the split I7 (#2627) exists to close, together with a migration for the
//!   identifiers that already exist. Closing it *here* would silently move
//!   live protocol values.
//! * **Concatenation ambiguity.** There is no domain tag, no length prefix and
//!   no separator, so `sha256("")` is a reachable contract identity.
//!
//! Neither may be repaired outside a migration. These tests fail if either is.

#![allow(unknown_lints, clippy::unwrap_used, clippy::expect_used)]

use icn_ccl::ast::Contract;
use icn_ccl::compute_contract_code_hash;
use icn_ccl::ContractDeploymentMessage;
use icn_identity::Did;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Corpus construction — deterministic, so the goldens are reproducible.
// ---------------------------------------------------------------------------

/// A DID built from a deterministic Ed25519 key, spelled base58btc.
fn key_did(seed: u8) -> Did {
    let vk = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key();
    Did::from_public_key(&vk)
}

/// The same 32 identifier bytes as [`key_did`], spelled base16-lower.
///
/// `did:icn:` identifiers are multibase, so `f` + lowercase hex is an accepted
/// spelling of exactly the principal `key_did` names. `Did::from_str` accepts
/// it and stores the spelling verbatim.
fn key_did_base16(seed: u8) -> Did {
    let raw = ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    Did::from_str(&format!("did:icn:f{}", hex::encode(raw)))
        .expect("base16 is an accepted multibase spelling")
}

fn anchor_did() -> Did {
    Did::from_anchor_id(&[9u8; 32])
}

fn hex_of(c: &Contract) -> String {
    hex::encode(compute_contract_code_hash(c).as_bytes())
}

// ---------------------------------------------------------------------------
// Pinned spellings. If these move, the goldens below describe different inputs
// and every assertion in this file becomes meaningless.
// ---------------------------------------------------------------------------

const P1_BASE58: &str = "did:icn:zGmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
const P1_BASE16: &str = "did:icn:fea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
const P2_BASE58: &str = "did:icn:z7v54NWdBtkjuAFJrLGsS2SXnuk8nKam81mZJeeYxVFi9";
const ANCHOR: &str = "did:icn:zcGfHiC6Kgg3FpFZvgwGcswsCRtp4aBP2fzuXRQPizuN";

#[test]
fn corpus_spellings_are_the_ones_the_goldens_were_taken_over() {
    assert_eq!(key_did(7).as_str(), P1_BASE58);
    assert_eq!(key_did_base16(7).as_str(), P1_BASE16);
    assert_eq!(key_did(11).as_str(), P2_BASE58);
    assert_eq!(anchor_did().as_str(), ANCHOR);
}

#[test]
fn the_two_p1_spellings_name_one_principal_but_are_textually_distinct() {
    let a = key_did(7);
    let b = key_did_base16(7);
    assert_ne!(
        a.as_str(),
        b.as_str(),
        "corpus needs two distinct spellings"
    );
    assert_eq!(
        a.identifier_bytes().unwrap(),
        b.identifier_bytes().unwrap(),
        "corpus needs those spellings to name the SAME principal"
    );
}

// ---------------------------------------------------------------------------
// Goldens — captured from the pre-consolidation production method.
// ---------------------------------------------------------------------------

const GOLDEN_EMPTY_MINIMAL: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const GOLDEN_ORDINARY_NO_PARTICIPANTS: &str =
    "948f4ce95b2f0769c9f2719563cb90fc2b3a91c4d4b27d681796cdc0ffb0b700";
const GOLDEN_ONE_PARTICIPANT: &str =
    "c9bcf62bf560dd613dc1dd875ef4d94e90e4da2f20b623b1db77412854d5b8aa";
const GOLDEN_ORDER_AB: &str = "4c8d1641394f3a85affe293c6aeb8dd2cda3aa61f0e1a5ad4802b540274cedab";
const GOLDEN_ORDER_BA: &str = "2bcf7b2cc394ed88004b516cf10803b2ebfab53bc322eb4a9163a0be387c2305";
const GOLDEN_ALIAS_BASE16: &str =
    "50a66980bbcecbd13757ce4554a5de26d3a89539d4988815184f3e62e0bdb429";
const GOLDEN_ANCHOR_ONLY: &str = "272446633bb72aae88c8f04793b1ad7672bf48814ed75f9f4483acf32b391951";
const GOLDEN_ANCHOR_PLUS_KEY: &str =
    "eb31a36862983f1c8c4204a40c743ec4eeb99d23269b96ccfa04277e2bca3031";
const GOLDEN_EMPTY_NAME_ONE_PARTICIPANT: &str =
    "0bc0d4d4b82ec59d70572a2fd686fd4f81205c500d1a112b55d4b94ebd81c9f1";
const GOLDEN_DEBUG_SENSITIVE_NAME: &str =
    "5365c9217f59f08dca267786a3cf24ecb3cd51f0f818442101f78fa14beb0aeb";
const GOLDEN_ICNCTL_DEPLOY_SHAPE: &str =
    "261ec68d37e40bf8def9d380a3dc71fd960120bbdc40d9444b368f5b5394efeb";
const GOLDEN_ICNCTL_COSIGN_SHAPE: &str =
    "f82c2283bc41f24017358aa029b7c1d42d37e3baaa35b0925f4f809e2bff3a45";
const GOLDEN_INTEGRATION_THREE_NODES: &str =
    "503ad89cbd35f586dcfa19c31a062cd92f2b48d3a52241e60ca549253cb6a97e";

/// A name whose `Debug` form differs from its `Display` form: it contains a
/// quote, a backslash and a newline, all of which `Debug` would escape.
fn debug_sensitive_name() -> String {
    "Te\"st\\Contract\n".to_string()
}

fn ordinary() -> Contract {
    Contract::new("TestContract".to_string())
}

#[test]
fn canonical_rule_reproduces_every_pre_consolidation_golden() {
    let p1 = key_did(7);
    let p2 = key_did(11);
    let anchor = anchor_did();

    let cases: Vec<(&str, Contract, &str)> = vec![
        (
            "empty_minimal",
            Contract::new(String::new()),
            GOLDEN_EMPTY_MINIMAL,
        ),
        (
            "ordinary_no_participants",
            ordinary(),
            GOLDEN_ORDINARY_NO_PARTICIPANTS,
        ),
        (
            "one_participant",
            ordinary().add_participant(p1.clone()),
            GOLDEN_ONE_PARTICIPANT,
        ),
        (
            "two_distinct_order_AB",
            ordinary()
                .add_participant(p1.clone())
                .add_participant(p2.clone()),
            GOLDEN_ORDER_AB,
        ),
        (
            "two_distinct_order_BA",
            ordinary()
                .add_participant(p2.clone())
                .add_participant(p1.clone()),
            GOLDEN_ORDER_BA,
        ),
        (
            "alias_base16_same_principal",
            ordinary().add_participant(key_did_base16(7)),
            GOLDEN_ALIAS_BASE16,
        ),
        (
            "anchor_did_only",
            ordinary().add_participant(anchor.clone()),
            GOLDEN_ANCHOR_ONLY,
        ),
        (
            "anchor_plus_key",
            ordinary()
                .add_participant(anchor.clone())
                .add_participant(p1.clone()),
            GOLDEN_ANCHOR_PLUS_KEY,
        ),
        (
            "empty_name_one_participant",
            Contract::new(String::new()).add_participant(p1.clone()),
            GOLDEN_EMPTY_NAME_ONE_PARTICIPANT,
        ),
        (
            "debug_sensitive_name",
            Contract::new(debug_sensitive_name()).add_participant(p1.clone()),
            GOLDEN_DEBUG_SENSITIVE_NAME,
        ),
        (
            "icnctl_deploy_shape",
            Contract::new("DeployedContract".to_string())
                .add_participant(p1.clone())
                .add_participant(p2.clone()),
            GOLDEN_ICNCTL_DEPLOY_SHAPE,
        ),
        (
            "icnctl_cosign_shape",
            Contract::new("CoSigned".to_string())
                .add_participant(p1.clone())
                .add_participant(p2.clone())
                .add_participant(anchor.clone()),
            GOLDEN_ICNCTL_COSIGN_SHAPE,
        ),
        (
            "integration_three_nodes",
            ordinary()
                .add_participant(key_did(21))
                .add_participant(key_did(22))
                .add_participant(key_did(23)),
            GOLDEN_INTEGRATION_THREE_NODES,
        ),
    ];

    for (name, contract, golden) in &cases {
        assert_eq!(
            &hex_of(contract),
            golden,
            "case `{name}`: consolidation changed the contract code hash. \
             These bytes are signed, gossiped and accepted from remote peers — \
             a change here moves live protocol identifiers."
        );
    }
}

// ---------------------------------------------------------------------------
// Independent encoding oracle.
//
// The goldens prove the output did not move. This proves *why*: the feed is a
// bare concatenation of the raw name bytes and the `Debug` form of each
// participant, in `Vec` order, with nothing between them. Built here from
// first principles rather than by calling the canonical implementation.
// ---------------------------------------------------------------------------

fn oracle(name: &str, participants: &[&Did]) -> String {
    let mut feed: Vec<u8> = Vec::new();
    feed.extend_from_slice(name.as_bytes());
    for p in participants {
        feed.extend_from_slice(format!("{p:?}").as_bytes());
    }
    hex::encode(Sha256::digest(&feed))
}

#[test]
fn encoding_is_a_bare_concatenation_with_no_separator_or_prefix() {
    let p1 = key_did(7);
    let p2 = key_did(11);

    assert_eq!(
        oracle("TestContract", &[&p1, &p2]),
        GOLDEN_ORDER_AB,
        "the historical feed is name-bytes then Debug-of-each-participant, unseparated"
    );
    assert_eq!(
        oracle("", &[]),
        GOLDEN_EMPTY_MINIMAL,
        "an empty contract hashes to sha256 of the empty string"
    );
}

#[test]
fn empty_contract_identity_is_literally_sha256_of_nothing() {
    // Pinning the ambiguity, not endorsing it: with no domain tag and no length
    // prefix, this value is reachable. Adding either would move every deployed
    // contract identifier, so it belongs to the I7 migration (#2627), not here.
    assert_eq!(
        hex_of(&Contract::new(String::new())),
        hex::encode(Sha256::digest(b"")),
    );
}

// ---------------------------------------------------------------------------
// Discrimination — each test builds the mutant a careless change would produce
// and proves the pinned value rejects it.
// ---------------------------------------------------------------------------

#[test]
fn discriminates_a_change_of_hash_algorithm() {
    let p1 = key_did(7);
    let mutant = hex::encode(blake3::hash(b"TestContract").as_bytes());
    assert_ne!(mutant, GOLDEN_ORDINARY_NO_PARTICIPANTS);

    let mut feed = Vec::new();
    feed.extend_from_slice(b"TestContract");
    feed.extend_from_slice(format!("{p1:?}").as_bytes());
    let sha512_truncated = hex::encode(&sha2::Sha512::digest(&feed)[..32]);
    assert_ne!(
        sha512_truncated, GOLDEN_ONE_PARTICIPANT,
        "swapping the digest must not go unnoticed"
    );
}

#[test]
fn discriminates_debug_replaced_by_display() {
    let p1 = key_did(7);
    let mut feed = Vec::new();
    feed.extend_from_slice(b"TestContract");
    feed.extend_from_slice(format!("{p1}").as_bytes()); // Display, not Debug
    assert_ne!(
        hex::encode(Sha256::digest(&feed)),
        GOLDEN_ONE_PARTICIPANT,
        "Display drops the `Did(` wrapper and the quotes that Debug contributes"
    );
}

#[test]
fn discriminates_debug_replaced_by_as_str() {
    let p1 = key_did(7);
    let mut feed = Vec::new();
    feed.extend_from_slice(b"TestContract");
    feed.extend_from_slice(p1.as_str().as_bytes());
    assert_ne!(
        hex::encode(Sha256::digest(&feed)),
        GOLDEN_ONE_PARTICIPANT,
        "as_str() is the bare spelling; Debug adds `Did(\"` and `\")`"
    );
}

#[test]
fn discriminates_the_name_feed_being_debug_quoted_instead_of_raw() {
    // The name is fed raw. A name containing a quote, a backslash and a newline
    // is where raw and Debug diverge most visibly.
    let name = debug_sensitive_name();
    let p1 = key_did(7);

    let mut mutant = Vec::new();
    mutant.extend_from_slice(format!("{name:?}").as_bytes()); // Debug-quoted name
    mutant.extend_from_slice(format!("{p1:?}").as_bytes());
    assert_ne!(
        hex::encode(Sha256::digest(&mutant)),
        GOLDEN_DEBUG_SENSITIVE_NAME
    );

    // and the pinned value really is the raw-name feed
    assert_eq!(oracle(&name, &[&p1]), GOLDEN_DEBUG_SENSITIVE_NAME);
}

#[test]
fn discriminates_the_name_component_being_dropped() {
    let p1 = key_did(7);
    assert_ne!(
        oracle("", &[&p1]),
        GOLDEN_ONE_PARTICIPANT,
        "dropping the name must change the identity"
    );
    assert_eq!(oracle("", &[&p1]), GOLDEN_EMPTY_NAME_ONE_PARTICIPANT);
}

#[test]
fn discriminates_an_inserted_separator_or_length_prefix() {
    let p1 = key_did(7);
    let p2 = key_did(11);

    let mut with_separator = Vec::new();
    with_separator.extend_from_slice(b"TestContract");
    for p in [&p1, &p2] {
        with_separator.push(0x1f);
        with_separator.extend_from_slice(format!("{p:?}").as_bytes());
    }
    assert_ne!(
        hex::encode(Sha256::digest(&with_separator)),
        GOLDEN_ORDER_AB
    );

    let mut with_length_prefix = Vec::new();
    with_length_prefix.extend_from_slice(&(12u32).to_le_bytes());
    with_length_prefix.extend_from_slice(b"TestContract");
    for p in [&p1, &p2] {
        let d = format!("{p:?}");
        with_length_prefix.extend_from_slice(&(d.len() as u32).to_le_bytes());
        with_length_prefix.extend_from_slice(d.as_bytes());
    }
    assert_ne!(
        hex::encode(Sha256::digest(&with_length_prefix)),
        GOLDEN_ORDER_AB
    );
}

#[test]
fn discriminates_participants_being_sorted() {
    // Sorting is the most natural "determinism improvement" someone could make.
    // It would collapse ORDER_AB and ORDER_BA into one identity.
    assert_ne!(
        GOLDEN_ORDER_AB, GOLDEN_ORDER_BA,
        "participant order is part of contract identity today"
    );

    let p1 = key_did(7);
    let p2 = key_did(11);

    // A sorting mutant maps BOTH declaration orders onto one feed...
    let mut from_ab = [&p1, &p2];
    from_ab.sort_by_key(|d| d.as_str().to_string());
    let mut from_ba = [&p2, &p1];
    from_ba.sort_by_key(|d| d.as_str().to_string());
    let sorted_hex = oracle("TestContract", &from_ab);
    assert_eq!(
        sorted_hex,
        oracle("TestContract", &from_ba),
        "a sorting mutant collapses the two declaration orders"
    );

    // ...so it cannot reproduce both goldens. Here `z7v5…` sorts before
    // `zGma…`, so sorting yields the BA feed and it is the AB golden that
    // catches the mutation.
    assert_ne!(
        sorted_hex, GOLDEN_ORDER_AB,
        "sorting would give the AB contract the BA identity"
    );
}

#[test]
fn discriminates_aliases_being_principalized_early() {
    // THIS TEST MUST KEEP FAILING TO BE EQUAL until I7 lands with a migration.
    // Two accepted spellings of one principal produce two contract identities
    // today. Making them agree here — without moving the identifiers that
    // already exist — would silently re-key live deployments.
    assert_ne!(
        GOLDEN_ONE_PARTICIPANT, GOLDEN_ALIAS_BASE16,
        "contract identity is still DID-spelling-sensitive in this tranche"
    );

    let canonical_spelling = key_did(7);
    let alias_spelling = key_did_base16(7);
    assert_ne!(
        hex_of(&ordinary().add_participant(canonical_spelling)),
        hex_of(&ordinary().add_participant(alias_spelling)),
    );

    // What a premature principalization would look like: feeding decoded bytes.
    let bytes = key_did(7).identifier_bytes().unwrap();
    let mut principalized = Vec::new();
    principalized.extend_from_slice(b"TestContract");
    principalized.extend_from_slice(&bytes);
    let principalized = hex::encode(Sha256::digest(&principalized));
    assert_ne!(principalized, GOLDEN_ONE_PARTICIPANT);
    assert_ne!(principalized, GOLDEN_ALIAS_BASE16);
}

#[test]
fn discriminates_a_change_to_the_bytes_that_get_signed() {
    // The code hash reaches the wire through compute_signing_bytes. Pin that
    // binding too, so a change to either half is caught.
    let contract = ordinary()
        .add_participant(key_did(7))
        .add_participant(key_did(11));
    let code_hash = compute_contract_code_hash(&contract);
    let installed_at: u64 = 1_700_000_000;

    let signing = ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);

    let mut expected = Vec::new();
    expected.extend_from_slice(&hex::decode(GOLDEN_ORDER_AB).unwrap());
    expected.extend_from_slice(&installed_at.to_le_bytes());
    assert_eq!(
        hex::encode(&signing),
        hex::encode(Sha256::digest(&expected)),
        "signing bytes are sha256(code_hash || installed_at_le)"
    );

    // A one-second difference must produce different signing bytes.
    let other = ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at + 1);
    assert_ne!(signing, other);
}

#[test]
fn fields_outside_name_and_participants_do_not_affect_identity() {
    // Currency, state vars, rules and triggers are NOT part of the hash today.
    // Pinned because folding them in would move every deployed identifier.
    let base = ordinary()
        .add_participant(key_did(7))
        .add_participant(key_did(11));
    let embellished = ordinary()
        .add_participant(key_did(7))
        .add_participant(key_did(11))
        .with_currency("USD".to_string());

    assert_eq!(hex_of(&base), GOLDEN_ORDER_AB);
    assert_eq!(
        hex_of(&embellished),
        GOLDEN_ORDER_AB,
        "currency is outside the code hash today; changing that is a migration"
    );
}

// ---------------------------------------------------------------------------
// Anti-drift: the rule must exist exactly once in the workspace.
//
// The consolidation's whole purpose is that a future I7 migration edits one
// place. The second `icnctl` signing site had no coupling comment at all and
// was missed by every search that looked for the function name rather than the
// algorithm — so this guard searches for the algorithm.
// ---------------------------------------------------------------------------

#[test]
fn the_rule_is_implemented_exactly_once_in_the_workspace() {
    let rust_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/icn-ccl sits two levels below the cargo root")
        .to_path_buf();

    let canonical = rust_root.join("crates/icn-ccl/src/code_hash.rs");
    assert!(
        canonical.is_file(),
        "canonical module not found at {canonical:?}"
    );

    let mut offenders: Vec<String> = Vec::new();
    let mut stack = vec![rust_root.join("crates"), rust_root.join("bins")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") && path != canonical {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // The algorithm's fingerprint: a participant fed through Debug
                // into a hasher. Matching on behaviour, not on a name.
                if text.contains("hasher.update(format!(\"{participant:?}\")") {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the contract code-hash rule was re-implemented outside \
         icn-ccl/src/code_hash.rs. Call `icn_ccl::compute_contract_code_hash` \
         instead — this rule is signed, gossiped and accepted from remote \
         peers, and a copy that drifts breaks verification for every peer. \
         Offending files: {offenders:?}"
    );
}
