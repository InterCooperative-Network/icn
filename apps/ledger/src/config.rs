//! Configuration conversion for ledger services
//!
//! Provides conversion functions that take primitive config values and produce
//! `icn_ledger` config types. This keeps `icn-core` config structs free of
//! `icn_ledger` imports — the mapping from config fields to primitives lives
//! in `icnd` (the daemon binary), not here.

use icn_identity::Did;
use std::collections::HashMap;

// Re-export from icn-ledger so icnd and drift-guard tests can reference it.
pub use icn_ledger::oracle::DEFAULT_SUSPICIOUS_RATE_THRESHOLD;

/// Error when converting witness settings to WitnessConfig
#[derive(Debug, Clone, thiserror::Error)]
pub enum WitnessConfigError {
    /// Invalid policy string
    #[error(
        "Invalid witness policy '{0}'. Valid options: none, counterparty, quorum, all_parties"
    )]
    InvalidPolicy(String),

    /// Missing required field for quorum policy
    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    /// Invalid DID format
    #[error("Invalid DID '{0}': {1}")]
    InvalidDid(String, String),

    /// Duplicate witness in quorum configuration
    #[error("Duplicate witness DID in quorum configuration: '{0}'")]
    DuplicateWitness(String),

    /// Quorum requires more signatures than available witnesses
    #[error("Quorum requires {required} signatures but only {available} witnesses configured")]
    QuorumTooLarge { required: u32, available: usize },
}

/// Build a [`icn_ledger::WitnessConfig`] from primitive config values.
///
/// This is the single place that interprets witness policy strings and
/// constructs the domain type. All parameters are primitives so the caller
/// (icnd) does not need to depend on `icn_ledger`.
pub fn build_witness_config(
    default_policy: &str,
    threshold: Option<u64>,
    quorum_required: Option<u32>,
    quorum_witnesses: Option<&[String]>,
    collection_timeout_secs: u64,
    min_witness_trust: Option<f64>,
) -> Result<icn_ledger::WitnessConfig, WitnessConfigError> {
    let policy = match default_policy.to_lowercase().as_str() {
        "none" => icn_ledger::WitnessPolicy::None,
        "counterparty" => icn_ledger::WitnessPolicy::Counterparty,
        "all_parties" | "allparties" => icn_ledger::WitnessPolicy::AllParties,
        "quorum" => {
            let required = quorum_required.ok_or(WitnessConfigError::MissingField(
                "quorum_required for quorum policy",
            ))?;
            let witness_strs =
                quorum_witnesses.ok_or(WitnessConfigError::MissingField(
                    "quorum_witnesses for quorum policy",
                ))?;

            let mut witnesses = Vec::with_capacity(witness_strs.len());
            let mut seen_dids = std::collections::HashSet::new();
            for did_str in witness_strs {
                let did = Did::from_str(did_str).map_err(|e| {
                    WitnessConfigError::InvalidDid(did_str.clone(), e.to_string())
                })?;
                if !seen_dids.insert(did.clone()) {
                    return Err(WitnessConfigError::DuplicateWitness(did_str.clone()));
                }
                witnesses.push(did);
            }

            if (required as usize) > witnesses.len() {
                return Err(WitnessConfigError::QuorumTooLarge {
                    required,
                    available: witnesses.len(),
                });
            }

            icn_ledger::WitnessPolicy::Quorum {
                required,
                witnesses,
            }
        }
        other => return Err(WitnessConfigError::InvalidPolicy(other.to_string())),
    };

    Ok(icn_ledger::WitnessConfig {
        default_policy: policy,
        threshold,
        collection_timeout_secs,
        min_witness_trust,
    })
}

/// Build an [`icn_ledger::oracle::OracleConfig`] from primitive config values.
///
/// All parameters are primitives so the caller (icnd) does not need to
/// depend on `icn_ledger`.
pub fn build_oracle_config(
    default_ttl_secs: u64,
    min_sources_for_consensus: usize,
    outlier_threshold: f64,
    staleness_threshold_secs: u64,
    default_suspicious_rate_threshold: f64,
    suspicious_rate_thresholds: HashMap<String, f64>,
) -> icn_ledger::oracle::OracleConfig {
    icn_ledger::oracle::OracleConfig {
        default_ttl_secs,
        min_sources_for_consensus,
        outlier_threshold,
        staleness_threshold_secs,
        default_suspicious_rate_threshold,
        suspicious_rate_thresholds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Drift guard: icn-core's serde default for this field is 1000.0.
    // If icn-ledger ever changes the constant, this test fails so we
    // update the serde default in lockstep.
    #[test]
    fn threshold_drift_guard() {
        assert!(
            (DEFAULT_SUSPICIOUS_RATE_THRESHOLD - 1000.0).abs() < f64::EPSILON,
            "DEFAULT_SUSPICIOUS_RATE_THRESHOLD changed from 1000.0 to {}; \
             update the serde default in icn-core/src/config/ledger.rs",
            DEFAULT_SUSPICIOUS_RATE_THRESHOLD,
        );
    }

    #[test]
    fn test_witness_settings_none_policy() {
        let config = build_witness_config("none", None, None, None, 300, None).unwrap();
        assert!(matches!(
            config.default_policy,
            icn_ledger::WitnessPolicy::None
        ));
    }

    #[test]
    fn test_witness_settings_counterparty_policy() {
        let config =
            build_witness_config("counterparty", Some(1000), None, None, 300, None).unwrap();
        assert!(matches!(
            config.default_policy,
            icn_ledger::WitnessPolicy::Counterparty
        ));
        assert_eq!(config.threshold, Some(1000));
    }

    #[test]
    fn test_witness_settings_all_parties_policy() {
        let config = build_witness_config("all_parties", None, None, None, 300, None).unwrap();
        assert!(matches!(
            config.default_policy,
            icn_ledger::WitnessPolicy::AllParties
        ));
    }

    #[test]
    fn test_witness_settings_quorum_policy() {
        let keypair1 = icn_identity::KeyPair::generate().unwrap();
        let keypair2 = icn_identity::KeyPair::generate().unwrap();
        let keypair3 = icn_identity::KeyPair::generate().unwrap();

        let witnesses = vec![
            keypair1.did().to_string(),
            keypair2.did().to_string(),
            keypair3.did().to_string(),
        ];

        let config =
            build_witness_config("quorum", None, Some(2), Some(&witnesses), 600, None).unwrap();

        if let icn_ledger::WitnessPolicy::Quorum {
            required,
            witnesses,
        } = config.default_policy
        {
            assert_eq!(required, 2);
            assert_eq!(witnesses.len(), 3);
        } else {
            panic!("Expected Quorum policy");
        }
        assert_eq!(config.collection_timeout_secs, 600);
    }

    #[test]
    fn test_witness_settings_quorum_missing_required() {
        let witnesses = vec!["did:icn:test".to_string()];
        let result = build_witness_config("quorum", None, None, Some(&witnesses), 300, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WitnessConfigError::MissingField(_)
        ));
    }

    #[test]
    fn test_witness_settings_quorum_missing_witnesses() {
        let result = build_witness_config("quorum", None, Some(2), None, 300, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WitnessConfigError::MissingField(_)
        ));
    }

    #[test]
    fn test_witness_settings_quorum_too_large() {
        let keypair1 = icn_identity::KeyPair::generate().unwrap();
        let keypair2 = icn_identity::KeyPair::generate().unwrap();

        let witnesses = vec![keypair1.did().to_string(), keypair2.did().to_string()];

        let result = build_witness_config("quorum", None, Some(5), Some(&witnesses), 300, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WitnessConfigError::QuorumTooLarge { .. }
        ));
    }

    #[test]
    fn test_witness_settings_duplicate_witness() {
        let keypair = icn_identity::KeyPair::generate().unwrap();

        let witnesses = vec![keypair.did().to_string(), keypair.did().to_string()];

        let result = build_witness_config("quorum", None, Some(2), Some(&witnesses), 300, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WitnessConfigError::DuplicateWitness(_)
        ));
    }

    #[test]
    fn test_witness_settings_invalid_policy() {
        let result = build_witness_config("invalid_policy", None, None, None, 300, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WitnessConfigError::InvalidPolicy(_)
        ));
    }

    #[test]
    fn test_witness_settings_invalid_did() {
        let witnesses = vec!["not-a-valid-did".to_string()];
        let result = build_witness_config("quorum", None, Some(1), Some(&witnesses), 300, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WitnessConfigError::InvalidDid(_, _)
        ));
    }

    #[test]
    fn test_to_oracle_config() {
        let mut thresholds = HashMap::new();
        thresholds.insert("USD:JPY".to_string(), 200.0);

        let oracle_config = build_oracle_config(3600, 1, 0.15, 86400, 500.0, thresholds);

        assert_eq!(oracle_config.default_ttl_secs, 3600);
        assert!(
            (oracle_config.default_suspicious_rate_threshold - 500.0).abs() < f64::EPSILON
        );
        assert_eq!(
            oracle_config.suspicious_rate_thresholds.get("USD:JPY"),
            Some(&200.0)
        );
    }
}
