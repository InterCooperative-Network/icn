//! Adversarial fixtures for the N2-A startup gate (#2627).
//!
//! Every fixture is a real on-disk data directory with real sled databases,
//! because the gate's claims are about what a key-equality binary finds when
//! it opens a store — a simulated store would only restate the test.
//!
//! The controls matter as much as the refusals: each refusal has a sibling
//! that differs in exactly one fact and is clear, so a test cannot pass by
//! refusing everything.

// Test-only: assertions and fixture setup panic on failure by design.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use icn_identity::Did;
use icn_store::n2a_startup_gate::{
    enforce, receipt_path, Blocker, GateReceipt, GateRefusal, Verdict,
    PRINCIPAL_IDENTITY_GENERATION, RECEIPT_SCHEMA,
};
use icn_store::{SledStore, Store};

/// A real Ed25519 verifying key derived from a seed, so every spelling of it
/// parses as a `Did` and the fixtures exercise the same values production
/// would.
fn principal(seed: u8) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes()
}

fn spell(bytes: &[u8; 32], base: multibase::Base) -> String {
    format!("did:icn:{}", multibase::encode(base, bytes))
}

/// Two distinct strings naming one principal.
fn two_spellings(seed: u8) -> (String, String) {
    let bytes = principal(seed);
    (
        spell(&bytes, multibase::Base::Base58Btc),
        spell(&bytes, multibase::Base::Base16Lower),
    )
}

fn canonical(seed: u8) -> String {
    spell(&principal(seed), multibase::Base::Base58Btc)
}

fn now() -> SystemTime {
    SystemTime::now()
}

/// Create a sled database at `root` holding `rows`, flushed and closed.
fn make_store(root: &Path, rows: &[(String, &[u8])]) {
    std::fs::create_dir_all(root).unwrap();
    let store = SledStore::open(root).unwrap();
    for (key, value) in rows {
        store.put(key.as_bytes(), value).unwrap();
    }
    store.flush().unwrap();
}

/// Every row in a database's default tree, read and closed.
fn rows_of(root: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let store = SledStore::open(root).unwrap();
    store.scan(b"").unwrap()
}

fn read_receipt(data_dir: &Path) -> GateReceipt {
    let bytes = std::fs::read(receipt_path(data_dir)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn receipt_text(data_dir: &Path) -> String {
    std::fs::read_to_string(receipt_path(data_dir)).unwrap()
}

/// The blockers recorded for the store whose path ends with `suffix`.
fn blockers_for(receipt: &GateReceipt, suffix: &str) -> Vec<Blocker> {
    receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("receipt lists no store ending in {suffix}"))
        .blocking
        .clone()
}

fn expect_blocked(result: Result<GateReceipt, GateRefusal>) -> GateReceipt {
    match result {
        Err(GateRefusal::Blocked { receipt, .. }) => *receipt,
        Err(other) => panic!("expected Blocked, got: {other}"),
        Ok(_) => panic!("expected Blocked, got clear"),
    }
}

fn store_root(data_dir: &Path, rel: &str) -> PathBuf {
    data_dir.join("store").join(rel)
}

// ---------------------------------------------------------------------------
// Fixture guard: the whole file rests on these two facts.
// ---------------------------------------------------------------------------

#[test]
fn fixture_spellings_are_distinct_strings_that_are_one_did_under_i7() {
    let (a, b) = two_spellings(1);
    assert_ne!(a, b, "the fixture must use two different strings");

    // The gate groups by the same decode `Did` equality uses. Prove the tie at
    // the type that carries the invariant: the two spellings are `==` and hash
    // alike as `Did`, so a group the gate forms is a group I7 would form.
    let did_a: Did = a.parse().unwrap();
    let did_b: Did = b.parse().unwrap();
    assert_eq!(did_a, did_b, "I7: one principal, however spelled");
    let hash = |d: &Did| {
        let mut h = DefaultHasher::new();
        d.hash(&mut h);
        h.finish()
    };
    assert_eq!(hash(&did_a), hash(&did_b), "I7: one hash, however spelled");
    assert_ne!(
        did_a.as_str(),
        did_b.as_str(),
        "representation is untouched: the spellings stay distinct"
    );
}

// ---------------------------------------------------------------------------
// Clear paths and what they record.
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_data_dir_with_no_stores_is_clear_and_receipted() {
    let dir = tempfile::tempdir().unwrap();

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert!(receipt.stores.is_empty());
    assert_eq!(receipt.schema, RECEIPT_SCHEMA);
    assert_eq!(receipt.generation, PRINCIPAL_IDENTITY_GENERATION);
    assert_eq!(
        read_receipt(dir.path()),
        receipt,
        "the receipt on disk is the one returned"
    );
}

#[test]
fn a_missing_data_dir_is_refused_and_not_created() {
    let base = tempfile::tempdir().unwrap();
    let missing = base.path().join("never-made");

    let err = enforce(&missing, now()).unwrap_err();

    assert!(matches!(err, GateRefusal::DataDirMissing(_)), "{err}");
    assert!(!missing.exists(), "the gate does not own the layout");
}

#[test]
fn single_spelling_rows_across_every_store_are_clear_and_each_store_is_listed() {
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(2);
    let two = canonical(3);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[(format!("ledger:balance:\"{one}\""), b"10")],
    );
    make_store(
        &store_root(dir.path(), "network"),
        &[(format!("replay_max_seq:{two}"), b"7")],
    );
    // A database at the data-directory level, as deployments keep several.
    make_store(
        &dir.path().join("commons.sled"),
        &[(format!("member:coop-a:{one}"), b"m")],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let paths: Vec<&str> = receipt.stores.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths.len(), 3, "every database found: {paths:?}");
    assert!(paths.iter().any(|p| p.ends_with("commons.sled")));
    assert!(paths.iter().any(|p| p.ends_with("store/ledger")));
    assert!(paths.iter().any(|p| p.ends_with("store/network")));
    for s in &receipt.stores {
        assert_eq!(s.verdict, Verdict::Clear);
        assert!(s.blocking.is_empty());
        assert_eq!(s.rows_with_embedded_did, 1);
    }
}

#[test]
fn an_alias_collision_under_a_rule_live_in_tree_does_not_block_but_is_recorded() {
    // `replay_max_seq` merges by maximum, and that rule is implemented in the
    // live loader (#2644): the rebuild is lossless, so the gate lets it through
    // — and still records that the group exists.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(4);
    make_store(
        &store_root(dir.path(), "network"),
        &[
            (format!("replay_max_seq:{a}"), b"5"),
            (format!("replay_max_seq:{b}"), b"9"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let replay = receipt.stores[0]
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-net/replay_max_seq")
        .unwrap();
    assert_eq!(replay.collision_groups, 1, "the group is on the record");
    assert_eq!(replay.rows_in_collisions, 2);
    assert!(!replay.must_fail_closed);
}

// ---------------------------------------------------------------------------
// Discovery completeness: a database can hold databases.
// ---------------------------------------------------------------------------

#[test]
fn a_database_nested_inside_another_database_is_audited_not_omitted() {
    // `icnctl init-coop` opens `<data_dir>/store` itself as a sled database,
    // and `icnd` keeps `store/ledger`, `store/trust`, `store/cooperative`, …
    // beneath it. Discovery that records the parent and stops there leaves
    // every nested domain database unaudited: the parent is clean, the gate
    // writes a CLEAR receipt, and the daemon then opens a blocker unexamined.
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("store");
    make_store(&parent, &[("unrelated:key".to_string(), b"v")]);
    let (a, b) = two_spellings(21);
    let nested = store_root(dir.path(), "cooperative");
    make_store(
        &nested,
        &[
            (format!("member:coop-a:{a}"), b"m"),
            (format!("member:coop-a:{b}"), b"m"),
        ],
    );
    let before = rows_of(&nested);

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let paths: Vec<&str> = receipt.stores.iter().map(|s| s.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("/store")),
        "the parent database is audited: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("store/cooperative")),
        "the database nested inside it is audited too: {paths:?}"
    );
    let blockers = blockers_for(&receipt, "store/cooperative");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, .. } if keyspace == "icn-coop/member"),
        "{blockers:?}"
    );
    assert_eq!(rows_of(&nested), before, "the gate moved a byte");
}

#[test]
fn control_a_clean_database_holding_a_clean_database_is_clear_and_lists_both() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &dir.path().join("store"),
        &[("unrelated:key".to_string(), b"v")],
    );
    make_store(
        &store_root(dir.path(), "cooperative"),
        &[(format!("member:coop-a:{}", canonical(21)), b"m")],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert_eq!(receipt.stores.len(), 2, "both databases are on the receipt");
}

// ---------------------------------------------------------------------------
// A documented merge rule is not an implemented one.
// ---------------------------------------------------------------------------

#[test]
fn an_alias_collision_in_the_receiver_sequence_keyspace_refuses_because_nothing_folds_it() {
    // `trust/sequences/receiver/<issuer>` is a replay floor, and the migration
    // record wrote down a max-monotonic merge for it "by precedent". But
    // `apps/trust-app/src/sequence.rs` reads and writes the exact spelling and
    // folds nothing, so two spellings of one issuer are two independent floors
    // and the issuer can submit under whichever is lower. Until a loader
    // implements the fold, the gate must refuse rather than clear.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(22);
    let root = store_root(dir.path(), "trust");
    make_store(
        &root,
        &[
            (format!("trust/sequences/receiver/{a}"), b"5"),
            (format!("trust/sequences/receiver/{b}"), b"9"),
        ],
    );
    let before = rows_of(&root);

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/trust");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, basis, disposition, collision_groups, .. }
            if keyspace == "trust-app/sequences_receiver"
                && basis == "awaiting-domain-sign-off"
                && disposition == "max-monotonic"
                && *collision_groups == 1),
        "{blockers:?}"
    );
    assert_eq!(rows_of(&root), before, "the gate moved a byte");
}

#[test]
fn control_two_issuers_receiver_sequences_are_clear() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &store_root(dir.path(), "trust"),
        &[
            (format!("trust/sequences/receiver/{}", canonical(22)), b"5"),
            (format!("trust/sequences/receiver/{}", canonical(23)), b"9"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let seq = receipt.stores[0]
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "trust-app/sequences_receiver")
        .unwrap();
    assert_eq!(seq.collision_groups, 0);
}

// ---------------------------------------------------------------------------
// Refusals, each with its one-fact-different control.
// ---------------------------------------------------------------------------

#[test]
fn an_alias_collision_under_an_unsigned_off_rule_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(5);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (format!("ledger:balance:\"{a}\""), b"10"),
            (format!("ledger:balance:\"{b}\""), b"20"),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    assert_eq!(receipt.verdict, Verdict::Refused);
    let blockers = blockers_for(&receipt, "store/ledger");
    assert_eq!(blockers.len(), 1);
    match &blockers[0] {
        Blocker::Keyspace {
            keyspace,
            basis,
            collision_groups,
            rows_in_collisions,
            principals,
            ..
        } => {
            assert_eq!(keyspace, "icn-ledger/balance");
            assert_eq!(basis, "awaiting-domain-sign-off");
            assert_eq!(*collision_groups, 1);
            assert_eq!(*rows_in_collisions, 2);
            assert_eq!(principals.len(), 1);
            assert_eq!(principals[0].len(), 8, "a fingerprint, not an identifier");
        }
        other => panic!("wrong blocker: {other:?}"),
    }
    assert_eq!(
        read_receipt(dir.path()).verdict,
        Verdict::Refused,
        "a refusal is recorded, not just returned"
    );
}

#[test]
fn control_the_same_spelling_twice_is_one_row_and_clear() {
    // Discrimination check for the test above: the refusal came from the two
    // spellings, not from the keyspace. One spelling written twice is one row.
    let dir = tempfile::tempdir().unwrap();
    let a = canonical(5);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (format!("ledger:balance:\"{a}\""), b"10"),
            (format!("ledger:balance:\"{a}\""), b"20"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert_eq!(receipt.stores[0].total_rows, 1);
}

#[test]
fn control_two_different_principals_under_the_same_rule_are_clear() {
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(6);
    let two = canonical(7);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (format!("ledger:balance:\"{one}\""), b"10"),
            (format!("ledger:balance:\"{two}\""), b"20"),
        ],
    );

    assert_eq!(enforce(dir.path(), now()).unwrap().verdict, Verdict::Clear);
}

#[test]
fn an_alias_collision_in_a_fail_closed_keyspace_refuses() {
    // Cooperative membership: merging two rows decides who is a member, which
    // no identity-layer rule may decide.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(8);
    make_store(
        &store_root(dir.path(), "cooperative"),
        &[
            (format!("member:coop-a:{a}"), b"m"),
            (format!("member:coop-a:{b}"), b"m"),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/cooperative");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, disposition, .. }
            if keyspace == "icn-coop/member" && disposition == "FAIL-CLOSED"),
        "{blockers:?}"
    );
}

#[test]
fn an_alias_collision_in_the_security_namespace_refuses() {
    // Deferred for disposition, not for detection: the security loader folds
    // alias rows into one principal-keyed map and writes the survivor back at
    // shutdown. Starting over this store would perform that merge.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(9);
    make_store(
        &store_root(dir.path(), "security"),
        &[
            (format!("security:reputation:{a}"), b"0.2"),
            (format!("security:reputation:{b}"), b"1.0"),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/security");
    assert_eq!(blockers.len(), 1);
    assert!(
        matches!(&blockers[0], Blocker::Deferred { namespace, collision_groups, .. }
            if namespace == "security/misbehavior" && *collision_groups == 1),
        "{blockers:?}"
    );
}

#[test]
fn control_a_single_security_row_per_principal_is_clear() {
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(9);
    let two = canonical(10);
    make_store(
        &store_root(dir.path(), "security"),
        &[
            (format!("security:reputation:{one}"), b"0.2"),
            (format!("security:banned:{two}"), b"1"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let security = receipt.stores[0]
        .deferred
        .iter()
        .find(|d| d.namespace == "security/misbehavior")
        .unwrap();
    assert_eq!(security.did_bearing_rows, 2);
    assert_eq!(security.collision_groups, 0);
    assert!(!security.blocks);
}

#[test]
fn an_alias_collision_in_the_vote_namespace_is_reported_and_does_not_refuse() {
    // Votes are behind §7.5. The runtime reads them per proposal and fails
    // closed on conflicting rows at tally time, writing nothing back, so the
    // gate must not decide this for governance — but it must make the
    // collision visible on the receipt.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(11);
    make_store(
        &store_root(dir.path(), "governance"),
        &[
            (format!("gov:vote:prop-1:{a}"), b"yes"),
            (format!("gov:vote:prop-1:{b}"), b"no"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let votes = receipt.stores[0]
        .deferred
        .iter()
        .find(|d| d.namespace == "governance/votes")
        .unwrap();
    assert_eq!(votes.posture, "report-only");
    assert_eq!(votes.collision_groups, 1, "visible on the receipt");
    assert_eq!(votes.rows_in_collisions, 2);
    assert!(!votes.blocks);
}

#[test]
fn an_alias_collision_in_the_auth_challenge_namespace_is_reported_and_does_not_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(12);
    make_store(
        &store_root(dir.path(), "rpc"),
        &[
            (format!("auth:challenge:{a}"), b"n1"),
            (format!("auth:challenge:{b}"), b"n2"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let challenges = receipt.stores[0]
        .deferred
        .iter()
        .find(|d| d.namespace == "rpc/auth-challenges")
        .unwrap();
    assert_eq!(challenges.collision_groups, 1);
    assert!(!challenges.blocks);
}

#[test]
fn an_uncovered_principal_row_refuses_and_names_the_shape_not_the_identifier() {
    // A keyspace nobody registered or deferred is forced into the open: the
    // gate refuses and shows the masked shape a reviewer needs to register it.
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(13);
    make_store(
        &store_root(dir.path(), "newapp"),
        &[(format!("newapp:thing:{one}:extra"), b"v")],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/newapp");
    assert_eq!(blockers.len(), 1);
    match &blockers[0] {
        Blocker::Uncovered { shape, rows } => {
            assert_eq!(shape, "newapp:thing:<did>:extra");
            assert_eq!(*rows, 1);
        }
        other => panic!("wrong blocker: {other:?}"),
    }
    assert!(
        !receipt_text(dir.path()).contains(&one[8..]),
        "the receipt never carries the identifier"
    );
}

#[test]
fn control_an_uncovered_row_without_a_principal_is_clear() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &store_root(dir.path(), "newapp"),
        &[("newapp:thing:not-a-did:extra".to_string(), b"v".as_slice())],
    );

    assert_eq!(enforce(dir.path(), now()).unwrap().verdict, Verdict::Clear);
}

#[test]
fn a_malformed_principal_row_in_a_registered_keyspace_refuses() {
    // A row whose spelling does not decode cannot be classified: the gate does
    // not know what it would merge into, so it refuses rather than skips.
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &store_root(dir.path(), "ledger"),
        &[(
            "ledger:frozen:did:icn:zNOTAKEY".to_string(),
            b"v".as_slice(),
        )],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/ledger");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, rows_unreadable, collision_groups, .. }
            if keyspace == "icn-ledger/frozen" && *rows_unreadable == 1 && *collision_groups == 0),
        "{blockers:?}"
    );
}

#[test]
fn principal_rows_in_a_named_tree_refuse_as_unreachable() {
    // `Store::scan` reads only the default tree. A principal row the scan
    // cannot examine is not one it can clear.
    let dir = tempfile::tempdir().unwrap();
    let root = store_root(dir.path(), "gateway");
    std::fs::create_dir_all(&root).unwrap();
    {
        let db = sled::open(&root).unwrap();
        let tree = db.open_tree(b"services").unwrap();
        let one = canonical(14);
        tree.insert(format!("svc:{one}").as_bytes(), b"v".as_slice())
            .unwrap();
        db.flush().unwrap();
    }

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/gateway");
    assert!(
        matches!(&blockers[0], Blocker::Unreachable { rows } if *rows == 1),
        "{blockers:?}"
    );
    let store = &receipt.stores[0];
    assert!(store
        .trees
        .iter()
        .any(|t| t.name == "services" && t.rows_with_embedded_did == 1));
}

#[test]
fn a_store_held_open_elsewhere_cannot_be_verified_and_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let root = store_root(dir.path(), "ledger");
    make_store(&root, &[]);
    let _held = SledStore::open(&root).unwrap();

    let err = enforce(dir.path(), now()).unwrap_err();

    assert!(
        matches!(&err, GateRefusal::StoreUnverifiable { store, .. } if store == &root),
        "{err}"
    );
    assert!(
        !receipt_path(dir.path()).exists(),
        "nothing was verified, so nothing is recorded as verified"
    );
}

// ---------------------------------------------------------------------------
// The receipt: generation boundary, record-not-token, crash safety.
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_receipt_refuses_before_any_store_is_touched_and_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    make_store(&store_root(dir.path(), "ledger"), &[]);
    std::fs::write(receipt_path(dir.path()), b"{ this is not json").unwrap();

    let err = enforce(dir.path(), now()).unwrap_err();

    assert!(
        matches!(err, GateRefusal::UnreadableReceipt { .. }),
        "{err}"
    );
    assert_eq!(
        std::fs::read(receipt_path(dir.path())).unwrap(),
        b"{ this is not json",
        "the gate never overwrites a receipt it could not read"
    );
}

#[test]
fn a_receipt_with_a_foreign_schema_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        receipt_path(dir.path()),
        br#"{"schema":"something/else/v9","generation":1}"#,
    )
    .unwrap();

    let err = enforce(dir.path(), now()).unwrap_err();

    assert!(
        matches!(err, GateRefusal::UnreadableReceipt { .. }),
        "{err}"
    );
}

#[test]
fn a_receipt_from_a_newer_generation_refuses_and_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    make_store(&store_root(dir.path(), "ledger"), &[]);
    let newer = PRINCIPAL_IDENTITY_GENERATION + 1;
    // Extra, unknown fields: a later schema will have them, and the generation
    // must still be read out from around them.
    let body = format!(
        r#"{{"schema":"icn/n2a-startup-gate/v2","generation":{newer},"future":{{"x":1}}}}"#
    );
    std::fs::write(receipt_path(dir.path()), &body).unwrap();

    let err = enforce(dir.path(), now()).unwrap_err();

    assert!(
        matches!(err, GateRefusal::NewerGeneration { found, supported, .. }
            if found == newer && supported == PRINCIPAL_IDENTITY_GENERATION),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(receipt_path(dir.path())).unwrap(),
        body,
        "a newer generation's record is not overwritten by an older binary"
    );
}

#[test]
fn a_receipt_from_an_older_generation_is_superseded_by_a_fresh_audit() {
    let dir = tempfile::tempdir().unwrap();
    make_store(&store_root(dir.path(), "ledger"), &[]);
    std::fs::write(
        receipt_path(dir.path()),
        br#"{"schema":"icn/n2a-startup-gate/v0","generation":0}"#,
    )
    .unwrap();

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.generation, PRINCIPAL_IDENTITY_GENERATION);
    assert_eq!(
        read_receipt(dir.path()).generation,
        PRINCIPAL_IDENTITY_GENERATION
    );
}

#[test]
fn the_receipt_is_a_record_not_a_skip_token() {
    let dir = tempfile::tempdir().unwrap();
    let root = store_root(dir.path(), "ledger");
    let (a, b) = two_spellings(15);
    make_store(&root, &[(format!("ledger:balance:\"{a}\""), b"10")]);

    // A clear receipt exists...
    assert_eq!(enforce(dir.path(), now()).unwrap().verdict, Verdict::Clear);

    // ...and an alias row arrives afterwards, as an unsigned `from` lets any
    // peer arrange. The next start must find it, receipt or no receipt.
    {
        let store = SledStore::open(&root).unwrap();
        store
            .put(format!("ledger:balance:\"{b}\"").as_bytes(), b"20")
            .unwrap();
        store.flush().unwrap();
    }
    let refused = expect_blocked(enforce(dir.path(), now()));
    assert_eq!(refused.verdict, Verdict::Refused);
    assert_eq!(read_receipt(dir.path()).verdict, Verdict::Refused);

    // And the converse: a refused receipt does not poison a store that has
    // since been dispositioned by hand.
    {
        let store = SledStore::open(&root).unwrap();
        store
            .delete(format!("ledger:balance:\"{b}\"").as_bytes())
            .unwrap();
        store.flush().unwrap();
    }
    assert_eq!(enforce(dir.path(), now()).unwrap().verdict, Verdict::Clear);
    assert_eq!(read_receipt(dir.path()).verdict, Verdict::Clear);
}

#[test]
fn the_gate_is_idempotent_and_writes_nothing_to_any_store() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(16);
    let ledger = store_root(dir.path(), "ledger");
    let security = store_root(dir.path(), "security");
    // One clear store and one that refuses: neither may be touched either way.
    make_store(&ledger, &[(format!("ledger:balance:\"{a}\""), b"10")]);
    make_store(
        &security,
        &[
            (format!("security:banned:{a}"), b"1"),
            (format!("security:banned:{b}"), b"1"),
        ],
    );
    let ledger_before = rows_of(&ledger);
    let security_before = rows_of(&security);

    let first = expect_blocked(enforce(dir.path(), now()));
    let second = expect_blocked(enforce(dir.path(), now()));

    assert_eq!(rows_of(&ledger), ledger_before, "clear store untouched");
    assert_eq!(
        rows_of(&security),
        security_before,
        "refused store untouched"
    );
    // Same verdicts, same blockers, on every run.
    let strip = |r: &GateReceipt| (r.verdict, r.stores.clone());
    assert_eq!(strip(&first), strip(&second));
}

#[test]
fn a_stale_temporary_receipt_from_an_interrupted_write_is_not_read_as_the_receipt() {
    let dir = tempfile::tempdir().unwrap();
    make_store(&store_root(dir.path(), "ledger"), &[]);
    let tmp = dir.path().join("n2a-startup-gate.json.tmp");
    std::fs::write(&tmp, b"garbage from a crash mid-write").unwrap();

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert!(!tmp.exists(), "the interrupted write is replaced, not kept");
    assert_eq!(read_receipt(dir.path()), receipt);
}

#[test]
fn the_gate_never_creates_a_database_where_none_exists() {
    // `sled::open` on a directory that is not a database *creates* one. The
    // gate must find databases, not manufacture them.
    let dir = tempfile::tempdir().unwrap();
    let not_a_db = store_root(dir.path(), "plain-directory");
    std::fs::create_dir_all(&not_a_db).unwrap();
    std::fs::write(dir.path().join("identity.age"), b"not a database").unwrap();
    std::fs::write(not_a_db.join("notes.txt"), b"nor is this").unwrap();

    let receipt = enforce(dir.path(), now()).unwrap();

    assert!(receipt.stores.is_empty());
    assert!(!not_a_db.join("conf").exists(), "no database was created");
    assert!(!dir.path().join("conf").exists());
}

#[test]
fn the_receipt_carries_no_stored_payload_and_no_full_identifier() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(17);
    let payload = b"SECRET-PAYLOAD-7f3a9c";
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (format!("ledger:balance:\"{a}\""), payload.as_slice()),
            (format!("ledger:balance:\"{b}\""), payload.as_slice()),
        ],
    );

    let _ = expect_blocked(enforce(dir.path(), now()));

    let text = receipt_text(dir.path());
    assert!(
        !text.contains("SECRET-PAYLOAD"),
        "payload leaked into the receipt"
    );
    assert!(!text.contains(&a[8..]), "spelling leaked into the receipt");
    assert!(!text.contains(&b[8..]), "spelling leaked into the receipt");
    assert!(text.contains("awaiting-domain-sign-off"));
    assert!(text.contains("icn-ledger/balance"));
}

#[test]
fn the_refusal_message_is_actionable_and_payload_free() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(18);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (format!("ledger:balance:\"{a}\""), b"SECRET-VALUE"),
            (format!("ledger:balance:\"{b}\""), b"SECRET-VALUE"),
        ],
    );

    let err = enforce(dir.path(), now()).unwrap_err();
    let message = err.to_string();

    assert!(message.contains("REFUSED"), "{message}");
    assert!(message.contains("icn-ledger/balance"), "{message}");
    assert!(message.contains("awaiting-domain-sign-off"), "{message}");
    assert!(message.contains("did-collision-scan"), "{message}");
    assert!(message.contains("no bypass"), "{message}");
    assert!(message.contains("n2a-startup-gate.json"), "{message}");
    assert!(!message.contains("SECRET-VALUE"), "{message}");
    assert!(!message.contains(&a[8..]), "{message}");
}

// ---------------------------------------------------------------------------
// federation/attestations (#2703/#2704)
//
// The gate and `icn_federation::AttestationStore` read the same rows through
// two different doors — one asks "may this node open this state?", the other
// "may this operation interpret it?". They must agree on what a collision *is*
// or the answer depends on which door you came through. These fixtures write
// the exact bytes the store writes and assert the gate's side of that
// agreement.
// ---------------------------------------------------------------------------

/// A row exactly as `AttestationStore` keys it.
fn attestation_row(spelling: &str, source: &str) -> String {
    format!("federation/attestations/{spelling}/{source}")
}

#[test]
fn an_alias_collision_in_the_federation_attestation_keyspace_refuses() {
    // Two persisted claims from one source cooperative about one principal,
    // spelled two ways. They can only differ by disagreeing, and no
    // federation-domain rule authorizes choosing between them, so the node
    // must not open the store — exactly what `AttestationStore` does when a
    // lookup, a listing, a write or the expiry sweep meets the same pair.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(30);
    let root = store_root(dir.path(), "federation");
    make_store(
        &root,
        &[
            (attestation_row(&a, "food-coop"), b"{}"),
            (attestation_row(&b, "food-coop"), b"{}"),
        ],
    );
    let before = rows_of(&root);

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/federation");
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, disposition, collision_groups, rows_in_collisions, .. }
            if keyspace == "icn-federation/attestations"
                && disposition == "FAIL-CLOSED"
                && *collision_groups == 1
                && *rows_in_collisions == 2),
        "{blockers:?}"
    );
    assert_eq!(receipt.verdict, Verdict::Refused);

    // The gate reports; it never repairs. Both physical spellings survive
    // byte-for-byte, because the evidence an operator needs to disposition the
    // pair is the pair.
    assert_eq!(rows_of(&root), before, "the gate re-keyed or dropped a row");
    assert_eq!(before.len(), 2);
}

#[test]
fn control_one_principal_under_two_spellings_from_two_sources_is_clear() {
    // The same two spellings of the same principal, differing in exactly one
    // fact: the source cooperative. Attestations from different cooperatives
    // about one member are the federation's ordinary union, so this must not
    // refuse — otherwise the fixture above would pass by blocking everything.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(31);
    make_store(
        &store_root(dir.path(), "federation"),
        &[
            (attestation_row(&a, "food-coop"), b"{}"),
            (attestation_row(&b, "housing-coop"), b"{}"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let federation = receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("store/federation"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-federation/attestations")
        .expect("the gate consumes the registered federation descriptor");
    assert_eq!(federation.rows_scanned, 2);
    assert_eq!(federation.distinct_principals, 2);
    assert_eq!(federation.rows_unreadable, 0);
    assert_eq!(federation.collision_groups, 0);
    assert!(!federation.must_fail_closed);
}

#[test]
fn a_source_cooperative_id_containing_a_did_does_not_refuse_the_start() {
    // Nothing in the federation domain forbids a cooperative identifier that
    // contains `did:icn:`, and `AttestationStore` compares source ids as exact
    // bytes. A gate that parsed inside the source would refuse a start over a
    // store the running node reads without difficulty (#2704 review, P2).
    let dir = tempfile::tempdir().unwrap();
    let member = canonical(32);
    let (source_a, source_b) = two_spellings(33);
    make_store(
        &store_root(dir.path(), "federation"),
        &[
            (attestation_row(&member, &format!("coop-{source_a}")), b"{}"),
            (attestation_row(&member, &format!("coop-{source_b}")), b"{}"),
            (attestation_row(&member, "did:icn:!!!!"), b"{}"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let federation = receipt.stores[0]
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-federation/attestations")
        .unwrap();
    assert_eq!(federation.rows_scanned, 3);
    assert_eq!(
        federation.distinct_principals, 3,
        "one member, three source ids the scan never parses"
    );
    assert_eq!(federation.rows_unreadable, 0);
    assert_eq!(federation.collision_groups, 0);
}

#[test]
fn a_federation_row_whose_member_segment_names_no_principal_refuses() {
    // The layout puts a principal at the anchor, so a row without one is a row
    // nobody can classify — and `AttestationStore` refuses it too, because its
    // loader rebuilds the key from the value and finds a mismatch.
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(34);
    make_store(
        &store_root(dir.path(), "federation"),
        &[
            (attestation_row("did:icn:!!!!", "food-coop"), b"{}"),
            (attestation_row(&format!("{one}junk"), "food-coop"), b"{}"),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/federation");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, rows_unreadable, collision_groups, .. }
            if keyspace == "icn-federation/attestations"
                && *rows_unreadable == 2
                && *collision_groups == 0),
        "{blockers:?}"
    );
}

#[test]
fn a_federation_attestation_row_is_never_reported_as_uncovered() {
    // Before #2703 registered this prefix a populated attestation row could
    // only surface as an uncovered shape: blocking, but unclassified. The
    // registration is what turns it into a keyspace the gate can speak about.
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &store_root(dir.path(), "federation"),
        &[(attestation_row(&canonical(35), "food-coop"), b"{}")],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert!(
        !receipt_text(dir.path()).contains("UNCOVERED"),
        "an attestation row must be classified, not merely unaccounted for"
    );
}

// ---------------------------------------------------------------------------
// idx_agreement_party/ (#2627 row 28, #2707)
//
// The gate and `icn_federation::agreement::AgreementStore` read the same rows
// through two different doors. The layout is the attestation layout's shape —
// one anchored party spelling, `/`, an agreement id the scan never parses —
// under the opposite disposition: the rows are a projection of the canonical
// `federation/agreements/` rows, which the store proves membership from on
// every read, so an alias pair for one agreement is two derivations of one
// fact and the start is clear, where an attestation pair is two claims and
// refuses. These fixtures write the exact bytes the store writes and assert
// the gate's side of that agreement.
// ---------------------------------------------------------------------------

/// A row exactly as `AgreementStore` keys it.
fn party_index_row(spelling: &str, agreement_id: &str) -> String {
    format!("idx_agreement_party/{spelling}/{agreement_id}")
}

#[test]
fn an_alias_pair_in_the_agreement_party_index_for_one_agreement_is_clear_and_untouched() {
    // One party, two spellings, one agreement: two projection rows derived
    // from one canonical membership fact. The registered disposition is
    // `Equivalent`, so the gate records the group and clears the start — and,
    // being read-only, moves no row. A CLEAR here says only that this spelling
    // collision is safe under the projection disposition; whether the
    // projection is complete or current is the store's business, proven by
    // its canonical-membership reads and its explicit rebuild, not the gate's.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(40);
    let root = store_root(dir.path(), "agreements");
    make_store(
        &root,
        &[
            (party_index_row(&a, "agr-1"), b"agr-1"),
            (party_index_row(&b, "agr-1"), b"agr-1"),
        ],
    );
    let before = rows_of(&root);

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let party_index = receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("store/agreements"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-federation/agreement_party_index")
        .expect("the gate consumes the registered party-index descriptor");
    assert_eq!(party_index.rows_scanned, 2);
    assert_eq!(party_index.distinct_principals, 1);
    assert_eq!(
        party_index.collision_groups, 1,
        "the alias group is classified, not ignored"
    );
    assert_eq!(party_index.rows_in_collisions, 2);
    assert_eq!(party_index.rows_unreadable, 0);
    assert_eq!(party_index.disposition, "equivalent");
    assert_eq!(party_index.basis, "established");
    assert!(!party_index.must_fail_closed);
    assert!(blockers_for(&receipt, "store/agreements").is_empty());

    // The gate reports; it never repairs. Both spellings survive byte-for-byte:
    // retiring one is the store's `rebuild_party_index`, an explicit
    // projection-repair operation and not a startup step.
    assert_eq!(rows_of(&root), before, "the gate re-keyed or dropped a row");
    assert_eq!(before.len(), 2);
}

#[test]
fn control_one_party_under_two_spellings_in_two_agreements_is_two_shapes() {
    // The same two spellings, differing in exactly one fact: the agreement id.
    // One party in two agreements is two canonical facts, so the rows must not
    // group — otherwise the fixture above would pass by grouping everything
    // one principal ever touched.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(41);
    make_store(
        &store_root(dir.path(), "agreements"),
        &[
            (party_index_row(&a, "agr-1"), b"agr-1"),
            (party_index_row(&b, "agr-2"), b"agr-2"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let party_index = receipt.stores[0]
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-federation/agreement_party_index")
        .unwrap();
    assert_eq!(party_index.rows_scanned, 2);
    assert_eq!(
        party_index.distinct_principals, 2,
        "two (party, agreement) tuples"
    );
    assert_eq!(party_index.collision_groups, 0);
    assert_eq!(party_index.rows_unreadable, 0);
}

#[test]
fn a_party_index_row_whose_spelling_names_no_principal_refuses() {
    // The layout puts a principal at the anchor, so a row without one is a row
    // nobody can classify — and `AgreementStore` refuses it too, as a
    // malformed projection row, before any operation reads around it.
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(42);
    let root = store_root(dir.path(), "agreements");
    make_store(
        &root,
        &[
            (party_index_row("did:icn:!!!!", "agr-1"), b"agr-1"),
            (party_index_row(&format!("{one}junk"), "agr-1"), b"agr-1"),
        ],
    );
    let before = rows_of(&root);

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/agreements");
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, rows_unreadable, collision_groups, .. }
            if keyspace == "icn-federation/agreement_party_index"
                && *rows_unreadable == 2
                && *collision_groups == 0),
        "{blockers:?}"
    );
    assert_eq!(receipt.verdict, Verdict::Refused);
    assert_eq!(rows_of(&root), before, "the gate re-keyed or dropped a row");
}

#[test]
fn an_agreement_id_containing_a_did_does_not_refuse_the_start() {
    // `AgreementId::new` accepts any string and `AgreementStore` compares ids
    // as exact bytes, anchoring its own parse on the id the row's value names,
    // so nothing forbids an agreement id that contains — or is — a `did:icn:`
    // spelling. A gate that parsed inside the id would group rows the store
    // holds apart and refuse a start over a store the running node reads
    // without difficulty.
    let dir = tempfile::tempdir().unwrap();
    let party = canonical(43);
    let (other_a, other_b) = two_spellings(44);
    let id_a = format!("agr-{other_a}");
    let id_b = format!("agr-{other_b}");
    make_store(
        &store_root(dir.path(), "agreements"),
        &[
            (party_index_row(&party, &id_a), id_a.as_bytes()),
            (party_index_row(&party, &id_b), id_b.as_bytes()),
            (party_index_row(&party, "did:icn:!!!!"), b"did:icn:!!!!"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let party_index = receipt.stores[0]
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-federation/agreement_party_index")
        .unwrap();
    assert_eq!(party_index.rows_scanned, 3);
    assert_eq!(
        party_index.distinct_principals, 3,
        "one party, three agreement ids the scan never parses"
    );
    assert_eq!(party_index.rows_unreadable, 0);
    assert_eq!(party_index.collision_groups, 0);
}

#[test]
fn an_agreement_party_row_is_never_reported_as_uncovered() {
    // Before #2707 registered this prefix a populated party-index row could
    // only surface as an uncovered shape: blocking, but unclassified. The
    // registration is what turns it into a keyspace the gate can speak about.
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &store_root(dir.path(), "agreements"),
        &[(party_index_row(&canonical(45), "agr-1"), b"agr-1")],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert!(
        !receipt_text(dir.path()).contains("UNCOVERED"),
        "a party-index row must be classified, not merely unaccounted for"
    );
}

// ---------------------------------------------------------------------------
// ledger:treasury:<did> (#2627 rows 10/41, M1)
//
// The gate and `icn_ledger::TreasuryManager::with_store` read the same rows
// through two different doors. The primary treasury row is keyed by the
// treasury principal alone and nothing follows the spelling; the descriptor
// claims it through the DID scheme, so the budget, rule, audit, index and
// velocity-limit siblings beneath the same lexical parent are outside it.
// The disposition is FAIL-CLOSED and established in the loader: an alias pair
// refuses the start exactly as it refuses hydration, and neither layer
// elects a survivor. The loader-side fixtures are
// `icn-ledger/tests/treasury_principal_rows.rs`.
// ---------------------------------------------------------------------------

/// A row exactly as `TreasuryManager::persist_treasury` keys it.
fn treasury_row(spelling: &str) -> String {
    format!("ledger:treasury:{spelling}")
}

fn treasury_keyspace_receipt(
    receipt: &GateReceipt,
) -> &icn_store::n2a_startup_gate::KeyspaceReceipt {
    receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("store/ledger"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-ledger/treasury")
        .expect("the gate consumes the registered treasury descriptor")
}

#[test]
fn an_alias_collision_in_the_treasury_keyspace_refuses() {
    // Two persisted treasury records for one principal, spelled two ways.
    // They can disagree about every field and no economics rule authorizes
    // choosing between them, so the node must not open the store — exactly
    // what `TreasuryManager::with_store` does with the same pair.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(50);
    let root = store_root(dir.path(), "ledger");
    make_store(
        &root,
        &[(treasury_row(&a), b"{}"), (treasury_row(&b), b"{}")],
    );
    let before = rows_of(&root);

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/ledger");
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, disposition, collision_groups, rows_in_collisions, rows_unreadable, .. }
            if keyspace == "icn-ledger/treasury"
                && disposition == "FAIL-CLOSED"
                && *collision_groups == 1
                && *rows_in_collisions == 2
                && *rows_unreadable == 0),
        "{blockers:?}"
    );
    assert_eq!(receipt.verdict, Verdict::Refused);

    // The gate reports; it never repairs. Both physical spellings survive
    // byte-for-byte: the evidence an operator needs is the pair itself.
    assert_eq!(rows_of(&root), before, "the gate re-keyed or dropped a row");
    assert_eq!(before.len(), 2);
}

#[test]
fn control_a_single_treasury_row_is_clear_and_covered() {
    // One registered treasury and its cooperative index — the rows an
    // ordinary registration leaves. Before this registration the primary row
    // could only surface as an uncovered shape, so a node holding one
    // treasury refused to start; now the keyspace is classified and clear.
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(51);
    let root = store_root(dir.path(), "ledger");
    make_store(
        &root,
        &[
            (treasury_row(&one), b"{}"),
            (
                "ledger:treasury:idx:coop:food-coop".to_string(),
                one.as_bytes(),
            ),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let treasury = treasury_keyspace_receipt(&receipt);
    assert_eq!(treasury.rows_scanned, 1);
    assert_eq!(treasury.distinct_principals, 1);
    assert_eq!(treasury.rows_unreadable, 0);
    assert_eq!(treasury.collision_groups, 0);
    assert!(!treasury.must_fail_closed);
    assert!(
        !receipt_text(dir.path()).contains("UNCOVERED"),
        "a treasury row must be classified, not merely unaccounted for"
    );
}

#[test]
fn control_two_treasury_principals_are_clear() {
    let dir = tempfile::tempdir().unwrap();
    let root = store_root(dir.path(), "ledger");
    make_store(
        &root,
        &[
            (treasury_row(&canonical(52)), b"{}"),
            (treasury_row(&canonical(53)), b"{}"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let treasury = treasury_keyspace_receipt(&receipt);
    assert_eq!(treasury.rows_scanned, 2);
    assert_eq!(treasury.distinct_principals, 2);
    assert_eq!(treasury.collision_groups, 0);
}

#[test]
fn an_unreadable_treasury_spelling_refuses() {
    // A row whose spelling does not decode, and one with material after a
    // spelling that does: neither is a row the writer produces, and the
    // loader refuses both as unreadable keys. The gate refuses rather than
    // skips, because a skipped row would make the rest look unambiguous.
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(54);
    let root = store_root(dir.path(), "ledger");
    make_store(
        &root,
        &[
            ("ledger:treasury:did:icn:zNOTAKEY".to_string(), b"{}"),
            (format!("ledger:treasury:{one}junk"), b"{}"),
        ],
    );
    let before = rows_of(&root);

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/ledger");
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, rows_unreadable, collision_groups, .. }
            if keyspace == "icn-ledger/treasury"
                && *rows_unreadable == 2
                && *collision_groups == 0),
        "{blockers:?}"
    );
    assert_eq!(rows_of(&root), before);
}

#[test]
fn treasury_sibling_subspaces_are_not_misread_as_primary_rows() {
    // One primary row plus one row of every sibling whose key carries no
    // principal: budget, rule, cooperative index (its value is a spelling,
    // which the gate never reads) and velocity limit. The start is clear and
    // the treasury keyspace counts exactly one row — the siblings share the
    // lexical parent, not the descriptor.
    let dir = tempfile::tempdir().unwrap();
    let one = canonical(55);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (treasury_row(&one), b"{}"),
            ("ledger:treasury:budget:budget-1".to_string(), b"{}"),
            ("ledger:treasury:rule:rule-1".to_string(), b"{}"),
            (
                "ledger:treasury:idx:coop:food-coop".to_string(),
                one.as_bytes(),
            ),
            ("ledger:treasury:vlimit:vlimit-1".to_string(), b"{}"),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let treasury = treasury_keyspace_receipt(&receipt);
    assert_eq!(
        treasury.rows_scanned, 1,
        "siblings are outside the descriptor"
    );
    assert_eq!(treasury.collision_groups, 0);
}

#[test]
fn a_did_looking_coop_id_in_the_treasury_index_is_never_a_treasury_spelling() {
    // Opaque-discriminator control. The cooperative index is keyed by a coop
    // id the ledger never validates, so one can be a DID spelling — here the
    // alias of the primary row's own principal. The treasury descriptor must
    // not read it as a second spelling of that row: no treasury collision.
    // Carrying a spelling under no registered prefix, it surfaces as the one
    // thing the gate can truthfully say about it — UNCOVERED, unclassified,
    // exactly as before M1 — which is a different refusal for a different
    // reason, and the loader (which reads the index's *value*, never its key)
    // does not disagree about the primary row.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(56);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (treasury_row(&a), b"{}"),
            (format!("ledger:treasury:idx:coop:{b}"), a.as_bytes()),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/ledger");
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(
        matches!(&blockers[0], Blocker::Uncovered { shape, rows: 1 }
            if shape == "ledger:treasury:idx:coop:<did>"),
        "{blockers:?}"
    );
    let treasury = treasury_keyspace_receipt(&receipt);
    assert_eq!(treasury.rows_scanned, 1);
    assert_eq!(
        treasury.collision_groups, 0,
        "no treasury alias was inferred"
    );
}

#[test]
fn treasury_siblings_that_embed_a_spelling_stay_uncovered_not_misclassified() {
    // The audit and budget-index subspaces embed the treasury spelling as key
    // structure. M1 dispositions the primary row only; these keep the status
    // they had before it — principal-bearing rows under no registered
    // keyspace — and are never folded into the treasury keyspace's count or
    // its collision groups, even when spelled as the primary row's alias.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(57);
    make_store(
        &store_root(dir.path(), "ledger"),
        &[
            (treasury_row(&a), b"{}"),
            (
                format!("ledger:treasury:audit:{b}:1700000000:audit-1"),
                b"{}",
            ),
            (
                format!("ledger:treasury:idx:budgets:{b}:budget-1"),
                b"budget-1",
            ),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "store/ledger");
    assert_eq!(blockers.len(), 2, "{blockers:?}");
    assert!(blockers
        .iter()
        .all(|b| matches!(b, Blocker::Uncovered { .. })));
    let treasury = treasury_keyspace_receipt(&receipt);
    assert_eq!(treasury.rows_scanned, 1);
    assert_eq!(treasury.collision_groups, 0);
}

// ---------------------------------------------------------------------------
// ADR-0014 by-grantee projection (#2627 M2)
//
// The gateway opens its store at `<data_dir>/gateway_store`, beside the
// per-domain databases under `store/`, so the gate finds it the same way. Its
// by-grantee rows are binary: length-framed and tag-discriminated. Before this
// tranche registered them, one ordinary Person grant produced an uncovered
// shape and refused the start.
// ---------------------------------------------------------------------------

/// Create a sled database at `root` holding raw-byte rows, flushed and closed.
fn make_store_raw(root: &Path, rows: &[(Vec<u8>, &[u8])]) {
    std::fs::create_dir_all(root).unwrap();
    let store = SledStore::open(root).unwrap();
    for (key, value) in rows {
        store.put(key, value).unwrap();
    }
    store.flush().unwrap();
}

/// Reproduce `ReceiptStore::grant_by_grantee_key` byte-for-byte.
fn grant_by_grantee_row(tag: u8, body: &[u8], valid_from: u64, grant_id: &str) -> Vec<u8> {
    let mut region = vec![tag];
    region.extend_from_slice(body);
    let mut key = b"adr0014:grant:by_grantee:".to_vec();
    key.extend_from_slice(&(region.len() as u32).to_be_bytes());
    key.extend_from_slice(&region);
    key.extend_from_slice(&valid_from.to_be_bytes());
    key.extend_from_slice(grant_id.as_bytes());
    key
}

fn person_grant_row(spelling: &str, valid_from: u64, grant_id: &str) -> Vec<u8> {
    grant_by_grantee_row(0x01, spelling.as_bytes(), valid_from, grant_id)
}

const GATE_GRANT_A: &str = "11111111-1111-4111-8111-111111111111";
const GATE_GRANT_B: &str = "22222222-2222-4222-8222-222222222222";

#[test]
fn one_ordinary_person_grant_no_longer_refuses_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("gateway_store");
    make_store_raw(
        &root,
        &[(
            person_grant_row(&canonical(50), 1_000, GATE_GRANT_A),
            GATE_GRANT_A.as_bytes(),
        )],
    );

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    assert!(
        blockers_for(&receipt, "gateway_store").is_empty(),
        "an ordinary Person grant must not block a start"
    );
    let ks = receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("gateway_store"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-gateway/adr0014_grant_by_grantee")
        .expect("the gate consumes the registered by-grantee descriptor");
    assert_eq!(ks.rows_scanned, 1);
    assert_eq!(ks.distinct_principals, 1);
    assert_eq!(ks.rows_unreadable, 0);
}

#[test]
fn an_alias_pair_for_one_grant_is_clear_and_untouched() {
    // One principal, two spellings, one canonical grant: equivalent derived
    // evidence. The gate classifies the group, clears, and moves no byte.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(51);
    let root = dir.path().join("gateway_store");
    make_store_raw(
        &root,
        &[
            (
                person_grant_row(&a, 1_000, GATE_GRANT_A),
                GATE_GRANT_A.as_bytes(),
            ),
            (
                person_grant_row(&b, 1_000, GATE_GRANT_A),
                GATE_GRANT_A.as_bytes(),
            ),
        ],
    );
    let before = rows_of(&root);

    let receipt = enforce(dir.path(), now()).unwrap();

    assert_eq!(receipt.verdict, Verdict::Clear);
    let ks = receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("gateway_store"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-gateway/adr0014_grant_by_grantee")
        .unwrap();
    assert_eq!(ks.rows_scanned, 2);
    assert_eq!(ks.distinct_principals, 1);
    assert_eq!(ks.collision_groups, 1, "the alias group is classified");
    assert_eq!(ks.rows_in_collisions, 2);
    assert_eq!(ks.disposition, "equivalent");
    assert_eq!(ks.basis, "established");
    assert!(blockers_for(&receipt, "gateway_store").is_empty());
    assert_eq!(rows_of(&root), before, "the gate re-keyed or dropped a row");
}

#[test]
fn control_two_grants_for_one_principal_are_two_shapes() {
    // Differs in exactly one fact from the fixture above: the grant id. A
    // principal may hold several distinct grants, so these must not group —
    // otherwise the alias fixture would pass by grouping everything.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(52);
    make_store_raw(
        &dir.path().join("gateway_store"),
        &[
            (
                person_grant_row(&a, 1_000, GATE_GRANT_A),
                GATE_GRANT_A.as_bytes(),
            ),
            (
                person_grant_row(&b, 2_000, GATE_GRANT_B),
                GATE_GRANT_B.as_bytes(),
            ),
        ],
    );

    let receipt = enforce(dir.path(), now()).unwrap();
    assert_eq!(receipt.verdict, Verdict::Clear);
    let ks = receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("gateway_store"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-gateway/adr0014_grant_by_grantee")
        .unwrap();
    assert_eq!(ks.distinct_principals, 2);
    assert_eq!(ks.collision_groups, 0);
}

#[test]
fn control_an_entity_grantee_row_that_spells_a_did_names_no_principal() {
    // Tag 0x02 carries an entity id the granting domain chose. The registry
    // must not principalize it merely because its bytes look like a DID.
    let dir = tempfile::tempdir().unwrap();
    make_store_raw(
        &dir.path().join("gateway_store"),
        &[(
            grant_by_grantee_row(0x02, canonical(53).as_bytes(), 1_000, GATE_GRANT_A),
            GATE_GRANT_A.as_bytes(),
        )],
    );

    let receipt = enforce(dir.path(), now()).unwrap();
    assert_eq!(receipt.verdict, Verdict::Clear);
    let ks = receipt
        .stores
        .iter()
        .find(|s| s.path.ends_with("gateway_store"))
        .unwrap()
        .keyspaces
        .iter()
        .find(|k| k.keyspace == "icn-gateway/adr0014_grant_by_grantee")
        .unwrap();
    assert_eq!(ks.rows_scanned, 1);
    assert_eq!(ks.distinct_principals, 0, "an entity id is not a principal");
    assert_eq!(ks.rows_unreadable, 0, "and it is not unreadable either");
}

#[test]
fn a_person_grant_row_whose_spelling_names_no_principal_refuses() {
    // A row this writer could not have produced. It cannot be classified, so
    // it cannot be ruled out as one that hides a real grant.
    let dir = tempfile::tempdir().unwrap();
    make_store_raw(
        &dir.path().join("gateway_store"),
        &[(
            person_grant_row("did:icn:not-a-spelling!!", 1_000, GATE_GRANT_A),
            GATE_GRANT_A.as_bytes(),
        )],
    );

    let err = enforce(dir.path(), now()).unwrap_err();
    assert!(
        matches!(err, GateRefusal::Blocked { .. }),
        "an unreadable principal row must refuse; got {err:?}"
    );
}

#[test]
fn broken_binary_framing_in_a_grantee_row_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let mut overrun = b"adr0014:grant:by_grantee:".to_vec();
    overrun.extend_from_slice(&u32::MAX.to_be_bytes());
    overrun.extend_from_slice(b"\x01did:icn:z");
    make_store_raw(
        &dir.path().join("gateway_store"),
        &[(overrun, GATE_GRANT_A.as_bytes())],
    );

    let err = enforce(dir.path(), now()).unwrap_err();
    assert!(
        matches!(err, GateRefusal::Blocked { .. }),
        "a length field that overruns the key must refuse; got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// icn-commons `commons/holders/by_did/` — the weak-holder identity index
// (#2627 M3)
//
// A weak `CommonsHolderRecord`'s durable id is `SHA-256` of the textual DID it
// was minted from, so two spellings of one principal name two independent
// holders with their own status, personhood level and rights. The gate must
// refuse that state for the same reason the live mint seam refuses to create
// it: choosing a survivor is a decision about a member's standing, and no
// identity-layer rule makes it.
//
// The store is the real one: `commons.sled` sits at the data-directory level,
// where `icn_core::supervisor::lifecycle` and `icn_gateway::server` open it.
// ---------------------------------------------------------------------------

fn commons_root(data_dir: &Path) -> PathBuf {
    data_dir.join("commons.sled")
}

fn holder_by_did_key(spelling: &str) -> String {
    format!("commons/holders/by_did/{spelling}")
}

/// A: one valid row is covered and clear.
///
/// Before registration this row blocked as `UNCOVERED` — an ordinary
/// deployment holding a single weak holder could not start. Registration is
/// what turns it into a dispositioned, passing row.
#[test]
fn a_single_holder_by_did_row_is_covered_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &commons_root(dir.path()),
        &[(holder_by_did_key(&canonical(20)), b"aa")],
    );

    let receipt = enforce(dir.path(), now()).expect("one spelling, one holder, nothing to decide");
    assert_eq!(receipt.verdict, Verdict::Clear);
    assert!(
        receipt
            .stores
            .iter()
            .any(|s| s.path.ends_with("commons.sled")),
        "the commons store is listed, not skipped: {:?}",
        receipt.stores.iter().map(|s| &s.path).collect::<Vec<_>>()
    );
}

/// B: two spellings of one principal naming two different holders refuses.
#[test]
fn an_alias_pair_of_holder_by_did_rows_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(21);
    make_store(
        &commons_root(dir.path()),
        &[
            (holder_by_did_key(&a), b"aa"),
            (holder_by_did_key(&b), b"bb"),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));

    let blockers = blockers_for(&receipt, "commons.sled");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, disposition, collision_groups, .. }
            if keyspace == "icn-commons/holder_by_did"
                && disposition == "FAIL-CLOSED"
                && *collision_groups == 1),
        "{blockers:?}"
    );

    // Nothing is repaired on the way past: the gate reports, it does not merge.
    assert_eq!(
        rows_of(&commons_root(dir.path())).len(),
        2,
        "both rows survive the refusal untouched"
    );
}

/// C: two spellings pointing at *one* holder id still refuses.
///
/// It looks like the benign case, and it is not one this tranche may wave
/// through: nothing in the registry authorizes collapsing two spellings, and
/// the equal values would make a rebuild's choice look free. `Equivalent` is a
/// claim about derivation that only the owning domain can make.
#[test]
fn an_alias_pair_pointing_at_one_holder_still_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = two_spellings(22);
    make_store(
        &commons_root(dir.path()),
        &[
            (holder_by_did_key(&a), b"aa"),
            (holder_by_did_key(&b), b"aa"),
        ],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));
    let blockers = blockers_for(&receipt, "commons.sled");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, disposition, .. }
            if keyspace == "icn-commons/holder_by_did" && disposition == "FAIL-CLOSED"),
        "{blockers:?}"
    );
}

/// D: two distinct principals are not a collision. Before registration this
/// state blocked too, as uncovered — a false refusal.
#[test]
fn control_two_distinct_principals_hold_two_holders_and_are_clear() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &commons_root(dir.path()),
        &[
            (holder_by_did_key(&canonical(23)), b"aa"),
            (holder_by_did_key(&canonical(24)), b"cc"),
        ],
    );

    let receipt = enforce(dir.path(), now()).expect("two principals, two holders, no collision");
    assert_eq!(receipt.verdict, Verdict::Clear);
}

/// E: a suffix that carries the DID scheme but no readable principal is
/// unreadable evidence, not a principal-free row.
#[test]
fn a_malformed_holder_by_did_suffix_refuses_as_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &commons_root(dir.path()),
        &[(holder_by_did_key("did:icn:zzz!!!not-multibase"), b"aa")],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));
    let blockers = blockers_for(&receipt, "commons.sled");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, rows_unreadable, .. }
            if keyspace == "icn-commons/holder_by_did" && *rows_unreadable == 1),
        "{blockers:?}"
    );
}

/// E, second form: `did_ends_key` states the writer's shape, so a spelling with
/// anything appended is a row the real loader would never produce and cannot be
/// read as a clean principal.
#[test]
fn a_holder_by_did_row_with_bytes_after_the_spelling_refuses_as_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &commons_root(dir.path()),
        &[(
            format!("{}:trailing", holder_by_did_key(&canonical(25))),
            b"aa",
        )],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));
    let blockers = blockers_for(&receipt, "commons.sled");
    assert!(
        matches!(&blockers[0], Blocker::Keyspace { keyspace, rows_unreadable, .. }
            if keyspace == "icn-commons/holder_by_did" && *rows_unreadable == 1),
        "{blockers:?}"
    );
}

/// F: sibling isolation. The primary holder record and the by-anchor index live
/// under the same lexical parent and are keyed by opaque hex, not by a
/// spelling. Neither is claimed by this descriptor, and a `did:icn:` that
/// appears in a sibling's stored *value* is not key material.
#[test]
fn the_holder_primary_and_by_anchor_siblings_are_outside_this_descriptor() {
    let dir = tempfile::tempdir().unwrap();
    let opaque = hex::encode(principal(26));
    let spelling = canonical(26);
    make_store(
        &commons_root(dir.path()),
        &[
            // The two siblings, under two spellings' worth of opaque ids.
            (format!("commons/holders/{opaque}"), b"{}"),
            (
                format!("commons/holders/{}", hex::encode(principal(27))),
                b"{}",
            ),
            (format!("commons/holders/by_anchor/{opaque}"), b"aa"),
            // A sibling whose *value* spells a DID.
            (
                format!("commons/holders/by_anchor/{}", hex::encode(principal(27))),
                spelling.as_bytes(),
            ),
            // One real index row, so the descriptor is exercised at the same
            // time and the clear verdict is not vacuous.
            (holder_by_did_key(&spelling), b"aa"),
        ],
    );

    let receipt = enforce(dir.path(), now())
        .expect("opaque sibling keys carry no spelling and are not this keyspace");
    assert_eq!(receipt.verdict, Verdict::Clear);
    assert_eq!(
        rows_of(&commons_root(dir.path())).len(),
        5,
        "every planted row is still there"
    );
}

/// §20, behaviourally: a sibling row whose *key* happens to carry a spelling
/// must not be read as a holder-by-DID row.
///
/// The two siblings are keyed by hex and cannot spell `did:icn:` when the
/// writer produces them, so this row is durable bytes no writer makes — which
/// is exactly the case a gate exists for. Under the registered prefix it is
/// outside every keyspace and blocks as `UNCOVERED`. Under a prefix widened to
/// the lexical parent it would fall inside this descriptor, be read as one more
/// index row, form no collision on its own, and clear the start silently.
#[test]
fn a_by_anchor_key_carrying_a_spelling_is_uncovered_not_read_as_an_index_row() {
    let dir = tempfile::tempdir().unwrap();
    make_store(
        &commons_root(dir.path()),
        &[(
            format!("commons/holders/by_anchor/{}", canonical(28)),
            b"aa",
        )],
    );

    let receipt = expect_blocked(enforce(dir.path(), now()));
    let blockers = blockers_for(&receipt, "commons.sled");
    assert!(
        matches!(&blockers[0], Blocker::Uncovered { shape, rows }
            if shape.contains("by_anchor") && *rows == 1),
        "a spelling under the by-anchor prefix belongs to no registered keyspace \
         and must be reported as such, not cleared: {blockers:?}"
    );
}

/// The descriptor claims the index prefix and nothing lexically near it.
#[test]
fn the_holder_by_did_descriptor_claims_only_the_index_prefix() {
    let d = icn_store::did_collision_scan::n2a_keyspaces()
        .into_iter()
        .find(|d| d.name == "icn-commons/holder_by_did")
        .expect("the holder-by-DID index is registered (#2627 M3)");

    assert_eq!(d.prefix, b"commons/holders/by_did/");
    assert_eq!(d.inventory_rows, &[67]);
    assert!(
        !d.disposition.is_automatable(),
        "the disposition must stay fail-closed: no rule chooses a surviving holder"
    );
    assert!(d.did_ends_key, "the writer appends the spelling and stops");
    assert!(!d.slash_ends_did);

    // The two siblings are not merely uncollected — they do not start with the
    // registered prefix, so no scan of it can reach them.
    for sibling in [
        &b"commons/holders/deadbeef"[..],
        &b"commons/holders/by_anchor/deadbeef"[..],
    ] {
        assert!(
            !sibling.starts_with(d.prefix),
            "{} is inside the descriptor and must not be",
            String::from_utf8_lossy(sibling)
        );
    }
}
