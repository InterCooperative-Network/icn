//! Integration tests for backup and restore functionality
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get the path to the icnctl binary
fn icnctl_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../target/debug/icnctl");
    path
}

#[test]
fn test_backup_and_restore_roundtrip() -> Result<()> {
    use std::sync::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    let _lock = TEST_LOCK.lock().unwrap(); // Serialize this test

    // Create temporary directories for test
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let backup_file = temp_dir.path().join("backup.tar");
    let restore_dir = temp_dir.path().join("restore");

    // Use unique passphrase for this test
    let passphrase = "test_backup_123_unique";

    // Initialize identity
    let output = Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("id")
        .arg("init")
        .output()?;

    assert!(
        output.status.success(),
        "Failed to init identity: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Get the original DID
    let show_output = Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("id")
        .arg("show")
        .output()?;

    assert!(
        show_output.status.success(),
        "Failed to show identity: stderr={}",
        String::from_utf8_lossy(&show_output.stderr)
    );
    let original_output = String::from_utf8_lossy(&show_output.stdout);
    let original_did = extract_did(&original_output).expect("Failed to extract DID");

    // Create backup
    let backup_output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("backup")
        .arg(&backup_file)
        .output()?;

    assert!(
        backup_output.status.success(),
        "Failed to create backup: {}",
        String::from_utf8_lossy(&backup_output.stderr)
    );
    assert!(backup_file.exists(), "Backup file was not created");

    // Verify backup contains expected files
    let tar_list = Command::new("tar").arg("-tf").arg(&backup_file).output()?;

    assert!(
        tar_list.status.success(),
        "Failed to list backup contents: {}",
        String::from_utf8_lossy(&tar_list.stderr)
    );

    let tar_contents = String::from_utf8_lossy(&tar_list.stdout);
    assert!(
        tar_contents.contains("identity.age"),
        "Backup missing identity.age"
    );
    assert!(
        tar_contents.contains("backup_metadata.json"),
        "Backup missing metadata"
    );

    // Restore to different directory
    let restore_output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&restore_dir)
        .arg("restore")
        .arg(&backup_file)
        .output()?;

    assert!(
        restore_output.status.success(),
        "Failed to restore backup: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );
    assert!(
        restore_dir.join("identity.age").exists(),
        "Restored identity.age not found"
    );

    // Verify restored identity matches
    let restored_show = Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&restore_dir)
        .arg("id")
        .arg("show")
        .output()?;

    assert!(
        restored_show.status.success(),
        "Failed to show restored identity"
    );
    let restored_output = String::from_utf8_lossy(&restored_show.stdout);
    let restored_did = extract_did(&restored_output).expect("Failed to extract restored DID");

    assert_eq!(
        original_did, restored_did,
        "Restored DID does not match original"
    );

    Ok(())
}

#[test]
fn test_backup_nonexistent_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let nonexistent_dir = temp_dir.path().join("nonexistent");
    let backup_file = temp_dir.path().join("backup.tar");

    let output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&nonexistent_dir)
        .arg("backup")
        .arg(&backup_file)
        .output()?;

    assert!(
        !output.status.success(),
        "Backup should fail for nonexistent directory"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist"),
        "Error message should mention directory does not exist"
    );

    Ok(())
}

#[test]
fn test_restore_without_force_fails() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let backup_file = temp_dir.path().join("backup.tar");

    // Set test passphrase
    let passphrase = "test_force_123";

    // Initialize identity and create backup
    Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("id")
        .arg("init")
        .output()?;

    Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("backup")
        .arg(&backup_file)
        .output()?;

    // Try to restore without --force (should fail since data_dir exists)
    let restore_output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("restore")
        .arg(&backup_file)
        .output()?;

    assert!(
        !restore_output.status.success(),
        "Restore should fail without --force"
    );
    let stderr = String::from_utf8_lossy(&restore_output.stderr);
    assert!(
        stderr.contains("already exists") || stderr.contains("--force"),
        "Error should mention directory exists or --force flag"
    );

    Ok(())
}

#[test]
fn test_restore_with_force_creates_backup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let backup_file = temp_dir.path().join("backup.tar");

    // Set test passphrase
    let passphrase = "test_force_backup_123";

    // Initialize identity and create backup
    Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("id")
        .arg("init")
        .output()?;

    Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("backup")
        .arg(&backup_file)
        .output()?;

    // Count files in parent directory before restore
    let parent_entries_before: Vec<_> = fs::read_dir(temp_dir.path())?
        .filter_map(|e| e.ok())
        .collect();

    // Restore with --force
    let restore_output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("restore")
        .arg(&backup_file)
        .arg("--force")
        .output()?;

    assert!(
        restore_output.status.success(),
        "Restore with --force should succeed: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );

    // Check that a backup directory was created
    let parent_entries_after: Vec<_> = fs::read_dir(temp_dir.path())?
        .filter_map(|e| e.ok())
        .collect();

    assert!(
        parent_entries_after.len() > parent_entries_before.len(),
        "A backup directory should have been created"
    );

    // Verify a .backup-* directory exists
    let backup_dirs: Vec<_> = parent_entries_after
        .iter()
        .filter(|e| e.file_name().to_string_lossy().starts_with("data.backup-"))
        .collect();

    assert!(
        !backup_dirs.is_empty(),
        "Should have created a .backup-* directory"
    );

    Ok(())
}

#[test]
fn test_backup_includes_state_snapshot() -> Result<()> {
    use std::sync::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    let _lock = TEST_LOCK.lock().unwrap();

    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let backup_file = temp_dir.path().join("backup.tar");

    // Create data directory
    fs::create_dir_all(&data_dir)?;

    // Create a mock state.snapshot file
    let snapshot_path = data_dir.join("state.snapshot");
    fs::write(
        &snapshot_path,
        r#"{"version":1,"created_at":1234567890,"gossip_state":null,"network_state":null}"#,
    )?;

    // Also create identity.age so backup doesn't fail
    let identity_path = data_dir.join("identity.age");
    fs::write(&identity_path, "mock_encrypted_keystore")?;

    // Create backup
    let backup_output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("backup")
        .arg(&backup_file)
        .output()?;

    assert!(
        backup_output.status.success(),
        "Failed to create backup: {}",
        String::from_utf8_lossy(&backup_output.stderr)
    );

    // Verify backup contains state.snapshot
    let tar_list = Command::new("tar").arg("-tf").arg(&backup_file).output()?;

    assert!(
        tar_list.status.success(),
        "Failed to list backup contents: {}",
        String::from_utf8_lossy(&tar_list.stderr)
    );

    let tar_contents = String::from_utf8_lossy(&tar_list.stdout);
    assert!(
        tar_contents.contains("state.snapshot"),
        "Backup missing state.snapshot file. Contents:\n{tar_contents}"
    );

    // Restore to different directory
    let restore_dir = temp_dir.path().join("restore");
    let restore_output = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(&restore_dir)
        .arg("restore")
        .arg(&backup_file)
        .output()?;

    assert!(
        restore_output.status.success(),
        "Failed to restore backup: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );

    // Verify state.snapshot was restored
    let restored_snapshot = restore_dir.join("state.snapshot");
    assert!(
        restored_snapshot.exists(),
        "state.snapshot was not restored"
    );

    // Verify content matches
    let restored_content = fs::read_to_string(&restored_snapshot)?;
    assert!(
        restored_content.contains("\"version\":1"),
        "Restored snapshot content doesn't match"
    );

    Ok(())
}

/// Extract DID from icnctl output
fn extract_did(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.trim().starts_with("DID:") {
            return line.split(':').nth(1).map(|s| s.trim().to_string());
        }
    }
    None
}
