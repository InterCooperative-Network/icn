//! Integration tests for QR code functionality in device pairing
//!
//! Tests verify:
//! - QR code generation with --qr flag
//! - PNG image export with --qr-image flag
//! - Size threshold warnings
//! - Edge cases (special characters in device names)
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Get the path to the icnctl binary (set by Cargo for integration tests)
fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// Extract DID from `icnctl id show` output
fn extract_did(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.contains("did:icn:") {
            // Find the DID in the line
            if let Some(start) = line.find("did:icn:") {
                let did_part = &line[start..];
                // DID ends at whitespace or end of line
                let end = did_part
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(did_part.len());
                return Some(did_part[..end].to_string());
            }
        }
    }
    None
}

/// Initialize identity and return the DID
fn init_identity(data_dir: &std::path::Path, passphrase: &str) -> Result<String> {
    // Initialize identity
    let init_output = Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("id")
        .arg("init")
        .output()?;

    assert!(
        init_output.status.success(),
        "Failed to init identity: {}",
        String::from_utf8_lossy(&init_output.stderr)
    );

    // Get the DID
    let show_output = Command::new(icnctl_bin())
        .env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("id")
        .arg("show")
        .output()?;

    assert!(
        show_output.status.success(),
        "Failed to show identity: {}",
        String::from_utf8_lossy(&show_output.stderr)
    );

    let output_str = String::from_utf8_lossy(&show_output.stdout);
    extract_did(&output_str)
        .ok_or_else(|| anyhow::anyhow!("Could not extract DID from output: {}", output_str))
}

/// Run device add command with DID provided via stdin
fn run_device_add(
    data_dir: &std::path::Path,
    passphrase: &str,
    device_name: &str,
    did: &str,
    qr: bool,
    qr_image: Option<&std::path::Path>,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(icnctl_bin());
    cmd.env("ICN_PASSPHRASE", passphrase)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("device")
        .arg("add")
        .arg(device_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if qr {
        cmd.arg("--qr");
    }

    if let Some(path) = qr_image {
        cmd.arg("--qr-image").arg(path);
    }

    let mut child = cmd.spawn()?;

    // Write DID to stdin
    if let Some(stdin) = child.stdin.as_mut() {
        writeln!(stdin, "{}", did)?;
    }

    Ok(child.wait_with_output()?)
}

#[test]
fn test_device_add_with_qr_flag() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let passphrase = "test_qr_code_123";

    let did = init_identity(&data_dir, passphrase)?;

    // Create device-add request with QR flag
    let output = run_device_add(&data_dir, passphrase, "Test Phone", &did, true, None)?;

    assert!(
        output.status.success(),
        "device add --qr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify QR code was generated
    assert!(
        stdout.contains("QR CODE"),
        "Output should contain QR code header"
    );
    assert!(
        stdout.contains("QR code data size:"),
        "Output should show QR data size"
    );
    assert!(
        stdout.contains("SECURITY WARNING"),
        "Output should contain security warning"
    );

    // Verify request file was still created
    let request_file = data_dir.join("device-add-test-phone.json");
    assert!(
        request_file.exists(),
        "Request file should be created alongside QR"
    );

    Ok(())
}

#[test]
fn test_device_add_with_qr_image_export() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let image_path = temp_dir.path().join("qr-code.png");
    let passphrase = "test_qr_image_456";

    let did = init_identity(&data_dir, passphrase)?;

    // Create device-add request with QR image export
    let output = run_device_add(
        &data_dir,
        passphrase,
        "Test Laptop",
        &did,
        false,
        Some(&image_path),
    )?;

    assert!(
        output.status.success(),
        "device add --qr-image failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify PNG file was created
    assert!(image_path.exists(), "QR code PNG should be created");

    // Verify output mentions the saved image
    assert!(
        stdout.contains("QR code image saved"),
        "Output should confirm image was saved"
    );

    // Verify PNG file has reasonable size (should be a few KB)
    let metadata = std::fs::metadata(&image_path)?;
    assert!(
        metadata.len() > 100,
        "PNG file should have non-trivial size"
    );
    assert!(
        metadata.len() < 100_000,
        "PNG file should not be excessively large"
    );

    // Verify it's a valid PNG (check magic bytes)
    let contents = std::fs::read(&image_path)?;
    assert!(
        contents.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
        "File should be a valid PNG"
    );

    Ok(())
}

#[test]
fn test_device_add_with_both_qr_and_image() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let image_path = temp_dir.path().join("both-qr.png");
    let passphrase = "test_qr_both_789";

    let did = init_identity(&data_dir, passphrase)?;

    // Create device-add request with both --qr and --qr-image
    let output = run_device_add(
        &data_dir,
        passphrase,
        "Test Tablet",
        &did,
        true,
        Some(&image_path),
    )?;

    assert!(
        output.status.success(),
        "device add --qr --qr-image failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify both terminal QR and image were generated
    assert!(stdout.contains("QR CODE"), "Should display QR in terminal");
    assert!(image_path.exists(), "Should also create PNG file");
    assert!(
        stdout.contains("QR code image saved"),
        "Should confirm image saved"
    );

    Ok(())
}

#[test]
fn test_device_add_special_characters_in_name() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let passphrase = "test_qr_special_chars";

    let did = init_identity(&data_dir, passphrase)?;

    // Test with special characters in device name
    let output = run_device_add(
        &data_dir,
        passphrase,
        "John's iPhone (2024)",
        &did,
        true,
        None,
    )?;

    assert!(
        output.status.success(),
        "device add with special chars failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify QR code was generated successfully
    assert!(
        stdout.contains("QR CODE"),
        "Should generate QR for name with special chars"
    );

    // Verify request file was created with sanitized name
    let request_file = data_dir.join("device-add-john's-iphone-(2024).json");
    assert!(
        request_file.exists(),
        "Request file should be created with sanitized name"
    );

    Ok(())
}

#[test]
fn test_device_add_qr_data_size_display() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let passphrase = "test_qr_size_display";

    let did = init_identity(&data_dir, passphrase)?;

    // Create device-add request with QR flag
    let output = run_device_add(&data_dir, passphrase, "Size Test Device", &did, true, None)?;

    assert!(
        output.status.success(),
        "device add --qr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify size is displayed and within expected range
    // Typical QR data is ~280-350 bytes
    assert!(
        stdout.contains("QR code data size:"),
        "Should display QR data size"
    );
    assert!(stdout.contains("bytes"), "Size should be in bytes");

    // Should NOT show size warning for normal-sized data
    assert!(
        !stdout.contains("Warning: QR code data is large"),
        "Normal-sized QR should not show size warning"
    );

    Ok(())
}

#[test]
fn test_device_add_without_qr_flag() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("data");
    let passphrase = "test_no_qr_flag";

    let did = init_identity(&data_dir, passphrase)?;

    // Create device-add request WITHOUT QR flag
    let output = run_device_add(&data_dir, passphrase, "No QR Device", &did, false, None)?;

    assert!(
        output.status.success(),
        "device add without --qr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify QR code was NOT generated
    assert!(
        !stdout.contains("QR CODE"),
        "Should NOT display QR without flag"
    );
    assert!(
        !stdout.contains("QR code data size:"),
        "Should NOT show QR data size"
    );

    // Verify request file was still created
    let request_file = data_dir.join("device-add-no-qr-device.json");
    assert!(
        request_file.exists(),
        "Request file should still be created"
    );

    // Verify file-based instructions are shown
    assert!(
        stdout.contains("Transfer") || stdout.contains("device approve"),
        "Should show file transfer instructions"
    );

    Ok(())
}
