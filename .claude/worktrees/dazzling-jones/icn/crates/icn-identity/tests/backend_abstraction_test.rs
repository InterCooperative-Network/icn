//! Integration tests for backend abstraction layer

use icn_identity::{AgeKeyStore, BackendConfig, KeyStore, KeyStoreBackend};
use tempfile::tempdir;

#[test]
fn test_backend_config_types() {
    // Test Age backend config
    let age_config = BackendConfig::Age {
        path: "/tmp/keystore.age".to_string(),
    };

    assert_eq!(age_config.backend_type(), "age");
    assert!(!age_config.is_hardware());

    // Test HSM backend config (only when feature is enabled)
    #[cfg(feature = "hsm")]
    {
        use icn_identity::Pkcs11Config;

        let hsm_config = BackendConfig::Pkcs11(Pkcs11Config {
            library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
            slot_id: 0,
            key_label: "test-key".to_string(),
            token_label: None,
        });

        assert_eq!(hsm_config.backend_type(), "pkcs11");
        assert!(hsm_config.is_hardware());
    }

    // Test TPM backend config (only when feature is enabled)
    #[cfg(feature = "tpm-experimental")]
    {
        use icn_identity::TpmConfig;

        let tpm_config = BackendConfig::Tpm(TpmConfig {
            device_path: "/dev/tpmrm0".to_string(),
            key_handle: 0x81000001,
            platform_binding: true,
            attestation: true,
            sealed_blob_dir: None,
        });

        assert_eq!(tpm_config.backend_type(), "tpm");
        assert!(tpm_config.is_hardware());
    }
}

#[test]
fn test_backend_abstraction() {
    // This test verifies that the backend trait compiles
    // Actual functionality requires hardware/simulator

    #[allow(dead_code)]
    fn accept_backend<T: KeyStoreBackend>(_backend: T) {
        // Type check only
    }

    // Age backend should implement the trait
    // (actual construction would require existing files)

    // HSM backend should implement the trait (when feature is enabled)
    #[cfg(feature = "hsm")]
    {
        // Would need actual HSM to test
        // accept_backend(Pkcs11Backend::new(...));
    }

    // TPM backend should implement the trait (when feature is enabled)
    #[cfg(feature = "tpm-experimental")]
    {
        // Would need actual TPM to test
        // accept_backend(TpmBackend::new(...));
    }
}

#[test]
#[allow(clippy::unwrap_used)]
fn test_age_keystore_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("keypair.age");
    let passphrase = b"test-passphrase";

    let mut keystore = AgeKeyStore::init(&path, passphrase).unwrap();
    let did = keystore.get_keypair().unwrap().did().clone();

    keystore.lock();
    let mut reopened = AgeKeyStore::open(&path).unwrap();
    assert!(reopened.is_locked());

    reopened.unlock(passphrase).unwrap();
    assert_eq!(reopened.get_keypair().unwrap().did(), &did);
}
