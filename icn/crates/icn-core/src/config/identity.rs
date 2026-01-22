//! Identity and keystore backend configuration

use serde::{Deserialize, Serialize};

/// Identity configuration (backend selection and options)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityConfig {
    /// Backend type: "age" (default), "pkcs11", or "tpm"
    #[serde(default = "default_backend")]
    pub backend: String,

    /// PKCS#11 HSM configuration (only used when backend = "pkcs11")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkcs11: Option<Pkcs11BackendConfig>,

    /// TPM 2.0 configuration (only used when backend = "tpm")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm: Option<TpmBackendConfig>,
}

fn default_backend() -> String {
    "age".to_string()
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            pkcs11: None,
            tpm: None,
        }
    }
}

impl IdentityConfig {
    /// Validate the identity configuration
    ///
    /// Returns Ok(warnings) for non-fatal issues,
    /// Err(errors) for fatal configuration problems.
    pub fn validate(&self) -> Result<Vec<String>, Vec<String>> {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        match self.backend.as_str() {
            "age" => {
                // Age backend is always available, no additional config needed
            }
            "pkcs11" => {
                if self.pkcs11.is_none() {
                    errors.push(
                        "identity.backend=pkcs11 requires [identity.pkcs11] config section"
                            .to_string(),
                    );
                } else if let Some(ref pkcs11_config) = self.pkcs11 {
                    validate_pkcs11(pkcs11_config, &mut warnings, &mut errors);
                }
                warnings.push(
                    "PKCS#11 backend is scaffolding only and not yet functional. \
                     IdentityBundle must be hardware-keyed before enabling."
                        .to_string(),
                );
            }
            "tpm" => {
                if self.tpm.is_none() {
                    errors.push(
                        "identity.backend=tpm requires [identity.tpm] config section".to_string(),
                    );
                } else if let Some(ref tpm_config) = self.tpm {
                    validate_tpm(tpm_config, &mut warnings, &mut errors);
                }
                warnings.push(
                    "TPM backend is scaffolding only and not yet functional. \
                     Key sealing/unsealing + hardware-keyed IdentityBundle required."
                        .to_string(),
                );
            }
            unknown => {
                errors.push(format!(
                    "Unknown identity backend '{}' (valid: age, pkcs11, tpm)",
                    unknown
                ));
            }
        }

        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }
    }
}

/// PKCS#11 HSM backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pkcs11BackendConfig {
    /// Path to PKCS#11 library (e.g., /usr/lib/softhsm/libsofthsm2.so)
    pub library_path: String,

    /// HSM slot ID
    pub slot_id: u64,

    /// Key label for finding the keypair
    #[serde(default = "default_key_label")]
    pub key_label: String,

    /// Optional token label filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_label: Option<String>,
}

fn default_key_label() -> String {
    "icn-identity".to_string()
}

fn validate_pkcs11(
    cfg: &Pkcs11BackendConfig,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if cfg.library_path.trim().is_empty() {
        errors.push("identity.pkcs11.library_path cannot be empty".to_string());
    }
    if cfg.key_label.trim().is_empty() {
        errors.push("identity.pkcs11.key_label cannot be empty".to_string());
    }
    // Optional gentle warning for unusually large slot IDs
    if cfg.slot_id > 1_000_000 {
        warnings.push("identity.pkcs11.slot_id looks unusually large".to_string());
    }
}

/// TPM 2.0 backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmBackendConfig {
    /// TPM device path (e.g., /dev/tpmrm0 or /dev/tpm0)
    #[serde(default = "default_tpm_device")]
    pub device_path: String,

    /// Persistent handle for the key (hex format)
    #[serde(default = "default_key_handle")]
    pub key_handle: u32,

    /// Enable platform binding (seal key to PCRs)
    #[serde(default)]
    pub platform_binding: bool,

    /// Enable attestation support
    #[serde(default)]
    pub attestation: bool,
}

fn default_tpm_device() -> String {
    "/dev/tpmrm0".to_string()
}

fn default_key_handle() -> u32 {
    0x8100_0000 // Default persistent handle in user range
}

#[allow(clippy::ptr_arg)] // Consistent signature with validate_pkcs11, may use warnings in future
fn validate_tpm(cfg: &TpmBackendConfig, _warnings: &mut Vec<String>, errors: &mut Vec<String>) {
    if cfg.device_path.trim().is_empty() {
        errors.push("identity.tpm.device_path cannot be empty".to_string());
    }
    // Handle 0 is almost certainly wrong
    if cfg.key_handle == 0 {
        errors.push("identity.tpm.key_handle cannot be 0".to_string());
    }
    // Validate key handle is in persistent range (0x81000000-0x81FFFFFF)
    if cfg.key_handle < 0x8100_0000 || cfg.key_handle > 0x81FF_FFFF {
        errors.push(format!(
            "Invalid key_handle {:#x} (must be in range 0x81000000-0x81FFFFFF)",
            cfg.key_handle
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = IdentityConfig::default();
        assert_eq!(config.backend, "age");
        assert!(config.pkcs11.is_none());
        assert!(config.tpm.is_none());
    }

    #[test]
    fn test_age_backend_validation() {
        let config = IdentityConfig {
            backend: "age".to_string(),
            pkcs11: None,
            tpm: None,
        };

        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_backend() {
        let config = IdentityConfig {
            backend: "invalid".to_string(),
            pkcs11: None,
            tpm: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.contains("Unknown identity backend")));
    }

    #[test]
    fn test_pkcs11_missing_config() {
        let config = IdentityConfig {
            backend: "pkcs11".to_string(),
            pkcs11: None,
            tpm: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("identity.pkcs11")));
    }

    #[test]
    fn test_pkcs11_validation() {
        let config = IdentityConfig {
            backend: "pkcs11".to_string(),
            pkcs11: Some(Pkcs11BackendConfig {
                library_path: "/usr/lib/libsofthsm2.so".to_string(),
                slot_id: 0,
                key_label: "icn-key".to_string(),
                token_label: None,
            }),
            tpm: None,
        };

        let result = config.validate();
        // Should succeed with warnings
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings.iter().any(|w| w.contains("scaffolding")));
    }

    #[test]
    fn test_tpm_validation() {
        let config = IdentityConfig {
            backend: "tpm".to_string(),
            pkcs11: None,
            tpm: Some(TpmBackendConfig {
                device_path: "/dev/tpmrm0".to_string(),
                key_handle: 0x8100_0000,
                platform_binding: true,
                attestation: false,
            }),
        };

        let result = config.validate();
        // Should succeed with warnings
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings.iter().any(|w| w.contains("scaffolding")));
    }

    #[test]
    fn test_tpm_handle_validation() {
        let config = TpmBackendConfig {
            device_path: "/dev/tpmrm0".to_string(),
            key_handle: 0x1234, // Invalid handle (not in persistent range)
            platform_binding: true,
            attestation: false,
        };

        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        validate_tpm(&config, &mut warnings, &mut errors);

        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e: &String| e.contains("Invalid key_handle")));
    }
}
