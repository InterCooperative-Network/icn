//! `icnctl device-authority`: the stateless relying party over an N4-B bundle, end to end.
//!
//! The binary is driven as a downstream consumer would drive it: a bundle file on disk, the
//! Subject and position supplied by the caller, one JSON document on stdout, the verdict in the
//! exit code. No data directory is populated, no daemon runs, no network is reached.
//!
//! Fixture identities only: every key is derived from a seed byte; nothing here is a real person.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{
    authorize_event, revoke_event, CapabilitySet, ContextNonce, ContinuityRoot, DeviceCapability,
    EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1, SignedDeviceAct};
use icn_identity::device_authority_bundle::DeviceAuthorityBundleV1;
use serde_json::Value;

fn icnctl() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

struct Fixture {
    subject: SubjectId,
    a_key: SigningKey,
    a: PrincipalKey,
    b_key: SigningKey,
    b: PrincipalKey,
    /// Positions: 1 = authorize A, 2 = authorize B, 3 = revoke A.
    facts: Vec<SignedAuthorityEvent>,
}

fn stranger(seed: u8) -> (SigningKey, PrincipalKey) {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let principal = PrincipalKey::try_from_verifying_key(key.verifying_key()).unwrap();
    (key, principal)
}

fn fixture() -> Fixture {
    let root = ContinuityRoot::with_plan(
        [0x01; 32],
        ContextNonce::from_bytes([0x01 ^ 0xa5; 32]),
        vec![EstablishmentKind::Rotate; 4],
    );
    let inception = root.incept().unwrap();
    let subject = inception.body.subject();
    let establishment = root.authority_signing_key(0);
    let (a_key, a) = stranger(0x11);
    let (b_key, b) = stranger(0x22);
    let caps = CapabilitySet::new([DeviceCapability::Sign, DeviceCapability::Present]);
    let auth_a = authorize_event(
        &establishment,
        subject,
        1,
        inception.body.event_id(),
        a,
        caps.clone(),
        None,
    );
    let auth_b = authorize_event(
        &establishment,
        subject,
        2,
        auth_a.body.event_id(),
        b,
        caps,
        None,
    );
    let revoke_a = revoke_event(&establishment, subject, 3, auth_b.body.event_id(), a);
    Fixture {
        subject,
        a_key,
        a,
        b_key,
        b,
        facts: vec![inception, auth_a, auth_b, revoke_a],
    }
}

fn signed(f: &Fixture, device: PrincipalKey, key: &SigningKey, position: u64) -> SignedDeviceAct {
    let act = DeviceActV1::new(
        f.subject,
        device,
        DeviceCapability::Sign,
        position,
        b"katie:open-workspace".to_vec(),
    )
    .unwrap();
    sign_device_act(&act, key).unwrap()
}

fn write_bundle(dir: &Path, facts: &[SignedAuthorityEvent], act: SignedDeviceAct) -> PathBuf {
    let bundle = DeviceAuthorityBundleV1::new(facts.iter().cloned(), act).unwrap();
    let path = dir.join("bundle.bin");
    std::fs::write(&path, bundle.canonical_bytes()).unwrap();
    path
}

/// Run the verb with an empty data directory; returns (exit code, parsed stdout, stderr).
fn run(data_dir: &Path, args: &[&str]) -> (i32, Value, String) {
    let output = Command::new(icnctl())
        .arg("--data-dir")
        .arg(data_dir)
        .arg("device-authority")
        .args(args)
        .output()
        .expect("icnctl runs");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let document: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout is one JSON document ({e}): {stdout:?}\nstderr: {stderr}")
    });
    (output.status.code().unwrap(), document, stderr)
}

#[test]
fn verify_accepts_the_sibling_device_and_prints_the_grant_as_evidence() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path(), &f.facts, signed(&f, f.b, &f.b_key, 3));
    let subject = hex::encode(f.subject.as_bytes());
    let (code, doc, _) = run(
        dir.path(),
        &[
            "verify",
            "--bundle",
            bundle.to_str().unwrap(),
            "--subject",
            &subject,
            "--position",
            "3",
        ],
    );
    assert_eq!(code, 0);
    assert_eq!(doc["verdict"], "authorized");
    assert_eq!(doc["subject"], subject);
    assert_eq!(doc["device"], hex::encode(f.b.as_bytes()));
    assert_eq!(doc["capability"], "Sign");
    assert_eq!(doc["evaluation_position"], 3);
    assert_eq!(doc["grant"]["granted_at"], 2);
    assert_eq!(doc["grant"]["validity"], Value::Null);
    assert_eq!(
        doc["grant"]["capabilities"],
        serde_json::json!(["Sign", "Present"])
    );
    assert_eq!(doc["generation"], 0);
    assert_eq!(doc["fact_count"], 4);
    assert_eq!(doc["act_id"].as_str().unwrap().len(), 64);
    assert!(doc["device_did"].as_str().unwrap().starts_with("did:icn:"));
}

#[test]
fn verify_refuses_the_revoked_device_and_names_the_class() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path(), &f.facts, signed(&f, f.a, &f.a_key, 3));
    let subject = hex::encode(f.subject.as_bytes());
    let (code, doc, _) = run(
        dir.path(),
        &[
            "verify",
            "--bundle",
            bundle.to_str().unwrap(),
            "--subject",
            &subject,
            "--position",
            "3",
        ],
    );
    assert_eq!(code, 1);
    assert_eq!(doc["verdict"], "refused");
    assert_eq!(doc["class"], "DeviceNotAuthorized");
    assert!(doc.get("grant").is_none(), "a refusal carries no grant");
}

#[test]
fn the_caller_is_the_relying_party_the_bundle_chooses_neither_subject_nor_position() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path(), &f.facts, signed(&f, f.b, &f.b_key, 3));
    let other = hex::encode([0x5au8; 32]);
    let (code, doc, _) = run(
        dir.path(),
        &[
            "verify",
            "--bundle",
            bundle.to_str().unwrap(),
            "--subject",
            &other,
            "--position",
            "3",
        ],
    );
    assert_eq!(
        (code, doc["class"].as_str()),
        (1, Some("ActSubjectMismatch"))
    );

    let subject = hex::encode(f.subject.as_bytes());
    let (code, doc, _) = run(
        dir.path(),
        &[
            "verify",
            "--bundle",
            bundle.to_str().unwrap(),
            "--subject",
            &subject,
            "--position",
            "2",
        ],
    );
    assert_eq!(
        (code, doc["class"].as_str()),
        (1, Some("EvaluationPositionMismatch"))
    );
}

#[test]
fn inspect_describes_the_facts_and_the_act_without_deciding() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let bundle = write_bundle(dir.path(), &f.facts, signed(&f, f.a, &f.a_key, 3));
    let (code, doc, _) = run(
        dir.path(),
        &["inspect", "--bundle", bundle.to_str().unwrap()],
    );
    assert_eq!(
        code, 0,
        "inspect never refuses a well-formed bundle, even one that would fail verification"
    );
    assert_eq!(doc["fact_count"], 4);
    let kinds: Vec<u64> = doc["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind_tag"].as_u64().unwrap())
        .collect();
    assert!(kinds.contains(&0x01) && kinds.contains(&0x03) && kinds.contains(&0x04));
    assert_eq!(doc["act"]["subject"], hex::encode(f.subject.as_bytes()));
    assert_eq!(doc["act"]["capability"], "Sign");
    assert_eq!(doc["act"]["evaluation_position"], 3);
    assert!(doc.get("verdict").is_none());
}

#[test]
fn malformed_bytes_and_a_bad_subject_argument_exit_2() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let garbage = dir.path().join("garbage.bin");
    std::fs::write(&garbage, b"not a bundle").unwrap();
    let subject = hex::encode(f.subject.as_bytes());
    let (code, doc, _) = run(
        dir.path(),
        &[
            "verify",
            "--bundle",
            garbage.to_str().unwrap(),
            "--subject",
            &subject,
            "--position",
            "3",
        ],
    );
    assert_eq!((code, doc["verdict"].as_str()), (2, Some("malformed")));

    let bundle = write_bundle(dir.path(), &f.facts, signed(&f, f.b, &f.b_key, 3));
    let (code, doc, _) = run(
        dir.path(),
        &[
            "verify",
            "--bundle",
            bundle.to_str().unwrap(),
            "--subject",
            "beef",
            "--position",
            "3",
        ],
    );
    assert_eq!((code, doc["verdict"].as_str()), (2, Some("malformed")));
}
