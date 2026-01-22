//! TPM Identity Example
//!
//! This example demonstrates how to use the TPM backend to create and manage
//! ICN identities with hardware-backed key storage.
//!
//! # Requirements
//!
//! - TPM 2.0 hardware or swtpm simulator
//! - Build with `--features tpm-experimental`
//!
//! # Usage
//!
//! ```bash
//! # Start swtpm (optional, for testing)
//! mkdir -p /tmp/tpm-state
//! swtpm socket --tpmstate dir=/tmp/tpm-state --tpm2 \
//!     --ctrl type=tcp,port=2322 \
//!     --server type=tcp,port=2321 \
//!     --flags not-need-init
//!
//! # Run the example
//! cargo run --example tpm_identity --features tpm-experimental
//! ```

#[cfg(feature = "tpm-experimental")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ed25519_dalek::Verifier;
    use icn_identity::{KeyStoreBackend, TpmBackend, TpmConfig};

    println!("=== ICN TPM Identity Example ===\n");

    // Configure TPM backend
    let config = TpmConfig {
        device_path: "/dev/tpmrm0".to_string(), // or "swtpm:host=localhost,port=2321"
        key_handle: 0x81000100,
        platform_binding: false, // Set to true for PCR binding (when implemented)
        attestation: false,
    };

    println!("1. Creating TPM backend...");
    println!("   Device: {}", config.device_path);
    println!("   Key Handle: {:#x}", config.key_handle);
    println!("   Platform Binding: {}", config.platform_binding);

    let mut backend = TpmBackend::new(config)?;
    println!("   ✓ Backend created\n");

    // Initialize new identity
    println!("2. Initializing new identity...");
    let bundle = backend.init(&[])?;

    println!("   DID: {}", bundle.did());
    println!("   Hardware-backed: {}", backend.is_hardware_backed());
    println!("   Backend type: {}", backend.backend_type());
    println!("   ✓ Identity initialized\n");

    // Sign a message
    println!("3. Signing a message...");
    let message = b"Hello from ICN with TPM!";
    println!("   Message: {}", String::from_utf8_lossy(message));

    let signature = bundle.sign(message)?;
    println!("   ✓ Message signed\n");

    // Verify signature
    println!("4. Verifying signature...");
    let verifying_key = bundle.did_key().verifying_key();

    match verifying_key.verify(message, &signature) {
        Ok(_) => println!("   ✓ Signature verified successfully\n"),
        Err(e) => println!("   ✗ Signature verification failed: {}\n", e),
    }

    // Lock the backend
    println!("5. Locking backend...");
    backend.lock();
    println!("   Is locked: {}", backend.is_locked());
    println!("   ✓ Backend locked\n");

    // Unlock the backend
    println!("6. Unlocking backend...");
    backend.unlock(&[])?;
    println!("   Is locked: {}", backend.is_locked());
    println!("   ✓ Backend unlocked\n");

    // Sign another message after unlock
    println!("7. Signing after unlock...");
    let unlocked_bundle = backend.get_identity_bundle()?;
    let message2 = b"Second message after unlock";
    let signature2 = unlocked_bundle.sign(message2)?;

    let verifying_key2 = unlocked_bundle.did_key().verifying_key();
    match verifying_key2.verify(message2, &signature2) {
        Ok(_) => println!("   ✓ Second signature verified successfully\n"),
        Err(e) => println!("   ✗ Second signature verification failed: {}\n", e),
    }

    println!("=== Example completed successfully ===\n");

    println!("Note: This is a placeholder implementation.");
    println!("Real TPM sealing will be implemented in Phase 2.");
    println!("See docs/tpm-setup.md for more information.");

    Ok(())
}

#[cfg(not(feature = "tpm-experimental"))]
fn main() {
    eprintln!("This example requires the 'tpm-experimental' feature.");
    eprintln!("Build with: cargo run --example tpm_identity --features tpm-experimental");
    std::process::exit(1);
}
