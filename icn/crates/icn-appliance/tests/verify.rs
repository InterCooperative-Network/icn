#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use icn_appliance::{
    sha256_file_hex, ApplianceError, ApplianceManifest, BinaryRecord, MANIFEST_VERSION,
};
use tempfile::TempDir;

/// Build a temp tree (image, base, one binary) and a manifest whose recorded
/// hashes match those files. Returns the dir (keep it alive) and the manifest.
fn fixture() -> (TempDir, ApplianceManifest) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(root.join("image.qcow2"), b"image-bytes").unwrap();
    fs::write(root.join("base.qcow2"), b"base-bytes").unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    fs::write(root.join("bin/icnd"), b"icnd-bytes").unwrap();

    let manifest = ApplianceManifest {
        manifest_version: MANIFEST_VERSION,
        appliance_id: "icn-appliance-dev".to_string(),
        version: "0.1.0-test".to_string(),
        arch: "amd64".to_string(),
        image_format: "qcow2".to_string(),
        image_path: "image.qcow2".to_string(),
        image_sha256: sha256_file_hex(root.join("image.qcow2")).unwrap(),
        base_image_path: "base.qcow2".to_string(),
        base_image_sha256: sha256_file_hex(root.join("base.qcow2")).unwrap(),
        git_commit: "deadbeef".to_string(),
        build_timestamp_utc: "2026-06-30T00:00:00Z".to_string(),
        built_binaries: vec![BinaryRecord {
            path: "/usr/local/bin/icnd".to_string(),
            source: "bin/icnd".to_string(),
            sha256: sha256_file_hex(root.join("bin/icnd")).unwrap(),
        }],
        non_production: true,
        signed: false,
        immutable: false,
        demo_profile: false,
    };
    (dir, manifest)
}

#[test]
fn verify_ok_for_matching_artifacts() {
    let (dir, m) = fixture();
    let r = m.verify(dir.path());
    assert!(r.is_ok(), "expected Ok, got {r:?}");
}

#[test]
fn verify_rejects_image_hash_mismatch() {
    let (dir, mut m) = fixture();
    m.image_sha256 = "00".repeat(32);
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::HashMismatch { .. })
    ));
}

#[test]
fn verify_rejects_base_hash_mismatch() {
    let (dir, mut m) = fixture();
    m.base_image_sha256 = "00".repeat(32);
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::HashMismatch { .. })
    ));
}

#[test]
fn verify_rejects_binary_hash_mismatch() {
    let (dir, mut m) = fixture();
    m.built_binaries[0].sha256 = "00".repeat(32);
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::HashMismatch { .. })
    ));
}

#[test]
fn verify_rejects_missing_image() {
    let (dir, mut m) = fixture();
    m.image_path = "does-not-exist.qcow2".to_string();
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::MissingArtifact { .. })
    ));
}

#[test]
fn verify_rejects_missing_binary() {
    let (dir, mut m) = fixture();
    m.built_binaries[0].source = "bin/missing".to_string();
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::MissingArtifact { .. })
    ));
}

#[test]
fn verify_rejects_unsupported_version() {
    let (dir, mut m) = fixture();
    m.manifest_version = MANIFEST_VERSION + 1;
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::UnsupportedVersion { .. })
    ));
}

#[test]
fn verify_rejects_empty_binaries() {
    let (dir, mut m) = fixture();
    m.built_binaries.clear();
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::EmptyBinaries)
    ));
}

#[test]
fn verify_rejects_production_without_signed_or_immutable() {
    let (dir, mut m) = fixture();
    // non_production: false while signed=false and immutable=false.
    m.non_production = false;
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::PostureContradiction { .. })
    ));
}

#[test]
fn verify_rejects_demo_production() {
    let (dir, mut m) = fixture();
    // A demo image cannot claim production posture.
    m.non_production = false;
    m.signed = true;
    m.immutable = true;
    m.demo_profile = true;
    assert!(matches!(
        m.verify(dir.path()),
        Err(ApplianceError::PostureContradiction { .. })
    ));
}

#[test]
fn check_posture_ok_for_coherent_production() {
    let (_dir, mut m) = fixture();
    m.non_production = false;
    m.signed = true;
    m.immutable = true;
    m.demo_profile = false;
    assert!(m.check_posture().is_ok());
}
