#![allow(clippy::expect_used, clippy::unwrap_used)]

use icn_appliance::hash::sha256_hex;
use icn_appliance::{ApplianceManifest, BinaryRecord, MANIFEST_VERSION};

#[test]
fn sha256_hex_matches_known_vector() {
    // NIST/standard SHA-256 of the ASCII string "abc".
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

fn sample() -> ApplianceManifest {
    ApplianceManifest {
        manifest_version: MANIFEST_VERSION,
        appliance_id: "icn-appliance-dev".to_string(),
        version: "0.1.0-test".to_string(),
        arch: "amd64".to_string(),
        image_format: "qcow2".to_string(),
        image_path: "/out/icn-appliance.qcow2".to_string(),
        image_sha256: "aa".repeat(32),
        base_image_path: "/base/debian-12-genericcloud-amd64.qcow2".to_string(),
        base_image_sha256: "bb".repeat(32),
        git_commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
        build_timestamp_utc: "2026-06-30T00:00:00Z".to_string(),
        built_binaries: vec![
            BinaryRecord {
                path: "/usr/local/bin/icnd".to_string(),
                source: "icn/target/release/icnd".to_string(),
                sha256: "cc".repeat(32),
            },
            BinaryRecord {
                path: "/usr/local/bin/icnctl".to_string(),
                source: "icn/target/release/icnctl".to_string(),
                sha256: "dd".repeat(32),
            },
        ],
        non_production: true,
        signed: false,
        immutable: false,
        demo_profile: false,
    }
}

#[test]
fn manifest_json_round_trips() {
    let m = sample();
    let json = m.to_json_pretty().expect("serialize");
    let back = ApplianceManifest::from_json_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn manifest_records_schema_version_one() {
    assert_eq!(MANIFEST_VERSION, 1);
    let json = sample().to_json_pretty().expect("serialize");
    assert!(
        json.contains("\"manifest_version\": 1"),
        "json was:\n{json}"
    );
}

#[test]
fn manifest_rejects_unknown_fields() {
    // A field the wire-stable contract does not define must be rejected, not
    // silently ignored — emitter/consumer drift is an error, not a no-op.
    let json = sample().to_json_pretty().expect("serialize");
    let tampered = json.replacen(
        "\"manifest_version\": 1",
        "\"manifest_version\": 1,\n  \"surprise_field\": true",
        1,
    );
    assert!(
        ApplianceManifest::from_json_str(&tampered).is_err(),
        "unknown fields must be rejected, not silently ignored"
    );
}
