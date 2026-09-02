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
